use crate::types::invalid;
use crate::{Event, Kind, Size, MAX_BYTES, MAX_EVENTS, MAX_OUTPUT};
use std::io::{self, Write};

/// An append-only, bounded recording. Equal timestamps preserve insertion order.
#[derive(Debug)]
pub struct Recording {
    initial: Size,
    events: Vec<Event>,
    bytes: usize,
    pub(crate) discarded_events: u64,
    pub(crate) start_time: u64,
}

impl Recording {
    pub fn new(initial: Size) -> io::Result<Self> {
        Size::new(initial.columns, initial.rows)?;
        Ok(Self {
            initial,
            events: Vec::new(),
            bytes: 0,
            discarded_events: 0,
            start_time: 0,
        })
    }
    pub fn initial_size(&self) -> Size {
        self.initial
    }
    pub fn events(&self) -> &[Event] {
        &self.events
    }
    pub fn duration(&self) -> u64 {
        self.events.last().map_or(self.start_time, |e| e.at)
    }
    /// Number of events omitted from the persisted prefix by retention.
    pub fn discarded_events(&self) -> u64 {
        self.discarded_events
    }
    /// Timestamp of the last discarded event, or zero for an untrimmed recording.
    pub fn start_time(&self) -> u64 {
        self.start_time
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    pub fn end_at(&self, at: u64) -> usize {
        self.events.partition_point(|e| e.at <= at)
    }
    pub fn append(&mut self, at: u64, kind: Kind) -> io::Result<()> {
        let cost = self.validate_append(at, &kind)?;
        if self.events.len() >= MAX_EVENTS || cost > MAX_BYTES - self.bytes {
            return Err(io::Error::other(
                "recording limit reached (32 MiB / 100,000 events)",
            ));
        }
        self.push(at, kind, cost);
        Ok(())
    }

    /// Append while retaining the newest suffix when the bounded recording fills.
    /// Returns the number of events discarded by this append.
    pub fn append_retained(&mut self, at: u64, kind: Kind) -> io::Result<usize> {
        // Validation must complete before any prefix is discarded.
        let cost = self.validate_append(at, &kind)?;
        if cost > MAX_BYTES {
            return Err(io::Error::other("event exceeds recording limit"));
        }
        let mut evicted = 0;
        if self.events.len() >= MAX_EVENTS || cost > MAX_BYTES - self.bytes {
            let event_pressure = self.events.len() >= MAX_EVENTS;
            let byte_pressure = cost > MAX_BYTES - self.bytes;
            let remove_count = if event_pressure {
                (MAX_EVENTS / 4).max(1)
            } else {
                0
            };
            let mut removed_bytes = 0;
            let mut count = 0;
            while count < remove_count || (byte_pressure && removed_bytes < MAX_BYTES / 4) {
                let Some(event) = self.events.get(count) else {
                    break;
                };
                removed_bytes += event.kind.retained_bytes();
                count += 1;
            }
            // If the event-count limit alone triggered, count-sized batch is sufficient.
            if count == 0 {
                count = 1;
                removed_bytes = self.events[0].kind.retained_bytes();
            }
            for event in self.events.drain(..count) {
                if let Kind::Resize(size) = event.kind {
                    self.initial = size;
                }
                self.start_time = event.at;
                self.discarded_events = self.discarded_events.saturating_add(1);
                evicted += 1;
            }
            self.bytes -= removed_bytes;
        }
        self.push(at, kind, cost);
        Ok(evicted)
    }

    fn validate_append(&self, at: u64, kind: &Kind) -> io::Result<usize> {
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
        match kind {
            Kind::Output(bytes) => {
                if bytes.is_empty() || bytes.len() > MAX_OUTPUT {
                    return Err(invalid("invalid output chunk length"));
                }
            }
            Kind::Resize(size) => {
                Size::new(size.columns, size.rows)?;
            }
            Kind::Exit(_) => {}
            Kind::Submitted { context, input, .. } => {
                if input.is_empty() || input.len() > 64 * 1024 || input.contains('\0') {
                    return Err(invalid("invalid submitted input"));
                }
                if context.is_empty()
                    || context.len() > 128
                    || !context
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
                {
                    return Err(invalid("invalid input context"));
                }
            }
        };
        // Include per-event bookkeeping; many small events also consume memory.
        Ok(kind.retained_bytes())
    }

    fn push(&mut self, at: u64, kind: Kind, cost: usize) {
        self.events.push(Event { at, kind });
        self.bytes += cost;
    }
    pub fn write_to(&self, mut out: impl Write) -> io::Result<()> {
        if self.discarded_events == 0 {
            crate::write_header(&mut out, self.initial)?;
        } else {
            crate::format::write_retained_header(
                &mut out,
                self.initial,
                self.discarded_events,
                self.start_time,
            )?;
        }
        for event in &self.events {
            crate::write_event(&mut out, event)?;
        }
        out.flush()
    }
}

#[cfg(test)]
#[path = "../tests/unit/recording.rs"]
mod tests;
