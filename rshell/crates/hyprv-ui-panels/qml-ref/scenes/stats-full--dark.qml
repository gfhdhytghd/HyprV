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
        property real networkRxRate: 1572864.0
        property real networkTxRate: 90112.0
        property var cpuHistory: [52.2, 54.232660339751575, 55.23108499806537, 55.36674787126415, 54.92503620785565, 54.2570115563163, 53.72189531517782, 53.62886550667002, 54.18684236072511, 55.46975144780145, 57.40245972952184, 59.76951995751998, 62.245483191331935, 64.44234518457247, 65.96715064081404, 66.48126447948593, 65.75255470532237, 63.69275658121059, 60.37445201391579, 56.0250830945571, 50.99878543301164, 45.73007990326164, 40.67612199395563, 36.25588462862673, 32.795096388686375, 30.484906974854216, 29.360230695124617, 29.300824329609224, 30.0548139884524, 31.281088443246308, 32.60420915089278, 33.673657014537305, 34.218612038932, 34.090135995039894, 33.284500452279346, 31.944197056504052, 30.336471910641603, 28.812554368761038, 27.753612038948837, 27.511436617843522, 28.352658511837276, 30.414784876365438, 33.680629499589486, 37.97500772389471, 42.9843019022414, 48.296143620067475, 53.453502501951526, 58.0153530304391, 61.61512226976077, 64.00844530873155, 65.10332398069366, 64.96836712844562, 63.81800726867692, 61.97696560656416, 59.82927800286249, 57.75945132359774, 56.09446066340426, 55.05515558672392, 54.724239112126256, 55.035523837206554, 55.786017847017945, 56.66901329228948, 57.32324074608649, 57.39077086137337, 56.575042430158675, 54.69036474054707, 51.69548699191284, 47.706165389788815, 42.984745357914804, 37.90815771036357, 32.918905829341575, 28.466127963399185, 24.945285521164053, 22.645238622740862, 21.710393044316305, 22.123396423425078, 23.71085075449591, 26.171135786313325, 29.120200126771373, 32.14855001193645, 34.88103618095285, 37.03064664089486, 38.43841472414578, 39.09361460012102, 39.131336224537904, 38.80787834829107, 38.45767860611318, 38.43823059092147, 39.07121686643464, 40.58865193676731, 43.09209692047743, 46.53108577183554, 50.704074263396556, 55.281904054465315, 59.85045372042113, 63.96631559326491, 67.21740893788812, 69.27970048479777, 69.96176179528702, 69.23066742623458, 67.21546746124083, 64.1877511603664, 60.52217224445828, 56.64273728441093, 52.96273567743, 49.82710735048268, 47.46566790493431, 45.963996184306936, 45.25617427710042, 45.1403400585319, 45.314643326583884, 45.42818463719831, 45.139304703866266, 44.172507369246155, 42.36550045878164, 39.699296648194476, 36.30681612347198, 32.45860614536118, 28.527672272327205, 24.93850414426685]
        property var memoryHistory: [71.2, 71.33899726894451, 71.45593877584784, 71.551045498588, 71.62492476605652, 71.67855333180776, 71.7132537557864, 71.73066453930053, 71.732704572163, 71.721532554451, 71.6995021467486, 71.66911368036874, 71.6329633214897, 71.59369062924398, 71.55392547672943, 71.51623531514258, 71.48307375456243, 71.45673141045845, 71.4392899232103, 71.4325799995671, 71.43814425111168, 71.4572055167733, 71.4906412558607, 71.53896448679946, 71.6023116267846, 71.68043746109105, 71.77271734012692, 71.87815656985313, 71.99540682934462, 72.12278932043658, 72.25832423093979, 72.39976597706787, 72.54464358461823, 72.6903054740167, 72.83396783330282, 72.97276569697337, 73.1038057985276, 73.22422023148178, 73.33121993814767, 73.42214704788556, 73.49452510680234, 73.54610627860396, 73.5749146508409, 73.57928485111398, 73.55789526265436, 73.50979522651127, 73.43442572659123, 73.33163317200754, 73.20167601646493, 73.04522408443775, 72.86335060633387, 72.65751709725302, 72.42955134393976, 72.18161888972357, 71.91618852535389, 71.63599240252157, 71.34398148452672, 71.0432771332265, 70.7371197015307, 70.42881505503306, 70.12167998387694, 69.81898748597949, 69.52391290490564, 69.23948188996117, 68.96852111274283, 68.71361262405661, 68.47705266870653, 68.2608156943792, 68.06652419618426, 67.89542493208545, 67.74837192840772, 67.62581657094971, 67.52780494822305, 67.4539824813294, 67.40360574238277, 67.37556123260305, 67.36839076463221, 67.38032297356386, 67.40931036981786, 67.45307124636466, 67.50913566474206, 67.57489467041508, 67.64765182965338, 67.72467613830413, 67.80325532838114, 67.88074859171827, 67.95463775116811, 68.02257593875667, 68.08243288630541, 68.13233599645864, 68.17070643966848, 68.19628961407477, 68.208179408703, 68.20583582410528, 68.18909562640712, 68.15817583847833, 68.11367000328377, 68.05653728699619, 67.988084620748, 67.90994220756663, 67.82403284275068, 67.73253560947728, 67.63784461471654, 67.5425235216784, 67.44925671236959, 67.3607979759788, 67.27991766460461, 67.20934928645386, 67.15173651754695, 67.10958160596796, 67.08519611792026, 67.08065493274063, 67.09775433535333, 67.13797498048014, 67.20245041461173, 67.29194174090028, 67.40681890059037, 67.54704892439702, 67.71219138055744, 67.90140111543577]
        property var networkHistory: [2400000.0, 4800000.0, 7200000.0, 9600000.0, 120000.0, 150000.0, 180000.0, 0.0, 30000.0, 60000.0, 90000.0, 120000.0, 150000.0, 180000.0, 0.0, 30000.0, 60000.0, 7200000.0, 9600000.0, 12000000.0, 2400000.0, 0.0, 30000.0, 60000.0, 90000.0, 120000.0, 150000.0, 180000.0, 0.0, 30000.0, 60000.0, 90000.0, 120000.0, 150000.0, 12000000.0, 2400000.0, 4800000.0, 7200000.0, 90000.0, 120000.0, 150000.0, 180000.0, 0.0, 30000.0, 60000.0, 90000.0, 120000.0, 150000.0, 180000.0, 0.0, 30000.0, 4800000.0, 7200000.0, 9600000.0, 12000000.0, 180000.0, 0.0, 30000.0, 60000.0, 90000.0, 120000.0, 150000.0, 180000.0, 0.0, 30000.0, 60000.0, 90000.0, 120000.0, 9600000.0, 12000000.0, 2400000.0, 4800000.0, 60000.0, 90000.0, 120000.0, 150000.0, 180000.0, 0.0, 30000.0, 60000.0, 90000.0, 120000.0, 150000.0, 180000.0, 0.0, 2400000.0, 4800000.0, 7200000.0, 9600000.0, 150000.0, 180000.0, 0.0, 30000.0, 60000.0, 90000.0, 120000.0, 150000.0, 180000.0, 0.0, 30000.0, 60000.0, 90000.0, 7200000.0, 9600000.0, 12000000.0, 2400000.0, 30000.0, 60000.0, 90000.0, 120000.0, 150000.0, 180000.0, 0.0, 30000.0, 60000.0, 90000.0, 120000.0, 150000.0, 180000.0, 12000000.0]
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
        property string defaultInterface: "wlan0"
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
