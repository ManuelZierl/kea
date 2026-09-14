# Kea

**The terminal, rethought as a persistent document.**

Kea explores a different terminal model: instead of treating a mutable character grid as the canonical user interface, treat the terminal session as structured, persistent state.

The intended experience is closer to an editor than a traditional terminal:

- command input is real editable text, including natural multiline editing;
- executing a command turns that input into a persistent command block;
- command output is read-only and remains associated with the command that produced it;
- interactive PTY/TUI programs still work without modification;
- terminal state changes are retained instead of disappearing when a program redraws the screen.

That model enables ordinary editor behavior, structured command/output history, better search and inspection, transient-error recovery, agent observability, and time travel through interactive applications. **Replay is a consequence of the model, not Kea's main purpose.**

## Why

A conventional terminal exposes one mutable character grid. Input and output share that grid, shell editing uses terminal-specific conventions, scrollback is only a partial history, and a TUI is free to overwrite what was visible a moment ago.

Kea's target model is different:

```text
Session document

[command]
$ cargo test

[read-only output]
running 42 tests
...
test result: ok

[command editor]
docker compose run --rm backend \
    python manage.py migrate

Enter       -> newline
Ctrl+Enter  -> execute
```

Interactive programs remain compatible through a PTY, but their display is backed by an ordered terminal event history. The current screen is therefore only one view of the session, not the session itself.

This is what makes otherwise unusual capabilities natural rather than bolted on:

- recover text that a TUI displayed only briefly and then erased;
- inspect or search historical terminal states;
- rewind an interactive application without rewinding or re-executing the process;
- associate commands with output, cwd, duration and exit status when shell metadata is available;
- collapse, bookmark, compare, copy or revisit previous command results;
- give IDEs and coding agents structured terminal history instead of forcing them to scrape an ephemeral screen.

## What exists today

The first implementation deliberately proves the hardest compatibility foundation before the full document UX.

Today Kea has a native Rust/GPUI window, a real PTY, Alacritty terminal emulation, bounded terminal-event history, historical reconstruction, a timeline, playback, optional recording files and a synthetic demo. The live process continues running while an older terminal state is inspected, and replay never re-executes commands.

**The editor-like command document is not implemented yet.** Current input is still terminal-style input, and Kea is not yet a replacement for a mature terminal. The event/history engine is the foundation on which the document-native interaction model will be built.

Kea is intentionally structured so the history/session engine can remain independent of the UI and potentially be integrated into editors such as Zed instead of requiring the standalone application.

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
cargo run --release -- --demo
```

The demo briefly displays `ERROR: connection failed`, overwrites it with `Ready`, and lets you recover the error by dragging the timeline backwards. This demonstrates one consequence of retaining terminal state changes; it is not intended as the complete Kea UX. The demo does not run a shell, access a network, or create a recording file.

```bash
# Default shell, with in-memory history only:
cargo run --release

# Explicit executable and arguments, without shell-string interpolation:
cargo run --release -- -- bash
cargo run --release -- -- opencode

# Record to a NEW file; existing files are never overwritten:
cargo run --release -- --record session.kea -- bash

# Inspect a recording without starting any process:
cargo run --release -- --replay session.kea
```

The executable is `target/release/kea`. `cargo install --path crates/kea-app` installs it locally. There is no crates.io release or installer yet.

## Controls

| Control | Action |
| --- | --- |
| Timeline click/drag | Seek to a recorded time |
| F6 / F7 | Previous / next event |
| Shift+F6 / Shift+F7 | Back / forward five seconds |
| F8 | Play / pause |
| F9 or LIVE | Return to the current terminal |
| Ctrl+Shift+C / Copy screen | Copy the entire visible screen |
| Ctrl+V | Paste; multiline paste requires bracketed-paste support |
| Ctrl+C | Interrupt the live program, not copy |
| Shift+Enter | Send a distinct modified-Enter sequence |
| Ctrl+Shift+Q | Quit |

Cmd+C/V/Q equivalents are handled on macOS. F6–F9 are reserved, not forwarded to the child. Copy currently copies the entire screen, not a selection. History rejects process input. The live process continues collecting output while you inspect history. Closing the window requests termination of the directly managed child; this is not a detachable terminal server.

## Structure

| Crate | Responsibility |
| --- | --- |
| `kea-core` | Standard-library-only events, recording format, validation, replay trait |
| `kea-alacritty` | Replaceable Alacritty terminal projection and silent replay |
| `kea-pty` | Standalone Unix PTY / Windows ConPTY transport |
| `kea-session` | Separate live/history state, playback, optional disk writer |
| `kea-app` | GPUI window, timeline, keyboard bridge, cell rendering |

The core does not depend on Zed, GPUI, a shell, a PTY library or Alacritty. The standalone application uses pinned published Alacritty and GPUI crates; it does not copy Zed's terminal code. A future Zed integration should reuse Zed's existing process ownership and renderer rather than embed the whole standalone application.

See [architecture](docs/architecture.md), [recording format](docs/recording-format.md), [roadmap](docs/roadmap.md), and [development invariants](AGENTS.md).

## Privacy and limits

History stays in memory unless `--record` is supplied. **Raw keystrokes are not recorded, but output can contain echoed commands, passwords, tokens and private documents.** Recordings are neither encrypted nor automatically redacted. Unix files are created with mode `0600`; Windows files inherit directory permissions. Keep recordings private and out of Git.

History is bounded to 32 MiB of accounted event data/overhead or 100,000 events. On exhaustion, capture stops with a warning while the live terminal continues. The retained prefix is not silently overwritten. Disk queue/storage failures stop persistence with a warning. These limits are not a claim that the whole application uses only 32 MiB of RAM.

Backward seeking currently replays from the beginning; forward playback reuses its historical engine. There are no full-state checkpoints, compression, history text search or long-session disk browsing yet. Timestamps are measured at ingestion, not at the program's internal write time. Several updates in one output chunk cannot yet be selected as separate timestamped frames.

Input/rendering remain basic: no full IME integration, mouse reporting, selection, traditional scrollback UI, complete Kitty keyboard protocol, terminal image protocol or accessibility implementation. Wide/combining characters are represented, but font metrics and complex shaping need work. Shift+Enter has an encoding test; full interactive OpenCode compatibility needs testing on real machines.

## Development

```bash
cargo fmt --all -- --check
cargo test -p kea-core -p kea-alacritty -p kea-pty -p kea-session
cargo clippy -p kea-core -p kea-alacritty -p kea-pty -p kea-session --all-targets -- -D warnings
cargo test -p kea-app
cargo build -p kea-app
```

CI runs engine/session/transport tests on Linux, macOS and Windows, and builds/tests the GPUI app on Linux. It also runs a Linux Xvfb desktop smoke test of the synthetic demo: launch, step back to the overwritten error, copy historical text, return to latest, and quit. Check the actual results for your commit; a configured CI job is not a compatibility guarantee. macOS/Windows desktop packaging and interactive validation remain outstanding.

Tests cover overwritten errors, alternate screens, split UTF-8/escapes, read-only replay, concurrent live capture, recorded resizes, quota limits, corrupted/truncated files, exclusive recording creation and final output draining before exit.

## License

MIT. Dependencies retain their respective licenses. Kea is independent, not an official Zed feature or extension.
