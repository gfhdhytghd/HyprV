//! View models (the "Data contract" sections of docs/spec/ui-panels*.md) and outgoing
//! actions. These are plain types so the UI logic is independent of provider crates;
//! `crate::services` (feature `services`) converts hyprv-svc-* State types into them and
//! maps [`Action`] onto the service Action enums.
use hyprv_ui::skia::{svg, Image};
use std::{collections::BTreeMap, rc::Rc, time::Duration};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Theme {
    pub dark: bool,
}

/// The output a popup surface lives on (logical size, scale).
#[derive(Clone, Debug, PartialEq)]
pub struct Output {
    pub name: String,
    pub width: f32,
    pub height: f32,
    pub scale: f32,
}
impl Default for Output {
    fn default() -> Self {
        Self { name: "DP-4".into(), width: 3072., height: 1728., scale: 2. }
    }
}

/// Shell geometry the panels anchor to (ui-panels.md §6).
#[derive(Clone, Debug, PartialEq)]
pub struct ShellGeometry {
    /// `round(25 + 33·reveal)`; 58 with the bar fully shown.
    pub bar_exclusive_zone: f32,
    /// Right edge (output-local) of the primary bar's right tray pill (QuickAdjust anchor).
    pub quick_adjust_anchor_right: Option<f32>,
}
impl Default for ShellGeometry {
    fn default() -> Self {
        Self { bar_exclusive_zone: 58., quick_adjust_anchor_right: None }
    }
}

/// An anchor item in output-local logical coordinates (`mapToGlobal − screen`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Anchor {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl Anchor {
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }
}

