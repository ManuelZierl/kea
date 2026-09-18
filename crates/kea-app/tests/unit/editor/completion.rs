use super::*;

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
