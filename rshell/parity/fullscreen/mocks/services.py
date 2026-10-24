#!/usr/bin/python3 -I
"""D-Bus service mocks for the nested capture of the ORIGINAL shell.

Runs on the harness's PRIVATE bus only (run.py points both DBUS_SESSION_BUS_ADDRESS and
DBUS_SYSTEM_BUS_ADDRESS at a dbus-run-session bus with no service activation), and refuses
to start otherwise. It publishes the fixture `services` the real Quickshell modules read:

  tray       StatusNotifierItem per entry (+ com.canonical.dbusmenu when it has a menu),
             registered with the shell's own org.kde.StatusNotifierWatcher
  mpris      org.mpris.MediaPlayer2.parity<N> per player (Stopped, empty metadata)
  bluetooth  org.bluez adapter hci0 + devices (ObjectManager)
  upower     org.freedesktop.UPower + DisplayDevice (battery when `upower` is given)
  powerProfile  net.hadess.PowerProfiles and org.freedesktop.UPower.PowerProfiles

usage: services.py FIXTURE_SERVICES_JSON ASSETS_DIR READY_FILE
Writes READY_FILE once every name is owned and every tray item is registered.
"""
import json
import os
import sys

import dbus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

PROPS = "org.freedesktop.DBus.Properties"
OM = "org.freedesktop.DBus.ObjectManager"


def assert_private_bus():
    addr = os.environ.get("DBUS_SESSION_BUS_ADDRESS", "")
    sysaddr = os.environ.get("DBUS_SYSTEM_BUS_ADDRESS", "")
    if not addr or addr != sysaddr or "/tmp/claude-1000/" not in addr:
        sys.exit(f"services.py: refusing to run outside the harness's private bus ({addr!r})")


class PropObject(dbus.service.Object):
    """Object exporting org.freedesktop.DBus.Properties over a dict per interface."""

    def __init__(self, conn, path, props):
        super().__init__(conn, path)
        self.props = props  # {iface: {name: dbus value}}

    @dbus.service.method(PROPS, in_signature="ss", out_signature="v")
    def Get(self, iface, name):
        return self.props[iface][name]

    @dbus.service.method(PROPS, in_signature="s", out_signature="a{sv}")
    def GetAll(self, iface):
        return dbus.Dictionary(self.props.get(iface, {}), signature="sv")

    @dbus.service.method(PROPS, in_signature="ssv")
    def Set(self, iface, name, value):
        self.props[iface][name] = value
        self.PropertiesChanged(iface, {name: value}, [])

    @dbus.service.signal(PROPS, signature="sa{sv}as")
    def PropertiesChanged(self, iface, changed, invalidated):
        pass


# ------------------------------------------------------------------ tray (SNI + dbusmenu)
def icon_props(icon, assets):
    """SNI/dbusmenu icon: theme name, absolute path, or @asset:file (sent as PNG bytes)."""
    if not icon:
        return {}
    if icon.startswith("@asset:"):
        with open(os.path.join(assets, icon[len("@asset:"):]), "rb") as f:
            return {"icon-data": dbus.ByteArray(f.read())}
    return {"icon-name": dbus.String(icon)}


