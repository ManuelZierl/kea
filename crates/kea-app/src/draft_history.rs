use std::{
    collections::VecDeque,
    fs::{self, File},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
};

const DEFAULT_CAPACITY: usize = 500;
const MAX_HISTORY_FILE_BYTES: u64 = 16 * 1024 * 1024;

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
    persistence_warning: Option<String>,
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
            persistence_warning: None,
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
        // Memory remains authoritative, but an opted-in disk copy must not fail
        // silently. Stop retrying after the first error, like the session journal;
        // the current process can still recall every retained entry.
        if let Err(error) = self.save() {
            if let Some(path) = self.persist_path.take() {
                self.persistence_warning = Some(format!(
                    "Draft history persistence stopped: {}: {error}. In-memory recall remains available for this session.",
                    path.display()
                ));
            }
        }
    }

    /// Point recall at a plaintext file and seed it with previously saved entries
    /// (oldest first). Manual acceptance decision: off by default, explicit opt-in
    /// via `persist_history`, because submissions can contain secrets.
    pub fn set_persisted_entries(&mut self, path: PathBuf, entries: Vec<String>) {
        self.persist_path = Some(path);
        self.persistence_warning = None;
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

    pub fn take_persistence_warning(&mut self) -> Option<String> {
        self.persistence_warning.take()
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
        if encoded.len() as u64 > MAX_HISTORY_FILE_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "retained drafts need {} bytes; the persistence maximum is {MAX_HISTORY_FILE_BYTES}",
                    encoded.len()
                ),
            ));
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
    let mut file = match File::open(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        result => result?,
    };
    let metadata = file.metadata()?;
    if metadata.len() > MAX_HISTORY_FILE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "history file is {} bytes; the maximum is {MAX_HISTORY_FILE_BYTES}",
                metadata.len()
            ),
        ));
    }
    let mut text = String::new();
    (&mut file)
        .take(MAX_HISTORY_FILE_BYTES + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > MAX_HISTORY_FILE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("history file exceeds the {MAX_HISTORY_FILE_BYTES}-byte maximum while reading"),
        ));
    }
    let mut entries = text
        .lines()
        .rev()
        .map(decode_entry)
        .filter(|entry| !entry.is_empty())
        .take(DEFAULT_CAPACITY)
        .collect::<Vec<_>>();
    entries.reverse();
    Ok(entries)
}

/// Submissions can contain secrets, so the history file is owner-only. Write a
/// complete sibling first so an interrupted write cannot truncate the last good
/// history file.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let mut temporary = tempfile::Builder::new()
        .prefix(".draft-history-")
        .tempfile_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    temporary.persist(path).map_err(|error| error.error)?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
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
        let warning = history.take_persistence_warning().unwrap();
        assert!(warning.contains("persistence stopped"));
        assert!(warning.contains("In-memory recall remains available"));
        assert_eq!(history.previous(""), Some("kept".into()));
        history.record("also kept".into());
        assert!(history.take_persistence_warning().is_none());
        let _ = std::fs::remove_dir_all(&root);
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
}
