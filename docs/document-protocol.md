# Optional command metadata protocol

Kea's optional command blocks need exact boundaries while retaining a normal PTY
and an ordinary interactive shell. Prompt parsing is forbidden because prompts
are arbitrary, localized and mutable. Blocks observe execution; missing or
invalid metadata never prevents commands from running.

The standalone shell adapter emits private OSC messages into the terminal output
stream for **Run in shell**. **Send to app** and ordinary terminal input do not
add command-boundary wrappers. There is no separate execution mode.

## Start

```text
ESC ] 777 ; kea ; start ; <id> ; <base64-utf8-command> BEL
```

## Completion

```text
ESC ] 777 ; kea ; done ; <id> ; <signed-exit-status> BEL
```

`id` is an unsigned 64-bit command identifier. Command text is Base64-encoded UTF-8 so arbitrary newlines, semicolons and ordinary shell syntax cannot interfere with marker fields. The command is currently limited to 64 KiB.

Everything observed after a valid start marker and before the matching completion marker belongs to that command's output block. Bytes outside an active block remain ordinary terminal output (for example shell prompts).

## Streaming requirements

PTY reads are arbitrary chunks. A marker may be split across any number of reads or share a read with prompts and command output. `kea-document` therefore uses a streaming scanner with bounded pending marker memory rather than assuming one marker per output event.

Unknown or malformed marker-looking bytes fail open as ordinary output. The parser never waits without a bound for a terminator.

## Recording

Markers are not a new `kea-core` frame type. They are ordinary output bytes inside v1 recordings. This keeps the terminal event stream canonical and makes structured document reconstruction optional/derived.

Because the original command is carried by the start marker, an offline reader can rebuild command blocks without access to raw keyboard input and without guessing prompt text.

## Shell wrapper

A shell adapter is responsible for:

1. emitting the start marker;
2. evaluating the submitted editor text in the current interactive shell scope;
3. preserving the resulting status;
4. emitting the completion marker.

The application sends this wrapper as hidden application-owned PTY input. The wrapper text itself is not the user command block.

Adapters cover supported POSIX-style shells and PowerShell. Other shells and
applications remain usable through terminal input and Send to app. Run in shell
requires an integrated-shell adapter and an explicit prompt-ready report; see
the [interaction contract](unified-session.md).

## Trust

OSC 777 here is a Kea-private convention, not an authentication channel. A process that deliberately emits the same syntax can forge markers. The protocol prevents accidental prompt ambiguity; it does not establish trusted provenance for hostile terminal output or imported recordings.
