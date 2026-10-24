//! StatusNotifier host (replaces Quickshell `SystemTray`, `shell.qml:503`, `TrayButton.qml`,
//! `TrayMenuPopup`, and the `hypr-comp/moon.qml:10` consumer).
//!
//! Registers `org.kde.StatusNotifierHost-<pid>` with the existing watcher and only serves
//! `org.kde.StatusNotifierWatcher` itself when no watcher owns the name (never steals it).
//! Items are re-read on their `New*` signals (coalesced per item). dbusmenu layouts are
//! fetched only while a menu is open (`MenuOpen` … `MenuClose`); `LayoutUpdated` /
//! `ItemsPropertiesUpdated` are only subscribed while at least one menu is open.
pub mod model;
mod wire;

use crate::common::{action_queue, err, Backoff, Context, Error, Handle, Reply, Result};
use futures_util::{FutureExt, StreamExt};
use hyprv_core::store::{Topic, TopicKey};
pub use model::{ButtonType, CheckState, TrayItem, TrayMenu, TrayMenuEntry, TrayPixmap};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::{Arc, Mutex}, time::Duration};
use tokio::sync::{mpsc, watch};
use zbus::{
    fdo,
    message::{Header, Type as MessageType},
    names::{BusName, WellKnownName},
    object_server::SignalEmitter,
    zvariant::{OwnedValue, Value},
    MatchRule, MessageStream,
};

