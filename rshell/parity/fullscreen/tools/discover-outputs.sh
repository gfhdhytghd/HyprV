#!/usr/bin/env bash
# Read-only discovery of the user's real output layout -> fixtures/outputs.json.
# Only `hyprctl monitors all -j` (a query) is sent to the live Hyprland; nothing is changed.
set -euo pipefail
here="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
# The newest live instance that answers a read-only query (env signatures can be stale).
sig=""
for d in $(ls -1t "/run/user/$(id -u)/hypr" 2>/dev/null); do
    if HYPRLAND_INSTANCE_SIGNATURE="$d" hyprctl -j version >/dev/null 2>&1; then sig="$d"; break; fi
done
[[ -n "$sig" ]] || { echo "no live Hyprland instance found" >&2; exit 1; }
HYPRLAND_INSTANCE_SIGNATURE="$sig" hyprctl monitors all -j | python3 -I -c '
import json, sys
mons = [m for m in json.load(sys.stdin) if not m.get("disabled")]
outs = []
for m in sorted(mons, key=lambda m: (m["x"], m["y"])):
    s = m["scale"]
    # Logical size as Hyprland reports it to clients (transform 0/2 assumed; 1/3 swap).
    w, h = (m["height"], m["width"]) if m.get("transform", 0) % 2 else (m["width"], m["height"])
    outs.append({"name": m["name"], "make": m.get("make", ""), "model": m.get("model", ""),
                 "x": m["x"], "y": m["y"], "width": round(w / s), "height": round(h / s), "scale": s,
                 "width_px": w, "height_px": h, "refresh": round(m["refreshRate"], 3),
                 "transform": m.get("transform", 0)})
print(json.dumps({"source": "hyprctl monitors all -j (read-only)", "outputs": outs}, indent=1))
' > "$here/fixtures/outputs.json"
cat "$here/fixtures/outputs.json"
