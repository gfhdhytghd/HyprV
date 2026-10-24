# Proposed hyprv-services API

Companion to [services.md](services.md). This is an implementation proposal, not an existing crate API. Source wire formats remain in that audit; actors publish native Rust data rather than legacy shell text. Core owns the store, topic subscriptions, IPC and calloop bridge; services owns provider protocols and transformations. UI owns strings, icons, popup visibility and animation. Internal APIs do not imply public IPC authorization.

## Modules and event sources

| Module | Actor, source and action ownership |
|---|---|
| `network` | NetworkManager system bus, connections/APs/SecretAgent; radio, scan, connect/disconnect, secrets and enterprise prompts. |
| `bluetooth` | BlueZ ObjectManager/Agent1 plus rfkill; adapter power, bounded discovery, pair/trust/connect/disconnect/remove. |
| `audio` | PipeWire registry/default metadata/SPA parameters; volume and mute. |
| `spectrum` | Demand-only PipeWire monitor stream and FFT, at most24Hz, releases capture while hidden. |
| `mpris` | Session bus owner/property/Seeked events, selection/capabilities, local position anchor and playback/seek/raise. |
| `cider` | Track-keyed HTTP compatibility/cache adapter; metadata-triggered and30s demand fallback only. |
| `tray` | StatusNotifier host/watcher and per-item/dbusmenu subscriptions/actions. |
| `notifications` | SwayNC introspected native protocol; state, DND/visibility, CSS refresh; freedesktop Notify for shell messages. |
| `power` | UPower, power profiles, logind suspend; profile changes and battery state. |
| `battery_history` | Legacy1800s signed-power window/import, detail sampling lease and estimates. |
| `backlight` | Per-output sysfs/DDC plus Hyprland HDR controls; authoritative hardware/SDR curve, sequencing/readback. |
| `sysstats` | Shared demand sampler for proc/sys/RAPL; rtnetlink routes, udev sensor discovery, bounded histories. |
| `fan` | Client of persistent independently owned privileged cooling controller; limit commands and safety telemetry. |
| `systemd` | Shared bus connection, Subscribe/Unit/Job lifecycle; system/user scope explicitly typed. |
| `vm` | windows-autoshop.service start/stop jobs and auth handoff; action lifetime independent of panel. |
| `mihomo` | Config watcher, HTTP settings/actions; traffic/connections streaming; service manager adapter. |
| `hyprland` | Native request socket +socket2 events, geometry/workspaces/keymap/config/motion; no hyprctl subprocess. |
| `calendar` | dcal native discovery integration, two NDJSON sockets, request IDs, subscription and week queries. |
| `weather` | OpenWeather HTTP, .env/cache watch, visible-demand freshness deadline. |
| `agent_island` | Observation hooks/file/transcript/process events, expiry, display/focus only. |
| `deskflow` | Config generation, cgroup/pidfd/SIGHUP/log acknowledgement and independent IR action. |
| `viewflow` | Native discovery integration, units/config/readiness/supervisor and separate HDMI/input observations. |
| `ir` | tty udev +serialized115200baud newline JSON protocol; database and learn/send. |
| `egpu` | udev pci/thunderbolt inventory and authenticated validated removal. |
| `inhibit` | logind Inhibit FD ownership, distinct manual/demo leases. |
| `demo` | Compositor monitor/headless/mirror transaction and rollback journal. |
| `recording` | Managed recording lifecycle, optional visible foreign-recorder discovery. |
| `theme` | State watch, native desktop theme application and per-app partial results. |
| `applications` | XDG desktop-file index/watch; app activation, lock/logout/manager and launcher integration. |
| `startup` | Explicit approval-file transaction for startup prompt. |
| `protocol` | Shared action IDs, availability/error types, native adapters and helper wire schema. |

