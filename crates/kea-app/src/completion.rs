//! Conservative local suggestions. Never evaluate shell code to complete a draft.
use crate::shell::ShellFlavor;
use std::{
    collections::BTreeSet,
    ops::Range,
    path::{Path, PathBuf},
};
const MAX_RESULTS: usize = 20;
const MAX_ENTRIES: usize = 4096;
#[derive(Clone, Debug)]
pub struct Candidate {
    pub label: String,
    pub replacement: String,
    pub range: Range<usize>,
}
/// Run off the GUI thread. Complex shell expressions are left to native Tab.
pub fn suggest(
    text: &str,
    cursor: usize,
    directory: Option<&Path>,
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
    let Some(directory) = directory else {
        return results;
    };
    let line = prefix.rsplit('\n').next().unwrap_or(prefix);
    if line.chars().any(|c| {
        matches!(
            c,
            '\'' | '"'
                | '`'
                | '$'
                | '*'
                | '?'
                | '['
                | ']'
                | '('
                | ')'
                | ';'
                | '|'
                | '&'
                | '<'
                | '>'
        )
    }) {
        return results;
    }
    if !cfg!(windows) && line.contains('\\') {
        return results;
    }
    let start = prefix.rfind(char::is_whitespace).map_or(0, |n| n + 1);
    let token = &prefix[start..];
    if token.is_empty() || token.starts_with('-') {
        return results;
    }
    let separator = token.rfind(|c| c == '/' || (cfg!(windows) && c == '\\'));
    let (parent, leaf) = separator.map_or(("", token), |n| (&token[..=n], &token[n + 1..]));
    let path = if parent.starts_with("~/") {
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .map(PathBuf::from)
            .map(|home| home.join(&parent[2..]))
    } else {
        Some(directory.join(parent))
    };
    let Some(path) = path else {
        return results;
    };
    if cfg!(windows) && path.to_string_lossy().starts_with(r"\\") {
        return results;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return results;
    };
    let mut names = Vec::new();
    for entry in entries.take(MAX_ENTRIES).flatten() {
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
            names.push((name, entry.file_type().is_ok_and(|t| t.is_dir())));
        }
    }
    names.sort();
    for (name, is_dir) in names {
        let value = format!("{parent}{name}{}", if is_dir { "/" } else { "" });
        let replacement = if value
            .chars()
            .all(|c| c.is_alphanumeric() || "._-/:~".contains(c) || (cfg!(windows) && c == '\\'))
        {
            value.clone()
        } else {
            let value = if value.starts_with("~/") {
                match std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
                    Some(home) => PathBuf::from(home)
                        .join(&value[2..])
                        .to_string_lossy()
                        .into_owned(),
                    None => continue,
                }
            } else {
                value.clone()
            };
            match shell {
                Some(ShellFlavor::Posix) => format!("'{}'", value.replace('\'', "'\"'\"'")),
                Some(ShellFlavor::PowerShell) => format!("'{}'", value.replace('\'', "''")),
                None => continue,
            }
        };
        if replacement != token && seen.insert(replacement.clone()) {
            results.push(Candidate {
                label: format!("Path: {value}"),
                replacement,
                range: start..cursor,
            });
            if results.len() == MAX_RESULTS {
                break;
            }
        }
    }
    results
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_is_literal_and_does_not_run_anything() {
        let history = vec!["echo $(danger)".into(), "echo hello".into()];
        let c = suggest("echo h", 6, None, &history, None);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].replacement, "echo hello");
        assert!(suggest("ä", 1, None, &history, None).is_empty());
    }
    #[test]
    fn paths_are_quoted_and_replace_only_the_token() {
        let root = std::env::temp_dir().join(format!("kea-complete-{}", std::process::id()));
        std::fs::create_dir_all(root.join("space ä")).unwrap();
        let c = suggest("cd spa", 6, Some(&root), &[], Some(ShellFlavor::Posix));
        assert_eq!(c[0].replacement, "'space ä/'");
        assert_eq!(c[0].range, 3..6);
        assert!(suggest("echo $(spa", 10, Some(&root), &[], Some(ShellFlavor::Posix)).is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
}
