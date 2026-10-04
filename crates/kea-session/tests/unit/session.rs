use super::*;

#[test]
fn automatic_retention_keeps_capturing_and_preserves_a_frozen_reader() {
    let size = Size::new(20, 2).unwrap();
    let mut session = Session::from_recording(Recording::new(size).unwrap()).unwrap();
    session.presentation = Presentation::Filtered(Vec::new());
    assert!(session.record_at(0, Kind::Output(b"old frame".to_vec())));
    session
        .presentation
        .push(PresentationEvent::Output(b"old frame".to_vec()));
    for at in 1..kea_core::MAX_EVENTS {
        assert!(session.record_at(at as u64, Kind::Resize(size)));
        session.presentation.push(PresentationEvent::Resize(size));
    }
    session.seek(1).unwrap();
    let frozen = session.screen().text();
    assert!(session.record_at(
        kea_core::MAX_EVENTS as u64,
        Kind::Output(b"recent".to_vec())
    ));
    session
        .presentation
        .push(PresentationEvent::Output(b"recent".to_vec()));
    assert!(!session.capture_stopped);
    assert!(session.recording.discarded_events() > 0);
    assert_eq!(session.screen().text(), frozen);
    assert!(session.warning.as_ref().unwrap().contains("trimmed"));
    session.seek_time(0).unwrap();
    assert_eq!(session.position(), session.recording.start_time());
    assert!(!session.screen().text().contains("old frame"));
    session.seek(session.recording.events().len()).unwrap();
    assert!(session.screen().text().contains("recent"));
    assert!(session.record_at(
        kea_core::MAX_EVENTS as u64 + 1,
        Kind::Output(b" next".to_vec())
    ));
    session
        .presentation
        .push(PresentationEvent::Output(b" next".to_vec()));
    session.step(1).unwrap();
    assert!(session.screen().text().contains("recent next"));
}

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

#[test]
fn frozen_grid_stays_unchanged_while_live_engine_advances_and_input_is_denied() {
    let size = Size::new(20, 3).unwrap();
    let mut session = Session::from_recording(Recording::new(size).unwrap()).unwrap();
    session.go_live();
    session.live.output(b"initial");
    session.freeze_display();
    assert!(session.is_frozen());
    assert!(!session.input_allowed());
    let frozen = session.screen().text();
    assert!(frozen.contains("initial"));
    assert!(!session.is_history());
    session.begin_terminal_selection(TerminalPoint { row: 0, column: 0 });
    session.update_terminal_selection(TerminalPoint { row: 0, column: 6 });
    assert_eq!(
        session.terminal_selection_text().as_deref(),
        Some("initial")
    );

    session.live.output(b"\r\x1b[2Kupdated");
    session.resize(Size::new(30, 5).unwrap()).unwrap();
    assert_eq!(session.screen().text(), frozen);
    assert_eq!(session.screen().size, size);
    assert_eq!(
        session.terminal_selection_text().as_deref(),
        Some("initial")
    );
    assert!(session.live.screen().text().contains("updated"));
    session.go_live();
    assert!(!session.is_frozen());
    assert!(session.screen().text().contains("updated"));
    assert!(!session.terminal_has_selection());
}

#[test]
fn freezing_playback_stops_only_the_replay_clock_and_keeps_the_displayed_frame() {
    let mut session = Session::demo().unwrap();
    session.seek(2).unwrap();
    session.toggle_playback();
    assert!(session.is_playing());
    let frame = session.screen().text();
    session.freeze_display();
    assert!(!session.is_playing());
    assert!(!session.is_history());
    assert!(session.is_frozen());
    assert_eq!(session.screen().text(), frame);
    session.seek(0).unwrap();
    assert!(!session.is_frozen());
    assert!(session.is_history());
}
