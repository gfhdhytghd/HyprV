//! One shared provider graph. UI visibility owns core subscriptions, never timers.
use hyprv_core::{
    calloop::ping::Ping,
    tokio::{self, sync::watch},
};
use hyprv_core::{
    runtime::ServiceRuntime,
    store::{StateStore, Topic},
};
use hyprv_svc_desktop as desktop;
use hyprv_svc_media as media;
use hyprv_svc_net as net;
use hyprv_svc_power as power;
use hyprv_svc_sys as sys;
use std::{collections::BTreeMap, sync::Arc};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Consumer {
    Bar,
    Clock,
    Panel,
    Wifi,
    Stats,
    Battery,
    Calendar,
    Island,
    AgentModel,
    Media,
    Presentation,
    Tray,
}
/// Effective painted bar demand, aggregated over outputs. A compact strip has
/// no hidden detailed metric lease; panels negotiate their independent demand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BarStatsDemand { #[default] Hidden, Compact, Detailed }
impl BarStatsDemand {
    fn request(self) -> Option<(Vec<power::sysstats::Metric>,std::time::Duration)> {
        use power::sysstats::Metric::*;
        match self {
            Self::Hidden => None,
            Self::Compact => Some((vec![CpuTotal,Memory],std::time::Duration::from_secs(5))),
            Self::Detailed => Some((vec![CpuTotal,Memory,Network,Temperature,CpuPackagePower],std::time::Duration::from_secs(1))),
        }
    }
}
pub struct Visibility(BTreeMap<Consumer, watch::Sender<bool>>, watch::Sender<BarStatsDemand>);
impl Default for Visibility {
    fn default() -> Self {
        Self(
            [
                Consumer::Bar,
                Consumer::Clock,
                Consumer::Panel,
                Consumer::Wifi,
                Consumer::Stats,
                Consumer::Battery,
                Consumer::Calendar,
                Consumer::Island,
                Consumer::AgentModel,
                Consumer::Media,
                Consumer::Presentation,
                Consumer::Tray,
            ]
            .into_iter()
            .map(|k| (k, watch::channel(false).0))
            .collect(),
            watch::channel(BarStatsDemand::Hidden).0,
        )
    }
}
impl Visibility {
    pub fn set_bar_stats(&self, demand: BarStatsDemand) {
        self.1.send_if_modified(|old| { if *old == demand {false} else {*old=demand;true} });
    }
    pub fn bar_stats(&self) -> watch::Receiver<BarStatsDemand> { self.1.subscribe() }
    pub fn set(&self, consumer: Consumer, visible: bool) {
        self.0[&consumer].send_if_modified(|v| {
            if *v == visible {
                false
            } else {
                *v = visible;
                true
            }
        });
    }
    pub fn gate(&self, consumer: Consumer) -> watch::Receiver<bool> {
        self.0[&consumer].subscribe()
    }
}
/// The subscription exists only inside an active visibility epoch. No invisible
/// watch is retained just to forward values; transitions deliver retained state.
pub async fn forward<T: Clone + PartialEq + Send + Sync + 'static>(
    topic: Topic<T>, visible: watch::Receiver<bool>, stop: watch::Receiver<bool>, sender: Ping,
) {
    forward_projected(topic, visible, stop, sender, Clone::clone, None).await;
}
pub(crate) async fn forward_projected<T, K, F>(
    topic: Topic<T>, visible: watch::Receiver<bool>, stop: watch::Receiver<bool>,
    sender: Ping, project: F, pending: Option<Arc<std::sync::atomic::AtomicU8>>,
) where T: Clone + PartialEq + Send + Sync + 'static, K: PartialEq + Send,
    F: Fn(&T) -> K + Send,
{
    forward_with::<T,K,F,_,_>(visible,stop,sender,project,pending,move || topic.subscribe()).await;
}
trait Observation<T: Clone> {
    fn subscription(&mut self)->&mut hyprv_core::store::Subscription<T>;
}
impl<T:Clone> Observation<T> for hyprv_core::store::Subscription<T> {
    fn subscription(&mut self)->&mut Self{self}
}
impl Observation<power::sysstats::State> for power::sysstats::SamplingLease {
    fn subscription(&mut self)->&mut hyprv_core::store::Subscription<power::sysstats::State>{&mut self.subscription}
}
async fn forward_with<T,K,F,A,S>(
    mut visible:watch::Receiver<bool>,mut stop:watch::Receiver<bool>,sender:Ping,
    project:F,pending:Option<Arc<std::sync::atomic::AtomicU8>>,acquire:A,
) where T:Clone+PartialEq+Send+Sync+'static,K:PartialEq+Send,F:Fn(&T)->K+Send,
    A:Fn()->S+Send,S:Observation<T>+Send,
{
    loop {
        if *stop.borrow() { return; }
        if !*visible.borrow_and_update() {
            tokio::select! { r=visible.changed()=>if r.is_err(){return}, _=stop.changed()=>return }
            continue;
        }
        let mut observation = acquire();
        let subscription = observation.subscription();
        let mut key = project(&subscription.current().value);
        notify::<T>(&sender, pending.as_deref());
        loop {
            tokio::select! {
                r=subscription.changed()=>{
                    let Ok(snapshot) = r else { return };
                    let next = project(&snapshot.value);
                    if next != key {
                        key = next;
                        notify::<T>(&sender, pending.as_deref());
                    } else {
                        hyprv_ui::diagnostics::count("service.metadata_only");
                    }
                },
                r=visible.changed()=>{if r.is_err(){return};if !*visible.borrow_and_update(){break}},
                _=stop.changed()=>return,
            }
        }
    }
}
async fn forward_stats<K,F>(stats:power::sysstats::ServiceHandle,metrics:Vec<power::sysstats::Metric>,
    visible:watch::Receiver<bool>,stop:watch::Receiver<bool>,sender:Ping,project:F,
    pending:Option<Arc<std::sync::atomic::AtomicU8>>)
where K:PartialEq+Send,F:Fn(&power::sysstats::State)->K+Send {
    forward_with::<power::sysstats::State,K,F,_,_>(visible,stop,sender,project,pending,
        move||stats.lease(metrics.clone(),std::time::Duration::from_secs(1)).expect("three shell stats leases fit capacity")).await;
}