Do not copy global timers per screen. `SamplingLease` below is granted/revoked through core subscriber lifecycle. Needed backend initialization may outlive subscriptions; data collection timers do not. Provider retries exist only while desired connection is absent, with exponential backoff and jitter. A reconnect first invalidates stale state, subscribes, then snapshots; avoid a state-update feedback loop.

## Shared types and lifecycle

Illustrative Rust uses only standard-library types below; provider-specific IDs retain their original identity. Internal f64 units appear in field names. A production implementation should add validation/newtypes and bounded strings/collections at each untrusted protocol boundary.

```rust
use std::{collections::BTreeMap, path::PathBuf, time::{Duration, Instant}};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ObjectId(pub String); // qualified bus owner + object path, or provider ID
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RequestId(pub u64);
#[derive(Clone, Debug)]
pub enum Availability { Available, Absent, Connecting, Unsupported, Failed(ServiceError) }
#[derive(Clone, Debug)]
pub struct ServiceError { pub code: String, pub message: String, pub retryable: bool }
#[derive(Clone, Debug)]
pub struct Observed<T> {
    pub generation: u64, pub availability: Availability,
    pub value: Option<T>, pub observed_at: Instant,
}
#[derive(Clone, Debug)]
pub enum ActionPhase { Queued, AwaitingUiRelease, AwaitingAuthorization, Running, Succeeded, Failed(ServiceError), Cancelled }
#[derive(Clone, Debug)]
pub struct ActionStatus { pub id: RequestId, pub phase: ActionPhase }
#[derive(Clone, Debug)]
pub enum Metric { CpuTotal, CpuCores, Memory, Network, Temperature, CpuPackagePower, BatteryPower, FanRpm }
#[derive(Clone, Debug)]
pub struct SamplingLease { pub subscriber: u64, pub metrics: Vec<Metric>, pub period: Duration }
#[derive(Clone, Debug)]
pub enum ServiceEvent {
    Network(Observed<NetworkState>), Bluetooth(Observed<BluetoothState>),
    Audio(Observed<AudioState>), Media(Observed<MediaState>),
    Spectrum(Observed<SpectrumState>), Tray(Observed<TrayState>),
    Notifications(Observed<NotificationState>), Power(Observed<PowerState>),
    Battery(Observed<BatteryDetail>), Brightness(Observed<BrightnessState>),
    Stats(Observed<SystemStats>), Fan(Observed<FanState>), Vm(Observed<VmState>),
    Proxy(Observed<MihomoState>), Desktop(Observed<DesktopState>),
    Calendar(Observed<CalendarState>), Weather(Observed<WeatherState>),
    Agents(Observed<AgentIslandState>), Targets(Observed<TargetState>),
    Ir(Observed<IrState>), Egpu(Observed<EgpuState>),
    Inhibit(Observed<InhibitState>), Demo(Observed<DemoState>),
    Recording(Observed<RecordingState>), Theme(Observed<ThemeState>),
    Applications(Observed<ApplicationState>), Startup(Observed<StartupState>),
    Action(ActionStatus), Prompt(Prompt),
}
#[derive(Clone, Debug)]
pub enum Prompt {
    NetworkSecrets { request: RequestId, connection: ObjectId, setting: String, hints: Vec<String> },
    Bluetooth { request: RequestId, device: ObjectId, kind: PairPrompt },
    ReleaseInput { request: RequestId },
}
#[derive(Clone, Debug)]
pub enum PairPrompt { Pin, Passkey, DisplayPin(String), DisplayPasskey { value: u32, entered: u16 }, Confirm(u32), Authorize(String) }
```

Values are nullable through Option or Availability; do not turn read errors into valid zero samples. `observed_at` is monotonic; legacy persistence carries epoch timestamps separately. Rate baseline reset/suspend advances generation. Action results do not optimistically overwrite observed hardware state. Logind PrepareForSleep(true) suspends non-safety sampling; false resnapshots buses and resets counters/position anchors.

## Network, Bluetooth, audio and media

