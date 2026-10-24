//! NetworkManagerQt/plasma-nm-style connection and SecretAgent lifecycle.
pub use crate::wifi_scan::ScanLease;
use crate::wifi_scan::Scanner;
use crate::{
    bus::{self, *},
    keyring::{Keyring, SecretStore},
    prompts::{Broker, Kind, PromptEnd, Reply},
    rfkill,
    secret::{self, Fields, SProps, SSettings, SValue, Secret, Zeroize, Zeroizing},
    Context, PendingAction, Result,
};
use async_trait::async_trait;
use hyprv_core::{
    profile::{profile, Profile},
    store::{Topic, TopicKey},
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc,
    },
};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinSet,
};
use zbus::{
    zvariant::{OwnedObjectPath, OwnedValue},
    Connection,
};
// Laptop BarServices.qml:18–20 selects the first connected Wi-Fi device, else
// the first Wi-Fi device. Preserve desktop's existing last-active preference.
fn prefer_wifi(profile: Profile, selected_active: Option<bool>, active: bool) -> bool {
    selected_active.is_none()
        || (active && (profile == Profile::Desktop || selected_active == Some(false)))
}
const WIFI: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
const AP: &str = "org.freedesktop.NetworkManager.AccessPoint";
const SETTINGS: &str = "/org/freedesktop/NetworkManager/Settings";
const AGENT: &str = "/org/freedesktop/NetworkManager/SecretAgent";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Network {
    pub id: String,
    pub ssid: String,
    pub ssid_bytes: Vec<u8>,
    pub signal: u8,
    pub security: String,
    pub secure: bool,
    pub enterprise: bool,
    pub active: bool,
    pub known: bool,
    pub bars: String,
    pub bssid: String,
    #[serde(skip)]
    pub key_management_flags: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub available: bool,
    pub present: bool,
    pub iface: String,
    pub device: Option<String>,
    pub enabled: bool,
    pub hardware_enabled: bool,
    pub connected: bool,
    pub ssid: String,
    pub signal: u8,
    pub security: String,
    pub networks: Vec<Network>,
    pub wired_connected: bool,
    pub other_connected: bool,
    pub default_iface: Option<String>,
    pub scanning: bool,
    pub rfkill: rfkill::State,
    pub busy: bool,
    pub message: String,
    pub error: Option<String>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            available: false,
            present: false,
            iface: String::new(),
            device: None,
            enabled: false,
            hardware_enabled: true,
            connected: false,
            ssid: String::new(),
            signal: 0,
            security: String::new(),
            networks: vec![],
            wired_connected: false,
            other_connected: false,
            default_iface: None,
            scanning: false,
            rfkill: rfkill::State::default(),
            busy: false,
            message: String::new(),
            error: None,
        }
    }
}
pub struct StateTopic;
impl TopicKey for StateTopic {
    type Value = State;
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Enterprise {
    pub identity: String,
    pub anonymous_identity: Option<String>,
    pub eap: Vec<String>,
    pub phase2_auth: Option<String>,
    pub ca_cert: Option<String>,
    pub domain_suffix_match: Option<String>,
    pub client_cert: Option<String>,
    pub private_key: Option<String>,
    pub phase2_autheap: Option<String>,
}
/// Passwords intentionally have neither Debug nor Serialize implementations;
/// a password still held by the action is wiped when it is dropped.
pub enum Action {
    SetRadio(bool),
    Rescan,
    Disconnect,
    Connect {
        ssid: String,
        password: Option<String>,
        security: String,
        enterprise: Option<Box<Enterprise>>,
    },
}
impl Drop for Action {
    fn drop(&mut self) {
        if let Action::Connect {
            password: Some(p), ..
        } = self
        {
            p.zeroize();
        }
    }
}
#[async_trait]
pub trait Transport: Send + Sync {
    async fn snapshot(&self) -> Result<State>;
    async fn execute(&self, action: Action) -> Result<String>;
    async fn next_event(&self) -> Event;
    /// Cached NM LastScan age; no D-Bus read or timer. Laptop scan throttling only.
    fn scan_delay(&self) -> Duration {
        Duration::ZERO
    }
    async fn owner_changed(&self) -> Result<()> {
        Ok(())
    }
    /// The daemon disappeared: forget registration, send nothing.
    async fn owner_lost(&self) {}
    async fn shutdown(&self) {}
}
#[derive(Clone)]
pub struct Handle {
    pub state: Topic<State>,
    pub prompts: Topic<Vec<crate::prompts::Prompt>>,
    pub scan_lease: ScanLease,
    broker: Broker,
    actions: mpsc::Sender<(Action, oneshot::Sender<Result<()>>)>,
}
impl Handle {
    pub async fn dispatch(&self, action: Action) -> Result<PendingAction> {
        let (tx, rx) = oneshot::channel();
        self.actions
            .send((action, tx))
            .await
            .map_err(|_| "network service stopped")?;
        Ok(PendingAction(rx))
    }
    pub fn respond(&self, id: u64, reply: Reply) -> Result<()> {
        self.broker.respond(id, reply)
    }
}
pub fn spawn(ctx: Context) -> Handle {
    let broker = Broker::new();
    spawn_inner(ctx, broker, None)
}
pub fn spawn_with_transport(ctx: Context, transport: Arc<dyn Transport>, broker: Broker) -> Handle {
    spawn_inner(ctx, broker, Some(transport))
}
fn spawn_inner(ctx: Context, broker: Broker, transport: Option<Arc<dyn Transport>>) -> Handle {
    spawn_profile(ctx, broker, transport, profile())
}
fn spawn_profile(
    ctx: Context,
    broker: Broker,
    transport: Option<Arc<dyn Transport>>,
    profile: Profile,
) -> Handle {
    let state = ctx.store.topic::<StateTopic>(State::default());
    let scan_lease = ScanLease::default();
    let scanner = Scanner::new(profile, ctx.read_only, &scan_lease);
    let (tx, rx) = mpsc::channel(32);
    let handle = Handle {
        scan_lease,
        state: state.clone(),
        prompts: broker.topic(),
        broker: broker.clone(),
        actions: tx,
    };
    let real_transport = transport.is_none();
    let setup = ctx.clone();
    let shutdown = ctx.runtime.shutdown_signal();
    crate::supervise(&ctx, async move {
        let transport = match transport {
            Some(t) => Ok(t),
            None => Native::new(&setup, broker.clone())
                .await
                .map(|n| Arc::new(n) as Arc<dyn Transport>),
        };
        match transport {
            Ok(t) => run(t, state, rx, broker, scanner, shutdown, real_transport).await,
            Err(e) => {
                let mut s = state.snapshot().value;
                s.error = Some(e);
                state.publish(s);
            }
        }
    });
    handle
}
async fn run(
    transport: Arc<dyn Transport>,
    state: Topic<State>,
    mut actions: mpsc::Receiver<(Action, oneshot::Sender<Result<()>>)>,
    broker: Broker,
    mut scanner: Scanner,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
    monitor_rfkill: bool,
) {
    let mut jobs = JoinSet::new();
    let mut rf = if monitor_rfkill {
        rfkill::Watcher::new(1).ok()
    } else {
        None
    };
    let mut rf_state = rfkill::State::default();
    if rf.is_none() {
        rf_state.error = Some(
            "/dev/rfkill unavailable; NM WirelessHardwareEnabled remains authoritative".into(),
        );
    }
    let mut busy = 0usize;
    let mut message = String::new();
    let mut action_error = None;
    let mut refresh = true;
    loop {
        if refresh {
            let mut s = match transport.snapshot().await {
                Ok(s) => s,
                Err(e) => State {
                    error: Some(e),
                    ..State::default()
                },
            };
            s.rfkill = rf_state.clone();
            s.hardware_enabled &= !rf_state.hard_blocked;
            s.busy = busy > 0;
            if scanner.laptop {
                s.scanning = scanner.visible() && s.present;
            }
            s.message = message.clone();
            if action_error.is_some() {
                s.error = action_error.clone();
            }
            state.publish(s);
            refresh = false;
        }
        if scanner.laptop {
            let scan_delay = if scanner.visible() && !scanner.readonly {
                transport.scan_delay()
            } else {
                Duration::ZERO
            };
            scanner.update(&state.snapshot().value, busy > 0, scan_delay);
        }
        tokio::select! {
            _=shutdown.changed()=>break,
            r=scanner.demand.changed(),if scanner.laptop=>{
                if r.is_err(){break;}
                refresh=true;
            },
            result=async{scanner.job.as_mut().unwrap().await},if scanner.job.is_some()=>{
                scanner.job=None;
                match result {Ok(Ok(_))=>{}, Ok(Err(e))=>action_error=Some(e), Err(e)=>action_error=Some(e.to_string())}
                refresh=true;
            },
            request=actions.recv()=>match request {
                Some((action,reply))=> {
                    if scanner.readonly { let _=reply.send(Err("network service is read-only".into()));continue; }
                    if busy>0 {let _=reply.send(Err("Wi-Fi action already in progress".into()));continue;}
                    let scan_gate = (scanner.laptop && matches!(action, Action::Rescan)).then(|| scanner.demand.clone());
                    if scan_gate.is_some() {
                        if !scanner.visible() { let _=reply.send(Err("Wi-Fi page is hidden".into()));continue; }
                        if scanner.job.is_some() { let _=reply.send(Err("Wi-Fi scan already in progress".into()));continue; }
                        scanner.requested(state.snapshot().value.device.as_deref());
                    }
                    let t=transport.clone();busy+=1;let mut s=state.snapshot().value;s.busy=true;s.error=None;state.publish(s);
                    jobs.spawn(async move { let result=if scan_gate.as_ref().is_some_and(|d| *d.borrow()==0) {
                        Err("Wi-Fi page is hidden".into())
                    } else {t.execute(action).await};let completion=result.as_ref().map(|_|()).map_err(Clone::clone);let _=reply.send(completion);result });
                },None=>break,
            },
            result=jobs.join_next(),if !jobs.is_empty()=> {
                busy=busy.saturating_sub(1);
                match result {Some(Ok(Ok(m)))=>{message=m;action_error=None;},Some(Ok(Err(e)))=>action_error=Some(e),Some(Err(e))=>action_error=Some(e.to_string()),None=>{} }
                refresh=true;
            },
            event=transport.next_event()=>match event {
                Event::Changed=>refresh=true,
                Event::OwnerChanged=> {scanner.reset();jobs.abort_all();busy=0;broker.cancel_all();action_error=transport.owner_changed().await.err();refresh=true;},
                Event::OwnerLost=> {scanner.reset();jobs.abort_all();busy=0;broker.cancel_all();transport.owner_lost().await;action_error=None;refresh=true;},
                Event::Disconnected(e)=>{let mut s=state.snapshot().value;s.available=false;s.error=Some(e);state.publish(s);break;},
            },
            _=async{tokio::time::sleep_until(scanner.deadline.unwrap()).await},if scanner.deadline.is_some()=>{
                // Recheck demand after readiness; never send a queued hidden-page scan.
                if scanner.visible() && !scanner.readonly {
                    scanner.requested(state.snapshot().value.device.as_deref());
                    let t=transport.clone();let demand=scanner.demand.clone();
                    scanner.job=Some(tokio::spawn(async move {
                        if *demand.borrow()==0 {return Ok(String::new());}
                        t.execute(Action::Rescan).await
                    }));
                }
            },
            result=async {rf.as_mut().unwrap().next().await},if rf.is_some()=>{
                match result {Ok(s)=>rf_state=s,Err(e)=>{rf=None;rf_state.available=false;rf_state.error=Some(e);}} refresh=true;
            },
        }
    }
    scanner.reset();
    jobs.abort_all();
    broker.cancel_all();
    transport.shutdown().await;
}

pub struct Native {
    profile: Profile,
    connection: Connection,
    watch: Watch,
    broker: Broker,
    read_only: bool,
    scan_baseline: AtomicI64,
    observed_scan: AtomicI64,
    registration_error: std::sync::Mutex<Option<String>>,
    secrets: Arc<AgentSecrets>,
    /// NetworkManager currently has an owner; while false nothing is sent
    /// to it (no D-Bus activation of a stopped daemon).
    present: std::sync::atomic::AtomicBool,
}
impl Native {
    pub async fn new(ctx: &Context, broker: Broker) -> Result<Self> {
        Self::with_store(ctx, broker, Arc::new(Keyring)).await
    }
    /// Same as [`Native::new`] with an injected secret store (tests, mocks).
    pub(crate) async fn with_store(
        ctx: &Context,
        broker: Broker,
        store: Arc<dyn SecretStore>,
    ) -> Result<Self> {
        let c = match &ctx.system_bus {
            Some(c) => c.clone(),
            None => Connection::system().await.map_err(err)?,
        };
        // Match rules are installed before the first state enumeration.
        let watch = Watch::new(&c, NM).await?;
        let secrets = Arc::new(AgentSecrets::new(store));
        if !ctx.read_only {
            c.object_server()
                .at(
                    AGENT,
                    SecretAgent {
                        connection: c.clone(),
                        broker: broker.clone(),
                        secrets: secrets.clone(),
                        pending: Default::default(),
                    },
                )
                .await
                .map_err(err)?;
        }
        let present = bus::has_owner(&c, NM).await;
        let native = Self {
            profile: profile(),
            connection: c,
            watch,
            broker,
            read_only: ctx.read_only,
            scan_baseline: AtomicI64::new(i64::MIN),
            observed_scan: AtomicI64::new(-1),
            registration_error: std::sync::Mutex::new(None),
            secrets,
            present: std::sync::atomic::AtomicBool::new(present),
        };
        if !ctx.read_only {
            *native.registration_error.lock().unwrap() = native.register().await.err();
        } // absent daemon remains observable through NameOwnerChanged
        Ok(native)
    }
    fn present(&self) -> bool {
        self.present.load(Ordering::Acquire)
    }
    async fn register(&self) -> Result<()> {
        // Absent daemon: registration happens when it appears (OwnerChanged).
        if self.read_only || !self.present() {
            return Ok(());
        }
        let p = proxy(
            &self.connection,
            NM,
            "/org/freedesktop/NetworkManager/AgentManager",
            "org.freedesktop.NetworkManager.AgentManager",
        )
        .await?;
        bus::call::<_, ()>(&p, "RegisterWithCapabilities", &("org.hyprv.Shell", 0u32)).await
    }
    async fn device(&self) -> Result<String> {
        self.snapshot()
            .await?
            .device
            .ok_or("No Wi-Fi device found".into())
    }
    async fn profiles(&self) -> Result<Vec<(OwnedObjectPath, Settings)>> {
        let paths: Vec<OwnedObjectPath> = proxy(
            &self.connection,
            NM,
            SETTINGS,
            "org.freedesktop.NetworkManager.Settings",
        )
        .await?
        .call("ListConnections", &())
        .await
        .map_err(err)?;
        let mut profiles = vec![];
        for path in paths {
            if let Ok(settings) = proxy(
                &self.connection,
                NM,
                path.as_str(),
                "org.freedesktop.NetworkManager.Settings.Connection",
            )
            .await?
            .call::<_, _, Settings>("GetSettings", &())
            .await
            {
                profiles.push((path, settings));
            }
        }
        Ok(profiles)
    }
}
impl Native {
    /// A password the user typed for an already saved profile. Its secret
    /// flags are never changed (plasma-nm semantics):
    /// - NOT_REQUIRED: the profile does not use it;
    /// - AGENT_OWNED / NOT_SAVED: handed to the agent for this activation
    ///   (AGENT_OWNED is also stored in the keyring, like a prompt reply);
    /// - system-owned (NONE): the stored secret is replaced in the profile.
    async fn apply_typed_secret(
        &self,
        profile: &OwnedObjectPath,
        settings: &Settings,
        password: Secret,
    ) -> Result<()> {
        let enterprise = settings.contains_key("802-1x");
        let setting = if enterprise {
            "802-1x"
        } else {
            "802-11-wireless-security"
        };
        let key = if enterprise {
            "password".to_owned()
        } else if settings
            .get(setting)
            .map(|p| text(p, "key-mgmt"))
            .as_deref()
            == Some("none")
        {
            format!(
                "wep-key{}",
                settings
                    .get(setting)
                    .map(|p| number(p, "wep-tx-keyidx"))
                    .unwrap_or(0)
                    .min(3)
            )
        } else {
            "psk".to_owned()
        };
        let flags = secret_flags(settings, setting, &key);
        let id = uuid(settings);
        if flags & NOT_REQUIRED != 0 {
            return Ok(());
        }
        if flags & (AGENT_OWNED | NOT_SAVED) != 0 {
            if id.is_empty() {
                return Err("Saved profile has no UUID".into());
            }
            let fields = Fields::from([(key, password)]);
            if agent_owned(flags) {
                let _ = self
                    .secrets
                    .store
                    .save(&id, &connection_text(settings, "id"), setting, &fields)
                    .await;
            }
            self.secrets.stage(&id, setting, fields);
            return Ok(());
        }
        let mut update = secret::from_settings(settings);
        update
            .entry(setting.to_owned())
            .or_default()
            .insert(key, SValue::Secret(password));
        proxy(
            &self.connection,
            NM,
            profile.as_str(),
            "org.freedesktop.NetworkManager.Settings.Connection",
        )
        .await?
        .call::<_, _, ()>("Update", &(update,))
        .await
        .map_err(err)
    }
}
#[async_trait]
impl Transport for Native {
    fn scan_delay(&self) -> Duration {
        let mut now = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        if unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut now) } != 0 {
            return crate::wifi_scan::INTERVAL;
        }
        crate::wifi_scan::backend_delay(
            self.observed_scan.load(Ordering::Relaxed),
            now.tv_sec
                .saturating_mul(1000)
                .saturating_add(now.tv_nsec / 1_000_000),
        )
    }
    async fn snapshot(&self) -> Result<State> {
        if !self.present() {
            return Err("NetworkManager is not running".into());
        }
        let c = &self.connection;
        let root = properties(c, NM, NM_ROOT, NM).await?;
        let mut state = State {
            available: true,
            enabled: boolean(&root, "WirelessEnabled"),
            hardware_enabled: boolean(&root, "WirelessHardwareEnabled"),
            ..State::default()
        };
        state.error = self.registration_error.lock().unwrap().clone();
        let devices: Vec<OwnedObjectPath> = proxy(c, NM, NM_ROOT, NM)
            .await?
            .call("GetDevices", &())
            .await
            .map_err(err)?;
        let mut selected = None;
        for device in devices {
            let p = match properties(c, NM, device.as_str(), DEVICE).await {
                Ok(p) => p,
                Err(_) => continue,
            };
            let kind = number(&p, "DeviceType");
            let active = number(&p, "State") == 100;
            if kind == 1 && active {
                state.wired_connected = true;
            }
            if kind != 1 && kind != 2 && active {
                state.other_connected = true;
            }
            if kind == 2
                && prefer_wifi(
                    self.profile,
                    selected.as_ref().map(|(_, p)| number(p, "State") == 100),
                    active,
                )
            {
                selected = Some((device, p));
            }
        }
        if let Some(primary) = path(&root, "PrimaryConnection") {
            if let Ok(p) = properties(
                c,
                NM,
                &primary,
                "org.freedesktop.NetworkManager.Connection.Active",
            )
            .await
            {
                if let Some(d) = paths(&p, "Devices").first() {
                    if let Ok(dp) = properties(c, NM, d.as_str(), DEVICE).await {
                        state.default_iface = Some(text(&dp, "Interface"));
                    }
                }
            }
        }
        let Some((device, p)) = selected else {
            return Ok(state);
        };
        state.present = true;
        state.iface = text(&p, "Interface");
        state.device = Some(device.to_string());
        let wireless = properties(c, NM, device.as_str(), WIFI).await?;
        let last_scan = wireless
            .get("LastScan")
            .and_then(|v| i64::try_from(v).ok())
            .unwrap_or(-1);
        self.observed_scan.store(last_scan, Ordering::Relaxed);
        let baseline = self.scan_baseline.load(Ordering::Relaxed);
        state.scanning = baseline != i64::MIN && last_scan <= baseline;
        if baseline != i64::MIN && last_scan > baseline {
            self.scan_baseline.store(i64::MIN, Ordering::Relaxed);
        }
        let device_active = number(&p, "State") == 100;
        let active = path(&wireless, "ActiveAccessPoint");
        let mut aps = paths(&wireless, "AccessPoints");
        if let Some(a) = &active {
            let a = OwnedObjectPath::try_from(a.clone()).map_err(err)?;
            if !aps.contains(&a) {
                aps.push(a);
            }
        }
        let profiles = self.profiles().await?;
        let known: Vec<Vec<u8>> = profiles
            .iter()
            .filter_map(|(_, s)| s.get("802-11-wireless").map(|p| bytes(p, "ssid")))
            .collect();
        let mut networks = vec![];
        for ap in aps {
            let Ok(p) = properties(c, NM, ap.as_str(), AP).await else {
                continue;
            };
            let raw = bytes(&p, "Ssid");
            if raw.is_empty() {
                continue;
            }
            let flags = number(&p, "WpaFlags") | number(&p, "RsnFlags");
            let enterprise = flags & ((1 << 9) | (1 << 13)) != 0;
            let secure = flags != 0 || number(&p, "Flags") & 1 != 0;
            networks.push(Network {
                id: ap.to_string(),
                ssid: String::from_utf8_lossy(&raw).into_owned(),
                ssid_bytes: raw.clone(),
                signal: p
                    .get("Strength")
                    .and_then(|v| u8::try_from(v).ok())
                    .unwrap_or(0),
                security: if enterprise {
                    "802.1X"
                } else if flags != 0 {
                    "WPA"
                } else if secure {
                    "WEP"
                } else {
                    ""
                }
                .into(),
                secure,
                enterprise,
                active: active.as_deref() == Some(ap.as_str()) && device_active,
                known: known.contains(&raw),
                bars: String::new(),
                bssid: text(&p, "HwAddress"),
                key_management_flags: flags,
            });
        }
        state.networks = normalize_networks(networks);
        if let Some(ap) = state.networks.iter().find(|n| n.active) {
            state.connected = true;
            state.ssid = ap.ssid.clone();
            state.signal = ap.signal;
            state.security = ap.security.clone();
        }
        Ok(state)
    }
    async fn execute(&self, mut action: Action) -> Result<String> {
        if self.read_only {
            return Err("network service is read-only".into());
        }
        if !self.present() {
            return Err("NetworkManager is not running".into());
        }
        let c = &self.connection;
        match &mut action {
            Action::SetRadio(enabled) => {
                let enabled = *enabled;
                proxy(c, NM, NM_ROOT, NM)
                    .await?
                    .set_property("WirelessEnabled", enabled)
                    .await
                    .map_err(err)?;
                Ok(if enabled {
                    "Wi-Fi enabled"
                } else {
                    "Wi-Fi disabled"
                }
                .into())
            }
            Action::Rescan => {
                let d = self.device().await?;
                let w = properties(c, NM, &d, WIFI).await?;
                let baseline = w
                    .get("LastScan")
                    .and_then(|v| i64::try_from(v).ok())
                    .unwrap_or(-1);
                self.scan_baseline.store(baseline, Ordering::Relaxed);
                let wireless = proxy(c, NM, &d, WIFI).await?;
                if let Err(e) = bus::call::<_, ()>(
                    &wireless,
                    "RequestScan",
                    &(HashMap::<String, OwnedValue>::new(),),
                )
                .await
                {
                    self.scan_baseline.store(i64::MIN, Ordering::Relaxed);
                    return Err(e);
                }
                Ok("Scan started".into())
            }
            Action::Disconnect => {
                let d = self.device().await?;
                call_unit(c, NM, &d, DEVICE, "Disconnect").await?;
                Ok("Disconnected".into())
            }
            Action::Connect {
                ssid,
                password,
                security,
                enterprise,
            } => {
                let (ssid, security, mut enterprise) =
                    (ssid.clone(), security.clone(), enterprise.take());
                // Empty input is "no input": it never replaces a stored secret.
                let mut password: Option<Secret> = password
                    .take()
                    .map(Zeroizing::new)
                    .filter(|p| !p.is_empty());
                if ssid.is_empty() || ssid.len() > 32 {
                    return Err("SSID must contain 1..32 bytes".into());
                }
                let state = self.snapshot().await?;
                let d = state.device.ok_or("No Wi-Fi device found")?;
                let ap = state.networks.iter().find(|n| n.ssid == ssid);
                let specific =
                    OwnedObjectPath::try_from(ap.map(|n| n.id.clone()).unwrap_or("/".into()))
                        .map_err(err)?;
                let d = OwnedObjectPath::try_from(d).map_err(err)?;
                let known = self.profiles().await?.into_iter().find(|(_, s)| {
                    s.get("802-11-wireless")
                        .is_some_and(|w| bytes(w, "ssid") == ssid.as_bytes())
                });
                if let Some((profile, settings)) = known {
                    if let Some(password) = password.take() {
                        self.apply_typed_secret(&profile, &settings, password)
                            .await?;
                    }
                    let _: OwnedObjectPath = proxy(c, NM, NM_ROOT, NM)
                        .await?
                        .call("ActivateConnection", &(profile, d, specific))
                        .await
                        .map_err(err)?;
                } else {
                    let is_enterprise = ap.is_some_and(|n| n.enterprise)
                        || security.contains("802.1X")
                        || security.contains("EAP");
                    if is_enterprise && enterprise.is_none() {
                        let mut reply = self
                            .broker
                            .request(Kind::Enterprise { ssid: ssid.clone() })
                            .await
                            .map_err(|e| e.to_string())?;
                        let Some(fields) = reply.take_fields() else {
                            return Err("Enterprise configuration cancelled".into());
                        };
                        if password.is_none() {
                            password = fields.get("password").filter(|p| !p.is_empty()).cloned();
                        }
                        enterprise = Some(Box::new(enterprise_from_fields(&fields)?));
                    }
                    let security = if ap.is_some_and(|n| {
                        n.key_management_flags & (1 << 10) != 0
                            && n.key_management_flags & (1 << 8) == 0
                    }) {
                        "SAE"
                    } else if ap
                        .is_some_and(|n| n.key_management_flags & ((1 << 11) | (1 << 12)) != 0)
                    {
                        "OWE"
                    } else if ap.is_some_and(|n| {
                        n.key_management_flags & (1 << 13) != 0
                            && n.key_management_flags & (1 << 9) == 0
                    }) {
                        "WPA3-SuiteB192"
                    } else {
                        &security
                    };
                    let settings = connection_settings(
                        &ssid,
                        password.as_deref().map(String::as_str),
                        security,
                        enterprise.as_deref(),
                        ap.map(|n| n.secure).unwrap_or(!security.is_empty()),
                    )?;
                    let _: (
                        OwnedObjectPath,
                        OwnedObjectPath,
                        HashMap<String, OwnedValue>,
                    ) = proxy(c, NM, NM_ROOT, NM)
                        .await?
                        .call(
                            "AddAndActivateConnection2",
                            &(settings, d, specific, HashMap::<String, OwnedValue>::new()),
                        )
                        .await
                        .map_err(err)?;
                }
                Ok(format!("Connection requested for {ssid}"))
            }
        }
    }
    async fn next_event(&self) -> Event {
        self.watch.next().await
    }
    async fn owner_changed(&self) -> Result<()> {
        self.scan_baseline.store(i64::MIN, Ordering::Relaxed);
        self.observed_scan.store(-1, Ordering::Relaxed);
        self.present.store(true, Ordering::Release);
        let result = self.register().await;
        *self.registration_error.lock().unwrap() = result.as_ref().err().cloned();
        result
    }
    async fn owner_lost(&self) {
        self.present.store(false, Ordering::Release);
        self.scan_baseline.store(i64::MIN, Ordering::Relaxed);
        self.observed_scan.store(-1, Ordering::Relaxed);
        *self.registration_error.lock().unwrap() = None;
    }
    async fn shutdown(&self) {
        if !self.read_only && self.present() {
            let _ = call_unit(
                &self.connection,
                NM,
                "/org/freedesktop/NetworkManager/AgentManager",
                "org.freedesktop.NetworkManager.AgentManager",
                "Unregister",
            )
            .await;
            let _ = self
                .connection
                .object_server()
                .remove::<SecretAgent, _>(AGENT)
                .await;
        }
    }
}
pub fn normalize_networks(networks: Vec<Network>) -> Vec<Network> {
    let mut best = BTreeMap::<Vec<u8>, Network>::new();
    for n in networks {
        let key = n.ssid_bytes.clone();
        if best
            .get(&key)
            .is_none_or(|old| (n.active, n.signal) > (old.active, old.signal))
        {
            best.insert(key, n);
        }
    }
    let mut networks: Vec<_> = best.into_values().collect();
    networks.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then(b.signal.cmp(&a.signal))
            .then(a.ssid.cmp(&b.ssid))
    });
    networks
}
fn enterprise_from_fields(f: &Fields) -> Result<Enterprise> {
    let get = |k: &str| f.get(k).map(|v| v.as_str().to_owned());
    let identity = get("identity")
        .filter(|s| !s.is_empty())
        .ok_or("Enterprise identity is required")?;
    let eap = get("eap")
        .ok_or("Enterprise EAP method is required")?
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    Ok(Enterprise {
        identity,
        eap,
        anonymous_identity: get("anonymousIdentity"),
        phase2_auth: get("phase2Auth"),
        ca_cert: get("caCert"),
        domain_suffix_match: get("domainSuffixMatch"),
        client_cert: get("clientCert"),
        private_key: get("privateKey"),
        phase2_autheap: get("phase2Autheap"),
    })
}
fn secret_value(p: &str) -> SValue {
    SValue::Secret(Zeroizing::new(p.to_owned()))
}
/// Settings for a new profile. Legacy parity (`nmcli dev wifi connect ...
/// password`, `wifi-action.sh:64`) wins over plasma-nm here: no `*-flags` key
/// is set, so secrets have flags NONE (0) and are stored in the system
/// profile, available before login and for auto-connect at boot. Existing
/// profiles are never changed this way (see `apply_typed_secret`).
pub fn connection_settings(
    ssid: &str,
    password: Option<&str>,
    security: &str,
    enterprise: Option<&Enterprise>,
    secure: bool,
) -> Result<SSettings> {
    let password = password.filter(|p| !p.is_empty());
    let mut s = SSettings::new();
    s.insert(
        "connection".into(),
        SProps::from([
            ("id".into(), SValue::plain(ssid.to_owned())),
            ("type".into(), SValue::plain("802-11-wireless")),
        ]),
    );
    s.insert(
        "802-11-wireless".into(),
        SProps::from([("ssid".into(), SValue::plain(ssid.as_bytes().to_vec()))]),
    );
    if let Some(e) = enterprise {
        if e.identity.is_empty() || e.eap.is_empty() {
            return Err("Enterprise identity and EAP method are required".into());
        }
        // Never silently disable server-certificate validation for a new enterprise profile.
        if e.ca_cert.is_none() && e.domain_suffix_match.is_none() {
            return Err(
                "Enterprise profile requires CA certificate or system CA domain validation".into(),
            );
        }
        let mut fields = SProps::from([
            ("identity".into(), SValue::plain(e.identity.clone())),
            ("eap".into(), SValue::plain(e.eap.clone())),
            ("system-ca-certs".into(), SValue::plain(true)),
        ]);
        for (key, value) in [
            ("anonymous-identity", &e.anonymous_identity),
            ("phase2-auth", &e.phase2_auth),
            ("phase2-autheap", &e.phase2_autheap),
            ("domain-suffix-match", &e.domain_suffix_match),
        ] {
            if let Some(value) = value {
                fields.insert(key.into(), SValue::plain(value.clone()));
            }
        }
        for (key, path) in [
            ("ca-cert", &e.ca_cert),
            ("client-cert", &e.client_cert),
            ("private-key", &e.private_key),
        ] {
            if let Some(path) = path {
                let mut uri = format!("file://{}", path.trim_start_matches("file://")).into_bytes();
                uri.push(0);
                fields.insert(key.into(), SValue::plain(uri));
            }
        }
        if e.eap.iter().any(|method| method == "tls")
            && (e.client_cert.is_none() || e.private_key.is_none())
        {
            return Err("EAP-TLS requires client certificate and private key".into());
        }
        if let Some(p) = password {
            fields.insert("password".into(), secret_value(p));
        }
        s.insert("802-1x".into(), fields);
        s.insert(
            "802-11-wireless-security".into(),
            SProps::from([(
                "key-mgmt".into(),
                SValue::plain(if security.contains("SuiteB") {
                    "wpa-eap-suite-b-192"
                } else {
                    "wpa-eap"
                }),
            )]),
        );
    } else if secure {
        let mut fields = SProps::new();
        if security.contains("OWE") {
            fields.insert("key-mgmt".into(), SValue::plain("owe"));
        } else if security.contains("WEP") {
            fields.insert("key-mgmt".into(), SValue::plain("none"));
            if let Some(p) = password {
                fields.insert("wep-key0".into(), secret_value(p));
            }
        } else {
            fields.insert(
                "key-mgmt".into(),
                SValue::plain(if security.contains("SAE") || security.contains("WPA3") {
                    "sae"
                } else {
                    "wpa-psk"
                }),
            );
            if let Some(p) = password {
                fields.insert("psk".into(), secret_value(p));
            }
        }
        s.insert("802-11-wireless-security".into(), fields);
    }
    Ok(s)
}

