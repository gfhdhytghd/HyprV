//! Conversions from hyprv-svc-* State types into panel view models, and from panel
//! [`Action`]s into service Action enums (feature `services`).
use crate::model::{self as m, Action, HostAction};
use hyprv_svc_desktop as desk;
use hyprv_svc_media as media;
use hyprv_svc_net as net;
use hyprv_svc_power as power;
use hyprv_svc_sys as sys;

/// `isWifiInterfaceName` / `isWiredInterfaceName` (shell.qml:474-490).
pub fn net_type(iface: &str) -> m::NetType {
    let i = iface.to_lowercase();
    if i.starts_with("wl") || i.starts_with("wlan") || i.starts_with("wifi") {
        m::NetType::Wifi
    } else if i.starts_with("en") || i.starts_with("eth") {
        m::NetType::Wired
    } else if i.is_empty() {
        m::NetType::Offline
    } else {
        m::NetType::Other
    }
}

pub fn network(s: &net::network::State) -> m::Network {
    let default_interface = s.default_iface.clone().unwrap_or_default();
    m::Network {
        active_type: net_type(&default_interface),
        default_interface,
        wifi_device_present: s.present,
        wifi_capability_detected: s.available,
        wifi_radio_enabled: s.enabled,
        wifi_hardware_enabled: s.hardware_enabled,
        wifi_connected: s.connected,
        wifi_ssid: s.ssid.clone(),
        wifi_signal_strength: s.signal as f32 / 100.,
        wifi_secure: !s.security.is_empty(),
        wifi_interface: s.iface.clone(),
        networks: s
            .networks
            .iter()
            .map(|n| m::WifiNetwork { ssid: n.ssid.clone(), signal: n.signal, security: n.security.clone(), secure: n.secure, known: n.known, active: n.active, enterprise: n.enterprise })
            .collect(),
        action_busy: s.busy,
        action_message: s.message.clone(),
    }
}

pub fn bluetooth(s: &net::bluetooth::State) -> m::Bluetooth {
    m::Bluetooth {
        present: s.present,
        enabled: s.enabled,
        discovering: s.discovering,
        pairable: s.pairable,
        busy: s.busy,
        action_message: s.message.clone(),
        devices: s
            .devices
            .iter()
            .map(|d| m::BtDevice {
                address: d.address.clone(),
                name: d.name.clone(),
                display_name: d.display_name.clone(),
                has_name: d.has_name,
                icon: d.icon.clone(),
                paired: d.paired,
                trusted: d.trusted,
                connected: d.connected,
                blocked: d.blocked,
                rssi: d.rssi,
            })
            .collect(),
    }
}

pub fn audio(s: &media::audio::AudioState) -> m::Audio {
    m::Audio { available: s.available, muted: s.muted, volume_percent: s.volume_percent }
}

/// Album art is decoded by the host and set on `Media::art`.
pub fn media(s: &media::mpris::MprisState) -> m::Media {
    m::Media { available: s.available, playing: s.playing, title: s.title.clone(), artist: s.artist.clone(), player_name: s.player_name.clone(), art_url: s.art_url.clone(), art: None }
}

pub fn profile(p: &power::power::PowerProfile) -> m::Profile {
    match p {
        power::power::PowerProfile::PowerSaver => m::Profile::PowerSaver,
        power::power::PowerProfile::Performance => m::Profile::Performance,
        _ => m::Profile::Balanced,
    }
}

/// Fill the control-panel fields that come from services. `output` selects the
/// backlight output (the primary bar's output).
#[allow(clippy::too_many_arguments)]
pub fn controls(
    into: &mut m::Controls,
    power_state: &power::power::State,
    backlight: &power::backlight::State,
    output: &str,
    fan: &power::fan::State,
    inhibit: &power::inhibit::State,
    demo: &desk::demo::DemoState,
    notifications: &media::notifications::NotificationState,
    vm: &sys::vm::State,
    viewflow: &desk::viewflow::ViewflowState,
    mihomo: &sys::mihomo::State,
) {
    into.profile = profile(&power_state.profile);
    if let Some(o) = backlight.outputs.iter().find(|o| o.output == output).or(backlight.outputs.first()) {
        into.brightness_percent = o.level_percent.round().clamp(0., 100.) as i32;
    }
    into.fan_available = fan.available;
    for c in &fan.channels {
        match c.channel {
            hyprv_privhelper::FanChannel::Chassis => {
                into.chassis_fan_percent = c.limit_percent as i32;
                into.chassis_fan_rpm = c.rpm.unwrap_or(0);
            }
            hyprv_privhelper::FanChannel::Pump => {
                into.pump_fan_percent = c.limit_percent as i32;
                into.pump_fan_rpm = c.rpm.unwrap_or(0);
            }
        }
    }
    into.prevent_sleep = inhibit.owners.contains(&power::inhibit::Owner::Manual);
    into.demo_mode = demo.enabled;
    into.dnd = notifications.dnd || notifications.alt.contains("dnd");
    into.vm_running = vm.running;
    into.vm_busy = vm.busy;
    into.viewflow_target = if viewflow.target == desk::Target::MacOs { m::Target::MacOs } else { m::Target::Windows };
    into.viewflow_busy = viewflow.busy;
    into.mihomo = m::Mihomo {
        online: mihomo.online,
        service: mihomo.legacy_service_scope.clone(),
        version: mihomo.version.clone(),
        mode: mihomo.mode.clone(),
        connections: mihomo.connections,
        upload: mihomo.upload_bytes_per_s,
        download: mihomo.download_bytes_per_s,
        dashboard: mihomo.dashboard_url.clone(),
        // runAction in flight ≈ a queued systemd job on the unit (start/stop); HTTP mode
        // switches have no service-side busy state
        busy: mihomo.unit.as_ref().is_some_and(|u| u.job.is_some()),
        message: mihomo.last_error.as_ref().map(|e| format!("{e:?}")).unwrap_or_default(),
    };
}

