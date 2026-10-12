import QtQuick

Rectangle {
    id: root

    property var shellRoot: null
    property var bridge: null
    property bool embeddedSurface: false
    property var timedLayout: []
    property var allDayLayout: []
    property var hoveredEvent: null
    property real tooltipAnchorX: 0
    property real tooltipBelowY: 0
    property real tooltipAboveY: 0
    property int allDayLaneCount: 0
    property real hourHeight: 44
    property bool horizontalScrollInitialized: false
    property bool rebasingHorizontalScroll: false
    readonly property int bufferDayCount: 21
    readonly property int bufferLeadingDays: 7
    readonly property real toolbarHeight: 58
    readonly property real dayHeaderHeight: 48
    readonly property real allDayHeight: Math.max(34, Math.min(106, 12 + allDayLaneCount * 24))
    readonly property real timeColumnWidth: 58
    readonly property real dayWidth: (width - timeColumnWidth - 12) / 7
    readonly property bool pointerInside: visible && panelHover.hovered
    readonly property date currentTime: shellRoot ? shellRoot.now : new Date()
    readonly property color textColor: shellRoot ? shellRoot.primaryText : "#cdd6f4"
    readonly property color mutedColor: shellRoot ? shellRoot.withAlpha(textColor, shellRoot.darkMode ? 0.62 : 0.58) : "#8a8a96"
    readonly property color accentColor: shellRoot ? shellRoot.launchColor : "#407cdd"
    readonly property color lineColor: shellRoot ? shellRoot.withAlpha(textColor, shellRoot.darkMode ? 0.11 : 0.09) : "#303030"
    readonly property color elevatedColor: shellRoot ? shellRoot.withAlpha(shellRoot.darkMode ? "#ffffff" : "#ffffff", shellRoot.darkMode ? 0.055 : 0.30) : "#303030"

    signal closeRequested()
    signal expandRequested()

    radius: embeddedSurface ? 0 : 28
    color: embeddedSurface ? "transparent" : (shellRoot ? shellRoot.moduleBackground : "#1e1e2e")
    border.width: embeddedSurface ? 0 : 1
    border.color: lineColor
    clip: true
    antialiasing: true

    component HeaderButton: Rectangle {
        id: headerButton
        property string label: ""
        property bool emphasized: false
        signal clicked()

        implicitWidth: Math.max(36, buttonLabel.implicitWidth + 22)
        implicitHeight: 34
        radius: 17
        color: emphasized ? root.accentColor : (buttonMouse.containsMouse ? root.elevatedColor : "transparent")
        border.width: emphasized ? 0 : 1
        border.color: root.lineColor
        scale: buttonMouse.pressed ? 0.96 : 1

        Behavior on color { ColorAnimation { duration: 120 } }
        Behavior on scale { NumberAnimation { duration: 90 } }

        Text {
            id: buttonLabel
            anchors.centerIn: parent
            text: headerButton.label
            color: headerButton.emphasized ? "#ffffff" : root.textColor
            font.family: root.shellRoot ? root.shellRoot.baseFont : "sans-serif"
            font.pixelSize: 13
            font.weight: Font.DemiBold
            renderType: Text.NativeRendering
        }

        MouseArea {
            id: buttonMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: headerButton.clicked()
        }
    }

    function localDay(value) {
        const d = new Date(value);
        d.setHours(0, 0, 0, 0);
        return d;
    }

    function allDayDate(value) {
        const d = new Date(value);
        return new Date(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate());
    }

    function dayDiff(left, right) {
        const a = Date.UTC(left.getFullYear(), left.getMonth(), left.getDate());
        const b = Date.UTC(right.getFullYear(), right.getMonth(), right.getDate());
        return Math.round((a - b) / 86400000);
    }

    function calendarFor(event) {
        if (!bridge)
            return null;
        const list = bridge.calendars || [];
        for (let i = 0; i < list.length; i++) {
            if (list[i].id === event.calendarId)
                return list[i];
        }
        return null;
    }

    function eventVisible(event) {
        const calendar = calendarFor(event);
        return !calendar || (!calendar.hidden && !calendar.holdsTasks);
    }

    function eventColor(event) {
        const calendar = calendarFor(event);
        return calendar && calendar.color ? calendar.color : root.accentColor;
    }

    function showEventTooltip(event, item) {
        const top = item.mapToItem(root, 0, 0);
        const bottom = item.mapToItem(root, 0, item.height);
        hoveredEvent = event;
        tooltipAnchorX = top.x + item.width / 2;
        tooltipBelowY = bottom.y + 7;
        tooltipAboveY = top.y - 7;
    }

    function eventTimeText(event) {
        if (!event)
            return "";
        if (event.allDay)
            return "全天";
        return Qt.formatDateTime(new Date(event.start), "M月d日 HH:mm")
            + " – " + Qt.formatTime(new Date(event.end), "HH:mm");
    }

    function eventCalendarText(event) {
        const calendar = event ? calendarFor(event) : null;
        return calendar ? (calendar.summary || calendar.name || "") : "";
    }

    function rebuildLayouts() {
        if (!bridge)
            return;
        const week = bufferStartDate();
        const raw = (bridge.events || []).filter(eventVisible);
        const timed = [];
        const allDay = [];

        for (let i = 0; i < raw.length; i++) {
            const event = raw[i];
            if (event.allDay) {
                const startDay = allDayDate(event.start);
                const endDay = allDayDate(event.end);
                const startIndex = Math.max(0, dayDiff(startDay, week));
                const endIndex = Math.min(bufferDayCount, Math.max(startIndex + 1, dayDiff(endDay, week)));
                if (startIndex < bufferDayCount && endIndex > 0) {
                    allDay.push({
                        "event": event,
                        "startDay": startIndex,
                        "span": Math.max(1, endIndex - startIndex),
                        "lane": 0
                    });
                }
                continue;
            }

            const eventStart = new Date(event.start);
            const eventEnd = new Date(event.end);
            for (let day = 0; day < bufferDayCount; day++) {
                const dayStart = new Date(week);
                dayStart.setDate(dayStart.getDate() + day);
                const dayEnd = new Date(dayStart);
                dayEnd.setDate(dayEnd.getDate() + 1);
                if (eventEnd <= dayStart || eventStart >= dayEnd)
                    continue;
                const segmentStart = new Date(Math.max(eventStart.getTime(), dayStart.getTime()));
                const segmentEnd = new Date(Math.min(eventEnd.getTime(), dayEnd.getTime()));
                const startMinute = (segmentStart.getTime() - dayStart.getTime()) / 60000;
                const endMinute = Math.max(startMinute + 15, (segmentEnd.getTime() - dayStart.getTime()) / 60000);
                timed.push({
                    "event": event,
                    "day": day,
                    "startMinute": startMinute,
                    "endMinute": endMinute,
                    "lane": 0,
                    "lanes": 1
                });
            }
        }

        allDay.sort((a, b) => a.startDay - b.startDay || b.span - a.span);
        const allDayEnds = [];
        for (let i = 0; i < allDay.length; i++) {
            let lane = 0;
            while (lane < allDayEnds.length && allDayEnds[lane] > allDay[i].startDay)
                lane++;
            if (lane === allDayEnds.length)
                allDayEnds.push(0);
            allDay[i].lane = lane;
            allDayEnds[lane] = allDay[i].startDay + allDay[i].span;
        }

        for (let day = 0; day < bufferDayCount; day++) {
            const rows = timed.filter(item => item.day === day).sort((a, b) => a.startMinute - b.startMinute || a.endMinute - b.endMinute);
            const laneEnds = [];
            for (let i = 0; i < rows.length; i++) {
                let lane = 0;
                while (lane < laneEnds.length && laneEnds[lane] > rows[i].startMinute)
                    lane++;
                if (lane === laneEnds.length)
                    laneEnds.push(0);
                rows[i].lane = lane;
                laneEnds[lane] = rows[i].endMinute;
            }
            const lanes = Math.max(1, laneEnds.length);
            for (let i = 0; i < rows.length; i++)
                rows[i].lanes = lanes;
        }

        allDayLaneCount = allDayEnds.length;
        allDayLayout = allDay;
        timedLayout = timed;
    }

    function bufferStartDate() {
        const d = new Date(bridge ? bridge.weekStart : currentTime);
        d.setDate(d.getDate() - bufferLeadingDays);
        return d;
    }

    function dayAt(index) {
        const d = bufferStartDate();
        d.setDate(d.getDate() + index);
        return d;
    }

    function isToday(value) {
        const now = currentTime;
        return value.getFullYear() === now.getFullYear()
            && value.getMonth() === now.getMonth()
            && value.getDate() === now.getDate();
    }

    function rangeLabel() {
        if (!bridge)
            return "";
        const firstIndex = Math.max(0, Math.min(bufferDayCount - 1, Math.floor(timeline.contentX / dayWidth + 0.0001)));
        const lastIndex = Math.max(firstIndex, Math.min(bufferDayCount - 1,
            Math.ceil((timeline.contentX + timeline.width) / dayWidth - 0.0001) - 1));
        const start = dayAt(firstIndex);
        const end = dayAt(lastIndex);
        if (start.getFullYear() !== end.getFullYear())
            return Qt.formatDate(start, "yyyy/M/d") + " – " + Qt.formatDate(end, "yyyy/M/d");
        if (start.getMonth() !== end.getMonth())
            return Qt.formatDate(start, "yyyy/M/d") + " – " + Qt.formatDate(end, "M/d");
        return Qt.formatDate(start, "yyyy/M/d") + " – " + Qt.formatDate(end, "d");
    }

    function positionNearNow() {
        const hour = dayDiff(currentTime, bufferStartDate()) >= 0
                && dayDiff(currentTime, bufferStartDate()) < bufferDayCount
            ? currentTime.getHours() : 8;
        timeline.contentY = Math.max(0, Math.min(timeline.contentHeight - timeline.height, (hour - 2) * hourHeight));
    }

    function initializeHorizontalScroll() {
        if (horizontalScrollInitialized || dayWidth <= 0)
            return;
        rebasingHorizontalScroll = true;
        timeline.contentX = bufferLeadingDays * dayWidth;
        rebasingHorizontalScroll = false;
        horizontalScrollInitialized = true;
    }

    function rebaseHorizontalScroll() {
        if (!horizontalScrollInitialized || rebasingHorizontalScroll || !bridge || dayWidth <= 0)
            return;
        const leftThreshold = 3 * dayWidth;
        const rightThreshold = 11 * dayWidth;
        if (timeline.contentX >= leftThreshold && timeline.contentX <= rightThreshold)
            return;

        rebasingHorizontalScroll = true;
        if (timeline.contentX < leftThreshold) {
            timeline.contentX += 7 * dayWidth;
            bridge.previousWeek();
        } else {
            timeline.contentX -= 7 * dayWidth;
            bridge.nextWeek();
        }
        rebasingHorizontalScroll = false;
    }

    function scrollByWeek(direction) {
        initializeHorizontalScroll();
        timeline.contentX = Math.max(0, Math.min(timeline.contentWidth - timeline.width,
            timeline.contentX + direction * 7 * dayWidth));
    }

    function handleCalendarScroll(event) {
        const pixelX = event.pixelDelta ? event.pixelDelta.x : 0;
        const pixelY = event.pixelDelta ? event.pixelDelta.y : 0;
        const deltaX = pixelX !== 0 ? pixelX : event.angleDelta.x;
        const deltaY = pixelY !== 0 ? pixelY : event.angleDelta.y;

        if (deltaX === 0 && deltaY === 0) {
            event.accepted = false;
            return;
        }

        if (Math.abs(deltaX) > Math.abs(deltaY)) {
            const scrollDelta = pixelX !== 0 ? pixelX : deltaX / 120 * dayWidth;
            timeline.contentX = Math.max(0, Math.min(timeline.contentWidth - timeline.width,
                timeline.contentX - scrollDelta));
        } else {
            const scrollDelta = pixelY !== 0 ? pixelY : deltaY / 120 * hourHeight * 3;
            timeline.contentY = Math.max(0, Math.min(timeline.contentHeight - timeline.height,
                timeline.contentY - scrollDelta));
        }
        event.accepted = true;
    }

    onBridgeChanged: {
        rebuildLayouts();
        horizontalScrollInitialized = false;
        Qt.callLater(initializeHorizontalScroll);
    }
    onVisibleChanged: if (visible) Qt.callLater(() => {
        initializeHorizontalScroll();
        positionNearNow();
    })

    Connections {
        target: root.bridge
        function onEventsChanged() { root.rebuildLayouts(); }
        function onCalendarsChanged() { root.rebuildLayouts(); }
        function onWeekStartChanged() { root.rebuildLayouts(); Qt.callLater(root.positionNearNow); }
    }

    HoverHandler {
        id: panelHover
        enabled: root.visible && root.opacity > 0.01
    }

    Component.onCompleted: Qt.callLater(initializeHorizontalScroll)

    Row {
        id: toolbar
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.leftMargin: 14
        anchors.rightMargin: 14
        height: root.toolbarHeight

        Item {
            width: 102
            height: parent.height
            HeaderButton {
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                label: "＋ 创建日程"
                emphasized: true
                onClicked: if (root.bridge) root.bridge.createEvent()
            }
        }

        Row {
            width: parent.width - 178
            height: parent.height
            spacing: 7

            Item { width: Math.max(0, (parent.width - navContent.width) / 2); height: 1 }
            Row {
                id: navContent
                anchors.verticalCenter: parent.verticalCenter
                spacing: 7

                HeaderButton { label: "‹"; onClicked: root.scrollByWeek(-1) }
                HeaderButton {
                    label: root.rangeLabel() + "  ·  " + Qt.formatTime(root.currentTime, "HH:mm")
                    onClicked: root.closeRequested()
                }
                HeaderButton { label: "›"; onClicked: root.scrollByWeek(1) }
            }
        }

        Item {
            width: 76
            height: parent.height
            HeaderButton {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                label: "↗"
                onClicked: root.expandRequested()
            }
        }
    }

    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        y: root.toolbarHeight - 1
        height: 1
        color: root.lineColor
    }

    Item {
        id: dayHeaderViewport
        x: root.timeColumnWidth
        y: root.toolbarHeight
        width: parent.width - root.timeColumnWidth - 12
        height: root.dayHeaderHeight
        clip: true

        WheelHandler {
            target: null
            acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
            onWheel: event => root.handleCalendarScroll(event)
        }

        Row {
            id: dayHeaders
            x: -timeline.contentX
            width: root.bufferDayCount * root.dayWidth
            height: parent.height

            Repeater {
                model: root.bufferDayCount
                delegate: Item {
                    required property int index
                    width: root.dayWidth
                    height: dayHeaders.height
                    readonly property date day: root.dayAt(index)

                    Column {
                        anchors.centerIn: parent
                        spacing: 1
                        Text {
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: ["一", "二", "三", "四", "五", "六", "日"][index % 7]
                            color: root.mutedColor
                            font.family: root.shellRoot ? root.shellRoot.baseFont : "sans-serif"
                            font.pixelSize: 11
                            font.weight: Font.DemiBold
                        }
                        Rectangle {
                            anchors.horizontalCenter: parent.horizontalCenter
                            width: 28
                            height: 24
                            radius: 12
                            color: root.isToday(parent.parent.day) ? root.accentColor : "transparent"
                            Text {
                                anchors.centerIn: parent
                                text: parent.parent.parent.day.getDate()
                                color: root.isToday(parent.parent.parent.day) ? "#ffffff" : root.textColor
                                font.family: root.shellRoot ? root.shellRoot.baseFont : "sans-serif"
                                font.pixelSize: 14
                                font.weight: Font.Bold
                            }
                        }
                    }
                }
            }
        }
    }

    Item {
        id: allDayArea
        x: root.timeColumnWidth
        y: root.toolbarHeight + root.dayHeaderHeight
        width: parent.width - root.timeColumnWidth - 12
        height: root.allDayHeight
        clip: true

        WheelHandler {
            target: null
            acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
            onWheel: event => root.handleCalendarScroll(event)
        }

        Item {
            x: -timeline.contentX
            width: root.bufferDayCount * root.dayWidth
            height: parent.height

            Repeater {
                model: root.allDayLayout
                delegate: Rectangle {
                    required property var modelData
                    x: modelData.startDay * root.dayWidth + 3
                    y: 6 + modelData.lane * 24
                    width: modelData.span * root.dayWidth - 6
                    height: 20
                    radius: 6
                    color: root.shellRoot ? root.shellRoot.withAlpha(root.eventColor(modelData.event), 0.80) : root.eventColor(modelData.event)
                    border.width: 1
                    border.color: root.shellRoot ? root.shellRoot.withAlpha("#ffffff", 0.22) : "transparent"

                    Text {
                        anchors.fill: parent
                        anchors.leftMargin: 7
                        anchors.rightMargin: 5
                        text: modelData.event.summary || "(无标题)"
                        color: "#ffffff"
                        font.family: root.shellRoot ? root.shellRoot.baseFont : "sans-serif"
                        font.pixelSize: 10
                        font.weight: Font.DemiBold
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                    }
                    MouseArea {
                        id: allDayMouse
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onEntered: root.showEventTooltip(modelData.event, parent)
                        onExited: if (root.hoveredEvent === modelData.event) root.hoveredEvent = null
                        onClicked: if (root.bridge) root.bridge.openEvent(modelData.event)
                    }
                }
            }
        }
    }

    Text {
        x: 8
        y: allDayArea.y + 8
        width: root.timeColumnWidth - 14
        text: "全天"
        color: root.mutedColor
        font.family: root.shellRoot ? root.shellRoot.baseFont : "sans-serif"
        font.pixelSize: 10
        horizontalAlignment: Text.AlignRight
    }

    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        y: allDayArea.y + allDayArea.height - 1
        height: 1
        color: root.lineColor
    }

    Flickable {
        id: timeline
        x: root.timeColumnWidth
        y: allDayArea.y + allDayArea.height
        width: parent.width - root.timeColumnWidth - 12
        height: parent.height - y
        contentWidth: root.bufferDayCount * root.dayWidth
        contentHeight: 24 * root.hourHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickDeceleration: 2600
        flickableDirection: Flickable.HorizontalAndVerticalFlick
        pressDelay: 80

        onContentXChanged: root.rebaseHorizontalScroll()

        WheelHandler {
            target: null
            acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
            onWheel: event => root.handleCalendarScroll(event)
        }

        Item {
            id: timelineContent
            width: timeline.contentWidth
            height: timeline.contentHeight

            Repeater {
                model: 25
                delegate: Item {
                    required property int index
                    y: index * root.hourHeight
                    width: timelineContent.width
                    height: 1

                    Rectangle {
                        width: timelineContent.width
                        height: 1
                        color: root.lineColor
                    }
                }
            }

            Repeater {
                model: root.bufferDayCount + 1
                delegate: Rectangle {
                    required property int index
                    x: index * root.dayWidth
                    y: 0
                    width: 1
                    height: timelineContent.height
                    color: root.lineColor
                }
            }

            Repeater {
                model: root.timedLayout
                delegate: Rectangle {
                    required property var modelData
                    readonly property real laneWidth: (root.dayWidth - 6) / modelData.lanes
                    x: modelData.day * root.dayWidth + 3 + modelData.lane * laneWidth
                    y: modelData.startMinute / 60 * root.hourHeight + 1
                    width: Math.max(18, laneWidth - 2)
                    height: Math.max(18, (modelData.endMinute - modelData.startMinute) / 60 * root.hourHeight - 2)
                    radius: 7
                    color: root.shellRoot ? root.shellRoot.withAlpha(root.eventColor(modelData.event), 0.84) : root.eventColor(modelData.event)
                    border.width: 1
                    border.color: root.shellRoot ? root.shellRoot.withAlpha("#ffffff", 0.24) : "transparent"
                    clip: true

                    Column {
                        anchors.fill: parent
                        anchors.margins: 5
                        spacing: 1
                        Text {
                            width: parent.width
                            text: modelData.event.summary || "(无标题)"
                            color: "#ffffff"
                            font.family: root.shellRoot ? root.shellRoot.baseFont : "sans-serif"
                            font.pixelSize: 10
                            font.weight: Font.Bold
                            elide: Text.ElideRight
                        }
                        Text {
                            width: parent.width
                            visible: parent.parent.height >= 34
                            text: Qt.formatTime(new Date(modelData.event.start), "HH:mm")
                            color: "#f7f7fb"
                            opacity: 0.84
                            font.family: root.shellRoot ? root.shellRoot.baseFont : "sans-serif"
                            font.pixelSize: 9
                        }
                    }
                    MouseArea {
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onEntered: root.showEventTooltip(modelData.event, parent)
                        onExited: if (root.hoveredEvent === modelData.event) root.hoveredEvent = null
                        onClicked: if (root.bridge) root.bridge.openEvent(modelData.event)
                    }
                }
            }

            Rectangle {
                visible: root.bridge && root.dayDiff(root.currentTime, root.bufferStartDate()) >= 0
                    && root.dayDiff(root.currentTime, root.bufferStartDate()) < root.bufferDayCount
                x: root.dayDiff(root.currentTime, root.bufferStartDate()) * root.dayWidth
                y: (root.currentTime.getHours() * 60 + root.currentTime.getMinutes() + root.currentTime.getSeconds() / 60) / 60 * root.hourHeight
                width: root.dayWidth
                height: 2
                color: root.accentColor
                z: 20
                Rectangle {
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    width: 7
                    height: 7
                    radius: 4
                    color: root.accentColor
                }
            }
        }

        Rectangle {
            anchors.right: parent.right
            anchors.rightMargin: 4
            y: timeline.visibleArea.yPosition * timeline.height
            width: 3
            height: Math.max(28, timeline.visibleArea.heightRatio * timeline.height)
            radius: 2
            color: root.mutedColor
            opacity: timeline.moving ? 0.65 : 0.25
        }
    }

    Item {
        x: 0
        y: timeline.y
        width: root.timeColumnWidth
        height: timeline.height
        clip: true

        Item {
            y: -timeline.contentY
            width: parent.width
            height: timeline.contentHeight

            Repeater {
                model: 25
                delegate: Text {
                    required property int index
                    x: 7
                    y: index * root.hourHeight - 7
                    width: root.timeColumnWidth - 14
                    text: index < 24 ? (index < 10 ? "0" : "") + index + ":00" : ""
                    color: root.mutedColor
                    font.family: root.shellRoot ? root.shellRoot.baseFont : "sans-serif"
                    font.pixelSize: 9
                    horizontalAlignment: Text.AlignRight
                }
            }
        }
    }

    Rectangle {
        anchors.centerIn: parent
        width: statusText.implicitWidth + 28
        height: 34
        radius: 17
        color: root.elevatedColor
        visible: root.bridge && (!root.bridge.connected || root.bridge.lastError.length > 0)
        Text {
            id: statusText
            anchors.centerIn: parent
            text: root.bridge ? (root.bridge.lastError || "正在连接日历…") : ""
            color: root.textColor
            font.family: root.shellRoot ? root.shellRoot.baseFont : "sans-serif"
            font.pixelSize: 12
        }
    }

    Rectangle {
        id: eventTooltip

        readonly property bool placeAbove: root.tooltipBelowY + implicitHeight > root.height - 8
        z: 100
        width: 270
        implicitHeight: tooltipContent.implicitHeight + 22
        x: Math.max(8, Math.min(root.width - width - 8, root.tooltipAnchorX - width / 2))
        y: placeAbove ? Math.max(8, root.tooltipAboveY - height) : root.tooltipBelowY
        radius: 12
        color: root.shellRoot ? root.shellRoot.moduleBackground : "#24242f"
        border.width: 1
        border.color: root.lineColor
        visible: root.hoveredEvent !== null

        Column {
            id: tooltipContent
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 11
            spacing: 4

            Text {
                width: parent.width
                text: root.hoveredEvent ? (root.hoveredEvent.summary || "(无标题)") : ""
                color: root.textColor
                font.family: root.shellRoot ? root.shellRoot.baseFont : "sans-serif"
                font.pixelSize: 13
                font.weight: Font.Bold
                elide: Text.ElideRight
            }
            Text {
                width: parent.width
                text: root.eventTimeText(root.hoveredEvent)
                color: root.mutedColor
                font.family: root.shellRoot ? root.shellRoot.baseFont : "sans-serif"
                font.pixelSize: 11
            }
            Text {
                width: parent.width
                visible: text.length > 0
                text: root.hoveredEvent ? (root.hoveredEvent.location || root.eventCalendarText(root.hoveredEvent)) : ""
                color: root.mutedColor
                font.family: root.shellRoot ? root.shellRoot.baseFont : "sans-serif"
                font.pixelSize: 11
                elide: Text.ElideRight
            }
            Text {
                width: parent.width
                visible: text.length > 0
                text: root.hoveredEvent ? (root.hoveredEvent.description || "") : ""
                color: root.mutedColor
                font.family: root.shellRoot ? root.shellRoot.baseFont : "sans-serif"
                font.pixelSize: 11
                wrapMode: Text.Wrap
                maximumLineCount: 3
                elide: Text.ElideRight
            }
        }
    }
}
