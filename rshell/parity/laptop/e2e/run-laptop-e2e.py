#!/usr/bin/env python3
"""Laptop-profile e2e scaffold (docs/verify/laptop-qa-plan.md §4).

  python3 -I parity/laptop/e2e/run-laptop-e2e.py [--shell BIN] [--report FILE]

1. Self-tests the fakes on a private dbus-daemon: fake UPower / PowerProfiles (both
   names) / login1 answer, emit PropertiesChanged and log client writes; the fake sysfs
   tree has every attribute the readers need; the hypr.controls stub's plan() matches
   the closed form for L = 0..100.
2. Probes the shell binary for the laptop hooks it needs (HYPRV_PROFILE,
   HYPRV_SYSFS_ROOT, the laptop functional suite). A case whose prerequisites are
   missing is PENDING with the missing hook named; nothing is faked into a PASS.
3. When the shell has the laptop suite, runs it nested (HYPRV_PROFILE=laptop,
   1440x900 @2, HYPRV_FUNCTIONAL_FILTER=lap. so the desktop docs/verify/e2e.md is never
   rewritten) with DBUS_SESSION_BUS_ADDRESS = the private fake bus (the nested driver
   forwards it as the system bus too), HYPRV_SYSFS_ROOT = the fake tree and the
   hypr.controls stub, and merges its manifest by case ID. A case the suite does not
   report stays PENDING. --desktop also runs the whole desktop suite under auto-detect
   for lap.desktop.regression, with an empty filter (matches every ID, but keeps the
   report in the run directory instead of docs/verify/e2e.md) and
   HYPRV_FUNCTIONAL_LIFECYCLE=1 so the lifecycle cases still run.

Writes docs/verify/laptop-e2e.md. Nothing touches the live session: private bus,
private sysfs tree under a temp dir, nested compositors only.
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
FAKES = HERE.parent / "fakes"
REPO = HERE.parents[2]

# (id, action, expected, requires). Requires: profile, sysfs, suite (laptop functional
# suite in hyprv-shell), plus fakes (always provided here).
CASES = [
    ("lap.profile.detect.dmi", "no override; fake DMI XPS 13 9350 + BAT0", "laptop profile; reason logged", ["profile", "sysfs"]),
    ("lap.profile.detect.desktop", "fake desktop DMI, no battery", "desktop profile; desktop subset unchanged", ["profile", "sysfs"]),
    ("lap.profile.override", "HYPRV_PROFILE=desktop on XPS fakes and vice versa", "override wins", ["profile"]),
    ("lap.bar.boot.compact", "start", "all bars compact, exclusive zone 25, no island content", ["profile", "suite"]),
    ("lap.bar.toggle", "ipc call bar toggle eDP-1", "expanded after the slide; layoutStatus keys = legacy", ["profile", "suite"]),
    ("lap.bar.toggle.debounce", "two toggles 300 ms apart", "one transition", ["profile", "suite"]),
    ("lap.bar.toggle.synced", "two outputs, toggle one", "both expand together", ["profile", "suite"]),
    ("lap.battery.discharge.tick", "UPower Percentage 21 -> 20", "glyph/colour flip, no other damage", ["profile", "suite"]),
    ("lap.battery.plug", "State Discharging -> Charging, OnBattery false", "plug glyph", ["profile", "suite"]),
    ("lap.battery.full", "FullyCharged", "full rendering", ["profile", "suite"]),
    ("lap.battery.popup.open", "click the battery module", "popup; fake sysfs history read; estimate text per mode", ["profile", "sysfs", "suite"]),
    ("lap.battery.popup.close", "click outside / Esc", "closes; 1 s sampling stops", ["profile", "sysfs", "suite"]),
    ("lap.power.profile.set", "CP profile tile -> power-saver", "fake PPD ActiveProfile set once; UI follows signal", ["profile", "suite"]),
    ("lap.power.profile.external", "fake changes profile", "bar/CP update, no command", ["profile", "suite"]),
    ("lap.brightness.slider", "drag CP slider 50 -> 71", "ordered BrightnessSet per distinct value, last 71; never BrightnessFinish; one read-back 90 ms after the last set, no overwrite mid-drag", ["profile", "suite"]),
    ("lap.brightness.wheel", "wheel one notch on the CP slider; IPC step", "wheel ignored (laptop ControlPanelSlider.qml has no wheel handler); IPC/key step 1", ["profile", "suite"]),
    ("lap.brightness.osd", "ipc quickAdjust showBrightnessLevel 70", "OSD at 70", ["profile", "suite"]),
    ("lap.brightness.plan", "Lua plan(L), L = 0..100", "matches the closed form", []),
    ("lap.lid.close_open", "login1 LidClosed true -> false", "no UI change, no wakeup burst", ["profile", "suite"]),
    ("lap.sleep.resume", "PrepareForSleep(true) then (false)", "state re-synced within one frame", ["profile", "suite"]),
    ("lap.thermal.source", "zones: pkg -> hwmon only -> none", "temperature source and --", ["profile", "sysfs", "suite"]),
    ("lap.media.compact.idle", "MPRIS playing, bar compact", "no cava, no position reads", ["profile", "suite"]),
    ("lap.media.expand", "toggle bar while playing", "track + position on first frame", ["profile", "suite"]),
    ("lap.island.hidden.compact", "agent active, bar compact", "no island draw; model still updates", ["profile", "suite"]),
    ("lap.island.agent.focus", "click a session row", "focus action", ["profile", "suite"]),
    ("lap.island.readonly", "pending approval in state.json, expand", "no pending card or buttons (main semantics)", ["profile", "suite"]),
    ("lap.island.hook.nonblocking", "hyprv-agent-hook PermissionRequest", "exits at once; installer timeouts 5", ["profile", "suite"]),
    ("lap.island.list_inplace", "9 sessions, scroll, write state.json", "scroll and rows kept", ["profile", "suite"]),
    ("lap.cp.nofans", "open the CP", "no fan section; no fan D-Bus calls", ["profile", "suite"]),
    ("lap.privhelper.inert", "laptop profile start", "no GetFans/SubscribeCpuPower/RemoveExternalGpu; no root units planned", ["profile", "suite"]),
    ("lap.egpu.ui", "open the CP, no eGPU device", "no eGPU card (laptop confirmEgpuAction() is never called); no root service, inactive channel rejects actions", ["profile", "suite"]),
    ("lap.strip.control", "click the compact-strip control-panel icon", "CP opens anchored to the strip", ["profile", "suite"]),
    ("lap.strip.stats", "compact strip stats", "CPU % / MEM % / power text from minimal sampling", ["profile", "suite"]),
    ("lap.strip.clock", "compact strip clock", "minute-aligned hh:mm", ["profile", "suite"]),
    ("lap.strip.workspace", "click a strip workspace", "workspace switch action", ["profile", "suite"]),
    ("ipc.shell.kill", "hyprv-shell kill at suite end", "clean shutdown acknowledged, IPC socket removed", ["profile", "suite"]),
    ("lap.wifi.scan.visible", "open the Wi-Fi page", "scan enabled only while visible", ["profile", "suite"]),
    ("lap.desktop.regression", "desktop nested e2e + parity, profile auto on this desktop", "204/204, parity counts unchanged", ["profile"]),
]

FAKE_CHECKS = []


def check(name):
    def wrap(fn):
        FAKE_CHECKS.append((name, fn))
        return fn
    return wrap


class Bus:
    """Private dbus-daemon + fake-system-bus.py."""

    def __init__(self, work, scenario):
        self.work = work
        self.daemon = subprocess.Popen(
            ["dbus-daemon", "--session", "--nofork", "--print-address=1",
             f"--address=unix:dir={work}"], stdout=subprocess.PIPE, text=True)
        self.address = self.daemon.stdout.readline().strip()
        self.log = work / "fake-bus.ndjson"
        scenario_file = work / "scenario.json"
        scenario_file.write_text(json.dumps(scenario))
        self.fake = subprocess.Popen(
            [sys.executable, "-I", str(FAKES / "fake-system-bus.py"), "--address", self.address,
             "--scenario", str(scenario_file), "--log", str(self.log)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        ready = self.fake.stdout.readline()
        if not ready.startswith("ready"):
            raise RuntimeError(f"fake bus did not start: {ready!r}")

    def step(self, step):
        self.fake.stdin.write(json.dumps(step) + "\n")
        self.fake.stdin.flush()

    def close(self):
        for proc in (self.fake, self.daemon):
            proc.terminate()
            try:
                proc.wait(5)
            except subprocess.TimeoutExpired:
                proc.kill()


def gdbus(bus, *args):
    out = subprocess.run(["gdbus", "call", "--address", bus.address, *args], capture_output=True, text=True, timeout=10)
    if out.returncode:
        raise AssertionError(out.stderr.strip())
    return out.stdout.strip()


def prop(bus, dest, path, iface, name):
    return gdbus(bus, "--dest", dest, "--object-path", path, "--method", "org.freedesktop.DBus.Properties.Get",
                 iface, name)


@check("fake UPower: display device, OnBattery, enumerate")
def _upower(bus, _root):
    assert "/org/freedesktop/UPower/devices/DisplayDevice" in gdbus(
        bus, "--dest", "org.freedesktop.UPower", "--object-path", "/org/freedesktop/UPower",
        "--method", "org.freedesktop.UPower.GetDisplayDevice")
    assert "true" in prop(bus, "org.freedesktop.UPower", "/org/freedesktop/UPower", "org.freedesktop.UPower", "OnBattery")
    pct = prop(bus, "org.freedesktop.UPower", "/org/freedesktop/UPower/devices/DisplayDevice",
               "org.freedesktop.UPower.Device", "Percentage")
    assert "57" in pct, pct
    assert "battery_BAT0" in gdbus(bus, "--dest", "org.freedesktop.UPower", "--object-path", "/org/freedesktop/UPower",
                                   "--method", "org.freedesktop.UPower.EnumerateDevices")


@check("fake UPower: scenario step emits PropertiesChanged and updates state")
def _upower_signal(bus, _root):
    mon = subprocess.Popen(["gdbus", "monitor", "--address", bus.address, "--dest", "org.freedesktop.UPower"],
                           stdout=subprocess.PIPE, text=True)
    time.sleep(0.3)
    bus.step({"battery": {"Percentage": 20.0, "State": 1}, "on_battery": False})
    time.sleep(0.5)
    mon.terminate()
    out = mon.communicate(timeout=5)[0]
    assert "PropertiesChanged" in out and "Percentage" in out and "OnBattery" in out, out[-400:]
    assert "20" in prop(bus, "org.freedesktop.UPower", "/org/freedesktop/UPower/devices/DisplayDevice",
                        "org.freedesktop.UPower.Device", "Percentage")


@check("fake PowerProfiles: both names, client write logged and mirrored")
def _profiles(bus, _root):
    for dest, path in (("org.freedesktop.UPower.PowerProfiles", "/org/freedesktop/UPower/PowerProfiles"),
                       ("net.hadess.PowerProfiles", "/net/hadess/PowerProfiles")):
        assert "balanced" in prop(bus, dest, path, dest, "ActiveProfile")
    gdbus(bus, "--dest", "net.hadess.PowerProfiles", "--object-path", "/net/hadess/PowerProfiles",
          "--method", "org.freedesktop.DBus.Properties.Set", "net.hadess.PowerProfiles", "ActiveProfile",
          "<'power-saver'>")
    assert "power-saver" in prop(bus, "org.freedesktop.UPower.PowerProfiles", "/org/freedesktop/UPower/PowerProfiles",
                                 "org.freedesktop.UPower.PowerProfiles", "ActiveProfile")
    log = [json.loads(line) for line in bus.log.read_text().splitlines()]
    assert any(e.get("event") == "set" and e.get("member") == "ActiveProfile" for e in log)


@check("fake login1: LidClosed, PrepareForSleep signal, Inhibit returns an fd")
def _login(bus, _root):
    assert "false" in prop(bus, "org.freedesktop.login1", "/org/freedesktop/login1",
                           "org.freedesktop.login1.Manager", "LidClosed")
    mon = subprocess.Popen(["gdbus", "monitor", "--address", bus.address, "--dest", "org.freedesktop.login1"],
                           stdout=subprocess.PIPE, text=True)
    time.sleep(0.3)
    bus.step({"lid_closed": True, "prepare_for_sleep": True})
    time.sleep(0.5)
    mon.terminate()
    out = mon.communicate(timeout=5)[0]
    assert "PrepareForSleep" in out and "LidClosed" in out, out[-400:]
    out = gdbus(bus, "--dest", "org.freedesktop.login1", "--object-path", "/org/freedesktop/login1",
                "--method", "org.freedesktop.login1.Manager.Inhibit", "sleep", "hyprv", "test", "delay")
    assert "handle" in out or "0" in out, out


@check("org.hyprv.Test.Step: presets apply synchronously and emit their signals")
def _test_driver(bus, _root):
    def step(arg):
        out = gdbus(bus, "--dest", "org.hyprv.Test", "--object-path", "/org/hyprv/Test",
                    "--method", "org.hyprv.Test.Step", arg)
        return json.loads(out[2:-3].encode().decode("unicode_escape"))  # ('...',)
    mon = subprocess.Popen(["dbus-monitor", "--address", bus.address, "type='signal'"],
                           stdout=subprocess.PIPE, text=True)
    time.sleep(0.3)
    try:
        st = step("unplug")
        assert st["on_battery"] and st["battery"]["State"] == 2, st
        st = step("plug")
        assert not st["on_battery"] and st["battery"]["State"] == 1, st
        st = step("full")
        assert st["battery"]["State"] == 4 and st["battery"]["Percentage"] == 100.0, st
        st = step("tick:21")
        assert st["battery"]["Percentage"] == 21.0, st
        assert step("lid-close")["lid_closed"] and not step("lid-open")["lid_closed"]
        assert step("suspend")["preparing"] and not step("resume")["preparing"]
        assert step("profile:performance")["profile"] == "performance"
        assert step('{"profile": "balanced"}')["profile"] == "balanced"
        bad = subprocess.run(["gdbus", "call", "--address", bus.address, "--dest", "org.hyprv.Test",
                              "--object-path", "/org/hyprv/Test", "--method", "org.hyprv.Test.Step", "nope"],
                             capture_output=True, text=True, timeout=10)
        assert bad.returncode and "InvalidArgs" in bad.stderr, bad.stderr
    finally:
        time.sleep(0.3)
        mon.terminate()
    out = mon.communicate(timeout=5)[0]
    for needle in ("OnBattery", '"State"', "Percentage", "LidClosed", "LidIsClosed", "PrepareForSleep",
                   "ActiveProfile"):
        assert needle in out, f"no {needle} signal"
    assert out.count("PrepareForSleep") >= 2, "suspend and resume"


@check("fake sysfs: battery, AC, backlight, thermal, hwmon, DMI attributes")
def _sysfs(_bus, root):
    need = ["class/power_supply/BAT0/capacity", "class/power_supply/BAT0/status", "class/power_supply/BAT0/energy_now",
            "class/power_supply/BAT0/power_now", "class/power_supply/AC/online",
            "class/backlight/intel_backlight/max_brightness", "class/thermal/thermal_zone1/type",
            "class/hwmon/hwmon0/temp1_input", "class/dmi/id/product_name"]
    missing = [n for n in need if not (root / n).is_file()]
    assert not missing, missing
    assert (root / "class/thermal/thermal_zone1/type").read_text().strip() == "x86_pkg_temp"
    subprocess.run([sys.executable, "-I", str(FAKES / "make-sysfs.py"), "--set", str(root),
                    "class/power_supply/BAT0/capacity=20"], check=True)
    assert (root / "class/power_supply/BAT0/capacity").read_text().strip() == "20"


def lua_plan_table():
    lua = shutil.which("lua") or shutil.which("luajit")
    if not lua:
        return None, "no lua interpreter"
    script = ("package.path='" + str(FAKES) + "/?.lua;'..package.path; local M=require('hypr.controls');"
              "for l=0,100 do local r,d,c=M.plan(l); print(l,r,d,c) end")
    out = subprocess.run([lua, "-e", script], capture_output=True, text=True, env={**os.environ,
                         "HYPRV_FAKE_CONTROLS_DIR": tempfile.gettempdir()})
    if out.returncode:
        return False, out.stderr.strip()
    bad = []
    for line in out.stdout.split("\n"):
        if not line.strip():
            continue
        level, raw, desk, client = (float(v) for v in line.split())
        hw = level if level <= 10 else 10 * 10 ** ((min(level, 70) - 10) / 60)
        want_raw = min(400, max(1, int(400 * hw / 100 + 0.5)))
        t = min(1, max(0, (level - 70) / 30))
        want_white = int(153 + 247 * t + 0.5)
        if (int(raw), int(desk), int(client)) != (want_raw, want_white, want_white):
            bad.append((level, raw, desk, want_raw, want_white))
    return (not bad), (f"101 levels match" if not bad else f"mismatch {bad[:3]}")


def shell_capabilities(binary):
    caps = {"profile": False, "sysfs": False, "suite": False}
    if not binary.is_file():
        return caps, f"shell binary missing: {binary}"
    data = binary.read_bytes()
    caps["profile"] = b"HYPRV_PROFILE" in data
    caps["sysfs"] = b"HYPRV_SYSFS_ROOT" in data
    caps["suite"] = b"lap.bar.toggle" in data
    return caps, f"{binary} ({len(data) >> 20} MiB)"


MISSING = {
    "profile": "HYPRV_PROFILE / profile detection not in the shell binary (ARCHITECTURE.md profile contract, Phase B pending)",
    "sysfs": "HYPRV_SYSFS_ROOT not honoured (docs/requests/hyprv-svc-power.md, hyprv-svc-sys.md)",
    "suite": "laptop cases not in hyprv-shell --nested-functional-test (docs/requests/hyprv-shell.md)",
}


def stage_assets(dest):
    """Private HYPRV_ASSET_ROOT for the nested shell (runtime_assets.rs: installed,
    content-addressed). Every payload of the catalog compiled into this checkout is copied
    from the first source whose SHA-256 matches; nothing is installed under HOME."""
    import hashlib
    catalog_file = REPO / "crates/hyprv-shell/src/runtime-assets.json"
    catalog = json.loads(catalog_file.read_text())
    roots = [REPO / "packaging/dist/release/runtime", REPO.parent / "quickshell",
             Path.home() / ".local/share/hyprv-shell", Path.home() / ".local/share"]
    dest.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(catalog_file, dest / "runtime-assets.json")
    missing = []
    for name, digest in catalog["files"].items():
        for root in roots:
            source = root / name
            if source.is_file() and hashlib.sha256(source.read_bytes()).hexdigest() == digest:
                (dest / name).parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, dest / name)
                break
        else:
            missing.append(name)
    return missing


def nested_suite(binary, work, bus, sysroot, profile, case_filter, timeout):
    """One `--nested-functional-test` run; returns ({id: row}, note)."""
    out = work / f"nested-{profile}"
    controls = work / "controls"
    controls.mkdir(exist_ok=True)
    env = {**os.environ,
           "HYPRV_PROFILE": profile,
           "HYPRV_FUNCTIONAL_FILTER": case_filter,
           "DBUS_SESSION_BUS_ADDRESS": bus.address,
           "DBUS_SYSTEM_BUS_ADDRESS": bus.address,  # -> HYPRV_TEST_SYSTEM_BUS_ADDRESS (org.hyprv.Test lives here)
           "HYPRV_SYSFS_ROOT": str(sysroot),
           "HYPRV_FAKE_CONTROLS_LUA": str(FAKES),
           "HYPRV_FAKE_CONTROLS_DIR": str(controls)}
    if profile == "desktop":
        env.pop("HYPRV_PROFILE")  # lap.desktop.regression: auto-detect on this desktop
        env.pop("HYPRV_SYSFS_ROOT")
        env["HYPRV_FUNCTIONAL_LIFECYCLE"] = "1"
    if profile == "laptop":
        env.update(HYPRV_NESTED_WIDTH="1440", HYPRV_NESTED_HEIGHT="900", HYPRV_NESTED_SCALE="2")
    missing = stage_assets(work / "assets")
    if missing:
        return {}, f"nested {profile}: cannot stage runtime assets, no matching source for {missing[:3]}"
    env["HYPRV_ASSET_ROOT"] = str(work / "assets")
    log = work / f"nested-{profile}.log"
    try:
        with open(log, "w") as fh:
            proc = subprocess.run([str(binary), "--nested-functional-test", str(out)], env=env,
                                  stdout=fh, stderr=subprocess.STDOUT, timeout=timeout)
        code = proc.returncode
    except subprocess.TimeoutExpired:
        code = "timeout"
    manifest = out / "manifest.json"
    if not manifest.is_file():
        tail = log.read_text(errors="replace")[-300:].replace("\n", " ")
        return {}, f"nested {profile} run produced no manifest (exit {code}): {tail}"
    data = json.loads(manifest.read_text())
    rows = {row.get("id"): row for row in data.get("cases", [])}
    note = f"nested {profile}: exit {code}, suite_complete={data.get('suite_complete')}, {len(rows)} cases"
    return rows, note


def row_result(row):
    status = row.get("status", "FAIL")
    detail = row.get("detail") if status == "PASS" else row.get("error")
    text = json.dumps(detail, ensure_ascii=False) if not isinstance(detail, str) else detail
    return status, (text or "")[:300].replace("|", "/").replace("\n", " ")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--shell", default=str(REPO / "target/release/hyprv-shell"))
    parser.add_argument("--report", default=str(REPO / "docs/verify/laptop-e2e.md"))
    parser.add_argument("--desktop", action="store_true",
                        help="also run the desktop suite under auto-detect for lap.desktop.regression")
    parser.add_argument("--timeout", type=int, default=3600)
    parser.add_argument("--keep", help="copy the nested run directories (logs, manifests) here before cleanup")
    args = parser.parse_args()

    runtime = Path(os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}"))
    work = Path(tempfile.mkdtemp(prefix="hvl.", dir=runtime))  # short path: unix socket limit
    results, fake_rows, run_notes = {}, [], []
    try:
        scenario = json.loads((FAKES / "scenarios/discharging-57.json").read_text())
        sysroot = work / "sys"
        subprocess.run([sys.executable, "-I", str(FAKES / "make-sysfs.py"), str(FAKES / "scenarios/discharging-57.json"),
                        str(sysroot)], check=True, capture_output=True)
        bus = Bus(work, scenario)
        try:
            for name, fn in FAKE_CHECKS:
                try:
                    fn(bus, sysroot)
                    fake_rows.append((name, "PASS", ""))
                except Exception as error:  # noqa: BLE001 - report every failure
                    fake_rows.append((name, "FAIL", str(error)[:200]))
        finally:
            bus.close()
        ok, detail = lua_plan_table()
        fake_rows.append(("hypr.controls stub plan() = closed form", "PASS" if ok else ("PENDING" if ok is None else "FAIL"), detail))

        caps, shell_note = shell_capabilities(Path(args.shell))
        nested = {}
        if caps["profile"] and caps["suite"]:
            # Fresh world: the self-test above mutated the fake bus and tree.
            run_root = work / "run"
            run_root.mkdir()
            # HYPRV_SYSFS_ROOT is a filesystem root (hyprv_core::profile::sys_path("sys/...")):
            # the generated sysfs tree goes to <root>/sys.
            subprocess.run([sys.executable, "-I", str(FAKES / "make-sysfs.py"),
                            str(FAKES / "scenarios/discharging-57.json"), str(run_root / "fs" / "sys")],
                           check=True, capture_output=True)
            run_bus = Bus(run_root, scenario)
            try:
                nested, note = nested_suite(Path(args.shell), run_root, run_bus, run_root / "fs",
                                            "laptop", "lap.", args.timeout)
                run_notes.append(note)
            finally:
                run_bus.close()
        if args.desktop and caps["profile"]:
            desk_root = work / "desk"
            desk_root.mkdir()
            desk_bus = Bus(desk_root, json.loads((FAKES / "scenarios/no-battery-desktop-dmi.json").read_text()))
            try:
                rows, note = nested_suite(Path(args.shell), desk_root, desk_bus, Path("/"), "desktop", "",
                                          args.timeout)
            finally:
                desk_bus.close()
            run_notes.append(note)
            passed = sum(1 for r in rows.values() if r.get("status") == "PASS")
            nested["lap.desktop.regression"] = {
                "status": "PASS" if rows and passed == len(rows) == 204 else "FAIL",
                "detail": f"desktop suite {passed}/{len(rows)} (want 204/204); parity counts: see parity gates"}
        for case_id, action, expected, needs in CASES:
            if case_id == "lap.brightness.plan" and case_id not in nested:
                results[case_id] = ("PASS" if ok else "FAIL", f"stub table: {detail}; nested Hyprland eval pending suite")
                continue
            if case_id in nested:
                results[case_id] = row_result(nested[case_id])
                continue
            missing = [n for n in needs if not caps.get(n)]
            if missing:
                results[case_id] = ("PENDING", "; ".join(MISSING[n] for n in missing))
            else:
                results[case_id] = ("PENDING", "not reported by the nested laptop suite")
    finally:
        if args.keep:
            for sub in ("run", "desk"):
                if (work / sub).is_dir():
                    shutil.copytree(work / sub, Path(args.keep) / sub, dirs_exist_ok=True, symlinks=True,
                                    ignore=shutil.ignore_patterns("*.sock", "bus*", "wayland-*", "hyprland*"),
                                    ignore_dangling_symlinks=True)
        shutil.rmtree(work, ignore_errors=True)

    fake_fail = sum(1 for r in fake_rows if r[1] == "FAIL")
    lines = [
        "# Laptop e2e — scaffold status",
        "",
        f"Generated {time.strftime('%Y-%m-%d %H:%M %Z')} by `parity/laptop/e2e/run-laptop-e2e.py`. Plan:",
        "`docs/verify/laptop-qa-plan.md` §4. Shell probed: " + shell_note + ".",
        "",
    ] + [f"- {n}" for n in run_notes] + [
        "",
        "## Fakes self-test (private dbus-daemon, temp sysfs tree)",
        "",
        "| check | result | detail |",
        "| --- | --- | --- |",
    ] + [f"| {n} | {r} | {d.replace('|', '/')} |" for n, r, d in fake_rows] + [
        "",
        "## Cases",
        "",
        "PENDING names the missing shell hook. No case is passed by the scaffold itself, except",
        "the table check of the brightness stub.",
        "",
        "| ID | action | expected | result | note |",
        "| --- | --- | --- | --- | --- |",
    ] + [f"| `{cid}` | {a} | {e} | {results[cid][0]} | {results[cid][1]} |" for cid, a, e, _ in CASES]
    Path(args.report).write_text("\n".join(lines) + "\n")
    counts = {}
    for status, _ in results.values():
        counts[status] = counts.get(status, 0) + 1
    print(f"fakes: {len(fake_rows) - fake_fail}/{len(fake_rows)} ok; cases: {counts}; report {args.report}")
    return 1 if fake_fail else 0


if __name__ == "__main__":
    sys.exit(main())