class DbusMenu(PropObject):
    IFACE = "com.canonical.dbusmenu"

    def __init__(self, conn, path, entries, assets):
        super().__init__(conn, path, {self.IFACE: {
            "Version": dbus.UInt32(3), "TextDirection": "ltr", "Status": "normal",
            "IconThemePath": dbus.Array([], signature="s")}})
        self.items = {}  # id -> (props, child ids)
        self.next_id = 1
        self.items[0] = ({"children-display": "submenu"}, self._add_all(entries, assets))

    def _add_all(self, entries, assets):
        ids = []
        for e in entries:
            i = self.next_id
            self.next_id += 1
            props = {}
            if e.get("isSeparator"):
                props["type"] = "separator"
            else:
                props["label"] = e.get("text", "")
                if "icon" in e:
                    props.update(icon_props(e["icon"], assets))
                if e.get("enabled") is False:
                    props["enabled"] = False
                bt = e.get("buttonType", 0)
                if bt:
                    props["toggle-type"] = "checkmark" if bt == 1 else "radio"
                    props["toggle-state"] = dbus.Int32(1 if e.get("checkState", 0) == 2 else 0)
            children = self._add_all(e.get("entries", []), assets) if e.get("entries") else []
            if children:
                props["children-display"] = "submenu"
            self.items[i] = (props, children)
            ids.append(i)
        return ids

    def _layout(self, item_id, depth):
        props, children = self.items[item_id]
        kids = [] if depth == 0 else [self._layout(c, depth - 1) for c in children]
        return dbus.Struct((dbus.Int32(item_id), dbus.Dictionary(props, signature="sv"),
                            dbus.Array([dbus.Struct(k, variant_level=1) for k in kids], signature="v")),
                           signature="ia{sv}av")

    @dbus.service.method(IFACE, in_signature="iias", out_signature="u(ia{sv}av)")
    def GetLayout(self, parent, depth, names):
        return dbus.UInt32(1), self._layout(parent, depth)

    @dbus.service.method(IFACE, in_signature="aias", out_signature="a(ia{sv})")
    def GetGroupProperties(self, ids, names):
        ids = ids or list(self.items)
        return dbus.Array([dbus.Struct((dbus.Int32(i), dbus.Dictionary(self.items[i][0], signature="sv")))
                           for i in ids if i in self.items], signature="(ia{sv})")

    @dbus.service.method(IFACE, in_signature="is", out_signature="v")
    def GetProperty(self, item_id, name):
        return self.items[item_id][0][name]

    @dbus.service.method(IFACE, in_signature="isvu")
    def Event(self, item_id, event, data, timestamp):
        pass

    @dbus.service.method(IFACE, in_signature="a(isvu)", out_signature="ai")
    def EventGroup(self, events):
        return dbus.Array([], signature="i")

    @dbus.service.method(IFACE, in_signature="i", out_signature="b")
    def AboutToShow(self, item_id):
        return False

    @dbus.service.method(IFACE, in_signature="ai", out_signature="aiai")
    def AboutToShowGroup(self, ids):
        return dbus.Array([], signature="i"), dbus.Array([], signature="i")

    @dbus.service.signal(IFACE, signature="ui")
    def LayoutUpdated(self, revision, parent):
        pass


class TrayItem(PropObject):
    IFACE = "org.kde.StatusNotifierItem"

    def __init__(self, conn, spec, assets):
        has_menu = bool(spec.get("menu"))
        icon = spec.get("icon", "")
        props = {
            "Category": "ApplicationStatus", "Id": spec.get("id", ""), "Title": spec.get("title", ""),
            "Status": "Active", "WindowId": dbus.Int32(0), "IconThemePath": "",
            "IconName": "", "IconPixmap": dbus.Array([], signature="(iiay)"),
            "OverlayIconName": "", "OverlayIconPixmap": dbus.Array([], signature="(iiay)"),
            "AttentionIconName": "", "AttentionIconPixmap": dbus.Array([], signature="(iiay)"),
            "AttentionMovieName": "",
            "ToolTip": dbus.Struct(("", dbus.Array([], signature="(iiay)"), spec.get("tooltipTitle", ""),
                                    spec.get("tooltipDescription", "")), signature="sa(iiay)ss"),
            "ItemIsMenu": dbus.Boolean(spec.get("onlyMenu", False)),
            "Menu": dbus.ObjectPath("/MenuBar" if has_menu else "/NO_DBUSMENU"),
        }
        if icon.startswith("@asset:"):
            props["IconName"] = os.path.join(assets, icon[len("@asset:"):])
        else:
            props["IconName"] = icon
        super().__init__(conn, "/StatusNotifierItem", {self.IFACE: props})
        self.menu = DbusMenu(conn, "/MenuBar", spec["menu"].get("entries", []), assets) if has_menu else None

    @dbus.service.method(IFACE, in_signature="ii")
    def Activate(self, x, y):
        pass

    @dbus.service.method(IFACE, in_signature="ii")
    def SecondaryActivate(self, x, y):
        pass

    @dbus.service.method(IFACE, in_signature="ii")
    def ContextMenu(self, x, y):
        pass

    @dbus.service.method(IFACE, in_signature="is")
    def Scroll(self, delta, orientation):
        pass