```rust
#[derive(Clone, Debug)]
pub struct WifiNetwork {
    pub id: ObjectId, pub ssid: Vec<u8>, pub display_ssid: String,
    pub strength_percent: u8, pub security: String, pub secure: bool,
    pub enterprise: bool, pub active: bool, pub known: bool,
}
#[derive(Clone, Debug)]
pub struct NetworkState {
    pub iface: Option<String>, pub wifi_enabled: bool, pub wifi_hardware_enabled: bool,
    pub connected: bool, pub active_ap: Option<ObjectId>, pub networks: Vec<WifiNetwork>,
    pub default_iface: Option<String>, pub wired_connected: bool, pub other_connected: bool,
    pub scanning: bool,
}
#[derive(Clone, Debug)]
pub struct BluetoothDevice {
    pub id: ObjectId, pub address: String, pub name: String, pub icon: Option<String>,
    pub paired: bool, pub bonded: bool, pub trusted: bool, pub blocked: bool,
    pub connected: bool, pub pairing: bool,
}
#[derive(Clone, Debug)]
pub struct BluetoothState {
    pub adapter: Option<ObjectId>, pub powered: bool, pub hardware_blocked: bool,
    pub discovering: bool, pub pairable: bool, pub devices: Vec<BluetoothDevice>,
}
#[derive(Clone, Debug)]
pub struct AudioState { pub sink: Option<ObjectId>, pub muted: bool, pub volume_percent: f64, pub channel_volumes: Vec<f32> }
#[derive(Clone, Debug)]
pub struct SpectrumState { pub bars: [f32; 16] } // normalized0..100 at UI boundary
#[derive(Clone, Debug)]
pub enum Playback { Playing, Paused, Stopped }
#[derive(Clone, Debug)]
pub struct PositionAnchor { pub position_us: i64, pub sampled_at: Instant, pub rate: f64, pub playback: Playback }
#[derive(Clone, Debug)]
pub struct MediaCapabilities { pub play: bool, pub pause: bool, pub next: bool, pub previous: bool, pub seek: bool, pub raise: bool }
#[derive(Clone, Debug)]
pub struct MediaPlayer {
    pub id: ObjectId, pub identity: String, pub desktop_entry: Option<String>,
    pub track_id: Option<ObjectId>, pub title: String, pub artists: Vec<String>,
    pub album: String, pub art_url: Option<String>, pub length_us: Option<i64>,
    pub position: PositionAnchor, pub capabilities: MediaCapabilities, pub cider: bool,
}
#[derive(Clone, Debug)]
pub struct MediaState { pub selected: Option<ObjectId>, pub players: Vec<MediaPlayer> }
#[derive(Clone, Debug)]
pub struct NotificationState { pub alt: String, pub tooltip: String, pub dnd: bool, pub panel_visible: Option<bool>, pub count: Option<u32> }
#[derive(Clone, Debug)]
pub struct TrayPixmap { pub width: u32, pub height: u32, pub argb: Vec<u8> }
#[derive(Clone, Debug)]
pub struct TrayMenuEntry { pub id: i32, pub label: String, pub enabled: bool, pub visible: bool, pub toggle: Option<bool>, pub children: Vec<TrayMenuEntry> }
#[derive(Clone, Debug)]
pub struct TrayItem {
    pub id: ObjectId, pub title: String, pub status: String,
    pub icon_name: Option<String>, pub icon_pixmaps: Vec<TrayPixmap>,
    pub attention_icon: Option<String>, pub overlay_icon: Option<String>,
    pub tooltip_title: String, pub tooltip_description: String,
    pub menu: Option<ObjectId>, pub menu_revision: u32, pub menu_entries: Vec<TrayMenuEntry>,
}
#[derive(Clone, Debug)]
pub struct TrayState { pub items: Vec<TrayItem> }
```

