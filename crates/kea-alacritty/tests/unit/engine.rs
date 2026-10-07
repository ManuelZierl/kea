use super::*;
use kea_core::Kind;

fn log() -> Recording {
    Recording::new(Size::new(40, 6).unwrap()).unwrap()
}

#[test]
fn overwritten_error_is_recoverable() {
    let mut log = log();
    log.append(1, Kind::Output(b"ERROR: connection failed".to_vec()))
        .unwrap();
    log.append(2, Kind::Output(b"\r\x1b[2KReady".to_vec()))
        .unwrap();
    assert!(Engine::at(&log, 1)
        .unwrap()
        .screen()
        .text()
        .contains("ERROR"));
    assert!(!Engine::at(&log, 2)
        .unwrap()
        .screen()
        .text()
        .contains("ERROR"));
}

#[test]
fn alternate_screen_is_preserved_in_history() {
    let mut log = log();
    log.append(1, Kind::Output(b"shell".to_vec())).unwrap();
    log.append(2, Kind::Output(b"\x1b[?1049h\x1b[Htransient".to_vec()))
        .unwrap();
    log.append(3, Kind::Output(b"\x1b[?1049l".to_vec()))
        .unwrap();
    assert!(Engine::at(&log, 2)
        .unwrap()
        .screen()
        .text()
        .contains("transient"));
    assert!(Engine::at(&log, 3)
        .unwrap()
        .screen()
        .text()
        .contains("shell"));
}

#[test]
fn split_utf8_and_escape_sequences_survive_record_boundaries() {
    let mut log = log();
    for (i, byte) in "\x1b[31mKea 🦜\x1b[0m".as_bytes().iter().enumerate() {
        log.append(i as u64, Kind::Output(vec![*byte])).unwrap();
    }
    assert!(Engine::at(&log, log.events().len())
        .unwrap()
        .screen()
        .text()
        .contains("Kea 🦜"));
}

#[test]
fn replay_is_silent_and_resize_is_replayed() {
    let mut log = log();
    log.append(1, Kind::Resize(Size::new(20, 4).unwrap()))
        .unwrap();
    log.append(2, Kind::Output(b"\x1b[6n\x1b]52;c;YQ==\x07".to_vec()))
        .unwrap();
    let mut replay = Engine::at(&log, 2).unwrap();
    assert!(replay.drain_replies().is_empty());
    assert_eq!(replay.drain_clipboard_stores(), ClipboardStores::default());
    assert_eq!(replay.screen().size, Size::new(20, 4).unwrap());
    let mut live = Engine::new(log.initial_size(), true);
    live.output(b"\x1b[6n");
    assert!(!live.drain_replies().is_empty());
}

#[test]
fn live_engine_surfaces_clipboard_stores_but_not_primary_selection() {
    let mut live = Engine::new(Size::new(40, 6).unwrap(), true);
    live.output(b"\x1b]52;c;S0VBX09TQzUyX1RFU1Q=\x07");
    assert_eq!(
        live.drain_clipboard_stores().latest,
        Some("KEA_OSC52_TEST".to_string())
    );

    live.output(b"\x1b]52;p;cHJpbWFyeQ==\x07");
    assert_eq!(live.drain_clipboard_stores(), ClipboardStores::default());
}

#[test]
fn live_engine_keeps_only_the_latest_accepted_clipboard_store() {
    let mut live = Engine::new(Size::new(40, 6).unwrap(), true);
    live.output(b"\x1b]52;c;Zmlyc3Q=\x07\x1b]52;c;c2Vjb25k\x07\x1b]52;c;dGhpcmQ=\x07");
    let stores = live.drain_clipboard_stores();
    assert_eq!(stores.latest, Some("third".to_string()));
    assert_eq!(stores.rejected, 0);
}

#[test]
fn live_engine_rejects_oversized_clipboard_store_and_reports_it() {
    let mut live = Engine::new(Size::new(40, 6).unwrap(), true);
    let encoded = "AAAA".repeat((MAX_CLIPBOARD_STORE_BYTES / 3) + 1);
    live.output(format!("\x1b]52;c;{encoded}\x07").as_bytes());
    let stores = live.drain_clipboard_stores();
    assert_eq!(stores.latest, None);
    assert_eq!(stores.rejected, 1);
}

