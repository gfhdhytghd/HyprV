import QtQuick

QtObject {
    property QtObject window: null
    property Item item: null
    property rect rect: Qt.rect(0, 0, 0, 0)
    property int edges: 0
    property int gravity: 0
    property int adjustment: 0
    property var margins: ({ left: 0, right: 0, top: 0, bottom: 0 })
}
