import QtQuick
import Quickshell.Io

// Keep subscriptions alive without spawning a command for every update.
Item {
    id: stream

    property var command: []
    signal received(string line)
    signal stopped()

    Process {
        id: process
        command: stream.command
        running: true
        stdout: SplitParser {
            onRead: line => stream.received(line)
        }
        onExited: {
            stream.stopped();
        }
    }

    Timer {
        interval: 5000
        repeat: true
        running: !process.running
        onTriggered: process.running = true
    }
}
