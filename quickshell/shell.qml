//@ pragma UseQApplication

import QtQuick
import QtQuick.Effects
import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import Quickshell.Services.SystemTray
import Quickshell.Services.UPower
import Quickshell.Wayland

ShellRoot {
    id: root

    readonly property string homeDir: Quickshell.env("HOME") || ""
    readonly property string configDir: homeDir + "/.config/HyprV"

    property bool darkMode: false
    property date now: new Date()
    property real cpuUsage: 0
    property real memoryUsage: 0
    property real temperatureC: 0
    property string defaultInterface: ""
    property real networkRxRate: 0
    property real networkTxRate: 0
    property var cpuHistory: []
    property var memoryHistory: []
    property var networkHistory: []
    property var cpuCoreUsages: []
    property bool wifiDevicePresent: false
    property bool wifiRadioEnabled: false
    property bool wifiHardwareEnabled: true
    property bool wifiConnected: false
    property real wifiSignalStrength: 0
    property string wifiInterface: ""
    property string wifiSsid: ""
    property bool wifiSecure: false
    property var wifiNetworks: []
    property bool wifiCapabilityDetected: false
    property string wifiActionMessage: ""
    property bool wifiActionBusy: false
    property string notificationAlt: "none"
    property string notificationTooltip: ""
    property string powerProfileText: "⚖️"
    property string fluentLightIconDir: ""
    property string fluentDarkIconDir: ""
    property string fluentBaseIconDir: ""
    property var trayMenuController: null

    property real _previousCpuTotal: -1
    property real _previousCpuIdle: -1
    property var _previousCpuCoreTotals: []
    property var _previousCpuCoreIdles: []
    property real _previousRxBytes: -1
    property real _previousTxBytes: -1
    property string _previousInterface: ""

    readonly property real pillOpacity: 0.8
    readonly property color moduleBackground: withAlpha(darkMode ? "#1e1e2e" : "#e7e7ec", pillOpacity)
    readonly property color primaryText: darkMode ? "#cdd6f4" : "#2b2b2c"
    readonly property color mutedWorkspaceText: darkMode ? "#575b6a" : "#859ABF"
    readonly property color activeWorkspaceText: darkMode ? "#0c0d14" : "#1b1b1b"
    readonly property color activeWorkspaceBackground: darkMode ? Qt.darker("#8e90cb", 1.05) : "#8EB6EC"
    readonly property color urgentWorkspaceText: "#11111b"
    readonly property color urgentWorkspaceBackground: "#a6e3a1"
    readonly property color launchColor: darkMode ? "#89b4fa" : "#407cdd"
    readonly property color batteryColor: darkMode ? "#a6e3a1" : "#1d7715"
    readonly property color microphoneColor: darkMode ? "#cba6f7" : "#ad6bfd"
    readonly property color criticalColor: "#e92d4d"
    readonly property color usageLowColor: darkMode ? "#7ad48b" : "#2f9e44"
    readonly property color usageMediumColor: darkMode ? "#f2d36b" : "#c99700"
    readonly property color mediaInactiveColor: darkMode ? "#6c7086" : "#808080"
    readonly property color workspaceHoverBackground: darkMode ? "#000000" : activeWorkspaceBackground
    readonly property color systemChartAccent: darkMode ? "#d7a26a" : "#b9782f"
    readonly property string baseFont: "JetBrainsMono Nerd Font"
    readonly property string iconFont: "JetBrainsMono Nerd Font"
    readonly property int trayMenuTextPixelSize: 14
    readonly property int statsHistoryLimit: 120

    readonly property var hyprWorkspaces: {
        const values = Array.from(Hyprland.workspaces?.values || []);
        const filtered = values.filter(ws => (ws?.id ?? -1) > -1);
        filtered.sort((a, b) => (a?.id ?? 0) - (b?.id ?? 0));
        return filtered.length > 0 ? filtered : [{
                id: 1,
                name: "1",
                urgent: false,
                active: true,
                activate: function () {}
            }];
    }
    readonly property int activeWorkspaceId: Hyprland.focusedWorkspace?.id || 1
    readonly property string activeWindowTitle: Hyprland.activeToplevel?.title || ""
    property bool audioAvailable: false
    property bool audioMuted: false
    property int audioVolumePercent: 0
    readonly property var batteryDevice: UPower.displayDevice
    readonly property real batteryPercent: {
        const percent = batteryDevice?.percentage;
        if (percent === undefined || percent === null || isNaN(percent)) {
            return 0;
        }
        return Math.max(0, Math.min(100, percent * 100));
    }
    readonly property bool batteryCharging: batteryDevice?.state === UPowerDeviceState.Charging || batteryDevice?.state === UPowerDeviceState.PendingCharge
    readonly property bool batteryPlugged: batteryCharging || batteryDevice?.state === UPowerDeviceState.FullyCharged
    readonly property bool batteryCritical: batteryPercent <= 20
    readonly property string batteryText: {
        if (!batteryDevice) {
            return "";
        }
        const rounded = Math.round(batteryPercent);
        if (batteryCharging || batteryPlugged) {
            return " " + rounded + "%";
        }
        return batteryGlyph(rounded) + " " + rounded + "%";
    }
    property var batteryInfo: ({
        available: false,
        status: "",
        mode: "unknown",
        capacity: 0,
        powerW: 0,
        averagePowerW: 0,
        sampleCount: 0,
        sampleWindowSeconds: 0,
        windowComplete: false,
        estimateSeconds: null,
        estimateBasis: "none",
        energyNowWh: 0,
        energyFullWh: 0
    })
    readonly property string batteryPopupTitle: batteryText.length > 0 ? batteryText : "电源"
    readonly property string batteryStatusText: {
        const mode = batteryInfo?.mode || "";
        if (mode === "charging") {
            return "正在充电";
        }
        if (mode === "discharging") {
            return "电池供电中";
        }
        if (mode === "full") {
            return "已充满";
        }
        if (mode === "plugged") {
            return "已接通电源，当前未充电";
        }
        return batteryInfo?.status || "电池状态未知";
    }
    readonly property color batteryDetailAccentColor: {
        const mode = batteryInfo?.mode || "";
        if (mode === "charging" || mode === "full" || mode === "plugged") {
            return root.batteryColor;
        }
        if (mode === "discharging" && root.batteryCritical) {
            return root.criticalColor;
        }
        return root.primaryText;
    }
    readonly property string batteryPowerDetailText: batteryInfo?.available ? formatPower(Number(batteryInfo?.powerW || 0), true) : "--"
    readonly property string batteryAveragePowerDetailText: batteryInfo?.available ? formatPower(Number(batteryInfo?.averagePowerW || 0), true) : "--"
    readonly property string batteryEstimateTitle: {
        const mode = batteryInfo?.mode || "";
        if (mode === "charging" || mode === "full" || mode === "plugged") {
            return "预计剩余充电时间";
        }
        return "预计剩余使用时间";
    }
    readonly property string batteryEstimateText: {
        const mode = batteryInfo?.mode || "";
        if (mode === "full") {
            return "已充满";
        }
        if (mode === "plugged") {
            return "已接通电源，当前未充电";
        }
        const seconds = Number(batteryInfo?.estimateSeconds);
        if (!isFinite(seconds) || seconds < 0) {
            return "计算中";
        }
        const basis = batteryInfo?.estimateBasis === "current" ? "按当前功率" : "按均值";
        return formatDuration(seconds) + " (" + basis + ")";
    }
    readonly property string batterySampleWindowText: {
        if (!batteryInfo?.available) {
            return "";
        }
        const seconds = Number(batteryInfo?.sampleWindowSeconds || 0);
        if (seconds >= 1800) {
            return "最近 30 分钟采样";
        }
        if (seconds >= 60) {
            return "已采样 " + formatDuration(seconds);
        }
        return "开始采样中";
    }
    readonly property string volumeIcon: {
        const percent = audioVolumePercent;
        if (!audioAvailable) {
            return "";
        }
        if (audioMuted) {
            return "";
        }
        if (percent <= 20) {
            return "";
        }
        if (percent <= 50) {
            return "";
        }
        return "";
    }
    function resetAudioState() {
        root.audioAvailable = false;
        root.audioMuted = false;
        root.audioVolumePercent = 0;
    }
    function updateAudioState(output) {
        let available = false;
        let muted = false;
        let volume = 0;
        const lines = (output || "").split("\n");
        for (let i = 0; i < lines.length; i++) {
            const line = lines[i].trim();
            if (line.startsWith("available=")) {
                available = line.slice(10).trim() === "true";
            } else if (line.startsWith("muted=")) {
                muted = line.slice(6).trim() === "true";
            } else if (line.startsWith("volume=")) {
                const parsed = Number(line.slice(7).trim());
                volume = isFinite(parsed) ? parsed : 0;
            }
        }
        root.audioAvailable = available;
        root.audioMuted = muted;
        root.audioVolumePercent = Math.max(0, Math.round(volume));
    }
    function isWifiInterfaceName(name) {
        const iface = (name || "").toLowerCase();
        return iface.startsWith("wl") || iface.startsWith("wlan") || iface.startsWith("wifi");
    }
    readonly property string networkIcon: {
        if (!defaultInterface) {
            return "󰤮";
        }
        return isWifiInterfaceName(defaultInterface) ? "󰖩" : "󰈀";
    }
    readonly property string networkText: defaultInterface ? humanRate(networkRxRate + networkTxRate) : "nocon"
    readonly property bool wifiWidgetVisible: wifiCapabilityDetected || wifiDevicePresent || wifiNetworks.length > 0 || isWifiInterfaceName(defaultInterface)
    readonly property bool notificationDoNotDisturb: notificationAlt.indexOf("dnd") >= 0
    readonly property bool notificationHasDot: notificationAlt.indexOf("notification") >= 0
    readonly property string notificationIcon: notificationDoNotDisturb ? "" : ""
    readonly property var sortedTrayItems: {
        const items = Array.from(SystemTray.items.values || []);
        const hideDedicatedWifiItems = root.wifiWidgetVisible;
        return items
        .filter(item => {
                if (!hideDedicatedWifiItems) {
                    return true;
                }
                const key = [item?.id || "", item?.title || "", item?.tooltipTitle || "", item?.tooltipDescription || "", item?.icon || ""].join(" ").toLowerCase();
                return key.indexOf("nm-applet") < 0 && key.indexOf("networkmanager") < 0 && key.indexOf("network-manager") < 0;
            })
        .map((item, index) => ({
                item: item,
                index: index,
                priority: trayItemPriority(item)
            }))
        .sort((a, b) => a.priority === b.priority ? a.index - b.index : a.priority - b.priority)
        .map(entry => entry.item);
    }

    component TextModule: Item {
        id: module

        property string label: ""
        property color textColor: root.primaryText
        property string fontFamily: root.baseFont
        property int fontPixelSize: 16
        property int fontWeight: Font.Bold
        property real paddingLeft: 8
        property real paddingRight: 8
        property real minimumWidth: 0
        property real moduleHeight: 37
        property bool interactive: false
        property bool wheelInteractive: false
        property bool hoverable: false
        property bool highlighted: false
        property color highlightColor: root.activeWorkspaceBackground
        property color highlightedTextColor: root.activeWorkspaceText
        property real highlightInset: 0
        readonly property real effectivePaddingLeft: Math.max(0, paddingLeft)
        readonly property real effectivePaddingRight: Math.max(0, paddingRight)

        signal leftClicked()
        signal rightClicked()
        signal wheelUp()
        signal wheelDown()

        implicitWidth: Math.max(labelText.implicitWidth + effectivePaddingLeft + effectivePaddingRight, minimumWidth)
        implicitHeight: module.moduleHeight

        Rectangle {
            anchors.fill: parent
            anchors.margins: module.highlightInset
            radius: 10
            color: highlighted ? module.highlightColor : root.workspaceHoverBackground
            visible: highlighted || (module.hoverable && mouseArea.containsMouse)
        }

        Text {
            id: labelText

            anchors.left: parent.left
            anchors.right: parent.right
            anchors.leftMargin: module.effectivePaddingLeft
            anchors.rightMargin: module.effectivePaddingRight
            anchors.verticalCenter: parent.verticalCenter
            text: module.label
            color: module.highlighted ? module.highlightedTextColor : module.textColor
            font.family: module.fontFamily
            font.pixelSize: module.fontPixelSize
            font.weight: module.fontWeight
            horizontalAlignment: Text.AlignHCenter
            renderType: Text.NativeRendering
        }

        MouseArea {
            id: mouseArea

            anchors.fill: parent
            enabled: module.interactive || module.wheelInteractive || module.hoverable
            hoverEnabled: module.hoverable || module.interactive || module.wheelInteractive
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            cursorShape: module.interactive || module.wheelInteractive ? Qt.PointingHandCursor : Qt.ArrowCursor

            onClicked: function(mouse) {
                if (mouse.button === Qt.LeftButton) {
                    module.leftClicked();
                } else if (mouse.button === Qt.RightButton) {
                    module.rightClicked();
                }
            }

            onWheel: function(wheel) {
                if (!module.wheelInteractive) {
                    return;
                }
                if (wheel.angleDelta.y > 0) {
                    module.wheelUp();
                } else if (wheel.angleDelta.y < 0) {
                    module.wheelDown();
                }
            }
        }
    }

    component TrayMenuPopup: Item {
        id: trayMenuPopupRoot

        property var trayItem: null
        property var sourceItem: null
        property var parentWindow: null
        property var menuHandle: null
        property int textPixelSize: root.trayMenuTextPixelSize
        readonly property int animationDuration: 200
        readonly property int rowHeight: Math.max(34, textPixelSize + 18)
        readonly property int menuPadding: 8
        readonly property int menuWidth: 300
        readonly property int menuMaxHeight: 420
        readonly property color glassFill: withAlpha(root.darkMode ? "#101214" : "#ffffff", root.darkMode ? 0.42 : 0.28)
        readonly property color glassStroke: withAlpha(root.primaryText, root.darkMode ? 0.14 : 0.10)
        readonly property color hoverFill: withAlpha(root.primaryText, root.darkMode ? 0.10 : 0.12)
        readonly property var rootMenuEntry: menuHandle?.menu || null
        property bool menuVisible: false
        property bool animatingClose: false
        property int hydratorSequence: 0
        property bool hydratorOpen: false
        property bool openAnimationPending: false

        function topEntry() {
            return entryStack.count ? entryStack.get(entryStack.count - 1).handle : null;
        }

        function hydrateMenu(handle) {
            if (!handle) {
                return;
            }
            hydratorSequence += 1;
            const sequence = hydratorSequence;
            if (hydratorOpen) {
                submenuHydrator.close();
                hydratorOpen = false;
            }
            submenuHydrator.menu = handle;
            submenuHydrator.open();
            hydratorOpen = true;
            Qt.callLater(function() {
                if (sequence !== hydratorSequence) {
                    return;
                }
                if (!hydratorOpen || !trayMenuWindow.visible) {
                    hydratorOpen = false;
                    return;
                }
                submenuHydrator.close();
                hydratorOpen = false;
            });
        }

        function entryIndicator(entry) {
            if (!entry || entry.buttonType === undefined || entry.buttonType === 0) {
                return "";
            }
            if (entry.buttonType === 1) {
                return entry.checkState === Qt.Checked ? "[x]" : "[ ]";
            }
            if (entry.buttonType === 2) {
                return entry.checkState === Qt.Checked ? "(o)" : "( )";
            }
            return "";
        }

        function scheduleOpenAnimation() {
            openAnimationPending = true;
            menuChrome.prepareOpenAnimation();
            openAnimationTimer.restart();
        }

        function openFor(item, source, window) {
            if (!item || !item.hasMenu || !source || !window) {
                return;
            }
            trayItem = item;
            sourceItem = source;
            parentWindow = window;
            menuHandle = item?.menu || null;
            entryStack.clear();
            animatingClose = false;
            menuVisible = true;
            positionTimer.restart();
            if (trayMenuWindow.visible) {
                if (rootMenuEntry && typeof rootMenuEntry.updateLayout === "function") {
                    rootMenuEntry.updateLayout();
                }
                if (rootMenuEntry && typeof rootMenuEntry.sendOpened === "function") {
                    rootMenuEntry.sendOpened();
                }
                hydrateMenu(rootMenuEntry || menuHandle);
                trayMenuWindow.updateMenuPosition();
                scheduleOpenAnimation();
            } else {
                trayMenuWindow.visible = true;
            }
        }

        function closeMenu() {
            if ((!menuVisible && !animatingClose) || !trayMenuWindow.visible) {
                menuVisible = false;
                animatingClose = false;
                return;
            }
            if (animatingClose) {
                return;
            }
            menuVisible = false;
            animatingClose = true;
            closeTimer.stop();
            menuChrome.playCloseAnimation();
        }

        function showSubMenu(entry) {
            if (!entry || !entry.hasChildren) {
                return;
            }
            entryStack.append({
                handle: entry
            });
            const handle = entry.menu || entry;
            if (handle && typeof handle.updateLayout === "function") {
                handle.updateLayout();
            }
            hydrateMenu(handle);
            positionTimer.restart();
        }

        function goBack() {
            if (!entryStack.count) {
                return;
            }
            entryStack.remove(entryStack.count - 1);
            positionTimer.restart();
        }

        function triggerEntry(entry) {
            if (!entry || entry.isSeparator || entry.enabled === false) {
                return;
            }
            if (entry.hasChildren) {
                showSubMenu(entry);
                return;
            }
            if (typeof entry.activate === "function") {
                entry.activate();
            } else if (typeof entry.triggered === "function") {
                entry.triggered();
            }
            closeTimer.restart();
        }

        Timer {
            id: positionTimer

            interval: 0
            repeat: false
            onTriggered: trayMenuWindow.updateMenuPosition()
        }

        Timer {
            id: openAnimationTimer

            interval: 16
            repeat: false
            onTriggered: {
                if (!trayMenuWindow.visible || !trayMenuPopupRoot.menuVisible || trayMenuPopupRoot.animatingClose) {
                    trayMenuPopupRoot.openAnimationPending = false;
                    return;
                }
                if (menuContent.implicitHeight <= 0) {
                    openAnimationTimer.restart();
                    return;
                }
                trayMenuPopupRoot.openAnimationPending = false;
                trayMenuWindow.updateMenuPosition();
                menuChrome.playOpenAnimation();
            }
        }

        Timer {
            id: closeTimer

            interval: 80
            repeat: false
            onTriggered: trayMenuPopupRoot.closeMenu()
        }

        Timer {
            id: clearTimer

            interval: 120
            repeat: false
            onTriggered: {
                if (trayMenuPopupRoot.menuVisible) {
                    return;
                }
                entryStack.clear();
                trayMenuPopupRoot.trayItem = null;
                trayMenuPopupRoot.sourceItem = null;
                trayMenuPopupRoot.parentWindow = null;
                trayMenuPopupRoot.menuHandle = null;
            }
        }

        ListModel {
            id: entryStack
        }

        QsMenuAnchor {
            id: submenuHydrator

            anchor.window: trayMenuWindow
        }

        QsMenuOpener {
            id: rootMenuOpener

            menu: trayMenuPopupRoot.rootMenuEntry || trayMenuPopupRoot.menuHandle || null
        }

        QsMenuOpener {
            id: submenuOpener

            menu: {
                const entry = trayMenuPopupRoot.topEntry();
                return entry ? (entry.menu || entry) : null;
            }
        }

        PanelWindow {
            id: trayMenuWindow

            screen: trayMenuPopupRoot.parentWindow?.screen || null
            visible: false
            color: "transparent"
            aboveWindows: true
            focusable: visible
            exclusiveZone: -1

            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None
            WlrLayershell.namespace: "shell:hyprv-tray-menu"

            anchors.top: true
            anchors.left: true
            anchors.right: true
            anchors.bottom: true

            function updateMenuPosition() {
                if (!visible || !trayMenuPopupRoot.sourceItem || !screen) {
                    return;
                }
                const point = trayMenuPopupRoot.sourceItem.mapToGlobal(Math.round(trayMenuPopupRoot.sourceItem.width / 2), trayMenuPopupRoot.sourceItem.height);
                const relativeX = point.x - screen.x;
                const relativeY = point.y - screen.y;
                const maxX = Math.max(8, width - menuChrome.width - 8);
                const desiredX = Math.round(relativeX - menuChrome.width / 2);
                menuChrome.x = Math.max(8, Math.min(maxX, desiredX));

                const belowY = Math.round(relativeY + 10);
                const aboveY = Math.round(relativeY - menuChrome.fullPanelHeight - 10);
                const fitsBelow = belowY + menuChrome.fullPanelHeight <= height - 8;
                const fitsAbove = aboveY >= 8;

                if (fitsBelow || !fitsAbove) {
                    menuChrome.y = Math.max(8, Math.min(height - menuChrome.fullPanelHeight - 8, belowY));
                } else {
                    menuChrome.y = Math.max(8, aboveY);
                }
            }

            onVisibleChanged: {
                if (visible) {
                    if (trayMenuPopupRoot.rootMenuEntry && typeof trayMenuPopupRoot.rootMenuEntry.updateLayout === "function") {
                        trayMenuPopupRoot.rootMenuEntry.updateLayout();
                    }
                    if (trayMenuPopupRoot.rootMenuEntry && typeof trayMenuPopupRoot.rootMenuEntry.sendOpened === "function") {
                        trayMenuPopupRoot.rootMenuEntry.sendOpened();
                    }
                    trayMenuPopupRoot.hydrateMenu(trayMenuPopupRoot.rootMenuEntry || trayMenuPopupRoot.menuHandle);
                    menuFocusScope.forceActiveFocus();
                    updateMenuPosition();
                    if (!trayMenuPopupRoot.animatingClose) {
                        trayMenuPopupRoot.scheduleOpenAnimation();
                    }
                } else {
                    if (trayMenuPopupRoot.rootMenuEntry && typeof trayMenuPopupRoot.rootMenuEntry.sendClosed === "function") {
                        trayMenuPopupRoot.rootMenuEntry.sendClosed();
                    }
                    trayMenuPopupRoot.animatingClose = false;
                    trayMenuPopupRoot.hydratorSequence += 1;
                    trayMenuPopupRoot.hydratorOpen = false;
                    trayMenuPopupRoot.openAnimationPending = false;
                    openAnimationTimer.stop();
                    menuChrome.stopAnimations();
                    menuChrome.resetAnimationState();
                    clearTimer.restart();
                }
            }

            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
                onClicked: trayMenuPopupRoot.closeMenu()
            }

            FocusScope {
                id: menuFocusScope

                anchors.fill: parent
                focus: trayMenuWindow.visible

                Keys.onEscapePressed: {
                    if (entryStack.count > 0) {
                        trayMenuPopupRoot.goBack();
                    } else {
                        trayMenuPopupRoot.closeMenu();
                    }
                }
            }

            AnimatedGlassPanel {
                id: menuChrome

                width: trayMenuPopupRoot.menuWidth
                fullPanelHeight: Math.min(trayMenuPopupRoot.menuMaxHeight, menuContent.implicitHeight + trayMenuPopupRoot.menuPadding * 2)
                fillColor: trayMenuPopupRoot.glassFill
                strokeColor: trayMenuPopupRoot.glassStroke
                shadowColor: root.darkMode ? withAlpha("#000000", 0.45) : withAlpha("#111111", 0.18)
                devicePixelRatio: trayMenuWindow.devicePixelRatio
                openRevealDuration: trayMenuPopupRoot.animationDuration
                openContentDelay: 20
                openFadeDuration: 140
                openSlideDuration: 180
                openContentOffset: -8
                closeRevealDuration: trayMenuPopupRoot.animationDuration
                closeFadeDuration: 90
                closeSlideDuration: 150
                closeContentOffset: -6

                onFullPanelHeightChanged: {
                    if (trayMenuPopupRoot.openAnimationPending) {
                        positionTimer.restart();
                        openAnimationTimer.restart();
                        return;
                    }
                    if (trayMenuWindow.visible && !trayMenuPopupRoot.animatingClose) {
                        if (menuChrome.openAnimationRunning || menuChrome.closeAnimationRunning) {
                            positionTimer.restart();
                            return;
                        }
                        revealHeight = fullPanelHeight;
                        contentOpacity = 1;
                        contentOffset = 0;
                    } else if (!menuChrome.openAnimationRunning && !menuChrome.closeAnimationRunning) {
                        revealHeight = fullPanelHeight;
                        if (!trayMenuWindow.visible) {
                            contentOpacity = 1;
                            contentOffset = 0;
                        }
                    }
                    positionTimer.restart();
                }

                onOpenAnimationFinished: {
                    if (!trayMenuWindow.visible || trayMenuPopupRoot.animatingClose) {
                        return;
                    }
                    positionTimer.restart();
                }

                onCloseAnimationFinished: {
                    if (trayMenuPopupRoot.animatingClose && !trayMenuPopupRoot.menuVisible) {
                        trayMenuPopupRoot.animatingClose = false;
                        trayMenuWindow.visible = false;
                    }
                }

                Flickable {
                    anchors.fill: parent
                    anchors.margins: trayMenuPopupRoot.menuPadding
                    clip: true
                    contentWidth: width
                    contentHeight: menuContent.implicitHeight

                    Column {
                        id: menuContent

                        width: parent.width
                        spacing: 1
                        onImplicitHeightChanged: {
                            positionTimer.restart();
                            if (trayMenuPopupRoot.openAnimationPending) {
                                openAnimationTimer.restart();
                            }
                        }

                        Rectangle {
                            width: parent.width
                            height: trayMenuPopupRoot.rowHeight
                            radius: 8
                            visible: entryStack.count > 0
                            color: backArea.containsMouse ? trayMenuPopupRoot.hoverFill : "transparent"

                            Text {
                                anchors.left: parent.left
                                anchors.leftMargin: 10
                                anchors.verticalCenter: parent.verticalCenter
                                text: "< Back"
                                color: root.primaryText
                                font.family: root.baseFont
                                font.pixelSize: trayMenuPopupRoot.textPixelSize
                                renderType: Text.NativeRendering
                            }

                            MouseArea {
                                id: backArea

                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: trayMenuPopupRoot.goBack()
                            }
                        }

                        Repeater {
                            model: entryStack.count > 0 ? (submenuOpener.children ? submenuOpener.children : (trayMenuPopupRoot.topEntry()?.children || [])) : rootMenuOpener.children

                            delegate: Rectangle {
                                required property var modelData

                                readonly property var menuEntry: modelData

                                width: menuContent.width
                                height: menuEntry?.isSeparator ? 1 : trayMenuPopupRoot.rowHeight
                                radius: menuEntry?.isSeparator ? 0 : 8
                                color: {
                                    if (menuEntry?.isSeparator) {
                                        return trayMenuPopupRoot.glassStroke;
                                    }
                                    if (itemArea.containsMouse && menuEntry?.enabled !== false) {
                                        return trayMenuPopupRoot.hoverFill;
                                    }
                                    return "transparent";
                                }

                                MouseArea {
                                    id: itemArea

                                    anchors.fill: parent
                                    hoverEnabled: true
                                    enabled: !menuEntry?.isSeparator && menuEntry?.enabled !== false
                                    acceptedButtons: Qt.LeftButton
                                    cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                                    onClicked: trayMenuPopupRoot.triggerEntry(menuEntry)
                                }

                                Item {
                                    anchors.fill: parent
                                    anchors.leftMargin: 10
                                    anchors.rightMargin: 10
                                    visible: !menuEntry?.isSeparator

                                    Text {
                                        id: indicatorText

                                        anchors.left: parent.left
                                        anchors.verticalCenter: parent.verticalCenter
                                        visible: text.length > 0
                                        text: trayMenuPopupRoot.entryIndicator(menuEntry)
                                        color: root.primaryText
                                        font.family: root.baseFont
                                        font.pixelSize: Math.max(11, trayMenuPopupRoot.textPixelSize - 1)
                                        renderType: Text.NativeRendering
                                    }

                                    Image {
                                        id: entryIcon

                                        anchors.left: indicatorText.visible ? indicatorText.right : parent.left
                                        anchors.leftMargin: indicatorText.visible ? 8 : 0
                                        anchors.verticalCenter: parent.verticalCenter
                                        visible: (menuEntry?.icon ?? "") !== ""
                                        width: 16
                                        height: 16
                                        source: menuEntry?.icon || ""
                                        sourceSize.width: 16
                                        sourceSize.height: 16
                                        fillMode: Image.PreserveAspectFit
                                        smooth: true
                                    }

                                    Text {
                                        id: submenuArrow

                                        anchors.right: parent.right
                                        anchors.verticalCenter: parent.verticalCenter
                                        visible: menuEntry?.hasChildren ?? false
                                        text: ">"
                                        color: root.primaryText
                                        font.family: root.baseFont
                                        font.pixelSize: trayMenuPopupRoot.textPixelSize
                                        renderType: Text.NativeRendering
                                    }

                                    Text {
                                        anchors.left: entryIcon.visible ? entryIcon.right : (indicatorText.visible ? indicatorText.right : parent.left)
                                        anchors.leftMargin: entryIcon.visible || indicatorText.visible ? 8 : 0
                                        anchors.right: submenuArrow.visible ? submenuArrow.left : parent.right
                                        anchors.rightMargin: submenuArrow.visible ? 8 : 0
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: menuEntry?.text || ""
                                        color: menuEntry?.enabled === false ? withAlpha(root.primaryText, 0.55) : root.primaryText
                                        font.family: root.baseFont
                                        font.pixelSize: trayMenuPopupRoot.textPixelSize
                                        elide: Text.ElideRight
                                        wrapMode: Text.NoWrap
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

    component BatteryInfoPopup: Item {
        id: batteryPopupRoot

        property var sourceItem: null
        property var parentWindow: null
        readonly property bool openVisible: popupRequested
        readonly property int popupWidth: 324
        readonly property int popupPadding: 12
        readonly property color glassFill: withAlpha(root.darkMode ? "#101214" : "#ffffff", root.darkMode ? 0.42 : 0.28)
        readonly property color glassStroke: withAlpha(root.primaryText, root.darkMode ? 0.14 : 0.10)
        readonly property color mutedTextColor: withAlpha(root.primaryText, root.darkMode ? 0.72 : 0.68)
        property bool popupRequested: false
        property bool animatingClose: false
        property bool openAnimationPending: false

        function openFor(source, window) {
            if (!source || !window) {
                return;
            }
            sourceItem = source;
            parentWindow = window;
            popupRequested = true;
            animatingClose = false;
            batteryInfoPoll.refresh();
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
            popupCard.playCloseAnimation();
        }

        function toggleFor(source, window) {
            if (popupWindow.visible && sourceItem === source && parentWindow === window) {
                closePopup();
                return;
            }
            openFor(source, window);
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
                if (!popupWindow.visible || !batteryPopupRoot.popupRequested || batteryPopupRoot.animatingClose) {
                    batteryPopupRoot.openAnimationPending = false;
                    return;
                }
                if (popupContent.implicitHeight <= 0) {
                    popupOpenTimer.restart();
                    return;
                }
                batteryPopupRoot.openAnimationPending = false;
                popupWindow.updatePopupPosition();
                popupCard.playOpenAnimation();
            }
        }

        PanelWindow {
            id: popupWindow

            screen: batteryPopupRoot.parentWindow?.screen || null
            visible: false
            color: "transparent"
            aboveWindows: true
            focusable: visible
            exclusiveZone: -1

            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None
            WlrLayershell.namespace: "shell:hyprv-battery-info"

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
                if (!visible || !batteryPopupRoot.sourceItem || !screen) {
                    return;
                }
                const point = batteryPopupRoot.sourceItem.mapToGlobal(Math.round(batteryPopupRoot.sourceItem.width / 2), batteryPopupRoot.sourceItem.height);
                const relativeX = point.x - screen.x;
                const relativeY = point.y - screen.y;
                const maxX = Math.max(8, width - popupCard.width - 8);
                const desiredX = Math.round(relativeX - popupCard.width / 2);
                popupCard.x = Math.max(8, Math.min(maxX, desiredX));

                const belowY = Math.round(relativeY + 10);
                const aboveY = Math.round(relativeY - popupCard.height - 10);
                const fitsBelow = belowY + popupCard.height <= height - 8;
                const fitsAbove = aboveY >= 8;

                if (fitsBelow || !fitsAbove) {
                    popupCard.y = Math.max(8, Math.min(height - popupCard.height - 8, belowY));
                } else {
                    popupCard.y = Math.max(8, aboveY);
                }
            }

            onVisibleChanged: {
                if (visible) {
                    updatePopupPosition();
                    popupFocusScope.forceActiveFocus();
                    if (!batteryPopupRoot.animatingClose) {
                        batteryPopupRoot.openAnimationPending = true;
                        popupCard.prepareOpenAnimation();
                        popupOpenTimer.restart();
                    }
                } else {
                    batteryPopupRoot.animatingClose = false;
                    batteryPopupRoot.openAnimationPending = false;
                    popupOpenTimer.stop();
                    popupCard.stopAnimations();
                    popupCard.resetAnimationState();
                    batteryPopupRoot.sourceItem = null;
                    batteryPopupRoot.parentWindow = null;
                }
            }

            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
                onClicked: batteryPopupRoot.closePopup()
            }

            FocusScope {
                id: popupFocusScope

                anchors.fill: parent
                focus: popupWindow.visible

                Keys.onEscapePressed: batteryPopupRoot.closePopup()
            }

            AnimatedGlassPanel {
                id: popupCard

                width: batteryPopupRoot.popupWidth
                fullPanelHeight: popupContent.implicitHeight + batteryPopupRoot.popupPadding * 2
                fillColor: batteryPopupRoot.glassFill
                strokeColor: batteryPopupRoot.glassStroke
                shadowColor: root.darkMode ? withAlpha("#000000", 0.45) : withAlpha("#111111", 0.18)
                devicePixelRatio: popupWindow.devicePixelRatio
                openRevealPause: 20
                openRevealDuration: 200
                openContentDelay: 20
                openFadeDuration: 140
                openSlideDuration: 180
                openContentOffset: -8
                closeRevealPause: 30
                closeRevealDuration: 180
                closeFadeDuration: 90
                closeSlideDuration: 150
                closeContentOffset: -8

                onFullPanelHeightChanged: {
                    if (batteryPopupRoot.openAnimationPending) {
                        positionTimer.restart();
                        popupOpenTimer.restart();
                        return;
                    }
                    if (popupWindow.visible && !batteryPopupRoot.animatingClose) {
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
                    if (!popupWindow.visible || batteryPopupRoot.animatingClose) {
                        return;
                    }
                    positionTimer.restart();
                }

                onCloseAnimationFinished: {
                    if (batteryPopupRoot.animatingClose && !batteryPopupRoot.popupRequested) {
                        batteryPopupRoot.animatingClose = false;
                        popupWindow.visible = false;
                    }
                }

                MouseArea {
                    anchors.fill: parent
                    acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
                }

                Column {
                    id: popupContent

                    anchors.fill: parent
                    anchors.margins: batteryPopupRoot.popupPadding
                    spacing: 10
                    onImplicitHeightChanged: {
                        if (batteryPopupRoot.openAnimationPending) {
                            popupOpenTimer.restart();
                        }
                    }

                    Text {
                        text: root.batteryPopupTitle
                        color: root.batteryDetailAccentColor
                        font.family: root.baseFont
                        font.pixelSize: 15
                        font.weight: Font.Bold
                        renderType: Text.NativeRendering
                    }

                    Text {
                        text: root.batteryStatusText
                        color: batteryPopupRoot.mutedTextColor
                        font.family: root.baseFont
                        font.pixelSize: 12
                        font.weight: Font.DemiBold
                        renderType: Text.NativeRendering
                        wrapMode: Text.WordWrap
                    }

                    Rectangle {
                        width: parent.width
                        height: 1
                        radius: 1
                        color: batteryPopupRoot.glassStroke
                    }

                    BatteryInfoLine {
                        shellRoot: root
                        width: parent.width
                        title: "当前净功率"
                        value: root.batteryPowerDetailText
                        valueColor: root.batteryDetailAccentColor
                    }

                    BatteryInfoLine {
                        shellRoot: root
                        width: parent.width
                        title: "半小时平均功率"
                        value: root.batteryAveragePowerDetailText
                    }

                    BatteryInfoLine {
                        shellRoot: root
                        width: parent.width
                        title: root.batteryEstimateTitle
                        value: root.batteryEstimateText
                    }

                    Text {
                        width: parent.width
                        text: root.batterySampleWindowText
                        visible: text.length > 0
                        color: batteryPopupRoot.mutedTextColor
                        font.family: root.baseFont
                        font.pixelSize: 11
                        font.weight: Font.Medium
                        horizontalAlignment: Text.AlignRight
                        renderType: Text.NativeRendering
                        wrapMode: Text.WordWrap
                    }
                }
            }
        }
    }

    function withAlpha(colorString, alpha) {
        const color = Qt.color(colorString);
        return Qt.rgba(color.r, color.g, color.b, alpha);
    }

    function formatPower(value, signed) {
        const number = Number(value);
        if (!isFinite(number)) {
            return "--";
        }
        const absolute = Math.abs(number);
        const decimals = absolute >= 10 ? 1 : 2;
        let prefix = "";
        if (signed) {
            if (number > 0.004) {
                prefix = "+";
            } else if (number < -0.004) {
                prefix = "-";
            }
        }
        return prefix + absolute.toFixed(decimals) + " W";
    }

    function formatDuration(totalSeconds) {
        const value = Number(totalSeconds);
        if (!isFinite(value) || value < 0) {
            return "计算中";
        }
        const roundedMinutes = Math.round(value / 60);
        if (roundedMinutes <= 0) {
            return "0分钟";
        }
        const hours = Math.floor(roundedMinutes / 60);
        const minutes = roundedMinutes % 60;
        if (hours > 0 && minutes > 0) {
            return hours + "小时" + minutes + "分钟";
        }
        if (hours > 0) {
            return hours + "小时";
        }
        return roundedMinutes + "分钟";
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

    function usageSeverityColor(value) {
        const number = Number(value);
        if (!isFinite(number)) {
            return primaryText;
        }
        if (number >= 90) {
            return criticalColor;
        }
        if (number >= 60) {
            return usageMediumColor;
        }
        return usageLowColor;
    }

    function appendHistory(history, value, limit) {
        const next = Array.isArray(history) ? history.slice(0) : [];
        next.push(Math.max(0, Number(value) || 0));
        if (next.length > limit) {
            next.splice(0, next.length - limit);
        }
        return next;
    }

    function splitSections(text) {
        const sections = {};
        let current = "";
        const lines = (text || "").split("\n");
        for (let i = 0; i < lines.length; i++) {
            const line = lines[i];
            if (line.startsWith("__") && line.endsWith("__")) {
                current = line;
                sections[current] = [];
            } else if (current) {
                sections[current].push(line);
            }
        }
        return sections;
    }

    function batteryGlyph(percent) {
        const icons = ["󰂎", "󰁺", "󰁻", "󰁼", "󰁽", "󰁾", "󰁿", "󰂀", "󰂁", "󰂂", "󰁹"];
        const index = Math.max(0, Math.min(icons.length - 1, Math.round(percent / 10)));
        return icons[index];
    }

    function resetBatteryInfo() {
        batteryInfo = {
            available: false,
            status: "",
            mode: "unknown",
            capacity: 0,
            powerW: 0,
            averagePowerW: 0,
            sampleCount: 0,
            sampleWindowSeconds: 0,
            windowComplete: false,
            estimateSeconds: null,
            estimateBasis: "none",
            energyNowWh: 0,
            energyFullWh: 0
        };
    }

    function updateBatteryInfo(raw) {
        if (!raw) {
            resetBatteryInfo();
            return;
        }
        try {
            const data = JSON.parse(raw);
            if (data && typeof data === "object") {
                batteryInfo = data;
                return;
            }
        } catch (_) {}
        resetBatteryInfo();
    }

    function parseNumberMap(text) {
        const result = {};
        const lines = (text || "").split("\n");
        for (let i = 0; i < lines.length; i++) {
            const match = lines[i].match(/^([A-Za-z_()]+):\s+(\d+)/);
            if (match) {
                result[match[1]] = parseInt(match[2], 10);
            }
        }
        return result;
    }

    function parseDefaultInterface(text) {
        const lines = (text || "").trim().split("\n");
        for (let i = 1; i < lines.length; i++) {
            const parts = lines[i].trim().split(/\s+/);
            if (parts.length >= 8 && parts[1] === "00000000" && parts[7] === "00000000") {
                return parts[0];
            }
        }
        return "";
    }

    function interfaceCounters(text, iface) {
        if (!iface) {
            return null;
        }
        const lines = (text || "").split("\n");
        for (let i = 2; i < lines.length; i++) {
            const line = lines[i].trim();
            if (!line.startsWith(iface + ":")) {
                continue;
            }
            const parts = line.replace(":", " ").trim().split(/\s+/);
            if (parts.length < 10) {
                return null;
            }
            return {
                rx: parseInt(parts[1], 10),
                tx: parseInt(parts[9], 10)
            };
        }
        return null;
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

    function updateNotificationState(raw) {
        if (!raw) {
            notificationAlt = "none";
            notificationTooltip = "";
            return;
        }
        try {
            const data = JSON.parse(raw);
            notificationAlt = data.alt || "none";
            notificationTooltip = data.tooltip || "";
        } catch (_) {
            notificationAlt = "none";
            notificationTooltip = "";
        }
    }

    function resetWifiStatus() {
        wifiDevicePresent = false;
        wifiRadioEnabled = false;
        wifiHardwareEnabled = true;
        wifiConnected = false;
        wifiInterface = "";
        wifiSsid = "";
        wifiSecure = false;
        wifiSignalStrength = 0;
        wifiNetworks = [];
    }

    function applyWifiStatus(data) {
        const devicePresent = !!data.present;
        const iface = data.iface || "";

        resetWifiStatus();
        wifiHardwareEnabled = data.hardwareEnabled !== false;
        wifiDevicePresent = devicePresent;
        if (devicePresent || iface.length > 0) {
            wifiCapabilityDetected = true;
        }

        if (!devicePresent) {
            return;
        }

        wifiRadioEnabled = !!data.enabled;
        wifiConnected = !!data.connected;
        wifiInterface = iface;
        wifiSsid = data.ssid || "";
        wifiSecure = (data.security || "").trim().length > 0;
        wifiSignalStrength = wifiConnected ? Math.max(0, Math.min(1, (Number(data.signal) || 0) / 100)) : 0;
        wifiNetworks = Array.isArray(data.networks) ? data.networks : [];
    }

    function updateWifiStatus(raw) {
        if (!raw) {
            resetWifiStatus();
            return;
        }
        try {
            applyWifiStatus(JSON.parse(raw));
        } catch (_) {
            resetWifiStatus();
        }
    }

    function updateSystemStats() {
        const sections = splitSections(systemSnapshot.output);
        const statLines = sections.__STAT__ || [];
        const statLine = statLines[0] || "";
        if (statLine) {
            const values = statLine.trim().split(/\s+/).slice(1).map(v => parseInt(v, 10));
            const idle = (values[3] || 0) + (values[4] || 0);
            let total = 0;
            for (let i = 0; i < values.length; i++) {
                total += values[i] || 0;
            }
            if (_previousCpuTotal >= 0 && total > _previousCpuTotal) {
                const totalDiff = total - _previousCpuTotal;
                const idleDiff = idle - _previousCpuIdle;
                cpuUsage = Math.max(0, Math.min(100, (1 - idleDiff / totalDiff) * 100));
            }
            _previousCpuTotal = total;
            _previousCpuIdle = idle;
        }

        const nextCpuCoreTotals = [];
        const nextCpuCoreIdles = [];
        const nextCpuCoreUsages = [];
        for (let i = 1; i < statLines.length; i++) {
            const line = statLines[i] || "";
            if (!/^cpu\d+\s/.test(line)) {
                continue;
            }
            const values = line.trim().split(/\s+/).slice(1).map(v => parseInt(v, 10));
            const idle = (values[3] || 0) + (values[4] || 0);
            let total = 0;
            for (let j = 0; j < values.length; j++) {
                total += values[j] || 0;
            }
            const coreIndex = nextCpuCoreTotals.length;
            let usage = coreIndex < cpuCoreUsages.length ? Math.max(0, Math.min(100, Number(cpuCoreUsages[coreIndex]) || 0)) : 0;
            const previousTotal = coreIndex < _previousCpuCoreTotals.length ? Number(_previousCpuCoreTotals[coreIndex]) : -1;
            const previousIdle = coreIndex < _previousCpuCoreIdles.length ? Number(_previousCpuCoreIdles[coreIndex]) : -1;
            if (previousTotal >= 0 && total > previousTotal) {
                const totalDiff = total - previousTotal;
                const idleDiff = idle - previousIdle;
                usage = Math.max(0, Math.min(100, (1 - idleDiff / totalDiff) * 100));
            }
            nextCpuCoreTotals.push(total);
            nextCpuCoreIdles.push(idle);
            nextCpuCoreUsages.push(usage);
        }
        _previousCpuCoreTotals = nextCpuCoreTotals;
        _previousCpuCoreIdles = nextCpuCoreIdles;
        cpuCoreUsages = nextCpuCoreUsages;

        const mem = parseNumberMap((sections.__MEM__ || []).join("\n"));
        const memTotal = mem.MemTotal || 0;
        const memAvailable = mem.MemAvailable || (mem.MemFree || 0) + (mem.Buffers || 0) + (mem.Cached || 0);
        if (memTotal > 0) {
            memoryUsage = ((memTotal - memAvailable) / memTotal) * 100;
        }

        const tempRaw = parseFloat((sections.__TEMP__ || []).join("\n").trim());
        if (!isNaN(tempRaw)) {
            temperatureC = tempRaw > 1000 ? tempRaw / 1000 : tempRaw;
        }

        const iface = parseDefaultInterface((sections.__ROUTE__ || []).join("\n"));
        defaultInterface = iface;
        const counters = interfaceCounters((sections.__NET__ || []).join("\n"), iface);
        if (!iface || !counters) {
            networkRxRate = 0;
            networkTxRate = 0;
            _previousRxBytes = -1;
            _previousTxBytes = -1;
            _previousInterface = "";
        } else if (_previousInterface !== iface) {
            _previousInterface = iface;
            _previousRxBytes = counters.rx;
            _previousTxBytes = counters.tx;
            networkRxRate = 0;
            networkTxRate = 0;
        } else {
            if (_previousRxBytes >= 0 && _previousTxBytes >= 0) {
                networkRxRate = Math.max(0, counters.rx - _previousRxBytes);
                networkTxRate = Math.max(0, counters.tx - _previousTxBytes);
            }

            _previousRxBytes = counters.rx;
            _previousTxBytes = counters.tx;
        }

        cpuHistory = appendHistory(cpuHistory, cpuUsage, statsHistoryLimit);
        memoryHistory = appendHistory(memoryHistory, memoryUsage, statsHistoryLimit);
        networkHistory = appendHistory(networkHistory, networkRxRate + networkTxRate, statsHistoryLimit);
    }

    function trayItemPriority(item) {
        const id = (item?.id || "").toLowerCase();
        const title = (item?.title || "").toLowerCase();
        const tooltipTitle = (item?.tooltipTitle || "").toLowerCase();
        const tooltipDescription = (item?.tooltipDescription || "").toLowerCase();
        const icon = (item?.icon || "").toLowerCase();
        const key = [id, title, tooltipTitle, tooltipDescription, icon].join(" ");

        if (id === "chrome_status_icon_1" && !tooltipTitle && !tooltipDescription) {
            return 0;
        }
        if (key.indexOf("discord") >= 0) {
            return 1;
        }
        if (key.indexOf("keepass") >= 0 || key.indexOf("password.kdbx") >= 0) {
            return 2;
        }
        if (key.indexOf("local-ai-service") >= 0 || key.indexOf("lais_gui") >= 0 || key.indexOf("preferences-system") >= 0) {
            return 3;
        }
        if (key.indexOf("懒猫") >= 0 || key.indexOf("微服") >= 0) {
            return 4;
        }
        if (key.indexOf("nm-applet") >= 0 || key.indexOf("wi-fi") >= 0 || key.indexOf("网络") >= 0) {
            return 5;
        }
        if (key.indexOf("fcitx") >= 0 || key.indexOf("输入法") >= 0) {
            return 6;
        }
        if (key.indexOf("wechat") >= 0 || key.indexOf("微信") >= 0) {
            return 7;
        }
        return 100;
    }

    function trayIconSource(item) {
        let icon = item?.icon || "";
        if (!icon) {
            return "";
        }
        if (icon.includes("?path=")) {
            const split = icon.split("?path=");
            if (split.length === 2) {
                let fileName = split[0].substring(split[0].lastIndexOf("/") + 1);
                if (fileName.startsWith("dropboxstatus")) {
                    fileName = "hicolor/16x16/status/" + fileName;
                }
                return "file://" + split[1] + "/" + fileName;
            }
        }
        if (icon.startsWith("/") && !icon.startsWith("file://")) {
            return "file://" + icon;
        }
        if (icon.indexOf("image://") === 0 || icon.indexOf("qrc:/") === 0 || icon.indexOf("file://") === 0 || icon.indexOf("http://") === 0 || icon.indexOf("https://") === 0) {
            return icon;
        }
        return "image://icon/" + icon;
    }

    function openTrayMenu(item, sourceItem, parentWindow) {
        if (!item || !item.hasMenu || !sourceItem || !parentWindow) {
            return;
        }
        if (trayMenuController) {
            trayMenuController.openFor(item, sourceItem, parentWindow);
        }
    }

    function openWifiManager() {
        runDetached(["sh", "-lc", "if command -v nm-connection-editor >/dev/null 2>&1; then exec nm-connection-editor; elif command -v iwgtk >/dev/null 2>&1; then exec iwgtk; else exec alacritty -t nmtui -e nmtui; fi"]);
    }

    function refreshWifiStatus() {
        wifiStatusPoll.refresh();
    }

    function startWifiAction(command, pendingMessage, successMessage) {
        if (!command || command.length === 0 || wifiActionRunner.running) {
            return;
        }
        wifiActionBusy = true;
        wifiActionMessage = pendingMessage || "";
        _wifiActionSuccessMessage = successMessage || "";
        wifiActionRunner.command = command;
        wifiActionRunner.running = true;
    }

    function wifiSetRadio(enabled) {
        startWifiAction(["sh", root.configDir + "/quickshell/scripts/wifi-action.sh", "toggle", enabled ? "on" : "off"], enabled ? "Turning Wi-Fi on..." : "Turning Wi-Fi off...", enabled ? "Wi-Fi enabled" : "Wi-Fi disabled");
    }

    function wifiRescan() {
        startWifiAction(["sh", root.configDir + "/quickshell/scripts/wifi-action.sh", "rescan"], "Scanning for networks...", "Scan started");
    }

    function wifiDisconnect() {
        startWifiAction(["sh", root.configDir + "/quickshell/scripts/wifi-action.sh", "disconnect"], "Disconnecting...", "Disconnected");
    }

    function wifiConnect(ssid, password, security) {
        const command = ["sh", root.configDir + "/quickshell/scripts/wifi-action.sh", "connect", ssid || "", password || "", security || ""];
        startWifiAction(command, "Connecting to " + (ssid || "network") + "...", "Connection requested for " + (ssid || "network"));
    }

    function runDetached(command) {
        if (!command || command.length === 0) {
            return;
        }
        detachedRunner.command = command;
        detachedRunner.startDetached();
    }

    Process {
        id: fluentIconLocator

        running: true
        command: ["sh", "-lc", "for base in \"$HOME/.local/share/icons\" /usr/local/share/icons /usr/share/icons; do [ -z \"$light\" ] && [ -d \"$base/Fluent-light\" ] && light=\"$base/Fluent-light\"; [ -z \"$dark\" ] && [ -d \"$base/Fluent-dark\" ] && dark=\"$base/Fluent-dark\"; [ -z \"$base_theme\" ] && [ -d \"$base/Fluent\" ] && base_theme=\"$base/Fluent\"; done; printf 'light=%s\\ndark=%s\\nbase=%s\\n' \"$light\" \"$dark\" \"$base_theme\""]
        stdout: StdioCollector {
            id: fluentIconLocatorStdout
        }

        onExited: function(exitCode) {
            if (exitCode !== 0) {
                return;
            }
            const lines = (fluentIconLocatorStdout.text || "").trim().split("\n");
            for (let i = 0; i < lines.length; i++) {
                const line = lines[i];
                if (line.startsWith("light=")) {
                    root.fluentLightIconDir = line.slice(6).trim();
                } else if (line.startsWith("dark=")) {
                    root.fluentDarkIconDir = line.slice(5).trim();
                } else if (line.startsWith("base=")) {
                    root.fluentBaseIconDir = line.slice(5).trim();
                }
            }
        }
    }

    Process {
        id: detachedRunner

        running: false
    }

    property string _wifiActionSuccessMessage: ""

    Process {
        id: wifiActionRunner

        running: false
        stdout: StdioCollector {
            id: wifiActionStdout
        }
        stderr: StdioCollector {
            id: wifiActionStderr
        }

        onExited: function(exitCode) {
            const stdout = (wifiActionStdout.text || "").trim();
            const stderr = (wifiActionStderr.text || "").trim();
            wifiActionBusy = false;
            if (exitCode === 0) {
                wifiActionMessage = stdout.length > 0 ? stdout : _wifiActionSuccessMessage;
            } else {
                wifiActionMessage = stderr.length > 0 ? stderr : (stdout.length > 0 ? stdout : "Wi-Fi action failed");
            }
            wifiStatusPoll.refresh();
            wifiFollowupRefresh.restart();
        }
    }

    Timer {
        id: wifiFollowupRefresh

        interval: 1500
        repeat: false
        onTriggered: wifiStatusPoll.refresh()
    }

    Timer {
        id: audioFollowupRefresh

        interval: 150
        repeat: false
        onTriggered: audioStatusPoll.refresh()
    }

    Timer {
        interval: 1000
        repeat: true
        running: true
        triggeredOnStart: true
        onTriggered: root.now = new Date()
    }

    PollCommand {
        id: systemSnapshot

        interval: 1000
        command: ["sh", "-lc", "printf '__STAT__\\n'; cat /proc/stat; printf '\\n__MEM__\\n'; cat /proc/meminfo; printf '\\n__TEMP__\\n'; cat /sys/class/thermal/thermal_zone1/temp; printf '\\n__ROUTE__\\n'; cat /proc/net/route; printf '\\n__NET__\\n'; cat /proc/net/dev"]
        onUpdated: function(output, exitCode) {
            if (exitCode === 0 && output.length > 0) {
                root.updateSystemStats();
            }
        }
    }

    PollCommand {
        id: themePoll

        interval: 2000
        command: [root.configDir + "/quickshell/scripts/ui-state.sh", "print"]
        onUpdated: function(output, exitCode) {
            if (exitCode === 0) {
                root.darkMode = output.indexOf("theme=dark") >= 0;
            }
        }
    }

    PollCommand {
        id: wifiStatusPoll

        interval: 8000
        command: ["sh", root.configDir + "/quickshell/scripts/wifi-status.sh"]
        onUpdated: function(output, exitCode) {
            if (exitCode === 0) {
                root.updateWifiStatus(output);
            } else {
                root.updateWifiStatus("");
            }
        }
    }

    PollCommand {
        id: notificationPoll

        interval: 2000
        command: ["sh", "-lc", "swaync-client -swb | head -n 1"]
        onUpdated: function(output, exitCode) {
            if (exitCode === 0) {
                root.updateNotificationState((output || "").split("\n")[0] || "");
            }
        }
    }

    PollCommand {
        id: audioStatusPoll

        interval: 750
        command: ["sh", root.configDir + "/quickshell/scripts/audio-status.sh"]
        onUpdated: function(output, exitCode) {
            if (exitCode === 0) {
                root.updateAudioState(output);
            } else {
                root.resetAudioState();
            }
        }
    }

    PollCommand {
        id: powerProfilePoll

        interval: 3000
        command: [root.configDir + "/quickshell/scripts/power-profile.sh"]
        onUpdated: function(output, exitCode) {
            if (exitCode === 0 && output.length > 0) {
                root.powerProfileText = output;
            }
        }
    }

    PollCommand {
        id: batteryInfoPoll

        interval: batteryInfoPopup.openVisible ? 1000 : 30000
        command: ["sh", root.configDir + "/quickshell/scripts/battery-info.sh"]
        onUpdated: function(output, exitCode) {
            if (exitCode === 0) {
                root.updateBatteryInfo(output);
            } else {
                root.resetBatteryInfo();
            }
        }
    }

    TrayMenuPopup {
        id: trayMenuPopup

        textPixelSize: root.trayMenuTextPixelSize
        Component.onCompleted: root.trayMenuController = this
        Component.onDestruction: if (root.trayMenuController === this) {
            root.trayMenuController = null;
        }
    }

    BatteryInfoPopup {
        id: batteryInfoPopup
    }

    SystemStatsPopup {
        id: systemStatsPopup

        shellRoot: root
    }

    Variants {
        model: Quickshell.screens

        PanelWindow {
            id: barWindow

            required property var modelData

            screen: modelData

            WlrLayershell.layer: WlrLayer.Top
            WlrLayershell.namespace: "hyprv-quickshell"

            anchors.top: true
            anchors.left: true
            anchors.right: true

            implicitHeight: 47
            exclusiveZone: implicitHeight + 10
            margins.bottom: 10
            color: "transparent"

            Item {
                id: contentRoot

                anchors.fill: parent

                Row {
                    id: leftSection

                    anchors.left: parent.left
                    anchors.leftMargin: 10
                    anchors.top: parent.top
                    anchors.topMargin: 10
                    spacing: 9.5

                    GroupPill {
                        shellRoot: root
                        TextModule {
                            label: ""
                            textColor: root.launchColor
                            fontFamily: root.baseFont
                            fontPixelSize: 17
                            moduleHeight: 37
                            interactive: true
                            paddingLeft: 6
                            paddingRight: 3
                            onLeftClicked: root.runDetached(["rofi", "-show", "drun"])
                        }

                        Item {
                            implicitWidth: workspaceRow.implicitWidth + 4
                            implicitHeight: 37

                            Row {
                                id: workspaceRow

                                x: 4
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 0

                                Repeater {
                                    model: root.hyprWorkspaces

                                    delegate: TextModule {
                                        required property var modelData

                                        label: String(modelData.name || modelData.id)
                                        textColor: root.mutedWorkspaceText
                                        interactive: true
                                        hoverable: true
                                        moduleHeight: 37
                                        paddingLeft: 5
                                        paddingRight: 5
                                        minimumWidth: 30
                                        highlightInset: 0
                                        highlighted: root.activeWorkspaceId === modelData.id || (modelData.urgent && root.activeWorkspaceId !== modelData.id)
                                        highlightColor: modelData.urgent && root.activeWorkspaceId !== modelData.id ? root.urgentWorkspaceBackground : root.activeWorkspaceBackground
                                        highlightedTextColor: modelData.urgent && root.activeWorkspaceId !== modelData.id ? root.urgentWorkspaceText : root.activeWorkspaceText
                                        onLeftClicked: modelData.activate()
                                    }
                                }
                            }
                        }
                    }

                    GroupPill {
                        shellRoot: root
                        TextModule {
                            id: cpuTrigger

                            label: " " + Math.round(root.cpuUsage) + "%"
                            interactive: true
                            paddingLeft: 10
                            paddingRight: 4
                            onLeftClicked: systemStatsPopup.toggleFor(cpuTrigger, barWindow)
                            onRightClicked: root.runDetached(["alacritty", "-t", "btop", "-o", "window.startup_mode=Fullscreen", "-e", "btop"])
                        }

                        TextModule {
                            id: memoryTrigger

                            label: " " + Math.round(root.memoryUsage) + "%"
                            interactive: true
                            paddingLeft: 6
                            paddingRight: 4
                            onLeftClicked: systemStatsPopup.toggleFor(memoryTrigger, barWindow)
                            onRightClicked: root.runDetached(["alacritty", "-t", "btop", "-o", "window.startup_mode=Fullscreen", "-e", "btop"])
                        }

                        TextModule {
                            id: networkTrigger

                            label: root.networkIcon + " " + root.networkText
                            interactive: true
                            paddingLeft: 6
                            paddingRight: 8
                            onLeftClicked: systemStatsPopup.toggleFor(networkTrigger, barWindow)
                        }
                    }

                }

                GroupPill {
                    id: centerSection
                    shellRoot: root
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.top: parent.top
                    anchors.topMargin: 10

                    TextModule {
                        label: ""
                        interactive: true
                        paddingLeft: 11
                        paddingRight: 11
                        onLeftClicked: root.runDetached(["hyprlock"])
                    }

                    TextModule {
                        label: Qt.formatTime(root.now, "hh:mm")
                        paddingLeft: 0
                        paddingRight: 0
                    }

                    TextModule {
                        label: ""
                        interactive: true
                        paddingLeft: 11
                        paddingRight: 11
                        onLeftClicked: root.runDetached(["wlogout", "--protocol", "layer-shell", "-b", "5"])
                    }
                }

                Rectangle {
                    id: windowSection

                    readonly property real availableWidth: Math.max(0, centerSection.x - (leftSection.x + leftSection.width) - 19)

                    anchors.top: parent.top
                    anchors.topMargin: 10
                    x: leftSection.x + leftSection.width + 9.5
                    width: Math.min(windowSection.availableWidth, windowLabel.implicitWidth + 16)
                    height: 37
                    radius: 10
                    color: root.moduleBackground
                    visible: root.activeWindowTitle.length > 0 && width > 0

                    Text {
                        id: windowLabel

                        anchors.left: parent.left
                        anchors.leftMargin: 8
                        anchors.right: parent.right
                        anchors.rightMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        text: root.activeWindowTitle
                        color: root.primaryText
                        font.family: root.baseFont
                        font.pixelSize: 16
                        font.weight: Font.Bold
                        renderType: Text.NativeRendering
                        elide: Text.ElideRight
                    }
                }

                Row {
                    id: rightSection

                    anchors.right: parent.right
                    anchors.rightMargin: 9.5
                    anchors.top: parent.top
                    anchors.topMargin: 10
                    spacing: 9.5

                    GroupPill {
                        shellRoot: root
                        TextModule {
                            label: (root.temperatureC >= 70 ? " " : " ") + Math.round(root.temperatureC) + "°C"
                            textColor: root.temperatureC >= 70 ? root.criticalColor : root.primaryText
                            interactive: true
                            paddingLeft: 10
                            paddingRight: 5
                            onLeftClicked: root.runDetached(["alacritty", "-t", "btop", "-o", "window.startup_mode=Fullscreen", "-e", "btop"])
                        }

                        Item {
                            id: batteryTrigger

                            width: batteryModule.implicitWidth
                            height: batteryModule.implicitHeight
                            implicitWidth: batteryModule.implicitWidth
                            implicitHeight: batteryModule.implicitHeight

                            TextModule {
                                id: batteryModule

                                anchors.fill: parent
                                label: root.batteryText
                                textColor: root.batteryCritical && !root.batteryCharging ? root.criticalColor : root.batteryColor
                                interactive: root.batteryText.length > 0
                                paddingLeft: 8
                                paddingRight: 8
                                onLeftClicked: batteryInfoPopup.toggleFor(batteryTrigger, barWindow)
                            }
                        }
                    }

                    GroupPill {
                        shellRoot: root
                        TextModule {
                            label: ""
                            interactive: true
                            paddingLeft: 8
                            paddingRight: 5
                            onLeftClicked: root.runDetached(["playerctl", "previous"])
                        }

                        TextModule {
                            label: ""
                            interactive: true
                            paddingLeft: 5
                            paddingRight: 5
                            onLeftClicked: root.runDetached(["playerctl", "play-pause"])
                        }

                        TextModule {
                            label: ""
                            interactive: true
                            paddingLeft: 0
                            paddingRight: 4
                            onLeftClicked: root.runDetached(["playerctl", "next"])
                        }

                        TextModule {
                            label: root.volumeIcon
                            textColor: root.launchColor
                            fontFamily: root.iconFont
                            interactive: true
                            wheelInteractive: true
                            paddingLeft: 2
                            paddingRight: 10
                            onLeftClicked: {
                                root.runDetached([root.configDir + "/hypr/scripts/volume", "--toggle"]);
                                audioFollowupRefresh.restart();
                            }
                            onRightClicked: root.runDetached(["pavucontrol"])
                            onWheelUp: {
                                root.runDetached([root.configDir + "/hypr/scripts/volume", "--dec"]);
                                audioFollowupRefresh.restart();
                            }
                            onWheelDown: {
                                root.runDetached([root.configDir + "/hypr/scripts/volume", "--inc"]);
                                audioFollowupRefresh.restart();
                            }
                        }
                    }

                    GroupPill {
                        shellRoot: root
                        Item {
                            implicitWidth: wifiTrayLoader.item && wifiTrayLoader.item.available ? wifiTrayLoader.item.implicitWidth : 0
                            implicitHeight: 37
                            visible: implicitWidth > 0

                            Loader {
                                id: wifiTrayLoader
                                anchors.fill: parent
                                active: root.wifiWidgetVisible
                                source: Qt.resolvedUrl("WifiNative.qml")

                                onLoaded: {
                                    if (item) {
                                        item.shellRoot = root;
                                        item.parentWindow = barWindow;
                                    }
                                }
                            }
                        }

                        Item {
                            implicitWidth: trayRow.implicitWidth > 0 ? trayRow.implicitWidth +2 : 0
                            implicitHeight: 37
                            visible: trayRow.implicitWidth > 0

                            Row {
                                id: trayRow

                                x: 1
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 7

                                Repeater {
                                    model: root.sortedTrayItems

                                    delegate: TrayButton {
                                        required property var modelData

                                        shellRoot: root
                                        trayItem: modelData
                                        parentWindow: barWindow
                                    }
                                }
                            }
                        }

                        TextModule {
                            label: root.powerProfileText
                            fontPixelSize: 14
                            interactive: true
                            paddingLeft: 10
                            paddingRight: 5
                            onLeftClicked: {
                                root.runDetached([root.configDir + "/quickshell/scripts/power-profile.sh", "toggle"]);
                                powerProfileRefresh.restart();
                            }
                        }

                        Item {
                            implicitWidth: notificationGlyph.implicitWidth + 10
                            implicitHeight: 37

                            Text {
                                id: notificationGlyph

                                anchors.centerIn: parent
                                text: root.notificationIcon
                                color: root.primaryText
                                font.family: root.iconFont
                                font.pixelSize: 14
                                font.weight: Font.Bold
                                renderType: Text.NativeRendering
                            }

                            Text {
                                anchors.top: parent.top
                                anchors.right: parent.right
                                anchors.topMargin: 4
                                anchors.rightMargin: 2
                                text: ""
                                visible: root.notificationHasDot
                                color: "#ff0000"
                                font.family: root.iconFont
                                font.pixelSize: 10
                                font.weight: Font.Bold
                                renderType: Text.NativeRendering
                            }

                            MouseArea {
                                anchors.fill: parent
                                acceptedButtons: Qt.LeftButton | Qt.RightButton
                                cursorShape: Qt.PointingHandCursor

                                onClicked: function(mouse) {
                                    if (mouse.button === Qt.LeftButton) {
                                        root.runDetached(["swaync-client", "-t", "-sw"]);
                                    } else if (mouse.button === Qt.RightButton) {
                                        root.runDetached(["swaync-client", "-d", "-sw"]);
                                        notificationRefresh.restart();
                                    }
                                }
                            }
                        }

                        TextModule {
                            label: "󰐾"
                            interactive: true
                            paddingLeft: 6
                            paddingRight: 12
                            onLeftClicked: {
                                root.runDetached([root.configDir + "/quickshell/scripts/toggle-theme.sh"]);
                                themeRefresh.restart();
                            }
                        }
                    }
                }
            }
        }
    }

    Timer {
        id: themeRefresh

        interval: 350
        repeat: false
        onTriggered: themePoll.refresh()
    }

    Timer {
        id: notificationRefresh

        interval: 350
        repeat: false
        onTriggered: notificationPoll.refresh()
    }

    Timer {
        id: powerProfileRefresh

        interval: 350
        repeat: false
        onTriggered: powerProfilePoll.refresh()
    }
}
