import QtQuick
import Quickshell.Io

Item {
    id: root

    property var shellRoot: null
    property bool expanded: false
    property bool online: false
    property string service: "none"
    property string mode: ""
    property bool busy: false
    readonly property string helper: shellRoot ? shellRoot.configDir + "/quickshell/scripts/mihomo-control.sh" : ""

    signal detailsClicked()

    function applyStatus(output) {
        try {
            const data = JSON.parse(output);
            online = data.online === true;
            service = String(data.service || "none");
            mode = String(data.mode || "").toLowerCase();
        } catch (error) {
            online = false;
        }
    }

    function toggleProxy() {
        if (busy || (service !== "user" && service !== "system")) return;
        busy = true;
        action.command = ["bash", helper, online ? "stop" : "start"];
        action.running = true;
    }

    implicitHeight: 62

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
        stdout: StdioCollector { id: actionOutput }
        stderr: StdioCollector {}
        onExited: function(exitCode) {
            root.busy = false;
            if (exitCode === 0) root.applyStatus(actionOutput.text || "{}");
            poll.refresh();
        }
    }

    ControlPanelSplitTile {
        anchors.fill: parent
        shellRoot: root.shellRoot
        icon: root.online ? "󰀂" : "󰀝"
        title: "Proxy"
        subtitle: root.busy ? "Switching…" : (root.online ? ((root.mode || "Rule") + " mode") : "Off")
        active: root.online
        expanded: root.expanded
        leftEnabled: !root.busy && (root.service === "user" || root.service === "system")
        rightEnabled: true
        expandIndicatorVisible: true
        onLeftClicked: root.toggleProxy()
        onRightClicked: root.detailsClicked()
    }
}
