use super::*;

#[test]
fn records_exact_shell_and_application_text_without_interpreting_it() {
    let mut history = DraftHistory::default();
    history.record("echo one\necho two".into());
    history.record("Explain this diff:\n\n```rs\nfn main() {}\n```".into());
    assert_eq!(
        history.recent(10),
        vec![
            "echo one\necho two".to_string(),
            "Explain this diff:\n\n```rs\nfn main() {}\n```".to_string(),
        ]
    );
}

#[test]
fn previous_and_next_restore_the_unsubmitted_scratch_draft() {
    let mut history = DraftHistory::default();
    history.record("first".into());
    history.record("second".into());

    assert_eq!(history.previous("draft"), Some("second".into()));
    assert_eq!(history.previous("second"), Some("first".into()));
    assert_eq!(history.next("first"), Some("second".into()));
    assert_eq!(history.next("second"), Some("draft".into()));
    assert_eq!(history.next("draft"), None);
}

#[test]
fn editing_recalled_text_starts_a_new_navigation_chain() {
    let mut history = DraftHistory::default();
    history.record("one".into());
    history.record("two".into());
    assert_eq!(history.previous(""), Some("two".into()));
    assert_eq!(history.previous("two edited"), Some("two".into()));
    assert_eq!(history.next("two"), Some("two edited".into()));
}

#[test]
fn retention_is_bounded_but_keeps_recent_authored_text() {
    let mut history = DraftHistory::with_capacity(2);
    history.record("one".into());
    history.record("two".into());
    history.record("three".into());
    assert_eq!(
        history.recent(10),
        vec!["two".to_string(), "three".to_string()]
    );
}

#[test]
fn history_file_codec_round_trips_multiline_unicode_and_backslashes() {
    for entry in [
        "echo simple",
        "multiline\nsecond line\nthird",
        "cariage\rreturn",
        "back\\slash",
        "trailing\\",
        "literal \\n escape",
        "unicode ä😀ö",
        "tabs\tand spaces  ",
    ] {
        assert_eq!(decode_entry(&encode_entry(entry)), entry, "{entry:?}");
    }
    // Decoding is total: foreign lines load literally instead of vanishing.
    assert_eq!(decode_entry("plain"), "plain");
    assert_eq!(decode_entry("\\x"), "\\x");
}

#[test]
fn persisted_entries_survive_a_restart_bounded_and_owner_only() {
    let root = std::env::temp_dir().join(format!("kea-history-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let path = root.join("draft-history.txt");

    let mut history = DraftHistory::with_capacity(2);
    history.set_persisted_entries(
        path.clone(),
        vec!["stale\ndraft".into(), "fresh ä😀".into()],
    );
    assert_eq!(
        history.recent(10),
        vec!["stale\ndraft".to_string(), "fresh ä😀".to_string()]
    );
    // Recording evicts the oldest entry and writes the file through.
    history.record("third".into());
    assert_eq!(
        history.recent(10),
        vec!["fresh ä😀".to_string(), "third".to_string()]
    );

    let mut restarted = DraftHistory::with_capacity(2);
    let loaded = load_history_file(&path).unwrap();
    restarted.set_persisted_entries(path.clone(), loaded);
    assert_eq!(
        restarted.recent(10),
        vec!["fresh ä😀".to_string(), "third".to_string()]
    );
    assert_eq!(restarted.previous(""), Some("third".into()));
    assert_eq!(restarted.previous("third"), Some("fresh ä😀".into()));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "history may contain secrets");
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn missing_history_file_loads_empty_and_failed_saves_keep_memory() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("no-such-dir").join("draft-history.txt");
    assert!(load_history_file(&path).unwrap().is_empty());

    // A regular file cannot be a parent directory on any supported OS.
    // Unlike /proc or permission bits, this also fails on Windows and as root.
    let blocker = root.path().join("not-a-directory");
    std::fs::write(&blocker, b"preserve me").unwrap();
    let mut history = DraftHistory::default();
    history.set_persisted_entries(blocker.join("draft-history.txt"), vec![]);
    history.record("kept".into());
    let warning = history.take_persistence_warning().unwrap();
    assert!(warning.contains("persistence stopped"));
    assert!(warning.contains("In-memory recall remains available"));
    assert_eq!(history.previous(""), Some("kept".into()));
    history.record("also kept".into());
    assert!(history.take_persistence_warning().is_none());
    assert_eq!(std::fs::read(&blocker).unwrap(), b"preserve me");
}

#[test]
fn loading_is_bounded_to_recent_entries_and_rejects_oversized_files() {
    let root = std::env::temp_dir().join(format!("kea-history-bounds-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("draft-history.txt");
    let text = (0..DEFAULT_CAPACITY + 2)
        .map(|index| format!("entry-{index}\n"))
        .collect::<String>();
    std::fs::write(&path, text).unwrap();
    let loaded = load_history_file(&path).unwrap();
    assert_eq!(loaded.len(), DEFAULT_CAPACITY);
    assert_eq!(loaded.first().map(String::as_str), Some("entry-2"));
    assert_eq!(loaded.last().map(String::as_str), Some("entry-501"));

    File::create(&path)
        .unwrap()
        .set_len(MAX_HISTORY_FILE_BYTES + 1)
        .unwrap();
    assert_eq!(
        load_history_file(&path).unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn failed_replacement_keeps_the_previous_history_path_intact() {
    let root = std::env::temp_dir().join(format!("kea-history-atomic-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("draft-history.txt");
    std::fs::create_dir_all(&path).unwrap();

    assert!(write_private(&path, b"replacement").is_err());
    assert!(path.is_dir());
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
fn locked_windows_history_survives_failed_replacement() {
    use std::os::windows::fs::OpenOptionsExt as _;

    let root = std::env::temp_dir().join(format!("kea-history-locked-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("draft-history.txt");
    std::fs::write(&path, b"previous").unwrap();
    let locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&path)
        .unwrap();

    assert!(write_private(&path, b"replacement").is_err());
    drop(locked);
    assert_eq!(std::fs::read(&path).unwrap(), b"previous");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    std::fs::remove_dir_all(root).unwrap();
}
