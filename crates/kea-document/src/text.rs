pub(crate) fn terminalish_text(bytes: &[u8]) -> String {
    let stripped = strip_escape_sequences(bytes);
    let text = String::from_utf8_lossy(&stripped);
    let mut lines: Vec<Vec<char>> = vec![Vec::new()];
    let mut row = 0usize;
    let mut column = 0usize;
    for ch in text.chars() {
        match ch {
            '\r' => column = 0,
            '\n' => {
                row += 1;
                column = 0;
                if lines.len() <= row {
                    lines.push(Vec::new());
                }
            }
            '\u{8}' => column = column.saturating_sub(1),
            '\t' => {
                let next = (column / 8 + 1) * 8;
                while column < next {
                    write_char(&mut lines[row], column, ' ');
                    column += 1;
                }
            }
            ch if !ch.is_control() => {
                write_char(&mut lines[row], column, ch);
                column += 1;
            }
            _ => {}
        }
    }
    let mut rendered = lines
        .into_iter()
        .map(|mut line| {
            while line.last() == Some(&' ') {
                line.pop();
            }
            line.into_iter().collect::<String>()
        })
        .collect::<Vec<_>>();
    while rendered.last().is_some_and(String::is_empty) {
        rendered.pop();
    }
    rendered.join("\n")
}

fn write_char(line: &mut Vec<char>, column: usize, ch: char) {
    if line.len() <= column {
        line.resize(column + 1, ' ');
    }
    line[column] = ch;
}

fn strip_escape_sequences(bytes: &[u8]) -> Vec<u8> {
    #[derive(Clone, Copy)]
    enum State {
        Ground,
        Escape,
        Csi,
        String,
        StringEscape,
    }
    let mut state = State::Ground;
    let mut out = Vec::with_capacity(bytes.len());
    for &byte in bytes {
        state = match state {
            State::Ground => match byte {
                0x1b => State::Escape,
                0x00..=0x06 | 0x07 | 0x0b..=0x0c | 0x0e..=0x1f | 0x7f => State::Ground,
                _ => {
                    out.push(byte);
                    State::Ground
                }
            },
            State::Escape => match byte {
                b'[' => State::Csi,
                b']' | b'P' | b'X' | b'^' | b'_' => State::String,
                _ => State::Ground,
            },
            State::Csi => {
                if (0x40..=0x7e).contains(&byte) {
                    State::Ground
                } else {
                    State::Csi
                }
            }
            State::String => match byte {
                0x07 => State::Ground,
                0x1b => State::StringEscape,
                _ => State::String,
            },
            State::StringEscape => {
                if byte == b'\\' {
                    State::Ground
                } else if byte == 0x1b {
                    State::StringEscape
                } else {
                    State::String
                }
            }
        };
    }
    out
}

#[cfg(test)]
#[path = "../tests/unit/text.rs"]
mod tests;