# ------------------------------------------------------------------ MPRIS
class MprisPlayer(PropObject):
    def __init__(self, conn, spec):
        super().__init__(conn, "/org/mpris/MediaPlayer2", {
            "org.mpris.MediaPlayer2": {
                "Identity": spec.get("identity", "Player"), "DesktopEntry": spec.get("desktopEntry", ""),
                "CanQuit": False, "CanRaise": False, "HasTrackList": False, "Fullscreen": False,
                "CanSetFullscreen": False,
                "SupportedUriSchemes": dbus.Array([], signature="s"),
                "SupportedMimeTypes": dbus.Array([], signature="s")},
            "org.mpris.MediaPlayer2.Player": {
                "PlaybackStatus": spec.get("playbackStatus", "Stopped"), "LoopStatus": "None",
                "Rate": 1.0, "Shuffle": False, "Volume": 1.0, "Position": dbus.Int64(0),
                "MinimumRate": 1.0, "MaximumRate": 1.0,
                "Metadata": dbus.Dictionary({"mpris:trackid": dbus.ObjectPath("/org/mpris/MediaPlayer2/TrackList/NoTrack")},
                                            signature="sv"),
                "CanGoNext": True, "CanGoPrevious": True, "CanPlay": True, "CanPause": True,
                "CanSeek": False, "CanControl": True}})

    @dbus.service.method("org.mpris.MediaPlayer2.Player")
    def PlayPause(self):
        pass


# ------------------------------------------------------------------ ObjectManager trees (BlueZ)
class ObjectManagerRoot(dbus.service.Object):
    def __init__(self, conn, path, objects):
        super().__init__(conn, path)
        self.objects = objects  # path -> PropObject

    @dbus.service.method(OM, out_signature="a{oa{sa{sv}}}")
    def GetManagedObjects(self):
        return dbus.Dictionary({dbus.ObjectPath(p): dbus.Dictionary(
            {i: dbus.Dictionary(v, signature="sv") for i, v in o.props.items()}, signature="sa{sv}")
            for p, o in self.objects.items()}, signature="oa{sa{sv}}")

    @dbus.service.signal(OM, signature="oa{sa{sv}}")
    def InterfacesAdded(self, path, ifaces):
        pass


class BluezAdapter(PropObject):
    IFACE = "org.bluez.Adapter1"

    @dbus.service.method(IFACE)
    def StartDiscovery(self):
        pass

    @dbus.service.method(IFACE)
    def StopDiscovery(self):
        pass


