import QtQuick
import ParityHost

// Never executes anything. Output comes from the first fixture "processes"
// entry whose "match" substring occurs in the command line:
//   { "match": "media-status.sh", "stdout": "available=true\n...", "exitCode": 0,
//     "delay_ms": 0,
//     "lines": [{"at": 0, "data": "..."}, {"at": 1000, "data": "..."}],
//     "stream": false }
// "stdout" is delivered whole (StdioCollector) or split (SplitParser); "lines"
// are delivered one by one at virtual times relative to the start; with
// "stream": true the process keeps running after the last line.
QtObject {
    id: proc

    property var command: []
    property bool running: false
    property var environment: ({})
    property bool clearEnvironment: false
    property string workingDirectory: ""
    property bool stdinEnabled: false
    property QtObject stdout: null
    property QtObject stderr: null
    readonly property int processId: running ? 4242 : 0

    signal started()
    signal exited(int exitCode, int exitStatus)

    property var _entry: null
    property int _generation: 0
    property bool _completed: false

    function startDetached() {
        console.log("parity: startDetached (not run):", JSON.stringify(command));
    }
    function write(data) {}

    function _deliver(text) {
        if (!stdout || text === undefined || text === null)
            return;
        text = String(text).replace(/@HOME@/g, Harness.home).replace(/@ASSETS@/g, Harness.assets);
        if (typeof stdout.read === "function") {
            const marker = stdout.splitMarker || "\n";
            const parts = String(text).split(marker);
            if (parts.length > 0 && parts[parts.length - 1] === "")
                parts.pop();
            for (const part of parts)
                stdout.read(part);
        } else {
            stdout.text = stdout.text + String(text);
        }
    }

    function _finish(generation) {
        if (generation !== _generation || !running)
            return;
        const entry = _entry;
        if (entry && entry.stream)
            return;
        if (stdout && typeof stdout.read !== "function")
            stdout.streamFinished();
        running = false;
        exited(entry ? (entry.exitCode || 0) : 1, 0);
    }

    function _start() {
        _generation += 1;
        const generation = _generation;
        _entry = ParityState.processFor(command);
        if (stdout && typeof stdout.read !== "function")
            stdout.text = "";
        started();
        const entry = _entry;
        const delay = entry ? Number(entry.delay_ms || 0) : 0;
        const lines = entry && entry.lines ? entry.lines : [];
        let last = delay;
        for (const line of lines) {
            const at = delay + Number(line.at || 0);
            last = Math.max(last, at);
            _schedule(at, () => { if (generation === _generation && running) _deliver(line.data + "\n"); });
        }
        _schedule(delay, () => { if (generation === _generation && running && entry && entry.stdout !== undefined) _deliver(entry.stdout); });
        _schedule(last, () => _finish(generation));
    }

    // Virtual-clock timers (QML Timer runs on the harness animation driver).
    function _schedule(ms, fn) {
        const t = timerComponent.createObject(proc, { interval: Math.max(1, ms) });
        t.triggered.connect(() => { fn(); t.destroy(); });
        t.start();
    }
    property Component timerComponent: Component { Timer { repeat: false } }

    onRunningChanged: {
        if (!_completed)
            return;
        if (running)
            _start();
        else
            _generation += 1;
    }
    Component.onCompleted: {
        _completed = true;
        if (running)
            _start();
    }
}