async fn forward_bar_stats(stats:power::sysstats::ServiceHandle,
    mut demand:watch::Receiver<BarStatsDemand>,mut stop:watch::Receiver<bool>,
    sender:Ping,pending:Arc<std::sync::atomic::AtomicU8>) {
    loop {
        if *stop.borrow() { return; }
        let request = demand.borrow_and_update().request();
        if let Some((metrics,period)) = request {
            // Cancellation of this future drops its SamplingLease before the
            // next mode is acquired. No orphan task or overlap during a switch.
            let (_visible, visible) = watch::channel(true);
            let task = forward_with::<_,_,_,_,_>(visible,stop.clone(),sender.clone(),bar_stats_key,
                Some(pending.clone()),||stats.lease(metrics.clone(),period).expect("bar lease capacity"));
            tokio::select! {
                _=task => return,
                r=demand.changed()=>if r.is_err(){return},
                _=stop.changed()=>return,
            }
        } else {
            tokio::select! {
                r=demand.changed()=>if r.is_err(){return},
                _=stop.changed()=>return,
            }
        }
    }
}
fn notify<T: 'static>(sender: &Ping, pending: Option<&std::sync::atomic::AtomicU8>) {
    if let Some(pending) = pending {
        let bit = if std::any::TypeId::of::<T>() == std::any::TypeId::of::<power::sysstats::State>() { 1 } else { 2 };
        pending.fetch_or(bit, std::sync::atomic::Ordering::AcqRel);
    }
    hyprv_ui::diagnostics::count(std::any::type_name::<T>());
    sender.ping();
}
/// Bar layout consumes formatted values, not histories/generation or sub-display
/// precision. Keep threshold flags too: 69.6 and 70 degrees share a rounded
/// label but have different icons/colors. Expanded panels retain raw forwarding.
#[derive(Debug, PartialEq)]
pub(crate) struct BarStatsKey {
    labels: [String; 5],
    iface: Option<Arc<str>>,
    hot: bool,
    power_severity: Option<u8>,
}
pub(crate) fn bar_stats_key(state: &power::sysstats::State) -> BarStatsKey {
    use hyprv_ui_bar::format::{human_rate, js_round};
    let temp = state.temperature_c.unwrap_or(0.);
    let watts = state.package_power_w.filter(|v| v.is_finite());
    BarStatsKey {
        labels: [
            js_round(state.cpu_percent.unwrap_or(0.)).to_string(),
            js_round(state.memory_percent.unwrap_or(0.)).to_string(),
            js_round(temp).to_string(),
            human_rate(state.rx_bytes_per_s.unwrap_or(0.) + state.tx_bytes_per_s.unwrap_or(0.)),
            match watts {
                Some(w) if w >= 100. => format!("{w:.0} W"),
                Some(w) => format!("{w:.1} W"),
                None => "-- W".into(),
            },
        ],
        iface: state.iface.clone(),
        hot: temp >= 70.,
        power_severity: state.package_power_w.map(|w| {
            if w >= 90. { 2 } else if w >= 60. { 1 } else if w.is_finite() { 0 } else { 3 }
        }),
    }
}
/// Tooltip strings are not painted by the shell. Their classification still
/// affects tray order/network filtering, and menus and click routing remain live.
fn tray_wake_key(state: &media::tray::TrayState) -> (media::tray::TrayState, Vec<(u8, bool)>) {
    if hyprv_ui::diagnostics::enabled() {
        static LAST: std::sync::Mutex<Option<media::tray::TrayState>> = std::sync::Mutex::new(None);
        let mut last = LAST.lock().unwrap();
        if let Some(before) = &*last {
            for (name, changed) in [
                ("tray.item_count", before.items.len() != state.items.len()),
                ("tray.menus", before.menus != state.menus),
                ("tray.watcher", before.watcher_available != state.watcher_available || before.watcher_owned != state.watcher_owned),
            ] { if changed { hyprv_ui::diagnostics::count(name); } }
            for (a,b) in before.items.iter().zip(&state.items) {
                for (name, changed) in [
                    ("tray.identity", a.key != b.key || a.service != b.service || a.path != b.path || a.id != b.id || a.order != b.order),
                    ("tray.tooltip", a.title != b.title || a.tooltip_title != b.tooltip_title || a.tooltip_description != b.tooltip_description || a.tooltip_icon_name != b.tooltip_icon_name),
                    ("tray.icon_pixmaps", a.icon_pixmaps != b.icon_pixmaps),
                    ("tray.icon_name", a.icon != b.icon || a.icon_name != b.icon_name || a.icon_theme_path != b.icon_theme_path),
                    ("tray.attention", a.attention_icon_name != b.attention_icon_name || a.attention_pixmaps != b.attention_pixmaps),
                    ("tray.overlay", a.overlay_icon_name != b.overlay_icon_name || a.overlay_pixmaps != b.overlay_pixmaps),
                    ("tray.status", a.status != b.status || a.category != b.category),
                    ("tray.click_flags", a.only_menu != b.only_menu || a.menu != b.menu || a.has_menu != b.has_menu),
                ] { if changed { hyprv_ui::diagnostics::count(name); } }
            }
        }
        *last = Some(state.clone());
    }
    let mut state = state.clone();
    let classes = state.items.iter_mut().map(|item| {
        let key = hyprv_ui_bar::layout::tray_key(item);
        let class = (hyprv_ui_bar::layout::tray_priority(item),
            ["nm-applet", "networkmanager", "network-manager"].iter().any(|s| key.contains(s)));
        item.title.clear(); item.tooltip_title.clear(); item.tooltip_description.clear();
        class
    }).collect();
    (state, classes)
}
pub struct Hub {
    pub runtime: Arc<ServiceRuntime>,
    pub visibility: Visibility,
    pub store: Arc<StateStore>,
    pub read_only: bool,
    pub media_counters: Arc<media::Counters>,
    brightness_sequence: std::sync::atomic::AtomicU64,
    brightness_last: std::sync::Mutex<BTreeMap<String, (i32, u64)>>,
    providers: Option<Providers>,
    pending: Arc<std::sync::atomic::AtomicU8>,
    mock_actions: Option<std::sync::Mutex<Vec<serde_json::Value>>>,
}
/// Optional workstation domain. Laptop holds state only, no VM service, system-bus
/// subscription, retry timer or job. Desktop dispatch is the original handle.
pub struct VmSlot {
    pub state: hyprv_core::store::Topic<sys::vm::State>,
    native: Option<sys::Handle<sys::vm::State,sys::vm::Action>>,
}
impl VmSlot {
    fn new(ctx: sys::Context) -> Self {
        if crate::laptop::active() { Self {state:ctx.store.topic::<sys::vm::Key>(Default::default()),native:None} }
        else {let native=sys::vm::spawn(ctx);Self {state:native.state.clone(),native:Some(native)}}
    }
    pub async fn dispatch(&self, action:sys::vm::Action)->sys::Result<serde_json::Value> {
        match &self.native {Some(native)=>native.dispatch(action).await,None=>Err(sys::Error::new("unavailable","VM is not part of the laptop service graph"))}
    }
}
pub struct Providers {
    pub audio: media::audio::AudioHandle,
    pub cider: media::cider::CiderHandle,
    pub mpris: media::mpris::MprisHandle,
    pub spectrum: media::spectrum::SpectrumHandle,
    pub tray: media::tray::TrayHandle,
    pub notifications: media::notifications::NotificationHandle,
    pub network: net::network::Handle,
    pub bluetooth: net::bluetooth::Handle,
    pub power: power::Handle<power::power::State, power::power::Action>,
    pub battery: power::Handle<power::battery_history::State, power::battery_history::Action>,
    pub backlight: power::Handle<power::backlight::State, power::backlight::Action>,
    pub fan: power::Handle<power::fan::State, power::fan::Action>,
    pub inhibit: power::Handle<power::inhibit::State, power::inhibit::Action>,
    pub stats: power::sysstats::ServiceHandle,
    pub desktop: desktop::DesktopServices,
    pub systemd: sys::Handle<sys::systemd::State, sys::systemd::Action>,
    pub vm: VmSlot,
    pub mihomo: sys::Handle<sys::mihomo::State, sys::mihomo::Action>,
    pub hyprland: sys::Handle<sys::hyprland::State, sys::hyprland::Action>,
    pub calendar: sys::Handle<sys::calendar::State, sys::calendar::Action>,
    pub weather: sys::Handle<sys::weather::State, sys::weather::Action>,
}
impl std::ops::Deref for Hub {
    type Target = Providers;
    fn deref(&self) -> &Providers {
        self.providers
            .as_ref()
            .expect("mock service graph must not access native providers")
    }
}
impl Hub {
    /// Only the explicitly guarded private functional harness constructs this graph.
    pub fn mocked(runtime: Arc<ServiceRuntime>, store: Arc<StateStore>) -> Arc<Self> {
        Arc::new(Self {
            runtime,
            store,
            visibility: Visibility::default(),
            read_only: false,
            media_counters: Default::default(),
            brightness_sequence: std::sync::atomic::AtomicU64::new(0),
            brightness_last: Default::default(),
            providers: None,
            pending: Default::default(),
            mock_actions: Some(std::sync::Mutex::new(Vec::new())),
        })
    }
    /// IPC `desktop setVariant <v1|v2|v3|bt>` and `desktop deskflowReflow`, called by the
    /// patched hypr scripts. Answered off the UI thread and only once the service is done,
    /// so rotate-screen-ccw.sh can undo its rotation when the reflow fails (as with
    /// deskflow-target.sh).
    pub async fn desktop_ipc(
        &self,
        request: hyprv_core::ipc::Request,
    ) -> hyprv_core::ipc::Response {
        use hyprv_core::ipc::Response;
        if let Err(e) = request.validate() {
            return Response::error(e);
        }
        let result = match request.function.as_str() {
            "setVariant" => {
                let Some(variant) = desktop::theme::Variant::parse(&request.args[0]) else {
                    return Response::error("variant must be v1, v2, v3 or bt");
                };
                let action = desktop::theme::ThemeAction::SetVariant(variant);
                if self.intercept("theme", &action) {
                    return Response::success("");
                }
                if self.read_only {
                    return Response::error("read-only shell");
                }
                self.desktop.theme.request(action).await
            }
            "deskflowReflow" if crate::laptop::active() => return Response::success(""),
            "deskflowReflow" => {
                let action = desktop::deskflow::DeskflowAction::Reflow;
                if self.intercept("deskflow", &action) {
                    return Response::success("");
                }
                if self.read_only {
                    return Response::error("read-only shell");
                }
                self.desktop.deskflow.request(action).await
            }
            _ => return Response::error("unknown IPC target/function"),
        };
        match result {
            Ok(text) => Response::success(text.trim_end().to_string()),
            Err(e) => Response::error(e.to_string()),
        }
    }
    pub fn take_stats_only(&self) -> bool {
        self.pending.swap(0, std::sync::atomic::Ordering::AcqRel) == 1
    }
    pub fn is_mock(&self) -> bool {
        self.mock_actions.is_some()
    }
    pub fn reset_mock_sequence(&self) {
        if self.is_mock() {
            self.brightness_sequence
                .store(0, std::sync::atomic::Ordering::Relaxed);
            self.brightness_last.lock().unwrap().clear();
        }
    }
    pub fn recorded(&self, clear: bool) -> Vec<serde_json::Value> {
        let Some(log) = &self.mock_actions else {
            return vec![];
        };
        let mut log = log.lock().unwrap();
        if clear {
            std::mem::take(&mut *log)
        } else {
            log.clone()
        }
    }
    fn intercept(&self, service: &str, action: &impl std::fmt::Debug) -> bool {
        let Some(log) = &self.mock_actions else {
            return false;
        };
        log.lock()
            .unwrap()
            .push(serde_json::json!({"service":service,"action":format!("{action:?}")}));
        true
    }
    pub fn agent_focus(&self, id: String) {
        let action = desktop::agent_island::AgentAction::FocusSession(id);
        if !self.read_only && !self.intercept("agent", &action) {
            let _ = self.desktop.agent_island.dispatch(action);
        }
    }
}
impl Hub {
    pub fn spawn(
        runtime: Arc<ServiceRuntime>,
        store: Arc<StateStore>,
        changed: Ping,
        read_only: bool,
    ) -> Result<Arc<Self>, Box<dyn std::error::Error>> {
        let profile = hyprv_core::profile::profile();
        let mut mc = media::Context::new(store.clone(), runtime.clone());
        mc.read_only = read_only;
        let mut nc = net::Context::new(store.clone(), runtime.clone());
        nc.read_only = read_only;
        // Without a system bus the shell keeps running with a placeholder connection
        // (system services report unavailable) and a backoff supervisor; see system_bus.rs.
        let (system_bus, system_bus_up) = crate::system_bus::connect(&runtime, &store)?;
        crate::system_bus::supervise(&runtime, &store, system_bus.clone(), system_bus_up);
        let mut pc = power::Context {
            runtime: Arc::downgrade(&runtime),
            shutdown: runtime.shutdown_signal(),
            bus: system_bus,
            paths: power::Paths::default(),
            store: store.clone(),
            sleeping: hyprv_core::tokio::sync::watch::channel(false).0,
        };
        pc.store = store.clone();
        let sleep = store.topic::<hyprv_core::clock::SleepTopic>(Default::default());
        let stop = runtime.shutdown_signal();
        let bus = pc.bus.clone();
        if system_bus_up {
            runtime.spawn(async move {
                if let Err(e) = hyprv_core::clock::run_sleep_topic(sleep, Some(bus), stop).await {
                    eprintln!("sleep topic: {e}")
                }
            });
        }
        let sc = sys::Context {
            runtime: runtime.clone(),
            store: store.clone(),
        };
        let dc = desktop::Ctx::new(
            store.clone(),
            runtime.enter(|| tokio::runtime::Handle::current()),
            runtime.shutdown_signal(),
        );
        let cider = media::cider::spawn(&mc);
        let mpris = media::mpris::spawn(&mc, Some(cider.requester()));
        let spectrum = media::spectrum::spawn(&mc, media::mpris::playing_gate(&mc, &mpris.topic));
        let inhibit = power::inhibit::spawn(pc.clone());
        let desktop = spawn_desktop(&dc, inhibit.clone());
        let hub = Arc::new(Self {
            runtime: runtime.clone(),
            store,
            read_only,
            media_counters: mc.counters.clone(),
            brightness_sequence: std::sync::atomic::AtomicU64::new(0),
            brightness_last: Default::default(),
            visibility: Visibility::default(),
            pending: Default::default(),
            mock_actions: None,
            providers: Some(Providers {
                audio: media::audio::spawn(&mc),
                cider,
                mpris,
                spectrum,
                tray: media::tray::spawn(&mc),
                notifications: media::notifications::spawn(&mc),
                network: net::network::spawn(nc.clone()),
                bluetooth: net::bluetooth::spawn(nc),
                power: power::power::spawn_for_profile(pc.clone(), profile),
                battery: power::battery_history::spawn_for_profile(pc.clone(), profile),
                backlight: power::backlight::spawn_for_profile(pc.clone(), profile),
                fan: power::fan::spawn_for_profile(pc.clone(), profile),
                inhibit,
                stats: power::sysstats::spawn_for_profile(pc, profile),
                desktop,
                systemd: sys::systemd::spawn(sc.clone()),
                vm: VmSlot::new(sc.clone()),
                mihomo: sys::mihomo::spawn(sc.clone()),
                hyprland: sys::hyprland::spawn(sc.clone()),
                calendar: sys::calendar::spawn(sc.clone()),
                weather: sys::weather::spawn(sc),
            }),
        });
        macro_rules! observe {
            ($topic:expr,$consumer:ident) => {
                runtime.spawn(forward_projected(
                    $topic.clone(),
                    hub.visibility.gate(Consumer::$consumer),
                    runtime.shutdown_signal(),
                    changed.clone(), Clone::clone, Some(hub.pending.clone()),
                ));
            };
        }
        if profile == power::Profile::Laptop { observe!(hub.audio.topic, Presentation); } else { observe!(hub.audio.topic, Bar); }
        if profile == power::Profile::Laptop {
            observe!(hub.mpris.topic, Presentation);
            let mut gate=hub.visibility.gate(Consumer::Presentation);
            let mut stop=runtime.shutdown_signal();let store=hub.store.clone();
            runtime.spawn(async move {let mut held=None;loop {if *stop.borrow(){return}
if *gate.borrow_and_update(){if held.is_none(){held=Some(media::mpris::present(&store));}}else{held=None;}tokio::select!{r=gate.changed()=>if r.is_err(){return},_=stop.changed()=>return}}});
        } else { observe!(hub.mpris.topic, Bar); }
        runtime.spawn(forward_projected(hub.tray.topic.clone(),
            hub.visibility.gate(if profile == power::Profile::Laptop { Consumer::Tray } else { Consumer::Bar }), runtime.shutdown_signal(),
            changed.clone(), tray_wake_key, Some(hub.pending.clone())));
        observe!(hub.notifications.topic, Bar);
        observe!(hub.network.state, Bar);
        observe!(hub.power.state, Bar);
        observe!(hub.hyprland.state, Bar);
        use power::sysstats::Metric;
        // The bar paints temperature/power; per-core tiles exist only in Stats.
        let bar_metrics=vec![Metric::CpuTotal,Metric::Memory,Metric::Network,Metric::Temperature,Metric::CpuPackagePower];
        runtime.spawn(forward_bar_stats(hub.stats.clone(),hub.visibility.bar_stats(),
            runtime.shutdown_signal(),changed.clone(),hub.pending.clone()));
        let mut chart_metrics=bar_metrics.clone();chart_metrics.push(Metric::CpuCores);
        runtime.spawn(forward_stats(hub.stats.clone(),chart_metrics,
            hub.visibility.gate(Consumer::Stats),runtime.shutdown_signal(),
            changed.clone(),Clone::clone,Some(hub.pending.clone())));
        if profile == power::Profile::Desktop {
            runtime.spawn(forward_stats(hub.stats.clone(),bar_metrics,
                hub.visibility.gate(Consumer::Panel),runtime.shutdown_signal(),
                changed.clone(),Clone::clone,Some(hub.pending.clone())));
        }
        observe!(hub.spectrum.topic, Media);
        observe!(hub.battery.state, Battery);
        observe!(hub.backlight.state, Panel);
        observe!(hub.fan.state, Panel);
        observe!(hub.inhibit.state, Panel);
        observe!(hub.bluetooth.state, Panel);
        observe!(hub.mihomo.state, Panel);
        observe!(hub.vm.state, Panel);
        observe!(hub.desktop.startup.topic, Bar);
        observe!(hub.desktop.recording.topic, Bar);
        observe!(hub.desktop.theme.topic, Bar);
        observe!(hub.desktop.demo.topic, Panel);
        observe!(hub.desktop.viewflow.topic, Panel);
        observe!(hub.desktop.agent_island.topic, AgentModel);
        observe!(hub.calendar.state, Calendar);
        observe!(hub.weather.state, Calendar);
        // The laptop scanner is independent of passive network snapshots.
        if profile == power::Profile::Laptop {
            let lease = hub.network.scan_lease.clone();
            let mut gate = hub.visibility.gate(Consumer::Wifi);
            let mut stop = runtime.shutdown_signal();
            runtime.spawn(async move {
                let mut held = None;
                loop {
                    if *stop.borrow() { return; }
                    if *gate.borrow_and_update() { if held.is_none() { held=Some(lease.acquire()); } } else { held=None; }
                    tokio::select! { r=gate.changed()=>if r.is_err(){return}, _=stop.changed()=>return }
                }
            });
        }
        for (lease, consumer) in [
            (
                hub.desktop.agent_island.scan_lease.clone(),
                Consumer::Island,
            ),
            (hub.desktop.recording.scan_lease.clone(), Consumer::Panel),
        ] {
            let mut gate = hub.visibility.gate(consumer);
            let mut stop = runtime.shutdown_signal();
            runtime.spawn(async move {let mut held=None;loop{if *stop.borrow(){return}
if *gate.borrow_and_update(){if held.is_none(){held=Some(lease.acquire());}}else{held=None;}tokio::select!{r=gate.changed()=>if r.is_err(){return},_=stop.changed()=>return}}});
        }
        Ok(hub)
    }
}
impl Hub {
    fn finish<E: std::fmt::Display>(result: Result<(), E>) {
        if let Err(e) = result {
            eprintln!("service action: {e}");
        }
    }
    pub fn bar_action(self: &Arc<Self>, action: hyprv_ui_bar::Action) {
        if self.read_only {
            return;
        }
        use hyprv_ui_bar::Action as A;
        let hub = self.clone();
        self.runtime.spawn(async move {
            match action {
                A::Audio(a) => {
                    if !hub.intercept("audio", &a) {
                        Self::finish(hub.audio.action(a).result().await);
                    }
                }
                A::Media(a) => {
                    if hub.intercept("mpris", &a) {
                        return;
                    }
                    match hub.mpris.action(a).result().await {
                        Ok(media::mpris::MprisOutcome::FocusFallback { pid, class }) => {
                            let state = hub.hyprland.state.snapshot().value;
                            if let Some(window) = state.windows.iter().find(|w| {
                                pid.is_some_and(|pid| w.pid == Some(pid))
                                    || class
                                        .as_ref()
                                        .is_some_and(|class| w.class.eq_ignore_ascii_case(class))
                            }) {
                                let _ = hub
                                    .hyprland
                                    .dispatch(sys::hyprland::Action::FocusWindow(
                                        window.address.clone(),
                                    ))
                                    .await;
                            }
                        }
                        Ok(_) => {}
                        Err(e) => eprintln!("media action: {e}"),
                    }
                }
                A::Desktop(a) => {
                    if hub.intercept("hyprland", &a) {
                        return;
                    }
                    if let Err(e) = hub.hyprland.dispatch(a).await {
                        eprintln!("desktop action: {e}")
                    }
                }
                A::Tray(a) => {
                    if !hub.intercept("tray", &a) {
                        Self::finish(hub.tray.action(a).result().await);
                    }
                }
                A::Notification(a) => {
                    if hub.intercept("notifications", &a) {
                        return;
                    }
                    if let Err(e) = hub.notifications.action(a).result().await {
                        eprintln!("notification action: {e}")
                    }
                }
                A::AcquireBatteryDetails(visible) => hub.visibility.set(Consumer::Battery, visible),
                A::Host(a) => hub.bar_host(a),
            }
        });
    }
    pub fn launch(&self, target: desktop::applications::LaunchTarget) {
        if self.read_only {
            return;
        }
        let action = desktop::applications::AppAction::Launch(target);
        if self.intercept("applications", &action) {
            return;
        }
        if let Err(e) = self.desktop.applications.dispatch(action) {
            eprintln!("launch: {e}")
        }
    }
    pub fn bar_host(&self, action: hyprv_ui_bar::model::HostAction) {
        use desktop::applications::LaunchTarget as L;
        use hyprv_ui_bar::model::HostAction as A;
        match action {
            A::Launcher => self.launch(L::Launcher),
            A::ViewflowStart { global_x, global_y } => self.launch(L::ViewflowWindowsStart {
                x: global_x as i64,
                y: global_y as i64,
            }),
            A::ViewflowMenu => self.launch(L::ViewflowWindowsMenu),
            A::OpenSystemMonitor => self.launch(L::Btop),
            A::OpenAudioMixer => self.launch(L::Pavucontrol),
            _ => eprintln!("host intent must be handled by assembly: {action:?}"),
        }
    }
    pub fn panel_host(&self, action: hyprv_ui_panels::HostAction) {
        if self.read_only {
            return;
        }
        use hyprv_ui_panels::HostAction as A;
        match action {
            A::OpenWifiManager => self.launch(desktop::applications::LaunchTarget::NetworkEditor),
            A::OpenBluetoothManager => {
                self.launch(desktop::applications::LaunchTarget::BluetoothManager)
            }
            A::WifiPanelVisible(visible) => self.visibility.set(Consumer::Wifi, visible),
            A::StartupDecision(approved) => {
                let _ = self
                    .desktop
                    .startup
                    .dispatch(desktop::startup::StartupAction::Decide(approved));
            }
            _ => {}
        }
    }
    /// Legacy laptop debounce: one read-only probe 90 ms after the last Set.
    pub fn brightness_probe(self: &Arc<Self>) {
        let hub = self.clone();
        self.runtime.spawn(async move {
            let action = power::backlight::Action::Refresh;
            if !hub.intercept("backlight", &action) { Self::finish(hub.backlight.action(action).await); }
        });
    }
    pub fn panel_action(self: &Arc<Self>, mut action: hyprv_ui_panels::Action, output: &str) {
        use hyprv_ui_panels::model::{Action as A, FanChannel, Profile, Target};
        if self.read_only {
            if let A::WifiConnect { password, .. } = &mut action {
                zeroize::Zeroize::zeroize(password);
            }
            return;
        }
        let hub = self.clone();
        let output = output.to_owned();
        self.runtime.spawn(async move {
            macro_rules! power_action {
                ($handle:ident,$a:expr) => {
                    { let action=$a; if !hub.intercept(stringify!($handle), &action) {Self::finish(hub.$handle.action(action).await)} }
                };
            }
            macro_rules! system_action {
                ($handle:ident,$a:expr) => {
                    { let action=$a; if !hub.intercept(stringify!($handle), &action) { if let Err(e)=hub.$handle.dispatch(action).await {eprintln!("service action: {e}")} } }
                };
            }
            let network = match &mut action {
                A::WifiSetRadio(v) => Some(net::network::Action::SetRadio(*v)),
                A::WifiRescan => Some(net::network::Action::Rescan),
                A::WifiDisconnect => Some(net::network::Action::Disconnect),
                A::WifiConnect {
                    ssid,
                    password,
                    security,
                } => Some(net::network::Action::Connect {
                    ssid: ssid.clone(),
                    // Empty field = no password: use the saved profile secret.
                    password: (!password.is_empty()).then(|| std::mem::take(password)),
                    security: security.clone(),
                    enterprise: None,
                }),
                _ => None,
            };
            if let Some(a) = network {
                if hub.is_mock() {
                    let text=match &a {
                        net::network::Action::SetRadio(v)=>format!("SetRadio({v})"),
                        net::network::Action::Rescan=>"Rescan".into(),
                        net::network::Action::Disconnect=>"Disconnect".into(),
                        net::network::Action::Connect {ssid,password,security,enterprise}=>format!("Connect {{ ssid: {ssid:?}, password: {password:?}, security: {security:?}, enterprise: {} }}",if enterprise.is_some(){"Some"}else{"None"}),
                    };
                    hub.mock_actions.as_ref().unwrap().lock().unwrap().push(serde_json::json!({"service":"network","action":text}));
                    return;
                }
                match hub.network.dispatch(a).await {
                    Ok(p) => Self::finish(p.result().await),
                    Err(e) => eprintln!("network: {e}"),
                };
                return;
            }
            let bluetooth = match &action {
                A::BluetoothSetPower(v) => Some(net::bluetooth::Action::SetPower(*v)),
                A::BluetoothScan => Some(net::bluetooth::Action::Scan),
                A::BluetoothConnect { address, paired } => Some(net::bluetooth::Action::Connect {
                    address: address.clone(),
                    paired: *paired,
                }),
                A::BluetoothDisconnect { address } => Some(net::bluetooth::Action::Disconnect {
                    address: address.clone(),
                }),
                _ => None,
            };
            if let Some(a) = bluetooth {
                if hub.intercept("bluetooth", &a) {return;}
                match hub.bluetooth.dispatch(a).await {
                    Ok(p) => Self::finish(p.result().await),
                    Err(e) => eprintln!("bluetooth: {e}"),
                };
                return;
            }
            match action {
                A::SetVolume(v) => {let a=media::audio::AudioAction::SetVolume(v as f64);if !hub.intercept("audio",&a){Self::finish(hub.audio.action(a).result().await);}},
                A::ToggleMute => {let a=media::audio::AudioAction::ToggleMute;if !hub.intercept("audio",&a){Self::finish(hub.audio.action(a).result().await);}},
                A::MediaPrevious | A::MediaPlayPause | A::MediaNext => {
                    let a = match action {
                        A::MediaPrevious => media::mpris::MprisAction::Previous,
                        A::MediaNext => media::mpris::MprisAction::Next,
                        _ => media::mpris::MprisAction::PlayPause,
                    };
                    if hub.intercept("mpris", &a) {return;}
                    if let Err(e) = hub.mpris.action(a).result().await {
                        eprintln!("media: {e}")
                    }
                }
                A::BrightnessSet(v) | A::BrightnessFinish(v) => {
                    let finishing=matches!(action,A::BrightnessFinish(_));
                    let previous=hub.brightness_last.lock().unwrap().get(&output).copied();
                    let reuse=finishing.then_some(previous).flatten().filter(|(percent,_)|*percent==v);
                    let sequence=if let Some((_,sequence))=reuse {sequence}else{
                        let old=if hub.is_mock(){0}else{hub.backlight.state.snapshot().value.outputs.into_iter().find(|o|o.output==output).map_or(0,|o|o.request_sequence)};
                        hub.brightness_sequence.fetch_max(old,std::sync::atomic::Ordering::Relaxed);
                        let sequence=hub.brightness_sequence.fetch_add(1,std::sync::atomic::Ordering::Relaxed)+1;
                        hub.brightness_last.lock().unwrap().insert(output.clone(),(v,sequence));
                        power_action!(backlight,power::backlight::Action::Set{output:output.clone(),percent:v as f64,sequence});sequence
                    };
                    if finishing && !crate::laptop::active() {power_action!(backlight,power::backlight::Action::Finish{output,sequence});}
                }
                A::FanLimit { channel, percent } => power_action!(
                    fan,
                    power::fan::Action::SetLimit {
                        channel: match channel {
                            FanChannel::Chassis => power::fan::FanChannel::Chassis,
                            FanChannel::Pump => power::fan::FanChannel::Pump,
                        },
                        percent: percent.clamp(0, 100) as u8
                    }
                ),
                A::ProfileSet(profile) => power_action!(
                    power,
                    power::power::Action::ProfileSet(match profile {
                        Profile::PowerSaver => power::power::PowerProfile::PowerSaver,
                        Profile::Balanced => power::power::PowerProfile::Balanced,
                        Profile::Performance => power::power::PowerProfile::Performance,
                    })
                ),
                A::TogglePreventSleep => power_action!(
                    inhibit,
                    power::inhibit::Action::Toggle(power::inhibit::Owner::Manual)
                ),
                A::ToggleDnd => {
                    let a=media::notifications::NotificationAction::ToggleDnd;
                    if !hub.intercept("notifications",&a) {if let Err(e)=hub.notifications.action(a).result().await {eprintln!("notifications: {e}");}}
                }
                A::ToggleRecording => {let a=desktop::recording::RecordingAction::Toggle;if !hub.intercept("recording",&a){let _=hub.desktop.recording.dispatch(a);}},
                A::ToggleDemo => {let a=desktop::demo::DemoAction::Toggle;if !hub.intercept("demo",&a){let _=hub.desktop.demo.dispatch(a);}},
                A::ToggleTheme => {let a=desktop::theme::ThemeAction::ToggleTheme;if !hub.intercept("theme",&a){let _=hub.desktop.theme.dispatch(a);}},
                A::VmToggle => system_action!(vm, sys::vm::Action::Toggle),
                A::ViewflowSet(target) => {
                    let a=desktop::viewflow::ViewflowAction::Set(match target {Target::Windows=>desktop::Target::Windows, Target::MacOs=>desktop::Target::MacOs});
                    if !hub.intercept("viewflow",&a){let _=hub.desktop.viewflow.dispatch(a);}
                }
                A::MihomoStart => system_action!(
                    mihomo,
                    sys::mihomo::Action::Service(sys::systemd::UnitOperation::Start)
                ),
                A::MihomoStop => system_action!(
                    mihomo,
                    sys::mihomo::Action::Service(sys::systemd::UnitOperation::Stop)
                ),
                A::MihomoMode(mode) => system_action!(mihomo, sys::mihomo::Action::Mode(mode)),
                A::MihomoRefresh => system_action!(mihomo, sys::mihomo::Action::Refresh),
                A::MihomoOpenDashboard => {
                    system_action!(mihomo, sys::mihomo::Action::OpenDashboard)
                }
                A::CalendarPreviousWeek => {
                    system_action!(calendar, sys::calendar::Action::PreviousWeek)
                }
                A::CalendarNextWeek => system_action!(calendar, sys::calendar::Action::NextWeek),
                A::CalendarOpenEvent { uid, start } => system_action!(
                    calendar,
                    sys::calendar::Action::OpenEvent {
                        uid,
                        start_iso: start
                    }
                ),
                A::CalendarNewEvent => system_action!(calendar, sys::calendar::Action::NewEvent),
                A::CalendarShowFullApp => system_action!(calendar, sys::calendar::Action::Show),
                A::Host(a) => hub.panel_host(a),
                A::WifiSetRadio(_)
                | A::WifiRescan
                | A::WifiDisconnect
                | A::WifiConnect { .. }
                | A::BluetoothSetPower(_)
                | A::BluetoothScan
                | A::BluetoothConnect { .. }
                | A::BluetoothDisconnect { .. } => unreachable!(),
            }
        });
    }
}

