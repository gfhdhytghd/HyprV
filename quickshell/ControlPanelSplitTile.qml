import QtQuick

Item {
    id: tile

    property var shellRoot: null
    property string icon: ""
    property string title: ""
    property string subtitle: ""
    property bool active: false
    property bool destructive: false
    property bool leftEnabled: true
    property bool rightEnabled: true
    property bool expandIndicatorVisible: false
    property bool expanded: false
    property real iconAreaWidth: height

    signal leftClicked()
    signal rightClicked()

    implicitHeight: 72

    readonly property color accentColor: destructive
        ? (shellRoot ? shellRoot.criticalColor : "#d9485f")
        : (shellRoot ? shellRoot.launchColor : "#89b4fa")
    readonly property color textColor: shellRoot ? shellRoot.primaryText : "#cdd6f4"
    readonly property color mutedTextColor: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, 0.68) : "#b0b0b0"
    readonly property color baseFill: shellRoot ? shellRoot.withAlpha(shellRoot.darkMode ? "#ffffff" : "#ffffff", shellRoot.darkMode ? 0.07 : 0.22) : "#2a2a2a"
    readonly property color baseStroke: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.12 : 0.08) : "#454545"
    readonly property color activeFill: shellRoot ? shellRoot.withAlpha(accentColor, shellRoot.darkMode ? 0.28 : 0.22) : Qt.rgba(0.54, 0.71, 0.98, 0.22)
    readonly property color activeStroke: shellRoot ? shellRoot.withAlpha(accentColor, shellRoot.darkMode ? 0.42 : 0.28) : Qt.rgba(0.54, 0.71, 0.98, 0.32)
    readonly property color iconWellFill: shellRoot
        ? shellRoot.withAlpha(shellRoot.darkMode ? "#ffffff" : "#ffffff", shellRoot.darkMode ? (active ? 0.14 : 0.11) : (active ? 0.26 : 0.32))
        : "#303030"
    readonly property real frameRadius: 19
    readonly property real inset: Math.max(6, Math.round(height * 0.14))
    readonly property real iconWellWidth: Math.max(28, iconAreaWidth - inset * 2)
    readonly property real iconWellHeight: Math.max(28, height - inset * 2)
    readonly property real iconWellRadius: 11
    readonly property real iconPixelSize: Math.max(16, Math.min(20, Math.min(iconWellWidth, iconWellHeight) * 0.48))
    readonly property real iconTextGap: 9
    readonly property real contentStartX: inset + iconWellWidth + iconTextGap

    Rectangle {
        id: frame

        anchors.fill: parent
        radius: tile.frameRadius
        color: tile.active ? tile.activeFill : tile.baseFill
        border.width: 1
        border.color: tile.active ? tile.activeStroke : tile.baseStroke
        antialiasing: true
    }

    Rectangle {
        id: iconWell

        x: tile.inset
        y: tile.inset
        width: tile.iconWellWidth
        height: tile.iconWellHeight
        radius: tile.iconWellRadius
        color: iconArea.pressed
            ? Qt.darker(tile.iconWellFill, 1.08)
            : (iconArea.containsMouse ? Qt.lighter(tile.iconWellFill, 1.04) : tile.iconWellFill)
        antialiasing: true
    }

    Text {
        anchors.centerIn: iconWell
        text: tile.icon
        color: tile.active ? "#fff7ef" : tile.textColor
        font.family: tile.shellRoot ? tile.shellRoot.iconFont : "JetBrainsMono Nerd Font"
        font.pixelSize: tile.iconPixelSize
        font.weight: Font.Bold
        renderType: Text.NativeRendering
    }

    Item {
        anchors.left: parent.left
        anchors.leftMargin: tile.contentStartX
        anchors.right: parent.right
        anchors.rightMargin: 10
        anchors.verticalCenter: parent.verticalCenter
        height: contentColumn.implicitHeight

        Column {
            id: contentColumn

            anchors.left: parent.left
            anchors.right: chevron.visible ? chevron.left : parent.right
            anchors.rightMargin: chevron.visible ? 6 : 0
            anchors.verticalCenter: parent.verticalCenter
            spacing: subtitle.length > 0 ? 2 : 0

            Text {
                text: tile.title
                color: tile.active ? "#ffffff" : tile.textColor
                font.family: tile.shellRoot ? tile.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                font.pixelSize: 11
                font.weight: Font.Bold
                renderType: Text.NativeRendering
                elide: Text.ElideRight
            }

            Text {
                visible: tile.subtitle.length > 0
                text: tile.subtitle
                color: tile.active ? Qt.rgba(1, 1, 1, 0.82) : tile.mutedTextColor
                font.family: tile.shellRoot ? tile.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                font.pixelSize: 9
                font.weight: Font.Medium
                renderType: Text.NativeRendering
                elide: Text.ElideRight
            }
        }

        Text {
            id: chevron

            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            visible: tile.expandIndicatorVisible
            text: tile.expanded ? "" : ""
            color: tile.active ? "#ffffff" : tile.mutedTextColor
            font.family: tile.shellRoot ? tile.shellRoot.iconFont : "JetBrainsMono Nerd Font"
            font.pixelSize: 11
            font.weight: Font.Bold
            renderType: Text.NativeRendering
        }
    }

    MouseArea {
        id: iconArea

        x: 0
        y: 0
        width: tile.contentStartX
        height: parent.height
        enabled: tile.leftEnabled
        hoverEnabled: true
        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: tile.leftClicked()
    }

    MouseArea {
        id: contentArea

        x: tile.contentStartX
        y: 0
        width: parent.width - tile.contentStartX
        height: parent.height
        enabled: tile.rightEnabled
        hoverEnabled: true
        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: tile.rightClicked()
    }
}
