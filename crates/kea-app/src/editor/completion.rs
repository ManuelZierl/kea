//! Bounded local editor completion. Never evaluate shell code to produce suggestions.
use crate::shell::ShellFlavor;
use std::{
    collections::BTreeSet,
    ffi::OsString,
    ops::Range,
    path::{Path, PathBuf},
};

const MAX_RESULTS: usize = 40;
const MAX_DIRECTORY_ENTRIES: usize = 4096;
const MAX_PATH_ENTRIES: usize = 8192;
const MAX_PATH_DIRS: usize = 256;

#[derive(Clone, Debug)]
pub struct Candidate {
    pub label: String,
    pub replacement: String,
    pub range: Range<usize>,
}

/// Suggest retained commands, executable names from the *reported shell PATH*, and
/// local filesystem paths relative to the *reported shell cwd*. Run this off the UI
/// thread. Complex shell expressions are deliberately left to the shell's native Tab.
pub fn suggest(
    text: &str,
    cursor: usize,
    directory: Option<&Path>,
    shell_path: Option<&str>,
    history: &[String],
    shell: Option<ShellFlavor>,
) -> Vec<Candidate> {
    if cursor > text.len() || !text.is_char_boundary(cursor) {
        return Vec::new();
    }
    let prefix = &text[..cursor];
    let mut results = Vec::new();
    let mut seen = BTreeSet::new();

    if cursor == text.len() && !prefix.trim().is_empty() {
        for command in history.iter().rev() {
            if command.starts_with(prefix) && command != prefix && seen.insert(command.clone()) {
                results.push(Candidate {
                    label: format!("History: {command}"),
                    replacement: command.clone(),
                    range: 0..cursor,
                });
                if results.len() == MAX_RESULTS {
                    return results;
                }
            }
        }
    }

    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let line = &prefix[line_start..];
    if line.chars().any(is_complex_shell_character) {
        return results;
    }
    if !cfg!(windows) && line.contains('\\') {
        return results;
    }

    let token_start = prefix
        .char_indices()
        .take_while(|(index, _)| *index < cursor)
        .filter(|(_, ch)| ch.is_whitespace())
        .map(|(index, ch)| index + ch.len_utf8())
        .last()
        .unwrap_or(line_start)
        .max(line_start);
    let token = &prefix[token_start..cursor];
    let before_token = &prefix[line_start..token_start];
    let command_position = before_token.trim().is_empty();

    if command_position && !token.is_empty() && !token.contains(['/', '\\']) {
        if let Some(path) = shell_path {
            complete_commands(
                token,
                token_start..cursor,
                path,
                directory,
                shell,
                &mut results,
                &mut seen,
            );
            if results.len() == MAX_RESULTS {
                return results;
            }
        }
    }

    if let Some(directory) = directory {
        complete_paths(
            token,
            token_start..cursor,
            directory,
            shell,
            &mut results,
            &mut seen,
        );
    }
    results
}

fn is_complex_shell_character(ch: char) -> bool {
    matches!(
        ch,
        '\'' | '"' | '`' | '$' | '*' | '?' | '[' | ']' | '(' | ')' | ';' | '|' | '&' | '<' | '>'
    )
}

fn complete_commands(
    token: &str,
    range: Range<usize>,
    shell_path: &str,
    reported_cwd: Option<&Path>,
    shell: Option<ShellFlavor>,
    results: &mut Vec<Candidate>,
    seen: &mut BTreeSet<String>,
) {
    let mut candidates = BTreeSet::new();
    let path = OsString::from(shell_path);
    let mut inspected = 0usize;
    for path_entry in std::env::split_paths(&path).take(MAX_PATH_DIRS) {
        let directory = if path_entry.is_absolute() {
            path_entry
        } else {
            let Some(cwd) = reported_cwd else {
                continue;
            };
            if path_entry.as_os_str().is_empty() {
                cwd.to_path_buf()
            } else {
                cwd.join(path_entry)
            }
        };
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            inspected += 1;
            if inspected > MAX_PATH_ENTRIES {
                break;
            }
            let Some(name) = executable_name(&entry.path()) else {
                continue;
            };
            if name.starts_with(token) && name != token {
                candidates.insert(name);
            }
        }
        if inspected > MAX_PATH_ENTRIES {
            break;
        }
    }
    for name in candidates {
        let replacement = shell_quote(&name, shell);
        if seen.insert(replacement.clone()) {
            results.push(Candidate {
                label: format!("Command: {name}"),
                replacement,
                range: range.clone(),
            });
            if results.len() == MAX_RESULTS {
                return;
            }
        }
    }
}