struct DemoInhibitor(power::Handle<power::inhibit::State, power::inhibit::Action>);
impl desktop::demo::SleepInhibitor for DemoInhibitor {
    fn is_enabled(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + '_>> {
        Box::pin(async move {
            self.0
                .state
                .snapshot()
                .value
                .owners
                .contains(&power::inhibit::Owner::Demo)
        })
    }
    fn enable(
        &self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), desktop::ServiceError>> + Send + '_>,
    > {
        Box::pin(async move {
            self.0
                .action(power::inhibit::Action::Enable(power::inhibit::Owner::Demo))
                .await
                .map_err(|e| desktop::ServiceError::new("inhibit", e.to_string()))
        })
    }
    fn disable(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            let _ = self
                .0
                .action(power::inhibit::Action::Disable(power::inhibit::Owner::Demo))
                .await;
        })
    }
}
pub(crate) fn spawn_desktop(
    ctx: &desktop::Ctx,
    inhibit: power::Handle<power::inhibit::State, power::inhibit::Action>,
) -> desktop::DesktopServices {
    // Unsupported desktop domains do not exist in the laptop graph. Closed
    // command channels fail explicitly; no fake background actor/timer is created.
    macro_rules! inactive { ($key:ty) => { desktop::Handle::new(ctx.store.topic::<$key>(Default::default())).0 }; }
    let laptop=crate::laptop::active();
    let ir: desktop::ir::IrHandle = if laptop { inactive!(desktop::ir::IrTopic) } else { desktop::ir::spawn(ctx) };
    desktop::DesktopServices {
        agent_island: desktop::agent_island::spawn(ctx),
        deskflow: if laptop { inactive!(desktop::deskflow::DeskflowTopic) } else { desktop::deskflow::spawn(ctx, ir.clone()) },
        viewflow: if laptop { inactive!(desktop::viewflow::ViewflowTopic) } else { desktop::viewflow::spawn(ctx, ir.clone()) },
        ir,
        egpu: if laptop { inactive!(desktop::egpu::EgpuTopic) } else { desktop::egpu::spawn(ctx) },
        demo: desktop::demo::spawn(ctx, Arc::new(DemoInhibitor(inhibit))),
        recording: desktop::recording::spawn(ctx),
        theme: desktop::theme::spawn(ctx),
        applications: desktop::applications::spawn(ctx),
        startup: desktop::startup::spawn(ctx),
    }
}

