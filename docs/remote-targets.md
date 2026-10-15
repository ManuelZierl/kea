---
title: Remote targets (proposal)
nav_order: 21
---

# Remote targets: SSH and tmux as first-class concepts

**Status: proposal, not implemented.** It builds on the tmux manager and SSH
discovery from alpha.4 ([terminal tabs](terminal-tabs.md)).

## Problem

Today a tab is either a plain process or a tmux client
(`TerminalBacking::{Process, Tmux { session_name, host }}`). A host is either
local or one validated SSH operand (`TmuxHost { destination: Option<String> }`).
Users really work in chains:

- `local shell`
- `local tmux`
- `ssh work`
- `ssh work → tmux main`
- `ssh bastion → ssh db1 → tmux ops`
- `ssh work → tmux main → (inside a pane) ssh db1`

Kea should be able to say, open, reconnect and kill these explicitly. It must
still never infer them from terminal output (AGENTS.md: no scraping, and markers
are not authentication).

## Model: a launched route plus an observed tail

Each tab owns two separate things:

1. **`launched: Route`**: what Kea itself started. It comes only from explicit
   user actions (tmux manager, a New SSH tab, the CLI). It is authoritative:
   Open, Kill, Reconnect and Restore use only this.
2. **`observed: ObservedRoute`**: hops reported later by cooperating shell
   integration that the user installed on a remote host. It is advisory, shown
   muted with a "reported" label, and can never launch, attach, kill, choose a
   completion provider or enable local filesystem access.

```rust
struct SshDestination(String);              // today's validated operand
enum Host { Local, Ssh { jumps: Vec<SshDestination>, dest: SshDestination } }
struct TmuxRef { host: Host, id: String, name: String, guard: TmuxGuard }
enum Hop { Ssh(SshDestination), Tmux(TmuxRef) }
struct Route(Vec<Hop>);                     // launch invariant: Ssh* Tmux?
enum Leaf { LocalShell, RemoteLogin, Command, TmuxClient }
struct TabTarget { launched: Route, leaf: Leaf, observed: ObservedRoute }
```

How the example chains map:

| Chain | `launched` | `observed` |
| --- | --- | --- |
| local shell | `[]`, `LocalShell` | none |
| local tmux | `[Tmux main@Local]` | none |
| ssh work | `[Ssh work]`, `RemoteLogin` | none |
| ssh work → tmux main | `[Ssh work, Tmux main]` | none |
| bastion → db1 → tmux | `[Ssh bastion, Ssh db1, Tmux ops]`, launched as `ssh -J bastion db1` | none |
| ssh work → tmux → ssh db1 (typed) | `[Ssh work, Tmux main]` | `db1 (reported)`, only with remote integration |

The last row is the key point about nesting. Anything started inside a tmux
pane belongs to tmux's state, not Kea's. Kea can show it if the remote side
reports it, but it can't promise to reopen it. That keeps "Kea launched it" and
"something says it's there" separate, which matches the existing readiness and
cwd rules ("current" vs "last reported").

## UI

- **Breadcrumb in the terminal header** from launched hops:
  `Local › ssh work › tmux main`. Observed hops follow in muted style:
  `› db1 (reported)`, with the tooltip "Reported by shell integration, not
  verified".
- **Tab titles are derived from the target** instead of formatted by hand. The
  last launched hop wins, and an observed context is a suffix.
- **Close and Kill dialogs name the full launched route**, so "Kill tmux main
  on db1 via bastion" can't be confused with a local session.
- The tmux manager's host picker becomes a `Host` picker with optional jump
  hosts. **New SSH tab** opens `ssh -tt [-J …] -- dest` as a `RemoteLogin` leaf.
  The manager opened from that tab preselects its host.

## Reconnect and restore

- **Reconnect** (offered when a tab with a non-empty route ends) rebuilds the
  argv from `launched`. For tmux it lists sessions on the same `Host` again and
  requires an id plus guard match. A same-name session with a different identity
  is offered only as an explicit, confirmed choice. It never falls back to local.
- **Restore** (opt-in setting) persists launched routes only: host strings,
  tmux names and ids. It stores no output and no observed hops, in a bounded,
  owner-only file. At startup restored tabs appear as *Disconnected* with a
  Reconnect button. Kea never connects over SSH automatically.

## Observed hops (later)

The optional marker is `OSC 780;kea;route;1;<context>;<tmux|ssh>;<label>`. The
user-installed `--print-shell-integration` hook emits it, reporting `$TMUX` and
the session name it already knows. It is parsed like the 779 context: bounded,
fail-closed, and stored apart from `InputContext`, so it never affects
`local_shell_ready`.

Caveat: tmux swallows unknown OSC sequences unless `allow-passthrough` is on and
the hook wraps them in `DCS tmux;`. Switching panes does not re-report, so the
display is always "last reported".

## Staged plan

1. **alpha.4/5 (refactor, no new trust surface):**
   - Add `target.rs` with `SshDestination`, `Host` and `Route`, and port
     `TmuxHost` with identical argv and tests.
   - Replace `TerminalBacking` with `TabTarget`.
   - Derive titles and dialogs from it, and render the breadcrumb.
   - Add a wt rule: `Hop`/`Route` must not be constructed in output-parsing
     modules (`terminal/`, `shell_metadata.rs`).
2. **Next:**
   - New SSH tab.
   - Jump hosts in the manager.
   - Reconnect for ended tabs.
3. **Later:**
   - Opt-in restore.
   - Observed-route marker with tmux passthrough.
   - Opt-in Kea-private `ControlPath`, so discovery can reuse an interactively
     authenticated master (password users).
   - tmux new-session and custom sockets.
   - tmux `-CC` control mode, mapping windows to Kea tabs.
   - WSL support.
