use super::*;

#[test]
fn parses_and_sorts_tmux_sessions() {
    let sessions =
        parse_sessions(b"$8\trml\t1\t0\t100\t200\t300\n$2\tqaiva\t3\t2\t100\t200\t301\n").unwrap();

    assert_eq!(
        sessions,
        vec![
            TmuxSession {
                host: TmuxHost::default(),
                id: "$2".into(),
                name: "qaiva".into(),
                windows: 3,
                attached_clients: 2,
                server_pid: 100,
                server_start_time: 200,
                session_created: 301,
            },
            TmuxSession {
                host: TmuxHost::default(),
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
fn ssh_destinations_are_single_operands_not_commands_or_options() {
    for destination in [
        "work",
        "manuel@server.example",
        "192.0.2.1",
        "[::1]",
        "user@2001:db8::1",
    ] {
        assert_eq!(
            TmuxHost::ssh(destination).unwrap().destination(),
            Some(destination)
        );
    }
    for destination in [
        "",
        "-oProxyCommand=touch",
        "ssh work",
        "work;touch /tmp/x",
        "$(id)",
        "work\nother",
        "ssh://work",
        "work'",
        "user@host/path",
    ] {
        assert!(
            TmuxHost::ssh(destination).is_err(),
            "accepted {destination:?}"
        );
    }
    assert!(TmuxHost::ssh(&"x".repeat(256)).is_err());
}

#[test]
fn ssh_discovery_is_noninteractive_and_attach_allocates_a_remote_pty() {
    let host = TmuxHost::ssh("user@work").unwrap();
    let query = host.command(LIST_ARGS.map(String::from).into(), false);
    assert!(query
        .last()
        .unwrap()
        .starts_with("'tmux' '-u' 'list-sessions'"));
    assert_eq!(query[0], "ssh");
    for option in [
        "BatchMode=yes",
        "StrictHostKeyChecking=yes",
        "ConnectTimeout=5",
        "ClearAllForwardings=yes",
        "ForwardAgent=no",
        "-T",
    ] {
        assert!(query.iter().any(|arg| arg == option), "missing {option}");
    }
    assert_eq!(
        &query[query.len() - 3..query.len() - 1],
        ["--", "user@work"]
    );
    let attach = host.command(
        vec!["attach-session".into(), "-t".into(), "$1".into()],
        true,
    );
    assert!(attach.iter().any(|arg| arg == "-tt"));
    // Attach may prompt for a password, but never trusts an unknown host key.
    assert!(attach.iter().any(|arg| arg == "StrictHostKeyChecking=yes"));
    assert!(!attach.iter().any(|arg| arg == "BatchMode=yes"));
    assert_eq!(host.timeout(), SSH_COMMAND_TIMEOUT);
    assert_eq!(TmuxHost::default().timeout(), COMMAND_TIMEOUT);
}

#[test]
#[cfg(unix)]
fn ssh_remote_shell_preserves_every_tmux_operand_and_guard() {
    let mut selected = session("$12", "name is display only; $(false)", 100, 200, 300);
    selected.host = TmuxHost::ssh("work").unwrap();
    let command = attach_command(&selected).unwrap();
    let remote = command.last().unwrap().to_str().unwrap();
    // Simulate SSH's remote shell parsing, not just matching quote spelling.
    let script = format!("tmux() {{ printf '%s\\0' \"$@\"; }}; {remote}");
    let output = run_command("sh", ["-c", &script], Duration::from_secs(1)).unwrap();
    assert!(output.status.success());
    let mut expected = vec!["-u".to_owned()];
    expected.extend(guarded_command_args(
        &selected,
        "attach-session -t $12",
        None,
    ));
    let mut bytes = Vec::new();
    for arg in expected {
        bytes.extend(arg.as_bytes());
        bytes.push(0);
    }
    assert_eq!(output.stdout, bytes);
}

#[test]
#[cfg(unix)]
fn ssh_errors_are_not_empty_local_results_and_sessions_keep_their_host() {
    use std::os::unix::process::ExitStatusExt as _;
    let host = TmuxHost::ssh("work").unwrap();
    let output = |code, stdout: &[u8], stderr: &[u8]| Output {
        status: std::process::ExitStatus::from_raw(code << 8),
        stdout: stdout.into(),
        stderr: stderr.into(),
    };
    assert!(sessions_from_output(
        &host,
        &output(255, b"", b"identity file: No such file or directory")
    )
    .is_err());
    assert!(
        sessions_from_output(&host, &output(127, b"", b"tmux: No such file or directory")).is_err()
    );
    assert!(
        sessions_from_output(&host, &output(1, b"", b"no server running"))
            .unwrap()
            .is_empty()
    );
    let sessions =
        sessions_from_output(&host, &output(0, b"$1\twork\t1\t0\t10\t20\t30\n", b"")).unwrap();
    assert_eq!(sessions[0].host(), &host);
    let mut local = sessions[0].clone();
    local.host = TmuxHost::default();
    assert_ne!(sessions[0], local);
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
#[cfg(unix)]
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
            "-u",
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
#[cfg(not(unix))]
fn native_tmux_actions_report_unsupported_platform() {
    let selected = session("$12", "alpha", 100, 200, 300);
    assert!(attach_command(&selected)
        .unwrap_err()
        .to_string()
        .contains("supported only on Unix"));
    assert!(kill_session(&selected)
        .unwrap_err()
        .to_string()
        .contains("supported only on Unix"));
    assert!(list_sessions().is_err());
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
    let mut locale_args = vec![
        "LC_ALL=C".to_owned(),
        format!("TMUX_TMPDIR={}", tempdir.path().display()),
        "tmux".to_owned(),
        "-L".to_owned(),
        socket.clone(),
    ];
    locale_args.extend(LIST_ARGS.map(String::from));
    let c_locale = run_command(
        "env",
        locale_args.iter().map(String::as_str),
        Duration::from_secs(2),
    )
    .unwrap();
    assert!(c_locale.status.success());
    assert_eq!(
        parse_sessions(&c_locale.stdout).unwrap(),
        vec![attach_session.clone()]
    );
    if std::process::Command::new("script")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
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
        let transcript = tempdir.path().join("attach-transcript");
        let transcript_path = transcript.to_string_lossy();
        // Hosted jobs need an explicit TERM. Observe rendered output before
        // stopping the server; the regex spelling cannot match script's header
        // containing this command line. -f flushes the transcript as it arrives.
        let command = format!(
            "(for attempt in $(seq 1 200); do grep -q 'PTY[_]ATTACH[_]OK' {} 2>/dev/null && break; sleep 0.01; done; tmux -L {} kill-server) & env TERM=xterm-256color {attach_line}",
            shell_quote(&transcript_path),
            shell_quote(&socket)
        );
        let pty_result = run_command_with_tmpdir(
            "script",
            ["-qfec", &command, &transcript_path],
            Duration::from_secs(3),
            Some(tempdir.path()),
        )
        .unwrap();
        assert!(
            String::from_utf8_lossy(&pty_result.stdout).contains("PTY_ATTACH_OK"),
            "attach did not render the pane; status={} stdout={:?} stderr={:?}",
            pty_result.status,
            String::from_utf8_lossy(&pty_result.stdout),
            String::from_utf8_lossy(&pty_result.stderr)
        );
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
    let output = isolated_tmux(root, socket, LIST_ARGS);
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
        host: TmuxHost::default(),
        id: id.into(),
        name: name.into(),
        windows: 1,
        attached_clients: 0,
        server_pid,
        server_start_time,
        session_created,
    }
}

#[test]
#[cfg(unix)]
fn isolated_tmux_history_is_joined_and_guarded() {
    if std::process::Command::new("tmux")
        .arg("-V")
        .output()
        .is_err()
    {
        return;
    }
    let tempdir = tempfile::tempdir().unwrap();
    let socket = format!("kea-history-{}", std::process::id());
    let _ = isolated_tmux(tempdir.path(), &socket, ["kill-server"]);
    // A narrow pane forces soft wraps; more lines than rows forces scrollback.
    assert!(isolated_tmux(
        tempdir.path(),
        &socket,
        [
            "new-session",
            "-d",
            "-x",
            "20",
            "-y",
            "5",
            "-s",
            "history",
            "sh",
            "-c",
            "for i in $(seq 1 60); do echo \"line $i with a long tail ä\"; done; echo DONE; sleep 30",
        ]
    )
    .status
    .success());
    let session = discover_isolated_session(tempdir.path(), &socket);
    let mut text = String::new();
    for _ in 0..200 {
        let output = isolated_tmux(tempdir.path(), &socket, capture_args(&session).unwrap());
        let history = history_from_output(&output).unwrap();
        assert!(!history.truncated);
        text = history.text;
        if text.ends_with("DONE") {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(lines.first(), Some(&"line 1 with a long tail ä"), "{text}");
    assert_eq!(lines.len(), 61, "{text}");
    assert!(lines.iter().all(|line| line.trim_end() == *line));

    let stale = TmuxSession {
        session_created: session.session_created + 1,
        ..session.clone()
    };
    let output = isolated_tmux(tempdir.path(), &socket, capture_args(&stale).unwrap());
    assert!(history_from_output(&output).is_err());
    let _ = isolated_tmux(tempdir.path(), &socket, ["kill-server"]);
}

#[test]
#[cfg(unix)]
fn history_truncation_follows_tmux_history_size_not_joined_line_count() {
    use std::os::unix::process::ExitStatusExt;
    let output = |stdout: String| Output {
        status: std::process::ExitStatus::from_raw(0),
        stdout: stdout.into_bytes(),
        stderr: Vec::new(),
    };
    // Joined wrapped rows: far fewer lines than the cap, but tmux had more rows.
    let over = output(format!(
        "{HISTORY_SIZE_PREFIX}{}\nshort\nlines\n\n",
        MAX_HISTORY_LINES + 1
    ));
    let history = history_from_output(&over).unwrap();
    assert!(history.truncated);
    assert_eq!(history.text, "short\nlines");
    let complete = output(format!("{HISTORY_SIZE_PREFIX}{MAX_HISTORY_LINES}\nall\n"));
    assert!(!history_from_output(&complete).unwrap().truncated);
    // A missing size is an error, never an unlabelled possibly-partial history.
    assert!(history_from_output(&output("text only\n".into())).is_err());
}
