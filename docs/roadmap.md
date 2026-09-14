# Roadmap and acceptance checks

## Product direction

Kea is not primarily a replay terminal. The target is a document-native terminal: editable command input, persistent read-only output, structured command/output history, and full PTY/TUI compatibility when an application needs a terminal screen.

Replay and time travel are consequences of retaining terminal state instead of treating the current mutable screen as the only truth.

### Target interaction model

- Command input behaves like an editor, not a shell line discipline.
- Enter inserts a newline; an explicit configurable shortcut executes the command.
- Executed input becomes a persistent command block with its output.
- Output is read-only and remains inspectable after later commands or TUI redraws.
- Interactive programs continue to run through a PTY and can still receive raw key input when appropriate.
- All user-facing shortcuts are configurable.
- Default shortcuts follow host-OS conventions where practical rather than inheriting terminal conventions blindly. In particular, copy should default to the OS-native copy shortcut (`Ctrl+C` on Linux/Windows, `Cmd+C` on macOS). Terminal interrupt must remain available through a separate configurable action/binding. Users may override either behavior.
- Shortcut resolution belongs to the application/UI layer; `kea-core` must not encode platform keybindings.

## Implemented scope

GPUI window, PTY shell/program launch, Alacritty screen, bounded output history, resize/exit events, separate read-only replay, event/time seeking, playback, return to live, optional persistence/reopening, whole-screen copy, paste, tests and a synthetic no-process demo.

This is the compatibility/history foundation. The document-style command editor and structured command blocks are not implemented yet.

## Manual Ubuntu acceptance

1. Run `cargo run --release -- --demo`. Recover the overwritten error around one second using the timeline or F6/F7; resume with F8.
2. Launch `cargo run --release -- -- bash`, run `printf 'before\r'; sleep 1; printf 'after\n'`, rewind and return to LIVE. Input must be blocked in history.
3. Launch `cargo run --release -- -- opencode`. Check typing, Ctrl+C, Shift+Enter, resize, colors and alternate screen. Rewind while output arrives and verify live progress continues.
4. Record a short non-sensitive session with `--record example.kea`; reopen with `--replay example.kea` and compare old screens.
5. Resize in live/history mode. Old dimensions must be preserved, and the current size must reach the child after return to LIVE.

Compilation and unit tests do not substitute for these application-specific checks. Include OS, terminal application/version, keyboard layout and display backend in compatibility reports.

## Next capabilities

**Document UX:** make command input a real multiline editor; add configurable keybindings with OS-native defaults; introduce persistent command/output blocks; add optional shell metadata for cwd, exit status and command boundaries; make prior output naturally searchable, collapsible and copyable. Do not infer command boundaries from arbitrary prompt regexes.

**Desktop correctness:** full keyboard protocol negotiation, IME/AltGr/non-US layouts, mouse/focus events, selection, accessibility, measured font layout/shaping, clipping and ordinary scrollback behavior for interactive terminal blocks. Explicitly validate OpenCode, shells, Vim, less, SSH and tmux. Add Windows/macOS desktop builds/packages.

**History as a consequence:** searchable historical states, transient-text indexing, bookmarks/diff, efficient snapshots/checkpoints, async cancellable seek, chunked indexed/compressed storage, retention policy, and latency/CPU/memory/load benchmarks.

**Zed proposal:** measured overhead, privacy policy, fixtures and a small integration using Zed's existing PTY/renderer. The long-term value proposition should be the document-native terminal model, with terminal history/replay as one enabling capability rather than the headline feature. No deep Zed dependency in the core; no claim that this independent experiment is an accepted Zed feature.