fn executable_name(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?.to_string();
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = metadata.permissions().mode();
        (mode & 0o111 != 0).then_some(name)
    }
    #[cfg(windows)]
    {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !matches!(extension.as_str(), "exe" | "com" | "bat" | "cmd" | "ps1") {
            return None;
        }
        if matches!(extension.as_str(), "exe" | "com" | "bat" | "cmd") {
            return path.file_stem()?.to_str().map(ToOwned::to_owned);
        }
        Some(name)
    }
    #[cfg(not(any(unix, windows)))]
    {
        Some(name)
    }
}

fn complete_paths(
    token: &str,
    range: Range<usize>,
    directory: &Path,
    shell: Option<ShellFlavor>,
    results: &mut Vec<Candidate>,
    seen: &mut BTreeSet<String>,
) {
    if token.starts_with('-') {
        return;
    }
    let separator = token.rfind(|ch| ch == '/' || (cfg!(windows) && ch == '\\'));
    let (parent, leaf) =
        separator.map_or(("", token), |index| (&token[..=index], &token[index + 1..]));
    let path = if parent.starts_with("~/") {
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .map(PathBuf::from)
            .map(|home| home.join(&parent[2..]))
    } else {
        Some(directory.join(parent))
    };
    let Some(path) = path else {
        return;
    };
    if cfg!(windows) && path.to_string_lossy().starts_with(r"\\") {
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    let mut names = Vec::new();
    for entry in entries.take(MAX_DIRECTORY_ENTRIES).flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name.chars().any(char::is_control) {
            continue;
        }
        let matches = if cfg!(windows) {
            name.to_lowercase().starts_with(&leaf.to_lowercase())
        } else {
            name.starts_with(leaf)
        };
        if matches && (!name.starts_with('.') || leaf.starts_with('.')) {
            names.push((name, entry.file_type().is_ok_and(|kind| kind.is_dir())));
        }
    }
    names.sort();
    for (name, is_dir) in names {
        let value = format!("{parent}{name}{}", if is_dir { "/" } else { "" });
        let replacement = if let Some(relative) = value.strip_prefix("~/") {
            match std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
                Some(home) => {
                    shell_quote(&PathBuf::from(home).join(relative).to_string_lossy(), shell)
                }
                None => continue,
            }
        } else {
            shell_quote(&value, shell)
        };
        if replacement != token && seen.insert(replacement.clone()) {
            results.push(Candidate {
                label: format!("Path: {value}"),
                replacement,
                range: range.clone(),
            });
            if results.len() == MAX_RESULTS {
                return;
            }
        }
    }
}

fn shell_quote(value: &str, shell: Option<ShellFlavor>) -> String {
    let leading_tilde_is_literal = value.starts_with('~') && !value.starts_with("~/");
    if !leading_tilde_is_literal
        && value.chars().all(|ch| {
            ch.is_alphanumeric() || "._-/:~".contains(ch) || (cfg!(windows) && ch == '\\')
        })
    {
        return value.to_owned();
    }
    match shell {
        Some(ShellFlavor::Posix) => format!("'{}'", value.replace('\'', "'\"'\"'")),
        Some(ShellFlavor::PowerShell) => format!("'{}'", value.replace('\'', "''")),
        None => value.to_owned(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/editor/completion.rs"]
mod tests;
