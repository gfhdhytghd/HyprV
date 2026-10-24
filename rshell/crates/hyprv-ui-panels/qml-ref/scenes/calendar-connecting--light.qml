import QtQuick
import ".."
import "../../../../../quickshell"

Item {
    width: 520; height: 500
    ShellMock { id: sh; darkMode: false; now: new Date(2026, 9, 23, 14, 30, 0) }
    QtObject {
        id: bridge
        property var weekStart: sh.now
        Component.onCompleted: Qt.callLater(() => bridge.weekStart = new Date(2026, 9, 19))
        property bool connected: false
        property string lastError: ""
        property var calendars: [{"color": "#e66100", "hidden": false, "holdsTasks": false, "id": "work", "name": "Work"}, {"color": null, "hidden": false, "holdsTasks": false, "id": "home", "name": "Home"}]
        property var events: []
        function previousWeek() {}
        function nextWeek() {}
        function openEvent(e) {}
        function createEvent() {}
    }
    WeekCalendarPanel { anchors.fill: parent; shellRoot: sh; bridge: bridge; embeddedSurface: true }
}
