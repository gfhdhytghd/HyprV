import QtQuick
import ".."
import "../../../../../quickshell"

Item {
    width: 278; height: 62
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
    ControlPanelSlider {
        anchors.fill: parent
        shellRoot: sh
        iconLeftInset: 24
        icon: "󰃟"
        iconClickable: false
        value: sh.brightnessPercent
    }
}
