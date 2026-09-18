use super::*;

#[test]
fn named_terminal_keys_without_key_char_are_not_unfinished_composition() {
    for (spec, bytes) in [
        ("enter", b"\r".as_slice()),
        ("return", b"\r".as_slice()),
        ("shift-enter", b"\r".as_slice()),
        ("ctrl-enter", b"\r".as_slice()),
        ("tab", b"\t".as_slice()),
        ("shift-tab", b"\x1b[Z".as_slice()),
        ("space", b" ".as_slice()),
    ] {
        let key = Keystroke::parse(spec).unwrap();
        assert!(key.key_char.is_none());
        assert!(!defer_to_ime(&key, false), "swallowed {spec}");
        assert_eq!(encode(&key, false, false).as_deref(), Some(bytes));
        assert!(defer_to_ime(&key, true), "composition lost {spec}");
    }
}

#[test]
fn actual_composition_still_precedes_terminal_and_selection_routing() {
    assert!(defer_to_ime(&Keystroke::parse("a").unwrap(), false));
    assert!(!defer_to_ime(&Keystroke::parse("a->ä").unwrap(), false));
    for spec in ["enter", "escape", "left", "shift-right", "ctrl-c"] {
        assert!(defer_to_ime(&Keystroke::parse(spec).unwrap(), true));
    }
}

#[test]
fn modified_enter_respects_keyboard_protocol_negotiation() {
    let enter = Keystroke::parse("enter").unwrap();
    let shifted = Keystroke::parse("shift-enter").unwrap();
    assert_eq!(encode(&enter, false, false).unwrap(), b"\r");
    assert_eq!(encode(&shifted, false, false).unwrap(), b"\r");
    assert_eq!(encode(&shifted, false, true).unwrap(), b"\x1b[13;2u");
    assert_eq!(
        encode(&Keystroke::parse("ctrl-c").unwrap(), false, false).unwrap(),
        vec![3]
    );
    assert_eq!(
        encode(&Keystroke::parse("tab").unwrap(), false, false).unwrap(),
        b"\t"
    );
    assert_eq!(
        encode(&Keystroke::parse("shift-tab").unwrap(), false, false).unwrap(),
        b"\x1b[Z"
    );
}

#[test]
fn text_requires_a_completed_character_but_keeps_unicode() {
    assert!(encode(&Keystroke::parse("a").unwrap(), false, false).is_none());
    assert_eq!(
        encode(
            &Keystroke::parse("space").unwrap().with_simulated_ime(),
            false,
            false,
        )
        .unwrap(),
        b" "
    );
    assert_eq!(
        encode(&Keystroke::parse("a->ä").unwrap(), false, false).unwrap(),
        "ä".as_bytes()
    );
}

#[test]
fn application_cursor_and_modified_arrows() {
    assert_eq!(
        encode(&Keystroke::parse("up").unwrap(), true, false).unwrap(),
        b"\x1bOA"
    );
    assert_eq!(
        encode(&Keystroke::parse("ctrl-left").unwrap(), true, false).unwrap(),
        b"\x1b[1;5D"
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

#[test]
fn terminal_paste_chord_leaves_plain_ctrl_v_as_child_input() {
    // Plain Ctrl+V must keep encoding to 0x16 for the child (e.g. OpenCode).
    assert_eq!(
        encode(&Keystroke::parse("ctrl-v").unwrap(), false, false).unwrap(),
        vec![22]
    );
    assert!(!is_terminal_paste(&Keystroke::parse("ctrl-v").unwrap()));
    // Explicit clipboard paste: Ctrl+Shift+V everywhere, Cmd+V on macOS.
    assert!(is_terminal_paste(
        &Keystroke::parse("ctrl-shift-v").unwrap()
    ));
    assert!(is_terminal_paste(&Keystroke::parse("cmd-v").unwrap()));
    assert!(is_terminal_paste(&Keystroke::parse("cmd-shift-v").unwrap()));
    assert!(!is_terminal_paste(&Keystroke::parse("ctrl-c").unwrap()));
    assert!(!is_terminal_paste(&Keystroke::parse("ctrl-alt-v").unwrap()));
}

#[test]
fn terminal_text_limit_applies_to_the_post_replacement_value() {
    assert_eq!(MAX_TERMINAL_TEXT_BYTES, 64 * 1024);
    assert!(terminal_replacement_fits(MAX_TERMINAL_TEXT_BYTES, 4, 4));
    assert!(terminal_replacement_fits(MAX_TERMINAL_TEXT_BYTES, 4, 0));
    assert!(!terminal_replacement_fits(MAX_TERMINAL_TEXT_BYTES, 0, 1));
    assert!(!terminal_replacement_fits(
        MAX_TERMINAL_TEXT_BYTES - 1024,
        0,
        2 * 1024
    ));
}

#[test]
fn space_control_and_function_keys_are_encoded() {
    for spec in ["space", "shift-space"] {
        assert_eq!(
            encode(&Keystroke::parse(spec).unwrap(), false, false).unwrap(),
            b" "
        );
    }
    for number in 1..=24 {
        assert!(encode(
            &Keystroke::parse(&format!("f{number}")).unwrap(),
            false,
            false,
        )
        .is_some());
    }
    for (spec, expected) in [
        ("ctrl-c", 3),
        ("ctrl-v", 22),
        ("ctrl-z", 26),
        ("ctrl-l", 12),
        ("ctrl-space", 0),
    ] {
        assert_eq!(
            encode(&Keystroke::parse(spec).unwrap(), false, false).unwrap(),
            vec![expected]
        );
    }
}
