import QtQuick

QtObject {
    id: adapter
    property var config: ({})
    readonly property bool enabled: !!config.enabled
    readonly property bool discovering: !!config.discovering
    readonly property bool pairable: !!config.pairable
    readonly property bool discoverable: false
    readonly property int state: enabled ? 1 : 0
    readonly property string name: "parity-hci0"
    readonly property var devices: ({ values: deviceObjects })
    property var deviceObjects: []
    property Component deviceComponent: Component { BluetoothDevice {} }

    onConfigChanged: {
        for (const d of deviceObjects)
            d.destroy();
        deviceObjects = (config.devices || []).map(d => deviceComponent.createObject(adapter, { config: d }));
    }
}
