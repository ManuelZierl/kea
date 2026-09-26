---
title: Active input and providers
nav_order: 16
---

# Active input and completion providers

Kea has one terminal receiver, not a local-shell runner plus a different SSH/TUI
send mode. Ctrl+Enter is **Submit**. `run_shell` and `send_application` are retained
configuration aliases; the latter is no longer a separate button or a bypass.

## Submission policy

An explicitly reported **ready, empty, compatible text-input area** allows
immediate submission. Nonempty, busy or unknown input requires confirmation.
The first shortcut sends no bytes. Release it, then press Enter (or Ctrl+Enter)
to send the same draft to the same reported context. Another key cancels and
continues normal editing. Draft changes, focus loss, and a new context report
cancel the pending confirmation. Held-key repetition cannot confirm the first
press. The Submit button uses the same policy.

Text is sent using the existing terminal paste encoder, followed by Enter. That
encoder bounds input, removes ESC/NUL and normalizes newlines. Multiline text
requires negotiated bracketed paste; without it, the draft is preserved with an
error. No shell wrapper is inserted. No implicit Ctrl+C, remote command install,
line clearing, retry, or queued execution occurs. The current `post_submit_focus`
setting controls focus after success.

A confirmation authorizes raw terminal input; it does not promise that an unknown
program will treat it as a command or message. In particular, appending to an
existing terminal line is not equivalent to replacing it. A future rich provider
may implement transactional insertion, but this raw-input fallback does not.

## Explicit receiver reports

The live terminal stream can contain:

```text
ESC ] 779;kea;input;1;<context>;<kind>;<state> BEL
```

`ESC \\` is also accepted as the terminator. Context IDs are 1–128 ASCII letters,
digits, hyphens, underscores or dots. `kind` is `posix`, `powershell`, or `app`.
`state` is `ready`, `nonempty`, `busy`, or `unknown`. A producer must report ready
only for an empty text-input area where paste plus Enter has the expected meaning.
Every report starts a new generation, including repeated reports of the same
values. Any forwarded user input invalidates ready state. Malformed/oversized
reports fail closed; the parser is bounded to 512 bytes.

The same protocol is used locally, remotely and in nested shells. Kea does not
inspect executable names or prompt text to infer readiness. `local` is reserved
for the original integrated shell, allowing local cwd/PATH suggestions. Never
reuse that ID in a remote integration: remote paths must not be opened locally.

These reports are interoperability metadata, **not authentication**. An arbitrary
terminal program can emit them. They do not authorize process launch, configure
endpoints or supply trusted provenance. Connection closure and replay remain
independent hard guards against input.

## Shell integration installation

The initial local shell is integrated automatically. PowerShell receives startup
hooks after its profile, without typing a driver command into PSReadLine. Bash
uses PS0/PROMPT_COMMAND and zsh uses preexec/precmd. Other POSIX shells can report
prompt state but may not expose reliable native command boundaries.

To integrate a nested or remote Bash explicitly, generate the script on the
machine with Kea, inspect it, and install/source it in that shell's interactive
startup configuration:

```bash
kea --print-shell-integration bash remote-work > kea-remote-work.sh
```

Use `zsh`, `pwsh`, or another supported shell name for the corresponding script.
The command prints only the integration; it does not connect, install or execute
anything remotely. A context ID identifies a live receiver within a Kea session;
use different IDs for independently nested receivers. Source the script only in
an intended interactive shell. Repeated sourcing is guarded. Exiting a nested
shell lets the parent report its own context again.

Without integration, SSH and other apps still use Ctrl+Enter with confirmation.
They do not require a different shortcut. Native terminal Tab remains unchanged.
Installing prompt integration alone does **not** install a completion provider.

## Optional structured completion

A cooperating provider can serve native candidates for its actual live receiver.
Kea's transport is implemented; bundled native Bash, PowerShell and OpenCode
provider servers are not. A fresh shell process with a copied cwd is not a
substitute for the active shell's aliases/functions/runtime state.

