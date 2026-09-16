//! UI-independent, bounded input memory. Records describe submissions, not success.
//! No PTY access, shell evaluation, keystroke capture, or terminal-output parsing.
use std::{
    collections::HashMap,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub const MAX_ENTRIES: usize = 5_000;
pub const MAX_TEXT_BYTES: usize = 64 * 1024;
pub const MAX_RETAINED_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_RESULTS: usize = 80;
pub const MAX_QUERY_CHARS: usize = 512;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InputKind {
    Posix,
    PowerShell,
    Application,
}

impl InputKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Posix => "POSIX shell",
            Self::PowerShell => "PowerShell",
            Self::Application => "Literal app text",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    pub id: String,
    pub text: String,
    pub kind: InputKind,
    pub directory: Option<String>,
    pub submitted_ms: u64,
    /// A name distinguishes an explicitly saved memory from a historical occurrence.
    pub name: Option<String>,
    /// None is global. Directory scope uses path-component equality, not a prefix.
    pub scope: Option<String>,
}

impl Entry {
    pub fn submitted(text: String, kind: InputKind, directory: Option<String>) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        Self {
            id: format!("{:x}-{:x}-{:x}", time.as_nanos(), std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)),
            text,
            kind,
            directory,
            submitted_ms: time.as_millis().min(u128::from(u64::MAX)) as u64,
            name: None,
            scope: None,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() || self.id.len() > 100
            || !self.id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
        {
            return Err("Invalid input-memory identifier.".into());
        }
        if self.text.trim().is_empty() || self.text.len() > MAX_TEXT_BYTES {
            return Err("Input memory requires non-empty text no larger than 64 KiB.".into());
        }
        if self.name.as_ref().is_some_and(|name| name.trim().is_empty() || name.len() > 200) {
            return Err("Memory names must contain 1–200 bytes.".into());
        }
        for directory in [&self.directory, &self.scope].into_iter().flatten() {
            if directory.is_empty() || directory.len() > 4096 || directory.contains('\0') {
                return Err("Invalid memory directory.".into());
            }
        }
        if self.name.is_none() && self.scope.is_some() {
            return Err("Only saved memories can have a scope.".into());
        }
        Ok(())
    }

    pub fn retained_bytes(&self) -> usize {
        self.id.len() + self.text.len() + self.name.as_ref().map_or(0, String::len)
            + self.directory.as_ref().map_or(0, String::len)
            + self.scope.as_ref().map_or(0, String::len) + 128
    }
}

#[derive(Clone, Debug)]
pub struct Hit {
    pub entry: Entry,
    /// Every retained occurrence remains addressable even when the result is grouped.
    pub occurrence_ids: Vec<String>,
    pub latest_ms: u64,
    pub local: bool,
    score: i64,
}

#[derive(Clone, Default)]
pub struct Library {
    entries: Vec<Entry>,
    bytes: usize,
}

impl Library {
    pub fn entries(&self) -> &[Entry] { &self.entries }

    pub fn insert(&mut self, entry: Entry) -> Result<(), String> {
        entry.validate()?;
        let existing = self.entries.iter().position(|old| old.id == entry.id);
        let old_bytes = existing.map_or(0, |i| self.entries[i].retained_bytes());
        let bytes = self.bytes.saturating_sub(old_bytes).saturating_add(entry.retained_bytes());
        if (existing.is_none() && self.entries.len() >= MAX_ENTRIES) || bytes > MAX_RETAINED_BYTES {
            return Err("Input memory reached its 5,000-entry / 16 MiB limit. Forget entries to retain more; terminal execution is unaffected.".into());
        }
        if let Some(index) = existing { self.entries[index] = entry; }
        else { self.entries.push(entry); }
        self.bytes = bytes;
        Ok(())
    }

    pub fn remove(&mut self, id: &str) {
        self.entries.retain(|entry| entry.id != id);
        self.bytes = self.entries.iter().map(Entry::retained_bytes).sum();
    }

