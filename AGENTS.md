# Working on Kea

Read README.md and docs/architecture.md before changing the design.

## Invariants

- Raw output bytes plus ordered resize/lifecycle events are canonical. Do not replace them with lossy UTF-8 or rendered text.
- Structured command blocks must come from explicit application-owned/shell-provided boundaries. Never infer command completion from prompt regexes, cursor position, idle time or `$`/`>` text.
- `kea-document` is a derived portable layer over `kea-core`; keep GPUI, Zed, PTY, OS and shell-adapter dependencies out of both crates.
- Replay is observation, never execution. Historical engines cannot issue PTY replies, send input, mutate clipboard, open URLs or change windows.
- Keep live and historical terminal emulator state separate; live output and required protocol replies continue during rewind.
- Direct PTY mode and Document mode must refer to the same underlying process/session. Do not fake compatibility by silently spawning a second shell.
- Application-owned shell wrappers are not user command content. Hidden-input suppression must fail open: never drop real command output merely to hide a wrapper echo.
- No raw input recording by default. Document boundary markers intentionally retain submitted command text, and output can contain secrets. Persistence stays explicit, bounded and non-overwriting.
- Keep all retained structures bounded. Surface quota exhaustion, disk failures, truncation and gaps; never claim incomplete history/document content is complete.
- A replay checkpoint needs full parser state, partial escapes/UTF-8, both buffers, modes, margins, tabs, cursor and colors. A grid clone is not a checkpoint.
- Document protocol markers are structural metadata, not authentication. Do not treat imported terminal output as trusted provenance.
- Distinguish compilation, automated tests and interactive validation when reporting platform compatibility.
- Do not copy GPL Zed code into these MIT crates. A Zed integration should use Zed code in Zed and keep Kea's portable layers independently licensed.

Run `cargo fmt --all`, portable tests, portable clippy with `-D warnings`, `cargo test -p kea-app`, and the Linux desktop smoke test when native dependencies are available. Commit `Cargo.lock` and use `--locked` for reproducible CI.

Current known quality gaps include editor selections/undo/IME, richer shell metadata such as cwd, full terminal keyboard/mouse/image protocols, accessibility, efficient long-session checkpoints/indexing and packaged Windows/macOS validation. Implement real capabilities with regression tests rather than placeholders that merely look operational.
