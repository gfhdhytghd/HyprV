#!/usr/bin/python3 -I
"""Derive the full-screen fixtures from the component fixtures (parity/fixtures/*).

    python3 -I parity/fullscreen/tools/gen-fixtures.py

Each fullscreen fixture is a component fixture of the desktop scene with:
  - `component: "fullscreen"`, `crop` = the whole output (no crop),
  - `outputs`: the user's real layout (fixtures/outputs.json, from discover-outputs.sh),
    and `services.screens` rewritten to match it,
  - `derived_from`: the component fixture it came from,
  - state the real shell cannot take through `set.root` moved to where the real shell
    reads it (battery: battery-info.sh output + an UPower battery),
  - `targets` (Qt-resolved pointer coordinates) are added later by resolve-targets.py.
The JSON written here is the source of truth for both capture paths.
"""
import copy
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
FS = HERE.parent
PARITY = FS.parent
SRC_FIXTURES = PARITY / "fixtures"
OUT = FS / "fixtures"

# state name -> (component fixture, note)
STATES = {
    "idle": ("bar/baseline", "idle bar + idle island"),
    "media-playing": ("bar/media-playing", "island showing a playing track"),
    "island-expanded": ("bar/island-expanded-host", "island expanded by hover (bar surface grows)"),
    "calendar": ("panels/calendar-week", "calendar presented (island host morph settled)"),
    "calendar-morph": ("panels/calendar-morph", "island -> calendar morph frames, open and close"),
    "control": ("panels/cp-main-idle", "control panel open"),
    "control-open-close": ("panels/cp-open-close", "control panel open/close frames"),
    "wifi": ("panels/wifi-connected-4", "Wi-Fi popup, connected, 4 networks"),
    "stats": ("panels/stats-120", "stats popup after 120 s of samples"),
    "volume": ("panels/qa-volume-30", "volume quick-adjust OSD at 30 %"),
    "tray-menu": ("bar/tray-menu-root", "tray context menu, root level"),
    "tray-submenu": ("bar/tray-menu-submenu", "tray context menu with a submenu open"),
}
# Not reproducible in the real shell (documented in README.md "Unreachable states"):
#   battery popup — its trigger is interactive only while batteryInfo.available, which is
#   only polled while the popup is already open (shell.qml:3342, 4078-4080).


def output_screens(outputs):
    return [{"name": o["name"], "width": o["width"], "height": o["height"], "x": o["x"], "y": o["y"],
             "scale": o["scale"]} for o in outputs]


def move_battery(fx):
    """set.root.batteryInfo -> battery-info.sh stdout + an UPower laptop battery."""
    steps = []
    for step in fx["steps"]:
        info = step.get("set", {}).get("root", {}).get("batteryInfo")
        if info is None:
            steps.append(step)
            continue
        rest = {k: v for k, v in step["set"].items() if k != "root"}
        if set(step["set"].get("root", {})) != {"batteryInfo"}:
            sys.exit("unsupported set.root keys")
        state = {"charging": 1, "discharging": 2, "full": 4}.get(info.get("mode"), 0)
        fx["services"]["upower"] = {"onBattery": info.get("mode") == "discharging", "displayDevice": {
            "isPresent": True, "percentage": float(info["capacity"]), "state": state,
            "energy": info.get("energyNowWh", 0), "energyFull": info.get("energyFullWh", 0),
            "energyRate": abs(info.get("powerW", 0)),
            "timeToFull": info["estimateSeconds"] if state == 1 and info.get("estimateSeconds") else 0,
            "timeToEmpty": info["estimateSeconds"] if state == 2 and info.get("estimateSeconds") else 0}}
        procs = [p for p in fx["processes"] if p["match"] != "battery-info.sh"]
        procs.append({"match": "battery-info.sh", "stdout": json.dumps(info) + "\n"})
        fx["processes"] = procs
        if rest:
            steps.append({"set": rest, **{k: v for k, v in step.items() if k != "set"}})
        elif len(step) > 1:
            steps.append({k: v for k, v in step.items() if k != "set"})
    fx["steps"] = steps


def tray_overflow(base, outputs):
    """Enough tray items that the real 3072-px bar overflows; open the overflow grid."""
    fx = copy.deepcopy(base)
    icons = [t for t in fx["services"]["tray"]]
    tray = []
    for n in range(96):
        t = dict(icons[n % len(icons)])
        t["id"] = f"{t['id']}-{n}"
        tray.append(t)
    fx["services"]["tray"] = tray
    fx["steps"] = [{"click": {"text": "⋯"}}, {"advance": 600}, {"capture": ""}]
    fx["note"] = "tray overflow grid: 96 items overflow the real-width bar (component fixture used a 1000-px output)"
    fx["derived_from"] = "fixtures/bar/tray-overflow-7.json (+ 96 items at real width)"
    fx["spec"] = base.get("spec", "")
    return fx


def main():
    outputs = json.loads((OUT / "outputs.json").read_text())["outputs"]
    if len(outputs) != 1:
        sys.exit("gen-fixtures: multi-output layouts are not wired into the nested compositor yet")
    out = outputs[0]
    written = []
    for state, (src, note) in STATES.items():
        fx = json.loads((SRC_FIXTURES / f"{src}.json").read_text())
        fx["component"] = "fullscreen"
        fx["derived_from"] = f"fixtures/{src}.json"
        fx["note"] = f"{note} — full output; was: {fx.get('note', '')}"
        fx["outputs"] = outputs
        fx["services"]["screens"] = output_screens(outputs)
        fx["crop"] = [0, 0, out["width"], out["height"]]
        fx.pop("targets", None)
        fx.pop("targets_by_theme", None)
        move_battery(fx)
        for step in fx["steps"]:
            if "invoke" in step or "services" in step.get("set", {}) or "root" in step.get("set", {}):
                sys.exit(f"{state}: step not replayable in the real shell: {step}")
        written.append((state, fx))
    base = json.loads((SRC_FIXTURES / "bar/baseline.json").read_text())
    base.update(component="fullscreen", outputs=outputs, crop=[0, 0, out["width"], out["height"]])
    base["services"]["screens"] = output_screens(outputs)
    written.append(("tray-overflow", tray_overflow(base, outputs)))
    for state, fx in written:
        path = OUT / f"{state}.json"
        old = json.loads(path.read_text()) if path.exists() else {}
        for key in ("targets", "targets_by_theme"):
            if key in old:
                fx[key] = old[key]  # keep resolved targets; resolve-targets.py refreshes them
        path.write_text(json.dumps(fx, ensure_ascii=False, indent=1) + "\n")
        print(path.relative_to(PARITY.parent))


if __name__ == "__main__":
    main()
