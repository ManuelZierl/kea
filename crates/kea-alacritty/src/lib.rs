//! Alacritty adapter. Historical engines cannot emit external effects.

mod engine;
mod modes;
mod screen;
mod selection;

pub use engine::{ClipboardStores, Engine, MAX_CLIPBOARD_STORE_BYTES};
pub use modes::{MouseEncoding, MouseTracking};
pub use screen::{Cell, Screen};
pub use selection::{SelectionMotion, TerminalPoint};

pub const TERMINAL_SCROLLBACK_LINES: usize = 10_000;
