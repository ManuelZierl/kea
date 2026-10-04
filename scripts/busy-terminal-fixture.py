"""Redrawing raw PTY peer for frozen-selection graphical acceptance."""
import os
import select
import sys
import tty
from pathlib import Path

tty.setraw(0)
path = Path(sys.argv[1])
counter_path = path.with_suffix(".tick")
os.write(1, b"\x1b[?1003h\x1b[?1006h\x1b[?25l\x1b[2J")
with path.open("wb", buffering=0) as output:
    counter = 0
    while True:
        counter += 1
        os.write(1, f"\x1b[H   FRAME-{counter:06d}\x1b[K".encode())
        counter_path.write_text(str(counter))
        ready, _, _ = select.select([0], [], [], 0.05)
        if ready:
            try:
                data = os.read(0, 4096)
            except OSError:
                break
            if not data:
                break
            output.write(data)
