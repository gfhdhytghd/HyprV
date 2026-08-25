import QtQuick
import Quickshell
import Quickshell.Wayland

Item {
    id: popupRoot

    property var shellRoot: null
    property var sourceItem: null
    property var parentWindow: null
    property bool popupRequested: false
    property bool animatingClose: false
    property bool openAnimationPending: false
    property bool egpuConfirmPending: false
    property string currentPage: "main"
    property string pendingPage: ""
    readonly property bool wifiExpanded: currentPage === "wifi"
    readonly property bool bluetoothExpanded: currentPage === "bluetooth"
    readonly property bool mihomoExpanded: currentPage === "mihomo"
    property bool powerExpanded: false

    readonly property int popupScreenMargin: 10
    readonly property int popupPadding: 0
    readonly property int columnSpacing: 10
    readonly property int cardSpacing: 10
    readonly property int moduleSize: 62
    readonly property int smallModuleSize: 48
    readonly property real columnWidth: moduleSize * 2 + cardSpacing
    readonly property int popupWidth: currentPage === "main"
        ? (popupPadding * 2 + columnWidth * 2 + columnSpacing)
        : 384
    readonly property color glassFill: shellRoot ? shellRoot.withAlpha(shellRoot.darkMode ? "#101214" : "#ffffff", shellRoot.darkMode ? 0.42 : 0.28) : "#202020"
    readonly property color glassStroke: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.14 : 0.1) : "#3a3a3a"
    readonly property color panelShadowColor: shellRoot ? (shellRoot.darkMode ? shellRoot.withAlpha("#000000", 0.45) : shellRoot.withAlpha("#000000", 0.16)) : Qt.rgba(0, 0, 0, 0.3)
    readonly property color mutedTextColor: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, 0.68) : "#b0b0b0"
    readonly property color detailFill: shellRoot ? shellRoot.withAlpha(shellRoot.darkMode ? "#ffffff" : "#ffffff", shellRoot.darkMode ? 0.07 : 0.22) : "#2a2a2a"
    readonly property color detailStroke: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.12 : 0.08) : "#454545"

    function resetExpandedState() {
        currentPage = "main";
        pendingPage = "";
        powerExpanded = false;
        egpuConfirmPending = false;
    }

    function openFor(source, window) {
        if (!source || !window) {
            return;
        }
        sourceItem = source;
        parentWindow = window;
        popupRequested = true;
        animatingClose = false;
        resetExpandedState();
        positionTimer.restart();
        if (popupWindow.visible) {
            popupCard.prepareOpenAnimation();
            openAnimationPending = true;
            popupOpenTimer.restart();
            popupWindow.updatePopupPosition();
        } else {
            popupWindow.visible = true;
        }
    }

    function closePopup() {
        if ((!popupRequested && !animatingClose) || !popupWindow.visible) {
            popupRequested = false;
            animatingClose = false;
            return;
        }
        if (animatingClose) {
            return;
        }
        popupRequested = false;
        animatingClose = true;
        pendingPage = "";
        egpuConfirmPending = false;
        popupCard.playCloseAnimation();
    }

    function animateCurrentPageOpen() {
        if (!popupWindow.visible || animatingClose) {
            return;
        }
        openAnimationPending = true;
        popupCard.prepareOpenAnimation();
        positionTimer.restart();
        popupOpenTimer.restart();
    }

    function switchPageWithAnimation(targetPage) {
        if (!targetPage || currentPage === targetPage) {
            return;
        }
        if (!popupWindow.visible || animatingClose) {
            currentPage = targetPage;
            return;
        }
        pendingPage = targetPage;
        openAnimationPending = false;
        popupOpenTimer.stop();
        popupCard.stopAnimations();
        popupCard.playCloseAnimation();
    }

    function toggleFor(source, window) {
        if (popupWindow.visible && sourceItem === source && parentWindow === window) {
            closePopup();
            return;
        }
        openFor(source, window);
    }

    function toggleCentered(window) {
        if (popupWindow.visible) {
            closePopup();
            return;
        }
        if (!window) {
            return;
        }
        sourceItem = null;
        parentWindow = window;
        popupRequested = true;
        animatingClose = false;
        resetExpandedState();
        popupWindow.visible = true;
    }

    function runAndRefresh(command) {
        if (!shellRoot) {
            return;
        }
        shellRoot.runDetached(command);
        controlPanelRefreshTimer.restart();
    }

    function clampPercent(value) {
        return Math.max(0, Math.min(100, Math.round(value)));
    }

    function setBrightnessPercent(value) {
        if (!shellRoot) {
            return;
        }
        shellRoot.applyBrightnessPercent(clampPercent(value));
    }

    function setAudioVolumePercent(value) {
        const nextValue = clampPercent(value);
        if (shellRoot) {
            shellRoot.audioVolumePercent = nextValue;
        }
        if (shellRoot) {
            shellRoot.runDetached(["wpctl", "set-volume", "@DEFAULT_AUDIO_SINK@", (nextValue / 100).toFixed(2)]);
            shellRoot.scheduleAudioRefresh();
        }
    }

    function setFanPercent(channel, value) {
        if (!shellRoot) {
            return;
        }
        shellRoot.applyFanPercent(channel, clampPercent(value));
    }

    function toggleAudioMute() {
        if (!shellRoot) {
            return;
        }
        shellRoot.audioAvailable = true;
        shellRoot.audioMuted = !shellRoot.audioMuted;
        shellRoot.runDetached(["wpctl", "set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"]);
        shellRoot.scheduleAudioRefresh();
    }

    function toggleWifiEnabled() {
        if (!shellRoot) {
            return;
        }
        const enabled = !shellRoot.wifiRadioEnabled;
        shellRoot.wifiRadioEnabled = enabled;
        shellRoot.wifiEnabled = enabled;
        shellRoot.wifiSetRadio(enabled);
        controlPanelRefreshTimer.restart();
    }

    function toggleBluetoothEnabled() {
        if (!shellRoot) {
            return;
        }
        shellRoot.bluetoothSetPower(!shellRoot.bluetoothEnabled);
    }

    function toggleDnd() {
        if (shellRoot) {
            shellRoot.dndEnabled = !shellRoot.dndEnabled;
        }
        runAndRefresh(["swaync-client", "-d", "-sw"]);
    }

    function toggleTheme() {
        if (!shellRoot) {
            return;
        }
        shellRoot.darkMode = !shellRoot.darkMode;
        shellRoot.runDetached([shellRoot.configDir + "/quickshell/scripts/toggle-theme.sh"]);
    }

    function toggleRecording() {
        if (!shellRoot) {
            return;
        }
        const nextState = !shellRoot.screenRecording;
        shellRoot.screenRecording = nextState;
        if (nextState) {
            runAndRefresh([shellRoot.homeDir + "/.config/hypr/scripts/record-script.sh", "--fullscreen-sound"]);
        } else {
            runAndRefresh(["sh", "-c", "pkill -INT wf-recorder || pkill -INT wl-screenrec || pkill -INT gpu-screen-recorder"]);
        }
    }

    function togglePreventSleep() {
        if (!shellRoot) {
            return;
        }
        shellRoot.preventSleepEnabled = !shellRoot.preventSleepEnabled;
        runAndRefresh([shellRoot.configDir + "/quickshell/scripts/prevent-sleep.sh", "toggle"]);
    }

    function toggleDemoMode() {
        if (!shellRoot) {
            return;
        }
        shellRoot.demoModeEnabled = !shellRoot.demoModeEnabled;
        runAndRefresh([shellRoot.configDir + "/quickshell/scripts/demo-mode.sh", "toggle"]);
    }

    function toggleWifiExpanded() {
        if (!shellRoot) {
            return;
        }
        popupRequested = false;
        animatingClose = false;
        popupWindow.visible = false;
        shellRoot.openWifiPanel();
    }

    function toggleBluetoothExpanded() {
        const wasExpanded = bluetoothExpanded;
        if (wasExpanded) {
            showMainPage();
            return;
        }
        if (shellRoot) {
            shellRoot.refreshBluetoothStatus();
        }
        switchPageWithAnimation("bluetooth");
    }

    function toggleMihomoExpanded() {
        if (mihomoExpanded) {
            showMainPage();
            return;
        }
        switchPageWithAnimation("mihomo");
    }

    function showMainPage() {
        powerExpanded = false;
        switchPageWithAnimation("main");
    }

    function powerProfileLabel(profile) {
        if (profile === "power-saver") {
            return "Battery";
        }
        if (profile === "performance") {
            return "Performance";
        }
        return "Balanced";
    }

    function powerProfileIcon(profile) {
        if (profile === "power-saver") {
            return "󰾆";
        }
        if (profile === "performance") {
            return "󰓅";
        }
        return "󰾅";
    }

    function setPowerProfile(profile) {
        if (!shellRoot || !profile) {
            return;
        }
        shellRoot.powerProfile = profile;
        shellRoot.runDetached(["powerprofilesctl", "set", profile]);
        controlPanelRefreshTimer.restart();
        powerProfileFollowupRefresh.restart();
    }

    function cyclePowerProfile() {
        if (!shellRoot) {
            return;
        }
        const order = ["power-saver", "balanced", "performance"];
        const current = order.indexOf(shellRoot.powerProfile);
        const nextIndex = current >= 0 ? (current + 1) % order.length : 1;
        setPowerProfile(order[nextIndex]);
    }

    function togglePowerExpanded() {
        powerExpanded = !powerExpanded;
    }

    function handleMediaAction(command) {
        if (!shellRoot || !shellRoot.mediaAvailable) {
            return;
        }
        if (Array.isArray(command) && command.length >= 2 && command[0] === "playerctl" && command[1] === "play-pause") {
            shellRoot.mediaPlaying = !shellRoot.mediaPlaying;
        }
        shellRoot.runDetached(command);
        mediaFollowupRefresh.restart();
    }

    function currentWifiTitle() {
        return "WIFI";
    }

    function currentWifiSubtitle() {
        if (!shellRoot) {
            return "";
        }
        if (!shellRoot.wifiRadioEnabled) {
            return "Turned off";
        }
        if (shellRoot.wifiConnected) {
            return shellRoot.wifiSsid.length > 0 ? shellRoot.wifiSsid : "Connected";
        }
        return "Not connected";
    }

    function currentBluetoothTitle() {
        if (!shellRoot || !Array.isArray(shellRoot.bluetoothDevices)) {
            return "Bluetooth";
        }
        for (let i = 0; i < shellRoot.bluetoothDevices.length; i++) {
            const device = shellRoot.bluetoothDevices[i];
            if (device.connected && (device.name || "").length > 0) {
                return device.name;
            }
        }
        return "Bluetooth";
    }

    function currentBluetoothSubtitle() {
        if (!shellRoot) {
            return "";
        }
        if (!shellRoot.bluetoothPresent) {
            return "No adapter";
        }
        if (!shellRoot.bluetoothEnabled) {
            return "Turned off";
        }
        const devices = Array.isArray(shellRoot.bluetoothDevices) ? shellRoot.bluetoothDevices : [];
        let connectedCount = 0;
        let pairedCount = 0;
        for (let i = 0; i < devices.length; i++) {
            if (devices[i].connected) {
                connectedCount += 1;
            }
            if (devices[i].paired) {
                pairedCount += 1;
            }
        }
        if (connectedCount > 1) {
            return connectedCount + " connected";
        }
        if (connectedCount === 1) {
            return "Connected";
        }
        if (shellRoot.bluetoothDiscovering) {
            return "Scanning";
        }
        if (pairedCount > 0) {
            return pairedCount + " paired";
        }
        return "Ready";
    }

    function confirmEgpuAction() {
        if (popupRoot.egpuConfirmPending) {
            popupRoot.egpuConfirmPending = false;
            egpuConfirmResetTimer.stop();
            if (popupRoot.shellRoot) {
                popupRoot.shellRoot.runDetached([popupRoot.shellRoot.configDir + "/quickshell/scripts/egpu-disconnect.sh"]);
            }
            return;
        }
        popupRoot.egpuConfirmPending = true;
        egpuConfirmResetTimer.restart();
    }

    Timer {
        id: controlPanelRefreshTimer

        interval: 400
        repeat: false
        onTriggered: if (shellRoot) {
            shellRoot.refreshControlPanelStatus();
        }
    }

    Timer {
        id: mediaFollowupRefresh

        interval: 250
        repeat: false
        onTriggered: if (shellRoot) {
            shellRoot.refreshMediaStatus();
        }
    }

    Timer {
        id: powerProfileFollowupRefresh

        interval: 350
        repeat: false
        onTriggered: if (shellRoot) {
            shellRoot.refreshPowerProfileStatus();
        }
    }

    Timer {
        id: egpuConfirmResetTimer

        interval: 3000
        repeat: false
        onTriggered: popupRoot.egpuConfirmPending = false
    }

    Timer {
        id: positionTimer

        interval: 0
        repeat: false
        onTriggered: popupWindow.updatePopupPosition()
    }

    Timer {
        id: popupOpenTimer

        interval: 16
        repeat: false
        onTriggered: {
            if (!popupWindow.visible || !popupRoot.popupRequested || popupRoot.animatingClose) {
                popupRoot.openAnimationPending = false;
                return;
            }
            if (popupContent.implicitHeight <= 0) {
                popupOpenTimer.restart();
                return;
            }
            popupWindow.updatePopupPosition();
            popupCard.playOpenAnimation();
        }
    }

    Component {
        id: bluetoothPageComponent

        Column {
            width: popupContent.width
            spacing: popupRoot.cardSpacing

            ControlPanelBluetoothDetails {
                width: parent.width
                shellRoot: popupRoot.shellRoot
                useExternalPanelBackground: true
                onCloseRequested: popupRoot.showMainPage()
            }
        }
    }

    Component {
        id: mihomoPageComponent

        Column {
            width: popupContent.width
            spacing: popupRoot.cardSpacing

            MihomoControlPanel {
                width: parent.width
                shellRoot: popupRoot.shellRoot
                useExternalPanelBackground: true
                onCloseRequested: popupRoot.showMainPage()
            }
        }
    }

    Component {
        id: mainPageComponent

        Column {
            width: popupContent.width
            spacing: popupRoot.cardSpacing

            Row {
                width: parent.width
                spacing: popupRoot.columnSpacing

                Column {
                    width: popupRoot.columnWidth
                    spacing: popupRoot.cardSpacing

                    ControlPanelSplitTile {
                        width: parent.width
                        height: popupRoot.moduleSize
                        shellRoot: popupRoot.shellRoot
                        icon: active ? "󰤨" : "󰤭"
                        title: popupRoot.currentWifiTitle()
                        subtitle: popupRoot.currentWifiSubtitle()
                        active: popupRoot.shellRoot ? popupRoot.shellRoot.wifiRadioEnabled : false
                        expanded: popupRoot.wifiExpanded
                        onLeftClicked: popupRoot.toggleWifiEnabled()
                        onRightClicked: popupRoot.toggleWifiExpanded()
                    }

                    ControlPanelSplitTile {
                        width: parent.width
                        height: popupRoot.moduleSize
                        shellRoot: popupRoot.shellRoot
                        icon: active ? "󰂯" : "󰂲"
                        title: popupRoot.currentBluetoothTitle()
                        subtitle: popupRoot.currentBluetoothSubtitle()
                        active: popupRoot.shellRoot ? popupRoot.shellRoot.bluetoothEnabled : false
                        expanded: popupRoot.bluetoothExpanded
                        onLeftClicked: popupRoot.toggleBluetoothEnabled()
                        onRightClicked: popupRoot.toggleBluetoothExpanded()
                    }

                    MihomoControlTile {
                        width: parent.width
                        height: popupRoot.moduleSize
                        shellRoot: popupRoot.shellRoot
                        expanded: popupRoot.mihomoExpanded
                        onDetailsClicked: popupRoot.toggleMihomoExpanded()
                    }

                    Row {
                        width: parent.width
                        spacing: popupRoot.cardSpacing

                        ControlPanelToggle {
                            width: (parent.width - popupRoot.cardSpacing) / 2
                            height: popupRoot.moduleSize
                            shellRoot: popupRoot.shellRoot
                            icon: "󰻃"
                            iconOnly: true
                            label: "Screen\nrecord"
                            active: popupRoot.shellRoot ? popupRoot.shellRoot.screenRecording : false
                            onClicked: popupRoot.toggleRecording()
                        }

                        ControlPanelToggle {
                            width: (parent.width - popupRoot.cardSpacing) / 2
                            height: popupRoot.moduleSize
                            shellRoot: popupRoot.shellRoot
                            icon: active ? "" : ""
                            iconOnly: true
                            label: "Prevent\nsleep"
                            active: popupRoot.shellRoot ? popupRoot.shellRoot.preventSleepEnabled : false
                            onClicked: popupRoot.togglePreventSleep()
                        }
                    }
                }

                Column {
                    width: popupRoot.columnWidth
                    spacing: popupRoot.cardSpacing

                    ControlPanelMediaCard {
                        width: parent.width
                        height: popupRoot.moduleSize * 2 + popupRoot.cardSpacing
                        shellRoot: popupRoot.shellRoot
                        available: popupRoot.shellRoot ? popupRoot.shellRoot.mediaAvailable : false
                        playing: popupRoot.shellRoot ? popupRoot.shellRoot.mediaPlaying : false
                        title: popupRoot.shellRoot ? popupRoot.shellRoot.mediaTitle : ""
                        subtitle: popupRoot.shellRoot ? popupRoot.shellRoot.mediaArtist : ""
                        playerName: popupRoot.shellRoot ? popupRoot.shellRoot.mediaPlayerName : ""
                        artUrl: popupRoot.shellRoot ? popupRoot.shellRoot.mediaArtUrl : ""
                        onPreviousClicked: popupRoot.handleMediaAction(["playerctl", "previous"])
                        onPlayPauseClicked: popupRoot.handleMediaAction(["playerctl", "play-pause"])
                        onNextClicked: popupRoot.handleMediaAction(["playerctl", "next"])
                    }

                    Row {
                        width: parent.width
                        spacing: popupRoot.cardSpacing

                        ControlPanelToggle {
                            width: (parent.width - popupRoot.cardSpacing) / 2
                            height: popupRoot.moduleSize
                            shellRoot: popupRoot.shellRoot
                            iconSource: Qt.resolvedUrl("assets/control-panel/light-dark.svg")
                            iconOnly: true
                            label: popupRoot.shellRoot && popupRoot.shellRoot.darkMode ? "Dark\nmode" : "Light\nmode"
                            active: popupRoot.shellRoot ? popupRoot.shellRoot.darkMode : false
                            onClicked: popupRoot.toggleTheme()
                        }

                        ControlPanelToggle {
                            width: (parent.width - popupRoot.cardSpacing) / 2
                            height: popupRoot.moduleSize
                            shellRoot: popupRoot.shellRoot
                            icon: "󰂛"
                            iconOnly: true
                            label: "No\ndisturb"
                            active: popupRoot.shellRoot ? popupRoot.shellRoot.dndEnabled : false
                            onClicked: popupRoot.toggleDnd()
                        }
                    }

                    Row {
                        width: parent.width
                        spacing: popupRoot.cardSpacing

                        ControlPanelToggle {
                            width: (parent.width - popupRoot.cardSpacing) / 2
                            height: popupRoot.moduleSize
                            shellRoot: popupRoot.shellRoot
                            icon: "󰍹"
                            iconOnly: true
                            active: popupRoot.shellRoot ? popupRoot.shellRoot.demoModeEnabled : false
                            onClicked: popupRoot.toggleDemoMode()
                        }

                        ControlPanelToggle {
                            width: (parent.width - popupRoot.cardSpacing) / 2
                            height: popupRoot.moduleSize
                            shellRoot: popupRoot.shellRoot
                            icon: popupRoot.powerProfileIcon(popupRoot.shellRoot ? popupRoot.shellRoot.powerProfile : "balanced")
                            iconOnly: true
                            active: false
                            onClicked: popupRoot.cyclePowerProfile()
                        }
                    }
                }
            }

            Grid {
                width: parent.width
                columns: 2
                spacing: popupRoot.cardSpacing

                ControlPanelSlider {
                    shellRoot: popupRoot.shellRoot
                    width: popupRoot.columnWidth
                    height: popupRoot.moduleSize
                    icon: "󰃟"
                    label: "Brightness"
                    value: popupRoot.shellRoot ? popupRoot.shellRoot.brightnessPercent : 50
                    accentColor: popupRoot.shellRoot ? popupRoot.shellRoot.brightnessColor : "#d47b1f"
                    onValueChangeRequested: function(newValue) { popupRoot.setBrightnessPercent(newValue); }
                }

                ControlPanelSlider {
                    shellRoot: popupRoot.shellRoot
                    width: popupRoot.columnWidth
                    height: popupRoot.moduleSize
                    icon: popupRoot.shellRoot ? popupRoot.shellRoot.volumeIcon : ""
                    iconClickable: true
                    label: "Volume"
                    value: popupRoot.shellRoot ? popupRoot.shellRoot.audioVolumePercent : 50
                    accentColor: popupRoot.shellRoot ? popupRoot.shellRoot.launchColor : "#89b4fa"
                    onValueChangeRequested: function(newValue) { popupRoot.setAudioVolumePercent(newValue); }
                    onIconClicked: popupRoot.toggleAudioMute()
                }

                ControlPanelSlider {
                    shellRoot: popupRoot.shellRoot
                    width: popupRoot.columnWidth
                    height: popupRoot.moduleSize
                    enabled: popupRoot.shellRoot ? popupRoot.shellRoot.fanControlAvailable : false
                    opacity: enabled ? 1 : 0.48
                    icon: "󰈐"
                    label: "Chassis Fan · " + (popupRoot.shellRoot ? popupRoot.shellRoot.chassisFanRpm : 0) + " RPM"
                    value: popupRoot.shellRoot ? popupRoot.shellRoot.chassisFanPercent : 0
                    accentColor: popupRoot.shellRoot ? popupRoot.shellRoot.usageLowColor : "#2f9e44"
                    onValueChangeRequested: function(newValue) { popupRoot.setFanPercent("chassis", newValue); }
                }

                ControlPanelSlider {
                    shellRoot: popupRoot.shellRoot
                    width: popupRoot.columnWidth
                    height: popupRoot.moduleSize
                    enabled: popupRoot.shellRoot ? popupRoot.shellRoot.fanControlAvailable : false
                    opacity: enabled ? 1 : 0.48
                    icon: "󰈐"
                    label: "Pump Fan · " + (popupRoot.shellRoot ? popupRoot.shellRoot.pumpFanRpm : 0) + " RPM"
                    value: popupRoot.shellRoot ? popupRoot.shellRoot.pumpFanPercent : 0
                    accentColor: popupRoot.shellRoot ? popupRoot.shellRoot.batteryColor : "#40a37d"
                    onValueChangeRequested: function(newValue) { popupRoot.setFanPercent("pump", newValue); }
                }
            }
        }
    }

    PanelWindow {
        id: popupWindow

        screen: popupRoot.parentWindow ? popupRoot.parentWindow.screen : null
        visible: false
        color: "transparent"
        aboveWindows: true
        focusable: visible
        exclusiveZone: -1

        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None
        WlrLayershell.namespace: "shell:hyprv-control-panel"

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
            if (!visible || !screen) {
                return;
            }
            let barBottom = popupRoot.parentWindow ? popupRoot.parentWindow.implicitHeight : 0;
            if (popupRoot.sourceItem) {
                const point = popupRoot.sourceItem.mapToGlobal(0, popupRoot.sourceItem.height);
                barBottom = Math.round(point.y - screen.y);
            }
            popupCard.x = Math.max(popupRoot.popupScreenMargin, width - popupCard.width - popupRoot.popupScreenMargin);
            popupCard.y = barBottom + popupRoot.popupScreenMargin;
        }

        onVisibleChanged: {
                if (visible) {
                    updatePopupPosition();
                    popupFocusScope.forceActiveFocus();
                    if (shellRoot) {
                        shellRoot.refreshBrightnessStatus();
                        shellRoot.refreshControlPanelStatus();
                        shellRoot.refreshMediaStatus();
                        shellRoot.refreshWifiStatus();
                    shellRoot.refreshBluetoothStatus();
                }
                if (!popupRoot.animatingClose) {
                    popupRoot.openAnimationPending = true;
                    popupCard.prepareOpenAnimation();
                    popupOpenTimer.restart();
                }
            } else {
                popupRoot.animatingClose = false;
                popupRoot.openAnimationPending = false;
                popupRoot.resetExpandedState();
                popupOpenTimer.stop();
                popupCard.stopAnimations();
                popupCard.resetAnimationState();
                popupRoot.sourceItem = null;
                popupRoot.parentWindow = null;
            }
        }

        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
            onClicked: popupRoot.closePopup()
        }

        FocusScope {
            id: popupFocusScope

            anchors.fill: parent
            focus: popupWindow.visible

            Keys.onEscapePressed: popupRoot.closePopup()
        }

        AnimatedGlassPanel {
            id: popupCard

            width: popupRoot.popupWidth
            fullPanelHeight: popupContent.implicitHeight + popupRoot.popupPadding * 2
            fillColor: popupRoot.currentPage !== "main" ? popupRoot.glassFill : "transparent"
            strokeColor: popupRoot.currentPage !== "main" ? popupRoot.glassStroke : "transparent"
            shadowColor: "transparent"
            devicePixelRatio: popupWindow.devicePixelRatio
            surfaceOpacity: popupRoot.currentPage !== "main" ? 0.82 : 0
            openRevealPause: popupRoot.currentPage === "bluetooth" ? 85 : 20
            openRevealDuration: popupRoot.currentPage === "bluetooth" ? 280 : 200
            openContentDelay: popupRoot.currentPage === "bluetooth" ? 60 : 20
            openFadeDuration: popupRoot.currentPage === "bluetooth" ? 180 : 140
            openSlideDuration: popupRoot.currentPage === "bluetooth" ? 220 : 180
            openContentOffset: popupRoot.currentPage === "bluetooth" ? -10 : -8
            closeRevealPause: popupRoot.currentPage === "bluetooth" ? 35 : 30
            closeRevealDuration: popupRoot.currentPage === "bluetooth" ? 210 : 180
            closeFadeDuration: popupRoot.currentPage === "bluetooth" ? 110 : 90
            closeSlideDuration: popupRoot.currentPage === "bluetooth" ? 170 : 150
            closeContentOffset: -8

            onFullPanelHeightChanged: {
                if (popupRoot.openAnimationPending) {
                    positionTimer.restart();
                    if (!popupCard.openAnimationRunning) {
                        popupOpenTimer.restart();
                    }
                    return;
                }
                if (popupWindow.visible && !popupRoot.animatingClose) {
                    if (popupCard.openAnimationRunning || popupCard.closeAnimationRunning) {
                        positionTimer.restart();
                        return;
                    }
                    revealHeight = fullPanelHeight;
                    contentOpacity = 1;
                    contentOffset = 0;
                } else if (!popupCard.openAnimationRunning && !popupCard.closeAnimationRunning) {
                    revealHeight = fullPanelHeight;
                    if (!popupWindow.visible) {
                        contentOpacity = 1;
                        contentOffset = 0;
                    }
                }
                positionTimer.restart();
            }

            onOpenAnimationFinished: {
                popupRoot.openAnimationPending = false;
                if (!popupWindow.visible || popupRoot.animatingClose) {
                    return;
                }
                positionTimer.restart();
            }

            onCloseAnimationFinished: {
                if (popupRoot.pendingPage.length > 0 && popupRoot.popupRequested && popupWindow.visible && !popupRoot.animatingClose) {
                    popupRoot.currentPage = popupRoot.pendingPage;
                    popupRoot.pendingPage = "";
                    positionTimer.restart();
                    popupRoot.animateCurrentPageOpen();
                    return;
                }
                if (popupRoot.animatingClose && !popupRoot.popupRequested) {
                    popupRoot.animatingClose = false;
                    popupWindow.visible = false;
                }
            }

            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
            }

            Item {
                id: popupContent

                anchors.fill: parent
                anchors.margins: popupRoot.popupPadding
                implicitHeight: pageLoader.item ? pageLoader.item.implicitHeight : 0
                onImplicitHeightChanged: {
                    if (popupRoot.openAnimationPending && !popupCard.openAnimationRunning) {
                        popupOpenTimer.restart();
                    }
                }

                Loader {
                    id: pageLoader

                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    sourceComponent: popupRoot.currentPage === "bluetooth"
                        ? bluetoothPageComponent
                        : (popupRoot.currentPage === "mihomo" ? mihomoPageComponent : mainPageComponent)
                }
            }
        }
    }
}
