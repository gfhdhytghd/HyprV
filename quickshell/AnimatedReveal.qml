import QtQuick

Item {
    id: root

    default property alias contentData: contentItem.data
    property alias contentItem: contentItem

    readonly property bool openAnimationRunning: openAnimation.running
    readonly property bool closeAnimationRunning: closeAnimation.running

    property real fullHeight: 0
    property real lineHeight: 2
    property bool layoutHeightFollowsReveal: false

    property real openContentOffset: -8
    property real closeContentOffset: -8

    property int openRevealPause: 0
    property int openRevealDuration: 200
    property int openContentDelay: 20
    property int openFadeDuration: 140
    property int openSlideDuration: 180

    property int closeRevealPause: 0
    property int closeRevealDuration: 180
    property int closeFadeDuration: 90
    property int closeSlideDuration: 150

    property real revealHeight: 0
    property real contentOpacity: 0
    property real contentOffset: openContentOffset

    signal openAnimationFinished()
    signal closeAnimationFinished()

    implicitHeight: layoutHeightFollowsReveal ? revealHeight : fullHeight
    height: implicitHeight

    function resetAnimationState() {
        revealHeight = fullHeight;
        contentOpacity = 1;
        contentOffset = 0;
    }

    function collapseImmediately() {
        stopAnimations();
        revealHeight = 0;
        contentOpacity = 0;
        contentOffset = openContentOffset;
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

    Item {
        id: viewport

        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: root.revealHeight
        clip: true

        Item {
            id: contentItem

            x: 0
            y: root.contentOffset
            width: viewport.width
            height: root.fullHeight
            opacity: root.contentOpacity
        }
    }

    SequentialAnimation {
        id: openAnimation

        onFinished: {
            root.revealHeight = root.fullHeight;
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
                to: root.fullHeight
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
}
