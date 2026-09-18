use super::*;

#[test]
fn captures_output_and_drains_before_exit() {
    #[cfg(unix)]
    let command = vec![
        "sh".into(),
        "-c".into(),
        "printf 'kea-pty-ok\n'; pwd; exit 7".into(),
    ];
    #[cfg(windows)]
    let command = vec![
        "cmd.exe".into(),
        "/C".into(),
        "echo kea-pty-ok & cd & exit /b 7".into(),
    ];
    let mut pty = Pty::spawn(&command, Size::new(160, 24).unwrap()).unwrap();
    let mut output = Vec::new();
    #[cfg(windows)]
    let mut answered_cursor_query = false;
    let start = std::time::Instant::now();
    loop {
        while let Some(message) = pty.try_recv() {
            if let Message::Output(bytes) = message {
                output.extend_from_slice(&bytes);
                // portable-pty enables PSEUDOCONSOLE_INHERIT_CURSOR. A bare
                // byte-transport test must act as its terminal peer. Real
                // sessions use the live emulator's protocol reply instead.
                #[cfg(windows)]
                if !answered_cursor_query && output.windows(4).any(|bytes| bytes == b"\x1b[6n") {
                    pty.send(b"\x1b[1;1R".to_vec()).unwrap();
                    answered_cursor_query = true;
                }
            }
        }
        if let Some(code) = pty.exit_code().unwrap() {
            assert_eq!(code, 7);
            break;
        }
        assert!(
            start.elapsed().as_secs() < 15,
            "PTY did not terminate; status={:?}, eof={}, output={:?}",
            pty.status,
            pty.eof,
            String::from_utf8_lossy(&output)
        );
        thread::sleep(std::time::Duration::from_millis(10));
    }
    let output = String::from_utf8_lossy(&output);
    assert!(output.contains("kea-pty-ok"));
    let directory = std::env::current_dir().unwrap();
    assert!(output.contains(directory.to_string_lossy().as_ref()));
}
