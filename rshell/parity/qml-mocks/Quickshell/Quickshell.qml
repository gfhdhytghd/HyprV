pragma Singleton
import QtQuick
import ParityHost

// Mock of the Quickshell singleton. Screens come from fixture services.screens:
// [{ "name": "DP-4", "width": 3072, "height": 1728, "x": 0, "y": 0, "scale": 2 }]
QtObject {
    id: qs

    readonly property var screenData: ParityState.service("screens", [{ name: "DP-4", width: 3072, height: 1728, x: 0, y: 0, scale: 2 }])
    property var screens: []
    readonly property string shellDir: Harness.source

    function env(name) { return ParityState.envValue(name); }
    function execDetached(command) { console.log("parity: execDetached (not run):", JSON.stringify(command)); }

    property string _screenKey: ""
    property Component _screenComponent: Component { ShellScreen {} }
    function _rebuildScreens() {
        const key = JSON.stringify(screenData);
        if (key === _screenKey)
            return;
        _screenKey = key;
        screens = screenData.map(s => _screenComponent.createObject(qs, {
            name: s.name, x: s.x || 0, y: s.y || 0, width: s.width, height: s.height,
            devicePixelRatio: s.scale || 1
        }));
    }
    onScreenDataChanged: _rebuildScreens()
    Component.onCompleted: _rebuildScreens()
}
