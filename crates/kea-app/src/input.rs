//! Basic keyboard bridge, isolated from recording. Full keyboard/IME coverage
//! is a separate compatibility milestone, not a claim of this bootstrap.
use gpui::Keystroke;

pub fn encode(key: &Keystroke, application_cursor: bool) -> Option<Vec<u8>> {
    let m = key.modifiers;
    if m.platform {
        return None;
    }
    let modifier = 1 + u8::from(m.shift) + 2 * u8::from(m.alt) + 4 * u8::from(m.control);
    let arrow = match key.key.as_str() {
        "up" => Some('A'),
        "down" => Some('B'),
        "right" => Some('C'),
        "left" => Some('D'),
        "home" => Some('H'),
        "end" => Some('F'),
        _ => None,
    };
    if let Some(arrow) = arrow {
        let sequence = if modifier != 1 {
            format!("\x1b[1;{modifier}{arrow}")
        } else if application_cursor {
            format!("\x1bO{arrow}")
        } else {
            format!("\x1b[{arrow}")
        };
        return Some(sequence.into_bytes());
    }
    let numbered = match key.key.as_str() {
        "insert" => Some(2),
        "delete" => Some(3),
        "pageup" => Some(5),
        "pagedown" => Some(6),
        "f5" => Some(15),
        "f6" => Some(17),
        "f7" => Some(18),
        "f8" => Some(19),
        "f9" => Some(20),
        "f10" => Some(21),
        "f11" => Some(23),
        "f12" => Some(24),
        "f13" => Some(25),
        "f14" => Some(26),
        "f15" => Some(28),
        "f16" => Some(29),
        "f17" => Some(31),
        "f18" => Some(32),
        "f19" => Some(33),
        "f20" => Some(34),
        _ => None,
    };
    if let Some(number) = numbered {
        return Some(
            if modifier == 1 {
                format!("\x1b[{number}~")
            } else {
                format!("\x1b[{number};{modifier}~")
            }
            .into_bytes(),
        );
    }
    if let Some(letter) = match key.key.as_str() {
        "f1" => Some('P'),
        "f2" => Some('Q'),
        "f3" => Some('R'),
        "f4" => Some('S'),
        _ => None,
    } {
        return Some(
            if modifier == 1 {
                format!("\x1bO{letter}")
            } else {
                format!("\x1b[1;{modifier}{letter}")
            }
            .into_bytes(),
        );
    }
    let bytes = match key.key.as_str() {
        // Control keys have explicit terminal semantics, even when GPUI supplies
        // no completed text character. Do not run them through the IME text gate.
        "enter" | "return" if modifier != 1 => format!("\x1b[13;{modifier}u").into_bytes(),
        "enter" | "return" => vec![b'\r'],
        // Windows GPUI intentionally reports named Space without key_char.
        // It is not an unfinished composition; encode it before the text gate.
        "space" | " " => {
            let mut bytes = vec![if m.control { 0 } else { b' ' }];
            if m.alt {
                bytes.insert(0, 27);
            }
            bytes
        }
        "escape" => {
            if m.alt {
                vec![27, 27]
            } else {
                vec![27]
            }
        }
        "backspace" => vec![if m.control { 8 } else { 127 }],
        "tab" if m.shift => b"\x1b[Z".to_vec(),
        "tab" => vec![b'\t'],
        _ => {
            if key.is_ime_in_progress() {
                return None;
            }
            // Prefer composed text for AltGr, not a Ctrl+Alt control character.
            if m.control && m.alt {
                if let Some(text) = &key.key_char {
                    return Some(text.as_bytes().to_vec());
                }
            }
            if m.control {
                let c = match key.key.as_str() {
                    "space" | "@" | "2" => 0,
                    "[" => 27,
                    "\\" => 28,
                    "]" => 29,
                    "^" | "6" => 30,
                    "_" | "-" => 31,
                    value if value.len() == 1 && value.as_bytes()[0].is_ascii_alphabetic() => {
                        value.as_bytes()[0].to_ascii_uppercase() - b'@'
                    }
                    _ => return None,
                };
                let mut bytes = vec![c];
                if m.alt {
                    bytes.insert(0, 27);
                }
                return Some(bytes);
            }
            let text = key
                .key_char
                .as_deref()
                .or_else(|| (key.key.chars().count() == 1).then_some(key.key.as_str()))?;
            let mut bytes = text.as_bytes().to_vec();
            if m.alt {
                bytes.insert(0, 27);
            }
            return Some(bytes);
        }
    };
    Some(bytes)
}

