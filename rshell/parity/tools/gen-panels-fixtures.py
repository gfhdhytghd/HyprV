#!/usr/bin/env python3
"""Regenerate parity/fixtures/panels/*.json (docs/spec/ui-panels.md §5).

Panels are rendered from the ORIGINAL shell.qml (scene desktop.qml) and opened
the way a user or Hyprland bind opens them: clicks on bar items or IPC calls.
Builds on the bar generator's synthetic services/process outputs.
"""
import importlib.util
import json
import sys
from pathlib import Path

_spec = importlib.util.spec_from_file_location("genbar", Path(__file__).with_name("gen-bar-fixtures.py"))
bar = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(bar)

CP_CROP = [2590, 50, 482, 760]
WIFI_CROP = [2380, 50, 692, 720]
QA_CROP = [2380, 50, 692, 260]
STATS_CROP = [0, 50, 1000, 760]
CAL_CROP = [1000, 0, 1072, 540]


def cp_status(brightness=50, dnd=False, recording=False, profile="balanced", prevent_sleep=False,
              fans=True, chassis=40, chassis_rpm=1200, pump=60, pump_rpm=2400):
    b = lambda v: "true" if v else "false"
    return "\n".join([
        f"brightness={brightness}", f"dnd={b(dnd)}", f"recording={b(recording)}",
        f"power_profile={profile}", f"prevent_sleep={b(prevent_sleep)}", "demo_mode=false",
        f"fan_available={b(fans)}", f"fan_chassis_percent={chassis}", f"fan_chassis_rpm={chassis_rpm}",
        f"fan_pump_percent={pump}", f"fan_pump_rpm={pump_rpm}"]) + "\n"


def mihomo(online=True, mode="rule", upload=2048, download=48_000):
    return json.dumps({"online": online, "service": "user" if online else "none", "version": "v1.19.2",
                       "mode": mode, "connections": 37 if online else 0, "upload": upload if online else 0,
                       "download": download if online else 0, "dashboard": "http://127.0.0.1:9090/ui"})


def panel_processes(status=None, vm="stopped", viewflow="windows", mihomo_status=None, brightness=50,
                    vm_busy=False, **bar_kwargs):
    procs = bar.processes(**bar_kwargs)
    procs += [
        {"match": "control-panel-status.sh", "stdout": status or cp_status(brightness=brightness)},
        {"match": "brightness_get", "stdout": f"{brightness}\n"},
        {"match": "brightness_set", "stdout": ""},
        {"match": "brightness_finish", "stdout": ""},
        {"match": "windows-vm-control.sh status", "stdout": vm + "\n"},
        {"match": "windows-vm-control.sh start", "delay_ms": 600000},
        {"match": "viewflow-target.sh status", "stdout": viewflow + "\n"},
        {"match": "deskflow-target.sh status", "stdout": viewflow + "\n"},
        {"match": "mihomo-control.sh status", "stdout": mihomo_status or mihomo()},
        {"match": "dcal --json socket", "stdout": json.dumps({"path": "/run/user/1000/dcal/parity.sock"})},
    ]
    return procs


WIFI_ON = bar.wifi_payload(present=True, connected=True, signal=75, ssid="Home-5G", security="WPA2")
BT_ONE = {"enabled": True, "devices": [
    {"name": "WH-1000XM5", "address": "AC:80:0A:11:22:33", "connected": True, "paired": True, "icon": "audio-headset", "battery": 80}]}
BT_THREE = {"enabled": True, "devices": BT_ONE["devices"] + [
    {"name": "MX Master 3S", "address": "D4:11:22:33:44:55", "connected": False, "paired": True, "icon": "input-mouse"},
    {"name": "", "address": "5C:AA:BB:CC:DD:EE", "connected": False, "paired": False, "icon": ""}]}
BT_MANY = {"enabled": True, "devices": [
    {"name": f"Device {i:02d}", "address": f"00:11:22:33:44:{i:02X}", "connected": i == 0, "paired": i < 4, "icon": "bluetooth"}
    for i in range(14)]}

PLAYING_SVC = dict(mpris={"players": [{"identity": "Spotify"}]})

fixtures = {}


