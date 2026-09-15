<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/kea-logo-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="assets/kea-logo.svg">
    <img alt="Kea" src="assets/kea-logo.svg" width="220">
  </picture>
</p>

<h1 align="center">Kea</h1>

<p align="center"><strong>The terminal, rethought as a persistent document.</strong></p>

Kea combines an ordinary multiline editor with a real terminal session. Compose input with normal editing tools, submit it explicitly, and retain output for inspection. Existing terminal applications keep their terminal; optional command blocks organize shell history without becoming a requirement for using the live session.

**Terminal above, editor below, one process underneath.** There is no Document-versus-PTY execution switch. Click the surface you want to use. Replay is a consequence of retained output history, not the product's main purpose.

## Everyday interaction

The bottom editor stays available even while OpenCode, Vim, a REPL or another interactive program is running. Selection, clipboard, undo/redo, mouse editing, multiline navigation and composition are supplied by GPUI Component and its platform text-input integration. Kea does not maintain another homemade text buffer/undo engine.

| Focus or action | Behavior |
| --- | --- |
| Terminal | Keystrokes go to the child through the terminal encoder. Kea reserves no live-terminal shortcuts. |
| Editor | Ordinary editing; Enter inserts a newline, copy copies the selection. |
| Run in shell — Ctrl+Enter | Evaluates the focused draft through the integrated shell and retains a command record. Requires an explicit shell-ready report. |
| Send to app — Ctrl+Shift+Enter | Pastes the focused draft to the same running program and sends Enter. Never adds shell source/wrappers. |
| Tab in terminal | Goes to the child for its native completion/behavior. |
| Tab in editor | Offers local path and retained-history suggestions; leading whitespace is indentation. Click a result or press Tab again to accept the first. |

Run and Send are distinct because text for a running program is not necessarily shell source. Multiline Send requires the program's bracketed-paste mode; otherwise Kea refuses instead of sending executable lines one by one. Successful submission creates a fresh draft and focuses the terminal. Undo only changes the draft, never a past execution or its side effects.

In terminal focus, Ctrl+C interrupts according to the child/PTY, Ctrl+V is delivered to the child rather than turned into a GUI paste, and function keys stay available. Use the visible toolbar to focus the editor, paste to the child, copy a view or inspect history. OS-reserved shortcuts and distinctions unsupported by the terminal protocol cannot be forwarded magically.

## Directory and completion

The header shows the **last reported shell directory**, updated by explicit shell prompt hooks, including after native terminal `cd`. Unknown program/remote directories are shown as unknown rather than guessed. The reported shell directory is not a claim about the internal directory of an active TUI or remote shell.

Interactive POSIX-style shells and PowerShell have adapters; noninteractive `-c`, `-Command` and script launches are not injected. Existing Bash prompt commands and the PowerShell prompt function are retained. POSIX directory encoding needs `base64` and `tr`. Modified/missing hooks may require native shell interaction.

Editor completion is intentionally conservative: bounded retained-history and file/directory suggestions, off the UI thread, with no command evaluation. Path suggestions require a reported idle shell directory. Complex quoting/substitution/glob expressions are left to native shell completion. Full shell flag/argument completion, remote completion and LSP services are not implemented.

## Optional command blocks

The block inspector is hidden by default (`show_blocks = false`). Show it beside the terminal when command/output association is useful. Each retained shell submission has its original source, output, timing, status and reported starting directory. Blocks are a derived view; the live TUI never has to fit into a linear block renderer.

Output text is read-only and selectable. Blocks support Copy block, collapse, command/output filtering, bounded paging, and Edit as new. Editing as new does not execute anything, rewrite the original record or overwrite a nonempty draft. Selected/focused live block text stays still when more output arrives and offers an explicit refresh. New/populated block views initially follow the tail; older-page navigation does not request latest-page scrolling.

The terminal's rows/columns use the actual canvas layout, keeping its current cursor visible when the editor or inspector changes size. The terminal currently shows its active screen; it does not yet provide full scrollback navigation.

## Shortcuts and defaults

Text-component shortcuts use desktop defaults: Ctrl+C/V/X/Z on Linux/Windows and Cmd+C/V/X/Z on macOS. They do not reserve those shortcuts in a live terminal. Kea actions are configurable separately from editor text input.

