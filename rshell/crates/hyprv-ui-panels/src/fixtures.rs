//! Deterministic sample models for offscreen tests, examples and parity scenes
//! (mirroring the ui-panels.md §5 screenshot list).
use crate::model::*;

use std::{path::Path, rc::Rc};

/// Load the SVG assets the host would supply (read-only from the QML tree and the
/// Fluent icon theme) — test/example helper only; the UI never touches the filesystem.
pub fn load_assets(quickshell: &Path, fluent_base: &Path) -> Assets {
    let mut a = Assets::default();
    let fm = hyprv_ui::skia::FontMgr::new();
    let mut add = |key: String, path: std::path::PathBuf| {
        if let Ok(bytes) = std::fs::read(&path) {
            if let Some(dom) = crate::assets::load_svg(&bytes, fm.clone()) {
                a.svgs.insert(key, dom);
            }
        }
    };
    add("assets/control-panel/light-dark.svg".into(), quickshell.join("assets/control-panel/light-dark.svg"));
    add("assets/tray/network-wired-light.svg".into(), quickshell.join("assets/tray/network-wired-light.svg"));
    for b in ["0", "25", "50", "75", "100"] {
        for sec in ["", "-secure"] {
            let n = format!("nm-signal-{b}{sec}");
            add(format!("24/panel/{n}.svg"), fluent_base.join("Fluent-light/24/panel").join(format!("{n}.svg")));
            add(format!("symbolic/status/{n}-symbolic.svg"), fluent_base.join("Fluent-dark/symbolic/status").join(format!("{n}-symbolic.svg")));
        }
    }
    for n in ["network-wireless-hardware-disabled", "network-wireless-disabled", "network-wireless-disconnected", "network-wired", "network-wired-disconnected"] {
        add(format!("symbolic/status/{n}-symbolic.svg"), fluent_base.join("Fluent-dark/symbolic/status").join(format!("{n}-symbolic.svg")));
    }
    for n in ["network-wireless-offline", "network-wireless-off", "network-wireless-disconnected"] {
        add(format!("24/panel/{n}.svg"), fluent_base.join("Fluent-light/24/panel").join(format!("{n}.svg")));
    }
    a
}

pub fn default_assets() -> Rc<Assets> {
    let home = std::env::var("HOME").unwrap_or_default();
    Rc::new(load_assets(&Path::new(&home).join(".config/HyprV/quickshell"), &Path::new(&home).join(".local/share/icons")))
}

/// ui-panels.md §5 item 1: everything on.
pub fn control_idle(dark: bool) -> Model {
    let mut m = Model::default();
    m.theme.dark = dark;
    m.assets = default_assets();
    m.network = Network {
        default_interface: "wlan0".into(),
        active_type: NetType::Wifi,
        wifi_device_present: true,
        wifi_capability_detected: true,
        wifi_radio_enabled: true,
        wifi_hardware_enabled: true,
        wifi_connected: true,
        wifi_ssid: "HomeNet-5G".into(),
        wifi_signal_strength: 0.78,
        wifi_secure: true,
        wifi_interface: "wlan0".into(),
        networks: vec![
            WifiNetwork { ssid: "HomeNet-5G".into(), signal: 78, security: "WPA2".into(), secure: true, known: true, active: true, enterprise: false },
            WifiNetwork { ssid: "Office".into(), signal: 61, security: "WPA2".into(), secure: true, known: true, active: false, enterprise: false },
            WifiNetwork { ssid: "Cafe Free WiFi".into(), signal: 44, security: String::new(), secure: false, known: false, active: false, enterprise: false },
            WifiNetwork { ssid: "Neighbour".into(), signal: 23, security: "WPA3".into(), secure: true, known: false, active: false, enterprise: false },
        ],
        action_busy: false,
        action_message: String::new(),
    };
    m.bluetooth = Bluetooth {
        present: true,
        enabled: true,
        discovering: false,
        pairable: true,
        busy: false,
        action_message: String::new(),
        devices: vec![BtDevice { address: "AC:80:0A:12:34:56".into(), name: "WH-1000XM5".into(), display_name: "WH-1000XM5".into(), has_name: true, icon: "audio-headset".into(), paired: true, trusted: true, connected: true, blocked: false, rssi: None }],
    };
    m.media = Media { available: true, playing: true, title: "Midnight City".into(), artist: "M83".into(), player_name: "spotify".into(), art_url: String::new(), art: None };
    m.audio = Audio { available: true, muted: false, volume_percent: 35 };
    m.controls = Controls {
        brightness_percent: 50,
        fan_available: true,
        chassis_fan_percent: 40,
        pump_fan_percent: 60,
        chassis_fan_rpm: 1200,
        pump_fan_rpm: 2100,
        ..Default::default()
    };
    m
}

