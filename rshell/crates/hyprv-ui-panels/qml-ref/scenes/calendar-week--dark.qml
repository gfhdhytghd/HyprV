import QtQuick
import ".."
import "../../../../../quickshell"

Item {
    width: 520; height: 500
    ShellMock { id: sh; darkMode: true; now: new Date(2026, 9, 23, 14, 30, 0) }
    QtObject {
        id: bridge
        property var weekStart: sh.now
        Component.onCompleted: Qt.callLater(() => bridge.weekStart = new Date(2026, 9, 19))
        property bool connected: true
        property string lastError: ""
        property var calendars: [{"color": "#e66100", "hidden": false, "holdsTasks": false, "id": "work", "name": "Work"}, {"color": null, "hidden": false, "holdsTasks": false, "id": "home", "name": "Home"}]
        property var events: [{"allDay": true, "calendarId": "home", "description": "", "end": "2026-10-24", "location": "Kyoto", "start": "2026-10-21", "summary": "Trip to Kyoto", "uid": "a"}, {"allDay": true, "calendarId": "work", "description": "", "end": "2026-10-23", "location": "", "start": "2026-10-22", "summary": "Release day", "uid": "b"}, {"allDay": false, "calendarId": "work", "description": "Daily sync", "end": "2026-10-23T10:00:00", "location": "Room 4", "start": "2026-10-23T09:30:00", "summary": "Standup", "uid": "c"}, {"allDay": false, "calendarId": "work", "description": "Review the new panel layouts and the parity report.", "end": "2026-10-23T14:00:00", "location": "", "start": "2026-10-23T12:00:00", "summary": "Design review", "uid": "d"}, {"allDay": false, "calendarId": "home", "description": "", "end": "2026-10-23T13:30:00", "location": "", "start": "2026-10-23T12:30:00", "summary": "Lunch", "uid": "e"}]
        function previousWeek() {}
        function nextWeek() {}
        function openEvent(e) {}
        function createEvent() {}
    }
    WeekCalendarPanel { anchors.fill: parent; shellRoot: sh; bridge: bridge; embeddedSurface: true }
}
