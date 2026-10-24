#!/usr/bin/env python3
"""Fake laptop system-bus services for nested e2e (docs/verify/laptop-qa-plan.md §2.3-2.4).

Exports, on the bus at --address (a private dbus-daemon; the nested driver hands it
to the shell as both session and system bus):
  org.freedesktop.UPower             /org/freedesktop/UPower (+ devices/DisplayDevice,
                                     devices/battery_BAT0, devices/line_power_AC)
  org.freedesktop.UPower.PowerProfiles  /org/freedesktop/UPower/PowerProfiles
  net.hadess.PowerProfiles           /net/hadess/PowerProfiles (same state)
  org.freedesktop.login1             /org/freedesktop/login1 (Manager subset)
  org.hyprv.Test                     /org/hyprv/Test: test-only scenario driver for the
                                     nested suite (see below)

State comes from a scenario JSON ("state" + timed "steps"); further steps can be fed
as NDJSON lines on stdin. Every method call and property write is logged as NDJSON to
--log. Step forms:
  {"at_ms": 500, "battery": {"Percentage": 20.0, "State": 2}, "on_battery": true}
  {"at_ms": 900, "profile": "power-saver"}
  {"at_ms": 1200, "lid_closed": true}
  {"at_ms": 1500, "prepare_for_sleep": true}
Prints "ready <address>" on stdout once every name is owned.

Scenario driver (the nested suite cannot reach this process's stdin; it gets this bus as
HYPRV_TEST_SYSTEM_BUS_ADDRESS):
  org.hyprv.Test.Step(s step) -> (s state)
      step = a step JSON object (forms above, "at_ms" ignored) or a preset name:
        plug       Discharging -> Charging, OnBattery false (lap.battery.plug)
        unplug     Charging -> Discharging, OnBattery true
        full       FullyCharged at 100 %, OnBattery false (lap.battery.full)
        tick:<pct> Percentage = pct (lap.battery.discharge.tick)
        lid-close / lid-open                   (lap.lid.close_open)
        suspend    PrepareForSleep(true)       (lap.sleep.resume)
        resume     PrepareForSleep(false)
        profile:<name>  external ActiveProfile change (lap.power.profile.external)
      Applied synchronously: every PropertiesChanged / PrepareForSleep is emitted before
      the reply, so a client that waits for the reply and then for its own signal
      handling never races the step. Returns State().
  org.hyprv.Test.State() -> (s json)
      {"on_battery", "battery": {...}, "profile", "lid_closed", "preparing", "calls": n}
      where calls = method calls + property writes logged so far (excluding org.hyprv.Test).

Requires python-gobject (already required by the legacy agent watcher). No root.
"""
import argparse
import json
import os
import sys
import time

from gi.repository import Gio, GLib

UPOWER_XML = """
<node>
 <interface name="org.freedesktop.UPower">
  <method name="EnumerateDevices"><arg type="ao" direction="out"/></method>
  <method name="GetDisplayDevice"><arg type="o" direction="out"/></method>
  <method name="GetCriticalAction"><arg type="s" direction="out"/></method>
  <signal name="DeviceAdded"><arg type="o"/></signal>
  <signal name="DeviceRemoved"><arg type="o"/></signal>
  <property name="DaemonVersion" type="s" access="read"/>
  <property name="OnBattery" type="b" access="read"/>
  <property name="LidIsClosed" type="b" access="read"/>
  <property name="LidIsPresent" type="b" access="read"/>
 </interface>
</node>"""

DEVICE_XML = """
<node>
 <interface name="org.freedesktop.UPower.Device">
  <method name="Refresh"/>
  <property name="NativePath" type="s" access="read"/>
  <property name="Vendor" type="s" access="read"/>
  <property name="Model" type="s" access="read"/>
  <property name="Type" type="u" access="read"/>
  <property name="PowerSupply" type="b" access="read"/>
  <property name="Online" type="b" access="read"/>
  <property name="IsPresent" type="b" access="read"/>
  <property name="IsRechargeable" type="b" access="read"/>
  <property name="State" type="u" access="read"/>
  <property name="Percentage" type="d" access="read"/>
  <property name="Energy" type="d" access="read"/>
  <property name="EnergyFull" type="d" access="read"/>
  <property name="EnergyFullDesign" type="d" access="read"/>
  <property name="EnergyRate" type="d" access="read"/>
  <property name="Voltage" type="d" access="read"/>
  <property name="TimeToEmpty" type="x" access="read"/>
  <property name="TimeToFull" type="x" access="read"/>
  <property name="BatteryLevel" type="u" access="read"/>
  <property name="WarningLevel" type="u" access="read"/>
  <property name="IconName" type="s" access="read"/>
 </interface>
</node>"""

