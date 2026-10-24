import QtQuick

// Input-region description; recorded only (the harness does not mask input).
QtObject {
    property real x: 0
    property real y: 0
    property real width: 0
    property real height: 0
    property Item item: null
    property int intersection: 0
    default property list<QtObject> regions
}
