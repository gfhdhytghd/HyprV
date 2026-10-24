import QtQuick
import ParityHost

// Line-oriented socket mock. A socket "connects" only when fixture
// "sockets" has an entry whose key is a substring of `path`:
//   "sockets": { "dcal": { "methods": { "events.list": {...result...} } } }
// Each written line is parsed as a JSON-RPC request {id, method, params};
// the reply {id, result} (or {id, error} for unknown methods) is delivered to
// `parser` one virtual frame later. Without an entry nothing ever connects.
QtObject {
    id: socket
    property string path: ""
    property bool connected: false
    property QtObject parser: null
    signal connectionStateChanged()
    signal error(int error)

    property bool _linked: false
    property string _buffer: ""

    function _config() {
        const sockets = ParityState.fixture.sockets || {};
        for (const key in sockets) {
            if (path && path.indexOf(key) >= 0)
                return sockets[key];
        }
        return null;
    }

    function write(data) {
        if (!_linked)
            return;
        _buffer += String(data);
        let index;
        while ((index = _buffer.indexOf("\n")) >= 0) {
            const line = _buffer.slice(0, index);
            _buffer = _buffer.slice(index + 1);
            _handle(line);
        }
    }
    function flush() {}

    function _handle(line) {
        let request;
        try { request = JSON.parse(line); } catch (e) { return; }
        const methods = (_config() || {}).methods || {};
        const reply = request.method in methods
            ? { id: request.id, result: methods[request.method] }
            : { id: request.id, error: "parity: no canned reply for " + request.method };
        Qt.callLater(() => { if (_linked && parser) parser.read(JSON.stringify(reply)); });
    }

    onConnectedChanged: {
        if (connected === _linked)
            return;
        if (connected && !_config()) {
            Qt.callLater(() => { connected = false; connectionStateChanged(); });
            return;
        }
        _linked = connected;
        Qt.callLater(() => connectionStateChanged());
    }
}
