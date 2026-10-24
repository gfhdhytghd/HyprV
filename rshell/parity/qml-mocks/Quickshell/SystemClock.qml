import QtQuick
import ParityHost

// The fixture fixes the wall clock (fixture "now", ISO string).
QtObject {
    enum Precision { Hours, Minutes, Seconds }
    property int precision: SystemClock.Minutes
    property bool enabled: true
    readonly property date date: ParityState.now
    readonly property int hours: date.getHours()
    readonly property int minutes: date.getMinutes()
    readonly property int seconds: date.getSeconds()
}
