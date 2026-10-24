import QtQuick

QtObject {
    property string text: ""
    property bool waitForEnd: true
    readonly property string data: text
    signal streamFinished()
}
