import QtQuick
import Quickshell.Widgets

Item {
    id: root

    property var shellRoot: null
    property string iconSource: ""
    property string fallbackLabel: "󰤮"
    property int iconSize: 24
    readonly property bool usesSymbolicSource: (iconSource || "").indexOf("/symbolic/") >= 0
    readonly property real contentScale: usesSymbolicSource ? (16 / 24) : 1
    readonly property int effectiveIconSize: Math.max(16, Math.round(iconSize * contentScale))
    readonly property int effectiveVerticalOffset: usesSymbolicSource ? -2 : 0
    property int fallbackPixelSize: Math.max(13, effectiveIconSize - 2)
    property color fallbackColor: shellRoot ? shellRoot.primaryText : "white"

    implicitWidth: iconSize
    implicitHeight: iconSize
    width: implicitWidth
    height: implicitHeight

    IconImage {
        id: iconImage

        anchors.centerIn: parent
        anchors.verticalCenterOffset: root.effectiveVerticalOffset
        width: root.effectiveIconSize
        height: root.effectiveIconSize
        source: root.iconSource
        asynchronous: true
        smooth: true
        mipmap: true
        visible: status === Image.Ready
    }

    Text {
        anchors.centerIn: parent
        anchors.verticalCenterOffset: root.effectiveVerticalOffset
        visible: !iconImage.visible
        text: root.fallbackLabel
        color: root.fallbackColor
        font.family: root.shellRoot ? root.shellRoot.iconFont : "JetBrainsMono Nerd Font"
        font.pixelSize: root.fallbackPixelSize
        font.weight: Font.Bold
        renderType: Text.NativeRendering
    }
}