#[test]
fn output_does_not_return_a_scrolled_reader_to_the_tail() {
    let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
    engine.output(b"one\r\ntwo\r\nthree\r\nfour");
    engine.scroll_lines(1);
    let before = engine.display_offset();

    engine.output(b"\r\nfive");

    assert!(engine.display_offset() >= before);
    assert_ne!(engine.display_offset(), 0);
}
#[test]
fn frozen_grid_preserves_active_buffer_colors_cursor_and_native_selection() {
    use crate::TerminalPoint;
    for alternate in [false, true] {
        let mut live = Engine::new(Size::new(8, 3).unwrap(), true);
        if alternate {
            live.output(b"\x1b[?1049h");
        }
        live.output(
            "\x1b]4;1;rgb:12/34/56\x07\x1b[31mab界e\u{301}XYZ0123\r\nlast\x1b[?25l".as_bytes(),
        );
        let before = live.screen();
        let mut frozen = live.frozen_grid();
        let actual = frozen.screen();
        assert_eq!(before.text(), actual.text());
        assert_eq!(before.cursor, actual.cursor);
        assert_eq!(before.history_size, actual.history_size);
        for (expected, actual) in before.cells.iter().zip(&actual.cells) {
            assert_eq!(expected.foreground, actual.foreground);
            assert_eq!(expected.background, actual.background);
            assert_eq!(expected.wide, actual.wide);
            assert_eq!(expected.spacer, actual.spacer);
        }
        for block in [false, true] {
            let start = TerminalPoint { row: 0, column: 1 };
            let end = TerminalPoint { row: 1, column: 3 };
            live.begin_selection_kind(start, block);
            live.update_selection(end);
            frozen.begin_selection_kind(start, block);
            frozen.update_selection(end);
            assert!(frozen.selection_text().is_some());
            assert_eq!(frozen.selection_text(), live.selection_text());
        }
        let selected = frozen.selection_text();
        live.output(b"\x1b[2J\x1b[Hchanged\x1b[6n");
        assert!(!live.drain_replies().is_empty());
        assert!(frozen.drain_replies().is_empty());
        assert_eq!(frozen.drain_clipboard_stores(), ClipboardStores::default());
        assert_eq!(frozen.screen().text(), before.text());
        assert_eq!(frozen.selection_text(), selected);
        assert!(!live.screen().text().contains("last"));
    }
}

#[test]
fn frozen_grid_preserves_selection_anchor_head_and_explicit_caret() {
    use crate::TerminalPoint;

    let mut live = Engine::new(Size::new(12, 2).unwrap(), false);
    live.output(b"abcdef");
    live.begin_selection(TerminalPoint { row: 0, column: 1 });
    live.update_selection(TerminalPoint { row: 0, column: 4 });
    let mut frozen = live.frozen_grid();

    assert_eq!(frozen.selection_text(), live.selection_text());
    frozen.extend_selection(TerminalPoint { row: 0, column: 5 });
    assert_eq!(frozen.selection_text(), Some("bcdef".into()));

    live.place_selection_caret(TerminalPoint { row: 0, column: 3 });
    let frozen = live.frozen_grid();
    assert!(frozen.explicit_selection_active());
    assert!(frozen.local_selection_active());
    assert!(frozen.selection_text().is_none());
}

#[test]
fn frozen_grid_keeps_scrollback_offset_when_live_output_scrolls() {
    let mut live = Engine::new(Size::new(12, 3).unwrap(), true);
    live.output(b"one\r\ntwo\r\nthree\r\nfour\r\nfive");
    live.scroll_lines(2);
    let mut frozen = live.frozen_grid();
    let frame = frozen.screen().text();
    assert!(frozen.display_offset() > 0);
    assert_eq!(frozen.display_offset(), live.display_offset());
    live.output(b"\r\nsix\r\nseven");
    assert_eq!(frozen.screen().text(), frame);
    frozen.scroll_bottom();
    assert!(frozen.screen().text().contains("five"));
    assert!(!frozen.screen().text().contains("seven"));
}
