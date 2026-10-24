//! Isolated fixture inputs for the production Assembly, not a second UI renderer.
//! Legacy mock stream parsing is promoted from panels' independently tested adapter.
mod translate;
use hyprv_ui_bar::{self as bar, model::Output};
use hyprv_ui_panels as panels;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};
pub fn merge(base: &mut Value, patch: &Value) {
    if let (Some(a), Some(b)) = (base.as_object_mut(), patch.as_object()) {
        for (k, v) in b {
            merge(a.entry(k).or_insert(Value::Null), v);
        }
    } else {
        *base = patch.clone();
    }
}
fn state<T: Serialize + DeserializeOwned>(base: T, patch: &Value) -> T {
    if patch.is_null() {
        return base;
    }
    let mut value = serde_json::to_value(&base).expect("provider serialization");
    merge(&mut value, patch);
    serde_json::from_value(value).expect("invalid fixture provider snapshot")
}
pub struct Fixture {
    pub json: Value,
    pub revision: u64,
    pub time: f64,
    /// Animation time handed to component models (`now`): [`Fixture::time`] unless the
    /// replay's Qt animation-driver emulation holds it at the last driver tick.
    pub clock: f64,
    pub dark: bool,
    pub scale: f32,
    root: PathBuf,
    translator: translate::Translator,
    assets: Rc<panels::model::Assets>,
    origin: chrono::NaiveDateTime,
}
impl Fixture {
    pub fn load(
        path: &Path,
        dark: bool,
        scale: f32,
        assets: Rc<panels::model::Assets>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let json: Value = serde_json::from_slice(&std::fs::read(path)?)?;
        if json["scene"].as_str() != Some("desktop.qml") {
            return Err("assembled fixtures must use scene=desktop.qml".into());
        }
        if json["entry"].as_str().is_some_and(|s| s != "shell.qml") {
            return Err("assembled fixtures require entry=shell.qml".into());
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let translator = translate::Translator::new(&json, dark, scale, &root, assets.clone());
        let origin = translator.model.wall;
        Ok(Self {
            json,
            revision: 0,
            time: 0.,
            clock: 0.,
            dark,
            scale,
            root,
            translator,
            assets,
            origin,
        })
    }
    pub fn advance_to(&mut self, ms: f64) {
        self.time = ms;
        self.clock = ms;
        self.translator.advance_to(ms);
        self.translator.model.wall =
            crate::mapping::minute_wall(self.origin + chrono::Duration::milliseconds(ms.floor() as i64));
    }
    pub fn apply(&mut self, patch: &Value) {
        self.revision = self.revision.wrapping_add(1);
        if let Some(providers) = patch.get("providers") { merge(&mut self.json["providers"], providers); }
        if let Some(services) = patch.get("services") {
            merge(&mut self.json["services"], services);
            self.translator = translate::Translator::new(
                &self.json,
                self.dark,
                self.scale,
                &self.root,
                self.assets.clone(),
            );
            self.advance_to(self.time);
            // Explicit native-provider acknowledgements override the seed's
            // legacy control-panel-status stdout, which is only a fallback.
            if let Some(profile) = services["powerProfile"].as_str() {
                self.translator.model.controls.profile = match profile {
                    "performance" => panels::model::Profile::Performance,
                    "power-saver" => panels::model::Profile::PowerSaver,
                    _ => panels::model::Profile::Balanced,
                };
            }
        }
        if let Some(root) = patch.get("root") {
            if !self.json["props"].is_object() {
                self.json["props"] = json!({});
            }

            if !self.json["props"]["root"].is_object() {
                self.json["props"]["root"] = json!({});
            }
            merge(&mut self.json["props"]["root"], root);
        }
    }
    pub fn wall(&self) -> chrono::NaiveDateTime { self.translator.model.wall }
    pub fn panels(&self, output: &Output) -> panels::Model {
        let mut m = self.translator.model.clone();
        let b = self.bar(output);
        let native = &self.json["providers"];
        if native["network"].is_object() {
            m.network = panels::services::network(&b.network);
        }
        if native["audio"].is_object() {
            m.audio = panels::services::audio(&b.audio);
        }
        if native["mpris"].is_object() {
            m.media = panels::services::media(&b.media);
        }
        if native["sysstats"].is_object() {
            m.stats = panels::services::stats(&b.stats);
        }
        m.output = panels::model::Output {
            name: output.name.clone(),
            width: output.width,
            height: output.height,
            scale: output.scale,
        };
        if let Some(v) = self.json["props"]["root"]["brightnessPercent"].as_i64() { m.controls.brightness_percent=v.clamp(0,100) as i32; }
        m.now = Duration::from_millis(self.clock.floor() as u64);
        m
    }
    pub fn bar(&self, output: &Output) -> bar::Model {
        use hyprv_svc_sys::hyprland::{DesktopWindow, Workspace};
        let p = &self.translator.model;
        let s = &self.json["services"];
        let hypr = &s["hyprland"];
        let root = &self.json["props"]["root"];
        let mut m = bar::Model {
            output: output.clone(),
            theme: bar::Theme { dark: self.dark },
            now: Duration::from_millis(self.clock.floor() as u64),
            ..Default::default()
        };
        m.desktop.connected = true;
        m.desktop.workspaces = hypr["workspaces"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| {
                let id = v["id"].as_i64().unwrap_or(1) as i32;
                Workspace {
                    id,
                    name: v["name"].as_str().unwrap_or("").into(),
                    monitor: output.name.clone(),
                    active: id == hypr["focusedWorkspace"].as_i64().unwrap_or(1) as i32,
                    windows: 0,
                    tooltip: String::new(),
                    state: String::new(),
                }
            })
            .collect();
        m.urgent_workspaces = hypr["workspaces"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|v| v["urgent"] == true)
            .filter_map(|v| v["id"].as_i64().map(|x| x as i32))
            .collect();
        if let Some(title) = hypr["activeToplevel"]["title"].as_str() {
            m.desktop.windows.push(DesktopWindow {
                address: "fixture-active".into(),
                title: title.into(),
                class: String::new(),
                pid: None,
                monitor: 0,
                geometry: Default::default(),
                fullscreen: hypr["fullscreen"] == true,
            });
            m.desktop.active_window = Some("fixture-active".into());
        }
        m.fullscreen = hypr["fullscreen"] == true;
        m.audio.available = p.audio.available;
        m.audio.muted = p.audio.muted;
        m.audio.volume_percent = p.audio.volume_percent;
        m.media.available = p.media.available;
        m.media.playing = p.media.playing;
        m.media.title = p.media.title.clone();
        m.media.artist = p.media.artist.clone();
        m.media.player_name = p.media.player_name.clone();
        m.media.available = root["mediaAvailable"]
            .as_bool()
            .unwrap_or(m.media.available);
        m.media.playing = root["mediaPlaying"].as_bool().unwrap_or(m.media.playing);
        if let Some(value) = root["mediaTitle"].as_str() {
            m.media.title = value.into();
        }
        if let Some(value) = root["mediaArtist"].as_str() {
            m.media.artist = value.into();
        }
        if let Some(value) = root["mediaPlayerName"].as_str() {
            m.media.player_name = value.into();
        }

        m.stats = state(
            m.stats,
            &json!({"cpu_percent":p.stats.cpu_usage,"memory_percent":p.stats.memory_usage,"temperature_c":Value::Null,"package_power_w":Value::Null,"rx_bytes_per_s":p.stats.rx_rate,"tx_bytes_per_s":p.stats.tx_rate}),
        );
        m.network = state(
            m.network,
            &json!({"enabled":p.network.wifi_radio_enabled,"hardwareEnabled":p.network.wifi_hardware_enabled,"present":p.network.wifi_device_present,"connected":p.network.wifi_connected,"signal":(p.network.wifi_signal_strength*100.).round() as u8,"security":if p.network.wifi_secure {"wpa2"}else{""}}),
        );
        for item in s["tray"].as_array().into_iter().flatten() {
            let key = item["id"].as_str().unwrap_or("");
            m.tray.items.push(state(bar::model::TrayItem::default(),&json!({"id":key,"key":key,"title":item["title"].as_str().unwrap_or(""),"icon":item["icon"].as_str().unwrap_or(""),"icon_name":tray_icon_name(item["icon"].as_str().unwrap_or(""),&self.root),"has_menu":item["hasMenu"].as_bool().unwrap_or(item["menu"].is_object()),"only_menu":item["onlyMenu"].as_bool().unwrap_or(false)})));
        }
        m.tray = crate::mapping::tray(m.tray);
        m.network.default_iface =
            (!p.network.default_interface.is_empty()).then(|| p.network.default_interface.clone());
        m.network.wired_connected = !p.network.default_interface.is_empty()
            && !p.network.default_interface.starts_with("wl");
        m.network.ssid = p.network.wifi_ssid.clone();
        for process in self.json["processes"].as_array().into_iter().flatten() {
            for line in process["lines"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|l| l["at"].as_f64().unwrap_or(0.) <= self.time)
            {
                if let Ok(v) = serde_json::from_str::<Value>(line["data"].as_str().unwrap_or("")) {
                    if process["match"].as_str().unwrap_or("").contains("swaync") {
                        m.notifications.alt = v["alt"].as_str().unwrap_or("").into();
                    }
                    if v["type"] == "system" {
                        let data = v["data"].as_str().unwrap_or("");
                        let section = |key: &str| {
                            data.split(key)
                                .nth(1)
                                .and_then(|s| s.strip_prefix('\n'))
                                .and_then(|s| s.split("\n__").next())
                                .unwrap_or("")
                                .trim()
                        };
                        m.stats.package_power_w = section("__CPU_POWER__").parse().ok();
                        m.stats.temperature_c =
                            section("__TEMP__").parse::<f64>().ok().map(|v| v / 1000.);
                    }
                }
            }
        }
        for item in &m.tray.items {
            if let Some(image) = crate::assets::fixture_tray(item, output.scale) {
                m.assets.images.insert(item.key.clone(), image);
            }
        }
        for item in s["tray"].as_array().into_iter().flatten() {
            if let Some(entries) = item["menu"]["entries"].as_array() {
                let key = item["id"].as_str().unwrap_or("");
                let mut next = 0;
                let children = entries
                    .iter()
                    .map(|v| menu_entry(v, &mut next, key, &self.root, &mut m.assets))
                    .collect();
                m.tray.menus.push(hyprv_svc_media::tray::TrayMenu {
                    item_key: key.into(),
                    revision: 1,
                    root: bar::model::TrayMenuEntry {
                        children,
                        ..Default::default()
                    },
                });
            }
        }
        let native = &self.json["providers"];
        m.desktop = state(m.desktop, &native["hyprland"]);
        m.stats = state(m.stats, &native["sysstats"]);
        m.network = state(m.network, &native["network"]);
        m.audio = state(m.audio, &native["audio"]);
        m.media = state(m.media, &native["mpris"]);
        m.power = state(m.power, &native["power"]);
        m.battery = state(m.battery, &native["battery"]);
        m.tray = state(m.tray, &native["tray"]);
        m.notifications = state(m.notifications, &native["notifications"]);
        if let Some(b) = root["batteryInfo"].as_object() {
            let b = Value::Object(b.clone());
            let mode = match b["mode"].as_str().unwrap_or("") {
                "charging" => bar::model::BatteryMode::Charging,
                "discharging" => bar::model::BatteryMode::Discharging,
                "full" => bar::model::BatteryMode::Full,
                "plugged" => bar::model::BatteryMode::Plugged,
                _ => bar::model::BatteryMode::Idle,
            };
            m.battery = bar::model::BatteryDetail {
                available: b["available"] == true,
                status: b["status"].as_str().unwrap_or("").into(),
                mode,
                capacity_percent: b["capacity"].as_f64(),
                power_w: b["powerW"].as_f64(),
                average_power_w: b["averagePowerW"].as_f64(),
                samples: b["sampleCount"].as_u64().unwrap_or(0) as u32,
                window_s: b["sampleWindowSeconds"].as_u64().unwrap_or(0) as u32,
                window_complete: b["windowComplete"] == true,
                estimate_s: b["estimateSeconds"].as_f64(),
                estimate_basis: b["estimateBasis"].as_str().unwrap_or("").into(),
                energy_now_wh: b["energyNowWh"].as_f64(),
                energy_full_wh: b["energyFullWh"].as_f64(),
            };
            m.power.battery_present = m.battery.available;
            m.power.battery_percent = m.battery.capacity_percent;
            m.power.on_battery = m.battery.mode == bar::model::BatteryMode::Discharging;
        }
        m
    }
    pub fn island(&self) -> hyprv_ui_island::Inputs {
        let p = &self.translator.model;
        let r = &self.json["props"]["root"];
        let string = |key: &str, fallback: &str| r[key].as_str().unwrap_or(fallback).to_owned();
        let status = self.json["processes"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|v| {
                v["match"]
                    .as_str()
                    .is_some_and(|s| s.contains("media-status"))
            })
            .and_then(|p| p["stdout"].as_str())
            .unwrap_or("");
        let number = |key: &str| {
            status
                .lines()
                .find_map(|l| l.strip_prefix(key).and_then(|v| v.parse::<f64>().ok()))
                .unwrap_or(0.)
        };
        let mut inputs = hyprv_ui_island::Inputs {
            dark: self.dark,
            now_hh_mm: p.wall.format("%H:%M").to_string(),
            media: hyprv_ui_island::Media {
                available: r["mediaAvailable"].as_bool().unwrap_or(p.media.available),
                playing: r["mediaPlaying"].as_bool().unwrap_or(p.media.playing),
                title: string("mediaTitle", &p.media.title),
                artist: string("mediaArtist", &p.media.artist),
                player: string("mediaPlayerName", &p.media.player_name),
                // The island draws art only for a non-empty art_url (view.rs:90).
                art_url: string("mediaArtUrl", &p.media.art_url),
                position_s: r["mediaPositionSeconds"]
                    .as_f64()
                    .unwrap_or_else(|| number("position=")),
                length_s: r["mediaLengthSeconds"]
                    .as_f64()
                    .unwrap_or_else(|| number("length=")),
            },
            ..Default::default()
        };
        inputs.art = p.media.art.clone();
        inputs.spectrum = match r["spectrumValues"].as_array() {
            Some(values) => values
                .iter()
                .map(|v| v.as_f64().unwrap_or(0.) as f32)
                .collect(),
            None => self.translator.spectrum.clone(),
        };
        inputs.agents = r["agentSessions"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|a| {
                let get = |key: &str| a[key].as_str().unwrap_or("").to_string();
                hyprv_ui_island::Agent {
                    id: get("id"),
                    source: get("source"),
                    source_label: get("source_label"),
                    title: get("title"),
                    project: get("project"),
                    status: get("status"),
                    current_tool: get("current_tool"),
                    detail: get("detail"),
                    progress: a["progress"].as_f64(),
                }
            })
            .collect();
        inputs
    }
}

/// Fixture asset images, decoded once: the bar model is rebuilt every frame, and a fresh
/// decode would give the menu a new image identity (and a full re-render) each time.
fn fixture_asset(path: &Path) -> Option<hyprv_ui::skia::Image> {
    thread_local! {
        static CACHE: std::cell::RefCell<std::collections::HashMap<PathBuf, Option<hyprv_ui::skia::Image>>> = Default::default();
    }
    CACHE.with(|c| {
        c.borrow_mut()
            .entry(path.to_path_buf())
            .or_insert_with(|| {
                std::fs::read(path)
                    .ok()
                    .and_then(|b| crate::assets::decode(&b))
            })
            .clone()
    })
}

/// Fixture tray `icon`: a theme name, an absolute path, or `@asset:file` (fixture assets),
/// as the SNI `IconName` the shell resolves.
fn tray_icon_name(icon: &str, root: &Path) -> String {
    match icon.strip_prefix("@asset:") {
        Some(file) => root
            .join("parity/fixtures/assets")
            .join(file)
            .to_string_lossy()
            .into_owned(),
        None => icon.to_owned(),
    }
}

fn menu_entry(
    v: &Value,
    next: &mut i32,
    key: &str,
    root: &Path,
    assets: &mut bar::model::Assets,
) -> bar::model::TrayMenuEntry {
    use bar::model::{ButtonType, CheckState, TrayMenuEntry};
    *next += 1;
    let id = *next;
    let icon = v["icon"].as_str().unwrap_or("");
    if let Some(path) = icon.strip_prefix("@asset:") {
        if let Some(image) = fixture_asset(&root.join("parity/fixtures/assets").join(path)) {
            assets.images.insert(format!("menu:{key}:{id}"), image);
        }
    }
    let children = v["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| menu_entry(v, next, key, root, assets))
        .collect::<Vec<_>>();
    TrayMenuEntry {
        id,
        text: v["text"].as_str().unwrap_or("").into(),
        enabled: v["enabled"].as_bool().unwrap_or(true),
        visible: v["visible"].as_bool().unwrap_or(true),
        is_separator: v["isSeparator"] == true,
        button_type: match v["buttonType"].as_u64() {
            Some(1) => ButtonType::CheckBox,
            Some(2) => ButtonType::RadioButton,
            _ => ButtonType::None,
        },
        check_state: match v["checkState"].as_u64() {
            Some(2) => CheckState::Checked,
            Some(1) => CheckState::PartiallyChecked,
            _ => CheckState::Unchecked,
        },
        has_children: !children.is_empty(),
        icon_name: icon.into(),
        children,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(spectrum_running: bool) -> Fixture {
        let json = json!({
            "scene": "desktop.qml",
            "now": "2026-10-23T09:41:00",
            "services": {
                "screens": [{"name": "DP-4", "width": 3072, "height": 1728, "x": 0, "y": 0, "scale": 2}],
                "mpris": {"players": [{"identity": "Spotify"}]},
                "tray": [
                    {"id": "chrome", "title": "Chrome", "icon": "google-chrome"},
                    {"id": "asset", "title": "A", "icon": "@asset:cover-warm.png"}
                ]
            },
            "processes": [
                {"match": "command -v cava", "exitCode": if spectrum_running { 0 } else { 1 }},
                {"match": "media-status.sh", "stdout": "available=true\nplaying=true\ntitle=Midnight City\nartist=M83\nplayer=spotify\nart_url=file://@ASSETS@/cover-warm.png\nposition=83\nlength=225\n"},
                {"match": "audio-spectrum.sh", "stream": true, "lines": [
                    {"at": 0, "data": "12;38;-4;x;;92"},
                    {"at": 500, "data": ";;"},
                    {"at": 1000, "data": "1;2"}
                ]}
            ],
            "steps": []
        });
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("hyprv-fixture-test-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("f.json");
        std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
        let f = Fixture::load(&path, false, 2., Rc::new(Default::default())).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
        f
    }

    #[test]
    fn island_gets_art_url_and_the_streamed_spectrum() {
        let mut f = fixture(true);
        f.advance_to(0.);
        let i = f.island();
        assert!(
            i.media
                .art_url
                .ends_with("parity/fixtures/assets/cover-warm.png"),
            "{}",
            i.media.art_url
        );
        assert!(i.art.is_some(), "fixture art decoded");
        // QML parse: empty parts skipped, NaN → 0, negatives → 0.
        assert_eq!(i.spectrum, vec![12., 38., 0., 0., 92.]);
        // A line without values keeps the previous values.
        f.advance_to(600.);
        assert_eq!(f.island().spectrum, vec![12., 38., 0., 0., 92.]);
        f.advance_to(1000.);
        assert_eq!(f.island().spectrum, vec![1., 2.]);
    }

    #[test]
    fn spectrum_needs_cava_like_audio_spectrum_process_running() {
        let mut f = fixture(false);
        f.advance_to(1000.);
        assert!(f.island().spectrum.is_empty());
    }

    #[test]
    fn tray_items_carry_icon_name_for_theme_lookup() {
        let f = fixture(false);
        let out = bar::model::Output {
            name: "DP-4".into(),
            width: 3072.,
            height: 1728.,
            scale: 2.,
            ..Default::default()
        };
        let m = f.bar(&out);
        let chrome = m.tray.items.iter().find(|i| i.key == "chrome").unwrap();
        assert_eq!(chrome.icon_name, "google-chrome");
        let asset = m.tray.items.iter().find(|i| i.key == "asset").unwrap();
        assert!(
            asset.icon_name.starts_with('/')
                && asset
                    .icon_name
                    .ends_with("parity/fixtures/assets/cover-warm.png")
        );
        // Every fixture tray item gets an image (theme icon or Quickshell's placeholder).
        assert!(m.assets.images.contains_key("chrome") && m.assets.images.contains_key("asset"));
    }
}