/// ui-panels.md §5 item 2: everything off.
pub fn control_off(dark: bool) -> Model {
    let mut m = control_idle(dark);
    m.network.wifi_radio_enabled = false;
    m.network.wifi_connected = false;
    m.network.default_interface = String::new();
    m.network.active_type = NetType::Offline;
    m.bluetooth.enabled = false;
    m.bluetooth.devices.clear();
    m.media = Media::default();
    m.audio = Audio { available: true, muted: true, volume_percent: 35 };
    m.controls.fan_available = false;
    m.controls.screen_recording = true;
    m.controls.prevent_sleep = true;
    m
}

/// Bluetooth page: on with 3 devices (connected, paired, unnamed) and a message.
pub fn bt_devices(dark: bool) -> Model {
    let mut m = control_idle(dark);
    m.bluetooth.action_message = "Connected to WH-1000XM5".into();
    m.bluetooth.devices = vec![
        BtDevice { address: "AC:80:0A:12:34:56".into(), name: "WH-1000XM5".into(), display_name: "WH-1000XM5".into(), has_name: true, icon: "audio-headset".into(), paired: true, trusted: true, connected: true, blocked: false, rssi: None },
        BtDevice { address: "F4:73:35:AA:BB:CC".into(), name: "MX Master 3S".into(), display_name: "MX Master 3S".into(), has_name: true, icon: "input-mouse".into(), paired: true, trusted: false, connected: false, blocked: false, rssi: Some(-61) },
        BtDevice { address: "5C:E9:1E:01:02:03".into(), name: String::new(), display_name: "5C:E9:1E:01:02:03".into(), has_name: false, icon: String::new(), paired: false, trusted: false, connected: false, blocked: false, rssi: Some(-80) },
    ];
    m
}
/// Bluetooth page: adapter off.
pub fn bt_off(dark: bool) -> Model {
    let mut m = control_idle(dark);
    m.bluetooth.enabled = false;
    m.bluetooth.devices.clear();
    m
}
/// Bluetooth page: on, scanning, empty list.
pub fn bt_scanning(dark: bool) -> Model {
    let mut m = control_idle(dark);
    m.bluetooth.discovering = true;
    m.bluetooth.devices.clear();
    m
}

pub type Fixture = fn(bool) -> Model;
pub fn all() -> Vec<(&'static str, Fixture)> {
    vec![
        ("control-idle", control_idle),
        ("control-off", control_off),
        ("bt-devices", bt_devices),
        ("bt-off", bt_off),
        ("bt-scanning", bt_scanning),
        ("wifi-connected", wifi_connected),
        ("wifi-ethernet", wifi_ethernet),
        ("wifi-off", wifi_off),
        ("wifi-blocked", wifi_blocked),
        ("wifi-many", wifi_many),
        ("stats-full", stats_full),
        ("stats-one", stats_one),
        ("stats-empty", stats_empty),
        ("calendar-week", calendar_week),
        ("calendar-connecting", calendar_connecting),
        ("quick-brightness", control_idle),
        ("quick-volume", control_idle),
        ("startup", control_idle),
    ]
}
/// Which control-panel page a fixture shows.
pub fn page_of(name: &str) -> &'static str {
    for (p, k) in [("bt-", "bluetooth"), ("wifi-", "wifi"), ("stats-", "stats"), ("calendar-", "calendar"), ("quick-brightness", "quick-brightness"), ("quick-volume", "quick-volume"), ("startup", "startup")] {
        if name.starts_with(p) {
            return k;
        }
    }
    "main"
}

// ---------------------------------------------------------------- Wi-Fi (ui-panels.md §5 items 10-13)

/// Item 10: connected Wi-Fi, 4 networks (active, saved, open, secure-unknown).
pub fn wifi_connected(dark: bool) -> Model {
    control_idle(dark)
}
/// Item 12: Ethernet connected, no wireless device.
pub fn wifi_ethernet(dark: bool) -> Model {
    let mut m = control_idle(dark);
    m.network = Network { default_interface: "enp5s0".into(), active_type: NetType::Wired, wifi_hardware_enabled: true, ..Default::default() };
    m
}
/// Item 12: Wi-Fi off with a pending action message.
pub fn wifi_off(dark: bool) -> Model {
    let mut m = control_idle(dark);
    m.network.wifi_radio_enabled = false;
    m.network.wifi_connected = false;
    m.network.default_interface = String::new();
    m.network.active_type = NetType::Offline;
    m.network.networks.clear();
    m.network.action_message = "Turning Wi-Fi on...".into();
    m
}
/// Item 12: hardware blocked.
pub fn wifi_blocked(dark: bool) -> Model {
    let mut m = wifi_off(dark);
    m.network.wifi_hardware_enabled = false;
    m.network.action_message.clear();
    m
}
/// Item 13: more than 5 networks.
pub fn wifi_many(dark: bool) -> Model {
    let mut m = control_idle(dark);
    for (i, s) in ["Upstairs", "Garage", "Printer-Direct", "Guest", "IoT"].iter().enumerate() {
        m.network.networks.push(WifiNetwork { ssid: (*s).into(), signal: 50 - i as u8 * 8, security: if i % 2 == 0 { "WPA2".into() } else { String::new() }, secure: i % 2 == 0, known: false, active: false, enterprise: false });
    }
    m
}

