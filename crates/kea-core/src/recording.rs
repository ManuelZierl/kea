use crate::types::invalid;
use crate::{Event, Kind, Size, MAX_BYTES, MAX_EVENTS, MAX_OUTPUT};
use std::io::{self, Write};

/// An append-only, bounded recording. Equal timestamps preserve insertion order.
#[derive(Debug)]
pub struct Recording {
    initial: Size,
    events: Vec<Event>,
    bytes: usize,
}

impl Recording {
    pub fn new(initial: Size) -> io::Result<Self> {
        Size::new(initial.columns, initial.rows)?;
        Ok(Self {
            initial,
            events: Vec::new(),
            bytes: 0,
        })
    }
    pub fn initial_size(&self) -> Size {
        self.initial
    }
    pub fn events(&self) -> &[Event] {
        &self.events
    }
    pub fn duration(&self) -> u64 {
        self.events.last().map_or(0, |e| e.at)
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    pub fn end_at(&self, at: u64) -> usize {
        self.events.partition_point(|e| e.at <= at)
    }
    pub fn append(&mut self, at: u64, kind: Kind) -> io::Result<()> {
        if self
            .events
            .last()
            .is_some_and(|e| matches!(e.kind, Kind::Exit(_)))
        {
            return Err(invalid("recording already ended"));
        }
        if at < self.duration() {
            return Err(invalid("timestamps moved backwards"));
        }
        let payload = match &kind {
            Kind::Output(bytes) => {
                if bytes.is_empty() || bytes.len() > MAX_OUTPUT {
                    return Err(invalid("invalid output chunk length"));
                }
                bytes.len()
            }
            Kind::Resize(size) => {
                Size::new(size.columns, size.rows)?;
                4
            }
            Kind::Exit(_) => 4,
        };
        // Include per-event bookkeeping; many small events also consume memory.
        let cost = payload + 64;
        if self.events.len() >= MAX_EVENTS || cost > MAX_BYTES - self.bytes {
            return Err(io::Error::other(
                "recording limit reached (32 MiB / 100,000 events)",
            ));
        }
        self.events.push(Event { at, kind });
        self.bytes += cost;
        Ok(())
    }
    pub fn write_to(&self, mut out: impl Write) -> io::Result<()> {
        crate::write_header(&mut out, self.initial)?;
        for event in &self.events {
            crate::write_event(&mut out, event)?;
        }
        out.flush()
    }
}

#[cfg(test)]
#[path = "../tests/unit/recording.rs"]
mod tests;
