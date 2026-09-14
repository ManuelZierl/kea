//! Small keyboard bridge, isolated from recording. Full keyboard/IME coverage
//! is a separate compatibility milestone, not a claim of this bootstrap.
use gpui::Keystroke;

pub fn encode(key: &Keystroke, application_cursor: bool) -> Option<Vec<u8>> {
    let m = key.modifiers;
    if m.platform || key.is_ime_in_progress() { return None; }
    let modifier = 1 + u8::from(m.shift) + 2 * u8::from(m.alt) + 4 * u8::from(m.control);
    let arrow = match key.key.as_str() { "up" => Some('A'), "down" => Some('B'), "right" => Some('C'), "left" => Some('D'), "home" => Some('H'), "end" => Some('F'), _ => None };
    if let Some(arrow) = arrow {
        let sequence = if modifier != 1 { format!("\x1b[1;{modifier}{arrow}") } else if application_cursor { format!("\x1bO{arrow}") } else { format!("\x1b[{arrow}") };
        return Some(sequence.into_bytes());
    }
    let numbered = match key.key.as_str() { "insert" => Some(2), "delete" => Some(3), "pageup" => Some(5), "pagedown" => Some(6), "f5" => Some(15), "f10" => Some(21), "f11" => Some(23), "f12" => Some(24), _ => None };
    if let Some(number) = numbered {
        return Some(if modifier == 1 { format!("\x1b[{number}~") } else { format!("\x1b[{number};{modifier}~") }.into_bytes());
    }
    if let Some(letter) = match key.key.as_str() { "f1" => Some('P'), "f2" => Some('Q'), "f3" => Some('R'), "f4" => Some('S'), _ => None } {
        return Some(if modifier == 1 { format!("\x1bO{letter}") } else { format!("\x1b[1;{modifier}{letter}") }.into_bytes());
    }
    let bytes = match key.key.as_str() {
        // Explicit CSI-u binding keeps modified Enter distinguishable for OpenCode.
        "enter" if modifier != 1 => format!("\x1b[13;{modifier}u").into_bytes(),
        "enter" => vec![b'\r'],
        "escape" => vec![27],
        "backspace" => vec![if m.control { 8 } else { 127 }],
        "tab" if m.shift => b"\x1b[Z".to_vec(),
        "tab" => vec![b'\t'],
        _ => {
            // Prefer composed text for AltGr rather than interpreting Ctrl+Alt as
            // a control character. This does not substitute for a full IME bridge.
            if m.control && m.alt {
                if let Some(text) = &key.key_char { return Some(text.as_bytes().to_vec()); }
            }
            if m.control {
                let c = match key.key.as_str() { "space" | "@" | "2" => 0, "[" => 27, "\\" => 28, "]" => 29, "^" | "6" => 30, "_" | "-" => 31, value if value.len() == 1 && value.as_bytes()[0].is_ascii_alphabetic() => value.as_bytes()[0].to_ascii_uppercase() - b'@', _ => return None };
                let mut bytes = vec![c];
                if m.alt { bytes.insert(0, 27); }
                return Some(bytes);
            }
            let text = key.key_char.as_deref().or_else(|| (key.key.chars().count() == 1).then_some(key.key.as_str()))?;
            let mut bytes = text.as_bytes().to_vec();
            if m.alt { bytes.insert(0, 27); }
            return Some(bytes);
        }
    };
    Some(bytes)
}

pub fn paste(text: &str, bracketed: bool) -> anyhow::Result<Vec<u8>> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n").replace(['\x1b', '\0'], "");
    if !bracketed && text.contains('\n') { anyhow::bail!("Multiline paste requires bracketed-paste support in the running program."); }
    Ok(if bracketed { format!("\x1b[200~{text}\x1b[201~").into_bytes() } else { text.into_bytes() })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enter_shift_enter_and_interrupt_are_distinct() {
        assert_eq!(encode(&Keystroke::parse("enter").unwrap(), false).unwrap(), b"\r");
        assert_eq!(encode(&Keystroke::parse("shift-enter").unwrap(), false).unwrap(), b"\x1b[13;2u");
        assert_eq!(encode(&Keystroke::parse("ctrl-c").unwrap(), false).unwrap(), vec![3]);
    }
    #[test]
    fn application_cursor_and_modified_arrows() {
        assert_eq!(encode(&Keystroke::parse("up").unwrap(), true).unwrap(), b"\x1bOA");
        assert_eq!(encode(&Keystroke::parse("ctrl-left").unwrap(), true).unwrap(), b"\x1b[1;5D");
    }
    #[test]
    fn paste_cannot_inject_a_bracket_terminator() {
        assert!(paste("a\nb", false).is_err());
        assert_eq!(paste("a\x1b[201~b", true).unwrap(), b"\x1b[200~a[201~b\x1b[201~");
    }
}
