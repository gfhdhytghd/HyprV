import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Networking
import Quickshell.Services.Mpris
import Quickshell.Services.Pipewire
import Quickshell.Services.UPower

// Shared subscriptions for every screen. Hidden bar items do not own pollers.
Item {
    id: services
    required property var shellRoot
    property bool mediaVisible: false
    property bool wifiPageVisible: false
    readonly property bool nativeWifi: Networking.backend === NetworkBackendType.NetworkManager
    readonly property var wifiDevice: {
        const devices = Networking.devices.values;
        return devices.find(device => device.type === DeviceType.Wifi && device.connected)
            || devices.find(device => device.type === DeviceType.Wifi) || null;
    }
    readonly property var wifiSnapshot: {
        const device = wifiDevice;
        const networks = device ? device.networks.values.map(network => ({
            active: network.connected,
            ssid: network.name,
            signal: Math.round(network.signalStrength * 100),
            security: network.security === WifiSecurityType.Open ? "" : WifiSecurityType.toString(network.security),
            secure: network.security !== WifiSecurityType.Open,
            known: network.known,
            enterprise: [WifiSecurityType.Wpa2Eap, WifiSecurityType.WpaEap,
                WifiSecurityType.Wpa3SuiteB192, WifiSecurityType.DynamicWep,
                WifiSecurityType.Leap].indexOf(network.security) >= 0,
            bars: ""
        })).sort((a, b) => Number(b.active) - Number(a.active) || b.signal - a.signal) : [];
        const active = networks.find(network => network.active);
        return {
            present: device !== null, iface: device ? device.name : "",
            enabled: Networking.wifiEnabled, hardwareEnabled: Networking.wifiHardwareEnabled,
            connected: !!active, ssid: active ? active.ssid : "",
            signal: active ? active.signal : 0, security: active ? active.security : "",
            networks: networks
        };
    }
    onWifiSnapshotChanged: syncWifi()
    function syncWifi() {
        if (nativeWifi) {
            shellRoot.applyWifiStatus(wifiSnapshot);
        }
    }
    Binding {
        target: services.wifiDevice
        property: "scannerEnabled"
        value: services.wifiPageVisible
        when: services.wifiDevice !== null
    }

    readonly property var audioNode: Pipewire.defaultAudioSink
    readonly property bool audioAvailable: Pipewire.ready && !!audioNode && audioNode.ready && !!audioNode.audio
    readonly property bool audioMuted: audioAvailable && audioNode.audio.muted
    readonly property int audioVolume: audioAvailable ? Math.round(audioNode.audio.volume * 100) : 0
    PwObjectTracker { objects: [services.audioNode] }
    onAudioAvailableChanged: syncAudio()
    onAudioMutedChanged: syncAudio()
    onAudioVolumeChanged: syncAudio()
    function syncAudio() {
        shellRoot.audioAvailable = audioAvailable;
        shellRoot.audioMuted = audioMuted;
        shellRoot.audioVolumePercent = Math.max(0, audioVolume);
    }

    readonly property string powerProfile: PowerProfiles.profile === PowerProfile.Performance
        ? "performance" : PowerProfiles.profile === PowerProfile.PowerSaver ? "power-saver" : "balanced"
    onPowerProfileChanged: syncPowerProfile()
    function syncPowerProfile() {
        shellRoot.powerProfile = powerProfile;
        shellRoot.powerProfileText = powerProfile === "performance" ? "⚡" : powerProfile === "power-saver" ? "🔋" : "⚖️";
    }

    readonly property var battery: UPower.displayDevice
    readonly property bool batteryPresent: !!battery && battery.ready && battery.isPresent && battery.isLaptopBattery
    readonly property real batteryPercent: batteryPresent ? battery.percentage * 100 : 0
    readonly property string batteryMode: !batteryPresent ? "unknown"
        : battery.state === UPowerDeviceState.Charging || battery.state === UPowerDeviceState.PendingCharge ? "charging"
        : battery.state === UPowerDeviceState.Discharging || battery.state === UPowerDeviceState.PendingDischarge ? "discharging"
        : battery.state === UPowerDeviceState.FullyCharged ? "full" : UPower.onBattery ? "idle" : "plugged"

    readonly property var player: {
        const players = Mpris.players.values;
        let pausedContent = null;
        let content = null;
        let paused = null;
        for (const candidate of players) {
            if (candidate.isPlaying) return candidate;
            const hasContent = candidate.trackTitle.length > 0 || candidate.trackArtist.length > 0 || candidate.trackArtUrl.length > 0;
            if (hasContent && !content) content = candidate;
            if (candidate.playbackState === MprisPlaybackState.Paused) {
                if (!paused) paused = candidate;
                if (hasContent && !pausedContent) pausedContent = candidate;
            }
        }
        return pausedContent || content || paused || players[0] || null;
    }
    readonly property string playerName: player ? player.dbusName.replace(/^org\.mpris\.MediaPlayer2\./, "") : ""
    // Metadata is event driven. Position is read from Quickshell's local clock.
    readonly property string mediaSnapshot: JSON.stringify(player ? {
        name: playerName, title: player.trackTitle, artist: player.trackArtist,
        art: player.trackArtUrl, album: player.trackAlbum, length: player.lengthSupported ? player.length : 0,
        playing: player.isPlaying, identity: player.identity
    } : null)
    onMediaSnapshotChanged: {
        syncMedia();
        compatibilityRefresh.restart();
    }
    property string correctedMediaSnapshot: ""
    property string correctedMediaOutput: ""
    function syncMedia() {
        if (!player) {
            shellRoot.resetMediaState();
            ciderPlayer = "";
            correctedMediaSnapshot = "";
            correctedMediaOutput = "";
            return;
        }
        if (correctedMediaSnapshot === mediaSnapshot) {
            shellRoot.updateMediaState(correctedMediaOutput);
            syncPosition();
            return;
        }
        const snapshot = JSON.parse(mediaSnapshot);
        shellRoot.updateMediaState([
            "available=true", "playing=" + snapshot.playing, "player=" + snapshot.name,
            "title=" + snapshot.title, "artist=" + snapshot.artist,
            "art_url=" + snapshot.art, "length=" + snapshot.length,
            "position=" + (player.positionSupported ? player.position : 0)
        ].join("\n"));
    }
    function syncPosition() {
        if (player && player.positionSupported) shellRoot.mediaPositionSeconds = Math.max(0, player.position);
    }
    Connections {
        target: services.player
        function onPositionChanged() { services.syncPosition(); }
    }
    Timer {
        interval: 1000
        repeat: true
        running: services.mediaVisible && !!services.player && services.player.isPlaying
        onTriggered: services.syncPosition()
    }
    onMediaVisibleChanged: if (mediaVisible) {
        syncPosition();
        compatibilityRefresh.restart();
    }

    // Cider sometimes publishes incomplete Chromium metadata. Retain the existing
    // repair on metadata events, with a slow retry only for a confirmed Cider.
    property string ciderPlayer: ""
    property string compatibilityKey: ""
    property string compatibilityPlayer: ""
    property bool compatibilityQueued: false
    function refreshCompatibility() {
        if (!player || !/chromium|cider/i.test(playerName + " " + player.identity)) return;
        if (compatibilityProcess.running) {
            compatibilityQueued = true;
            return;
        }
        compatibilityKey = mediaSnapshot;
        compatibilityPlayer = playerName;
        compatibilityProcess.command = [shellRoot.configDir + "/quickshell/scripts/media-status.sh", "--player", playerName];
        compatibilityProcess.running = true;
    }
    Timer {
        id: compatibilityRefresh
        interval: 200
        onTriggered: services.refreshCompatibility()
    }
    Timer {
        interval: 30000
        repeat: true
        running: services.mediaVisible && services.playerName === services.ciderPlayer && services.ciderPlayer.length > 0
            && !!services.player && services.player.isPlaying
        onTriggered: services.refreshCompatibility()
    }
    Process {
        id: compatibilityProcess
        stdout: StdioCollector { id: compatibilityOutput }
        stderr: StdioCollector {}
        onExited: function(exitCode) {
            const output = compatibilityOutput.text || "";
            if (exitCode === 0 && services.compatibilityKey === services.mediaSnapshot) {
                if (output.indexOf("cider=true\n") >= 0) {
                    services.ciderPlayer = services.compatibilityPlayer;
                    services.correctedMediaSnapshot = services.compatibilityKey;
                    services.correctedMediaOutput = output;
                    services.shellRoot.updateMediaState(output);
                    services.syncPosition();
                }
            }
            if (services.compatibilityQueued) {
                services.compatibilityQueued = false;
                compatibilityRefresh.restart();
            }
        }
    }

    FileView {
        id: themeState
        path: (Quickshell.env("XDG_STATE_HOME") || services.shellRoot.homeDir + "/.local/state") + "/hyprv/quickshell/ui-state.env"
        watchChanges: true
        onFileChanged: reload()
        onLoaded: services.shellRoot.darkMode = /^theme=dark$/m.test(text())
    }
    function refreshTheme() { themeState.reload(); }

    Component.onCompleted: {
        syncAudio();
        syncPowerProfile();
        syncMedia();
        syncWifi();
    }
}
