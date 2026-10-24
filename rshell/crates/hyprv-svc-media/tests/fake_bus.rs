//! Actor tests against fake peers on a private dbus-daemon.
mod common;
use common::{wait_for, Harness};
use hyprv_svc_media::{mpris, notifications, tray};
use std::{collections::HashMap, sync::{Arc, Mutex}};
use zbus::{object_server::SignalEmitter, zvariant::{ObjectPath, OwnedValue, Value}};

// ------------------------------------------------------------------ fake MPRIS player

struct Root;
#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    #[zbus(property)]
    fn identity(&self) -> String { "Fake Player".into() }
    #[zbus(property)]
    fn can_raise(&self) -> bool { false }
}

struct Player {
    status: String,
    title: String,
    calls: Arc<Mutex<Vec<String>>>,
}
#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    fn play_pause(&mut self) { self.calls.lock().unwrap().push("PlayPause".into()) }
    fn next(&mut self) { self.calls.lock().unwrap().push("Next".into()) }
    fn set_position(&mut self, track: ObjectPath<'_>, position: i64) {
        self.calls.lock().unwrap().push(format!("SetPosition {track} {position}"))
    }
    #[zbus(property)]
    fn playback_status(&self) -> String { self.status.clone() }
    #[zbus(property)]
    fn rate(&self) -> f64 { 1.0 }
    #[zbus(property)]
    fn position(&self) -> i64 { 30_000_000 }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let mut m = HashMap::new();
        m.insert("xesam:title".into(), OwnedValue::try_from(Value::from(self.title.as_str())).unwrap());
        m.insert("xesam:artist".into(), OwnedValue::try_from(Value::from(vec!["Band"])).unwrap());
        m.insert("mpris:length".into(), OwnedValue::from(180_000_000i64));
        m.insert("mpris:trackid".into(), OwnedValue::try_from(Value::from(ObjectPath::try_from("/track/1").unwrap())).unwrap());
        m
    }
    #[zbus(property)]
    fn can_control(&self) -> bool { true }
    #[zbus(property)]
    fn can_go_next(&self) -> bool { true }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool { false }
    #[zbus(property)]
    fn can_seek(&self) -> bool { true }
    #[zbus(signal)]
    async fn seeked(emitter: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;
}

#[test]
fn mpris_discovery_signals_actions_and_owner_loss() {
    let h = Harness::new(false);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let peer = h.peer();
    // Player exists before the actor starts: exercises snapshot.
    h.peers.block_on(async {
        peer.object_server().at("/org/mpris/MediaPlayer2", Root).await.unwrap();
        peer.object_server().at("/org/mpris/MediaPlayer2", Player { status: "Paused".into(), title: "First".into(), calls: calls.clone() }).await.unwrap();
        peer.request_name("org.mpris.MediaPlayer2.fake").await.unwrap();
    });
    let handle = mpris::spawn(&h.ctx, None);
    let s = wait_for(&handle.topic, "player", |s| s.available);
    assert_eq!((s.title.as_str(), s.artist.as_str(), s.player_name.as_str(), s.length_seconds, s.playing), ("First", "Band", "fake", 180.0, false));
    assert_eq!(s.position_seconds(hyprv_svc_media::monotonic_us()), 30.0);
    assert_eq!(s.players[0].identity, "Fake Player");

    // PropertiesChanged: playing + new title.
    h.peers.block_on(async {
        let iface = peer.object_server().interface::<_, Player>("/org/mpris/MediaPlayer2").await.unwrap();
        {
            let mut p = iface.get_mut().await;
            p.status = "Playing".into();
            p.title = "Second".into();
        }
        let p = iface.get().await;
        p.playback_status_changed(iface.signal_emitter()).await.unwrap();
        p.metadata_changed(iface.signal_emitter()).await.unwrap();
    });
    let s = wait_for(&handle.topic, "playing", |s| s.playing && s.title == "Second");
    assert!(s.position.playing);

    // Seeked re-anchors the local clock.
    h.peers.block_on(async {
        let iface = peer.object_server().interface::<_, Player>("/org/mpris/MediaPlayer2").await.unwrap();
        Player::seeked(iface.signal_emitter(), 90_000_000).await.unwrap();
    });
    let s = wait_for(&handle.topic, "seeked", |s| s.position.position_us == 90_000_000);
    let now = hyprv_svc_media::monotonic_us();
    assert!(s.position_seconds(now) >= 90.0 && s.position_seconds(now) < 91.0);

    // Actions go to the selected player; capability flags are enforced.
    let rt = &h.runtime;
    assert_eq!(rt.block_on(handle.action(mpris::MprisAction::PlayPause).result()), Ok(mpris::MprisOutcome::Done));
    assert!(rt.block_on(handle.action(mpris::MprisAction::Previous).result()).is_err());
    assert_eq!(rt.block_on(handle.action(mpris::MprisAction::Seek(500.0)).result()), Ok(mpris::MprisOutcome::Done));
    let outcome = rt.block_on(handle.action(mpris::MprisAction::Raise).result()).unwrap();
    assert!(matches!(outcome, mpris::MprisOutcome::FocusFallback { pid: Some(_), class: None }));
    assert_eq!(*calls.lock().unwrap(), vec!["PlayPause".to_string(), "SetPosition /track/1 180000000".to_string()]);

    // Owner loss removes the player.
    h.peers.block_on(async { peer.release_name("org.mpris.MediaPlayer2.fake").await.unwrap(); });
    let s = wait_for(&handle.topic, "player gone", |s| !s.available);
    assert!(s.players.is_empty() && s.title.is_empty());

    // Owner restart (subscribe-before-snapshot path for late players).
    h.peers.block_on(async { peer.request_name("org.mpris.MediaPlayer2.fake").await.unwrap(); });
    wait_for(&handle.topic, "player back", |s| s.available && s.title == "Second");
    h.runtime.request_shutdown();
}