// ------------------------------------------------------------ secret agent

/// NMSettingSecretFlags.
pub const AGENT_OWNED: u32 = 0x1;
pub const NOT_SAVED: u32 = 0x2;
pub const NOT_REQUIRED: u32 = 0x4;
/// NMSecretAgentGetSecretsFlags.
const ALLOW_INTERACTION: u32 = 0x1;
const REQUEST_NEW: u32 = 0x2;
/// Settings whose secrets this agent handles (Wi-Fi and 802.1X).
const SECRET_SETTINGS: &[&str] = &["802-11-wireless-security", "802-1x"];

#[derive(zbus::DBusError, Debug)]
#[zbus(prefix = "org.freedesktop.NetworkManager.SecretAgent")]
enum AgentError {
    PermissionDenied(String),
    InvalidConnection(String),
    UserCanceled(String),
    AgentCanceled(String),
    NoSecrets(String),
    Failed(String),
}

/// Secret state shared by the exported agent and the Connect action.
pub(crate) struct AgentSecrets {
    store: Arc<dyn SecretStore>,
    /// Secrets the user typed for the next activation of a profile whose
    /// secret is agent-provided (AGENT_OWNED or NOT_SAVED); consumed by the
    /// next GetSecrets for (uuid, setting) and never written to the profile.
    staged: std::sync::Mutex<HashMap<(String, String), Fields>>,
}
impl AgentSecrets {
    pub(crate) fn new(store: Arc<dyn SecretStore>) -> Self {
        Self {
            store,
            staged: Default::default(),
        }
    }
    fn stage(&self, uuid: &str, setting: &str, fields: Fields) {
        self.staged
            .lock()
            .unwrap()
            .insert((uuid.to_owned(), setting.to_owned()), fields);
    }
    fn take_staged(&self, uuid: &str, setting: &str) -> Option<Fields> {
        self.staged
            .lock()
            .unwrap()
            .remove(&(uuid.to_owned(), setting.to_owned()))
    }
}

