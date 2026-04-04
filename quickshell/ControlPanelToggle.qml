import QtQuick
import QtQuick.Effects

Rectangle {
    id: toggle

    property var shellRoot: null
    property string icon: ""
    property url iconSource: ""
    property string label: ""
    property bool iconOnly: false
    property bool active: false
    property bool destructive: false
    property bool toggleEnabled: true

    signal clicked()

    readonly property color activeColor: destructive
        ? (shellRoot ? shellRoot.criticalColor : "#d9485f")
        : (shellRoot ? shellRoot.launchColor : "#89b4fa")
    readonly property color titleColor: shellRoot ? shellRoot.primaryText : "#cdd6f4"
    readonly property color activeFill: shellRoot ? shellRoot.withAlpha(activeColor, shellRoot.darkMode ? 0.28 : 0.22) : Qt.rgba(0.54, 0.71, 0.98, 0.22)
    readonly property color inactiveFill: shellRoot ? shellRoot.withAlpha(shellRoot.darkMode ? "#ffffff" : "#ffffff", shellRoot.darkMode ? 0.07 : 0.22) : "#2a2a2a"
    readonly property color activeStroke: shellRoot ? shellRoot.withAlpha(activeColor, shellRoot.darkMode ? 0.42 : 0.28) : Qt.rgba(0.54, 0.71, 0.98, 0.32)
    readonly property color inactiveStroke: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.12 : 0.08) : "#454545"
    readonly property color iconWellFill: shellRoot
        ? shellRoot.withAlpha(shellRoot.darkMode ? "#ffffff" : "#ffffff", shellRoot.darkMode ? (active ? 0.14 : 0.11) : (active ? 0.26 : 0.32))
        : "#303030"
    readonly property color iconColor: toggle.active ? "#ffffff" : toggle.titleColor
    readonly property real frameRadius: 19
    readonly property real iconBoxSize: iconOnly ? Math.max(22, Math.min(30, Math.min(toggle.width, toggle.height) - 12)) : Math.max(28, Math.min(34, toggle.height - 16))
    readonly property real iconRadius: Math.round(iconBoxSize * 0.34)
    readonly property real iconPixelSize: iconOnly ? Math.max(15, Math.min(18, iconBoxSize * 0.5)) : Math.max(15, Math.min(18, iconBoxSize * 0.46))
    readonly property real iconImageSize: iconOnly ? Math.max(18, Math.min(22, iconBoxSize * 0.74)) : Math.max(18, Math.min(20, iconBoxSize * 0.64))

    radius: frameRadius
    height: 72
    color: {
        const base = active ? activeFill : inactiveFill;
        if (!toggleEnabled) {
            return Qt.rgba(base.r, base.g, base.b, 0.48);
        }
        if (area.pressed) {
            return Qt.darker(base, 1.06);
        }
        if (area.containsMouse) {
            return Qt.lighter(base, 1.04);
        }
        return base;
    }
    border.width: 1
    border.color: {
        if (!toggleEnabled) {
            return inactiveStroke;
        }
        if (area.containsMouse) {
            return active ? Qt.lighter(activeStroke, 1.08) : Qt.lighter(inactiveStroke, 1.04);
        }
        return active ? activeStroke : inactiveStroke;
    }
    opacity: toggleEnabled ? 1 : 0.56
    antialiasing: true

    Column {
        anchors.centerIn: parent
        spacing: iconOnly ? 0 : 6

        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            width: toggle.iconBoxSize
            height: toggle.iconBoxSize
            radius: toggle.iconRadius
            color: toggle.iconOnly
                ? "transparent"
                : (area.pressed
                    ? Qt.darker(toggle.iconWellFill, 1.05)
                    : (area.containsMouse ? Qt.lighter(toggle.iconWellFill, 1.03) : toggle.iconWellFill))
            antialiasing: true

            Image {
                id: iconImage

                anchors.centerIn: parent
                visible: toggle.iconSource.toString().length > 0
                width: toggle.iconImageSize
                height: toggle.iconImageSize
                source: toggle.iconSource
                fillMode: Image.PreserveAspectFit
                smooth: true
                mipmap: true
                sourceSize.width: Math.max(32, Math.round(width * 2))
                sourceSize.height: Math.max(32, Math.round(height * 2))
                layer.enabled: true
                layer.effect: MultiEffect {
                    colorization: 1
                    colorizationColor: toggle.iconColor
                }
            }

            Text {
                anchors.centerIn: parent
                visible: !iconImage.visible
                text: toggle.icon
                color: toggle.iconColor
                font.family: toggle.shellRoot ? toggle.shellRoot.iconFont : "JetBrainsMono Nerd Font"
                font.pixelSize: toggle.iconPixelSize
                font.weight: Font.Bold
                renderType: Text.NativeRendering
            }
        }

        Text {
            visible: !toggle.iconOnly && toggle.label.length > 0
            width: Math.max(40, toggle.width - 14)
            text: toggle.label
            color: toggle.active ? "#ffffff" : toggle.titleColor
            font.family: toggle.shellRoot ? toggle.shellRoot.baseFont : "JetBrainsMono Nerd Font"
            font.pixelSize: 10
            font.weight: Font.Bold
            renderType: Text.NativeRendering
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
        }
    }

    MouseArea {
        id: area

        anchors.fill: parent
        enabled: toggle.toggleEnabled
        hoverEnabled: true
        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: toggle.clicked()
    }
}
