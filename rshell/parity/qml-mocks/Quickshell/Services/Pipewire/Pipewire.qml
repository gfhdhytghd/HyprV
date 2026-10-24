pragma Singleton
import QtQuick
import ParityHost

// services.audio: { available: true, volume: 0.35, muted: false }
QtObject {
    readonly property bool ready: ParityState.service("audio.available", true)
    readonly property QtObject defaultAudioSink: ready ? sink : null
    readonly property QtObject defaultAudioSource: null

    property QtObject sink: QtObject {
        readonly property bool ready: true
        readonly property string name: "parity-sink"
        readonly property string description: "Parity Speakers"
        readonly property QtObject audio: QtObject {
            readonly property real volume: ParityState.service("audio.volume", 0.35)
            readonly property bool muted: ParityState.service("audio.muted", false)
            readonly property var volumes: [volume, volume]
        }
    }
}
