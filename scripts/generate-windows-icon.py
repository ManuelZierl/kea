#!/usr/bin/env python3
"""Pack the committed Kea PNG artwork into a deterministic multi-size ICO.

No image library or network is needed. --check rejects stale/missing assets.
"""
import argparse
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[1]
SIZES = (16, 24, 32, 48, 64, 128, 256)

def encode_icon(images: list[tuple[int, bytes]]) -> bytes:
    if not images or len(images) > 65535:
        raise ValueError("ICO requires 1..65535 images")
    offset = 6 + 16 * len(images)
    entries = []
    payloads = []
    for size, data in images:
        if not 1 <= size <= 256 or len(data) < 33 or data[:8] != b"\x89PNG\r\n\x1a\n":
            raise ValueError("invalid PNG icon")
        if data[12:16] != b"IHDR" or struct.unpack_from(">II", data, 16) != (size, size):
            raise ValueError("PNG dimensions do not match icon size")
        entries.append(struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(data), offset))
        payloads.append(data)
        offset += len(data)
    return struct.pack("<HHH", 0, 1, len(images)) + b"".join(entries + payloads)

def generated_icon() -> bytes:
    return encode_icon([(size, (ROOT / f"assets/linux/icons/hicolor/{size}x{size}/apps/kea.png").read_bytes()) for size in SIZES])

def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    path = ROOT / "assets/windows/kea.ico"
    data = generated_icon()
    if args.check:
        if not path.exists() or path.read_bytes() != data:
            raise SystemExit("Windows icon is missing or stale; run scripts/generate-windows-icon.py")
        print("Windows icon matches all seven committed PNG sizes")
    else:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        print(f"Wrote {path} ({len(data)} bytes)")

if __name__ == "__main__":
    main()