PROFILES_XML = """
<node>
 <interface name="{iface}">
  <method name="HoldProfile">
   <arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/>
   <arg type="u" direction="out"/>
  </method>
  <method name="ReleaseProfile"><arg type="u" direction="in"/></method>
  <signal name="ProfileReleased"><arg type="u"/></signal>
  <property name="ActiveProfile" type="s" access="readwrite"/>
  <property name="PerformanceDegraded" type="s" access="read"/>
  <property name="PerformanceInhibited" type="s" access="read"/>
  <property name="Profiles" type="aa{{sv}}" access="read"/>
  <property name="Actions" type="as" access="read"/>
  <property name="ActiveProfileHolds" type="aa{{sv}}" access="read"/>
 </interface>
</node>"""

LOGIN_XML = """
<node>
 <interface name="org.freedesktop.login1.Manager">
  <method name="Inhibit">
   <arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/>
   <arg type="s" direction="in"/><arg type="h" direction="out"/>
  </method>
  <method name="Suspend"><arg type="b" direction="in"/></method>
  <signal name="PrepareForSleep"><arg type="b"/></signal>
  <signal name="PrepareForShutdown"><arg type="b"/></signal>
  <property name="PreparingForSleep" type="b" access="read"/>
  <property name="LidClosed" type="b" access="read"/>
  <property name="OnExternalPower" type="b" access="read"/>
  <property name="Docked" type="b" access="read"/>
 </interface>
</node>"""

DEVICE_TYPES = {"Type": "u", "State": "u", "BatteryLevel": "u", "WarningLevel": "u",
                "TimeToEmpty": "x", "TimeToFull": "x", "NativePath": "s", "Vendor": "s", "Model": "s",
                "IconName": "s"}
BOOL_PROPS = {"PowerSupply", "Online", "IsPresent", "IsRechargeable"}

ROOT = "/org/freedesktop/UPower"
DISPLAY = ROOT + "/devices/DisplayDevice"
BAT = ROOT + "/devices/battery_BAT0"
AC = ROOT + "/devices/line_power_AC"


TEST_XML = """
<node>
 <interface name="org.hyprv.Test">
  <method name="Step"><arg type="s" direction="in"/><arg type="s" direction="out"/></method>
  <method name="State"><arg type="s" direction="out"/></method>
 </interface>
</node>
"""

PRESETS = {
    "plug": {"battery": {"State": 1}, "on_battery": False},
    "unplug": {"battery": {"State": 2}, "on_battery": True},
    "full": {"battery": {"State": 4, "Percentage": 100.0}, "on_battery": False},
    "lid-close": {"lid_closed": True},
    "lid-open": {"lid_closed": False},
    "suspend": {"prepare_for_sleep": True},
    "resume": {"prepare_for_sleep": False},
}


def preset(text):
    """A Step argument: JSON object, or a preset name (see the module docstring)."""
    text = text.strip()
    if text.startswith("{"):
        return json.loads(text)
    if text in PRESETS:
        return PRESETS[text]
    kind, _, value = text.partition(":")
    if kind == "profile" and value:
        return {"profile": value}
    if kind == "tick" and value:
        return {"battery": {"Percentage": float(value)}}
    raise ValueError(f"unknown step {text!r}")


def variant_for(name, value):
    if name in BOOL_PROPS:
        return GLib.Variant("b", bool(value))
    kind = DEVICE_TYPES.get(name, "d")
    if kind == "d":
        value = float(value)
    elif kind in ("u", "x"):
        value = int(value)
    return GLib.Variant(kind, value)


