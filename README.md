# Kea

**The terminal, rethought as a persistent document.**

Kea explores a different terminal model: normal shell work is a sequence of persistent command/output blocks, while a real PTY remains underneath for compatibility with existing terminal software.

In Document mode:

- command input is editable multiline text;
- `Enter` inserts a newline and an explicit action executes the block;
- an executed command becomes a first-class command block;
- output is retained as read-only content associated with that command;
- each block has lifecycle state, exit status and duration;
- saved `.kea` recordings can reconstruct those blocks without guessing shell prompts;
- interactive programs can temporarily use the exact same PTY through Direct mode.

Terminal state changes are recorded too, which enables inspection and rewind of transient TUI states. **Replay is a consequence of the model, not Kea's main purpose.**

## The model

A conventional terminal exposes one mutable character grid. Shell input, command output and interactive applications all compete for that surface, and history is mostly whatever happened to scroll off it.

Kea separates the concepts:

```text
Session document

┌ command #1 · exit 0 · 1.42s ──────────────────────┐
│ cargo test                                         │
├ read-only output ──────────────────────────────────┤
│ running 42 tests                                   │
│ ...                                                │
│ test result: ok                                    │
└────────────────────────────────────────────────────┘

┌ command editor ────────────────────────────────────┐
│ docker compose run --rm backend \                  │
│     python manage.py migrate                       │
│                                                    │
│ Enter = newline        Ctrl+Enter = execute        │
└────────────────────────────────────────────────────┘
```

For something such as OpenCode, Vim, `less`, SSH or a REPL, switch to **Direct PTY**. Kea does not start a second shell: keystrokes go to the same live PTY. When the interactive program exits, switch back and continue the same document session.

This gives Kea two complementary views over one session:

1. **Document view** for ordinary commands and persistent read-only output.
2. **Terminal view** for software that genuinely needs terminal-screen semantics, including historical reconstruction.

## Structured command blocks

Kea does not infer command boundaries from prompt regexes. In Document mode it owns submission explicitly and wraps the command with a small shell adapter that emits private OSC markers before and after execution. The markers carry a command ID and the original command text; the completion marker carries the exit status.

The markers are ordinary terminal output bytes, so the existing append-only recording remains canonical. `kea-document` derives command blocks from that stream. Reopening a recorded session therefore reconstructs the same structured document without re-executing commands or requiring a separate sidecar database.

Currently supported Document-mode shell adapters are:

- POSIX-style interactive shells: `sh`, `bash`, `dash`, `zsh`, `ksh`, `mksh`;
- PowerShell: `pwsh` / Windows PowerShell.

Other programs still work in Direct PTY mode. On Windows, launching Kea without an explicit program uses Windows PowerShell for Document mode rather than `cmd.exe`.

Document metadata is deliberately modest for now: command text, lifecycle, exit status, timing and retained output. Working-directory metadata is not fabricated when the shell has not explicitly supplied it.

## Input modes

### Document mode

This is the default when Kea recognizes the interactive shell.

- `Enter` inserts a newline.
- `Ctrl+Enter` executes the whole editor buffer by default.
- arrow keys, Home/End, Backspace/Delete and Tab edit the local buffer.
- paste inserts text into the local buffer.
- only one Document command is submitted at a time, because one interactive shell cannot reliably delimit overlapping foreground commands.
- if a command opens a TUI, switch to Direct PTY and interact with it there; the command block stays running until the shell regains control.

### Direct PTY mode

Direct mode forwards ordinary terminal key sequences to the current child. Toggle modes with `Ctrl+Shift+Space` by default, or start directly there:

```bash
kea --direct -- opencode
```

In Direct mode `Ctrl+Enter` is left available to the child rather than being captured as Kea's execute action. This is useful for applications such as OpenCode that distinguish modified Enter from ordinary Enter.

## Shortcuts

Kea actions and physical shortcuts are separate and configurable. Defaults follow desktop conventions rather than inheriting terminal conventions blindly.

| Action | Linux / Windows | macOS |
| --- | --- | --- |
| Copy current document/view | `Ctrl+C` | `Cmd+C` |
| Paste | `Ctrl+V` | `Cmd+V` |
| Interrupt child | `Ctrl+Shift+C` | `Ctrl+C` |
| Execute document input | `Ctrl+Enter` | `Ctrl+Enter` |
| Toggle Document / Direct PTY | `Ctrl+Shift+Space` | `Ctrl+Shift+Space` |
| Previous / next terminal-history event | `F6` / `F7` | `F6` / `F7` |
| Back / forward five seconds | `Shift+F6` / `Shift+F7` | `Shift+F6` / `Shift+F7` |
| Play / pause terminal history | `F8` | `F8` |
| Return to live | `F9` | `F9` |
| Quit | `Ctrl+Shift+Q` | `Cmd+Q` |

In Document view, copy currently copies the retained structured document. In terminal/history view it copies the visible terminal screen. Fine-grained text selection is not implemented yet.

### Configure shortcuts

Kea reads an optional `keybindings.conf` from:

- Linux: `$XDG_CONFIG_HOME/kea/keybindings.conf`, or `~/.config/kea/keybindings.conf`
- macOS: `~/Library/Application Support/Kea/keybindings.conf`
- Windows: `%APPDATA%\Kea\keybindings.conf`

