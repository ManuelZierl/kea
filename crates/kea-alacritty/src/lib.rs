//! Alacritty adapter. Historical engines cannot emit external effects.

mod engine;
mod modes;
mod screen;
mod selection;

pub use engine::Engine;
pub use modes::{MouseEncoding, MouseTracking};
pub use screen::{Cell, Screen};
pub use selection::{SelectionMotion, TerminalPoint};

pub const TERMINAL_SCROLLBACK_LINES: usize = 10_000;
