#!/usr/bin/env python3
"""hot.py qt.png ours.png [threshold] — cluster pixels whose premultiplied delta exceeds threshold."""
import sys
sys.path.insert(0, "/home/wilf/.config/HyprV/rshell/parity/tools")
from pngstat import read_png
w, h, A = read_png(sys.argv[1]); _, _, B = read_png(sys.argv[2])
t = int(sys.argv[3]) if len(sys.argv) > 3 else 24
cells = {}
for y in range(h):
    for x in range(w):
        a = A[y][x*4:x*4+4]; b = B[y][x*4:x*4+4]
        pa = [a[i]*a[3]/255 for i in range(3)] + [a[3]]; pb = [b[i]*b[3]/255 for i in range(3)] + [b[3]]
        d = max(abs(pa[i]-pb[i]) for i in range(4))
        if d > t:
            k = (x//24, y//24); c = cells.setdefault(k, [0, 0, x, y]); c[0] += 1; c[1] = max(c[1], d)
for k, (n, m, x, y) in sorted(cells.items(), key=lambda kv: -kv[1][0])[:15]:
    print(f"cell x{k[0]*24}-{k[0]*24+23} y{k[1]*24}-{k[1]*24+23}: {n} px, max {m:.0f} (e.g. {x},{y})")
