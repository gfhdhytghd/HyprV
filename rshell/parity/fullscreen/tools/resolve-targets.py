#!/usr/bin/python3 -I
"""Resolve semantic pointer targets to Qt output-logical coordinates, with parity-host.

    python3 -I parity/fullscreen/tools/resolve-targets.py [STATE...]

For every pointer step whose target is semantic ({text}/{contains}/{image}/{item}), the
fixture prefix up to that step is replayed in parity-host (spec-island's harness, run
unchanged through parity/capture.sh in a private weston) followed by {"dump": true}. The
dump lists every visible Text/Image with its scene rect; the target is picked exactly as
parity-host's pointFor() does (visible matches sorted by scene y then x, `index`-th,
centre + x/y offset, floored). The result is written to the fixture as
`targets: {"STEP/key": [x, y]}` — the convention of crates/hyprv-ui-panels/tests/
shared_common/targets.json — and is what the nested capture clicks. Where the dark theme
resolves elsewhere (e.g. a themed SVG with another size) the dark point is in
`targets_by_theme: {"dark": {"STEP/key": [x, y]}}`, which overrides `targets`.
"""
import json
import math
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

FS = Path(__file__).resolve().parent.parent
PARITY = FS.parent
POINTER_KEYS = ("hover", "press", "click", "rightclick", "drag", "wheel", "release")
DUMP = re.compile(r"^dump: (text|image)\s+(-?[\d.]+)\s+(-?[\d.]+)\s+(-?[\d.]+) x\s+(-?[\d.]+)  (.*)$")


def semantic(value):
    return isinstance(value, dict) and any(k in value for k in ("text", "contains", "image", "item"))


def run_dump(fixture, steps, theme, scratch):
    fx = dict(fixture)
    fx["steps"] = steps + [{"dump": True}]
    fx.pop("targets", None)
    fixdir = Path(scratch) / "fixtures" / "fullscreen"
    fixdir.mkdir(parents=True, exist_ok=True)
    (fixdir / "probe.json").write_text(json.dumps(fx))
    env = dict(os.environ, PARITY_FIXTURES=str(Path(scratch) / "fixtures"), PARITY_OUT=str(Path(scratch) / "out"),
               PARITY_THEMES=theme, PARITY_SCALES=str(fixture["outputs"][0]["scale"]))
    proc = subprocess.run(["bash", str(PARITY / "capture.sh"), "fullscreen", "probe"], env=env,
                          capture_output=True, text=True)
    lines = (proc.stdout + proc.stderr).splitlines()
    items = []
    for line in lines:
        m = DUMP.match(line)
        if m:
            items.append({"kind": m[1], "x": float(m[2]), "y": float(m[3]), "w": float(m[4]), "h": float(m[5]),
                          "what": m[6]})
    if not items:
        sys.exit(f"no dump from parity-host (exit {proc.returncode}):\n" + "\n".join(lines[-30:]))
    return items


def pick(items, target):
    if "item" in target:
        sys.exit("{item} targets need objectName introspection; not supported by the dump")
    partial = "contains" in target
    by_text = "text" in target or partial
    needle = target["contains"] if partial else target["text"] if by_text else target["image"]
    hits = []
    for it in items:
        if by_text and it["kind"] == "text":
            ok = needle in it["what"] if partial else it["what"] == needle
        elif not by_text and it["kind"] == "image":
            ok = needle in it["what"]  # dump prints the file name of the source URL
        else:
            ok = False
        if ok:
            hits.append(it)
    hits.sort(key=lambda it: (it["y"], it["x"]))  # stable: equal keys keep tree order, as std::sort may not
    index = target.get("index", 0)
    if index >= len(hits):
        sys.exit(f"no visible match for {target} (found {len(hits)})")
    h = hits[index]
    return [math.floor(h["x"] + h["w"] / 2 + target.get("x", 0)), math.floor(h["y"] + h["h"] / 2 + target.get("y", 0))]


def main():
    names = sys.argv[1:] or sorted(p.stem for p in (FS / "fixtures").glob("*.json") if p.stem != "outputs")
    for name in names:
        path = FS / "fixtures" / f"{name}.json"
        fx = json.loads(path.read_text())
        targets, dark = {}, {}
        for i, step in enumerate(fx["steps"]):
            for key in POINTER_KEYS:
                if key in step and semantic(step[key]):
                    with tempfile.TemporaryDirectory(dir=os.environ.get("FS_SCRATCH")) as scratch:
                        pts = {}
                        for theme in ("light", "dark"):
                            items = run_dump(fx, fx["steps"][:i], theme, scratch)
                            pts[theme] = pick(items, step[key])
                    targets[f"{i}/{key}"] = pts["light"]
                    if pts["dark"] != pts["light"]:
                        dark[f"{i}/{key}"] = pts["dark"]
                    print(f"{name} {i}/{key} {step[key]} -> {pts}")
        fx.pop("targets", None)
        fx.pop("targets_by_theme", None)
        if targets:
            fx["targets"] = targets
        if dark:
            fx["targets_by_theme"] = {"dark": dark}
        path.write_text(json.dumps(fx, ensure_ascii=False, indent=1) + "\n")


if __name__ == "__main__":
    main()
