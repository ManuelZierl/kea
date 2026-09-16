"""PTY peer that negotiates alpha-critical terminal input protocols."""
import os
import sys
import tty


tty.setraw(0)
# Bracketed paste, button-drag tracking, SGR mouse coordinates and Kitty keyboard
# level 1 (disambiguate escape codes). Kea should only use the extended encodings
# after these sequences have been consumed by the terminal emulator.
os.write(
    1,
    b"\x1b[?2004h\x1b[?1002h\x1b[?1006h\x1b[>1u"
    b"compat fixture ready\r\n",
)
with open(sys.argv[1], "wb", buffering=0) as output:
    while True:
        try:
            data = os.read(0, 4096)
        except OSError:
            break
        if not data:
            break
        output.write(data)
