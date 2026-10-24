#![recursion_limit = "512"]
//! Export fixture models as JSON for qml-ref/gen.py (keeps QML reference scenes in sync).
use hyprv_ui_panels::{fixtures, model::*};
use serde_json::{json, Value};

fn model_json(m: &Model) -> Value {
    json!({
        "dark": m.theme.dark,
        "wifiRadioEnabled": m.network.wifi_radio_enabled,
        "wifiConnected": m.network.wifi_connected,
        "wifiSsid": m.network.wifi_ssid,
        "bluetoothPresent": m.bluetooth.present,
        "bluetoothEnabled": m.bluetooth.enabled,
        "bluetoothDiscovering": m.bluetooth.discovering,
        "bluetoothPairable": m.bluetooth.pairable,
        "bluetoothActionBusy": m.bluetooth.busy,
        "bluetoothActionMessage": m.bluetooth.action_message,
        "bluetoothShowUnnamedDevices": m.bluetooth_show_unnamed,
        "bluetoothDevices": m.bluetooth.devices.iter().map(|d| json!({
            "address": d.address, "name": d.name, "displayName": d.display_name, "hasName": d.has_name,
            "icon": d.icon, "paired": d.paired, "trusted": d.trusted, "connected": d.connected,
            "blocked": d.blocked, "rssi": d.rssi })).collect::<Vec<_>>(),
        "screenRecording": m.controls.screen_recording,
        "preventSleepEnabled": m.controls.prevent_sleep,
        "mediaAvailable": m.media.available,
        "mediaPlaying": m.media.playing,
        "mediaTitle": m.media.title,
        "mediaArtist": m.media.artist,
        "dndEnabled": m.controls.dnd,
        "demoModeEnabled": m.controls.demo_mode,
        "powerProfile": match m.controls.profile { Profile::PowerSaver => "power-saver", Profile::Balanced => "balanced", Profile::Performance => "performance" },
        "brightnessPercent": m.controls.brightness_percent,
        "audioAvailable": m.audio.available,
        "audioMuted": m.audio.muted,
        "audioVolumePercent": m.audio.volume_percent,
        "fanControlAvailable": m.controls.fan_available,
        "chassisFanPercent": m.controls.chassis_fan_percent,
        "pumpFanPercent": m.controls.pump_fan_percent,
        "vmRunning": m.controls.vm_running,
        "vmBusy": m.controls.vm_busy,
        "viewflowTarget": if m.controls.viewflow_target == Target::MacOs { "macos" } else { "windows" },
        "viewflowBusy": m.controls.viewflow_busy,
        "net": {
            "defaultInterface": m.network.default_interface,
            "activeType": format!("{:?}", m.network.active_type).to_lowercase(),
            "wifiDevicePresent": m.network.wifi_device_present,
            "wifiCapabilityDetected": m.network.wifi_capability_detected,
            "wifiHardwareEnabled": m.network.wifi_hardware_enabled,
            "wifiSignalStrength": m.network.wifi_signal_strength,
            "wifiSecure": m.network.wifi_secure,
            "wifiInterface": m.network.wifi_interface,
            "wifiActionBusy": m.network.action_busy,
            "wifiActionMessage": m.network.action_message,
            "networks": m.network.networks.iter().map(|n| json!({"ssid": n.ssid, "signal": n.signal, "security": n.security, "secure": n.secure, "known": n.known, "active": n.active, "enterprise": n.enterprise})).collect::<Vec<_>>(),
        },
        "stats": {
            "cpuUsage": m.stats.cpu_usage, "memoryUsage": m.stats.memory_usage,
            "cpuCoreUsages": m.stats.cpu_core_usages, "defaultInterface": m.stats.default_interface,
            "networkRxRate": m.stats.rx_rate, "networkTxRate": m.stats.tx_rate,
            "cpuHistory": m.stats.cpu_history, "memoryHistory": m.stats.memory_history, "networkHistory": m.stats.network_history,
        },
        "calendar": {
            "connected": m.calendar.connected, "lastError": m.calendar.last_error, "weekStart": m.calendar.week_start,
            "calendars": m.calendar.calendars.iter().map(|c| json!({"id": c.id, "name": c.name, "color": c.color, "hidden": c.hidden, "holdsTasks": c.holds_tasks})).collect::<Vec<_>>(),
            "events": m.calendar.events.iter().map(|e| json!({"uid": e.uid, "calendarId": e.calendar_id, "summary": e.summary, "start": e.start, "end": e.end, "allDay": e.all_day, "location": e.location, "description": e.description})).collect::<Vec<_>>(),
        },
        "wall": m.wall.format("%Y-%m-%dT%H:%M:%S").to_string(),
    })
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or("qml-ref/fixtures.json".into());
    let mut all = serde_json::Map::new();
    for (name, f) in fixtures::all() {
        for dark in [false, true] {
            let m = f(dark);
            all.insert(format!("{name}--{}", if dark { "dark" } else { "light" }), json!({ "page": fixtures::page_of(name), "model": model_json(&m) }));
        }
    }
    std::fs::write(out, serde_json::to_string_pretty(&Value::Object(all)).unwrap()).unwrap();
}
