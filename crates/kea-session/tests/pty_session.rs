use kea_core::{Kind, Size};
use kea_session::Session;
use std::time::{Duration, Instant};

#[test]
fn real_session_answers_queries_while_history_stays_read_only() {
    #[cfg(unix)]
    let command = vec![
        "sh".into(),
        "-c".into(),
        "printf kea-session-ok; exit 3".into(),
    ];
    #[cfg(windows)]
    let command = vec![
        "cmd.exe".into(),
        "/C".into(),
        "echo kea-session-ok & exit /b 3".into(),
    ];
    let mut session = Session::spawn(&command, Size::new(80, 24).unwrap(), None).unwrap();
    session.seek(0).unwrap();
    assert!(session.send(b"not-sent".to_vec()).is_err());
    let started = Instant::now();
    while session.is_running() {
        session.pump();
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "session did not exit: {:?}",
            session.warning
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(session.screen().text().trim().is_empty());
    assert_eq!(session.exit_code, Some(3));
    assert!(matches!(
        session.recording().events().last().map(|event| &event.kind),
        Some(Kind::Exit(3))
    ));
    session.go_live();
    assert!(session.screen().text().contains("kea-session-ok"));
    assert!(session.send(b"still-not-sent".to_vec()).is_err());
}
