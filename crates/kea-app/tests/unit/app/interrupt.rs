use gpui::Keystroke;
use kea_app::terminal::input;

#[test]
fn interrupt_classification_covers_chords_with_the_same_terminal_encoding() {
    assert!(super::is_ctrl_c(&Keystroke::parse("ctrl-shift-c").unwrap()));
    assert!(!super::is_ctrl_c(&Keystroke::parse("ctrl-alt-c").unwrap()));
    let ctrl_shift_c = Keystroke::parse("ctrl-shift-c").unwrap();
    let bytes = input::encode(&ctrl_shift_c, false, false).unwrap();
    assert_eq!(bytes, [3]);
}

#[test]
fn shift_modified_ctrl_c_is_not_consumed_as_selection_copy() {
    let local = kea_app::terminal::selection::local_key("c", true, true, false, false, false, true);
    assert_eq!(local, kea_app::terminal::selection::LocalKey::Forward);
    let copy = kea_app::terminal::selection::local_key("c", false, true, false, false, false, true);
    assert_eq!(copy, kea_app::terminal::selection::LocalKey::Copy);
}
