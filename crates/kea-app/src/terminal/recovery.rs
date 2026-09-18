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
#[path = "../../tests/unit/terminal/recovery.rs"]
mod tests;
