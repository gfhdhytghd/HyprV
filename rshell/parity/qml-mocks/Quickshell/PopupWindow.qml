import QtQuick
import ParityHost

// xdg-popup mock (laptop bar tooltips). Placed at anchor.item's scene position
// plus anchor.rect, composited above every layer-shell window (scene slot z).
QtObject {
    id: win

    readonly property bool isPopup: true
    property bool visible: false
    property color color: "white"
    property real implicitWidth: 0
    property real implicitHeight: 0
    property bool grabFocus: false
    property QtObject mask: null
    property var screen: null
    readonly property real devicePixelRatio: Harness.scale || 1
    readonly property PopupAnchor anchor: PopupAnchor {}
    readonly property SurfaceFormat surfaceFormat: SurfaceFormat {}

    readonly property real width: implicitWidth
    readonly property real height: implicitHeight
    // Re-evaluated on every anchor/size change and when the popup is shown.
    readonly property point _origin: {
        const item = anchor.item;
        void visible; void anchor.rect; void implicitWidth;
        if (!item)
            return Qt.point(anchor.rect.x, anchor.rect.y);
        const p = item.mapToItem(null, anchor.rect.x, anchor.rect.y);
        return Qt.point(p.x, p.y);
    }
    readonly property real x: _origin.x
    readonly property real y: _origin.y

    // Under the scene from creation (ParityState.earlyParent), so items that load during
    // construction (Image sourceSize x DPR) see the render DPR like a real QQuickWindow.
    readonly property Item contentItem: Item {
        id: content
        parent: ParityState.earlyParent
        width: win.width
        height: win.height
        Rectangle {
            anchors.fill: parent
            z: -1
            color: win.color
        }
    }
    default property alias data: content.data

    function itemPosition(item) { return item.mapToItem(content, 0, 0); }
    function itemRect(item) { const p = item.mapToItem(content, 0, 0); return Qt.rect(p.x, p.y, item.width, item.height); }
    function mapFromItem(item, x, y) { return item.mapToItem(content, x, y); }

    Component.onCompleted: ParityState.registerWindow(win)
    Component.onDestruction: ParityState.unregisterWindow(win)
}
