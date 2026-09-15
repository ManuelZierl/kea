# Roadmap and acceptance checks

## Product direction

Kea is a document-native terminal. Replay/time travel is a consequence of retaining terminal state, not the top-level product.

The core interaction model is now implemented end to end:

- editable multiline command input;
- explicit configurable execute action;
- first-class persistent command/output blocks;
- block lifecycle, exit status and timing;
- read-only document output;
- Direct PTY compatibility over the same process;
- explicit shell boundary protocol rather than prompt inference;
- reconstruction of structured blocks from saved terminal recordings;
- configurable semantic shortcuts with OS-native defaults.

## Manual Ubuntu acceptance

1. Launch `cargo run --release -- -- bash --noprofile --norc`. Type `printf hello`, execute with Ctrl+Enter and verify a command block appears with `hello` and exit 0.
2. Enter a multiline shell construct. Plain Enter must add lines locally; only the execute action may submit the buffer.
3. Execute `cd /tmp`, then `pwd` as a second block. It must run in the same shell session and print `/tmp`.
4. Execute a TUI such as `vim` or `opencode`; while its block is running, switch to Direct PTY, interact with it, exit it, then return to Document mode. The block must finish rather than spawning a second shell.
5. Interrupt a long Document command using the configured interrupt action. The shell should recover and the block should finish with its non-zero status when the shell wrapper regains control.
6. Record a short non-sensitive document session with `--record example.kea`; reopen using `--replay example.kea`. Command text/output/status must reconstruct without executing anything.
7. In terminal view/demo, recover the transient overwritten error with the timeline/F6 and return to LIVE. History must remain read-only while the live process continues.
8. Override copy/interrupt/execute in `keybindings.conf` and verify the semantic actions follow the configured bindings.

Compilation and unit tests do not substitute for interactive checks. Include OS, shell/version, keyboard layout and display backend in compatibility reports.

## Next quality work

**Editor quality:** selections, undo/redo, better multiline navigation, IME/AltGr/non-US layouts, syntax highlighting/completion and reusable/rerunnable earlier command blocks.

**Document richness:** explicit shell-provided cwd/environment metadata where trustworthy, block collapse/bookmarks/diff/search, selectable/copyable ranges, stdout/stderr separation when execution is not PTY-merged, and better rendering choices for commands that contain heavy TUI output.

**Terminal correctness:** mouse/focus events, selection, accessibility, measured font shaping/layout, ordinary scrollback behavior, full keyboard protocol negotiation, image protocols and validation against OpenCode, Vim, less, SSH and tmux on real machines.

**Long sessions:** async cancellable historical seek, full-state checkpoints with equivalence tests, indexed/compressed storage, retention configuration, historical full-text indexing and latency/CPU/memory/storage benchmarks.

**Cross-platform desktop:** package and interactively validate Windows/macOS applications, especially PowerShell document execution, shortcut conventions, ConPTY behavior and IME.

**Zed proposal:** demonstrate the document model and measured overhead in the standalone app, then propose a narrow integration that reuses Zed's PTY/renderer/editor/actions rather than transplanting the entire app. No claim that Kea is an accepted Zed feature until maintainers agree.
