import QtQuick
import Quickshell.Hyprland

// One animation clock per output, shared by the bar and its corner window.
Item {
    id: screenState
    required property var shellRoot
    required property var screenInfo
    readonly property var monitor: Hyprland.monitorFor(screenInfo)
    readonly property bool expanded: shellRoot.barExpanded(screenInfo.name)
    readonly property bool fullscreen: {
        const workspace = monitor ? monitor.activeWorkspace : null;
        if (!workspace || !workspace.hasFullscreen) return false;
        // hasFullscreen also covers maximized windows, which retain the bar.
        return workspace.toplevels.values.some(window => Number(window.lastIpcObject.fullscreen) === 2);
    }
    readonly property bool hideForFullscreen: fullscreen
    property real reveal: expanded ? 1 : 0
    property real fullscreenHide: hideForFullscreen ? 1 : 0
    readonly property real compactOffset: -15 * fullscreenHide
    // Keep the joined edge on screen during the native curve's overshoot.
    readonly property real topCornerOffset: Math.max(0, Math.min(15, 15 * (1 - reveal) * (1 - fullscreenHide)))
    readonly property real bottomCornerOffset: 0
    readonly property bool presented: fullscreenHide < 1

    Behavior on reveal {
        NumberAnimation {
            duration: screenState.shellRoot.barMotionDuration
            easing.type: Easing.BezierSpline
            easing.bezierCurve: screenState.shellRoot.barMotionCurve
        }
    }
    Behavior on fullscreenHide {
        NumberAnimation {
            duration: screenState.shellRoot.barMotionDuration
            easing.type: Easing.BezierSpline
            easing.bezierCurve: screenState.shellRoot.barMotionCurve
        }
    }

    Component.onCompleted: shellRoot.registerBarScreenState(screenInfo.name, screenState)
    Component.onDestruction: shellRoot.unregisterBarScreenState(screenInfo.name, screenState)
}
