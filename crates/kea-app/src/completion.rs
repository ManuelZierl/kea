//! Conservative, read-only local completion. Never evaluate the draft, expand
//! shell expressions, or write a speculative command into the live PTY.
//! Text editing/undo is still performed by the host's existing editor component.
use crate::shell::ShellFlavor;
use std::{collections::BTreeSet, ops::Range, path::{Path, PathBuf}, time::{Duration, Instant}};

const LIMIT: usize = 128;
const SCAN_BUDGET: usize = 8192;

#[derive(Clone, Debug)]
pub struct Suggestion {
    pub label: String,
    pub replacement: String,
    pub range: Range<usize>,
}
#[derive(Clone)]
pub struct Request {
    pub text: String,
    pub cursor: usize,
    pub directory: Option<PathBuf>,
    pub shell: ShellFlavor,
    pub history: Vec<String>,
    pub path: Vec<PathBuf>,
}
#[derive(Debug)]
struct Token {
    range: Range<usize>,
    value: String,
    command: bool,
    directories_only: bool,
}

/// Limit to a literal token at the cursor; ambiguous shell syntax gets no
/// filesystem completion rather than a fabricated expansion.
fn token(text: &str, cursor: usize, shell: ShellFlavor) -> Option<Token> {
    let before = text.get(..cursor)?;
    let line_start = before.rfind('\n').map_or(0, |p| p + 1);
    let mut start = line_start;
    let mut quote = None;
    let mut escaped = false;
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut iter = before[line_start..].char_indices().peekable();
    while let Some((offset, c)) = iter.next() {
        let offset = line_start + offset;
        if escaped { current.push(c); escaped = false; continue; }
        if (c == '\\' && shell == ShellFlavor::Posix && quote != Some('\''))
            || (c == '`' && shell == ShellFlavor::PowerShell && quote != Some('\'')) {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            if c == q {
                if shell == ShellFlavor::PowerShell && q == '\'' && iter.peek().is_some_and(|(_, n)| *n == '\'') {
                    iter.next(); current.push('\'');
                } else { quote = None; }
            } else {
                if q == '"' && matches!(c, '$' | '`') { return None; }
                current.push(c);
            }
        } else if c == '\'' || c == '"' {
            quote = Some(c);
        } else if c.is_whitespace() {
            if !current.is_empty() { words.push(std::mem::take(&mut current)); }
            start = offset + c.len_utf8();
        } else if matches!(c, ';' | '|' | '&') {
            words.clear(); current.clear(); start = offset + c.len_utf8();
        } else {
            if matches!(c, '$' | '`' | '(' | ')' | '<' | '>' | '*' | '?' | '[' | ']') { return None; }
            current.push(c);
        }
    }
    if escaped || current.is_empty() { return None; }
    let mut end = cursor;
    if let Some(next) = text.get(cursor..)?.chars().next() {
        if Some(next) == quote {
            end += next.len_utf8();
        } else if !next.is_whitespace() && !matches!(next, ';' | '|' | '&') {
            return None;
        }
    }
    let directories_only = words.first().is_some_and(|word| matches!(word.to_ascii_lowercase().as_str(), "cd" | "chdir" | "set-location" | "pushd" | "push-location"));
    Some(Token { range: start..end, value: current, command: words.is_empty(), directories_only })
}

fn matches_prefix(value: &str, prefix: &str, shell: ShellFlavor) -> bool {
    if shell == ShellFlavor::PowerShell { value.to_lowercase().starts_with(&prefix.to_lowercase()) }
    else { value.starts_with(prefix) }
}
fn quote_literal(value: &str, shell: ShellFlavor, command: bool) -> String {
    if !value.is_empty() && value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | ':' ) || (c == '\\' && shell == ShellFlavor::PowerShell)) {
        return value.into();
    }
    match shell {
        ShellFlavor::Posix => format!("'{}'", value.replace('\'', "'\"'\"'")),
        ShellFlavor::PowerShell => format!("{}'{}'", if command { "& " } else { "" }, value.replace('\'', "''")),
    }
}
fn executable(path: &Path) -> bool {
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        path.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))] {
        path.is_file() && path.extension().is_some_and(|ext| matches!(ext.to_string_lossy().to_ascii_lowercase().as_str(), "exe" | "com" | "cmd" | "bat" | "ps1"))
    }
}

