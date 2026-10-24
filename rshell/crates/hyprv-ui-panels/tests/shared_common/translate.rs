//! Shared parity fixture (`parity/fixtures/panels/*.json`) → panels [`Model`].
//!
//! The fixtures drive the ORIGINAL `shell.qml` through mock services and mock helper
//! processes. This ports exactly the QML parsers those mocks feed, so the Rust panels see
//! the same values the QML panels saw:
//! - `desktop-state.py` stream: `system` → `updateSystemStats` (shell.qml:2314),
//!   `wifi` → `applyWifiStatus`/`resolveWifiNetworks` (shell.qml:2046, :2018);
//! - `media-status.sh` → `updateMediaState` (shell.qml:400) when MPRIS players exist;
//! - `control-panel-status.sh` → `updateControlPanelState` (shell.qml:1620);
//! - `swaync-client` → `updateNotificationState` (dnd from `alt`);
//! - VM / Viewflow / Mihomo status → `WindowsVmController.qml:16`,
//!   `ViewflowTargetControl.qml:14`, `MihomoControlPanel.qml:47`;
//! - mock services: audio (`Pipewire.qml` mock), Bluetooth (`sortedBluetoothSnapshots`),
//!   `powerProfile`;
//! - `dcal` sockets → calendar list/events (piece B).
use chrono::NaiveDateTime;
use hyprv_ui_panels::model::*;
use serde_json::Value;
use std::{path::Path, rc::Rc};

const HISTORY_LIMIT: usize = 120;

pub struct Translator {
    fixture: Value,
    assets_dir: std::path::PathBuf,
    /// (at ms, decoded line) sorted by time; `next` = first undelivered.
    stream: Vec<(f64, Value)>,
    next: usize,
    pub model: Model,
    // updateSystemStats state
    prev_cpu_total: i64,
    prev_cpu_idle: i64,
    prev_core_totals: Vec<i64>,
    prev_core_idles: Vec<i64>,
    prev_rx: i64,
    prev_tx: i64,
    prev_iface: String,
    prev_net_time: f64,
}

fn process<'a>(fixture: &'a Value, name: &str) -> Option<&'a Value> {
    fixture["processes"].as_array()?.iter().find(|p| p["match"].as_str() == Some(name))
}
fn stdout<'a>(fixture: &'a Value, name: &str) -> Option<&'a str> {
    process(fixture, name)?.get("stdout")?.as_str()
}

impl Translator {
    pub fn new(fixture: &Value, dark: bool, scale: f32, rshell: &Path, assets: Rc<Assets>) -> Self {
        let assets_dir = rshell.join("parity/fixtures/assets");
        let mut model = Model { assets, ..Default::default() };
        model.theme.dark = dark;
        let screen = &fixture["services"]["screens"][0];
        model.output = Output {
            name: screen["name"].as_str().unwrap_or("DP-4").into(),
            width: screen["width"].as_f64().unwrap_or(3072.) as f32,
            height: screen["height"].as_f64().unwrap_or(1728.) as f32,
            scale,
        };
        if let Some(now) = fixture["now"].as_str() {
            if let Ok(t) = NaiveDateTime::parse_from_str(now, "%Y-%m-%dT%H:%M:%S") {
                model.wall = t;
            }
        }
        let mut stream = Vec::new();
        if let Some(p) = process(fixture, "desktop-state.py") {
            for l in p["lines"].as_array().into_iter().flatten() {
                if let Ok(v) = serde_json::from_str::<Value>(l["data"].as_str().unwrap_or("")) {
                    stream.push((l["at"].as_f64().unwrap_or(0.), v));
                }
            }
        }
        stream.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let mut t = Self {
            fixture: fixture.clone(), assets_dir, stream, next: 0, model,
            prev_cpu_total: -1, prev_cpu_idle: -1, prev_core_totals: Vec::new(), prev_core_idles: Vec::new(),
            prev_rx: -1, prev_tx: -1, prev_iface: String::new(), prev_net_time: 0.,
        };
        t.apply_static();
        t
    }

