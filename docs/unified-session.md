---
title: Interaction contract
nav_order: 3
---

# Per-tab sessions, independent surfaces

Each Kea tab has one live terminal session and one persistent editor surface. They coexist. There is no Document/PTY mode and no hidden submission-target state.

The optional block inspector is a derived view of observed shell markers. Hiding it does not change input routing, execution, the child process, recording or replay.

## Workspace and daily workflow

The terminal is the largest surface, with the resizable composer below it.
Compact chrome provides focus, optional blocks, explicit copy, Save session and
History actions. Detailed replay controls appear only in History. The status
bar identifies cwd, notices and persistence state.

The daily loop is compose → Submit → keep composing while output streams →
focus the terminal when the child needs input → return to the composer → recall,
edit and submit previous authored text. Users should be able to complete that
loop without a mouse or understanding PTY/event internals.

`focus_editor` (Ctrl+L / Cmd+L) is the single switch key: it returns from the
terminal to the composer, and the same chord returns from the composer to the
terminal. `focus_terminal` (Ctrl+Shift+L / Cmd+Shift+L) remains available as an
explicit composer → terminal alternative. Both are configurable, and sharing one
chord across both settings is intentional, not ambiguous. Ctrl+Up / Ctrl+Down
recall successful submissions only in the composer, preserve the scratch draft
and never execute automatically. Failed sends and Edit as new are not
submitted-history entries.

## Focus owns physical input

When the editor or a read-only text component is focused, GPUI Component/platform services own ordinary editing: selection, clipboard, undo/redo, pointer interaction and composition.

When the live terminal is focused, Kea masks semantic accelerators including user overrides, except the configurable `focus_editor` switch key. With no local selection/caret, representable Ctrl combinations, Tab, modified Enter and function keys reach the terminal encoder; plain Ctrl+V therefore reaches the child as `0x16`. Explicit clipboard paste into the live terminal uses Ctrl+Shift+V (Cmd+V on macOS) or the terminal Paste button and sends bracketed-paste bytes when supported. A visible Kea selection/caret temporarily owns Copy, Esc and navigation/extension; other input clears it and follows normal terminal routing. Composed text uses GPUI's platform text-input handler and is forwarded only after commit. Active composition owns its candidate navigation/cancellation keys.

“Pass all keys to the TUI” has a protocol boundary: terminal programs receive encoded bytes/sequences, not raw physical keyboard events. Classic terminal protocols intentionally collapse some combinations (for example Tab and Ctrl+I); the OS/window manager may reserve others; newer distinctions require extended keyboard protocols. Beyond the documented host escape and local text commands, Kea preserves every distinction exposed by the platform + negotiated terminal protocol.

## One submission action

**Submit** (`run_shell`, default Ctrl+Enter) sends the authored draft, using the
receiver's bracketed-paste mode when supported, followed by Enter. It never
constructs a shell program around the draft. `send_application` and its legacy
Ctrl+Shift+Enter binding are aliases for the same policy, not a bypass.

An explicit ready input report permits immediate submission. Otherwise the first
press only arms a visible confirmation. Release the first chord, then press Enter
or Ctrl+Enter to send that exact draft to that same context. Another key cancels
and continues normal editing. Changes to the draft, focus, or reported input
context cancel confirmation. No Ctrl+C is generated automatically; Interrupt is
an independent action. The Submit button follows the same confirmation policy.

Readiness is live session state, independent of optional command blocks. Local,
nested and remote shell integrations use the same protocol. An unintegrated SSH
session, REPL or TUI uses the guarded raw-input fallback. Kea does not claim to
know that an unknown program is safe to receive text. Confirmation grants intent,
not an atomic delivery guarantee; sends are not automatically retried.

Enter remains editor-native newline by default. Terminal/chat-style configuration:

```text
run_shell = enter
newline = shift-enter
```

Enter accepts an active completion without submitting. Ctrl+Enter submits the
actual draft, not an unaccepted highlighted candidate. A successful submission
creates a fresh draft. Undo never changes an execution or reverses its effects.
History and ended sessions reject input while preserving the draft. Multiline
input requires negotiated bracketed paste, rather than executing lines through
an unknown line editor.

## Observational shell integration and blocks

Shell hooks report prompt readiness, cwd/PATH and, where supported, native
execution start/done. Authored commands use the shell's ordinary line editor,
command execution and history; no eval/Invoke-Expression wrapper is submitted.
The one-time POSIX hook installation is fail-open; PowerShell hooks are installed
through its startup command, after the user's profile.

