<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/kea-logo-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="assets/kea-logo.svg">
    <img alt="Kea" src="assets/kea-logo.svg" width="220">
  </picture>
</p>

<h1 align="center">Kea</h1>

<p align="center"><strong>A persistent terminal workspace built around a normal text editor.</strong></p>

Kea keeps a real terminal and a normal multiline editor visible over **one process/session**. Existing shells, SSH, OpenCode, Vim, REPLs and other TUIs keep a PTY. The editor supplies ordinary desktop editing. Kea adds explicit execution, retained history and optional structure around them.

Replay is a consequence of the persistent event history, not the main product.

## The interaction model

```text
┌──────────────────────────────────────────────────────┐
│ TERMINAL                                             │
│ shell / OpenCode / Vim / SSH / REPL / other TUI    │
│                                                      │
│ when focused, child application owns terminal input │
└──────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────┐
│ EDITOR                                               │
│ normal selection / clipboard / undo / IME / mouse   │
│ multiline draft / syntax / completion               │
│                                                      │
│ explicit actions: Run in shell / Send to app        │
└──────────────────────────────────────────────────────┘

                 one PTY + retained history
                           │
             ┌─────────────┼─────────────┐
             ▼             ▼             ▼
          replay        metadata      optional blocks
```

There is no Document-versus-PTY mode. Clicking a surface changes focus, not processes or execution models.

### Editor defaults

Kea deliberately keeps editor-native behavior by default:

| Action | Default |
| --- | --- |
| Newline | Enter |
| Run draft in integrated shell | Ctrl+Enter |
| Send draft to current application | Ctrl+Shift+Enter |
| Complete draft | Tab |
| Focus editor | Ctrl+L / Cmd+L |
| Show/hide optional blocks | Ctrl+Shift+Space |

`Enter` is not hard-coded by Kea; it remains the editor component's normal newline action. Users who prefer terminal/chat-style input can configure:

```text
run_shell = enter
newline = shift-enter
```

All Kea semantic shortcuts are configurable in `keybindings.conf`.

### Run in shell vs Send to app

These are separate **actions**, never hidden modes.

**Run in shell** evaluates the draft in the integrated local shell. It is available only when that shell has explicitly reported an idle prompt. Kea does not guess readiness from `$`, `>`, cursor position or idle time.

**Send to app** sends the draft literally to whichever application currently owns stdin and then sends Enter. It never injects shell wrappers. Multiline Send requires bracketed-paste support; otherwise Kea refuses rather than accidentally executing lines separately.

A successful submission creates a fresh editor draft and fresh undo history. Undo edits text; it never pretends to undo a shell side effect.

## Terminal input and TUIs

With the live terminal focused, **Kea reserves none of its own shortcuts**. Ctrl+C/V/Z/L, Tab, modified Enter and function keys go through the terminal input bridge rather than triggering Kea actions. Composed Unicode/IME text uses the platform text-input path and is forwarded only after composition commits.

There is one unavoidable qualification to “the TUI gets all keys”: terminal applications do not receive physical keyboard events directly. The path is roughly:

```text
OS key/text event → GPUI → terminal encoding → bytes/escape sequence → PTY
```

Some physical keys are indistinguishable in classic terminal protocols (`Ctrl+I` and Tab, `Ctrl+M` and Enter, for example), some OS/window-manager shortcuts may never reach Kea, and newer distinctions such as modified Enter depend on terminal keyboard-protocol support. Kea's rule is therefore: **never steal child input, and preserve every distinction the OS + terminal protocol make available**. Broader modern keyboard-protocol negotiation remains compatibility work.

Toolbar controls remain clickable while the terminal owns the keyboard.

## Current directory: explicit, not guessed

For integrated interactive local shells, Kea installs a small prompt hook that preserves the user's existing prompt/profile. Every time the shell becomes idle it reports its cwd explicitly.

This means cwd updates after both:

```text
Run in shell:  cd /tmp
```

and a command typed directly into the terminal:

```text
cd /tmp
```

When the local shell is at its prompt, Kea labels this **Current shell directory**. While OpenCode, SSH, Vim or another foreground application owns the terminal, Kea cannot truthfully know that application's internal or remote cwd, so it shows **Shell directory (last reported)** until the local shell prompt returns.

Kea does not scrape prompt text and does not fabricate remote cwd information.

Supported shell integration currently covers interactive `sh`, Bash, dash, zsh, ksh/mksh, `pwsh` and Windows PowerShell. Noninteractive script/`-c`/`-Command` launches are not injected. Windows PowerShell starts with the user's profile enabled.

## Autocomplete

Autocomplete has two complementary paths.

**Terminal Tab is native.** With terminal focus, Tab goes directly to Bash/PowerShell/OpenCode/a REPL/etc., so the application's own aliases, functions, argument completers and configuration remain authoritative.

**Editor Tab is safe local completion.** When an integrated local shell is idle, Kea uses the cwd and effective `PATH` reported by that shell to offer:

- retained command-history prefixes;
- executable names from the shell's effective `PATH`;
- files and directories relative to the shell's current cwd.

Suggestions run off the UI thread, are bounded, are discarded if the draft changed, and are inserted as ordinary undoable editor text. Kea never evaluates the draft to discover suggestions. Complex shell syntax, programmable flag completion, remote completion and application-specific completion remain available through native terminal Tab rather than being approximated incorrectly.

## Optional command blocks

Blocks are an **observer**, not the execution model. They are hidden by default (`show_blocks = false`). Shell start/done markers can derive command source, output, timing, exit status and starting directory after execution has been sent.

Crucially:

> **Block retention or parsing failure must never prevent or strand command execution.**

