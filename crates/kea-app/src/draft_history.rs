use std::{
    collections::VecDeque,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

const DEFAULT_CAPACITY: usize = 500;

/// Exact text the user intentionally submitted from Kea's editor.
///
/// This is deliberately independent of shell command blocks. `Send to app` text is
/// valuable authored input even when the child application exposes no structured
/// acknowledgement, and recalling text must never imply that it is safe to re-run.
#[derive(Debug)]
pub struct DraftHistory {
    entries: VecDeque<String>,
    capacity: usize,
    cursor: Option<usize>,
    scratch: String,
    persist_path: Option<PathBuf>,
}

impl Default for DraftHistory {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }
}

impl DraftHistory {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            capacity: capacity.max(1),
            cursor: None,
            scratch: String::new(),
            persist_path: None,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn record(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(text);
        self.reset_navigation();
        // Persistence is best-effort and opt-in: memory stays authoritative, so a
        // failing disk never breaks recall. Failures are intentionally silent here;
        // startup surfaces an unreadable location instead.
        if self.persist_path.is_some() {
            let _ = self.save();
        }
    }

    /// Point recall at a plaintext file and seed it with previously saved entries
    /// (oldest first). Manual acceptance decision: off by default, explicit opt-in
    /// via `persist_history`, because submissions can contain secrets.
    pub fn set_persisted_entries(&mut self, path: PathBuf, entries: Vec<String>) {
        self.persist_path = Some(path);
        for entry in entries {
            if entry.is_empty() {
                continue;
            }
            if self.entries.len() == self.capacity {
                self.entries.pop_front();
            }
            self.entries.push_back(entry);
        }
        self.reset_navigation();
    }

    fn save(&self) -> std::io::Result<()> {
        let Some(path) = self.persist_path.as_deref() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut encoded = String::new();
        for entry in &self.entries {
            encoded.push_str(&encode_entry(entry));
            encoded.push('\n');
        }
        write_private(path, encoded.as_bytes())
    }

    pub fn recent(&self, limit: usize) -> Vec<String> {
        self.entries
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    /// Move toward older submissions. The current unsubmitted draft is retained as
    /// scratch text so navigating forward can restore it exactly.
    pub fn previous(&mut self, current: &str) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }
        self.rebase_if_edited(current);
        let index = match self.cursor {
            Some(0) => 0,
            Some(index) => index - 1,
            None => {
                self.scratch = current.to_owned();
                self.entries.len() - 1
            }
        };
        self.cursor = Some(index);
        self.entries.get(index).cloned()
    }

    /// Move toward newer submissions. Moving past the newest submission restores
    /// the exact scratch draft that existed before history navigation began.
    pub fn next(&mut self, current: &str) -> Option<String> {
        let index = self.cursor?;
        if self
            .entries
            .get(index)
            .is_some_and(|entry| entry != current)
        {
            self.reset_navigation();
            return None;
        }
        if index + 1 < self.entries.len() {
            let next = index + 1;
            self.cursor = Some(next);
            self.entries.get(next).cloned()
        } else {
            self.cursor = None;
            Some(std::mem::take(&mut self.scratch))
        }
    }

    pub fn reset_navigation(&mut self) {
        self.cursor = None;
        self.scratch.clear();
    }

    fn rebase_if_edited(&mut self, current: &str) {
        if let Some(index) = self.cursor {
            if self
                .entries
                .get(index)
                .is_some_and(|entry| entry != current)
            {
                self.cursor = None;
                self.scratch = current.to_owned();
            }
        }
    }
}

/// One entry per physical line. Decoding is total: unknown escapes are preserved
/// literally, so loading never drops user text, however the file was produced.
fn encode_entry(entry: &str) -> String {
    let mut encoded = String::with_capacity(entry.len());
    for character in entry.chars() {
        match character {
            '\\' => encoded.push_str("\\\\"),
            '\n' => encoded.push_str("\\n"),
            '\r' => encoded.push_str("\\r"),
            character => encoded.push(character),
        }
    }
    encoded
}

fn decode_entry(line: &str) -> String {
    let mut decoded = String::with_capacity(line.len());
    let mut characters = line.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            decoded.push(character);
            continue;
        }
        match characters.next() {
            Some('n') => decoded.push('\n'),
            Some('r') => decoded.push('\r'),
            Some('\\') => decoded.push('\\'),
            Some(next) => {
                decoded.push('\\');
                decoded.push(next);
            }
            None => decoded.push('\\'),
        }
    }
    decoded
}

/// Read previously persisted entries, oldest first. Missing files load as empty;
/// only real I/O failures propagate so startup can warn visibly.
pub fn load_history_file(path: &Path) -> std::io::Result<Vec<String>> {
    let text = match fs::read_to_string(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        result => result?,
    };
    Ok(text
        .lines()
        .map(decode_entry)
        .filter(|entry| !entry.is_empty())
        .collect())
}

/// Submissions can contain secrets, so the history file is owner-only.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
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
        let root = std::env::temp_dir().join(format!("kea-history-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let path = root.join("no-such-dir").join("draft-history.txt");
        assert!(load_history_file(&path).unwrap().is_empty());

        // Point persistence at an unwritable location: recall must keep working.
        let mut history = DraftHistory::default();
        history.set_persisted_entries("/proc/kea-history-test/draft-history.txt".into(), vec![]);
        history.record("kept".into());
        assert_eq!(history.previous(""), Some("kept".into()));
        let _ = std::fs::remove_dir_all(&root);
    }
}