NM credentials must not derive Debug or enter state snapshots. Store secret buffers in `zeroize::Zeroizing`, send only via dedicated prompt reply channel; serialize secrets neither to diagnostics nor public IPC responses. Pairing requests expire/cancel with provider requests. Byte SSIDs avoid lossy identity; display labels may use UTF-8 fallback. Artwork fetch is cancellable/keyed by track, cached and size bounded. Menu traversal is revisioned and bounded.

## Power, cooling, telemetry and system jobs

```rust
#[derive(Clone, Debug)]
pub enum BatteryMode { Unknown, Charging, Discharging, Full, Idle, Plugged }
#[derive(Clone, Debug)]
pub enum PowerProfile { PowerSaver, Balanced, Performance, Other(String) }
#[derive(Clone, Debug)]
pub struct PowerState {
    pub on_battery: bool, pub battery_present: bool, pub battery_percent: Option<f64>,
    pub battery_mode: BatteryMode, pub profile: PowerProfile,
    pub profiles: Vec<PowerProfile>, pub performance_degraded: Option<String>, pub sleeping: bool,
}
#[derive(Clone, Debug)]
pub struct BatteryDetail {
    pub status: String, pub mode: BatteryMode, pub capacity_percent: f64,
    pub power_w: Option<f64>, pub average_power_w: Option<f64>,
    pub samples: u32, pub window_s: u32, pub window_complete: bool,
    pub estimate_s: Option<f64>, pub estimate_basis: String,
    pub energy_now_wh: Option<f64>, pub energy_full_wh: Option<f64>,
}
#[derive(Clone, Debug)]
pub enum BacklightBackend { Sysfs { device: PathBuf }, Ddc { connector: String, bus: u32 } }
#[derive(Clone, Debug)]
pub struct BrightnessState {
    pub output: String, pub backend: BacklightBackend, pub level_percent: f64,
    pub raw: Option<u32>, pub raw_max: Option<u32>, pub reference_luminance_nits: f64,
    pub request_sequence: u64, pub confirmed_sequence: Option<u64>, pub dragging: bool,
}
#[derive(Clone, Debug)]
pub struct SystemStats {
    pub cpu_percent: Option<f64>, pub core_percent: Vec<Option<f64>>,
    pub memory_total_bytes: u64, pub memory_available_bytes: u64,
    pub temperature_c: Option<f64>, pub package_power_w: Option<f64>,
    pub iface: Option<String>, pub rx_bytes: u64, pub tx_bytes: u64,
    pub rx_bytes_per_s: Option<f64>, pub tx_bytes_per_s: Option<f64>,
    pub elapsed: Duration,
}
#[derive(Clone, Copy, Debug)]
pub enum FanChannel { Chassis, Pump }
#[derive(Clone, Debug)]
pub struct FanReading { pub channel: FanChannel, pub limit_percent: u8, pub pwm: u8, pub rpm: Option<u32> }
#[derive(Clone, Debug)]
pub struct FanState { pub chip: String, pub temperature_c: Option<f64>, pub desktop_policy: bool, pub channels: Vec<FanReading> }
#[derive(Clone, Copy, Debug)]
pub enum UnitScope { User, System }
#[derive(Clone, Debug)]
pub struct UnitState { pub name: String, pub scope: UnitScope, pub active_state: String, pub sub_state: String, pub job: Option<u32> }
#[derive(Clone, Debug)]
pub struct VmState { pub unit: UnitState, pub running: bool, pub action: Option<RequestId> }
#[derive(Clone, Debug)]
pub struct MihomoState {
    pub online: bool, pub unit: Option<UnitState>, pub legacy_service_scope: String,
    pub version: String, pub mode: String, pub connections: u64,
    pub upload_bytes_per_s: u64, pub download_bytes_per_s: u64, pub dashboard_url: String,
}
```

