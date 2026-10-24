#!/usr/bin/python3 -I
"""Diff hyprv-shell --nested-test full-screen captures against parity/fullscreen/reference.

    python3 -I parity/fullscreen/tools/compare.py RUN_DIR [--out DIR]

RUN_DIR is the --nested-test output (fullscreen-results.json). For every job capture the
reference entry is matched like hyprv-shell's reference_entry() (fixture stem, frame,
theme, scale), both images are compared as PREMULTIPLIED RGBA under docs/ARCHITECTURE.md's
gate (footprint within 1 logical px, painted IoU >= 0.98; distance_field_text -> max <= 8
and painted MAE <= 0.5, else exact), and every failure gets triage evidence:

  regions   connected areas of differing pixels (>2), each attributed to the reference
            layer-shell surface(s) it lies in (manifest `layers`)
  layout    per region, the integer shift (|dx|,|dy| <= 6 px) of the actual image that
            minimises the error, and how much of the error it removes
  timing    for frame sequences, which reference frame (any virtual time of the same
            state/theme) the actual frame matches best
  matte     differing pixels that are in the reference's matte-uncertain list
Writes OUT/compare.json and heat/<file>.png (red = actual too strong, blue = too weak).
"""
import argparse
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image

FS = Path(__file__).resolve().parent.parent
REF = FS / "reference"


def premul(path):
    a = np.asarray(Image.open(path).convert("RGBA")).astype(np.int32)
    out = a.copy()
    out[..., :3] = (a[..., :3] * a[..., 3:4] + 127) // 255
    return out


def gate(act, ref, df, scale):
    if act.shape != ref.shape:
        return {"pass": False, "status": "size_mismatch", "actual": list(act.shape), "reference": list(ref.shape)}
    d = np.abs(act - ref)
    pa, pr = act[..., 3] > 0, ref[..., 3] > 0
    union, inter = pa | pr, pa & pr
    iou = float(inter.sum() / union.sum()) if union.any() else 1.0
    mae = float(d[union].sum() / max(1, union.sum() * 4))
    mx = int(d.max())

    def bbox(m):
        ys, xs = np.nonzero(m)
        return None if len(xs) == 0 else [int(xs.min()), int(ys.min()), int(xs.max()) + 1, int(ys.max()) + 1]
    ba, br = bbox(pa), bbox(pr)
    fp = (ba is None and br is None) or (ba is not None and br is not None
                                          and all(abs(x - y) <= scale for x, y in zip(ba, br)))
    ok = fp and iou >= 0.98 and ((mx <= 8 and mae <= 0.5) if df else mx == 0)
    return {"pass": bool(ok), "status": "pass" if ok else "fail", "max_channel_diff": mx, "painted_mae": round(mae, 4),
            "painted_iou": round(iou, 5), "footprint_ok": bool(fp), "footprint_actual": ba, "footprint_reference": br,
            "different_px": int((d.max(axis=2) > 0).sum()), "over8_px": int((d.max(axis=2) > 8).sum())}


