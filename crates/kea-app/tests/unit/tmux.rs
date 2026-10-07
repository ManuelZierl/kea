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
    assert!(no_server_running(
        "error connecting to /tmp/tmux-1000/default (No such file or directory)"
    ));
    assert!(no_server_running("no sessions"));
    assert!(!no_server_running(
        "failed to connect to server: Connection refused"
    ));
    assert!(!no_server_running(
        "failed to connect to server: Permission denied"
    ));
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
            "#{&&:#{&&:#{==:#{pid},100},#{==:#{start_time},200}},#{==:#{session_created},300}}",
            "attach-session -t $12",
            "display-message -p 'Kea: tmux session changed; refresh and retry'"
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
    let tempdir = tempfile::tempdir().unwrap();
    let pid_path = tempdir.path().join("escaped-pipe-holder.pid");
    let script = format!("setsid sh -c 'sleep 30' & echo $! > {}", pid_path.display());
    let output = run_command("sh", ["-c", &script], Duration::from_millis(50));
    assert!(output.is_err());
    assert!(started.elapsed() < Duration::from_secs(1));
    if let Ok(pid) = std::fs::read_to_string(pid_path) {
        let _ = std::process::Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", pid.trim())])
            .status();
    }
}

#[test]
fn guard_contains_server_and_session_identity() {
    let old = session("$1", "old", 10, 20, 30);
    let restarted = session("$1", "new", 11, 21, 40);
    assert_ne!(guard_condition(&old), guard_condition(&restarted));
    assert!(guard_condition(&old).contains("#{pid},10"));
    assert!(guard_condition(&old).contains("#{session_created},30"));
}

