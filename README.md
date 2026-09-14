# Kea

**Rewind what your terminal displayed. Keep the live process running.**

Kea is a Rust/GPUI terminal experiment with an embeddable history engine. Raw terminal output and window-size changes are recorded so historical screens can be reconstructed, including text overwritten by a TUI. Replay never re-executes commands.

The first slice includes a native window, a real PTY, Alacritty emulation, a timeline, playback, optional recording files and a synthetic demo. It is **not yet a replacement for a mature terminal**. Editor-like multiline input and Zed integration are future goals, not implemented features.

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

The demo briefly displays `ERROR: connection failed`, overwrites it with `Ready`, and lets you recover the error by dragging the timeline backwards. It does not run a shell, access a network, or create a recording file.

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
