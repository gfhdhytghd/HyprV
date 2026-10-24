import QtQuick
import ".."
import "../../../../../quickshell"

Item {
    id: popupRoot
    width: 548; height: statsColumn.implicitHeight + 32

    property var shellRoot: sh
    property bool popupRequested: true
    property bool animatingClose: false
    property bool openAnimationPending: false

    readonly property int popupScreenMargin: 10
    readonly property int popupWidth: 548
    readonly property int popupPadding: 16
    readonly property color glassFill: shellRoot ? shellRoot.withAlpha(shellRoot.darkMode ? "#101214" : "#ffffff", shellRoot.darkMode ? 0.42 : 0.28) : Qt.rgba(0.06, 0.07, 0.08, 0.42)
    readonly property color glassStroke: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.14 : 0.10) : Qt.rgba(0.8, 0.8, 0.8, 0.12)
    readonly property color mutedTextColor: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.72 : 0.68) : Qt.rgba(0.8, 0.8, 0.8, 0.7)
    readonly property color panelShadowColor: shellRoot ? (shellRoot.darkMode ? shellRoot.withAlpha("#000000", 0.45) : shellRoot.withAlpha("#111111", 0.18)) : Qt.rgba(0, 0, 0, 0.35)
    readonly property color metricTextColor: shellRoot ? (shellRoot.darkMode ? shellRoot.primaryText : "#000000") : "#000000"
    readonly property real panelSurfaceOpacity: 0.82
    readonly property color accentColor: shellRoot ? shellRoot.systemChartAccent : "#b9782f"
    readonly property color cpuChartColor: shellRoot ? shellRoot.usageSeverityColor(shellRoot.cpuUsage || 0) : "#2f9e44"
    readonly property color memoryChartColor: shellRoot ? shellRoot.usageSeverityColor(shellRoot.memoryUsage || 0) : "#2f9e44"
    readonly property color networkChartColor: shellRoot ? shellRoot.launchColor : "#407cdd"
    readonly property string cpuCurrentText: Math.round(shellRoot ? (shellRoot.cpuUsage || 0) : 0) + "%"
    readonly property string memoryCurrentText: Math.round(shellRoot ? (shellRoot.memoryUsage || 0) : 0) + "%"
    readonly property string networkCurrentText: shellRoot && shellRoot.defaultInterface ? shellRoot.humanRate((shellRoot.networkRxRate || 0) + (shellRoot.networkTxRate || 0)) : "未连接"
    readonly property string networkDetailText: shellRoot ? ("↓ " + shellRoot.humanRate(shellRoot.networkRxRate || 0) + "   ↑ " + shellRoot.humanRate(shellRoot.networkTxRate || 0)) : ""
    readonly property var cpuCoreDisplayData: {
        const values = shellRoot && Array.isArray(shellRoot.cpuCoreUsages) ? shellRoot.cpuCoreUsages : [];
        const display = [];
        for (let i = 0; i < 8; i++) {
            display.push({
                label: "C" + (i + 1),
                value: i < values.length ? values[i] : null
            });
        }
        return display;
    }

    function formatPercent(value) {
        const number = Number(value);
        if (!isFinite(number)) {
            return "--";
        }
        return Math.round(number) + "%";
    }

    function coreUsageColor(value) {
        const number = Number(value);
        if (!isFinite(number)) {
            return metricTextColor;
        }
        return metricTextColor;
    }

    function panelColor(colorValue) {
        return Qt.rgba(colorValue.r, colorValue.g, colorValue.b, colorValue.a * panelSurfaceOpacity);
    }

    QtObject {
        id: sh
        property real cpuUsage: 45.2
        property real memoryUsage: 70.4
        property var cpuCoreUsages: [12.0, 48.6, 7.4, 91.2, 33.3, 25.0, 60.5, 3.0]
        property real networkRxRate: 0.0
        property real networkTxRate: 0.0
        property var cpuHistory: []
        property var memoryHistory: []
        property var networkHistory: []
        readonly property color systemChartAccent: darkMode ? "#d7a26a" : "#b9782f"
        function usageSeverityColor(value) {{ const n = Number(value); if (!isFinite(n)) return primaryText; if (n >= 90) return criticalColor; if (n >= 60) return usageMediumColor; return usageLowColor; }}
        property bool darkMode: true
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
        property string defaultInterface: ""
        readonly property bool networkConnected: defaultInterface.length > 0
        readonly property string activeNetworkType: networkTypeForInterface(defaultInterface)
        readonly property bool wifiConnectionActive: activeNetworkType === "wifi"
        readonly property bool wiredConnectionActive: activeNetworkType === "wired"
        readonly property bool otherConnectionActive: activeNetworkType === "other"
        property bool wifiDevicePresent: false
        property bool wifiCapabilityDetected: false
        property bool wifiRadioEnabled: false
        property bool wifiHardwareEnabled: false
        property bool wifiConnected: false
        property real wifiSignalStrength: 0.0
        property string wifiSsid: ""
        property bool wifiSecure: false
        property string wifiInterface: ""
        property var wifiNetworks: []
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
    AnimatedGlassPanel {
        width: parent.width
        fullPanelHeight: parent.height
        fillColor: popupRoot.glassFill
        strokeColor: popupRoot.glassStroke
        shadowColor: popupRoot.panelShadowColor
        devicePixelRatio: 2
        revealHeight: fullPanelHeight
        contentOpacity: 1
        contentOffset: 0
    }
    Item {
        id: popupContent
        x: 16; y: 16; width: 516; height: parent.height - 32
                        Column {
            id: statsColumn
                    width: 516
                    spacing: 18
                    onImplicitHeightChanged: {
                        if (popupRoot.openAnimationPending) {
                            popupOpenTimer.restart();
                        }
                    }

                    Item {
                        width: parent.width
                        height: Math.max(titleLabel.implicitHeight, refreshLabel.implicitHeight)

                        Text {
                            id: titleLabel

                            anchors.left: parent.left
                            anchors.verticalCenter: parent.verticalCenter
                            text: "系统资源"
                            color: popupRoot.shellRoot ? popupRoot.shellRoot.primaryText : "#2b2b2c"
                            font.family: popupRoot.shellRoot ? popupRoot.shellRoot.baseFont : "monospace"
                            font.pixelSize: 15
                            font.weight: Font.Bold
                            renderType: Text.NativeRendering
                        }

                        Text {
                            id: refreshLabel

                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            text: "过去 120s · 1s"
                            color: popupRoot.mutedTextColor
                            font.family: popupRoot.shellRoot ? popupRoot.shellRoot.baseFont : "monospace"
                            font.pixelSize: 11
                            font.weight: Font.Medium
                            renderType: Text.NativeRendering
                        }
                    }

                    Column {
                        width: parent.width
                        spacing: 8

                        Item {
                            width: parent.width
                            height: Math.max(cpuTitle.implicitHeight, cpuValue.implicitHeight)

                            Text {
                                id: cpuTitle

                                anchors.left: parent.left
                                anchors.verticalCenter: parent.verticalCenter
                                text: "CPU:"
                                color: popupRoot.shellRoot ? popupRoot.shellRoot.primaryText : "#2b2b2c"
                                font.family: popupRoot.shellRoot ? popupRoot.shellRoot.baseFont : "monospace"
                                font.pixelSize: 16
                                font.weight: Font.Bold
                                renderType: Text.NativeRendering
                            }

                            Text {
                                id: cpuValue

                                anchors.left: cpuTitle.right
                                anchors.leftMargin: 8
                                anchors.verticalCenter: parent.verticalCenter
                                text: popupRoot.cpuCurrentText
                                color: popupRoot.metricTextColor
                                font.family: popupRoot.shellRoot ? popupRoot.shellRoot.baseFont : "monospace"
                                font.pixelSize: 16
                                font.weight: Font.Bold
                                renderType: Text.NativeRendering
                            }
                        }

                        Item {
                            width: parent.width
                            implicitHeight: Math.max(cpuChart.implicitHeight, cpuCoreGrid.implicitHeight)

                            SystemTrendChart {
                                active: popupRoot.popupRequested || popupRoot.animatingClose
                                id: cpuChart

                                shellRoot: popupRoot.shellRoot
                                samples: popupRoot.shellRoot ? popupRoot.shellRoot.cpuHistory : []
                                accentColor: popupRoot.cpuChartColor
                                width: Math.max(240, parent.width - cpuCoreGrid.implicitWidth - 16)
                                height: Math.max(112, cpuCoreGrid.implicitHeight)
                                maxValue: 100
                                autoScale: false
                            }

                            Grid {
                                id: cpuCoreGrid

                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                columns: 2
                                rowSpacing: 8
                                columnSpacing: 10

                                Repeater {
                                    model: popupRoot.cpuCoreDisplayData

                                    delegate: Rectangle {
                                        required property var modelData

                                        width: 74
                                        height: 32
                                        radius: 10
                                        color: popupRoot.panelColor(popupRoot.shellRoot ? popupRoot.shellRoot.withAlpha(popupRoot.shellRoot.primaryText, popupRoot.shellRoot.darkMode ? 0.06 : 0.04) : Qt.rgba(0.2, 0.2, 0.2, 0.04))
                                        border.width: 1
                                        border.color: popupRoot.glassStroke
                                        antialiasing: true

                                        Text {
                                            anchors.centerIn: parent
                                            text: modelData.label + " " + popupRoot.formatPercent(modelData.value)
                                            color: popupRoot.coreUsageColor(modelData.value)
                                            font.family: popupRoot.shellRoot ? popupRoot.shellRoot.baseFont : "monospace"
                                            font.pixelSize: 13
                                            font.weight: Font.Bold
                                            renderType: Text.NativeRendering
                                        }
                                    }
                                }
                            }
                        }
                    }

                    Column {
                        width: parent.width
                        spacing: 8

                        Item {
                            width: parent.width
                            height: Math.max(ramTitle.implicitHeight, ramValue.implicitHeight)

                            Text {
                                id: ramTitle

                                anchors.left: parent.left
                                anchors.verticalCenter: parent.verticalCenter
                                text: "RAM:"
                                color: popupRoot.shellRoot ? popupRoot.shellRoot.primaryText : "#2b2b2c"
                                font.family: popupRoot.shellRoot ? popupRoot.shellRoot.baseFont : "monospace"
                                font.pixelSize: 16
                                font.weight: Font.Bold
                                renderType: Text.NativeRendering
                            }

                            Text {
                                id: ramValue

                                anchors.left: ramTitle.right
                                anchors.leftMargin: 8
                                anchors.verticalCenter: parent.verticalCenter
                                text: popupRoot.memoryCurrentText
                                color: popupRoot.metricTextColor
                                font.family: popupRoot.shellRoot ? popupRoot.shellRoot.baseFont : "monospace"
                                font.pixelSize: 16
                                font.weight: Font.Bold
                                renderType: Text.NativeRendering
                            }
                        }

                        SystemTrendChart {
                            active: popupRoot.popupRequested || popupRoot.animatingClose
                            shellRoot: popupRoot.shellRoot
                            samples: popupRoot.shellRoot ? popupRoot.shellRoot.memoryHistory : []
                            accentColor: popupRoot.memoryChartColor
                            width: parent.width
                            height: 112
                            maxValue: 100
                            autoScale: false
                        }
                    }

                    Column {
                        width: parent.width
                        spacing: 8

                        Item {
                            width: parent.width
                            height: Math.max(networkTitle.implicitHeight, networkValue.implicitHeight, networkDetail.implicitHeight)

                            Text {
                                id: networkTitle

                                anchors.left: parent.left
                                anchors.verticalCenter: parent.verticalCenter
                                text: "Network:"
                                color: popupRoot.shellRoot ? popupRoot.shellRoot.primaryText : "#2b2b2c"
                                font.family: popupRoot.shellRoot ? popupRoot.shellRoot.baseFont : "monospace"
                                font.pixelSize: 16
                                font.weight: Font.Bold
                                renderType: Text.NativeRendering
                            }

                            Text {
                                id: networkValue

                                anchors.left: networkTitle.right
                                anchors.leftMargin: 8
                                anchors.verticalCenter: parent.verticalCenter
                                text: popupRoot.networkCurrentText
                                color: popupRoot.metricTextColor
                                font.family: popupRoot.shellRoot ? popupRoot.shellRoot.baseFont : "monospace"
                                font.pixelSize: 16
                                font.weight: Font.Bold
                                renderType: Text.NativeRendering
                            }

                            Text {
                                id: networkDetail

                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                text: popupRoot.networkDetailText
                                color: popupRoot.metricTextColor
                                font.family: popupRoot.shellRoot ? popupRoot.shellRoot.baseFont : "monospace"
                                font.pixelSize: 11
                                font.weight: Font.Medium
                                renderType: Text.NativeRendering
                            }
                        }

                        SystemTrendChart {
                            active: popupRoot.popupRequested || popupRoot.animatingClose
                            shellRoot: popupRoot.shellRoot
                            samples: popupRoot.shellRoot ? popupRoot.shellRoot.networkHistory : []
                            accentColor: popupRoot.networkChartColor
                            width: parent.width
                            height: 112
                            autoScale: true
                        }
                    }
                }
    }
}
