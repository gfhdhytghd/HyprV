import QtQuick

// Theme tokens and the shell properties the panel components read
// (shell.qml:212-240, 369-384, 1670). Values are set per fixture by the scene.
QtObject {
    property bool darkMode: false
    property string configDir: ""
    readonly property real pillOpacity: 0.8
    readonly property color moduleBackground: withAlpha(darkMode ? "#1e1e2e" : "#e7e7ec", pillOpacity)
    readonly property color primaryText: darkMode ? "#cdd6f4" : "#2b2b2c"
    readonly property color launchColor: darkMode ? "#89b4fa" : "#407cdd"
    readonly property color batteryColor: darkMode ? "#a6e3a1" : "#1d7715"
    readonly property color microphoneColor: darkMode ? "#cba6f7" : "#ad6bfd"
    readonly property color criticalColor: "#e92d4d"
    readonly property color usageLowColor: darkMode ? "#7ad48b" : "#2f9e44"
    readonly property color usageMediumColor: darkMode ? "#f2d36b" : "#c99700"
    readonly property color brightnessColor: darkMode ? "#f3b35c" : "#d47b1f"
    readonly property color systemChartAccent: darkMode ? "#d7a26a" : "#b9782f"
    readonly property string baseFont: "JetBrainsMono Nerd Font"
    readonly property string iconFont: "JetBrainsMono Nerd Font"

    property bool wifiRadioEnabled: false
    property bool bluetoothPresent: false
    property bool bluetoothEnabled: false
    property bool bluetoothDiscovering: false
    property bool bluetoothPairable: false
    property bool bluetoothShowUnnamedDevices: true
    property bool bluetoothActionBusy: false
    property string bluetoothActionMessage: ""
    property var bluetoothDevices: []
    property bool screenRecording: false
    property bool preventSleepEnabled: false
    property bool mediaAvailable: false
    property bool mediaPlaying: false
    property string mediaTitle: ""
    property string mediaArtist: ""
    property string mediaPlayerName: ""
    property string mediaArtUrl: ""
    property bool dndEnabled: false
    property bool demoModeEnabled: false
    property string powerProfile: "balanced"
    property int brightnessPercent: 50
    property bool audioAvailable: false
    property bool audioMuted: false
    property int audioVolumePercent: 0
    property bool fanControlAvailable: false
    property int chassisFanPercent: 0
    property int pumpFanPercent: 0
    property int chassisFanRpm: 0
    property int pumpFanRpm: 0
    property var now: new Date(2026, 9, 23, 14, 30, 0)
    readonly property string volumeIcon: {
        const percent = audioVolumePercent;
        if (!audioAvailable) return "\u{f026}";
        if (audioMuted) return "\u{f466}";
        if (percent <= 20) return "\u{f026}";
        if (percent <= 50) return "\u{f027}";
        return "\u{f028}";
    }

    function withAlpha(colorString, alpha) {
        const color = Qt.color(colorString);
        return Qt.rgba(color.r, color.g, color.b, alpha);
    }
    function bluetoothSetPower(v) {}
    function bluetoothScan() {}
    function bluetoothConnect(a, p, n) {}
    function bluetoothDisconnect(a, n) {}
    function openBluetoothManager() {}
    function usageSeverityColor(value) {
        const number = Number(value);
        if (!isFinite(number)) return primaryText;
        if (number >= 90) return criticalColor;
        if (number >= 60) return usageMediumColor;
        return usageLowColor;
    }
}
