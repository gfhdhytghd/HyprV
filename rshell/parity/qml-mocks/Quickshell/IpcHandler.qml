import QtQuick
import ParityHost

// Functions declared on an instance are invoked by fixture steps
// {"ipc": ["target", "function", args...]} through ParityState.callIpc.
QtObject {
    property string target: ""
    property bool enabled: true
    Component.onCompleted: ParityState.registerIpc(this)
}
