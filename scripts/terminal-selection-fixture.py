"""Stable, repeated terminal rows and raw input capture for selection smoke tests."""
import os
import signal
import sys
import tty


tty.setraw(0)
row = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"


def paint(*_):
    columns, rows = os.get_terminal_size(1)
    output = bytearray(b"\x1b[2J")
    for line in range(1, rows + 1):
        output.extend(f"\x1b[{line};1H".encode() + row[:columns])
    output.extend(b"\x1b[H")
    os.write(1, output)


signal.signal(signal.SIGWINCH, paint)
if "--mouse" in sys.argv[2:]:
    os.write(1, b"\x1b[?1002h\x1b[?1006h")
paint()
with open(sys.argv[1], "wb", buffering=0) as output:
    while True:
        try:
            data = os.read(0, 4096)
        except OSError:
            break
        if not data:
            break
        output.write(data)
