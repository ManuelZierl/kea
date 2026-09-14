//! Headless session controller. Live and historical state are always separate.
mod journal;
use std::{ffi::OsString, path::Path, time::Instant};
use anyhow::{Context, Result};
use kea_alacritty::{Engine, Screen};
use kea_core::{Kind, Projection, Recording, Size};
use kea_pty::{Message, Pty};
use journal::Journal;

pub struct Session {
    recording: Recording,
    live: Engine,
    history: Option<(usize, Engine)>,
    pty: Option<Pty>,
    journal: Option<Journal>,
    started: Instant,
    playing: Option<(Instant, u64)>,
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
        Ok(Self { recording, live: Engine::new(size, true), history: None, pty: Some(pty), journal, started, playing: None, capture_stopped: false, persistence_stopped: false, warning: None, exit_code: None })
    }
    pub fn from_recording(recording: Recording) -> Result<Self> {
        let end = recording.events().len();
        let live = Engine::at(&recording, end)?;
        let historical = Engine::at(&recording, end)?;
        Ok(Self { recording, live, history: Some((end, historical)), pty: None, journal: None, started: Instant::now(), playing: None, capture_stopped: false, persistence_stopped: false, warning: None, exit_code: None })
    }
    pub fn demo() -> Result<Self> {
        let mut r = Recording::new(Size::new(90, 22)?)?;
        r.append(0, Kind::Output(b"\x1b[2J\x1b[HKea - terminal history, not command re-execution\r\n\r\nA transient error will appear below, then be overwritten.\r\n\r\nWorking...".to_vec()))?;
        r.append(1_000_000, Kind::Output(b"\r\x1b[2K\x1b[31mERROR: connection failed\x1b[0m".to_vec()))?;
        r.append(1_500_000, Kind::Output(b"\r\x1b[2K\x1b[32mReady. The error has disappeared from the live screen.\x1b[0m".to_vec()))?;
        r.append(3_000_000, Kind::Output(b"\r\n\r\nDrag the timeline back or press F6 to recover it.\r\nF7 steps forward. F8 plays/pauses. F9 returns to live.\r\n\r\nNo shell or external command is executed in this demo.".to_vec()))?;
        r.append(4_000_000, Kind::Exit(0))?;
        let mut session = Self::from_recording(r)?;
        session.seek(0)?;
        session.toggle_playback();
        Ok(session)
    }
    pub fn recording(&self) -> &Recording { &self.recording }
    pub fn is_history(&self) -> bool { self.history.is_some() }
    pub fn is_playing(&self) -> bool { self.playing.is_some() }
    pub fn is_running(&self) -> bool { self.pty.is_some() && self.exit_code.is_none() }
    pub fn input_allowed(&self) -> bool { self.is_running() && !self.is_history() }
    pub fn end(&self) -> usize { self.history.as_ref().map_or(self.recording.events().len(), |(end, _)| *end) }
    pub fn position(&self) -> u64 { self.recording.events().get(self.end().saturating_sub(1)).filter(|_| self.end() != 0).map_or(0, |e| e.at) }
    pub fn screen(&self) -> Screen { self.history.as_ref().map_or(&self.live, |(_, engine)| engine).screen() }
    pub fn application_cursor(&self) -> bool { self.live.application_cursor() }
    pub fn bracketed_paste(&self) -> bool { self.live.bracketed_paste() }
    pub fn send(&self, bytes: Vec<u8>) -> Result<()> {
        if !self.input_allowed() { anyhow::bail!("History is read-only. Return to LIVE before sending input."); }
        self.pty.as_ref().context("terminal has ended")?.send(bytes)
    }
    pub fn resize(&mut self, size: Size) -> Result<()> {
        Size::new(size.columns, size.rows)?;
        if !self.input_allowed() || size == self.live.size() { return Ok(()); }
        self.pty.as_ref().context("terminal has ended")?.resize(size)?;
        self.live.resize(size);
        self.record(Kind::Resize(size));
        Ok(())
    }
    pub fn go_live(&mut self) { self.history = None; self.playing = None; }
    pub fn seek(&mut self, end: usize) -> Result<()> {
        self.playing = None;
        self.seek_inner(end)
    }
    fn seek_inner(&mut self, end: usize) -> Result<()> {
        if end > self.recording.events().len() { anyhow::bail!("seek outside recording"); }
        if let Some((previous, engine)) = &mut self.history {
            if end >= *previous {
                for event in &self.recording.events()[*previous..end] {
                    match &event.kind { Kind::Output(bytes) => engine.output(bytes), Kind::Resize(size) => engine.resize(*size), Kind::Exit(_) => () }
                }
                *previous = end;
                return Ok(());
            }
        }
        self.history = Some((end, Engine::at(&self.recording, end)?));
        Ok(())
    }
    pub fn step(&mut self, delta: isize) -> Result<()> { self.seek(self.end().saturating_add_signed(delta).min(self.recording.events().len())) }
    pub fn seek_time(&mut self, at: u64) -> Result<()> { self.seek(self.recording.end_at(at)) }
    pub fn toggle_playback(&mut self) {
        if self.playing.take().is_some() { return; }
        if !self.is_history() || self.end() == self.recording.events().len() {
            if let Err(error) = self.seek(0) { self.warning = Some(error.to_string()); return; }
        }
        self.playing = Some((Instant::now(), self.position()));
    }
    fn record(&mut self, kind: Kind) {
        if self.capture_stopped { return; }
        let at = self.started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        if let Err(error) = self.recording.append(at, kind) {
            self.capture_stopped = true;
            self.warning = Some(format!("HISTORY STOPPED: {error}. Live terminal continues; retained history is incomplete."));
            if let Some(journal) = &mut self.journal { journal.stop(); }
            return;
        }
        if !self.persistence_stopped {
            if let (Some(journal), Some(event)) = (&mut self.journal, self.recording.events().last()) {
                if let Err(error) = journal.append(event) {
                    journal.stop();
                    self.persistence_stopped = true;
                    self.warning = Some(format!("DISK RECORDING STOPPED: {error}. In-memory history continues."));
                }
            }
        }
    }
    /// Bounded work per frame. The PTY keeps running while a historical view is selected.
    pub fn pump(&mut self) -> bool {
        let mut changed = false;
        for _ in 0..64 {
            let message = self.pty.as_mut().and_then(Pty::try_recv);
            match message {
                Some(Message::Output(bytes)) => {
                    self.live.output(&bytes);
                    self.record(Kind::Output(bytes));
                    for reply in self.live.drain_replies() {
                        if let Some(pty) = &self.pty {
                            if let Err(error) = pty.send(reply.into_bytes()) { self.warning = Some(error.to_string()); }
                        }
                    }
                    changed = true;
                }
                Some(Message::Error(error)) => { self.warning = Some(error); changed = true; }
                Some(Message::Eof) => { changed = true; }
                None => break,
            }
        }
        if let Some(pty) = &mut self.pty {
            match pty.exit_code() {
                Ok(Some(code)) => {
                    self.record(Kind::Exit(code));
                    self.exit_code = Some(code);
                    self.pty.take();
                    if let Some(journal) = &mut self.journal { journal.stop(); }
                    changed = true;
                }
                Err(error) => { self.warning = Some(error.to_string()); changed = true; }
                Ok(None) => (),
            }
        }
        if let Some(error) = self.journal.as_ref().and_then(Journal::error) {
            self.persistence_stopped = true;
            self.warning = Some(format!("DISK RECORDING FAILED: {error}"));
            changed = true;
        }
        if let Some((started, offset)) = self.playing {
            let elapsed = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
            let target = offset.saturating_add(elapsed);
            let end = self.recording.end_at(target);
            if end != self.end() {
                if let Err(error) = self.seek_inner(end) { self.warning = Some(error.to_string()); self.playing = None; }
                changed = true;
            }
            if target >= self.recording.duration() { self.playing = None; changed = true; }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_playback_never_accepts_input_even_after_go_live() {
        let mut s = Session::demo().unwrap();
        assert!(s.send(b"rm -rf not-executed".to_vec()).is_err());
        s.go_live();
        assert!(s.send(b"echo not-executed".to_vec()).is_err());
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
    #[cfg(unix)]
    #[test]
    fn live_capture_continues_during_rewind_and_blocks_input() {
        let mut s = Session::spawn(&["sh".into(), "-c".into(), "printf before; sleep 0.1; printf after".into()], Size::new(80, 24).unwrap(), None).unwrap();
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
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let directory = std::env::temp_dir().join(format!("kea-test-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("session.kea");
        let size = Size::new(80, 24).unwrap();
        {
            let mut journal = Journal::create(&path, size).unwrap();
            assert!(Journal::create(&path, size).is_err());
            journal.append(&kea_core::Event { at: 0, kind: Kind::Output(b"ok".to_vec()) }).unwrap();
        }
        let loaded = kea_core::read_from(std::fs::File::open(&path).unwrap()).unwrap();
        assert_eq!(loaded.recording.events().len(), 1);
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
