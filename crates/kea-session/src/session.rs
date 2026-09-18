use crate::{
    journal::Journal,
    presentation::{EchoFilter, Presentation, PresentationEvent},
};
use anyhow::{Context, Result};
use kea_alacritty::{Engine, MouseEncoding, Screen, SelectionMotion, TerminalPoint};
use kea_core::{Kind, Projection, Recording, Size};
use kea_pty::Pty;
use std::{ffi::OsString, path::Path, time::Instant};

pub struct Session {
    pub(crate) recording: Recording,
    pub(crate) presentation: Presentation,
    pub(crate) live: Engine,
    pub(crate) history: Option<(usize, Engine)>,
    pub(crate) pty: Option<Pty>,
    pub(crate) journal: Option<Journal>,
    pub(crate) started: Instant,
    pub(crate) playing: Option<(Instant, u64)>,
    pub(crate) history_position: Option<u64>,
    pub(crate) hidden_echo: Option<EchoFilter>,
    pub(crate) capture_stopped: bool,
    pub(crate) persistence_stopped: bool,
    pub warning: Option<String>,
    pub exit_code: Option<u32>,
}

impl Session {
    pub fn spawn(command: &[OsString], size: Size, path: Option<&Path>) -> Result<Self> {
        let recording = Recording::new(size)?;
        let journal = path.map(|path| Journal::create(path, size)).transpose()?;
        let started = Instant::now();
        let pty = Pty::spawn(command, size)?;
        Ok(Self {
            recording,
            presentation: Presentation::Filtered(Vec::new()),
            live: Engine::new(size, true),
            history: None,
            pty: Some(pty),
            journal,
            started,
            playing: None,
            history_position: None,
            hidden_echo: None,
            capture_stopped: false,
            persistence_stopped: false,
            warning: None,
            exit_code: None,
        })
    }

    pub fn from_recording(recording: Recording) -> Result<Self> {
        let end = recording.events().len();
        let duration = recording.duration();
        let live = Engine::at(&recording, end)?;
        let historical = Engine::at(&recording, end)?;
        Ok(Self {
            recording,
            presentation: Presentation::Canonical,
            live,
            history: Some((end, historical)),
            pty: None,
            journal: None,
            started: Instant::now(),
            playing: None,
            history_position: Some(duration),
            hidden_echo: None,
            capture_stopped: false,
            persistence_stopped: false,
            warning: None,
            exit_code: None,
        })
    }

    pub fn demo() -> Result<Self> {
        let mut r = Recording::new(Size::new(90, 22)?)?;
        r.append(0, Kind::Output(b"\x1b[2J\x1b[HKea - terminal history, not command re-execution\r\n\r\nA transient error will appear below, then be overwritten.\r\n\r\nWorking...".to_vec()))?;
        r.append(
            1_000_000,
            Kind::Output(b"\r\x1b[2K\x1b[31mERROR: connection failed\x1b[0m".to_vec()),
        )?;
        r.append(
            1_500_000,
            Kind::Output(
                b"\r\x1b[2K\x1b[32mReady. The error has disappeared from the live screen.\x1b[0m"
                    .to_vec(),
            ),
        )?;
        r.append(3_000_000, Kind::Output(b"\r\n\r\nDrag the timeline back or press F6 to recover it.\r\nF7 steps forward. F8 plays/pauses. F9 returns to live.\r\n\r\nNo shell or external command is executed in this demo.".to_vec()))?;
        r.append(4_000_000, Kind::Exit(0))?;
        let mut session = Self::from_recording(r)?;
        session.seek(0)?;
        session.toggle_playback();
        Ok(session)
    }

    pub fn recording(&self) -> &Recording {
        &self.recording
    }

    pub fn elapsed_micros(&self) -> u64 {
        self.started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
    }

    pub fn is_history(&self) -> bool {
        self.history.is_some()
    }

    pub fn is_playing(&self) -> bool {
        self.playing.is_some()
    }

    pub fn is_running(&self) -> bool {
        self.pty.is_some() && self.exit_code.is_none()
    }

    pub fn input_allowed(&self) -> bool {
        self.is_running() && !self.is_history()
    }

    pub fn end(&self) -> usize {
        self.history
            .as_ref()
            .map_or(self.recording.events().len(), |(end, _)| *end)
    }

