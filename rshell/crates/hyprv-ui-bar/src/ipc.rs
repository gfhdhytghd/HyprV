//! Read-only bar diagnostics for the host's core IPC dispatcher.
use crate::{layout, model::*, motion};
#[derive(Clone, Copy, Debug, Default)]
pub struct Diagnostics {
    pub spectrum: bool,
    pub panel_sampling: bool,
    pub battery_detail_sampling: bool,
    /// Laptop `anyBarExpanded` over every output (host-wide); None = this model only.
    pub any_bar_expanded: Option<bool>,
    /// Laptop `detailedStatsVisible` (any bar expanded or stats popup requested).
    pub detailed_stats: Option<bool>,
    /// Laptop live subscriptions (laptop-bar.md §11); desktop reports literals.
    pub wifi_scanning: bool,
    pub notifications_alive: Option<bool>,
    pub agent_watcher_alive: Option<bool>,
}
pub fn layout_status(m: Option<&Model>) -> String {
    match m{None=>"null".into(),Some(m)=>serde_json::json!({"expanded":!m.laptop()||m.expanded,"fullscreen":m.fullscreen,"reveal":m.reveal,"fullscreenHide":m.fullscreen_hide,"compactOffset":-15.*m.fullscreen_hide,"topCornerOffset":m.top_corner_offset(),"bottomCornerOffset":0,"presented":m.fullscreen_hide<1.,"duration":m.desktop.move_duration_ms,"curve":m.desktop.move_bezier}).to_string()}
}
pub fn input_status(m: Option<&Model>) -> String {
    match m {
        None => "null".into(),
        Some(m) => {
            let l = layout::compute(m, &m.theme);
            serde_json::json!({"monitor":m.output.name,"reveal":m.reveal,"contentY":l.content_y,"inputX":0,"inputY":l.content_y.floor(),"inputWidth":m.output.width.ceil(),"inputHeight":58}).to_string()
        }
    }
}
pub fn data_status(m: &Model, d: Diagnostics) -> String {
    let laptop = m.laptop();
    let expanded = !laptop || d.any_bar_expanded.unwrap_or(m.expanded);
    let detailed = !laptop || d.detailed_stats.unwrap_or(expanded);
    serde_json::json!({"expanded":expanded,"systemInterval":if detailed {1000} else {5000},"detailedStats":detailed,"nativeAudio":m.audio.available,"volume":m.audio.volume_percent,"nativeWifi":true,"wifiConnected":m.network.connected,"wifiScanning":laptop&&d.wifi_scanning,"wifiNetworkCount":m.network.networks.len(),"networkInterface":m.stats.iface.as_deref().unwrap_or(""),"dnd":m.dnd(),"cpu":m.stats.cpu_percent,"memory":m.stats.memory_percent,"networkRxRate":m.stats.rx_bytes_per_s,"networkTxRate":m.stats.tx_bytes_per_s,"mediaPlayer":m.media.player_name,"mediaAvailable":m.media.available,"mediaPlaying":m.media.playing,"mediaPosition":m.media.position_seconds(m.monotonic_us),"spectrum":d.spectrum,"notifications":!laptop||d.notifications_alive.unwrap_or(true),"agentWatcher":!laptop||d.agent_watcher_alive.unwrap_or(true),"powerProfile":m.power.profile.name(),"batteryAvailable":m.power.battery_present,"batteryPercent":m.power.battery_percent,"darkMode":m.theme.dark,"panelPolling":d.panel_sampling,"batteryDetailPolling":d.battery_detail_sampling}).to_string()
}
pub fn toggle(m: &mut Model) {
    motion::toggle_bar(m);
}
/// Laptop `toggleBar` debounce (laptop-bar.md §1, QUIRK_BAR_TOGGLE_GLOBAL_DEBOUNCE):
/// one gate for all outputs, keyed on wall-clock milliseconds (`Date.now()`).
#[derive(Clone, Copy, Debug, Default)]
pub struct ToggleGate {
    last_ms: Option<u64>,
}
impl ToggleGate {
    pub const DEBOUNCE_MS: u64 = 500;
    /// True when a toggle at `now_ms` may proceed; it then becomes the last toggle.
    pub fn allow(&mut self, now_ms: u64) -> bool {
        if self
            .last_ms
            .is_some_and(|last| now_ms.saturating_sub(last) < Self::DEBOUNCE_MS)
        {
            return false;
        }
        self.last_ms = Some(now_ms);
        true
    }
}
