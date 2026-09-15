"""Raw PTY peer used only by the graphical acceptance test."""
import os
import sys
import tty

tty.setraw(0)
os.write(1, b"\x1b[?2004hRaw terminal fixture: editor remains available below.\r\n")
with open(sys.argv[1], "wb", buffering=0) as output:
    while True:
        try:
            data = os.read(0, 4096)
        except OSError:
            break
        if not data:
            break
        output.write(data)
        os.write(1, data.hex().encode() + b"\r\n")