    /// Values that do not change over the fixture: services and polled helper outputs.
    fn apply_static(&mut self) {
        let f = self.fixture.clone();
        let s = &f["services"];
        // Audio (Pipewire mock): available ⇒ ready sink; round(volume·100).
        let audio = &s["audio"];
        let available = audio.get("available").and_then(Value::as_bool).unwrap_or(true);
        let volume = audio.get("volume").and_then(Value::as_f64).unwrap_or(0.35);
        self.model.audio = Audio {
            available,
            muted: available && audio.get("muted").and_then(Value::as_bool).unwrap_or(false),
            volume_percent: if available { (volume * 100.).round().max(0.) as i32 } else { 0 },
        };
        // Bluetooth adapter mock + sortedBluetoothSnapshots.
        self.model.bluetooth = bluetooth(&s["bluetooth"]);
        // Power profile: native PowerProfiles first, then control-panel-status.
        self.model.controls.profile = profile(s["powerProfile"].as_str().unwrap_or("balanced"));
        // Laptop fixtures: native Networking devices (`services.wifi.devices`) instead of
        // the desktop-state stream.
        if let Some(dev) = s["wifi"]["devices"].as_array().and_then(|d| d.iter().find(|d| d["type"] == "wifi")) {
            let net = dev["networks"].as_array().and_then(|n| n.iter().find(|w| w["connected"] == true));
            let d = serde_json::json!({
                "present": true,
                "iface": dev["name"],
                "enabled": s["wifi"]["enabled"].as_bool().unwrap_or(true),
                "hardwareEnabled": s["wifi"]["hardwareEnabled"].as_bool().unwrap_or(true),
                "connected": dev["connected"].as_bool().unwrap_or(false) && net.is_some(),
                "ssid": net.map_or(Value::Null, |w| w["name"].clone()),
                "security": net.map_or(Value::Null, |w| w["security"].clone()),
                "signal": net.and_then(|w| w["signal"].as_f64()).map_or(0., |v| v * 100.),
            });
            self.wifi(&d);
        }
        if let Some(out) = stdout(&f, "control-panel-status.sh") {
            self.control_panel_state(out);
        }
        // brightness_get primes brightnessPercent on panel open (shell.qml quickAdjust probe).
        if let Some(b) = stdout(&f, "brightness_get").and_then(|v| v.trim().parse::<f64>().ok()) {
            self.model.controls.brightness_percent = b.round().clamp(0., 100.) as i32;
        }
        if let Some(out) = stdout(&f, "windows-vm-control.sh status") {
            self.model.controls.vm_running = out.trim() == "running";
        }
        if let Some(out) = stdout(&f, "viewflow-target.sh status") {
            match out.trim() {
                "macos" => self.model.controls.viewflow_target = Target::MacOs,
                "windows" => self.model.controls.viewflow_target = Target::Windows,
                _ => {}
            }
        }
        if let Some(out) = stdout(&f, "mihomo-control.sh status") {
            self.model.controls.mihomo = mihomo(out);
        }
        // swaync-client stream: alt containing "dnd".
        if let Some(p) = process(&f, "swaync-client") {
            for l in p["lines"].as_array().into_iter().flatten() {
                if let Ok(v) = serde_json::from_str::<Value>(l["data"].as_str().unwrap_or("")) {
                    if v["alt"].as_str().is_some_and(|a| a.contains("dnd")) {
                        self.model.controls.dnd = true;
                    }
                }
            }
        }
        // media-status.sh poll, only while MPRIS players exist (shell.qml:3315).
        let players = s["mpris"]["players"].as_array().map_or(0, Vec::len);
        if players > 0 {
            if let Some(p) = process(&f, "media-status.sh") {
                if p.get("exitCode").and_then(Value::as_i64).unwrap_or(0) == 0 {
                    self.media_state(p["stdout"].as_str().unwrap_or(""));
                }
            }
        }
        self.calendar_sockets();
    }

    /// Deliver stream lines up to virtual time `t_ms`; set the frame clock.
    pub fn advance_to(&mut self, t_ms: f64) {
        // Process.qml schedules even at=0 through a Timer with interval >=1ms.
        // Date.now() observes its actual animation-frame delivery time, not the
        // requested deadline (stats-120's first interval is 1000-50=950ms).
        let delivered_ms = t_ms.floor();
        while self.next < self.stream.len() && self.stream[self.next].0.max(1.) <= delivered_ms {
            let (_, v) = self.stream[self.next].clone();
            self.next += 1;
            match v["type"].as_str() {
                Some("system") => self.system(v["data"].as_str().unwrap_or(""), delivered_ms),
                Some("wifi") => self.wifi(&v["data"]),
                _ => {}
            }
        }
        self.model.now = std::time::Duration::from_millis(t_ms.floor() as u64);
    }

