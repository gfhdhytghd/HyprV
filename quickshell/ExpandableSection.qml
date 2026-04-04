import QtQuick

AnimatedReveal {
    id: root

    property bool expanded: false
    property bool openAnimationPending: false

    fullHeight: contentItem.childrenRect.height

    layoutHeightFollowsReveal: true
    lineHeight: 0

    Component.onCompleted: {
        if (expanded) {
            resetAnimationState();
        } else {
            collapseImmediately();
        }
    }

    onExpandedChanged: {
        openTimer.stop();
        if (expanded) {
            if (fullHeight <= 0) {
                openAnimationPending = true;
                openTimer.restart();
                return;
            }
            openAnimationPending = false;
            prepareOpenAnimation();
            playOpenAnimation();
            return;
        }
        openAnimationPending = false;
        playCloseAnimation();
    }

    onFullHeightChanged: {
        if (openAnimationPending) {
            openTimer.restart();
            return;
        }
        if (expanded && !openAnimationRunning && !closeAnimationRunning) {
            resetAnimationState();
        }
    }

    Timer {
        id: openTimer

        interval: 16
        repeat: false
        onTriggered: {
            if (!root.expanded) {
                root.openAnimationPending = false;
                return;
            }
            if (root.fullHeight <= 0) {
                openTimer.restart();
                return;
            }
            root.openAnimationPending = false;
            root.prepareOpenAnimation();
            root.playOpenAnimation();
        }
    }
}
