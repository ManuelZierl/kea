# Frozen Shift selection anchor

Freezing an identical grid must preserve a prior local selection/caret so ordinary
Shift-click and Shift-drag extend from its anchor. The rule catches the original
unconditional clear sequence, not guarded clearing for a fresh Ctrl+Shift gesture.
Engine, session and pointer-routing tests verify the corresponding state behavior.