    fn control_panel_state(&mut self, out: &str) {
        let c = &mut self.model.controls;
        for line in out.lines().map(str::trim) {
            let num = |p: &str| line.strip_prefix(p).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite());
            let flag = |p: &str| line.strip_prefix(p).map(|v| v.trim() == "true");
            if let Some(v) = num("brightness=") { c.brightness_percent = v.round().clamp(0., 100.) as i32 }
            else if let Some(v) = flag("dnd=") { c.dnd = v }
            else if let Some(v) = flag("recording=") { c.screen_recording = v }
            else if let Some(p) = line.strip_prefix("power_profile=") { if !p.trim().is_empty() { c.profile = profile(p.trim()) } }
            else if let Some(v) = flag("prevent_sleep=") { c.prevent_sleep = v }
            else if let Some(v) = flag("demo_mode=") { c.demo_mode = v }
            else if let Some(v) = flag("fan_available=") { c.fan_available = v }
            else if let Some(v) = num("fan_chassis_percent=") { c.chassis_fan_percent = v.round().clamp(0., 100.) as i32 }
            else if let Some(v) = num("fan_chassis_rpm=") { c.chassis_fan_rpm = v.round().max(0.) as u32 }
            else if let Some(v) = num("fan_pump_percent=") { c.pump_fan_percent = v.round().clamp(0., 100.) as i32 }
            else if let Some(v) = num("fan_pump_rpm=") { c.pump_fan_rpm = v.round().max(0.) as u32 }
        }
    }

    /// `updateMediaState` (fresh state, so the retention rule never applies here).
    fn media_state(&mut self, out: &str) {
        let mut m = Media::default();
        for line in out.lines().map(str::trim) {
            if let Some(v) = line.strip_prefix("available=") { m.available = v.trim() == "true" }
            else if let Some(v) = line.strip_prefix("playing=") { m.playing = v.trim() == "true" }
            else if let Some(v) = line.strip_prefix("title=") { m.title = v.trim().into() }
            else if let Some(v) = line.strip_prefix("artist=") { m.artist = v.trim().into() }
            else if let Some(v) = line.strip_prefix("player=") { m.player_name = v.trim().into() }
            else if let Some(v) = line.strip_prefix("art_url=") { m.art_url = v.trim().into() }
        }
        if let Some(rest) = m.art_url.strip_prefix("file://@ASSETS@/") {
            let path = self.assets_dir.join(rest);
            m.art_url = format!("file://{}", path.display());
            m.art = std::fs::read(&path).ok().and_then(|b| hyprv_ui::skia::Image::from_encoded(hyprv_ui::skia::Data::new_copy(&b)));
        }
        self.model.media = m;
    }

    /// `applyWifiStatus` (+ `resolveWifiNetworks`, which only differs for empty lists).
    fn wifi(&mut self, d: &Value) {
        let n = &mut self.model.network;
        let present = d["present"].as_bool().unwrap_or(false);
        let iface = d["iface"].as_str().unwrap_or("").to_owned();
        n.wifi_device_present = false;
        n.wifi_radio_enabled = false;
        n.wifi_hardware_enabled = d.get("hardwareEnabled").and_then(Value::as_bool) != Some(false);
        n.wifi_connected = false;
        n.wifi_interface.clear();
        n.wifi_ssid.clear();
        n.wifi_secure = false;
        n.wifi_signal_strength = 0.;
        n.networks.clear();
        n.wifi_device_present = present;
        if present || !iface.is_empty() {
            n.wifi_capability_detected = true;
        }
        if !present {
            return;
        }
        n.wifi_radio_enabled = d["enabled"].as_bool().unwrap_or(false);
        n.wifi_connected = d["connected"].as_bool().unwrap_or(false);
        n.wifi_interface = iface;
        n.wifi_ssid = d["ssid"].as_str().unwrap_or("").into();
        n.wifi_secure = !d["security"].as_str().unwrap_or("").trim().is_empty();
        n.wifi_signal_strength = if n.wifi_connected { (d["signal"].as_f64().unwrap_or(0.) / 100.).clamp(0., 1.) as f32 } else { 0. };
        n.networks = d["networks"].as_array().into_iter().flatten().map(|w| WifiNetwork {
            ssid: w["ssid"].as_str().unwrap_or("").into(),
            signal: w["signal"].as_f64().unwrap_or(0.).clamp(0., 100.) as u8,
            security: w["security"].as_str().unwrap_or("").into(),
            secure: w["secure"].as_bool().unwrap_or(false),
            known: w["known"].as_bool().unwrap_or(false),
            active: w["active"].as_bool().unwrap_or(false),
            enterprise: w["enterprise"].as_bool().unwrap_or(false),
        }).collect();
    }

    /// `updateSystemStats` for one `system` snapshot delivered at `at_ms`.
    fn system(&mut self, text: &str, at_ms: f64) {
        let sections = split_sections(text);
        let st = &mut self.model.stats;
        let stat = sections.get("__STAT__").cloned().unwrap_or_default();
        if let Some(line) = stat.first().filter(|l| !l.is_empty()) {
            let (total, idle) = jiffies(line);
            if self.prev_cpu_total >= 0 && total > self.prev_cpu_total {
                let td = (total - self.prev_cpu_total) as f64;
                let id = (idle - self.prev_cpu_idle) as f64;
                st.cpu_usage = ((1. - id / td) * 100.).clamp(0., 100.);
            }
            self.prev_cpu_total = total;
            self.prev_cpu_idle = idle;
        }
        let (mut totals, mut idles, mut usages) = (Vec::new(), Vec::new(), Vec::new());
        for line in stat.iter().skip(1) {
            let is_core = line.starts_with("cpu") && line[3..].chars().next().is_some_and(|c| c.is_ascii_digit())
                && line.split_whitespace().next().is_some_and(|w| w[3..].chars().all(|c| c.is_ascii_digit()));
            if !is_core || !line.contains(char::is_whitespace) {
                continue;
            }
            let (total, idle) = jiffies(line);
            let i = totals.len();
            let mut usage = st.cpu_core_usages.get(i).copied().unwrap_or(0.).clamp(0., 100.);
            let pt = self.prev_core_totals.get(i).copied().unwrap_or(-1);
            let pi = self.prev_core_idles.get(i).copied().unwrap_or(-1);
            if pt >= 0 && total > pt {
                usage = ((1. - (idle - pi) as f64 / (total - pt) as f64) * 100.).clamp(0., 100.);
            }
            totals.push(total);
            idles.push(idle);
            usages.push(usage);
        }
        self.prev_core_totals = totals;
        self.prev_core_idles = idles;
        st.cpu_core_usages = usages;
        let mem = number_map(&sections.get("__MEM__").cloned().unwrap_or_default());
        let total = *mem.get("MemTotal").unwrap_or(&0) as f64;
        let avail = mem.get("MemAvailable").copied().filter(|v| *v != 0)
            .unwrap_or_else(|| mem.get("MemFree").copied().unwrap_or(0) + mem.get("Buffers").copied().unwrap_or(0) + mem.get("Cached").copied().unwrap_or(0)) as f64;
        if total > 0. {
            st.memory_usage = (total - avail) / total * 100.;
        }
        st.default_interface = default_interface(&sections.get("__ROUTE__").cloned().unwrap_or_default());
        let net = sections.get("__NET__");
        match net {
            None => {
                self.prev_rx = -1;
                self.prev_tx = -1;
                self.prev_net_time = 0.;
            }
            Some(lines) => {
                // Date.now(): the virtual clock (ms) in the parity host.
                let elapsed = ((at_ms - self.prev_net_time) / 1000.).max(0.001);
                let iface = st.default_interface.clone();
                match counters(lines, &iface).filter(|_| !iface.is_empty()) {
                    None => {
                        st.rx_rate = 0.;
                        st.tx_rate = 0.;
                        self.prev_rx = -1;
                        self.prev_tx = -1;
                        self.prev_iface.clear();
                    }
                    Some((rx, tx)) if self.prev_iface != iface => {
                        self.prev_iface = iface;
                        self.prev_rx = rx;
                        self.prev_tx = tx;
                        st.rx_rate = 0.;
                        st.tx_rate = 0.;
                    }
                    Some((rx, tx)) => {
                        if self.prev_rx >= 0 && self.prev_tx >= 0 {
                            st.rx_rate = (rx - self.prev_rx).max(0) as f64 / elapsed;
                            st.tx_rate = (tx - self.prev_tx).max(0) as f64 / elapsed;
                        }
                        self.prev_rx = rx;
                        self.prev_tx = tx;
                    }
                }
                self.prev_net_time = at_ms;
            }
        }
        let cpu = st.cpu_usage;
        let memu = st.memory_usage;
        push(&mut st.cpu_history, cpu);
        push(&mut st.memory_history, memu);
        if net.is_some() {
            let r = st.rx_rate + st.tx_rate;
            push(&mut st.network_history, r);
        }
        // shell.qml networkConnected / defaultInterface drive the control panel too.
        self.model.network.default_interface = st.default_interface.clone();
        self.model.network.active_type = net_type(&st.default_interface);
    }

    /// `dcal` socket mock → calendar list and events (CalendarBridge.qml).
    fn calendar_sockets(&mut self) {
        // shell.qml:3980 `CalendarBridge { weekStart: new Date() }` overrides the Monday
        // default; only loadWeek() (socket up) snaps it to Monday. Offline the week starts
        // today: an empty week_start makes the panel use the wall date.
        use chrono::Datelike;
        let today = self.model.wall.date();
        let monday = today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
        self.model.calendar.week_start.clear();
        let c = &mut self.model.calendar;
        let Some(sockets) = self.fixture.get("sockets").and_then(Value::as_object) else {
            c.connected = false;
            return;
        };
        c.week_start = monday.format("%Y-%m-%d").to_string();
        let Some((_, sock)) = sockets.iter().next() else { return };
        let m = &sock["methods"];
        c.connected = true;
        c.calendars = m["calendars.list"].as_array().into_iter().flatten().map(|k| CalendarInfo {
            id: k["id"].as_str().unwrap_or("").into(),
            name: k["summary"].as_str().or(k["name"].as_str()).unwrap_or("").into(),
            color: k["color"].as_str().map(str::to_owned),
            hidden: k["hidden"].as_bool().unwrap_or(false),
            holds_tasks: k["holdsTasks"].as_bool().unwrap_or(false),
        }).collect();
        let events = m["events.list"].get("events").or(Some(&m["events.list"])).and_then(Value::as_array);
        c.events = events.into_iter().flatten().map(|e| CalendarEvent {
            uid: e["uid"].as_str().or(e["id"].as_str()).unwrap_or("").into(),
            calendar_id: e["calendarId"].as_str().unwrap_or("").into(),
            summary: e["summary"].as_str().unwrap_or("").into(),
            start: e["start"].as_str().unwrap_or("").into(),
            end: e["end"].as_str().unwrap_or("").into(),
            all_day: e["allDay"].as_bool().unwrap_or(false),
            location: e["location"].as_str().unwrap_or("").into(),
            description: e["description"].as_str().unwrap_or("").into(),
        }).collect();
    }

}