In editor/chrome focus, the defaults also include F6/F7 for previous/next history event, F8 for playback, F9 for live, F10 for explicit document/screen copy, Ctrl+L for editor focus, and Ctrl+Shift+Space for showing/hiding blocks. The legacy configuration name `toggle_direct` now means only the optional inspector. The same functions have toolbar controls.

Configuration files:

- Linux: `$XDG_CONFIG_HOME/kea/`, otherwise `~/.config/kea/`.
- macOS: `~/Library/Application Support/Kea/`.
- Windows: `%APPDATA%\Kea\`.

Example `keybindings.conf`:

```text
execute = ctrl-enter
send_text = ctrl-shift-enter
complete = tab
toggle_direct = ctrl-shift-space
```

Multiple shortcuts may be comma-separated; `none` unbinds an action. Invalid or ambiguous files produce a warning and fall back to defaults. `KEA_KEYBINDINGS` selects an explicit file.

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

`system` font settings mean no Kea override of component defaults. Appearance follows system light/dark changes. Configuration is loaded at startup; `KEA_SETTINGS` selects an explicit file. Bash has Tree-sitter highlighting; PowerShell currently uses the editor without a dedicated grammar. Kea adds no smart-quote/autocorrection substitutions to executable text.

## Run from source

Install a current stable Rust toolchain. On Ubuntu install native desktop build dependencies; the graphical application requires a working Vulkan driver:

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

```bash
# Both surfaces remain present; --direct selects initial terminal focus only.
cargo run --locked --release -- --direct -- opencode

# Record a new session without overwriting an existing file.
cargo run --locked --release -- --record session.kea -- bash

# Inspect without starting a shell or re-executing commands.
cargo run --locked --release -- --replay session.kea

# Transient-error/replay demonstration.
cargo run --locked --release -- --demo
```

The Windows CI artifact is `kea-windows-x86_64` and contains `kea.exe`. It is an unsigned development build, not an installer or tagged release. The executable uses the GUI subsystem so it does not allocate a companion console. Startup errors and `--help` use a GUI window. Application-control policies still apply. The default Windows shell is Windows PowerShell with the user's profile enabled.

## Recording, limits and trust

Canonical history is ordered raw terminal output, resize and lifecycle events. Shell command/prompt markers derive optional structured metadata from that same stream. Replay uses a separate emulator and cannot send input, execute commands, change the clipboard or issue other recorded side effects. The live process continues while an old view is inspected.

Disk recording requires `--record`, is create-only, bounded and **unencrypted**. Output can include echoed passwords/tokens; command metadata explicitly retains submitted text. No raw keystroke recording is added. Keep recordings private and out of Git. Unix files are created with mode 0600; Windows files inherit directory permissions.

Terminal history stops capturing with a warning at 32 MiB of accounted event data/overhead or 100,000 events; the live PTY can continue. Structured retention is separately bounded to 64 KiB per command, 4 MiB output per block, 64 MiB aggregate retained command/output/directory data and 10,000 blocks. Truncation is visible. Shell metadata is not authenticated provenance: a program can forge terminal output.

Backward seeking replays from the start; efficient full-state checkpoints and long-session indexes remain outstanding. Complex TUIs need the terminal projection, not the conservative text projection of command output. The echo filter fails open when it cannot safely hide a shell adapter's redrawn input, so cosmetic wrapper text can remain rather than risking loss of real output.

## Architecture and validation

`kea-core` uses only std. `kea-document` depends only on kea-core. Neither depends on GPUI, an OS UI, shell adapters, PTY libraries or Zed. The standalone host composes GPUI Component editing, Alacritty emulation and portable-pty. A future Zed host should use Zed's own editor, actions, process ownership and rendering.

CI runs portable tests/Clippy on Linux, macOS and Windows. Desktop validation includes app tests, Linux graphical interaction checks, a Windows build and GUI-subsystem verification. These are not certification of every IME, accessibility service, OS prediction/dictation feature, terminal keyboard/mouse/image protocol or OpenCode version. Terminal selection/mouse reporting and comprehensive platform-specific input acceptance remain work in progress.

See [unified interaction](docs/unified-session.md), [architecture](docs/architecture.md), [editor integration](docs/editor-integration.md), [document protocol](docs/document-protocol.md), [recording format](docs/recording-format.md), and [development invariants](AGENTS.md).

## License

MIT. Dependencies retain their licenses. Kea is independent, not an official Zed feature or extension.
