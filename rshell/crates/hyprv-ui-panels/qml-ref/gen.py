#!/usr/bin/env python3
"""Generate QML reference scenes from fixtures.json (exported from the Rust fixtures).

Scenes instantiate the ORIGINAL quickshell components (read-only import) with a mock
shellRoot. Layout blocks that live inline in popup files are copied verbatim from
ControlPanelPopup.qml (mainPageComponent :518-737) with popupRoot.* resolved."""
import json, os, sys

here = os.path.dirname(os.path.abspath(__file__))
quickshell = os.path.realpath(os.path.join(here, "../../../../quickshell"))
scenes = os.path.join(here, "scenes")
os.makedirs(scenes, exist_ok=True)
fixtures = json.load(open(os.path.join(here, "fixtures.json")))

def q(v):
    return json.dumps(v, ensure_ascii=False)

def shell(m):
    props = [f"darkMode: {q(m['dark'])}"]
    for k, v in m.items():
        if k in ("dark", "vmRunning", "vmBusy", "viewflowTarget", "viewflowBusy", "wifiConnected", "wifiSsid", "net", "stats", "calendar", "wall"):
            continue
        props.append(f"{k}: {q(v)}")
    return "ShellMock {\n        id: sh\n        " + "\n        ".join(props) + f"\n        property bool wifiConnected: {q(m['wifiConnected'])}\n        property string wifiSsid: {q(m['wifiSsid'])}\n    }}"

HELPERS = r'''
    function currentWifiSubtitle() {
        if (!sh.wifiRadioEnabled) return "Turned off";
        if (sh.wifiConnected) return sh.wifiSsid.length > 0 ? sh.wifiSsid : "Connected";
        return "Not connected";
    }
    function currentBluetoothTitle() {
        for (let i = 0; i < sh.bluetoothDevices.length; i++) {
            const device = sh.bluetoothDevices[i];
            if (device.connected && (device.name || "").length > 0) return device.name;
        }
        return "Bluetooth";
    }
    function currentBluetoothSubtitle() {
        if (!sh.bluetoothPresent) return "No adapter";
        if (!sh.bluetoothEnabled) return "Turned off";
        let c = 0, p = 0;
        for (let i = 0; i < sh.bluetoothDevices.length; i++) { if (sh.bluetoothDevices[i].connected) c++; if (sh.bluetoothDevices[i].paired) p++; }
        if (c > 1) return c + " connected";
        if (c === 1) return "Connected";
        if (sh.bluetoothDiscovering) return "Scanning";
        if (p > 0) return p + " paired";
        return "Ready";
    }
    function powerProfileIcon(profile) {
        if (profile === "power-saver") return "\u{f0f86}";
        if (profile === "performance") return "\u{f04c5}";
        return "\u{f0f85}";
    }
'''

