#[derive(Debug, Default)]
pub struct PromptLineTracker {
    remaining_ascii: Option<usize>,
    touched: bool,
}

impl PromptLineTracker {
    pub fn note_prompt(&mut self) {
        self.remaining_ascii = Some(0);
        self.touched = false;
    }

    pub fn note_text(&mut self, text: &str) {
        if text
            .bytes()
            .all(|byte| byte == b' ' || byte.is_ascii_graphic())
        {
            if let Some(remaining) = &mut self.remaining_ascii {
                *remaining = remaining.saturating_add(text.len());
                self.touched |= !text.is_empty();
            }
        } else {
            self.invalidate();
        }
    }

    pub fn note_backspace(&mut self) {
        match &mut self.remaining_ascii {
            Some(remaining) if *remaining > 0 => {
                *remaining -= 1;
                self.touched = true;
            }
            _ => self.invalidate(),
        }
    }

    pub fn invalidate(&mut self) {
        self.remaining_ascii = None;
        self.touched = false;
    }

    pub fn can_recover_empty_line(&self) -> bool {
        self.touched && self.remaining_ascii == Some(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_ascii_can_be_recovered_only_after_every_byte_is_erased() {
        let mut tracker = PromptLineTracker::default();
        tracker.note_prompt();
        tracker.note_text("abc");
        assert!(!tracker.can_recover_empty_line());
        tracker.note_backspace();
        tracker.note_backspace();
        tracker.note_backspace();
        assert!(tracker.can_recover_empty_line());
    }

    #[test]
    fn unsupported_edits_and_extra_backspace_invalidate_recovery() {
        for edit in ["ä", "line\n", "\u{1b}"] {
            let mut tracker = PromptLineTracker::default();
            tracker.note_prompt();
            tracker.note_text(edit);
            assert!(!tracker.can_recover_empty_line());
        }

        let mut tracker = PromptLineTracker::default();
        tracker.note_prompt();
        tracker.note_backspace();
        assert!(!tracker.can_recover_empty_line());
    }

    #[test]
    fn a_new_prompt_starts_a_fresh_candidate() {
        let mut tracker = PromptLineTracker::default();
        tracker.note_prompt();
        tracker.note_text("x");
        tracker.invalidate();
        tracker.note_prompt();
        tracker.note_text("y");
        tracker.note_backspace();
        assert!(tracker.can_recover_empty_line());
    }
}