/// Decoded assets supplied by the host (no filesystem/theme probing in UI crates).
/// Keys: Fluent icon paths relative to the theme dir exactly as the QML builds them
/// (`symbolic/status/<name>-symbolic.svg` dark, `24/panel/<name>.svg` light, see
/// [`crate::icons`]), plus `assets/control-panel/light-dark.svg`,
/// `assets/tray/network-wired-light.svg`, and media art URLs.
#[derive(Clone, Default)]
pub struct Assets {
    pub svgs: BTreeMap<String, svg::Dom>,
    pub images: BTreeMap<String, Image>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NetType {
    Wifi,
    Wired,
    Other,
    #[default]
    Offline,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WifiNetwork {
    pub ssid: String,
    /// 0..100
    pub signal: u8,
    pub security: String,
    pub secure: bool,
    pub known: bool,
    pub active: bool,
    pub enterprise: bool,
}

/// ui-panels-wifi.md §6
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Network {
    pub default_interface: String,
    pub active_type: NetType,
    pub wifi_device_present: bool,
    pub wifi_capability_detected: bool,
    pub wifi_radio_enabled: bool,
    pub wifi_hardware_enabled: bool,
    pub wifi_connected: bool,
    pub wifi_ssid: String,
    /// 0..1
    pub wifi_signal_strength: f32,
    pub wifi_secure: bool,
    pub wifi_interface: String,
    pub networks: Vec<WifiNetwork>,
    pub action_busy: bool,
    pub action_message: String,
}
impl Network {
    pub fn network_connected(&self) -> bool {
        !self.default_interface.is_empty()
    }
    pub fn wifi_controls_available(&self) -> bool {
        self.wifi_device_present || self.wifi_capability_detected
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BtDevice {
    pub address: String,
    pub name: String,
    pub display_name: String,
    pub has_name: bool,
    pub icon: String,
    pub paired: bool,
    pub trusted: bool,
    pub connected: bool,
    pub blocked: bool,
    pub rssi: Option<i16>,
}

/// ui-panels-control.md §3.3
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bluetooth {
    pub present: bool,
    pub enabled: bool,
    pub discovering: bool,
    pub pairable: bool,
    pub busy: bool,
    pub action_message: String,
    /// Service order: connected, paired, trusted, then case-insensitive name.
    pub devices: Vec<BtDevice>,
}

#[derive(Clone, Default)]
pub struct Media {
    pub available: bool,
    pub playing: bool,
    pub title: String,
    pub artist: String,
    pub player_name: String,
    pub art_url: String,
    /// Decoded album art (host-loaded from `art_url`).
    pub art: Option<Image>,
}
impl std::fmt::Debug for Media {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Media({},{},{:?},{:?},{})", self.available, self.playing, self.title, self.art_url, self.art.is_some())
    }
}
impl PartialEq for Media {
    fn eq(&self, o: &Self) -> bool {
        self.available == o.available
            && self.playing == o.playing
            && self.title == o.title
            && self.artist == o.artist
            && self.player_name == o.player_name
            && self.art_url == o.art_url
            && self.art.as_ref().map(|i| i.unique_id()) == o.art.as_ref().map(|i| i.unique_id())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Audio {
    pub available: bool,
    pub muted: bool,
    pub volume_percent: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Profile {
    PowerSaver,
    #[default]
    Balanced,
    Performance,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Target {
    #[default]
    Windows,
    MacOs,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mihomo {
    pub online: bool,
    /// "user" | "system" | "verge" | "none"
    pub service: String,
    pub version: String,
    pub mode: String,
    pub connections: u64,
    pub upload: u64,
    pub download: u64,
    pub dashboard: String,
    pub busy: bool,
    pub message: String,
}

/// Everything the control panel main page shows (ui-panels-control.md §1.6).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Controls {
    pub screen_recording: bool,
    pub prevent_sleep: bool,
    pub demo_mode: bool,
    pub dnd: bool,
    pub profile: Profile,
    /// 0..100
    pub brightness_percent: i32,
    pub fan_available: bool,
    pub chassis_fan_percent: i32,
    pub pump_fan_percent: i32,
    pub chassis_fan_rpm: u32,
    pub pump_fan_rpm: u32,
    pub vm_running: bool,
    pub vm_busy: bool,
    pub viewflow_target: Target,
    pub viewflow_busy: bool,
    pub mihomo: Mihomo,
}

/// ui-panels-stats.md §2.4. Histories hold the last ≤120 samples (oldest first).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub cpu_core_usages: Vec<f64>,
    pub default_interface: String,
    pub rx_rate: f64,
    pub tx_rate: f64,
    pub cpu_history: Vec<f64>,
    pub memory_history: Vec<f64>,
    pub network_history: Vec<f64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CalendarInfo {
    pub id: String,
    pub name: String,
    /// `#RRGGBB` or `#AARRGGBB`
    pub color: Option<String>,
    pub hidden: bool,
    pub holds_tasks: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CalendarEvent {
    pub uid: String,
    pub calendar_id: String,
    pub summary: String,
    /// ISO-8601 (`YYYY-MM-DD` for all-day dates)
    pub start: String,
    pub end: String,
    pub all_day: bool,
    pub location: String,
    pub description: String,
}

/// ui-panels-calendar.md §2.6
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Calendar {
    /// The bridge exists (panel presented).
    pub bridge: bool,
    pub connected: bool,
    pub last_error: String,
    /// Monday (local) `YYYY-MM-DD` of the loaded week; empty until the first `loadWeek`.
    pub week_start: String,
    pub calendars: Vec<CalendarInfo>,
    pub events: Vec<CalendarEvent>,
}

/// All inputs. `now` is the frame clock (monotonic) used for animations; `wall` the local
/// wall clock (minute precision like the shell's SystemClock).
#[derive(Clone)]
pub struct Model {
    pub theme: Theme,
    pub output: Output,
    pub shell: ShellGeometry,
    pub now: Duration,
    pub wall: chrono::NaiveDateTime,
    pub network: Network,
    pub bluetooth: Bluetooth,
    pub bluetooth_show_unnamed: bool,
    pub media: Media,
    pub audio: Audio,
    pub controls: Controls,
    pub stats: Stats,
    pub calendar: Calendar,
    pub assets: Rc<Assets>,
}
impl Default for Model {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            output: Output::default(),
            shell: ShellGeometry::default(),
            now: Duration::ZERO,
            wall: chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap().and_hms_opt(14, 30, 0).unwrap(),
            network: Network::default(),
            bluetooth: Bluetooth::default(),
            bluetooth_show_unnamed: true,
            media: Media::default(),
            audio: Audio::default(),
            controls: Controls::default(),
            stats: Stats::default(),
            calendar: Calendar::default(),
            assets: Rc::new(Assets::default()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FanChannel {
    Chassis,
    Pump,
}

/// Outgoing actions. Service actions map 1:1 onto hyprv-svc-* Action enums (see
/// `crate::services`); `Host` actions are intents for the shell host.
/// `Debug` is hand-written so `WifiConnect.password` is never printed. Hosts must move
/// the password out (`std::mem::take`) into hyprv-svc-net's `Connect`, which zeroizes it,
/// or zeroize it themselves; the panel's own buffer is wiped when the action is made.
#[derive(Clone, PartialEq)]
pub enum Action {
    WifiSetRadio(bool),
    WifiRescan,
    WifiDisconnect,
    WifiConnect { ssid: String, password: String, security: String },
    BluetoothSetPower(bool),
    BluetoothScan,
    BluetoothConnect { address: String, paired: bool },
    BluetoothDisconnect { address: String },
    /// `volume("set", v)`
    SetVolume(i32),
    ToggleMute,
    MediaPrevious,
    MediaPlayPause,
    MediaNext,
    /// `applyBrightnessPercent` (serialised set, local snap to 2 %).
    BrightnessSet(i32),
    /// `finishBrightnessPercent`
    BrightnessFinish(i32),
    FanLimit { channel: FanChannel, percent: i32 },
    ProfileSet(Profile),
    ToggleDnd,
    ToggleRecording,
    TogglePreventSleep,
    ToggleDemo,
    ToggleTheme,
    VmToggle,
    ViewflowSet(Target),
    MihomoStart,
    MihomoStop,
    MihomoMode(String),
    MihomoRefresh,
    MihomoOpenDashboard,
    CalendarPreviousWeek,
    CalendarNextWeek,
    CalendarOpenEvent { uid: String, start: String },
    CalendarNewEvent,
    CalendarShowFullApp,
    Host(HostAction),
}

impl std::fmt::Debug for Action {
    /// Same output as a derived Debug, except the Wi-Fi password is redacted.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Action as A;
        match self {
            A::WifiConnect { ssid, password, security } => f
                .debug_struct("WifiConnect")
                .field("ssid", ssid)
                .field("password", &if password.is_empty() { "<empty>" } else { "<redacted>" })
                .field("security", security)
                .finish(),
            A::WifiSetRadio(v) => f.debug_tuple("WifiSetRadio").field(v).finish(),
            A::WifiRescan => f.write_str("WifiRescan"),
            A::WifiDisconnect => f.write_str("WifiDisconnect"),
            A::BluetoothSetPower(v) => f.debug_tuple("BluetoothSetPower").field(v).finish(),
            A::BluetoothScan => f.write_str("BluetoothScan"),
            A::BluetoothConnect { address, paired } => f
                .debug_struct("BluetoothConnect")
                .field("address", address)
                .field("paired", paired)
                .finish(),
            A::BluetoothDisconnect { address } => f.debug_struct("BluetoothDisconnect").field("address", address).finish(),
            A::SetVolume(v) => f.debug_tuple("SetVolume").field(v).finish(),
            A::ToggleMute => f.write_str("ToggleMute"),
            A::MediaPrevious => f.write_str("MediaPrevious"),
            A::MediaPlayPause => f.write_str("MediaPlayPause"),
            A::MediaNext => f.write_str("MediaNext"),
            A::BrightnessSet(v) => f.debug_tuple("BrightnessSet").field(v).finish(),
            A::BrightnessFinish(v) => f.debug_tuple("BrightnessFinish").field(v).finish(),
            A::FanLimit { channel, percent } => f
                .debug_struct("FanLimit")
                .field("channel", channel)
                .field("percent", percent)
                .finish(),
            A::ProfileSet(v) => f.debug_tuple("ProfileSet").field(v).finish(),
            A::ToggleDnd => f.write_str("ToggleDnd"),
            A::ToggleRecording => f.write_str("ToggleRecording"),
            A::TogglePreventSleep => f.write_str("TogglePreventSleep"),
            A::ToggleDemo => f.write_str("ToggleDemo"),
            A::ToggleTheme => f.write_str("ToggleTheme"),
            A::VmToggle => f.write_str("VmToggle"),
            A::ViewflowSet(v) => f.debug_tuple("ViewflowSet").field(v).finish(),
            A::MihomoStart => f.write_str("MihomoStart"),
            A::MihomoStop => f.write_str("MihomoStop"),
            A::MihomoMode(v) => f.debug_tuple("MihomoMode").field(v).finish(),
            A::MihomoRefresh => f.write_str("MihomoRefresh"),
            A::MihomoOpenDashboard => f.write_str("MihomoOpenDashboard"),
            A::CalendarPreviousWeek => f.write_str("CalendarPreviousWeek"),
            A::CalendarNextWeek => f.write_str("CalendarNextWeek"),
            A::CalendarOpenEvent { uid, start } => f
                .debug_struct("CalendarOpenEvent")
                .field("uid", uid)
                .field("start", start)
                .finish(),
            A::CalendarNewEvent => f.write_str("CalendarNewEvent"),
            A::CalendarShowFullApp => f.write_str("CalendarShowFullApp"),
            A::Host(v) => f.debug_tuple("Host").field(v).finish(),
        }
    }
}
#[cfg(test)]
mod action_debug_tests {
    use super::*;
    #[test]
    fn wifi_password_is_redacted_and_other_variants_match_derive_shape() {
        let a = Action::WifiConnect { ssid: "Home".into(), password: "hunter2-secret".into(), security: "WPA".into() };
        let text = format!("{a:?} {a:#?}");
        assert!(!text.contains("hunter2-secret"), "{text}");
        assert!(text.contains("WifiConnect { ssid: \"Home\", password: \"<redacted>\", security: \"WPA\" }"));
        assert_eq!(format!("{:?}", Action::SetVolume(5)), "SetVolume(5)");
        assert_eq!(format!("{:?}", Action::WifiRescan), "WifiRescan");
        assert_eq!(
            format!("{:?}", Action::BluetoothConnect { address: "AA".into(), paired: true }),
            "BluetoothConnect { address: \"AA\", paired: true }"
        );
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum HostAction {
    /// Unmap happened; the popup surface can be destroyed.
    Unmapped(PanelKind),
    /// Map the popup's surface (it became visible).
    Mapped(PanelKind),
    /// `openWifiPanel()` from the control-panel Wi-Fi tile.
    OpenWifiPanel,
    OpenWifiManager,
    OpenBluetoothManager,
    /// UI-only preference `bluetoothShowUnnamedDevices`.
    SetBluetoothShowUnnamed(bool),
    /// While the Wi-Fi popup is visible the scanner runs more actively.
    WifiPanelVisible(bool),
    /// Panel-open data refresh (brightness probe, status, media, BT sync).
    RefreshPanelData,
    /// Calendar: `requestCalendarClose(force)`.
    CloseCalendar { force: bool },
    /// Calendar `expandRequested`: close (forced) then `bridge.showFullApp()`.
    ExpandCalendar,
    /// The pointer entered/left the calendar panel (leave-timer logic lives in the bar).
    CalendarPointerInside(bool),
    /// StartupConfirm decision (true = write `launch`, quit; false = quit).
    StartupDecision(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PanelKind {
    ControlPanel,
    Wifi,
    QuickAdjust,
    Stats,
    Startup,
}

