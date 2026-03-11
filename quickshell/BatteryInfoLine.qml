import QtQuick

Item {
    id: infoLine

    property var shellRoot: null
    property string title: ""
    property string value: ""
    property color titleColor: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.72 : 0.68) : "#b0b0b0"
    property color valueColor: shellRoot ? shellRoot.primaryText : "white"
    property real titleWidth: 118

    width: parent ? parent.width : implicitWidth
    implicitWidth: 296
    implicitHeight: Math.max(titleText.implicitHeight, valueText.implicitHeight)

    Text {
        id: titleText

        anchors.left: parent.left
        anchors.top: parent.top
        width: infoLine.titleWidth
        text: infoLine.title
        color: infoLine.titleColor
        font.family: shellRoot ? shellRoot.baseFont : ""
        font.pixelSize: 13
        font.weight: Font.DemiBold
        renderType: Text.NativeRendering
        wrapMode: Text.WordWrap
    }

    Text {
        id: valueText

        anchors.top: parent.top
        anchors.left: titleText.right
        anchors.leftMargin: 12
        anchors.right: parent.right
        text: infoLine.value
        color: infoLine.valueColor
        font.family: shellRoot ? shellRoot.baseFont : ""
        font.pixelSize: 13
        font.weight: Font.Bold
        horizontalAlignment: Text.AlignRight
        renderType: Text.NativeRendering
        wrapMode: Text.WordWrap
    }
}