fn push(h: &mut Vec<f64>, v: f64) {
    h.push(if v.is_finite() { v.max(0.) } else { 0. });
    if h.len() > HISTORY_LIMIT {
        let n = h.len() - HISTORY_LIMIT;
        h.drain(0..n);
    }
}

fn jiffies(line: &str) -> (i64, i64) {
    let v: Vec<i64> = line.split_whitespace().skip(1).map(|x| x.parse().unwrap_or(0)).collect();
    let idle = v.get(3).copied().unwrap_or(0) + v.get(4).copied().unwrap_or(0);
    (v.iter().sum(), idle)
}

pub fn split_sections(text: &str) -> std::collections::HashMap<String, Vec<String>> {
    let mut out: std::collections::HashMap<String, Vec<String>> = Default::default();
    let mut current = String::new();
    for line in text.split('\n') {
        if line.starts_with("__") && line.ends_with("__") && line.len() >= 4 {
            current = line.to_owned();
            out.insert(current.clone(), Vec::new());
        } else if !current.is_empty() {
            out.get_mut(&current).unwrap().push(line.to_owned());
        }
    }
    out
}

fn number_map(lines: &[String]) -> std::collections::HashMap<String, i64> {
    let mut m = std::collections::HashMap::new();
    for l in lines {
        if let Some((k, v)) = l.split_once(':') {
            if k.chars().all(|c| c.is_ascii_alphabetic() || "_()".contains(c)) {
                if let Some(n) = v.split_whitespace().next().and_then(|n| n.parse::<i64>().ok()) {
                    m.insert(k.to_owned(), n);
                }
            }
        }
    }
    m
}

