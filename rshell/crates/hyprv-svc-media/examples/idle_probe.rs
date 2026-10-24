//! Read-only idle probe: starts every media actor in a read-only context, lets them
//! settle, then reports per-thread context switches over a quiet window.
//! `cargo run --example idle_probe -- [seconds]`
use hyprv_core::{runtime::ServiceRuntime, store::StateStore};
use hyprv_svc_media::{audio, cider, mpris, notifications, spectrum, tray, Context};
use std::{collections::BTreeMap, sync::Arc, time::Duration};

fn switches() -> BTreeMap<String, u64> {
    let mut out = BTreeMap::new();
    for entry in std::fs::read_dir("/proc/self/task").unwrap().flatten() {
        let dir = entry.path();
        let name = std::fs::read_to_string(dir.join("comm")).unwrap_or_default().trim().to_owned();
        let status = std::fs::read_to_string(dir.join("status")).unwrap_or_default();
        let total: u64 = status
            .lines()
            .filter(|l| l.starts_with("voluntary_ctxt_switches") || l.starts_with("nonvoluntary_ctxt_switches"))
            .filter_map(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
            .sum();
        out.insert(format!("{name}[{}]", entry.file_name().to_string_lossy()), total);
    }
    out
}

fn main() {
    let seconds: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let runtime = Arc::new(ServiceRuntime::new().unwrap());
    let mut ctx = Context::new(Arc::new(StateStore::default()), runtime.clone());
    ctx.read_only = true;
    let c = cider::spawn(&ctx);
    let m = mpris::spawn(&ctx, Some(c.requester()));
    let gate = mpris::playing_gate(&ctx, &m.topic);
    let _s = spectrum::spawn(&ctx, gate); // no subscriber → no capture
    let a = audio::spawn(&ctx);
    let n = notifications::spawn(&ctx);
    let t = tray::spawn(&ctx);
    // Bar-like subscribers (spectrum deliberately not subscribed: media hidden).
    let _subs = (a.topic.subscribe(), m.topic.subscribe(), n.topic.subscribe(), t.topic.subscribe());
    std::thread::sleep(Duration::from_secs(3));
    let before = switches();
    std::thread::sleep(Duration::from_secs(seconds));
    let after = switches();
    println!("context switches over {seconds}s (excluding main thread):");
    for (thread, count) in &after {
        let delta = count - before.get(thread).copied().unwrap_or(0);
        println!("  {thread:<32} {delta}");
    }
    println!("audio: {:?}", a.topic.snapshot().value);
    println!("mpris available={} playing={}", m.topic.snapshot().value.available, m.topic.snapshot().value.playing);
    runtime.request_shutdown();
}
