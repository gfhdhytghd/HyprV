import QtQuick
import QtQuick.Effects

Item {
    id: island

    property var shellRoot: null
    property date now: new Date()
    property bool mediaAvailable: false
    property bool mediaPlaying: false
    property string mediaTitle: ""
    property string mediaArtist: ""
    property string mediaPlayerName: ""
    property string mediaArtUrl: ""
    property real mediaPositionSeconds: 0
    property real mediaLengthSeconds: 0
    property var spectrumValues: []
    property bool mediaManuallyHidden: false
    property var agentSessions: []
    property var agentPending: null
    property int agentPendingCount: 0

    signal lockClicked()
    signal powerClicked()
    signal previousClicked()
    signal playPauseClicked()
    signal nextClicked()
    signal appFocusRequested()
    signal agentFocusRequested(string sessionId)
    signal agentApproveRequested(string requestId)
    signal agentDenyRequested(string requestId)
    signal agentReplyRequested(string requestId)
    signal agentAnswerRequested(string requestId, string answer)
    signal seekRequested(real positionSeconds)

    property bool seeking: false
    property real seekPreviewSeconds: 0
    property string hoverTarget: ""
    property bool keepPrimaryCollapsedForMusic: false

    readonly property bool hasMusicContent: mediaAvailable && (
        mediaPlaying
        || mediaTitle.length > 0
        || mediaArtist.length > 0
        || mediaArtUrl.length > 0
    )
    readonly property bool agentActive: Array.isArray(agentSessions) && agentSessions.length > 0
    readonly property var primaryAgent: agentActive ? agentSessions[0] : null
    readonly property bool musicActive: hasMusicContent && !mediaManuallyHidden
    readonly property bool musicPrimaryActive: musicActive && !agentActive
    readonly property bool dualIslandActive: agentActive && musicActive
    readonly property bool agentExpanded: agentActive && hoverTarget === "agent" && islandHover.hovered
    readonly property bool musicExpanded: musicActive && hoverTarget === "music" && islandHover.hovered
    readonly property bool expanded: agentExpanded || musicExpanded
    readonly property bool primaryFrameCollapsedForMusic: dualIslandActive && keepPrimaryCollapsedForMusic
    readonly property real swipeThreshold: 34
    readonly property real agentMinimalLeftPadding: 3
    readonly property real agentMinimalClockWidth: 54
    readonly property real agentMinimalGap: 0
    readonly property real agentMinimalStatusSlotWidth: 31
    readonly property real agentMinimalRingSize: 24
    readonly property real agentMinimalRightPadding: 6
    readonly property real agentMinimalWidth: agentMinimalLeftPadding + agentMinimalClockWidth + agentMinimalGap + agentMinimalStatusSlotWidth + agentMinimalRightPadding
    readonly property real agentMinimalClockCenterX: agentMinimalLeftPadding + agentMinimalStatusSlotWidth + agentMinimalGap + agentMinimalClockWidth / 2
    readonly property real agentCompactClockWidth: 58
    readonly property real agentCompactLogoSize: 28
    readonly property real agentCompactLogoGap: 3
    readonly property real agentCompactRingSize: 24
    readonly property real agentCompactRingGap: 3
    readonly property real agentCompactSidePadding: 8
    readonly property real agentCompactLeftExtent: agentCompactSidePadding + agentCompactLogoSize + agentCompactLogoGap
    readonly property real agentCompactRightExtent: agentCompactRingGap + agentCompactRingSize + agentCompactSidePadding
    readonly property real agentCompactPillWidth: agentCompactLeftExtent + agentCompactClockWidth + agentCompactRightExtent
    readonly property real agentCompactClockCenterX: agentCompactLeftExtent + agentCompactClockWidth / 2
    readonly property real agentAttachedWidth: dualIslandActive && !expanded ? agentMinimalWidth : agentCompactPillWidth
    readonly property real musicDetachedWidth: 38
    readonly property real agentDetachedGap: 4
    readonly property real dualCollapsedWidth: agentMinimalWidth + agentDetachedGap + musicDetachedWidth
    readonly property real dualMusicLeftX: agentMinimalWidth + agentDetachedGap
    readonly property real dualMusicCenterX: agentMinimalWidth + agentDetachedGap + musicDetachedWidth / 2
    readonly property real agentCompactWidth: dualIslandActive ? dualCollapsedWidth : agentCompactPillWidth
    readonly property real primaryCollapsedWidth: dualIslandActive ? agentMinimalWidth : agentCompactPillWidth
    readonly property real mainIslandWidth: agentActive
        ? (agentExpanded ? agentExpandedWidth : primaryCollapsedWidth)
        : targetWidth
    readonly property real compactWidth: compactMusicRow.anchors.leftMargin
        + compactMusicRow.anchors.rightMargin
        + compactAlbumArt.width
        + compactTimeLabel.width
        + compactSpectrum.width
        + compactMusicRow.spacing * 2
    readonly property real expandedWidth: 392
    readonly property real agentExpandedWidth: 448
    readonly property real collapsedHeight: 38
    readonly property real expandedHeight: 176
    readonly property real agentExpandedBudgetHeight: 490
    readonly property real agentSessionRowHeight: 42
    readonly property real agentSessionSpacing: 8
    readonly property int visibleAgentSessionCount: agentActive ? agentSessions.length : 0
    readonly property real agentExpandedTopReserve: 26 + 58 + (agentPending ? 11 + 74 + 13 : 20)
    readonly property real agentSessionContentHeight: visibleAgentSessionCount * agentSessionRowHeight + Math.max(0, visibleAgentSessionCount - 1) * agentSessionSpacing
    readonly property real agentSessionListHeight: Math.max(
        agentSessionRowHeight,
        Math.min(agentSessionContentHeight, agentExpandedBudgetHeight - agentExpandedTopReserve)
    )
    readonly property real agentExpandedHeight: Math.max(
        agentPending ? 216 : 184,
        agentExpandedTopReserve + agentSessionListHeight
    )
    readonly property real hoverBridgeMargin: 10
    readonly property real targetWidth: agentActive
        ? (agentExpanded ? agentExpandedWidth : (musicExpanded ? expandedWidth : agentCompactWidth))
        : (musicExpanded ? expandedWidth : (musicPrimaryActive ? compactWidth : idleRow.implicitWidth))
    readonly property real targetHeight: agentExpanded ? agentExpandedHeight : (musicExpanded ? expandedHeight : collapsedHeight)
    property real attachedCenterOffset: agentActive
        ? (expanded
            ? 0
            : targetWidth / 2 - (dualIslandActive ? agentMinimalClockCenterX : agentCompactClockCenterX))
        : 0
    readonly property color surfaceColor: shellRoot ? shellRoot.moduleBackground : "#303030"
    readonly property color textColor: shellRoot ? shellRoot.primaryText : "#cdd6f4"
    readonly property color mutedTextColor: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.68 : 0.62) : Qt.rgba(0.8, 0.8, 0.8, 0.68)
    readonly property color strokeColor: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.13 : 0.10) : Qt.rgba(0.8, 0.8, 0.8, 0.12)
    readonly property color hoverFill: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.10 : 0.12) : Qt.rgba(0.8, 0.8, 0.8, 0.12)
    readonly property color softFill: shellRoot ? shellRoot.withAlpha(shellRoot.darkMode ? "#ffffff" : "#ffffff", shellRoot.darkMode ? 0.07 : 0.23) : Qt.rgba(1, 1, 1, 0.1)
    readonly property color accentColor: shellRoot ? shellRoot.launchColor : "#407cdd"
    readonly property color accentSoftColor: shellRoot ? shellRoot.withAlpha(shellRoot.launchColor, shellRoot.darkMode ? 0.72 : 0.78) : "#407cdd"
    readonly property real displayedPositionSeconds: seeking ? seekPreviewSeconds : mediaPositionSeconds
    readonly property real progressRatio: mediaLengthSeconds > 0 ? Math.max(0, Math.min(1, displayedPositionSeconds / mediaLengthSeconds)) : 0

    width: targetWidth
    height: targetHeight
    implicitWidth: width
    implicitHeight: height
    z: expanded ? 40 : 1
    transformOrigin: Item.Top
    scale: 1
    clip: true

    Behavior on width {
        NumberAnimation {
            duration: 210
            easing.type: Easing.OutCubic
        }
    }

    Behavior on height {
        NumberAnimation {
            duration: 210
            easing.type: Easing.OutCubic
        }
    }

    Behavior on attachedCenterOffset {
        NumberAnimation {
            duration: 210
            easing.type: Easing.OutCubic
        }
    }

    function trackTime(seconds) {
        const value = Number(seconds);
        if (!isFinite(value) || value < 0) {
            return "0:00";
        }
        const rounded = Math.floor(value);
        const minutes = Math.floor(rounded / 60);
        const remainder = rounded % 60;
        return minutes + ":" + (remainder < 10 ? "0" : "") + remainder;
    }

    function displayTitle() {
        return mediaTitle.length > 0 ? mediaTitle : "Unknown track";
    }

    function displayArtist() {
        if (mediaArtist.length > 0) {
            return mediaArtist;
        }
        return "";
    }

    function clampTrackSeconds(seconds) {
        const value = Number(seconds);
        if (!isFinite(value) || value < 0) {
            return 0;
        }
        if (mediaLengthSeconds <= 0) {
            return value;
        }
        return Math.max(0, Math.min(mediaLengthSeconds, value));
    }

    function agentField(agent, key, fallback) {
        if (!agent || agent[key] === undefined || agent[key] === null) {
            return fallback;
        }
        return String(agent[key]);
    }

    function agentIconSource(agent) {
        const source = agentField(agent, "source", "");
        if (!shellRoot) {
            return "";
        }
        if (source === "codex") {
            return "file://" + shellRoot.configDir + "/quickshell/assets/agent/codex.png";
        }
        if (source === "claude") {
            return "file://" + shellRoot.configDir + "/quickshell/assets/agent/claude.png";
        }
        return "";
    }

    function agentDisplayTitle(agent) {
        const project = agentField(agent, "project", "");
        if (project.length > 0 && project !== "Session") {
            return project;
        }
        return agentField(agent, "source_label", "Agent");
    }

    function agentStatusText(agent) {
        const status = agentField(agent, "status", "");
        if (status === "waitingApproval") {
            return "Waiting approval";
        }
        if (status === "waitingQuestion") {
            return "Waiting input";
        }
        if (status === "running") {
            return agentField(agent, "current_tool", "Running") || "Running";
        }
        if (status === "processing") {
            return "Thinking";
        }
        if (isAgentDone(agent)) {
            return "Done";
        }
        return agentField(agent, "detail", "Working");
    }

    function isAgentDone(agent) {
        const status = agentField(agent, "status", "").toLowerCase();
        return status === "completed"
            || status === "complete"
            || status === "done"
            || status === "success"
            || status === "succeeded";
    }

    function agentProgress(agent) {
        if (!agent) {
            return 0;
        }
        if (isAgentDone(agent)) {
            return 1;
        }
        const parsed = Number(agent.progress);
        if (!isFinite(parsed)) {
            return 0.35;
        }
        return Math.max(0, Math.min(1, parsed));
    }

    function agentAccent(agent) {
        return accentColor;
    }

    function dualCompactHoverTarget(localX) {
        const value = Number(localX);
        if (!isFinite(value)) {
            return "";
        }
        return value >= agentMinimalWidth ? "music" : "agent";
    }

    function agentMinimalGlyph(agent) {
        const status = agentField(agent, "status", "");
        if (status === "waitingApproval") {
            return "!";
        }
        if (status === "waitingQuestion") {
            return "?";
        }
        if (isAgentDone(agent)) {
            return "✓";
        }
        return "";
    }

    function pendingKind() {
        if (!agentPending || agentPending.kind === undefined || agentPending.kind === null) {
            return "";
        }
        return String(agentPending.kind);
    }

    function pendingRequestId() {
        if (!agentPending || agentPending.id === undefined || agentPending.id === null) {
            return "current";
        }
        return String(agentPending.id);
    }

    function pendingQuestionText() {
        if (!agentPending) {
            return "";
        }
        const value = agentPending.question || agentPending.detail || "";
        return String(value);
    }

    function pendingOptions() {
        if (!agentPending || !Array.isArray(agentPending.options)) {
            return [];
        }
        return agentPending.options.slice(0, 3).map(option => String(option));
    }

    onHasMusicContentChanged: {
        if (!hasMusicContent) {
            mediaManuallyHidden = false;
        }
    }

    onMusicExpandedChanged: {
        if (!dualIslandActive) {
            keepPrimaryCollapsedForMusic = false;
            musicFrameRelease.stop();
            return;
        }
        keepPrimaryCollapsedForMusic = true;
        if (musicExpanded) {
            musicFrameRelease.stop();
        } else {
            musicFrameRelease.restart();
        }
    }

    component IslandTextButton: Item {
        id: button

        property string label: ""
        property color labelColor: island.textColor
        property string fontFamily: island.shellRoot ? island.shellRoot.baseFont : "JetBrainsMono Nerd Font"
        property int fontPixelSize: 16
        property int fontWeight: Font.Bold
        property real paddingLeft: 8
        property real paddingRight: 8
        property bool interactive: true

        signal clicked()

        readonly property real effectivePaddingLeft: Math.max(0, paddingLeft)
        readonly property real effectivePaddingRight: Math.max(0, paddingRight)

        width: labelText.implicitWidth + effectivePaddingLeft + effectivePaddingRight
        height: island.collapsedHeight
        implicitWidth: width
        implicitHeight: height
        scale: buttonMouse.pressed ? 0.96 : 1

        Behavior on scale {
            NumberAnimation {
                duration: 120
                easing.type: Easing.OutCubic
            }
        }

        Rectangle {
            anchors.fill: parent
            anchors.margins: 0
            radius: 19
            color: island.hoverFill
            opacity: buttonMouse.containsMouse ? 1 : 0

            Behavior on opacity {
                NumberAnimation {
                    duration: 120
                    easing.type: Easing.OutCubic
                }
            }
        }

        Text {
            id: labelText

            anchors.left: parent.left
            anchors.right: parent.right
            anchors.leftMargin: button.effectivePaddingLeft
            anchors.rightMargin: button.effectivePaddingRight
            anchors.verticalCenter: parent.verticalCenter
            text: button.label
            color: button.labelColor
            font.family: button.fontFamily
            font.pixelSize: button.fontPixelSize
            font.weight: button.fontWeight
            horizontalAlignment: Text.AlignHCenter
            renderType: Text.NativeRendering
        }

        MouseArea {
            id: buttonMouse

            anchors.fill: parent
            enabled: button.interactive
            hoverEnabled: true
            cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
            onClicked: button.clicked()
        }
    }

    component AlbumArtFrame: Rectangle {
        id: artFrame

        property real artRadius: 9

        radius: artRadius
        color: island.softFill
        border.width: 0
        clip: true
        antialiasing: true

        Rectangle {
            id: albumArtMask

            anchors.fill: parent
            radius: artFrame.artRadius
            visible: false
            layer.enabled: true
        }

        Image {
            id: albumArt

            anchors.fill: parent
            source: island.musicActive && island.mediaArtUrl.length > 0 ? island.mediaArtUrl : ""
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            smooth: true
            mipmap: true
            visible: status === Image.Ready && String(source).length > 0
            layer.enabled: true
            layer.effect: MultiEffect {
                maskEnabled: true
                maskSource: albumArtMask
                maskThresholdMin: 0.5
                maskSpreadAtMin: 1
            }
        }

        Text {
            anchors.centerIn: parent
            visible: !albumArt.visible
            text: ""
            color: island.accentColor
            font.family: island.shellRoot ? island.shellRoot.iconFont : "JetBrainsMono Nerd Font"
            font.pixelSize: Math.round(Math.min(parent.width, parent.height) * 0.46)
            font.weight: Font.Bold
            renderType: Text.NativeRendering
        }
    }

    component MediaGlyphButton: Item {
        id: control

        property string glyph: ""
        property bool primary: false

        signal clicked()

        width: primary ? 44 : 40
        height: primary ? 44 : 40
        scale: controlMouse.pressed ? 0.92 : (controlMouse.containsMouse ? 1.06 : 1)

        Behavior on scale {
            NumberAnimation {
                duration: 120
                easing.type: Easing.OutCubic
            }
        }

        Rectangle {
            anchors.fill: parent
            radius: height / 2
            color: control.primary ? island.accentSoftColor : island.softFill
            border.width: control.primary ? 0 : 1
            border.color: island.strokeColor
            opacity: controlMouse.containsMouse || control.primary ? 1 : 0.82
        }

        Text {
            anchors.centerIn: parent
            anchors.verticalCenterOffset: control.primary ? 1 : 0
            text: control.glyph
            color: control.primary ? "#ffffff" : island.textColor
            font.family: island.shellRoot ? island.shellRoot.iconFont : "JetBrainsMono Nerd Font"
            font.pixelSize: control.primary ? 21 : 17
            font.weight: Font.Bold
            renderType: Text.NativeRendering
        }

        MouseArea {
            id: controlMouse

            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: control.clicked()
        }
    }

    component AgentProgressRing: Item {
        id: ring

        property real value: 0
        property color ringColor: island.accentColor
        property color trackColor: island.softFill

        width: 25
        height: 25

        onValueChanged: ringCanvas.requestPaint()
        onRingColorChanged: ringCanvas.requestPaint()
        onTrackColorChanged: ringCanvas.requestPaint()
        onWidthChanged: ringCanvas.requestPaint()
        onHeightChanged: ringCanvas.requestPaint()

        Canvas {
            id: ringCanvas

            anchors.fill: parent
            antialiasing: true
            onPaint: {
                const ctx = getContext("2d");
                ctx.reset();
                const size = Math.min(width, height);
                const center = size / 2;
                const radius = Math.max(1, center - 3);
                const progress = Math.max(0, Math.min(1, ring.value));
                ctx.lineWidth = 3;
                ctx.lineCap = "round";
                ctx.strokeStyle = ring.trackColor;
                ctx.beginPath();
                ctx.arc(center, center, radius, 0, Math.PI * 2, false);
                ctx.stroke();
                ctx.strokeStyle = ring.ringColor;
                ctx.beginPath();
                ctx.arc(center, center, radius, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * progress, false);
                ctx.stroke();
            }
        }
    }

    component AgentLogo: Item {
        id: logo

        property var agentData: null

        width: 24
        height: 24

        Image {
            anchors.fill: parent
            anchors.margins: 3
            source: island.agentIconSource(logo.agentData)
            fillMode: Image.PreserveAspectFit
            smooth: true
            mipmap: true
            visible: status === Image.Ready
        }

        Text {
            anchors.centerIn: parent
            visible: parent.children[0].status !== Image.Ready
            text: island.agentField(logo.agentData, "source", "a").slice(0, 1).toUpperCase()
            color: island.agentAccent(logo.agentData)
            font.family: island.shellRoot ? island.shellRoot.baseFont : "JetBrainsMono Nerd Font"
            font.pixelSize: 12
            font.weight: Font.Black
            renderType: Text.NativeRendering
        }
    }

    component AgentCompactPill: Rectangle {
        id: agentPill

        property var agentData: null
        property bool minimal: false

        width: agentPill.minimal ? island.agentMinimalWidth : island.agentCompactPillWidth
        height: island.collapsedHeight
        radius: height / 2
        color: agentPill.minimal && island.primaryFrameCollapsedForMusic ? island.surfaceColor : "transparent"
        border.width: agentPill.minimal && island.primaryFrameCollapsedForMusic ? 1 : 0
        border.color: island.strokeColor
        antialiasing: true

        Item {
            id: minimalLayout

            anchors.fill: parent
            visible: agentPill.minimal

            Text {
                width: island.agentMinimalClockWidth
                x: island.agentMinimalLeftPadding + island.agentMinimalStatusSlotWidth + island.agentMinimalGap
                anchors.verticalCenter: parent.verticalCenter
                text: Qt.formatTime(island.now, "hh:mm")
                color: island.textColor
                font.family: island.shellRoot ? island.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                font.pixelSize: 16
                font.weight: Font.Bold
                horizontalAlignment: Text.AlignHCenter
                renderType: Text.NativeRendering
            }

            Item {
                width: island.agentMinimalStatusSlotWidth
                height: parent.height
                x: island.agentMinimalLeftPadding
                anchors.verticalCenter: parent.verticalCenter

                AgentProgressRing {
                    width: island.agentMinimalRingSize
                    height: island.agentMinimalRingSize
                    anchors.centerIn: parent
                    value: island.agentProgress(agentPill.agentData)
                    ringColor: island.agentAccent(agentPill.agentData)
                    trackColor: island.softFill
                }

                Text {
                    anchors.centerIn: parent
                    text: island.agentMinimalGlyph(agentPill.agentData)
                    visible: text.length > 0
                    color: island.textColor
                    font.family: island.shellRoot ? island.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                    font.pixelSize: 10
                    font.weight: Font.Black
                    renderType: Text.NativeRendering
                }
            }
        }

        Item {
            id: compactLayout

            anchors.fill: parent
            visible: !agentPill.minimal

            AgentLogo {
                id: compactLogo

                width: island.agentCompactLogoSize
                height: island.agentCompactLogoSize
                anchors.right: compactClock.left
                anchors.rightMargin: island.agentCompactLogoGap
                anchors.verticalCenter: parent.verticalCenter
                agentData: agentPill.agentData
            }

            Text {
                id: compactClock

                width: island.agentCompactClockWidth
                x: island.agentCompactLeftExtent
                anchors.verticalCenter: parent.verticalCenter
                text: Qt.formatTime(island.now, "hh:mm")
                color: island.textColor
                font.family: island.shellRoot ? island.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                font.pixelSize: 16
                font.weight: Font.Bold
                horizontalAlignment: Text.AlignHCenter
                renderType: Text.NativeRendering
            }

            AgentProgressRing {
                id: compactRing

                width: island.agentCompactRingSize
                height: island.agentCompactRingSize
                anchors.left: compactClock.right
                anchors.leftMargin: island.agentCompactRingGap
                anchors.verticalCenter: parent.verticalCenter
                value: island.agentProgress(agentPill.agentData)
                ringColor: island.agentAccent(agentPill.agentData)
                trackColor: island.softFill
            }

            Text {
                anchors.centerIn: compactRing
                text: "✓"
                visible: island.isAgentDone(agentPill.agentData)
                color: island.textColor
                font.family: island.shellRoot ? island.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                font.pixelSize: 10
                font.weight: Font.Black
                renderType: Text.NativeRendering
            }
        }

        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: island.agentFocusRequested(island.agentField(agentPill.agentData, "id", ""))
            onEntered: island.hoverTarget = "agent"
        }
    }

    component MusicMinimalPill: Rectangle {
        id: musicPill

        width: island.musicDetachedWidth
        height: island.collapsedHeight
        radius: height / 2
        color: island.surfaceColor
        border.width: 1
        border.color: island.strokeColor
        antialiasing: true

        AlbumArtFrame {
            width: 28
            height: 28
            artRadius: 14
            anchors.centerIn: parent
        }

        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: island.appFocusRequested()
            onEntered: island.hoverTarget = "music"
        }
    }

    component AgentActionButton: Item {
        id: actionButton

        property string label: ""
        property color buttonColor: island.softFill
        property color labelColor: island.textColor

        signal clicked()

        width: Math.max(56, labelText.implicitWidth + 22)
        height: 30
        scale: actionMouse.pressed ? 0.95 : 1

        Rectangle {
            anchors.fill: parent
            radius: 15
            color: actionButton.buttonColor
            border.width: 1
            border.color: island.strokeColor
            opacity: actionMouse.containsMouse ? 1 : 0.86
        }

        Text {
            id: labelText

            anchors.centerIn: parent
            text: actionButton.label
            color: actionButton.labelColor
            font.family: island.shellRoot ? island.shellRoot.baseFont : "JetBrainsMono Nerd Font"
            font.pixelSize: 12
            font.weight: Font.Bold
            renderType: Text.NativeRendering
        }

        MouseArea {
            id: actionMouse

            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: actionButton.clicked()
        }
    }

    Rectangle {
        id: islandFrame

        x: island.primaryFrameCollapsedForMusic
            ? island.width / 2 - island.attachedCenterOffset - island.agentMinimalClockCenterX
            : 0
        anchors.top: parent.top
        width: island.mainIslandWidth
        height: island.primaryFrameCollapsedForMusic ? island.collapsedHeight : parent.height
        radius: island.primaryFrameCollapsedForMusic ? height / 2 : (island.expanded ? 30 : 24)
        color: island.surfaceColor
        border.width: island.musicPrimaryActive || island.agentActive ? 1 : 0
        border.color: island.strokeColor
        opacity: 1
        visible: !island.primaryFrameCollapsedForMusic
        antialiasing: true
        clip: true

        Behavior on opacity {
            NumberAnimation {
                duration: 120
                easing.type: Easing.OutQuad
            }
        }

        Behavior on radius {
            NumberAnimation {
                duration: 210
                easing.type: Easing.OutCubic
            }
        }

        Behavior on width {
            NumberAnimation {
                duration: 210
                easing.type: Easing.OutCubic
            }
        }
    }

    Rectangle {
        id: musicExpandedFrame

        z: island.musicExpanded || island.keepPrimaryCollapsedForMusic ? 12 : 0
        x: island.dualIslandActive && island.musicExpanded ? 0 : island.dualMusicLeftX
        anchors.top: parent.top
        width: island.dualIslandActive && island.musicExpanded ? island.expandedWidth : island.musicDetachedWidth
        height: parent.height
        radius: island.musicExpanded ? 30 : height / 2
        color: island.surfaceColor
        border.width: 1
        border.color: island.strokeColor
        opacity: island.dualIslandActive && island.musicExpanded ? 1 : 0
        visible: opacity > 0.01
        antialiasing: true
        clip: true

        Behavior on opacity {
            NumberAnimation {
                duration: 120
                easing.type: Easing.OutQuad
            }
        }

        Behavior on radius {
            NumberAnimation {
                duration: 210
                easing.type: Easing.OutCubic
            }
        }

        Behavior on x {
            NumberAnimation {
                duration: 210
                easing.type: Easing.OutCubic
            }
        }

        Behavior on width {
            NumberAnimation {
                duration: 210
                easing.type: Easing.OutCubic
            }
        }
    }

    Item {
        id: idleContent

        anchors.fill: islandFrame
        opacity: island.musicPrimaryActive || island.agentActive ? 0 : 1
        visible: opacity > 0.01

        Behavior on opacity {
            NumberAnimation {
                duration: 120
                easing.type: Easing.OutQuad
            }
        }

        Row {
            id: idleRow

            anchors.centerIn: parent
            spacing: 0

            IslandTextButton {
                label: ""
                paddingLeft: 11
                paddingRight: 12
                onClicked: island.lockClicked()
            }

            IslandTextButton {
                label: Qt.formatTime(island.now, "hh:mm")
                paddingLeft: 0
                paddingRight: 0
                interactive: false
            }

            IslandTextButton {
                label: ""
                paddingLeft: 10
                paddingRight: 13
                onClicked: island.powerClicked()
            }
        }
    }

    Item {
        id: compactMusic

        anchors.fill: islandFrame
        opacity: island.musicPrimaryActive && !island.expanded ? 1 : 0
        visible: opacity > 0.01

        Behavior on opacity {
            NumberAnimation {
                duration: 140
                easing.type: Easing.OutQuad
            }
        }

        Row {
            id: compactMusicRow

            anchors.fill: parent
            anchors.leftMargin: 7
            anchors.rightMargin: 7
            anchors.topMargin: 6
            anchors.bottomMargin: 6
            spacing: 3

            AlbumArtFrame {
                id: compactAlbumArt

                width: 24
                height: 24
                artRadius: 8
                anchors.verticalCenter: parent.verticalCenter
                anchors.verticalCenterOffset: 0
            }

            Text {
                id: compactTimeLabel

                width: 54
                anchors.verticalCenter: parent.verticalCenter
                text: Qt.formatTime(island.now, "hh:mm")
                color: island.textColor
                font.family: island.shellRoot ? island.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                font.pixelSize: 16
                font.weight: Font.Bold
                horizontalAlignment: Text.AlignHCenter
                renderType: Text.NativeRendering
            }

            AudioSpectrum {
                id: compactSpectrum

                width: 22
                height: 22
                anchors.verticalCenter: parent.verticalCenter
                values: island.spectrumValues
                active: island.mediaPlaying
                barCount: 6
                barColor: island.accentColor
                peakColor: island.textColor
                quietColor: island.mutedTextColor
                minimumBarRatio: 0.18
                amplitudeGain: 2.05
                heightRangeScale: 0.65
            }
        }

        TapHandler {
            acceptedButtons: Qt.LeftButton
            enabled: island.musicPrimaryActive && !island.expanded && !island.seeking
            gesturePolicy: TapHandler.WithinBounds
            onTapped: island.appFocusRequested()
        }

        HoverHandler {
            enabled: island.musicPrimaryActive && !island.expanded
            onHoveredChanged: if (hovered) {
                island.hoverTarget = "music";
            }
        }
    }

    Item {
        id: expandedMusic

        z: island.musicExpanded || island.keepPrimaryCollapsedForMusic ? 13 : 0
        anchors.fill: island.dualIslandActive ? musicExpandedFrame : islandFrame
        anchors.margins: 13
        opacity: island.musicExpanded ? 1 : 0
        visible: opacity > 0.01

        Behavior on opacity {
            NumberAnimation {
                duration: 150
                easing.type: Easing.OutQuad
            }
        }

        HoverHandler {
            enabled: island.musicExpanded
            onHoveredChanged: if (hovered) {
                island.hoverTarget = "music";
            }
        }

        Row {
            id: expandedTopRow

            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            height: 72
            spacing: 12

            AlbumArtFrame {
                width: 70
                height: 70
                artRadius: 16
            }

            Column {
                width: parent.width - 70 - 12 - 70 - 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 4

                Text {
                    width: parent.width
                    text: island.displayTitle()
                    color: island.textColor
                    font.family: island.shellRoot ? island.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                    font.pixelSize: 18
                    font.weight: Font.Bold
                    elide: Text.ElideRight
                    renderType: Text.NativeRendering
                }

                Text {
                    width: parent.width
                    text: island.displayArtist()
                    visible: text.length > 0
                    color: island.mutedTextColor
                    font.family: island.shellRoot ? island.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                    font.pixelSize: 13
                    font.weight: Font.DemiBold
                    elide: Text.ElideRight
                    renderType: Text.NativeRendering
                }

            }

            AudioSpectrum {
                width: 70
                height: 56
                anchors.verticalCenter: parent.verticalCenter
                values: island.spectrumValues
                active: island.mediaPlaying
                barCount: 8
                barColor: island.accentColor
                peakColor: island.textColor
                quietColor: island.mutedTextColor
                minimumBarRatio: 0.10
                amplitudeGain: 1.75
                heightRangeScale: 0.65
            }

            TapHandler {
                acceptedButtons: Qt.LeftButton
                enabled: island.musicExpanded && !island.seeking
                gesturePolicy: TapHandler.WithinBounds
                onTapped: island.appFocusRequested()
            }
        }

        Rectangle {
            id: progressTrack

            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: expandedTopRow.bottom
            anchors.topMargin: 14
            height: 5
            radius: height / 2
            color: island.softFill
            clip: true

            function secondsForX(localX) {
                if (island.mediaLengthSeconds <= 0 || width <= 0) {
                    return 0;
                }
                const ratio = Math.max(0, Math.min(1, Number(localX) / width));
                return island.clampTrackSeconds(ratio * island.mediaLengthSeconds);
            }

            Rectangle {
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                width: Math.max(parent.height, parent.width * island.progressRatio)
                radius: parent.radius
                color: island.accentColor
                visible: island.mediaLengthSeconds > 0

                Behavior on width {
                    enabled: !island.seeking

                    NumberAnimation {
                        duration: 180
                        easing.type: Easing.OutCubic
                    }
                }
            }
        }

        MouseArea {
            id: progressSeekMouse

            anchors.fill: progressTrack
            anchors.margins: -8
            enabled: island.mediaLengthSeconds > 0
            hoverEnabled: true
            cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
            z: 2

            function updateSeekPreview(mouseX, mouseY) {
                const point = progressSeekMouse.mapToItem(progressTrack, mouseX, mouseY);
                const target = progressTrack.secondsForX(point.x);
                island.seekPreviewSeconds = target;
                return target;
            }

            onPressed: function(mouse) {
                seekPreviewReset.stop();
                island.seeking = true;
                updateSeekPreview(mouse.x, mouse.y);
            }

            onPositionChanged: function(mouse) {
                if (progressSeekMouse.pressed) {
                    updateSeekPreview(mouse.x, mouse.y);
                }
            }

            onReleased: function(mouse) {
                const target = updateSeekPreview(mouse.x, mouse.y);
                island.seekRequested(target);
                seekPreviewReset.restart();
            }

            onCanceled: seekPreviewReset.restart()
        }

        Row {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: progressTrack.bottom
            anchors.topMargin: 5

            Text {
                width: parent.width / 2
                text: island.mediaLengthSeconds > 0 ? island.trackTime(island.displayedPositionSeconds) : "--:--"
                color: island.mutedTextColor
                font.family: island.shellRoot ? island.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                font.pixelSize: 9
                font.weight: Font.Bold
                renderType: Text.NativeRendering
            }

            Text {
                width: parent.width / 2
                text: island.mediaLengthSeconds > 0 ? "-" + island.trackTime(Math.max(0, island.mediaLengthSeconds - island.displayedPositionSeconds)) : "--:--"
                color: island.mutedTextColor
                font.family: island.shellRoot ? island.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                font.pixelSize: 9
                font.weight: Font.Bold
                horizontalAlignment: Text.AlignRight
                renderType: Text.NativeRendering
            }
        }

        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom
            height: 44
            spacing: 18

            MediaGlyphButton {
                anchors.verticalCenter: parent.verticalCenter
                glyph: "\u{f04ae}"
                onClicked: island.previousClicked()
            }

            MediaGlyphButton {
                anchors.verticalCenter: parent.verticalCenter
                glyph: island.mediaPlaying ? "\u{f03e4}" : "\u{f040a}"
                primary: true
                onClicked: island.playPauseClicked()
            }

            MediaGlyphButton {
                anchors.verticalCenter: parent.verticalCenter
                glyph: "\u{f04ad}"
                onClicked: island.nextClicked()
            }
        }
    }

    Item {
        id: compactAgent

        x: island.dualIslandActive && island.expanded
            ? island.width / 2 - island.attachedCenterOffset - island.agentMinimalClockCenterX
            : 0
        y: 0
        height: island.collapsedHeight
        width: island.agentCompactWidth
        opacity: island.agentActive && !island.expanded ? 1 : 0
        visible: opacity > 0.01
        enabled: island.agentActive && !island.expanded

        Behavior on opacity {
            NumberAnimation {
                duration: 140
                easing.type: Easing.OutQuad
            }
        }

        HoverHandler {
            enabled: island.agentActive && !island.expanded && !island.dualIslandActive
            onHoveredChanged: if (hovered) {
                island.hoverTarget = "agent";
            }
        }

        AgentCompactPill {
            id: attachedAgentPill

            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            agentData: island.primaryAgent
            minimal: island.dualIslandActive
        }

        MusicMinimalPill {
            id: detachedMusicPill

            anchors.left: attachedAgentPill.right
            anchors.leftMargin: island.agentDetachedGap
            anchors.verticalCenter: parent.verticalCenter
            visible: island.dualIslandActive
        }

        MouseArea {
            id: dualHoverResolver

            anchors.fill: parent
            enabled: island.dualIslandActive && !island.expanded
            hoverEnabled: true
            acceptedButtons: Qt.NoButton
            cursorShape: Qt.PointingHandCursor
            z: 10

            function updateHoverTarget(localX) {
                const target = island.dualCompactHoverTarget(localX);
                if (target.length > 0) {
                    island.hoverTarget = target;
                }
            }

            onEntered: {
                updateHoverTarget(mouseX);
            }

            onPositionChanged: {
                updateHoverTarget(mouseX);
            }
        }
    }

    Item {
        id: expandedAgent

        anchors.fill: islandFrame
        anchors.margins: 13
        opacity: island.agentExpanded ? 1 : 0
        visible: opacity > 0.01

        Behavior on opacity {
            NumberAnimation {
                duration: 150
                easing.type: Easing.OutQuad
            }
        }

        HoverHandler {
            enabled: island.agentExpanded
            onHoveredChanged: if (hovered) {
                island.hoverTarget = "agent";
            }
        }

        Row {
            id: expandedAgentHeader

            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            height: 58
            spacing: 12

            AgentLogo {
                width: 50
                height: 50
                anchors.verticalCenter: parent.verticalCenter
                agentData: island.primaryAgent
            }

            Column {
                width: parent.width - 50 - 12 - 58 - 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 4

                Text {
                    width: parent.width
                    text: island.agentDisplayTitle(island.primaryAgent)
                    color: island.textColor
                    font.family: island.shellRoot ? island.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                    font.pixelSize: 18
                    font.weight: Font.Bold
                    elide: Text.ElideRight
                    renderType: Text.NativeRendering
                }

                Text {
                    width: parent.width
                    text: island.agentStatusText(island.primaryAgent)
                    color: island.mutedTextColor
                    font.family: island.shellRoot ? island.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                    font.pixelSize: 12
                    font.weight: Font.DemiBold
                    elide: Text.ElideRight
                    renderType: Text.NativeRendering
                }
            }

            Item {
                width: 50
                height: 50
                anchors.verticalCenter: parent.verticalCenter

                AgentProgressRing {
                    anchors.fill: parent
                    value: island.agentProgress(island.primaryAgent)
                    ringColor: island.agentAccent(island.primaryAgent)
                    trackColor: island.softFill
                }

                Text {
                    anchors.centerIn: parent
                    text: "✓"
                    visible: island.isAgentDone(island.primaryAgent)
                    color: island.textColor
                    font.family: island.shellRoot ? island.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                    font.pixelSize: 18
                    font.weight: Font.Black
                    renderType: Text.NativeRendering
                }
            }
        }

        Rectangle {
            id: pendingAgentCard

            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: expandedAgentHeader.bottom
            anchors.topMargin: 11
            height: island.agentPending ? 74 : 0
            radius: 16
            color: island.softFill
            border.width: island.agentPending ? 1 : 0
            border.color: island.strokeColor
            visible: island.agentPending !== null

            Column {
                anchors.left: parent.left
                anchors.right: pendingActions.left
                anchors.leftMargin: 12
                anchors.rightMargin: 10
                anchors.verticalCenter: parent.verticalCenter
                spacing: 4

                Text {
                    width: parent.width
                    text: island.pendingKind().indexOf("question") >= 0 ? "Question" : "Approval"
                    color: island.agentAccent(island.primaryAgent)
                    font.family: island.shellRoot ? island.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                    font.pixelSize: 11
                    font.weight: Font.Black
                    renderType: Text.NativeRendering
                }

                Text {
                    width: parent.width
                    text: island.pendingQuestionText()
                    color: island.textColor
                    font.family: island.shellRoot ? island.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                    font.pixelSize: 12
                    font.weight: Font.DemiBold
                    elide: Text.ElideRight
                    renderType: Text.NativeRendering
                }
            }

            Row {
                id: pendingActions

                anchors.right: parent.right
                anchors.rightMargin: 10
                anchors.verticalCenter: parent.verticalCenter
                spacing: 7

                Repeater {
                    model: island.pendingKind().indexOf("question") >= 0 ? island.pendingOptions() : []

                    delegate: AgentActionButton {
                        required property string modelData

                        label: modelData
                        buttonColor: island.agentAccent(island.primaryAgent)
                        labelColor: "#ffffff"
                        onClicked: island.agentAnswerRequested(island.pendingRequestId(), modelData)
                    }
                }

                AgentActionButton {
                    visible: island.pendingKind().indexOf("question") >= 0
                    label: "Reply"
                    onClicked: island.agentReplyRequested(island.pendingRequestId())
                }

                AgentActionButton {
                    visible: island.pendingKind().indexOf("question") < 0
                    label: "Allow"
                    buttonColor: island.agentAccent(island.primaryAgent)
                    labelColor: "#ffffff"
                    onClicked: island.agentApproveRequested(island.pendingRequestId())
                }

                AgentActionButton {
                    visible: island.pendingKind().indexOf("question") < 0
                    label: "Deny"
                    onClicked: island.agentDenyRequested(island.pendingRequestId())
                }
            }
        }

        ListView {
            id: agentSessionList

            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: island.agentSessionListHeight
            clip: true
            spacing: island.agentSessionSpacing
            model: island.agentSessions
            boundsBehavior: Flickable.StopAtBounds
            interactive: contentHeight > height

            delegate: Rectangle {
                required property var modelData

                width: agentSessionList.width
                height: island.agentSessionRowHeight
                radius: 13
                color: island.softFill
                border.width: 1
                border.color: island.strokeColor

                Row {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    anchors.rightMargin: 8
                    spacing: 7

                    AgentLogo {
                        width: 23
                        height: 23
                        anchors.verticalCenter: parent.verticalCenter
                        agentData: modelData
                    }

                    Column {
                        width: parent.width - 23 - 7 - (island.isAgentDone(modelData) ? 18 + 7 : 0)
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 1

                        Text {
                            width: parent.width
                            text: island.agentDisplayTitle(modelData)
                            color: island.textColor
                            font.family: island.shellRoot ? island.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                            font.pixelSize: 11
                            font.weight: Font.Bold
                            elide: Text.ElideRight
                            renderType: Text.NativeRendering
                        }

                        Text {
                            width: parent.width
                            text: island.agentStatusText(modelData)
                            color: island.mutedTextColor
                            font.family: island.shellRoot ? island.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                            font.pixelSize: 9
                            font.weight: Font.DemiBold
                            elide: Text.ElideRight
                            renderType: Text.NativeRendering
                        }
                    }

                    Item {
                        id: doneGlyphSlot

                        width: 18
                        height: parent.height
                        visible: island.isAgentDone(modelData)

                        Text {
                            anchors.centerIn: parent
                            text: "✓"
                            color: island.agentAccent(modelData)
                            font.family: island.shellRoot ? island.shellRoot.baseFont : "JetBrainsMono Nerd Font"
                            font.pixelSize: 13
                            font.weight: Font.Black
                            renderType: Text.NativeRendering
                        }
                    }
                }

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: island.agentFocusRequested(island.agentField(modelData, "id", ""))
                }
            }
        }

        Rectangle {
            anchors.right: agentSessionList.right
            anchors.rightMargin: 4
            y: agentSessionList.y + agentSessionList.visibleArea.yPosition * agentSessionList.height
            width: 3
            height: Math.max(24, agentSessionList.height * agentSessionList.visibleArea.heightRatio)
            radius: 2
            color: island.mutedTextColor
            opacity: agentSessionList.contentHeight > agentSessionList.height ? 0.34 : 0

            Behavior on opacity {
                NumberAnimation {
                    duration: 120
                    easing.type: Easing.OutQuad
                }
            }
        }
    }

    Timer {
        id: musicFrameRelease

        interval: 230
        repeat: false
        onTriggered: island.keepPrimaryCollapsedForMusic = false
    }

    Timer {
        id: seekPreviewReset

        interval: 700
        repeat: false
        onTriggered: island.seeking = false
    }

    HoverHandler {
        id: islandHover

        margin: island.hoverBridgeMargin
        onHoveredChanged: {
            if (!hovered) {
                island.hoverTarget = "";
                return;
            }
            if (island.hoverTarget.length === 0 && !island.dualIslandActive) {
                island.hoverTarget = island.agentActive ? "agent" : (island.musicActive ? "music" : "");
                return;
            }
        }
    }

    DragHandler {
        id: islandSwipe

        target: null
        xAxis.enabled: island.musicActive && !island.seeking
        yAxis.enabled: false

        onActiveChanged: {
            if (active || !island.hasMusicContent) {
                return;
            }

            const dx = translation.x;
            const dy = translation.y;
            if (Math.abs(dx) < island.swipeThreshold || Math.abs(dx) < Math.abs(dy) * 1.4) {
                return;
            }

            if (dx > 0) {
                island.mediaManuallyHidden = true;
            } else {
                island.mediaManuallyHidden = false;
            }
        }
    }
}
