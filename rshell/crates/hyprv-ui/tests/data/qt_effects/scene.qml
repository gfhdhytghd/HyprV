import QtQuick
import QtQuick.Effects
// Rendered by experiments/text/qt-ref textref at dpr 2 (unmapped Wayland window).
Item { width: 200; height: 80
  // ControlPanelToggle icon: Image + layer MultiEffect colorization
  Image { x: 10; y: 10; width: 20; height: 20; source: "light-dark.svg"; fillMode: Image.PreserveAspectFit; smooth: true; mipmap: true; sourceSize.width: 40; sourceSize.height: 40
    layer.enabled: true; layer.effect: MultiEffect { colorization: 1; colorizationColor: "#ffffff" } }
  Image { x: 40; y: 10; width: 20; height: 20; source: "light-dark.svg"; fillMode: Image.PreserveAspectFit; smooth: true; mipmap: true; sourceSize.width: 40; sourceSize.height: 40
    layer.enabled: true; layer.effect: MultiEffect { colorization: 1; colorizationColor: "#2b2b2c" } }
  Image { x: 70; y: 10; width: 20; height: 20; source: "light-dark.svg"; fillMode: Image.PreserveAspectFit; smooth: true; mipmap: true; sourceSize.width: 40; sourceSize.height: 40
    layer.enabled: true; layer.effect: MultiEffect { colorization: 0.5; colorizationColor: "#cc3311" } }
  // ControlPanelMediaCard album art: PreserveAspectCrop + MultiEffect mask (rounded Rectangle layer)
  Item { x: 110; y: 10; width: 44; height: 44
    Rectangle { id: m; anchors.fill: parent; radius: 11; visible: false; layer.enabled: true }
    Image { anchors.fill: parent; source: "cover-warm.png"; fillMode: Image.PreserveAspectCrop; smooth: true; mipmap: true
      layer.enabled: true; layer.effect: MultiEffect { maskEnabled: true; maskSource: m; maskThresholdMin: 0.5; maskSpreadAtMin: 1 } }
  }
}
