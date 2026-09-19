# Working on Kea

Contributor setup and checks live in [CONTRIBUTING.md](CONTRIBUTING.md).
Target `develop` for ordinary pull requests; `main` holds release-ready work.
Release tags must match the workspace version and point to a commit on `main`.
See [docs/releasing.md](docs/releasing.md) for publication and Pages setup.

Read README.md, docs/architecture.md, docs/unified-session.md and docs/editor-integration.md before changing the design.

## Product and host invariants

- Kea has one terminal session and a persistent editor visible together. Focus changes input ownership; there is no Document/Direct execution mode and no hidden submission-target state.
- Blocks are optional/fail-open observers. **Never make command execution depend on creating, retaining or completing a block.** Missing structure must degrade to untracked execution, not queued/stuck execution.
- `Run in shell` and `Send to app` are explicit separate actions. Run requires an explicit integrated-shell prompt-ready marker. Send never adds shell wrappers. Never infer readiness from prompt text, cursor position or idle time.
- Editor-native Enter/newline is the default, but submission/newline shortcuts are semantic and configurable. Support terminal/chat policy (`run_shell = enter`, `newline = shift-enter`) without changing execution architecture.
- While the live child owns the keyboard, `focus_editor` is the sole configurable Kea accelerator. Mask other Kea accelerators, including user overrides; visible chrome remains available. A visible Kea selection/caret owns only the local read-only commands in docs/terminal-text-selection.md, with unrelated input clearing local state and following normal terminal routing.
- Preserve every key distinction exposed by the OS + terminal protocol. Do not claim physical-key distinctions that classic terminal encoding cannot represent; extended keyboard protocol support is a compatibility concern.
- Reuse platform services and established editor/terminal components instead of implementing another buffer, cursor, selection or undo engine.
- Ordinary editor text/composition arrives through the component/platform input handler, not manual keycode-to-character conversion. Terminal committed IME text also uses the platform text-input bridge.
- Copy means focused selection. Copy block/view/screen is explicit. Never silently replace selection-copy with whole-document copy.
- Undo affects only the current draft, never execution or recorded output. Successful Run/Send creates a fresh draft. Edit as new preserves the prior command and never executes automatically.
- Read-only output must remain selectable, searchable and copyable. Keep editor entities stable across renders. Live output cannot reset a reading selection or unconditionally scroll to bottom.
- Follow system appearance by default; use component/platform defaults unless the user explicitly overrides them.

## Cwd and completion invariants

- Integrated local-shell prompt hooks explicitly report cwd every time the shell returns to its prompt, including after a command typed directly in the terminal. Never scrape prompt text.
- Label cwd **Current shell directory** only while the integrated local shell has explicitly reported an idle prompt. While a TUI/SSH/REPL owns stdin, label it **last reported** rather than pretending to know the foreground application's cwd.
- Preserve the user's shell profile/prompt configuration. In particular, default Windows PowerShell must not use `-NoProfile`.
- Terminal Tab remains native application completion.
- Editor completion may use retained history, the integrated shell's reported effective PATH and its reported cwd. Completion never evaluates the draft, blocks the UI thread indefinitely or invents remote/application-specific context.
- Complex/programmable/remote completion belongs to native terminal Tab unless a future explicit provider supplies it safely.

## Data and execution invariants

- Raw output bytes plus ordered resize/lifecycle events are canonical. Do not replace them with lossy UTF-8 or rendered text.
- Command blocks require explicit application-owned/shell-provided boundaries, never prompt regexes, cursor position, idle time or `$`/`>` text.
- `kea-document` depends only on `kea-core`; keep GPUI, editor, Zed, PTY, OS and shell-adapter dependencies out of both crates.
- Replay is observation, never execution. Historical engines cannot issue PTY replies, send input, mutate clipboard, open URLs or change windows.
- Live and historical emulator state stay separate; live output/protocol replies continue during rewind.
- Shell wrappers are implementation input. Hidden-echo suppression fails open; do not drop real output to hide cosmetic wrapper echoes.
- No raw keystroke recording by default. Submitted command markers and output can contain secrets. Persistence is explicit, bounded and non-overwriting.
- Bound retained data and UI entities. Surface quota exhaustion, disk failures, truncation and gaps; do not call incomplete history complete.
- Replay checkpoints need full parser state, partial escapes/UTF-8, both buffers, modes, margins, tabs, cursor and colors. A grid clone is not a checkpoint.
- Metadata markers are interoperability, not authentication. Imported terminal output is not trusted provenance.
- Distinguish compilation, automated/component tests, graphical smoke tests and real platform validation.
- Do not copy GPL Zed code into MIT Kea crates.

Run cargo fmt, portable tests/Clippy with -D warnings, cargo test -p kea-app --lib, cargo build -p kea-app and the Linux desktop smoke test where dependencies exist. Commit Cargo.lock and use --locked. Preserve real assertions rather than disabling tests to obtain a green build.

Outstanding work includes cross-platform/application acceptance and edge cases for terminal mouse, selection and scrollback; higher extended-keyboard protocol levels; image protocols; actual OS input-service/accessibility acceptance; long-session indexing/checkpoints; and platform packages. Existing terminal support is implemented by Kea's adapters, not automatically provided by the editor dependency.
