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
