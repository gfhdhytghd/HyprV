#!/usr/bin/env python3
"""Regenerate parity/fixtures/bar/*.json (docs/spec/ui-bar.md §14).

The bar is rendered from the ORIGINAL shell.qml (scene desktop.qml) with mock
Quickshell services. Script-backed data is fed through canned process outputs
(see qml-mocks/Quickshell/Io/Process.qml); this script synthesises them so
shell.qml's own parsers compute the intended values (CPU %, MEM %, rates, ...).
"""
import copy
import json
import sys
from pathlib import Path

SCREEN = {"name": "DP-4", "width": 3072, "height": 1728, "x": 0, "y": 0, "scale": 2}
BAR_CROP = [0, 0, 3072, 70]

# ---------------------------------------------------------------- system data


def system_snapshot(tick, cpu=7.0, mem=42.0, temp=45.0, power=None, iface="enp5s0", rate=0.0):
    """One desktop-state.py "system" payload; tick 0 and 1 are 1000 ms apart.

    shell.qml derives CPU% from the delta of two /proc/stat samples and the
    network rate from two /proc/net/dev samples over Date.now() (virtual).
    """
    total0, idle0 = 1_000_000, 900_000
    busy = cpu / 100.0
    total = total0 + 1000 * tick
    idle = idle0 + round((1 - busy) * 1000) * tick
    user = total - idle
    cpu_lines = [f"cpu  {user} 0 0 {idle} 0 0 0 0 0 0"]
    for core in range(4):
        cu = user // 4 + core * 10 * tick
        ci = idle // 4 - core * 10 * tick
        cpu_lines.append(f"cpu{core} {cu} 0 0 {ci} 0 0 0 0 0 0")
    mem_total = 65_536_000
    mem_avail = round(mem_total * (1 - mem / 100.0))
    meminfo = f"MemTotal:       {mem_total} kB\nMemFree:        {mem_avail // 2} kB\nMemAvailable:   {mem_avail} kB"
    route = "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT"
    if iface:
        route += f"\n{iface}\t00000000\t0101A8C0\t0003\t0\t0\t100\t00000000\t0\t0\t0"
    rx = 5_000_000 + round(rate * 0.75) * tick
    tx = 1_000_000 + round(rate * 0.25) * tick
    netdev = ("Inter-|   Receive                                                |  Transmit\n"
              " face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n"
              "    lo:  100 1 0 0 0 0 0 0  100 1 0 0 0 0 0 0")
    if iface:
        netdev += f"\n{iface}: {rx} 100 0 0 0 0 0 0 {tx} 100 0 0 0 0 0 0"
    sections = [("__STAT__", "\n".join(cpu_lines)), ("__MEM__", meminfo),
                ("__TEMP__", str(round(temp * 1000))), ("__CPU_POWER__", "" if power is None else f"{power}"),
                ("__ROUTE__", route), ("__NET__", netdev)]
    return "\n".join(name + "\n" + value for name, value in sections)


def wifi_payload(present=False, enabled=True, connected=False, signal=0, ssid="", security="", hardware=True,
                 iface="wlan0"):
    networks = []
    if present and enabled:
        networks = [{"active": connected, "ssid": ssid or "Home-5G", "signal": signal, "security": security,
                     "bars": "", "secure": bool(security), "known": True, "enterprise": False},
                    {"active": False, "ssid": "Cafe", "signal": 40, "security": "", "bars": "",
                     "secure": False, "known": False, "enterprise": False}]
    return {"present": present, "hardwareEnabled": hardware, "enabled": enabled, "connected": connected,
            "iface": iface if present else "", "ssid": ssid if connected else "", "security": security,
            "signal": signal, "networks": networks}


def desktop_state(system_args=None, wifi=None, agents=None):
    system_args = system_args or {}
    lines = [
        {"at": 0, "data": json.dumps({"type": "system", "data": system_snapshot(0, **system_args)})},
        {"at": 1000, "data": json.dumps({"type": "system", "data": system_snapshot(1, **system_args)})},
        {"at": 0, "data": json.dumps({"type": "wifi", "data": wifi or wifi_payload()})},
        {"at": 0, "data": json.dumps({"type": "agent", "data": {"active": bool(agents), "count": len(agents or []),
                                                               "sessions": agents or []}})},
    ]
    return {"match": "desktop-state.py", "stream": True, "lines": lines}


