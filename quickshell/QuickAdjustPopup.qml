import QtQuick
import Quickshell
import Quickshell.Wayland

Item {
    id: popupRoot

    property var shellRoot: null
    property string mode: "brightness"
    property bool closing: false

    readonly property int popupScreenMargin: 10
    readonly property int moduleSize: 62
    readonly property int cardSpacing: 10
    readonly property int popupWidth: moduleSize * 4 + cardSpacing * 3
    readonly property int popupHeight: moduleSize
    readonly property real hiddenScale: 0.96

    property real cardOpacity: 0
    property real cardScale: hiddenScale

    function normalizedMode(kind) {
        return kind === "volume" ? "volume" : "brightness";
    }

    function show(kind) {
        mode = normalizedMode(kind);
        closing = false;
        closeFinalizeTimer.stop();
        hideTimer.restart();

        if (!popupWindow.visible) {
            cardOpacity = 0;
            cardScale = hiddenScale;
            popupWindow.visible = true;
            popupWindow.updatePopupPosition();
        }

        cardOpacity = 1;
        cardScale = 1;
    }

    function restartAutoHide() {
        hideTimer.restart();
    }

    function hide() {
        if (!popupWindow.visible) {
            return;
        }
        closing = true;
        hideTimer.stop();
        cardOpacity = 0;
        cardScale = hiddenScale;
        closeFinalizeTimer.restart();
    }

    function applyValue(newValue) {
        if (!shellRoot) {
            return;
        }

        const nextValue = Math.max(0, Math.min(100, Math.round(newValue)));
        restartAutoHide();

        if (mode === "brightness") {
            shellRoot.applyBrightnessPercent(nextValue);
            return;
        }

        shellRoot.audioAvailable = true;
        shellRoot.audioVolumePercent = nextValue;
        if (shellRoot.audioMuted && nextValue > 0) {
            shellRoot.audioMuted = false;
        }
        shellRoot.runDetached(["wpctl", "set-volume", "@DEFAULT_AUDIO_SINK@", (nextValue / 100).toFixed(2)]);
        shellRoot.scheduleAudioRefresh();
    }

    function toggleAudioMute() {
        if (!shellRoot || mode !== "volume") {
            return;
        }
        restartAutoHide();
        shellRoot.audioAvailable = true;
        shellRoot.audioMuted = !shellRoot.audioMuted;
        shellRoot.runDetached(["wpctl", "set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"]);
        shellRoot.scheduleAudioRefresh();
    }

    Timer {
        id: hideTimer

        interval: 3000
        repeat: false
        onTriggered: popupRoot.hide()
    }

    Timer {
        id: closeFinalizeTimer

        interval: 140
        repeat: false
        onTriggered: {
            if (popupRoot.closing) {
                popupRoot.closing = false;
                popupWindow.visible = false;
            }
        }
    }

    PanelWindow {
        id: popupWindow

        screen: popupRoot.shellRoot && popupRoot.shellRoot.primaryBarWindow
            ? popupRoot.shellRoot.primaryBarWindow.screen
            : null
        visible: false
        aboveWindows: true
        focusable: false
        exclusiveZone: -1
        color: "transparent"
        surfaceFormat.opaque: false
        mask: Region {
            item: popupBounds
        }

        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        WlrLayershell.namespace: "shell:hyprv-quick-adjust"

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
        onScreenChanged: if (visible) {
            updatePopupPosition();
        }
        onVisibleChanged: if (visible) {
            updatePopupPosition();
        }

        function updatePopupPosition() {
            if (!visible || !screen) {
                return;
            }
            const anchor = popupRoot.shellRoot ? popupRoot.shellRoot.quickAdjustAnchorItem : null;
            if (anchor) {
                const point = anchor.mapToGlobal(anchor.width, anchor.height);
                const anchorRight = Math.round(point.x - screen.x);
                const anchorBottom = Math.round(point.y - screen.y);
                popupBounds.x = Math.max(
                    popupRoot.popupScreenMargin,
                    Math.min(width - popupBounds.width - popupRoot.popupScreenMargin, anchorRight - popupBounds.width)
                );
                popupBounds.y = anchorBottom + 10;
                return;
            }

            popupBounds.x = Math.max(popupRoot.popupScreenMargin, width - popupBounds.width - popupRoot.popupScreenMargin);
            const barBottom = popupRoot.shellRoot && popupRoot.shellRoot.primaryBarWindow
                ? Math.max(0, popupRoot.shellRoot.primaryBarWindow.exclusiveZone || 58)
                : 58;
            popupBounds.y = barBottom + 10;
        }

        Item {
            id: popupBounds

            width: popupRoot.popupWidth
            height: popupRoot.popupHeight

            ControlPanelSlider {
                anchors.fill: parent
                shellRoot: popupRoot.shellRoot
                icon: popupRoot.mode === "volume"
                    ? (popupRoot.shellRoot ? popupRoot.shellRoot.volumeIcon : "")
                    : "󰃟"
                iconClickable: popupRoot.mode === "volume"
                label: popupRoot.mode === "volume" ? "Volume" : "Brightness"
                value: popupRoot.mode === "volume"
                    ? (popupRoot.shellRoot ? popupRoot.shellRoot.audioVolumePercent : 0)
                    : (popupRoot.shellRoot ? popupRoot.shellRoot.brightnessPercent : 0)
                accentColor: popupRoot.mode === "volume"
                    ? (popupRoot.shellRoot ? popupRoot.shellRoot.launchColor : "#89b4fa")
                    : (popupRoot.shellRoot ? popupRoot.shellRoot.brightnessColor : "#d47b1f")
                opacity: popupRoot.cardOpacity
                scale: popupRoot.cardScale
                transformOrigin: Item.TopRight

                Behavior on opacity {
                    NumberAnimation {
                        duration: 120
                        easing.type: Easing.OutQuad
                    }
                }

                Behavior on scale {
                    NumberAnimation {
                        duration: 160
                        easing.type: Easing.OutCubic
                    }
                }

                onValueChangeRequested: function(newValue) {
                    popupRoot.applyValue(newValue);
                }
                onIconClicked: popupRoot.toggleAudioMute()
            }
        }
    }
}
