use crate::{Engine, SelectionMotion, TerminalPoint, TERMINAL_SCROLLBACK_LINES};
use kea_core::{Projection, Size};

#[test]
fn terminal_selection_uses_viewport_coordinates_and_copies_only_selection() {
    let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
    engine.output(b"hello");
    engine.begin_selection(TerminalPoint { row: 0, column: 1 });
    engine.update_selection(TerminalPoint { row: 0, column: 3 });

    assert_eq!(engine.selection_text().as_deref(), Some("ell"));
    let screen = engine.screen();
    let selected = screen
        .cells
        .iter()
        .enumerate()
        .filter_map(|(index, cell)| cell.selected.then_some(index))
        .collect::<Vec<_>>();
    assert_eq!(selected, vec![1, 2, 3]);

    engine.begin_selection(TerminalPoint { row: 0, column: 3 });
    engine.update_selection(TerminalPoint { row: 0, column: 1 });
    assert_eq!(engine.selection_text().as_deref(), Some("ell"));

    engine.clear_selection();
    assert!(engine.selection_text().is_none());
    assert!(engine.screen().cells.iter().all(|cell| !cell.selected));
}

#[test]
fn terminal_selection_works_in_a_mouse_reporting_alternate_screen() {
    let mut engine = Engine::new(Size::new(16, 3).unwrap(), false);
    engine.output(b"\x1b[?1049h\x1b[H\x1b[?1000hOpenCode output");
    assert!(engine.mouse_reporting());

    engine.begin_selection(TerminalPoint { row: 0, column: 0 });
    engine.update_selection(TerminalPoint { row: 0, column: 7 });

    assert_eq!(engine.selection_text().as_deref(), Some("OpenCode"));
    assert!(engine.screen().cells.iter().any(|cell| cell.selected));
}

#[test]
fn appended_output_preserves_a_scrollback_selection() {
    let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
    engine.output(b"one\r\ntwo\r\nthree\r\nfour");
    engine.scroll_lines(1);
    engine.begin_selection(TerminalPoint { row: 0, column: 0 });
    engine.update_selection(TerminalPoint { row: 0, column: 2 });
    let selected = engine.selection_text();

    engine.output(b"\r\nfive");

    assert_eq!(engine.selection_text(), selected);
    assert!(engine.screen().cells.iter().any(|cell| cell.selected));
}

/// Manual acceptance: select, scroll the viewport, then copy. Scrolling
/// must neither move nor lose the content-anchored selection, and further
/// drags must extend from the current viewport offset.
#[test]
fn scrolling_preserves_and_extends_a_scrollback_selection() {
    let mut engine = Engine::new(Size::new(8, 4).unwrap(), false);
    engine.output(b"L1\r\nL2\r\nL3\r\nL4\r\nL5");
    // Viewport shows L2..L5; select the bottom line.
    engine.begin_selection(TerminalPoint { row: 3, column: 0 });
    engine.update_selection(TerminalPoint { row: 3, column: 1 });
    assert_eq!(engine.selection_text().as_deref(), Some("L5"));

    // Scroll two lines up: the selection stays glued to L5 content even
    // though L5 has left the viewport.
    engine.scroll_lines(2);
    assert!(engine.display_offset() > 0);
    assert_eq!(engine.selection_text().as_deref(), Some("L5"));

    // Extend from the new viewport top (now showing L1) while the button is
    // still held: the anchor stays at the original press, so the selection
    // grows back across the scrolled content.
    engine.update_selection(TerminalPoint { row: 0, column: 1 });
    assert_eq!(engine.selection_text().as_deref(), Some("1\nL2\nL3\nL4\nL"));
}

#[test]
fn block_selection_and_local_caret_are_distinct() {
    let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
    engine.output(b"abcd\r\nefgh");
    engine.begin_selection_kind(TerminalPoint { row: 0, column: 1 }, true);
    engine.update_selection(TerminalPoint { row: 1, column: 2 });
    assert_eq!(engine.selection_text().as_deref(), Some("bc\nfg"));
    assert!(!engine.explicit_selection_active());

    engine.clear_selection();
    engine.enter_local_selection();
    assert!(engine.local_selection_active());
    assert!(engine.screen().local_caret.is_some());
    assert!(engine.selection_text().is_none());
}