#[cfg(test)]
mod visibility_tests {
    use super::*;
    #[test]
    fn bar_stats_projection_preserves_display_and_severity_boundaries() {
        let mut state = power::sysstats::State {
            cpu_percent: Some(4.1), memory_percent: Some(30.1),
            temperature_c: Some(69.6), package_power_w: Some(59.99),
            ..Default::default()
        };
        let first = bar_stats_key(&state);
        let mut model = hyprv_ui_bar::Model { stats: state.clone(), ..Default::default() };
        let original_modules = hyprv_ui_bar::layout::compute(&model, &model.theme).modules;
        state.cpu_percent = Some(4.2);
        state.memory_percent = Some(30.2);
        state.generation += 1;
        state.elapsed = std::time::Duration::from_secs(1);
        state.cpu_history.push_back(Some(4.2));
        state.memory_available_bytes = 123;
        assert_eq!(bar_stats_key(&state), first);
        model.stats = state.clone();
        assert_eq!(hyprv_ui_bar::layout::compute(&model, &model.theme).modules, original_modules,
            "projection equality must agree with the actual bar layout");
        state.temperature_c = Some(70.);
        assert_ne!(bar_stats_key(&state), first, "same rounded temperature, different icon/color");
        state.temperature_c = Some(69.6);
        state.package_power_w = Some(60.01);
        assert_ne!(bar_stats_key(&state), first, "same power label, different severity");
        state.package_power_w = Some(59.99);
        state.cpu_percent = Some(4.5);
        assert_ne!(bar_stats_key(&state), first);
        state.cpu_percent = Some(4.1);
        state.iface = Some("wlan0".into());
        assert_ne!(bar_stats_key(&state), first);
        state.package_power_w = None;
        let absent = bar_stats_key(&state);
        state.package_power_w = Some(f64::NAN);
        assert_ne!(bar_stats_key(&state), absent, "unavailable retains severity, NaN does not");
    }
    #[test]
    fn coalesced_service_wakes_never_drop_non_stats_changes() {
        let pending = std::sync::atomic::AtomicU8::new(0);
        let (ping, _) = hyprv_core::calloop::ping::make_ping().unwrap();
        notify::<power::sysstats::State>(&ping, Some(&pending));
        assert_eq!(pending.swap(0, std::sync::atomic::Ordering::AcqRel), 1);
        notify::<power::sysstats::State>(&ping, Some(&pending));
        notify::<media::audio::AudioState>(&ping, Some(&pending));
        assert_eq!(pending.swap(0, std::sync::atomic::Ordering::AcqRel), 3);
    }
    #[test]
    fn tray_metadata_projection_preserves_order_icons_and_menu_actions() {
        let mut state = media::tray::TrayState::default();
        state.items.push(media::tray::TrayItem { title: "app".into(), ..Default::default() });
        let initial = tray_wake_key(&state);
        state.items[0].tooltip_description = "CPU 12%".into();
        assert_eq!(tray_wake_key(&state), initial);
        state.items[0].tooltip_description = "networkmanager".into();
        assert_ne!(tray_wake_key(&state), initial);
        state.items[0].tooltip_description.clear();
        state.items[0].only_menu = true;
        assert_ne!(tray_wake_key(&state), initial);
        state.items[0].only_menu = false;
        state.items[0].icon_name = "changed".into();
        assert_ne!(tray_wake_key(&state), initial);
    }
    #[test]
    fn compact_stats_demand_has_no_hidden_metrics_or_repeated_notifications() {
        use power::sysstats::Metric;
        let visibility = Visibility::default();
        let mut mode = visibility.bar_stats();
        assert_eq!(*mode.borrow_and_update(),BarStatsDemand::Hidden);
        assert!(BarStatsDemand::Hidden.request().is_none());
        visibility.set_bar_stats(BarStatsDemand::Compact);
        assert!(mode.has_changed().unwrap());
        assert_eq!(mode.borrow_and_update().request(),Some((vec![Metric::CpuTotal,Metric::Memory],std::time::Duration::from_secs(5))));
        visibility.set_bar_stats(BarStatsDemand::Compact);
        assert!(!mode.has_changed().unwrap(),"unchanged demand must not wake the runtime");
        visibility.set_bar_stats(BarStatsDemand::Detailed);
        assert_eq!(mode.borrow_and_update().request().unwrap().1,std::time::Duration::from_secs(1));
        visibility.set_bar_stats(BarStatsDemand::Hidden);
        assert!(mode.borrow_and_update().request().is_none());
    }
    #[test]
    fn hiding_last_consumer_releases_sampling_subscription() {
        let rt = ServiceRuntime::new().unwrap();
        let visibility = Visibility::default();
        let topic = Topic::new(0u32);
        let (ping, _source) = hyprv_core::calloop::ping::make_ping().unwrap();
        let task = rt.spawn(forward(
            topic.clone(),
            visibility.gate(Consumer::Stats),
            rt.shutdown_signal(),
            ping,
        ));
        let wait = |count| {
            rt.block_on(async {
                tokio::time::timeout(std::time::Duration::from_secs(1), async {
                    while topic.subscriber_count() != count {
                        tokio::task::yield_now().await
                    }
                })
                .await
                .unwrap()
            })
        };
        wait(0);
        visibility.set(Consumer::Stats, true);
        wait(1);
        visibility.set(Consumer::Stats, false);
        wait(0);
        rt.request_shutdown();
        rt.block_on(task).unwrap();
    }
}

#[cfg(test)]
mod mock_boundary_tests {
    use super::*;
    #[test]
    fn brightness_commit_reuses_last_preview_sequence_without_a_second_write() {
        let runtime = Arc::new(ServiceRuntime::new().unwrap());
        let hub = Hub::mocked(runtime.clone(), Arc::new(StateStore::default()));
        assert!(hub.providers.is_none());
        hub.panel_action(hyprv_ui_panels::Action::BrightnessSet(76), "TEST");
        runtime.block_on(async {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        });
        hub.panel_action(hyprv_ui_panels::Action::BrightnessFinish(76), "TEST");
        runtime.block_on(async {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        });
        assert_eq!(hub.recorded(false),serde_json::json!([
            {"service":"backlight","action":"Set { output: \"TEST\", percent: 76.0, sequence: 1 }"},
            {"service":"backlight","action":"Finish { output: \"TEST\", sequence: 1 }"}
        ]).as_array().unwrap().clone());
        runtime.request_shutdown();
    }
}