def main_page(m):
    return f'''import QtQuick
import ".."
import "{os.path.relpath(quickshell, scenes)}"

Item {{
    id: popupRoot
    width: 278; height: 422
    readonly property var shellRoot: sh
    readonly property int moduleSize: 62
    readonly property int cardSpacing: 10
    readonly property int columnSpacing: 10
    readonly property int columnWidth: 134
    {shell(m)}
    QtObject {{ id: windowsVmController; property bool running: {q(m["vmRunning"])}; property bool busy: {q(m["vmBusy"])}; function toggleVm() {{}} }}
{HELPERS}
    Column {{
        width: 278
        spacing: popupRoot.cardSpacing
        Row {{
            width: parent.width
            spacing: popupRoot.columnSpacing
            Column {{
                width: popupRoot.columnWidth
                spacing: popupRoot.cardSpacing
                ControlPanelSplitTile {{ width: parent.width; height: popupRoot.moduleSize; shellRoot: sh; icon: active ? "\\u{{f0928}}" : "\\u{{f092d}}"; title: "WIFI"; subtitle: popupRoot.currentWifiSubtitle(); active: sh.wifiRadioEnabled }}
                ControlPanelSplitTile {{ width: parent.width; height: popupRoot.moduleSize; shellRoot: sh; icon: active ? "\\u{{f00af}}" : "\\u{{f00b2}}"; title: popupRoot.currentBluetoothTitle(); subtitle: popupRoot.currentBluetoothSubtitle(); active: sh.bluetoothEnabled }}
                Row {{
                    width: parent.width
                    spacing: popupRoot.cardSpacing
                    WindowsVmControl {{ width: (parent.width - popupRoot.cardSpacing) / 2; height: popupRoot.moduleSize; shellRoot: sh; controller: windowsVmController }}
                    // ViewflowTargetControl (= ControlPanelToggle with these bindings; its Process/poll omitted)
                    ControlPanelToggle {{ width: (parent.width - popupRoot.cardSpacing) / 2; height: popupRoot.moduleSize; shellRoot: sh; icon: {q(m["viewflowTarget"])} === "macos" ? "\\u{{f179}}" : "\\u{{f05b3}}"; iconOnly: true; active: false; toggleEnabled: !{q(m["viewflowBusy"])} }}
                }}
                Row {{
                    width: parent.width
                    spacing: popupRoot.cardSpacing
                    ControlPanelToggle {{ width: (parent.width - popupRoot.cardSpacing) / 2; height: popupRoot.moduleSize; shellRoot: sh; icon: "\\u{{f0ec3}}"; iconOnly: true; label: "Screen\\nrecord"; active: sh.screenRecording }}
                    ControlPanelToggle {{ width: (parent.width - popupRoot.cardSpacing) / 2; height: popupRoot.moduleSize; shellRoot: sh; icon: active ? "\\u{{f0f4}}" : "\\u{{f236}}"; iconOnly: true; label: "Prevent\\nsleep"; active: sh.preventSleepEnabled }}
                }}
            }}
            Column {{
                width: popupRoot.columnWidth
                spacing: popupRoot.cardSpacing
                ControlPanelMediaCard {{ width: parent.width; height: popupRoot.moduleSize * 2 + popupRoot.cardSpacing; shellRoot: sh; available: sh.mediaAvailable; playing: sh.mediaPlaying; title: sh.mediaTitle; subtitle: sh.mediaArtist; playerName: sh.mediaPlayerName; artUrl: sh.mediaArtUrl }}
                Row {{
                    width: parent.width
                    spacing: popupRoot.cardSpacing
                    ControlPanelToggle {{ width: (parent.width - popupRoot.cardSpacing) / 2; height: popupRoot.moduleSize; shellRoot: sh; iconSource: "file://{quickshell}/assets/control-panel/light-dark.svg"; iconOnly: true; active: sh.darkMode }}
                    ControlPanelToggle {{ width: (parent.width - popupRoot.cardSpacing) / 2; height: popupRoot.moduleSize; shellRoot: sh; icon: "\\u{{f009b}}"; iconOnly: true; active: sh.dndEnabled }}
                }}
                Row {{
                    width: parent.width
                    spacing: popupRoot.cardSpacing
                    ControlPanelToggle {{ width: (parent.width - popupRoot.cardSpacing) / 2; height: popupRoot.moduleSize; shellRoot: sh; icon: "\\u{{f0379}}"; iconOnly: true; active: sh.demoModeEnabled }}
                    ControlPanelToggle {{ width: (parent.width - popupRoot.cardSpacing) / 2; height: popupRoot.moduleSize; shellRoot: sh; icon: popupRoot.powerProfileIcon(sh.powerProfile); iconOnly: true; active: false }}
                }}
            }}
        }}
        Grid {{
            width: parent.width
            columns: 2
            spacing: popupRoot.cardSpacing
            ControlPanelSlider {{ shellRoot: sh; width: popupRoot.columnWidth; height: popupRoot.moduleSize; icon: "\\u{{f00df}}"; value: sh.brightnessPercent }}
            ControlPanelSlider {{ shellRoot: sh; width: popupRoot.columnWidth; height: popupRoot.moduleSize; icon: sh.volumeIcon; iconClickable: true; value: sh.audioVolumePercent }}
            ControlPanelSlider {{ shellRoot: sh; width: popupRoot.columnWidth; height: popupRoot.moduleSize; enabled: sh.fanControlAvailable; opacity: enabled ? 1 : 0.48; icon: "\\u{{f0210}}"; value: sh.chassisFanPercent }}
            ControlPanelSlider {{ shellRoot: sh; width: popupRoot.columnWidth; height: popupRoot.moduleSize; enabled: sh.fanControlAvailable; opacity: enabled ? 1 : 0.48; icon: "\\u{{f0210}}"; value: sh.pumpFanPercent }}
        }}
    }}
}}
'''

