use super::*;

fn empty_session() -> Session {
    let mut session = Session::from_recording(
        kea_core::Recording::new(kea_core::Size::new(80, 24).unwrap()).unwrap(),
    )
    .unwrap();
    session.go_live();
    session.presentation = crate::presentation::Presentation::Filtered(Vec::new());
    session
}

#[test]
fn output_groups_preserve_bytes_and_live_observation_without_each_read_becoming_a_frame() {
    let mut session = empty_session();
    let mut observed = Vec::new();
    let chunks: [&[u8]; 4] = [b"\x1b[3", b"1m", b"\xe2\x82", b"\xac\xff"];
    for (index, chunk) in chunks.into_iter().enumerate() {
        session.accept_output(index as u64, chunk.to_vec(), chunk.to_vec(), &mut observed);
    }
    assert_eq!(observed.len(), 4);
    assert!(session.screen().text().contains('€'));
    assert!(session.recording.events().is_empty());
    session.flush_output();
    assert_eq!(session.recording.events().len(), 1);
    assert_eq!(session.recording.events()[0].at, 3);
    assert_eq!(
        session.recording.events()[0].kind,
        Kind::Output(chunks.concat())
    );
    let live = session.screen().text();
    session.seek(1).unwrap();
    assert_eq!(session.screen().text(), live);
}

#[test]
fn output_grouping_respects_time_size_and_metadata_boundaries() {
    let mut session = empty_session();
    let mut observed = Vec::new();
    session.accept_output(0, b"one".to_vec(), b"one".to_vec(), &mut observed);
    session.accept_output(
        OUTPUT_GROUP_MICROS,
        b"two".to_vec(),
        b"two".to_vec(),
        &mut observed,
    );
    assert_eq!(session.recording.events().len(), 1);
    let bytes = vec![b'x'; OUTPUT_GROUP_BYTES];
    session.accept_output(OUTPUT_GROUP_MICROS + 1, bytes.clone(), bytes, &mut observed);
    assert_eq!(session.recording.events().len(), 2);
    let size = kea_core::Size::new(40, 10).unwrap();
    assert!(session.record_at(OUTPUT_GROUP_MICROS + 2, Kind::Resize(size)));
    session.presentation.push(PresentationEvent::Resize(size));
    assert_eq!(session.recording.events().len(), 4);
    assert!(matches!(
        session.recording.events()[2].kind,
        Kind::Output(_)
    ));
    assert_eq!(session.recording.events()[3].kind, Kind::Resize(size));
    session.seek(4).unwrap();
    assert_eq!(session.screen().size, size);
}

#[cfg(unix)]
#[test]
fn live_capture_continues_during_rewind_and_blocks_input() {
    use super::*;
    use kea_core::Size;
    use std::time::Instant;

    let mut s = Session::spawn(
        &[
            "sh".into(),
            "-c".into(),
            "printf before; sleep 0.1; printf after".into(),
        ],
        Size::new(80, 24).unwrap(),
        None,
    )
    .unwrap();
    s.seek(0).unwrap();
    assert!(s.send(b"unexpected".to_vec()).is_err());
    let start = Instant::now();
    while s.is_running() {
        s.pump();
        assert!(start.elapsed().as_secs() < 10);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(!s.recording().events().is_empty());
    assert!(s.screen().text().trim().is_empty());
    s.go_live();
    assert!(s.screen().text().contains("beforeafter"));
}
