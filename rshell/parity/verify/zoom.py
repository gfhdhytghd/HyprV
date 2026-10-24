#!/usr/bin/env python3
"""Zoomed reference / actual / heat stack of one logical region (triage aid).

    python3 parity/verify/zoom.py bar network-offline--dark@2.png 2700 0 372 70 out.png [zoom]
x/y/w/h are logical px in output coordinates (island: canvas coordinates); crops are
translated through the manifest's crop_logical.
"""
import json
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[2]


def main():
    component, file, x, y, w, h, out = sys.argv[1:8]
    zoom = int(sys.argv[8]) if len(sys.argv) > 8 else 2
    scale = float(file.rsplit("@", 1)[1][:-4])
    ref = component.removesuffix("-gl")  # out/<c>-gl slots diff against reference/<c>
    # x/y are output (or canvas) logical coordinates; subtract the capture's crop origin
    manifest = json.loads((ROOT / "parity/reference" / ref / "manifest.json").read_text())
    entry = next((e for e in manifest["images"] if e["file"] == file), {})
    ox, oy = (entry.get("crop_logical") or [0, 0])[:2]
    x, y = float(x) - ox, float(y) - oy
    box = tuple(int(round(float(v) * scale)) for v in (x, y, x + float(w), y + float(h)))
    rows = []
    for path in (ROOT / "parity/reference" / ref / file,
                 ROOT / "parity/verify/out" / component / "actual" / file,
                 ROOT / "parity/verify/out" / component / "heat" / file):
        img = Image.open(path).convert("RGBA")
        bg = Image.new("RGBA", img.size, (127, 140, 153, 255))
        img = Image.alpha_composite(bg, img).crop(box)
        rows.append(img.resize((img.width * zoom, img.height * zoom), Image.NEAREST))
    sheet = Image.new("RGB", (rows[0].width, sum(r.height for r in rows) + 8), (30, 30, 30))
    yy = 0
    for r in rows:
        sheet.paste(r.convert("RGB"), (0, yy))
        yy += r.height + 4
    sheet.save(out)


if __name__ == "__main__":
    main()
