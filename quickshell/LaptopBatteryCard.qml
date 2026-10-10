import QtQuick

Rectangle {
    id: card
    property var shellRoot: null
    readonly property bool available: shellRoot ? shellRoot.batteryAvailable : false
    readonly property color textColor: shellRoot ? shellRoot.primaryText : "#cdd6f4"
    radius: 12
    color: shellRoot ? shellRoot.withAlpha(shellRoot.darkMode ? "#ffffff" : "#ffffff", shellRoot.darkMode ? 0.07 : 0.22) : "#2a2a2a"
    border.width: 1
    border.color: shellRoot ? shellRoot.withAlpha(textColor, 0.12) : "#454545"

    Column {
        anchors.centerIn: parent
        width: parent.width - 18
        spacing: 3
        Text {
            width: parent.width
            text: card.available ? card.shellRoot.batteryText : "电池不可用"
            color: card.available ? card.shellRoot.batteryDetailAccentColor : card.textColor
            font.family: card.shellRoot ? card.shellRoot.baseFont : "sans-serif"
            font.pixelSize: 13
            font.bold: true
            horizontalAlignment: Text.AlignHCenter
        }
        Text {
            width: parent.width
            text: card.available ? card.shellRoot.batteryStatusText : ""
            color: card.textColor
            font.pixelSize: 10
            horizontalAlignment: Text.AlignHCenter
            elide: Text.ElideRight
        }
        Text {
            width: parent.width
            text: card.available ? card.shellRoot.batteryPowerDetailText : ""
            color: card.textColor
            font.pixelSize: 10
            horizontalAlignment: Text.AlignHCenter
        }
    }
}
