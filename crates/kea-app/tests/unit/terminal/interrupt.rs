use super::*;

#[test]
fn confirmation_forwards_exact_encoded_bytes_once() {
    let mut guard = InterruptGuard::default();
    let bytes = b"\x1b[99;5u".to_vec();
    guard.arm(4, bytes.clone());
    assert_eq!(guard.confirm(4, false), None);
    assert_eq!(guard.confirm(4, true), Some(bytes));
    assert_eq!(guard.confirm(4, true), None);
}

#[test]
fn repeats_cannot_queue_or_replace_an_interrupt() {
    let mut guard = InterruptGuard::default();
    guard.arm(4, vec![3]);
    guard.arm(4, vec![3, 3]);
    assert_eq!(guard.confirm(4, true), Some(vec![3]));
}

#[test]
fn context_change_or_lost_target_cancels() {
    let mut guard = InterruptGuard::default();
    guard.arm(1, vec![3]);
    assert_eq!(guard.confirm(2, true), None);
    guard.arm(2, vec![3]);
    guard.validate(2, false);
    assert!(!guard.is_pending());
    assert_eq!(guard.confirm(2, true), None);
    guard.arm(2, vec![3]);
    guard.validate(2, true); // Streaming output with no context change.
    assert!(guard.is_pending());
    guard.cancel();
    assert!(!guard.is_pending());
}

#[test]
fn held_enter_and_modifier_release_never_count_as_fresh() {
    let mut latch = KeyLatch::default();
    assert!(latch.press("enter"));
    assert!(!latch.press("return"));
    latch.release("control");
    assert!(!latch.press("enter"));
    latch.release("return");
    assert!(latch.press("enter"));
}
