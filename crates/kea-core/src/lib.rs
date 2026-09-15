//! Platform-, shell- and renderer-independent terminal recordings.
//! Output is opaque bytes, not text. Input is deliberately absent.
use std::io::{self, Read, Write};

const MAGIC: &[u8; 8] = b"KEA\x01\r\n\x1a\n";
pub const MAX_OUTPUT: usize = 1024 * 1024;
pub const MAX_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_EVENTS: usize = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub columns: u16,
    pub rows: u16,
}

impl Size {
    pub fn new(columns: u16, rows: u16) -> io::Result<Self> {
        if !(2..=512).contains(&columns) || !(1..=256).contains(&rows) {
            return Err(invalid(
                "terminal dimensions must be 2..512 columns and 1..256 rows",
            ));
        }
        Ok(Self { columns, rows })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Output(Vec<u8>),
    Resize(Size),
    Exit(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    /// Monotonic microseconds since session start, measured at ingestion.
    pub at: u64,
    pub kind: Kind,
}

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
        write_header(&mut out, self.initial)?;
        for event in &self.events {
            write_event(&mut out, event)?;
        }
        out.flush()
    }
}

/// A projection is supplied by the embedding application. It must start empty
/// at the recording's initial size. No input or process-execution API is exposed.
pub trait Projection {
    fn output(&mut self, bytes: &[u8]);
    fn resize(&mut self, size: Size);
}

pub fn replay(recording: &Recording, end: usize, into: &mut impl Projection) -> io::Result<()> {
    if end > recording.events.len() {
        return Err(invalid("seek beyond recording"));
    }
    for event in &recording.events[..end] {
        match &event.kind {
            Kind::Output(bytes) => into.output(bytes),
            Kind::Resize(size) => into.resize(*size),
            Kind::Exit(_) => (),
        }
    }
    Ok(())
}

pub struct Loaded {
    pub recording: Recording,
    /// Only an incomplete final frame is recoverable; corruption is an error.
    pub truncated_tail: bool,
}

pub fn write_header(mut out: impl Write, size: Size) -> io::Result<()> {
    Size::new(size.columns, size.rows)?;
    out.write_all(MAGIC)?;
    out.write_all(&size.columns.to_le_bytes())?;
    out.write_all(&size.rows.to_le_bytes())
}

pub fn write_event(mut out: impl Write, event: &Event) -> io::Result<()> {
    let mut body = Vec::new();
    body.extend_from_slice(&event.at.to_le_bytes());
    match &event.kind {
        Kind::Output(bytes) => {
            if bytes.is_empty() || bytes.len() > MAX_OUTPUT {
                return Err(invalid("invalid output length"));
            }
            body.push(0);
            body.extend_from_slice(bytes);
        }
        Kind::Resize(size) => {
            Size::new(size.columns, size.rows)?;
            body.push(1);
            body.extend_from_slice(&size.columns.to_le_bytes());
            body.extend_from_slice(&size.rows.to_le_bytes());
        }
        Kind::Exit(code) => {
            body.push(2);
            body.extend_from_slice(&code.to_le_bytes());
        }
    }
    out.write_all(&(body.len() as u32).to_le_bytes())?;
    out.write_all(&body)?;
    out.write_all(&checksum(&body).to_le_bytes())
}

pub fn read_from(mut input: impl Read) -> io::Result<Loaded> {
    let mut header = [0; 12];
    input.read_exact(&mut header)?;
    if &header[..8] != MAGIC {
        return Err(invalid("not a supported Kea v1 recording"));
    }
    let initial = Size::new(
        u16::from_le_bytes([header[8], header[9]]),
        u16::from_le_bytes([header[10], header[11]]),
    )?;
    let mut recording = Recording::new(initial)?;
    loop {
        let mut length = [0; 4];
        let count = read_partial(&mut input, &mut length)?;
        if count == 0 {
            return Ok(Loaded {
                recording,
                truncated_tail: false,
            });
        }
        if count < 4 {
            return Ok(Loaded {
                recording,
                truncated_tail: true,
            });
        }
        let length = u32::from_le_bytes(length) as usize;
        if !(10..=MAX_OUTPUT + 9).contains(&length) {
            return Err(invalid("invalid frame length"));
        }
        let mut body = vec![0; length];
        let mut crc = [0; 4];
        if read_partial(&mut input, &mut body)? < length || read_partial(&mut input, &mut crc)? < 4
        {
            return Ok(Loaded {
                recording,
                truncated_tail: true,
            });
        }
        if checksum(&body) != u32::from_le_bytes(crc) {
            return Err(invalid("frame checksum mismatch"));
        }
        let at = u64::from_le_bytes(body[..8].try_into().map_err(|_| invalid("bad timestamp"))?);
        let kind = match body[8] {
            0 => Kind::Output(body[9..].to_vec()),
            1 if length == 13 => Kind::Resize(Size::new(
                u16::from_le_bytes([body[9], body[10]]),
                u16::from_le_bytes([body[11], body[12]]),
            )?),
            2 if length == 13 => Kind::Exit(u32::from_le_bytes(
                body[9..13]
                    .try_into()
                    .map_err(|_| invalid("bad exit code"))?,
            )),
            _ => return Err(invalid("unknown or malformed event")),
        };
        recording.append(at, kind)?;
    }
}

