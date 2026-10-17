#!/usr/bin/env python3
"""One read-only stream for kernel samples, NM events, and agent observations."""
from __future__ import annotations

import json
from pathlib import Path
import sys
import time

import gi

gi.require_version("NM", "1.0")
from gi.repository import GLib, NM

from agent_island import PROCESS_SCAN_INTERVAL_SECONDS, locked_state, public_snapshot, state_file, write_state


class Publisher:
    def __init__(self):
        self.previous = {}

    def emit(self, kind, data):
        encoded = json.dumps(data, ensure_ascii=False, separators=(",", ":"))
        if self.previous.get(kind) != encoded:
            print('{"type":' + json.dumps(kind) + ',"data":' + encoded + '}', flush=True)
            self.previous[kind] = encoded


def read_text(path):
    try:
        return Path(path).read_text()
    except OSError:
        return ""


class KernelSampler:
    def __init__(self, publisher):
        self.publisher = publisher
        self.temperature = None
        self.last_discovery = -60.0

    def discover_temperature(self):
        zones = list(Path("/sys/class/thermal").glob("thermal_zone*"))
        for name in ("x86_pkg_temp", "TCPU", "cpu_thermal", "cpu-thermal", "soc_thermal"):
            for zone in zones:
                if read_text(zone / "type").strip() == name:
                    value = read_text(zone / "temp").strip()
                    if value.isdigit() and 1 <= int(value) <= 125000:
                        return zone / "temp"
        for hwmon in Path("/sys/class/hwmon").glob("hwmon*"):
            if read_text(hwmon / "name").strip() not in ("coretemp", "k10temp", "zenpower"):
                continue
            for label in hwmon.glob("temp*_label"):
                if read_text(label).strip() in ("Package id 0", "Tctl", "CPU Package", "CPU"):
                    path = label.with_name(label.name.replace("_label", "_input"))
                    value = read_text(path).strip()
                    if value.isdigit() and 1 <= int(value) <= 125000:
                        return path
        return None

    def tick(self):
        now = time.monotonic()
        if (self.temperature is None or not self.temperature.exists()) and now - self.last_discovery >= 30:
            self.temperature = self.discover_temperature()
            self.last_discovery = now
        # Interrupt counters and scheduler counters are never consumed by the UI.
        cpu = "\n".join(line for line in read_text("/proc/stat").splitlines() if line.startswith("cpu"))
        sections = [
            ("__STAT__", cpu), ("__MEM__", read_text("/proc/meminfo")),
            ("__TEMP__", read_text(self.temperature) if self.temperature else ""),
            ("__CPU_POWER__", read_text("/run/hyprv/cpu-package-power-w")),
            ("__ROUTE__", read_text("/proc/net/route")), ("__NET__", read_text("/proc/net/dev")),
        ]
        self.publisher.emit("system", "\n".join(name + "\n" + value.rstrip() for name, value in sections))
        return GLib.SOURCE_CONTINUE


