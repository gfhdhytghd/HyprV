import QtQuick
import QtQuick.Effects

Item {
    id: root

    default property alias contentData: panelContent.data
    property alias contentItem: panelContent

    readonly property bool openAnimationRunning: openAnimation.running
    readonly property bool closeAnimationRunning: closeAnimation.running

    property real revealHeight: 0
    property real contentOpacity: 0
    property real contentOffset: openContentOffset

    property real fullPanelHeight: 0
    property real lineHeight: 2
    property real radius: 19
    property real surfaceOpacity: 0.82
    property color fillColor: "#202020"
    property color strokeColor: "#404040"
    property color shadowColor: "transparent"
    property real devicePixelRatio: 1

    property int openRevealPause: 0
    property int openRevealDuration: 200
    property int openContentDelay: 20
    property int openFadeDuration: 140
    property int openSlideDuration: 180
    property real openContentOffset: -8

    property int closeRevealPause: 0
    property int closeRevealDuration: 180
    property int closeFadeDuration: 90
    property int closeSlideDuration: 150
    property real closeContentOffset: -8

    signal openAnimationFinished()
    signal closeAnimationFinished()

    implicitHeight: fullPanelHeight
    height: fullPanelHeight

    function resetAnimationState() {
        revealHeight = fullPanelHeight;
        contentOpacity = 1;
        contentOffset = 0;
    }

    function prepareOpenAnimation() {
        stopAnimations();
        revealHeight = 0;
        contentOpacity = 0;
        contentOffset = openContentOffset;
    }

    function playOpenAnimation() {
        stopAnimations();
        openAnimation.restart();
    }

    function playCloseAnimation() {
        stopAnimations();
        closeAnimation.restart();
    }

    function stopAnimations() {
        openAnimation.stop();
        closeAnimation.stop();
    }

    SequentialAnimation {
        id: openAnimation

        onFinished: {
            root.revealHeight = root.fullPanelHeight;
            root.contentOpacity = 1;
            root.contentOffset = 0;
            root.openAnimationFinished();
        }

        PauseAnimation {
            duration: root.openRevealPause
        }

        ParallelAnimation {
            NumberAnimation {
                target: root
                property: "revealHeight"
                to: root.fullPanelHeight
                duration: root.openRevealDuration
                easing.type: Easing.OutCubic
            }

            SequentialAnimation {
                PauseAnimation {
                    duration: root.openContentDelay
                }

                ParallelAnimation {
                    NumberAnimation {
                        target: root
                        property: "contentOpacity"
                        to: 1
                        duration: root.openFadeDuration
                        easing.type: Easing.OutQuad
                    }

                    NumberAnimation {
                        target: root
                        property: "contentOffset"
                        to: 0
                        duration: root.openSlideDuration
                        easing.type: Easing.OutCubic
                    }
                }
            }
        }
    }

    ParallelAnimation {
        id: closeAnimation

        NumberAnimation {
            target: root
            property: "contentOpacity"
            to: 0
            duration: root.closeFadeDuration
            easing.type: Easing.InQuad
        }

        NumberAnimation {
            target: root
            property: "contentOffset"
            to: root.closeContentOffset
            duration: root.closeSlideDuration
            easing.type: Easing.InCubic
        }

        SequentialAnimation {
            PauseAnimation {
                duration: root.closeRevealPause
            }

            NumberAnimation {
                target: root
                property: "revealHeight"
                to: root.lineHeight
                duration: root.closeRevealDuration
                easing.type: Easing.InCubic
            }
        }

        onFinished: root.closeAnimationFinished()
    }

    Item {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: root.revealHeight
        clip: true

        Rectangle {
            id: panelFrame

            x: 0
            y: 0
            width: root.width
            height: root.fullPanelHeight
            radius: root.radius
            color: "transparent"
            border.width: 0
            antialiasing: true
            clip: true

            Item {
                id: shadowLayer

                anchors.fill: parent
                layer.enabled: true
                layer.smooth: true
                layer.textureSize: Qt.size(Math.max(1, Math.round(width * root.devicePixelRatio)), Math.max(1, Math.round(height * root.devicePixelRatio)))
                layer.textureMirroring: ShaderEffectSource.MirrorVertically

                readonly property int blurMax: 64

                layer.effect: MultiEffect {
                    autoPaddingEnabled: true
                    shadowEnabled: true
                    blurEnabled: false
                    maskEnabled: false
                    shadowBlur: 10 / shadowLayer.blurMax
                    shadowScale: 1
                    shadowColor: root.shadowColor
                }

                Rectangle {
                    anchors.fill: parent
                    radius: root.radius
                    opacity: root.surfaceOpacity
                    color: root.fillColor
                    border.width: 1
                    border.color: root.strokeColor
                    antialiasing: true
                }
            }

            Item {
                id: panelContent

                anchors.fill: parent
                opacity: root.contentOpacity
                y: root.contentOffset
            }
        }
    }
}