def bluez_objects(conn, spec):
    objects = {}
    adapter = "/org/bluez/hci0"
    objects[adapter] = BluezAdapter(conn, adapter, {"org.bluez.Adapter1": {
        "Address": "00:1A:7D:DA:71:13", "AddressType": "public", "Name": "parity", "Alias": "parity",
        "Class": dbus.UInt32(0x6c010c), "Powered": bool(spec.get("enabled", True)),
        "PowerState": "on" if spec.get("enabled", True) else "off",
        "Discoverable": False, "DiscoverableTimeout": dbus.UInt32(180), "Pairable": bool(spec.get("pairable", True)),
        "PairableTimeout": dbus.UInt32(0), "Discovering": bool(spec.get("discovering", False)),
        "UUIDs": dbus.Array([], signature="s"), "Modalias": ""}})
    for n, d in enumerate(spec.get("devices", [])):
        addr = d.get("address", f"AA:BB:CC:DD:EE:{n:02X}")
        path = f"{adapter}/dev_{addr.replace(':', '_')}"
        ifaces = {"org.bluez.Device1": {
            "Address": addr, "AddressType": "public", "Name": d.get("name", ""), "Alias": d.get("name", ""),
            "Icon": d.get("icon", ""), "Class": dbus.UInt32(0), "Paired": bool(d.get("paired", False)),
            "Bonded": bool(d.get("paired", False)), "Trusted": bool(d.get("trusted", False)),
            "Blocked": bool(d.get("blocked", False)), "LegacyPairing": False, "Connected": bool(d.get("connected", False)),
            "UUIDs": dbus.Array([], signature="s"), "Adapter": dbus.ObjectPath(adapter), "ServicesResolved": False}}
        if d.get("battery") is not None:
            ifaces["org.bluez.Battery1"] = {"Percentage": dbus.Byte(int(d["battery"]))}
        objects[path] = PropObject(conn, path, ifaces)
    return objects


# ------------------------------------------------------------------ UPower / PowerProfiles
class UPowerRoot(PropObject):
    IFACE = "org.freedesktop.UPower"

    def __init__(self, conn, spec, devices):
        super().__init__(conn, "/org/freedesktop/UPower", {self.IFACE: {
            "DaemonVersion": "1.90.0", "OnBattery": bool(spec.get("onBattery", False)), "LidIsClosed": False,
            "LidIsPresent": False}})
        self.devices = devices

    @dbus.service.method(IFACE, out_signature="ao")
    def EnumerateDevices(self):
        return dbus.Array([dbus.ObjectPath(p) for p in self.devices if not p.endswith("DisplayDevice")], signature="o")

    @dbus.service.method(IFACE, out_signature="o")
    def GetDisplayDevice(self):
        return dbus.ObjectPath("/org/freedesktop/UPower/devices/DisplayDevice")

    @dbus.service.method(IFACE, out_signature="s")
    def GetCriticalAction(self):
        return "PowerOff"


UPOWER_STATES = {"charging": 1, "discharging": 2, "empty": 3, "full": 4, "fully-charged": 4,
                 "pending-charge": 5, "pending-discharge": 6}


def upower_device(conn, path, d):
    present = bool(d.get("isPresent", bool(d)))
    state = d.get("state", 0)
    state = UPOWER_STATES.get(state, 0) if isinstance(state, str) else int(state)
    return PropObject(conn, path, {"org.freedesktop.UPower.Device": {
        "NativePath": "BAT0" if present else "", "Vendor": "", "Model": "", "Serial": "",
        "UpdateTime": dbus.UInt64(0), "Type": dbus.UInt32(2 if present else 0), "PowerSupply": present,
        "HasHistory": False, "HasStatistics": False, "Online": False,
        "Energy": float(d.get("energy", 0)), "EnergyEmpty": 0.0, "EnergyFull": float(d.get("energyFull", 0)),
        "EnergyFullDesign": float(d.get("energyFull", 0)), "EnergyRate": float(d.get("energyRate", 0)),
        "Voltage": 0.0, "ChargeCycles": dbus.Int32(-1), "Luminosity": 0.0,
        "TimeToEmpty": dbus.Int64(int(d.get("timeToEmpty", 0))), "TimeToFull": dbus.Int64(int(d.get("timeToFull", 0))),
        "Percentage": float(d.get("percentage", 0)), "Temperature": 0.0, "IsPresent": present,
        "State": dbus.UInt32(state), "IsRechargeable": present, "Capacity": 100.0, "Technology": dbus.UInt32(1),
        "WarningLevel": dbus.UInt32(1), "BatteryLevel": dbus.UInt32(1), "IconName": d.get("iconName", "")}})


