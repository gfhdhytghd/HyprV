import QtQuick
import Quickshell
import Quickshell.Wayland

WifiIndicator {
    id: root

    property var shellRoot: null
    property var parentWindow: null
    property bool popupVisible: false
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
    readonly property int panelMaxHeight: 960
    readonly property int panelVerticalPadding: 20
    readonly property int panelSectionSpacing: 10
    readonly property int panelPadding: 10
    readonly property int innerPadding: 10
    readonly property int innerRadius: 9
    readonly property real fixedSectionHeight: headerRow.height
        + statusCard.implicitHeight
        + actionRow.implicitHeight
        + (messageCard.visible ? messageCard.implicitHeight : 0)
        + (emptyStateCard.visible ? emptyStateCard.implicitHeight : 0)
    readonly property int fixedSectionCount: 3
        + (messageCard.visible ? 1 : 0)
        + (emptyStateCard.visible ? 1 : 0)
    readonly property real fixedSpacingHeight: Math.max(0, fixedSectionCount - 1) * panelSectionSpacing
    readonly property real networkListTopSpacing: root.networks.length > 0 ? panelSectionSpacing : 0
    readonly property real maxNetworkListHeight: Math.max(0, panelMaxHeight - panelVerticalPadding - fixedSectionHeight - fixedSpacingHeight - networkListTopSpacing)
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

    available: shellRoot ? shellRoot.networkWidgetVisible : false
    iconSource: trayIconUrl
    fallbackLabel: shellRoot ? shellRoot.networkTrayGlyph() : "󰤮"

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

    onLeftClicked: togglePopup()

    onXChanged: {
        if (popup.visible) {
            updatePopupAnchor();
        }
    }

    onYChanged: {
        if (popup.visible) {
            updatePopupAnchor();
        }
    }

    Component.onCompleted: syncNetworkList(true)

    Connections {
        target: root.shellRoot

        function onWifiNetworksChanged() {
            root.syncNetworkList(false);
        }
    }

    PanelWindow {
        id: popup

        property bool animatingClose: false
        property bool openAnimationPending: false

        screen: root.parentWindow ? root.parentWindow.screen : null
        visible: root.popupVisible || animatingClose
        color: "transparent"
        aboveWindows: true
        focusable: visible
        exclusiveZone: -1

        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None
        WlrLayershell.namespace: "shell:hyprv-wifi"

        anchors.top: true
        anchors.left: true
        anchors.right: true
        anchors.bottom: true

        onWidthChanged: if (visible) {
            updatePopupPosition();
        }
        onHeightChanged: if (visible) {
            updatePopupPosition();
        }

        function updatePopupPosition() {
            if (!visible || !root.parentWindow || !screen) {
                return;
            }
            const point = root.mapToGlobal(Math.round(root.width / 2), root.height);
            const relativeY = point.y - screen.y;
            const belowY = Math.round(relativeY + 10);
            const aboveY = Math.round(relativeY - popupChrome.height - 10);
            const fitsBelow = belowY + popupChrome.height <= height - root.popupScreenMargin;
            const fitsAbove = aboveY >= root.popupScreenMargin;
            popupChrome.x = Math.max(
                root.popupScreenMargin,
                Math.min(width - popupChrome.width - root.popupScreenMargin, width - popupChrome.width - root.popupRightMargin)
            );

            if (fitsBelow || !fitsAbove) {
                popupChrome.y = Math.max(
                    root.popupScreenMargin,
                    Math.min(height - popupChrome.height - root.popupScreenMargin, belowY)
                );
            } else {
                popupChrome.y = Math.max(root.popupScreenMargin, aboveY);
            }
        }

        onVisibleChanged: {
            if (visible) {
                root.updatePopupAnchor();
                popupFocusScope.forceActiveFocus();
                if (!animatingClose) {
                    popup.openAnimationPending = true;
                    popupChrome.prepareOpenAnimation();
                    popupOpenTimer.restart();
                }
            } else {
                animatingClose = false;
                openAnimationPending = false;
                popupOpenTimer.stop();
                popupChrome.stopAnimations();
                popupChrome.resetAnimationState();
            }
        }

        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
            onClicked: root.closePopup()
        }

        FocusScope {
            id: popupFocusScope

            anchors.fill: parent
            focus: popup.visible

            Keys.onEscapePressed: root.closePopup()

            AnimatedGlassPanel {
                id: popupChrome

                width: root.popupPanelWidth
                fullPanelHeight: Math.min(root.panelMaxHeight, panelColumn.implicitHeight + root.panelVerticalPadding)
                fillColor: root.glassFill
                strokeColor: root.glassStroke
                shadowColor: "transparent"
                devicePixelRatio: popup.devicePixelRatio
                openRevealPause: 85
                openRevealDuration: 280
                openContentDelay: 60
                openFadeDuration: 180
                openSlideDuration: 220
                openContentOffset: -10
                closeRevealPause: 35
                closeRevealDuration: 210
                closeFadeDuration: 110
                closeSlideDuration: 170
                closeContentOffset: -8

                onFullPanelHeightChanged: {
                    if (popup.openAnimationPending) {
                        root.updatePopupAnchor();
                        popupOpenTimer.restart();
                        return;
                    }
                    if (!popupChrome.openAnimationRunning && !popupChrome.closeAnimationRunning) {
                        revealHeight = fullPanelHeight;
                        if (!popup.visible) {
                            contentOpacity = 1;
                            contentOffset = 0;
                        }
                    }
                }

                onOpenAnimationFinished: {
                    if (!popup.visible || popup.animatingClose) {
                        return;
                    }
                    root.updatePopupAnchor();
                }

                onCloseAnimationFinished: {
                    if (popup.animatingClose && !root.popupVisible) {
                        popup.animatingClose = false;
                    }
                }

                MouseArea {
                    anchors.fill: parent
                    acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
                }

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

                    width: parent.width
                    height: visible ? Math.min(contentHeight, root.maxNetworkListHeight) : 0
                    contentHeight: networkColumn.implicitHeight
                    visible: root.networks.length > 0
                    clip: true
                    interactive: contentHeight > height
                    boundsBehavior: Flickable.StopAtBounds

                    Column {
                        id: networkColumn

                        width: networkList.width
                        spacing: 10

                        Repeater {
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
                    }
                }
            }
        }

        Timer {
            id: popupOpenTimer

            interval: 16
            repeat: false
            onTriggered: {
                if (!popup.visible || !root.popupVisible || popup.animatingClose) {
                    popup.openAnimationPending = false;
                    return;
                }
                if (panelColumn.implicitHeight <= 0) {
                    popupOpenTimer.restart();
                    return;
                }
                popup.openAnimationPending = false;
                root.updatePopupAnchor();
                popupChrome.playOpenAnimation();
            }
        }
    }
}
