---
title: Command metadata
nav_order: 10
---

# Optional command metadata protocol

Blocks observe execution in one normal PTY; missing or malformed metadata never
prevents input. Readiness belongs to the separate live
[active-input context](active-input.md), not the retained block model. Prompt
text, cursor positions and idle time are never used to find execution boundaries.

## Native scoped boundaries

Authored text follows the normal terminal input and shell execution path. Shell
hooks observe native start and completion without per-command eval wrappers:

```text
ESC ] 777;kea;native-start;<context> BEL
ESC ] 777;kea;native-done;<context>;<signed-status> BEL
```

Context identifies a receiver within the session. Local, nested and remote
integrations use the same convention. Bash/zsh report integer shell status;
PowerShell reports success/failure (0/1), not exact native exit codes. Hooks must
preserve the status seen by the user's next command and original prompt.

After a successful ready-shell composer submission, Kea records a separate
`Submitted { id, context, input }` event. A matching native-start can turn that
pending observation into a block; a matching native-done finishes it. The
submitted text is not carried by the native marker and never needs to be hidden
from the shell's line editor. Output remains the raw terminal stream.

A prompt without a matching start clears a pending observation rather than
creating a stuck execution queue. An unscoped legacy prompt cannot finish an
active native block: a remote prompt may appear inside an outer command.
The current document model is flat, so an outer tracked command can contain
untracked nested input. That limitation does not restrict submission or completion.
Direct terminal keystrokes are not recorded as composer submissions.

## Legacy markers

These remain accepted for existing recordings and cooperating legacy producers:

```text
ESC ] 777;kea;start;<id>;<base64-utf8-command> BEL
ESC ] 777;kea;done;<id>;<signed-exit-status> BEL
```

ID is an unsigned 64-bit command identifier. Input is bounded to 64 KiB and encoded
as Base64 UTF-8 to avoid newline/semicolon ambiguity. Everything between a valid
start and its matching done belongs to the output block; bytes outside active
blocks remain ordinary terminal output. Kea no longer sends the historical
shell wrappers that produced these markers.

## Streaming and retention

PTY reads are arbitrary chunks. A marker can span reads or share a read with
other output. `kea-document` uses a streaming scanner with bounded pending data.
Unknown or malformed command-marker-looking bytes fail open as ordinary output.
This differs from malformed readiness reports: those fail closed to unknown
input state, since they govern confirmation rather than output retention.

Command text, output per block, aggregate retained bytes and block count all have
limits. Exhaustion or missing boundaries degrades observation; it never cancels,
queues or prevents a terminal write. A block can end Aborted when the session ends
without a matching completion report.

## Recording and trust

v1 stores legacy markers inside output frames. v2 adds separate Submitted events
while preserving raw output. Both formats remain readable. Terminal replay
ignores submission metadata, while a document-aware reader may reconstruct blocks.
See the [recording format](recording-format.md) for framing and validation.

OSC metadata is interoperability, not authentication. Any terminal program can
forge these messages. They do not establish trusted command provenance or grant
permission to launch a process, install a provider or contact a supplied address.
Authored commands and output may contain secrets; persistence is explicit,
bounded, plaintext and non-overwriting.
