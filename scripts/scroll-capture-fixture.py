#!/usr/bin/env python3
"""Full-screen fixture for scroll capture: an alternate-screen chat view with a
fixed header, input box and side panel. Only the wheel scrolls the transcript,
three lines per notch, like typical agent TUIs. Writes received input bytes to
argv[1] so tests can prove capture does not inject input."""
import os
import re
import select
import shutil
import sys
import termios
import tty

LINES = [f"message {i:03}: transcript text ä {'·' * (i % 7)}" for i in range(1, 151)]
SIDE = 12
received = open(sys.argv[1], "wb", buffering=0)


def draw(top: int, rows: int, columns: int) -> None:
    body = rows - 2
    out = ["\x1b[?2026h\x1b[H"]
    out.append("== chat (scroll capture fixture) ==".ljust(columns)[:columns] + "\r\n")
    for row in range(body):
        side = (f" side {row:02}" if row < 5 else "").ljust(SIDE - 1) + "│"
        index = top + row
        text = LINES[index] if 0 <= index < len(LINES) else ""
        out.append((side + " " + text).ljust(columns)[:columns] + "\r\n")
    out.append("> input box".ljust(columns)[:columns])
    out.append("\x1b[?2026l")
    sys.stdout.write("".join(out))
    sys.stdout.flush()


def main() -> None:
    fd = sys.stdin.fileno()
    saved = termios.tcgetattr(fd)
    tty.setraw(fd)
    sys.stdout.write("\x1b[?1049h\x1b[?25l\x1b[?1000h\x1b[?1006h")
    try:
        columns, rows = shutil.get_terminal_size()
        body = rows - 2
        top = max(0, len(LINES) - body)
        draw(top, rows, columns)
        pending = b""
        while True:
            ready, _, _ = select.select([fd], [], [], 0.2)
            size = shutil.get_terminal_size()
            if size != (columns, rows):
                columns, rows = size
                body = rows - 2
                draw(top, rows, columns)
            if not ready:
                continue
            data = os.read(fd, 1024)
            if not data:
                break
            received.write(data)
            pending += data
            for button, _, _ in re.findall(rb"\x1b\[<(\d+);(\d+);(\d+)[Mm]", pending):
                if button == b"64":
                    top = max(0, top - 3)
                elif button == b"65":
                    top = min(max(0, len(LINES) - body), top + 3)
            pending = pending[pending.rfind(b"\x1b") :] if b"\x1b" in pending else b""
            if b"q" in data:
                break
            draw(top, rows, columns)
    finally:
        sys.stdout.write("\x1b[?1006l\x1b[?1000l\x1b[?25h\x1b[?1049l")
        sys.stdout.flush()
        termios.tcsetattr(fd, termios.TCSADRAIN, saved)


main()
