#!/usr/bin/env python3
"""Resolve in-panel pointer targets of parity/fixtures/panels with the Qt parity host.

The panels crate rasterises text into pictures, so `{"text": ...}` targets cannot be
found in its scene. This dev tool reproduces `parity/host/main.cpp::pointFor` exactly:
it copies each fixture into a scratch dir, inserts a probe capture right before every
step that targets a Text/Image, runs `parity/capture.sh` (headless, read-only) into a
scratch output, and resolves the n-th match (top-to-bottom, left-to-right) from the
probe's regions sidecar: centre + (x, y), floored, in output coordinates.

Writes tests/shared_common/targets.json: {fixture: {"<step index>/<key>": [x, y]}}.
Usage: python3 -I gen_targets.py [scratch-dir]
"""
import json, math, os, subprocess, sys, tempfile
from pathlib import Path

here = Path(__file__).resolve().parent
rshell = here.parents[3]
fixtures = rshell / "parity/fixtures/panels"
scratch = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(tempfile.mkdtemp(prefix="panels-targets-"))
fx_dir, out_dir = scratch / "fixtures/panels", scratch / "out"
fx_dir.mkdir(parents=True, exist_ok=True)
out_dir.mkdir(parents=True, exist_ok=True)

KEYS = ("hover", "press", "click", "drag", "release")


def is_target(v):
    return isinstance(v, dict) and any(k in v for k in ("text", "contains", "image"))


def bar_target(v):
    # Bar-level items: handled by the harness's bar mapping, not resolved here.
    return v.get("image") in ("control-panel-", "nm-signal-") or ("contains" in v and ("%" in v["contains"] or v["contains"] == "nocon"))


plan = {}
for f in sorted(fixtures.glob("*.json")):
    d = json.loads(f.read_text())
    steps, probes = [], []
    for i, s in enumerate(d["steps"]):
        for k in KEYS:
            v = s.get(k)
            if is_target(v) and not bar_target(v):
                name = f"probe-{i}-{k}"
                steps.append({"capture": name})
                probes.append((i, k, v, name))
        steps.append(s)
    if probes:
        d["steps"] = steps
        (fx_dir / f.name).write_text(json.dumps(d))
        plan[f.stem] = (d.get("crop", [0, 0, 3072, 1728]), probes)

if plan:
    env = dict(os.environ, PARITY_FIXTURES=str(scratch / "fixtures"), PARITY_OUT=str(out_dir),
               PARITY_SCALES="1", PARITY_THEMES="light")
    for stem in plan:
        subprocess.run([str(rshell / "parity/capture.sh"), "panels", stem], env=env, check=True)

targets = {}
for stem, (crop, probes) in plan.items():
    for i, k, v, name in probes:
        side = out_dir / "panels/regions" / f"{stem}--{name}--light@1.json"
        if not side.exists():
            side = out_dir / "regions" / f"{stem}--{name}--light@1.json"
        regions = json.loads(side.read_text())
        if "image" in v:
            hits = [r for r in regions if r["kind"] == "image" and v["image"] in r.get("source", "")]
        elif "contains" in v:
            hits = [r for r in regions if r["kind"] == "text" and v["contains"] in r.get("text", "")]
        else:
            hits = [r for r in regions if r["kind"] == "text" and r.get("text") == v["text"]]
        hits.sort(key=lambda r: (r["rect"][1], r["rect"][0]))
        idx = v.get("index", 0)
        if idx >= len(hits):
            raise SystemExit(f"{stem} step {i} {k}: no match for {v} ({len(hits)} hits)")
        x, y, w, h = hits[idx]["rect"]
        px = math.floor(crop[0] + x + w / 2 + v.get("x", 0))
        py = math.floor(crop[1] + y + h / 2 + v.get("y", 0))
        targets.setdefault(stem, {})[f"{i}/{k}"] = [px, py]

(here / "targets.json").write_text(json.dumps(targets, indent=1, sort_keys=True) + "\n")
print(f"wrote {here / 'targets.json'} ({sum(len(t) for t in targets.values())} targets); scratch {scratch}")
