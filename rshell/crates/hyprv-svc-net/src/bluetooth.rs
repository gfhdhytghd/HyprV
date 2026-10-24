//! BluezQt-style ObjectManager model and a native KeyboardDisplay Agent1.
use crate::{
    bus::{self, *},
    prompts::{Broker, Kind, Reply},
    rfkill,
    secret::SecretString,
    Context, PendingAction, Result,
};
use async_trait::async_trait;
use hyprv_core::store::{Topic, TopicKey};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinSet,
};
use zbus::{zvariant::OwnedObjectPath, Connection};
const ADAPTER: &str = "org.bluez.Adapter1";
const DEVICE: &str = "org.bluez.Device1";
const AGENT: &str = "/org/hyprv/Shell/BluetoothAgent";
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub adapter: String,
    pub address: String,
    pub name: String,
    pub display_name: String,
    pub has_name: bool,
    pub icon: String,
    pub paired: bool,
    pub bonded: bool,
    pub trusted: bool,
    pub connected: bool,
    pub blocked: bool,
    pub pairing: bool,
    pub rssi: Option<i16>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub available: bool,
    pub present: bool,
    pub adapter: Option<String>,
    pub enabled: bool,
    pub discovering: bool,
    pub pairable: bool,
    pub devices: Vec<Device>,
    pub rfkill: rfkill::State,
    pub busy: bool,
    pub action_address: Option<String>,
    pub message: String,
    pub error: Option<String>,
}
pub struct StateTopic;
impl TopicKey for StateTopic {
    type Value = State;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    SetPower(bool),
    Scan,
    StopScan,
    Connect { address: String, paired: bool },
    Disconnect { address: String },
    Remove { address: String },
    SetTrusted { address: String, trusted: bool },
    SetBlocked { address: String, blocked: bool },
    CancelPairing { address: String },
}
#[async_trait]
pub trait Transport: Send + Sync {
    async fn snapshot(&self) -> Result<State>;
    async fn execute(&self, action: Action) -> Result<String>;
    async fn next_event(&self) -> Event;
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
    broker: Broker,
    actions: mpsc::Sender<(Action, oneshot::Sender<Result<()>>)>,
}
impl Handle {
    pub async fn dispatch(&self, action: Action) -> Result<PendingAction> {
        let (tx, rx) = oneshot::channel();
        self.actions
            .send((action, tx))
            .await
            .map_err(|_| "Bluetooth service stopped")?;
        Ok(PendingAction(rx))
    }
    pub fn respond(&self, id: u64, reply: Reply) -> Result<()> {
        self.broker.respond(id, reply)
    }
}
pub fn spawn(ctx: Context) -> Handle {
    spawn_inner(ctx, Broker::new(), None)
}
pub fn spawn_with_transport(ctx: Context, transport: Arc<dyn Transport>, broker: Broker) -> Handle {
    spawn_inner(ctx, broker, Some(transport))
}
fn spawn_inner(ctx: Context, broker: Broker, transport: Option<Arc<dyn Transport>>) -> Handle {
    let state = ctx.store.topic::<StateTopic>(State::default());
    let (tx, rx) = mpsc::channel(32);
    let handle = Handle {
        state: state.clone(),
        prompts: broker.topic(),
        broker: broker.clone(),
        actions: tx,
    };
    let real_transport = transport.is_none();
    let setup = ctx.clone();
    let readonly = ctx.read_only;
    let shutdown = ctx.runtime.shutdown_signal();
    crate::supervise(&ctx, async move {
        let transport = match transport {
            Some(t) => Ok(t),
            None => Native::new(&setup, broker.clone())
                .await
                .map(|n| Arc::new(n) as Arc<dyn Transport>),
        };
        match transport {
            Ok(t) => run(t, state, rx, broker, readonly, shutdown, real_transport).await,
            Err(e) => {
                state.publish(State {
                    error: Some(e),
                    ..State::default()
                });
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
    readonly: bool,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
    monitor_rfkill: bool,
) {
    let mut jobs = JoinSet::new();
    let mut rf = if monitor_rfkill {
        rfkill::Watcher::new(2).ok()
    } else {
        None
    };
    let mut rf_state = rfkill::State::default();
    if rf.is_none() {
        rf_state.error = Some("/dev/rfkill unavailable".into());
    }
    let mut busy = 0usize;
    let mut message = String::new();
    let mut error = None;
    let mut address = None;
    let mut scan_owned = false;
    let mut scan_deadline = None;
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
            s.busy = busy > 0;
            s.message = message.clone();
            s.action_address = address.clone();
            if let Some(a) = &address {
                if let Some(d) = s.devices.iter_mut().find(|d| &d.address == a) {
                    d.pairing = busy > 0 && !d.paired;
                }
            }
            if error.is_some() {
                s.error = error.clone();
            }
            state.publish(s);
            refresh = false;
        }
        tokio::select! {
            _=shutdown.changed()=>break,
            request=actions.recv()=>match request{
                Some((action,reply))=>{
                    if readonly{let _=reply.send(Err("Bluetooth service is read-only".into()));continue;}
                    if busy>0&&!matches!(action,Action::CancelPairing{..}){let _=reply.send(Err("Bluetooth action already in progress".into()));continue;}
                    let scan=action==Action::Scan;let stop=action==Action::StopScan;
                    address=match &action{Action::Connect{address,..}|Action::Disconnect{address}|Action::Remove{address}|Action::SetTrusted{address,..}|Action::SetBlocked{address,..}|Action::CancelPairing{address}=>Some(address.clone()),_=>None};
                    busy+=1;error=None;refresh=true;let t=transport.clone();
                    let b=broker.clone();
                    jobs.spawn(async move{
                        let cancel=match &action{Action::Connect{address,..}=>Some(address.clone()),_=>None};
                        let r=match tokio::time::timeout(Duration::from_secs(12),t.execute(action)).await{
                            Ok(r)=>r,Err(_)=>{if let Some(address)=cancel{let _=t.execute(Action::CancelPairing{address}).await;}b.cancel_all();Err("Bluetooth action timed out".into())}
                        };let completion=r.as_ref().map(|_|()).map_err(Clone::clone);let _=reply.send(completion);(scan,stop,r)});
                },None=>break,
            },
            result=jobs.join_next(),if !jobs.is_empty()=>{
                busy=busy.saturating_sub(1);
                match result{
                    Some(Ok((scan,stop,Ok(m))))=>{message=m;error=None;if scan{scan_owned=true;scan_deadline=Some(tokio::time::Instant::now()+Duration::from_secs(4));}
                    if stop{scan_owned=false;scan_deadline=None;}},
                    Some(Ok((_,_,Err(e))))=>error=Some(e),Some(Err(e))=>error=Some(e.to_string()),None=>{},
                }refresh=true;
            },
            _=async{tokio::time::sleep_until(scan_deadline.unwrap()).await},if scan_deadline.is_some()=>{
                scan_deadline=None;busy+=1;let t=transport.clone();jobs.spawn(async move{(false,true,t.execute(Action::StopScan).await)});
            },
            event=transport.next_event()=>match event{
                Event::Changed=>refresh=true,
                Event::OwnerChanged=>{jobs.abort_all();busy=0;broker.cancel_all();scan_deadline=None;scan_owned=false;error=transport.owner_changed().await.err();refresh=true;},
                Event::OwnerLost=>{jobs.abort_all();busy=0;broker.cancel_all();scan_deadline=None;scan_owned=false;transport.owner_lost().await;error=None;refresh=true;},
                Event::Disconnected(e)=>{let mut s=state.snapshot().value;s.available=false;s.error=Some(e);state.publish(s);break;},
            },
            result=async{rf.as_mut().unwrap().next().await},if rf.is_some()=>{
                match result{Ok(s)=>rf_state=s,Err(e)=>{rf=None;rf_state.available=false;rf_state.error=Some(e);}}refresh=true;
            },
        }
    }
    jobs.abort_all();
    broker.cancel_all();
    if scan_owned {
        let _ = transport.execute(Action::StopScan).await;
    }
    transport.shutdown().await;
}

pub struct Native {
    connection: Connection,
    watch: Watch,
    read_only: bool,
    registration_error: std::sync::Mutex<Option<String>>,
    discovery_owned: std::sync::atomic::AtomicBool,
    pairing_device: std::sync::Mutex<Option<String>>,
    /// `org.bluez` currently has an owner. While false nothing is sent to it,
    /// so the shell never D-Bus-activates a stopped bluetoothd.
    present: std::sync::atomic::AtomicBool,
}
impl Native {
    pub async fn new(ctx: &Context, broker: Broker) -> Result<Self> {
        let c = match &ctx.system_bus {
            Some(c) => c.clone(),
            None => Connection::system().await.map_err(err)?,
        };
        let watch = Watch::new(&c, BLUEZ).await?;
        let present = bus::has_owner(&c, BLUEZ).await;
        if !ctx.read_only {
            c.object_server()
                .at(
                    AGENT,
                    PairingAgent {
                        connection: c.clone(),
                        broker,
                    },
                )
                .await
                .map_err(err)?;
        }
        let native = Self {
            connection: c,
            watch,
            read_only: ctx.read_only,
            registration_error: std::sync::Mutex::new(None),
            discovery_owned: std::sync::atomic::AtomicBool::new(false),
            pairing_device: std::sync::Mutex::new(None),
            present: std::sync::atomic::AtomicBool::new(present),
        };
        if !ctx.read_only {
            *native.registration_error.lock().unwrap() = native.register().await.err();
        }
        Ok(native)
    }
    fn present(&self) -> bool {
        self.present.load(std::sync::atomic::Ordering::Acquire)
    }
    async fn register(&self) -> Result<()> {
        // Absent daemon: registration happens when it appears (OwnerChanged).
        if self.read_only || !self.present() {
            return Ok(());
        }
        let p = proxy(
            &self.connection,
            BLUEZ,
            "/org/bluez",
            "org.bluez.AgentManager1",
        )
        .await?;
        let path = OwnedObjectPath::try_from(AGENT).map_err(err)?;
        bus::call::<_, ()>(&p, "RegisterAgent", &(path.clone(), "KeyboardDisplay")).await?;
        bus::call::<_, ()>(&p, "RequestDefaultAgent", &(path,)).await
    }
    async fn objects(&self) -> Result<Objects> {
        if !self.present() {
            return Err("Bluetooth service (org.bluez) is not running".into());
        }
        let p = proxy(
            &self.connection,
            BLUEZ,
            "/",
            "org.freedesktop.DBus.ObjectManager",
        )
        .await?;
        bus::call(&p, "GetManagedObjects", &()).await
    }
}
#[async_trait]
impl Transport for Native {
    async fn snapshot(&self) -> Result<State> {
        let mut state = state_from_objects(&self.objects().await?);
        state.error = self.registration_error.lock().unwrap().clone();
        Ok(state)
    }
    async fn execute(&self, action: Action) -> Result<String> {
        if self.read_only {
            return Err("Bluetooth service is read-only".into());
        }
        if !self.present() {
            return Err("Bluetooth service (org.bluez) is not running".into());
        }
        let state = self.snapshot().await?;
        let c = &self.connection;
        let adapter = state
            .adapter
            .as_deref()
            .ok_or("No Bluetooth controller found")?;
        let device = |address: &str| {
            state
                .devices
                .iter()
                .find(|d| d.address.eq_ignore_ascii_case(address))
                .ok_or_else(|| format!("Unknown Bluetooth device: {address}"))
        };
        match action {
            Action::SetPower(enabled) => {
                proxy(c, BLUEZ, adapter, ADAPTER)
                    .await?
                    .set_property("Powered", enabled)
                    .await
                    .map_err(err)?;
                Ok(if enabled {
                    "Bluetooth enabled"
                } else {
                    "Bluetooth disabled"
                }
                .into())
            }
            Action::Scan => {
                if !state.enabled {
                    return Err("Bluetooth is turned off".into());
                }
                if !self
                    .discovery_owned
                    .load(std::sync::atomic::Ordering::Relaxed)
                {
                    call_unit(c, BLUEZ, adapter, ADAPTER, "StartDiscovery").await?;
                    self.discovery_owned
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                }
                Ok("Bluetooth scan started".into())
            }
            Action::StopScan => {
                if self
                    .discovery_owned
                    .load(std::sync::atomic::Ordering::Relaxed)
                {
                    call_unit(c, BLUEZ, adapter, ADAPTER, "StopDiscovery").await?;
                    self.discovery_owned
                        .store(false, std::sync::atomic::Ordering::Relaxed);
                }
                Ok("Bluetooth scan complete".into())
            }
            Action::Connect { address, paired: _ } => {
                let d = device(&address)?;
                let p = proxy(c, BLUEZ, &d.id, DEVICE).await?;
                if d.blocked {
                    return Err("Bluetooth device is blocked".into());
                }
                if !d.paired {
                    *self.pairing_device.lock().unwrap() = Some(d.id.clone());
                    let pairing = tokio::time::timeout(
                        Duration::from_secs(120),
                        p.call::<_, _, ()>("Pair", &()),
                    )
                    .await;
                    *self.pairing_device.lock().unwrap() = None;
                    match pairing {
                        Ok(Ok(())) => {}
                        Ok(Err(e)) => return Err(e.to_string()),
                        Err(_) => {
                            let _ = p.call::<_, _, ()>("CancelPairing", &()).await;
                            return Err("Bluetooth pairing timed out".into());
                        }
                    }
                    p.set_property("Trusted", true).await.map_err(err)?;
                }
                tokio::time::timeout(Duration::from_secs(12), p.call::<_, _, ()>("Connect", &()))
                    .await
                    .map_err(|_| "Bluetooth connect timed out")?
                    .map_err(err)?;
                Ok(format!("Connected to {address}"))
            }
            Action::Disconnect { address } => {
                let d = device(&address)?;
                call_unit(c, BLUEZ, &d.id, DEVICE, "Disconnect").await?;
                Ok(format!("Disconnected {address}"))
            }
            Action::Remove { address } => {
                let d = device(&address)?;
                proxy(c, BLUEZ, &d.adapter, ADAPTER)
                    .await?
                    .call::<_, _, ()>(
                        "RemoveDevice",
                        &(OwnedObjectPath::try_from(d.id.clone()).map_err(err)?,),
                    )
                    .await
                    .map_err(err)?;
                Ok(format!("Removed {address}"))
            }
            Action::SetTrusted { address, trusted } => {
                proxy(c, BLUEZ, &device(&address)?.id, DEVICE)
                    .await?
                    .set_property("Trusted", trusted)
                    .await
                    .map_err(err)?;
                Ok("Trust changed".into())
            }
            Action::SetBlocked { address, blocked } => {
                proxy(c, BLUEZ, &device(&address)?.id, DEVICE)
                    .await?
                    .set_property("Blocked", blocked)
                    .await
                    .map_err(err)?;
                Ok("Block changed".into())
            }
            Action::CancelPairing { address } => {
                call_unit(c, BLUEZ, &device(&address)?.id, DEVICE, "CancelPairing").await?;
                Ok("Pairing cancelled".into())
            }
        }
    }
    async fn next_event(&self) -> Event {
        self.watch.next().await
    }
    async fn owner_changed(&self) -> Result<()> {
        self.discovery_owned
            .store(false, std::sync::atomic::Ordering::Relaxed);
        *self.pairing_device.lock().unwrap() = None;
        self.present
            .store(true, std::sync::atomic::Ordering::Release);
        let result = self.register().await;
        *self.registration_error.lock().unwrap() = result.as_ref().err().cloned();
        result
    }
    async fn owner_lost(&self) {
        self.present
            .store(false, std::sync::atomic::Ordering::Release);
        self.discovery_owned
            .store(false, std::sync::atomic::Ordering::Relaxed);
        *self.pairing_device.lock().unwrap() = None;
        *self.registration_error.lock().unwrap() = None;
    }
    async fn shutdown(&self) {
        if !self.read_only && self.present() {
            let pairing = self.pairing_device.lock().unwrap().take();
            if let Some(device) = pairing {
                let _ = call_unit(&self.connection, BLUEZ, &device, DEVICE, "CancelPairing").await;
            }
            if self
                .discovery_owned
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                let _ = self.execute(Action::StopScan).await;
            }
            if let Ok(p) = proxy(
                &self.connection,
                BLUEZ,
                "/org/bluez",
                "org.bluez.AgentManager1",
            )
            .await
            {
                let _ = bus::call::<_, ()>(
                    &p,
                    "UnregisterAgent",
                    &(OwnedObjectPath::try_from(AGENT).unwrap(),),
                )
                .await;
            }
            let _ = self
                .connection
                .object_server()
                .remove::<PairingAgent, _>(AGENT)
                .await;
        }
    }
}
pub fn state_from_objects(objects: &Objects) -> State {
    let mut adapters: Vec<_> = objects
        .iter()
        .filter_map(|(path, i)| i.get(ADAPTER).map(|p| (path, p)))
        .collect();
    adapters.sort_by(|a, b| {
        boolean(b.1, "Powered")
            .cmp(&boolean(a.1, "Powered"))
            .then(a.0.cmp(b.0))
    });
    let Some((adapter, p)) = adapters.first() else {
        return State {
            available: true,
            ..State::default()
        };
    };
    let mut state = State {
        available: true,
        present: true,
        adapter: Some(adapter.to_string()),
        enabled: boolean(p, "Powered"),
        discovering: boolean(p, "Discovering"),
        pairable: boolean(p, "Pairable"),
        ..State::default()
    };
    for (path, interfaces) in objects {
        let Some(p) = interfaces.get(DEVICE) else {
            continue;
        };
        if bus::path(p, "Adapter").as_deref() != Some(adapter.as_str()) {
            continue;
        }
        let address = text(p, "Address");
        let name = text(p, "Alias");
        let name = if name.is_empty() {
            text(p, "Name")
        } else {
            name
        };
        let normalized = |s: &str| {
            s.chars()
                .filter(|c| *c != ':' && *c != '-')
                .flat_map(char::to_lowercase)
                .collect::<String>()
        };
        let name = if normalized(&name) == normalized(&address) {
            String::new()
        } else {
            name.trim().to_owned()
        };
        state.devices.push(Device {
            id: path.to_string(),
            adapter: bus::path(p, "Adapter").unwrap_or_else(|| adapter.to_string()),
            address: address.clone(),
            display_name: if name.is_empty() {
                address
            } else {
                name.clone()
            },
            has_name: !name.is_empty(),
            name,
            icon: if text(p, "Icon").is_empty() {
                "bluetooth".into()
            } else {
                text(p, "Icon")
            },
            paired: boolean(p, "Paired"),
            bonded: boolean(p, "Bonded"),
            trusted: boolean(p, "Trusted"),
            connected: boolean(p, "Connected"),
            blocked: boolean(p, "Blocked"),
            pairing: false,
            rssi: p.get("RSSI").and_then(|v| i16::try_from(v).ok()),
        });
    }
    state.devices.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then(b.paired.cmp(&a.paired))
            .then(b.trusted.cmp(&a.trusted))
            .then(
                a.display_name
                    .to_lowercase()
                    .cmp(&b.display_name.to_lowercase()),
            )
            .then(a.address.cmp(&b.address))
    });
    state
}
#[derive(zbus::DBusError, Debug)]
#[zbus(prefix = "org.bluez.Error")]
enum AgentError {
    Rejected(String),
    Canceled(String),
}
struct PairingAgent {
    connection: Connection,
    broker: Broker,
}
impl PairingAgent {
    async fn auth(
        &self,
        header: &zbus::message::Header<'_>,
    ) -> std::result::Result<(), AgentError> {
        bus::authorize(&self.connection, BLUEZ, header)
            .await
            .map_err(AgentError::Rejected)
    }
    async fn accept(&self, kind: Kind) -> std::result::Result<(), AgentError> {
        let reply = self
            .broker
            .request(kind)
            .await
            .map_err(|e| AgentError::Canceled(e.to_string()))?;
        if reply.is_accept() {
            Ok(())
        } else {
            Err(AgentError::Rejected("Pairing rejected".into()))
        }
    }
}
#[zbus::interface(name = "org.bluez.Agent1")]
impl PairingAgent {
    async fn release(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> std::result::Result<(), AgentError> {
        self.auth(&header).await?;
        self.broker.cancel_all();
        Ok(())
    }
    async fn cancel(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> std::result::Result<(), AgentError> {
        self.auth(&header).await?;
        self.broker.cancel_all();
        Ok(())
    }
    async fn request_pin_code(
        &self,
        device: OwnedObjectPath,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> std::result::Result<SecretString, AgentError> {
        self.auth(&header).await?;
        let mut reply = self
            .broker
            .request(Kind::Pin {
                device: device.to_string(),
            })
            .await
            .map_err(|e| AgentError::Canceled(e.to_string()))?;
        match reply.take_pin() {
            Some(pin) if !pin.is_empty() && pin.len() <= 16 => Ok(SecretString(pin)),
            _ => Err(AgentError::Rejected("PIN must contain 1..16 bytes".into())),
        }
    }
    async fn request_passkey(
        &self,
        device: OwnedObjectPath,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> std::result::Result<u32, AgentError> {
        self.auth(&header).await?;
        match self
            .broker
            .request(Kind::Passkey {
                device: device.to_string(),
            })
            .await
            .map_err(|e| AgentError::Canceled(e.to_string()))?
        {
            Reply::Passkey(key) if key <= 999999 => Ok(key),
            _ => Err(AgentError::Rejected("Invalid passkey".into())),
        }
    }
    async fn display_pin_code(
        &self,
        device: OwnedObjectPath,
        pincode: String,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> std::result::Result<(), AgentError> {
        self.auth(&header).await?;
        self.broker.display(Kind::DisplayPin {
            device: device.to_string(),
            pin: pincode,
        });
        Ok(())
    }
    async fn display_passkey(
        &self,
        device: OwnedObjectPath,
        passkey: u32,
        entered: u16,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> std::result::Result<(), AgentError> {
        self.auth(&header).await?;
        self.broker.display(Kind::DisplayPasskey {
            device: device.to_string(),
            passkey,
            entered,
        });
        Ok(())
    }
    async fn request_confirmation(
        &self,
        device: OwnedObjectPath,
        passkey: u32,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> std::result::Result<(), AgentError> {
        self.auth(&header).await?;
        self.accept(Kind::Confirmation {
            device: device.to_string(),
            passkey,
        })
        .await
    }
    async fn request_authorization(
        &self,
        device: OwnedObjectPath,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> std::result::Result<(), AgentError> {
        self.auth(&header).await?;
        self.accept(Kind::Authorization {
            device: device.to_string(),
            service: None,
        })
        .await
    }
    async fn authorize_service(
        &self,
        device: OwnedObjectPath,
        uuid: String,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> std::result::Result<(), AgentError> {
        self.auth(&header).await?;
        self.accept(Kind::Authorization {
            device: device.to_string(),
            service: Some(uuid),
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    #[test]
    fn object_manager_selection_fields_sort_and_removal() {
        let mut objects = Objects::new();
        let adapter = OwnedObjectPath::try_from("/org/bluez/hci0").unwrap();
        objects.insert(
            adapter.clone(),
            HashMap::from([(
                ADAPTER.into(),
                HashMap::from([
                    ("Powered".into(), true.into()),
                    ("Pairable".into(), true.into()),
                ]),
            )]),
        );
        let props = |address: &str, name: &str, connected: bool| {
            HashMap::from([
                ("Address".into(), value_string(address)),
                ("Name".into(), value_string(name)),
                (
                    "Adapter".into(),
                    zbus::zvariant::OwnedValue::try_from(zbus::zvariant::Value::from(
                        adapter.clone(),
                    ))
                    .unwrap(),
                ),
                ("Connected".into(), connected.into()),
                ("RSSI".into(), (-42i16).into()),
            ])
        };
        let a = OwnedObjectPath::try_from("/org/bluez/hci0/dev_a").unwrap();
        let b = OwnedObjectPath::try_from("/org/bluez/hci0/dev_b").unwrap();
        objects.insert(
            a.clone(),
            HashMap::from([(
                DEVICE.into(),
                props("AA:BB:CC:DD:EE:FF", "AA-BB-CC-DD-EE-FF", true),
            )]),
        );
        objects.insert(
            b,
            HashMap::from([(
                DEVICE.into(),
                props("BB:BB:CC:DD:EE:FF", "Headphones", false),
            )]),
        );
        let s = state_from_objects(&objects);
        assert!(s.available && s.present && s.enabled && s.pairable);
        assert_eq!(s.devices[0].address, "AA:BB:CC:DD:EE:FF");
        assert!(!s.devices[0].has_name);
        assert_eq!(s.devices[0].rssi, Some(-42));
        assert_eq!(s.devices[0].icon, "bluetooth");
        objects.remove(&a);
        assert_eq!(state_from_objects(&objects).devices.len(), 1);
        objects.clear();
        let absent = state_from_objects(&objects);
        assert!(absent.available && !absent.present);
    }
    #[test]
    fn qml_display_fields_serialize() {
        let json = serde_json::to_value(Device::default()).unwrap();
        for k in [
            "address",
            "name",
            "displayName",
            "hasName",
            "icon",
            "paired",
            "trusted",
            "connected",
            "blocked",
            "rssi",
        ] {
            assert!(json.get(k).is_some(), "{k}");
        }
    }
}

#[cfg(test)]
mod deadline_tests {
    use super::*;
    use std::sync::Mutex;
    struct Fake {
        state: Mutex<State>,
        calls: Mutex<Vec<Action>>,
        broker: Broker,
    }
    #[async_trait]
    impl Transport for Fake {
        async fn snapshot(&self) -> Result<State> {
            Ok(self.state.lock().unwrap().clone())
        }
        async fn execute(&self, action: Action) -> Result<String> {
            self.calls.lock().unwrap().push(action.clone());
            match action {
                Action::Scan => self.state.lock().unwrap().discovering = true,
                Action::StopScan => self.state.lock().unwrap().discovering = false,
                Action::Connect { .. } => {
                    self.broker
                        .request(Kind::Confirmation {
                            device: "device".into(),
                            passkey: 123,
                        })
                        .await
                        .map_err(|e| e.to_string())?;
                }
                _ => {}
            }
            Ok("ok".into())
        }
        async fn next_event(&self) -> Event {
            std::future::pending().await
        }
    }
    async fn settle() {
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
    }
    #[tokio::test(start_paused = true)]
    async fn scan_stops_at_four_seconds_without_idle_timer() {
        let broker = Broker::new();
        let fake = Arc::new(Fake {
            state: Mutex::new(State {
                available: true,
                ..State::default()
            }),
            calls: Mutex::new(vec![]),
            broker: broker.clone(),
        });
        let topic = Topic::new(State::default());
        let (tx, rx) = mpsc::channel(4);
        let (stop, shutdown) = tokio::sync::watch::channel(false);
        let task = tokio::spawn(run(fake.clone(), topic, rx, broker, false, shutdown, false));
        let (reply, wait) = oneshot::channel();
        tx.send((Action::Scan, reply)).await.unwrap();
        wait.await.unwrap().unwrap();
        settle().await;
        tokio::time::advance(Duration::from_millis(3999)).await;
        settle().await;
        assert_eq!(fake.calls.lock().unwrap().len(), 1);
        tokio::time::advance(Duration::from_millis(1)).await;
        settle().await;
        assert_eq!(
            fake.calls.lock().unwrap().as_slice(),
            [Action::Scan, Action::StopScan]
        );
        tokio::time::advance(Duration::from_secs(3600)).await;
        settle().await;
        assert_eq!(fake.calls.lock().unwrap().len(), 2);
        stop.send_replace(true);
        task.await.unwrap();
    }
    #[tokio::test(start_paused = true)]
    async fn pairing_timeout_cancels_provider_and_prompt() {
        let broker = Broker::new();
        let fake = Arc::new(Fake {
            state: Mutex::new(State {
                available: true,
                ..State::default()
            }),
            calls: Mutex::new(vec![]),
            broker: broker.clone(),
        });
        let topic = Topic::new(State::default());
        let (tx, rx) = mpsc::channel(4);
        let (stop, shutdown) = tokio::sync::watch::channel(false);
        let task = tokio::spawn(run(
            fake.clone(),
            topic,
            rx,
            broker.clone(),
            false,
            shutdown,
            false,
        ));
        let (reply, wait) = oneshot::channel();
        tx.send((
            Action::Connect {
                address: "AA".into(),
                paired: false,
            },
            reply,
        ))
        .await
        .unwrap();
        settle().await;
        assert_eq!(broker.topic().snapshot().value.len(), 1);
        tokio::time::advance(Duration::from_secs(12)).await;
        settle().await;
        assert!(wait.await.unwrap().is_err());
        assert!(broker.topic().snapshot().value.is_empty());
        assert!(fake.calls.lock().unwrap().contains(&Action::CancelPairing {
            address: "AA".into()
        }));
        stop.send_replace(true);
        task.await.unwrap();
    }
}

#[cfg(test)]
#[path = "bluetooth_tests.rs"]
mod agent_tests;