def regions(mask, min_px=4):
    """Connected components (8-neighbour) on a coarse 8x8 grid, mapped back to pixel boxes."""
    h, w = mask.shape
    gh, gw = (h + 7) // 8, (w + 7) // 8
    grid = np.zeros((gh, gw), bool)
    ys, xs = np.nonzero(mask)
    grid[ys // 8, xs // 8] = True
    seen = np.zeros_like(grid)
    out = []
    for gy, gx in zip(*np.nonzero(grid)):
        if seen[gy, gx]:
            continue
        stack, cells = [(gy, gx)], []
        seen[gy, gx] = True
        while stack:
            y, x = stack.pop()
            cells.append((y, x))
            for dy in (-1, 0, 1):
                for dx in (-1, 0, 1):
                    ny, nx = y + dy, x + dx
                    if 0 <= ny < gh and 0 <= nx < gw and grid[ny, nx] and not seen[ny, nx]:
                        seen[ny, nx] = True
                        stack.append((ny, nx))
        cy, cx = zip(*cells)
        box = [int(min(cx)) * 8, int(min(cy)) * 8, int(min(w, (max(cx) + 1) * 8)), int(min(h, (max(cy) + 1) * 8))]
        n = int(mask[box[1]:box[3], box[0]:box[2]].sum())
        if n >= min_px:
            out.append({"box_px": box, "px": n})
    return sorted(out, key=lambda r: -r["px"])


def layers_at(box, layers, scale):
    """Layer surfaces under the region, most specific (smallest) first; whole-output
    overlays are listed only when nothing smaller covers the region."""
    x0, y0, x1, y1 = (v / scale for v in box)
    hits = []
    for l in layers:
        if l["w"] <= 0 or l["h"] <= 0:
            continue
        if x0 < l["x"] + l["w"] and x1 > l["x"] and y0 < l["y"] + l["h"] and y1 > l["y"]:
            hits.append((l["w"] * l["h"], l["namespace"]))
    hits.sort()
    names = []
    for _, n in hits:
        if n not in names:
            names.append(n)
    if len(names) > 1:
        names = [n for n in names if "corner-overlay" not in n] or names
    return names


def best_shift(act, ref, box, r=6):
    x0, y0, x1, y1 = box
    H, W = ref.shape[:2]
    pad = r
    xa, ya, xb, yb = max(0, x0 - pad), max(0, y0 - pad), min(W, x1 + pad), min(H, y1 + pad)
    refc = ref[ya:yb, xa:xb]
    base = float(np.abs(act[ya:yb, xa:xb] - refc).sum())
    best = (0, 0, base)
    for dy in range(-r, r + 1):
        for dx in range(-r, r + 1):
            sx0, sy0 = xa + dx, ya + dy
            if sx0 < 0 or sy0 < 0 or sx0 + (xb - xa) > W or sy0 + (yb - ya) > H:
                continue
            e = float(np.abs(act[sy0:sy0 + (yb - ya), sx0:sx0 + (xb - xa)] - refc).sum())
            if e < best[2]:
                best = (dx, dy, e)
    return {"dx_px": best[0], "dy_px": best[1], "error_removed": round(1 - best[2] / base, 3) if base else 0.0}


def heat(act, ref, path):
    d = act[..., 3].astype(np.int32) - ref[..., 3].astype(np.int32)
    c = np.abs(act[..., :3] - ref[..., :3]).max(axis=2)
    img = np.zeros(act.shape[:2] + (3,), np.uint8)
    base = (ref[..., 3] > 0) | (act[..., 3] > 0)
    img[base] = (40, 40, 40)
    strength = np.clip(np.maximum(np.abs(d), c) * 8, 0, 255).astype(np.uint8)
    pos = (d > 0) | ((d == 0) & (c > 0))
    img[..., 0] = np.where(pos & (strength > 0), strength, img[..., 0])
    img[..., 2] = np.where((d < 0) & (strength > 0), strength, img[..., 2])
    Image.fromarray(img).save(path, optimize=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("run")
    ap.add_argument("--out")
    args = ap.parse_args()
    run = Path(args.run)
    out = Path(args.out or run / "compare")
    (out / "heat").mkdir(parents=True, exist_ok=True)
    manifest = json.loads((REF / "manifest.json").read_text())
    refs = manifest["images"]
    results = json.loads((run / "fullscreen-results.json").read_text())
    rows = []
    for job in results["jobs"]:
        jm_path = Path(job["manifest"])
        if not jm_path.exists():
            rows.append({"job": job["fixture"], "theme": job["theme"], "error": job.get("error") or "no job manifest"})
            continue
        jm = json.loads(jm_path.read_text())
        # Captures are listed in the job manifest; PNGs live next to its capture_manifest.
        cap_path = jm_path.parent / jm.get("capture_manifest", "manifest.json")
        caps = jm.get("captures") or []
        if jm.get("error"):
            rows.append({"job": Path(job["fixture"]).stem, "theme": job["theme"], "error": jm["error"]})
        for c in caps:
            stem, frame, theme, scale = c["fixture"], c.get("frame", ""), c["theme"], c["scale"]
            entry = next((e for e in refs if e["state"] == stem and e["frame"] == frame and e["theme"] == theme
                          and abs(e["scale"] - scale) < 1e-4), None)
            row = {"state": stem, "frame": frame, "theme": theme, "file": c["file"], "virtual_ms": c["virtual_ms"],
                   "engine_comparison": c.get("comparison", {})}
            if entry is None:
                row["error"] = "no reference entry"
                rows.append(row)
                continue
            act = premul(cap_path.parent / c["file"])
            ref = premul(REF / entry["file"])
            g = gate(act, ref, entry["distance_field_text"], scale)
            row.update(gate=g, reference=entry["file"], reference_virtual_ms=entry["virtual_ms"])
            if not g["pass"] and act.shape == ref.shape:
                d = np.abs(act - ref).max(axis=2)
                unc = {tuple(p) for p in entry["matte"].get("uncertain_px_coords", []) if isinstance(p, list)}
                ys, xs = np.nonzero(d > 2)
                row["matte_uncertain_share"] = round(sum((int(x), int(y)) in unc for x, y in zip(xs, ys)) / max(1, len(xs)), 4)
                regs = regions(d > 2)[:12]
                for rg in regs:
                    rg["box_logical"] = [round(v / scale, 1) for v in rg["box_px"]]
                    rg["layers"] = layers_at(rg["box_px"], entry["layers"], scale)
                    rg["shift"] = best_shift(act, ref, rg["box_px"])
                    sub = (act - ref)[rg["box_px"][1]:rg["box_px"][3], rg["box_px"][0]:rg["box_px"][2]]
                    rg["alpha_only"] = bool(np.abs(sub[..., 3]).sum() > 0 and np.abs(sub[..., :3]).sum() <= np.abs(sub[..., 3]).sum())
                    rg["mean_signed_alpha"] = round(float(sub[..., 3].mean()), 2)
                row["regions"] = regs
                if frame:
                    best = []
                    for e in refs:
                        if e["state"] == stem and e["theme"] == theme and e["frame"]:
                            r2 = premul(REF / e["file"])
                            best.append((float(np.abs(act - r2).mean()), e["frame"]))
                    best.sort()
                    row["timing_best_match"] = [{"frame": f, "mean_abs": round(m, 4)} for m, f in best[:3]]
                heat(act, ref, out / "heat" / c["file"])
            rows.append(row)
            print(f"{c['file']:50s} {'PASS' if g['pass'] else 'FAIL'} max {g.get('max_channel_diff')} "
                  f"mae {g.get('painted_mae')} iou {g.get('painted_iou')} fp {g.get('footprint_ok')}", flush=True)
    (out / "compare.json").write_text(json.dumps(rows, indent=1, ensure_ascii=False))
    print(f"{sum(1 for r in rows if r.get('gate', {}).get('pass'))}/{len(rows)} pass -> {out / 'compare.json'}")


if __name__ == "__main__":
    main()
