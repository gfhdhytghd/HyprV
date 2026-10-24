pragma Singleton
import QtQuick
import ParityHost

// services.wifi.devices (laptop BarServices native path), each:
//   {name, type: "wifi"|"wired", connected, networks: [{name, signal (0..1), security
//    (WifiSecurityType key, e.g. "Wpa2Psk"), known, connected}]}
// Absent (desktop fixtures) -> no devices, as before.
QtObject {
    id: net
    readonly property int backend: 1
    readonly property bool wifiEnabled: ParityState.service("wifi.enabled", true)
    readonly property bool wifiHardwareEnabled: ParityState.service("wifi.hardwareEnabled", true)
    readonly property var _raw: ParityState.service("wifi.devices", [])
    property var _objects: []
    readonly property var securityKeys: ["Wpa3SuiteB192", "Sae", "Wpa2Eap", "Wpa2Psk", "WpaEap", "WpaPsk",
        "StaticWep", "DynamicWep", "Leap", "Owe", "Open", "Unknown"]
    readonly property var devices: ({ values: _objects })

    property Component deviceComponent: Component {
        QtObject {
            property string name: ""
            property int type: 0
            property bool connected: false
            property bool scannerEnabled: false
            property var networks: ({ values: [] })
            onScannerEnabledChanged: console.log("parity: wifi scannerEnabled", name, scannerEnabled)
        }
    }
    property Component networkComponent: Component {
        QtObject {
            property string name: ""
            property real signalStrength: 0
            property int security: 10
            property bool known: false
            property bool connected: false
        }
    }

    function _rebuild() {
        for (const d of _objects) d.destroy();
        const built = [];
        for (const d of _raw || []) {
            const nets = (d.networks || []).map(n => networkComponent.createObject(net, {
                name: n.name || "", signalStrength: Number(n.signal || 0),
                security: Math.max(0, net.securityKeys.indexOf(n.security || "Open")),
                known: !!n.known, connected: !!n.connected }));
            built.push(deviceComponent.createObject(net, {
                name: d.name || "", type: d.type === "wired" ? 2 : 1, connected: !!d.connected,
                networks: { values: nets } }));
        }
        _objects = built;
    }
    on_RawChanged: _rebuild()
    Component.onCompleted: _rebuild()
}