def media(playing=None, player="spotify"):
    if playing is None:
        return {"match": "media-status.sh", "stdout": "available=false\nplaying=false\n", "exitCode": 1}
    return {"match": "media-status.sh", "exitCode": 0, "stdout": (
        "cider=false\navailable=true\nplaying=" + ("true" if playing else "false") +
        "\ntitle=Midnight City\nartist=M83\nplayer=" + player +
        "\nart_url=file://@ASSETS@/cover-warm.png\nposition=83\nlength=225\n")}


ANIMATIONS = json.dumps([
    [{"name": "global", "overridden": True, "bezier": "default", "enabled": True, "speed": 8.0, "style": ""},
     {"name": "windows", "overridden": True, "bezier": "myBezier", "enabled": True, "speed": 7.0, "style": "slide"},
     {"name": "windowsMove", "overridden": False, "bezier": "", "enabled": True, "speed": 0.0, "style": ""}],
    [{"name": "myBezier", "X0": 0.1, "Y0": 0.9, "X1": 0.1, "Y1": 1.05},
     {"name": "default", "X0": 0.0, "Y0": 0.75, "X1": 0.15, "Y1": 1.0}]])

FLUENT = ("light=@HOME@/.local/share/icons/Fluent-light\ndark=@HOME@/.local/share/icons/Fluent-dark\n"
          "base=@HOME@/.local/share/icons/Fluent\n")


def processes(system_args=None, wifi=None, notification="none", cava=True, playing=None, agents=None):
    return [
        desktop_state(system_args, wifi, agents),
        {"match": "swaync-client", "stream": True,
         "lines": [{"at": 0, "data": json.dumps({"text": "", "alt": notification, "tooltip": "", "class": notification})}]},
        {"match": "command -v cava", "exitCode": 0 if cava else 1},
        {"match": "hyprctl -j animations", "stdout": ANIMATIONS},
        {"match": "Fluent-light", "stdout": FLUENT},
        media(playing),
        {"match": "audio-spectrum.sh", "stream": True,
         "lines": [{"at": 0, "data": "12;38;74;92;66;41;58;80;95;61;33;47;70;52;28;15;"}]},
    ]


TRAY = [
    {"id": "chrome_status_icon_1", "title": "Google Chrome", "icon": "google-chrome"},
    {"id": "discord", "title": "Discord", "icon": "discord"},
    {"id": "org.keepassxc.KeePassXC", "title": "KeePassXC", "icon": "keepassxc"},
    {"id": "Fcitx", "title": "输入法", "icon": "fcitx"},
    {"id": "wechat", "title": "微信", "icon": "wechat"},
    {"id": "telegram", "title": "Telegram", "icon": "telegram"},
]

WORKSPACES = [{"id": i, "name": str(i)} for i in range(1, 6)]


def services(**over):
    base = {
        "screens": [SCREEN],
        "hyprland": {"workspaces": WORKSPACES, "focusedWorkspace": 1,
                     "activeToplevel": {"title": "Firefox — Quickshell spec"}, "fullscreen": False},
        "audio": {"available": True, "volume": 0.35, "muted": False},
        "mpris": {"players": []},
        "tray": TRAY,
        "bluetooth": {"enabled": True, "devices": []},
        "powerProfile": "balanced",
    }
    for key, value in over.items():
        if isinstance(value, dict) and isinstance(base.get(key), dict):
            base[key] = {**base[key], **value}
        else:
            base[key] = value
    return base


fixtures = {}


def fx(name, note, svc=None, procs=None, steps=None, crop=None, canvas=None, settle=2500):
    data = {
        "component": "bar",
        "scene": "desktop.qml",
        "spec": "docs/spec/ui-bar.md §14",
        "note": note,
        "now": "2026-10-23T09:41:00",
        "crop": crop or BAR_CROP,
        "settle_ms": settle,
        "services": svc or services(),
        "processes": procs or processes(),
        "props": {},
        "steps": steps or [{"capture": ""}],
    }
    if canvas:
        data["canvas"] = canvas
    fixtures[name] = data