def fx(name, note, crop, steps, svc=None, procs=None, settle=2500, tick_ms=None):
    data = {
        "component": "panels", "scene": "desktop.qml", "spec": "docs/spec/ui-panels.md §5", "note": note,
        "now": "2026-10-23T09:41:00", "crop": crop, "settle_ms": settle,
        "services": svc or bar.services(), "processes": procs or panel_processes(), "props": {}, "steps": steps,
    }
    if tick_ms:
        data["tick_ms"] = tick_ms
    fixtures[name] = data


CP_ICON = {"image": "control-panel-"}
open_cp = [{"click": CP_ICON}, {"advance": 800}]

# 1-2 control panel main page
fx("cp-main-idle", "§5.1 Wi-Fi connected, BT 1 device, media playing with art, brightness 50, volume 35, fans 40/60 %",
   CP_CROP, open_cp + [{"capture": ""}],
   svc=bar.services(bluetooth=BT_ONE, **PLAYING_SVC),
   procs=panel_processes(system_args={"iface": "wlan0"}, wifi=WIFI_ON, playing=True))
fx("cp-main-off", "§5.2 Wi-Fi off, BT off, media idle, fans unavailable, muted, recording + prevent-sleep on",
   CP_CROP, open_cp + [{"capture": ""}],
   svc=bar.services(bluetooth={"enabled": False, "devices": []}, audio={"volume": 0.35, "muted": True}),
   procs=panel_processes(status=cp_status(fans=False, recording=True, prevent_sleep=True),
                         system_args={"iface": ""}, wifi=bar.wifi_payload(present=True, enabled=False)))
# 3 hover / press
fx("cp-hover-toggle", "§5.3 pointer over the DND toggle (inactive), then over the dark-mode toggle",
   CP_CROP, open_cp + [{"hover": {"text": "󰂛"}}, {"advance": 300}, {"capture": "dnd"},
                       {"hover": {"image": "light-dark"}}, {"advance": 300}, {"capture": "theme"}])
fx("cp-press-toggle", "§5.3 pressed toggle (held)", CP_CROP,
   open_cp + [{"press": {"text": "󰂛"}}, {"frames": {"name": "press", "at": [0, 60, 200]}}, {"release": True}])
fx("cp-press-splittile", "§5.3 Wi-Fi split tile icon well pressed", CP_CROP,
   open_cp + [{"press": {"text": "WIFI", "x": -55}}, {"frames": {"name": "press", "at": [0, 60, 200]}}],
   procs=panel_processes(system_args={"iface": "wlan0"}, wifi=WIFI_ON))
# 4 sliders
for level in (0, 37, 100):
    fx(f"cp-brightness-{level}", f"§5.4 brightness {level} (icon two-tone split at the fill edge)", CP_CROP,
       open_cp + [{"capture": ""}], procs=panel_processes(status=cp_status(brightness=level), brightness=level))
fx("cp-brightness-drag", "§5.4 brightness slider mid-drag", CP_CROP,
   open_cp + [{"press": {"text": "󰃟", "x": 40}}, {"drag": {"text": "󰃟", "x": 75}}, {"capture": "held"},
              {"release": True}])
# 5 VM toggle
fx("cp-vm-running", "§5.5 Windows VM running (active)", CP_CROP, open_cp + [{"capture": ""}],
   procs=panel_processes(vm="running"))
# 6 open / close animation
fx("cp-open-close", "§5.6 open frames 16/60/120/220 ms, close 60/150/210 ms", CP_CROP,
   [{"click": CP_ICON}, {"frames": {"name": "open", "at": [0, 16, 60, 120, 220, 400]}},
    {"click": CP_ICON}, {"frames": {"name": "close", "at": [0, 60, 150, 210, 400]}}])
# 7-8 bluetooth page
# The split tile's right half (label) opens the details page.
BT_LABEL = {"text": "WIFI", "y": 72}  # Bluetooth tile label sits 72 px below the Wi-Fi label
open_bt = open_cp + [{"click": BT_LABEL}, {"advance": 800}]
fx("cp-bt-off", "§5.7 BT page, adapter off", CP_CROP, open_bt + [{"capture": ""}],
   svc=bar.services(bluetooth={"enabled": False, "devices": []}))
fx("cp-bt-scanning-empty", "§5.7 BT on + scanning + empty", CP_CROP, open_bt + [{"capture": ""}],
   svc=bar.services(bluetooth={"enabled": True, "discovering": True, "devices": []}))
