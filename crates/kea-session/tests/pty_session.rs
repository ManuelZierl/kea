use kea_core::{Kind, Size};
use kea_session::Session;
use std::time::{Duration, Instant};

#[cfg(unix)]
#[test]
fn frozen_selection_blocks_user_input_but_keeps_child_replies_and_recording_live() {
    let command = vec![
        "sh".into(),
        "-c".into(),
        r#"stty raw -echo; printf initial; dd bs=1 count=1 >/dev/null 2>&1; printf '\033[6n'; answer=$(dd bs=1 count=6 2>/dev/null); [ "$answer" = "$(printf '\033[1;8R')" ] || exit 7; printf '\r\033[2Kchanged'"#.into(),
    ];
    let mut session = Session::spawn(&command, Size::new(20, 3).unwrap(), None).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !session.screen().text().contains("initial") {
        session.pump();
        assert!(Instant::now() < deadline, "initial output missing");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(session.input_allowed());
    session.send(b"g".to_vec()).unwrap();
    session.freeze_display();
    assert!(session.send(b"must not reach child".to_vec()).is_err());
    session.begin_terminal_selection(kea_alacritty::TerminalPoint { row: 0, column: 0 });
    session.update_terminal_selection(kea_alacritty::TerminalPoint { row: 0, column: 6 });
    while session.is_running() {
        session.pump();
        assert!(
            Instant::now() < deadline,
            "child did not receive its protocol reply"
        );
        assert_eq!(
            session.terminal_selection_text().as_deref(),
            Some("initial")
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(session.exit_code, Some(0));
    let bytes: Vec<_> = session
        .recording()
        .events()
        .iter()
        .filter_map(|event| match &event.kind {
            Kind::Output(bytes) => Some(bytes.as_slice()),
            _ => None,
        })
        .flatten()
        .copied()
        .collect();
    assert!(bytes.windows(7).any(|bytes| bytes == b"changed"));
    assert!(session.screen().text().contains("initial"));
    session.go_live();
    assert!(session.screen().text().contains("changed"));
    assert!(!session.terminal_has_selection());
}

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