/// Hand an unexecuted single-line draft to the running application's own
/// completion. Never append Enter and never inject shell-specific wrappers.
pub fn complete(text: &str, bracketed: bool) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(
        !text.contains(['\r', '\n']),
        "Completion handoff requires a single-line draft."
    );
    anyhow::ensure!(
        !text.chars().any(char::is_control),
        "Completion handoff refuses control characters."
    );
    let mut bytes = paste(text, bracketed)?;
    bytes.push(b'\t');
    Ok(bytes)
}

/// Explicit Send submits the text once. Multiline input requires the child's
/// bracketed-paste support; otherwise preserve the draft instead of running lines.
pub fn submit(text: &str, bracketed: bool) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(
        !text.contains(['\x1b', '\0']),
        "Input contains terminal control characters."
    );
    let mut bytes = paste(text, bracketed)?;
    bytes.push(b'\r');
    Ok(bytes)
}

pub fn paste(text: &str, bracketed: bool) -> anyhow::Result<Vec<u8>> {
    let text = text
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace(['\x1b', '\0'], "");
    if !bracketed && text.contains('\n') {
        anyhow::bail!("Multiline paste requires bracketed-paste support in the running program.");
    }
    Ok(if bracketed {
        format!("\x1b[200~{text}\x1b[201~").into_bytes()
    } else {
        text.into_bytes()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enter_shift_enter_and_interrupt_are_distinct() {
        assert_eq!(
            encode(&Keystroke::parse("enter").unwrap(), false).unwrap(),
            b"\r"
        );
        assert_eq!(
            encode(&Keystroke::parse("shift-enter").unwrap(), false).unwrap(),
            b"\x1b[13;2u"
        );
        assert_eq!(
            encode(&Keystroke::parse("ctrl-c").unwrap(), false).unwrap(),
            vec![3]
        );
        assert_eq!(
            encode(&Keystroke::parse("tab").unwrap(), false).unwrap(),
            b"\t"
        );
        assert_eq!(
            encode(&Keystroke::parse("shift-tab").unwrap(), false).unwrap(),
            b"\x1b[Z"
        );
    }
    #[test]
    fn text_requires_a_completed_character_but_keeps_unicode() {
        assert!(encode(&Keystroke::parse("a").unwrap(), false).is_none());
        assert_eq!(
            encode(
                &Keystroke::parse("space").unwrap().with_simulated_ime(),
                false
            )
            .unwrap(),
            b" "
        );
        assert_eq!(
            encode(&Keystroke::parse("a->ä").unwrap(), false).unwrap(),
            "ä".as_bytes()
        );
    }
    #[test]
    fn application_cursor_and_modified_arrows() {
        assert_eq!(
            encode(&Keystroke::parse("up").unwrap(), true).unwrap(),
            b"\x1bOA"
        );
        assert_eq!(
            encode(&Keystroke::parse("ctrl-left").unwrap(), true).unwrap(),
            b"\x1b[1;5D"
        );
    }
    #[test]
    fn named_windows_space_and_all_history_function_keys_reach_child() {
        for (spec, expected) in [
            ("space", b" ".as_slice()),
            ("shift-space", b" "),
            ("ctrl-space", b"\0"),
            ("alt-space", b"\x1b "),
            ("f6", b"\x1b[17~"),
            ("f7", b"\x1b[18~"),
            ("f8", b"\x1b[19~"),
            ("f9", b"\x1b[20~"),
            ("ctrl-c", b"\x03"),
            ("ctrl-v", b"\x16"),
            ("ctrl-z", b"\x1a"),
        ] {
            assert_eq!(
                encode(&Keystroke::parse(spec).unwrap(), false).as_deref(),
                Some(expected),
                "{spec}"
            );
        }
    }
    #[test]
    fn draft_handoff_does_not_execute_or_strip_spaces() {
        assert_eq!(complete("git ch", false).unwrap(), b"git ch\t");
        assert_eq!(complete("", false).unwrap(), b"\t");
        assert!(complete("echo one\necho two", true).is_err());
        assert!(complete("abc\tdef", false).is_err());
        assert_eq!(
            submit("  hello world  ", false).unwrap(),
            b"  hello world  \r"
        );
        assert!(submit("one\ntwo", false).is_err());
        assert_eq!(
            submit("one\ntwo", true).unwrap(),
            b"\x1b[200~one\ntwo\x1b[201~\r"
        );
    }
    #[test]
    fn paste_cannot_inject_a_bracket_terminator() {
        assert!(paste("a\nb", false).is_err());
        assert_eq!(
            paste("a\x1b[201~b", true).unwrap(),
            b"\x1b[200~a[201~b\x1b[201~"
        );
    }
}
