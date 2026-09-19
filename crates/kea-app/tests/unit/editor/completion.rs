use super::*;

#[cfg(unix)]
fn write_test_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::write(path, b"#!/bin/sh\nprintf KEA_COMPLETION_OK\n").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn completion_hunt_symlink_executable_is_offered_like_its_target() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("kea-hunt-target");
    write_test_executable(&target);
    std::os::unix::fs::symlink(&target, root.path().join("kea-hunt-link")).unwrap();
    let output = std::process::Command::new("/bin/sh")
        .args(["-c", "kea-hunt-link"])
        .env("PATH", root.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"KEA_COMPLETION_OK");
    let candidates = suggest(
        "kea-hunt-",
        9,
        None,
        root.path().to_str(),
        &[],
        Some(ShellFlavor::Posix),
    );
    assert!(candidates
        .iter()
        .any(|c| c.label == "Command: kea-hunt-target"));
    assert!(
        candidates
            .iter()
            .any(|c| c.label == "Command: kea-hunt-link"),
        "shell-resolvable symlink missing: {candidates:?}"
    );
}

#[cfg(unix)]
#[test]
fn completion_hunt_relative_path_uses_reported_shell_cwd() {
    let root = tempfile::tempdir().unwrap();
    // Unique relative directory cannot accidentally exist in the test process cwd.
    let bin = tempfile::Builder::new()
        .prefix("kea-hunt-bin-")
        .tempdir_in(root.path())
        .unwrap();
    write_test_executable(&bin.path().join("kea-hunt-relative"));
    let relative_path = bin.path().file_name().unwrap().to_str().unwrap();
    let output = std::process::Command::new("/bin/sh")
        .args(["-c", "kea-hunt-relative"])
        .current_dir(root.path())
        .env("PATH", relative_path)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"KEA_COMPLETION_OK");
    let absolute = suggest(
        "kea-hunt-",
        9,
        Some(root.path()),
        bin.path().to_str(),
        &[],
        Some(ShellFlavor::Posix),
    );
    assert!(absolute
        .iter()
        .any(|c| c.label == "Command: kea-hunt-relative"));
    let relative = suggest(
        "kea-hunt-",
        9,
        Some(root.path()),
        Some(relative_path),
        &[],
        Some(ShellFlavor::Posix),
    );
    assert!(
        relative
            .iter()
            .any(|c| c.label == "Command: kea-hunt-relative"),
        "relative PATH must resolve against shell cwd: {relative:?}"
    );
    let without_cwd = suggest(
        "kea-hunt-",
        9,
        None,
        Some(relative_path),
        &[],
        Some(ShellFlavor::Posix),
    );
    assert!(
        without_cwd
            .iter()
            .all(|c| c.label != "Command: kea-hunt-relative"),
        "relative PATH must not fall back to the completion process cwd: {without_cwd:?}"
    );
}

#[cfg(unix)]
#[test]
fn completion_empty_path_entry_uses_reported_shell_cwd() {
    let root = tempfile::tempdir().unwrap();
    write_test_executable(&root.path().join("kea-hunt-empty-path"));
    let candidates = suggest(
        "kea-hunt-",
        9,
        Some(root.path()),
        Some(""),
        &[],
        Some(ShellFlavor::Posix),
    );
    assert!(candidates
        .iter()
        .any(|c| c.label == "Command: kea-hunt-empty-path"));
}

#[test]
fn completion_hunt_tilde_filename_remains_literal_in_shell() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("~"), b"").unwrap();
    let candidates = suggest(
        "cat ",
        4,
        Some(root.path()),
        None,
        &[],
        Some(ShellFlavor::Posix),
    );
    let candidate = candidates.iter().find(|c| c.label == "Path: ~").unwrap();
    assert_eq!(candidate.replacement, "'~'");
    // Evaluate only this synthetic candidate on hosts with a POSIX shell.
    // The completion assertion above still runs on Windows.
    #[cfg(unix)]
    {
        let output = std::process::Command::new("/bin/sh")
            .args(["-c", &format!("printf '%s' {}", candidate.replacement)])
            .env("HOME", root.path())
            .current_dir(root.path())
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            output.stdout, b"~",
            "completion changed the filename through tilde expansion"
        );
    }
}

