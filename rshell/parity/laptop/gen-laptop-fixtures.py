#!/usr/bin/env python3
"""Regenerate parity/laptop/fixtures/<component>/*.json for the laptop (Dell XPS) profile.

Reference source: origin/laptop@24972f7 (see capture-laptop.sh), screen eDP-1
2880x1800 at scale 2 = 1440x900 logical. Plans: docs/verify/laptop-qa-plan.md §3,
docs/spec/laptop-{bar,panels,quickadjust,island}.md.

Components:
  laptop-bar            adaptive bar (compact strip / full bar), battery, island host
  laptop-panels         control panel, battery popup, QuickAdjust on the laptop geometry
  laptop-island-legacy  legacy interactive pending card (D1 = read-only: expected
                        divergence, recorded only, never gated)

The laptop shell polls system-snapshot.sh (--minimal every 5 s while compact, --full
every 1 s while detailed) and streams agent-island-watch.py; both are canned here.
"""
import copy
import importlib.util
import json
import sys
from pathlib import Path

TOOLS = Path(__file__).resolve().parent.parent / "tools"


def _load(name, file):
    spec = importlib.util.spec_from_file_location(name, TOOLS / file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


bar = _load("genbar", "gen-bar-fixtures.py")
isl = _load("genisland", "gen-island-fixtures.py")

SOURCE_REF = "origin/laptop@24972f7"
SCREEN = {"name": "eDP-1", "width": 1440, "height": 900, "x": 0, "y": 0, "scale": 2}
NOW = "2026-10-23T09:41:00"
BAR_CROP = [0, 0, 1440, 70]
HOST_CROP = [0, 0, 1440, 300]      # bar + expanded island
FULL_CROP = [0, 0, 1440, 900]      # panels: whole output
SETTLE = 5600                      # second --minimal sample lands at 5000 ms

# UPowerDeviceState (qml-mocks/Quickshell/Services/UPower/UPowerDeviceState.qml)
UNKNOWN, CHARGING, DISCHARGING, EMPTY, FULL, PENDING_CHARGE, PENDING_DISCHARGE = range(7)


def upower(percent=57, state=DISCHARGING, present=True, on_battery=None):
    if on_battery is None:
        on_battery = state in (DISCHARGING, PENDING_DISCHARGE, EMPTY)
    return {"onBattery": on_battery,
            "displayDevice": {"ready": True, "isPresent": present, "isLaptopBattery": present,
                              "percentage": percent / 100.0, "state": state}}


def _minimal(text):
    """system-snapshot.sh --minimal: first /proc/stat line, no temperature, no __NET__."""
    out, section = [], None
    for line in text.split("\n"):
        if line.startswith("__") and line.endswith("__"):
            section = line
            if section == "__NET__":
                break
            out.append(line)
            continue
        if section == "__STAT__" and not line.startswith("cpu "):
            continue
        if section == "__TEMP__":
            continue
        out.append(line)
    return "\n".join(out)


def snapshot(ticks=40, **system):
    system.setdefault("iface", "wlan0")
    system.setdefault("power", None)
    full = [bar.system_snapshot(1000 + i, **system) for i in range(ticks)]
    minimal = [_minimal(bar.system_snapshot(i, **system)) for i in range(ticks)]
    return [
        {"match": "system-snapshot.sh --minimal", "sequence": [{"stdout": s} for s in minimal]},
        {"match": "system-snapshot.sh --full", "sequence": [{"stdout": s} for s in full]},
    ]


def agent_watch(sessions=None, pending=None, later=None):
    def snap(sess, pend):
        return json.dumps({"version": 1, "active": bool(sess), "count": len(sess or []), "sessions": sess or [],
                           "pending": pend, "pending_count": 1 if pend else 0, "updated_at": 0},
                          ensure_ascii=False, separators=(",", ":"))
    lines = [{"at": 0, "data": snap(sessions, pending)}]
    for at, sess, pend in later or []:
        lines.append({"at": at, "data": snap(sess, pend)})
    return {"match": "agent-island-watch.py", "stream": True, "lines": lines}


def battery_info(mode="discharging", capacity=57, power=8.4, average=7.9, estimate=14_400, basis="average",
                 samples=40, complete=True):
    if mode == "discharging":  # battery-info.sh signs discharge power negative
        power, average = -abs(power), -abs(average)
    status = {"charging": "Charging", "discharging": "Discharging", "full": "Full",
              "plugged": "Not charging"}.get(mode, "Unknown")
    return json.dumps({"available": True, "status": status, "mode": mode, "capacity": capacity,
                       "powerW": power, "averagePowerW": average, "sampleCount": samples,
                       "sampleWindowSeconds": 1800 if complete else 60, "windowComplete": complete,
                       "estimateSeconds": estimate, "estimateBasis": basis,
                       "energyNowWh": round(54.0 * capacity / 100, 2), "energyFullWh": 54.0})


def cp_status(brightness=50, profile="balanced", dnd=False, recording=False):
    b = lambda v: "true" if v else "false"
    # Laptop control-panel-status.sh prints fan_available=false (script line 43).
    return "\n".join([f"brightness={brightness}", f"dnd={b(dnd)}", f"recording={b(recording)}",
                      f"power_profile={profile}", "prevent_sleep=false", "demo_mode=false",
                      "fan_available=false"]) + "\n"


def processes(system=None, notification="none", cava=True, sessions=None, pending=None, later=None,
              brightness=50, profile="balanced", info=None, playing=None):
    procs = snapshot(**(system or {}))
    procs += [
        agent_watch(sessions, pending, later),
        {"match": "agent-island-codex-appserver.py", "exitCode": 0},
        {"match": "swaync-client -swb", "stream": True,
         "lines": [{"at": 0, "data": json.dumps({"text": "", "alt": notification, "tooltip": "", "class": notification})}]},
        {"match": "command -v cava", "exitCode": 0 if cava else 1},
        {"match": "hyprctl -j animations", "stdout": bar.ANIMATIONS},
        {"match": "Fluent-light", "stdout": bar.FLUENT},
        {"match": "ui-state.sh print", "stdout": "theme=light\n"},
        {"match": "audio-spectrum.sh", "stream": True,
         "lines": [{"at": 0, "data": "12;38;74;92;66;41;58;80;95;61;33;47;70;52;28;15;"}]},
        bar.media(playing),
        {"match": "control-panel-status.sh", "stdout": cp_status(brightness, profile)},
        {"match": "brightness_get", "stdout": f"{brightness}\n"},
        {"match": "brightness_set", "stdout": ""},
        {"match": "brightness_finish", "stdout": ""},
        {"match": "battery-info.sh", "stdout": info or battery_info()},
        {"match": "dcal --json socket", "exitCode": 1},
    ]
    return procs


PLAYER = {"identity": "Spotify", "dbusName": "org.mpris.MediaPlayer2.spotify", "isPlaying": True,
          "playbackState": 1, "trackTitle": "Midnight City", "trackArtist": "M83",
          "trackArtUrl": "file://@ASSETS@/cover-warm.png", "trackAlbum": "Hurry Up, We're Dreaming",
          "length": 225, "lengthSupported": True, "position": 83, "positionSupported": True}

WIFI_DEVICE = [{"name": "wlan0", "type": "wifi", "connected": True, "networks": [
    {"name": "Home-5G", "signal": 0.75, "security": "Wpa2Psk", "known": True, "connected": True},
    {"name": "Cafe", "signal": 0.40, "security": "Open", "known": False, "connected": False}]}]


def services(battery=None, playing=False, wifi=True, fullscreen=False, profile="balanced", **over):
    base = {
        "screens": [SCREEN],
        "hyprland": {"workspaces": [{"id": i, "name": str(i)} for i in range(1, 6)], "focusedWorkspace": 2,
                     "activeToplevel": {"title": "Firefox — laptop profile"}, "fullscreen": fullscreen},
        "audio": {"available": True, "volume": 0.35, "muted": False},
        # playing: False -> no player, True -> playing, "paused" -> paused with metadata.
        "mpris": {"players": [dict(PLAYER, isPlaying=playing is True, playbackState=1 if playing is True else 2)]
                  if playing else []},
        "tray": bar.TRAY[:3],
        "bluetooth": {"enabled": True, "devices": []},
        "powerProfile": profile,
        "upower": battery if battery is not None else upower(),
        "wifi": {"enabled": True, "hardwareEnabled": True, "devices": WIFI_DEVICE if wifi else []},
    }
    base.update(over)
    return base


fixtures = {}


def fx(component, name, note, crop, steps, svc=None, procs=None, settle=SETTLE, tick_ms=None, spec=None):
    data = {"component": component, "scene": "desktop.qml", "entry": "shell.qml", "source_ref": SOURCE_REF,
            "spec": spec or "docs/verify/laptop-qa-plan.md §3", "note": note, "now": NOW,
            "fonts": ["JetBrainsMono Nerd Font"], "crop": crop, "settle_ms": settle,
            "services": svc or services(), "processes": procs or processes(), "props": {}, "steps": steps}
    if tick_ms:
        data["tick_ms"] = tick_ms
    fixtures.setdefault(component, {})[name] = data


EXPAND = {"set": {"ipc": ["bar", "toggle", "eDP-1"]}}
CAP = [{"capture": ""}]
BARSPEC = "docs/spec/laptop-bar.md §14"

# ------------------------------------------------------------ laptop-bar: compact strip
fx("laptop-bar", "lap-compact-idle", "compact strip, ws 2, battery 57 % discharging, Wi-Fi connected", BAR_CROP, CAP,
   spec=BARSPEC)
fx("laptop-bar", "lap-compact-charging", "compact, 57 % charging (plug glyph)", BAR_CROP, CAP,
   svc=services(battery=upower(57, CHARGING)), spec=BARSPEC)
fx("laptop-bar", "lap-compact-critical", "compact, 18 % discharging (critical colour)", BAR_CROP, CAP,
   svc=services(battery=upower(18, DISCHARGING)), spec=BARSPEC)
fx("laptop-bar", "lap-compact-idle-mode", "UPower state Unknown with OnBattery=true", BAR_CROP, CAP,
   svc=services(battery=upower(57, UNKNOWN, on_battery=True)), spec=BARSPEC)
fx("laptop-bar", "lap-compact-full", "FullyCharged 100 %", BAR_CROP, CAP,
   svc=services(battery=upower(100, FULL)))
fx("laptop-bar", "lap-compact-nobattery", "no battery: CPU power text fallback", BAR_CROP, CAP,
   svc=services(battery=upower(0, UNKNOWN, present=False, on_battery=False)), spec=BARSPEC)
fx("laptop-bar", "lap-compact-wired", "Wi-Fi disconnected glyph", BAR_CROP, CAP,
   svc=services(wifi=False), procs=processes(system={"iface": ""}), spec=BARSPEC)
fx("laptop-bar", "lap-compact-dnd-dot", "notifications DND with unread dot", BAR_CROP, CAP,
   procs=processes(notification="dnd-notification"), spec=BARSPEC)
for pct, state in ((20, DISCHARGING), (21, DISCHARGING), (20, CHARGING)):
    fx("laptop-bar", f"lap-compact-boundary-{pct}-{'chg' if state == CHARGING else 'dis'}",
       "critical boundary: <= 20 and not plugged is red", BAR_CROP, CAP, svc=services(battery=upower(pct, state)))
for pct in (0, 4, 5, 14, 15, 50, 94, 95, 100):
    fx("laptop-bar", f"lap-glyph-{pct:03d}", "discharging glyph bucket round(p/10)", BAR_CROP, CAP,
       svc=services(battery=upower(pct, DISCHARGING)))

# ------------------------------------------------------------ laptop-bar: reveal / full bar
fx("laptop-bar", "lap-expand", "compact -> full reveal frames every 100 ms", HOST_CROP,
   [EXPAND, {"frames": {"name": "expand", "at": list(range(0, 800, 100))}}], spec=BARSPEC)
fx("laptop-bar", "lap-collapse", "full -> compact frames every 100 ms", HOST_CROP,
   [EXPAND, {"advance": 1200}, EXPAND, {"frames": {"name": "collapse", "at": list(range(0, 800, 100))}}], spec=BARSPEC)
fx("laptop-bar", "lap-toggle-debounce", "two toggles 300 ms apart: the second is dropped (stays full)", HOST_CROP,
   [EXPAND, {"advance": 300}, EXPAND, {"advance": 1200}, {"capture": ""}])
fx("laptop-bar", "lap-full-battery", "full bar, battery 57 % discharging, detailed stats", HOST_CROP,
   [EXPAND, {"advance": 2500}, {"capture": ""}], spec=BARSPEC)
fx("laptop-bar", "lap-full-nobattery", "full bar, no battery (CPU W / -- W)", HOST_CROP,
   [EXPAND, {"advance": 2500}, {"capture": ""}],
   svc=services(battery=upower(0, UNKNOWN, present=False, on_battery=False)), spec=BARSPEC)
for watts in (12.3, 64.0, 104):
    fx("laptop-bar", f"lap-full-nobattery-cpuw-{int(watts)}", "no battery, CPU W severity colours", HOST_CROP,
       [EXPAND, {"advance": 2500}, {"capture": ""}],
       svc=services(battery=upower(0, UNKNOWN, present=False, on_battery=False)),
       procs=processes(system={"power": watts}), spec=BARSPEC)
# The temperature tooltip hangs off the CPU-power module (laptop shell.qml:4196-4206), which is
# only visible with a power sample, so these fixtures carry 12.3 W.
for temp in (45, 82, 99):
    fx("laptop-bar", f"lap-tooltip-temp-{temp}", "CPU 温度 tooltip: not shown at 349 ms, shown at 350 ms",
       HOST_CROP, [EXPAND, {"advance": 2500}, {"hover": {"contains": "12.3 W"}},
                   {"frames": {"name": "tooltip", "at": [349, 350, 700]}}],
       procs=processes(system={"temp": temp, "power": 12.3}), spec=BARSPEC)
fx("laptop-bar", "lap-fullscreen-compact", "fullscreen client while compact", BAR_CROP, CAP,
   svc=services(fullscreen=True), spec=BARSPEC)
fx("laptop-bar", "lap-fullscreen-full", "fullscreen client while full", HOST_CROP,
   [EXPAND, {"advance": 2500}, {"capture": ""}], svc=services(fullscreen=True))

# island inside the bar (gated: laptop-island.md §4 / §9 item 9)
RUN = [isl.RUNNING, dict(isl.agent(1, "running", source="claude", progress=0.3), id="claude-session-1")]
fx("laptop-bar", "lap-island-compact-hidden", "agent + music active, bar compact: island not rendered",
   HOST_CROP, CAP, svc=services(playing=True), procs=processes(sessions=RUN, playing=True),
   spec="docs/spec/laptop-island.md §4")
fx("laptop-bar", "lap-island-reveal", "agent + music: island slides in with the bar", HOST_CROP,
   [EXPAND, {"frames": {"name": "reveal", "at": [0, 100, 200, 300, 500, 800]}}],
   svc=services(playing=True), procs=processes(sessions=RUN, playing=True), spec="docs/spec/laptop-island.md §4")
fx("laptop-bar", "lap-island-full-music", "full bar, music playing, no agent (island compact music)", HOST_CROP,
   [EXPAND, {"advance": 2500}, {"capture": ""}], svc=services(playing=True), procs=processes(playing=True))
fx("laptop-bar", "lap-island-full-agent-expanded", "full bar, 2 agents, island expanded by hover", HOST_CROP,
   [EXPAND, {"advance": 2500}, {"hover": {"text": "09:41", "x": -40}}, {"advance": 800}, {"capture": ""}],
   procs=processes(sessions=RUN))

# ------------------------------------------------------------ laptop-panels
PANELS = "docs/spec/laptop-panels.md §7"
CP_TOGGLE = {"set": {"ipc": ["controlPanel", "toggle"]}}
for profile in ("balanced", "power-saver", "performance"):
    fx("laptop-panels", f"lap-cp-main-{profile}", "control panel via IPC, full bar, no fan section", FULL_CROP,
       [EXPAND, {"advance": 1500}, CP_TOGGLE, {"advance": 900}, {"capture": ""}],
       svc=services(playing=True, profile=profile), procs=processes(profile=profile, playing=True), spec=PANELS)
fx("laptop-panels", "lap-cp-compact", "control panel via IPC while the bar is compact", FULL_CROP,
   [CP_TOGGLE, {"advance": 900}, {"capture": ""}], spec=PANELS)
fx("laptop-panels", "lap-cp-open-close", "control panel open/close frames (compact bar)", FULL_CROP,
   [CP_TOGGLE, {"frames": {"name": "open", "at": [0, 60, 120, 220, 400]}},
    CP_TOGGLE, {"frames": {"name": "close", "at": [0, 60, 150, 210, 400]}}])
for level in (0, 1, 10, 11, 37, 69, 70, 71, 100):
    fx("laptop-panels", f"lap-cp-brightness-{level:03d}", "CP brightness slider level (1 % UI step)", FULL_CROP,
       [EXPAND, {"advance": 1500}, CP_TOGGLE, {"advance": 900}, {"capture": ""}],
       procs=processes(brightness=level))
QA = "docs/spec/laptop-quickadjust.md §7"
fx("laptop-panels", "lap-qa-brightness-37", "QuickAdjust brightness 37, bar full (card at y 58)", FULL_CROP,
   [EXPAND, {"advance": 1500}, {"set": {"ipc": ["quickAdjust", "showBrightnessLevel", 37]}}, {"advance": 600},
    {"capture": ""}], procs=processes(brightness=37), spec=QA)
fx("laptop-panels", "lap-qa-brightness-compact", "QuickAdjust brightness 37, bar compact (card at y 25)",
   FULL_CROP, [{"set": {"ipc": ["quickAdjust", "showBrightnessLevel", 37]}}, {"advance": 600}, {"capture": ""}],
   procs=processes(brightness=37), spec=QA)
fx("laptop-panels", "lap-qa-step", "showBrightnessIncrease x3 from 50 -> 53", FULL_CROP,
   [EXPAND, {"advance": 1500}] + [{"set": {"ipc": ["quickAdjust", "showBrightnessIncrease"]}}, {"advance": 120}] * 3
   + [{"advance": 400}, {"capture": ""}], spec=QA)
fx("laptop-panels", "lap-qa-volume-compact", "QuickAdjust volume while compact (y 25)", FULL_CROP,
   [{"set": {"ipc": ["quickAdjust", "showVolume"]}}, {"advance": 600}, {"capture": ""}], spec=QA)
fx("laptop-panels", "lap-qa-toggle-while-shown", "OSD shown while compact, then a bar toggle", FULL_CROP,
   [{"set": {"ipc": ["quickAdjust", "showVolume"]}}, {"advance": 300}, EXPAND,
    {"frames": {"name": "toggle", "at": [0, 200, 400, 700]}}], spec=QA)
for mode, pct, state, kw in (
        ("discharging", 57, DISCHARGING, {}),
        ("discharging-critical", 18, DISCHARGING, {"capacity": 18, "estimate": 2400, "basis": "current"}),
        ("charging", 57, CHARGING, {"mode": "charging", "power": 31.5, "average": 29.0, "estimate": 3600}),
        ("full", 100, FULL, {"mode": "full", "capacity": 100, "power": 0, "average": 0, "estimate": None}),
        ("plugged", 98, UNKNOWN, {"mode": "plugged", "capacity": 98, "power": 0, "average": 0, "estimate": None}),
        ("estimating", 57, DISCHARGING, {"estimate": None, "samples": 1, "complete": False})):
    kw.setdefault("mode", "discharging")
    kw.setdefault("capacity", pct)
    fx("laptop-panels", f"lap-battery-popup-{mode}", "battery popup from the full bar module", FULL_CROP,
       [EXPAND, {"advance": 1500}, {"click": {"contains": f"{pct}%"}}, {"advance": 1500}, {"capture": ""}],
       svc=services(battery=upower(pct, state, on_battery=state == DISCHARGING)),
       procs=processes(info=battery_info(**kw)), spec="docs/spec/laptop-panels.md §5")
fx("laptop-panels", "lap-battery-popup-compact", "battery popup from the compact strip", FULL_CROP,
   [{"click": {"contains": "57%"}}, {"advance": 1500}, {"capture": ""}], spec="docs/spec/laptop-panels.md §5")

# ------------------------------------------------------------ laptop-island-legacy (record only)
LEGACY = "docs/spec/laptop-island.md §9 (expected divergence, D1 = read-only)"
APPROVAL = {"id": "codex-codex-session-0-call_1", "kind": "approval", "session_id": "codex-session-0",
            "source": "codex", "tool": "Bash", "detail": "rm -rf target", "created_at": 1}


def question(options, kind="question_from_permission", text="Which crate should own the battery history?"):
    return {"id": "claude-claude-session-0-call_2", "kind": kind, "session_id": "claude-session-0",
            "source": "claude", "question": text, "options": options, "answer_key": "Crate", "created_at": 2}


def legacy(name, sessions, pending, note, steps=None):
    n = len(sessions)
    content = 42 * n + 8 * max(0, n - 1)
    reserve = 26 + 58 + (11 + 74 + 13 if pending else 20)
    height = max(216 if pending else 184, reserve + max(42, min(content, 490 - reserve)))
    props = dict(isl.NO_MUSIC, mediaManuallyHidden=False, now="09:41", agentSessions=sessions,
                 agentPending=pending, agentPendingCount=1 if pending else 0)
    fixtures.setdefault("laptop-island-legacy", {})[name] = {
        "component": "laptop-island-legacy", "scene": "island.qml", "source_ref": SOURCE_REF, "spec": LEGACY,
        "note": note, "canvas": {"width": 640, "height": height + 24}, "fonts": ["JetBrainsMono Nerd Font"],
        "props": props,
        "steps": [{"hover": {"item": "island", "x": 10, "y": 19}}, {"advance": 600}] + (steps or [{"capture": ""}])}


Q = dict(isl.QUESTION, id="claude-session-0")
legacy("pending-approval-n1", [isl.APPROVAL], APPROVAL, "approval, n = 1 (224 tall)")
legacy("pending-approval-long", [isl.APPROVAL],
       dict(APPROVAL, detail="cargo test -p hyprv-svc-power --release -- --nocapture battery_history_window"),
       "approval with a long detail (elide)")
for count in (0, 1, 3, 4):
    legacy(f"pending-question-{count}opt", [Q], question(["Option %d" % i for i in range(1, count + 1)]),
           f"question with {count} options (max 3 shown)")
legacy("pending-question-cjk", [Q], question(["电池历史服务", "电源配置守护进程", "特权辅助进程"], text="哪个组件应负责电池历史？"),
       "3 long CJK options (row overflow quirk)")
legacy("pending-n3", [Q] + [isl.agent(i, "running") for i in range(1, 3)], question(["A", "B"]), "pending with n = 3")
legacy("pending-n9", [Q] + [isl.agent(i, "running") for i in range(1, 9)], question(["A", "B"]),
       "pending with n = 9 (budget cap 490)")
legacy("pending-accent-nonprimary", [isl.RUNNING, dict(isl.PROCESSING, id="claude-session-0")],
       question(["Yes", "No"]), "Codex primary, Claude-owned pending (accent quirk)")


def main(argv):
    root = Path(argv[1]) if len(argv) > 1 else Path(__file__).resolve().parent / "fixtures"
    total = 0
    for component, items in fixtures.items():
        out = root / component
        out.mkdir(parents=True, exist_ok=True)
        for stale in out.glob("*.json"):
            if stale.stem not in items:
                stale.unlink()
        for name, data in items.items():
            (out / f"{name}.json").write_text(json.dumps(data, ensure_ascii=False, indent=1) + "\n")
        total += len(items)
        print(f"{component}: {len(items)} fixtures")
    print(f"total {total}")


if __name__ == "__main__":
    main(sys.argv)
