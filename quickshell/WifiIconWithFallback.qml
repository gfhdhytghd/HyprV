import QtQuick
import Quickshell.Widgets

Item {
    id: root

    property var shellRoot: null
    property string iconSource: ""
    property string fallbackLabel: "󰤮"
    property int iconSize: 24
    property int fallbackPixelSize: Math.max(13, iconSize - 2)
    property color fallbackColor: shellRoot ? shellRoot.primaryText : "white"

    implicitWidth: iconSize
    implicitHeight: iconSize
    width: implicitWidth
    height: implicitHeight

    IconImage {
        id: iconImage

        anchors.centerIn: parent
        width: root.iconSize
        height: root.iconSize
        source: root.iconSource
        asynchronous: true
        smooth: true
        mipmap: true
        visible: status === Image.Ready
    }

    Text {
        anchors.centerIn: parent
        visible: !iconImage.visible
        text: root.fallbackLabel
        color: root.fallbackColor
        font.family: root.shellRoot ? root.shellRoot.iconFont : "NotoSansMono Nerd Font"
        font.pixelSize: root.fallbackPixelSize
        font.weight: Font.Bold
        renderType: Text.NativeRendering
    }
}
