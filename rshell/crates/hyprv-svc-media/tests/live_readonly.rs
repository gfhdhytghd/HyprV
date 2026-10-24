//! READ-ONLY checks against the live user session (`cargo test -- --ignored`).
//! Context is `read_only`: no actions, no bus names owned, no host registration.
//! Audio/MPRIS/SwayNC/tray are only observed; nothing is changed.
use hyprv_core::{runtime::ServiceRuntime, store::StateStore};
use hyprv_svc_media::{audio, cider, mpris, notifications, spectrum, tray, Context};
use std::{sync::Arc, time::{Duration, Instant}};

fn ctx() -> (Arc<ServiceRuntime>, Context) {
    let runtime = Arc::new(ServiceRuntime::new().unwrap());
    let mut ctx = Context::new(Arc::new(StateStore::default()), runtime.clone());
    ctx.read_only = true;
    (runtime, ctx)
}

fn settle<T: Clone + PartialEq + std::fmt::Debug>(topic: &hyprv_core::store::Topic<T>, pred: impl Fn(&T) -> bool) -> T {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let v = topic.snapshot().value;
        if pred(&v) { return v }
        std::thread::sleep(Duration::from_millis(20));
    }
    topic.snapshot().value
}

#[test]
#[ignore = "needs the live PipeWire session"]
fn live_audio_observes_default_sink() {
    let (rt, ctx) = ctx();
    let h = audio::spawn(&ctx);
    let s = settle(&h.topic, |s| s.available);
    eprintln!("audio: {s:?}");
    assert!(s.ready, "PipeWire registry not ready");
    assert!(rt.block_on(h.action(audio::AudioAction::ToggleMute).result()).is_err(), "read-only must refuse");
    rt.request_shutdown();
    rt.block_on(h.join());
}

#[test]
#[ignore = "needs the live session bus"]
fn live_mpris_notifications_tray() {
    let (rt, ctx) = ctx();
    let c = cider::spawn(&ctx);
    let m = mpris::spawn(&ctx, Some(c.requester()));
    let n = notifications::spawn(&ctx);
    let t = tray::spawn(&ctx);
    let ms = settle(&m.topic, |s| s.available);
    eprintln!("mpris: available={} playing={} player={:?} title={:?} artist={:?} len={} pos={:.1} cider={} players={}",
        ms.available, ms.playing, ms.player_name, ms.title, ms.artist, ms.length_seconds,
        ms.position_seconds(hyprv_svc_media::monotonic_us()), ms.cider, ms.players.len());
    let ns = settle(&n.topic, |s| s.available);
    eprintln!("notifications: {ns:?}");
    let ts = settle(&t.topic, |s| s.watcher_available && !s.items.is_empty() && s.items.iter().all(|i| !i.id.is_empty()));
    for i in &ts.items {
        eprintln!("tray: id={:?} title={:?} icon={:?} pixmaps={} menu={:?} tooltip={:?}", i.id, i.title, i.icon, i.icon_pixmaps.len(), i.menu, i.tooltip_title);
    }
    assert!(!ts.watcher_owned, "read-only must not take the watcher name");
    rt.request_shutdown();
}

#[test]
#[ignore = "needs live PipeWire; creates a passive capture stream only while running"]
fn live_spectrum_demand_lifecycle() {
    let (rt, ctx) = ctx();
    let (gate_tx, gate) = tokio::sync::watch::channel(false);
    let h = spectrum::spawn(&ctx, gate);
    std::thread::sleep(Duration::from_millis(200));
    assert!(!h.topic.snapshot().value.running, "no capture without demand");
    let sub = h.topic.subscribe();
    std::thread::sleep(Duration::from_millis(200));
    assert!(!h.topic.snapshot().value.running, "no capture while gate closed");
    gate_tx.send_replace(true);
    let s = settle(&h.topic, |s| s.running);
    assert!(s.running);
    std::thread::sleep(Duration::from_millis(500));
    eprintln!("spectrum: {:?}", h.topic.snapshot().value);
    drop(sub);
    let s = settle(&h.topic, |s| !s.running);
    assert_eq!(s, spectrum::SpectrumState::default());
    rt.request_shutdown();
}
