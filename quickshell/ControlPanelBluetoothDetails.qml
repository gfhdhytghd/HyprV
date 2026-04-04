import QtQuick

Rectangle {
    id: root

    property var shellRoot: null
    property bool useExternalPanelBackground: false

    signal closeRequested()

    readonly property bool bluetoothPresent: shellRoot ? shellRoot.bluetoothPresent : false
    readonly property bool bluetoothEnabled: shellRoot ? shellRoot.bluetoothEnabled : false
    readonly property bool bluetoothDiscovering: shellRoot ? shellRoot.bluetoothDiscovering : false
    readonly property bool bluetoothPairable: shellRoot ? shellRoot.bluetoothPairable : false
    readonly property bool busy: shellRoot ? shellRoot.bluetoothActionBusy : false
    readonly property var devices: shellRoot && Array.isArray(shellRoot.bluetoothDevices) ? shellRoot.bluetoothDevices : []
    readonly property var connectedDevices: devices.filter(device => !!device.connected)
    readonly property int connectedCount: connectedDevices.length
    readonly property int pairedCount: devices.filter(device => !!device.paired).length
    readonly property color cardFill: shellRoot ? shellRoot.withAlpha(shellRoot.darkMode ? "#ffffff" : "#ffffff", shellRoot.darkMode ? 0.07 : 0.22) : "#2a2a2a"
    readonly property color cardStrongFill: shellRoot ? shellRoot.withAlpha(shellRoot.darkMode ? "#ffffff" : "#ffffff", shellRoot.darkMode ? 0.11 : 0.3) : "#303030"
    readonly property color cardStroke: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.12 : 0.08) : "#454545"
    readonly property color accentFill: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.1 : 0.08) : "#445566"
    readonly property color accentStroke: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.18 : 0.14) : "#667788"
    readonly property color mutedText: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, 0.68) : "#b0b0b0"
    readonly property color panelFill: shellRoot ? shellRoot.withAlpha(shellRoot.darkMode ? "#101214" : "#ffffff", shellRoot.darkMode ? 0.42 : 0.28) : "#202020"
    readonly property color panelStroke: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.14 : 0.1) : "#3a3a3a"
    readonly property real panelSurfaceOpacity: 0.82
    readonly property int pageSpacing: 10
    readonly property int panelPadding: 10
    readonly property int innerPadding: 10
    readonly property int innerRadius: 9
    readonly property int maxDeviceListHeight: 420
    readonly property string statusLabel: {
        if (!bluetoothPresent) {
            return "No BT";
        }
        if (!bluetoothEnabled) {
            return "Off";
        }
        if (connectedCount > 0) {
            return "Online";
        }
        if (bluetoothDiscovering) {
            return "Scan";
        }
        return "Ready";
    }
    readonly property string connectionSummary: {
        if (!bluetoothPresent) {
            return "No Bluetooth adapter detected";
        }
        if (!bluetoothEnabled) {
            return "Bluetooth is turned off";
        }
        if (connectedCount > 1) {
            return connectedCount + " Bluetooth devices connected";
        }
        if (connectedCount === 1) {
            return connectedDevices[0].name || connectedDevices[0].address || "Bluetooth connected";
        }
        if (bluetoothDiscovering) {
            return "Scanning nearby Bluetooth devices";
        }
        if (pairedCount > 0) {
            return pairedCount + " paired Bluetooth devices";
        }
        return "Bluetooth ready";
    }
    readonly property string detailText: {
        if (!bluetoothPresent) {
            return "No Bluetooth controller was detected";
        }
        if (!bluetoothEnabled) {
            return "Turn Bluetooth on to discover and connect devices";
        }
        if (connectedCount > 0) {
            return connectedCount === 1
                ? "Connected and ready for audio or input"
                : connectedCount + " active Bluetooth connections";
        }
        if (bluetoothDiscovering) {
            return "Searching for nearby devices";
        }
        if (pairedCount > 0) {
            return pairedCount + " saved devices available";
        }
        return bluetoothPairable ? "Pairable and ready for nearby devices" : "Bluetooth adapter available";
    }

    radius: 19
    color: useExternalPanelBackground ? "transparent" : panelFill
    border.width: useExternalPanelBackground ? 0 : 1
    border.color: useExternalPanelBackground ? "transparent" : panelStroke
    antialiasing: true
    implicitHeight: contentColumn.implicitHeight + panelPadding * 2

    function panelColor(colorValue) {
        return Qt.rgba(colorValue.r, colorValue.g, colorValue.b, colorValue.a * panelSurfaceOpacity);
    }

    function deviceGlyph(device) {
        const icon = (device.icon || "").toLowerCase();
        if (icon.indexOf("audio-headset") >= 0 || icon.indexOf("audio-headphones") >= 0 || icon.indexOf("audio-card") >= 0 || icon.indexOf("audio-speakers") >= 0) {
            return "󰋋";
        }
        if (icon.indexOf("input-mouse") >= 0) {
            return "󰍽";
        }
        if (icon.indexOf("input-keyboard") >= 0) {
            return "󰌌";
        }
        if (icon.indexOf("phone") >= 0) {
            return "󰏲";
        }
        if (icon.indexOf("camera") >= 0) {
            return "󰄀";
        }
        if (icon.indexOf("printer") >= 0) {
            return "󰐪";
        }
        if (icon.indexOf("computer") >= 0) {
            return "󰌢";
        }
        return bluetoothEnabled ? "󰂯" : "󰂲";
    }

    function deviceMeta(device) {
        const parts = [];
        if (device.connected) {
            parts.push("Connected");
        } else if (device.paired) {
            parts.push("Paired");
        } else {
            parts.push("Available");
        }
        if (device.trusted) {
            parts.push("Trusted");
        }
        if (device.blocked) {
            parts.push("Blocked");
        }
        if (device.rssi !== null && device.rssi !== undefined) {
            parts.push(device.rssi + " dBm");
        }
        return parts.join("  •  ");
    }

    function deviceActionIcon(device) {
        if (device.connected) {
            return "";
        }
        return "";
    }

    function activateDevice(device) {
        if (!shellRoot || busy) {
            return;
        }
        if (device.connected) {
            shellRoot.bluetoothDisconnect(device.address, device.name || "");
            return;
        }
        shellRoot.bluetoothConnect(device.address, !!device.paired, device.name || "");
    }

    Column {
        id: contentColumn

        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: root.panelPadding
        spacing: root.pageSpacing

        Item {
            width: parent.width
            height: 36

            Text {
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                text: "Bluetooth"
                color: root.shellRoot ? root.shellRoot.primaryText : "white"
                font.family: root.shellRoot ? root.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                font.pixelSize: 17
                font.weight: Font.Bold
                renderType: Text.NativeRendering
            }

            WifiActionChip {
                x: parent.width - width
                anchors.verticalCenter: parent.verticalCenter
                shellRoot: root.shellRoot
                cornerRadius: root.innerRadius
                label: "Close"
                minimumWidth: 76
                fillColor: root.shellRoot ? root.shellRoot.withAlpha(root.shellRoot.primaryText, root.shellRoot.darkMode ? 0.08 : 0.12) : "#333333"
                strokeColor: root.cardStroke
                onClicked: root.closeRequested()
            }
        }

        Rectangle {
            width: parent.width
            implicitHeight: statusBody.implicitHeight + 24
            radius: root.innerRadius
            color: root.panelColor(root.connectedCount > 0 ? root.accentFill : root.cardStrongFill)
            border.width: 1
            border.color: root.connectedCount > 0 ? root.accentStroke : root.cardStroke

            Item {
                id: statusBody

                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: root.innerPadding
                implicitHeight: Math.max(statusInfo.implicitHeight, statusIcon.implicitHeight, statusPill.implicitHeight)

                Text {
                    id: statusIcon

                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.bluetoothEnabled ? "󰂯" : "󰂲"
                    color: root.shellRoot ? root.shellRoot.primaryText : "white"
                    font.family: root.shellRoot ? root.shellRoot.iconFont : "JetBrainsMono Nerd Font"
                    font.pixelSize: 24
                    font.weight: Font.Bold
                    renderType: Text.NativeRendering
                }

                WifiActionChip {
                    id: statusPill

                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    shellRoot: root.shellRoot
                    cornerRadius: root.innerRadius
                    label: root.statusLabel
                    disabled: true
                    minimumWidth: 72
                    fillColor: root.cardFill
                    foregroundColor: root.connectedCount > 0
                        ? (root.shellRoot ? root.shellRoot.launchColor : "white")
                        : (root.shellRoot ? root.shellRoot.primaryText : "white")
                    strokeColor: root.connectedCount > 0
                        ? (root.shellRoot ? root.shellRoot.withAlpha(root.shellRoot.launchColor, 0.18) : "#5176d2")
                        : root.cardStroke
                }

                Column {
                    id: statusInfo

                    anchors.left: statusIcon.right
                    anchors.leftMargin: root.innerPadding
                    anchors.right: statusPill.left
                    anchors.rightMargin: root.innerPadding
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 4

                    Text {
                        width: parent.width
                        text: root.connectionSummary
                        elide: Text.ElideRight
                        color: root.shellRoot ? root.shellRoot.primaryText : "white"
                        font.family: root.shellRoot ? root.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                        font.pixelSize: 14
                        font.weight: Font.Bold
                        renderType: Text.NativeRendering
                    }

                    Text {
                        width: parent.width
                        text: root.detailText
                        elide: Text.ElideRight
                        color: root.mutedText
                        font.family: root.shellRoot ? root.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                        font.pixelSize: 12
                        renderType: Text.NativeRendering
                    }
                }
            }
        }

        Row {
            width: parent.width
            spacing: 10

            WifiActionChip {
                shellRoot: root.shellRoot
                cornerRadius: root.innerRadius
                label: root.bluetoothEnabled ? "Turn Off" : "Turn On"
                minimumWidth: 96
                disabled: !root.shellRoot || !root.bluetoothPresent || root.busy
                fillColor: root.cardStrongFill
                foregroundColor: root.shellRoot ? root.shellRoot.launchColor : "white"
                strokeColor: root.shellRoot ? root.shellRoot.withAlpha(root.shellRoot.launchColor, 0.18) : "#5176d2"
                onClicked: root.shellRoot.bluetoothSetPower(!root.bluetoothEnabled)
            }

            WifiActionChip {
                shellRoot: root.shellRoot
                cornerRadius: root.innerRadius
                label: root.bluetoothDiscovering ? "Scanning" : "Scan"
                minimumWidth: 88
                disabled: !root.shellRoot || !root.bluetoothPresent || !root.bluetoothEnabled || root.busy
                fillColor: root.cardFill
                strokeColor: root.cardStroke
                onClicked: root.shellRoot.bluetoothScan()
            }

            WifiActionChip {
                shellRoot: root.shellRoot
                cornerRadius: root.innerRadius
                label: "Advanced"
                minimumWidth: 98
                disabled: !root.shellRoot
                fillColor: root.cardFill
                strokeColor: root.cardStroke
                onClicked: root.shellRoot.openBluetoothManager()
            }
        }

        Rectangle {
            width: parent.width
            visible: root.shellRoot && root.shellRoot.bluetoothActionMessage.length > 0
            implicitHeight: actionMessageLabel.implicitHeight + 18
            radius: root.innerRadius
            color: root.panelColor(root.cardFill)
            border.width: 1
            border.color: root.cardStroke

            Text {
                id: actionMessageLabel

                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: root.innerPadding
                text: root.shellRoot ? root.shellRoot.bluetoothActionMessage : ""
                color: root.shellRoot ? root.shellRoot.withAlpha(root.shellRoot.primaryText, 0.82) : "#d8d8d8"
                font.family: root.shellRoot ? root.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                font.pixelSize: 12
                wrapMode: Text.Wrap
                renderType: Text.NativeRendering
            }
        }

        Rectangle {
            width: parent.width
            visible: root.devices.length === 0
            implicitHeight: emptyState.implicitHeight + 26
            radius: root.innerRadius
            color: root.panelColor(root.cardFill)
            border.width: 1
            border.color: root.cardStroke

            Text {
                id: emptyState

                anchors.centerIn: parent
                width: parent.width - 28
                horizontalAlignment: Text.AlignHCenter
                text: !root.bluetoothPresent
                    ? "No Bluetooth adapter detected."
                    : (!root.bluetoothEnabled
                        ? "Turn Bluetooth on to scan for nearby devices."
                        : (root.bluetoothDiscovering
                            ? "Searching for nearby devices..."
                            : "No nearby or paired Bluetooth devices right now."))
                color: root.mutedText
                font.family: root.shellRoot ? root.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                font.pixelSize: 13
                wrapMode: Text.Wrap
                renderType: Text.NativeRendering
            }
        }

        Flickable {
            id: deviceList

            width: parent.width
            height: visible ? Math.min(contentHeight, root.maxDeviceListHeight) : 0
            contentHeight: deviceColumn.implicitHeight
            visible: root.devices.length > 0
            clip: true
            interactive: contentHeight > height
            boundsBehavior: Flickable.StopAtBounds

            Column {
                id: deviceColumn

                width: deviceList.width
                spacing: 10

                Repeater {
                    model: root.devices

                    delegate: Rectangle {
                        id: deviceCard

                        required property var modelData

                        width: deviceColumn.width
                        implicitHeight: deviceBody.implicitHeight + 20
                        radius: root.innerRadius
                        color: root.panelColor(modelData.connected ? root.accentFill : root.cardFill)
                        border.width: 1
                        border.color: modelData.connected ? root.accentStroke : root.cardStroke

                        Column {
                            id: deviceBody

                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.top: parent.top
                            anchors.margins: 10
                            spacing: 10

                            Item {
                                width: parent.width
                                implicitHeight: Math.max(deviceInfo.implicitHeight, actionChip.implicitHeight, deviceIcon.implicitHeight)

                                Text {
                                    id: deviceIcon

                                    anchors.left: parent.left
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: root.deviceGlyph(modelData)
                                    color: root.shellRoot ? root.shellRoot.primaryText : "white"
                                    font.family: root.shellRoot ? root.shellRoot.iconFont : "JetBrainsMono Nerd Font"
                                    font.pixelSize: 24
                                    font.weight: Font.Bold
                                    renderType: Text.NativeRendering
                                }

                                WifiActionChip {
                                    id: actionChip

                                    anchors.right: parent.right
                                    anchors.verticalCenter: parent.verticalCenter
                                    shellRoot: root.shellRoot
                                    cornerRadius: root.innerRadius
                                    label: ""
                                    iconLabel: root.deviceActionIcon(modelData)
                                    minimumWidth: 50
                                    disabled: !root.shellRoot || root.busy || (!root.bluetoothEnabled && !modelData.connected)
                                    fillColor: modelData.connected ? root.cardStrongFill : root.cardFill
                                    foregroundColor: modelData.connected
                                        ? (root.shellRoot ? root.shellRoot.criticalColor : "white")
                                        : (root.shellRoot ? root.shellRoot.launchColor : "white")
                                    strokeColor: modelData.connected
                                        ? (root.shellRoot ? root.shellRoot.withAlpha(root.shellRoot.criticalColor, 0.2) : "#a55454")
                                        : (root.shellRoot ? root.shellRoot.withAlpha(root.shellRoot.launchColor, 0.18) : "#5176d2")
                                    onClicked: root.activateDevice(modelData)
                                }

                                Column {
                                    id: deviceInfo

                                    anchors.left: deviceIcon.right
                                    anchors.leftMargin: root.innerPadding
                                    anchors.right: actionChip.left
                                    anchors.rightMargin: root.innerPadding
                                    anchors.verticalCenter: parent.verticalCenter
                                    spacing: 4

                                    Text {
                                        width: parent.width
                                        text: modelData.name || modelData.address
                                        elide: Text.ElideRight
                                        color: root.shellRoot ? root.shellRoot.primaryText : "white"
                                        font.family: root.shellRoot ? root.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                                        font.pixelSize: 14
                                        font.weight: Font.Bold
                                        renderType: Text.NativeRendering
                                    }

                                    Text {
                                        width: parent.width
                                        text: root.deviceMeta(modelData)
                                        elide: Text.ElideRight
                                        color: root.mutedText
                                        font.family: root.shellRoot ? root.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                                        font.pixelSize: 11
                                        renderType: Text.NativeRendering
                                    }
                                }
                            }

                        }
                    }
                }
            }
        }
    }
}