fx("cp-bt-three", "§5.7 BT with 3 devices (connected, paired, unnamed)", CP_CROP, open_bt + [{"capture": ""}],
   svc=bar.services(bluetooth=BT_THREE))
fx("cp-bt-many", "§5.7 BT list > 420 px (clipped/scroll)", CP_CROP, open_bt + [{"capture": ""}],
   svc=bar.services(bluetooth=BT_MANY))
fx("cp-page-switch", "§5.8 main → bluetooth mid-transition", CP_CROP,
   open_cp + [{"click": BT_LABEL}, {"frames": {"name": "switch", "at": [0, 60, 120, 200, 400, 800]}}],
   svc=bar.services(bluetooth=BT_THREE))
# 9 mihomo
# Unreachable from the UI today (ui-panels-control.md): opened by invoking the page switch.
OPEN_MIHOMO = {"invoke": {"type": "ControlPanelPopup", "method": "toggleMihomoExpanded"}}
fx("cp-mihomo-online", "§5.9 Mihomo page online (rule mode, rates > 1 KB/s); harness-only state", CP_CROP,
   open_cp + [OPEN_MIHOMO, {"advance": 800}, {"capture": ""}])
fx("cp-mihomo-offline", "§5.9 Mihomo page offline", CP_CROP,
   open_cp + [OPEN_MIHOMO, {"advance": 800}, {"capture": ""}],
   procs=panel_processes(mihomo_status=mihomo(online=False)))

# 10-13 Wi-Fi popup
WIFI_NETS = [
    {"active": True, "ssid": "Home-5G", "signal": 82, "security": "WPA2", "bars": "", "secure": True, "known": True, "enterprise": False},
    {"active": False, "ssid": "Office", "signal": 64, "security": "WPA2", "bars": "", "secure": True, "known": True, "enterprise": False},
    {"active": False, "ssid": "Cafe Free", "signal": 41, "security": "", "bars": "", "secure": False, "known": False, "enterprise": False},
    {"active": False, "ssid": "Neighbour", "signal": 22, "security": "WPA3", "bars": "", "secure": True, "known": False, "enterprise": False},
]


def wifi_with(networks, **kw):
    payload = bar.wifi_payload(**kw)
    payload["networks"] = networks
    return payload


open_wifi = [{"click": {"image": "nm-signal-"}}, {"advance": 800}]
fx("wifi-connected-4", "§5.10 connected, 4 networks (active, saved, open, secure-unknown)", WIFI_CROP,
   open_wifi + [{"capture": ""}],
   procs=panel_processes(system_args={"iface": "wlan0"},
                         wifi=wifi_with(WIFI_NETS, present=True, connected=True, signal=82, ssid="Home-5G", security="WPA2")))
fx("wifi-expanded-secure", "§5.11 secure network expanded with password field (empty → Join disabled)", WIFI_CROP,
   open_wifi + [{"click": {"text": "Neighbour", "x": 247, "y": 10}}, {"advance": 500}, {"capture": ""}],  # row key button
   procs=panel_processes(system_args={"iface": "wlan0"},
                         wifi=wifi_with(WIFI_NETS, present=True, connected=True, signal=82, ssid="Home-5G", security="WPA2")))
fx("wifi-many", "§5.13 > 5 networks (list capped to 5 cards)", WIFI_CROP, open_wifi + [{"capture": ""}],
   procs=panel_processes(system_args={"iface": "wlan0"},
                         wifi=wifi_with(WIFI_NETS + [dict(WIFI_NETS[2], ssid=f"Extra {i}", signal=30 - i) for i in range(5)],
                                        present=True, connected=True, signal=82, ssid="Home-5G", security="WPA2")))
fx("wifi-off", "§5.12 Wi-Fi radio off (popup opened from the control panel tile)", WIFI_CROP,
   open_cp + [{"click": {"text": "WIFI"}}, {"advance": 800}, {"capture": ""}],
   procs=panel_processes(system_args={"iface": "enp5s0"}, wifi=bar.wifi_payload(present=True, enabled=False)))

# 14 quick adjust
for level in (0, 50, 100):
    fx(f"qa-brightness-{level}", f"§5.14 QuickAdjust brightness {level}", QA_CROP,
       [{"set": {"ipc": ["quickAdjust", "showBrightnessLevel", level]}}, {"advance": 400}, {"capture": ""}],
       procs=panel_processes(status=cp_status(brightness=level), brightness=level))
