pragma Singleton
import QtQuick
import ParityHost

// services.bluetooth: null (no adapter) or
//   { enabled, discovering, pairable, devices: [{name, deviceName, address, connected,
//     paired, trusted, blocked, icon, batteryAvailable, battery}] }
QtObject {
    readonly property var config: ParityState.service("bluetooth", null)
    readonly property QtObject defaultAdapter: config ? adapter : null
    readonly property var adapters: ({ values: config ? [adapter] : [] })
    readonly property var devices: adapter.devices

    property BluetoothAdapter adapter: BluetoothAdapter { config: Bluetooth.config || ({}) }
}