def bt_page(m):
    # Card chrome = AnimatedGlassPanel at rest (fill/border × 0.82, radius 19), width 384.
    return f'''import QtQuick
import ".."
import "{os.path.relpath(quickshell, scenes)}"

Item {{
    id: root
    width: 384; height: details.implicitHeight
    {shell(m)}
    AnimatedGlassPanel {{
        width: parent.width
        fullPanelHeight: parent.height
        fillColor: sh.withAlpha(sh.darkMode ? "#101214" : "#ffffff", sh.darkMode ? 0.42 : 0.28)
        strokeColor: sh.withAlpha(sh.primaryText, sh.darkMode ? 0.14 : 0.10)
        shadowColor: "transparent"
        devicePixelRatio: 2
        revealHeight: fullPanelHeight
        contentOpacity: 1
        contentOffset: 0
    }}
    Column {{
        width: 384
        spacing: 10
        ControlPanelBluetoothDetails {{ id: details; width: parent.width; shellRoot: sh; useExternalPanelBackground: true }}
    }}
}}
'''

def read(name):
    return open(os.path.join(quickshell, name), encoding="utf-8").read().split("\n")

def block(lines, start):
    """Balanced {...} block starting at 1-based line `start` (the line with the opening brace)."""
    depth, out = 0, []
    for l in lines[start - 1:]:
        out.append(l)
        depth += l.count("{") - l.count("}")
        if depth == 0 and "{" in "".join(out):
            break
    return "\n".join(out)

def find_line(lines, needle, after=0):
    for i, l in enumerate(lines[after:], after + 1):
        if needle in l:
            return i
    raise SystemExit(f"not found: {needle}")

SHELL = read("shell.qml")
def shell_fn(name):
    return block(SHELL, find_line(SHELL, f"function {name}("))

def net_shell(m):
    n = m["net"]
    fns = "\n".join(shell_fn(f) for f in ["isWifiInterfaceName", "isWiredInterfaceName", "networkTypeForInterface", "fileUrl", "wifiSignalBucket", "fluentWifiIconSource", "wifiTrayIconSource", "wiredTrayIconSource", "networkTrayIconSource", "humanRate", "wifiListIconSource", "wifiSignalGlyph", "wifiTrayGlyph", "networkTrayGlyph"])
    home = os.path.expanduser("~")
    return f'''QtObject {{
        id: sh
        property bool darkMode: {q(m["dark"])}
        readonly property color primaryText: darkMode ? "#cdd6f4" : "#2b2b2c"
        readonly property color launchColor: darkMode ? "#89b4fa" : "#407cdd"
        readonly property color criticalColor: "#e92d4d"
        readonly property color usageLowColor: darkMode ? "#7ad48b" : "#2f9e44"
        readonly property color usageMediumColor: darkMode ? "#f2d36b" : "#c99700"
        readonly property color moduleBackground: withAlpha(darkMode ? "#1e1e2e" : "#e7e7ec", 0.8)
        readonly property string baseFont: "JetBrainsMono Nerd Font"
        readonly property string iconFont: "JetBrainsMono Nerd Font"
        readonly property string configDir: "{os.path.dirname(quickshell)}"
        property string fluentLightIconDir: "{home}/.local/share/icons/Fluent-light"
        property string fluentDarkIconDir: "{home}/.local/share/icons/Fluent-dark"
        property string fluentBaseIconDir: "{home}/.local/share/icons/Fluent"
        property string defaultInterface: {q(n["defaultInterface"])}
        readonly property bool networkConnected: defaultInterface.length > 0
        readonly property string activeNetworkType: networkTypeForInterface(defaultInterface)
        readonly property bool wifiConnectionActive: activeNetworkType === "wifi"
        readonly property bool wiredConnectionActive: activeNetworkType === "wired"
        readonly property bool otherConnectionActive: activeNetworkType === "other"
        property bool wifiDevicePresent: {q(n["wifiDevicePresent"])}
        property bool wifiCapabilityDetected: {q(n["wifiCapabilityDetected"])}
        property bool wifiRadioEnabled: {q(m["wifiRadioEnabled"])}
        property bool wifiHardwareEnabled: {q(n["wifiHardwareEnabled"])}
        property bool wifiConnected: {q(m["wifiConnected"])}
        property real wifiSignalStrength: {q(n["wifiSignalStrength"])}
        property string wifiSsid: {q(m["wifiSsid"])}
        property bool wifiSecure: {q(n["wifiSecure"])}
        property string wifiInterface: {q(n["wifiInterface"])}
        property var wifiNetworks: {q(n["networks"])}
        property bool wifiActionBusy: {q(n["wifiActionBusy"])}
        property string wifiActionMessage: {q(n["wifiActionMessage"])}
        property int openWifiPanelCount: 0
        function withAlpha(colorString, alpha) {{ const color = Qt.color(colorString); return Qt.rgba(color.r, color.g, color.b, alpha); }}
        function refreshWifiStatus() {{}}
        function wifiDisconnect() {{}}
        function wifiConnect(a, b, c) {{}}
        function wifiSetRadio(v) {{}}
        function wifiRescan() {{}}
        function openWifiManager() {{}}
{fns}
    }}'''