for label, vol, muted in (("0-muted", 0.0, True), ("30", 0.3, False), ("100", 1.0, False)):
    fx(f"qa-volume-{label}", f"§5.14 QuickAdjust volume {label}", QA_CROP,
       [{"set": {"ipc": ["quickAdjust", "showVolume"]}}, {"advance": 400}, {"capture": ""}],
       svc=bar.services(audio={"volume": vol, "muted": muted}))
fx("qa-show-hide", "§5.14 mid-show (60 ms) and hide frames", QA_CROP,
   [{"set": {"ipc": ["quickAdjust", "showVolume"]}}, {"frames": {"name": "show", "at": [0, 60, 120, 400]}},
    {"advance": 3000}, {"frames": {"name": "later", "at": [0, 200, 400]}}])


# 15-16 system stats (history needs 120 one-second samples: coarser 50 ms ticks)
def history_lines(samples, iface="enp5s0"):
    """`samples` system snapshots 1 s apart with cumulative /proc counters, so
    every consecutive pair yields the intended CPU % / rate (shell.qml
    appends one history point per snapshot)."""
    lines, user, idle, rx, tx = [], 100_000, 900_000, 5_000_000, 1_000_000
    cores = [[12_500, 112_500] for _ in range(8)]
    for i in range(samples):
        cpu = 45 + 15 * ((i * 37) % 11 - 5) / 5
        mem = 70 + 2 * ((i * 13) % 5 - 2) / 2
        rate = [0, 2048, 512000, 64000, 3_000_000, 1024][i % 6]
        if i > 0:
            user += round(cpu * 10)
            idle += 1000 - round(cpu * 10)
            for c, core in enumerate(cores):
                busy = max(0, min(100, cpu + (c - 3.5) * 6))
                core[0] += round(busy * 10)
                core[1] += 1000 - round(busy * 10)
            rx += round(rate * 0.75)
            tx += round(rate * 0.25)
        stat = "\n".join([f"cpu  {user} 0 0 {idle} 0 0 0 0 0 0"] +
                         [f"cpu{c} {u} 0 0 {d} 0 0 0 0 0 0" for c, (u, d) in enumerate(cores)])
        total = 65_536_000
        meminfo = f"MemTotal:       {total} kB\nMemAvailable:   {round(total * (1 - mem / 100))} kB"
        route = "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT"
        netdev = "Inter-|\n face |\n    lo:  100 1 0 0 0 0 0 0  100 1 0 0 0 0 0 0"
        if iface:
            route += f"\n{iface}\t00000000\t0101A8C0\t0003\t0\t0\t100\t00000000\t0\t0\t0"
            netdev += f"\n{iface}: {rx} 100 0 0 0 0 0 0 {tx} 100 0 0 0 0 0 0"
        text = "\n".join(name + "\n" + value for name, value in (
            ("__STAT__", stat), ("__MEM__", meminfo), ("__TEMP__", "52000"), ("__CPU_POWER__", "64.2"),
            ("__ROUTE__", route), ("__NET__", netdev)))
        lines.append({"at": 1000 * i, "data": json.dumps({"type": "system", "data": text})})
    return lines


def stats_procs(samples, iface="enp5s0"):
    procs = panel_processes(system_args={"iface": iface})
    stream = procs[0]
    stream["lines"] = [l for l in stream["lines"] if '"type": "system"' not in l["data"]] + history_lines(samples, iface)
    return procs


STATS_OPEN = [{"click": {"contains": "%", "index": 0}}, {"advance": 800}, {"capture": ""}]
fx("stats-120", "§5.15 SystemStats with 120 samples (CPU ~45 %, RAM ~70 %, bursty network)", STATS_CROP,
   STATS_OPEN, procs=stats_procs(120), settle=121000, tick_ms=50)
fx("stats-1", "§5.15 SystemStats with 1 sample", STATS_CROP, STATS_OPEN, procs=stats_procs(1), settle=1500)
fx("stats-0", "§5.15 SystemStats with 0 samples (grid only)", STATS_CROP,
   [{"click": {"contains": "0%", "index": 0}}, {"advance": 800}, {"capture": ""}], procs=stats_procs(0), settle=1500)
