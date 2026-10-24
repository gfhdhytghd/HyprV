import QtQuick
Item {
    id: root
    width: 300; height: 420
    property bool listMode: false
    property int clicks: 0
    property alias flick: loader.item
    Loader {
        id: loader
        width: 282; height: root.listMode ? 386 : 402
        sourceComponent: root.listMode ? list : menu
    }
    Component {
        id: menu
        Flickable {
            width: 282; height: 402; clip: true
            contentWidth: width; contentHeight: rows.height
            Column {
                id: rows
                width: 282; spacing: 1
                Repeater {
                    model: 30
                    Rectangle {
                        width: 282; height: 34
                        MouseArea { anchors.fill: parent; onClicked: root.clicks++ }
                    }
                }
            }
        }
    }
    Component {
        id: list
        ListView {
            width: 282; height: 386; clip: true
            spacing: 8; model: 30
            boundsBehavior: Flickable.StopAtBounds
            interactive: contentHeight > height
            delegate: Rectangle {
                width: 282; height: 42
                MouseArea { anchors.fill: parent; onClicked: root.clicks++ }
            }
        }
    }
}
