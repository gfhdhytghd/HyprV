import QtQuick
Item { width: 240; height: 120
  Repeater { model: 8
    Item { x: 6 + index * 28 + [0, 0.25, 0.5, 0.75, 0, 0.25, 0.5, 0.75][index]; y: 6 + (index >= 4 ? 0.5 : 0); width: 24; height: 24
      Image { anchors.fill: parent; anchors.margins: 3; source: "claude.png"; fillMode: Image.PreserveAspectFit; smooth: true; mipmap: true } } }
  Repeater { model: 4
    Item { x: 6 + index * 28; y: 40; width: [24, 30, 17, 64][index]; height: [24, 30, 17, 64][index]
      Image { anchors.fill: parent; anchors.margins: 3; source: "codex.png"; fillMode: Image.PreserveAspectFit; smooth: true; mipmap: true } } }
}
