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

#[test]
fn wrappers_transport_multiline_input_as_one_physical_line() {
    let input = "ls\nls\nprintf 'ä\\n'\n";
    for shell in [ShellFlavor::Posix, ShellFlavor::PowerShell] {
        let wrapper = shell.wrap(42, input, 7).unwrap();
        assert_eq!(wrapper.last(), Some(&b'\r'));
        assert!(!wrapper[..wrapper.len() - 1].contains(&b'\n'));
        assert!(String::from_utf8_lossy(&wrapper).contains("start;42"));
        assert!(String::from_utf8_lossy(&wrapper).contains("done;42"));
    }
}

#[test]
fn posix_wrapper_restores_user_exit_status_after_instrumentation() {
    let wrapper = String::from_utf8(ShellFlavor::Posix.wrap(7, "false", 0).unwrap()).unwrap();
    assert!(wrapper.contains("__kea_status_7=$?"));
    assert!(wrapper.contains("__kea_source_7_kea_rc=$?"));
    assert!(wrapper.ends_with("(exit \"${__kea_source_7_kea_rc}\")\r"));
}

#[cfg(unix)]
#[test]
fn posix_installation_keeps_wrapper_lines_out_of_bash_history() {
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
    let run = feed(ShellFlavor::Posix.wrap(7, "echo from-run", 0).unwrap());

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
        stdin.write_all(&run).expect("feed run wrapper");
        stdin.write_all(b"history\n").expect("feed history");
        stdin.write_all(b"exit\n").expect("feed exit");
    }
    let output = child.wait_with_output().expect("reap bash");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("from-run"),
        "expected the Run wrapper to execute; got:\n{stdout}"
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
        !persisted.contains("__kea_"),
        "expected no Kea driver lines in the history file; got:\n{persisted}"
    );
}

#[cfg(unix)]
#[test]
fn posix_wrapper_preserves_entry_status_in_real_bash() {
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
    let failing = feed(ShellFlavor::Posix.wrap(7, "false", 0).unwrap());
    let probe = feed(ShellFlavor::Posix.wrap(9, "echo MARKER:$?", 0).unwrap());

    // Keep the probed shell out of the developer's real history file.
    let history_root = tempfile::tempdir().expect("history isolation dir");
    let mut child = Command::new("bash")
        .args(["--noprofile", "--norc", "-i"])
        .env("HISTFILE", history_root.path().join("history"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn bash for wrapper status check");
    {
        let stdin = child.stdin.as_mut().expect("piped bash stdin");
        stdin.write_all(&integration).expect("feed hook");
        stdin.write_all(&failing).expect("feed failing command");
        stdin.write_all(&probe).expect("feed status probe");
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
fn posix_source_encoding_keeps_user_text_out_of_driver_line() {
    let encoded = encode_posix_source("ls\nls\n'ä'");
    assert!(!encoded.contains('\n'));
    assert!(!encoded.contains('\''));
    assert!(encoded.starts_with("\\0154\\0163\\0012\\0154\\0163"));
}

#[test]
fn command_presentation_preserves_lines_but_escapes_terminal_controls() {
    assert_eq!(
        display_source("printf ok\nprintf '\u{1b}'"),
        "printf ok\nprintf '\\u{1b}'\n"
    );
}
