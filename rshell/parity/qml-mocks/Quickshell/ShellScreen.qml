import QtQuick

// Screen object (Quickshell's ShellScreen is a QObject; identity comparisons
// such as calendarScreen === modelData rely on that).
QtObject {
    property string name: ""
    property real x: 0
    property real y: 0
    property real width: 0
    property real height: 0
    property real devicePixelRatio: 1
    property string model: "Parity Display"
    property string serialNumber: ""
    property real physicalPixelDensity: 0
    property real logicalPixelDensity: 0
}
