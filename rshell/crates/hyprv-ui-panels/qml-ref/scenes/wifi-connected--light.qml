import QtQuick
import ".."
import "../../../../../quickshell"

Item {
    id: root
    width: 384; height: panelColumn.implicitHeight + 20
    property var shellRoot: sh
    property var parentWindow: null
    property bool popupVisible: true
    property bool _scanRegistered: false
    Component.onDestruction: if (_scanRegistered && shellRoot) shellRoot.openWifiPanelCount--
    property string expandedSsid: ""
    property string passwordText: ""
    property var displayedNetworks: []
    property double displayedNetworksTimestamp: 0

    readonly property bool wifiEnabled: shellRoot ? shellRoot.wifiRadioEnabled : false
    readonly property bool wifiConnectedState: shellRoot ? (shellRoot.wifiConnectionActive || shellRoot.wifiConnected) : false
    readonly property bool networkConnectedState: shellRoot ? shellRoot.networkConnected : false
    readonly property bool wiredConnectedState: shellRoot ? shellRoot.wiredConnectionActive : false
    readonly property bool otherConnectedState: shellRoot ? shellRoot.otherConnectionActive : false
    readonly property bool wifiControlsAvailable: shellRoot ? (shellRoot.wifiDevicePresent || shellRoot.wifiCapabilityDetected) : false
    readonly property real wifiStrength: shellRoot ? shellRoot.wifiSignalStrength : 0
    readonly property var liveNetworks: shellRoot ? shellRoot.wifiNetworks : []
    readonly property var networks: displayedNetworks
    readonly property color glassFill: shellRoot ? shellRoot.withAlpha(shellRoot.darkMode ? "#101214" : "#ffffff", shellRoot.darkMode ? 0.42 : 0.28) : "#202020"
    readonly property color glassStroke: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.14 : 0.1) : "#3a3a3a"
    readonly property color cardFill: shellRoot ? shellRoot.withAlpha(shellRoot.darkMode ? "#ffffff" : "#ffffff", shellRoot.darkMode ? 0.07 : 0.22) : "#2a2a2a"
    readonly property color cardStrongFill: shellRoot ? shellRoot.withAlpha(shellRoot.darkMode ? "#ffffff" : "#ffffff", shellRoot.darkMode ? 0.11 : 0.3) : "#303030"
    readonly property color cardStroke: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.12 : 0.08) : "#454545"
    readonly property color accentFill: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.1 : 0.08) : "#445566"
    readonly property color accentStroke: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.18 : 0.14) : "#667788"
    readonly property color mutedText: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, 0.68) : "#b0b0b0"
    readonly property color softText: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, 0.44) : "#8a8a8a"
    readonly property color inputFill: shellRoot ? shellRoot.withAlpha(shellRoot.darkMode ? "#000000" : "#ffffff", shellRoot.darkMode ? 0.2 : 0.34) : "#1d1d1d"
    readonly property real panelSurfaceOpacity: 0.82
    readonly property int popupPanelWidth: 384
    readonly property int popupRightMargin: 10
    readonly property int popupScreenMargin: 8
    readonly property int maxVisibleNetworks: 5
    readonly property int panelVerticalPadding: 20
    readonly property int panelSectionSpacing: 10
    readonly property int panelPadding: 10
    readonly property int innerPadding: 10
    readonly property int innerRadius: 9
    readonly property string trayIconUrl: shellRoot ? shellRoot.networkTrayIconSource() : ""
    readonly property string connectionSummary: {
        if (!shellRoot) {
            return "";
        }
        if (wiredConnectedState) {
            return "Ethernet connected";
        }
        if (otherConnectedState) {
            return "Connected via " + (shellRoot.defaultInterface || "network");
        }
        if (!wifiControlsAvailable) {
            return "No wireless device detected";
        }
        if (!shellRoot.wifiHardwareEnabled) {
            return "Hardware blocked";
        }
        if (!wifiEnabled) {
            return "Wi-Fi disabled";
        }
        if (wifiConnectedState) {
            return (shellRoot.wifiSsid || "Wi-Fi connected") + "  " + Math.round(wifiStrength * 100) + "%";
        }
        return "Not connected";
    }


    function updatePopupAnchor() {
        if (popup.visible) {
            popup.updatePopupPosition();
        }
    }

    function panelColor(colorValue) {
        return Qt.rgba(colorValue.r, colorValue.g, colorValue.b, colorValue.a * panelSurfaceOpacity);
    }

    function cloneNetwork(network) {
        return {
            active: !!network.active,
            ssid: network.ssid || "",
            signal: Number(network.signal) || 0,
            security: network.security || "",
            secure: !!network.secure,
            known: !!network.known,
            enterprise: !!network.enterprise
        };
    }

    function mergeNetworkLists(primary, fallback) {
        const bySsid = {};
        const merged = [];

        for (let i = 0; i < primary.length; i++) {
            const network = cloneNetwork(primary[i]);
            if (!network.ssid || bySsid[network.ssid]) {
                continue;
            }
            bySsid[network.ssid] = true;
            merged.push(network);
        }

        for (let i = 0; i < fallback.length; i++) {
            const network = cloneNetwork(fallback[i]);
            if (!network.ssid || bySsid[network.ssid]) {
                continue;
            }
            bySsid[network.ssid] = true;
            merged.push(network);
        }

        return merged;
    }

    function syncNetworkList(force) {
        const source = Array.isArray(root.liveNetworks) ? root.liveNetworks : [];
        const nextNetworks = source.map(cloneNetwork);
        const cacheAgeMs = Date.now() - displayedNetworksTimestamp;
        const popupShouldHoldSnapshot = root.popupVisible
            && root.wifiEnabled
            && (!root.shellRoot || root.shellRoot.wifiHardwareEnabled);

        if (!force && expandedSsid.length > 0) {
            const expandedNetworkStillAvailable = source.some(network => (network.ssid || "") === expandedSsid);
            if (expandedNetworkStillAvailable) {
                return;
            }
        }

        if (nextNetworks.length > 0) {
            const shouldMergeWithDisplayed = popupShouldHoldSnapshot
                && displayedNetworks.length > 0
                && nextNetworks.length < displayedNetworks.length
                && cacheAgeMs < 30000;

            displayedNetworks = shouldMergeWithDisplayed
                ? mergeNetworkLists(nextNetworks, displayedNetworks)
                : nextNetworks;
            displayedNetworksTimestamp = Date.now();
        } else if (!(popupShouldHoldSnapshot && displayedNetworks.length > 0 && cacheAgeMs < 30000)) {
            displayedNetworks = [];
            displayedNetworksTimestamp = Date.now();
        }

        if (expandedSsid.length > 0) {
            const expandedNetworkStillAvailable = displayedNetworks.some(network =>
                network.ssid === expandedSsid && network.secure && !network.known
            );
            if (!expandedNetworkStillAvailable) {
                expandedSsid = "";
                passwordText = "";
            }
        }
    }

    function openPopup() {
        if (shellRoot && (!Array.isArray(root.liveNetworks) || root.liveNetworks.length === 0)) {
            shellRoot.refreshWifiStatus();
        }
        popupVisible = true;
        if (popup.animatingClose) {
            popup.animatingClose = false;
            updatePopupAnchor();
            popupChrome.playOpenAnimation();
            return;
        }
        updatePopupAnchor();
    }

    function closePopup() {
        if (!popup.visible || popup.animatingClose) {
            popupVisible = false;
            return;
        }
        popup.animatingClose = true;
        popupVisible = false;
        popupChrome.playCloseAnimation();
    }

    function togglePopup() {
        if (popupVisible && !popup.animatingClose) {
            closePopup();
        } else {
            openPopup();
        }
    }

    function networkMeta(network) {
        const parts = [];
        if (network.active) {
            parts.push("Connected");
        } else if (network.known) {
            parts.push("Saved");
        }
        if (network.enterprise) {
            parts.push("802.1X");
        } else if (network.security) {
            parts.push(network.security);
        } else {
            parts.push("Open");
        }
        parts.push(network.signal + "%");
        return parts.join("  •  ");
    }

    function activateNetwork(network) {
        if (!shellRoot || shellRoot.wifiActionBusy) {
            return;
        }
        if (network.active) {
            expandedSsid = "";
            passwordText = "";
            shellRoot.wifiDisconnect();
            return;
        }
        if (network.enterprise && !network.known) {
            shellRoot.wifiActionMessage = "802.1X networks need a saved profile. Open the editor for first-time setup.";
            shellRoot.openWifiManager();
            return;
        }
        if (network.secure && !network.known && expandedSsid === network.ssid && passwordText.length === 0) {
            return;
        }
        if (network.secure && !network.known && expandedSsid !== network.ssid) {
            expandedSsid = network.ssid;
            passwordText = "";
            return;
        }
        shellRoot.wifiConnect(network.ssid, network.secure && !network.known ? passwordText : "", network.security || "");
        expandedSsid = "";
        passwordText = "";
    }

    onShellRootChanged: syncNetworkList(true)

    onPopupVisibleChanged: {
        if (shellRoot && popupVisible !== _scanRegistered) {
            shellRoot.openWifiPanelCount += popupVisible ? 1 : -1;
            _scanRegistered = popupVisible;
        }
        if (popupVisible) {
            syncNetworkList(true);
            return;
        }
        expandedSsid = "";
        passwordText = "";
        syncNetworkList(true);
    }

    onExpandedSsidChanged: {
        if (expandedSsid.length === 0) {
            passwordText = "";
            syncNetworkList(true);
        }
    }

    QtObject {
        id: sh
        property bool darkMode: false
        readonly property color primaryText: darkMode ? "#cdd6f4" : "#2b2b2c"
        readonly property color launchColor: darkMode ? "#89b4fa" : "#407cdd"
        readonly property color criticalColor: "#e92d4d"
        readonly property color usageLowColor: darkMode ? "#7ad48b" : "#2f9e44"
        readonly property color usageMediumColor: darkMode ? "#f2d36b" : "#c99700"
        readonly property color moduleBackground: withAlpha(darkMode ? "#1e1e2e" : "#e7e7ec", 0.8)
        readonly property string baseFont: "JetBrainsMono Nerd Font"
        readonly property string iconFont: "JetBrainsMono Nerd Font"
        readonly property string configDir: "/home/wilf/.config/HyprV"
        property string fluentLightIconDir: "/home/wilf/.local/share/icons/Fluent-light"
        property string fluentDarkIconDir: "/home/wilf/.local/share/icons/Fluent-dark"
        property string fluentBaseIconDir: "/home/wilf/.local/share/icons/Fluent"
        property string defaultInterface: "wlan0"
        readonly property bool networkConnected: defaultInterface.length > 0
        readonly property string activeNetworkType: networkTypeForInterface(defaultInterface)
        readonly property bool wifiConnectionActive: activeNetworkType === "wifi"
        readonly property bool wiredConnectionActive: activeNetworkType === "wired"
        readonly property bool otherConnectionActive: activeNetworkType === "other"
        property bool wifiDevicePresent: true
        property bool wifiCapabilityDetected: true
        property bool wifiRadioEnabled: true
        property bool wifiHardwareEnabled: true
        property bool wifiConnected: true
        property real wifiSignalStrength: 0.7799999713897705
        property string wifiSsid: "HomeNet-5G"
        property bool wifiSecure: true
        property string wifiInterface: "wlan0"
        property var wifiNetworks: [{"active": true, "enterprise": false, "known": true, "secure": true, "security": "WPA2", "signal": 78, "ssid": "HomeNet-5G"}, {"active": false, "enterprise": false, "known": true, "secure": true, "security": "WPA2", "signal": 61, "ssid": "Office"}, {"active": false, "enterprise": false, "known": false, "secure": false, "security": "", "signal": 44, "ssid": "Cafe Free WiFi"}, {"active": false, "enterprise": false, "known": false, "secure": true, "security": "WPA3", "signal": 23, "ssid": "Neighbour"}]
        property bool wifiActionBusy: false
        property string wifiActionMessage: ""
        property int openWifiPanelCount: 0
        function withAlpha(colorString, alpha) { const color = Qt.color(colorString); return Qt.rgba(color.r, color.g, color.b, alpha); }
        function refreshWifiStatus() {}
        function wifiDisconnect() {}
        function wifiConnect(a, b, c) {}
        function wifiSetRadio(v) {}
        function wifiRescan() {}
        function openWifiManager() {}
    function isWifiInterfaceName(name) {
        const iface = (name || "").toLowerCase();
        return iface.startsWith("wl") || iface.startsWith("wlan") || iface.startsWith("wifi");
    }
    function isWiredInterfaceName(name) {
        const iface = (name || "").toLowerCase();
        return iface.startsWith("en") || iface.startsWith("eth");
    }
    function networkTypeForInterface(name) {
        if (isWifiInterfaceName(name)) {
            return "wifi";
        }
        if (isWiredInterfaceName(name)) {
            return "wired";
        }
        return name ? "other" : "offline";
    }
    function fileUrl(path) {
        return path ? "file://" + path : "";
    }
    function wifiSignalBucket(signalPercent) {
        if (signalPercent < 20) {
            return "0";
        }
        if (signalPercent < 40) {
            return "25";
        }
        if (signalPercent < 60) {
            return "50";
        }
        if (signalPercent < 80) {
            return "75";
        }
        return "100";
    }
    function fluentWifiIconSource(iconName, useDarkVariant) {
        const themeDir = useDarkVariant ? (fluentDarkIconDir || fluentBaseIconDir) : (fluentLightIconDir || fluentBaseIconDir);
        if (!themeDir || !iconName) {
            return "";
        }
        const relativePath = useDarkVariant
        ? "symbolic/status/" + iconName + "-symbolic.svg"
        : "24/panel/" + iconName + ".svg";
        return fileUrl(themeDir + "/" + relativePath);
    }
    function wifiTrayIconSource(enabled, hardwareEnabled, connected, strength, secure) {
        const signalPercent = Math.round((strength || 0) * 100);
        if (darkMode) {
            if (!hardwareEnabled) {
                return fluentWifiIconSource("network-wireless-hardware-disabled", true);
            }
            if (!enabled) {
                return fluentWifiIconSource("network-wireless-disabled", true);
            }
            if (!connected) {
                return fluentWifiIconSource("network-wireless-disconnected", true);
            }
            return fluentWifiIconSource("nm-signal-" + wifiSignalBucket(signalPercent) + (secure ? "-secure" : ""), true);
        }

        if (!hardwareEnabled) {
            return fluentWifiIconSource("network-wireless-offline", false);
        }
        if (!enabled) {
            return fluentWifiIconSource("network-wireless-off", false);
        }
        if (!connected) {
            return fluentWifiIconSource("network-wireless-disconnected", false);
        }
        return fluentWifiIconSource("nm-signal-" + wifiSignalBucket(signalPercent) + (secure ? "-secure" : ""), false);
    }
    function wiredTrayIconSource(connected) {
        if (darkMode) {
            return fluentWifiIconSource(connected ? "network-wired" : "network-wired-disconnected", true);
        }
        return fileUrl(configDir + "/quickshell/assets/tray/" + (connected ? "network-wired-light.svg" : "network-wired-offline-light.svg"));
    }
    function networkTrayIconSource() {
        if (wiredConnectionActive || otherConnectionActive) {
            return wiredTrayIconSource(true);
        }
        if (wifiConnectionActive) {
            return wifiTrayIconSource(true, wifiHardwareEnabled, true, wifiSignalStrength, wifiSecure);
        }
        return wifiTrayIconSource(wifiRadioEnabled, wifiHardwareEnabled, wifiConnected, wifiSignalStrength, wifiSecure);
    }
    function humanRate(bytesPerSecond) {
        const value = Math.max(0, bytesPerSecond || 0);
        if (value < 1024) {
            return Math.round(value) + " B/s";
        }
        if (value < 1024 * 1024) {
            return (value / 1024).toFixed(1) + "kB/s";
        }
        if (value < 1024 * 1024 * 1024) {
            return (value / (1024 * 1024)).toFixed(1) + "MB/s";
        }
        return (value / (1024 * 1024 * 1024)).toFixed(1) + "GB/s";
    }
    function wifiListIconSource(signalPercent, secure) {
        return fluentWifiIconSource("nm-signal-" + wifiSignalBucket(signalPercent) + (secure ? "-secure" : ""), darkMode);
    }
    function wifiSignalGlyph(signalPercent) {
        if (signalPercent < 20) {
            return "󰤯";
        }
        if (signalPercent < 40) {
            return "󰤟";
        }
        if (signalPercent < 60) {
            return "󰤢";
        }
        if (signalPercent < 80) {
            return "󰤥";
        }
        return "󰤨";
    }
    function wifiTrayGlyph(enabled, connected, strength) {
        if (!enabled) {
            return "󰤭";
        }
        if (!connected) {
            return "󰤮";
        }
        return wifiSignalGlyph(Math.round((strength || 0) * 100));
    }
    function networkTrayGlyph() {
        if (wiredConnectionActive || otherConnectionActive) {
            return "󰈀";
        }
        if (wifiConnectionActive) {
            return wifiTrayGlyph(true, true, wifiSignalStrength);
        }
        return wifiTrayGlyph(wifiRadioEnabled, wifiConnected, wifiSignalStrength);
    }
    }
    QtObject { id: popup; property bool visible: true; property bool animatingClose: false; property bool openAnimationPending: false }
    Timer { id: popupOpenTimer }
    AnimatedGlassPanel {
        width: parent.width
        fullPanelHeight: parent.height
        fillColor: sh.withAlpha(sh.darkMode ? "#101214" : "#ffffff", sh.darkMode ? 0.42 : 0.28)
        strokeColor: sh.withAlpha(sh.primaryText, sh.darkMode ? 0.14 : 0.10)
        shadowColor: "transparent"
        devicePixelRatio: 2
        revealHeight: fullPanelHeight
        contentOpacity: 1
        contentOffset: 0
    }
    Item {
        id: popupChrome
        anchors.fill: parent
                Column {
                    id: panelColumn

                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.margins: root.panelPadding
                    spacing: root.panelSectionSpacing
                    onImplicitHeightChanged: {
                        if (popup.openAnimationPending) {
                            popupOpenTimer.restart();
                        }
                    }

                    Item {
                        id: headerRow

                        width: parent.width
                        height: 36

                        Text {
                            anchors.left: parent.left
                            anchors.verticalCenter: parent.verticalCenter
                            text: "Network"
                            color: root.shellRoot ? root.shellRoot.primaryText : "white"
                            font.family: root.shellRoot ? root.shellRoot.baseFont : ""
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
                            onClicked: root.closePopup()
                        }
                    }

                Rectangle {
                    id: statusCard

                    width: parent.width
                    implicitHeight: statusBody.implicitHeight + 24
                    radius: root.innerRadius
                    color: root.panelColor(root.networkConnectedState ? root.accentFill : root.cardStrongFill)
                    border.width: 1
                    border.color: root.networkConnectedState ? root.accentStroke : root.cardStroke

                    Item {
                        id: statusBody

                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.margins: root.innerPadding
                        implicitHeight: Math.max(statusInfo.implicitHeight, statusIcon.implicitHeight, statusPill.implicitHeight)

                        WifiIconWithFallback {
                            id: statusIcon

                            anchors.left: parent.left
                            anchors.verticalCenter: parent.verticalCenter
                            shellRoot: root.shellRoot
                            iconSource: root.trayIconUrl
                            fallbackLabel: root.shellRoot ? root.shellRoot.networkTrayGlyph() : "󰤮"
                            iconSize: 24
                        }

                        WifiActionChip {
                            id: statusPill

                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            shellRoot: root.shellRoot
                            cornerRadius: root.innerRadius
                            label: root.wiredConnectedState
                            ? "Wired"
                            : (root.otherConnectedState
                                ? "Online"
                                : (!root.wifiControlsAvailable
                                    ? "No Wi-Fi"
                                    : (!root.shellRoot || !root.shellRoot.wifiHardwareEnabled
                                        ? "Blocked"
                                        : (root.wifiEnabled ? (root.wifiConnectedState ? "Online" : "Ready") : "Off"))))
                            disabled: true
                            minimumWidth: 72
                            fillColor: root.cardFill
                            foregroundColor: root.networkConnectedState
                            ? (root.shellRoot ? root.shellRoot.launchColor : "white")
                            : (root.shellRoot ? root.shellRoot.primaryText : "white")
                            strokeColor: root.networkConnectedState
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
                                font.family: root.shellRoot ? root.shellRoot.baseFont : ""
                                font.pixelSize: 14
                                font.weight: Font.Bold
                                renderType: Text.NativeRendering
                            }

                            Text {
                                width: parent.width
                                text: root.shellRoot && root.networkConnectedState
                                ? "Interface: " + (root.shellRoot.defaultInterface || "network")
                                : (root.shellRoot && root.shellRoot.wifiDevicePresent
                                    ? "Wireless interface: " + (root.shellRoot.wifiInterface || "wifi")
                                    : "No wireless device detected")
                                elide: Text.ElideRight
                                color: root.mutedText
                                font.family: root.shellRoot ? root.shellRoot.baseFont : ""
                                font.pixelSize: 12
                                renderType: Text.NativeRendering
                            }
                        }
                    }
                }

                Row {
                    id: actionRow

                    width: parent.width
                    spacing: 10

                    WifiActionChip {
                        shellRoot: root.shellRoot
                        cornerRadius: root.innerRadius
                        label: root.wifiEnabled ? "Turn Off" : "Turn On"
                        minimumWidth: 96
                        disabled: !root.shellRoot || !root.wifiControlsAvailable || !root.shellRoot.wifiHardwareEnabled || root.shellRoot.wifiActionBusy
                        fillColor: root.cardStrongFill
                        foregroundColor: root.shellRoot ? root.shellRoot.launchColor : "white"
                        strokeColor: root.shellRoot ? root.shellRoot.withAlpha(root.shellRoot.launchColor, 0.18) : "#5176d2"
                        onClicked: root.shellRoot.wifiSetRadio(!root.wifiEnabled)
                    }

                    WifiActionChip {
                        shellRoot: root.shellRoot
                        cornerRadius: root.innerRadius
                        label: "Rescan"
                        minimumWidth: 88
                        disabled: !root.shellRoot || !root.wifiControlsAvailable || !root.wifiEnabled || root.shellRoot.wifiActionBusy
                        fillColor: root.cardFill
                        strokeColor: root.cardStroke
                        onClicked: root.shellRoot.wifiRescan()
                    }

                    WifiActionChip {
                        shellRoot: root.shellRoot
                        cornerRadius: root.innerRadius
                        label: "Advanced"
                        minimumWidth: 98
                        disabled: !root.shellRoot
                        fillColor: root.cardFill
                        strokeColor: root.cardStroke
                        onClicked: root.shellRoot.openWifiManager()
                    }
                }

                Rectangle {
                    id: messageCard

                    width: parent.width
                    visible: root.shellRoot && root.shellRoot.wifiActionMessage.length > 0
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
                        text: root.shellRoot ? root.shellRoot.wifiActionMessage : ""
                        color: root.shellRoot ? root.shellRoot.withAlpha(root.shellRoot.primaryText, 0.82) : "#d8d8d8"
                        font.family: root.shellRoot ? root.shellRoot.baseFont : ""
                        font.pixelSize: 12
                        wrapMode: Text.Wrap
                        renderType: Text.NativeRendering
                    }
                }

                Rectangle {
                    id: emptyStateCard

                    width: parent.width
                    visible: root.networks.length === 0
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
                        text: !root.wifiControlsAvailable
                        ? (root.networkConnectedState ? "Ethernet is active. No wireless device detected." : "No wireless device detected.")
                        : (root.wifiEnabled
                            ? (root.networkConnectedState ? "Ethernet is active. No visible Wi-Fi networks right now." : "No visible networks right now.")
                            : (root.networkConnectedState ? "Ethernet is active. Turn Wi-Fi on to scan for wireless networks." : "Turn Wi-Fi on to scan for networks."))
                        color: root.mutedText
                        font.family: root.shellRoot ? root.shellRoot.baseFont : ""
                        font.pixelSize: 13
                        wrapMode: Text.Wrap
                        renderType: Text.NativeRendering
                    }
                }

                Flickable {
                    id: networkList

                    readonly property real cappedContentHeight: {
                        // Keep the binding current when a password section changes a card's height.
                        const layoutHeight = networkColumn.implicitHeight;
                        const visibleCount = Math.min(root.maxVisibleNetworks, networkRepeater.count);
                        let cardsHeight = 0;
                        for (let i = 0; i < visibleCount; ++i) {
                            const card = networkRepeater.itemAt(i);
                            if (card) {
                                cardsHeight += card.height;
                            }
                        }
                        return cardsHeight + Math.max(0, visibleCount - 1) * networkColumn.spacing;
                    }

                    width: parent.width
                    height: visible ? Math.min(contentHeight, cappedContentHeight) : 0
                    contentHeight: networkColumn.implicitHeight
                    visible: root.networks.length > 0
                    clip: true
                    interactive: contentHeight > height
                    flickableDirection: Flickable.VerticalFlick
                    boundsBehavior: Flickable.StopAtBounds

                    Column {
                        id: networkColumn

                        width: networkList.width
                        spacing: 10

                        Repeater {
                            id: networkRepeater

                            model: root.networks

                            delegate: Rectangle {
                                id: networkCard

                                required property var modelData
                                readonly property bool expanded: root.expandedSsid === modelData.ssid
                                readonly property string buttonIconLabel: modelData.active
                                ? ""
                                : (modelData.enterprise && !modelData.known
                                    ? ""
                                    : (modelData.secure && !modelData.known && !expanded
                                        ? ""
                                        : ""))

                                width: networkColumn.width
                                implicitHeight: networkBody.implicitHeight + 20
                                radius: root.innerRadius
                                color: root.panelColor(modelData.active ? root.accentFill : root.cardFill)
                                border.width: 1
                                border.color: modelData.active ? root.accentStroke : root.cardStroke
                                onExpandedChanged: {
                                    if (expanded) {
                                        passwordFocusTimer.restart();
                                    } else {
                                        passwordFocusTimer.stop();
                                    }
                                }

                                Column {
                                    id: networkBody

                                    anchors.left: parent.left
                                    anchors.right: parent.right
                                    anchors.top: parent.top
                                    anchors.margins: 10
                                    spacing: 10

                                    Item {
                                        width: parent.width
                                        implicitHeight: Math.max(networkInfo.implicitHeight, connectChip.implicitHeight, networkIcon.implicitHeight)

                                        WifiIconWithFallback {
                                            id: networkIcon

                                            anchors.left: parent.left
                                            anchors.verticalCenter: parent.verticalCenter
                                            shellRoot: root.shellRoot
                                            iconSource: root.shellRoot ? root.shellRoot.wifiListIconSource(networkCard.modelData.signal, networkCard.modelData.secure) : ""
                                            fallbackLabel: root.shellRoot ? root.shellRoot.wifiSignalGlyph(networkCard.modelData.signal) : "󰤨"
                                            iconSize: 24
                                        }

                                        WifiActionChip {
                                            id: connectChip

                                            anchors.right: parent.right
                                            anchors.verticalCenter: parent.verticalCenter
                                            shellRoot: root.shellRoot
                                            cornerRadius: root.innerRadius
                                            label: ""
                                            iconLabel: networkCard.buttonIconLabel
                                            minimumWidth: 50
                                            disabled: !root.shellRoot
                                            || root.shellRoot.wifiActionBusy
                                            || (!root.wifiEnabled && !networkCard.modelData.active)
                                            || (networkCard.expanded && networkCard.modelData.secure && !networkCard.modelData.known && root.passwordText.length === 0)
                                            fillColor: networkCard.modelData.active ? root.cardStrongFill : root.cardFill
                                            foregroundColor: networkCard.modelData.active
                                            ? (root.shellRoot ? root.shellRoot.criticalColor : "white")
                                            : (root.shellRoot ? root.shellRoot.launchColor : "white")
                                            strokeColor: networkCard.modelData.active
                                            ? (root.shellRoot ? root.shellRoot.withAlpha(root.shellRoot.criticalColor, 0.2) : "#a55454")
                                            : (root.shellRoot ? root.shellRoot.withAlpha(root.shellRoot.launchColor, 0.18) : "#5176d2")
                                            onClicked: root.activateNetwork(networkCard.modelData)
                                        }

                                        Column {
                                            id: networkInfo

                                            anchors.left: networkIcon.right
                                            anchors.leftMargin: root.innerPadding
                                            anchors.right: connectChip.left
                                            anchors.rightMargin: root.innerPadding
                                            anchors.verticalCenter: parent.verticalCenter
                                            spacing: 4

                                            Item {
                                                id: titleLine

                                                property real badgeWidth: secureBadge.visible ? secureBadge.implicitWidth + titleRow.spacing : 0

                                                width: parent.width
                                                height: Math.max(networkTitle.implicitHeight, secureBadge.implicitHeight)
                                                clip: true

                                                Row {
                                                    id: titleRow

                                                    anchors.left: parent.left
                                                    anchors.verticalCenter: parent.verticalCenter
                                                    spacing: 6

                                                    Text {
                                                        id: networkTitle

                                                        width: Math.max(0, Math.min(implicitWidth, titleLine.width - titleLine.badgeWidth))
                                                        text: networkCard.modelData.ssid
                                                        elide: Text.ElideRight
                                                        color: root.shellRoot ? root.shellRoot.primaryText : "white"
                                                        font.family: root.shellRoot ? root.shellRoot.baseFont : ""
                                                        font.pixelSize: 14
                                                        font.weight: Font.Bold
                                                        renderType: Text.NativeRendering
                                                    }

                                                    WifiSecurityBadge {
                                                        id: secureBadge

                                                        shellRoot: root.shellRoot
                                                        cornerRadius: root.innerRadius
                                                        active: networkCard.modelData.secure
                                                    }
                                                }
                                            }

                                            Text {
                                                width: parent.width
                                                text: root.networkMeta(networkCard.modelData)
                                                elide: Text.ElideRight
                                                color: root.mutedText
                                                font.family: root.shellRoot ? root.shellRoot.baseFont : ""
                                                font.pixelSize: 11
                                                renderType: Text.NativeRendering
                                            }
                                        }
                                    }

                                    ExpandableSection {
                                        width: parent.width
                                        fullHeight: passwordRow.implicitHeight
                                        expanded: networkCard.expanded && networkCard.modelData.secure && !networkCard.modelData.known
                                        openRevealDuration: 180
                                        openContentDelay: 20
                                        openFadeDuration: 130
                                        openSlideDuration: 180
                                        openContentOffset: -6
                                        closeRevealDuration: 150
                                        closeFadeDuration: 90
                                        closeSlideDuration: 130
                                        closeContentOffset: -4

                                        Column {
                                            id: passwordRow

                                            width: parent.width
                                            spacing: root.panelSectionSpacing

                                            Rectangle {
                                                width: parent.width
                                                height: 38
                                                radius: root.innerRadius
                                                color: root.panelColor(root.inputFill)
                                                border.width: 1
                                                border.color: passwordInput.activeFocus ? root.accentStroke : root.cardStroke

                                                TextInput {
                                                    id: passwordInput

                                                    anchors.left: parent.left
                                                    anchors.right: parent.right
                                                    anchors.verticalCenter: parent.verticalCenter
                                                    anchors.leftMargin: root.innerPadding
                                                    anchors.rightMargin: root.innerPadding
                                                    text: networkCard.expanded ? root.passwordText : ""
                                                    activeFocusOnPress: true
                                                    focus: networkCard.expanded && popup.visible && !popup.animatingClose
                                                    selectByMouse: true
                                                    color: root.shellRoot ? root.shellRoot.primaryText : "white"
                                                    font.family: root.shellRoot ? root.shellRoot.baseFont : ""
                                                    font.pixelSize: 13
                                                    font.letterSpacing: 3
                                                    echoMode: TextInput.Password
                                                    inputMethodHints: Qt.ImhSensitiveData | Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                                                    renderType: Text.NativeRendering
                                                    onTextChanged: root.passwordText = text

                                                    Keys.onReturnPressed: {
                                                        if (root.passwordText.length > 0) {
                                                            root.activateNetwork(networkCard.modelData);
                                                        }
                                                    }
                                                }

                                                Text {
                                                    anchors.left: parent.left
                                                    anchors.verticalCenter: parent.verticalCenter
                                                    anchors.leftMargin: root.innerPadding
                                                    visible: passwordInput.text.length === 0
                                                    text: "Password"
                                                    color: root.softText
                                                    font.family: root.shellRoot ? root.shellRoot.baseFont : ""
                                                    font.pixelSize: 13
                                                    renderType: Text.NativeRendering
                                                }
                                            }

                                            Row {
                                                spacing: root.panelSectionSpacing

                                                WifiActionChip {
                                                    shellRoot: root.shellRoot
                                                    cornerRadius: root.innerRadius
                                                    label: "Join"
                                                    minimumWidth: 82
                                                    disabled: root.passwordText.length === 0 || (root.shellRoot && root.shellRoot.wifiActionBusy)
                                                    fillColor: root.cardStrongFill
                                                    foregroundColor: root.shellRoot ? root.shellRoot.launchColor : "white"
                                                    strokeColor: root.shellRoot ? root.shellRoot.withAlpha(root.shellRoot.launchColor, 0.18) : "#5176d2"
                                                    onClicked: root.activateNetwork(networkCard.modelData)
                                                }

                                                WifiActionChip {
                                                    shellRoot: root.shellRoot
                                                    cornerRadius: root.innerRadius
                                                    label: "Cancel"
                                                    minimumWidth: 82
                                                    fillColor: root.cardFill
                                                    strokeColor: root.cardStroke
                                                    onClicked: {
                                                        root.expandedSsid = "";
                                                        root.passwordText = "";
                                                    }
                                                }
                                            }
                                        }

                                        Timer {
                                            id: passwordFocusTimer

                                            interval: 60
                                            repeat: false
                                            onTriggered: {
                                                if (networkCard.expanded && popup.visible && !popup.animatingClose) {
                                                    passwordInput.forceActiveFocus();
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    MouseArea {
                        anchors.fill: parent
                        acceptedButtons: Qt.NoButton

                        onWheel: function(wheel) {
                            if (!networkList.interactive) {
                                return;
                            }
                            const pixelStep = wheel.pixelDelta.y;
                            const scrollStep = pixelStep !== 0 ? pixelStep : wheel.angleDelta.y / 2;
                            const maximumY = Math.max(0, networkList.contentHeight - networkList.height);
                            networkList.contentY = Math.max(0, Math.min(maximumY, networkList.contentY - scrollStep));
                            wheel.accepted = true;
                        }
                    }
                    }
                }
    }
}
