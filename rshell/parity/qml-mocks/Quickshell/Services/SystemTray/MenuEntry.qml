import QtQuick

// DBusMenu entry mock (Quickshell's QsMenuEntry is a QObject). `entries` holds
// child MenuEntry objects; QsMenuOpener exposes them as `children`.
QtObject {
    property string text: ""
    property string icon: ""
    property bool isSeparator: false
    property bool enabled: true
    property int buttonType: 0        // 0 none, 1 checkbox, 2 radio
    property int checkState: Qt.Unchecked
    property bool hasChildren: entries.length > 0
    property var entries: []
    readonly property QtObject menu: null
    function activate() { console.log("parity: menu entry activate", text); }
    function updateLayout() {}
    function sendOpened() {}
}