    pub fn position(&self) -> u64 {
        if let Some(position) = self.history_position {
            return position;
        }
        self.recording
            .events()
            .get(self.end().saturating_sub(1))
            .filter(|_| self.end() != 0)
            .map_or(0, |e| e.at)
    }

    pub fn screen(&self) -> Screen {
        self.displayed_engine().screen()
    }

    pub fn display_offset(&self) -> usize {
        self.displayed_engine().display_offset()
    }

    pub fn history_size(&self) -> usize {
        self.displayed_engine().history_size()
    }

    pub fn scroll_lines(&mut self, lines: i32) {
        self.displayed_engine_mut().scroll_lines(lines);
    }

    pub fn scroll_bottom(&mut self) {
        self.displayed_engine_mut().scroll_bottom();
    }

    pub fn begin_terminal_selection(&mut self, point: TerminalPoint) {
        self.displayed_engine_mut().begin_selection(point);
    }

    pub fn begin_terminal_selection_kind(&mut self, point: TerminalPoint, block: bool) {
        self.displayed_engine_mut()
            .begin_selection_kind(point, block);
    }

    pub fn update_terminal_selection(&mut self, point: TerminalPoint) {
        self.displayed_engine_mut().update_selection(point);
    }

    pub fn set_terminal_selection_block(&mut self, block: bool) {
        self.displayed_engine_mut().set_selection_block(block);
    }

    pub fn extend_terminal_selection(&mut self, point: TerminalPoint) {
        self.displayed_engine_mut().extend_selection(point);
    }

    pub fn clear_terminal_selection(&mut self) {
        self.displayed_engine_mut().clear_selection();
    }

    pub fn terminal_selection_text(&self) -> Option<String> {
        self.displayed_engine().selection_text()
    }

    pub fn terminal_has_selection(&self) -> bool {
        self.displayed_engine().has_selection()
    }

    pub fn terminal_local_selection_active(&self) -> bool {
        self.displayed_engine().local_selection_active()
    }

    pub fn terminal_explicit_selection_active(&self) -> bool {
        self.displayed_engine().explicit_selection_active()
    }

    pub fn enter_terminal_selection(&mut self) {
        self.displayed_engine_mut().enter_local_selection();
    }

    pub fn terminal_selection_focus_lost(&mut self) {
        self.displayed_engine_mut().selection_focus_lost();
    }

    pub fn place_terminal_selection_caret(&mut self, point: TerminalPoint) {
        self.displayed_engine_mut().place_selection_caret(point);
    }

    pub fn move_terminal_selection(&mut self, motion: SelectionMotion, extend: bool) {
        self.displayed_engine_mut().move_selection(motion, extend);
    }

    pub fn terminal_size(&self) -> Size {
        self.displayed_engine().size()
    }

    pub fn terminal_mouse_reporting(&self) -> bool {
        self.input_allowed() && self.live.mouse_reporting()
    }

    pub fn terminal_mouse_encoding(&self) -> Option<MouseEncoding> {
        self.input_allowed()
            .then(|| self.live.mouse_encoding())
            .flatten()
    }

    pub fn application_cursor(&self) -> bool {
        self.live.application_cursor()
    }

    pub fn bracketed_paste(&self) -> bool {
        self.live.bracketed_paste()
    }

    fn displayed_engine(&self) -> &Engine {
        self.history
            .as_ref()
            .map_or(&self.live, |(_, engine)| engine)
    }

    fn displayed_engine_mut(&mut self) -> &mut Engine {
        self.history
            .as_mut()
            .map_or(&mut self.live, |(_, engine)| engine)
    }

    pub fn send(&self, bytes: Vec<u8>) -> Result<()> {
        if !self.input_allowed() {
            anyhow::bail!("History is read-only. Return to LIVE before sending input.");
        }
        self.pty.as_ref().context("terminal has ended")?.send(bytes)
    }