#[test]
fn shell_quote_distinguishes_literal_tilde_from_home_path() {
    assert_eq!(shell_quote("~", Some(ShellFlavor::Posix)), "'~'");
    assert_eq!(
        shell_quote("~/kea-hunt-home", Some(ShellFlavor::Posix)),
        "~/kea-hunt-home"
    );
}

#[test]
fn completion_hunt_unicode_range_preserves_surrounding_draft() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("café"), b"").unwrap();
    let mut text = "echo 😀 ca suffix".to_string();
    let cursor = "echo 😀 ca".len();
    let candidates = suggest(
        &text,
        cursor,
        Some(root.path()),
        None,
        &[],
        Some(ShellFlavor::Posix),
    );
    let candidate = candidates.iter().find(|c| c.replacement == "café").unwrap();
    assert_eq!(&text[candidate.range.clone()], "ca");
    text.replace_range(candidate.range.clone(), &candidate.replacement);
    assert_eq!(text, "echo 😀 café suffix");
}

#[test]
fn completion_hunt_invalid_utf8_cursor_is_rejected() {
    for cursor in [1, 2, 3, 5] {
        assert!(suggest(
            "😀",
            cursor,
            None,
            None,
            &["😀 suffix".into()],
            Some(ShellFlavor::Posix)
        )
        .is_empty());
    }
}

#[test]
fn history_is_literal_and_never_evaluated() {
    let history = vec!["echo $(danger)".into(), "echo hello".into()];
    let candidates = suggest("echo h", 6, None, None, &history, None);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].replacement, "echo hello");
}

#[test]
fn paths_are_relative_to_reported_cwd_and_shell_quoted() {
    let root = std::env::temp_dir().join(format!("kea-complete-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("space ä")).unwrap();
    let candidates = suggest(
        "cd spa",
        6,
        Some(&root),
        None,
        &[],
        Some(ShellFlavor::Posix),
    );
    assert_eq!(candidates[0].replacement, "'space ä/'");
    assert_eq!(candidates[0].range, 3..6);
    assert!(suggest(
        "echo $(spa",
        10,
        Some(&root),
        None,
        &[],
        Some(ShellFlavor::Posix)
    )
    .is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn empty_argument_completes_visible_entries_from_reported_cwd() {
    let root = std::env::temp_dir().join(format!("kea-empty-path-complete-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("alpha")).unwrap();
    std::fs::write(root.join("beta"), b"").unwrap();
    std::fs::write(root.join(".hidden"), b"").unwrap();

    let candidates = suggest("ls ", 3, Some(&root), None, &[], Some(ShellFlavor::Posix));
    let replacements = candidates
        .iter()
        .map(|candidate| candidate.replacement.as_str())
        .collect::<Vec<_>>();
    assert!(replacements.contains(&"alpha/"));
    assert!(replacements.contains(&"beta"));
    assert!(!replacements.contains(&".hidden"));
    assert!(suggest("ls -", 4, Some(&root), None, &[], Some(ShellFlavor::Posix)).is_empty());

    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn first_word_completes_executables_from_reported_shell_path() {
    use std::os::unix::fs::PermissionsExt as _;
    let root = std::env::temp_dir().join(format!("kea-path-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let executable = root.join("kea-test-command");
    std::fs::write(&executable, b"#!/bin/sh\n").unwrap();
    let mut permissions = std::fs::metadata(&executable).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&executable, permissions).unwrap();
    let path = root.to_string_lossy();
    let candidates = suggest(
        "kea-test",
        8,
        Some(&root),
        Some(&path),
        &[],
        Some(ShellFlavor::Posix),
    );
    assert!(candidates
        .iter()
        .any(|candidate| candidate.replacement == "kea-test-command"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn command_completion_is_only_for_first_word() {
    let path = std::env::var("PATH").unwrap_or_default();
    let candidates = suggest(
        "echo ca",
        7,
        None,
        Some(&path),
        &[],
        Some(ShellFlavor::Posix),
    );
    assert!(candidates
        .iter()
        .all(|candidate| !candidate.label.starts_with("Command:")));
}