Helper API proposal: system bus `org.hyprv.PrivHelper1`, object `/org/hyprv/PrivHelper1`, interface `org.hyprv.PrivHelper1`. Typed methods `SetFanLimit(channel enum,percent u8)`, `GetFanState`, `RemoveExternalGpu(validated BDF token)`, optional `ReadPackageEnergy`/`SetBacklight(output ID,raw)`; signals FanStateChanged/DeviceRemoved/OperationCompleted. These names are **new**, not discovered existing services. Each mutating operation has its own polkit action, caller UID/session check, hardware identity revalidation, bounded data and no caller-specified shell/path. Cooling controller lifetime cannot depend on an open D-Bus client. Privhelper protocol changes require version handshake; deploy file ownership/early boot independently.

Mihomo `/traffic` and `/connections` native stream subscriptions close with last live-rate subscriber; /configs has no proven change push in source. Fetch at connect, own-action completion and visible refresh, and use a2s visible fallback only if external configuration edits must remain observable. A WebSocket endpoint must be capability-tested; support source's HTTP NDJSON stream where available. Read YAML controller/secret without the source regex's quoting limitations; HTTP secrets never appear in MihomoState.

## Desktop, calendar, weather and agents

```rust
#[derive(Clone, Debug)]
pub struct Rect { pub x: f64, pub y: f64, pub width: f64, pub height: f64 }
#[derive(Clone, Debug)]
pub struct DesktopWindow { pub address: String, pub pid: Option<u32>, pub class: String, pub title: String, pub monitor: i32, pub geometry: Rect, pub fullscreen: bool }
#[derive(Clone, Debug)]
pub struct DesktopMonitor { pub name: String, pub id: i32, pub geometry: Rect, pub scale: f64, pub transform: u8 }
#[derive(Clone, Debug)]
pub struct DesktopState { pub active_window: Option<String>, pub windows: Vec<DesktopWindow>, pub monitors: Vec<DesktopMonitor>, pub workspaces: Vec<Workspace>, pub keymap: String, pub move_duration_ms: u32, pub move_bezier: [f64; 6] }
#[derive(Clone, Debug)]
pub struct Workspace { pub id: i32, pub name: String, pub monitor: String, pub active: bool, pub windows: u32 }
#[derive(Clone, Debug)]
pub struct CalendarInfo { pub id: String, pub name: String, pub color: Option<String>, pub visible: bool }
#[derive(Clone, Debug)]
pub struct CalendarEvent { pub uid: String, pub calendar_id: String, pub title: String, pub start_iso: String, pub end_iso: String, pub all_day: bool, pub color: Option<String>, pub location: Option<String> }
#[derive(Clone, Debug)]
pub struct CalendarState { pub socket: Option<PathBuf>, pub connected: bool, pub subscribed: bool, pub loading: bool, pub week_start_local: String, pub request_serial: u64, pub calendars: Vec<CalendarInfo>, pub events: Vec<CalendarEvent>, pub last_error: Option<String> }
#[derive(Clone, Debug)]
pub enum TemperatureUnit { Celsius, Fahrenheit, Kelvin }
#[derive(Clone, Debug)]
pub struct WeatherHour { pub local_time: String, pub temperature: f64, pub icon: String, pub color: String }
#[derive(Clone, Debug)]
pub struct WeatherDay { pub id: String, pub local_date: String, pub weekday: String, pub high: f64, pub low: f64, pub feels_like: f64, pub wind: f64, pub humidity_percent: f64, pub precipitation_percent: f64, pub icon: String, pub color: String, pub description: String, pub hours: Vec<WeatherHour> }
#[derive(Clone, Debug)]
pub struct WeatherState { pub unit: TemperatureUnit, pub current_temperature: Option<f64>, pub current_icon: String, pub current_color: String, pub days: Vec<WeatherDay>, pub dummy: bool, pub stale: bool, pub selected_day: u8 }
#[derive(Clone, Debug)]
pub enum AgentStatus { Idle, Processing, Running, WaitingApproval, WaitingQuestion, Completed, Other(String) }
#[derive(Clone, Debug)]
pub struct AgentSession { pub id: String, pub source: String, pub source_label: String, pub title: String, pub project: String, pub status: AgentStatus, pub current_tool: String, pub detail: String, pub base_progress: f64, pub activity_epoch_s: f64 }
#[derive(Clone, Debug)]
pub struct AgentIslandState { pub active: bool, pub count: u32, pub sessions: Vec<AgentSession> }
```

