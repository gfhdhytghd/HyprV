#!/usr/bin/python3 -I
"""Merge run.py's per-run entries into reference/manifest.json (+ parity-host cross-check).

  - source_sha256 of every *.qml in the source dir (same scheme as parity/capture.sh)
  - distance_field_text per image: taken from parity-host's render of the SAME fixture
    (it can see which Text items use QtRendering inside the output; the real shell can't
    be introspected). parity-host runs unchanged via parity/capture.sh into
    reference/host/ — a mock-services, offscreen-RHI rendering of the same states.
  - host_crosscheck per image: premultiplied max channel delta / differing pixels between
    the nested capture and that parity-host render (evidence, not a gate).
"""
import argparse
import datetime
import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

import numpy as np
from PIL import Image

FS = Path(__file__).resolve().parent.parent
PARITY = FS.parent

UNREACHABLE = [
    {"state": "battery", "reason": "the battery trigger is interactive only while batteryInfo.available, and "
     "battery-info.sh is polled only while the popup is open (shell.qml:3342, 4078-4080); component "
     "references bar/battery-* are harness-seeded"},
    {"state": "control-mihomo", "reason": "reachable only through parity-host `invoke` (ui-panels-control.md)"},
]


def premul(path):
    a = np.asarray(Image.open(path).convert("RGBA")).astype(np.float64)
    a[..., :3] = np.round(a[..., :3] * a[..., 3:4] / 255.0)
    return a


def run_host(out, states, scale):
    """Render the same fixtures with parity-host (spec-island's harness, unchanged)."""
    host_out = out / "host"
    tmp = out / ".host-fixtures"
    if tmp.exists():
        shutil.rmtree(tmp)
    (tmp / "fullscreen").mkdir(parents=True)
    for s in states:
        shutil.copy(FS / "fixtures" / f"{s}.json", tmp / "fullscreen" / f"{s}.json")
    env = dict(os.environ, PARITY_FIXTURES=str(tmp), PARITY_OUT=str(host_out), PARITY_SCALES=f"{scale:g}")
    r = subprocess.run(["bash", str(PARITY / "capture.sh"), "fullscreen"], env=env, capture_output=True, text=True)
    shutil.rmtree(tmp)
    if r.returncode != 0:
        print(r.stdout[-2000:], r.stderr[-2000:], file=sys.stderr)
        sys.exit("parity-host pass failed")
    return json.loads((host_out / "fullscreen" / "manifest.json").read_text())


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--source", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--skip-host", action="store_true")
    args = ap.parse_args()
    out = Path(args.out)
    entries = []
    for frag in sorted((out / "logs").glob("*.entries.json")):
        entries += json.loads(frag.read_text())
    entries = [e for e in entries if (out / e["file"]).exists()]
    if not entries:
        sys.exit("manifest.py: no captured images")
    outputs = json.loads((FS / "fixtures/outputs.json").read_text())["outputs"]
    host = {}
    if not args.skip_host:
        hm = run_host(out, sorted({e["state"] for e in entries}), outputs[0]["scale"])
        host = {(h["state"], h["frame"], h["theme"]): h for h in hm["images"]}
    for e in entries:
        h = host.get((e["state"], e["frame"], e["theme"]))
        if h:
            e["distance_field_text"] = h["distance_field_text"]
            a = premul(out / e["file"])
            b = premul(out / "host" / "fullscreen" / h["file"])
            if a.shape == b.shape:
                d = np.abs(a - b).max(axis=2)
                e["host_crosscheck"] = {"file": f"host/fullscreen/{h['file']}", "max_delta": int(d.max()),
                                        "differing_px": int((d > 0).sum()), "over_2_px": int((d > 2).sum())}
        elif e.get("distance_field_text") is None:
            e["distance_field_text"] = True  # unknown -> the tolerant gate, flagged
            e["distance_field_text_source"] = "unknown (no parity-host render); assumed true"
    src = Path(args.source)
    manifest = {
        "component": "fullscreen",
        "generated": datetime.datetime.now().astimezone().isoformat(timespec="seconds"),
        "source_dir": str(src),
        "source_sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(src.glob("*.qml"))},
        "pixel_format": "RGBA8 straight alpha PNG, sRGB, transparent background (whole output, no crop)",
        "renderer": "original quickshell (QSG_RENDER_LOOP=basic, live env) on nested Hyprland "
                    "(wayland backend on headless KWin), grim screencopy; alpha from a black/white "
                    "compositor-background matte; virtual clock via LD_PRELOAD vclock",
        "tolerance": "compare premultiplied RGBA; exact, except images with distance_field_text: per-channel <= 2",
        "composition": {"background": "transparent (matte of compositor background_color black/white)",
                        "wallpaper": False, "blur": False, "compositor_animations": False,
                        "layer_order": "entry.layers: Hyprland levels background<bottom<top<overlay, "
                                       "list order within a level = Hyprland render order",
                        "clients": "one fully transparent xdg toplevel provides activeToplevel's title"},
        "outputs": outputs,
        "unreachable": UNREACHABLE,
        "images": sorted(entries, key=lambda e: e["file"]),
    }
    (out / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=1) + "\n")
    print(f"manifest: {len(entries)} images -> {out / 'manifest.json'}")


if __name__ == "__main__":
    main()