hover = lambda target: {"hover": target}
AWAY = {"hover": [1500, 600]}

fx("baseline", "§14.1 idle, 5 workspaces, ws 1 focused, title, tray 6 items, ethernet, no notifications")
fx("metrics", "§14.2 CPU 7 %, MEM 42 %, net 0 B/s, 45 °C, -- W (advance rounding)",
   procs=processes({"cpu": 7, "mem": 42, "temp": 45, "power": None, "rate": 0}))
fx("workspace-hover", "§14.3 pointer over inactive workspace 3",
   steps=[hover({"text": "3"}), {"advance": 300}, {"capture": ""}])
fx("workspace-urgent", "§14.4 urgent ws 4 while ws 1 focused",
   svc=services(hyprland={"workspaces": [dict(w, urgent=(w["id"] == 4)) for w in WORKSPACES]}))
fx("workspace-named", "§14.5 named workspace wider than 32 px",
   svc=services(hyprland={"workspaces": WORKSPACES[:3] + [{"id": 4, "name": "mail"}, {"id": 5, "name": "5"}]}))
fx("title-long", "§14.6 long window title elided",
   svc=services(hyprland={"activeToplevel": {"title": "A very long window title — " * 12}}))
fx("title-none", "§14.7 no active window", svc=services(hyprland={"activeToplevel": None}))
fx("temperature-critical", "§14.8 temperature ≥ 70 °C", procs=processes({"temp": 78}))
for label, watts in (("low", 42.5), ("medium", 75.3), ("critical", 96.4), ("integer", 123.7)):
    fx(f"cpu-power-{label}", f"§14.9 CPU package power {watts} W", procs=processes({"power": watts}))
fx("media-playing", "§14.10 media playing (pause glyph)",
   svc=services(mpris={"players": [{"identity": "Spotify"}]}), procs=processes(playing=True))
fx("media-paused", "§14.10 media paused (play glyph)",
   svc=services(mpris={"players": [{"identity": "Spotify"}]}), procs=processes(playing=False))
for label, vol, muted in (("muted", 0.35, True), ("20", 0.2, False), ("50", 0.5, False), ("80", 0.8, False)):
    fx(f"volume-{label}", f"§14.11 volume {label}", svc=services(audio={"volume": vol, "muted": muted}))
for label, alt in (("bell", "none"), ("dot", "notification"), ("dnd", "dnd-none")):
    fx(f"notifications-{label}", f"§14.12 notifications {alt}", procs=processes(notification=alt))
fx("network-ethernet", "§14.13 wired default route", procs=processes({"iface": "enp5s0", "rate": 1536}))
for sig in (0, 25, 50, 75, 100):
    for secure in (True, False):
        fx(f"network-wifi-{sig}-{'secure' if secure else 'open'}", f"§14.13 Wi-Fi signal {sig} {'secure' if secure else 'open'}",
           procs=processes({"iface": "wlan0", "rate": 0},
                           wifi_payload(present=True, connected=True, signal=sig, ssid="Home-5G",
                                        security="WPA2" if secure else "")))
fx("network-wifi-disconnected", "§14.13 Wi-Fi on, not connected",
   procs=processes({"iface": ""}, wifi_payload(present=True, connected=False)))
fx("network-wifi-off", "§14.13 Wi-Fi radio off",
   procs=processes({"iface": ""}, wifi_payload(present=True, enabled=False)))
fx("network-offline", "§14.13 offline (nocon)", procs=processes({"iface": ""}))
fx("network-wifi-hover", "§14.14 Wi-Fi indicator hover",
   procs=processes({"iface": "wlan0"}, wifi_payload(present=True, connected=True, signal=75, ssid="Home-5G", security="WPA2")),
   steps=[hover({"image": "nm-signal-"}), {"advance": 300}, {"capture": ""}])
