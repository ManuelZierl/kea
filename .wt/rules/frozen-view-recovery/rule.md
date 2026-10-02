# Frozen-view recovery

GPUI dispatches remaining keybindings after the terminal key-down handler. A blanket propagation stop for frozen views consumes F9 before Go Live can run. Return before encoding child input, but preserve chrome action dispatch. The busy-terminal graphical smoke tests copying an immutable frame and returning to live input.
