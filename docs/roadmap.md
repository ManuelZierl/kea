# Roadmap and acceptance checks

## Implemented scope

GPUI window, PTY shell/program launch, Alacritty screen, bounded output history, resize/exit events, separate read-only replay, event/time seeking, playback, return to live, optional persistence/reopening, whole-screen copy, paste, tests and a synthetic no-process demo.

## Manual Ubuntu acceptance

1. Run `cargo run --release -- --demo`. Recover the overwritten error around one second using the timeline or F6/F7; resume with F8.
2. Launch `cargo run --release -- -- bash`, run `printf 'before\r'; sleep 1; printf 'after\n'`, rewind and return to LIVE. Input must be blocked in history.
3. Launch `cargo run --release -- -- opencode`. Check typing, Ctrl+C, Shift+Enter, resize, colors and alternate screen. Rewind while output arrives and verify live progress continues.
4. Record a short non-sensitive session with `--record example.kea`; reopen with `--replay example.kea` and compare old screens.
5. Resize in live/history mode. Old dimensions must be preserved, and the current size must reach the child after return to LIVE.

Compilation and unit tests do not substitute for these application-specific checks. Include OS, terminal application/version, keyboard layout and display backend in compatibility reports.

## Next capabilities

**Desktop correctness:** full keyboard protocol negotiation, IME/AltGr/non-US layouts, mouse/focus events, selection, copy-when-selected, accessibility, measured font layout/shaping, clipping, ordinary scrollback. Explicitly validate OpenCode, shells, Vim, less, SSH and tmux. Add Windows/macOS desktop builds/packages.

**Long sessions:** async cancellable seek, full-state checkpoints with equivalence/property tests, chunked indexed/compressed storage, retention policy, historical text indexing including within-chunk transient text, latency/CPU/memory/load benchmarks.

**Document UX:** searchable historical states, deduplication with timestamps, bookmarks/diff, then the original editor-like multiline command input and optional shell integration. Do not infer command boundaries from arbitrary prompt regexes.

**Zed proposal:** measured overhead, privacy policy, fixtures and a small read-only timeline integration using Zed's existing PTY/renderer. No deep Zed dependency in the core; no claim that this independent experiment is an accepted Zed feature.
