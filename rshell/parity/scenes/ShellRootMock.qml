import QtQuick

// Theme tokens copied verbatim from quickshell/shell.qml:212-233 and :1670.
QtObject {
    property bool darkMode: false
    property string configDir: ""

    readonly property real pillOpacity: 0.8
    readonly property color moduleBackground: withAlpha(darkMode ? "#1e1e2e" : "#e7e7ec", pillOpacity)
    readonly property color primaryText: darkMode ? "#cdd6f4" : "#2b2b2c"
    readonly property color launchColor: darkMode ? "#89b4fa" : "#407cdd"
    readonly property string baseFont: "JetBrainsMono Nerd Font"
    readonly property string iconFont: "JetBrainsMono Nerd Font"

    function withAlpha(colorString, alpha) {
        const color = Qt.color(colorString);
        return Qt.rgba(color.r, color.g, color.b, alpha);
    }
}
