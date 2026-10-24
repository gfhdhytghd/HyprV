import QtQuick
Item {
    width: 480; height: 120
    function applyProps(props) {}
    Repeater {
        model: 12
        Item {
            required property int index
            x: 10+(index%6)*75; y: index < 6 ? 10 : 70
            width: 60; height: 40
            scale: [1,.96,.92,1.06,1.1,.5][index%6]
            Text {
                anchors.centerIn: parent
                width: parent.index < 6 ? implicitWidth : 54
                text: parent.index < 6 ? "\uf023" : "09:41"
                horizontalAlignment: parent.index < 6 ? Text.AlignLeft : Text.AlignHCenter
                font.family: "JetBrainsMono Nerd Font"; font.pixelSize: 16; font.weight: Font.Bold
                color: Harness.dark ? "#cdd6f4" : "#2b2b2c"
                renderType: Text.NativeRendering
            }
        }
    }
}
