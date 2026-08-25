import QtQuick
import Quickshell
import Quickshell.Io

Rectangle {
    id: root

    property var shellRoot: null
    property bool useExternalPanelBackground: false
    signal closeRequested()

    property bool online: false
    property string service: "none"
    property string version: ""
    property string mode: ""
    property int connections: 0
    property real upload: 0
    property real download: 0
    property string dashboard: ""
    property bool busy: false
    property string message: ""

    readonly property color cardFill: shellRoot ? shellRoot.withAlpha("#ffffff", shellRoot.darkMode ? 0.07 : 0.22) : "#2a2a2a"
    readonly property color cardStrongFill: shellRoot ? shellRoot.withAlpha("#ffffff", shellRoot.darkMode ? 0.11 : 0.3) : "#303030"
    readonly property color cardStroke: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.12 : 0.08) : "#454545"
    readonly property color mutedText: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, 0.68) : "#b0b0b0"
    readonly property color accent: shellRoot ? shellRoot.launchColor : "#89b4fa"
    readonly property color downColor: shellRoot ? shellRoot.usageLowColor : "#7ad48b"
    readonly property color upColor: shellRoot ? shellRoot.microphoneColor : "#cba6f7"
    readonly property string helper: shellRoot ? shellRoot.configDir + "/quickshell/scripts/mihomo-control.sh" : ""

    radius: 19
    color: useExternalPanelBackground ? "transparent" : cardFill
    border.width: useExternalPanelBackground ? 0 : 1
    border.color: useExternalPanelBackground ? "transparent" : cardStroke
    implicitHeight: content.implicitHeight + 20

    function formatRate(bytes) {
        const value = Math.max(0, Number(bytes) || 0);
        if (value >= 1048576) return (value / 1048576).toFixed(value >= 10485760 ? 0 : 1) + " MB/s";
        if (value >= 1024) return (value / 1024).toFixed(value >= 10240 ? 0 : 1) + " KB/s";
        return Math.round(value) + " B/s";
    }

    function applyStatus(text) {
        try {
            const data = JSON.parse(text);
            online = data.online === true;
            service = String(data.service || "none");
            version = String(data.version || "");
            mode = String(data.mode || "").toLowerCase();
            connections = Number(data.connections || 0);
            upload = Number(data.upload || 0);
            download = Number(data.download || 0);
            dashboard = String(data.dashboard || "");
            message = "";
        } catch (error) {
            message = "Could not read Mihomo status";
        }
    }

    function runAction(args) {
        if (!helper || actionProcess.running) return;
        busy = true;
        message = "";
        actionProcess.command = ["bash", helper].concat(args);
        actionProcess.running = true;
    }

    Component.onCompleted: statusPoll.refresh()

    PollCommand {
        id: statusPoll
        active: root.visible
        interval: 1500
        command: root.helper ? ["bash", root.helper, "status"] : []
        onUpdated: function(output, exitCode) {
            if (exitCode === 0) root.applyStatus(output);
        }
    }

    Process {
        id: actionProcess
        stdout: StdioCollector { id: actionOutput }
        stderr: StdioCollector { id: actionError }
        onExited: function(exitCode) {
            root.busy = false;
            if (exitCode === 0) {
                root.applyStatus(actionOutput.text || "{}");
                statusPoll.refresh();
            } else if (exitCode === 3) {
                root.message = "Service is still managed by Clash Verge";
            } else {
                root.message = (actionError.text || "Mihomo action failed").trim();
            }
        }
    }

    Column {
        id: content
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: 10
        spacing: 10

        Row {
            width: parent.width
            height: 40
            spacing: 10

            Rectangle {
                width: 40; height: 40; radius: 12
                color: root.online ? (root.shellRoot ? root.shellRoot.withAlpha(root.accent, 0.24) : "#334455") : root.cardStrongFill
                Text {
                    anchors.centerIn: parent
                    text: root.online ? "󰖂" : "󰖪"
                    color: root.online ? root.accent : root.mutedText
                    font.family: root.shellRoot ? root.shellRoot.iconFont : "JetBrainsMono Nerd Font"
                    font.pixelSize: 20; font.weight: Font.Bold
                }
            }

            Column {
                width: parent.width - 100
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Text {
                    width: parent.width
                    text: root.online ? "Mihomo · " + (root.mode || "Connected") : "Mihomo"
                    color: root.shellRoot ? root.shellRoot.primaryText : "white"
                    font.family: root.shellRoot ? root.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                    font.pixelSize: 13; font.weight: Font.Bold
                    elide: Text.ElideRight
                }
                Text {
                    width: parent.width
                    text: root.online ? root.connections + " active connections" : "Core is not reachable"
                    color: root.mutedText
                    font.family: root.shellRoot ? root.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                    font.pixelSize: 9; elide: Text.ElideRight
                }
            }

            Rectangle {
                width: 40; height: 24; radius: 12
                anchors.verticalCenter: parent.verticalCenter
                color: root.online ? (root.shellRoot ? root.shellRoot.withAlpha(root.downColor, 0.22) : "#335544") : root.cardStrongFill
                Text { anchors.centerIn: parent; text: root.online ? "ON" : "OFF"; color: root.online ? root.downColor : root.mutedText; font.pixelSize: 9; font.bold: true }
                MouseArea {
                    anchors.fill: parent
                    enabled: root.service === "user" || root.service === "system"
                    cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                    onClicked: root.runAction([root.online ? "stop" : "start"])
                }
            }
        }

        Row {
            width: parent.width
            spacing: 8
            Repeater {
                model: [
                    { glyph: "󰇚", label: "Download", value: root.formatRate(root.download), tone: root.downColor },
                    { glyph: "󰕒", label: "Upload", value: root.formatRate(root.upload), tone: root.upColor }
                ]
                delegate: Rectangle {
                    required property var modelData
                    width: (content.width - 8) / 2; height: 54; radius: 14
                    color: root.cardFill; border.width: 1; border.color: root.cardStroke
                    Row {
                        anchors.fill: parent; anchors.margins: 9; spacing: 8
                        Text { anchors.verticalCenter: parent.verticalCenter; text: modelData.glyph; color: modelData.tone; font.family: root.shellRoot ? root.shellRoot.iconFont : "JetBrainsMono Nerd Font"; font.pixelSize: 16 }
                        Column {
                            anchors.verticalCenter: parent.verticalCenter; spacing: 1
                            Text { text: modelData.value; color: root.shellRoot ? root.shellRoot.primaryText : "white"; font.family: root.shellRoot ? root.shellRoot.baseFont : "JetBrainsMono Nerd Font"; font.pixelSize: 11; font.bold: true }
                            Text { text: modelData.label; color: root.mutedText; font.family: root.shellRoot ? root.shellRoot.baseFont : "JetBrainsMono Nerd Font"; font.pixelSize: 8 }
                        }
                    }
                    SequentialAnimation on border.color {
                        running: root.online && Number(modelData.label === "Download" ? root.download : root.upload) > 1024
                        loops: Animation.Infinite
                        ColorAnimation { to: modelData.tone; duration: 650 }
                        ColorAnimation { to: root.cardStroke; duration: 650 }
                    }
                }
            }
        }

        Row {
            width: parent.width; spacing: 7
            Repeater {
                model: ["rule", "global", "direct"]
                delegate: Rectangle {
                    required property string modelData
                    width: (content.width - 14) / 3; height: 34; radius: 11
                    color: root.mode === modelData ? (root.shellRoot ? root.shellRoot.withAlpha(root.accent, 0.25) : "#334455") : root.cardFill
                    border.width: 1
                    border.color: root.mode === modelData ? root.accent : root.cardStroke
                    Text { anchors.centerIn: parent; text: modelData.charAt(0).toUpperCase() + modelData.slice(1); color: root.mode === modelData ? root.accent : root.mutedText; font.pixelSize: 10; font.bold: root.mode === modelData }
                    MouseArea { anchors.fill: parent; enabled: root.online && !root.busy; cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor; onClicked: root.runAction(["mode", modelData]) }
                }
            }
        }

        Row {
            width: parent.width; spacing: 8
            Rectangle {
                width: (parent.width - 8) / 2; height: 34; radius: 11; color: root.cardFill; border.width: 1; border.color: root.cardStroke
                Text { anchors.centerIn: parent; text: root.busy ? "Working…" : "Refresh"; color: root.mutedText; font.pixelSize: 10; font.bold: true }
                MouseArea { anchors.fill: parent; enabled: !root.busy; cursorShape: Qt.PointingHandCursor; onClicked: statusPoll.refresh() }
            }
            Rectangle {
                width: (parent.width - 8) / 2; height: 34; radius: 11; color: root.cardFill; border.width: 1; border.color: root.cardStroke
                Text { anchors.centerIn: parent; text: "Dashboard  ↗"; color: root.online ? root.accent : root.mutedText; font.pixelSize: 10; font.bold: true }
                MouseArea { anchors.fill: parent; enabled: root.online && root.dashboard.length > 0; cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor; onClicked: root.shellRoot.runDetached(["xdg-open", root.dashboard]) }
            }
        }

        Text {
            width: parent.width
            visible: root.message.length > 0 || root.service === "verge"
            text: root.message.length > 0 ? root.message : "Managed by Clash Verge · controls stay read-only until migration"
            color: root.message.length > 0 && root.shellRoot ? root.shellRoot.criticalColor : root.mutedText
            font.family: root.shellRoot ? root.shellRoot.baseFont : "JetBrainsMono Nerd Font"
            font.pixelSize: 8; wrapMode: Text.Wrap
        }

        Rectangle { width: parent.width; height: 1; color: root.cardStroke }

        Text {
            width: parent.width; text: "‹  Back"
            color: root.mutedText; font.pixelSize: 10; font.bold: true
            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.closeRequested() }
        }
    }
}
