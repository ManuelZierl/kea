//! Platform-, shell- and renderer-independent terminal recordings.
//! Output is opaque bytes, not text. Input is deliberately absent.

mod format;
mod recording;
mod replay;
mod types;

pub use format::{read_from, write_event, write_header, Loaded};
pub use recording::Recording;
pub use replay::{replay, Projection};
pub use types::{Event, Kind, Size, MAX_BYTES, MAX_EVENTS, MAX_OUTPUT};
