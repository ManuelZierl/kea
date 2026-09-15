<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/kea-logo-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="assets/kea-logo.svg">
    <img alt="Kea" src="assets/kea-logo.svg" width="220">
  </picture>
</p>

<h1 align="center">Kea</h1>

<p align="center"><strong>The terminal, rethought as a persistent document.</strong></p>

Kea is a minimal editor with executable input and persistent, read-only output. The live terminal and bottom text editor are available together. Write multiline input in the editor or click the terminal to interact directly. Command blocks are an optional view, not a requirement for using Kea.

Everyday editing belongs to a reusable, platform-integrated editor—not a terminal pretending to be a text field. Kea adds the execution and document model around it. Terminal replay is a consequence of retained output history, not the main purpose.

## Working in Kea

The command editor supports normal text selection, clipboard operations, undo/redo, mouse editing, multiline navigation, integrated text composition and optional Bash syntax highlighting. Enter adds a newline; Ctrl+Enter executes the focused draft. Paste never executes a command by itself.

Each submitted command becomes an immutable command record with retained output, lifecycle, exit status and timing. The same shell stays alive, so `cd`, variables and functions can persist between blocks. A successful submission starts a fresh draft with its own undo history: undo never changes an old execution or reverses a shell side effect.

Output is read-only **and selectable**. Copy copies the selection, while Copy block and Copy document are separate actions. If a running block changes while you are selecting/reading it, the visible snapshot stays still and offers an explicit refresh; the process and recording continue. Blocks can be collapsed, searched, paged through, and copied into a new draft with Edit as new. That action does not execute anything or overwrite a nonempty draft.

```text
Session document

┌ command #1 · exit 0 · 1.42s ──────────────────────┐
│ cargo test                                         │
├ read-only, selectable output ──────────────────────┤
│ running 42 tests                                   │
│ ...                                                │
│ test result: ok                                    │
└────────────────────────────────────────────────────┘

┌ command editor ────────────────────────────────────┐
│ docker compose run --rm backend \\                  │
│     python manage.py migrate                       │
│                                                    │
│ Enter = newline        Ctrl+Enter = execute         │
└────────────────────────────────────────────────────┘
```

For OpenCode, Vim, `less`, SSH or a REPL, click the terminal: its keystrokes belong to the child application. The bottom editor remains available for composing text. Use Show blocks to inspect tracked commands alongside the terminal. These are views of one process, never separate shells. See [shared session and input ownership](docs/shared-session.md).

## Shortcuts and system defaults

| Action | Linux / Windows | macOS |
| --- | --- | --- |
| Copy / cut / paste selection | Ctrl+C / X / V | Cmd+C / X / V |
| Undo / redo draft edits | Ctrl+Z / Ctrl+Shift+Z (also Ctrl+Y) | Cmd+Z / Cmd+Shift+Z |
| Select all / find in focused text | Ctrl+A / Ctrl+F | Cmd+A / Cmd+F |
| Focus command editor | Ctrl+L | Cmd+L |
| Execute focused command draft | Ctrl+Enter | Ctrl+Enter |
| Interrupt child | Ctrl+Shift+C | Ctrl+C |
| Change composer submission target | Ctrl+Shift+Space | Ctrl+Shift+Space |
| Complete single-line draft (handoff, no Enter) | Ctrl+Space | Ctrl+Space |
| Show / hide optional blocks | Ctrl+Shift+B | Ctrl+Shift+B |
| Copy whole document / terminal screen | F10 | F10 |
| Previous / next terminal-history event | F6 / F7 | F6 / F7 |
| Back / forward five seconds | Shift+F6 / Shift+F7 | Shift+F6 / Shift+F7 |
| Play / pause history; return to live | F8; F9 | F8; F9 |
| Quit | Ctrl+Shift+Q | Cmd+Q |

The table applies to editor/history focus. With a live terminal focused, Kea does not reserve these shortcuts: Ctrl+C/V/Z/L, Tab and F6–F10 go to the terminal application. Use the clickable toolbar to focus the editor, paste to the terminal, copy all, or inspect history. OS-reserved shortcuts and unsupported extended terminal protocols remain platform/compatibility limitations.

Appearance follows system light/dark by default. Clipboard, composition and text layout are integrated through GPUI Component and GPUI's platform interfaces. This does **not** guarantee every OS autocomplete, dictation, accessibility or IME service on every platform; see [platform integration and validation](docs/editor-integration.md).

### Configuration

Kea reads `keybindings.conf` and `settings.conf` from:

- Linux: `$XDG_CONFIG_HOME/kea/`, or `~/.config/kea/`
- macOS: `~/Library/Application Support/Kea/`
- Windows: `%APPDATA%\\Kea\\`

`KEA_KEYBINDINGS` and `KEA_SETTINGS` override the respective file locations. Settings are loaded at startup. Invalid files produce a warning and fall back to defaults.

```text
# keybindings.conf: optional overrides
copy = ctrl-shift-c
interrupt = ctrl-c
execute = alt-enter
copy_document = f10
```

Multiple shortcuts can be comma-separated; `none` unbinds an action, including its usual editor binding. Duplicate semantic assignments are rejected. Other configurable actions include `cut`, `paste`, `undo`, `redo`, `select_all`, `find`, `focus_editor`, `complete`, `toggle_blocks`, `toggle_direct` (also `toggle_input_target`), `previous_event`, `next_event`, `back_5s`, `forward_5s`, `play_pause`, `go_live`, and `quit`. Ordinary editor navigation retains component/platform defaults.

