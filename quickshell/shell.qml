//@ pragma UseQApplication

import QtQuick
import QtQuick.Effects
import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import Quickshell.Services.Pipewire
import Quickshell.Services.SystemTray
import Quickshell.Services.UPower
import Quickshell.Wayland
import Quickshell.Widgets

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
    property bool wifiDevicePresent: false
    property bool wifiRadioEnabled: false
    property bool wifiHardwareEnabled: true
    property bool wifiConnected: false
    property real wifiSignalStrength: 0
    property string wifiInterface: ""
    property string wifiSsid: ""
    property bool wifiSecure: false
    property var wifiNetworks: []
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
    property real _previousRxBytes: -1
    property real _previousTxBytes: -1
    property string _previousInterface: ""

    readonly property color moduleBackground: withAlpha(darkMode ? "#1e1e2e" : "#e7e7ec", 0.8)
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
    readonly property color mediaInactiveColor: darkMode ? "#6c7086" : "#808080"
    readonly property color workspaceHoverBackground: darkMode ? "#000000" : activeWorkspaceBackground
    readonly property string baseFont: "JetBrainsMono Nerd Font"
    readonly property string iconFont: "NotoSansMono Nerd Font"
    readonly property int trayMenuTextPixelSize: 14

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
    readonly property var audioSink: Pipewire.defaultAudioSink
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
    readonly property string volumeIcon: {
        const audio = audioSink?.audio;
        if (!audio) {
            return "";
        }
        if (audio.muted) {
            return "";
        }
        const percent = Math.round(audio.volume * 100);
        if (percent <= 0) {
            return "";
        }
        if (percent < 50) {
            return "";
        }
        return "";
    }
    readonly property string networkIcon: {
        if (!defaultInterface) {
            return "󰤮";
        }
        return defaultInterface.startsWith("wl") ? "󰖩" : "󰈀";
    }
    readonly property string networkText: defaultInterface ? humanRate(networkRxRate + networkTxRate) : "nocon"
    readonly property bool wifiWidgetVisible: wifiDevicePresent || wifiNetworks.length > 0 || defaultInterface.startsWith("wl")
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

    component PollCommand: Item {
        id: poll

        property var command: []
        property int interval: 1000
        property bool active: true
        property string output: ""

        signal updated(string output, int exitCode)

        function refresh() {
            if (!active || !command || command.length === 0 || proc.running) {
                return;
            }
            proc.command = command;
            proc.running = true;
        }

        Timer {
            interval: poll.interval
            repeat: true
            running: poll.active
            triggeredOnStart: true
            onTriggered: poll.refresh()
        }

        Process {
            id: proc

            running: false
            stdout: StdioCollector {
                id: collector
            }
            stderr: StdioCollector {}

            onExited: function(exitCode) {
                const text = (collector.text || "").trim();
                if (exitCode === 0) {
                    poll.output = text;
                }
                poll.updated(text, exitCode);
            }
        }
    }

    component GroupPill: Rectangle {
        id: pill

        default property alias contentData: contentRow.data

        radius: 10
        color: root.moduleBackground
        implicitWidth: contentRow.implicitWidth
        implicitHeight: 37
        width: implicitWidth
        height: implicitHeight

        Row {
            id: contentRow

            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            spacing: 0
        }
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

    component TrayButton: Item {
        id: trayButton

        property var trayItem: null
        property var parentWindow: null
        property string iconSource: root.trayIconSource(trayItem)

        implicitWidth: 18
        implicitHeight: 37

        IconImage {
            id: trayIcon
            anchors.verticalCenter: parent.verticalCenter
            width: 18
            height: 18
            source: trayButton.iconSource
            asynchronous: true
            smooth: true
            mipmap: true
            visible: status === Image.Ready
        }

        Text {
            anchors.centerIn: parent
            visible: !trayIcon.visible
            text: {
                const id = trayButton.trayItem?.id || "";
                return id ? id.charAt(0).toUpperCase() : "?";
            }
            color: root.primaryText
            font.family: root.baseFont
            font.pixelSize: 10
        }

        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            cursorShape: Qt.PointingHandCursor

            onClicked: function(mouse) {
                if (!trayButton.trayItem) {
                    return;
                }
                if (mouse.button === Qt.LeftButton) {
                    if (!trayButton.trayItem.onlyMenu) {
                        trayButton.trayItem.activate();
                    } else if (trayButton.trayItem.hasMenu) {
                        root.openTrayMenu(trayButton.trayItem, trayButton, trayButton.parentWindow);
                    }
                } else if (mouse.button === Qt.RightButton) {
                    if (trayButton.trayItem.hasMenu) {
                        root.openTrayMenu(trayButton.trayItem, trayButton, trayButton.parentWindow);
                    } else if (typeof trayButton.trayItem.secondaryActivate === "function") {
                        trayButton.trayItem.secondaryActivate();
                    }
                }
            }

            onWheel: function(wheel) {
                if (!trayButton.trayItem) {
                    return;
                }
                const delta = wheel.angleDelta.y > 0 ? 1 : -1;
                trayButton.trayItem.scroll(delta, false);
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
        readonly property color glassFill: withAlpha(root.darkMode ? "#101214" : "#f2f4f7", root.darkMode ? 0.42 : 0.34)
        readonly property color glassStroke: withAlpha(root.primaryText, root.darkMode ? 0.14 : 0.10)
        readonly property color hoverFill: withAlpha(root.primaryText, root.darkMode ? 0.10 : 0.12)
        readonly property var rootMenuEntry: menuHandle?.menu || null
        property bool menuVisible: false
        property bool animatingClose: false

        function topEntry() {
            return entryStack.count ? entryStack.get(entryStack.count - 1).handle : null;
        }

        function hydrateMenu(handle) {
            if (!handle) {
                return;
            }
            submenuHydrator.menu = handle;
            submenuHydrator.open();
            Qt.callLater(function() {
                submenuHydrator.close();
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
                menuChrome.playOpenAnimation();
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
                        menuChrome.playOpenAnimation();
                    }
                } else {
                    if (trayMenuPopupRoot.rootMenuEntry && typeof trayMenuPopupRoot.rootMenuEntry.sendClosed === "function") {
                        trayMenuPopupRoot.rootMenuEntry.sendClosed();
                    }
                    trayMenuPopupRoot.animatingClose = false;
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

            Item {
                id: menuChrome

                readonly property real lineHeight: 2
                readonly property real fullPanelHeight: Math.min(trayMenuPopupRoot.menuMaxHeight, menuContent.implicitHeight + trayMenuPopupRoot.menuPadding * 2)
                property real revealHeight: 0
                property real contentOpacity: 0
                property real contentOffset: -8

                width: trayMenuPopupRoot.menuWidth
                height: fullPanelHeight

                function resetAnimationState() {
                    revealHeight = fullPanelHeight;
                    contentOpacity = 1;
                    contentOffset = 0;
                }

                function playOpenAnimation() {
                    stopAnimations();
                    revealHeight = lineHeight;
                    contentOpacity = 0;
                    contentOffset = -8;
                    menuOpenAnimation.restart();
                }

                function playCloseAnimation() {
                    stopAnimations();
                    menuCloseAnimation.restart();
                }

                function stopAnimations() {
                    menuOpenAnimation.stop();
                    menuCloseAnimation.stop();
                }

                onFullPanelHeightChanged: {
                    if (trayMenuWindow.visible && !trayMenuPopupRoot.animatingClose) {
                        if (menuOpenAnimation.running) {
                            menuOpenAnimation.stop();
                        }
                        revealHeight = fullPanelHeight;
                        contentOpacity = 1;
                        contentOffset = 0;
                    } else if (!menuOpenAnimation.running && !menuCloseAnimation.running) {
                        revealHeight = fullPanelHeight;
                        if (!trayMenuWindow.visible) {
                            contentOpacity = 1;
                            contentOffset = 0;
                        }
                    }
                    positionTimer.restart();
                }

                SequentialAnimation {
                    id: menuOpenAnimation

                    ParallelAnimation {
                        NumberAnimation {
                            target: menuChrome
                            property: "revealHeight"
                            to: menuChrome.fullPanelHeight
                            duration: trayMenuPopupRoot.animationDuration
                            easing.type: Easing.OutCubic
                        }

                        SequentialAnimation {
                            PauseAnimation {
                                duration: 20
                            }

                            ParallelAnimation {
                                NumberAnimation {
                                    target: menuChrome
                                    property: "contentOpacity"
                                    to: 1
                                    duration: 140
                                    easing.type: Easing.OutQuad
                                }

                                NumberAnimation {
                                    target: menuChrome
                                    property: "contentOffset"
                                    to: 0
                                    duration: 180
                                    easing.type: Easing.OutCubic
                                }
                            }
                        }
                    }
                }

                ParallelAnimation {
                    id: menuCloseAnimation

                    NumberAnimation {
                        target: menuChrome
                        property: "contentOpacity"
                        to: 0
                        duration: 90
                        easing.type: Easing.InQuad
                    }

                    NumberAnimation {
                        target: menuChrome
                        property: "contentOffset"
                        to: -6
                        duration: 150
                        easing.type: Easing.InCubic
                    }

                    NumberAnimation {
                        target: menuChrome
                        property: "revealHeight"
                        to: menuChrome.lineHeight
                        duration: trayMenuPopupRoot.animationDuration
                        easing.type: Easing.InCubic
                    }

                    onFinished: {
                        if (trayMenuPopupRoot.animatingClose && !trayMenuPopupRoot.menuVisible) {
                            trayMenuPopupRoot.animatingClose = false;
                            trayMenuWindow.visible = false;
                        }
                    }
                }

                Item {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    height: menuChrome.revealHeight
                    clip: true

                    Rectangle {
                        id: menuPanel

                        x: 0
                        y: 0
                        width: trayMenuPopupRoot.menuWidth
                        height: menuChrome.fullPanelHeight
                        radius: 10
                        color: "transparent"
                        border.width: 0
                        antialiasing: true
                        clip: true

                        Item {
                            id: menuShadowLayer

                            anchors.fill: parent
                            layer.enabled: true
                            layer.smooth: true
                            layer.textureSize: Qt.size(Math.round(width * trayMenuWindow.devicePixelRatio), Math.round(height * trayMenuWindow.devicePixelRatio))
                            layer.textureMirroring: ShaderEffectSource.MirrorVertically

                            readonly property int blurMax: 64

                            layer.effect: MultiEffect {
                                autoPaddingEnabled: true
                                shadowEnabled: true
                                blurEnabled: false
                                maskEnabled: false
                                shadowBlur: 10 / menuShadowLayer.blurMax
                                shadowScale: 1
                                shadowColor: root.darkMode ? withAlpha("#000000", 0.45) : withAlpha("#111111", 0.18)
                            }

                            Rectangle {
                                anchors.fill: parent
                                radius: 10
                                color: trayMenuPopupRoot.glassFill
                                border.width: 1
                                border.color: trayMenuPopupRoot.glassStroke
                                antialiasing: true
                            }
                        }

                        Flickable {
                            anchors.fill: parent
                            anchors.margins: trayMenuPopupRoot.menuPadding
                            clip: true
                            contentWidth: width
                            contentHeight: menuContent.implicitHeight
                            opacity: menuChrome.contentOpacity
                            y: menuChrome.contentOffset

                            Column {
                                id: menuContent

                                width: parent.width
                                spacing: 1
                                onImplicitHeightChanged: positionTimer.restart()

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
        }
    }

    function withAlpha(colorString, alpha) {
        const color = Qt.color(colorString);
        return Qt.rgba(color.r, color.g, color.b, alpha);
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

    function parseKeyValueMap(text) {
        const result = {};
        const lines = (text || "").split("\n");
        for (let i = 0; i < lines.length; i++) {
            const line = lines[i];
            const separator = line.indexOf("=");
            if (separator <= 0) {
                continue;
            }
            result[line.slice(0, separator)] = line.slice(separator + 1);
        }
        return result;
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

    function normalizeWifiSignalDbm(signalDbm) {
        if (signalDbm === undefined || signalDbm === null || isNaN(signalDbm)) {
            return 0;
        }
        return Math.max(0, Math.min(1, (signalDbm + 90) / 50));
    }

    function wifiIconName(enabled, connected, strength) {
        if (!enabled) {
            return "network-wireless-off-symbolic";
        }
        if (!connected) {
            return "network-wireless-disconnected-symbolic";
        }
        if (strength < 0.2) {
            return "network-wireless-connected-00-symbolic";
        }
        if (strength < 0.4) {
            return "network-wireless-connected-25-symbolic";
        }
        if (strength < 0.6) {
            return "network-wireless-connected-50-symbolic";
        }
        if (strength < 0.8) {
            return "network-wireless-connected-75-symbolic";
        }
        return "network-wireless-connected-100-symbolic";
    }

    function wifiIconSource(enabled, connected, strength) {
        return wifiTrayIconSource(enabled, wifiHardwareEnabled, connected, strength, wifiSecure);
    }

    function wifiListIconName(signalPercent, secure) {
        const suffix = secure ? "-secure" : "";
        if (signalPercent < 20) {
            return "network-wireless" + suffix + "-signal-none";
        }
        if (signalPercent < 40) {
            return "network-wireless" + suffix + "-signal-low";
        }
        if (signalPercent < 60) {
            return "network-wireless" + suffix + "-signal-ok";
        }
        if (signalPercent < 80) {
            return "network-wireless" + suffix + "-signal-good";
        }
        return "network-wireless" + suffix + "-signal-excellent";
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

    function updateWifiStatus(raw) {
        if (!raw) {
            wifiDevicePresent = false;
            wifiRadioEnabled = false;
            wifiHardwareEnabled = true;
            wifiConnected = false;
            wifiInterface = "";
            wifiSsid = "";
            wifiSecure = false;
            wifiSignalStrength = 0;
            wifiNetworks = [];
            return;
        }
        try {
            const data = JSON.parse(raw);
            wifiDevicePresent = !!data.present;
            wifiRadioEnabled = !!data.enabled;
            wifiHardwareEnabled = data.hardwareEnabled !== false;
            wifiConnected = !!data.connected;
            wifiInterface = data.iface || "";
            wifiSsid = data.ssid || "";
            wifiSecure = (data.security || "").trim().length > 0;
            wifiSignalStrength = wifiConnected ? Math.max(0, Math.min(1, (Number(data.signal) || 0) / 100)) : 0;
            wifiNetworks = Array.isArray(data.networks) ? data.networks : [];

            if (!wifiDevicePresent) {
                wifiRadioEnabled = false;
                wifiConnected = false;
                wifiInterface = "";
                wifiSsid = "";
                wifiSecure = false;
                wifiSignalStrength = 0;
                wifiNetworks = [];
            }
        } catch (_) {
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
    }

    function updateSystemStats() {
        const sections = splitSections(systemSnapshot.output);
        const statLine = (sections.__STAT__ || [])[0] || "";
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
            return;
        }

        if (_previousInterface !== iface) {
            _previousInterface = iface;
            _previousRxBytes = counters.rx;
            _previousTxBytes = counters.tx;
            networkRxRate = 0;
            networkTxRate = 0;
            return;
        }

        if (_previousRxBytes >= 0 && _previousTxBytes >= 0) {
            networkRxRate = Math.max(0, counters.rx - _previousRxBytes);
            networkTxRate = Math.max(0, counters.tx - _previousTxBytes);
        }

        _previousRxBytes = counters.rx;
        _previousTxBytes = counters.tx;
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
        command: ["sh", "-lc", "readlink -f \"$HOME/.config/waybar/style.css\""]
        onUpdated: function(output, exitCode) {
            if (exitCode === 0) {
                root.darkMode = output.indexOf("-dark.css") >= 0;
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
        id: powerProfilePoll

        interval: 3000
        command: [root.configDir + "/waybar/scripts/power-profile.sh"]
        onUpdated: function(output, exitCode) {
            if (exitCode === 0 && output.length > 0) {
                root.powerProfileText = output;
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

    Variants {
        model: Quickshell.screens

        PanelWindow {
            id: barWindow

            required property var modelData

            screen: modelData

            WlrLayershell.layer: WlrLayer.Top
            WlrLayershell.namespace: "hyprv-v2-quickshell"

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
                        TextModule {
                            label: " " + Math.round(root.cpuUsage) + "%"
                            interactive: true
                            paddingLeft: 10
                            paddingRight: 4
                            onLeftClicked: root.runDetached(["alacritty", "-t", "btop", "-o", "window.startup_mode=Fullscreen", "-e", "btop"])
                        }

                        TextModule {
                            label: " " + Math.round(root.memoryUsage) + "%"
                            interactive: true
                            paddingLeft: 6
                            paddingRight: 4
                            onLeftClicked: root.runDetached(["alacritty", "-t", "btop", "-o", "window.startup_mode=Fullscreen", "-e", "btop"])
                        }

                        TextModule {
                            label: root.networkIcon + " " + root.networkText
                            paddingLeft: 6
                            paddingRight: 8
                        }
                    }

                }

                GroupPill {
                    id: centerSection
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
                        TextModule {
                            label: (root.temperatureC >= 70 ? " " + Math.round(root.temperatureC * 9 / 5 + 32) + "°F" : " " + Math.round(root.temperatureC) + "°C")
                            textColor: root.temperatureC >= 70 ? root.criticalColor : root.primaryText
                            interactive: true
                            paddingLeft: 10
                            paddingRight: 5
                            onLeftClicked: root.runDetached(["alacritty", "-t", "btop", "-o", "window.startup_mode=Fullscreen", "-e", "btop"])
                        }

                        TextModule {
                            label: root.batteryText
                            textColor: root.batteryCritical && !root.batteryCharging ? root.criticalColor : root.batteryColor
                            paddingLeft: 8
                            paddingRight: 8
                        }
                    }

                    GroupPill {
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
                            interactive: true
                            wheelInteractive: true
                            paddingLeft: 2
                            paddingRight: 10
                            onLeftClicked: root.runDetached([root.configDir + "/waybar/scripts/volume", "--toggle"])
                            onRightClicked: root.runDetached(["pavucontrol"])
                            onWheelUp: root.runDetached([root.configDir + "/waybar/scripts/volume", "--dec"])
                            onWheelDown: root.runDetached([root.configDir + "/waybar/scripts/volume", "--inc"])
                        }
                    }

                    GroupPill {
                        Item {
                            implicitWidth: wifiTrayLoader.item && wifiTrayLoader.item.available ? wifiTrayLoader.item.implicitWidth : 0
                            implicitHeight: 37
                            visible: implicitWidth > 0

                            Loader {
                                id: wifiTrayLoader
                                anchors.fill: parent
                                active: root.wifiWidgetVisible
                                source: Qt.resolvedUrl("WifiFallback.qml")

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
                                root.runDetached([root.configDir + "/waybar/scripts/power-profile.sh", "toggle"]);
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
                            onLeftClicked: root.runDetached([root.configDir + "/waybar/scripts/baraction"])
                        }
                    }
                }
            }
        }
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
