import QtQuick
import ParityHost

// Layer-shell window mock. Children are parented to `contentItem` at creation
// (like Quickshell), and the scene composites every registered window into
// the output canvas at (x, y) using the layer from WlrLayershell.
QtObject {
    id: win

    property var screen: null
    property bool visible: true
    property color color: "white"
    property real implicitWidth: 0
    property real implicitHeight: 0
    property int exclusiveZone: 0
    property int exclusionMode: 0
    property bool focusable: false
    property bool aboveWindows: true
    readonly property real devicePixelRatio: Harness.scale || 1
    property QtObject mask: null
    readonly property PanelAnchors anchors: PanelAnchors {}
    readonly property PanelMargins margins: PanelMargins {}
    readonly property SurfaceFormat surfaceFormat: SurfaceFormat {}

    readonly property var effectiveScreen: screen || Quickshell.screens[0]
    readonly property real width: anchors.left && anchors.right
        ? effectiveScreen.width - margins.left - margins.right : implicitWidth
    readonly property real height: anchors.top && anchors.bottom
        ? effectiveScreen.height - margins.top - margins.bottom : implicitHeight
    readonly property real x: anchors.left ? margins.left
        : (anchors.right ? effectiveScreen.width - width - margins.right : Math.round((effectiveScreen.width - width) / 2))
    readonly property real y: anchors.top ? margins.top
        : (anchors.bottom ? effectiveScreen.height - height - margins.bottom : Math.round((effectiveScreen.height - height) / 2))

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

    // Quickshell window API used by components.
    function itemPosition(item) { return item.mapToItem(content, 0, 0); }
    function itemRect(item) { const p = item.mapToItem(content, 0, 0); return Qt.rect(p.x, p.y, item.width, item.height); }
    function mapFromItem(item, x, y) { return item.mapToItem(content, x, y); }

    Component.onCompleted: ParityState.registerWindow(win)
    Component.onDestruction: ParityState.unregisterWindow(win)
}