    /// Send application-owned shell protocol input without painting or recording its
    /// terminal-driver echo. The program's actual output is unaffected.
    pub fn send_hidden(&mut self, bytes: Vec<u8>) -> Result<()> {
        if !self.input_allowed() {
            anyhow::bail!("History is read-only. Return to LIVE before sending input.");
        }
        if self.hidden_echo.is_some() {
            anyhow::bail!("previous hidden terminal input has not been echoed yet");
        }
        let pty = self.pty.as_ref().context("terminal has ended")?;
        self.hidden_echo = Some(EchoFilter::new(bytes.clone())?);
        if let Err(error) = pty.send(bytes) {
            self.hidden_echo = None;
            return Err(error);
        }
        Ok(())
    }

    pub fn resize(&mut self, size: Size) -> Result<()> {
        Size::new(size.columns, size.rows)?;
        if let Some((_, historical)) = &mut self.history {
            if size != historical.size() {
                historical.resize(size);
            }
            return Ok(());
        }
        if size == self.live.size() {
            return Ok(());
        }
        if !self.is_running() {
            self.live.resize(size);
            return Ok(());
        }
        self.pty
            .as_ref()
            .context("terminal has ended")?
            .resize(size)?;
        self.live.resize(size);
        let at = self.elapsed_micros();
        if self.record_at(at, Kind::Resize(size)) {
            self.presentation.push(PresentationEvent::Resize(size));
        }
        Ok(())
    }

    pub fn go_live(&mut self) {
        self.history = None;
        self.playing = None;
        self.history_position = None;
    }

    pub fn seek(&mut self, end: usize) -> Result<()> {
        self.playing = None;
        self.seek_inner(end)?;
        self.history_position = Some(self.event_position(end));
        Ok(())
    }

    pub(crate) fn seek_inner(&mut self, end: usize) -> Result<()> {
        if end > self.recording.events().len() {
            anyhow::bail!("seek outside recording");
        }
        if let Some((previous, engine)) = &mut self.history {
            if end >= *previous {
                for index in *previous..end {
                    self.presentation.apply(&self.recording, index, engine)?;
                }
                *previous = end;
                return Ok(());
            }
        }
        self.history = Some((end, self.historical_at(end)?));
        Ok(())
    }

    fn historical_at(&self, end: usize) -> Result<Engine> {
        if end > self.recording.events().len() {
            anyhow::bail!("seek outside recording");
        }
        let mut engine = Engine::new(self.recording.initial_size(), false);
        for index in 0..end {
            self.presentation
                .apply(&self.recording, index, &mut engine)?;
        }
        Ok(engine)
    }

    pub fn step(&mut self, delta: isize) -> Result<()> {
        self.seek(
            self.end()
                .saturating_add_signed(delta)
                .min(self.recording.events().len()),
        )
    }

    pub fn seek_time(&mut self, at: u64) -> Result<()> {
        self.playing = None;
        let at = at.min(self.recording.duration());
        self.seek_inner(self.recording.end_at(at))?;
        self.history_position = Some(at);
        Ok(())
    }

    pub fn toggle_playback(&mut self) {
        if self.playing.take().is_some() {
            return;
        }
        if !self.is_history() || self.end() == self.recording.events().len() {
            if let Err(error) = self.seek(0) {
                self.warning = Some(error.to_string());
                return;
            }
        }
        self.playing = Some((Instant::now(), self.position()));
    }

    fn event_position(&self, end: usize) -> u64 {
        self.recording
            .events()
            .get(end.saturating_sub(1))
            .filter(|_| end != 0)
            .map_or(0, |event| event.at)
    }

    pub(crate) fn record_at(&mut self, at: u64, kind: Kind) -> bool {
        if self.capture_stopped {
            return false;
        }
        if let Err(error) = self.recording.append(at, kind) {
            self.capture_stopped = true;
            self.warning = Some(format!("HISTORY STOPPED: {error}. Live terminal continues; retained history is incomplete."));
            if let Some(journal) = &mut self.journal {
                journal.stop();
            }
            return false;
        }
        if !self.persistence_stopped {
            if let (Some(journal), Some(event)) =
                (&mut self.journal, self.recording.events().last())
            {
                if let Err(error) = journal.append(event) {
                    journal.stop();
                    self.persistence_stopped = true;
                    self.warning = Some(format!(
                        "DISK RECORDING STOPPED: {error}. In-memory history continues."
                    ));
                }
            }
        }
        true
    }
}

#[cfg(test)]
#[path = "../tests/unit/session.rs"]
mod tests;
