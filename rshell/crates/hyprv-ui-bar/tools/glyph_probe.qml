// Diagnostic only: native glyph over transparent or the bar's translucent fill.
import QtQuick
Item {
    width: 80; height: 60
    function applyProps(props) {}
    Rectangle {
        visible: Harness.fixture.props.effect
        x: 10; y: 10; width: 223.375; height: 38; radius: 19
        color: Qt.rgba(30/255,30/255,46/255,.8)
    }
    Text {
        x: 17; y: 17; width: 17.1875; height: 23
        text: "\uf303"; color: "#89b4fa"
        font.family: "JetBrainsMono Nerd Font"; font.pixelSize: 17; font.weight: Font.Bold
        horizontalAlignment: Text.AlignHCenter; renderType: Text.NativeRendering
    }
}