    pub fn search(&self, query: &str, directory: Option<&str>, now_ms: u64) -> Result<Vec<Hit>, String> {
        if query.chars().count() > MAX_QUERY_CHARS {
            return Err("Search is limited to 512 characters. Shorten the query; the original draft is unchanged.".into());
        }
        let words: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
        let mut hits: Vec<Hit> = Vec::new();
        let mut groups = HashMap::new();
        for entry in &self.entries {
            if entry.scope.as_deref().is_some_and(|scope| !same_directory(Some(scope), directory)) {
                continue;
            }
            // Names are searched too, but shell dialect and cwd are filters/labels,
            // not extra fuzzy text that could produce surprising command matches.
            let body = entry.text.to_lowercase();
            let name = entry.name.as_deref().unwrap_or_default().to_lowercase();
            let Some(quality) = words.iter().try_fold(0_i64, |sum, word| {
                fuzzy_score(&body, word).max(fuzzy_score(&name, word)).map(|score| sum + score)
            }) else { continue; };
            let local = same_directory(entry.directory.as_deref(), directory);
            let key = (entry.kind, entry.text.as_str());
            if entry.name.is_none() {
                if let Some(&index) = groups.get(&key) {
                    let hit: &mut Hit = &mut hits[index];
                    hit.occurrence_ids.push(entry.id.clone());
                    hit.local |= local;
                    if entry.submitted_ms > hit.latest_ms {
                        hit.latest_ms = entry.submitted_ms;
                        hit.entry = entry.clone();
                    }
                    continue;
                }
                groups.insert(key, hits.len());
            }
            hits.push(Hit {
                entry: entry.clone(),
                occurrence_ids: if entry.name.is_none() { vec![entry.id.clone()] } else { vec![] },
                latest_ms: entry.submitted_ms,
                local,
                score: quality,
            });
        }
        for hit in &mut hits {
            let age_hours = now_ms.saturating_sub(hit.latest_ms) / 3_600_000;
            let recency = 48_i64.saturating_sub(age_hours.min(48) as i64);
            let frequency = (hit.occurrence_ids.len().max(1).ilog2() as i64).min(10) * 6;
            hit.score += recency + frequency + if hit.local { 45 } else { 0 }
                + if hit.entry.name.is_some() { 70 } else { 0 };
        }
        hits.sort_by(|a, b| b.score.cmp(&a.score)
            .then_with(|| b.latest_ms.cmp(&a.latest_ms))
            .then_with(|| a.entry.id.cmp(&b.entry.id)));
        hits.truncate(MAX_RESULTS);
        Ok(hits)
    }
}

fn same_directory(left: Option<&str>, right: Option<&str>) -> bool {
    matches!((left, right), (Some(left), Some(right)) if Path::new(left) == Path::new(right))
}

/// Unicode subsequence matching with contiguous and word-boundary bonuses.
/// Greedy O(text + query); bounded input makes a search predictable even for scripts.
fn fuzzy_score(text: &str, query: &str) -> Option<i64> {
    let mut wanted = query.chars().peekable();
    let mut score = 0;
    let mut consecutive = 0;
    let mut previous = ' ';
    for ch in text.chars() {
        if wanted.peek() == Some(&ch) {
            wanted.next();
            consecutive += 1;
            score += 12 + consecutive.min(8) * 3;
            if !previous.is_alphanumeric() { score += 12; }
            if wanted.peek().is_none() {
                return Some(score + if text.starts_with(query) { 60 } else { 0 });
            }
        } else { consecutive = 0; }
        previous = ch;
    }
    wanted.peek().is_none().then_some(score)
}

