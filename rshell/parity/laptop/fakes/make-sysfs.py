#!/usr/bin/env python3
"""Build (or update) a fake sysfs tree for the laptop profile under ROOT.

  make-sysfs.py SCENARIO.json ROOT            # create from scenario["sysfs"]
  make-sysfs.py --set ROOT class/power_supply/BAT0/capacity=20 ...   # atomic updates

The shell is pointed at it with HYPRV_SYSFS_ROOT=ROOT (requested contract, see
docs/requests/hyprv-svc-{power,sys}.md). Layout follows the real kernel attribute
names read by the legacy scripts (battery-info.sh, system-snapshot.sh, controls.lua):

  class/power_supply/BAT0/{type,present,status,capacity,energy_now,energy_full,
                           energy_full_design,power_now,voltage_now,manufacturer,model_name}
  class/power_supply/AC/{type,online}
  class/backlight/intel_backlight/{type,brightness,actual_brightness,max_brightness}
  class/thermal/thermal_zoneN/{type,temp}
  class/hwmon/hwmonN/{name,tempK_input,tempK_label}
  class/dmi/id/{sys_vendor,product_name,product_family,chassis_type}

Writes go through a temp file + rename, so inotify readers see one IN_MOVED_TO per value.
"""
import json
import os
import sys
from pathlib import Path

DEFAULT = {
    "dmi": {"sys_vendor": "Dell Inc.", "product_name": "XPS 13 9350", "product_family": "XPS",
            "chassis_type": "10"},
    "battery": {"type": "Battery", "present": "1", "status": "Discharging", "capacity": "57",
                "energy_now": "30780000", "energy_full": "54000000", "energy_full_design": "55000000",
                "power_now": "8400000", "voltage_now": "11900000", "manufacturer": "SMP",
                "model_name": "DELL XPS"},
    "ac": {"type": "Mains", "online": "0"},
    "backlight": {"type": "raw", "brightness": "120", "actual_brightness": "120", "max_brightness": "400"},
    "thermal_zones": [{"type": "acpitz", "temp": "38000"}, {"type": "x86_pkg_temp", "temp": "45000"}],
    "hwmon": [{"name": "coretemp", "temps": [{"input": "45000", "label": "Package id 0"},
                                              {"input": "43000", "label": "Core 0"}]}],
}


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(f".{path.name}.tmp")
    tmp.write_text(f"{value}\n")
    os.replace(tmp, path)


def build(spec, root):
    cfg = {**DEFAULT, **(spec or {})}
    for key, value in cfg["dmi"].items():
        write(root / "class/dmi/id" / key, value)
    if cfg.get("battery") is not None:
        for key, value in cfg["battery"].items():
            write(root / "class/power_supply/BAT0" / key, value)
    if cfg.get("ac") is not None:
        for key, value in cfg["ac"].items():
            write(root / "class/power_supply/AC" / key, value)
    if cfg.get("backlight") is not None:
        for key, value in cfg["backlight"].items():
            write(root / "class/backlight/intel_backlight" / key, value)
    for index, zone in enumerate(cfg.get("thermal_zones") or []):
        for key, value in zone.items():
            write(root / f"class/thermal/thermal_zone{index}" / key, value)
    for index, mon in enumerate(cfg.get("hwmon") or []):
        base = root / f"class/hwmon/hwmon{index}"
        write(base / "name", mon["name"])
        for k, temp in enumerate(mon.get("temps", []), start=1):
            write(base / f"temp{k}_input", temp["input"])
            if temp.get("label"):
                write(base / f"temp{k}_label", temp["label"])


def main(argv):
    if len(argv) >= 3 and argv[1] == "--set":
        root = Path(argv[2])
        for item in argv[3:]:
            rel, _, value = item.partition("=")
            write(root / rel, value)
        return 0
    if len(argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    scenario = json.loads(Path(argv[1]).read_text(encoding="utf-8"))
    root = Path(argv[2])
    build(scenario.get("sysfs"), root)
    print(root)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
