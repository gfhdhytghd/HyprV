//! Per-event cost of the agent-island refresh against the live state file.
use hyprv_svc_desktop::agent_island::{incremental::Incremental, model};
use std::time::Instant;
fn main() {
    let path = format!(
        "{}/.cache/hyprv/agent-island/state.json",
        std::env::var("HOME").unwrap()
    );
    let bytes = std::fs::read(&path).unwrap();
    let now = hyprv_svc_desktop::util::epoch_f64();
    let proc = hyprv_svc_desktop::util::Proc::default();
    let n = 500u32;
    // Old path: full Value parse + cleanup + view + expiry (+2x content_key before).
    let t = Instant::now();
    for _ in 0..n {
        let mut st: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let mut a = |p: i64| proc.exists(p as i32);
        model::cleanup_state(&mut st, now, &mut a);
        let _ = model::public_sessions(&mut st, now);
        let _ = model::next_expiry(&st, &mut a);
    }
    println!("full path (no content_key): {:?}/event", t.elapsed() / n);
    let mut engine = Incremental::default();
    let mut overlay = model::Obj::new();
    // The actor answers liveness from its pidfd set (no stat per session).
    let known: std::collections::HashSet<i64> = {
        let st: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        model::referenced_pids(&st)
            .into_iter()
            .filter(|p| proc.exists(*p as i32))
            .collect()
    };
    let mut a = |p: i64| known.contains(&p);
    let t = Instant::now();
    let v = engine.refresh(&bytes, &mut overlay, now, &mut a).unwrap();
    println!(
        "fast path cold: {:?} ({} rows, {} parsed)",
        t.elapsed(),
        v.rows.len(),
        engine.parsed_last
    );
    let t = Instant::now();
    for _ in 0..n {
        engine.refresh(&bytes, &mut overlay, now, &mut a).unwrap();
    }
    println!("fast path unchanged: {:?}/event", t.elapsed() / n);
    // One session changes per event (what a hook write does).
    let mut doc: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let sid = doc["sessions"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    let variants: Vec<Vec<u8>> = (0..n)
        .map(|i| {
            doc["sessions"][&sid]["last_activity"] = serde_json::json!(now + i as f64);
            serde_json::to_vec(&doc).unwrap()
        })
        .collect();
    let t = Instant::now();
    for b in &variants {
        engine.refresh(b, &mut overlay, now, &mut a).unwrap();
    }
    println!(
        "fast path one-changed: {:?}/event (parsed {})",
        t.elapsed() / n,
        engine.parsed_last
    );
}
