//! Actor-only cost: agent island on the live hook directory (read-only mode),
//! one subscriber counting publications, service-thread runtime from schedstat.
use hyprv_core::{runtime::ServiceRuntime, store::StateStore};
use hyprv_svc_desktop::{agent_island, Ctx, Env};
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
fn service_ns() -> (u64, u64) {
    let mut ns = 0;
    let mut slices = 0;
    for task in std::fs::read_dir("/proc/self/task").unwrap().flatten() {
        if std::fs::read_to_string(task.path().join("comm"))
            .unwrap_or_default()
            .trim()
            != "hyprv-services"
        {
            continue;
        }
        let s = std::fs::read_to_string(task.path().join("schedstat")).unwrap_or_default();
        let f: Vec<u64> = s
            .split_whitespace()
            .filter_map(|x| x.parse().ok())
            .collect();
        ns += f.first().copied().unwrap_or(0);
        slices += f.get(2).copied().unwrap_or(0);
    }
    (ns, slices)
}
fn main() {
    let secs: u64 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(30);
    let rt = ServiceRuntime::new().unwrap();
    let ctx = Ctx::new(
        Arc::new(StateStore::default()),
        rt.enter(tokio::runtime::Handle::current),
        rt.shutdown_signal(),
    )
    .with_env(Env::from_process());
    let island = rt.enter(|| agent_island::spawn(&ctx));
    let pubs = Arc::new(AtomicU64::new(0));
    let (p, mut sub) = (pubs.clone(), island.subscribe());
    rt.spawn(async move {
        while sub.changed().await.is_ok() {
            p.fetch_add(1, Ordering::Relaxed);
        }
    });
    std::thread::sleep(Duration::from_secs(3));
    let (n0, s0) = service_ns();
    let p0 = pubs.load(Ordering::Relaxed);
    std::thread::sleep(Duration::from_secs(secs));
    let (n1, s1) = service_ns();
    let n = pubs.load(Ordering::Relaxed) - p0;
    let ms = (n1 - n0) as f64 / 1e6;
    println!("{secs}s: service thread {ms:.1} ms CPU ({:.3}% of a core), {} timeslices, {n} publications, {:.0} µs/publication",
        ms / (secs as f64 * 10.0), s1 - s0, if n > 0 { ms * 1000.0 / n as f64 } else { 0.0 });
    rt.request_shutdown();
}
