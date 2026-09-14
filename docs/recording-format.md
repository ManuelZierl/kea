# Recording format v1

All integers are little-endian. Output is opaque bytes, not necessarily UTF-8.

Header: magic bytes `4b 45 41 01 0d 0a 1a 0a`, followed by columns:u16 and rows:u16. Dimensions are columns 2..512 and rows 1..256 inclusive.

Each frame is `body_length:u32 | timestamp_us:u64 | kind:u8 | payload | crc32:u32`.

The body length includes timestamp, kind and payload, excluding the length prefix and checksum. IEEE CRC-32 covers the body, with polynomial `0xedb88320` and initial/final complement. It detects corruption, not malicious modification.

Kinds: 0 = output (1 byte to 1 MiB); 1 = resize (columns:u16,rows:u16); 2 = exit (code:u32).

Timestamps are monotonic session-relative microseconds. Equal timestamps preserve file order. No events may follow an exit. Interrupted capture or window closure may leave a valid recording without an exit event.

Readers reject unknown versions/kinds, invalid lengths/dimensions, backwards timestamps, events after exit, corrupt checksums and quota violations. Only an incomplete final length/body/checksum is recoverable as a prefix with `truncated_tail=true`. EOF at a frame boundary is valid.

## Structured Document metadata

The v1 frame schema has no separate command/input frame type. Document-mode command boundaries are encoded as Kea-private OSC messages inside ordinary output frames. A start marker carries command ID + Base64 UTF-8 command text; a completion marker carries ID + exit status. See `document-protocol.md`.

This means raw terminal output remains the canonical recording and older v1 readers can still treat the marker bytes as terminal output. A document-aware reader can derive persistent command/output blocks from the same file without re-executing commands.

Raw keyboard events, environment snapshots and working-directory metadata are not stored as dedicated v1 fields. Output itself can contain secrets, and Document markers explicitly contain submitted command text. There are no snapshots, fonts, external resources, authentication or encryption fields.

Future format versions should add an emulator/config fingerprint before claiming equivalent rendering across engines. If richer structured metadata eventually becomes canonical rather than derived, it should be introduced as a versioned format change rather than silently overloading existing frame kinds.
