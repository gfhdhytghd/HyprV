#!/usr/bin/env python3
"""Parity QA loop tick: wait `interval` seconds, then report stale components.

    python3 parity/verify/watch.py [interval_s=600]
A component is stale when its UI crate, hyprv-ui or hyprv-widgets changed after its last
verify render. Prints `stale: <components>` (exit 0) or `stale: -` (exit 3).
"""
import json
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SKIP = ("artifacts", ".build", "target", "out")


def tree_mtime(crate):
    newest = 0.0
    for p in (ROOT / "crates" / crate).rglob("*"):
        if p.is_file() and not any(part in SKIP for part in p.parts):
            newest = max(newest, p.stat().st_mtime)
    return newest


# component -> crates whose changes invalidate its render
INPUTS = {"island": ["hyprv-ui-island", "hyprv-ui", "hyprv-widgets"],
          "bar": ["hyprv-ui-bar", "hyprv-ui", "hyprv-widgets"],
          "panels": ["hyprv-ui-panels", "hyprv-ui", "hyprv-widgets"]}


def stale_components():
    renders = json.loads((ROOT / "parity/verify/out/results.json").read_text())["renders"]
    stale = []
    for component, crates in INPUTS.items():
        seen = renders.get(component, {}).get("inputs_mtime") or {}
        if any(tree_mtime(c) > seen.get(c, 0) + 1 for c in crates):
            stale.append(component)
    return stale


def main():
    # Fixed cadence: wait the whole interval (crates change every few minutes while
    # owners work), then report which components need a re-render.
    interval = float(sys.argv[1]) if len(sys.argv) > 1 else 600
    time.sleep(interval)
    stale = stale_components()
    print("stale:", " ".join(stale) if stale else "-")
    return 0 if stale else 3


if __name__ == "__main__":
    sys.exit(main())