CalendarInfo/CalendarEvent are normalized internal models, not claims about wire spelling. The adapter must map the exact QML field accesses in services.md (including snake_case, date variants and nested calendar attributes). Preserve timezone/offset/all-day dates, recurrence identity uid+start; do not collapse date-only events into UTC midnight. Pending callbacks keyed by ID are actor-private with explicit timeout/cancel on disconnect. Visibility controls expensive week loading, not subscription correctness.

Agent focus metadata (PID, terminal identity, Kitty socket/window ID, transcript paths) stays actor-private and is never shipped to the display list. `FocusSession(id)` is the only UI action. No command for approve/reject/answer/type exists. Progress presentation uses the source's phase from activity time, UI frame callbacks only when animated. Hook state version1 import retains unknown fields until migration completes; output display state remains minimal.

## Target routing, devices, theme and actions

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target { Windows, MacOs }
#[derive(Clone, Debug)]
pub struct TargetState { pub deskflow_selected: Option<Target>, pub hdmi_selected: Option<Target>, pub input_route: Option<Target>, pub peer_ready: bool, pub topology_supervised: bool, pub action: Option<RequestId>, pub partial_error: Option<ServiceError> }
#[derive(Clone, Debug)]
pub struct IrButton { pub name: String, pub captured_at: String, pub source_pin: u8, pub polarity: String, pub durations_us: Vec<u32> }
#[derive(Clone, Debug)]
pub struct IrState { pub device: Option<PathBuf>, pub carrier_hz: u32, pub duty_cycle: f64, pub buttons: Vec<IrButton>, pub busy: bool }
#[derive(Clone, Debug)]
pub struct EgpuDevice { pub bdf: String, pub label: String, pub externally_verified: bool, pub removal_chain: Vec<String> }
#[derive(Clone, Debug)]
pub struct EgpuState { pub devices: Vec<EgpuDevice> }
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum InhibitOwner { Manual, Demo }
#[derive(Clone, Debug)]
pub struct InhibitState { pub owners: Vec<InhibitOwner>, pub held: bool }
#[derive(Clone, Debug)]
pub struct DemoState { pub enabled: bool, pub session: String, pub source: Option<String>, pub headless: Option<String>, pub created_headless: bool, pub acquired_inhibitor: bool }
#[derive(Clone, Debug)]
pub struct Recorder { pub pid: u32, pub name: String, pub managed: bool }
#[derive(Clone, Debug)]
pub struct RecordingState { pub active: bool, pub recorders: Vec<Recorder> }
#[derive(Clone, Copy, Debug)]
pub enum Theme { Light, Dark }
#[derive(Clone, Copy, Debug)]
pub enum Variant { V1, V2, V3, Bt }
#[derive(Clone, Debug)]
pub struct ThemeState { pub theme: Theme, pub variant: Variant, pub application_errors: BTreeMap<String, ServiceError> }
#[derive(Clone, Debug)]
pub struct Application { pub desktop_id: String, pub name: String, pub icon: Option<String>, pub categories: Vec<String>, pub terminal: bool }
#[derive(Clone, Debug)]
pub struct ApplicationState { pub applications: Vec<Application> }
#[derive(Clone, Debug)]
pub struct StartupState { pub approval_path: Option<PathBuf>, pub decided: bool, pub approved: bool }