Set `KEA_COMPLETION_PROVIDERS` to a local JSON file before launching Kea:

```json
{
  "local": { "address": "127.0.0.1:41231", "token": "provider-specific-secret" },
  "remote-work": { "address": "127.0.0.1:41232", "token": "another-secret" }
}
```

Endpoints must be numeric loopback addresses with nonzero ports. For a remote
provider, establish an explicitly authorized tunnel separately. Terminal output
cannot choose an address or enable an endpoint. Protect this configuration as it
may contain credentials; the provider receives the complete draft, which may
contain secrets. The endpoint must enforce its own authentication and avoid
logging draft/token contents. This connection is not encrypted by Kea; the
loopback restriction does not defend against other processes on the same account.

At a ready input context, composer Tab connects once and writes one newline-ended
UTF-8 JSON object (shown expanded here):

```json
{
  "protocol": 1,
  "request_id": 7,
  "context": "remote-work",
  "text": "git che",
  "cursor": 7,
  "token": "another-secret"
}
```

The provider returns one newline-ended object:

```json
{
  "protocol": 1,
  "request_id": 7,
  "context": "remote-work",
  "candidates": [
    {"label": "checkout", "replacement": "checkout", "start": 4, "end": 7}
  ]
}
```

Offsets are UTF-8 byte offsets with an exclusive end, **not** PowerShell UTF-16
positions. Providers must translate native offsets correctly and supply any
required quoting in `replacement`. Candidates are not executed or reparsed.

The transport has a two-second total deadline, a 1 MiB response cap, at most 200
candidates, labels up to 1 KiB and replacements up to 64 KiB. Invalid UTF-8 ranges,
control characters, unknown response fields and mismatched request/context IDs
are rejected. Configuration is capped at 64 KiB. There is one worker/request at a
time, no retry, no shell evaluation and no hidden PTY Tab probe. Result acceptance
also requires unchanged draft/cursor, focus, IME state and context generation.

A configured provider failure remains visible; it does not silently substitute
unrelated local results. Without a provider, only a confirmed original local
shell gets explicitly labelled local history/PATH/cwd suggestions. Unknown,
remote or TUI receivers direct the user to native terminal Tab instead of
pretending local paths belong to them. Menus use Left/Right and Tab/Shift+Tab for
cycling, Up/Down for visual rows, Enter to accept, and Escape to dismiss.

## Observation and recordings

Native shell hooks emit scoped start/done boundaries:

```text
ESC ] 777;kea;native-start;<context> BEL
ESC ] 777;kea;native-done;<context>;<integer-status> BEL
```

These do not contain commands. After a successful ready-shell Submit, Kea can
retain a separate `Submitted` event with authored input, ID and context. A block
is created only when native-start for the same context arrives. Missing hooks
never leave an execution queue or stop future input. The current block observer
is flat, so inner commands may remain untracked while an outer command is active.
PowerShell supplies success/failure (0/1), rather than exact native exit codes.

The recording writer uses v2; both v1 and v2 remain readable. v2 adds frame type 3
(timestamp, type, u64 submission ID, u16 UTF-8 context length, context, authored
UTF-8 input) using the existing length/checksum framing. Raw output is unchanged.
Terminal replay ignores submission frames; block reconstruction observes them.
Older Kea releases cannot read v2 files. Retention is bounded and persistence is
still explicit, plaintext and non-overwriting; authored commands can contain
secrets. Neither this protocol nor metadata enables execution during replay.

## Validation boundaries

Tests cover context parsing/generations, metadata bounds and replay, replacement
ranges and transport round trips, menu geometry, shortcut capture and GUI input
routing. Native shell tests exercise Bash history and status preservation. The
Linux desktop smoke checks actual click/type interaction and guarded submission.
Graphical Windows/PSReadLine, macOS, zsh and real remote/TUI provider acceptance
must be reported separately; compilation is not a substitute for those checks.
