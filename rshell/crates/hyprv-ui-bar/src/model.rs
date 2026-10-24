pub use hyprv_svc_media::{
    audio::{AudioAction, AudioState},
    mpris::{MprisAction, MprisState},
    tray::{ButtonType, CheckState, TrayAction, TrayItem, TrayMenuEntry, TrayState},
};
pub use hyprv_svc_net::network::State as NetworkState;
pub use hyprv_svc_power::{
    battery_history::State as BatteryDetail,
    power::{BatteryMode, State as PowerState},
    sysstats::State as SystemStats,
};
pub use hyprv_svc_sys::hyprland::{Action as DesktopAction, State as DesktopState};
pub use hyprv_core::profile::Profile;
use hyprv_ui::{geometry::Rect, scene::Node, skia::Image};
use std::{collections::BTreeMap, rc::Rc, time::Duration};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Theme {
    pub dark: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Output {
    pub name: String,
    pub width: f32,
    pub height: f32,
    pub scale: f32,
    pub global_x: f32,
    pub global_y: f32,
}
impl Default for Output {
    fn default() -> Self {
        Self {
            name: "DP-4".into(),
            width: 3072.,
            height: 1728.,
            scale: 2.,
            global_x: 0.,
            global_y: 0.,
        }
    }
}
/// Host supplies already decoded assets; no theme/icon/path probing happens in UI.
#[derive(Clone, Default)]
pub struct Assets {
    pub images: BTreeMap<String, Image>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IslandGeometry {
    pub width: f32,
    pub height: f32,
    pub attached_center_offset: f32,
    pub expanded: bool,
}
impl Default for IslandGeometry {
    fn default() -> Self {
        Self {
            width: 120.16,
            height: 38.,
            attached_center_offset: 0.,
            expanded: false,
        }
    }
}
pub use hyprv_svc_media::notifications::{NotificationAction, NotificationState as Notifications};
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target {
    Launcher,
    Viewflow,
    Workspace(i32),
    Cpu,
    Memory,
    Network,
    Temperature,
    /// Laptop `cpuPowerTrigger` (laptop-bar.md §3): hover-only, tooltip with temperature.
    CpuPower,
    Battery,
    Previous,
    PlayPause,
    Next,
    Volume,
    Wifi,
    Tray(String),
    Overflow,
    ControlPanel,
    Notifications,
    /// Laptop compact-strip clock (laptop-bar.md §2): expand, then toggle the calendar.
    StripClock,
}
/// Separate component crates are composed as pure builders by the host.
#[derive(Clone, Default)]
pub struct Slots {
    pub island: Option<Rc<dyn Fn() -> Node>>,
    pub calendar: Option<Rc<dyn Fn() -> Node>>,
}
/// Wall-clock fields the compact strip formats (`ddd  HH:mm`). Host supplied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WallClock {
    /// 0 = Monday … 6 = Sunday.
    pub weekday: u8,
    pub hour: u8,
    pub minute: u8,
}
#[derive(Clone)]
pub struct Model {
    /// Runtime policy, latched by the host from `hyprv_core::profile::profile()`.
    pub profile: Profile,
    /// Laptop: this output's adaptive-bar target (laptop-bar.md §1). Desktop ignores it.
    pub expanded: bool,
    pub clock: WallClock,
    pub output: Output,
    pub theme: Theme,
    pub now: Duration,
    pub monotonic_us: u64,
    pub desktop: DesktopState,
    pub stats: SystemStats,
    /// QML keeps the last finite CPU power for color even when its label is -- W.
    pub last_package_power_w: f64,
    pub power: PowerState,
    pub battery: BatteryDetail,
    pub audio: AudioState,
    pub media: MprisState,
    pub network: NetworkState,
    pub tray: TrayState,
    pub notifications: Notifications,
    pub assets: Assets,
    pub slots: Slots,
    pub island: IslandGeometry,
    pub fullscreen: bool,
    pub fullscreen_hide: f64,
    pub reveal: f64,
    pub calendar_open: bool,
    pub calendar_presented: bool,
    pub calendar_progress: f64,
    pub calendar_width: f32,
    pub calendar_height: f32,
    pub calendar_radius: f32,
    pub calendar_child_active: bool,
    pub calendar_child_opening: bool,
    pub calendar_pointer_armed: bool,
    pub is_primary: bool,
    pub hovered: Option<Target>,
    /// Source Hyprland data currently normalizes fullscreen to bool and omits urgency;
    /// host supplies exact fullscreen==2 and per-workspace urgency from raw state.
    pub wifi_capability_detected: bool,
    pub urgent_workspaces: Vec<i32>,
    pub network_widget_visible: Option<bool>,
    /// Laptop CPU-power tooltip shown (hovered for `tooltip::DELAY`).
    pub tooltip_visible: bool,
}
impl Default for Model {
    fn default() -> Self {
        Self {
            profile: Profile::Desktop,
            expanded: false,
            clock: WallClock::default(),
            output: Output::default(),
            theme: Theme::default(),
            now: Duration::ZERO,
            monotonic_us: 0,
            desktop: DesktopState::default(),
            stats: SystemStats::default(),
            last_package_power_w: 0.,
            power: PowerState::default(),
            battery: BatteryDetail::default(),
            audio: AudioState::default(),
            media: MprisState::default(),
            network: NetworkState::default(),
            tray: TrayState::default(),
            notifications: Notifications::default(),
            assets: Assets::default(),
            slots: Slots::default(),
            island: IslandGeometry::default(),
            fullscreen: false,
            fullscreen_hide: 0.,
            reveal: 1.,
            calendar_open: false,
            calendar_presented: false,
            calendar_progress: 0.,
            calendar_width: 120.16,
            calendar_height: 38.,
            calendar_radius: 24.,
            calendar_child_active: false,
            calendar_child_opening: false,
            calendar_pointer_armed: false,
            is_primary: true,
            hovered: None,
            wifi_capability_detected: false,
            urgent_workspaces: Vec::new(),
            network_widget_visible: None,
            tooltip_visible: false,
        }
    }
}
impl Model {
    /// Preserve the live QML's self-gated battery-info subscription by default.
    /// A seeded detail snapshot (parity fixtures) still exposes the latent UI.
    pub fn laptop(&self) -> bool {
        self.profile == Profile::Laptop
    }
    /// QUIRK_BAR_TOGGLE_IS_NOOP holds for the desktop QML only (laptop-bar.md §1).
    pub fn bar_toggle_is_noop(&self) -> bool {
        QUIRK_BAR_TOGGLE_IS_NOOP && !self.laptop()
    }
    /// Laptop reads UPower's DisplayDevice, so the battery popup is reachable (§5).
    pub fn battery_popup_never_opens(&self) -> bool {
        QUIRK_BATTERY_POPUP_NEVER_OPENS && !self.laptop()
    }
    pub fn battery_available(&self) -> bool {
        if self.battery_popup_never_opens() {
            self.battery.available
        } else {
            self.power.battery_present
        }
    }
    pub fn battery_percent(&self) -> f64 {
        if self.battery_popup_never_opens() {
            self.battery
                .capacity_percent
                .filter(|v| v.is_finite())
                .unwrap_or(0.)
                .clamp(0., 100.)
        } else {
            self.power.battery_percent.unwrap_or(0.)
        }
    }
    pub fn battery_on_battery(&self) -> bool {
        if self.battery_popup_never_opens() {
            !matches!(
                self.battery.mode,
                BatteryMode::Charging | BatteryMode::Plugged | BatteryMode::Full
            )
        } else if self.laptop() {
            // `batteryPlugged` is mode-based; UPower "idle" (Unknown + OnBattery) is unplugged.
            !matches!(
                self.power.battery_mode,
                BatteryMode::Charging | BatteryMode::Plugged | BatteryMode::Full
            )
        } else {
            self.power.on_battery
        }
    }
    /// Laptop `cpuPackagePowerAvailable`: finite and non-negative.
    pub fn cpu_power_available(&self) -> bool {
        self.stats.package_power_w.is_some_and(|w| w.is_finite() && w >= 0.)
    }
    /// BarScreenState.topCornerOffset.
    pub fn top_corner_offset(&self) -> f32 {
        (15. * (1. - self.reveal) * (1. - self.fullscreen_hide)).clamp(0., 15.) as f32
    }
    /// Laptop compact strip is drawn by the corner window while `reveal < 1`.
    pub fn compact_strip_visible(&self) -> bool {
        self.laptop() && self.reveal < 1.
    }
    pub fn network_visible(&self) -> bool {
        self.network_widget_visible.unwrap_or(
            self.wifi_capability_detected
                || self.network.present
                || !self.network.networks.is_empty()
                || self.stats.iface.as_ref().is_some_and(|s| !s.is_empty())
                || self
                    .network
                    .default_iface
                    .as_ref()
                    .is_some_and(|s| !s.is_empty()),
        )
    }
    pub fn focused_workspace(&self) -> i32 {
        self.desktop
            .workspaces
            .iter()
            .find(|w| w.active)
            .map(|w| w.id)
            .filter(|i| *i != 0)
            .unwrap_or(1)
    }
    pub fn title(&self) -> &str {
        self.desktop
            .active_window
            .as_ref()
            .and_then(|id| self.desktop.windows.iter().find(|w| &w.address == id))
            .filter(|w| {
                QUIRK_WINDOW_TITLE_GLOBAL_ACROSS_OUTPUTS
                    || self
                        .desktop
                        .monitors
                        .iter()
                        .any(|monitor| monitor.id == w.monitor && monitor.name == self.output.name)
            })
            .map(|w| w.title.as_str())
            .unwrap_or("")
    }
    pub fn dnd(&self) -> bool {
        self.notifications.dnd || self.notifications.alt.contains("dnd")
    }
    pub fn surface_height(&self) -> f32 {
        self.output
            .height
            .min(if (QUIRK_LAPTOP_TALL_BAR_SURFACE && self.laptop()) || self.calendar_presented || self.island.expanded {
                820.
            } else {
                58.
            })
    }
}
/// Non-service actions are host intents; they must be routed to native application/
/// panel owners. This crate never interprets them as commands to execute.
#[derive(Clone, Debug, PartialEq)]
pub enum HostAction {
    Launcher,
    ViewflowStart { global_x: i32, global_y: i32 },
    ViewflowMenu,
    OpenSystemMonitor,
    OpenAudioMixer,
    ToggleStats { anchor: Rect },
    ToggleControlPanel { anchor: Rect },
    ToggleWifi { anchor: Rect },
    ToggleCalendar,
    CloseCalendar { force: bool },
    /// Laptop `toggleBar(output)`; the host applies the global debounce (ipc::ToggleGate).
    ToggleBar,
    ShowVolume,
}
#[derive(Clone, Debug)]
pub enum Action {
    Audio(AudioAction),
    Media(MprisAction),
    Desktop(DesktopAction),
    Tray(TrayAction),
    Host(HostAction),
    Notification(NotificationAction),
    AcquireBatteryDetails(bool),
}
// Explicit switches for source quirks (ui-bar*.md). Defaults preserve current QML.
pub const QUIRK_BAR_TOGGLE_IS_NOOP: bool = true;
pub const QUIRK_INVERT_VOLUME_WHEEL: bool = true;
pub const QUIRK_HORIZONTAL_TRAY_WHEEL_SENDS_MINUS_ONE: bool = true;
pub const QUIRK_TRAY_EXCLUDED_FROM_SUPPLEMENTARY_THRESHOLD: bool = true;
pub const QUIRK_GLASS_ANCHORED_CONTENT_IGNORES_SLIDE: bool = true;
pub const QUIRK_WINDOW_TITLE_GLOBAL_ACROSS_OUTPUTS: bool = true;

pub const QUIRK_UNAVAILABLE_POWER_RETAINS_SEVERITY: bool = true;

/// shell.qml:3340-3349,4078-4081: details poll only while the popup is open,
/// but opening requires details availability. False opts into the UPower fix.
pub const QUIRK_BATTERY_POPUP_NEVER_OPENS: bool = true;

// Laptop profile (docs/spec/laptop-bar.md). Each applies only when `Model::profile`
// is Laptop; the desktop paths above are unchanged.
/// §1: every output (and hotplugged output) starts compact; state is not persisted.
pub const QUIRK_LAPTOP_BAR_STARTS_COMPACT: bool = true;
/// §1: one 500 ms debounce shared by every output (`lastBarToggle`).
pub const QUIRK_BAR_TOGGLE_GLOBAL_DEBOUNCE: bool = true;
/// §1.1: contentRoot.visible=false hides the island and calendar while compact.
pub const QUIRK_COMPACT_HIDES_ISLAND: bool = true;
/// §2: strip text has no renderType, so Qt draws it as distance-field text.
pub const QUIRK_COMPACT_STRIP_DF_TEXT: bool = true;
/// §2: the strip Wi-Fi glyph follows `wifiConnected` only (wired shows disconnected).
pub const QUIRK_STRIP_WIFI_IGNORES_ETHERNET: bool = true;
/// §2: the strip power text opens the battery popup even without a battery.
pub const QUIRK_STRIP_BATTERY_CLICK_UNGUARDED: bool = true;
/// §2: the 11×11 icon is the dark SVG rasterised at its 200 px intrinsic size.
pub const QUIRK_STRIP_ICON_FROM_200PX_SVG: bool = true;
/// §2: the strip clock click toggles the calendar even when the bar toggle is debounced.
pub const QUIRK_STRIP_CALENDAR_DEBOUNCE_GAP: bool = true;
/// §2.1: corner window input mask is the strip (top ceil(topCornerOffset) px).
pub const QUIRK_CORNER_MASK_FOLLOWS_STRIP: bool = true;
/// §6: no Viewflow windows button in the left pill.
pub const QUIRK_LAPTOP_NO_VIEWFLOW_BUTTON: bool = true;
/// §6.1, user decision 2026-10-24: keep main's `special:` filter on laptop too.
pub const QUIRK_LAPTOP_SPECIAL_WS_UNFILTERED: bool = false;
/// §8, user decision 2026-10-24: keep main's dynamic 58/820 surface on laptop.
pub const QUIRK_LAPTOP_TALL_BAR_SURFACE: bool = false;
/// §4: temperature in `--minimal` snapshots is empty; the tooltip keeps the last value.
pub const QUIRK_TEMP_STALE_IN_MINIMAL: bool = true;
/// §4: the tooltip's 21 px icon is distance-field text, its labels native.
pub const QUIRK_TOOLTIP_ICON_DF_TEXT: bool = true;
/// §5: 20.4 % shows "20%" but is not critical; the comparison is unrounded.
pub const QUIRK_BATTERY_CRITICAL_UNROUNDED: bool = true;
/// §1.1: the exclusive zone is recommitted whenever its rounded value changes.
pub const QUIRK_EXCLUSIVE_ZONE_PER_FRAME: bool = true;
