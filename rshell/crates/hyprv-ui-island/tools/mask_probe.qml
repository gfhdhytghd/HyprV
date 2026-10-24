import QtQuick
import QtQuick.Effects
Item {
    width: 80; height: 60
    function applyProps(props) {}
    Rectangle {
        x: 17.375; y: 10.25; width: 40; height: 40
        color: "#89b4fa"; opacity: .91
        layer.enabled: true
        layer.effect: MultiEffect {
            maskEnabled: true; maskSource: mask
            maskThresholdMin: .5; maskSpreadAtMin: 1
        }
    }
    Rectangle {
        id: mask
        width: 40; height: 40; radius: 11
        visible: false; layer.enabled: true
    }
}
