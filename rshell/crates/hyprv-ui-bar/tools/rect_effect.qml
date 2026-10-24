// Diagnostic scene only: independent Qt RHI rectangle/effect isolation.
import QtQuick
import QtQuick.Effects
Item {
    id: root
    width: 428; height: 358
    property var p: Harness.fixture.props
    function applyProps(props) {}
    Item {
        x: 64; y: 64; width: 300; height: 230
        layer.enabled: root.p.effect
        layer.smooth: true
        layer.textureSize: Qt.size(Math.round(width*Harness.scale), Math.round(height*Harness.scale))
        layer.effect: MultiEffect {
            autoPaddingEnabled: true
            shadowEnabled: true
            blurEnabled: false
            shadowBlur: 10/64
            shadowScale: 1
            shadowColor: Qt.rgba(0,0,0,.45)
        }
        Rectangle {
            anchors.fill: parent
            radius: 19; antialiasing: true
            color: Qt.rgba(16/255,18/255,20/255,.42)
            border.width: 1
            border.color: Qt.rgba(205/255,214/255,244/255,.14)
            opacity: .82
        }
    }
}
