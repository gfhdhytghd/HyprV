import QtQuick

QtObject {
    property var config: ({})
    readonly property string name: config.name || ""
    readonly property string deviceName: config.deviceName || config.name || ""
    readonly property string address: config.address || ""
    readonly property bool connected: !!config.connected
    readonly property bool paired: !!config.paired
    readonly property bool bonded: !!config.paired
    readonly property bool trusted: !!config.trusted
    readonly property bool blocked: !!config.blocked
    readonly property bool pairing: false
    readonly property string icon: config.icon || ""
    readonly property int state: connected ? 1 : 0
    readonly property bool batteryAvailable: config.battery !== undefined
    readonly property real battery: Number(config.battery || 0)
    function connect() { console.log("parity: bt connect", address); }
    function disconnect() {}
    function pair() {}
    function forget() {}
}
