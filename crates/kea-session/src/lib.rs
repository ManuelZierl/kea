//! Headless session controller. Live and historical state are always separate.
mod journal;
use anyhow::{Context, Result};
use journal::Journal;
use kea_alacritty::{Engine, MouseEncoding, Screen, SelectionMotion, TerminalPoint};
use kea_core::{Kind, Projection, Recording, Size};
use kea_pty::{Message, Pty};
use std::{ffi::OsString, path::Path, time::Instant};

#[derive(Clone, Debug)]
pub enum Observed {
    Output { at: u64, bytes: Vec<u8> },
    Exit { at: u64, code: u32 },
}

#[derive(Debug, Default)]
pub struct PumpResult {
    pub changed: bool,
    pub observed: Vec<Observed>,
}

#[derive(Debug)]
struct EchoFilter {
    needle: Vec<u8>,
    pending: Vec<u8>,
}

impl EchoFilter {
    fn new(needle: Vec<u8>) -> Result<Self> {
        if needle.is_empty() {
            anyhow::bail!("hidden terminal input cannot be empty");
        }
        Ok(Self {
            needle,
            pending: Vec::new(),
        })
    }

    /// Returns visible bytes and whether an execution marker confirms that no
    /// further line-editor redraws of the hidden input can arrive.
    fn push(&mut self, bytes: &[u8]) -> (Vec<u8>, bool) {
        self.pending.extend_from_slice(bytes);
        let mut visible = Vec::new();
        while let Some(at) = find_bytes(&self.pending, &self.needle) {
            visible.extend(self.pending.drain(..at));
            self.pending.drain(..self.needle.len());
            // PTYs commonly echo CR as CRLF. The CR is part of the hidden input;
            // suppress the synthetic LF too so document execution leaves no blank line.
            if self.pending.first() == Some(&b'\n') && self.needle.last() == Some(&b'\r') {
                self.pending.remove(0);
            }
        }

        // Readline and other shells may redraw the typed wrapper instead of echoing
        // its bytes verbatim. Once the private start marker appears, execution has
        // definitely begun; fail open rather than buffering real command output or
        // blocking future document submissions forever.
        if find_bytes(&self.pending, b"\x1b]777;kea;start;").is_some()
            || find_bytes(&self.pending, b"\x1b]777;kea;prompt;").is_some()
        {
            visible.extend(std::mem::take(&mut self.pending));
            return (visible, true);
        }

        let keep = [
            self.needle.as_slice(),
            b"\x1b]777;kea;start;",
            b"\x1b]777;kea;prompt;",
        ]
        .into_iter()
        .map(|prefix| suffix_prefix_len(&self.pending, prefix))
        .max()
        .unwrap_or(0);
        let emit = self.pending.len().saturating_sub(keep);
        visible.extend(self.pending.drain(..emit));
        (visible, false)
    }

    fn finish(mut self) -> Vec<u8> {
        std::mem::take(&mut self.pending)
    }
}

#[derive(Debug)]
enum PresentationEvent {
    Output(Vec<u8>),
    Resize(Size),
    Exit(Vec<u8>),
}

/// One presentation event per canonical event. Output payloads may be empty;
/// canonical timestamps and event indices remain the sole playback clock.
#[derive(Debug)]
enum Presentation {
    Canonical,
    Filtered(Vec<PresentationEvent>),
}

impl Presentation {
    fn push(&mut self, event: PresentationEvent) {
        if let Self::Filtered(events) = self {
            events.push(event);
        }
    }

    fn apply(&self, recording: &Recording, index: usize, engine: &mut Engine) -> Result<()> {
        let event = recording
            .events()
            .get(index)
            .context("presentation index is outside canonical history")?;
        match self {
            Self::Canonical => apply_kind(engine, &event.kind),
            Self::Filtered(events) => {
                let presentation = events
                    .get(index)
                    .context("presentation history is not aligned with canonical history")?;
                match (&event.kind, presentation) {
                    (Kind::Output(_), PresentationEvent::Output(bytes)) => engine.output(bytes),
                    (Kind::Resize(canonical), PresentationEvent::Resize(presentation))
                        if canonical == presentation =>
                    {
                        engine.resize(*presentation);
                    }
                    (Kind::Exit(_), PresentationEvent::Exit(bytes)) => engine.output(bytes),
                    _ => anyhow::bail!("presentation event does not match canonical history"),
                }
            }
        }
        Ok(())
    }
}

