# Roadmap and acceptance checks

## Product direction

Kea is not primarily a replay terminal. The target is a document-native terminal: editable command input, persistent read-only output, structured command/output history, and full PTY/TUI compatibility when an application needs a terminal screen.

Replay and time travel are consequences of retaining terminal state instead of treating the current mutable screen as the only truth.

### Interaction model

- Command input behaves like an editor, not a shell line discipline.
- Enter inserts a newline; an explicit configurable shortcut executes the buffer.
- Output is read-only.
- Interactive programs can switch to explicit Direct PTY input when they need individual key events.
- All user-facing shortcuts are semantic/configurable and use host-OS defaults where practical.
- Copy defaults to `Ctrl+C` on Linux/Windows and `Cmd+C` on macOS; terminal interrupt is a separate action.
- Shortcut resolution belongs to the application/UI layer; `kea-core` does not encode platform keybindings.

## Implemented scope

The current prototype includes:

- GPUI window, PTY shell/program launch and Alacritty screen projection;
- a local multiline Document input mode where Enter inserts a newline and an explicit action executes the buffer;
- a Direct PTY compatibility mode for TUIs, REPLs and programs that need immediate key forwarding;
- semantic, configurable keybindings with OS-native defaults and a separate interrupt action;
- bounded output history, resize/exit events, separate read-only historical projection, event/time seeking, playback and return to live;
- optional persistence/reopening, whole-screen copy, paste, tests and a synthetic no-process history demo.

The local editor is not yet a full structured command document: command/output blocks, selection, undo/redo, shell metadata and IDE-grade text editing remain future work.

## Manual Ubuntu acceptance

1. Run `cargo run --release`. Type a multiline command without sending it: Enter must add lines locally; Ctrl+Enter must submit the buffer.
2. Verify native clipboard policy: Ctrl+C copies the visible terminal output, Ctrl+V pastes into Document input, and Ctrl+Shift+C sends interrupt by default.
3. Add `~/.config/kea/keybindings.conf` that swaps copy/interrupt; restart and verify the override is applied.
4. Toggle Direct PTY mode with Ctrl+Shift+Space. Launch `cargo run --release -- --direct -- opencode` and check typing, Ctrl+Enter/Shift+Enter behavior, resize, colors and alternate screen.
5. Run `cargo run --release -- --demo`. Recover the overwritten error using the timeline or F6/F7; resume with F8.
6. Launch a shell, run `printf 'before\r'; sleep 1; printf 'after\n'`, rewind and return to LIVE. Input must be blocked in history.
7. Record a short non-sensitive session with `--record example.kea`; reopen with `--replay example.kea` and compare old screens.
8. Resize in live/history mode. Old dimensions must be preserved, and the current size must reach the child after return to LIVE.

Compilation and unit tests do not substitute for these application-specific checks. Include OS, terminal application/version, keyboard layout and display backend in compatibility reports.

## Next capabilities

**Document UX:** turn the local editor/output split into first-class structured command blocks. Add selection, copy of selections, undo/redo, richer cursor/mouse editing, IME and accessibility, command history/edit-and-rerun, collapsible output and search. Add optional shell integration for cwd, command boundaries, duration and exit status. Do not infer command boundaries from arbitrary prompt regexes.

**Input and compatibility:** full keyboard protocol negotiation, AltGr/non-US layouts, mouse/focus events, measured font layout/shaping, clipping and ordinary scrollback. Explicitly validate OpenCode, shells, Vim, less, SSH and tmux. Add Windows/macOS desktop builds/packages. Keep Direct PTY as an explicit compatibility path rather than leaking TUI conventions into Document mode.

**Long sessions:** async cancellable seek, full-state checkpoints with equivalence/property tests, chunked indexed/compressed storage, retention policy, historical text indexing including within-chunk transient text, latency/CPU/memory/load benchmarks.

**History as document data:** searchable historical states, deduplication with timestamps, bookmarks/diff, links from structured command blocks into terminal history and precise transient-output inspection. Replay remains a consequence of persistent terminal state, not the product hierarchy.

**Zed proposal:** once the interaction model and overhead are measured, propose the smallest reusable pieces. Reuse Zed's existing PTY, editor conventions and renderer; keep `kea-core` free of GPUI/keybinding/shell policy. Start with an isolated integration demonstrating document input plus read-only historical inspection rather than transplanting the standalone app wholesale.
