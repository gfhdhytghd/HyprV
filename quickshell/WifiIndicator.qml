import QtQuick

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

    WifiIconWithFallback {
        anchors.centerIn: parent
        shellRoot: root.shellRoot
        iconSize: 24
        iconSource: root.iconSource
        fallbackLabel: root.fallbackLabel
        fallbackPixelSize: 24
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