/// Display only: exact multiline/Unicode text is retained for insertion and copying.
pub fn summary(text: &str, limit: usize) -> String {
    let mut chars = text.chars();
    let mut result: String = chars.by_ref().take(limit).map(|c| {
        if c == '\n' || c == '\r' { '↵' } else if c.is_control() { ' ' } else { c }
    }).collect();
    if chars.next().is_some() { result.push('…'); }
    result
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default()
        .as_millis().min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry(text: &str, directory: Option<&str>) -> Entry {
        Entry::submitted(text.into(), InputKind::Posix, directory.map(str::to_string))
    }
    #[test]
    fn fuzzy_words_unicode_and_multiline_are_supported() {
        let mut library = Library::default();
        library.insert(entry("docker compose logs worker\nprintf 'Überprüfung 😀'", None)).unwrap();
        assert_eq!(library.search("dcl work", None, now_ms()).unwrap().len(), 1);
        assert_eq!(library.search("über 😀", None, now_ms()).unwrap().len(), 1);
        assert!(library.search("not-present", None, now_ms()).unwrap().is_empty());
    }
    #[test]
    fn groups_occurrences_but_never_conflates_shells_or_saved_memories() {
        let mut library = Library::default();
        library.insert(entry("echo hello", Some("/a"))).unwrap();
        library.insert(entry("echo hello", Some("/b"))).unwrap();
        let mut app = entry("echo hello", None);
        app.kind = InputKind::Application;
        library.insert(app).unwrap();
        let mut saved = entry("echo hello", None);
        saved.name = Some("greeting".into());
        library.insert(saved).unwrap();
        let hits = library.search("hello", Some("/a"), now_ms()).unwrap();
        assert_eq!(hits.len(), 3);
        assert_eq!(hits.iter().map(|hit| hit.occurrence_ids.len()).sum::<usize>(), 3);
        assert!(hits.iter().any(|hit| hit.local && hit.occurrence_ids.len() == 2));
    }
    #[test]
    fn scope_is_exact_and_unknown_is_not_local() {
        let mut library = Library::default();
        let mut saved = entry("pwd", None);
        saved.name = Some("where".into());
        saved.scope = Some("/work/a".into());
        library.insert(saved).unwrap();
        assert_eq!(library.search("", Some("/work/a"), now_ms()).unwrap().len(), 1);
        for directory in [None, Some("/work/ab"), Some("/work/a/child")] {
            assert!(library.search("", directory, now_ms()).unwrap().is_empty());
        }
        assert!(!same_directory(None, None));
    }
    #[test]
    fn ranking_boosts_local_saved_and_frequent_matches() {
        let mut library = Library::default();
        library.insert(entry("docker logs remote", Some("/other"))).unwrap();
        library.insert(entry("docker logs local", Some("/here"))).unwrap();
        let mut saved = entry("docker logs saved", None);
        saved.name = Some("logs".into());
        library.insert(saved).unwrap();
        let hits = library.search("docker", Some("/here"), now_ms()).unwrap();
        assert_eq!(hits[0].entry.text, "docker logs saved");
        assert_eq!(hits[1].entry.text, "docker logs local");
    }
    #[test]
    fn invalid_and_oversized_data_are_rejected_without_mutation() {
        let mut library = Library::default();
        assert!(library.insert(entry(&"x".repeat(MAX_TEXT_BYTES + 1), None)).is_err());
        let mut bad = entry("pwd", None);
        bad.id = "../../secret".into();
        assert!(library.insert(bad).is_err());
        assert!(library.entries().is_empty());
        assert!(library.search(&"x".repeat(513), None, 0).is_err());
    }
    #[test]
    fn forgetting_does_not_reappear_in_search_and_limits_are_bounded() {
        let mut library = Library::default();
        for n in 0..100 { library.insert(entry(&format!("command-{n}"), None)).unwrap(); }
        let hits = library.search("", None, now_ms()).unwrap();
        assert_eq!(hits.len(), MAX_RESULTS);
        let id = hits[0].entry.id.clone();
        library.remove(&id);
        assert!(library.entries().iter().all(|entry| entry.id != id));
    }
    #[test]
    fn display_summary_does_not_change_stored_text() {
        assert_eq!(summary("a\n😀b", 3), "a↵😀…");
    }
}
