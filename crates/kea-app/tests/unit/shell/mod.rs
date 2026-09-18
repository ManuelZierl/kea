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

    let mut child = Command::new("bash")
        .args(["--noprofile", "--norc", "-i"])
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
