import QtQuick

Item {
    id: slider

    property var shellRoot: null
    property string icon: ""
    property string label: ""
    property real value: 50
    property color accentColor: shellRoot ? shellRoot.launchColor : "#89b4fa"
    property bool iconClickable: false

    signal valueChangeRequested(real newValue)
    signal iconClicked()

    height: 62
    implicitHeight: 62

    readonly property color frameFill: shellRoot ? shellRoot.withAlpha(shellRoot.darkMode ? "#ffffff" : "#ffffff", shellRoot.darkMode ? 0.07 : 0.22) : "#2a2a2a"
    readonly property color frameStroke: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.12 : 0.08) : "#454545"
    readonly property color textColor: shellRoot ? shellRoot.primaryText : "#cdd6f4"
    readonly property color trackBg: shellRoot ? shellRoot.withAlpha(shellRoot.darkMode ? "#ffffff" : "#ffffff", shellRoot.darkMode ? 0.08 : 0.34) : "#2a2a2a"
    readonly property real clampedValue: Math.max(0, Math.min(100, value))
    readonly property string valueText: Math.round(clampedValue) + "%"
    readonly property real frameRadius: 19
    readonly property real outerPadding: 9
    readonly property real contentGap: 9
    readonly property real alignedIconSize: 44
    readonly property real alignedIconRadius: 11
    property real contentOffsetY: -8
    readonly property real iconBoxSize: Math.max(28, Math.min(alignedIconSize, height - outerPadding * 2))
    readonly property real iconRadius: Math.min(alignedIconRadius, iconBoxSize / 2)
    readonly property real iconPixelSize: Math.max(14, Math.min(18, iconBoxSize * 0.44))
    readonly property real titlePixelSize: 11
    readonly property real trackHeight: 6
    readonly property real thumbSize: 14

    Rectangle {
        anchors.fill: parent
        radius: slider.frameRadius
        color: slider.frameFill
        border.width: 1
        border.color: slider.frameStroke
        antialiasing: true

        Row {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.topMargin: slider.outerPadding
            anchors.leftMargin: slider.outerPadding
            anchors.rightMargin: slider.outerPadding
            spacing: slider.contentGap

            Rectangle {
                width: slider.iconBoxSize
                height: slider.iconBoxSize
                radius: slider.iconRadius
                color: slider.shellRoot
                    ? slider.shellRoot.withAlpha(slider.accentColor, slider.shellRoot.darkMode ? 0.18 : 0.12)
                    : Qt.rgba(slider.accentColor.r, slider.accentColor.g, slider.accentColor.b, 0.14)
                antialiasing: true

                Text {
                    anchors.centerIn: parent
                    text: slider.icon
                    color: slider.accentColor
                    font.family: slider.shellRoot ? slider.shellRoot.iconFont : "JetBrainsMono Nerd Font"
                    font.pixelSize: slider.iconPixelSize
                    font.weight: Font.Bold
                    renderType: Text.NativeRendering
                }

                MouseArea {
                    anchors.fill: parent
                    enabled: slider.iconClickable
                    hoverEnabled: slider.iconClickable
                    cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                    onClicked: slider.iconClicked()
                }
            }

            Item {
                width: parent.width - slider.iconBoxSize - parent.spacing
                anchors.verticalCenter: parent.verticalCenter
                anchors.verticalCenterOffset: slider.contentOffsetY
                height: Math.max(labelText.implicitHeight, valueLabel.implicitHeight)

                Text {
                    id: labelText

                    anchors.left: parent.left
                    anchors.right: valueLabel.left
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width
                    text: slider.label
                    color: slider.textColor
                    font.family: slider.shellRoot ? slider.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                    font.pixelSize: slider.titlePixelSize
                    font.weight: Font.Bold
                    renderType: Text.NativeRendering
                    elide: Text.ElideRight
                }

                Text {
                    id: valueLabel

                    anchors.right: parent.right
                    anchors.rightMargin: 2
                    anchors.verticalCenter: parent.verticalCenter
                    text: slider.valueText
                    color: slider.textColor
                    font.family: slider.shellRoot ? slider.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                    font.pixelSize: slider.titlePixelSize
                    font.weight: Font.Bold
                    renderType: Text.NativeRendering
                }
            }
        }

        Rectangle {
            id: track

            anchors.left: parent.left
            anchors.right: parent.right
            anchors.leftMargin: slider.outerPadding + slider.iconBoxSize + slider.contentGap
            anchors.rightMargin: slider.outerPadding
            anchors.bottom: parent.bottom
            anchors.bottomMargin: slider.outerPadding - slider.contentOffsetY
            height: slider.trackHeight
            radius: slider.trackHeight / 2
            color: slider.trackBg
            antialiasing: true

            Rectangle {
                id: fill

                width: Math.max(0, Math.min(parent.width, parent.width * slider.clampedValue / 100))
                height: parent.height
                radius: parent.radius
                color: slider.accentColor
                antialiasing: true

                Behavior on width {
                    enabled: !trackArea.pressed
                    NumberAnimation {
                        duration: 90
                        easing.type: Easing.OutCubic
                    }
                }
            }

            Rectangle {
                id: thumb

                x: Math.max(0, Math.min(track.width - width, track.width * slider.clampedValue / 100 - width / 2))
                anchors.verticalCenter: parent.verticalCenter
                width: slider.thumbSize
                height: slider.thumbSize
                radius: slider.thumbSize / 2
                color: shellRoot ? (shellRoot.darkMode ? "#f7f7f8" : "#ffffff") : "#ffffff"
                border.width: 2
                border.color: slider.accentColor
                antialiasing: true

                Behavior on x {
                    enabled: !trackArea.pressed
                    NumberAnimation {
                        duration: 90
                        easing.type: Easing.OutCubic
                    }
                }

                scale: trackArea.pressed ? 1.15 : (trackArea.containsMouse ? 1.05 : 1)
                Behavior on scale {
                    NumberAnimation {
                        duration: 100
                        easing.type: Easing.OutQuad
                    }
                }
            }

            MouseArea {
                id: trackArea

                anchors.fill: parent
                anchors.margins: -8
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor

                function computeValue(mouseX) {
                    const localX = mouseX - 8;
                    const ratio = Math.max(0, Math.min(1, localX / track.width));
                    return Math.round(ratio * 100);
                }

                onPressed: function(mouse) {
                    slider.valueChangeRequested(computeValue(mouse.x));
                }
                onPositionChanged: function(mouse) {
                    if (pressed) {
                        slider.valueChangeRequested(computeValue(mouse.x));
                    }
                }
            }
        }
    }
}