pub fn suggest(request: Request) -> Vec<Suggestion> {
    if request.text.len() > 64 * 1024 { return vec![]; }
    let mut suggestions = Vec::new();
    let mut seen = BTreeSet::new();
    let prefix = match request.text.get(..request.cursor) { Some(value) => value, None => return vec![] };
    // History candidates remain drafts, never a request to re-execute an entry.
    if request.cursor == request.text.len() && !prefix.trim().is_empty() {
        for command in request.history.iter().take(64) {
            if command != prefix && command.starts_with(prefix) && seen.insert(command.clone()) {
                suggestions.push(Suggestion { label: format!("History · {}", command.replace('\n', " ↵ ")), replacement: command.clone(), range: 0..request.cursor });
            }
        }
    }
    let Some(token) = token(&request.text, request.cursor, request.shell) else { return suggestions; };
    let Some(cwd) = request.directory.filter(|p| p.is_absolute()) else { return suggestions; };
    let split = token.value.rfind(|c| c == '/' || (c == '\\' && request.shell == ShellFlavor::PowerShell)).map_or(0, |i| i + 1);
    let (directory_prefix, name_prefix) = token.value.split_at(split);
    // Tilde and variables belong to the shell. Don't pretend host state is shell state.
    if directory_prefix.starts_with('~') { return suggestions; }
    let directory = cwd.join(directory_prefix);
    let deadline = Instant::now() + Duration::from_millis(300);
    let mut budget = SCAN_BUDGET;
    let mut candidates: Vec<(PathBuf, bool)> = vec![(directory, false)];
    if token.command && directory_prefix.is_empty() {
        candidates.extend(request.path.into_iter().take(128).map(|path| (if path.is_absolute() { path } else { cwd.join(path) }, true)));
    }
    for (directory, from_path) in candidates {
        if budget == 0 || Instant::now() >= deadline || suggestions.len() >= LIMIT { break; }
        let Ok(entries) = std::fs::read_dir(&directory) else { continue; };
        for entry in entries.flatten() {
            if budget == 0 || Instant::now() >= deadline || suggestions.len() >= LIMIT { break; }
            budget -= 1;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue; };
            if name.len() > 4096 || !matches_prefix(name, name_prefix, request.shell) { continue; }
            let is_dir = entry.path().is_dir();
            if token.directories_only && !is_dir { continue; }
            if from_path && !executable(&entry.path()) { continue; }
            let separator = if request.shell == ShellFlavor::PowerShell { "\\" } else { "/" };
            let mut value = if from_path { name.to_string() } else { format!("{directory_prefix}{name}") };
            if is_dir { value.push_str(separator); }
            if !from_path && token.command && !is_dir && directory_prefix.is_empty() {
                if !executable(&entry.path()) { continue; }
                value = format!(".{separator}{value}");
            }
            let replacement = quote_literal(&value, request.shell, token.command);
            if seen.insert(replacement.clone()) {
                suggestions.push(Suggestion { label: value, replacement, range: token.range.clone() });
            }
        }
    }
    suggestions.sort_by(|a,b| a.label.cmp(&b.label));
    suggestions.truncate(LIMIT);
    suggestions
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_windows_paths_and_partial_quotes_without_evaluation() {
        let value = "Set-Location 'C:\\Users\\Müller\\My D";
        let t = token(value, value.len(), ShellFlavor::PowerShell).unwrap();
        assert_eq!(t.value, "C:\\Users\\Müller\\My D");
        assert!(t.directories_only);
        assert_eq!(quote_literal("a'b $c", ShellFlavor::PowerShell, false), "'a''b $c'");
        assert_eq!(quote_literal("my app.exe", ShellFlavor::PowerShell, true), "& 'my app.exe'");
        for value in ["echo $(evil)", "cat $HOME/a", "echo `evil`", "cat file*"] {
            assert!(token(value, value.len(), ShellFlavor::Posix).is_none());
        }
    }
    #[test]
    fn ranges_preserve_unicode_prefixes_and_following_arguments() {
        let text = "echo ä; cat 'my f' --flag";
        let cursor = text.find("' --flag").unwrap();
        let t = token(text, cursor, ShellFlavor::Posix).unwrap();
        assert_eq!(&text[t.range.clone()], "'my f'");
        let mut completed = text.to_string();
        completed.replace_range(t.range, &quote_literal("my file", ShellFlavor::Posix, false));
        assert_eq!(completed, "echo ä; cat 'my file' --flag");
    }
    #[test]
    fn completes_files_and_directories_relative_to_reported_cwd() {
        let base = std::env::temp_dir().join(format!("kea-completion-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(base.join("my directory")).unwrap();
        std::fs::write(base.join("my file"), "test").unwrap();
        let request = |text: &str| Request { text: text.into(), cursor: text.len(), directory: Some(base.clone()), shell: ShellFlavor::Posix, history: vec![], path: vec![] };
        let values = suggest(request("cat my"));
        assert!(values.iter().any(|v| v.replacement == "'my file'"));
        let values = suggest(request("cd my"));
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].replacement, "'my directory/'");
        std::fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn history_can_complete_without_a_known_filesystem() {
        let result = suggest(Request { text: "git st".into(), cursor: 6, directory: None, shell: ShellFlavor::Posix, history: vec!["git status".into()], path: vec![] });
        assert_eq!(result[0].replacement, "git status");
    }
}
