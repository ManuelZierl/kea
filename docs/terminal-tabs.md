---
title: Terminal tabs
nav_order: 15
---

# Independent terminal tabs

Each tab owns a PTY/process, live and historical emulator, recording, blocks,
shell/input metadata, composer editor/undo, recall cursor and scratch draft,
completion request, reverse-search view and focus state. Switching never
recreates the session or reruns a command. Hidden sessions continue consuming
output, replying to terminal protocols and writing explicit recordings.

Use **+** or **Ctrl+Shift+T** to open a fresh local shell. New tabs start in
Kea's launch directory; Kea never changes its process-wide cwd or guesses a
remote directory. The initial CLI command and --record path apply only to the
initial tab. A new terminal does not repeat a command, SSH login or recording.

Initial and subsequent local tabs use the same shell-based labels: Shell,
PowerShell, or Terminal, followed by a stable tab number. The title bar shows
the running Kea version.

## tmux sessions

The **tmux** button opens a visual manager for sessions in the local tmux server.
Kea asks tmux for its session list instead of inferring session state from terminal
output. The list shows the tmux session name, window count and attached-client
count. **Open** starts a normal Kea terminal tab whose child process is
`tmux attach-session`; **Kill** is a separate confirmed action against the
persistent tmux session.

A tmux session is not owned by the Kea tab attached to it. Closing that tab ends
only Kea's tmux client and leaves the tmux session running. Closing the whole Kea
window has the same detach-only behavior for tmux-backed tabs. To end the
persistent session for all clients, use **Kill** in the tmux manager. Kea targets
tmux's stable session ID for attach/kill operations while displaying the current
session name.

This first manager intentionally covers the **local default tmux server** only.
Native tmux management is supported on Unix. On Windows the manager reports
that limitation; it does not connect to WSL or a remote tmux server.
An SSH process typed or launched in a terminal remains ordinary terminal input;
Kea does not scrape it to discover a remote host or remote tmux state. Remote
SSH-backed tmux management should compose an explicit host/connection resource
with this tmux-session model rather than guessing from terminal bytes.

From composer/chrome, **Ctrl+Tab / Ctrl+Shift+Tab** switch terminals,
**Ctrl+Shift+W** closes one, and **Ctrl+Shift+PageUp / PageDown** reorder it.
Tabs also support clicking, close buttons and drag reordering. Shortcuts are
semantic and configurable in Settings (`new_terminal`, `close_terminal`,
`next_terminal`, `previous_terminal`, `move_terminal_left`,
`move_terminal_right`). While a live child owns focus, these keys remain
child input. Use the existing Ctrl/Cmd+L escape first, or the visible buttons.

Closing a running terminal, nonempty draft or unsaved recording requires
confirmation. Closing the window confirms all affected terminals. A tab close
drops only its owned session and stops its pump; other sessions keep running.
Closing the final tab leaves an empty workspace with a New terminal button.
Reordering and confirmation use stable IDs, never potentially stale indices.
At most 32 tabs can be open to bound aggregate resource retention.

Settings and explicitly saved input memories are workspace-wide. Composer
recall/navigation stays tab-local; opted-in draft-history persistence uses
one shared archive writer so interleaved submissions cannot overwrite each
other's file snapshots. Prior persisted entries seed each tab. Persistence
changes still take effect on restart. Nothing enables disk history implicitly.

Changing tabs cancels pending submission confirmation and completion; a late
completion or deferred focus cannot target a different terminal. Draft text,
undo, history scratch and scrollback remain owned by their original tab.
Only the visible terminal is resized from actual canvas geometry; hidden
terminals retain their last nonzero dimensions.

This change provides tabs, not split panes or process restoration after exit.
Tab/context ownership is independent of layout, allowing future splits without
moving session state into a shared mutable terminal singleton.

## Acceptance checks

Open two tabs; run a long-lived command in one and edit a distinct draft in
the other. Switch repeatedly and verify ongoing output, independent undo and
recall, TUI state, terminal selection and per-tab Save/History controls.
Reorder with mouse and keyboard; close a background tab and cancel/confirm
close prompts. Check quitting, closing the final tab, opening after that,
completion finishing after a switch, and launch failure preserving all tabs.
Verify native TUI keys before using Ctrl/Cmd+L and the workspace shortcuts.
Test Windows PowerShell/SSH/OpenCode and actual platform IMEs separately from
Rust unit/component tests. No session persistence or remote cwd inference is
implied by successful graphical smoke tests.

## Composer sections and guarded interrupts

See [Composer sections and actions](composer-workflow.md) for independently
submitted drafts, opt-in Ctrl-C confirmation and explicitly accepted pattern
recommendations. A section is pending input for the existing receiver, not a
separate shell or an execution queue.
