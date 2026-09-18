use std::io;

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

pub(crate) fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