def wifi_page(m):
    W = read("WifiFallback.qml")
    # root properties/functions (WifiFallback.qml:8-262) minus the WifiIndicator assignments
    head = [l for l in W[7:find_line(W, "onLeftClicked: togglePopup()") - 1] if not l.strip().startswith(("available:", "iconSource:", "fallbackLabel:"))]
    head = [l.replace("property var shellRoot: null", "property var shellRoot: sh").replace("property bool popupVisible: false", "property bool popupVisible: true") for l in head]
    column = block(W, find_line(W, "id: panelColumn") - 1)
    return f'''import QtQuick
import ".."
import "{os.path.relpath(quickshell, scenes)}"

Item {{
    id: root
    width: 384; height: panelColumn.implicitHeight + 20
{chr(10).join(head)}
    {net_shell(m)}
    QtObject {{ id: popup; property bool visible: true; property bool animatingClose: false; property bool openAnimationPending: false }}
    Timer {{ id: popupOpenTimer }}
    AnimatedGlassPanel {{
        width: parent.width
        fullPanelHeight: parent.height
        fillColor: sh.withAlpha(sh.darkMode ? "#101214" : "#ffffff", sh.darkMode ? 0.42 : 0.28)
        strokeColor: sh.withAlpha(sh.primaryText, sh.darkMode ? 0.14 : 0.10)
        shadowColor: "transparent"
        devicePixelRatio: 2
        revealHeight: fullPanelHeight
        contentOpacity: 1
        contentOffset: 0
    }}
    Item {{
        id: popupChrome
        anchors.fill: parent
{column}
    }}
}}
'''

def stats_page(m):
    S = read("SystemStatsPopup.qml")
    props = [l for l in S[find_line(S, "id: popupRoot"):find_line(S, "function openFor") - 1] if "property var sourceItem" not in l and "property var parentWindow" not in l]
    props = [l.replace("property var shellRoot: null", "property var shellRoot: sh").replace("property bool popupRequested: false", "property bool popupRequested: true") for l in props]
    col = block(S, find_line(S, "sourceComponent: Column"))
    col = col.replace("sourceComponent: Column {", "Column {", 1).replace("width: popupContent.width", "width: 516")
    st = m["stats"]
    return f'''import QtQuick
import ".."
import "{os.path.relpath(quickshell, scenes)}"

Item {{
    id: popupRoot
    width: 548; height: statsColumn.implicitHeight + 32
{chr(10).join(props)}
    {net_shell(m).replace("id: sh", "id: sh\n        property real cpuUsage: " + q(st["cpuUsage"]) + "\n        property real memoryUsage: " + q(st["memoryUsage"]) + "\n        property var cpuCoreUsages: " + q(st["cpuCoreUsages"]) + "\n        property real networkRxRate: " + q(st["networkRxRate"]) + "\n        property real networkTxRate: " + q(st["networkTxRate"]) + "\n        property var cpuHistory: " + q(st["cpuHistory"]) + "\n        property var memoryHistory: " + q(st["memoryHistory"]) + "\n        property var networkHistory: " + q(st["networkHistory"]) + "\n        readonly property color systemChartAccent: darkMode ? \"#d7a26a\" : \"#b9782f\"\n        function usageSeverityColor(value) {{ const n = Number(value); if (!isFinite(n)) return primaryText; if (n >= 90) return criticalColor; if (n >= 60) return usageMediumColor; return usageLowColor; }}").replace('property string defaultInterface: ' + q(m["net"]["defaultInterface"]), 'property string defaultInterface: ' + q(st["defaultInterface"]))}
    AnimatedGlassPanel {{
        width: parent.width
        fullPanelHeight: parent.height
        fillColor: popupRoot.glassFill
        strokeColor: popupRoot.glassStroke
        shadowColor: popupRoot.panelShadowColor
        devicePixelRatio: 2
        revealHeight: fullPanelHeight
        contentOpacity: 1
        contentOffset: 0
    }}
    Item {{
        id: popupContent
        x: 16; y: 16; width: 516; height: parent.height - 32
        {col.replace("Column {", "Column {\n            id: statsColumn", 1)}
    }}
}}
'''

