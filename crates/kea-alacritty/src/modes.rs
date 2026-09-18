use alacritty_terminal::{grid::Dimensions, term::TermMode};

use crate::Engine;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MouseEncoding {
    Legacy,
    Utf8,
    Sgr,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MouseTracking {
    Click,
    Drag,
    Motion,
}

impl Engine {
    pub fn application_cursor(&self) -> bool {
        self.terminal.mode().contains(TermMode::APP_CURSOR)
    }

    pub fn bracketed_paste(&self) -> bool {
        self.terminal.mode().contains(TermMode::BRACKETED_PASTE)
    }

    pub fn extended_keyboard(&self) -> bool {
        self.terminal
            .mode()
            .intersects(TermMode::KITTY_KEYBOARD_PROTOCOL)
    }

    pub fn focus_reporting(&self) -> bool {
        self.terminal.mode().contains(TermMode::FOCUS_IN_OUT)
    }

    pub fn mouse_tracking(&self) -> Option<MouseTracking> {
        let mode = self.terminal.mode();
        if mode.contains(TermMode::MOUSE_MOTION) {
            Some(MouseTracking::Motion)
        } else if mode.contains(TermMode::MOUSE_DRAG) {
            Some(MouseTracking::Drag)
        } else if mode.contains(TermMode::MOUSE_REPORT_CLICK) {
            Some(MouseTracking::Click)
        } else {
            None
        }
    }

    pub fn mouse_reporting(&self) -> bool {
        self.mouse_tracking().is_some()
    }

    pub fn mouse_encoding(&self) -> Option<MouseEncoding> {
        let mode = self.terminal.mode();
        if self.mouse_tracking().is_none() {
            None
        } else if mode.contains(TermMode::SGR_MOUSE) {
            Some(MouseEncoding::Sgr)
        } else if mode.contains(TermMode::UTF8_MOUSE) {
            Some(MouseEncoding::Utf8)
        } else {
            Some(MouseEncoding::Legacy)
        }
    }

    pub fn display_offset(&self) -> usize {
        self.terminal.grid().display_offset()
    }

    pub fn history_size(&self) -> usize {
        self.terminal.grid().history_size()
    }
}

#[cfg(test)]
#[path = "../tests/unit/modes.rs"]
mod tests;
