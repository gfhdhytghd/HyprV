import QtQuick

Rectangle {
    id: root

    property var shellRoot: null
    property string expandedSsid: ""
    property string passwordText: ""

    radius: 19
    color: shellRoot ? (shellRoot.darkMode ? "#362c24" : "#f0dbc8") : "#f0dbc8"
    border.width: 1
    border.color: shellRoot ? (shellRoot.darkMode ? "#564435" : "#dcbda2") : "#dcbda2"
    implicitHeight: contentColumn.implicitHeight + 26
    antialiasing: true

    readonly property bool wifiEnabled: shellRoot ? shellRoot.wifiEnabled : false
    readonly property bool controlsAvailable: shellRoot ? (shellRoot.wifiDevicePresent || shellRoot.wifiCapabilityDetected) : false
    readonly property bool busy: shellRoot ? shellRoot.wifiActionBusy : false
    readonly property var networks: shellRoot && Array.isArray(shellRoot.wifiNetworks) ? shellRoot.wifiNetworks : []
    readonly property color titleColor: shellRoot ? (shellRoot.darkMode ? "#f8eadb" : "#5a4030") : "#5a4030"
    readonly property color mutedColor: shellRoot ? (shellRoot.darkMode ? "#d8bea7" : "#8b6a53") : "#8b6a53"
    readonly property color chipFill: shellRoot ? (shellRoot.darkMode ? "#43362a" : "#f7ebdf") : "#f7ebdf"
    readonly property color chipStroke: shellRoot ? (shellRoot.darkMode ? "#64513f" : "#d9baa0") : "#d9baa0"
    readonly property color fieldFill: shellRoot ? (shellRoot.darkMode ? "#2b231d" : "#fdf5ee") : "#fdf5ee"

    component InlineChip: Rectangle {
        id: chip

        property string label: ""
        property bool enabled: true

        signal tapped()

        height: 30
        radius: 15
        implicitWidth: chipText.contentWidth + 22
        color: !enabled
            ? Qt.rgba(root.chipFill.r, root.chipFill.g, root.chipFill.b, 0.45)
            : (chipArea.pressed ? Qt.darker(root.chipFill, 1.05) : (chipArea.containsMouse ? Qt.lighter(root.chipFill, 1.04) : root.chipFill))
        border.width: 1
        border.color: root.chipStroke
        opacity: enabled ? 1 : 0.56
        antialiasing: true

        Text {
            id: chipText

            anchors.centerIn: parent
            text: chip.label
            color: root.titleColor
            font.family: root.shellRoot ? root.shellRoot.baseFont : "JetBrainsMono Nerd Font"
            font.pixelSize: 11
            font.weight: Font.Bold
            renderType: Text.NativeRendering
        }

        MouseArea {
            id: chipArea

            anchors.fill: parent
            enabled: chip.enabled
            hoverEnabled: true
            cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
            onClicked: chip.tapped()
        }
    }

    function networkMeta(network) {
        const parts = [];
        if (network.active) {
            parts.push("Connected");
        } else if (network.known) {
            parts.push("Saved");
        }
        if (network.security) {
            parts.push(network.security);
        } else {
            parts.push("Open");
        }
        parts.push((Number(network.signal) || 0) + "%");
        return parts.join("  •  ");
    }

    function activateNetwork(network) {
        if (!shellRoot || busy) {
            return;
        }
        if (network.active) {
            expandedSsid = "";
            passwordText = "";
            shellRoot.wifiDisconnect();
            return;
        }
        if (network.secure && !network.known && expandedSsid !== network.ssid) {
            expandedSsid = network.ssid;
            passwordText = "";
            return;
        }
        shellRoot.wifiConnect(network.ssid || "", network.secure && !network.known ? passwordText : "", network.security || "");
        expandedSsid = "";
        passwordText = "";
    }

    Column {
        id: contentColumn

        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: 13
        spacing: 12

        Row {
            width: parent.width
            spacing: 8

            Text {
                width: parent.width - actionRow.implicitWidth - 8
                text: !controlsAvailable
                    ? "No wireless adapter detected"
                    : (wifiEnabled ? "Visible networks" : "Wi-Fi is turned off")
                color: root.titleColor
                font.family: root.shellRoot ? root.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                font.pixelSize: 12
                font.weight: Font.Bold
                renderType: Text.NativeRendering
                wrapMode: Text.WordWrap
            }

            Row {
                id: actionRow

                spacing: 6

                InlineChip {
                    label: "Scan"
                    enabled: root.shellRoot && wifiEnabled && !busy
                    onTapped: root.shellRoot.wifiRescan()
                }

                InlineChip {
                    label: "Manage"
                    enabled: !!root.shellRoot
                    onTapped: root.shellRoot.openWifiManager()
                }
            }
        }

        Text {
            width: parent.width
            visible: root.shellRoot && root.shellRoot.wifiActionMessage.length > 0
            text: root.shellRoot ? root.shellRoot.wifiActionMessage : ""
            color: root.mutedColor
            font.family: root.shellRoot ? root.shellRoot.baseFont : "JetBrainsMono Nerd Font"
            font.pixelSize: 11
            font.weight: Font.Medium
            renderType: Text.NativeRendering
            wrapMode: Text.WordWrap
        }

        Text {
            width: parent.width
            visible: !wifiEnabled || networks.length === 0
            text: !wifiEnabled ? "Enable Wi-Fi to scan nearby networks." : "No visible networks right now."
            color: root.mutedColor
            font.family: root.shellRoot ? root.shellRoot.baseFont : "JetBrainsMono Nerd Font"
            font.pixelSize: 11
            font.weight: Font.Medium
            renderType: Text.NativeRendering
            wrapMode: Text.WordWrap
        }

        Column {
            width: parent.width
            spacing: 8
            visible: wifiEnabled && networks.length > 0

            Repeater {
                model: root.networks

                delegate: Rectangle {
                    required property var modelData

                    width: parent.width
                    radius: 19
                    color: networkArea.pressed
                        ? Qt.darker(root.chipFill, 1.04)
                        : (networkArea.containsMouse ? Qt.lighter(root.chipFill, 1.03) : root.chipFill)
                    border.width: 1
                    border.color: root.chipStroke
                    implicitHeight: cardColumn.implicitHeight + 16
                    antialiasing: true

                    Column {
                        id: cardColumn

                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.margins: 10
                        spacing: 8

                        Row {
                            width: parent.width
                            spacing: 8

                            Text {
                                width: parent.width - actionText.width - 8
                                text: modelData.ssid || "Hidden network"
                                color: root.titleColor
                                font.family: root.shellRoot ? root.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                                font.pixelSize: 12
                                font.weight: Font.Bold
                                renderType: Text.NativeRendering
                                elide: Text.ElideRight
                            }

                            Text {
                                id: actionText

                                text: modelData.active ? "Disconnect" : "Connect"
                                color: root.mutedColor
                                font.family: root.shellRoot ? root.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                                font.pixelSize: 10
                                font.weight: Font.Bold
                                renderType: Text.NativeRendering
                            }
                        }

                        Text {
                            width: parent.width
                            text: root.networkMeta(modelData)
                            color: root.mutedColor
                            font.family: root.shellRoot ? root.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                            font.pixelSize: 10
                            font.weight: Font.Medium
                            renderType: Text.NativeRendering
                            wrapMode: Text.WordWrap
                        }

                        Column {
                            width: parent.width
                            spacing: 8
                            visible: modelData.secure && !modelData.known && root.expandedSsid === modelData.ssid

                            Rectangle {
                                width: parent.width
                                height: 36
                                radius: 14
                                color: root.fieldFill
                                border.width: 1
                                border.color: root.chipStroke

                                TextInput {
                                    id: passwordInput

                                    anchors.fill: parent
                                    anchors.margins: 10
                                    text: root.passwordText
                                    color: root.titleColor
                                    font.family: root.shellRoot ? root.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                                    font.pixelSize: 11
                                    font.weight: Font.Medium
                                    echoMode: TextInput.Password
                                    selectByMouse: true
                                    onTextChanged: root.passwordText = text
                                }

                                Text {
                                    anchors.left: parent.left
                                    anchors.leftMargin: 10
                                    anchors.verticalCenter: parent.verticalCenter
                                    visible: passwordInput.text.length === 0
                                    text: "Network password"
                                    color: root.mutedColor
                                    font.family: root.shellRoot ? root.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                                    font.pixelSize: 11
                                    font.weight: Font.Medium
                                    renderType: Text.NativeRendering
                                }
                            }

                            Row {
                                spacing: 8

                                InlineChip {
                                    label: "Connect"
                                    enabled: !root.busy && root.passwordText.length > 0
                                    onTapped: root.activateNetwork(modelData)
                                }

                                InlineChip {
                                    label: "Cancel"
                                    enabled: !root.busy
                                    onTapped: {
                                        root.expandedSsid = "";
                                        root.passwordText = "";
                                    }
                                }
                            }
                        }
                    }

                    MouseArea {
                        id: networkArea

                        anchors.fill: parent
                        enabled: !root.busy && !(modelData.secure && !modelData.known && root.expandedSsid === modelData.ssid)
                        hoverEnabled: true
                        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                        onClicked: root.activateNetwork(modelData)
                    }
                }
            }
        }
    }
}
