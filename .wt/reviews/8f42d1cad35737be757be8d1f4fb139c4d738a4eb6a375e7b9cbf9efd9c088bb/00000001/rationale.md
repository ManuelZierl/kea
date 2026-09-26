model.rs:232-249 validates the input and calls can_retain_block before this second blocks.push. On exhaustion it sets saturated and continues without gating terminal input; no block is appended.
