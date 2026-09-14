# Recording format v1

All integers are little-endian. Output is opaque bytes, not necessarily UTF-8.

Header: magic bytes `4b 45 41 01 0d 0a 1a 0a`, followed by columns:u16 and rows:u16. Dimensions are columns2..512 and rows1..256 inclusive.

Each frame is `body_length:u32 | timestamp_us:u64 | kind:u8 | payload | crc32:u32`.

The body length includes timestamp, kind and payload, excluding the length prefix and checksum. IEEE CRC-32 covers the body, with polynomial0xedb88320 and initial/final complement. It detects corruption, not malicious modification.

Kinds: 0 = output (1 byte to1MiB); 1 = resize (columns:u16,rows:u16); 2 = exit (code:u32).

Timestamps are monotonic session-relative microseconds. Equal timestamps preserve file order. No events may follow an exit. Interrupted capture or window closure may leave a valid recording without an exit event.

Readers reject unknown versions/kinds, invalid lengths/dimensions, backwards timestamps, events after exit, corrupt checksums and quota violations. Only an incomplete final length/body/checksum is recoverable as a prefix with truncated_tail=true. EOF at a frame boundary is valid.

No keyboard, environment or command metadata is explicitly stored; such text can still appear in output. There are no snapshots, fonts, external resources or encryption fields. Future versions should add an emulator/config fingerprint before claiming equivalent rendering across engines.
