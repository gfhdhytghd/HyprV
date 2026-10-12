import QtQuick
import Quickshell.Io

ControlPanelToggle {
    id: root

    property bool running: false
    property bool busy: false
    readonly property string helper: shellRoot ? shellRoot.configDir + "/quickshell/scripts/windows-vm-control.sh" : ""

    function applyStatus(output) {
        running = output.trim() === "running";
    }

    function toggleVm() {
        if (busy || !helper) return;
        busy = true;
        action.command = ["bash", helper, running ? "hibernate" : "start"];
        action.running = true;
    }

    icon: "󰖳"
    iconOnly: true
    label: busy ? (running ? "Sleeping" : "Starting") : (running ? "Windows on" : "Windows off")
    active: running
    toggleEnabled: !busy
    onClicked: toggleVm()

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
        stderr: StdioCollector {}
        onExited: function(exitCode) {
            root.busy = false;
            poll.refresh();
        }
    }
}