fn default_interface(lines: &[String]) -> String {
    let text = lines.join("\n");
    for l in text.trim().split('\n').skip(1) {
        let p: Vec<&str> = l.split_whitespace().collect();
        if p.len() >= 8 && p[1] == "00000000" && p[7] == "00000000" {
            return p[0].into();
        }
    }
    String::new()
}

fn counters(lines: &[String], iface: &str) -> Option<(i64, i64)> {
    for l in lines.iter().skip(2) {
        let l = l.trim();
        if !l.starts_with(&format!("{iface}:")) {
            continue;
        }
        let replaced = l.replacen(':', " ", 1);
        let p: Vec<&str> = replaced.split_whitespace().collect();
        if p.len() < 10 {
            return None;
        }
        return Some((p[1].parse().unwrap_or(0), p[9].parse().unwrap_or(0)));
    }
    None
}

/// `isWifiInterfaceName` / `isWiredInterfaceName` (same rule as `services::net_type`).
fn net_type(iface: &str) -> NetType {
    let i = iface.to_lowercase();
    if i.starts_with("wl") || i.starts_with("wifi") {
        NetType::Wifi
    } else if i.starts_with("en") || i.starts_with("eth") {
        NetType::Wired
    } else if i.is_empty() {
        NetType::Offline
    } else {
        NetType::Other
    }
}