```text
# settings.conf: defaults shown
theme = system
font_family = system
font_size = system
syntax_highlighting = true
line_numbers = false
soft_wrap = true
output_wrap = true
show_blocks = false
```

Font `system` means no Kea override of the component defaults. Explicit font sizes from 9 to 40 are accepted. Only Bash highlighting is currently configured; PowerShell uses the multiline text editor without a grammar. No automatic smart-quote or spelling substitutions are added to command text.

## Structured commands and compatibility

Kea does not guess command boundaries from prompts, regexes or idle time. Application-owned shell wrappers emit explicit OSC markers with command ID/text and completion status. The marker bytes enter the ordinary recording; `kea-document` derives blocks from that stream. Reopening a `.kea` recording reconstructs those blocks without re-executing commands.

Document adapters currently support `sh`, `bash`, `dash`, `zsh`, `ksh`, `mksh`, `pwsh` and Windows PowerShell. Other programs start in Direct mode. Windows defaults to PowerShell for Document mode rather than `cmd.exe`. The status bar shows the last directory explicitly reported by the shell at startup and around managed command submissions. Direct terminal commands and remote shells can make that report stale; it is labeled last reported, not guessed from prompts.

Direct mode retains the existing terminal renderer/keyboard adapter. Full mouse reporting, keyboard protocol negotiation, terminal IME, images and accessibility are separate compatibility work, not capabilities automatically provided by an editor dependency.

## Terminal history

Output bytes, ordered resizes and lifecycle events are retained for inspection. The live process continues while a separate historical terminal state is viewed. Rewind never sends input, opens links, changes the clipboard or executes commands. Explicit Copy is a user action, not a replay side effect.

Transient errors and overwritten TUI states can be recovered with the timeline. Backward seeking currently replays from the beginning; there are no full-state checkpoints, compressed long-session storage or historical full-text index. Multiple updates in one PTY read are not independently timestamped frames.

## Run on Ubuntu

Install a current stable Rust toolchain through [rustup](https://rustup.rs/), then native build dependencies. The desktop needs a graphical session and a working Vulkan driver.

```bash
sudo apt-get update
sudo apt-get install -y build-essential pkg-config cmake clang libclang-dev \\
  libasound2-dev libxkbcommon-x11-dev libwayland-dev libssl-dev \\
  libfontconfig-dev libfreetype-dev libx11-xcb-dev libxcb-shape0-dev \\
  libxcb-xfixes0-dev libxcb-randr0-dev libvulkan-dev

git clone git@github.com:ManuelZierl/kea.git
cd kea
cargo run --locked --release
```

```bash
# Direct terminal application
cargo run --locked --release -- --direct -- opencode
# Transient-error history demonstration
cargo run --locked --release -- --demo
# Explicitly save a new recording; existing files are never overwritten
cargo run --locked --release -- --record session.kea -- bash
# Reopen the recording without starting a process
cargo run --locked --release -- --replay session.kea
```

The binary is `target/release/kea`. `cargo install --locked --path crates/kea-app` installs it locally. There is no crates.io release or platform installer yet.

## Architecture

| Crate | Responsibility |
| --- | --- |
| `kea-core` | Standard-library-only canonical events, recording format, validation and replay trait |
| `kea-document` | UI-independent command/output model and streaming boundary parser |
| `kea-alacritty` | Alacritty terminal projection and silent replay |
| `kea-pty` | Unix PTY / Windows ConPTY transport |
| `kea-session` | Live/history state, observation tap, hidden application input, playback and optional disk writer |
| `kea-app` | GPUI Component editor surfaces, document presentation, shell adapters, focused actions and host settings |

The portable core/document crates have no GPUI, editor, OS UI or shell dependency. A Zed host should use Zed's editor/actions/PTY/renderer, not transplant this standalone UI. See [architecture](docs/architecture.md), [editor integration](docs/editor-integration.md), [document protocol](docs/document-protocol.md), [recording format](docs/recording-format.md), [roadmap](docs/roadmap.md), and [development invariants](AGENTS.md).

## Limits and privacy

History stays in memory unless `--record` is supplied. **Output and submitted command text can contain passwords, tokens and private documents.** Recordings are unencrypted and not automatically redacted. Unix files are created with mode `0600`; Windows files inherit directory permissions. Keep recordings private and out of Git. Marker metadata is not authenticated provenance.

Terminal recording is bounded to 32 MiB of accounted event data/overhead or 100,000 events. Document limits are 64 KiB per command, 4 MiB output per block, 64 MiB aggregate and 10,000 blocks. Reaching a quota is surfaced; the live PTY can continue. Output projection simplifies terminal control sequences; Direct mode is the fidelity path for complex TUIs. Only 24 blocks at a time have editor widgets; page navigation and filtering search retained blocks, not every historical screen.

## Development and validation

```bash
cargo fmt --all -- --check
cargo test --locked -p kea-core -p kea-document -p kea-alacritty -p kea-pty -p kea-session
cargo clippy --locked -p kea-core -p kea-document -p kea-alacritty -p kea-pty -p kea-session --all-targets -- -D warnings
cargo test --locked -p kea-app
cargo build --locked -p kea-app
```

CI runs portable tests on Linux, macOS and Windows and builds/tests the desktop on Linux. The graphical smoke test covers editor selection, clipboard, cut, undo/redo, Unicode multiline draft submission, read-only output and terminal replay. Component composition tests exercise the input-handler API, not a real OS IME. Real Wayland/IBus/Fcitx, macOS/Windows desktop, accessibility and packaging acceptance remain separately tracked.

## License

MIT. Dependencies retain their licenses. Kea is independent, not an official Zed feature or extension.
