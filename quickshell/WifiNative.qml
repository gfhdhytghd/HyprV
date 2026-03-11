import QtQuick
import Quickshell.Networking

WifiIndicator {
    id: root

    property var shellRoot: null

    readonly property var wifiDevice: {
        const devices = Array.from(Networking.devices?.values || []);
        return devices.find(device => device.type === DeviceType.Wifi) || null;
    }
    readonly property var activeNetwork: {
        const networks = Array.from(wifiDevice?.networks?.values || []);
        return networks.find(network => network.connected || network.state === NetworkState.Connecting || network.state === NetworkState.Connected) || null;
    }
    readonly property bool wifiEnabled: Networking.wifiEnabled && Networking.wifiHardwareEnabled
    readonly property bool wifiConnectedState: !!activeNetwork && activeNetwork.connected
    readonly property real wifiStrength: activeNetwork?.signalStrength || 0

    available: !!wifiDevice
    iconSource: shellRoot ? shellRoot.wifiIconSource(wifiEnabled, wifiConnectedState, wifiStrength) : ""
    fallbackLabel: wifiConnectedState ? "󰖩" : "󰤮"

    onLeftClicked: {
        if (shellRoot) {
            shellRoot.openWifiManager();
        }
    }

    onRightClicked: Networking.wifiEnabled = !Networking.wifiEnabled
}
