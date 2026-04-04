import QtQuick

Rectangle {
    id: badge

    property var shellRoot: null
    property bool active: false
    property int cornerRadius: 10

    visible: active
    implicitWidth: 22
    implicitHeight: 20
    radius: cornerRadius
    color: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.08 : 0.06) : "#2a2a2a"
    border.width: 1
    border.color: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, 0.08) : "#444444"

    Text {
        anchors.centerIn: parent
        text: ""
        color: badge.shellRoot ? badge.shellRoot.withAlpha(badge.shellRoot.primaryText, 0.82) : "white"
        font.family: badge.shellRoot ? badge.shellRoot.iconFont : "JetBrainsMono Nerd Font"
        font.pixelSize: 11
        font.weight: Font.Bold
        renderType: Text.NativeRendering
    }
}