// ---------------------------------------------------------------- stats (item 15)

fn wave(n: usize, base: f64, amp: f64, period: f64) -> Vec<f64> {
    (0..n).map(|i| (base + amp * (i as f64 / period).sin() + amp * 0.4 * (i as f64 / (period * 0.37)).cos()).max(0.)).collect()
}
pub fn stats_full(dark: bool) -> Model {
    let mut m = Model { theme: Theme { dark }, ..Default::default() };
    m.stats = Stats {
        cpu_usage: 45.2,
        memory_usage: 70.4,
        cpu_core_usages: vec![12., 48.6, 7.4, 91.2, 33.3, 25., 60.5, 3.],
        default_interface: "wlan0".into(),
        rx_rate: 1536. * 1024.,
        tx_rate: 88. * 1024.,
        cpu_history: wave(120, 45., 18., 7.),
        memory_history: wave(120, 70., 3., 20.),
        network_history: (0..120).map(|i| if i % 17 < 4 { 2.4e6 * (1. + (i % 5) as f64) } else { 3e4 * (i % 7) as f64 }).collect(),
    };
    m
}
pub fn stats_one(dark: bool) -> Model {
    let mut m = stats_full(dark);
    m.stats.cpu_history.truncate(1);
    m.stats.memory_history.truncate(1);
    m.stats.network_history.truncate(1);
    m.stats.cpu_core_usages.truncate(4);
    m
}
pub fn stats_empty(dark: bool) -> Model {
    let mut m = stats_full(dark);
    m.stats.cpu_history.clear();
    m.stats.memory_history.clear();
    m.stats.network_history.clear();
    m.stats.default_interface.clear();
    m.stats.rx_rate = 0.;
    m.stats.tx_rate = 0.;
    m
}

// ---------------------------------------------------------------- calendar (items 17-19)

/// Item 17: current week, 2 all-day events (one spanning 3 days), overlapping timed events.
pub fn calendar_week(dark: bool) -> Model {
    let mut m = Model { theme: Theme { dark }, ..Default::default() };
    // Friday 2026-10-23 14:30 → week of Monday 2026-10-19
    m.calendar = Calendar {
        bridge: true,
        connected: true,
        last_error: String::new(),
        week_start: "2026-10-19".into(),
        calendars: vec![
            CalendarInfo { id: "work".into(), name: "Work".into(), color: Some("#e66100".into()), hidden: false, holds_tasks: false },
            CalendarInfo { id: "home".into(), name: "Home".into(), color: None, hidden: false, holds_tasks: false },
        ],
        events: vec![
            CalendarEvent { uid: "a".into(), calendar_id: "home".into(), summary: "Trip to Kyoto".into(), start: "2026-10-21".into(), end: "2026-10-24".into(), all_day: true, location: "Kyoto".into(), description: String::new() },
            CalendarEvent { uid: "b".into(), calendar_id: "work".into(), summary: "Release day".into(), start: "2026-10-22".into(), end: "2026-10-23".into(), all_day: true, location: String::new(), description: String::new() },
            CalendarEvent { uid: "c".into(), calendar_id: "work".into(), summary: "Standup".into(), start: "2026-10-23T09:30:00".into(), end: "2026-10-23T10:00:00".into(), all_day: false, location: "Room 4".into(), description: "Daily sync".into() },
            CalendarEvent { uid: "d".into(), calendar_id: "work".into(), summary: "Design review".into(), start: "2026-10-23T12:00:00".into(), end: "2026-10-23T14:00:00".into(), all_day: false, location: String::new(), description: "Review the new panel layouts and the parity report.".into() },
            CalendarEvent { uid: "e".into(), calendar_id: "home".into(), summary: "Lunch".into(), start: "2026-10-23T12:30:00".into(), end: "2026-10-23T13:30:00".into(), all_day: false, location: String::new(), description: String::new() },
        ],
    };
    m
}
/// Item 19: connecting.
pub fn calendar_connecting(dark: bool) -> Model {
    let mut m = calendar_week(dark);
    m.calendar.connected = false;
    m.calendar.events.clear();
    m
}
