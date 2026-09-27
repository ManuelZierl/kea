# X11 key-release retention

## Intended constraint

Preserve physical X11 key releases in order, using verified XKB detectable autorepeat rather than batch-local timing guesses.

## What is detected

This rule reports the Rust AST forms `last_key_release = Some($EVENT)` and `key_press.time.saturating_sub(key_release.time)` in `vendor/gpui/src/platform/linux/x11/client.rs`.

## Evidence

Consecutive releases overwrote the pending slot, swallowing the next Submit. Separately, a repeat release/press pair crossing polling batches exposed a false physical release and confirmed a held Submit. Both were reproduced graphically. The fix verifies detectable autorepeat at startup and forwards every key event in order without a pending-release slot or timing filter.

## Known limitations

This is a precise source-form guard, not a proof of all key-release ordering behavior or XKB setup correctness. It does not inspect other paths or differently written equivalent implementations. Graphical held-key and batched-release assertions remain necessary.
