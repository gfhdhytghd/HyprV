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
        bluetoothActionMessage: ""
        bluetoothDevices: []
        bluetoothDiscovering: true
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
