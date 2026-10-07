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

#[cfg(unix)]
#[test]
fn drop_kills_child_before_pty_input_can_execute_a_pending_line() {
    use std::{
        fs,
        path::{Path, PathBuf},
        thread,
        time::{Duration, SystemTime},
    };

    struct TestDirectory(PathBuf);

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn test_directory() -> TestDirectory {
        let root = std::env::temp_dir();
        let name = format!(
            "kea-pty-drop-order-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = root.join(name);
        fs::create_dir(&path).unwrap();

        // Do not let an unwritable fixture turn the negative assertion into a
        // false pass, especially on hosted runners with unusual temp roots.
        let probe = path.join("writable-probe");
        fs::write(&probe, b"probe").unwrap();
        assert!(probe.is_file());
        fs::remove_file(probe).unwrap();

        TestDirectory(path)
    }

    fn run_attempt(directory: &Path, attempt: usize) {
        let marker = directory.join(format!("executed-{attempt}"));
        let command = vec![
            "sh".into(),
            "-c".into(),
            "printf READY; IFS= read -r line; printf executed > \"$0\"; sleep 10".into(),
            marker.as_os_str().to_owned(),
        ];
        let mut pty = Pty::spawn(&command, Size::new(80, 24).unwrap()).unwrap();
        let start = std::time::Instant::now();
        let mut ready = false;
        while start.elapsed() < Duration::from_secs(2) {
            while let Some(message) = pty.try_recv() {
                if let Message::Output(bytes) = message {
                    ready |= bytes.windows(5).any(|bytes| bytes == b"READY");
                }
            }
            if ready {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(ready, "fixture did not become ready");

        drop(pty);
        thread::sleep(Duration::from_millis(100));
        assert!(
            !marker.exists(),
            "PTY teardown executed implicit input before killing the child"
        );
    }

    let directory = test_directory();
    for attempt in 0..8 {
        run_attempt(&directory.0, attempt);
    }
}