#[test]
fn mpris_read_only_refuses_actions() {
    let h = Harness::new(true);
    let handle = mpris::spawn(&h.ctx, None);
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(h.runtime.block_on(handle.action(mpris::MprisAction::PlayPause).result()).is_err());
    h.runtime.request_shutdown();
}

// ------------------------------------------------------------------ fake swaync

struct Swaync { dnd: bool, count: u32 }
#[zbus::interface(name = "org.erikreider.swaync.cc")]
impl Swaync {
    fn get_subscribe_data(&self) -> (bool, bool, u32, bool) { (self.dnd, false, self.count, false) }
    async fn toggle_dnd(&mut self, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) -> bool {
        self.dnd = !self.dnd;
        let _ = Self::subscribe_v2(&emitter, self.count, self.dnd, false, false).await;
        self.dnd
    }
    #[zbus(signal)]
    async fn subscribe_v2(emitter: &SignalEmitter<'_>, count: u32, dnd: bool, cc_open: bool, inhibited: bool) -> zbus::Result<()>;
}

#[test]
fn swaync_snapshot_signal_action_and_loss() {
    let h = Harness::new(false);
    let handle = notifications::spawn(&h.ctx);
    // Daemon appears after the actor: NameOwnerChanged triggers the snapshot.
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(!handle.topic.snapshot().value.available);
    let peer = h.peer();
    h.peers.block_on(async {
        peer.object_server().at("/org/erikreider/swaync/cc", Swaync { dnd: false, count: 2 }).await.unwrap();
        peer.request_name("org.erikreider.swaync.cc").await.unwrap();
    });
    let s = wait_for(&handle.topic, "snapshot", |s| s.available);
    assert_eq!((s.alt.as_str(), s.tooltip.as_str()), ("notification", "2 Notifications"));
    let r = h.runtime.block_on(handle.action(notifications::NotificationAction::ToggleDnd).result());
    assert_eq!(r, Ok(None));
    let s = wait_for(&handle.topic, "dnd", |s| s.dnd);
    assert_eq!(s.alt, "dnd-notification");
    h.peers.block_on(async { peer.release_name("org.erikreider.swaync.cc").await.unwrap(); });
    let s = wait_for(&handle.topic, "loss", |s| !s.available);
    assert_eq!(s.alt, "dnd-notification", "last state kept like the QML stream");
    h.runtime.request_shutdown();
}

// ------------------------------------------------------------------ fake tray item

struct Item { title: String }
#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl Item {
    #[zbus(property)]
    fn id(&self) -> String { "fake-item".into() }
    #[zbus(property)]
    fn title(&self) -> String { self.title.clone() }
    #[zbus(property)]
    fn status(&self) -> String { "Active".into() }
    #[zbus(property)]
    fn icon_name(&self) -> String { "fake-icon".into() }
    #[zbus(property)]
    fn menu(&self) -> ObjectPath<'static> { ObjectPath::try_from("/Menu").unwrap() }
    #[zbus(property)]
    fn item_is_menu(&self) -> bool { false }
    fn activate(&self, _x: i32, _y: i32) {}
    #[zbus(signal)]
    async fn new_title(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

type Layout = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

struct Menu { events: Arc<Mutex<Vec<(i32, String)>>> }
#[zbus::interface(name = "com.canonical.dbusmenu")]
impl Menu {
    fn get_layout(&self, _parent: i32, _depth: i32, _props: Vec<String>) -> (u32, Layout) {
        let mut child_props: HashMap<String, Value> = HashMap::new();
        child_props.insert("label".into(), Value::from("_Quit"));
        let child = zbus::zvariant::StructureBuilder::new().add_field(5i32).add_field(child_props).add_field(Vec::<Value>::new()).build().unwrap();
        let child = OwnedValue::try_from(Value::Structure(child)).unwrap();
        (3, (0, HashMap::new(), vec![child]))
    }
    fn about_to_show(&self, _id: i32) -> bool { false }
    fn event(&self, id: i32, event: String, _data: Value<'_>, _ts: u32) {
        self.events.lock().unwrap().push((id, event));
    }
}

#[test]
fn tray_serves_watcher_tracks_items_and_menus() {
    let h = Harness::new(false);
    let handle = tray::spawn(&h.ctx);
    wait_for(&handle.topic, "watcher", |s| s.watcher_available && s.watcher_owned);
    let peer = h.peer();
    let events = Arc::new(Mutex::new(Vec::new()));
    h.peers.block_on(async {
        peer.object_server().at("/StatusNotifierItem", Item { title: "One".into() }).await.unwrap();
        peer.object_server().at("/Menu", Menu { events: events.clone() }).await.unwrap();
        peer.request_name("org.kde.StatusNotifierItem-1-1").await.unwrap();
        peer.call_method(Some("org.kde.StatusNotifierWatcher"), "/StatusNotifierWatcher", Some("org.kde.StatusNotifierWatcher"), "RegisterStatusNotifierItem", &("org.kde.StatusNotifierItem-1-1",)).await.unwrap();
    });
    let s = wait_for(&handle.topic, "item", |s| s.items.len() == 1 && s.items[0].title == "One");
    let item = &s.items[0];
    assert_eq!((item.id.as_str(), item.icon.as_str(), item.has_menu, item.key.as_str()), ("fake-item", "fake-icon", true, "org.kde.StatusNotifierItem-1-1/StatusNotifierItem"));

    // New* signal → coalesced refetch.
    h.peers.block_on(async {
        let iface = peer.object_server().interface::<_, Item>("/StatusNotifierItem").await.unwrap();
        iface.get_mut().await.title = "Two".into();
        Item::new_title(iface.signal_emitter()).await.unwrap();
    });
    wait_for(&handle.topic, "retitle", |s| s.items.first().is_some_and(|i| i.title == "Two"));

    // Menu lifecycle.
    let key = item.key.clone();
    assert_eq!(h.runtime.block_on(handle.action(tray::TrayAction::MenuOpen { item: key.clone() }).result()), Ok(()));
    let s = wait_for(&handle.topic, "menu", |s| s.menus.first().is_some_and(|m| m.revision == 3));
    assert_eq!(s.menus[0].root.children[0].text, "Quit");
    assert_eq!(h.runtime.block_on(handle.action(tray::TrayAction::MenuEvent { item: key.clone(), entry: 5, event: "clicked".into() }).result()), Ok(()));
    assert_eq!(h.runtime.block_on(handle.action(tray::TrayAction::MenuClose { item: key.clone() }).result()), Ok(()));
    wait_for(&handle.topic, "menu closed", |s| s.menus.is_empty());
    assert_eq!(*events.lock().unwrap(), vec![(0, "opened".to_string()), (5, "clicked".to_string()), (0, "closed".to_string())]);
    assert_eq!(h.runtime.block_on(handle.action(tray::TrayAction::Activate { item: key, x: 1, y: 2 }).result()), Ok(()));

    // Owner vanishes → unregistered from our watcher and dropped.
    h.peers.block_on(async { peer.release_name("org.kde.StatusNotifierItem-1-1").await.unwrap(); });
    wait_for(&handle.topic, "item gone", |s| s.items.is_empty());
    h.runtime.request_shutdown();
}

#[test]
fn tray_read_only_never_owns_watcher() {
    let h = Harness::new(true);
    let handle = tray::spawn(&h.ctx);
    std::thread::sleep(std::time::Duration::from_millis(200));
    let s = handle.topic.snapshot().value;
    assert!(!s.watcher_owned && !s.watcher_available);
    h.runtime.request_shutdown();
}
