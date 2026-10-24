import QtQuick
Item {
    width: 320; height: 240
    function applyProps(props) {}
    Rectangle { anchors.fill: parent; visible: Harness.fixture.props.effect; color: Qt.rgba(30/255,30/255,46/255,.8) }
    Repeater {
        model: 16
        Text {
            required property int index
            x: (index%4)*80+17; y: Math.floor(index/4)*60+17
            width: 45; height: 23; text: "\uf303AB"
            color: Qt.rgba(137/255,180/255,250/255,[1,.72,.14,.98][index%4])
            opacity: [1,.82,.45,.13][Math.floor(index/4)]
            font.family: "JetBrainsMono Nerd Font"; font.pixelSize: 17; font.weight: Font.Bold
            horizontalAlignment: Text.AlignHCenter; renderType: Text.NativeRendering
        }
    }
}
