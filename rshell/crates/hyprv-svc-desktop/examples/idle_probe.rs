//! Idle-wakeup probe: spawn every actor, let them settle, then count context
//! switches of the service thread over a quiet window. Read-only except the
//! agent-island directory, which is redirected to a temp dir.
use hyprv_core::{runtime::ServiceRuntime, store::StateStore};
use hyprv_svc_desktop::{Ctx, Env};
use std::{sync::Arc, time::Duration};

fn switches() -> u64 {
    let mut total = 0;
    for task in std::fs::read_dir("/proc/self/task").unwrap().flatten() {
        let name = std::fs::read_to_string(task.path().join("comm")).unwrap_or_default();
        if name.trim() != "hyprv-services" {
            continue;
        }
        let status = std::fs::read_to_string(task.path().join("status")).unwrap_or_default();
        for line in status.lines() {
            if let Some(v) = line.strip_prefix("voluntary_ctxt_switches:") {
                total += v.trim().parse::<u64>().unwrap_or(0);
            }
        }
    }
    total
}
fn main() {
    let secs: u64 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(10);
    let rt = ServiceRuntime::new().unwrap();
    let tmp = std::env::temp_dir().join(format!("hyprv-idle-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let mut vars: std::collections::BTreeMap<String, String> = std::env::vars().collect();
    vars.insert("HYPRV_AGENT_ISLAND_DIR".into(), tmp.display().to_string());
    let store = Arc::new(StateStore::default());
    let ctx = Ctx::new(
        store,
        rt.enter(tokio::runtime::Handle::current),
        rt.shutdown_signal(),
    )
    .with_env(Env::from_vars(vars));
    if let Ok(only) = std::env::var("PROBE_ONLY") {
        rt.enter(|| {
            use hyprv_svc_desktop::*;
            let ir = ir::spawn(&ctx);
            let _keep: Vec<Box<dyn std::any::Any>> = only
                .split(',')
                .map(|m| -> Box<dyn std::any::Any> {
                    match m {
                        "agent" => Box::new(agent_island::spawn(&ctx)),
                        "deskflow" => Box::new(deskflow::spawn(&ctx, ir.clone())),
                        "viewflow" => Box::new(viewflow::spawn(&ctx, ir.clone())),
                        "egpu" => Box::new(egpu::spawn(&ctx)),
                        "demo" => Box::new(demo::spawn(&ctx, demo::default_inhibitor())),
                        "recording" => Box::new(recording::spawn(&ctx)),
                        "theme" => Box::new(theme::spawn(&ctx)),
                        "apps" => Box::new(applications::spawn(&ctx)),
                        _ => Box::new(()),
                    }
                })
                .collect();
            std::mem::forget(_keep);
        });
        std::thread::sleep(Duration::from_secs(2));
        let before = switches();
        std::thread::sleep(Duration::from_secs(secs));
        println!("{only}: {}", switches() - before);
        return;
    }
    let services = rt.enter(|| hyprv_svc_desktop::spawn_all(&ctx));
    std::thread::sleep(Duration::from_secs(2));
    let before = switches();
    std::thread::sleep(Duration::from_secs(secs));
    let after = switches();
    println!(
        "service-thread wakeups over {secs}s idle: {}",
        after - before
    );
    println!("ir device: {:?}", services.ir.state().device);
    println!(
        "theme: {:?} dark={}",
        services.theme.state().theme,
        services.theme.state().dark_mode
    );
    println!("viewflow: {:?}", services.viewflow.state());
    println!("deskflow: {:?}", services.deskflow.state().target);
    println!("apps: {}", services.applications.state().applications.len());
    println!("egpu: {:?}", services.egpu.state().devices);
    println!("recording: {:?}", services.recording.state());
    println!("demo: {:?}", services.demo.state());
    rt.request_shutdown();
    let _ = std::fs::remove_dir_all(tmp);
}
