use super::*;

fn test_token() -> MarkerToken {
    MarkerToken::from_bytes([0x5a; 16])
}

#[test]
fn detects_supported_shells() {
    assert_eq!(
        ShellFlavor::from_program("/bin/bash"),
        Some(ShellFlavor::Posix)
    );
    assert_eq!(
        ShellFlavor::from_program(r"C:\\PowerShell\\pwsh.exe"),
        Some(ShellFlavor::PowerShell)
    );
    assert_eq!(ShellFlavor::from_program("opencode"), None);
}

#[cfg(unix)]
#[test]
fn native_submission_preserves_authored_bash_history_without_driver_lines() {
    use std::ffi::OsString;
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    let installed = Command::new("bash")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !installed {
        eprintln!("skipping: bash is not installed");
        return;
    }
    let feed = |bytes: Vec<u8>| {
        bytes
            .into_iter()
            .map(|b| if b == b'\r' { b'\n' } else { b })
            .collect::<Vec<_>>()
    };
    let root = tempfile::tempdir().expect("history isolation dir");
    let histfile = root.path().join("history");
    let integration =
        feed(ShellFlavor::Posix.integration(&[OsString::from("bash")], &test_token()));
    let run = b"echo from-run\n";

    let mut child = Command::new("bash")
        .args(["--noprofile", "--norc", "-i"])
        .env("HISTFILE", &histfile)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn bash for history check");
    {
        let stdin = child.stdin.as_mut().expect("piped bash stdin");
        stdin
            .write_all(b"echo plain-command\n")
            .expect("feed plain command");
        stdin.write_all(&integration).expect("feed hook");
        stdin.write_all(run).expect("feed authored command");
        stdin.write_all(b"history\n").expect("feed history");
        stdin.write_all(b"exit\n").expect("feed exit");
    }
    let output = child.wait_with_output().expect("reap bash");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("from-run"),
        "expected the authored command to execute; got:\n{stdout}"
    );
    assert!(
        stdout
            .lines()
            .any(|line| line.split_whitespace().last() == Some("history")),
        "expected history recording to be active in this harness; got:\n{stdout}"
    );
    assert!(
        !stdout.contains("__kea_"),
        "expected no Kea driver lines in shell history; got:\n{stdout}"
    );
    let persisted = std::fs::read_to_string(&histfile).unwrap_or_default();
    assert!(
        persisted.contains("echo from-run"),
        "authored input must stay in native history"
    );
    assert!(
        !persisted.contains("__kea_"),
        "expected no Kea driver lines in the history file; got:\n{persisted}"
    );
}

#[cfg(unix)]
#[test]
fn native_submission_preserves_entry_status_in_real_bash() {
    use std::ffi::OsString;
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    let installed = Command::new("bash")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !installed {
        eprintln!("skipping: bash is not installed");
        return;
    }
    let feed = |bytes: Vec<u8>| {
        bytes
            .into_iter()
            .map(|b| if b == b'\r' { b'\n' } else { b })
            .collect::<Vec<_>>()
    };
    let integration =
        feed(ShellFlavor::Posix.integration(&[OsString::from("bash")], &test_token()));
    let failing = b"false\n";
    let probe = b"echo MARKER:$?\n";

    // Keep the probed shell out of the developer's real history file.
    let history_root = tempfile::tempdir().expect("history isolation dir");
    let mut child = Command::new("bash")
        .args(["--noprofile", "--norc", "-i"])
        .env("HISTFILE", history_root.path().join("history"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn bash for native status check");
    {
        let stdin = child.stdin.as_mut().expect("piped bash stdin");
        stdin.write_all(&integration).expect("feed hook");
        stdin.write_all(failing).expect("feed failing command");
        stdin.write_all(probe).expect("feed status probe");
        stdin.write_all(b"exit\n").expect("feed exit");
    }
    let output = child.wait_with_output().expect("reap bash");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("MARKER:1"),
        "expected the probe to observe the previous command status; got:\n{stdout}"
    );
}

#[test]
fn hooks_report_scoped_readiness_and_native_boundaries_without_authored_eval() {
    for (shell, name) in [
        (ShellFlavor::Posix, "bash"),
        (ShellFlavor::Posix, "zsh"),
        (ShellFlavor::PowerShell, "pwsh"),
    ] {
        let source =
            String::from_utf8(shell.integration_for(&[name.into()], "remote-test", &test_token()))
                .unwrap();
        assert!(source.contains("remote-test"));
        assert!(source.contains("native-start"));
        assert!(source.contains("native-done"));
        assert!(source.contains("input;1;"));
        assert!(source.contains(test_token().as_str()));
        // Every marker carries the token; no untokened form is ever emitted.
        for untokened in ["kea;native", "kea;prompt", "kea;input", "kea;path"] {
            assert!(!source.contains(untokened), "{name}: {untokened}");
        }
        assert!(!source.contains("Invoke-Expression"));
        assert!(!source.contains("eval "));
    }
}

#[cfg(unix)]
#[test]
fn exported_posix_integration_is_sourceable_text() {
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    for name in ["bash", "zsh"] {
        let installed = Command::new(name).arg("--version").output().is_ok();
        if !installed {
            continue;
        }
        let source =
            ShellFlavor::Posix.integration_for(&[name.into()], "remote-test", &test_token());
        assert_eq!(source.last(), Some(&b'\n'));
        assert!(!source.contains(&b'\r'));

        let mut child = Command::new(name)
            .arg("-n")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn shell syntax check");
        child
            .stdin
            .as_mut()
            .expect("piped shell stdin")
            .write_all(&source)
            .expect("write exported integration");
        let output = child.wait_with_output().expect("reap shell syntax check");
        assert!(
            output.status.success(),
            "{name} rejected exported integration: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let bootstrap = ShellFlavor::Posix.integration(&["bash".into()], &test_token());
    assert_eq!(bootstrap.last(), Some(&b'\r'));
}

#[cfg(windows)]
#[test]
fn powershell_hooks_preserve_prompt_and_native_status_without_wrapping_input() {
    use std::process::Command;
    // Windows PowerShell is present on supported Windows runners. This exercises
    // the real hook, not a fake parser; graphical PSReadLine acceptance is separate.
    let setup = String::from_utf8(
        ShellFlavor::PowerShell.integration(&["powershell.exe".into()], &test_token()),
    )
    .unwrap();
    let script = format!(
        "function global:prompt {{ 'CUSTOM-PROMPT> ' }}\n{setup}\ncmd /c exit 7\nprompt\n[Console]::WriteLine('NATIVE-STATUS:'+$global:LASTEXITCODE)\nWrite-Output 'AUTHORED-OUTPUT'\nprompt\n"
    );
    let output = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &script,
        ])
        .output()
        .expect("run Windows PowerShell hook test");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("CUSTOM-PROMPT>"), "{stdout}");
    assert!(stdout.contains("NATIVE-STATUS:7"), "{stdout}");
    assert!(stdout.contains("native-done;local;1"), "{stdout}");
    assert!(stdout.contains("native-done;local;0"), "{stdout}");
    assert!(stdout.contains("AUTHORED-OUTPUT"), "{stdout}");
    assert!(
        stdout.contains(&format!(
            "779;kea;{};input;1;local;powershell;ready",
            test_token().as_str()
        )),
        "{stdout}"
    );
}