Kea no longer creates a local “queued” block as a prerequisite for Run in shell. If metadata cannot be retained, the command still executes; only the optional structured view is incomplete.

When shown, blocks provide read-only selectable output, collapse/expand, filtering, bounded paging, Copy block and Edit as new. Editing as new creates a draft; it never changes the historical execution.

## Persistent history and replay

Raw terminal output, resize and lifecycle events are retained as the canonical session history. Structured blocks are derived metadata on top of that stream.

A separate historical emulator can reconstruct old screen states while the live process continues receiving output. Rewind cannot send input, execute commands, open links or mutate the clipboard. Returning to LIVE changes only the view; it never restores process state.

Backward seeking currently replays from the beginning. Efficient complete-state checkpoints, compressed/indexed long-session storage and historical full-text search remain future work.

## OS/editor integration

Ordinary editor behavior belongs to GPUI Component and platform text services rather than a Kea-specific text engine: selection, clipboard, undo/redo, mouse editing, grapheme movement and composition are reused.

Appearance follows system light/dark by default. Kea adds no smart-quote/autocorrection substitutions to executable text.

This architecture intentionally leaves the portable core independent of GPUI. A Zed integration should use Zed's own editor/actions/PTY/renderer around the reusable Kea recording/document layers.

## Configuration

Configuration directories:

- Linux: `$XDG_CONFIG_HOME/kea/`, otherwise `~/.config/kea/`
- macOS: `~/Library/Application Support/Kea/`
- Windows: `%APPDATA%\Kea\`

`KEA_KEYBINDINGS` and `KEA_SETTINGS` select explicit files.

Example `keybindings.conf` using the defaults explicitly:

```text
run_shell = ctrl-enter
send_application = ctrl-shift-enter
complete = tab
toggle_blocks = ctrl-shift-space
```

Terminal-like editor policy:

```text
run_shell = enter
newline = shift-enter
```

Legacy names such as `execute`, `send_text` and `toggle_direct` remain accepted as aliases for configuration compatibility. Multiple shortcuts may be comma-separated; `none` unbinds an action. Ambiguous assignments are rejected and the file falls back safely.

Example `settings.conf`:

```text
theme = system
show_blocks = false
font_family = system
font_size = system
syntax_highlighting = true
line_numbers = false
soft_wrap = true
output_wrap = true
```

## Run from source

Install a current stable Rust toolchain. On Ubuntu:

```bash
sudo apt-get update
sudo apt-get install -y build-essential pkg-config cmake clang libclang-dev \
  libasound2-dev libxkbcommon-x11-dev libwayland-dev libssl-dev \
  libfontconfig-dev libfreetype-dev libx11-xcb-dev libxcb-shape0-dev \
  libxcb-xfixes0-dev libxcb-randr0-dev libvulkan-dev

git clone git@github.com:ManuelZierl/kea.git
cd kea
cargo run --locked --release
```

Useful launches:

```bash
# Start with terminal focus; editor still remains visible.
cargo run --locked --release -- --terminal-focus -- opencode

# Backwards-compatible alias:
cargo run --locked --release -- --direct -- opencode

# Explicitly persist a session.
cargo run --locked --release -- --record session.kea -- bash

# Inspect a recording without starting/re-executing a process.
cargo run --locked --release -- --replay session.kea

# Transient overwritten-error demonstration.
cargo run --locked --release -- --demo
```

On Windows, `kea.exe` uses the GUI subsystem, so it does not intentionally allocate a companion console window. Development builds remain unsigned; application-control/SmartScreen policy is separate from Kea's terminal behavior.

## Architecture

| Crate | Responsibility |
| --- | --- |
| `kea-core` | std-only canonical events, validated recording format and replay interface |
| `kea-document` | UI-independent optional command/output metadata derived from the stream |
| `kea-alacritty` | terminal projection/emulation |
| `kea-pty` | Unix PTY / Windows ConPTY transport |
| `kea-session` | live/history state, observation tap, input, playback and optional journal |
| `kea-app` | GPUI editor/terminal surfaces, shell integration, completion and host actions |

The child process starts in the directory from which Kea was launched rather than an unrelated PTY default directory.

## Limits and privacy

History stays in memory unless `--record` is supplied. Recordings are bounded, create-only and **unencrypted**. Terminal output and submitted command metadata may contain passwords, tokens or private documents. No raw keystroke log is added.

Terminal recording currently stops retaining new events after 32 MiB of accounted event data/overhead or 100,000 events; the live PTY can continue. Optional block retention has separate bounds (64 KiB command metadata, 4 MiB output per block, 64 MiB aggregate, 10,000 blocks). Hitting a block limit does not stop command execution.

Shell metadata is interoperability data, not authenticated provenance: an application can forge terminal output. Do not use it as an authorization boundary.

## Development validation

```bash
cargo fmt --all -- --check
cargo test --locked -p kea-core -p kea-document -p kea-alacritty -p kea-pty -p kea-session
cargo clippy --locked -p kea-core -p kea-document -p kea-alacritty -p kea-pty -p kea-session --all-targets -- -D warnings
cargo test --locked -p kea-app --lib
cargo build --locked -p kea-app
```

Real platform validation is intentionally distinguished from compilation and component tests. Important compatibility targets include Windows/OpenCode, Wayland/IBus/Fcitx, macOS/Windows IMEs, mouse-reporting TUIs, modern keyboard protocols, accessibility and packaging.

See [interaction contract](docs/unified-session.md), [architecture](docs/architecture.md), [editor integration](docs/editor-integration.md), [document protocol](docs/document-protocol.md), [recording format](docs/recording-format.md), [roadmap](docs/roadmap.md), and [development invariants](AGENTS.md).

## License

MIT. Dependencies retain their licenses. Kea is independent, not an official Zed feature or extension.