fx("tray-fallback-letter", "§14.15 tray item without a loadable icon → letter",
   svc=services(tray=TRAY[:3] + [{"id": "zzz-unknown", "title": "Unknown", "icon": "parity-no-such-icon"}]))
NARROW = dict(SCREEN, width=1280, height=720)
fx("narrow-output", "§14.16 1280 px output: supplementary info hidden, tray squeezed/overflow",
   svc=services(screens=[NARROW], tray=TRAY + [{"id": f"extra{i}", "title": f"Extra {i}", "icon": "applications-system"} for i in range(6)]),
   crop=[0, 0, 1280, 70])
fx("viewflow-hover", "§14.17 viewflow button hover",
   steps=[hover({"text": "󰖳"}), {"advance": 300}, {"capture": ""}])
fx("fullscreen-slide", "§14.18 bar slides up when a window goes fullscreen, and back",
   steps=[{"set": {"services": {"hyprland": {"workspaces": WORKSPACES, "focusedWorkspace": 1,
                                             "activeToplevel": {"title": "Firefox — Quickshell spec"}, "fullscreen": True}}}},
          {"frames": {"name": "hide", "at": [0, 100, 200, 350, 700, 900]}},
          {"set": {"services": {"hyprland": {"workspaces": WORKSPACES, "focusedWorkspace": 1,
                                             "activeToplevel": {"title": "Firefox — Quickshell spec"}, "fullscreen": False}}}},
          {"frames": {"name": "show", "at": [0, 100, 200, 350, 700, 900]}}])
fx("calendar-host", "§14.19 calendar presented (host frame; panel content per ui-panels)",
   steps=[{"set": {"ipc": ["calendar", "toggle"]}}, {"advance": 1000}, {"capture": ""}],
   crop=[1000, 0, 1072, 540])
fx("island-expanded-host", "§14.20 island expanded (bar surface 820 px tall)",
   svc=services(mpris={"players": [{"identity": "Spotify"}]}), procs=processes(playing=True),
   steps=[hover({"text": "09:41", "x": -40}), {"advance": 600}, {"capture": ""}],
   crop=[1236, 0, 600, 220])
for corner, rect in (("top-left", [0, 0, 60, 60]), ("top-right", [3012, 0, 60, 60]),
                     ("bottom-left", [0, 1668, 60, 60]), ("bottom-right", [3012, 1668, 60, 60])):
    fx(f"corner-{corner}", f"§14.21 screen corner {corner}", crop=rect)


# ---- popups (§14 items 22-24)
MENU = {"entries": [
    {"text": "Open Discord", "icon": "@asset:cover-warm.png"},
    {"isSeparator": True},
    {"text": "Mute", "buttonType": 1, "checkState": 2},
    {"text": "Deafen", "buttonType": 1, "checkState": 0},
    {"text": "Status", "entries": [
        {"text": "Online", "buttonType": 2, "checkState": 2},
        {"text": "Idle", "buttonType": 2, "checkState": 0},
        {"text": "Do Not Disturb", "buttonType": 2, "checkState": 0}]},
    {"text": "Check for updates (disabled)", "enabled": False},
    {"isSeparator": True},
    {"text": "Quit Discord"},
]}
MENU_TRAY = [dict(t, menu=MENU) if t["id"] == "discord" else t for t in TRAY]
MENU_CROP = [2560, 40, 512, 520]
open_menu = [{"rightclick": {"image": "discord"}}]
fx("tray-menu-root", "§14.22 tray menu root level (icon row, checkboxes, separator, disabled row, submenu arrow)",
   svc=services(tray=MENU_TRAY), crop=MENU_CROP, steps=open_menu + [{"advance": 600}, {"capture": ""}])
fx("tray-menu-hover", "§14.22 tray menu with hovered row", svc=services(tray=MENU_TRAY), crop=MENU_CROP,
   steps=open_menu + [{"advance": 600}, {"hover": {"text": "Deafen"}}, {"advance": 300}, {"capture": ""}])
