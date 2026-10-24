pragma Singleton
import QtQuick
import ParityHost

// services.mpris.players: [{identity, playbackState, ...}]. Desktop fixtures pass
// plain data (only the count and plain properties are read; details come from
// media-status.sh). Entries with "dbusName" (laptop BarServices reads players
// directly and connects to positionChanged) become QtObject players.
QtObject {
    id: mpris
    readonly property var _raw: ParityState.service("mpris.players", [])
    property var _objects: []
    readonly property var players: ({ values: _objects })

    property Component playerComponent: Component {
        QtObject {
            property string identity: ""
            property string dbusName: ""
            property bool isPlaying: false
            property int playbackState: 0
            property string trackTitle: ""
            property string trackArtist: ""
            property string trackArtUrl: ""
            property string trackAlbum: ""
            property real length: 0
            property bool lengthSupported: false
            property real position: 0
            property bool positionSupported: false
            property bool canSeek: true
            property bool canGoNext: true
            property bool canGoPrevious: true
            property bool canTogglePlaying: true
        }
    }

    function _rebuild() {
        for (const p of _objects)
            if (p && typeof p.destroy === "function") p.destroy();
        _objects = (_raw || []).map(entry => entry && entry.dbusName !== undefined
            ? playerComponent.createObject(mpris, entry) : entry);
    }
    on_RawChanged: _rebuild()
    Component.onCompleted: _rebuild()
}
