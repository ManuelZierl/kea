---
title: User guide
nav_order: 2
---

# User guide

Kea keeps a real terminal and a normal multiline composer visible over **one PTY/session**. Existing shells, SSH, OpenCode, Vim/Neovim, REPLs and other TUIs remain terminal applications; Kea does not replace them. It adds editor-grade composition, recoverable authored input, explicit execution, optional local persistence and inspectable terminal history around them.

Replay is supporting functionality, not the main product. The normal Kea loop is:

```text
compose → submit → keep working → interact with the TUI when needed
       → return to composer → recall/edit previous input → submit again
```

## Workspace

```text
┌──────────────────────────────────────────────────────┐
│ Terminal                                             │
│ shell / OpenCode / Vim / SSH / REPL / other TUI    │
│                                                      │
│ real PTY; native terminal input while focused       │
├──────────────────────────────────────────────────────┤
│ Composer                                             │
│ multiline editor / selection / clipboard / undo     │
│ syntax highlighting / local completion / draft recall│
└──────────────────────────────────────────────────────┘
```

The split is draggable. The default live UI keeps terminal + composer central; detailed replay controls appear when History is entered, and optional command blocks are a side inspector.

There is no hidden Document/PTY mode. Focus changes which surface receives input; it does not swap processes or execution models.

## Core keyboard loop

| Action | Default |
| --- | --- |
| Newline in composer | Enter |
| Submit draft to the active terminal receiver | Ctrl+Enter |
| Composer completion | Tab |
| Terminal ⇄ composer (single switch key) | Ctrl+L / Cmd+L |
| Composer → terminal (explicit alternative) | Ctrl+Shift+L / Cmd+Shift+L |
| Paste clipboard into live terminal | Ctrl+Shift+V (Cmd+V on macOS) |
| Select terminal text from composer/chrome | F4 |
| Previous submitted draft | Ctrl+Up |
| Search submitted input and saved memories | Ctrl+R |
| Next submitted draft / restore scratch | Ctrl+Down |
| Show/hide optional command blocks | Ctrl+Shift+Space |

The composer is editor-native by default. Users who prefer terminal/chat semantics can configure:

```text
run_shell = enter
newline = shift-enter
```

After a successful submission, Kea defaults to a fresh focused composer so the user can keep writing while output streams above. Set `post_submit_focus = terminal` to restore immediate terminal focus.

`focus_editor` (Ctrl/Cmd+L) is the deliberate host escape that remains available while the terminal owns input, and the same chord returns from the composer to the terminal. Other Kea semantic shortcuts stay out of the TUI's way. Users whose child application needs the default Ctrl/Cmd+L binding can remap or unbind it. Plain Ctrl+V remains ordinary child input (0x16); clipboard paste into the live terminal uses Ctrl+Shift+V (Cmd+V on macOS) or the terminal Paste button.

## Submit to the terminal

Ctrl+Enter sends the authored draft through the terminal paste path and then
sends Enter. No per-command shell wrapper is injected. At a reported empty,
ready text-input prompt, submission is immediate. Otherwise the first press
shows a warning without sending anything. Release it, then press Enter or
Ctrl+Enter to confirm that draft; another key cancels and continues editing.
Draft, focus and input-context changes cancel confirmation. Holding the first
chord cannot confirm it through key-repeat.

Local shells, integrated remote/nested shells, and cooperating applications use
the same readiness contract. SSH is not a special case. Without integration,
Ctrl+Enter still works using confirmation. Kea does not infer readiness from
process names, prompt text, cursor position or idle time. It never interrupts the
child or clears its existing input automatically. A confirmed raw send may append
to an existing line; it does not guarantee that an unknown program treats text
as a message rather than commands.

The old `run_shell` and `send_application` configuration names are compatibility
aliases for this same guarded behavior; Ctrl+Shift+Enter is no longer a separate
workflow or a bypass. Multiline submission requires negotiated bracketed paste;
otherwise the draft is preserved instead of risking line-by-line execution.

A successful submission creates a fresh draft and fresh undo history. Undo edits
text, not process side effects. Observational shell hooks provide optional block
boundaries without wrapping authored commands. See the full
[active-input contract](active-input.md), including its compatibility limits.

## Submitted-draft history

Commands and prompts written in Kea are authored work, even when they were sent to OpenCode or another arbitrary TUI rather than executed as a shell command.

