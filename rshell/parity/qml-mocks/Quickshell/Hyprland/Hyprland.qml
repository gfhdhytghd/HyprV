pragma Singleton
import QtQuick
import ParityHost

// services.hyprland: { workspaces: [{id, name, urgent}], focusedWorkspace: 1,
//                      activeToplevel: {title} | null, fullscreen: false }
QtObject {
    signal rawEvent(var event)

    readonly property var workspaces: ({
        values: ParityState.service("hyprland.workspaces", [{ id: 1, name: "1" }]).map(w => ({
            id: w.id, name: String(w.name ?? w.id), urgent: !!w.urgent,
            active: w.id === ParityState.service("hyprland.focusedWorkspace", 1),
            activate: function () { console.log("parity: workspace.activate", w.id); }
        }))
    })
    readonly property var focusedWorkspace: workspaces.values.find(w => w.active) || null
    readonly property var activeToplevel: ParityState.service("hyprland.activeToplevel", null)
    readonly property var toplevels: ({ values: [] })
    function refreshToplevels() {}
    function dispatch(request) { console.log("parity: Hyprland.dispatch (not run):", request); }
    function monitorFor(screen) {
        const fullscreen = !!ParityState.service("hyprland.fullscreen", false);
        return {
            name: screen ? screen.name : "",
            activeWorkspace: {
                hasFullscreen: fullscreen,
                toplevels: { values: fullscreen ? [{ lastIpcObject: { fullscreen: 2 } }] : [] }
            }
        };
    }
}
