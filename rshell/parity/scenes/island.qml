import QtQuick

// Island host: reproduces the centre of the bar surface (quickshell/shell.qml
// :3879-3958) around the ORIGINAL DynamicIsland.qml, loaded read-only from
// Harness.source. The canvas origin is the bar surface's (screenCentreX - W/2, 0).
// The calendar panel itself is out of scope here (ui-panels); calendar states
// render the host morph with an empty panel box.
Item {
    id: scene

    readonly property var canvas: Harness.fixture.canvas || ({})
    width: canvas.width || 640
    height: canvas.height || 80

    ShellRootMock {
        id: shellRoot
        darkMode: Harness.dark
        configDir: Harness.source.replace(/\/[^\/]+\/?$/, "")
    }

    // Keeps Qt's animation driver running so QML Timers advance on the
    // virtual clock instead of Qt's real-time pause timer.
    Item { id: clockAnchor; property real v: 0 }
    NumberAnimation {
        target: clockAnchor; property: "v"; from: 0; to: 1
        duration: 1000; loops: Animation.Infinite; running: true
    }

    // shell.qml barWindow calendar state (:3566-3628, :3676-3710)
    property bool calendarVisible: false
    property bool calendarPanelPresented: false
    property real calendarProgress: 0
    property bool islandExpanded: false
    Behavior on calendarProgress { NumberAnimation { duration: 210; easing.type: Easing.OutCubic } }
    onCalendarVisibleChanged: {
        if (calendarVisible) {
            calendarPanelCloseTimer.stop();
            calendarPanelPresented = true;
            Qt.callLater(() => calendarProgress = 1);
        } else {
            calendarProgress = 0;
            if (calendarPanelPresented)
                calendarPanelCloseTimer.restart();
        }
    }
    Timer {
        id: calendarPanelCloseTimer
        interval: 210
        repeat: false
        onTriggered: {
            if (scene.calendarVisible) {
                scene.calendarProgress = 1;
                return;
            }
            scene.calendarProgress = 0;
            scene.calendarPanelPresented = false;
        }
    }
    Timer {
        id: islandCollapseTimer
        interval: 230
        repeat: false
        onTriggered: scene.islandExpanded = false
    }

    property var island: null
    readonly property real barHeight: Math.min(1e9, calendarPanelPresented || islandExpanded ? 820 : 58)

    Rectangle {
        id: centerSection
        objectName: "centerSection"

        readonly property real attachedCenterOffset: scene.calendarPanelPresented ? 0 : (scene.island ? scene.island.attachedCenterOffset : 0)
        width: scene.calendarPanelPresented ? calendarPanel.width : (scene.island ? scene.island.width : 0)
        height: scene.calendarPanelPresented ? calendarPanel.height : (scene.island ? scene.island.height : 0)
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.horizontalCenterOffset: attachedCenterOffset
        anchors.top: parent.top
        anchors.topMargin: 10
        z: 50
        radius: scene.calendarVisible ? 30 : 24
        color: scene.calendarProgress > 0.001 ? shellRoot.moduleBackground : "transparent"
        border.width: scene.calendarProgress > 0.001 ? 1 : 0
        border.color: shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.11 : 0.09)
        antialiasing: true
        clip: true

        Behavior on width { enabled: scene.calendarPanelPresented; NumberAnimation { duration: 210; easing.type: Easing.OutCubic } }
        Behavior on height { enabled: scene.calendarPanelPresented; NumberAnimation { duration: 210; easing.type: Easing.OutCubic } }
        Behavior on radius { enabled: scene.calendarPanelPresented; NumberAnimation { duration: 210; easing.type: Easing.OutCubic } }

        Item {
            id: calendarPanel
            // Loader box from shell.qml:3962-3970 (content not rendered here).
            anchors.top: parent.top
            anchors.horizontalCenter: parent.horizontalCenter
            width: Math.min(520, (scene.canvas.surfaceWidth || 6144 / 2) - 48)
            height: Math.min(500, scene.barHeight - 78)
            visible: scene.calendarPanelPresented
            opacity: scene.calendarProgress
        }
    }

    Component.onCompleted: {
        const component = Qt.createComponent("file://" + Harness.source + "/DynamicIsland.qml");
        if (component.status !== Component.Ready) {
            console.error(component.errorString());
            Qt.exit(4);
            return;
        }
        island = component.createObject(centerSection, {
            objectName: "island",
            shellRoot: shellRoot,
        });
        island.anchors.horizontalCenter = centerSection.horizontalCenter;
        island.anchors.top = centerSection.top;
        island.calendarOpen = Qt.binding(() => scene.calendarPanelPresented);
        island.opacity = Qt.binding(() => 1 - scene.calendarProgress);
        island.visible = Qt.binding(() => island.opacity > 0.01);
        island.expandedChanged.connect(() => {
            if (island.expanded) {
                islandCollapseTimer.stop();
                scene.islandExpanded = true;
            } else {
                islandCollapseTimer.restart();
            }
        });
        if (Harness.fixture.trace)
            traceHandlers(island, "island");
        if (Harness.fixture.dump_text)
            Qt.callLater(() => dumpTexts(island));
        island.calendarClicked.connect(() => scene.calendarVisible = !scene.calendarVisible);
        // Local state effects of the host handlers (shell.qml:2872-2888, :3951-3955);
        // the external commands (playerctl, focus scripts) are not run.
        island.seekRequested.connect(seconds => {
            const length = island.mediaLengthSeconds;
            island.mediaPositionSeconds = length > 0 ? Math.max(0, Math.min(length, seconds)) : Math.max(0, seconds);
        });
        island.playPauseClicked.connect(() => island.mediaPlaying = !island.mediaPlaying);
    }

    // Debug aid (fixture "trace": true): log pointer-handler activation.
    function traceHandlers(item, path) {
        const kids = item.data || [];
        for (let i = 0; i < kids.length; ++i) {
            const o = kids[i];
            if (o && o.activeChanged !== undefined && o.translation !== undefined && console.log("trace: watching DragHandler at", path) === undefined)
            {
                o.activeChanged.connect(() => console.log("trace", path, "DragHandler active", o.active, "translation.x", o.translation.x,
                                                          "activeTranslation.x", o.activeTranslation ? o.activeTranslation.x : "n/a"));
                o.translationChanged.connect(() => console.log("trace", path, "translation.x", o.translation.x));
            }
            if (o && o.data !== undefined)
                traceHandlers(o, path + "/" + i);
        }
    }

    // Debug aid (fixture "dump_text": true): log Qt's measured text metrics.
    function dumpTexts(item) {
        const kids = item.children || [];
        for (let i = 0; i < kids.length; ++i) {
            const o = kids[i];
            if (o.font !== undefined && o.text !== undefined && o.implicitWidth !== undefined && o.text.length > 0)
                console.log("text", JSON.stringify(o.text), "px", o.font.pixelSize, "weight", o.font.weight,
                            "implicit", o.implicitWidth.toFixed(3) + "x" + o.implicitHeight.toFixed(3),
                            "contentW", o.contentWidth.toFixed(3), "visible", o.visible);
            dumpTexts(o);
        }
    }

    function resolve(value) {
        if (typeof value === "string" && value.startsWith("@asset:"))
            return "file://" + Harness.assets + "/" + value.slice(7);
        return value;
    }

    // Fixture "set" / "props": island properties, plus scene-level keys:
    //   now: "HH:MM" (or ISO date), calendar: bool
    function applyProps(map) {
        if (!map || !island)
            return;
        for (const key in map) {
            const value = map[key];
            if (key === "calendar") {
                scene.calendarVisible = !!value;
            } else if (key === "now") {
                const text = String(value);
                const d = text.length <= 5 ? new Date(2026, 9, 23, Number(text.slice(0, 2)), Number(text.slice(3, 5))) : new Date(text);
                island.now = d;
            } else if (value !== null && typeof value === "object") {
                // QVariantList/Map from C++ are not JS arrays (Array.isArray
                // fails in DynamicIsland.qml:49); deep-copy into JS values.
                island[key] = JSON.parse(JSON.stringify(value));
            } else {
                island[key] = resolve(value);
            }
        }
    }
}