Kea therefore retains the exact text of successful **Submit** operations independently of optional command blocks. Ctrl+Up / Ctrl+Down browse this history only while the composer owns focus.

Entering history preserves the current unsubmitted scratch draft. Navigating forward past the newest submission restores that scratch draft exactly. Recall edits the composer only: it never sends bytes or re-executes anything.

Recall is in-memory by default. `persist_history = true` opts into a plaintext
`draft-history.txt` file across restarts, separately from saved terminal sessions.
See [draft-history persistence and its current limits](session-persistence.md#submitted-draft-history).

Ctrl+R opens searchable history and named memories from the composer. Enter inserts
the selected text without executing it; Escape preserves the draft. Search history
has a separate `history_persistence = false` opt-in, configurable in Settings and
effective on the next launch. Explicit named saves persist independently. See
[reverse search](reverse-search.md) for scope, privacy and retention limits.

## Terminal/TUI compatibility

Terminal applications receive encoded terminal input, not raw physical keyboard events. Kea uses the terminal emulator's negotiated state rather than scraping screen text.

- Classic terminal encoding remains the fallback.
- Alacritty's Kitty keyboard negotiation is enabled; modified Enter is emitted as CSI-u only after the child negotiated extended keyboard input.
- Committed Unicode/IME text uses the platform text-input path; an in-progress composition is never submitted.
- AltGr/composed text wins over treating Ctrl+Alt as a control chord.
- Bracketed paste is honored.
- Legacy, UTF-8 and SGR mouse-wheel reports are forwarded when requested.
- Primary mouse press/release is forwarded when mouse reporting is active.
- Drag/motion is forwarded only for the negotiated DECSET 1002/1003 modes.
- **Shift+drag** selects locally by default. Shift-modified hover is suppressed so positioning for that reserved gesture cannot update the child TUI. Set `shift_mouse_selects_locally = false` to forward Shift pointer input to mouse-reporting applications. **Shift+wheel** always uses Kea scrollback.
- **Select text** in the terminal header enters local selection without a modifier gesture. F4 does the same from composer/chrome; from terminal focus use Ctrl/Cmd+L, then F4.
- A local selection/caret owns Copy, Esc and navigation/extension keys. Other input clears it and reaches the child on the first key. Alt-drag creates a column selection after local ownership is established.

Valid selections survive ordinary scrolling output. If selected text changes or
is discarded, Kea shows a recovery caret instead of silently turning the next
Copy chord into child input. See [terminal text selection](terminal-text-selection.md).

Some distinctions remain impossible in classic terminal protocols, and OS/window-manager shortcuts may never reach Kea. Advanced graphics protocols, higher Kitty key-release/repeat levels, focus-event forwarding and accessibility certification are tracked separately from the first alpha compatibility gate.

See [terminal compatibility gate](terminal-compatibility-alpha.md).

## Current directory and completion

For supported interactive local shells, Kea installs a small prompt hook that preserves the user's prompt/profile and explicitly reports cwd/readiness plus the effective PATH.

Bash uses PS0/PROMPT_COMMAND; zsh uses preexec/precmd. Authored commands use the
normal line editor and native history. Bash removes the one-time installer from
history. PowerShell hooks run after the profile at startup, rather than typing
implementation code into PSReadLine. Shells without execution hooks may remain
untracked without blocking terminal input.

This tracks `cd` whether entered in the composer or directly in the terminal.
While a TUI owns input, the displayed shell directory is last reported. An
explicit integration inside a nested or remote shell can report its own input
context. Export/install instructions are in [Active input](active-input.md).
Kea never automatically installs remote code or treats a remote cwd as local.

Supported shell detection covers interactive `sh`, Bash, dash, zsh, ksh/mksh,
`pwsh` and Windows PowerShell. Explicit noninteractive script/`-c`/`-Command`
launches are not injected.

Terminal Tab goes unchanged to the child. Composer Tab uses a configured
cooperating provider or clearly labelled local history/PATH/cwd suggestions at
a confirmed original local-shell prompt. The provider transport is implemented,
but native Bash/PowerShell/OpenCode provider servers are not bundled. A prompt
hook alone does not expose native completion. Unintegrated remote/TUI receivers
keep native Tab with terminal focus; Kea does not invent their suggestions.

With the composer menu open, Left/Right or Tab/Shift+Tab cycle candidates;
Up/Down navigate visual rows; Enter accepts without execution; Escape dismisses.
Ctrl+Enter submits the actual draft rather than an unaccepted highlighted item.
Typing, cursor changes, blur and context changes invalidate results. Forwarded
terminal input invalidates readiness; no hidden Ctrl+C recovery is performed.

## Session persistence

Sessions start **Temporary**. Kea does not silently write terminal history to disk because output may contain credentials or private documents.

Choose **Save session** to transition a running session to **Saving locally**. Kea first snapshots all canonical history retained so far, then journals subsequent events. Existing files are never overwritten.

Default saved-session locations:

- Linux: `$XDG_STATE_HOME/kea/sessions/`, otherwise `~/.local/state/kea/sessions/`
- macOS: `~/Library/Application Support/Kea/Sessions/`
- Windows: `%APPDATA%\Kea\Sessions\`

`--record NEW.kea` remains available when an explicit create-new path is desired.

The persistence status distinguishes **Temporary**, **Saving**, and **Saving
stopped**. A **History stopped** warning separately identifies exhausted retained
history: the PTY may continue, but replay and saved output are incomplete beyond
that point. Recordings are unencrypted and bounded; persistence failure never
stops the live PTY.

See [session persistence](session-persistence.md).

## History and replay

Raw terminal output, resize, lifecycle and separate authored-submission metadata form the canonical v2 session history. Optional blocks are observations on top. Existing v1 recordings remain readable; older Kea versions cannot read v2. See the [recording format](recording-format.md).

History uses a separate silent emulator while the live process continues receiving output. A historical view cannot send input or mutate process state. Returning Live changes only the view; it never rolls the process back.

Backward seeking currently replays from the beginning. Efficient checkpoints, compressed/indexed long-session storage and historical full-text search remain future work.

## Optional command blocks

Command blocks are an observer, not the execution model. They are hidden by default. When scoped shell start/done markers match submitted-input metadata, blocks can retain command source, output, timing, status and starting directory. The observer is flat; nested commands can remain untracked while an outer block is active. PowerShell reports success/failure rather than exact native exit codes.

Block parsing or retention failure must never gate command execution. When shown, blocks provide read-only selectable output, collapse/expand, filtering, bounded paging, Copy block and Edit as new.

## Configuration

Configuration directories:

- Linux: `$XDG_CONFIG_HOME/kea/`, otherwise `~/.config/kea/`
- macOS: `~/Library/Application Support/Kea/`
- Windows: `%APPDATA%\Kea\`

`KEA_KEYBINDINGS` and `KEA_SETTINGS` select explicit files.

Use the Settings button in the main toolbar to change appearance, composer and
workflow preferences. Choices are saved automatically to `settings.conf`; theme,
text presentation and input-policy changes apply without replacing the current
draft. Submitted-draft persistence changes take effect on the next launch because
the history file is an explicit plaintext opt-in. The default **System** theme
continues to follow operating-system light/dark changes; **Light** and **Dark**
are fixed overrides.

The **Keybindings** tab edits every configurable shortcut, including **Switch
terminal ⇄ composer** (`focus_editor`) and the composer-only **Focus terminal**
alternative (`focus_terminal`). Changes are validated together when you choose
**Save keybindings**, then take effect after restarting Kea. Pressing Enter
inside any shortcut field saves too. Invalid assignments
leave the previous file and your edits intact. The tab saves a complete
`keybindings.conf` snapshot; closing Settings discards unsaved keybinding edits.
Type shortcut names directly, or choose **Record** and press the combination.
Recording isolates that chord from terminal input and live shortcuts. Escape
cancels; a held chord stays captured until release. Save is still explicit.

Example `keybindings.conf`:

```text
run_shell = ctrl-enter
focus_editor = ctrl-l
focus_terminal = ctrl-shift-l
select_terminal_text = f4
previous_draft = ctrl-up
next_draft = ctrl-down
complete = tab
toggle_blocks = ctrl-shift-space
```

`focus_editor` is the single switch key (the same chord returns from composer
focus); `focus_terminal` is an explicit alternative. Sharing one chord across
both settings is allowed. Multiple shortcuts may be comma-separated; `none`
unbinds an action. Ambiguous assignments are rejected and Kea falls back safely.

Example `settings.conf`:

```text
theme = system
post_submit_focus = editor
persist_history = false
history_persistence = false
shift_mouse_selects_locally = true
animate_logo = true
show_blocks = false
font_family = system
font_size = system
syntax_highlighting = true
line_numbers = false
soft_wrap = true
output_wrap = true
```

Set `animate_logo = false` to keep the decorative composer bird still while
typing.

## Run from source

Install a current stable Rust toolchain. On Ubuntu:

```bash
sudo apt-get update
sudo apt-get install -y build-essential pkg-config cmake clang libclang-dev \
  libasound2-dev libxkbcommon-x11-dev libwayland-dev libssl-dev \
  libfontconfig-dev libfreetype-dev libx11-xcb-dev libxcb-shape0-dev \
  libxcb-xfixes0-dev libxcb-randr0-dev libvulkan-dev

git clone https://github.com/ManuelZierl/kea.git
cd kea
cargo run --locked --release
```

Linux taskbar icon: the app reports `kea` as its Wayland app id. To show the
bird icon next to it, install the desktop entry and icons once (replace `Exec`
with your binary location if it differs):

```bash
mkdir -p ~/.local/share/applications ~/.local/share/icons
sed "s|^Exec=kea$|Exec=$PWD/target/release/kea|" assets/linux/kea.desktop \
  > ~/.local/share/applications/kea.desktop
cp -r assets/linux/icons/hicolor ~/.local/share/icons/
update-desktop-database ~/.local/share/applications
gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor
```

Useful launches:

```bash
cargo run --locked --release -- --terminal-focus -- opencode
cargo run --locked --release -- --record session.kea -- bash
cargo run --locked --release -- --replay session.kea
cargo run --locked --release -- --demo
```

On Windows, `kea.exe` uses the GUI subsystem and does not intentionally allocate a companion console window. Development builds remain unsigned.

Version tags named exactly `v<workspace-version>` publish unsigned Linux, macOS
and Windows archives through GitHub Actions after the cross-platform CI and Linux
desktop smoke test pass. The tagged commit must be reachable from `main`.

## Architecture

| Crate | Responsibility |
| --- | --- |
| `kea-core` | std-only canonical events, validated recording format and replay interface |
| `kea-document` | UI-independent optional command/output metadata derived from the stream |
| `kea-alacritty` | terminal projection/emulation and negotiated terminal mode state |
| `kea-pty` | Unix PTY / Windows ConPTY transport |
| `kea-session` | live/history state, input, playback, observation and optional journal |
| `kea-app` | GPUI terminal/composer workspace, shell integration, completion and product actions |

## Limits and privacy

The live canonical history is bounded. Recording currently stops retaining new events after 32 MiB of accounted event data/overhead or 100,000 events; the live PTY can continue. Optional block retention has separate bounds (64 KiB command metadata, 4 MiB output per block, 64 MiB aggregate, 10,000 blocks).

Saved `.kea` files are create-only and **unencrypted**. Terminal output and submitted command metadata may contain passwords, tokens or private documents. Kea does not add a raw keystroke log.

Shell metadata is interoperability data, not authenticated provenance: terminal output can forge it and it is never an authorization boundary.

## Development validation

```bash
cargo fmt --all -- --check
cargo test --locked -p kea-core -p kea-document -p kea-alacritty -p kea-pty -p kea-session
cargo clippy --locked -p kea-core -p kea-document -p kea-alacritty -p kea-pty -p kea-session --all-targets -- -D warnings
cargo test --locked -p kea-app --lib
cargo build --locked -p kea-app
```

Real OS validation is separate from compilation/unit tests. See the
[terminal compatibility matrix](terminal-compatibility-alpha.md),
[persistence acceptance criteria](session-persistence.md#alpha-acceptance)
and [daily workflow](unified-session.md#workspace-and-daily-workflow).

See the [documentation index](index.md), [architecture](architecture.md),
[interaction contract](unified-session.md), [recording format](recording-format.md),
and [roadmap](roadmap.md).

## License

MIT. Dependencies retain their licenses. Kea is independent, not an official Zed feature or extension.

## Composer sections and guarded interrupts

See [Composer sections and actions](composer-workflow.md) for independently
submitted drafts, opt-in Ctrl-C confirmation and explicitly accepted pattern
recommendations. A section is pending input for the existing receiver, not a
separate shell or an execution queue.