fn profile(p: &str) -> Profile {
    match p {
        "performance" => Profile::Performance,
        "power-saver" => Profile::PowerSaver,
        _ => Profile::Balanced,
    }
}

fn mihomo(out: &str) -> Mihomo {
    match serde_json::from_str::<Value>(out) {
        Ok(d) => Mihomo {
            online: d["online"].as_bool() == Some(true),
            service: d["service"].as_str().unwrap_or("none").into(),
            version: d["version"].as_str().unwrap_or("").into(),
            mode: d["mode"].as_str().unwrap_or("").to_lowercase(),
            connections: d["connections"].as_f64().unwrap_or(0.) as u64,
            upload: d["upload"].as_f64().unwrap_or(0.) as u64,
            download: d["download"].as_f64().unwrap_or(0.) as u64,
            dashboard: d["dashboard"].as_str().unwrap_or("").into(),
            busy: false,
            message: String::new(),
        },
        Err(_) => Mihomo { service: "none".into(), message: "Could not read Mihomo status".into(), ..Default::default() },
    }
}

/// Bluetooth adapter mock (`Bluetooth.qml`) + `sortedBluetoothSnapshots`.
fn bluetooth(cfg: &Value) -> Bluetooth {
    if cfg.is_null() {
        return Bluetooth::default();
    }
    let mut devices: Vec<BtDevice> = cfg["devices"].as_array().into_iter().flatten().map(|d| {
        let address = d["address"].as_str().unwrap_or("").to_owned();
        let raw = d["name"].as_str().or(d["deviceName"].as_str()).unwrap_or("").trim().to_owned();
        let name = if looks_like_address(&raw, &address) { String::new() } else { raw };
        BtDevice {
            display_name: if name.is_empty() { if address.is_empty() { "Unknown device".into() } else { address.clone() } } else { name.clone() },
            has_name: !name.is_empty(),
            name,
            icon: d["icon"].as_str().filter(|s| !s.is_empty()).unwrap_or("bluetooth").into(),
            paired: d["paired"].as_bool().unwrap_or(false),
            trusted: d["trusted"].as_bool().unwrap_or(false),
            connected: d["connected"].as_bool().unwrap_or(false),
            blocked: d["blocked"].as_bool().unwrap_or(false),
            rssi: None,
            address,
        }
    }).collect();
    devices.sort_by(|a, b| {
        b.connected.cmp(&a.connected)
            .then(b.paired.cmp(&a.paired))
            .then(b.trusted.cmp(&a.trusted))
            .then_with(|| {
                let ka = if a.display_name.is_empty() { &a.address } else { &a.display_name };
                let kb = if b.display_name.is_empty() { &b.address } else { &b.display_name };
                ka.to_lowercase().cmp(&kb.to_lowercase())
            })
    });
    Bluetooth {
        present: true,
        enabled: cfg["enabled"].as_bool().unwrap_or(false),
        discovering: cfg["discovering"].as_bool().unwrap_or(false),
        pairable: cfg["pairable"].as_bool().unwrap_or(false),
        busy: false,
        action_message: String::new(),
        devices,
    }
}

fn looks_like_address(name: &str, address: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    let norm = |v: &str| v.trim().to_uppercase().replace('-', ":");
    if !address.is_empty() && norm(name) == norm(address) {
        return true;
    }
    let u = name.to_uppercase();
    let parts: Vec<&str> = u.split([':', '-']).collect();
    parts.len() == 6 && parts.iter().all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_hexdigit()))
}