After a successful shell-ready submission, Kea retains a typed authored-input
metadata event. Only a matching scoped native-start marker creates a block;
matching done finishes it. Missing hooks or exhausted retention degrade to
untracked execution. Nested shell reports must not finish an outer command.
Ordinary terminal keystrokes are not recorded as authored submissions. Directly
typed terminal commands remain in raw output, but do not acquire composer blocks.

Kea currently has a flat block observer, not a nested command tree. If an outer
command remains tracked (for example a composed SSH launch), inner commands may
execute without separate blocks. This never disables Submit or completion.
PowerShell hooks report success/failure (0/1), not every native exit-code value.

## Directory and completion

Shell cwd is current only at a reported ready shell prompt, otherwise last
reported. Prompt text is never parsed. Local filesystem suggestions use cwd/PATH
only when the original local shell reports ready. Remote directory strings are
never interpreted on the host filesystem.

Terminal Tab remains the active application's native completion. Composer Tab
uses a configured cooperating provider, or explicitly labelled local suggestions
at a local prompt. No provider means no remote/TUI composer completion; switch
focus for native Tab. The provider transport does not itself implement Bash,
PowerShell, or OpenCode's completion machinery. See [Active input](active-input.md)
for configuration, protocol, limits and installation in nested shells.

Completion work runs off the UI thread, one request at a time. A result must
match the current draft, UTF-8 cursor, input context generation and focus. It is
applied as an ordinary undoable replacement. IME composition takes precedence.

With the menu open, Tab/Shift+Tab and Left/Right cycle candidates. Up/Down use
measured visual rows rather than an assumed grid width. Enter accepts without
execution; Escape dismisses. Editing, cursor changes and blur invalidate results.

## Recording and trust

New recordings use v2: raw terminal output, resizes and lifecycle retain their
order and exact bytes; authored shell submissions are separate metadata events.
Both v1 and v2 are readable. Earlier releases cannot read v2. Terminal replay
ignores submission events; the block observer may use them to reconstruct blocks.

Markers are interoperability, not authentication. Terminal programs can forge
metadata. Provider endpoints and credentials must be explicitly configured
locally; terminal output never installs a provider or supplies a network address.
Input and output can contain secrets; persisted history/recordings are plaintext.

## Viewport and scrolling

PTY rows/columns are derived from the actual laid-out terminal canvas instead of guessed toolbar/editor heights. Dragging the terminal/command-draft divider or opening the block inspector therefore remeasures the PTY and cannot silently hide the terminal's last row.

The primary terminal screen retains up to 10,000 visual scrollback lines. Scrolling changes the emulator viewport without pausing the PTY or replay timeline. New output follows normally at the tail; while the reader is above the tail it stays there until **Return to bottom** or new terminal input explicitly returns it. Drag selection is viewport-aware, and selection copy never falls back to copying the whole screen.

Ordinary local selection and scrollback apply when the child has not requested mouse reporting. While reporting is active, vertical wheel input is forwarded with the negotiated legacy, UTF-8 or SGR encoding; Shift+wheel remains local scrollback. Primary-button gestures latch ownership at press. Shift+drag selects locally by default, including suppressing Shift-modified hover while the pointer is positioned; `shift_mouse_selects_locally = false` forwards that pointer input. Select text (header button or F4 outside terminal focus) provides explicit entry. Alt-drag creates a block selection when locally owned. Unreserved child motion follows DECSET 1002/1003 and current modifiers. Forwarded hover reports preserve local selection and reading position.

Ordinary output preserves retained selections. If selected text changes or the
engine invalidates a range, a visible recovery caret and feedback replace it;
caret-only interaction exits on focus loss. See the complete
[terminal text selection contract](terminal-text-selection.md).

New block views reveal their tail after layout without stealing focus. Focused/selected read-only snapshots are not replaced beneath a reader; explicit refresh remains available.

## Windows

The GUI executable uses the Windows subsystem, avoiding an intentionally allocated companion console. The default PowerShell launch keeps the user's profile enabled (`-NoLogo -NoExit`), so profile-defined prompt/completion behavior remains available. ConPTY remains the process transport.

This does not bypass SmartScreen/application-control policies, and unsigned development builds are not release installers.

## Validation boundary

Compilation/unit tests, graphical acceptance and real OS testing are separate evidence. Important ongoing compatibility targets include Windows + OpenCode, actual macOS/Windows IMEs, Wayland/IBus/Fcitx, terminal mouse-protocol forwarding, extended keyboard protocol negotiation, accessibility and long-session scrollback/selection behavior.

Window-level ownership and isolation are specified in [Terminal tabs](terminal-tabs.md).
