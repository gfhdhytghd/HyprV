import QtQuick
import ".."
import "../../../../../quickshell"

Item {
    id: root
    width: 384; height: details.implicitHeight
    ShellMock {
        id: sh
        darkMode: true
        audioAvailable: true
        audioMuted: false
        audioVolumePercent: 35
        bluetoothActionBusy: false
        bluetoothActionMessage: "Connected to WH-1000XM5"
        bluetoothDevices: [{"address": "AC:80:0A:12:34:56", "blocked": false, "connected": true, "displayName": "WH-1000XM5", "hasName": true, "icon": "audio-headset", "name": "WH-1000XM5", "paired": true, "rssi": null, "trusted": true}, {"address": "F4:73:35:AA:BB:CC", "blocked": false, "connected": false, "displayName": "MX Master 3S", "hasName": true, "icon": "input-mouse", "name": "MX Master 3S", "paired": true, "rssi": -61, "trusted": false}, {"address": "5C:E9:1E:01:02:03", "blocked": false, "connected": false, "displayName": "5C:E9:1E:01:02:03", "hasName": false, "icon": "", "name": "", "paired": false, "rssi": -80, "trusted": false}]
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
    AnimatedGlassPanel {
        width: parent.width
        fullPanelHeight: parent.height
        fillColor: sh.withAlpha(sh.darkMode ? "#101214" : "#ffffff", sh.darkMode ? 0.42 : 0.28)
        strokeColor: sh.withAlpha(sh.primaryText, sh.darkMode ? 0.14 : 0.10)
        shadowColor: "transparent"
        devicePixelRatio: 2
        revealHeight: fullPanelHeight
        contentOpacity: 1
        contentOffset: 0
    }
    Column {
        width: 384
        spacing: 10
        ControlPanelBluetoothDetails { id: details; width: parent.width; shellRoot: sh; useExternalPanelBackground: true }
    }
}
