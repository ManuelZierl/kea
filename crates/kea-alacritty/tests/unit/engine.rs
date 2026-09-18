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
    assert_eq!(replay.screen().size, Size::new(20, 4).unwrap());
    let mut live = Engine::new(log.initial_size(), true);
    live.output(b"\x1b[6n");
    assert!(!live.drain_replies().is_empty());
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
