//! Regression tests for docs/reviews/hyprv-svc-media.md MEDIA-2…MEDIA-5, on a private
//! dbus-daemon with fake peers (MEDIA-1 is covered in `spectrum::tests`).
mod common;
use common::{wait_for, Harness};
use hyprv_svc_media::{mpris, notifications, tray};
use std::{
    collections::HashMap,
    sync::{atomic::{AtomicU32, Ordering}, Arc, Mutex},
    time::Duration,
};
use tokio::sync::Notify;
use zbus::{object_server::SignalEmitter, zvariant::{ObjectPath, OwnedValue, Value}};

// ------------------------------------------------------------------ MEDIA-2

struct SlowPlayer {
    status: String,
}
#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl SlowPlayer {
    #[zbus(property)]
    fn playback_status(&self) -> String { self.status.clone() }
    #[zbus(property)]
    fn rate(&self) -> f64 { 1.0 }
    /// Stale answer, deliberately slow: 1 s.
    #[zbus(property)]
    async fn position(&self) -> i64 {
        tokio::time::sleep(Duration::from_millis(150)).await;
        1_000_000
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let mut m = HashMap::new();
        m.insert("xesam:title".into(), OwnedValue::try_from(Value::from("T")).unwrap());
        m.insert("mpris:length".into(), OwnedValue::from(300_000_000i64));
        m
    }
    #[zbus(property)]
    fn can_control(&self) -> bool { true }
    #[zbus(signal)]
    async fn seeked(emitter: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;
}

#[test]
fn media2_late_position_read_cannot_undo_seeked() {
    let h = Harness::new(true);
    let peer = h.peer();
    h.peers.block_on(async {
        peer.object_server().at("/org/mpris/MediaPlayer2", SlowPlayer { status: "Paused".into() }).await.unwrap();
        peer.request_name("org.mpris.MediaPlayer2.slow").await.unwrap();
    });
    let handle = mpris::spawn(&h.ctx, None);
    wait_for(&handle.topic, "player", |s| s.available);
    // Playback change → actor issues a (slow) Position read; a Seeked lands before it returns.
    h.peers.block_on(async {
        let iface = peer.object_server().interface::<_, SlowPlayer>("/org/mpris/MediaPlayer2").await.unwrap();
        iface.get_mut().await.status = "Playing".into();
        iface.get().await.playback_status_changed(iface.signal_emitter()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        SlowPlayer::seeked(iface.signal_emitter(), 90_000_000).await.unwrap();
    });
    wait_for(&handle.topic, "seeked", |s| s.position.position_us == 90_000_000);
    std::thread::sleep(Duration::from_millis(400)); // the stale read has returned by now
    let s = handle.topic.snapshot().value;
    assert_eq!(s.position.position_us, 90_000_000, "late read overwrote the Seeked anchor");
    h.runtime.request_shutdown();
}

// ------------------------------------------------------------------ MEDIA-3

struct Item;
#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl Item {
    #[zbus(property)]
    fn id(&self) -> String { "menu-item".into() }
    #[zbus(property)]
    fn menu(&self) -> ObjectPath<'static> { ObjectPath::try_from("/Menu").unwrap() }
}

