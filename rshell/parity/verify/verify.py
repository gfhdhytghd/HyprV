#!/usr/bin/env python3
"""Parity verification of the Rust UI crates against parity/reference.

    python3 parity/verify/verify.py                 # render + diff + report, all components
    python3 parity/verify/verify.py --no-render     # re-diff the last renders
    python3 parity/verify/verify.py --component bar

Renders come from each crate's own fixture replay (never modified here):
  island  cargo test -p hyprv-ui-island --test parity   (HYPRV_ISLAND_PARITY_OUT, mine-<file>)
  bar     python3 crates/hyprv-ui-bar/scripts/parity.py  (artifacts/reference-actual/<file>)
  panels  cargo test -p hyprv-ui-panels --test shared_parity (HYPRV_PANELS_PARITY_OUT, mine-<file>)
          + calendar-morph from hyprv-ui-island/tests/calendar_morph.rs once it exists

Diff: both images premultiplied; per pixel d = max channel |delta| (0..255).
Gate (docs/ARCHITECTURE.md "Parity acceptance"):
  * images without distance-field text (reference bit-exact run to run): d == 0 everywhere
  * images with distance-field text: max d <= 8 and mean d over painted pixels <= 0.5
  * any image: no structural difference (painted bounding box within 1 logical px,
    painted-mask IoU >= 0.98)
Outputs: parity/verify/out/<component>/{actual,heat}/, results.json, docs/verify/parity.md.
"""
import argparse
import fcntl
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
PARITY = ROOT / "parity"
OUT = PARITY / "verify" / "out"
REPORT = ROOT / "docs" / "verify" / "parity.md"
COMPONENTS = ["island", "bar", "panels"]
CRATES = {"island": "hyprv-ui-island", "bar": "hyprv-ui-bar", "panels": "hyprv-ui-panels"}
OWNERS = {"island": "svc-media", "bar": "svc-power", "panels": "spec-bar"}
_policy_spec = importlib.util.spec_from_file_location('parity_policy', ROOT / 'parity/verify/policy.py')
policy = importlib.util.module_from_spec(_policy_spec)
_policy_spec.loader.exec_module(policy)
EXCLUDED = policy.EXCLUDED
WATCHED = ["hyprv-ui-island", "hyprv-ui-bar", "hyprv-ui-panels", "hyprv-ui", "hyprv-widgets"]


# ------------------------------------------------------------------ rendering

def run(cmd, env=None, cwd=ROOT, log=None):
    started = time.time()
    proc = subprocess.run(cmd, cwd=cwd, env={**os.environ, **(env or {})}, capture_output=True, text=True)
    if log:
        log.write_text(proc.stdout + "\n--- stderr ---\n" + proc.stderr)
    return proc.returncode, time.time() - started


def publish(slot, files, code):
    """Replace OUT/<slot>/actual with `files` ({name: path}) only when the render produced
    something; a broken build keeps the last good renders (reported as stale)."""
    actual = OUT / slot / "actual"
    if not files:
        stale = time.strftime("%H:%M", time.localtime(actual.stat().st_mtime)) if actual.exists() else "never"
        return 0, f"render failed (exit {code}, see out/{slot}/render.log); showing last good render ({stale})"
    shutil.rmtree(actual, ignore_errors=True)
    actual.mkdir(parents=True)
    for name, path in files.items():
        shutil.copyfile(path, actual / name)
    return len(files), "ok"


def render_island(slot, env):
    raw = OUT / slot / "raw"
    shutil.rmtree(raw, ignore_errors=True)
    raw.mkdir(parents=True)
    code, secs = run(["cargo", "test", "-q", "-p", "hyprv-ui-island", "--test", "parity"],
                     env={**env, "HYPRV_ISLAND_PARITY_OUT": str(raw)}, log=OUT / slot / "render.log")
    n, status = publish(slot, {f.name[5:]: f for f in raw.glob("mine-*.png")}, code)
    shutil.rmtree(raw, ignore_errors=True)  # disk policy: actual/ keeps the only copy
    return {"status": status, "rendered": n, "seconds": round(secs), "exit": code, "masks": {}}