class PowerProfiles(PropObject):
    def __init__(self, conn, path, iface, profile):
        profiles = dbus.Array([dbus.Dictionary({"Profile": p, "Driver": "placeholder"}, signature="sv")
                               for p in ("power-saver", "balanced", "performance")], signature="a{sv}")
        super().__init__(conn, path, {iface: {
            "ActiveProfile": profile, "PerformanceInhibited": "", "PerformanceDegraded": "",
            "Profiles": profiles, "Actions": dbus.Array([], signature="s"),
            "ActiveProfileHolds": dbus.Array([], signature="a{sv}"), "Version": "0.21"}})


# ------------------------------------------------------------------ main
def main():
    assert_private_bus()
    services_path, assets, ready = sys.argv[1:4]
    with open(services_path, encoding="utf-8") as f:
        spec = json.load(f)
    DBusGMainLoop(set_as_default=True)
    addr = os.environ["DBUS_SESSION_BUS_ADDRESS"]
    main_conn = dbus.bus.BusConnection(addr)
    keep = []

    for n, player in enumerate(spec.get("mpris", {}).get("players", [])):
        conn = dbus.bus.BusConnection(addr)
        keep.append((conn, MprisPlayer(conn, player), dbus.service.BusName(f"org.mpris.MediaPlayer2.parity{n}", conn)))

    bt = spec.get("bluetooth")
    if bt is not None:
        conn = dbus.bus.BusConnection(addr)
        objects = bluez_objects(conn, bt)
        keep.append((conn, objects, ObjectManagerRoot(conn, "/", objects), dbus.service.BusName("org.bluez", conn)))

    up = spec.get("upower") or {}
    up_conn = dbus.bus.BusConnection(addr)
    devices = {}
    disp = up.get("displayDevice") or {}
    devices["/org/freedesktop/UPower/devices/DisplayDevice"] = upower_device(up_conn, "/org/freedesktop/UPower/devices/DisplayDevice", disp)
    if disp.get("isPresent"):
        devices["/org/freedesktop/UPower/devices/battery_BAT0"] = upower_device(up_conn, "/org/freedesktop/UPower/devices/battery_BAT0", disp)
    keep.append((up_conn, devices, UPowerRoot(up_conn, up, devices)))
    profile = spec.get("powerProfile", "balanced")
    keep.append(PowerProfiles(up_conn, "/net/hadess/PowerProfiles", "net.hadess.PowerProfiles", profile))
    keep.append(PowerProfiles(up_conn, "/org/freedesktop/UPower/PowerProfiles", "org.freedesktop.UPower.PowerProfiles", profile))
    for name in ("org.freedesktop.UPower", "net.hadess.PowerProfiles", "org.freedesktop.UPower.PowerProfiles"):
        keep.append(dbus.service.BusName(name, up_conn))

    # Tray: one connection (unique name) per item, registered in fixture order once the
    # shell's watcher exists. Registration order = SystemTray.items order.
    tray = spec.get("tray", [])
    items = []
    for t in tray:
        conn = dbus.bus.BusConnection(addr)
        items.append((conn, TrayItem(conn, t, assets)))
    keep.append(items)

    def try_register():
        if not main_conn.name_has_owner("org.kde.StatusNotifierWatcher"):
            return True  # poll again
        watcher = main_conn.get_object("org.kde.StatusNotifierWatcher", "/StatusNotifierWatcher")
        pending = [len(items)]

        def done(*_):
            pending[0] -= 1
            if pending[0] == 0:
                with open(ready, "w") as f:
                    f.write("tray-registered\n")

        def failed(err):
            print(f"services.py: tray registration failed: {err}", file=sys.stderr)

        # Asynchronous: the watcher calls back into the items (same main loop) before it replies.
        for conn, _ in items:
            watcher.RegisterStatusNotifierItem(conn.get_unique_name(), dbus_interface="org.kde.StatusNotifierWatcher",
                                               reply_handler=done, error_handler=failed)
        return False

    if items:
        GLib.timeout_add(50, try_register)
    else:
        with open(ready, "w") as f:
            f.write("ready\n")
    GLib.MainLoop().run()


if __name__ == "__main__":
    main()
