"""Verify the distributed PE is a GUI executable, not a console executable."""
import struct
import sys
from pathlib import Path

binary = Path(sys.argv[1]).read_bytes()
pe = struct.unpack_from("<I", binary, 0x3C)[0]
assert binary[pe:pe + 4] == b"PE\0\0", "Not a PE executable"
subsystem = struct.unpack_from("<H", binary, pe + 24 + 68)[0]
assert subsystem == 2, f"Expected Windows GUI subsystem 2, got {subsystem}"
print("Windows GUI subsystem verified; this executable does not allocate a console.")