pub fn stats(s: &power::sysstats::State) -> m::Stats {
    let hist = |h: &power::sysstats::Series<120>| h.iter().map(|v| v.unwrap_or(0.).max(0.)).collect();
    m::Stats {
        cpu_usage: s.cpu_percent.unwrap_or(0.),
        memory_usage: s.memory_percent.unwrap_or(0.),
        cpu_core_usages: s.core_percent.iter().map(|v| v.unwrap_or(0.)).collect(),
        default_interface: s.iface.as_deref().unwrap_or_default().to_owned(),
        rx_rate: s.rx_bytes_per_s.unwrap_or(0.),
        tx_rate: s.tx_bytes_per_s.unwrap_or(0.),
        cpu_history: hist(&s.cpu_history),
        memory_history: hist(&s.memory_history),
        network_history: hist(&s.network_history),
    }
}

/// shell.qml instantiates `CalendarBridge { weekStart: new Date() }`; only `loadWeek()`
/// (which needs the socket) snaps it to Monday. Offline, the week therefore starts today
/// (an empty `week_start` makes [`crate::calendar`] use the wall date).
pub const QUIRK_CALENDAR_OFFLINE_WEEK_STARTS_TODAY: bool = true;

pub fn calendar(s: &sys::calendar::State) -> m::Calendar {
    // CalendarBridge.lastError: 「日历服务未连接」 only from sendRequest with the link down
    // (the service's offline action error); socket discovery failure leaves it empty, so
    // the pill reads 「正在连接日历…」. Connected: the daemon's error message.
    let last_error = match &s.last_error {
        Some(e) if s.connected => e.message.clone(),
        Some(e) if e.code == "unavailable" => "日历服务未连接".into(),
        _ => String::new(),
    };
    let week_start = if s.connected || !QUIRK_CALENDAR_OFFLINE_WEEK_STARTS_TODAY {
        s.week_start_local.get(..10).unwrap_or_default().to_string()
    } else {
        String::new()
    };
    m::Calendar {
        bridge: true,
        connected: s.connected,
        last_error,
        week_start,
        calendars: s.calendars.iter().map(|c| m::CalendarInfo { id: c.id.clone(), name: c.name.clone(), color: c.color.clone(), hidden: c.hidden || !c.visible, holds_tasks: c.holds_tasks }).collect(),
        events: s
            .events
            .iter()
            .map(|e| m::CalendarEvent {
                uid: e.uid.clone(),
                calendar_id: e.calendar_id.clone(),
                summary: e.title.clone(),
                start: e.start_iso.clone(),
                end: e.end_iso.clone(),
                all_day: e.all_day,
                location: e.location.clone().unwrap_or_default(),
                // not carried by the service state (yet): the tooltip line stays hidden
                description: String::new(),
            })
            .collect(),
    }
}

/// A panel action routed to its owning service.
pub enum ServiceAction {
    Network(net::network::Action),
    Bluetooth(net::bluetooth::Action),
    Audio(media::audio::AudioAction),
    Mpris(media::mpris::MprisAction),
    Notification(media::notifications::NotificationAction),
    Backlight(power::backlight::Action),
    Fan(power::fan::Action),
    Power(power::power::Action),
    Inhibit(power::inhibit::Action),
    Demo(desk::demo::DemoAction),
    Vm(sys::vm::Action),
    Viewflow(desk::viewflow::ViewflowAction),
    Mihomo(sys::mihomo::Action),
    Calendar(sys::calendar::Action),
    /// Not a service action (recording, theme, managers, popups…): the host handles it.
    Host(HostAction),
    /// Recording / theme toggles: their actors are not yet exported by hyprv-svc-desktop.
    Unrouted(Action),
}

