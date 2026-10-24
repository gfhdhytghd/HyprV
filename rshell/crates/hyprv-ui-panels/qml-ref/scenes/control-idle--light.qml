import QtQuick
import ".."
import "../../../../../quickshell"

Item {
    id: popupRoot
    width: 278; height: 422
    readonly property var shellRoot: sh
    readonly property int moduleSize: 62
    readonly property int cardSpacing: 10
    readonly property int columnSpacing: 10
    readonly property int columnWidth: 134
    ShellMock {
        id: sh
        darkMode: false
        audioAvailable: true
        audioMuted: false
        audioVolumePercent: 35
        bluetoothActionBusy: false
        bluetoothActionMessage: ""
        bluetoothDevices: [{"address": "AC:80:0A:12:34:56", "blocked": false, "connected": true, "displayName": "WH-1000XM5", "hasName": true, "icon": "audio-headset", "name": "WH-1000XM5", "paired": true, "rssi": null, "trusted": true}]
        bluetoothDiscovering: false
        bluetoothEnabled: true
        bluetoothPairable: true
        bluetoothPresent: true
        bluetoothShowUnnamedDevices: true
        brightnessPercent: 50
        chassisFanPercent: 40
        demoModeEnabled: false
        dndEnabled: false
        fanControlAvailable: true
        mediaArtist: "M83"
        mediaAvailable: true
        mediaPlaying: true
        mediaTitle: "Midnight City"
        powerProfile: "balanced"
        preventSleepEnabled: false
        pumpFanPercent: 60
        screenRecording: false
        wifiRadioEnabled: true
        property bool wifiConnected: true
        property string wifiSsid: "HomeNet-5G"
    }
    QtObject { id: windowsVmController; property bool running: false; property bool busy: false; function toggleVm() {} }

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

    Column {
        width: 278
        spacing: popupRoot.cardSpacing
        Row {
            width: parent.width
            spacing: popupRoot.columnSpacing
            Column {
                width: popupRoot.columnWidth
                spacing: popupRoot.cardSpacing
                ControlPanelSplitTile { width: parent.width; height: popupRoot.moduleSize; shellRoot: sh; icon: active ? "\u{f0928}" : "\u{f092d}"; title: "WIFI"; subtitle: popupRoot.currentWifiSubtitle(); active: sh.wifiRadioEnabled }
                ControlPanelSplitTile { width: parent.width; height: popupRoot.moduleSize; shellRoot: sh; icon: active ? "\u{f00af}" : "\u{f00b2}"; title: popupRoot.currentBluetoothTitle(); subtitle: popupRoot.currentBluetoothSubtitle(); active: sh.bluetoothEnabled }
                Row {
                    width: parent.width
                    spacing: popupRoot.cardSpacing
                    WindowsVmControl { width: (parent.width - popupRoot.cardSpacing) / 2; height: popupRoot.moduleSize; shellRoot: sh; controller: windowsVmController }
                    // ViewflowTargetControl (= ControlPanelToggle with these bindings; its Process/poll omitted)
                    ControlPanelToggle { width: (parent.width - popupRoot.cardSpacing) / 2; height: popupRoot.moduleSize; shellRoot: sh; icon: "windows" === "macos" ? "\u{f179}" : "\u{f05b3}"; iconOnly: true; active: false; toggleEnabled: !false }
                }
                Row {
                    width: parent.width
                    spacing: popupRoot.cardSpacing
                    ControlPanelToggle { width: (parent.width - popupRoot.cardSpacing) / 2; height: popupRoot.moduleSize; shellRoot: sh; icon: "\u{f0ec3}"; iconOnly: true; label: "Screen\nrecord"; active: sh.screenRecording }
                    ControlPanelToggle { width: (parent.width - popupRoot.cardSpacing) / 2; height: popupRoot.moduleSize; shellRoot: sh; icon: active ? "\u{f0f4}" : "\u{f236}"; iconOnly: true; label: "Prevent\nsleep"; active: sh.preventSleepEnabled }
                }
            }
            Column {
                width: popupRoot.columnWidth
                spacing: popupRoot.cardSpacing
                ControlPanelMediaCard { width: parent.width; height: popupRoot.moduleSize * 2 + popupRoot.cardSpacing; shellRoot: sh; available: sh.mediaAvailable; playing: sh.mediaPlaying; title: sh.mediaTitle; subtitle: sh.mediaArtist; playerName: sh.mediaPlayerName; artUrl: sh.mediaArtUrl }
                Row {
                    width: parent.width
                    spacing: popupRoot.cardSpacing
                    ControlPanelToggle { width: (parent.width - popupRoot.cardSpacing) / 2; height: popupRoot.moduleSize; shellRoot: sh; iconSource: "file:///home/wilf/.config/HyprV/quickshell/assets/control-panel/light-dark.svg"; iconOnly: true; active: sh.darkMode }
                    ControlPanelToggle { width: (parent.width - popupRoot.cardSpacing) / 2; height: popupRoot.moduleSize; shellRoot: sh; icon: "\u{f009b}"; iconOnly: true; active: sh.dndEnabled }
                }
                Row {
                    width: parent.width
                    spacing: popupRoot.cardSpacing
                    ControlPanelToggle { width: (parent.width - popupRoot.cardSpacing) / 2; height: popupRoot.moduleSize; shellRoot: sh; icon: "\u{f0379}"; iconOnly: true; active: sh.demoModeEnabled }
                    ControlPanelToggle { width: (parent.width - popupRoot.cardSpacing) / 2; height: popupRoot.moduleSize; shellRoot: sh; icon: popupRoot.powerProfileIcon(sh.powerProfile); iconOnly: true; active: false }
                }
            }
        }
        Grid {
            width: parent.width
            columns: 2
            spacing: popupRoot.cardSpacing
            ControlPanelSlider { shellRoot: sh; width: popupRoot.columnWidth; height: popupRoot.moduleSize; icon: "\u{f00df}"; value: sh.brightnessPercent }
            ControlPanelSlider { shellRoot: sh; width: popupRoot.columnWidth; height: popupRoot.moduleSize; icon: sh.volumeIcon; iconClickable: true; value: sh.audioVolumePercent }
            ControlPanelSlider { shellRoot: sh; width: popupRoot.columnWidth; height: popupRoot.moduleSize; enabled: sh.fanControlAvailable; opacity: enabled ? 1 : 0.48; icon: "\u{f0210}"; value: sh.chassisFanPercent }
            ControlPanelSlider { shellRoot: sh; width: popupRoot.columnWidth; height: popupRoot.moduleSize; enabled: sh.fanControlAvailable; opacity: enabled ? 1 : 0.48; icon: "\u{f0210}"; value: sh.pumpFanPercent }
        }
    }
}