class World:
    def __init__(self, scenario, log):
        state = scenario.get("state", {})
        self.log_file = log
        self.on_battery = bool(state.get("on_battery", True))
        self.lid_closed = bool(state.get("lid_closed", False))
        self.lid_present = bool(state.get("lid_present", True))
        self.preparing = False
        self.profile = state.get("profile", "balanced")
        self.has_performance = bool(state.get("has_performance", True))
        self.degraded = state.get("performance_degraded", "")
        self.battery_present = bool(state.get("battery_present", True))
        battery = {"NativePath": "BAT0", "Vendor": "SMP", "Model": "DELL XPS", "Type": 2,
                   "PowerSupply": True, "Online": False, "IsPresent": self.battery_present,
                   "IsRechargeable": True, "State": 2, "Percentage": 57.0, "Energy": 30.8,
                   "EnergyFull": 54.0, "EnergyFullDesign": 55.0, "EnergyRate": 8.4, "Voltage": 11.9,
                   "TimeToEmpty": 13200, "TimeToFull": 0, "BatteryLevel": 1, "WarningLevel": 1,
                   "IconName": "battery-good-symbolic"}
        battery.update(state.get("battery", {}))
        self.devices = {
            BAT: battery,
            AC: {"NativePath": "AC", "Vendor": "", "Model": "", "Type": 1, "PowerSupply": True,
                 "Online": not self.on_battery, "IsPresent": True, "IsRechargeable": False, "State": 0,
                 "Percentage": 0.0, "Energy": 0.0, "EnergyFull": 0.0, "EnergyFullDesign": 0.0,
                 "EnergyRate": 0.0, "Voltage": 0.0, "TimeToEmpty": 0, "TimeToFull": 0, "BatteryLevel": 0,
                 "WarningLevel": 0, "IconName": "ac-adapter-symbolic"},
        }
        self.devices[DISPLAY] = dict(battery, NativePath="", Type=2 if self.battery_present else 0)
        self.conn = None
        self.next_cookie = 1
        self.inhibitors = []
        self.calls = 0

    def log(self, **entry):
        entry["t"] = round(time.monotonic(), 3)
        self.log_file.write(json.dumps(entry, ensure_ascii=False) + "\n")
        self.log_file.flush()

    # -- property helpers -------------------------------------------------------
    def emit_changed(self, path, iface, changed):
        if not changed:
            return
        self.conn.emit_signal(None, path, "org.freedesktop.DBus.Properties", "PropertiesChanged",
                              GLib.Variant("(sa{sv}as)", (iface, changed, [])))

    def profiles_value(self):
        names = ["power-saver", "balanced"] + (["performance"] if self.has_performance else [])
        return [{"Profile": GLib.Variant("s", n), "Driver": GLib.Variant("s", "fake"),
                 "PlatformDriver": GLib.Variant("s", "fake"), "CpuDriver": GLib.Variant("s", "fake")}
                for n in names]

    def profile_props(self):
        return {"ActiveProfile": GLib.Variant("s", self.profile),
                "PerformanceDegraded": GLib.Variant("s", self.degraded),
                "PerformanceInhibited": GLib.Variant("s", ""),
                "Profiles": GLib.Variant("aa{sv}", self.profiles_value()),
                "Actions": GLib.Variant("as", []),
                "ActiveProfileHolds": GLib.Variant("aa{sv}", [])}

    def get(self, path, iface, name):
        if iface == "org.freedesktop.UPower":
            return {"DaemonVersion": GLib.Variant("s", "1.90.6-fake"),
                    "OnBattery": GLib.Variant("b", self.on_battery),
                    "LidIsClosed": GLib.Variant("b", self.lid_closed),
                    "LidIsPresent": GLib.Variant("b", self.lid_present)}[name]
        if iface == "org.freedesktop.UPower.Device":
            return variant_for(name, self.devices[path][name])
        if iface.endswith("PowerProfiles"):
            return self.profile_props()[name]
        if iface == "org.freedesktop.login1.Manager":
            return {"PreparingForSleep": GLib.Variant("b", self.preparing),
                    "LidClosed": GLib.Variant("b", self.lid_closed),
                    "OnExternalPower": GLib.Variant("b", not self.on_battery),
                    "Docked": GLib.Variant("b", False)}[name]
        raise KeyError(name)

    def set_profile(self, profile, source):
        if profile == self.profile:
            return
        self.profile = profile
        self.log(event="profile", value=profile, source=source)
        for path, iface in (("/org/freedesktop/UPower/PowerProfiles", "org.freedesktop.UPower.PowerProfiles"),
                            ("/net/hadess/PowerProfiles", "net.hadess.PowerProfiles")):
            self.emit_changed(path, iface, {"ActiveProfile": GLib.Variant("s", profile)})

    # -- scenario steps ---------------------------------------------------------
    def apply(self, step):
        self.log(event="step", step=step)
        if "battery" in step:
            changed = {}
            for key, value in step["battery"].items():
                for path in (BAT, DISPLAY):
                    self.devices[path][key] = value
                changed[key] = variant_for(key, value)
            for path in (BAT, DISPLAY):
                self.emit_changed(path, "org.freedesktop.UPower.Device", changed)
        if "on_battery" in step:
            self.on_battery = bool(step["on_battery"])
            self.devices[AC]["Online"] = not self.on_battery
            self.emit_changed(ROOT, "org.freedesktop.UPower", {"OnBattery": GLib.Variant("b", self.on_battery)})
            self.emit_changed(AC, "org.freedesktop.UPower.Device", {"Online": GLib.Variant("b", not self.on_battery)})
            self.emit_changed("/org/freedesktop/login1", "org.freedesktop.login1.Manager",
                              {"OnExternalPower": GLib.Variant("b", not self.on_battery)})
        if "profile" in step:
            self.set_profile(step["profile"], "scenario")
        if "performance_degraded" in step:
            self.degraded = step["performance_degraded"]
            for path, iface in (("/org/freedesktop/UPower/PowerProfiles", "org.freedesktop.UPower.PowerProfiles"),
                                ("/net/hadess/PowerProfiles", "net.hadess.PowerProfiles")):
                self.emit_changed(path, iface, {"PerformanceDegraded": GLib.Variant("s", self.degraded)})
        if "lid_closed" in step:
            self.lid_closed = bool(step["lid_closed"])
            self.emit_changed(ROOT, "org.freedesktop.UPower", {"LidIsClosed": GLib.Variant("b", self.lid_closed)})
            self.emit_changed("/org/freedesktop/login1", "org.freedesktop.login1.Manager",
                              {"LidClosed": GLib.Variant("b", self.lid_closed)})
        if "prepare_for_sleep" in step:
            self.preparing = bool(step["prepare_for_sleep"])
            self.conn.emit_signal(None, "/org/freedesktop/login1", "org.freedesktop.login1.Manager",
                                  "PrepareForSleep", GLib.Variant("(b)", (self.preparing,)))
        return GLib.SOURCE_REMOVE

    def state_json(self):
        return json.dumps({"on_battery": self.on_battery, "battery": self.devices[BAT], "profile": self.profile,
                           "lid_closed": self.lid_closed, "preparing": self.preparing, "calls": self.calls},
                          default=str)

    # -- method dispatch ------------------------------------------------------------
    def method(self, conn, sender, path, iface, name, params, invocation):
        if iface == "org.hyprv.Test":
            if name == "Step":
                try:
                    step = preset(params.unpack()[0])
                except (ValueError, json.JSONDecodeError) as error:
                    return invocation.return_dbus_error("org.freedesktop.DBus.Error.InvalidArgs", str(error))
                self.log(event="test_step", sender=sender, step=step)
                self.apply({k: v for k, v in step.items() if k != "at_ms"})
                conn.flush_sync(None)
                return invocation.return_value(GLib.Variant("(s)", (self.state_json(),)))
            if name == "State":
                return invocation.return_value(GLib.Variant("(s)", (self.state_json(),)))
        self.calls += 1
        self.log(event="call", sender=sender, path=path, iface=iface, member=name, args=params.unpack())
        if iface == "org.freedesktop.UPower":
            if name == "EnumerateDevices":
                devs = [BAT, AC] if self.battery_present else [AC]
                return invocation.return_value(GLib.Variant("(ao)", (devs,)))
            if name == "GetDisplayDevice":
                return invocation.return_value(GLib.Variant("(o)", (DISPLAY,)))
            if name == "GetCriticalAction":
                return invocation.return_value(GLib.Variant("(s)", ("HybridSleep",)))
        if iface == "org.freedesktop.UPower.Device" and name == "Refresh":
            return invocation.return_value(None)
        if iface.endswith("PowerProfiles"):
            if name == "HoldProfile":
                cookie = self.next_cookie
                self.next_cookie += 1
                return invocation.return_value(GLib.Variant("(u)", (cookie,)))
            if name == "ReleaseProfile":
                return invocation.return_value(None)
        if iface == "org.freedesktop.login1.Manager":
            if name == "Inhibit":
                read_fd, write_fd = os.pipe()
                self.inhibitors.append(read_fd)  # held until the client closes its end
                fds = Gio.UnixFDList.new()
                index = fds.append(write_fd)
                os.close(write_fd)
                return invocation.return_value_with_unix_fd_list(GLib.Variant("(h)", (index,)), fds)
            if name == "Suspend":
                return invocation.return_value(None)
        invocation.return_dbus_error("org.freedesktop.DBus.Error.UnknownMethod", f"{iface}.{name}")

    def get_property(self, conn, sender, path, iface, name):
        return self.get(path, iface, name)

    def set_property(self, conn, sender, path, iface, name, value):
        self.calls += 1
        self.log(event="set", sender=sender, path=path, iface=iface, member=name, value=value.unpack())
        if iface.endswith("PowerProfiles") and name == "ActiveProfile":
            self.set_profile(value.unpack(), "client")
            return True
        return False


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--address", required=True)
    parser.add_argument("--scenario", required=True)
    parser.add_argument("--log", required=True)
    args = parser.parse_args()
    scenario = json.loads(open(args.scenario, encoding="utf-8").read())
    world = World(scenario, open(args.log, "a", encoding="utf-8"))
    conn = Gio.DBusConnection.new_for_address_sync(
        args.address, Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,
        None, None)
    world.conn = conn

    def register(xml, path):
        info = Gio.DBusNodeInfo.new_for_xml(xml).interfaces[0]
        conn.register_object(path, info, world.method, world.get_property, world.set_property)

    register(UPOWER_XML, ROOT)
    for path in (DISPLAY, BAT, AC):
        register(DEVICE_XML, path)
    register(PROFILES_XML.format(iface="org.freedesktop.UPower.PowerProfiles"), "/org/freedesktop/UPower/PowerProfiles")
    register(PROFILES_XML.format(iface="net.hadess.PowerProfiles"), "/net/hadess/PowerProfiles")
    register(LOGIN_XML, "/org/freedesktop/login1")
    register(TEST_XML, "/org/hyprv/Test")

    names = ["org.freedesktop.UPower", "org.freedesktop.UPower.PowerProfiles", "net.hadess.PowerProfiles",
             "org.freedesktop.login1", "org.hyprv.Test"]
    for name in names:
        reply = conn.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
                               "RequestName", GLib.Variant("(su)", (name, 4)), GLib.VariantType("(u)"),
                               Gio.DBusCallFlags.NONE, -1, None)
        if reply.unpack()[0] != 1:
            sys.exit(f"fake-system-bus: could not own {name}")
    world.log(event="ready", names=names)

    for step in scenario.get("steps", []):
        GLib.timeout_add(max(1, int(step.get("at_ms", 0))), world.apply, step)

    def on_stdin(_fd, condition):
        if condition & GLib.IOCondition.HUP:
            return GLib.SOURCE_REMOVE
        line = sys.stdin.readline()
        if not line:
            return GLib.SOURCE_REMOVE
        if line.strip():
            world.apply(json.loads(line))
        return GLib.SOURCE_CONTINUE

    GLib.io_add_watch(sys.stdin, GLib.IOCondition.IN | GLib.IOCondition.HUP, on_stdin)
    print(f"ready {args.address}", flush=True)
    GLib.MainLoop().run()


if __name__ == "__main__":
    main()
