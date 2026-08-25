import QtQuick
import Quickshell.Io

Item {
    id: root

    property alias path: socket.path
    property alias parser: socket.parser
    property bool connected: false
    property bool linkUp: false
    property int reconnectAttempt: 0

    signal connectionStateChanged()

    onConnectedChanged: socket.connected = connected

    function send(data) {
        const json = typeof data === "string" ? data : JSON.stringify(data);
        socket.write(json.endsWith("\n") ? json : json + "\n");
        socket.flush();
    }

    function scheduleReconnect() {
        const base = Math.min(400 * Math.pow(2, Math.min(reconnectAttempt, 7)), 12000);
        reconnectTimer.interval = base + Math.floor(Math.random() * Math.max(1, base / 4));
        reconnectAttempt++;
        reconnectTimer.restart();
    }

    Socket {
        id: socket

        onConnectionStateChanged: {
            root.linkUp = connected;
            root.connectionStateChanged();
            if (connected) {
                root.reconnectAttempt = 0;
            } else if (root.connected) {
                root.scheduleReconnect();
            }
        }
    }

    Timer {
        id: reconnectTimer
        repeat: false
        onTriggered: {
            socket.connected = false;
            Qt.callLater(() => socket.connected = true);
        }
    }
}