def render_bar(slot, env):
    started = time.time()
    crate = ROOT / "crates" / "hyprv-ui-bar"
    code, secs = run(["python3", str(crate / "scripts" / "parity.py")], env=env, log=OUT / slot / "render.log")
    source = crate / "artifacts" / "reference-actual"
    masks, files = {}, {}
    coverage = source / "coverage.json"
    # parity.py exits non-zero whenever a comparison fails, so freshness (written during
    # this run) is the success signal, not the exit code.
    if coverage.exists() and coverage.stat().st_mtime >= started:
        cov = json.loads(coverage.read_text())
        for entry in cov.get("exported", []):
            f = source / entry["file"]
            if f.exists():
                files[entry["file"]] = f
                # The island slot is empty in the bar's own export; island contents are
                # verified against the island references. Exclude that rect in BOTH images.
                masks[entry["file"]] = entry.get("foreign_island_local_rect")
    n, status = publish(slot, files, code)
    if not n:
        previous = OUT / "results.json"
        masks = json.loads(previous.read_text())["renders"].get(slot, {}).get("masks", {}) if previous.exists() else {}
    return {"status": status, "rendered": n, "seconds": round(secs), "exit": code, "masks": masks}


def render_panels(slot, env):
    # tests/shared_parity.rs (written by svc-media with spec-bar's agreement) replays
    # parity/fixtures/panels through the crate's public API. calendar-morph belongs to
    # piece B (hyprv-ui-island/tests/calendar_morph.rs) and is collected when it exists.
    raw = OUT / slot / "raw"
    shutil.rmtree(raw, ignore_errors=True)
    raw.mkdir(parents=True)
    # --no-default-features: the pure UI build (no hyprv-svc-* deps), so unrelated service
    # crates being mid-edit cannot block panel parity.
    code, secs = run(["cargo", "test", "-q", "-p", "hyprv-ui-panels", "--no-default-features", "--test", "shared_parity"],
                     env={**env, "HYPRV_PANELS_PARITY_OUT": str(raw)}, log=OUT / slot / "render.log")
    morph_raw = OUT / slot / "raw-morph"
    shutil.rmtree(morph_raw, ignore_errors=True)
    if (ROOT / "crates/hyprv-ui-island/tests/calendar_morph.rs").exists():
        morph_raw.mkdir(parents=True)
        run(["cargo", "test", "-q", "-p", "hyprv-ui-island", "--test", "calendar_morph"],
            env={**env, "HYPRV_ISLAND_PARITY_OUT": str(morph_raw), "HYPRV_CALENDAR_MORPH_OUT": str(morph_raw)},
            log=OUT / slot / "render-morph.log")
    files = {}
    for folder in (raw, morph_raw):
        for f in folder.glob("mine-*.png") if folder.exists() else []:
            files[f.name[5:]] = f
    n, status = publish(slot, files, code)
    shutil.rmtree(raw, ignore_errors=True)  # disk policy: actual/ keeps the only copy
    shutil.rmtree(morph_raw, ignore_errors=True)
    return {"status": status, "rendered": n, "seconds": round(secs), "exit": code, "masks": {}}


RENDERERS = {"island": render_island, "bar": render_bar, "panels": render_panels}

# ------------------------------------------------------------------ production GPU path
# parity/gpu (hyprv-parity-gpu) renders a Scene through hyprv_ui::egl::{SharedEgl,
# EglWindow} -- the live shell's EGL/GLES2 RGBA8 path -- offscreen on a private headless
# Weston. A harness takes it when HYPRV_PARITY_BACKEND=gl and it has wired the hook.
GPU_ENV = "HYPRV_PARITY_BACKEND"
HOOK_DIRS = {"island": ["tests"], "bar": ["examples", "tests", "scripts"], "panels": ["tests"]}