# After clicking "Status" the pointer stays where the row was, and the submenu rows that
# replace it under the pointer may or may not receive a hover depending on delivery order
# (one earlier capture had the "Do Not Disturb" hover, three did not: crates/hyprv-ui-bar/
# VALIDATION.md). An explicit hover makes the captured input state deterministic.
fx("tray-menu-submenu", "§14.22 submenu with radio indicators, the < Back row and an explicit hover on Do Not Disturb",
   svc=services(tray=MENU_TRAY), crop=MENU_CROP,
   steps=open_menu + [{"advance": 600}, {"click": {"text": "Status"}}, {"advance": 600},
                      {"hover": {"text": "Do Not Disturb"}}, {"advance": 300}, {"capture": ""}])
fx("tray-menu-anim", "§14.22 open frames 0/20/60/120/200 ms, close 0/45/90/150/180 ms (click outside)",
   svc=services(tray=MENU_TRAY), crop=MENU_CROP,
   steps=open_menu + [{"frames": {"name": "open", "at": [0, 20, 60, 120, 200, 400]}},
                      {"click": [1500, 900]}, {"frames": {"name": "close", "at": [0, 45, 90, 150, 180, 400]}}])

OVERFLOW_ICONS = ["google-chrome", "discord", "keepassxc", "fcitx", "wechat", "steam", "thunderbird", "spotify",
                  "applications-system", "firefox", "code", "vlc", "obs", "gimp"]
OVERFLOW_SCREEN = dict(SCREEN, width=1000, height=720)
# A 1-item overflow grid is unreachable: hiding one item costs a 22 px "⋯" button
# instead of an 18 px icon, so if all items do not fit, at least 2 overflow
# (shell.qml trayVisibleCountForBudget). At 1000 px, 7 items stay visible.
for overflow, total in ((2, 9), (6, 13), (7, 14)):
    tray = [{"id": f"app{i}", "title": f"App {i}", "icon": OVERFLOW_ICONS[i]} for i in range(total)]
    fx(f"tray-overflow-{overflow}", f"§14.23 overflow grid with {overflow} item(s) (1000 px output, {total} tray items)",
       svc=services(screens=[OVERFLOW_SCREEN], tray=tray), crop=[500, 0, 500, 200],
       steps=[{"click": {"text": "⋯"}}, {"advance": 600}, {"capture": ""}])

BATTERY = {
    "charging": {"available": True, "status": "Charging", "mode": "charging", "capacity": 64, "powerW": 38.5,
                 "averagePowerW": 35.2, "sampleCount": 120, "sampleWindowSeconds": 1800, "windowComplete": True,
                 "estimateSeconds": 2940, "estimateBasis": "average", "energyNowWh": 48.2, "energyFullWh": 75.3},
    "critical": {"available": True, "status": "Discharging", "mode": "discharging", "capacity": 14, "powerW": -11.8,
                 "averagePowerW": -12.6, "sampleCount": 40, "sampleWindowSeconds": 600, "windowComplete": False,
                 "estimateSeconds": 3360, "estimateBasis": "current", "energyNowWh": 10.5, "energyFullWh": 75.3},
    "full": {"available": True, "status": "Full", "mode": "full", "capacity": 100, "powerW": 0.0,
             "averagePowerW": 0.0, "sampleCount": 10, "sampleWindowSeconds": 30, "windowComplete": False,
             "estimateSeconds": None, "estimateBasis": "none", "energyNowWh": 75.3, "energyFullWh": 75.3},
}
for label, info in BATTERY.items():
    procs = processes() + [{"match": "battery-info.sh", "stdout": json.dumps(info)}]
    # Unreachable from the UI (batteryAvailable needs data that is only polled while the
    # popup is open): seed root.batteryInfo, then click the battery module as a user would.
    fx(f"battery-{label}", f"§14.24 battery popup {label} (harness-seeded; unreachable in the live shell)",
       procs=procs, crop=[2300, 40, 772, 420],
       steps=[{"set": {"root": {"batteryInfo": info}}}, {"advance": 200},
              {"click": {"contains": f"{info['capacity']}%"}}, {"advance": 800}, {"capture": ""}])


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