fn read_partial(input: &mut impl Read, buffer: &mut [u8]) -> io::Result<usize> {
    let mut count = 0;
    while count < buffer.len() {
        match input.read(&mut buffer[count..]) {
            Ok(0) => break,
            Ok(n) => count += n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(count)
}

// IEEE CRC-32. Integrity detection, not authentication or encryption.
fn checksum(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn recording() -> Recording {
        Recording::new(Size::new(80, 24).unwrap()).unwrap()
    }
    #[test]
    fn round_trip_preserves_non_utf8_and_equal_timestamp_order() {
        let mut r = recording();
        r.append(3, Kind::Output(vec![0xff, 0, 27])).unwrap();
        r.append(3, Kind::Resize(Size::new(40, 12).unwrap()))
            .unwrap();
        r.append(4, Kind::Exit(7)).unwrap();
        let mut bytes = Vec::new();
        r.write_to(&mut bytes).unwrap();
        let loaded = read_from(bytes.as_slice()).unwrap();
        assert_eq!(loaded.recording.events(), r.events());
        assert_eq!(loaded.recording.initial_size(), r.initial_size());
        assert_eq!(loaded.recording.end_at(3), 2);
        assert!(!loaded.truncated_tail);
    }
    #[test]
    fn truncated_tail_recovers_only_complete_events() {
        let mut r = recording();
        r.append(1, Kind::Output(b"visible".to_vec())).unwrap();
        let mut prefix = Vec::new();
        r.write_to(&mut prefix).unwrap();
        r.append(2, Kind::Output(b"tail".to_vec())).unwrap();
        let mut bytes = Vec::new();
        r.write_to(&mut bytes).unwrap();
        for len in prefix.len() + 1..bytes.len() {
            let loaded = read_from(&bytes[..len]).unwrap();
            assert!(loaded.truncated_tail);
            assert_eq!(loaded.recording.events().len(), 1);
        }
    }
    #[test]
    fn corruption_is_not_treated_as_truncation() {
        let mut r = recording();
        r.append(1, Kind::Output(b"hello".to_vec())).unwrap();
        let mut bytes = Vec::new();
        r.write_to(&mut bytes).unwrap();
        bytes[25] ^= 1;
        assert!(read_from(bytes.as_slice()).is_err());
    }
    #[test]
    fn rejects_bad_dimensions_clock_and_events_after_exit() {
        assert!(Size::new(0, 24).is_err());
        assert!(Size::new(80, u16::MAX).is_err());
        let mut r = recording();
        r.append(2, Kind::Output(vec![1])).unwrap();
        assert!(r.append(1, Kind::Output(vec![1])).is_err());
        r.append(3, Kind::Exit(0)).unwrap();
        assert!(r.append(4, Kind::Output(vec![1])).is_err());
    }
    #[test]
    fn rejects_large_frames_before_allocation() {
        let mut bytes = Vec::new();
        write_header(&mut bytes, Size::new(80, 24).unwrap()).unwrap();
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_from(bytes.as_slice()).is_err());
    }
    #[test]
    fn event_count_is_bounded_without_destroying_prefix() {
        let mut r = recording();
        for at in 0..MAX_EVENTS as u64 {
            r.append(at, Kind::Output(vec![b'x'])).unwrap();
        }
        assert!(r
            .append(MAX_EVENTS as u64, Kind::Output(vec![b'x']))
            .is_err());
        assert_eq!(r.events().len(), MAX_EVENTS);
    }
    #[test]
    fn checksum_matches_standard_vector() {
        assert_eq!(checksum(b"123456789"), 0xcbf43926);
    }
}
