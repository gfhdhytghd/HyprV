//! Read-only integration checks against the live session. Every directory an
//! actor could write (agent state, ui-state, demo journal, viewflow selectors)
//! is redirected into a temporary root; nothing here triggers an action.
use hyprv_core::store::StateStore;
use hyprv_svc_desktop::{agent_island, Ctx, Env};
use std::{sync::Arc, time::Duration};

fn temp_root(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("hyprv-svc-desktop-it-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}
fn live_env(root: &std::path::Path) -> Env {
    let mut vars: std::collections::BTreeMap<String, String> = std::env::vars().collect();
    vars.insert(
        "HYPRV_AGENT_ISLAND_DIR".into(),
        root.join("agent").display().to_string(),
    );
    vars.insert(
        "XDG_STATE_HOME".into(),
        root.join("state").display().to_string(),
    );
    Env::from_vars(vars)
}
async fn wait_for<T: Clone + PartialEq>(
    topic: &hyprv_core::store::Topic<T>,
    pred: impl Fn(&T) -> bool,
) -> T {
    let mut sub = topic.subscribe();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let cur = sub.current().value;
        if pred(&cur) {
            return cur;
        }
        tokio::time::timeout_at(deadline, sub.changed())
            .await
            .expect("timeout")
            .unwrap();
    }
}

#[tokio::test(flavor = "current_thread")]
async fn all_actors_start_and_publish() {
    let root = temp_root("all");
    let (_tx, shutdown) = tokio::sync::watch::channel(false);
    let ctx = Ctx::new(
        Arc::new(StateStore::default()),
        tokio::runtime::Handle::current(),
        shutdown,
    )
    .with_env(live_env(&root));
    let s = hyprv_svc_desktop::spawn_all(&ctx);

    let apps = wait_for(&s.applications.topic, |a| !a.applications.is_empty()).await;
    assert!(apps
        .applications
        .windows(2)
        .all(|w| w[0].name.to_lowercase() <= w[1].name.to_lowercase()));

    let ir = wait_for(&s.ir.topic, |i| !i.database.as_os_str().is_empty()).await;
    let picos = std::fs::read_dir("/dev/serial/by-id")
        .map(|it| {
            it.flatten()
                .filter(|e| {
                    let n = e.file_name().to_string_lossy().into_owned();
                    n.contains("MicroPython") || n.contains("Raspberry_Pi_Pico")
                })
                .count()
        })
        .unwrap_or(0);
    assert_eq!(ir.device.is_some(), picos == 1, "{ir:?}");

    let expected = hyprv_svc_desktop::deskflow::current_target(
        &ctx.env.config_home.join("Deskflow/deskflow-server.conf"),
    );
    let df = wait_for(&s.deskflow.topic, |_| true).await;
    assert_eq!(df.target, expected);

    let theme = wait_for(&s.theme.topic, |t| {
        t.theme == hyprv_svc_desktop::theme::legacy_theme(&ctx.env)
    })
    .await;
    assert_eq!(
        theme.dark_mode,
        theme.theme == hyprv_svc_desktop::theme::Theme::Dark
    );
    assert!(
        root.join("state/hyprv/quickshell/ui-state.env").exists(),
        "ensure_state wrote into the temp root"
    );

    // eGPU inventory, recording and demo must simply publish without panicking.
    let _ = s.egpu.state();
    let _ = s.recording.state();
    let demo = s.demo.state();
    assert!(!demo.enabled);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn agent_island_follows_hook_writes() {
    let root = temp_root("agent");
    let (_tx, shutdown) = tokio::sync::watch::channel(false);
    let ctx = Ctx::new(
        Arc::new(StateStore::default()),
        tokio::runtime::Handle::current(),
        shutdown,
    )
    .with_env(live_env(&root));
    let island = agent_island::spawn(&ctx);
    let first = wait_for(&island.topic, |_| true).await;
    assert!(!first.active);
    // Simulate a hook write (atomic replace, like the Python/Rust hooks).
    let now = hyprv_svc_desktop::util::epoch_f64();
    let state = serde_json::json!({"version":1,"updated_at":now,"pending_requests":{},"sessions":{
        "s1":{"id":"s1","source":"claude","source_label":"Claude Code","title":"t","project":"p",
              "status":"running","current_tool":"Bash","detail":"ls","progress":0.42,
              "last_activity":now,"event":"PreToolUse"}}});
    hyprv_svc_desktop::util::atomic_write(
        &root.join("agent/state.json"),
        state.to_string().as_bytes(),
        None,
    )
    .unwrap();
    let s = wait_for(&island.topic, |s| s.active).await;
    assert_eq!(s.count, 1);
    assert_eq!(s.sessions[0].current_tool, "Bash");
    assert!((s.sessions[0].progress_at(now) - 0.42).abs() < 1e-9);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn hook_binary_records_observation_only() {
    let root = temp_root("hook");
    let bin = env!("CARGO_BIN_EXE_hyprv-agent-hook");
    let mut child = std::process::Command::new(bin)
        .args(["--source", "claude-code"])
        .env("HYPRV_AGENT_ISLAND_DIR", root.join("agent"))
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"hook_event_name":"PermissionRequest","session_id":"abc","cwd":"/x/proj","tool_name":"Bash","tool_input":{"command":"rm -rf build"}}"#)
        .unwrap();
    assert!(child.wait().unwrap().success());
    let state: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("agent/state.json")).unwrap())
            .unwrap();
    let s = &state["sessions"]["abc"];
    assert_eq!(s["status"], "waitingApproval");
    assert_eq!(s["detail"], "rm -rf build");
    assert!(
        s.get("pending_request_id").is_none(),
        "hook never keeps a pending request"
    );
    assert!(state["pending_requests"].as_object().unwrap().is_empty());
    assert_eq!(
        s["terminal"]["pid"].as_i64().unwrap(),
        std::process::id() as i64
    );
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn hyprland_socket_readonly_queries() {
    let env = Env::from_process();
    let hypr = hyprv_svc_desktop::hypr::Hypr::new(&env);
    if hypr.instance_dir().is_none() {
        eprintln!("no Hyprland instance; skipping");
        return;
    }
    let monitors: Vec<serde_json::Value> = hypr.json("monitors all").await.unwrap();
    assert!(!monitors.is_empty());
    let externals = hyprv_svc_desktop::demo::external_outputs(&monitors);
    eprintln!(
        "external outputs: {externals:?}, position {}",
        hyprv_svc_desktop::demo::external_position(&monitors)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn systemd_user_manager_readonly() {
    let Ok(mgr) = hyprv_svc_desktop::systemd::UserManager::connect().await else {
        return;
    };
    // Querying never loads or starts anything (GetUnit on loaded units only).
    let paths = hyprv_svc_desktop::viewflow::Paths::from_env(&Env::from_process());
    let route = hyprv_svc_desktop::viewflow::current_target(&paths, Some(&mgr)).await;
    eprintln!(
        "viewflow input route: {route:?}, topology supervised: {}",
        mgr.is_active(hyprv_svc_desktop::viewflow::TOPOLOGY_UNIT)
            .await
    );
}
