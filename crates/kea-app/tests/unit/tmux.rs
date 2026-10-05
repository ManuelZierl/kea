use super::*;

#[test]
fn parses_and_sorts_tmux_sessions() {
    let sessions = parse_sessions(b"$8\trml\t1\t0\n$2\tqaiva\t3\t2\n").unwrap();

    assert_eq!(
        sessions,
        vec![
            TmuxSession {
                id: "$2".into(),
                name: "qaiva".into(),
                windows: 3,
                attached_clients: 2,
            },
            TmuxSession {
                id: "$8".into(),
                name: "rml".into(),
                windows: 1,
                attached_clients: 0,
            },
        ]
    );
}

#[test]
fn rejects_malformed_session_ids_and_counts() {
    assert!(parse_sessions(b"qaiva\tqaiva\t1\t0\n").is_err());
    assert!(parse_sessions(b"$1\tqaiva\tmany\t0\n").is_err());
    assert!(parse_sessions(b"$1\tqaiva\t1\n").is_err());
}

#[test]
fn recognizes_normal_empty_server_errors() {
    assert!(no_server_running(
        "no server running on /tmp/tmux-1000/default"
    ));
    assert!(no_server_running("failed to connect to server"));
    assert!(no_server_running("no sessions"));
    assert!(!no_server_running("permission denied"));
}

#[test]
fn attach_command_targets_stable_tmux_id_without_a_shell() {
    let command = attach_command("$12").unwrap();
    let rendered: Vec<_> = command
        .iter()
        .map(|part| part.to_string_lossy().into_owned())
        .collect();
    assert_eq!(rendered, ["tmux", "attach-session", "-t", "$12"]);
    assert!(attach_command("name").is_err());
}
