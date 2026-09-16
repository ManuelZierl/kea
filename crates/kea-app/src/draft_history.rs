use std::collections::VecDeque;

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
        let Some(index) = self.cursor else {
            return None;
        };
        if self.entries.get(index).is_some_and(|entry| entry != current) {
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
            if self.entries.get(index).is_some_and(|entry| entry != current) {
                self.cursor = None;
                self.scratch = current.to_owned();
            }
        }
    }
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
        assert_eq!(history.recent(10), vec!["two".to_string(), "three".to_string()]);
    }
}