fn apply_kind(engine: &mut Engine, kind: &Kind) {
    match kind {
        Kind::Output(bytes) => engine.output(bytes),
        Kind::Resize(size) => engine.resize(*size),
        Kind::Exit(_) => {}
    }
}

pub struct Session {
    recording: Recording,
    presentation: Presentation,
    live: Engine,
    history: Option<(usize, Engine)>,
    pty: Option<Pty>,
    journal: Option<Journal>,
    started: Instant,
    playing: Option<(Instant, u64)>,
    history_position: Option<u64>,
    hidden_echo: Option<EchoFilter>,
    capture_stopped: bool,
    persistence_stopped: bool,
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

    fn seek_inner(&mut self, end: usize) -> Result<()> {
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

    fn record_at(&mut self, at: u64, kind: Kind) -> bool {
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

    fn accept_output(
        &mut self,
        at: u64,
        raw: Vec<u8>,
        visible: Vec<u8>,
        observed: &mut Vec<Observed>,
    ) -> bool {
        if raw.is_empty() {
            return false;
        }
        self.live.output(&visible);
        if self.record_at(at, Kind::Output(raw.clone())) {
            self.presentation.push(PresentationEvent::Output(visible));
        }
        observed.push(Observed::Output { at, bytes: raw });
        self.drain_live_replies();
        true
    }

    fn accept_visible(&mut self, bytes: Vec<u8>) -> bool {
        if bytes.is_empty() {
            return false;
        }
        self.live.output(&bytes);
        self.drain_live_replies();
        true
    }

    fn drain_live_replies(&mut self) {
        for reply in self.live.drain_replies() {
            if let Some(pty) = &self.pty {
                if let Err(error) = pty.send(reply.into_bytes()) {
                    self.warning = Some(error.to_string());
                }
            }
        }
    }

    fn filter_hidden_echo(&mut self, bytes: Vec<u8>) -> Vec<u8> {
        let Some(filter) = &mut self.hidden_echo else {
            return bytes;
        };
        let (visible, done) = filter.push(&bytes);
        if done {
            self.hidden_echo = None;
        }
        visible
    }

    /// Bounded work per frame plus an unrecorded observation tap for document-mode
    /// structure. The tap keeps command lifecycle tracking working even if history
    /// recording hits its retention limit.
    pub fn pump_observed(&mut self) -> PumpResult {
        self.pump_inner(true)
    }

    /// Bounded work per frame. The PTY keeps running while a historical view is selected.
    pub fn pump(&mut self) -> bool {
        self.pump_inner(false).changed
    }

    fn pump_inner(&mut self, observe: bool) -> PumpResult {
        let mut result = PumpResult::default();
        for _ in 0..64 {
            let message = self.pty.as_mut().and_then(Pty::try_recv);
            match message {
                Some(Message::Output(bytes)) => {
                    let at = self.elapsed_micros();
                    let visible = self.filter_hidden_echo(bytes.clone());
                    let mut sink = Vec::new();
                    let observed = if observe {
                        &mut result.observed
                    } else {
                        &mut sink
                    };
                    result.changed |= self.accept_output(at, bytes, visible, observed);
                }
                Some(Message::Error(error)) => {
                    self.warning = Some(error);
                    result.changed = true;
                }
                Some(Message::Eof) => {
                    result.changed = true;
                }
                None => break,
            }
        }

        if let Some(pty) = &mut self.pty {
            match pty.exit_code() {
                Ok(Some(code)) => {
                    let at = self.elapsed_micros();
                    let pending = self
                        .hidden_echo
                        .take()
                        .map(EchoFilter::finish)
                        .unwrap_or_default();
                    result.changed |= self.accept_visible(pending.clone());
                    if self.record_at(at, Kind::Exit(code)) {
                        self.presentation.push(PresentationEvent::Exit(pending));
                    }
                    if observe {
                        result.observed.push(Observed::Exit { at, code });
                    }
                    self.exit_code = Some(code);
                    self.pty.take();
                    if let Some(journal) = &mut self.journal {
                        journal.stop();
                    }
                    result.changed = true;
                }
                Err(error) => {
                    self.warning = Some(error.to_string());
                    result.changed = true;
                }
                Ok(None) => (),
            }
        }

        if let Some(error) = self.journal.as_ref().and_then(Journal::error) {
            self.persistence_stopped = true;
            self.warning = Some(format!("DISK RECORDING FAILED: {error}"));
            result.changed = true;
        }

        if let Some((started, offset)) = self.playing {
            let elapsed = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
            let target = offset.saturating_add(elapsed);
            let end = self.recording.end_at(target);
            if end != self.end() {
                if let Err(error) = self.seek_inner(end) {
                    self.warning = Some(error.to_string());
                    self.playing = None;
                }
                result.changed = true;
            }
            let position = target.min(self.recording.duration());
            if self.history_position != Some(position) {
                self.history_position = Some(position);
                result.changed = true;
            }
            if target >= self.recording.duration() {
                self.playing = None;
                result.changed = true;
            }
        }

        result
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn suffix_prefix_len(bytes: &[u8], prefix: &[u8]) -> usize {
    if prefix.is_empty() {
        return 0;
    }
    (1..=bytes.len().min(prefix.len().saturating_sub(1)))
        .rev()
        .find(|length| bytes[bytes.len() - length..] == prefix[..*length])
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn echo_filter_removes_only_the_hidden_input_across_chunks() {
        let mut filter = EchoFilter::new(b"secret wrapper\r".to_vec()).unwrap();
        let (first, done) = filter.push(b"prompt> secret wr");
        assert_eq!(first, b"prompt> ");
        assert!(!done);
        let (second, done) = filter.push(b"apper\r\nactual output");
        assert_eq!(second, b"actual output");
        assert!(!done);
        let (third, done) = filter.push(b"\x1b]777;kea;start;1;ZWNobyBoaQ==\x07");
        assert_eq!(third, b"\x1b]777;kea;start;1;ZWNobyBoaQ==\x07");
        assert!(done);
    }

    #[test]
    fn echo_filter_removes_readline_redraw_after_the_initial_tty_echo() {
        let input = b"private integration\r".to_vec();
        let mut filter = EchoFilter::new(input.clone()).unwrap();
        let (first, done) = filter.push(b"private integration\r\n");
        assert!(first.is_empty());
        assert!(!done);

        let mut redraw = b"bash$ ".to_vec();
        redraw.extend_from_slice(&input);
        redraw.extend_from_slice(b"\n\x1b]777;kea;prompt;L3RtcA==\x07bash$ ");
        let (second, done) = filter.push(&redraw);

        assert!(done);
        assert_eq!(second, b"bash$ \x1b]777;kea;prompt;L3RtcA==\x07bash$ ");
    }

    #[test]
    fn echo_filter_fails_open_when_shell_redraws_before_start_marker() {
        let mut filter = EchoFilter::new(b"wrapper that will not match\r".to_vec()).unwrap();
        let (visible, done) =
            filter.push(b"\x1b[2Kredrawn wrapper\r\n\x1b]777;kea;start;1;ZWNobyBoaQ==\x07hi");
        assert!(done);
        let marker = b"\x1b]777;kea;sta";
        assert!(visible.windows(marker.len()).any(|window| window == marker));
    }

    #[test]
    fn echo_filter_rejects_an_empty_hidden_input() {
        assert!(EchoFilter::new(Vec::new()).is_err());
    }

    #[test]
    fn presentation_flushes_fail_open_bytes_at_the_aligned_exit_event() {
        let size = Size::new(20, 2).unwrap();
        let mut recording = Recording::new(size).unwrap();
        recording
            .append(1, Kind::Output(b"private-prefix".to_vec()))
            .unwrap();
        recording.append(2, Kind::Exit(0)).unwrap();
        let presentation = Presentation::Filtered(vec![
            PresentationEvent::Output(Vec::new()),
            PresentationEvent::Exit(b"private-prefix".to_vec()),
        ]);
        let mut replay = Engine::new(size, false);

        presentation.apply(&recording, 0, &mut replay).unwrap();
        assert!(!replay.screen().text().contains("private-prefix"));
        presentation.apply(&recording, 1, &mut replay).unwrap();
        assert!(replay.screen().text().contains("private-prefix"));
        assert!(matches!(
            &recording.events()[0].kind,
            Kind::Output(bytes) if bytes == b"private-prefix"
        ));
    }

    #[test]
    fn offline_playback_never_accepts_input_even_after_go_live() {
        let mut s = Session::demo().unwrap();
        assert!(s.send(b"rm -rf not-executed".to_vec()).is_err());
        s.go_live();
        assert!(s.send(b"echo not-executed".to_vec()).is_err());
    }

    #[test]
    fn history_view_rejects_sends_until_back_live() {
        // The composer stays editable in history, but no draft text may reach
        // the live process until the view returns to LIVE.
        let mut s = Session::demo().unwrap();
        s.seek(0).unwrap();
        assert!(s.is_history());
        assert!(s.send(b"not-executed".to_vec()).is_err());
        s.go_live();
        assert!(!s.is_history());
    }

    #[test]
    fn seeking_restores_error_and_preserves_latest_state() {
        let mut s = Session::demo().unwrap();
        s.seek(2).unwrap();
        assert!(s.screen().text().contains("ERROR: connection failed"));
        s.go_live();
        assert!(!s.screen().text().contains("ERROR: connection failed"));
        assert!(s.screen().text().contains("Ready."));
    }

    #[test]
    fn time_seek_preserves_the_requested_playhead_between_sparse_events() {
        let mut recording = Recording::new(Size::new(20, 2).unwrap()).unwrap();
        recording
            .append(1_000_000, Kind::Output(b"first".to_vec()))
            .unwrap();
        recording
            .append(4_000_000, Kind::Output(b" second".to_vec()))
            .unwrap();
        let mut session = Session::from_recording(recording).unwrap();

        session.seek_time(2_500_000).unwrap();

        assert_eq!(session.position(), 2_500_000);
        assert!(session.screen().text().contains("first"));
        assert!(!session.screen().text().contains("second"));
    }

    #[test]
    fn historical_viewport_scroll_does_not_move_the_live_viewport() {
        let mut recording = Recording::new(Size::new(8, 3).unwrap()).unwrap();
        recording
            .append(0, Kind::Output(b"one\r\ntwo\r\nthree\r\nfour".to_vec()))
            .unwrap();
        let mut session = Session::from_recording(recording).unwrap();

        session.scroll_lines(2);
        assert!(session.display_offset() > 0);
        session.go_live();
        assert_eq!(session.display_offset(), 0);
        assert!(session.history_size() > 0);
    }

    #[test]
    fn resizing_history_changes_only_the_display_projection() {
        let original_size = Size::new(8, 3).unwrap();
        let recording = Recording::new(original_size).unwrap();
        let mut session = Session::from_recording(recording).unwrap();
        let event_count = session.recording().events().len();

        session.resize(Size::new(12, 5).unwrap()).unwrap();
        assert_eq!(session.screen().size, Size::new(12, 5).unwrap());
        assert_eq!(session.recording().events().len(), event_count);

        session.go_live();
        assert_eq!(session.screen().size, original_size);
    }

    #[test]
    fn terminal_selection_controls_forward_to_displayed_engine() {
        let size = Size::new(12, 3).unwrap();
        let mut recording = Recording::new(size).unwrap();
        recording
            .append(0, Kind::Output(b"one\r\ntwo\r\nthree".to_vec()))
            .unwrap();
        let mut session = Session::from_recording(recording).unwrap();

        session.enter_terminal_selection();
        assert!(session.terminal_local_selection_active());
        assert!(session.terminal_explicit_selection_active());
        session.move_terminal_selection(SelectionMotion::Right, true);
        assert!(session.terminal_has_selection());
        session.terminal_selection_focus_lost();
        assert!(session.terminal_has_selection());
        session.clear_terminal_selection();
        assert!(!session.terminal_local_selection_active());
    }

    #[cfg(unix)]
    #[test]
    fn live_capture_continues_during_rewind_and_blocks_input() {
        let mut s = Session::spawn(
            &[
                "sh".into(),
                "-c".into(),
                "printf before; sleep 0.1; printf after".into(),
            ],
            Size::new(80, 24).unwrap(),
            None,
        )
        .unwrap();
        s.seek(0).unwrap();
        assert!(s.send(b"unexpected".to_vec()).is_err());
        let start = Instant::now();
        while s.is_running() {
            s.pump();
            assert!(start.elapsed().as_secs() < 10);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!s.recording().events().is_empty());
        assert!(s.screen().text().trim().is_empty());
        s.go_live();
        assert!(s.screen().text().contains("beforeafter"));
    }

    #[test]
    fn journal_is_exclusive_and_round_trips_after_close() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("kea-test-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("session.kea");
        let size = Size::new(80, 24).unwrap();
        {
            let mut journal = Journal::create(&path, size).unwrap();
            assert!(Journal::create(&path, size).is_err());
            journal
                .append(&kea_core::Event {
                    at: 0,
                    kind: Kind::Output(b"ok".to_vec()),
                })
                .unwrap();
        }
        let loaded = kea_core::read_from(std::fs::File::open(&path).unwrap()).unwrap();
        assert_eq!(loaded.recording.events().len(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
