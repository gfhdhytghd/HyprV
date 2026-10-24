import QtQuick
import ParityHost

// Contents come from fixture "files" (matched by path suffix). The theme file
// ui-state.env defaults to the theme being rendered.
QtObject {
    id: view
    property string path: ""
    property bool watchChanges: false
    property bool blockLoading: false
    property bool blockAllReads: false
    property bool blockWrites: false
    property bool printErrors: true
    property bool preload: true
    property string _text: ""
    property bool _completed: false

    signal loaded()
    signal loadFailed(int error)
    signal fileChanged()
    signal saved()
    signal saveFailed(int error)

    function text() { return _text; }
    function data() { return _text; }
    function setText(value) { _text = value; }
    function reload() { Qt.callLater(_load); }

    function _load() {
        if (!path)
            return;
        let content = ParityState.fileFor(path);
        if (content === null && path.endsWith("ui-state.env"))
            content = "theme=" + (Harness.dark ? "dark" : "light") + "\nvariant=v2\n";
        if (content === null) {
            loadFailed(2);
            return;
        }
        _text = content;
        loaded();
    }

    onPathChanged: if (_completed) reload()
    Component.onCompleted: { _completed = true; reload(); }
}