def calendar_page(m):
    c = m["calendar"]
    y, mo, d = (int(x) for x in (c["weekStart"] or m["wall"][:10]).split("-"))
    wy, wmo, wd = m["wall"][:10].split("-")
    hh, mi, ss = m["wall"][11:19].split(":")
    return f'''import QtQuick
import ".."
import "{os.path.relpath(quickshell, scenes)}"

Item {{
    width: 520; height: 500
    ShellMock {{ id: sh; darkMode: {q(m["dark"])}; now: new Date({wy}, {int(wmo) - 1}, {int(wd)}, {int(hh)}, {int(mi)}, {int(ss)}) }}
    QtObject {{
        id: bridge
        property var weekStart: sh.now
        Component.onCompleted: Qt.callLater(() => bridge.weekStart = new Date({y}, {mo - 1}, {d}))
        property bool connected: {q(c["connected"])}
        property string lastError: {q(c["lastError"])}
        property var calendars: {q(c["calendars"])}
        property var events: {q(c["events"])}
        function previousWeek() {{}}
        function nextWeek() {{}}
        function openEvent(e) {{}}
        function createEvent() {{}}
    }}
    WeekCalendarPanel {{ anchors.fill: parent; shellRoot: sh; bridge: bridge; embeddedSurface: true }}
}}
'''

def quick_page(m, volume):
    return f'''import QtQuick
import ".."
import "{os.path.relpath(quickshell, scenes)}"

Item {{
    width: 278; height: 62
    {shell(m)}
    ControlPanelSlider {{
        anchors.fill: parent
        shellRoot: sh
        iconLeftInset: 24
        icon: {"sh.volumeIcon" if volume else q(chr(0xF00DF))}
        iconClickable: {q(volume)}
        value: {"sh.audioVolumePercent" if volume else "sh.brightnessPercent"}
    }}
}}
'''

def startup_page(m):
    S = read("StartupConfirm.qml")
    card = block(S, find_line(S, "id: card") - 1)
    comp = block(S, find_line(S, "component ActionButton"))
    return f'''import QtQuick
import QtQuick.Controls.Basic

Item {{
    id: root
    width: 360; height: card.implicitHeight
    property bool keyboardReady: true
    readonly property bool dark: {q(m["dark"])}
    readonly property color foreground: dark ? "#cdd6f4" : "#2b2b2c"
    readonly property color accent: dark ? "#89b4fa" : "#1e66f5"
    function finish(launch) {{}}
{card}
{comp}
}}
'''

for name, f in fixtures.items():
    page = f["page"]
    m = f["model"]
    src = {
        "main": lambda: main_page(m),
        "bluetooth": lambda: bt_page(m),
        "wifi": lambda: wifi_page(m),
        "stats": lambda: stats_page(m),
        "calendar": lambda: calendar_page(m),
        "quick-brightness": lambda: quick_page(m, False),
        "quick-volume": lambda: quick_page(m, True),
        "startup": lambda: startup_page(m),
    }[page]()
    open(os.path.join(scenes, name + ".qml"), "w").write(src)
print(len(fixtures), "scenes")
