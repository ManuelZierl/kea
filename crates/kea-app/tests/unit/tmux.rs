use super::*;

#[test]
fn parses_and_sorts_tmux_sessions() {
    let sessions =
        parse_sessions(b"$8\trml\t1\t0\t100\t200\t300\n$2\tqaiva\t3\t2\t100\t200\t301\n").unwrap();

    assert_eq!(
        sessions,
        vec![
            TmuxSession {
                id: "$2".into(),
                name: "qaiva".into(),
                windows: 3,
                attached_clients: 2,
                server_pid: 100,
                server_start_time: 200,
                session_created: 301,
            },
            TmuxSession {
                id: "$8".into(),
                name: "rml".into(),
                windows: 1,
                attached_clients: 0,
                server_pid: 100,
                server_start_time: 200,
                session_created: 300,
            },
        ]
    );
}

#[test]
fn rejects_malformed_session_ids_and_counts() {
    assert!(parse_sessions(b"qaiva\tqaiva\t1\t0\t100\t200\t300\n").is_err());
    assert!(parse_sessions(b"$1\tqaiva\tmany\t0\t100\t200\t300\n").is_err());
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
    let selected = session("$12", "alpha", 100, 200, 300);
    let command = attach_command(&selected).unwrap();
    let rendered: Vec<_> = command
        .iter()
        .map(|part| part.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        rendered,
        [
            "tmux",
            "if-shell",
            "-F",
            "-t",
            "$12",
            "#{&&:#{==:#{pid},100},#{==:#{start_time},200},#{==:#{session_created},300}}",
            "attach-session -t $12",
            "run-shell 'exit 1'"
        ]
    );
    assert!(attach_command(&session("name", "alpha", 100, 200, 300)).is_err());
}

#[test]
#[cfg(unix)]
fn command_output_is_capped_and_hung_commands_are_reaped() {
    let output = run_command("sh", ["-c", "printf '%*s' 65537 x"], Duration::from_secs(1));
    assert!(output.is_err());

    let started = std::time::Instant::now();
    let output = run_command("sh", ["-c", "sleep 2"], Duration::from_millis(50));
    assert!(output.is_err());
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn guard_contains_server_and_session_identity() {
    let old = session("$1", "old", 10, 20, 30);
    let restarted = session("$1", "new", 11, 21, 40);
    assert_ne!(guard_condition(&old), guard_condition(&restarted));
    assert!(guard_condition(&old).contains("#{pid},10"));
    assert!(guard_condition(&old).contains("#{session_created},30"));
}

fn session(
    id: &str,
    name: &str,
    server_pid: u32,
    server_start_time: u64,
    session_created: u64,
) -> TmuxSession {
    TmuxSession {
        id: id.into(),
        name: name.into(),
        windows: 1,
        attached_clients: 0,
        server_pid,
        server_start_time,
        session_created,
    }
}
