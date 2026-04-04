import QtQuick

Rectangle {
    id: chip

    property var shellRoot: null
    property string label: ""
    property string iconLabel: ""
    property bool disabled: false
    property color fillColor: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, 0.08) : "#333333"
    property color foregroundColor: shellRoot ? shellRoot.primaryText : "white"
    property color strokeColor: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, 0.1) : "#444444"
    property int minimumWidth: 0
    property int iconPixelSize: 16
    property int cornerRadius: 10

    signal clicked()

    radius: cornerRadius
    height: 34
    implicitWidth: Math.max(minimumWidth, Math.max(chipLabel.contentWidth, chipIcon.contentWidth) + 24)
    color: disabled
    ? (shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, 0.06) : "#232323")
    : (chipArea.pressed
        ? Qt.darker(fillColor, 1.08)
        : (chipArea.containsMouse ? Qt.lighter(fillColor, 1.06) : fillColor))
    opacity: disabled ? 0.56 : 1
    border.width: 1
    border.color: disabled
    ? (shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, 0.06) : "#333333")
    : (chipArea.containsMouse
        ? (shellRoot ? shellRoot.withAlpha(foregroundColor, 0.22) : strokeColor)
        : strokeColor)
    antialiasing: true

    Text {
        id: chipLabel

        anchors.centerIn: parent
        text: chip.label
        visible: chip.iconLabel.length === 0
        color: chip.foregroundColor
        font.family: chip.shellRoot ? chip.shellRoot.baseFont : ""
        font.pixelSize: 13
        font.weight: Font.Bold
        renderType: Text.NativeRendering
    }

    Text {
        id: chipIcon

        anchors.centerIn: parent
        text: chip.iconLabel
        visible: chip.iconLabel.length > 0
        color: chip.foregroundColor
        font.family: chip.shellRoot ? chip.shellRoot.iconFont : "JetBrainsMono Nerd Font"
        font.pixelSize: chip.iconPixelSize
        font.weight: Font.Bold
        renderType: Text.NativeRendering
    }

    MouseArea {
        id: chipArea

        anchors.fill: parent
        enabled: !chip.disabled
        hoverEnabled: true
        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: chip.clicked()
    }
}
