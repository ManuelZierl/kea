use super::can_write_clipboard_store;

#[test]
fn clipboard_host_policy_is_visible_live_only() {
    assert!(can_write_clipboard_store(true, false, false));
    assert!(!can_write_clipboard_store(false, false, false));
    assert!(!can_write_clipboard_store(true, true, false));
    assert!(!can_write_clipboard_store(true, false, true));
}