#[test]
fn pointer_press_is_empty_until_extended_and_remains_valid_after_resize() {
    let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
    engine.output(b"abcdefgh");
    engine.begin_selection(TerminalPoint { row: 2, column: 7 });
    assert!(!engine.has_selection());
    assert!(engine.selection_text().is_none());
    engine.resize(Size::new(3, 1).unwrap());
    engine.update_selection(TerminalPoint { row: 0, column: 0 });
    assert!(engine.has_selection());
    assert_eq!(engine.screen().local_caret, Some((0, 0)));
}

#[test]
fn caret_and_selection_head_stay_visible_during_page_navigation() {
    let mut engine = Engine::new(Size::new(8, 4).unwrap(), false);
    engine.output(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven\r\neight\r\nnine");
    engine.place_selection_caret(TerminalPoint { row: 2, column: 1 });
    engine.move_selection(SelectionMotion::PageUp, true);
    assert_eq!(engine.display_offset(), 4);
    assert_eq!(engine.screen().local_caret, Some((2, 0)));
    assert!(engine.has_selection());
    engine.move_selection(SelectionMotion::Left, false);
    assert!(engine.screen().local_caret.is_some());
    engine.move_selection(SelectionMotion::PageDown, false);
    assert_eq!(engine.display_offset(), 0);
    assert!(engine.screen().local_caret.is_some());
    assert!(!engine.has_selection());
}

#[test]
fn focus_loss_clears_caret_but_preserves_a_range() {
    let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
    engine.output(b"text");
    engine.enter_local_selection();
    engine.selection_focus_lost();
    assert!(!engine.local_selection_active());
    engine.begin_selection(TerminalPoint { row: 0, column: 0 });
    engine.update_selection(TerminalPoint { row: 0, column: 2 });
    engine.selection_focus_lost();
    assert_eq!(engine.selection_text().as_deref(), Some("tex"));
    engine.move_selection(SelectionMotion::Right, false);
    engine.selection_focus_lost();
    assert!(!engine.local_selection_active());
}

#[test]
fn selection_uses_alacritty_wide_and_wrapped_cell_semantics() {
    let mut engine = Engine::new(Size::new(4, 3).unwrap(), false);
    engine.output("ab界d".as_bytes());
    engine.begin_selection(TerminalPoint { row: 0, column: 1 });
    engine.update_selection(TerminalPoint { row: 1, column: 1 });
    assert_eq!(engine.selection_text().as_deref(), Some("b界d"));
}

#[test]
fn navigation_collapses_then_extends_and_pages() {
    let mut engine = Engine::new(Size::new(8, 4).unwrap(), false);
    engine.output(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix");
    engine.begin_selection_kind(TerminalPoint { row: 1, column: 0 }, false);
    engine.update_selection(TerminalPoint { row: 1, column: 2 });
    engine.move_selection(SelectionMotion::Left, false);
    assert!(engine.explicit_selection_active());
    assert!(!engine.has_selection());
    engine.move_selection(SelectionMotion::Right, true);
    assert!(engine.has_selection());
    engine.move_selection(SelectionMotion::PageDown, false);
    assert!(!engine.has_selection());
    assert!(engine.screen().local_caret.is_some());
}

#[test]
fn collapse_does_not_take_an_extra_step_and_vertical_collapse_moves_head() {
    let mut engine = Engine::new(Size::new(8, 4).unwrap(), false);
    engine.output(b"zero\r\none\r\ntwo");
    engine.begin_selection(TerminalPoint { row: 1, column: 1 });
    engine.update_selection(TerminalPoint { row: 1, column: 3 });
    engine.move_selection(SelectionMotion::Left, false);
    assert_eq!(engine.screen().local_caret, Some((1, 1)));
    engine.move_selection(SelectionMotion::Left, false);
    assert_eq!(engine.screen().local_caret, Some((1, 0)));

    engine.begin_selection(TerminalPoint { row: 0, column: 0 });
    engine.update_selection(TerminalPoint { row: 1, column: 2 });
    engine.move_selection(SelectionMotion::Down, false);
    assert_eq!(engine.screen().local_caret, Some((2, 2)));
}

#[test]
fn home_and_end_stop_at_visual_row_boundaries() {
    let mut engine = Engine::new(Size::new(4, 3).unwrap(), false);
    engine.output(b"abcdef");
    engine.place_selection_caret(TerminalPoint { row: 0, column: 1 });
    engine.move_selection(SelectionMotion::End, false);
    assert_eq!(engine.screen().local_caret, Some((0, 3)));
    engine.move_selection(SelectionMotion::Home, false);
    assert_eq!(engine.screen().local_caret, Some((0, 0)));
}

#[test]
fn caret_extension_creates_a_simple_range_and_preserves_explicit_ownership() {
    let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
    engine.output(b"abcdef");
    engine.enter_local_selection();
    engine.place_selection_caret(TerminalPoint { row: 0, column: 1 });
    engine.extend_selection(TerminalPoint { row: 0, column: 3 });
    assert_eq!(engine.selection_text().as_deref(), Some("bcd"));
    assert!(engine.explicit_selection_active());
    engine.clear_selection();
    engine.enter_local_selection();
    engine.begin_selection_kind(TerminalPoint { row: 0, column: 1 }, false);
    engine.update_selection(TerminalPoint { row: 0, column: 2 });
    assert!(engine.explicit_selection_active());
}

#[test]
fn reverse_block_axes_remain_independent_when_scrolled() {
    let mut engine = Engine::new(Size::new(8, 4).unwrap(), false);
    engine.output(b"abcdefgh\r\nijklmnop\r\nqrstuvwx\r\nyzABCDEF\r\nGHIJKLMN");
    engine.begin_selection_kind(TerminalPoint { row: 2, column: 5 }, true);
    engine.update_selection(TerminalPoint { row: 1, column: 7 });
    engine.scroll_lines(1);
    engine.update_selection(TerminalPoint { row: 0, column: 6 });
    assert_eq!(engine.selection_text().as_deref(), Some("fg\nno\nvw\nDE"));
}

#[test]
fn invalidated_selection_becomes_a_caret() {
    let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
    engine.output(b"old text");
    engine.begin_selection(TerminalPoint { row: 0, column: 0 });
    engine.update_selection(TerminalPoint { row: 0, column: 2 });
    engine.output(b"\x1b[2J");
    assert!(!engine.has_selection());
    assert!(engine.local_selection_active());
    assert!(engine.selection_invalidated());
    assert!(engine.screen().local_caret.is_some());
}

#[test]
fn overwriting_selected_cells_invalidates_even_if_alacritty_retains_range() {
    let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
    engine.output(b"old text");
    engine.begin_selection(TerminalPoint { row: 0, column: 0 });
    engine.update_selection(TerminalPoint { row: 0, column: 2 });
    engine.output(b"\x1b[1;1HX");
    assert!(!engine.has_selection());
    assert!(engine.selection_invalidated());
    assert!(engine.screen().local_caret.is_some());
}

#[test]
fn buffer_switch_and_eviction_never_resurrect_a_stale_selection() {
    let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
    engine.output(b"selected");
    engine.begin_selection(TerminalPoint { row: 0, column: 0 });
    engine.update_selection(TerminalPoint { row: 0, column: 2 });
    engine.output(b"\x1b[?1049h\x1b[2Jalt\x1b[?1049l");
    assert!(!engine.has_selection());
    assert!(engine.selection_invalidated());
    assert!(engine.screen().local_caret.is_some());

    engine.clear_selection();
    engine.output(b"primary\r\n");
    engine.begin_selection(TerminalPoint { row: 0, column: 0 });
    engine.update_selection(TerminalPoint { row: 0, column: 2 });
    for _ in 0..TERMINAL_SCROLLBACK_LINES + 2 {
        engine.output(b"x\r\n");
    }
    assert!(!engine.has_selection());
    assert!(engine.selection_invalidated());
    assert!(engine.screen().local_caret.is_some());
}

#[test]
fn caret_is_clamped_after_resize_and_history_eviction() {
    let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
    engine.output(b"caret");
    engine.place_selection_caret(TerminalPoint { row: 2, column: 7 });
    engine.resize(Size::new(3, 1).unwrap());
    assert_eq!(engine.screen().local_caret, Some((0, 2)));
    for _ in 0..TERMINAL_SCROLLBACK_LINES + 2 {
        engine.output(b"x\r\n");
    }
    let caret = engine.screen().local_caret.expect("caret remains visible");
    assert!(caret.0 < 1 && caret.1 < 3);
}

#[test]
fn resizing_columns_invalidates_selection_but_resize_preserves_rows() {
    let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
    engine.output(b"wide content");
    engine.begin_selection(TerminalPoint { row: 0, column: 0 });
    engine.update_selection(TerminalPoint { row: 0, column: 3 });
    engine.resize(Size::new(10, 3).unwrap());
    assert!(!engine.has_selection());
    assert!(engine.selection_invalidated());
    assert!(engine.screen().local_caret.is_some());
}
