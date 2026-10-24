#!/usr/bin/env python3
"""crop.py qt.png ours.png x y w h out.png [zoom] — side-by-side magnified crops (qt | ours | diff)
and a best integer-offset search over the alpha channel."""
import sys, struct, zlib
sys.path.insert(0, "/home/wilf/.config/HyprV/rshell/parity/tools")
from pngstat import read_png

def write_png(path, w, h, rows):
    raw = b"".join(b"\x00" + bytes(r) for r in rows)
    def chunk(t, d): return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
    open(path, "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))

a_path, b_path, x, y, w, h, out = sys.argv[1], sys.argv[2], *map(int, sys.argv[3:7]), sys.argv[7]
z = int(sys.argv[8]) if len(sys.argv) > 8 else 6
aw, ah, A = read_png(a_path); bw, bh, B = read_png(b_path)
def px(R, X, Y):
    if 0 <= Y < len(R) and 0 <= X < len(R[0]) // 4: return R[Y][X*4:X*4+4]
    return b"\0\0\0\0"
def on_white(p):
    a = p[3] / 255
    return [round(c * a + 255 * (1 - a)) for c in p[:3]] + [255]
rows = []
for yy in range(h):
    for _ in range(z):
        row = []
        for src in (0, 1, 2):
            for xx in range(w):
                pa, pb = px(A, x + xx, y + yy), px(B, x + xx, y + yy)
                if src == 2:
                    d = max(abs(pa[i] - pb[i]) for i in range(4)); c = [255, 255 - d, 255 - d, 255]
                else:
                    c = on_white(pa if src == 0 else pb)
                row += c * z
            row += [0, 0, 255, 255] * 2
        rows.append(row)
write_png(out, (w * z + 2) * 3, h * z, rows)
best = None
for dy in range(-3, 4):
    for dx in range(-3, 4):
        s = sum(abs(px(A, x + xx, y + yy)[3] - px(B, x + xx + dx, y + yy + dy)[3]) for yy in range(h) for xx in range(w))
        if best is None or s < best[0]: best = (s, dx, dy)
s0 = sum(abs(px(A, x + xx, y + yy)[3] - px(B, x + xx, y + yy)[3]) for yy in range(h) for xx in range(w))
print(f"alpha L1 at (0,0): {s0}; best offset (ours shifted by) dx={best[1]} dy={best[2]} L1={best[0]}")
