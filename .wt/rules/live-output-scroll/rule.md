# Live output scrolling

AGENTS.md: live output may not unconditionally scroll or erase a reading selection. Review scroll_to_bottom and follow_output_tail calls in the block output view. New block editor creation and explicit latest-page navigation may be fine; the update path checks focus/selection before refresh. Terminal input's explicit return to bottom in actions.rs is out of scope. Structural matches cannot evaluate the guard.
