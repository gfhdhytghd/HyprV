import QtQuick

QtObject {
    property var menu: null
    readonly property PopupAnchor anchor: PopupAnchor {}
    readonly property bool visible: false
    signal opened()
    signal closed()
    function open() { console.log("parity: QsMenuAnchor.open (native menu not rendered)"); }
    function close() {}
}
