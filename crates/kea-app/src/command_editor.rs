#[derive(Default)]
pub struct CommandEditor {
    text: String,
    cursor: usize,
}

impl CommandEditor {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn insert(&mut self, text: &str) {
        let text = sanitize(text);
        self.text.insert_str(self.cursor, &text);
        self.cursor += text.len();
    }

    pub fn newline(&mut self) {
        self.insert("\n");
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let previous = self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0);
        self.text.replace_range(previous..self.cursor, "");
        self.cursor = previous;
    }

    pub fn delete(&mut self) {
        if self.cursor == self.text.len() {
            return;
        }
        let next = self.text[self.cursor..]
            .char_indices()
            .nth(1)
            .map(|(index, _)| self.cursor + index)
            .unwrap_or(self.text.len());
        self.text.replace_range(self.cursor..next, "");
    }

    pub fn left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor = self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0);
    }

    pub fn right(&mut self) {
        if self.cursor == self.text.len() {
            return;
        }
        self.cursor = self.text[self.cursor..]
            .char_indices()
            .nth(1)
            .map(|(index, _)| self.cursor + index)
            .unwrap_or(self.text.len());
    }

    pub fn home(&mut self) {
        self.cursor = self.text[..self.cursor]
            .rfind('\n')
            .map_or(0, |index| index + 1);
    }

    pub fn end(&mut self) {
        self.cursor = self.text[self.cursor..]
            .find('\n')
            .map_or(self.text.len(), |index| self.cursor + index);
    }

    pub fn up(&mut self) {
        self.move_vertical(-1);
    }

    pub fn down(&mut self) {
        self.move_vertical(1);
    }

    fn move_vertical(&mut self, direction: isize) {
        let line_start = self.text[..self.cursor]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        let column = self.text[line_start..self.cursor].chars().count();
        if direction < 0 {
            if line_start == 0 {
                return;
            }
            let previous_end = line_start - 1;
            let previous_start = self.text[..previous_end]
                .rfind('\n')
                .map_or(0, |index| index + 1);
            self.cursor = byte_at_column(&self.text, previous_start, previous_end, column);
        } else {
            let line_end = self.text[self.cursor..]
                .find('\n')
                .map_or(self.text.len(), |index| self.cursor + index);
            if line_end == self.text.len() {
                return;
            }
            let next_start = line_end + 1;
            let next_end = self.text[next_start..]
                .find('\n')
                .map_or(self.text.len(), |index| next_start + index);
            self.cursor = byte_at_column(&self.text, next_start, next_end, column);
        }
    }

    pub fn clear_after_submit(&mut self) -> String {
        self.cursor = 0;
        std::mem::take(&mut self.text)
    }

    pub fn rendered(&self) -> String {
        let mut rendered = self.text.clone();
        rendered.insert_str(self.cursor, "▏");
        rendered
    }
}

pub fn terminal_bytes(text: &str) -> Vec<u8> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut bytes = normalized.replace('\n', "\r").into_bytes();
    if bytes.last().copied() != Some(b'\r') {
        bytes.push(b'\r');
    }
    bytes
}

fn sanitize(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace(['\x1b', '\0'], "")
}

fn byte_at_column(text: &str, start: usize, end: usize, column: usize) -> usize {
    text[start..end]
        .char_indices()
        .nth(column)
        .map_or(end, |(offset, _)| start + offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_handles_multiline_unicode_without_splitting_codepoints() {
        let mut editor = CommandEditor::default();
        editor.insert("echo ä\nsecond");
        editor.home();
        editor.up();
        editor.end();
        editor.insert("!");
        assert_eq!(editor.text(), "echo ä!\nsecond");
        editor.backspace();
        editor.backspace();
        assert_eq!(editor.text(), "echo \nsecond");
    }

    #[test]
    fn vertical_movement_preserves_character_column_where_possible() {
        let mut editor = CommandEditor::default();
        editor.insert("12345\nab\nABCDE");
        editor.up();
        assert_eq!(editor.rendered(), "12345\nab▏\nABCDE");
        editor.up();
        assert_eq!(editor.rendered(), "12▏345\nab\nABCDE");
        editor.down();
        editor.down();
        assert_eq!(editor.rendered(), "12345\nab\nAB▏CDE");
    }

    #[test]
    fn submitted_multiline_text_is_sent_as_terminal_enters() {
        assert_eq!(
            terminal_bytes("printf one\nprintf two"),
            b"printf one\rprintf two\r"
        );
        assert_eq!(terminal_bytes("echo done\n"), b"echo done\r");
    }

    #[test]
    fn paste_is_sanitized_before_becoming_editor_text() {
        let mut editor = CommandEditor::default();
        editor.insert("a\r\nb\x1b[31m\0");
        assert_eq!(editor.text(), "a\nb[31m");
    }
}