def engine_switch():
    """hyprv-ui honours HYPRV_PARITY_BACKEND in Renderer::raster/png (preferred wiring)."""
    src = ROOT / "crates/hyprv-ui/src"
    return any(GPU_ENV in f.read_text(errors="ignore") for f in src.rglob("*.rs"))


def gpu_hook_wired(component):
    if engine_switch():
        if component != "panels":
            return True
        # panels paints into its own F16 surface unless it defers to the env switch
        common = ROOT / "crates/hyprv-ui-panels/tests/shared_common/mod.rs"
        return common.exists() and GPU_ENV in common.read_text(errors="ignore")
    crate = ROOT / "crates" / CRATES[component]
    for d in HOOK_DIRS[component]:
        for f in (crate / d).rglob("*") if (crate / d).exists() else []:
            if f.is_file() and f.suffix in (".rs", ".py") and (
                    "hyprv_parity_gpu" in f.read_text(errors="ignore") or GPU_ENV in f.read_text(errors="ignore")):
                return True
    return False


class PrivateWeston:
    """Headless Weston on its own socket (never the user's session), for EGL only."""

    def __enter__(self):
        self.socket = f"parity-verify-{os.getpid()}"
        runtime = os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")
        self.proc = subprocess.Popen(["weston", "--backend=headless", "--renderer=gl", "--shell=desktop",
                                      "--no-config", "--idle-time=0", f"--socket={self.socket}"],
                                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for _ in range(100):
            if Path(runtime, self.socket).exists():
                break
            time.sleep(0.05)
        return self.socket

    def __exit__(self, *exc):
        self.proc.terminate()
        self.proc.wait(timeout=10)


# ------------------------------------------------------------------ diffing

def load(path):
    img = np.asarray(Image.open(path).convert("RGBA"), dtype=np.float32)
    a = img[..., 3:4] / 255.0
    return np.concatenate([img[..., :3] * a, img[..., 3:4]], axis=2)  # premultiplied


def apply_mask(pm, rect, scale):
    if not rect:
        return
    x, y, w, h = rect
    x0, y0 = max(0, int(np.floor(x * scale))), max(0, int(np.floor(y * scale)))
    x1, y1 = int(np.ceil((x + w) * scale)), int(np.ceil((y + h) * scale))
    pm[y0:y1, x0:x1, :] = 0


def bbox(mask):
    ys, xs = np.nonzero(mask)
    if len(xs) == 0:
        return None
    return [int(xs.min()), int(ys.min()), int(xs.max()) + 1, int(ys.max()) + 1]


def region_masks(regions, shape, scale):
    h, w = shape
    masks = {k: np.zeros((h, w), bool) for k in ("text", "image", "canvas")}
    for r in regions:
        x, y, rw, rh = r["rect"]
        pad = 1.0  # glyph ink and AA bleed past the item box
        x0, y0 = max(0, int(np.floor((x - pad) * scale))), max(0, int(np.floor((y - pad) * scale)))
        x1, y1 = min(w, int(np.ceil((x + rw + pad) * scale))), min(h, int(np.ceil((y + rh + pad) * scale)))
        if x1 > x0 and y1 > y0:
            masks[r["kind"]][y0:y1, x0:x1] = True
    return masks


def heatmap(ref, d, path):
    lum = (ref[..., 0] * 0.3 + ref[..., 1] * 0.59 + ref[..., 2] * 0.11 + (255 - ref[..., 3]) * 0.8)
    base = np.clip(lum * 0.35 + 150, 0, 255)
    red = np.clip(d * 4, 0, 255)
    out = np.stack([np.maximum(base, red), base * (1 - red / 255), base * (1 - red / 255)], axis=2)
    Image.fromarray(out.astype(np.uint8), "RGB").save(path, optimize=False, compress_level=1)


def triptych(ref_path, act_path, heat_path, path):
    """reference | actual | heatmap side by side, over a neutral grey (both themes legible)."""
    imgs = []
    for f in (ref_path, act_path):
        img = Image.open(f).convert("RGBA")
        bg = Image.new("RGBA", img.size, (127, 140, 153, 255))
        imgs.append(Image.alpha_composite(bg, img).convert("RGB"))
    imgs.append(Image.open(heat_path).convert("RGB"))
    w, h = imgs[0].size
    sheet = Image.new("RGB", (w * 3 + 8, h), (40, 40, 40))
    for i, img in enumerate(imgs):
        sheet.paste(img, (i * (w + 4), 0))
    sheet.save(path, compress_level=1)


def classify(m):
    """Root-cause label from the attribution of failing pixels."""
    if m["structural"]:
        return "layout"
    total = m["px_over8"] or m["px_nonzero"]
    if not total:
        return "-"
    shares = m["shares"]
    if not m["px_over8"]:
        # only small differences: a uniform colour/alpha offset unless they hug edges
        return "rect AA" if m["edge_share"] >= 0.8 and shares["shape"] >= 0.5 else "fill/color"
    top = max(shares, key=shares.get)
    if top == "text":
        return "text"
    if top in ("image",):
        return "svg/icon"
    if top == "canvas":
        return "canvas"
    return "rect AA" if m["edge_share"] >= 0.8 else "fill/color"


def compare(component, entry, actual_dir, masks):
    file = entry["file"]
    ref_path = PARITY / "reference" / component / file
    act_path = actual_dir / file
    base = {"file": file, "state": entry["state"], "frame": entry["frame"], "theme": entry["theme"],
            "scale": entry["scale"], "df": entry.get("distance_field_text", False)}
    if not act_path.exists():
        return {**base, "result": "missing"}
    ref, act = load(ref_path), load(act_path)
    if ref.shape != act.shape:
        return {**base, "result": "fail", "cause": "layout",
                "note": f"size {act.shape[1]}x{act.shape[0]} vs reference {ref.shape[1]}x{ref.shape[0]}",
                "max": 255, "mean": 255.0, "structural": True}
    scale = float(entry["scale"])
    rect = masks.get(file)
    apply_mask(ref, rect, scale)
    apply_mask(act, rect, scale)
    d = np.abs(ref - act).max(axis=2)
    painted = (ref[..., 3] > 0) | (act[..., 3] > 0)
    npainted = int(painted.sum()) or 1
    m = {**base, "max": int(np.round(d.max())), "mean": float(d[painted].sum() / npainted),
         "px_nonzero": int((np.round(d) > 0).sum()), "px_over8": int((d > 8).sum())}
    # structure: painted footprint (alpha >= 50 %) in logical px
    # alpha > 16 footprint: robust for translucent glass cards (alpha ~0.3-0.5), where a
    # >= 50 % threshold flips on 1-2/255 noise
    ra, aa = ref[..., 3] > 16, act[..., 3] > 16
    rb, ab = bbox(ra), bbox(aa)
    iou = float((ra & aa).sum() / max(1, (ra | aa).sum()))
    shift = None if rb is None or ab is None else max(abs(p - q) for p, q in zip(rb, ab)) / scale
    # a footprint cannot be structurally different when no pixel differs by more than 8/255
    m["structural"] = m["max"] > 8 and ((rb is None) != (ab is None) or (shift is not None and shift > 1.0) or iou < 0.98)
    m["bbox_shift"] = shift
    m["iou"] = round(iou, 4)
    # attribution of failing pixels (> 8, or any when all are small)
    bad = d > 8 if m["px_over8"] else np.round(d) > 0
    regions_file = PARITY / "reference" / component / "regions" / file.replace(".png", ".json")
    regions = json.loads(regions_file.read_text()) if regions_file.exists() else []
    rm = region_masks(regions, d.shape, scale)
    nbad = int(bad.sum()) or 1
    text = bad & rm["text"]
    image = bad & rm["image"] & ~rm["text"]
    canvas = bad & rm["canvas"] & ~rm["text"] & ~rm["image"]
    shape = bad & ~rm["text"] & ~rm["image"] & ~rm["canvas"]
    m["shares"] = {"text": round(text.sum() / nbad, 3), "image": round(image.sum() / nbad, 3),
                   "canvas": round(canvas.sum() / nbad, 3), "shape": round(shape.sum() / nbad, 3)}
    # edge pixels: partially covered in the reference, or next to a coverage change
    alpha = ref[..., 3]
    partial = (alpha > 0) & (alpha < 255)
    grad = np.zeros_like(partial)
    grad[:, 1:] |= alpha[:, 1:] != alpha[:, :-1]
    grad[1:, :] |= alpha[1:, :] != alpha[:-1, :]
    m["edge_share"] = round(float((shape & (partial | grad)).sum() / max(1, shape.sum())), 3)
    if m["df"]:
        ok = m["max"] <= 8 and m["mean"] <= 0.5
    else:
        ok = m["max"] == 0
    m["gate_v1"] = (m["max"] <= 8 and m["mean"] <= 0.5 and not m["structural"])
    m["result"] = "pass" if ok and not m["structural"] else "fail"
    m["cause"] = classify(m) if m["result"] == "fail" else "-"
    heat_dir = actual_dir.parent / "heat"
    heat_dir.mkdir(exist_ok=True)
    if m["px_nonzero"]:
        heatmap(ref, d, heat_dir / file)
        m["heat"] = str((heat_dir / file).relative_to(ROOT))
        tri_dir = actual_dir.parent / "tri"
        tri_dir.mkdir(exist_ok=True)
        triptych(ref_path, act_path, heat_dir / file, tri_dir / file)
        m["tri"] = str((tri_dir / file).relative_to(ROOT))
    return m


def relabel_animation(rs):
    """Frames of one sequence (state, frame name, theme, scale): a failing mid frame
    that is >= 3x worse (mean) than the settled last frame is an animation problem
    (wrong timing/easing or layout that does not follow the animated geometry)."""
    seqs = {}
    for x in rs:
        frame = x.get("frame") or ""
        if "-t" not in frame or x["result"] == "missing":
            continue
        name = frame.rsplit("-t", 1)[0]
        seqs.setdefault((x["state"], name, x["theme"], x["scale"]), []).append(x)
    for xs in seqs.values():
        xs.sort(key=lambda x: int(x["frame"].rsplit("-t", 1)[1]))
        last = xs[-1]
        for x in xs[:-1]:
            if x["result"] == "fail" and x.get("mean", 0) >= 3 * max(last.get("mean", 0), 0.05) and x.get("mean", 0) > 1:
                x["cause"] = "animation"


# ------------------------------------------------------------------ report

def crate_mtime(component):
    return tree_mtime(ROOT / "crates" / CRATES[component])


def tree_mtime(crate):
    newest = 0.0
    for p in crate.rglob("*"):
        if p.is_file() and not any(part in ("artifacts", ".build", "target", "out") for part in p.parts):
            newest = max(newest, p.stat().st_mtime)
    return newest


def write_report(results, renders, deltas):
    # Gate on the production GPU path where the crate harness has wired it; otherwise on
    # the CPU harness, reported as provisional.
    gate_slot = {c: (f"{c}-gl" if results.get(f"{c}-gl") else c) for c in COMPONENTS}
    lines = ["# Parity verification",
             "",
             f"Generated {time.strftime('%Y-%m-%d %H:%M %Z')} by `python3 parity/verify/verify.py` "
             "(owner: parity QA). References: `parity/reference` (Qt RHI/OpenGL renders of the original QML, "
             "see `parity/README.md`). Heatmaps: red = premultiplied channel delta × 4 over the dimmed reference.",
             "",
             "**Gate** (`docs/ARCHITECTURE.md`, tightened): images without distance-field text must be "
             "**bit-exact** (their references are bit-exact run to run); images with distance-field text "
             "max ≤ 8/255 and mean ≤ 0.5/255 over painted pixels; no structural difference in any image. "
             "`v1` = the earlier ≤ 8 / ≤ 0.5 gate applied to every image, shown to track progress.",
             "",
             "Each crate is rendered twice: **cpu** = the crate's own harness raster (Skia CPU; panels "
             "uses an F16 surface), **gl** = the production path (`hyprv_ui::egl` EGL/GLES2 RGBA8 render "
             "target, read back offscreen via `parity/gpu`). The gate is decided on **gl** once a harness "
             "has wired the hook (`HYPRV_PARITY_BACKEND=gl`); until then the cpu result is provisional.",
             "",
             "| component | backend | crate (owner) | render | images | pass | fail | missing | excluded | gate v1 pass | causes of failures |",
             "|---|---|---|---|---|---|---|---|---|---|---|"]
    for slot in [x for c in COMPONENTS for x in (c, f"{c}-gl")]:
        c = slot.split("-gl")[0]
        backend = "gl" if slot.endswith("-gl") else "cpu"
        rs = results.get(slot, [])
        n = len(rs)
        cnt = lambda r: sum(1 for x in rs if x["result"] == r)
        v1 = sum(1 for x in rs if x.get("gate_v1"))
        causes = {}
        for x in rs:
            if x["result"] == "fail":
                causes[x["cause"]] = causes.get(x["cause"], 0) + 1
        cause_text = ", ".join(f"{k} {v}" for k, v in sorted(causes.items(), key=lambda kv: -kv[1])) or "-"
        mark = " **(gated)**" if gate_slot[c] == slot else ""
        lines.append(f"| {c} | {backend}{mark} | `{CRATES[c]}` ({OWNERS[c]}) | {renders.get(slot, {}).get('status', 'not run')} | "
                     f"{n} | {cnt('pass')} | {cnt('fail')} | {cnt('missing')} | {cnt('excluded')} | {v1} | {cause_text} |")
    gated = {c: [x for x in results.get(gate_slot[c], []) if x["result"] != "excluded"] for c in COMPONENTS}
    status = {c: ("PASS" if gated[c] and all(x["result"] == "pass" for x in gated[c]) else "FAIL")
              + ("" if gate_slot[c].endswith("-gl") else " (provisional: cpu)") for c in COMPONENTS}
    lines += ["", "**Gate status:** " + ", ".join(f"{c} {status[c]}" for c in COMPONENTS)
              + ". `excluded` = live-unreachable states (decision: match live behaviour, "
              "`QUIRK_BATTERY_POPUP_NEVER_OPENS`); their references are kept for a later fix-forward."]
    lines += ["", "Causes: `layout` = painted footprint moved > 1 logical px or mask IoU < 0.98; "
              "`text` / `svg/icon` / `canvas` = most failing pixels inside reference Text / Image / Canvas "
              "items (regions sidecars); `rect AA` = failing pixels outside those, ≥ 80 % on coverage edges; "
              "`fill/color` = interior shape pixels, or only small (≤ 8) differences spread over fills; `animation` = a mid-sequence frame ≥ 3× worse (mean) "
              "than the settled last frame of its sequence (timing/easing, or layout not following the animated "
              "geometry).", ""]
    lines += ["", "## Production GPU path vs CPU harness", "",
              "Same fixtures, same crate code; the only difference is the rasteriser. `differing` = images "
              "whose CPU and GPU renders are not byte-identical (premultiplied).", "",
              "| component | GPU hook | cpu pass | gl pass | images compared | differing | max delta | worst |",
              "|---|---|---|---|---|---|---|---|"]
    for c in COMPONENTS:
        d = deltas.get(c)
        cpu_pass = sum(1 for x in results.get(c, []) if x["result"] == "pass")
        gl_rs = results.get(f"{c}-gl", [])
        gl_pass = sum(1 for x in gl_rs if x["result"] == "pass") if gl_rs else "–"
        hook = "wired" if gl_rs else renders.get(f"{c}-gl", {}).get("status", "not run")
        if d:
            worst = ", ".join(f"`{w[1]}` ({w[0]})" for w in d["worst"][:3]) or "–"
            lines.append(f"| {c} | {hook} | {cpu_pass} | {gl_pass} | {d['compared']} | {d['differing']} | {d['max']} | {worst} |")
        else:
            lines.append(f"| {c} | {hook} | {cpu_pass} | {gl_pass} | – | – | – | – |")
    lines.append("")
    for c in COMPONENTS:
        rs = results.get(gate_slot[c], [])
        lines += [f"## {c} ({'gl' if gate_slot[c].endswith('-gl') else 'cpu, provisional'})", ""]
        if renders[c].get("masks"):
            lines += ["The bar export leaves the island slot empty; that rectangle (`foreign_island_local_rect`) "
                      "is cleared in both images. Island contents are verified in the island section. "
                      "`calendar-host` and `island-expanded-host` mount their production content without that mask; assembled full-screen coverage is additionally required by cutover preflight.", ""]
        if not rs:
            lines += [f"Not run: {renders[gate_slot[c]]['status']}.", ""]
            continue
        by_state = {}
        for x in rs:
            by_state.setdefault(x["state"], []).append(x)
        lines += ["| state | images | pass | worst max | worst mean | worst image | cause | heatmap |",
                  "|---|---|---|---|---|---|---|---|"]
        for state, xs in sorted(by_state.items()):
            if all(x["result"] == "excluded" for x in xs):
                lines.append(f"| {state} | {len(xs)} | – | – | – | – | excluded (live-unreachable) | – |")
                continue
            done = [x for x in xs if x["result"] not in ("missing", "excluded")]
            if not done:
                lines.append(f"| {state} | {len(xs)} | 0 | – | – | – | missing | – |")
                continue
            worst = max(done, key=lambda x: (x["result"] == "fail", x.get("max", 0), x.get("mean", 0)))
            passed = sum(1 for x in xs if x["result"] == "pass")
            heat = (f"[heat](../../{worst['heat']}) · [ref\\|ours\\|heat](../../{worst['tri']})"
                    if worst.get("heat") else "–")
            causes = sorted({x["cause"] for x in done if x["result"] == "fail"})
            lines.append(f"| {state} | {len(xs)} | {passed} | {max(x.get('max', 0) for x in done)} | "
                         f"{max(x.get('mean', 0) for x in done):.3f} | `{worst['file']}` | "
                         f"{', '.join(causes) or '-'} | {heat} |")
        lines.append("")
    lines += ["Per-image metrics (max, mean, pixel counts, IoU, bbox shift, attribution shares): "
              "`parity/verify/out/results.json`.", ""]
    REPORT.parent.mkdir(parents=True, exist_ok=True)
    REPORT.write_text("\n".join(lines))


def cpu_gpu_delta(component):
    """Direct CPU-harness vs production-GPU comparison of the same fixture renders."""
    cpu, gpu = OUT / component / "actual", OUT / f"{component}-gl" / "actual"
    if not gpu.exists():
        return None
    files = sorted(set(f.name for f in cpu.glob("*.png")) & set(f.name for f in gpu.glob("*.png")))
    worst, differing, maxd = [], 0, 0
    for name in files:
        a, b = load(cpu / name), load(gpu / name)
        if a.shape != b.shape:
            differing += 1
            worst.append((255, name, "size"))
            continue
        d = np.abs(a - b).max(axis=2)
        m = int(np.round(d.max()))
        if m:
            differing += 1
            maxd = max(maxd, m)
            worst.append((m, name, int((np.round(d) > 0).sum())))
    worst.sort(reverse=True)
    return {"compared": len(files), "differing": differing, "max": maxd, "worst": worst[:5]}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--component", choices=COMPONENTS, action="append")
    ap.add_argument("--no-render", action="store_true")
    ap.add_argument("--backend", choices=["cpu", "gl", "both"], default="both",
                    help="cpu = crate harness raster; gl = production EGL/GL path (gated when wired)")
    args = ap.parse_args()
    components = args.component or COMPONENTS
    backends = ["cpu", "gl"] if args.backend == "both" else [args.backend]
    OUT.mkdir(parents=True, exist_ok=True)
    # Runs share out/ and results.json: serialize them, or one run's rmtree of
    # actual/heat dirs breaks the other mid-diff. Held until the process exits.
    lock = open(OUT / ".verify.lock", "w")
    try:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        print("verify: another verify.py run holds out/.verify.lock; waiting", file=sys.stderr, flush=True)
        fcntl.flock(lock, fcntl.LOCK_EX)
    state_file = OUT / "results.json"
    previous = json.loads(state_file.read_text()) if state_file.exists() else {}
    results, renders = previous.get("results", {}), previous.get("renders", {})
    for c in COMPONENTS:
        for slot in (c, f"{c}-gl"):
            renders.setdefault(slot, {"status": "not run", "masks": {}})
    weston = PrivateWeston() if ("gl" in backends and not args.no_render) else None
    socket = weston.__enter__() if weston else None
    try:
        for c in components:
            for backend in backends:
                slot = c if backend == "cpu" else f"{c}-gl"
                (OUT / slot).mkdir(parents=True, exist_ok=True)
                wired = backend == "cpu" or gpu_hook_wired(c)
                if not args.no_render:
                    if not wired:
                        renders[slot] = {"status": "GPU hook not wired in the crate harness", "masks": {}}
                        shutil.rmtree(OUT / slot / "actual", ignore_errors=True)
                    else:
                        env = {} if backend == "cpu" else {GPU_ENV: "gl", "WAYLAND_DISPLAY": socket}
                        source_hashes = policy.inputs(ROOT)
                        stamp = {name: tree_mtime(ROOT / "crates" / name) for name in WATCHED}
                        renders[slot] = RENDERERS[c](slot, env)
                        renders[slot]["crate_mtime"] = crate_mtime(c)
                        renders[slot]["inputs_mtime"] = stamp
                        renders[slot]["sources"] = source_hashes
                        renders[slot]["backend"] = backend
                manifest = json.loads((PARITY / "reference" / c / "manifest.json").read_text())
                actual = OUT / slot / "actual"
                shutil.rmtree(OUT / slot / "heat", ignore_errors=True)
                shutil.rmtree(OUT / slot / "tri", ignore_errors=True)
                masks = renders[slot].get("masks") or renders[c].get("masks", {})
                rs = [compare(c, e, actual, masks) for e in manifest["images"]] if actual.exists() and any(actual.glob("*.png")) else []
                for x in rs:
                    if x["state"] in EXCLUDED.get(c, ()):
                        x["result"], x["cause"] = "excluded", "live-unreachable"
                relabel_animation(rs)
                results[slot] = rs
                if not args.no_render and wired:
                    renders[slot]['inputs_changed'] = renders[slot]['sources'] != policy.inputs(ROOT)
                    renders[slot]['actual_sha256'] = {p.name: policy.sha(p) for p in actual.glob('*.png')}
                print(f"{slot}: {renders[slot]['status']}; {sum(1 for r in rs if r['result'] == 'pass')}/{len(rs)} pass")
    finally:
        if weston:
            weston.__exit__(None, None, None)
    deltas = {c: cpu_gpu_delta(c) for c in COMPONENTS}
    state_file.write_text(json.dumps({"results": results, "renders": renders, "deltas": deltas}, indent=1))
    write_report(results, renders, deltas)
    print("report:", REPORT.relative_to(ROOT))


if __name__ == "__main__":
    main()
