use crate::{
    journal::Journal,
    presentation::{EchoFilter, PresentationEvent},
    session::Session,
};
use kea_core::{Kind, Projection};
use kea_pty::{Message, Pty};

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

impl Session {
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

#[cfg(test)]
#[path = "../tests/unit/observation.rs"]
mod tests;
