import QtQuick
import Quickshell.Widgets

Item {
    id: root

    property var shellRoot: null
    property string iconSource: ""
    property string fallbackLabel: "󰖩"
    property bool available: true

    signal leftClicked()
    signal rightClicked()

    visible: available
    implicitWidth: available ? 30 : 0
    implicitHeight: 37

    Rectangle {
        anchors.fill: parent
        radius: 10
        color: mouseArea.containsMouse && root.shellRoot
        ? root.shellRoot.withAlpha(root.shellRoot.activeWorkspaceBackground, root.shellRoot.darkMode ? 0.18 : 0.22)
        : "transparent"
    }

    IconImage {
        id: trayIcon

        anchors.centerIn: parent
        width: 24
        height: 24
        source: root.iconSource
        asynchronous: true
        smooth: true
        mipmap: true
        visible: status === Image.Ready
    }

    Text {
        anchors.centerIn: parent
        visible: !trayIcon.visible
        text: root.fallbackLabel
        color: root.shellRoot ? root.shellRoot.primaryText : "white"
        font.family: root.shellRoot ? root.shellRoot.iconFont : "NotoSansMono Nerd Font"
        font.pixelSize: 24
        font.weight: Font.Bold
        renderType: Text.NativeRendering
    }

    MouseArea {
        id: mouseArea

        anchors.fill: parent
        enabled: root.available
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        cursorShape: Qt.PointingHandCursor

        onClicked: function(mouse) {
            if (mouse.button === Qt.LeftButton) {
                root.leftClicked();
            } else if (mouse.button === Qt.RightButton) {
                root.rightClicked();
            }
        }
    }
}