#[test]
#[cfg(unix)]
fn isolated_tmux_fresh_kill_and_stale_attach_are_guarded() {
    if std::process::Command::new("tmux")
        .arg("-V")
        .output()
        .is_err()
    {
        return;
    }
    let tempdir = tempfile::tempdir().unwrap();
    let socket = format!("kea-unit-{}", std::process::id());
    let cleanup = || {
        let _ = isolated_tmux(tempdir.path(), &socket, ["kill-server"]);
    };
    cleanup();

    let absent = isolated_tmux(
        tempdir.path(),
        &socket,
        ["list-sessions", "-F", LIST_FORMAT],
    );
    assert!(!absent.status.success());
    assert!(no_server_running(&String::from_utf8_lossy(&absent.stderr)));

    assert!(isolated_tmux(
        tempdir.path(),
        &socket,
        [
            "new-session",
            "-d",
            "-s",
            "attach",
            "sh",
            "-c",
            "printf PTY_ATTACH_OK; sleep 30",
        ]
    )
    .status
    .success());
    let attach_session = discover_isolated_session(tempdir.path(), &socket);
    if std::process::Command::new("script")
        .arg("--version")
        .output()
        .is_ok()
    {
        let attach = attach_command(&attach_session).unwrap();
        let mut pty_args = vec!["tmux".to_owned(), "-L".to_owned(), socket.clone()];
        pty_args.extend(
            attach[1..]
                .iter()
                .map(|part| part.to_string_lossy().into_owned()),
        );
        let attach_line = pty_args
            .iter()
            .map(|part| shell_quote(part))
            .collect::<Vec<_>>()
            .join(" ");
        let command = format!(
            "(sleep 0.2; tmux -L {} kill-server) & {attach_line}",
            shell_quote(&socket)
        );
        let pty_result = run_command_with_tmpdir(
            "script",
            ["-qfec", &command, "/dev/null"],
            Duration::from_secs(2),
            Some(tempdir.path()),
        )
        .unwrap();
        assert!(String::from_utf8_lossy(&pty_result.stdout).contains("PTY_ATTACH_OK"));
        assert!(!String::from_utf8_lossy(&pty_result.stdout).contains(STALE_SESSION_MESSAGE));
    }
    assert!(isolated_tmux(
        tempdir.path(),
        &socket,
        ["new-session", "-d", "-s", "fresh"]
    )
    .status
    .success());
    let fresh = discover_isolated_session(tempdir.path(), &socket);
    let fresh_args = guarded_command_args(
        &fresh,
        &format!("kill-session -t {}", fresh.id()),
        Some(KILL_SUCCESS_MARKER),
    );
    let fresh_result = isolated_tmux(tempdir.path(), &socket, &fresh_args);
    assert!(
        fresh_result.status.success(),
        "fresh kill stderr={:?} stdout={:?}",
        String::from_utf8_lossy(&fresh_result.stderr),
        String::from_utf8_lossy(&fresh_result.stdout)
    );
    assert!(String::from_utf8_lossy(&fresh_result.stdout)
        .lines()
        .any(|line| line == KILL_SUCCESS_MARKER));
    assert!(confirm_kill(&fresh_result).is_ok());

    assert!(
        isolated_tmux(tempdir.path(), &socket, ["new-session", "-d", "-s", "old"])
            .status
            .success()
    );
    let old = discover_isolated_session(tempdir.path(), &socket);
    assert!(isolated_tmux(tempdir.path(), &socket, ["kill-server"])
        .status
        .success());
    assert!(isolated_tmux(
        tempdir.path(),
        &socket,
        ["new-session", "-d", "-s", "replacement"]
    )
    .status
    .success());

    let stale_args = guarded_command_args(
        &old,
        &format!("kill-session -t {}", old.id()),
        Some(KILL_SUCCESS_MARKER),
    );
    let stale_result = isolated_tmux(tempdir.path(), &socket, &stale_args);
    assert!(stale_result.status.success());
    let stale_output = String::from_utf8_lossy(&stale_result.stdout);
    assert!(stale_output.contains(STALE_SESSION_MESSAGE));
    assert!(!stale_output.contains(KILL_SUCCESS_MARKER));
    assert!(confirm_kill(&stale_result).is_err());
    assert!(
        isolated_tmux(tempdir.path(), &socket, ["has-session", "-t", old.id()])
            .status
            .success()
    );

    let replacement = discover_isolated_session(tempdir.path(), &socket);
    let failed_action = guarded_command_args(
        &replacement,
        "kill-session -t $999",
        Some(KILL_SUCCESS_MARKER),
    );
    let failed_result = isolated_tmux(tempdir.path(), &socket, &failed_action);
    assert!(!failed_result.status.success());
    assert!(!String::from_utf8_lossy(&failed_result.stdout).contains(KILL_SUCCESS_MARKER));
    assert!(confirm_kill(&failed_result).is_err());
    assert!(isolated_tmux(
        tempdir.path(),
        &socket,
        ["has-session", "-t", replacement.id()]
    )
    .status
    .success());

    let attach = attach_command(&old).unwrap();
    let attach_args: Vec<_> = attach[1..]
        .iter()
        .map(|part| part.to_string_lossy().into_owned())
        .collect();
    let attach_result = isolated_tmux(tempdir.path(), &socket, &attach_args);
    assert!(attach_result.status.success());
    assert!(String::from_utf8_lossy(&attach_result.stdout).contains(STALE_SESSION_MESSAGE));
    assert!(
        isolated_tmux(tempdir.path(), &socket, ["has-session", "-t", old.id()])
            .status
            .success()
    );
    cleanup();
}

#[cfg(unix)]
fn isolated_tmux<I, S>(root: &std::path::Path, socket: &str, args: I) -> std::process::Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut command = vec!["-L".to_owned(), socket.to_owned()];
    command.extend(args.into_iter().map(|arg| arg.as_ref().to_owned()));
    run_command_with_tmpdir(
        "tmux",
        command.iter().map(String::as_str),
        Duration::from_secs(2),
        Some(root),
    )
    .unwrap()
}

#[cfg(unix)]
fn discover_isolated_session(root: &std::path::Path, socket: &str) -> TmuxSession {
    let output = isolated_tmux(root, socket, ["list-sessions", "-F", LIST_FORMAT]);
    assert!(output.status.success());
    let sessions = parse_sessions(&output.stdout).unwrap();
    assert_eq!(sessions.len(), 1);
    sessions.into_iter().next().unwrap()
}

#[cfg(unix)]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
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
