use super::*;

#[test]
fn offline_playback_never_accepts_input_even_after_go_live() {
    let mut s = Session::demo().unwrap();
    assert!(s.send(b"rm -rf not-executed".to_vec()).is_err());
    s.go_live();
    assert!(s.send(b"echo not-executed".to_vec()).is_err());
}

#[test]
fn history_view_rejects_sends_until_back_live() {
    // The composer stays editable in history, but no draft text may reach
    // the live process until the view returns to LIVE.
    let mut s = Session::demo().unwrap();
    s.seek(0).unwrap();
    assert!(s.is_history());
    assert!(s.send(b"not-executed".to_vec()).is_err());
    s.go_live();
    assert!(!s.is_history());
}

#[test]
fn seeking_restores_error_and_preserves_latest_state() {
    let mut s = Session::demo().unwrap();
    s.seek(2).unwrap();
    assert!(s.screen().text().contains("ERROR: connection failed"));
    s.go_live();
    assert!(!s.screen().text().contains("ERROR: connection failed"));
    assert!(s.screen().text().contains("Ready."));
}

#[test]
fn time_seek_preserves_the_requested_playhead_between_sparse_events() {
    let mut recording = Recording::new(Size::new(20, 2).unwrap()).unwrap();
    recording
        .append(1_000_000, Kind::Output(b"first".to_vec()))
        .unwrap();
    recording
        .append(4_000_000, Kind::Output(b" second".to_vec()))
        .unwrap();
    let mut session = Session::from_recording(recording).unwrap();

    session.seek_time(2_500_000).unwrap();

    assert_eq!(session.position(), 2_500_000);
    assert!(session.screen().text().contains("first"));
    assert!(!session.screen().text().contains("second"));
}

#[test]
fn historical_viewport_scroll_does_not_move_the_live_viewport() {
    let mut recording = Recording::new(Size::new(8, 3).unwrap()).unwrap();
    recording
        .append(0, Kind::Output(b"one\r\ntwo\r\nthree\r\nfour".to_vec()))
        .unwrap();
    let mut session = Session::from_recording(recording).unwrap();

    session.scroll_lines(2);
    assert!(session.display_offset() > 0);
    session.go_live();
    assert_eq!(session.display_offset(), 0);
    assert!(session.history_size() > 0);
}

#[test]
fn resizing_history_changes_only_the_display_projection() {
    let original_size = Size::new(8, 3).unwrap();
    let recording = Recording::new(original_size).unwrap();
    let mut session = Session::from_recording(recording).unwrap();
    let event_count = session.recording().events().len();

    session.resize(Size::new(12, 5).unwrap()).unwrap();
    assert_eq!(session.screen().size, Size::new(12, 5).unwrap());
    assert_eq!(session.recording().events().len(), event_count);

    session.go_live();
    assert_eq!(session.screen().size, original_size);
}

#[test]
fn terminal_selection_controls_forward_to_displayed_engine() {
    let size = Size::new(12, 3).unwrap();
    let mut recording = Recording::new(size).unwrap();
    recording
        .append(0, Kind::Output(b"one\r\ntwo\r\nthree".to_vec()))
        .unwrap();
    let mut session = Session::from_recording(recording).unwrap();

    session.enter_terminal_selection();
    assert!(session.terminal_local_selection_active());
    assert!(session.terminal_explicit_selection_active());
    session.move_terminal_selection(SelectionMotion::Right, true);
    assert!(session.terminal_has_selection());
    session.terminal_selection_focus_lost();
    assert!(session.terminal_has_selection());
    session.clear_terminal_selection();
    assert!(!session.terminal_local_selection_active());
}
