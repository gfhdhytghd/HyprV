import QtQuick
import Quickshell
import Quickshell.Io

Item {
    id: root

    property string dcalPath: (Quickshell.env("HOME") || "") + "/.local/bin/dcal"
    property string socketPath: ""
    property bool connected: false
    property bool subscribed: false
    property bool loading: false
    property string lastError: ""
    property date weekStart: startOfWeek(new Date())
    property var calendars: []
    property var events: []
    property var pendingRequests: ({})
    property int requestCounter: 0

    signal childWindowRequested()

    Component.onCompleted: discoverSocket()

    function startOfWeek(value) {
        const d = new Date(value);
        d.setHours(0, 0, 0, 0);
        const mondayOffset = (d.getDay() + 6) % 7;
        d.setDate(d.getDate() - mondayOffset);
        return d;
    }

    function discoverSocket() {
        if (!socketDiscovery.running)
            socketDiscovery.running = true;
    }

    function nextId() {
        requestCounter++;
        return Date.now() + requestCounter;
    }

    function sendRequest(method, params, callback) {
        if (!requestSocket.linkUp) {
            lastError = "日历服务未连接";
            if (callback)
                callback({"error": lastError});
            return;
        }
        const id = nextId();
        if (callback)
            pendingRequests[id] = callback;
        requestSocket.send({"id": id, "method": method, "params": params || {}});
    }

    function handleResponse(line) {
        if (!line || line.length === 0)
            return;
        let message;
        try {
            message = JSON.parse(line);
        } catch (e) {
            return;
        }
        if (message.event) {
            handleSubscription(message);
            return;
        }
        if (!message.id)
            return;
        const callback = pendingRequests[message.id];
        if (!callback)
            return;
        delete pendingRequests[message.id];
        callback(message.error ? {"error": message.error} : {"result": message.result});
    }

    function handleSubscription(message) {
        switch (message.event) {
        case "accounts":
        case "calendars":
            refreshCalendars();
            refreshDebounce.restart();
            break;
        case "events":
        case "sync":
            refreshDebounce.restart();
            break;
        }
    }

    function refreshCalendars() {
        sendRequest("calendars.list", {}, response => {
            if (response.error) {
                lastError = response.error;
                return;
            }
            calendars = Array.isArray(response.result) ? response.result : [];
        });
    }

    function loadWeek(value) {
        weekStart = startOfWeek(value);
        if (!connected)
            return;
        const from = new Date(weekStart);
        const to = new Date(from);
        to.setDate(to.getDate() + 7);
        loading = true;
        sendRequest("events.list", {
            "from": from.toISOString(),
            "to": to.toISOString(),
            "limit": 2000
        }, response => {
            loading = false;
            if (response.error) {
                lastError = response.error;
                return;
            }
            const result = response.result || {};
            events = Array.isArray(result.events) ? result.events : [];
            lastError = "";
        });
    }

    function previousWeek() {
        const d = new Date(weekStart);
        d.setDate(d.getDate() - 7);
        loadWeek(d);
    }

    function nextWeek() {
        const d = new Date(weekStart);
        d.setDate(d.getDate() + 7);
        loadWeek(d);
    }

    function today() {
        loadWeek(new Date());
    }

    function openEvent(event) {
        if (!event || !event.uid)
            return;
        childWindowRequested();
        sendRequest("ui.openEvent", {
            "uid": event.uid,
            "start": event.start || "",
            "editorOnly": true
        });
    }

    function createEvent() {
        childWindowRequested();
        sendRequest("ui.newEvent", {"editorOnly": true});
    }

    function showFullApp() {
        sendRequest("ui.show", {"view": "week"});
    }

    Process {
        id: socketDiscovery
        command: [root.dcalPath, "--json", "socket"]
        running: false
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const result = JSON.parse(text);
                    if (!result.path)
                        throw new Error("missing socket path");
                    root.socketPath = result.path;
                    requestSocket.connected = true;
                } catch (e) {
                    root.lastError = "无法连接日历服务";
                    discoveryRetry.restart();
                }
            }
        }
        onExited: code => {
            if (code !== 0)
                discoveryRetry.restart();
        }
    }

    Timer {
        id: discoveryRetry
        interval: 1800
        repeat: false
        onTriggered: root.discoverSocket()
    }

    Timer {
        id: refreshDebounce
        interval: 350
        repeat: false
        onTriggered: root.loadWeek(root.weekStart)
    }

    CalendarSocket {
        id: requestSocket
        path: root.socketPath
        connected: false
        parser: SplitParser {
            onRead: line => root.handleResponse(line)
        }
        onConnectionStateChanged: {
            root.connected = linkUp;
            if (linkUp) {
                subscribeSocket.connected = true;
                root.refreshCalendars();
                root.loadWeek(root.weekStart);
            } else {
                root.subscribed = false;
                root.pendingRequests = ({});
            }
        }
    }

    CalendarSocket {
        id: subscribeSocket
        path: root.socketPath
        connected: false
        parser: SplitParser {
            onRead: line => root.handleResponse(line)
        }
        onConnectionStateChanged: {
            root.subscribed = linkUp;
            if (linkUp) {
                send({
                    "id": root.nextId(),
                    "method": "subscribe",
                    "params": {"topics": ["accounts", "calendars", "events", "sync"]}
                });
            }
        }
    }
}
