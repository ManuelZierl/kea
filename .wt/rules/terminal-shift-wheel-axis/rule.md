# Local Shift wheel axis

GPUI's X11, Wayland and Windows adapters map Shift+vertical wheel to horizontal
deltas. The terminal reserves Shift+wheel for local vertical scrollback. Simply
delegating `terminal_scroll_lines` to the vertical-only child wheel accumulator
loses these events and makes Shift+wheel silently do nothing.

This focused regression detector rejects the previous delegation shape. It is
not a proof that every alternative normalization is correct; unit tests and the
isolated held-Shift wheel smoke test cover direction, fractional accumulation,
selection extension and zero child input. Ordinary child wheel routing remains
vertical-only and is outside this rule's local-policy target.