pub enum Command {
    WifiRadio(bool), WifiScan, WifiDisconnect, WifiConnect { ap: ObjectId, profile: Option<ObjectId> },
    BluetoothPower(bool), BluetoothScan { duration: Duration },
    BluetoothPairConnect(ObjectId), BluetoothDisconnect(ObjectId), BluetoothRemove(ObjectId),
    VolumeSet(f64), VolumeDelta(f64), MuteToggle,
    MediaPrevious, MediaNext, MediaPlayPause, MediaSeek { track: ObjectId, position_us: i64 }, MediaRaise,
    TrayActivate { item: ObjectId, x: i32, y: i32 }, TraySecondary { item: ObjectId, x: i32, y: i32 },
    TrayContext { item: ObjectId, x: i32, y: i32 }, TrayScroll { item: ObjectId, delta: i32, horizontal: bool },
    TrayMenuEvent { item: ObjectId, entry: i32, event: String },
    DndSet(bool), NotificationPanelToggle, ProfileSet(PowerProfile),
    BrightnessBegin { output: String }, BrightnessSet { output: String, percent: f64, sequence: u64 }, BrightnessFinish { output: String, sequence: u64 },
    FanLimit { channel: FanChannel, percent: u8 }, VmStart, VmHibernate,
    MihomoMode(String), MihomoService { operation: UnitOperation }, OpenDashboard,
    CalendarWeek(String), CalendarOpenEvent { uid: String, start_iso: String }, CalendarNewEvent, CalendarShow,
    WeatherDay(u8), FocusSession(String), FocusWindow(String), WorkspaceActivate(i32), WorkspaceMoveWindow(i32),
    DeskflowTarget(Target), DeskflowReflow, ViewflowTarget(Target),
    IrSend(String), IrLearn { name: String, replace: bool }, IrScan(Duration), IrPin(u8),
    RemoveEgpu { token: String }, Inhibit { owner: InhibitOwner, enable: bool },
    DemoSet(bool), RecordingSet(bool), ThemeSet(Theme), ThemeToggle, VariantSet(Variant),
    ActivateApplication(String), OpenUri(String), Lock, LogoutUi,
    StartupDecision(bool),
    UiReleased(RequestId), Cancel(RequestId),
}
pub enum UnitOperation { Start, Stop, Restart }
```

Actors receive `(RequestId, Command)` via bounded queues and publish ActionStatus. Prompt replies use separate typed channels; credential replies are intentionally absent from this general/loggable enum. VM transitions AwaitingUiRelease → AwaitingAuthorization → Running; UiReleased is acknowledged after unmap/input focus release, not after an animation starts. Stop/hibernate completion requires unit observation after successful job, not timer expiry. Target switching supports partial failure and keeps actor alive when popup hides. Cancellation can stop waiting but cannot undo hardware already switched; compensation is explicit.

IR polarity and optional decoded protocol metadata should be deserialized from actual device/database variants before freezing wire types; internal IrButton above is illustrative and normalization must not discard raw database data on read-modify-write. Same applies to unknown provider status/profile values. Do not derive Debug for secrets or raw hook payloads.

## Implementation acceptance criteria

1. Unit tests with recorded protocol fixtures cover each exact legacy schema and conversion (µs/s, µW/W, KiB/bytes, signal percent/fraction); QML lexical evidence is indexed in services.md.
2. Fake bus/socket peers exercise subscribe-before-snapshot, owner restart, malformed/oversized data, action timeout/cancel, stale response generation, calendar reconnect/request cancellation and zero subscribers.
3. Virtual clock checks assert no periodic timer with zero sampling demand, paused media, inactive spectrum, unexpired weather cache and no pending reconnect; fan safety timer is documented independently.
4. Helper tests use fake sysfs and policy stubs for channel/BDF allowlist, fan curve/flooring, missing telemetry, locks, boot80% versus saved desktop, permission denied and suspend/replug.
5. UI integration proves VM exclusive input release before auth, panel-unload action survival, read-only agent view, eGPU confirmation, and partial Viewflow success. Build/static tests alone cannot prove these live interactions.
6. Runtime instrumentation must show no helper fork/exec path. All app activation, dcal discovery and external protocol gaps named in services.md must be resolved before claiming full parity.
