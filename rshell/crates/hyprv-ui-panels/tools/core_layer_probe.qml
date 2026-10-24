// Independent Qt Quick oracle for AnimatedGlassPanel's explicit layer texture size.
import QtQuick
import QtQuick.Effects
Item {
    width: 3072; height: 1728
    function applyProps(props) {}
    Item {
        x: 10; y: 58; width: 548; height: 572; clip: true
        Item {
            anchors.fill: parent
            layer.enabled: true
            layer.smooth: true
            layer.textureSize: Qt.size(Math.round(width * Harness.scale), Math.round(height * Harness.scale))
            layer.textureMirroring: ShaderEffectSource.MirrorVertically
            layer.effect: MultiEffect {
                autoPaddingEnabled: true
                shadowEnabled: true
                shadowBlur: 10 / 64
                shadowScale: 1
                shadowColor: Qt.rgba(0,0,0,.45)
            }
            Rectangle {
                anchors.fill: parent; radius: 19; opacity: .82
                color: Qt.rgba(16/255,18/255,20/255,.42)
                border.width: 1
                border.color: Qt.rgba(205/255,214/255,244/255,.14)
                antialiasing: true
            }
        }
    }
}