pub const WATCHER_NAME: &str = "org.kde.StatusNotifierWatcher";
pub const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const ITEM_IFACE: &str = "org.kde.StatusNotifierItem";
const MENU_IFACE: &str = "com.canonical.dbusmenu";
const CALL_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_ITEMS: usize = 128;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TrayState {
    /// Registration order (`SystemTray.items`); UI applies `trayItemPriority` sorting.
    pub items: Vec<TrayItem>,
    /// Layouts of currently open menus.
    pub menus: Vec<TrayMenu>,
    pub watcher_available: bool,
    /// True when this process serves the watcher because none existed.
    pub watcher_owned: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TrayAction {
    /// `trayItem.activate()`
    Activate { item: String, x: i32, y: i32 },
    /// `trayItem.secondaryActivate()`
    SecondaryActivate { item: String, x: i32, y: i32 },
    /// `ContextMenu` for items without a dbusmenu.
    ContextMenu { item: String, x: i32, y: i32 },
    /// `trayItem.scroll(delta, horizontal)`
    Scroll { item: String, delta: i32, horizontal: bool },
    /// Menu shown: `AboutToShow(0)`, `GetLayout`, `Event(0,"opened")`; layout kept live.
    MenuOpen { item: String },
    /// Menu hidden: `Event(0,"closed")`; layout dropped.
    MenuClose { item: String },
    /// Submenu hydrate (`AboutToShow(id)` then refetch if requested).
    MenuAboutToShow { item: String, entry: i32 },
    /// `triggerEntry` → `Event(id, "clicked")`; also "hovered".
    MenuEvent { item: String, entry: i32, event: String },
}

pub struct TrayTopic;
impl TopicKey for TrayTopic { type Value = TrayState; }
pub type TrayHandle = Handle<TrayState, TrayAction>;

pub fn spawn(ctx: &Context) -> TrayHandle {
    let topic = ctx.store.topic::<TrayTopic>(TrayState::default());
    let (actions, queue) = action_queue::<TrayAction, ()>();
    let buses = ctx.buses();
    let mut shutdown = ctx.runtime.shutdown_signal();
    let publish = topic.clone();
    let task = ctx.runtime.spawn(async move {
        let mut queue = queue;
        let mut backoff = Backoff::new();
        loop {
            if let Ok(conn) = buses.session().await {
                backoff.reset();
                let mut actor = Actor::new(conn, publish.clone(), buses.read_only);
                if actor.run(&mut queue, &mut shutdown).await.is_ok() {
                    return;
                }
                publish.publish(TrayState::default());
            }
            let sleep = tokio::time::sleep(backoff.next_delay());
            tokio::pin!(sleep);
            loop {
                tokio::select! {
                    _ = &mut sleep => break,
                    r = queue.recv() => match r {
                        Some((_, reply)) => { let _ = reply.send(err("session bus unavailable")); }
                        None => return,
                    },
                    _ = shutdown.changed() => return,
                }
            }
        }
    });
    Handle::new(topic, actions, task)
}

// ---------------------------------------------------------------- watcher server

#[derive(Default)]
struct Registry {
    items: Vec<String>,
    hosts: Vec<String>,
}

/// KDE `StatusNotifierWatcher` semantics, served only when the name is free.
pub struct Watcher {
    registry: Arc<Mutex<Registry>>,
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    async fn register_status_notifier_item(
        &self,
        service: String,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        let sender = header.sender().map(|s| s.to_string()).unwrap_or_default();
        let key = model::registration_key(&service, &sender);
        let added = {
            let mut r = self.registry.lock().unwrap();
            if r.items.contains(&key) || r.items.len() >= MAX_ITEMS {
                false
            } else {
                r.items.push(key.clone());
                true
            }
        };
        if added {
            Self::status_notifier_item_registered(&emitter, &key).await?;
            self.registered_status_notifier_items_changed(&emitter).await?;
        }
        Ok(())
    }

    async fn register_status_notifier_host(
        &self,
        service: String,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        let first = {
            let mut r = self.registry.lock().unwrap();
            let first = r.hosts.is_empty();
            if !r.hosts.contains(&service) { r.hosts.push(service) }
            first
        };
        if first {
            Self::status_notifier_host_registered(&emitter).await?;
            self.is_status_notifier_host_registered_changed(&emitter).await?;
        }
        Ok(())
    }

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> { self.registry.lock().unwrap().items.clone() }
    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool { !self.registry.lock().unwrap().hosts.is_empty() }
    #[zbus(property)]
    fn protocol_version(&self) -> i32 { 0 }

    #[zbus(signal)]
    async fn status_notifier_item_registered(emitter: &SignalEmitter<'_>, service: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn status_notifier_item_unregistered(emitter: &SignalEmitter<'_>, service: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn status_notifier_host_registered(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn status_notifier_host_unregistered(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

/// Drop registrations owned by a vanished bus name; emits the KDE signals.
async fn watcher_name_vanished(conn: &zbus::Connection, registry: &Arc<Mutex<Registry>>, name: &str) {
    let (gone_items, hosts_empty_now) = {
        let mut r = registry.lock().unwrap();
        let gone: Vec<String> = r.items.iter().filter(|k| model::split_key(k).0 == name).cloned().collect();
        r.items.retain(|k| model::split_key(k).0 != name);
        let had = !r.hosts.is_empty();
        r.hosts.retain(|h| h != name);
        (gone, had && r.hosts.is_empty())
    };
    let Ok(iface) = conn.object_server().interface::<_, Watcher>(WATCHER_PATH).await else { return };
    let emitter = iface.signal_emitter();
    for key in &gone_items {
        let _ = Watcher::status_notifier_item_unregistered(emitter, key).await;
    }
    if !gone_items.is_empty() {
        let _ = iface.get().await.registered_status_notifier_items_changed(emitter).await;
    }
    if hosts_empty_now {
        let _ = Watcher::status_notifier_host_unregistered(emitter).await;
        let _ = iface.get().await.is_status_notifier_host_registered_changed(emitter).await;
    }
}

// ---------------------------------------------------------------- host actor

struct ItemSlot {
    item: TrayItem,
    owner: String,
    fetching: bool,
    dirty: bool,
}

enum Internal {
    Fetched { key: String, owner: String, props: std::result::Result<wire::Properties, Error> },
    /// `None` root = the request failed (still clears the in-flight flag).
    Layout { key: String, generation: u64, serial: u64, revision: u32, root: Option<TrayMenuEntry> },
}

struct Actor {
    conn: zbus::Connection,
    topic: Topic<TrayState>,
    read_only: bool,
    host_name: String,
    registry: Option<Arc<Mutex<Registry>>>,
    items: HashMap<String, ItemSlot>,
    next_order: u64,
    menus: HashMap<String, TrayMenu>,
    /// MEDIA-3: per open menu, one GetLayout in flight; replies carry the open-menu
    /// generation and request serial, so stale or out-of-order replies are dropped.
    menu_meta: HashMap<String, MenuMeta>,
    next_menu_generation: u64,
    watcher_available: bool,
    tx: mpsc::UnboundedSender<Internal>,
    rx: mpsc::UnboundedReceiver<Internal>,
}

impl Actor {
    fn new(conn: zbus::Connection, topic: Topic<TrayState>, read_only: bool) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let host_name = format!("org.kde.StatusNotifierHost-{}-hyprv", std::process::id());
        Self {
            conn, topic, read_only, host_name, registry: None, items: HashMap::new(), next_order: 0,
            menus: HashMap::new(), menu_meta: HashMap::new(), next_menu_generation: 0, watcher_available: false, tx, rx,
        }
    }

    async fn run(&mut self, queue: &mut mpsc::Receiver<(TrayAction, Reply<()>)>, shutdown: &mut watch::Receiver<bool>) -> Result<()> {
        let rule = |member: &'static str| -> Result<MatchRule<'static>> {
            Ok(MatchRule::builder().msg_type(MessageType::Signal).interface(WATCHER_NAME)?.member(member)?.path(WATCHER_PATH)?.build())
        };
        let mut registered = MessageStream::for_match_rule(rule("StatusNotifierItemRegistered")?, &self.conn, Some(64)).await?;
        let mut unregistered = MessageStream::for_match_rule(rule("StatusNotifierItemUnregistered")?, &self.conn, Some(64)).await?;
        let watcher_owner_rule = MatchRule::builder()
            .msg_type(MessageType::Signal).sender("org.freedesktop.DBus")?.interface("org.freedesktop.DBus")?
            .member("NameOwnerChanged")?.arg(0, WATCHER_NAME)?.build();
        let mut watcher_owner = MessageStream::for_match_rule(watcher_owner_rule, &self.conn, Some(16)).await?;
        // Disappearances (arg2 == "") are only needed while we serve the watcher; a foreign
        // watcher emits StatusNotifierItemUnregistered itself.
        let mut vanished: Option<MessageStream> = None;
        let item_rule = MatchRule::builder().msg_type(MessageType::Signal).interface(ITEM_IFACE)?.build();
        let mut item_signals = MessageStream::for_match_rule(item_rule, &self.conn, Some(256)).await?;
        let mut menu_signals: Option<MessageStream> = None;

        if !self.read_only {
            let _ = self.conn.request_name_with_flags(
                WellKnownName::try_from(self.host_name.as_str())?,
                zbus::fdo::RequestNameFlags::DoNotQueue.into(),
            ).await;
        }
        self.attach_watcher().await;
        self.publish();

        loop {
            let wants_menu_signals = !self.menus.is_empty();
            if wants_menu_signals && menu_signals.is_none() {
                let rule = MatchRule::builder().msg_type(MessageType::Signal).interface(MENU_IFACE)?.build();
                menu_signals = MessageStream::for_match_rule(rule, &self.conn, Some(64)).await.ok();
            } else if !wants_menu_signals {
                menu_signals = None;
            }
            if self.owns_watcher() && vanished.is_none() {
                let rule = MatchRule::builder()
                    .msg_type(MessageType::Signal).sender("org.freedesktop.DBus")?.interface("org.freedesktop.DBus")?
                    .member("NameOwnerChanged")?.arg(2, "")?.build();
                vanished = MessageStream::for_match_rule(rule, &self.conn, Some(256)).await.ok();
            } else if !self.owns_watcher() {
                vanished = None;
            }
            let vanished_next = async {
                match vanished.as_mut() { Some(s) => s.next().await, None => std::future::pending().await }
            };
            let menu_next = async {
                match menu_signals.as_mut() { Some(s) => s.next().await, None => std::future::pending().await }
            };
            tokio::select! {
                m = registered.next() => match m {
                    Some(Ok(m)) => if let Ok((key,)) = m.body().deserialize::<(String,)>() { self.add_item(key) },
                    _ => return err("session bus closed"),
                },
                m = unregistered.next() => match m {
                    Some(Ok(m)) => if let Ok((key,)) = m.body().deserialize::<(String,)>() { self.remove_item(&key) },
                    _ => return err("session bus closed"),
                },
                m = watcher_owner.next() => match m {
                    Some(Ok(m)) => if let Ok((_, _, new)) = m.body().deserialize::<(String, String, String)>() {
                        if new.is_empty() { self.watcher_available = false; }
                        self.attach_watcher().await;
                    },
                    _ => return err("session bus closed"),
                },
                Some(Ok(m)) = vanished_next => if let Ok((name, _, _)) = m.body().deserialize::<(String, String, String)>() {
                    self.on_vanished(&name).await;
                },
                m = item_signals.next() => match m {
                    Some(Ok(m)) => {
                        
                        let mut burst = vec![m];
                        // Drain only already-delivered signals. No debounce timer or
                        // extra latency; a later signal still marks an in-flight read dirty.
                        for _ in 1..64 {
                            match item_signals.next().now_or_never() {
                                Some(Some(Ok(m))) => burst.push(m),
                                Some(_) => return err("session bus closed"),
                                None => break,
                            }
                        }
                        self.on_item_signals(&burst);
                        continue;
                    },
                    _ => return err("session bus closed"),
                },
                Some(Ok(m)) = menu_next => self.on_menu_signal(&m),
                Some(i) = self.rx.recv() => self.on_internal(i),
                r = queue.recv() => match r {
                    Some((action, reply)) => self.on_action(action, reply),
                    None => return Ok(()),
                },
                _ = shutdown.changed() => if *shutdown.borrow() { return Ok(()) },
            }
            self.publish();
        }
    }

    /// Find or become the watcher, register the host, then reconcile the item list.
    async fn attach_watcher(&mut self) {
        let Ok(dbus) = fdo::DBusProxy::new(&self.conn).await else { return };
        let name = BusName::from(WellKnownName::from_static_str_unchecked(WATCHER_NAME));
        let mut owner = dbus.get_name_owner(name.clone()).await.ok();
        if owner.is_none() && !self.read_only {
            let registry = self.registry.get_or_insert_with(|| Arc::new(Mutex::new(Registry::default()))).clone();
            let server = self.conn.object_server();
            let _ = server.at(WATCHER_PATH, Watcher { registry }).await;
            if let Ok(fdo::RequestNameReply::PrimaryOwner | fdo::RequestNameReply::AlreadyOwner) = self.conn
                .request_name_with_flags(WellKnownName::from_static_str_unchecked(WATCHER_NAME), fdo::RequestNameFlags::DoNotQueue.into())
                .await
            {
                owner = dbus.get_name_owner(name).await.ok();
            }
        }
        let Some(_owner) = owner else {
            self.watcher_available = false;
            return;
        };
        self.watcher_available = true;
        let proxy = zbus::Proxy::new(&self.conn, WATCHER_NAME, WATCHER_PATH, WATCHER_NAME).await;
        let Ok(proxy) = proxy else { return };
        if !self.read_only {
            let _ = tokio::time::timeout(CALL_TIMEOUT, proxy.call_method("RegisterStatusNotifierHost", &(self.host_name.as_str(),))).await;
        }
        let list: Option<Vec<String>> = match tokio::time::timeout(CALL_TIMEOUT, async {
            let props = fdo::PropertiesProxy::builder(&self.conn).destination(WATCHER_NAME)?.path(WATCHER_PATH)?
                .cache_properties(zbus::proxy::CacheProperties::No).build().await?;
            let v = props.get(zbus::names::InterfaceName::from_static_str_unchecked(WATCHER_NAME), "RegisteredStatusNotifierItems").await?;
            Ok::<_, Error>(Vec::<String>::try_from(v)?)
        }).await {
            Ok(Ok(list)) => Some(list),
            _ => None,
        };
        if let Some(list) = list {
            let stale: Vec<String> = self.items.keys().filter(|k| !list.contains(k)).cloned().collect();
            for k in stale { self.remove_item(&k) }
            for k in list { self.add_item(k) }
        }
    }

    fn owns_watcher(&self) -> bool { self.registry.is_some() && self.watcher_available }

    async fn on_vanished(&mut self, name: &str) {
        if let Some(registry) = &self.registry {
            if self.owns_watcher() { watcher_name_vanished(&self.conn, registry, name).await }
        }
        let gone: Vec<String> = self.items.iter()
            .filter(|(_, s)| s.owner == name || s.item.service == name)
            .map(|(k, _)| k.clone()).collect();
        for k in gone { self.remove_item(&k) }
    }

    fn add_item(&mut self, key: String) {
        if self.items.contains_key(&key) || self.items.len() >= MAX_ITEMS { return }
        let (service, path) = model::split_key(&key);
        let item = TrayItem { key: key.clone(), service, path, order: self.next_order, ..Default::default() };
        self.next_order += 1;
        self.items.insert(key.clone(), ItemSlot { item, owner: String::new(), fetching: false, dirty: false });
        self.fetch(&key);
    }

    fn remove_item(&mut self, key: &str) {
        self.items.remove(key);
        self.menus.remove(key);
        self.menu_meta.remove(key);
    }

    fn fetch(&mut self, key: &str) {
        let Some(slot) = self.items.get_mut(key) else { return };
        if slot.fetching { slot.dirty = true; return }
        slot.fetching = true;
        let (conn, tx, key, service, path) = (self.conn.clone(), self.tx.clone(), key.to_owned(), slot.item.service.clone(), slot.item.path.clone());
        tokio::spawn(async move {
            let fut = async {
                let owner = if service.starts_with(':') { service.clone() } else {
                    fdo::DBusProxy::new(&conn).await?.get_name_owner(BusName::try_from(service.as_str())?).await?.to_string()
                };
                let message = conn.call_method(Some(service.as_str()), path.as_str(),
                    Some("org.freedesktop.DBus.Properties"), "GetAll", &(ITEM_IFACE,)).await?;
                Ok::<_, Error>((owner, wire::Properties::decode(&message)?))
            };
            let (owner, props) = match tokio::time::timeout(CALL_TIMEOUT, fut).await {
                Ok(Ok((owner, props))) => (owner, Ok(props)),
                Ok(Err(e)) => (String::new(), Err(e)),
                Err(_) => (String::new(), err("item did not answer")),
            };
            let _ = tx.send(Internal::Fetched { key, owner, props });
        });
    }

    fn on_item_signals(&mut self, messages: &[zbus::Message]) {
        for key in item_signal_keys(&self.items, messages) { self.fetch(&key) }
    }

    fn on_menu_signal(&mut self, message: &zbus::Message) {
        let header = message.header();
        let (Some(sender), Some(path)) = (header.sender().map(|s| s.to_string()), header.path().map(|p| p.to_string())) else { return };
        let keys: Vec<String> = self.menus.keys().filter(|k| {
            self.items.get(*k).is_some_and(|s| (s.owner == sender || s.item.service == sender) && s.item.menu.as_deref() == Some(path.as_str()))
        }).cloned().collect();
        for k in keys { self.fetch_layout(&k, None) }
    }

    fn on_internal(&mut self, event: Internal) {
        match event {
            Internal::Fetched { key, owner, props } => {
                let Some(slot) = self.items.get_mut(&key) else { return };
                slot.fetching = false;
                match props {
                    Ok(map) => {
                        slot.owner = owner;
                        map.apply(&mut slot.item);
                    }
                    // An item that never answers is dropped like a vanished registration.
                    Err(_) if slot.item.id.is_empty() => { self.items.remove(&key); return; }
                    Err(_) => {}
                }
                if std::mem::take(&mut slot.dirty) { self.fetch(&key) }
            }
            Internal::Layout { key, generation, serial, revision, root } => {
                let Some(meta) = self.menu_meta.get_mut(&key).filter(|m| m.generation == generation) else { return };
                if serial != meta.serial {
                    return;
                }
                meta.in_flight = false;
                let refetch = std::mem::take(&mut meta.dirty);
                if let (Some(menu), Some(root)) = (self.menus.get_mut(&key), root) {
                    // dbusmenu revisions are u32 and may wrap: never step backwards.
                    let newer = menu.revision == 0 || (revision.wrapping_sub(menu.revision) as i32) >= 0;
                    if newer {
                        menu.revision = revision;
                        menu.root = root;
                    }
                }
                if refetch {
                    self.fetch_layout(&key, None);
                }
            }
        }
    }

    fn menu_target(&self, key: &str) -> Result<(String, String)> {
        let slot = self.items.get(key).ok_or_else(|| Error("unknown tray item".into()))?;
        let menu = slot.item.menu.clone().ok_or_else(|| Error("tray item has no menu".into()))?;
        Ok((slot.item.service.clone(), menu))
    }

    /// Coalesced GetLayout: at most one request per open menu; signals during a request
    /// mark it dirty and trigger exactly one follow-up. `about_to_show` is awaited first.
    fn fetch_layout(&mut self, key: &str, about_to_show: Option<i32>) {
        let Ok((service, menu)) = self.menu_target(key) else { return };
        let Some(meta) = self.menu_meta.get_mut(key) else { return };
        if meta.in_flight {
            meta.dirty = true;
            return;
        }
        meta.in_flight = true;
        meta.serial += 1;
        let (generation, serial) = (meta.generation, meta.serial);
        let (conn, tx, key) = (self.conn.clone(), self.tx.clone(), key.to_owned());
        tokio::spawn(async move {
            if let Some(id) = about_to_show {
                let body = (id,);
                let call = conn.call_method(Some(service.as_str()), menu.as_str(), Some(MENU_IFACE), "AboutToShow", &body);
                let _ = tokio::time::timeout(CALL_TIMEOUT, call).await;
            }
            let body = (0i32, -1i32, Vec::<String>::new());
            let fut = conn.call_method(Some(service.as_str()), menu.as_str(), Some(MENU_IFACE), "GetLayout", &body);
            type Layout = (u32, (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>));
            let parsed = match tokio::time::timeout(CALL_TIMEOUT, fut).await {
                Ok(Ok(reply)) => reply.body().deserialize::<Layout>().ok(),
                _ => None,
            };
            let (revision, root) = match parsed {
                Some((revision, (id, props, children))) => (revision, model::parse_layout_parts(id, &props, &children)),
                None => (0, None),
            };
            let _ = tx.send(Internal::Layout { key, generation, serial, revision, root });
        });
    }

    fn on_action(&mut self, action: TrayAction, reply: Reply<()>) {
        if self.read_only {
            let _ = reply.send(err("read-only context"));
            return;
        }
        let conn = self.conn.clone();
        let call = |d: String, p: String, iface: &'static str, method: &'static str, body: CallBody, reply: Option<Reply<()>>| {
            spawn_call(conn.clone(), d, p, iface, method, body, reply)
        };
        let item_target = |items: &HashMap<String, ItemSlot>, key: &str| -> Result<(String, String)> {
            let slot = items.get(key).ok_or_else(|| Error("unknown tray item".into()))?;
            Ok((slot.item.service.clone(), slot.item.path.clone()))
        };
        match action {
            TrayAction::Activate { item, x, y } => match item_target(&self.items, &item) {
                Ok((d, p)) => call(d, p, ITEM_IFACE, "Activate", CallBody::Point(x, y), Some(reply)),
                Err(e) => { let _ = reply.send(Err(e)); }
            },
            TrayAction::SecondaryActivate { item, x, y } => match item_target(&self.items, &item) {
                Ok((d, p)) => call(d, p, ITEM_IFACE, "SecondaryActivate", CallBody::Point(x, y), Some(reply)),
                Err(e) => { let _ = reply.send(Err(e)); }
            },
            TrayAction::ContextMenu { item, x, y } => match item_target(&self.items, &item) {
                Ok((d, p)) => call(d, p, ITEM_IFACE, "ContextMenu", CallBody::Point(x, y), Some(reply)),
                Err(e) => { let _ = reply.send(Err(e)); }
            },
            TrayAction::Scroll { item, delta, horizontal } => match item_target(&self.items, &item) {
                Ok((d, p)) => call(d, p, ITEM_IFACE, "Scroll", CallBody::Scroll(delta, if horizontal { "horizontal" } else { "vertical" }.into()), Some(reply)),
                Err(e) => { let _ = reply.send(Err(e)); }
            },
            TrayAction::MenuOpen { item } => match self.menu_target(&item) {
                Ok((d, m)) => {
                    self.menus.insert(item.clone(), TrayMenu { item_key: item.clone(), revision: 0, root: TrayMenuEntry::default() });
                    self.next_menu_generation += 1;
                    self.menu_meta.insert(item.clone(), MenuMeta { generation: self.next_menu_generation, ..Default::default() });
                    self.fetch_layout(&item, Some(0));
                    call(d, m, MENU_IFACE, "Event", CallBody::Event(0, "opened".into()), Some(reply));
                }
                Err(e) => { let _ = reply.send(Err(e)); }
            },
            TrayAction::MenuClose { item } => {
                let target = self.menu_target(&item);
                self.menus.remove(&item);
                self.menu_meta.remove(&item);
                match target {
                    Ok((d, m)) => call(d, m, MENU_IFACE, "Event", CallBody::Event(0, "closed".into()), Some(reply)),
                    Err(_) => { let _ = reply.send(Ok(())); }
                }
            }
            TrayAction::MenuAboutToShow { item, entry } => match self.menu_target(&item) {
                Ok((d, m)) => {
                    if self.menus.contains_key(&item) {
                        // AboutToShow(entry) then the dependent layout, in order.
                        self.fetch_layout(&item, Some(entry));
                        let _ = reply.send(Ok(()));
                    } else {
                        call(d, m, MENU_IFACE, "AboutToShow", CallBody::Id(entry), Some(reply));
                    }
                }
                Err(e) => { let _ = reply.send(Err(e)); }
            },
            TrayAction::MenuEvent { item, entry, event } => {
                if !matches!(event.as_str(), "clicked" | "hovered" | "opened" | "closed") {
                    let _ = reply.send(err("unsupported menu event"));
                    return;
                }
                match self.menu_target(&item) {
                    Ok((d, m)) => call(d, m, MENU_IFACE, "Event", CallBody::Event(entry, event), Some(reply)),
                    Err(e) => { let _ = reply.send(Err(e)); }
                }
            }
        }
    }

    fn publish(&self) {
        let mut items: Vec<TrayItem> = self.items.values().filter(|s| !s.item.id.is_empty() || !s.owner.is_empty()).map(|s| s.item.clone()).collect();
        items.sort_by_key(|i| i.order);
        let mut menus: Vec<TrayMenu> = self.menus.values().cloned().collect();
        menus.sort_by(|a, b| a.item_key.cmp(&b.item_key));
        self.topic.publish(TrayState { items, menus, watcher_available: self.watcher_available, watcher_owned: self.owns_watcher() });
    }
}

fn spawn_call(conn: zbus::Connection, destination: String, path: String, iface: &'static str, method: &'static str, body: CallBody, reply: Option<Reply<()>>) {
    tokio::spawn(async move {
        let fut = async {
            match body {
                CallBody::Point(x, y) => conn.call_method(Some(destination.as_str()), path.as_str(), Some(iface), method, &(x, y)).await,
                CallBody::Scroll(d, o) => conn.call_method(Some(destination.as_str()), path.as_str(), Some(iface), method, &(d, o)).await,
                CallBody::Id(id) => conn.call_method(Some(destination.as_str()), path.as_str(), Some(iface), method, &(id,)).await,
                CallBody::Event(id, event) => conn.call_method(Some(destination.as_str()), path.as_str(), Some(iface), method, &(id, event.as_str(), Value::I32(0), 0u32)).await,
            }
        };
        let result = match tokio::time::timeout(CALL_TIMEOUT, fut).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(Error::from(e)),
            Err(_) => err("tray item did not answer"),
        };
        if let Some(reply) = reply { let _ = reply.send(result); }
    });
}

#[derive(Default)]
struct MenuMeta {
    generation: u64,
    serial: u64,
    in_flight: bool,
    dirty: bool,
}

enum CallBody {
    Point(i32, i32),
    Scroll(i32, String),
    Id(i32),
    Event(i32, String),
}

fn item_signal_keys(items: &HashMap<String, ItemSlot>, messages: &[zbus::Message]) -> std::collections::HashSet<String> {
    let mut keys = std::collections::HashSet::new();
    for message in messages {
        let header = message.header();
        let (Some(sender), Some(path)) = (header.sender(), header.path()) else { continue };
        keys.extend(items.iter().filter(|(_, slot)|
            (slot.owner == sender.as_str() || slot.item.service == sender.as_str()) && slot.item.path == path.as_str()
        ).map(|(key, _)| key.clone()));
    }
    keys
}
#[cfg(test)]
mod idle_burst_tests {
    use super::*;
    #[test]
    fn paired_signals_schedule_one_read_per_item() {
        let mut items = HashMap::new();
        let key = ":1.99/StatusNotifierItem".to_string();
        items.insert(key.clone(), ItemSlot { item: TrayItem { service: ":1.99".into(), path: "/StatusNotifierItem".into(), ..Default::default() }, owner: ":1.99".into(), fetching: false, dirty: false });
        let signal = |member| zbus::Message::signal("/StatusNotifierItem", ITEM_IFACE, member).unwrap().sender(":1.99").unwrap().build(&()).unwrap();
        let keys = item_signal_keys(&items, &[signal("NewIcon"),signal("NewToolTip")]);
        assert_eq!(keys, [key.clone()].into_iter().collect());
        assert_eq!(item_signal_keys(&items, &[signal("NewIcon")]), [key].into_iter().collect(), "a later burst must remain actionable");
    }
}