/// GetLayout #n answers revision n; odd requests are slow, even ones fast — the old
/// actor (parallel requests) applied a slow older revision after a newer one.
struct RacyMenu {
    calls: Arc<AtomicU32>,
}
type Layout = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);
#[zbus::interface(name = "com.canonical.dbusmenu")]
impl RacyMenu {
    async fn get_layout(&self, _parent: i32, _depth: i32, _props: Vec<String>) -> (u32, Layout) {
        let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        let delay = if n % 2 == 1 { 120 } else { 5 };
        tokio::time::sleep(Duration::from_millis(delay)).await;
        (n, (0, HashMap::new(), Vec::new()))
    }
    fn about_to_show(&self, _id: i32) -> bool { false }
    fn event(&self, _id: i32, _event: String, _data: Value<'_>, _ts: u32) {}
    #[zbus(signal)]
    async fn layout_updated(emitter: &SignalEmitter<'_>, revision: u32, parent: i32) -> zbus::Result<()>;
}

#[test]
fn media3_layout_revision_never_regresses_and_settles_on_latest() {
    let h = Harness::new(false);
    let handle = tray::spawn(&h.ctx);
    wait_for(&handle.topic, "watcher", |s| s.watcher_owned);
    let peer = h.peer();
    let calls = Arc::new(AtomicU32::new(0));
    h.peers.block_on(async {
        peer.object_server().at("/StatusNotifierItem", Item).await.unwrap();
        peer.object_server().at("/Menu", RacyMenu { calls: calls.clone() }).await.unwrap();
        peer.request_name("org.kde.StatusNotifierItem-3-1").await.unwrap();
        peer.call_method(Some("org.kde.StatusNotifierWatcher"), "/StatusNotifierWatcher", Some("org.kde.StatusNotifierWatcher"), "RegisterStatusNotifierItem", &("org.kde.StatusNotifierItem-3-1",)).await.unwrap();
    });
    let s = wait_for(&handle.topic, "item", |s| s.items.first().is_some_and(|i| i.has_menu));
    let key = s.items[0].key.clone();
    let mut sub = handle.topic.subscribe();
    h.runtime.block_on(handle.action(tray::TrayAction::MenuOpen { item: key }).result()).unwrap();
    h.peers.block_on(async {
        let iface = peer.object_server().interface::<_, RacyMenu>("/Menu").await.unwrap();
        for r in 0..5 {
            RacyMenu::layout_updated(iface.signal_emitter(), r, 0).await.unwrap();
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    // Observe every published revision until things settle.
    let revisions = Arc::new(Mutex::new(Vec::new()));
    let seen = revisions.clone();
    h.runtime.block_on(async move {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(1500);
        while let Ok(Ok(snap)) = tokio::time::timeout_at(deadline, sub.changed()).await {
            if let Some(m) = snap.value.menus.first() {
                seen.lock().unwrap().push(m.revision);
            }
        }
    });
    let revisions = revisions.lock().unwrap().clone();
    assert!(revisions.windows(2).all(|w| w[1] >= w[0]), "revision regressed: {revisions:?}");
    let served = calls.load(Ordering::SeqCst);
    assert_eq!(revisions.last().copied(), Some(served), "final layout is the newest served");
    assert!(served <= 3, "signals during a request coalesce into one follow-up (served {served})");
    h.runtime.request_shutdown();
}

// ------------------------------------------------------------------ MEDIA-4 / MEDIA-5

struct Swaync {
    count: u32,
    hold: Option<Arc<Notify>>,
}
#[zbus::interface(name = "org.erikreider.swaync.cc")]
impl Swaync {
    async fn get_subscribe_data(&self) -> (bool, bool, u32, bool) {
        if let Some(hold) = &self.hold {
            hold.notified().await;
        }
        (false, false, self.count, false)
    }
    #[zbus(signal)]
    async fn subscribe_v2(emitter: &SignalEmitter<'_>, count: u32, dnd: bool, cc_open: bool, inhibited: bool) -> zbus::Result<()>;
}

#[test]
fn media4_signal_from_non_owner_is_ignored() {
    let h = Harness::new(true);
    let owner = h.peer();
    h.peers.block_on(async {
        owner.object_server().at("/org/erikreider/swaync/cc", Swaync { count: 2, hold: None }).await.unwrap();
        owner.request_name("org.erikreider.swaync.cc").await.unwrap();
    });
    let handle = notifications::spawn(&h.ctx);
    wait_for(&handle.topic, "snapshot", |s| s.available && s.count == 2);
    // An unrelated peer emits the same signal on the same path/interface.
    let spoofer = h.peer();
    h.peers.block_on(async {
        spoofer.object_server().at("/org/erikreider/swaync/cc", Swaync { count: 0, hold: None }).await.unwrap();
        let iface = spoofer.object_server().interface::<_, Swaync>("/org/erikreider/swaync/cc").await.unwrap();
        Swaync::subscribe_v2(iface.signal_emitter(), 777, true, false, false).await.unwrap();
    });
    std::thread::sleep(Duration::from_millis(200));
    let s = handle.topic.snapshot().value;
    assert_eq!((s.count, s.dnd), (2, false), "spoofed signal accepted: {s:?}");
    // The genuine owner's signal is still honoured.
    h.peers.block_on(async {
        let real = owner.object_server().interface::<_, Swaync>("/org/erikreider/swaync/cc").await.unwrap();
        Swaync::subscribe_v2(real.signal_emitter(), 3, false, false, false).await.unwrap();
    });
    wait_for(&handle.topic, "owner signal", |s| s.count == 3);
    h.runtime.request_shutdown();
}

#[test]
fn media5_snapshot_from_departed_owner_is_discarded() {
    let h = Harness::new(true);
    let handle = notifications::spawn(&h.ctx);
    std::thread::sleep(Duration::from_millis(100));
    let owner = h.peer();
    let hold = Arc::new(Notify::new());
    h.peers.block_on(async {
        owner.object_server().at("/org/erikreider/swaync/cc", Swaync { count: 9, hold: Some(hold.clone()) }).await.unwrap();
        owner.request_name("org.erikreider.swaync.cc").await.unwrap();
    });
    // The snapshot request is now held by the daemon; it loses its name meanwhile.
    std::thread::sleep(Duration::from_millis(150));
    h.peers.block_on(async { owner.release_name("org.erikreider.swaync.cc").await.unwrap(); });
    std::thread::sleep(Duration::from_millis(100));
    assert!(!handle.topic.snapshot().value.available);
    hold.notify_one(); // the old owner's reply finally arrives
    std::thread::sleep(Duration::from_millis(200));
    let s = handle.topic.snapshot().value;
    assert!(!s.available && s.count != 9, "departed owner's snapshot resurrected state: {s:?}");
    h.runtime.request_shutdown();
}
