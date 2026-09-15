#!/usr/bin/env python3
"""Raw-PTY acceptance peer. Writes only synthetic test input, never user sessions."""
import os
import pathlib
import sys
import termios
import tty

path = pathlib.Path(sys.argv[1])
fd = sys.stdin.fileno()
saved = termios.tcgetattr(fd)
try:
    tty.setraw(fd)
    path.write_bytes(b'')
    path.with_suffix('.ready').write_text('ready')
    cols, rows = os.get_terminal_size(fd)
    # Cursor clamping and the last visible row are checked in the screenshot.
    os.write(sys.stdout.fileno(), b'\x1b[999;1HKEA_BOTTOM_ROW')
    with path.open('ab', buffering=0) as output:
        while True:
            data = os.read(fd, 4096)
            if not data:
                break
            output.write(data)
finally:
    termios.tcsetattr(fd, termios.TCSANOW, saved)
