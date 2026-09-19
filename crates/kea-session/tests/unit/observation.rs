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
