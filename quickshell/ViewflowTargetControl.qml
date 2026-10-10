import QtQuick
import Quickshell
import Quickshell.Io

ControlPanelToggle {
    id: root

    property string target: "windows"
    property bool busy: false
    readonly property string helper: shellRoot ? shellRoot.configDir + "/quickshell/scripts/viewflow-target.sh" : ""

    function applyStatus(output) {
        const nextTarget = output.trim();
        if (nextTarget === "macos" || nextTarget === "windows") {
            target = nextTarget;
        }
    }

    function toggleTarget() {
        if (busy || !helper) return;
        busy = true;
        action.command = ["bash", helper, "set", target === "windows" ? "macos" : "windows"];
        action.running = true;
    }

    icon: target === "macos" ? "" : "󰖳"
    iconOnly: true
    label: busy ? "Switching HDMI / Viewflow" : (target === "macos" ? "HDMI → macOS" : "HDMI → Windows")
    active: false
    toggleEnabled: !busy
    onClicked: toggleTarget()

    PollCommand {
        id: poll
        active: root.visible
        interval: 2000
        command: root.helper ? ["bash", root.helper, "status"] : []
        onUpdated: function(output, exitCode) {
            if (exitCode === 0) root.applyStatus(output);
        }
    }

    Process {
        id: action
        stdout: StdioCollector {}
        stderr: StdioCollector { id: actionError }
        onExited: function(exitCode) {
            root.busy = false;
            poll.refresh();
            if (exitCode !== 0) {
                const message = actionError.text.trim() || "Switch failed";
                Quickshell.execDetached(["notify-send", "Viewflow Apple/Windows", message]);
            }
        }
    }
}
