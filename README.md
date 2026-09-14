# Kea

**The terminal, rethought as a persistent document.**

Kea explores a different terminal model: instead of treating one mutable character grid as the canonical user interface, treat the terminal session as persistent state with editor-like input and inspectable output.

The intended experience is closer to an editor than a traditional terminal:

- command input is editable multiline text;
- `Enter` inserts a newline and an explicit action executes the block;
- output is read-only rather than being the place where input editing happens;
- interactive PTY/TUI programs still work through a direct compatibility mode;
- terminal state changes are retained instead of disappearing when a program redraws the screen.

That model enables normal editor behavior, structured command/output history, better search and inspection, transient-error recovery, agent observability, and time travel through interactive applications. **Replay is a consequence of the model, not Kea's main purpose.**

## Why

A conventional terminal exposes one mutable character grid. Input and output share that grid, shell editing uses terminal-specific conventions, scrollback is only a partial history, and a TUI is free to overwrite what was visible a moment ago.

Kea's model is different:

```text
Session document

[read-only terminal/output]
$ cargo test
running 42 tests
...
test result: ok

[document input]
docker compose run --rm backend \
    python manage.py migrate

Enter       -> newline
Ctrl+Enter  -> execute (default; configurable)
```

Interactive programs remain compatible through a PTY. Their display is backed by an ordered terminal event history, so the current screen is only one view of the session rather than the session itself.

This makes otherwise unusual capabilities natural rather than bolted on:

- recover text that a TUI displayed only briefly and then erased;
- inspect or search historical terminal states;
- rewind an interactive application without rewinding or re-executing the process;
- eventually associate commands with output, cwd, duration and exit status when shell metadata is available;
- collapse, bookmark, compare, copy or revisit previous command results;
- give IDEs and coding agents structured terminal history instead of forcing them to scrape an ephemeral screen.

## Input modes

Kea currently has two explicit input modes.

### Document mode

This is the default. Keystrokes edit a local multiline command buffer instead of being sent immediately to the PTY.

- `Enter` inserts a newline.
- `Ctrl+Enter` executes the whole buffer by default.
- arrow keys, Home/End, Backspace/Delete and Tab edit the local buffer.
- paste inserts text into the local buffer.
- after successful submission the buffer is cleared.

The current editor is intentionally small. It does not yet have selections, undo/redo, full IME behavior, syntax highlighting, completion, or true structured command/output blocks. Submission currently feeds the buffered lines into the underlying PTY; Kea does not infer shell command boundaries from prompt text.

### Direct PTY mode

Some programs need individual key events immediately: OpenCode, Vim, REPLs, `less`, `htop`, SSH sessions and other TUIs. Direct PTY mode forwards ordinary terminal keys to the child using terminal protocol encoding.

Toggle Document/Direct mode with `Ctrl+Shift+Space` by default, or start directly in compatibility mode:

```bash
kea --direct -- opencode
```

In Direct mode `Ctrl+Enter` is left available to the child rather than being captured as Kea's execute action. This matters for applications such as OpenCode that distinguish modified Enter from ordinary Enter.

## Shortcuts

Kea actions and physical shortcuts are separate. The defaults follow desktop conventions rather than assuming traditional terminal bindings.

| Action | Linux / Windows | macOS |
| --- | --- | --- |
| Copy visible output | `Ctrl+C` | `Cmd+C` |
| Paste | `Ctrl+V` | `Cmd+V` |
| Interrupt child | `Ctrl+Shift+C` | `Ctrl+C` |
| Execute document input | `Ctrl+Enter` | `Ctrl+Enter` |
| Toggle Document / Direct PTY | `Ctrl+Shift+Space` | `Ctrl+Shift+Space` |
| Previous / next history event | `F6` / `F7` | `F6` / `F7` |
| Back / forward five seconds | `Shift+F6` / `Shift+F7` | `Shift+F6` / `Shift+F7` |
| Play / pause history | `F8` | `F8` |
| Return to live | `F9` | `F9` |
| Quit | `Ctrl+Shift+Q` | `Cmd+Q` |

Copy currently copies the entire visible terminal screen because text selection is not implemented yet.

### Configure shortcuts

Kea reads an optional `keybindings.conf` from:

- Linux: `$XDG_CONFIG_HOME/kea/keybindings.conf`, or `~/.config/kea/keybindings.conf`
- macOS: `~/Library/Application Support/Kea/keybindings.conf`
- Windows: `%APPDATA%\Kea\keybindings.conf`

Set `KEA_KEYBINDINGS=/some/path` to use an explicit file instead.

The format is deliberately small: one semantic action per line. Multiple shortcuts can be comma-separated and `none` unbinds an action.