class NetworkObserver:
    def __init__(self, publisher):
        self.publisher = publisher
        self.client = NM.Client.new(None)
        self.connections = []
        self.pending = 0
        self.client.connect("notify", self.schedule)
        self.client.connect("device-added", self.schedule)
        self.client.connect("device-removed", self.schedule)
        self.client.connect("connection-added", self.schedule)
        self.client.connect("connection-removed", self.schedule)
        self.refresh()

    def schedule(self, *_args):
        if not self.pending:
            self.pending = GLib.timeout_add(60, self.refresh)

    def watch(self, obj, signal):
        self.connections.append((obj, obj.connect(signal, self.schedule)))

    def refresh(self):
        self.pending = 0
        for obj, handler in self.connections:
            obj.disconnect(handler)
        self.connections.clear()
        devices = [d for d in self.client.get_devices() if isinstance(d, NM.DeviceWifi)]
        device = devices[0] if devices else None
        networks = []
        ssid = ""
        signal = 0
        security = ""
        connected = False
        settings = self.client.get_connections()
        known_ssids = set()
        for connection in settings:
            self.watch(connection, "changed")
            wireless = connection.get_setting_wireless()
            raw = wireless.get_ssid() if wireless else None
            if raw:
                known_ssids.add(NM.utils_ssid_to_utf8(raw.get_data()))
        if device:
            self.watch(device, "notify")
            self.watch(device, "access-point-added")
            self.watch(device, "access-point-removed")
            active = device.get_active_access_point()
            access_points = list(device.get_access_points())
            if active is not None and active not in access_points:
                access_points.append(active)
            flags_type = getattr(NM, "80211ApSecurityFlags")
            enterprise_flags = int(flags_type.KEY_MGMT_802_1X)
            for ap in access_points:
                self.watch(ap, "notify")
                raw = ap.get_ssid()
                name = NM.utils_ssid_to_utf8(raw.get_data()) if raw else ""
                if not name:
                    continue
                flags = int(ap.get_wpa_flags()) | int(ap.get_rsn_flags())
                enterprise = bool(flags & enterprise_flags)
                secure = bool(flags or int(ap.get_flags()) & 1)
                sec = "802.1X" if enterprise else ("WPA" if flags else ("WEP" if secure else ""))
                is_active = ap == active
                entry = {"ssid": name, "signal": ap.get_strength(), "security": sec,
                         "secure": secure, "enterprise": enterprise, "active": is_active,
                         "known": name in known_ssids}
                networks.append(entry)
                if is_active:
                    ssid, signal, security, connected = name, entry["signal"], sec, True
        # Same strongest/active AP per SSID rule as wifi-status.sh.
        best = {}
        for network in networks:
            previous = best.get(network["ssid"])
            if previous is None or (network["active"], network["signal"]) > (previous["active"], previous["signal"]):
                best[network["ssid"]] = network
        networks = sorted(best.values(), key=lambda n: (not n["active"], -n["signal"], n["ssid"]))
        self.publisher.emit("wifi", {
            "present": device is not None, "iface": device.get_iface() if device else "",
            "enabled": self.client.wireless_get_enabled(),
            "hardwareEnabled": self.client.wireless_hardware_get_enabled(),
            "connected": connected, "ssid": ssid, "signal": signal,
            "security": security, "networks": networks,
        })
        return GLib.SOURCE_REMOVE


class AgentObserver:
    def __init__(self, publisher):
        self.publisher = publisher
        self.state = None
        self.signature = None
        self.tick()

    def tick(self):
        path = state_file()
        try:
            signature = path.stat().st_mtime_ns
        except OSError:
            signature = None
        scan_due = self.state is None or time.time() - float(self.state.get("last_process_scan") or 0) >= PROCESS_SCAN_INTERVAL_SECONDS
        if self.state is None or signature != self.signature or scan_due:
            with locked_state(write=False) as state:
                snapshot = public_snapshot(state)
                write_state(state)
                self.state = state
                # Capture our own commit before releasing the lock. A hook write
                # immediately afterwards must invalidate the cached snapshot.
                self.signature = path.stat().st_mtime_ns
        else:
            # Animate from cached task state between scans. Hook writes invalidate
            # it on the next tick; never overwrite a newer hook with this cache.
            snapshot = public_snapshot(self.state, scan_processes=False)
        # Focus metadata and tool history remain in state.json. The bar only needs
        # display roles; don't repeatedly ship entire histories into QML models.
        fields = ("id", "source", "source_label", "title", "project", "status", "current_tool", "detail", "progress")
        sessions = [{k: s[k] for k in fields if k in s} for s in snapshot["sessions"]]
        self.publisher.emit("agent", {"active": snapshot["active"], "count": snapshot["count"], "sessions": sessions})
        GLib.timeout_add(650 if snapshot["active"] else 1200, self.tick)
        return GLib.SOURCE_REMOVE


def main():
    publisher = Publisher()
    kernel = KernelSampler(publisher)
    kernel.tick()
    GLib.timeout_add(1000, kernel.tick)
    network = NetworkObserver(publisher)
    agent = AgentObserver(publisher)
    GLib.MainLoop().run()


if __name__ == "__main__":
    try:
        main()
    except BrokenPipeError:
        sys.exit(0)
