//! Keyboard bridge isolated from recording. Classic terminal encoding remains the
//! fallback; distinctions like modified Enter are emitted only after the child has
//! negotiated an extended keyboard protocol through the terminal emulator.
use gpui::Keystroke;

pub const MAX_TERMINAL_TEXT_BYTES: usize = 64 * 1024;

/// Whether a platform text replacement keeps the terminal composition buffer
/// within its bounded committed-input limit.
pub fn terminal_replacement_fits(
    current_bytes: usize,
    replaced_bytes: usize,
    replacement_bytes: usize,
) -> bool {
    current_bytes
        .saturating_sub(replaced_bytes)
        .saturating_add(replacement_bytes)
        <= MAX_TERMINAL_TEXT_BYTES
}

/// Marked composition owns confirmation/navigation keys. Without marked text,
/// GPUI's `is_ime_in_progress` is only a missing-character heuristic: it also
/// returns true for ordinary Enter/Tab events whose `key_char` is absent.
/// These named terminal controls must still reach the encoder.
pub fn defer_to_ime(key: &Keystroke, has_marked_text: bool) -> bool {
    has_marked_text
        || (key.is_ime_in_progress()
            && !matches!(key.key.as_str(), "enter" | "return" | "tab" | "space"))
}

/// Explicit clipboard-paste chord for the live terminal.
///
/// Plain Ctrl+V (Linux/Windows) remains ordinary child input (`0x16`) and
/// Cmd+V handling stays platform-native; clipboard paste into the terminal
/// uses Ctrl+Shift+V everywhere, plus Cmd+V on macOS. The terminal key handler
/// checks this before encoding so the same screen shortcut pastes clipboard
/// text (bracketed when supported) instead of sending a control byte.
pub fn is_terminal_paste(key: &Keystroke) -> bool {
    if key.key.as_str() != "v" {
        return false;
    }
    let m = key.modifiers;
    if m.alt || m.function {
        return false;
    }
    // macOS: Cmd+V (allow Shift for Cmd+Shift+V variants), no Ctrl.
    if m.platform && !m.control {
        return true;
    }
    // Everywhere: Ctrl+Shift+V, no platform key.
    m.control && m.shift && !m.platform
}

pub fn encode(
    key: &Keystroke,
    application_cursor: bool,
    extended_keyboard: bool,
) -> Option<Vec<u8>> {
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
        "f21" => Some(42),
        "f22" => Some(43),
        "f23" => Some(44),
        "f24" => Some(45),
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
        // Classic terminal protocols collapse modified Enter into CR. Only advertise
        // the CSI-u distinction after the running application negotiated it.
        "enter" | "return" if modifier != 1 && extended_keyboard => {
            format!("\x1b[13;{modifier}u").into_bytes()
        }
        "enter" | "return" => vec![b'\r'],
        "escape" => vec![27],
        "space" if !m.control => {
            if m.alt {
                vec![27, b' ']
            } else {
                vec![b' ']
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
#[path = "../../tests/unit/terminal/input.rs"]
mod tests;