```text
# Restore traditional terminal copy/interrupt behavior on Linux:
copy = ctrl-shift-c
interrupt = ctrl-c

# Use another document execution shortcut:
execute = alt-enter

toggle_direct = ctrl-shift-space
go_live = f9
```

Duplicate shortcut assignments are rejected. If the file is invalid, Kea shows a warning and falls back to the OS defaults rather than silently applying a partial configuration.

## What exists today

The current prototype contains both halves needed to explore the model:

1. **document-oriented interaction:** a basic multiline local command editor, explicit execution, configurable semantic shortcuts and Direct PTY fallback;
2. **persistent terminal state:** native Rust/GPUI window, real PTY, Alacritty emulation, bounded terminal-event history, historical reconstruction, timeline/playback and optional recording files.

The live process continues running while an older terminal state is inspected, and replay never re-executes commands.

This is still not a replacement for a mature terminal. In particular, command/output blocks are not yet first-class objects, terminal rendering/input compatibility is incomplete, and the local editor is deliberately basic. Kea is structured so the core history/session machinery can remain independent of the UI and potentially be integrated into editors such as Zed.

## Try it on Ubuntu

Install a current stable Rust toolchain through [rustup](https://rustup.rs/), then install the native build dependencies. The Linux desktop requires a graphical session and a working Vulkan driver.

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
# Start in Direct PTY mode for a TUI:
cargo run --release -- --direct -- opencode

# Synthetic history demonstration:
cargo run --release -- --demo

# Record to a NEW file; existing files are never overwritten:
cargo run --release -- --record session.kea -- bash

# Inspect a recording without starting a process:
cargo run --release -- --replay session.kea
```

The demo briefly displays `ERROR: connection failed`, overwrites it with `Ready`, and lets you recover the error by moving backwards through history. It demonstrates one consequence of retaining terminal state changes; it is not the complete Kea UX.

The executable is `target/release/kea`. `cargo install --path crates/kea-app` installs it locally. There is no crates.io release or installer yet.

## Structure

| Crate | Responsibility |
| --- | --- |
| `kea-core` | Standard-library-only events, recording format, validation, replay trait |
| `kea-alacritty` | Replaceable Alacritty terminal projection and silent replay |
| `kea-pty` | Standalone Unix PTY / Windows ConPTY transport |
| `kea-session` | Separate live/history state, playback, optional disk writer |
| `kea-app` | GPUI UI, document editor, semantic keymap, timeline and terminal rendering |

The core does not depend on Zed, GPUI, a shell, a PTY library, Alacritty, or keybinding policy. The standalone application uses published Alacritty and GPUI crates; it does not copy Zed's terminal code. A future Zed integration should reuse Zed's process ownership, editor conventions and renderer rather than embedding Kea's whole standalone application.

See [architecture](docs/architecture.md), [recording format](docs/recording-format.md), [roadmap](docs/roadmap.md), and [development invariants](AGENTS.md).

## Privacy and limits

History stays in memory unless `--record` is supplied. **Raw keystrokes are not recorded, but output can contain echoed commands, passwords, tokens and private documents.** Recordings are neither encrypted nor automatically redacted. Unix files are created with mode `0600`; Windows files inherit directory permissions. Keep recordings private and out of Git.

History is bounded to 32 MiB of accounted event data/overhead or 100,000 events. On exhaustion, capture stops with a warning while the live terminal continues. The retained prefix is not silently overwritten. Disk queue/storage failures stop persistence with a warning. These limits are not a claim that the whole application uses only 32 MiB of RAM.

Backward seeking currently replays from the beginning; forward playback reuses its historical engine. There are no full-state checkpoints, compression, history text search or long-session disk browsing yet. Timestamps are measured at ingestion, not at the program's internal write time. Several updates in one output chunk cannot yet be selected as separate timestamped frames.

Input/rendering remain basic: no full IME integration, mouse reporting, selection, undo/redo, traditional scrollback UI, complete Kitty keyboard protocol, terminal image protocol or accessibility implementation. Wide/combining characters are represented, but font metrics and complex shaping need work. Full interactive OpenCode compatibility still needs validation on real machines.

## Development

```bash
cargo fmt --all -- --check
cargo test -p kea-core -p kea-alacritty -p kea-pty -p kea-session
cargo clippy -p kea-core -p kea-alacritty -p kea-pty -p kea-session --all-targets -- -D warnings
cargo test -p kea-app
cargo build -p kea-app
```

CI runs engine/session/transport tests on Linux, macOS and Windows, and builds/tests the GPUI app on Linux. It also runs a Linux Xvfb desktop smoke test of the synthetic history demo. Check the actual results for your commit; a configured CI job is not a compatibility guarantee. macOS/Windows desktop packaging and interactive validation remain outstanding.

## License

MIT. Dependencies retain their respective licenses. Kea is independent, not an official Zed feature or extension.
