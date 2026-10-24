//! Laptop QA `lap.media.compact.idle` / `lap.media.expand` at the media provider, against
//! a fake MPRIS player on a private dbus-daemon (docs/verify/laptop-qa-plan.md §4).
//!
//! Compact bar: media plays, nothing presents it (no `mpris::present` lease, no spectrum
//! subscriber). The provider must do no work at all: no actor turn, no `Get Position`,
//! no capture start, and the service thread must not wake. Expanding takes the lease;
//! the very first snapshot read must already carry the current track and position.
mod common;
use common::{wait_for, Harness};
use hyprv_svc_media::{monotonic_us, mpris, spectrum};
use std::{
    collections::HashMap,
    sync::{atomic::{AtomicU64, Ordering}, Arc},
    time::Duration,
};
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

struct Root;
#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    #[zbus(property)]
    fn identity(&self) -> String { "Fake Player".into() }
}

/// Playing player whose position follows its own clock (like a real player), counting
/// every Position read the shell makes.
struct Player {
    started_us: u64,
    position_gets: Arc<AtomicU64>,
}
const START_US: i64 = 30_000_000;
impl Player {
    fn now_us(started_us: u64) -> i64 { START_US + (monotonic_us() - started_us) as i64 }
}
#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    #[zbus(property)]
    fn playback_status(&self) -> String { "Playing".into() }
    #[zbus(property)]
    fn rate(&self) -> f64 { 1.0 }
    #[zbus(property)]
    fn position(&self) -> i64 {
        self.position_gets.fetch_add(1, Ordering::SeqCst);
        Self::now_us(self.started_us)
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let mut m = HashMap::new();
        m.insert("xesam:title".into(), OwnedValue::try_from(Value::from("Compact Song")).unwrap());
        m.insert("xesam:artist".into(), OwnedValue::try_from(Value::from(vec!["Band"])).unwrap());
        m.insert("mpris:length".into(), OwnedValue::from(240_000_000i64));
        m.insert("mpris:trackid".into(), OwnedValue::try_from(Value::from(ObjectPath::try_from("/track/7").unwrap())).unwrap());
        m
    }
    #[zbus(property)]
    fn can_control(&self) -> bool { true }
    #[zbus(property)]
    fn can_seek(&self) -> bool { true }
}

/// Voluntary + involuntary context switches of the service runtime thread(s).
fn service_thread_switches() -> u64 {
    let mut total = 0;
    for entry in std::fs::read_dir("/proc/self/task").unwrap().flatten() {
        let comm = std::fs::read_to_string(entry.path().join("comm")).unwrap_or_default();
        if comm.trim() != "hyprv-services" { continue }
        let status = std::fs::read_to_string(entry.path().join("status")).unwrap_or_default();
        total += status
            .lines()
            .filter(|l| l.starts_with("voluntary_ctxt_switches") || l.starts_with("nonvoluntary_ctxt_switches"))
            .filter_map(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
            .sum::<u64>();
    }
    total
}

#[test]
fn lap_media_compact_idle_then_expand() {
    let h = Harness::new(true);
    let position_gets = Arc::new(AtomicU64::new(0));
    let peer = h.peer();
    let started_us = monotonic_us();
    h.peers.block_on(async {
        peer.object_server().at("/org/mpris/MediaPlayer2", Root).await.unwrap();
        peer.object_server().at("/org/mpris/MediaPlayer2", Player { started_us, position_gets: position_gets.clone() }).await.unwrap();
        peer.request_name("org.mpris.MediaPlayer2.fake").await.unwrap();
    });
    let handle = mpris::spawn(&h.ctx, None);
    let _spectrum = spectrum::spawn(&h.ctx, mpris::playing_gate(&h.ctx, &handle.topic));
    // A compact-bar observer may keep the event subscription; it never polls.
    let _observer = handle.topic.subscribe();
    wait_for(&handle.topic, "playing player", |s| s.available && s.playing && s.title == "Compact Song");
    std::thread::sleep(Duration::from_millis(300)); // settle the initial snapshot

    // ---- lap.media.compact.idle
    let presentation = h.ctx.store.topic::<mpris::MediaPresentationTopic>(());
    assert_eq!(presentation.subscriber_count(), 0, "nothing presents media while compact");
    let (counters, gets, switches) = (h.ctx.counters.snapshot(), position_gets.load(Ordering::SeqCst), service_thread_switches());
    std::thread::sleep(Duration::from_secs(3));
    let after = h.ctx.counters.snapshot();
    assert_eq!(after, counters, "media work while compact: {counters:?} -> {after:?}");
    assert_eq!(after.spectrum_starts, 0, "no cava while compact");
    assert_eq!(position_gets.load(Ordering::SeqCst), gets, "Position read while compact");
    assert_eq!(service_thread_switches(), switches, "service thread woke while compact");
    assert!(!_spectrum.topic.snapshot().value.running);

    // ---- lap.media.expand: the first frame after taking the lease, no waiting.
    let (gets, switches) = (position_gets.load(Ordering::SeqCst), service_thread_switches());
    let lease = mpris::present(&h.ctx.store);
    let now = monotonic_us();
    let s = handle.topic.snapshot().value;
    let truth = Player::now_us(started_us) as f64 / 1e6;
    assert_eq!((s.title.as_str(), s.artist.as_str(), s.playing, s.length_seconds), ("Compact Song", "Band", true, 240.0));
    let shown = s.position_seconds(now);
    assert!((shown - truth).abs() < 0.05, "first-frame position {shown} vs player {truth}");
    assert!(shown > 33.0, "position advanced through the compact window: {shown}");
    assert_eq!(presentation.subscriber_count(), 1);

    // Presenting re-arms the Cider repair once (here: not Cider, a no-op) and never
    // starts periodic position reads.
    let counters = h.ctx.counters.snapshot();
    std::thread::sleep(Duration::from_millis(1500));
    let after = h.ctx.counters.snapshot();
    assert_eq!(after.position_reads, counters.position_reads, "no position polling while presented");
    assert_eq!(position_gets.load(Ordering::SeqCst), gets);
    assert_eq!(after.compat_refreshes - counters.compat_refreshes, 1, "one repair on reveal");
    assert!(service_thread_switches() > switches, "the switch probe sees the service thread");
    let s = handle.topic.snapshot().value;
    let (shown, truth) = (s.position_seconds(monotonic_us()), Player::now_us(started_us) as f64 / 1e6);
    assert!((shown - truth).abs() < 0.05, "position {shown} vs player {truth}");

    // Collapsing drops the demand again.
    drop(lease);
    assert_eq!(presentation.subscriber_count(), 0);
    let counters = h.ctx.counters.snapshot();
    std::thread::sleep(Duration::from_millis(500));
    let settled = h.ctx.counters.snapshot();
    assert!(settled.mpris_wakeups - counters.mpris_wakeups <= 1, "{counters:?} -> {settled:?}");
    let switches = service_thread_switches();
    std::thread::sleep(Duration::from_secs(1));
    assert_eq!(h.ctx.counters.snapshot(), settled);
    assert_eq!(service_thread_switches(), switches, "service thread woke after collapse");
    h.runtime.request_shutdown();
}
