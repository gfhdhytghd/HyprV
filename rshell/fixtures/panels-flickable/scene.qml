import QtQuick
Item {
    id: root
    width: 520; height: 500
    property string mode: "wifi"
    property int clicks: 0
    property alias flick: loader.item
    Loader {
        id: loader
        sourceComponent: root.mode.startsWith("calendar") ? calendar : list
    }
    Component {
        id: list
        Flickable {
            id: f
            width: 364
            property int rows: 10
            property real extra: 0
            height: root.mode === "wifi" ? Math.min(contentHeight, Math.min(5, rows) * 59 + Math.max(0, Math.min(5, rows)-1)*10 + extra) : Math.min(contentHeight, 420)
            contentHeight: column.height
            clip: true
            interactive: contentHeight > height
            boundsBehavior: Flickable.StopAtBounds
            flickableDirection: root.mode === "wifi" ? Flickable.VerticalFlick : Flickable.AutoFlickDirection
            Column {
                id: column
                width: f.width; spacing: 10
                Repeater {
                    model: f.rows
                    Rectangle {
                        width: f.width; height: (root.mode === "wifi" ? 59 : 58) + (index === 0 ? f.extra : 0)
                        MouseArea { anchors.fill: parent; onClicked: root.clicks++ }
                    }
                }
            }
            MouseArea {
                anchors.fill: parent; acceptedButtons: Qt.NoButton
                enabled: root.mode === "wifi"
                onWheel: function(wheel) {
                    if (!f.interactive) return;
                    const step = wheel.pixelDelta.y !== 0 ? wheel.pixelDelta.y : wheel.angleDelta.y / 2;
                    f.contentY = Math.max(0, Math.min(f.contentHeight-f.height, f.contentY-step));
                    wheel.accepted = true;
                }
            }
        }
    }
    Component {
        id: calendar
        Flickable {
            id: f
            width: 450; height: 360
            contentWidth: 1350; contentHeight: 1056
            contentX: 450; contentY: 308
            clip: true; boundsBehavior: Flickable.StopAtBounds
            flickDeceleration: 2600
            flickableDirection: Flickable.HorizontalAndVerticalFlick
            pressDelay: 80
            property bool ready: false
            property bool rebasing: false
            Component.onCompleted: ready = true
            onContentXChanged: {
                if (root.mode !== "calendar-rebase" || !ready || rebasing || (contentX >= 3*450/7 && contentX <= 11*450/7)) return;
                rebasing = true;
                contentX += contentX < 3*450/7 ? 450 : -450;
                rebasing = false;
                Qt.callLater(function() { f.contentY = 308; });
            }
            WheelHandler {
                target: null
                acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                onWheel: e => {
                    const dx = e.pixelDelta.x !== 0 ? e.pixelDelta.x : e.angleDelta.x;
                    const dy = e.pixelDelta.y !== 0 ? e.pixelDelta.y : e.angleDelta.y;
                    if (dx === 0 && dy === 0) { e.accepted = false; return; }
                    if (Math.abs(dx) > Math.abs(dy))
                        f.contentX = Math.max(0, Math.min(900, f.contentX-(e.pixelDelta.x !== 0 ? e.pixelDelta.x : dx/120*450/7)));
                    else
                        f.contentY = Math.max(0, Math.min(696, f.contentY-(e.pixelDelta.y !== 0 ? e.pixelDelta.y : dy/120*132)));
                    e.accepted = true;
                }
            }
            MouseArea { width: 1350; height: 1056; onClicked: root.clicks++ }
        }
    }
}