fx("stats-offline", "§5.15 network disconnected (未连接)", STATS_CROP,
   [{"click": {"contains": "nocon"}}, {"advance": 800}, {"capture": ""}], procs=stats_procs(6, iface=""), settle=6500)

# 17-20 calendar (daemon mocked through the socket JSON-RPC)
CALENDARS = [{"id": "work", "summary": "Work", "color": "#3f8cff"}, {"id": "home", "summary": "家庭", "color": "#2fb36a"}]
EVENTS = [
    {"id": "e1", "calendarId": "home", "summary": "国庆调休", "allDay": True, "start": "2026-10-23T00:00:00Z", "end": "2026-10-24T00:00:00Z"},
    {"id": "e2", "calendarId": "work", "summary": "Conference (3 days)", "allDay": True, "start": "2026-10-24T00:00:00Z", "end": "2026-10-27T00:00:00Z"},
    {"id": "e3", "calendarId": "work", "summary": "Standup", "allDay": False, "start": "2026-10-23T09:30:00", "end": "2026-10-23T10:00:00"},
    {"id": "e4", "calendarId": "home", "summary": "Dentist", "allDay": False, "start": "2026-10-23T09:45:00", "end": "2026-10-23T11:00:00"},
    {"id": "e5", "calendarId": "work", "summary": "Design review — rshell parity", "allDay": False, "start": "2026-10-23T13:00:00", "end": "2026-10-23T14:30:00"},
    {"id": "e6", "calendarId": "home", "summary": "跑步", "allDay": False, "start": "2026-10-25T07:00:00", "end": "2026-10-25T08:00:00"},
]


SOCKETS = {"parity.sock": {"methods": {"calendars.list": CALENDARS, "events.list": {"events": EVENTS},
                                       "subscribe": True, "ui.show": True}}}


def cal_fixture(name, note, steps, sockets=SOCKETS):
    fx(name, note, CAL_CROP, [{"set": {"ipc": ["calendar", "toggle"]}}, {"advance": 1500}] + steps)
    if sockets is not None:
        fixtures[name]["sockets"] = sockets


cal_fixture("calendar-week", "§5.17 current week, 2 all-day (one spans 3 days), overlapping timed events, now-line",
            [{"capture": ""}])
cal_fixture("calendar-tooltip", "§5.18 hover tooltip on an event", [{"hover": {"text": "Standup"}}, {"advance": 600}, {"capture": ""}])
cal_fixture("calendar-error", "§5.19 daemon unreachable (status pill)", [{"capture": ""}], sockets=None)
fx("calendar-morph", "§5.20 island → calendar morph mid-animation and close", CAL_CROP,
   [{"set": {"ipc": ["calendar", "toggle"]}}, {"frames": {"name": "open", "at": [0, 16, 100, 210, 500]}},
    {"set": {"ipc": ["calendar", "toggle"]}}, {"frames": {"name": "close", "at": [0, 100, 209, 211, 500]}}])
fixtures["calendar-morph"]["sockets"] = SOCKETS


# 21 StartupConfirm (separate entry point: `qs --path StartupConfirm.qml`); dark/light
# comes from the qt6ct palette, which capture.sh switches per theme.
START_CROP = [1290, 680, 492, 370]
for name, note, steps in (
        ("startup-confirm", "§5.21 initial (keyboard focus on 本次跳过 after 150 ms)", [{"advance": 400}, {"capture": ""}]),
        ("startup-confirm-hover", "§5.21 hovered 启动", [{"advance": 400}, {"hover": {"text": "启动"}}, {"advance": 300}, {"capture": ""}]),
        ("startup-confirm-press", "§5.21 pressed 启动 (held)", [{"advance": 400}, {"press": {"text": "启动"}}, {"advance": 200}, {"capture": ""}])):
    fx(name, note, START_CROP, steps, settle=0)
    fixtures[name]["entry"] = "StartupConfirm.qml"
    fixtures[name]["processes"] = []


def main() -> None:
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    for old in out.glob("*.json"):
        old.unlink()
    for name, data in fixtures.items():
        (out / f"{name}.json").write_text(json.dumps(data, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print(f"{len(fixtures)} fixtures → {out}")


if __name__ == "__main__":
    main()
