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
    shell: Option<ShellFlavor>,
    results: &mut Vec<Candidate>,
    seen: &mut BTreeSet<String>,
) {
    let mut candidates = BTreeSet::new();
    let path = OsString::from(shell_path);
    let mut inspected = 0usize;
    for directory in std::env::split_paths(&path).take(MAX_PATH_DIRS) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            inspected += 1;
            if inspected > MAX_PATH_ENTRIES {
                break;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_file() {
                continue;
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
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(path).ok()?.permissions().mode();
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
    if token.is_empty() || token.starts_with('-') {
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
        let replacement = if value.starts_with("~/") {
            match std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
                Some(home) => shell_quote(
                    &PathBuf::from(home).join(&value[2..]).to_string_lossy(),
                    shell,
                ),
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
    if value
        .chars()
        .all(|ch| ch.is_alphanumeric() || "._-/:~".contains(ch) || (cfg!(windows) && ch == '\\'))
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
mod tests {
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
}
