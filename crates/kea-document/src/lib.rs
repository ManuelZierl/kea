//! Structured command/output documents layered over an ordinary terminal session.
//!
//! The terminal byte stream remains the compatibility substrate. Document mode adds
//! private OSC markers so command boundaries never have
//! to be guessed from prompts or terminal text.

mod encoding;
mod markers;
mod model;
mod text;

pub use encoding::encode_input;
pub use model::{
    status_label, CommandBlock, CommandStatus, Document, Error, MAX_BLOCKS, MAX_BLOCK_OUTPUT,
    MAX_COMMAND_BYTES, MAX_DOCUMENT_BYTES,
};
