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

The **tmux** button opens a visual manager for sessions in a local or explicitly
chosen SSH host's default tmux server.
Kea asks tmux for its session list instead of inferring session state from terminal
output. The list shows the tmux session name, window count and attached-client
count. **Open** starts a normal Kea terminal tab whose child process is
`tmux attach-session` (over `ssh -tt` for remote hosts); **Kill** is a separate confirmed action against the
persistent tmux session.

A tmux session is not owned by the Kea tab attached to it. Closing that tab ends
only Kea's tmux client and leaves the tmux session running. Closing the whole Kea
window has the same detach-only behavior for tmux-backed tabs. To end the
persistent session for all clients, use **Kill** in the tmux manager. Kea targets
tmux's stable session ID for attach/kill operations while displaying the current
session name.

To manage remote sessions, click **SSH…**, enter an SSH alias or `user@hostname`,
then click **Connect** (or press Enter). Use **Local** to return to this computer.
Kea uses the system `ssh` client and your existing SSH configuration for keys,
ports and jump hosts. No remote Kea installation or shell integration is needed;
the remote host must have `tmux` in its noninteractive command PATH and a
POSIX-compatible command shell. Discovery uses key/agent authentication and
already-trusted host keys, with a ten-second deadline and bounded output. If login
or host verification is needed, connect with `ssh your-alias` in a terminal first,
then retry. Password-only discovery is not supported. Kea does not enable agent
or port forwarding for these connections.

The selected host is shown in the manager, remote tab titles and Kill confirmation.
Every discovered session retains its host as well as its server/session identity;
attach and Kill never fall back to the local server on failure. Opening the manager
from a Kea-created tmux tab selects that tab's host. A typed SSH command in an
ordinary terminal is not detected: Kea does not scrape terminal output or scan
processes/hosts. Host choices remain in memory for the window, not persisted.

The manager currently requires a Unix Kea host; Windows reports that limitation
for both local and SSH management. Custom tmux socket selection and WSL connection
management are not supported.

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

## Composer draft tabs and guarded interrupts

See [Composer draft tabs and actions](composer-workflow.md) for independently
submitted drafts, opt-in Ctrl-C confirmation and explicitly accepted pattern
recommendations. A draft tab is pending input for the existing receiver, not a
separate shell or an execution queue. **Move to ⇄** transfers a draft's text to
another terminal tab, which creates its own editor for it; editor entities,
pending confirmations, completions and recall state are never shared.
