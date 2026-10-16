import QtQuick
import Quickshell.Io

// Read effective windowsMove settings at startup and on config reload, without
// polling. Unset leaves inherit windows/global, just as in Hyprland's tree.
Item {
    id: motion
    property int duration: 700
    property var bezierCurve: [0.1, 0.9, 0.1, 1.05, 1, 1]
    property string curveName: "myBezier"
    property bool refreshQueued: false

    function refresh() {
        if (query.running) refreshQueued = true;
        else query.running = true;
    }
    function applySnapshot(text) {
        try {
            const snapshot = JSON.parse(text);
            const animations = snapshot[0];
            const curves = snapshot[1];
            const config = ["windowsMove", "windows", "global"]
                .map(name => animations.find(animation => animation.name === name))
                .find(animation => animation && animation.overridden);
            if (!config) return;
            const curve = curves.find(curve => curve.name === config.bezier);
            if (!curve) return;
            duration = config.enabled ? Math.max(0, Math.round(config.speed * 100)) : 0;
            curveName = config.bezier;
            bezierCurve = [curve.X0, curve.Y0, curve.X1, curve.Y1, 1, 1];
        } catch (error) {
            console.warn("Cannot read bar motion settings:", error);
        }
    }
    Process {
        id: query
        running: true
        command: ["hyprctl", "-j", "animations"]
        stdout: StdioCollector { id: snapshot }
        stderr: StdioCollector {}
        onExited: function(exitCode) {
            if (exitCode === 0) motion.applySnapshot(snapshot.text);
            if (motion.refreshQueued) {
                motion.refreshQueued = false;
                query.running = true;
            }
        }
    }
}
