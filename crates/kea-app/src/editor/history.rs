use std::{
    collections::VecDeque,
    fs::{self, File},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const DEFAULT_CAPACITY: usize = 500;
const MAX_HISTORY_FILE_BYTES: u64 = 16 * 1024 * 1024;
/// The append-only file is compacted to the newest entries past this size, so it
/// stays far below the load limit however many submissions accumulate.
const COMPACT_HISTORY_FILE_BYTES: u64 = 4 * 1024 * 1024;

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
    writer: Option<Writer>,
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
            writer: None,
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
        self.reset_navigation();
        // Memory remains authoritative. The opted-in disk copy is appended by a
        // worker so the UI never waits on disk; its first failure stops it visibly.
        if let Some(writer) = &self.writer {
            if writer.tx.send(Message::Append(text.clone())).is_err() {
                self.stop_persistence("the history writer stopped".into());
            }
        }
        self.entries.push_back(text);
        self.poll_writer();
    }

    fn poll_writer(&mut self) {
        while let Some(event) = self
            .writer
            .as_ref()
            .and_then(|writer| writer.errors.try_recv().ok())
        {
            match event {
                WriterEvent::Failed(error) => self.stop_persistence(error),
                // One oversized draft is not saved; later drafts still are.
                WriterEvent::Skipped(reason) => {
                    self.persistence_warning = Some(format!(
                        "A draft was not saved to draft history: {reason}. It is still recallable in this session."
                    ));
                }
            }
        }
    }

    fn stop_persistence(&mut self, error: String) {
        if let Some(writer) = self.writer.take() {
            self.persistence_warning = Some(format!(
                "Draft history persistence stopped: {}: {error}. In-memory recall remains available for this session.",
                writer.path.display()
            ));
        }
    }

    /// Wait until every recorded entry reached the file (or failed).
    pub fn flush(&mut self) {
        self.flush_within(Duration::MAX);
    }

    /// Wait at most `limit` for queued entries to reach the file; quitting must
    /// not lose the last submissions, nor hang on a stalled disk. Returns
    /// whether everything queued was written (or failed visibly).
    pub fn flush_within(&mut self, limit: Duration) -> bool {
        let mut complete = true;
        if let Some(writer) = &self.writer {
            let (done, wait) = mpsc::channel();
            if writer.tx.send(Message::Flush(done)).is_ok() {
                let deadline = Instant::now().checked_add(limit);
                complete = match deadline {
                    Some(deadline) => wait
                        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                        .is_ok(),
                    None => wait.recv().is_ok(),
                };
            }
        }
        self.poll_writer();
        complete
    }

    /// Point recall at a plaintext file and seed it with previously saved entries
    /// (oldest first). Manual acceptance decision: off by default, explicit opt-in
    /// via `persist_history`, because submissions can contain secrets.
    pub fn set_persisted_entries(&mut self, path: PathBuf, entries: Vec<String>) {
        self.writer = Some(Writer::start(path));
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
        self.poll_writer();
        self.persistence_warning.take()
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

enum Message {
    Append(String),
    Flush(mpsc::Sender<()>),
}

/// Appends entries off the UI thread. Every append reopens the file under an
/// exclusive lock, so concurrent Kea instances add to the same history instead
/// of replacing each other's, and compaction can safely swap the file.
#[derive(Debug)]
struct Writer {
    path: PathBuf,
    tx: mpsc::Sender<Message>,
    errors: mpsc::Receiver<WriterEvent>,
}

#[derive(Debug)]
enum WriterEvent {
    /// Persistence stopped; nothing later is written.
    Failed(String),
    /// This entry alone was not written.
    Skipped(String),
}

impl Writer {
    fn start(path: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel::<Message>();
        let (error_tx, errors) = mpsc::channel();
        let worker_path = path.clone();
        thread::spawn(move || {
            for message in rx {
                match message {
                    Message::Append(entry) => {
                        let bytes = encoded_line_len(&entry);
                        if bytes > COMPACT_HISTORY_FILE_BYTES {
                            let _ = error_tx.send(WriterEvent::Skipped(format!(
                                "it needs {bytes} bytes; the persistence maximum per draft is {COMPACT_HISTORY_FILE_BYTES}"
                            )));
                            continue;
                        }
                        if let Err(error) = append_entry(&worker_path, &entry) {
                            let _ = error_tx.send(WriterEvent::Failed(error.to_string()));
                            // Stop after the first failure; the UI reports it.
                            return;
                        }
                    }
                    Message::Flush(done) => {
                        let _ = done.send(());
                    }
                }
            }
        });
        Self { path, tx, errors }
    }
}

fn encoded_line_len(entry: &str) -> u64 {
    encode_entry(entry).len() as u64 + 1
}

fn append_entry(path: &Path, entry: &str) -> std::io::Result<()> {
    let mut line = encode_entry(entry);
    line.push('\n');
    if line.len() as u64 > COMPACT_HISTORY_FILE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("one draft needs {} bytes; the persistence maximum per draft is {COMPACT_HISTORY_FILE_BYTES}", line.len()),
        ));
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let _lock = HistoryLock::acquire(path)?;
    let mut options = fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    #[cfg(unix)]
    {
        // Submissions can contain secrets: keep an older or hand-made file private.
        use std::os::unix::fs::PermissionsExt as _;
        if file.metadata()?.permissions().mode() & 0o077 != 0 {
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
    }
    file.write_all(line.as_bytes())?;
    file.sync_data()?;
    let len = file.metadata()?.len();
    // Windows cannot replace a file that still has an open handle, so close
    // the append handle before compaction swaps in the rewritten file.
    drop(file);
    if len > COMPACT_HISTORY_FILE_BYTES {
        compact(path)?;
    }
    Ok(())
}

/// Keep only the newest entries. Runs under the history lock.
fn compact(path: &Path) -> std::io::Result<()> {
    let entries = load_history_file(path)?;
    let mut encoded = String::new();
    for entry in &entries {
        encoded.push_str(&encode_entry(entry));
        encoded.push('\n');
    }
    write_private(path, encoded.as_bytes())
}

/// Exclusive advisory lock on `<history>.lock`, shared by all Kea instances.
/// Without `flock` (Windows) appends still never rewrite other entries; only a
/// rare compaction could race a concurrent append.
struct HistoryLock {
    #[cfg(unix)]
    _lock: nix::fcntl::Flock<File>,
}

impl HistoryLock {
    fn acquire(path: &Path) -> std::io::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            let mut name = path.as_os_str().to_owned();
            name.push(".lock");
            let file = fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .mode(0o600)
                .open(PathBuf::from(name))?;
            let lock = nix::fcntl::Flock::lock(file, nix::fcntl::FlockArg::LockExclusive)
                .map_err(|(_, errno)| std::io::Error::from(errno))?;
            Ok(Self { _lock: lock })
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            Ok(Self {})
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
