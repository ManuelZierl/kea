use anyhow::{Context, Result};
use kea_alacritty::Engine;
use kea_core::{Kind, Projection, Recording, Size};

#[derive(Debug)]
pub(crate) struct EchoFilter {
    needle: Vec<u8>,
    pending: Vec<u8>,
}

impl EchoFilter {
    pub(crate) fn new(needle: Vec<u8>) -> Result<Self> {
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
    pub(crate) fn push(&mut self, bytes: &[u8]) -> (Vec<u8>, bool) {
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

    pub(crate) fn finish(mut self) -> Vec<u8> {
        std::mem::take(&mut self.pending)
    }
}

#[derive(Debug)]
pub(crate) enum PresentationEvent {
    Output(Vec<u8>),
    Resize(Size),
    Exit(Vec<u8>),
    Submitted,
}

/// One presentation event per canonical event. Output payloads may be empty;
/// canonical timestamps and event indices remain the sole playback clock.
#[derive(Debug)]
pub(crate) enum Presentation {
    Canonical,
    Filtered(Vec<PresentationEvent>),
}

impl Presentation {
    pub(crate) fn push(&mut self, event: PresentationEvent) {
        if let Self::Filtered(events) = self {
            events.push(event);
        }
    }

    pub(crate) fn apply(
        &self,
        recording: &Recording,
        index: usize,
        engine: &mut Engine,
    ) -> Result<()> {
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
                    (Kind::Submitted { .. }, PresentationEvent::Submitted) => {}
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
        Kind::Exit(_) | Kind::Submitted { .. } => {}
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
#[path = "../tests/unit/presentation.rs"]
mod tests;
