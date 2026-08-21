import QtQuick

Item {
    id: slider

    property var shellRoot: null
    property string icon: ""
    property string label: ""
    property real value: 50
    property color accentColor: shellRoot ? shellRoot.launchColor : "#89b4fa"
    property bool iconClickable: false
    property bool adjusting: false
    property real previewValue: 0

    signal valueChangeRequested(real newValue)
    signal iconClicked()

    height: 62
    implicitHeight: 62

    readonly property real displayValue: adjusting ? previewValue : value
    readonly property real clampedValue: Math.max(0, Math.min(100, displayValue))
    readonly property real frameRadius: 19
    readonly property real fillWidth: width * clampedValue / 100
    readonly property real iconCenterX: Math.min(width - height / 2, Math.max(height / 2, width * 0.24))
    readonly property real iconPixelSize: Math.max(18, Math.min(22, height * 0.34))
    readonly property color restingFill: shellRoot
        ? shellRoot.withAlpha(shellRoot.darkMode ? "#ffffff" : "#ffffff", shellRoot.darkMode ? 0.07 : 0.22)
        : "#2a2a2a"
    readonly property color progressFill: shellRoot
        ? shellRoot.withAlpha(shellRoot.launchColor, shellRoot.darkMode ? 0.28 : 0.22)
        : Qt.rgba(0.54, 0.71, 0.98, 0.22)
    readonly property color iconSolidColor: progressFill
    readonly property color frameStroke: shellRoot
        ? shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.12 : 0.08)
        : "#454545"

    Rectangle {
        id: frame

        anchors.fill: parent
        radius: slider.frameRadius
        color: slider.restingFill
        antialiasing: true

        Item {
            id: progressClip

            anchors.fill: parent
            clip: true

            Item {
                width: slider.fillWidth
                height: parent.height
                clip: true

                Rectangle {
                    width: slider.width
                    height: parent.height
                    radius: slider.frameRadius
                    color: slider.progressFill
                    antialiasing: true
                }
            }
        }

        Item {
            width: slider.fillWidth
            height: parent.height
            clip: true

            Text {
                x: slider.iconCenterX - width / 2
                anchors.verticalCenter: parent.verticalCenter
                text: slider.icon
                color: "#ffffff"
                font.family: slider.shellRoot ? slider.shellRoot.iconFont : "JetBrainsMono Nerd Font"
                font.pixelSize: slider.iconPixelSize
                font.weight: Font.Bold
                renderType: Text.NativeRendering
            }
        }

        Item {
            x: slider.fillWidth
            width: Math.max(0, parent.width - x)
            height: parent.height
            clip: true

            Text {
                x: slider.iconCenterX - parent.x - width / 2
                anchors.verticalCenter: parent.verticalCenter
                text: slider.icon
                color: slider.iconSolidColor
                font.family: slider.shellRoot ? slider.shellRoot.iconFont : "JetBrainsMono Nerd Font"
                font.pixelSize: slider.iconPixelSize
                font.weight: Font.Bold
                renderType: Text.NativeRendering
            }
        }

        Rectangle {
            anchors.fill: parent
            radius: slider.frameRadius
            color: "transparent"
            border.width: 1
            border.color: slider.frameStroke
            antialiasing: true
        }

        MouseArea {
            id: trackArea

            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor

            function computeValue(mouseX) {
                return Math.round(Math.max(0, Math.min(1, mouseX / width)) * 100);
            }

            function insideIcon(mouseX, mouseY) {
                const hitRadius = Math.max(18, slider.height * 0.3);
                return Math.abs(mouseX - slider.iconCenterX) <= hitRadius
                    && Math.abs(mouseY - slider.height / 2) <= hitRadius;
            }

            onPressed: function(mouse) {
                if (slider.iconClickable && insideIcon(mouse.x, mouse.y)) {
                    slider.iconClicked();
                    return;
                }
                slider.adjusting = true;
                slider.previewValue = computeValue(mouse.x);
                slider.valueChangeRequested(slider.previewValue);
            }

            onPositionChanged: function(mouse) {
                if (pressed && slider.adjusting) {
                    slider.previewValue = computeValue(mouse.x);
                    slider.valueChangeRequested(slider.previewValue);
                }
            }

            onReleased: function(mouse) {
                if (!slider.adjusting) {
                    return;
                }
                slider.previewValue = computeValue(mouse.x);
                slider.valueChangeRequested(slider.previewValue);
                slider.adjusting = false;
            }

            onCanceled: slider.adjusting = false
        }
    }
}
