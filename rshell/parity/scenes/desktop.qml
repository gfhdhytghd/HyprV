import QtQuick
import ParityHost
import Quickshell

// Loads the ORIGINAL shell.qml (read-only, from Harness.source) against the
// mock Quickshell modules and composites every mock PanelWindow of the first
// screen into one output-sized canvas, ordered like layer-shell: by layer
// (Background < Bottom < Top < Overlay), then by creation order. The canvas is
// transparent where no window paints (no wallpaper, no compositor blur).
Item {
    id: scene

    readonly property var screen0: Quickshell.screens[0] || ({ name: "DP-4", width: 3072, height: 1728 })
    width: (Harness.fixture.canvas && Harness.fixture.canvas.width) || screen0.width
    height: (Harness.fixture.canvas && Harness.fixture.canvas.height) || screen0.height

    // Keeps Qt's animation driver running so QML Timers advance on the
    // virtual clock instead of Qt's real-time pause timer.
    Item { id: clockAnchor; property real v: 0 }
    NumberAnimation {
        target: clockAnchor; property: "v"; from: 0; to: 1
        duration: 1000; loops: Animation.Infinite; running: true
    }

    property QtObject shell: null
    // Mock window contents live here until syncWindows() slots them (ParityState.earlyParent).
    Item { id: earlyHolder; visible: false }
    property var slots: []

    Component {
        id: slotComponent

        Item {
            property QtObject win: null
            property int order: 0
            readonly property QtObject layershell: win ? ParityLayershell.layershellOf(win) : null
            readonly property string namespace: layershell ? layershell.namespace : ""
            objectName: "window:" + namespace
            x: win ? win.x : 0
            y: win ? win.y : 0
            width: win ? win.width : 0
            height: win ? win.height : 0
            visible: !!win && win.visible
            // xdg popups (PopupWindow mock) stack above every layer surface.
            z: (win && win.isPopup ? 4 : (layershell ? layershell.layer : 2)) * 1000 + order
        }
    }

    function syncWindows() {
        const known = slots.map(s => s.win);
        for (const win of ParityState.windows) {
            if (known.indexOf(win) >= 0)
                continue;
            if (win.screen && win.screen.name !== screen0.name)
                continue;
            const slot = slotComponent.createObject(scene, { win: win, order: slots.length });
            win.contentItem.parent = slot;
            slots = slots.concat([slot]);
        }
    }

    Connections {
        target: ParityState
        function onWindowsChanged() { scene.syncWindows(); }
    }

    Component.onCompleted: {
        if (Harness.fixture.dump_js_time)
            console.log("parity: js time", new Date().toISOString(), Date.now());
        // Fixture "entry": the shell file to load (default shell.qml; e.g. StartupConfirm.qml).
        if (Harness.scene_in_window)
            ParityState.earlyParent = earlyHolder;
        const component = Qt.createComponent("file://" + Harness.source + "/" + (Harness.fixture.entry || "shell.qml"));
        if (component.status !== Component.Ready) {
            console.error(component.errorString());
            Qt.exit(4);
            return;
        }
        shell = component.createObject(scene);
        if (!shell) {
            console.error("entry failed to instantiate");
            Qt.exit(4);
            return;
        }
        syncWindows();
    }

    // Fixture "props" / "set" keys:
    //   services: {...}            merged into ParityState.services
    //   ipc: [target, fn, args...] call a registered IpcHandler function
    //   root: {property: value}    assign properties on the shell root object
    function applyProps(map) {
        if (!map)
            return;
        if (map.services)
            ParityState.mergeServices(JSON.parse(JSON.stringify(map.services)));
        if (map.root && shell) {
            for (const key in map.root)
                shell[key] = JSON.parse(JSON.stringify(map.root[key]));
        }
        if (map.ipc) {
            const call = JSON.parse(JSON.stringify(map.ipc));
            ParityState.callIpc(call[0], call[1], call.slice(2));
        }
    }
}