#[cfg(unix)]
#[test]
fn bash_hook_preserves_prompt_command_with_trailing_separator() {
    use std::ffi::OsString;
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    let installed = Command::new("bash")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !installed {
        eprintln!("skipping: bash is not installed");
        return;
    }
    // Common profile idiom: `PROMPT_COMMAND="history -a; $PROMPT_COMMAND"`
    // leaves a trailing `; `, which a `;`-joined hook turns into `; ;`.
    for user_hook in [
        "echo user-hook; ",
        "echo user-hook &>/dev/null; echo user-hook",
        "echo user-hook\n",
    ] {
        let mut integration =
            ShellFlavor::Posix.integration(&[OsString::from("bash")], &test_token());
        for byte in &mut integration {
            if *byte == b'\r' {
                *byte = b'\n';
            }
        }
        let mut child = Command::new("bash")
            .args(["--noprofile", "--norc", "-i"])
            .env("HISTFILE", "/dev/null")
            .env("PROMPT_COMMAND", user_hook)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn bash");
        {
            let stdin = child.stdin.as_mut().expect("piped bash stdin");
            stdin.write_all(&integration).expect("feed hook");
            stdin.write_all(b"true\nexit\n").expect("feed command");
        }
        let output = child.wait_with_output().expect("reap bash");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("syntax error"),
            "hook broke PROMPT_COMMAND {user_hook:?}:\n{stderr}"
        );
        assert!(
            stdout.contains(&format!(
                "\x1b]779;kea;{};input;1;local;posix;ready\x07",
                test_token().as_str()
            )),
            "expected a ready report with PROMPT_COMMAND {user_hook:?}; got:\n{stdout}"
        );
        assert!(
            stdout.matches("user-hook").count() >= 2,
            "user PROMPT_COMMAND {user_hook:?} must keep running; got:\n{stdout}"
        );
    }
}

#[test]
fn only_bash_bootstrap_is_unprefixed_for_explicit_history_deletion() {
    use std::ffi::OsString;
    let bash =
        ShellFlavor::Posix.integration_for(&[OsString::from("/bin/bash")], "local", &test_token());
    assert!(bash.starts_with(b"if "));
    for shell in ["/bin/zsh", "/bin/sh", "/usr/bin/ksh"] {
        let script =
            ShellFlavor::Posix.integration_for(&[OsString::from(shell)], "local", &test_token());
        assert!(
            script.starts_with(b" if "),
            "{shell} bootstrap must be space-prefixed"
        );
    }
}

#[cfg(unix)]
#[test]
fn file_bootstrap_installs_hooks_removes_itself_and_stays_out_of_history() {
    use std::ffi::OsString;
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    if !Command::new("bash")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        eprintln!("skipping: bash is not installed");
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let token = test_token();
    let (line, path) =
        ShellFlavor::posix_bootstrap(&[OsString::from("bash")], &token, root.path()).unwrap();
    assert!(line.len() < 200, "the typed line must stay short");
    assert!(!String::from_utf8_lossy(&line).contains(token.as_str()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
    let histfile = root.path().join("history");
    let mut child = Command::new("bash")
        .args(["--noprofile", "--norc", "-i"])
        .env("HISTFILE", &histfile)
        .env("PROMPT_COMMAND", "echo user-hook; ")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        let mut line = line.clone();
        *line.last_mut().unwrap() = b'\n';
        stdin.write_all(&line).unwrap();
        stdin.write_all(b"echo authored\nhistory\nexit\n").unwrap();
    }
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&format!(
            "\x1b]779;kea;{};input;1;local;posix;ready\x07",
            token.as_str()
        )),
        "{stdout}"
    );
    assert!(stdout.contains("user-hook"), "{stdout}");
    assert!(!path.exists(), "the integration file removes itself");
    assert!(
        !stdout.contains(".kea-shell-"),
        "bootstrap line left in history:\n{stdout}"
    );
}
