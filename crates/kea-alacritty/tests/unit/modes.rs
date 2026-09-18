use super::*;
use crate::{Engine, TERMINAL_SCROLLBACK_LINES};
use kea_core::{Projection, Size};

#[test]
fn scrollback_retention_and_mouse_reporting_are_explicitly_bounded() {
    let mut engine = Engine::new(Size::new(4, 2).unwrap(), false);
    for _ in 0..TERMINAL_SCROLLBACK_LINES + 20 {
        engine.output(b"x\r\n");
    }
    assert_eq!(engine.history_size(), TERMINAL_SCROLLBACK_LINES);

    assert_eq!(engine.mouse_encoding(), None);
    engine.output(b"\x1b[?1000h");
    assert_eq!(engine.mouse_tracking(), Some(MouseTracking::Click));
    assert_eq!(engine.mouse_encoding(), Some(MouseEncoding::Legacy));
    engine.output(b"\x1b[?1002h");
    assert_eq!(engine.mouse_tracking(), Some(MouseTracking::Drag));
    engine.output(b"\x1b[?1003h");
    assert_eq!(engine.mouse_tracking(), Some(MouseTracking::Motion));
    engine.output(b"\x1b[?1005h");
    assert_eq!(engine.mouse_encoding(), Some(MouseEncoding::Utf8));
    engine.output(b"\x1b[?1006h");
    assert_eq!(engine.mouse_encoding(), Some(MouseEncoding::Sgr));
    engine.output(b"\x1b[?1003l\x1b[?1002l\x1b[?1000l");
    assert_eq!(engine.mouse_encoding(), None);
}

#[test]
fn focus_reporting_mode_is_tracked_by_the_terminal_engine() {
    let mut engine = Engine::new(Size::new(4, 2).unwrap(), false);
    assert!(!engine.focus_reporting());
    engine.output(b"\x1b[?1004h");
    assert!(engine.focus_reporting());
    engine.output(b"\x1b[?1004l");
    assert!(!engine.focus_reporting());
}
