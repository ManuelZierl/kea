# Working on Kea

Read README.md and docs/architecture.md before changing the design.

## Invariants

- Raw output bytes plus ordered resize/lifecycle events are canonical. Do not replace them with lossy UTF-8 or rendered text.
- Replay is observation, never execution. Historical engines cannot issue PTY replies, send input, mutate clipboard, open URLs or change windows.
- Keep live and historical emulator state separate; live output and supported protocol replies continue during rewind.
- No raw input recording by default. Output itself can contain secrets. Persistence stays explicit, bounded and non-overwriting.
- No GPUI, Zed, terminal, shell or OS dependencies in kea-core. Keep adapter data out of the portable format. Do not copy GPL Zed code into these MIT crates.
- Use bounded queues/work. Surface quota exhaustion, disk failures and gaps; never claim incomplete history is complete.
- A replay checkpoint needs full parser state, partial escapes/UTF-8, both buffers, modes, margins, tabs, cursor and colors. A grid clone is not a checkpoint.
- Distinguish compilation, automated tests and interactive validation when reporting platform compatibility.

Run cargo fmt --all, engine tests, engine clippy with -D warnings, and cargo test -p kea-app when native dependencies are installed. Use cargo run -- --demo for UI inspection. Add regression tests for state/protocol bugs. Commit Cargo.lock and use --locked for reproducible builds.

The bootstrap deliberately lacks a multiline input editor, full keyboard protocol, IME, mouse reporting, selection, checkpoints and long-session storage. Implement real capabilities with tests, not placeholders that look operational.
