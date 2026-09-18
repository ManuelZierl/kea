//! Headless session controller. Live and historical state are always separate.
mod journal;
mod observation;
mod presentation;
mod session;

pub use observation::{Observed, PumpResult};
pub use session::Session;
