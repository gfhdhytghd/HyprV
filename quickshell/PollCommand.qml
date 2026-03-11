import QtQuick
import Quickshell.Io

Item {
    id: poll

    property var command: []
    property int interval: 1000
    property bool active: true
    property string output: ""

    signal updated(string output, int exitCode)

    function refresh() {
        if (!active || !command || command.length === 0 || proc.running) {
            return;
        }
        proc.command = command;
        proc.running = true;
    }

    Timer {
        interval: poll.interval
        repeat: true
        running: poll.active
        triggeredOnStart: true
        onTriggered: poll.refresh()
    }

    Process {
        id: proc

        running: false
        stdout: StdioCollector {
            id: collector
        }
        stderr: StdioCollector {}

        onExited: function(exitCode) {
            const text = (collector.text || "").trim();
            if (exitCode === 0) {
                poll.output = text;
            }
            poll.updated(text, exitCode);
        }
    }
}
