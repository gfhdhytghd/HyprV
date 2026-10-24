import QtQuick

// Verbatim behaviour of Quickshell's own Widgets/IconImage.qml (pure QML in the
// installed module): an Item wrapping an Image with sourceSize = actualSize.
// Note: `smooth` is NOT aliased (setting it affects the Item only), as upstream.
Item {
    id: root

    property alias source: image.source
    property alias asynchronous: image.asynchronous
    property alias status: image.status
    property alias mipmap: image.mipmap
    property alias backer: image
    property real implicitSize: 0
    readonly property real actualSize: Math.min(root.width, root.height)

    implicitWidth: root.implicitSize
    implicitHeight: root.implicitSize

    Image {
        id: image
        anchors.fill: parent
        fillMode: Image.PreserveAspectFit

        sourceSize.width: root.actualSize
        sourceSize.height: root.actualSize
    }
}
