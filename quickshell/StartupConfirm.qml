import QtQuick
import QtQuick.Controls.Basic
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

ShellRoot {
    id: root

    // Only an explicit click/key activation writes approval. Closing, crashes,
    // missing Quickshell, and write failures all leave the launcher cancelled.
    property bool decided: false
    property bool keyboardReady: false
    readonly property bool dark: systemPalette.window.hslLightness < 0.5
    readonly property color foreground: dark ? "#cdd6f4" : "#2b2b2c"
    readonly property color accent: dark ? "#89b4fa" : "#1e66f5"

    function finish(launch: bool) {
        if (decided) return;
        decided = true;
        if (launch && approval.path.length > 0) {
            approval.setText("launch\n");
        } else {
            Qt.quit();
        }
    }

    SystemPalette { id: systemPalette }
    FileView {
        id: approval
        path: Quickshell.env("HYPRV_STARTUP_APPROVAL") || ""
        blockWrites: true
        onSaved: Qt.quit()
        onSaveFailed: Qt.quit()
    }
    Timer {
        interval: 150
        running: true
        onTriggered: {
            skip.forceActiveFocus(Qt.TabFocusReason);
            root.keyboardReady = true;
        }
    }

    PanelWindow {
        id: window
        implicitWidth: 360
        implicitHeight: card.implicitHeight
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.namespace: "hyprv-startup-confirm"
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive

        Shortcut {
            sequences: ["Escape", "Alt+F4"]
            enabled: root.keyboardReady
            autoRepeat: false
            onActivated: root.finish(false)
        }

        Rectangle {
            id: card
            anchors.fill: parent
            implicitHeight: content.implicitHeight + 48
            radius: 12
            color: root.dark ? "#1e1e2e" : "#fafafb"
            border.width: 1
            border.color: root.dark ? "#20ffffff" : "#14000000"

            Column {
                id: content
                anchors { left: parent.left; right: parent.right; top: parent.top; margins: 24 }

                Text {
                    width: parent.width
                    text: "启动用户应用"
                    color: root.foreground
                    font { family: "HarmonyOS Sans SC"; pixelSize: 16; weight: Font.Medium }
                    lineHeightMode: Text.FixedHeight
                    lineHeight: 24
                }
                Item { width: 1; height: 6 }
                Text {
                    width: parent.width
                    text: "启动常用应用并恢复工作区布局。"
                    color: root.dark ? "#a6adc8" : "#5e5e63"
                    font { family: "HarmonyOS Sans SC"; pixelSize: 14 }
                    lineHeightMode: Text.FixedHeight
                    lineHeight: 22
                    wrapMode: Text.Wrap
                }
                Item { width: 1; height: 24 }
                Row {
                    anchors.right: parent.right
                    spacing: 8

                    ActionButton {
                        id: skip
                        text: "本次跳过"
                        KeyNavigation.right: launch
                        onClicked: root.finish(false)
                        Component.onCompleted: forceActiveFocus(Qt.TabFocusReason)
                    }
                    ActionButton {
                        id: launch
                        text: "启动"
                        KeyNavigation.left: skip
                        onClicked: root.finish(true)
                    }
                }
            }
        }
    }

    component ActionButton: Button {
        id: control
        implicitWidth: 88
        implicitHeight: 32
        activeFocusOnTab: true
        hoverEnabled: true
        Accessible.name: text
        Keys.onPressed: event => {
            if (!root.keyboardReady || event.isAutoRepeat) {
                event.accepted = true;
            } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                control.clicked();
                event.accepted = true;
            }
        }
        Keys.onReleased: event => {
            if (!root.keyboardReady || event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space)
                event.accepted = true;
        }
        contentItem: Text {
            id: label
            text: control.text
            color: root.foreground
            font { family: "HarmonyOS Sans SC"; pixelSize: 13; weight: Font.Medium }
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
        background: Rectangle {
            radius: 8
            color: root.dark
                ? (control.down ? "#585b70" : control.hovered ? "#45475a" : "#313244")
                : (control.down ? "#d9d9dd" : control.hovered ? "#e3e3e6" : "#ededed")
            Rectangle {
                anchors.fill: parent
                anchors.margins: -3
                radius: 11
                color: "transparent"
                border { width: 2; color: root.accent }
                visible: control.visualFocus
            }
        }
    }
}
