#!/usr/bin/env python3
"""Generate deterministic fixture images (no third-party modules)."""
import math
import struct
import sys
import zlib
from pathlib import Path


def write_png(path: Path, width: int, height: int, pixel) -> None:
    rows = bytearray()
    for y in range(height):
        rows.append(0)
        for x in range(width):
            rows.extend(pixel(x, y))

    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(rows), 9))
    png += chunk(b"IEND", b"")
    path.write_bytes(png)


def cover_warm(x: int, y: int):
    # Diagonal orange→magenta gradient with a light disc: exercises crop + mask edges.
    t = (x + y) / 1022
    r, g, b = 250 - 60 * t, 140 - 100 * t, 60 + 150 * t
    d = math.hypot(x - 330, y - 190)
    if d < 110:
        k = 0.75
        r, g, b = r + (255 - r) * k, g + (255 - g) * k, b + (255 - b) * k
    return bytes((int(r), int(g), int(b), 255))


def cover_wide(x: int, y: int):
    # 2:1 image with vertical stripes so PreserveAspectCrop centring is visible.
    stripe = (x // 40) % 2
    base = (40, 90, 160) if stripe else (220, 225, 235)
    return bytes((*base, 255))


def main() -> None:
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    write_png(out / "cover-warm.png", 512, 512, cover_warm)
    write_png(out / "cover-wide.png", 400, 200, cover_wide)


if __name__ == "__main__":
    main()
