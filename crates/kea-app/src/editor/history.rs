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
/// This is deliberately independent of shell command blocks. Submitted text is
/// valuable authored input even when the active receiver exposes no structured
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
#[path = "../../tests/unit/editor/history.rs"]
mod tests;
