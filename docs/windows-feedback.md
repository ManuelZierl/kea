# Desktop input, completion, and following output

## Direct mode belongs to the child

While a live Direct terminal is focused, Kea does not register its editor,
clipboard, replay, quit, or mode-switch shortcuts in the focus chain. Ctrl+C,
Ctrl+V, Tab, Shift+Enter and the F keys go to the terminal application. Use the
visible Document/Direct button, Paste button, timeline and window controls for
host actions. Observation shortcuts remain available for ended/replayed sessions.
The OS can still reserve global shortcuts; legacy terminal encoding cannot
represent every possible modifier combination uniquely. This change does not
claim complete Kitty keyboard or mouse protocol support.

Windows committed text (including plain Space) arrives through the platform
input handler when a key-down cannot be encoded. Kea must not consume the
unhandled key-down, or WM_CHAR never arrives. IME preedit is not sent to the child
until text is committed. There is no raw-keystroke recording.

Release Windows executables use the GUI subsystem so opening Kea does not open
an additional console. Debug builds retain console diagnostics. CI verifies the
subsystem in the actual distributed PE. This does not sign the executable.

## Directory reporting

Recognized interactive shells report their actual directory at startup and
around document commands through a bounded `OSC 777;kea;cwd;<hex UTF-8> BEL`
marker. Kea does not infer it from prompts or substitute its own launch directory.
The directory is reconstructed from recordings too. Direct mode labels the value
as **last reported**: a program or a nested SSH shell can change context without
reporting it. Noninteractive `-c`/`-Command` invocations do not get an injected
startup probe. Shell metadata is not authenticated provenance.

## Completion

Tab or Ctrl+Space requests read-only **local** path, inherited-PATH executable,
and retained-command-history suggestions. A single match inserts immediately;
multiple matches use a picker (Up/Down, Tab/Enter, Escape, or click). At line-start
whitespace or with a selection, Tab retains the editor's indentation behavior.
Completion acceptance uses the editor's range replacement and undo history. It
never executes a command or writes the draft to the PTY. Results are discarded
when the draft, cursor, focus, mode, or reported directory no longer matches.

Directory commands restrict suggestions to directories; literal spaces, quotes
and Unicode paths are quoted for the selected shell. Ambiguous shell expansions
are not evaluated. Scans and results are bounded and run off the UI thread.

These are not native programmable Bash/Zsh/PowerShell completions. Session-local
aliases, functions, changed PATH, command flags and remote files are not guessed.
The provider is isolated in `kea-app::completion` for subsequent shell-specific
integration. These distinctions must remain visible rather than implying full
shell completion support.

`complete`, `accept_completion`, `next_completion`, `previous_completion`, and
`dismiss_completion` are configurable semantic shortcuts. For ordinary Tab
indentation everywhere, set `complete = ctrl-space` in `keybindings.conf`.

## Initial output position

New command blocks reveal their tail after layout. Streaming follows while the
reader has not focused, selected, or manually scrolled a block. A reader's
snapshot is preserved; Follow latest output explicitly resumes and clears its
selection. The surrounding document follows new output only when already at the
bottom (or on explicit execution/Latest). The Direct PTY is sized from its actual
rendered rectangle, so its bottom row is not clipped by guessed toolbar heights.
