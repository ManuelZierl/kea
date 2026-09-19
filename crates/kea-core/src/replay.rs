use crate::{Recording, Size};
use std::io;

/// A projection is supplied by the embedding application. It must start empty
/// at the recording's initial size. No input or process-execution API is exposed.
pub trait Projection {
    fn output(&mut self, bytes: &[u8]);
    fn resize(&mut self, size: Size);
}

pub fn replay(recording: &Recording, end: usize, into: &mut impl Projection) -> io::Result<()> {
    if end > recording.events().len() {
        return Err(crate::types::invalid("seek beyond recording"));
    }
    for event in &recording.events()[..end] {
        match &event.kind {
            crate::Kind::Output(bytes) => into.output(bytes),
            crate::Kind::Resize(size) => into.resize(*size),
            crate::Kind::Exit(_) | crate::Kind::Submitted { .. } => (),
        }
    }
    Ok(())
}
