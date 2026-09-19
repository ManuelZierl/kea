---
title: Recording format
nav_order: 11
---

# Recording formats v1 and v2

The writer emits v2. Readers accept v1 and v2; earlier Kea releases cannot read
v2 files. All integers are little-endian. Output is opaque bytes, not necessarily
UTF-8.

The 12-byte header is magic `4b 45 41 <version> 0d 0a 1a 0a`, followed by
columns:u16 and rows:u16. Version is `01` or `02`. Dimensions are columns 2..512
and rows 1..256 inclusive.

Each frame is `body_length:u32 | timestamp_us:u64 | kind:u8 | payload | crc32:u32`.
The body length includes timestamp, kind and payload, excluding the length prefix
and checksum. IEEE CRC-32 covers the body, with polynomial `0xedb88320` and
initial/final complement. It detects corruption, not malicious modification.

## Event kinds

| Kind | Versions | Payload |
| --- | --- | --- |
| 0: output | 1, 2 | 1 byte to 1 MiB of raw terminal output |
| 1: resize | 1, 2 | columns:u16, rows:u16 |
| 2: exit | 1, 2 | code:u32 |
| 3: submitted input | 2 | id:u64, context_length:u16, context bytes, authored UTF-8 input |

Submission context is 1–128 ASCII letters, digits, hyphens, underscores or dots.
Input is nonempty valid UTF-8, at most 64 KiB, with no NUL. Its length is the
remaining payload after the context. There is no padding or input-length field.
Submission is metadata for a successful ready-shell send, not a raw keystroke
record and never an instruction to replay into a process. It is independently
bounded and counted against recording quotas.

Timestamps are monotonic session-relative microseconds. Equal timestamps preserve
file order. No events may follow an exit. Interrupted capture or window closure
may leave a valid recording without an exit event.

Readers reject unknown versions/kinds, invalid lengths/dimensions/UTF-8 metadata,
backwards timestamps, events after exit, corrupt checksums and quota violations.
Kind 3 is rejected under a v1 header. Only an incomplete final length/body/checksum
is recoverable as a prefix with `truncated_tail=true`. EOF at a frame boundary is
valid. Accounted retention is limited to 32 MiB or 100,000 events; it is not a
promise about identical file size or resident memory.

## Optional command metadata and replay

v1 has no input frame type. Legacy command markers carry an ID and Base64 UTF-8
command inside ordinary output frames. They remain readable for compatibility.
In v2, authored input is a separate Submitted event; native shell hooks supply
scoped start/done markers without carrying the command. See the
[command metadata protocol](document-protocol.md).

Raw output is preserved, not replaced by synthetic markers or rendered text.
Terminal replay ignores Submitted events; the optional document observer uses
them to correlate native boundaries. Playback never evaluates or submits input.
No provider requests, terminal replies or external side effects arise from
historical metadata.

Output and authored-input metadata can contain secrets. Recordings are plaintext,
bounded and create-only. There are no raw keyboard, environment snapshot, font,
external resource, authentication or encryption fields. Cwd and other shell
reports may exist inside output. Future checkpoints require complete parser and
emulator state; the format currently has no emulator/config fingerprint that
would guarantee equivalent rendering across engines.