Set `KEA_KEYBINDINGS=/some/path` to use an explicit file instead.

```text
# Restore traditional terminal copy/interrupt behavior on Linux:
copy = ctrl-shift-c
interrupt = ctrl-c

# Use another document execution shortcut:
execute = alt-enter

toggle_direct = ctrl-shift-space
go_live = f9
```

Multiple shortcuts can be comma-separated and `none` unbinds an action. Duplicate assignments are rejected. An invalid file produces a warning and Kea falls back to OS defaults.

## Terminal history and replay

Kea also records ordered terminal output, resizes and process lifecycle events. The live process keeps running while an older screen state is inspected. Historical replay never sends input or re-executes commands.

This is especially useful for:

- errors that flashed briefly and were overwritten;
- TUI redraws;
- inspecting what a coding agent displayed earlier;
- reopening a recorded session without starting its process again.

Document blocks and terminal history are related but not the same abstraction. A block is structured command/output state; terminal history is the underlying compatibility/event stream.

## Try it on Ubuntu

Install a current stable Rust toolchain through [rustup](https://rustup.rs/), then the native build dependencies. The desktop requires a graphical session and a working Vulkan driver.

```bash
sudo apt-get update
sudo apt-get install -y build-essential pkg-config cmake clang libclang-dev \
  libasound2-dev libxkbcommon-x11-dev libwayland-dev libssl-dev \
  libfontconfig-dev libfreetype-dev libx11-xcb-dev libxcb-shape0-dev \
  libxcb-xfixes0-dev libxcb-randr0-dev libvulkan-dev

git clone git@github.com:ManuelZierl/kea.git
cd kea
cargo run --release
```

Useful variants:

```bash
# Start a TUI directly in compatibility mode:
cargo run --release -- --direct -- opencode

# Synthetic terminal-history demonstration:
cargo run --release -- --demo

# Record a new session. Existing files are never overwritten:
cargo run --release -- --record session.kea -- bash

# Reopen a recording without starting a process. Structured blocks are rebuilt
# from the recorded document markers when present:
cargo run --release -- --replay session.kea
```

The executable is `target/release/kea`. `cargo install --path crates/kea-app` installs it locally. There is no crates.io release or installer yet.

## Structure

| Crate | Responsibility |
| --- | --- |
| `kea-core` | Standard-library-only canonical terminal events, recording format, validation and replay trait |
| `kea-document` | UI-independent command/output block model and streaming document-marker parser |
| `kea-alacritty` | Replaceable Alacritty terminal projection and silent replay |
| `kea-pty` | Standalone Unix PTY / Windows ConPTY transport |
| `kea-session` | Live/history state, observation tap, hidden application-owned input, playback and optional disk writer |
| `kea-app` | GPUI document/terminal views, multiline editor, shell adapters, semantic keymap and timeline |

`kea-core` and `kea-document` do not depend on GPUI, Zed, a PTY library or an OS UI. Shell-specific command wrapping lives in the host application rather than the portable document model. A future Zed integration should reuse Zed's process ownership, editor/keybinding conventions and renderer while reusing or adapting the portable recording/document layers.

See [architecture](docs/architecture.md), [document protocol](docs/document-protocol.md), [recording format](docs/recording-format.md), [roadmap](docs/roadmap.md), and [development invariants](AGENTS.md).

## Limits and privacy

History stays in memory unless `--record` is supplied. **Terminal output can contain echoed commands, passwords, tokens and private documents. Structured command blocks explicitly retain submitted command text.** Recording files are neither encrypted nor automatically redacted. Unix files are created with mode `0600`; Windows files inherit directory permissions. Keep recordings private and out of Git.

Terminal history is bounded to 32 MiB of accounted event data/overhead or 100,000 events. Structured document retention is separately bounded: a command is at most 64 KiB, a block retains at most 4 MiB of output, the in-memory document retains at most 64 MiB and 10,000 blocks. Hitting a limit is surfaced rather than silently pretending the document/history is complete; the underlying terminal can continue.

Backward terminal-history seeking currently replays from the beginning. There are no full-state checkpoints, compressed long-session storage or historical full-text index yet. Several terminal updates received inside one PTY read chunk cannot yet be selected as separate timestamped frames.

The local editor and terminal renderer are still intentionally small: no selection, undo/redo, full IME integration, mouse reporting, complete Kitty keyboard protocol, terminal image protocol or accessibility implementation. Document output uses a conservative text projection for command blocks; Direct mode remains the fidelity path for complex TUIs.

## Development

```bash
cargo fmt --all -- --check
cargo test -p kea-core -p kea-document -p kea-alacritty -p kea-pty -p kea-session
cargo clippy -p kea-core -p kea-document -p kea-alacritty -p kea-pty -p kea-session --all-targets -- -D warnings
cargo test -p kea-app
cargo build -p kea-app
```

CI runs portable engine/document/session tests on Linux, macOS and Windows, and builds/tests the GPUI application on Linux. The Linux desktop smoke test exercises both structured Document execution and terminal-history rewind/copy. macOS/Windows desktop packaging and interactive validation remain outstanding.

## License

MIT. Dependencies retain their respective licenses. Kea is independent, not an official Zed feature or extension.
