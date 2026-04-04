import QtQuick
import Quickshell.Widgets

Item {
    id: trayButton

    property var shellRoot: null
    property var trayItem: null
    property var parentWindow: null
    property string iconSource: shellRoot ? shellRoot.trayIconSource(trayItem) : ""

    implicitWidth: 18
    implicitHeight: 38

    IconImage {
        id: trayIcon

        anchors.verticalCenter: parent.verticalCenter
        width: 18
        height: 18
        source: trayButton.iconSource
        asynchronous: true
        smooth: true
        mipmap: true
        visible: status === Image.Ready
    }

    Text {
        anchors.centerIn: parent
        visible: !trayIcon.visible
        text: {
            const id = trayButton.trayItem && trayButton.trayItem.id ? trayButton.trayItem.id : "";
            return id ? id.charAt(0).toUpperCase() : "?";
        }
        color: shellRoot ? shellRoot.primaryText : "white"
        font.family: shellRoot ? shellRoot.baseFont : ""
        font.pixelSize: 10
    }

    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        cursorShape: Qt.PointingHandCursor

        onClicked: function(mouse) {
            if (!trayButton.trayItem || !shellRoot) {
                return;
            }
            if (mouse.button === Qt.LeftButton) {
                if (!trayButton.trayItem.onlyMenu) {
                    trayButton.trayItem.activate();
                } else if (trayButton.trayItem.hasMenu) {
                    shellRoot.openTrayMenu(trayButton.trayItem, trayButton, trayButton.parentWindow);
                }
            } else if (mouse.button === Qt.RightButton) {
                if (trayButton.trayItem.hasMenu) {
                    shellRoot.openTrayMenu(trayButton.trayItem, trayButton, trayButton.parentWindow);
                } else if (typeof trayButton.trayItem.secondaryActivate === "function") {
                    trayButton.trayItem.secondaryActivate();
                }
            }
        }

        onWheel: function(wheel) {
            if (!trayButton.trayItem) {
                return;
            }
            const delta = wheel.angleDelta.y > 0 ? 1 : -1;
            trayButton.trayItem.scroll(delta, false);
        }
    }
}
