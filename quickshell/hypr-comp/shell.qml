//@ pragma UseQApplication

import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

ShellRoot {
    id: root

    property bool launcherVisible: false

    IpcHandler {
        target: "hyprComp"
        enabled: true

        function openLauncher(): void {
            root.launcherVisible = true;
        }

        function closeLauncher(): void {
            root.launcherVisible = false;
        }

        function toggleLauncher(): void {
            root.launcherVisible = !root.launcherVisible;
        }
    }

    Loader { source: "Stars.qml" }
    Loader { source: "moon.qml" }

    PanelWindow {
        id: appLauncherWindow

        WlrLayershell.namespace: "hypr-comp-launcher"
        WlrLayershell.layer: WlrLayer.Overlay

        exclusionMode: ExclusionMode.Ignore
        focusable: visible
        color: "transparent"
        visible: root.launcherVisible

        implicitWidth: screen.width
        implicitHeight: screen.height

        Loader {
            anchors.fill: parent
            active: appLauncherWindow.visible
            source: "applauncher/applauncher.qml"
        }

        Shortcut {
            sequence: "Escape"
            enabled: appLauncherWindow.visible
            onActivated: root.launcherVisible = false
        }
    }
}
