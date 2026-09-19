use super::*;

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
    let integration = feed(ShellFlavor::Posix.integration(&[OsString::from("bash")]));
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
    let integration = feed(ShellFlavor::Posix.integration(&[OsString::from("bash")]));
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
        "expected the probe to observe the previous Run status; got:\n{stdout}"
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
            String::from_utf8(shell.integration_for(&[name.into()], "remote-test")).unwrap();
        assert!(source.contains("remote-test"));
        assert!(source.contains("native-start"));
        assert!(source.contains("native-done"));
        assert!(source.contains("779;kea;input;1;"));
        assert!(!source.contains("Invoke-Expression"));
        assert!(!source.contains("eval "));
    }
}

#[cfg(windows)]
#[test]
fn powershell_hooks_preserve_prompt_and_native_status_without_wrapping_input() {
    use std::process::Command;
    // Windows PowerShell is present on supported Windows runners. This exercises
    // the real hook, not a fake parser; graphical PSReadLine acceptance is separate.
    let setup =
        String::from_utf8(ShellFlavor::PowerShell.integration(&["powershell.exe".into()])).unwrap();
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
        stdout.contains("779;kea;input;1;local;powershell;ready"),
        "{stdout}"
    );
}
