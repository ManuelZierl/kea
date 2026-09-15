# Working on Kea

Read README.md, docs/architecture.md, docs/editor-integration.md and docs/shared-session.md before changing the design.

## Product and host invariants

- Treat Kea as a minimal editor with executable input and read-only output. Reuse platform services and established editor/terminal components instead of implementing another buffer, cursor, selection or undo engine.
- The standalone host uses GPUI Component; a Zed host should use Zed's editor/actions. Ordinary text arrives through the component's platform text-input handler, not manual keycode-to-character conversion.
- Copy means focused selection. Copy block/document/screen is explicit. Never silently copy the entire document when selection-copy has nothing selected.
- Undo affects the current draft only, never execution or recorded output. Successful submission creates a fresh draft. Edit as new preserves the prior command and a nonempty current draft.
- Execute requires focused command input and no active composition. It must not run from a search field, output block or Direct PTY shortcut.
- Scope keybindings by focused component. Remapping/unbinding must mask the inherited binding. Do not globally consume editing/navigation/TUI keys or steal platform input-method events.
- Read-only output must remain selectable, searchable and copyable. Keep editor entities stable across renders. Live output cannot reset a reading selection or unconditionally scroll to bottom.
- Follow system appearance by default; use component/platform defaults unless the user explicitly overrides them. Do not promise unsupported OS autocomplete, dictation, accessibility or IME features.

## Data and execution invariants

- Raw output bytes plus ordered resize/lifecycle events are canonical. Do not replace them with lossy UTF-8 or rendered text.
- Command blocks require explicit application-owned/shell-provided boundaries, never prompt regexes, cursor position, idle time or `$`/`>` text.
- `kea-document` depends only on `kea-core`; keep GPUI, editor, Zed, PTY, OS and shell-adapter dependencies out of both crates.
- Replay is observation, never execution. Historical engines cannot issue PTY replies, send input, mutate clipboard, open URLs or change windows.
- Live and historical emulator state stay separate; live output/protocol replies continue during rewind.
- The live terminal and composer coexist over the same process. Blocks are optional. Never silently spawn a second shell.
- Mask all Kea accelerators with live-terminal focus; keep clickable controls for host actions.
- Unmanaged terminal input invalidates shell readiness. Application text must never receive eval wrappers.
- Completion handoff uses the running application's Tab handling, without Enter; reject multiline/control-character drafts.
- Shell wrappers are implementation input. Hidden-echo suppression fails open; do not drop real output to hide cosmetic wrapper echoes.
- No raw keystroke recording by default. Submitted command markers and output can contain secrets. Persistence is explicit, bounded and non-overwriting.
- Bound retained data and UI entities. Surface quota exhaustion, disk failures, truncation and gaps; do not call incomplete history complete.
- Replay checkpoints need full parser state, partial escapes/UTF-8, both buffers, modes, margins, tabs, cursor and colors. A grid clone is not a checkpoint.
- Document markers are metadata, not authentication. Imported terminal output is not trusted provenance.
- Distinguish compilation, automated/component tests, graphical smoke tests and real platform validation.
- Do not copy GPL Zed code into MIT Kea crates.

Run cargo fmt, portable tests/Clippy with -D warnings, cargo test -p kea-app, cargo build -p kea-app and the Linux desktop smoke test where dependencies exist. Commit Cargo.lock and use --locked. Preserve real assertions rather than disabling tests to obtain a green build.

Outstanding work includes richer shell metadata, complete Direct PTY keyboard/mouse/image/input protocols, actual OS input-service/accessibility acceptance, long-session indexing/checkpoints and platform packages. Do not describe those as already provided by the editor dependency.
