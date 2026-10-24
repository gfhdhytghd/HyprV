#!/usr/bin/python3 -I
"""Markdown matrix from compare.py + residual.py output (RUN/compare/residual.json).

Cause tags come from where the residual (tray strip masked) differences sit:
  R1  tray strip still differs (draw stage: 32→36 stretch, QtSvg vs Skia SVG raster)
  T2  tray overflow grid order / content
  B1  Wi-Fi trigger highlight (bar, logical x 2828)
  I3  island expanded transport glyphs (logical 1528, 144)
  A1  control-panel open/close mid-frames;  A2  island→calendar morph mid-frames
  S1  ≤ 12/255 icon antialiasing on a few px
"""
import collections
import json
import sys
from pathlib import Path

run = Path(sys.argv[1])
rows = json.loads((run / "compare/residual.json").read_text())


def causes(r):
    c = []
    g, rg = r["gate"], r.get("residual_gate", {})
    if g["pass"]:
        return c
    if g["max_channel_diff"] > 8 and r["state"] != "tray-overflow":
        c.append("R1")
    s, f = r["state"], r["frame"]
    boxes = [(x["box_logical"], x["px"]) for x in r.get("residual_regions", [])]
    near = lambda x0, y0, x1, y1: any(b[0] < x1 and b[2] > x0 and b[1] < y1 and b[3] > y0 and n >= 50 for b, n in boxes)
    if s == "tray-overflow":
        c.append("T2")
    if near(2820, 0, 2860, 50):
        c.append("B1")
    if near(1500, 130, 1600, 170) and s == "island-expanded":
        c.append("I3")
    if s == "control-open-close" and f in ("open-t0060", "open-t0120", "open-t0220", "close-t0060"):
        c.append("A1")
    if s == "calendar-morph" and f in ("open-t0016", "open-t0100", "close-t0100"):
        c.append("A2")
    if not rg.get("pass") and not set(c) - {"R1"} and rg.get("over8_px", 0) <= 24:
        c.append("S1")
    return c


table = collections.OrderedDict()
for r in rows:
    if "gate" in r:
        table.setdefault((r["state"], r["frame"]), {})[r["theme"]] = r
print("| state | frame | light max / MAE / IoU | dark max / MAE / IoU | tray strip masked (light, dark) | causes |")
print("|---|---|---|---|---|---|")
for (s, f), t in table.items():
    def cell(th):
        r = t.get(th)
        if r is None:
            return "—"
        g = r["gate"]
        return f"{'PASS' if g['pass'] else 'FAIL'} {g['max_channel_diff']} / {g['painted_mae']:.3f} / {g['painted_iou']:.4f}"
    def res(th):
        r = t.get(th)
        if not r:
            return "—"
        g = r["residual_gate"]
        return f"{'**pass**' if g['pass'] else 'fail'} {g['max_channel_diff']}/{g['painted_mae']:.3f}"
    cs = sorted({c for r in t.values() for c in causes(r)})
    print(f"| {s} | {f or '—'} | {cell('light')} | {cell('dark')} | {res('light')}, {res('dark')} | {', '.join(cs)} |")
