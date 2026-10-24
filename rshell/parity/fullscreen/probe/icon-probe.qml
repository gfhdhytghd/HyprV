import QtQuick

// R1 probe: how Qt Quick draws a tray icon pixmap. Each variant shows the raw pixmap the
// live Quickshell provider returns (no sourceSize, so no decode-time scaling) in an
// 18x18 IconImage-like Image at the live shell's fractional x (.1), over the bar pill.
Item {
    id: scene
    readonly property var canvas: Harness.fixture.canvas || ({})
    width: canvas.width || 200
    height: canvas.height || 60
    function applyProps(p) {}
    readonly property string src: (Harness.fixture.props || {}).src || ""
    Rectangle { x: 0; y: 0; width: scene.width; height: 58; color: Qt.rgba(231/255, 231/255, 236/255, 204/255) }
    Repeater {
        model: [{x: 22.1, mipmap: true, smooth: true}, {x: 72.1, mipmap: false, smooth: true}, {x: 122.1, mipmap: false, smooth: false}]
        Image {
            x: modelData.x; y: 20; width: 18; height: 18
            source: scene.src
            fillMode: Image.PreserveAspectFit
            mipmap: modelData.mipmap
            smooth: modelData.smooth
            asynchronous: false
        }
    }
}
