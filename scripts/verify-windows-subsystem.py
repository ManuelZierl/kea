#!/usr/bin/env python3
"""Check the produced PE executable, not just a source-level cfg attribute."""
import pathlib
import struct
import sys

image = pathlib.Path(sys.argv[1]).read_bytes()
assert image[:2] == b'MZ', 'Not a PE executable'
pe = struct.unpack_from('<I', image, 0x3C)[0]
assert image[pe:pe + 4] == b'PE\0\0', 'Invalid PE signature'
optional = pe + 24
assert struct.unpack_from('<H', image, optional)[0] in (0x10B, 0x20B)
subsystem = struct.unpack_from('<H', image, optional + 68)[0]
assert subsystem == 2, f'Expected Windows GUI subsystem (2), got {subsystem}'
print('Verified Windows GUI subsystem: no automatically allocated companion console.')
