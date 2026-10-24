// Independent Qt RHI golden scene for native material/batch-root regressions.
import QtQuick
import QtQuick.Effects
Item {
    width: 3072; height: 1728
    property real imageOpacity: Harness.fixture.props.effect ? .72 : 1
    function applyProps(props) {}
    Rectangle { x: 10; y: 10; width: 223.375; height: 38; radius: 19; color: Qt.rgba(30/255,30/255,46/255,.8) }
    Rectangle { x: 2433.375; y: 10; width: 400.875; height: 38; radius: 19; color: Qt.rgba(1,1,1,.28); border.width: 1; border.color: Qt.rgba(.15,.15,.16,.10) }
    Image { x: 3012.109375; y: 21; width: 16; height: 16; source: Harness.fixture.props.control; sourceSize: Qt.size(32,32); smooth: true; mipmap: true; opacity: parent.imageOpacity }
    Item { x: 2755; y: 60; width: 100; height: 35; clip: true
        Image { x: 9; y: 9; width: 16; height: 16; source: Harness.fixture.props.cover; sourceSize: Qt.size(16,16); smooth: true; opacity: parent.parent.imageOpacity }
    }
    Item { x: 2516; y: 58; width: 324; height: 190
        layer.enabled: true; layer.smooth: true; layer.textureSize: Qt.size(Math.round(width*Harness.scale),Math.round(height*Harness.scale))
        layer.effect: MultiEffect { shadowEnabled: true; shadowBlur: 10/64; shadowColor: Qt.rgba(0,0,0,.45) }
        Rectangle { anchors.fill: parent; radius: 19; antialiasing: true; color: Qt.rgba(16/255,18/255,20/255,.42); border.width: 1; border.color: Qt.rgba(205/255,214/255,244/255,.14); opacity: .82 }
    }
}
