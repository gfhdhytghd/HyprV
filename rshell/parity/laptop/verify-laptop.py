#!/usr/bin/env python3
"""Laptop parity diff (docs/verify/laptop-qa-plan.md §6, gate B4).

  python3 parity/laptop/verify-laptop.py --component laptop-bar --actual DIR [--masks FILE]
  python3 parity/laptop/verify-laptop.py --component laptop-panels --actual DIR --regions FILE
  python3 parity/laptop/verify-laptop.py --report-only

DIR holds an rshell replay of parity/laptop/fixtures/<component> with the reference file
names (a UI crate's laptop replay output). Each image is diffed against
parity/laptop/reference/<component> with exactly the desktop gate of
parity/verify/verify.py (`compare`, same metrics, same pass rule); only the reference root
differs. --masks is the renderer's {file: [x, y, w, h]} foreign-island rect map (bar crops
exclude the island slot like the desktop bar gate).

--regions is the inverse: {file: [x, y, w, h]} in logical px (key "*" = default for every
file), and only pixels inside the rect are compared, in both images; everything outside is
zeroed before the gate, so metrics and the structural check see the region alone. This is the
laptop-panels contract (references are whole-shell frames: bar, corner shades and the card;
a panels replay renders only panel surfaces). An actual image that is exactly the region's
crop (round(rect x scale), as hyprv-ui-panels tests/laptop_parity.rs writes `mine-*`) is
placed at the region origin on a transparent full-size canvas first.

Runs merge: several owners replay disjoint subsets of one component (laptop-panels: CP by
ui-panels, QuickAdjust, battery popup by ui-bar). Only files present in DIR are re-diffed;
the other rows keep their last result (or "missing" if never rendered).

Outputs: parity/laptop/verify-out/<component>/{actual,heat,tri}/, results.json, and the
report docs/verify/parity-laptop.md (rebuilt from every component's results.json).
laptop-island-legacy is never gated (D1 = read-only, laptop-island.md §3).
"""
import argparse
import importlib.util
import json
import shutil
import sys
import time
from pathlib import Path

import numpy as np
from PIL import Image

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
OUT = HERE / "verify-out"
REPORT = ROOT / "docs" / "verify" / "parity-laptop.md"
GATED = ["laptop-bar", "laptop-panels"]

_spec = importlib.util.spec_from_file_location("parity_verify", ROOT / "parity/verify/verify.py")
verify = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(verify)
verify.PARITY = HERE  # compare() reads PARITY / "reference" / component


def region_loader(regions):
    """verify.load, keeping only the file's compare region (logical px, scaled by @N)."""
    plain = verify.load

    def load(path):
        pm = plain(path)
        name = Path(path).name
        rect = regions.get(name, regions.get("*"))
        if rect is None:
            return pm
        scale = SCALES.get(name, 1.0)
        x, y, w, h = rect
        x0, y0 = max(0, int(np.floor(x * scale))), max(0, int(np.floor(y * scale)))
        x1, y1 = int(np.ceil((x + w) * scale)), int(np.ceil((y + h) * scale))
        keep = np.zeros(pm.shape[:2], bool)
        keep[y0:y1, x0:x1] = True
        pm[~keep] = 0
        return pm
    return load


SCALES = {}