/// Map an action. `backlight_output` and `sequence` feed the backlight protocol.
pub fn route(a: &Action, backlight_output: &str, sequence: u64) -> ServiceAction {
    use ServiceAction as S;
    // by reference: Action zeroizes its password on drop, so fields are copied out
    match a {
        Action::WifiSetRadio(v) => S::Network(net::network::Action::SetRadio(*v)),
        Action::WifiRescan => S::Network(net::network::Action::Rescan),
        Action::WifiDisconnect => S::Network(net::network::Action::Disconnect),
        Action::WifiConnect { ssid, password, security } => S::Network(net::network::Action::Connect { ssid: ssid.clone(), password: (!password.is_empty()).then(|| password.clone()), security: security.clone(), enterprise: None }),
        Action::BluetoothSetPower(v) => S::Bluetooth(net::bluetooth::Action::SetPower(*v)),
        Action::BluetoothScan => S::Bluetooth(net::bluetooth::Action::Scan),
        Action::BluetoothConnect { address, paired } => S::Bluetooth(net::bluetooth::Action::Connect { address: address.clone(), paired: *paired }),
        Action::BluetoothDisconnect { address } => S::Bluetooth(net::bluetooth::Action::Disconnect { address: address.clone() }),
        Action::SetVolume(v) => S::Audio(media::audio::AudioAction::SetVolume(*v as f64)),
        Action::ToggleMute => S::Audio(media::audio::AudioAction::ToggleMute),
        Action::MediaPrevious => S::Mpris(media::mpris::MprisAction::Previous),
        Action::MediaPlayPause => S::Mpris(media::mpris::MprisAction::PlayPause),
        Action::MediaNext => S::Mpris(media::mpris::MprisAction::Next),
        Action::BrightnessSet(p) => S::Backlight(power::backlight::Action::Set { output: backlight_output.into(), percent: *p as f64, sequence }),
        Action::BrightnessFinish(_) => S::Backlight(power::backlight::Action::Finish { output: backlight_output.into(), sequence }),
        Action::FanLimit { channel, percent } => S::Fan(power::fan::Action::SetLimit {
            channel: match channel {
                m::FanChannel::Chassis => hyprv_privhelper::FanChannel::Chassis,
                m::FanChannel::Pump => hyprv_privhelper::FanChannel::Pump,
            },
            percent: (*percent).clamp(0, 100) as u8,
        }),
        Action::ProfileSet(p) => S::Power(power::power::Action::ProfileSet(match p {
            m::Profile::PowerSaver => power::power::PowerProfile::PowerSaver,
            m::Profile::Balanced => power::power::PowerProfile::Balanced,
            m::Profile::Performance => power::power::PowerProfile::Performance,
        })),
        Action::ToggleDnd => S::Notification(media::notifications::NotificationAction::ToggleDnd),
        Action::TogglePreventSleep => S::Inhibit(power::inhibit::Action::Toggle(power::inhibit::Owner::Manual)),
        Action::ToggleDemo => S::Demo(desk::demo::DemoAction::Toggle),
        Action::VmToggle => S::Vm(sys::vm::Action::Toggle),
        Action::ViewflowSet(t) => S::Viewflow(desk::viewflow::ViewflowAction::Set(if *t == m::Target::MacOs { desk::Target::MacOs } else { desk::Target::Windows })),
        Action::MihomoStart => S::Mihomo(sys::mihomo::Action::Service(sys::systemd::UnitOperation::Start)),
        Action::MihomoStop => S::Mihomo(sys::mihomo::Action::Service(sys::systemd::UnitOperation::Stop)),
        Action::MihomoMode(mode) => S::Mihomo(sys::mihomo::Action::Mode(mode.clone())),
        Action::MihomoRefresh => S::Mihomo(sys::mihomo::Action::Refresh),
        Action::MihomoOpenDashboard => S::Mihomo(sys::mihomo::Action::OpenDashboard),
        Action::CalendarPreviousWeek => S::Calendar(sys::calendar::Action::PreviousWeek),
        Action::CalendarNextWeek => S::Calendar(sys::calendar::Action::NextWeek),
        Action::CalendarOpenEvent { uid, start } => S::Calendar(sys::calendar::Action::OpenEvent { uid: uid.clone(), start_iso: start.clone() }),
        Action::CalendarNewEvent => S::Calendar(sys::calendar::Action::NewEvent),
        Action::CalendarShowFullApp => S::Calendar(sys::calendar::Action::Show),
        Action::Host(h) => S::Host(h.clone()),
        Action::ToggleRecording | Action::ToggleTheme => S::Unrouted(a.clone()),
    }
}
