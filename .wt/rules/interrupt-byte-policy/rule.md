# Interrupt byte policy

The terminal encoder collapses some modifier distinctions. Ctrl+Shift+C can therefore produce the same `0x03` byte as Ctrl+C. Interrupt confirmation must classify the exact encoded bytes rather than infer signal identity from a hand-maintained modifier predicate. Keep selection Copy and active IME routing ahead of the terminal interrupt gate.
