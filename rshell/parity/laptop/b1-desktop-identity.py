#!/usr/bin/env python3
"""Gate B1 (docs/verify/laptop-qa-plan.md §6): desktop references must not change.

  python3 -I parity/laptop/b1-desktop-identity.py RECAPTURE_DIR [PUBLISHED_DIR]

RECAPTURE_DIR holds a fresh `PARITY_OUT=... parity/capture.sh` run made with the
extended (laptop-capable) mocks; PUBLISHED_DIR defaults to parity/reference. Every
image must be byte-identical, except images whose manifest entry has
distance_field_text: true, which may differ by <= 2 per premultiplied channel (the
harness's documented run-to-run noise). Missing or extra images fail.
"""
import json
import sys
from pathlib import Path

from PIL import Image


def premultiplied(path):
    image = Image.open(path).convert("RGBA")
    px = image.tobytes()
    out = bytearray(len(px))
    for i in range(0, len(px), 4):
        a = px[i + 3]
        out[i] = (px[i] * a + 127) // 255
        out[i + 1] = (px[i + 1] * a + 127) // 255
        out[i + 2] = (px[i + 2] * a + 127) // 255
        out[i + 3] = a
    return image.size, out


def main(argv):
    fresh = Path(argv[1])
    published = Path(argv[2]) if len(argv) > 2 else Path(__file__).resolve().parents[1] / "reference"
    failures, identical, df_ok = [], 0, 0
    for manifest_path in sorted(published.glob("*/manifest.json")):
        component = manifest_path.parent.name
        manifest = json.loads(manifest_path.read_text())
        df = {e["file"]: e.get("distance_field_text", False) for e in manifest["images"]}
        new_dir = fresh / component
        new_files = {p.name for p in new_dir.glob("*.png")} if new_dir.is_dir() else set()
        for extra in sorted(new_files - set(df)):
            failures.append(f"{component}/{extra}: not in the published set")
        for name, has_df in sorted(df.items()):
            old, new = published / component / name, new_dir / name
            if not new.is_file():
                failures.append(f"{component}/{name}: missing from the recapture")
                continue
            if old.read_bytes() == new.read_bytes():
                identical += 1
                continue
            if not has_df:
                failures.append(f"{component}/{name}: bytes differ (no distance-field text)")
                continue
            (size_a, a), (size_b, b) = premultiplied(old), premultiplied(new)
            if size_a != size_b:
                failures.append(f"{component}/{name}: size {size_a} vs {size_b}")
                continue
            worst = max(abs(x - y) for x, y in zip(a, b))
            if worst > 2:
                failures.append(f"{component}/{name}: distance-field image max delta {worst} > 2")
            else:
                df_ok += 1
    print(f"B1: {identical} byte-identical, {df_ok} within DF tolerance, {len(failures)} failures")
    for line in failures[:50]:
        print("  " + line)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