def run_component(component, actual_src, masks, regions):
    manifest = json.loads((HERE / "reference" / component / "manifest.json").read_text())
    slot = OUT / component
    actual = slot / "actual"
    actual.mkdir(parents=True, exist_ok=True)
    fresh = {f.name for f in Path(actual_src).glob("*.png")}
    sizes = {e["file"]: (float(e["scale"]), e) for e in manifest["images"]}
    for name in fresh:
        for sub in ("heat", "tri"):
            (slot / sub / name).unlink(missing_ok=True)
        src = Path(actual_src) / name
        rect = (regions or {}).get(name, (regions or {}).get("*"))
        if rect and name in sizes:
            ref = Image.open(HERE / "reference" / component / name)
            img = Image.open(src)
            k = sizes[name][0]
            x, y, w, h = (round(v * k) for v in rect)
            if img.size == (w, h) and img.size != ref.size:
                canvas = Image.new("RGBA", ref.size, (0, 0, 0, 0))
                canvas.paste(img.convert("RGBA"), (x, y))
                canvas.save(actual / name)
                continue
        shutil.copyfile(src, actual / name)
    results = slot / "results.json"
    previous = {r["file"]: r for r in json.loads(results.read_text())["rows"]} if results.exists() else {}
    SCALES.update({e["file"]: float(e["scale"]) for e in manifest["images"]})
    if regions:
        verify.load = region_loader(regions)
    rows = []
    for entry in manifest["images"]:
        name = entry["file"]
        if name in fresh:
            row = verify.compare(component, entry, actual, masks)
            rect = regions.get(name, regions.get("*")) if regions else None
            if rect:
                row["region"] = rect
            rows.append(row)
        else:
            rows.append(previous.get(name, {"file": name, "state": entry["state"], "result": "missing"}))
    (slot / "results.json").write_text(json.dumps(
        {"component": component, "source_ref": manifest.get("source_ref"), "generated": time.strftime("%Y-%m-%d %H:%M %Z"),
         "rows": rows}, indent=1, default=str))
    return [r for r in rows if r["file"] in fresh]


def write_report():
    lines = ["# Laptop parity (rshell vs origin/laptop@24972f7)", "",
             "Generated by `parity/laptop/verify-laptop.py`. Gate rule = desktop `parity/verify/verify.py`",
             "(no distance-field text: max d = 0; distance-field text: max d <= 8 and mean <= 0.5; no",
             "structural difference). `laptop-island-legacy` is an expected divergence (D1) and not gated.",
             "`laptop-panels` is gated inside each panel's region only (`--regions`): its references are",
             "whole-shell frames, panel replays render panel surfaces alone. Rows with a `region` column",
             "value were compared inside that logical rect.", "",
             "| component | images | pass | fail | missing | status |", "| --- | --- | --- | --- | --- | --- |"]
    details = []
    for component in GATED:
        results = OUT / component / "results.json"
        manifest = json.loads((HERE / "reference" / component / "manifest.json").read_text())
        if not results.exists():
            lines.append(f"| `{component}` | {len(manifest['images'])} | - | - | - | PENDING: no rshell laptop replay yet |")
            continue
        data = json.loads(results.read_text())
        rows = data["rows"]
        count = {k: sum(1 for r in rows if r["result"] == k) for k in ("pass", "fail", "missing")}
        status = "PASS" if count["pass"] == len(rows) else "FAIL"
        lines.append(f"| `{component}` | {len(rows)} | {count['pass']} | {count['fail']} | {count['missing']} | "
                     f"{status} ({data['generated']}) |")
        bad = [r for r in rows if r["result"] != "pass"]
        if bad:
            details += ["", f"## {component}: non-passing images", "",
                        "| file | result | cause | max | mean | region | note |",
                        "| --- | --- | --- | --- | --- | --- | --- |"]
            details += [f"| `{r['file']}` | {r['result']} | {r.get('cause', '-')} | {r.get('max', '-')} | "
                        f"{round(r['mean'], 3) if 'mean' in r else '-'} | {r.get('region', '')} | "
                        f"{r.get('note', r.get('tri', ''))} |" for r in bad]
    REPORT.write_text("\n".join(lines + details) + "\n")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--component", choices=GATED)
    ap.add_argument("--actual")
    ap.add_argument("--masks")
    ap.add_argument("--regions", help="{file|*: [x,y,w,h]} logical rect; compare only inside it")
    ap.add_argument("--report-only", action="store_true")
    args = ap.parse_args()
    if not args.report_only:
        if not (args.component and args.actual):
            ap.error("--component and --actual are required")
        masks = json.loads(Path(args.masks).read_text()) if args.masks else {}
        regions = json.loads(Path(args.regions).read_text()) if args.regions else {}
        rows = run_component(args.component, args.actual, masks, regions)
        ok = sum(1 for r in rows if r["result"] == "pass")
        print(f"{args.component}: {ok}/{len(rows)} of this run's images pass")
    write_report()
    print(f"report {REPORT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
