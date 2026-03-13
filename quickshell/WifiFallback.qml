import QtQuick
import Quickshell

WifiIndicator {
    id: root

    property var shellRoot: null
    property var parentWindow: null
    property bool popupVisible: false
    property string expandedSsid: ""
    property string passwordText: ""

    readonly property bool wifiEnabled: shellRoot ? shellRoot.wifiRadioEnabled : false
    readonly property bool wifiConnectedState: shellRoot ? shellRoot.wifiConnected : false
    readonly property real wifiStrength: shellRoot ? shellRoot.wifiSignalStrength : 0
    readonly property var networks: shellRoot ? shellRoot.wifiNetworks : []
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
    readonly property int panelMaxHeight: 960
    readonly property int panelVerticalPadding: 28
    readonly property int panelSectionSpacing: 12
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
    readonly property string trayIconUrl: shellRoot ? shellRoot.wifiTrayIconSource(wifiEnabled, shellRoot.wifiHardwareEnabled, wifiConnectedState, wifiStrength, shellRoot.wifiSecure) : ""
    readonly property string connectionSummary: {
        if (!shellRoot) {
            return "";
        }
        if (!shellRoot.wifiHardwareEnabled) {
            return "Hardware blocked";
        }
        if (!wifiEnabled) {
            return "Wi-Fi disabled";
        }
        if (wifiConnectedState) {
            return shellRoot.wifiSsid + "  " + Math.round(wifiStrength * 100) + "%";
        }
        return "Not connected";
    }

    available: shellRoot ? shellRoot.wifiWidgetVisible : false
    iconSource: trayIconUrl
    fallbackLabel: shellRoot ? shellRoot.wifiTrayGlyph(wifiEnabled, wifiConnectedState, wifiStrength) : "󰤮"

    function updatePopupAnchor() {
        if (popup.visible && root.parentWindow && popup.anchor.window) {
            popup.anchor.updateAnchor();
        }
    }

    function panelColor(colorValue) {
        return Qt.rgba(colorValue.r, colorValue.g, colorValue.b, colorValue.a * panelSurfaceOpacity);
    }

    function openPopup() {
        if (shellRoot) {
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

    PopupWindow {
        id: popup

        property bool animatingClose: false
        property bool openAnimationPending: false

        visible: root.popupVisible || animatingClose
        color: "transparent"
        implicitWidth: root.popupPanelWidth
        implicitHeight: popupChrome.implicitHeight
        anchor.window: root.parentWindow

        onVisibleChanged: {
            if (visible) {
                root.updatePopupAnchor();
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

        anchor.onAnchoring: {
            if (!root.parentWindow) {
                return;
            }
            const point = root.mapToItem(root.parentWindow.contentItem, 0, root.height);
            anchor.rect.x = Math.round(root.parentWindow.width - root.popupPanelWidth - root.popupRightMargin);
            anchor.rect.y = Math.round(point.y + 10);
            anchor.rect.width = 1;
            anchor.rect.height = 1;
        }

        AnimatedGlassPanel {
            id: popupChrome

            width: parent.width
            fullPanelHeight: Math.min(root.panelMaxHeight, panelColumn.implicitHeight + root.panelVerticalPadding)
            fillColor: root.glassFill
            strokeColor: root.glassStroke
            shadowColor: "transparent"
            devicePixelRatio: root.parentWindow ? root.parentWindow.devicePixelRatio : 1
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

            Column {
                id: panelColumn

                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 14
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
                        text: "Wireless"
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
                    radius: 10
                    color: root.panelColor(root.wifiConnectedState ? root.accentFill : root.cardStrongFill)
                    border.width: 1
                    border.color: root.wifiConnectedState ? root.accentStroke : root.cardStroke

                    Item {
                        id: statusBody

                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.margins: 12
                        implicitHeight: Math.max(statusInfo.implicitHeight, statusIcon.implicitHeight, statusPill.implicitHeight)

                        WifiIconWithFallback {
                            id: statusIcon

                            anchors.left: parent.left
                            anchors.verticalCenter: parent.verticalCenter
                            shellRoot: root.shellRoot
                            iconSource: root.trayIconUrl
                            fallbackLabel: root.shellRoot ? root.shellRoot.wifiTrayGlyph(root.wifiEnabled, root.wifiConnectedState, root.wifiStrength) : "󰤮"
                            iconSize: 24
                        }

                        WifiActionChip {
                            id: statusPill

                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            shellRoot: root.shellRoot
                            label: !root.shellRoot || !root.shellRoot.wifiHardwareEnabled
                            ? "Blocked"
                            : (root.wifiEnabled ? (root.wifiConnectedState ? "Online" : "Ready") : "Off")
                            disabled: true
                            minimumWidth: 72
                            fillColor: root.cardFill
                            foregroundColor: root.wifiConnectedState
                            ? (root.shellRoot ? root.shellRoot.launchColor : "white")
                            : (root.shellRoot ? root.shellRoot.primaryText : "white")
                            strokeColor: root.wifiConnectedState
                            ? (root.shellRoot ? root.shellRoot.withAlpha(root.shellRoot.launchColor, 0.18) : "#5176d2")
                            : root.cardStroke
                        }

                        Column {
                            id: statusInfo

                            anchors.left: statusIcon.right
                            anchors.leftMargin: 12
                            anchors.right: statusPill.left
                            anchors.rightMargin: 12
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
                                text: root.shellRoot && root.shellRoot.wifiDevicePresent ? "Interface: " + (root.shellRoot.wifiInterface || "wifi") : "No wireless device detected"
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
                        label: root.wifiEnabled ? "Turn Off" : "Turn On"
                        minimumWidth: 96
                        disabled: !root.shellRoot || !root.shellRoot.wifiHardwareEnabled || root.shellRoot.wifiActionBusy
                        fillColor: root.cardStrongFill
                        foregroundColor: root.shellRoot ? root.shellRoot.launchColor : "white"
                        strokeColor: root.shellRoot ? root.shellRoot.withAlpha(root.shellRoot.launchColor, 0.18) : "#5176d2"
                        onClicked: root.shellRoot.wifiSetRadio(!root.wifiEnabled)
                    }

                    WifiActionChip {
                        shellRoot: root.shellRoot
                        label: "Rescan"
                        minimumWidth: 88
                        disabled: !root.shellRoot || !root.wifiEnabled || root.shellRoot.wifiActionBusy
                        fillColor: root.cardFill
                        strokeColor: root.cardStroke
                        onClicked: root.shellRoot.wifiRescan()
                    }

                    WifiActionChip {
                        shellRoot: root.shellRoot
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
                    radius: 10
                    color: root.panelColor(root.cardFill)
                    border.width: 1
                    border.color: root.cardStroke

                    Text {
                        id: actionMessageLabel

                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.margins: 9
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
                    radius: 10
                    color: root.panelColor(root.cardFill)
                    border.width: 1
                    border.color: root.cardStroke

                    Text {
                        id: emptyState

                        anchors.centerIn: parent
                        width: parent.width - 28
                        horizontalAlignment: Text.AlignHCenter
                        text: root.wifiEnabled ? "No visible networks right now." : "Turn Wi-Fi on to scan for networks."
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
                                radius: 10
                                color: root.panelColor(modelData.active ? root.accentFill : root.cardFill)
                                border.width: 1
                                border.color: modelData.active ? root.accentStroke : root.cardStroke

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
                                            anchors.leftMargin: 12
                                            anchors.right: connectChip.left
                                            anchors.rightMargin: 12
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
                                            spacing: 8

                                            Rectangle {
                                                width: parent.width
                                                height: 38
                                                radius: 10
                                                color: root.panelColor(root.inputFill)
                                                border.width: 1
                                                border.color: passwordInput.activeFocus ? root.accentStroke : root.cardStroke

                                                TextInput {
                                                    id: passwordInput

                                                    anchors.left: parent.left
                                                    anchors.right: parent.right
                                                    anchors.verticalCenter: parent.verticalCenter
                                                    anchors.leftMargin: 12
                                                    anchors.rightMargin: 12
                                                    text: networkCard.expanded ? root.passwordText : ""
                                                    color: root.shellRoot ? root.shellRoot.primaryText : "white"
                                                    font.family: root.shellRoot ? root.shellRoot.baseFont : ""
                                                    font.pixelSize: 13
                                                    echoMode: TextInput.Password
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
                                                    anchors.leftMargin: 12
                                                    visible: passwordInput.text.length === 0
                                                    text: "Password"
                                                    color: root.softText
                                                    font.family: root.shellRoot ? root.shellRoot.baseFont : ""
                                                    font.pixelSize: 13
                                                    renderType: Text.NativeRendering
                                                }
                                            }

                                            Row {
                                                spacing: 8

                                                WifiActionChip {
                                                    shellRoot: root.shellRoot
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
