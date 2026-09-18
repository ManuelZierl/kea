use super::*;

#[test]
fn echo_filter_removes_only_the_hidden_input_across_chunks() {
    let mut filter = EchoFilter::new(b"secret wrapper\r".to_vec()).unwrap();
    let (first, done) = filter.push(b"prompt> secret wr");
    assert_eq!(first, b"prompt> ");
    assert!(!done);
    let (second, done) = filter.push(b"apper\r\nactual output");
    assert_eq!(second, b"actual output");
    assert!(!done);
    let (third, done) = filter.push(b"\x1b]777;kea;start;1;ZWNobyBoaQ==\x07");
    assert_eq!(third, b"\x1b]777;kea;start;1;ZWNobyBoaQ==\x07");
    assert!(done);
}

#[test]
fn echo_filter_removes_readline_redraw_after_the_initial_tty_echo() {
    let input = b"private integration\r".to_vec();
    let mut filter = EchoFilter::new(input.clone()).unwrap();
    let (first, done) = filter.push(b"private integration\r\n");
    assert!(first.is_empty());
    assert!(!done);

    let mut redraw = b"bash$ ".to_vec();
    redraw.extend_from_slice(&input);
    redraw.extend_from_slice(b"\n\x1b]777;kea;prompt;L3RtcA==\x07bash$ ");
    let (second, done) = filter.push(&redraw);

    assert!(done);
    assert_eq!(second, b"bash$ \x1b]777;kea;prompt;L3RtcA==\x07bash$ ");
}

#[test]
fn echo_filter_fails_open_when_shell_redraws_before_start_marker() {
    let mut filter = EchoFilter::new(b"wrapper that will not match\r".to_vec()).unwrap();
    let (visible, done) =
        filter.push(b"\x1b[2Kredrawn wrapper\r\n\x1b]777;kea;start;1;ZWNobyBoaQ==\x07hi");
    assert!(done);
    let marker = b"\x1b]777;kea;sta";
    assert!(visible.windows(marker.len()).any(|window| window == marker));
}

#[test]
fn echo_filter_rejects_an_empty_hidden_input() {
    assert!(EchoFilter::new(Vec::new()).is_err());
}

#[test]
fn presentation_flushes_fail_open_bytes_at_the_aligned_exit_event() {
    let size = Size::new(20, 2).unwrap();
    let mut recording = Recording::new(size).unwrap();
    recording
        .append(1, Kind::Output(b"private-prefix".to_vec()))
        .unwrap();
    recording.append(2, Kind::Exit(0)).unwrap();
    let presentation = Presentation::Filtered(vec![
        PresentationEvent::Output(Vec::new()),
        PresentationEvent::Exit(b"private-prefix".to_vec()),
    ]);
    let mut replay = Engine::new(size, false);

    presentation.apply(&recording, 0, &mut replay).unwrap();
    assert!(!replay.screen().text().contains("private-prefix"));
    presentation.apply(&recording, 1, &mut replay).unwrap();
    assert!(replay.screen().text().contains("private-prefix"));
    assert!(matches!(
        &recording.events()[0].kind,
        Kind::Output(bytes) if bytes == b"private-prefix"
    ));
}