/// Candidate secret keys of a setting.
fn secret_keys(setting: &str) -> &'static [&'static str] {
    match setting {
        "802-11-wireless-security" => &[
            "psk",
            "wep-key0",
            "wep-key1",
            "wep-key2",
            "wep-key3",
            "leap-password",
        ],
        "802-1x" => &[
            "password",
            "password-raw",
            "pin",
            "private-key-password",
            "phase2-private-key-password",
        ],
        _ => &[],
    }
}
fn flag_key(key: &str) -> String {
    if key.starts_with("wep-key") {
        "wep-key-flags".to_owned()
    } else {
        format!("{key}-flags")
    }
}
/// A secret's NMSettingSecretFlags (absent means NONE: system-owned).
pub(crate) fn secret_flags(settings: &Settings, setting: &str, key: &str) -> u32 {
    settings
        .get(setting)
        .map(|p| number(p, &flag_key(key)))
        .unwrap_or(0)
}
/// Stored by an agent: AGENT_OWNED and neither NOT_SAVED nor NOT_REQUIRED.
pub(crate) fn agent_owned(flags: u32) -> bool {
    flags & AGENT_OWNED != 0 && flags & (NOT_SAVED | NOT_REQUIRED) == 0
}
fn strings(p: &Props, k: &str) -> Vec<String> {
    p.get(k)
        .and_then(|v| v.try_clone().ok())
        .and_then(|v| Vec::<String>::try_from(v).ok())
        .unwrap_or_default()
}
/// Secrets the setting needs (NetworkManagerQt `needSecrets`), excluding
/// NOT_REQUIRED ones.
fn required_keys(settings: &Settings, setting: &str) -> Vec<String> {
    let Some(p) = settings.get(setting) else {
        return vec![];
    };
    let mut keys: Vec<String> = match setting {
        "802-11-wireless-security" => match text(p, "key-mgmt").as_str() {
            "wpa-psk" | "sae" => vec!["psk".into()],
            "none" => vec![format!("wep-key{}", number(p, "wep-tx-keyidx").min(3))],
            "ieee8021x" if text(p, "auth-alg") == "leap" => vec!["leap-password".into()],
            _ => vec![],
        },
        "802-1x" => {
            if strings(p, "eap").iter().any(|m| m == "tls") {
                vec!["private-key-password".into()]
            } else {
                vec!["password".into()]
            }
        }
        _ => vec![],
    };
    keys.retain(|k| secret_flags(settings, setting, k) & NOT_REQUIRED == 0);
    keys
}
fn connection_text(settings: &Settings, key: &str) -> String {
    settings
        .get("connection")
        .map(|s| text(s, key))
        .unwrap_or_default()
}
fn uuid(settings: &Settings) -> String {
    connection_text(settings, "uuid")
}
/// Secret values NetworkManager itself passed in the connection.
fn secret_fields(settings: &Settings, setting: &str) -> Fields {
    let mut fields = Fields::new();
    if let Some(props) = settings.get(setting) {
        for key in secret_keys(setting) {
            let value = text(props, key);
            if !value.is_empty() {
                fields.insert((*key).into(), Zeroizing::new(value));
            }
        }
    }
    fields
}
/// The subset NetworkManager expects an agent to store.
fn owned_fields(connection: &Settings, setting: &str, fields: &Fields) -> Fields {
    fields
        .iter()
        .filter(|(key, _)| agent_owned(secret_flags(connection, setting, key)))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// GetSecrets for one setting, following plasma-nm/NetworkManagerQt:
/// 1. Without REQUEST_NEW, use secrets NetworkManager passed, then the
///    one-shot secret typed for this activation, then the keyring, the
///    keyring only for keys this profile marks AGENT_OWNED (never NOT_SAVED
///    or NOT_REQUIRED keys).
/// 2. Ask the user when a required secret is still missing or REQUEST_NEW is
///    set, if interaction is allowed. Empty input never counts as a secret.
/// 3. Store newly typed AGENT_OWNED secrets in the keyring (best effort).
async fn resolve_secrets(
    broker: &Broker,
    secrets: &AgentSecrets,
    connection: Settings,
    connection_path: OwnedObjectPath,
    setting_name: String,
    hints: Vec<String>,
    flags: u32,
) -> std::result::Result<SSettings, AgentError> {
    let setting = setting_name.as_str();
    let id = uuid(&connection);
    let request_new = flags & REQUEST_NEW != 0;
    let interactive = flags & ALLOW_INTERACTION != 0;
    let candidates = secret_keys(setting);
    let mut required = required_keys(&connection, setting);
    for hint in &hints {
        if candidates.contains(&hint.as_str())
            && !required.contains(hint)
            && secret_flags(&connection, setting, hint) & NOT_REQUIRED == 0
        {
            required.push(hint.clone());
        }
    }
    // Always consume a staged secret; after REQUEST_NEW it is known to be wrong.
    let staged = if id.is_empty() {
        None
    } else {
        secrets.take_staged(&id, setting)
    };
    let mut fields = Fields::new();
    if !request_new {
        fields = secret_fields(&connection, setting);
        for (k, v) in staged.into_iter().flatten() {
            fields.entry(k).or_insert(v);
        }
        if !id.is_empty() {
            if let Ok(stored) = secrets.store.load(&id, setting, interactive).await {
                for (k, v) in stored {
                    if agent_owned(secret_flags(&connection, setting, &k)) {
                        fields.entry(k).or_insert(v);
                    }
                }
            }
        }
    }
    // plasma-nm asks only for REQUEST_NEW or when needSecrets() is non-empty.
    let missing = required.iter().any(|k| !fields.contains_key(k));
    let mut typed_by_user = false;
    if request_new || missing {
        if !interactive {
            return Err(AgentError::NoSecrets(
                "No saved secrets and interaction is disallowed".into(),
            ));
        }
        let mut reply = broker
            .request(Kind::NetworkSecrets {
                connection: connection_path.to_string(),
                setting: setting_name.clone(),
                hints: if hints.is_empty() {
                    required.clone()
                } else {
                    hints
                },
                new_secrets: request_new,
            })
            .await
            .map_err(|end| match end {
                PromptEnd::Cancelled => {
                    AgentError::AgentCanceled("Secret request withdrawn".into())
                }
                PromptEnd::TimedOut => AgentError::NoSecrets("Secret prompt timed out".into()),
            })?;
        let Some(typed) = reply.take_fields() else {
            return Err(AgentError::UserCanceled(
                "Secret request cancelled by the user".into(),
            ));
        };
        for (k, v) in typed {
            if !v.is_empty() && candidates.contains(&k.as_str()) {
                fields.insert(k, v);
            }
        }
        typed_by_user = true;
    }
    if fields.is_empty() || required.iter().any(|k| !fields.contains_key(k)) {
        return Err(AgentError::NoSecrets("No secrets supplied".into()));
    }
    if typed_by_user && !id.is_empty() {
        let owned = owned_fields(&connection, setting, &fields);
        if !owned.is_empty() {
            // Best effort, like plasma-nm: a locked/absent keyring must not
            // fail an activation the user just supplied secrets for.
            let _ = secrets
                .store
                .save(&id, &connection_text(&connection, "id"), setting, &owned)
                .await;
        }
    }
    let mut values: SProps = fields
        .into_iter()
        .map(|(k, v)| (k, SValue::Secret(v)))
        .collect();
    values.insert("name".into(), SValue::plain(setting_name.clone()));
    Ok(SSettings::from([(setting_name, values)]))
}

/// One outstanding GetSecrets; CancelGetSecrets flips it.
#[derive(Default)]
struct Cancel {
    flag: std::sync::atomic::AtomicBool,
    notify: tokio::sync::Notify,
}
impl Cancel {
    fn cancel(&self) {
        self.flag.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }
    async fn cancelled(&self) {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.flag.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }
}
type PendingMap = Arc<std::sync::Mutex<HashMap<(String, String), Vec<Arc<Cancel>>>>>;
struct PendingGuard {
    map: PendingMap,
    key: (String, String),
    token: Arc<Cancel>,
}
impl Drop for PendingGuard {
    fn drop(&mut self) {
        let mut map = self.map.lock().unwrap();
        if let Some(list) = map.get_mut(&self.key) {
            list.retain(|t| !Arc::ptr_eq(t, &self.token));
            if list.is_empty() {
                map.remove(&self.key);
            }
        }
    }
}
struct SecretAgent {
    connection: Connection,
    broker: Broker,
    secrets: Arc<AgentSecrets>,
    pending: PendingMap,
}
#[zbus::interface(name = "org.freedesktop.NetworkManager.SecretAgent")]
impl SecretAgent {
    async fn get_secrets(
        &self,
        connection: Settings,
        connection_path: OwnedObjectPath,
        setting_name: String,
        hints: Vec<String>,
        flags: u32,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> std::result::Result<SSettings, AgentError> {
        // Registered before the first await so a following CancelGetSecrets
        // always finds it, whichever stage (keyring, prompt) is running.
        let key = (connection_path.to_string(), setting_name.clone());
        let token = Arc::new(Cancel::default());
        self.pending
            .lock()
            .unwrap()
            .entry(key.clone())
            .or_default()
            .push(token.clone());
        let _guard = PendingGuard {
            map: self.pending.clone(),
            key,
            token: token.clone(),
        };
        bus::authorize(&self.connection, NM, &header)
            .await
            .map_err(AgentError::PermissionDenied)?;
        if !SECRET_SETTINGS.contains(&setting_name.as_str()) {
            return Err(AgentError::NoSecrets(format!(
                "No secrets handled for setting {setting_name}"
            )));
        }
        tokio::select! {
            _ = token.cancelled() => Err(AgentError::AgentCanceled("Request cancelled by NetworkManager".into())),
            result = resolve_secrets(
                &self.broker,
                &self.secrets,
                connection,
                connection_path,
                setting_name,
                hints,
                flags,
            ) => result,
        }
    }

    async fn cancel_get_secrets(
        &self,
        connection_path: OwnedObjectPath,
        setting_name: String,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> std::result::Result<(), AgentError> {
        bus::authorize(&self.connection, NM, &header)
            .await
            .map_err(AgentError::PermissionDenied)?;
        let key = (connection_path.to_string(), setting_name.clone());
        for token in self
            .pending
            .lock()
            .unwrap()
            .remove(&key)
            .unwrap_or_default()
        {
            token.cancel();
        }
        self.broker.cancel_where(|p| matches!(&p.kind, Kind::NetworkSecrets { connection, setting, .. } if connection == connection_path.as_str() && setting == &setting_name));
        Ok(())
    }
    async fn save_secrets(
        &self,
        connection: Settings,
        _connection_path: OwnedObjectPath,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> std::result::Result<(), AgentError> {
        bus::authorize(&self.connection, NM, &header)
            .await
            .map_err(AgentError::PermissionDenied)?;
        let id = uuid(&connection);
        if id.is_empty() {
            return Err(AgentError::InvalidConnection(
                "Missing connection UUID".into(),
            ));
        }
        let name = connection_text(&connection, "id");
        for setting in SECRET_SETTINGS {
            if !connection.contains_key(*setting) {
                continue;
            }
            let fields = owned_fields(&connection, setting, &secret_fields(&connection, setting));
            self.secrets
                .store
                .save(&id, &name, setting, &fields)
                .await
                .map_err(AgentError::Failed)?;
            // Secrets that are no longer agent-owned must not linger.
            let stale: Vec<String> = secret_keys(setting)
                .iter()
                .filter(|k| !agent_owned(secret_flags(&connection, setting, k)))
                .map(|k| (*k).to_owned())
                .collect();
            self.secrets
                .store
                .remove(&id, setting, &stale)
                .await
                .map_err(AgentError::Failed)?;
        }
        Ok(())
    }
    async fn delete_secrets(
        &self,
        connection: Settings,
        _connection_path: OwnedObjectPath,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> std::result::Result<(), AgentError> {
        bus::authorize(&self.connection, NM, &header)
            .await
            .map_err(AgentError::PermissionDenied)?;
        let id = uuid(&connection);
        if id.is_empty() {
            return Err(AgentError::InvalidConnection(
                "Missing connection UUID".into(),
            ));
        }
        self.secrets
            .store
            .delete(&id)
            .await
            .map_err(AgentError::Failed)
    }
}

#[cfg(test)]
#[path = "network_tests.rs"]
mod tests;
