//! HDMI selector + Viewflow input route: port of
//! `/home/wilf/data/viewflow/tools/viewflow-target.sh` (delegated from
//! `scripts/viewflow-target.sh`).
//!
//! Observed separately: the HDMI selector file, the input route (user units),
//! whether the topology supervisor owns reconciliation, and peer readiness.
//! Sources are inotify (state/config/runtime dirs) and systemd unit
//! PropertiesChanged; readiness waits are event-triggered with one 20 s deadline.
use crate::{
    ir,
    systemd::{UnitProxy, UserManager},
    topic_key, util, ActionResult, Ctx, Handle, ServiceError, Target,
};
use futures_util::{stream::select_all, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub const TOPOLOGY_UNIT: &str = "viewflow-display-topology.service";
pub const DESKTOP_UNIT: &str = "viewflow-desktop.service";
const READY_DEADLINE: Duration = Duration::from_secs(20);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewflowState {
    /// QML `target` (`status`): `hdmi-target` when readable, else the input route.
    pub target: Target,
    /// Raw HDMI selector file, if present.
    pub hdmi_selected: Option<Target>,
    /// `current_target`: active input unit, else stored `viewflow-target`, else windows.
    pub input_route: Target,
    /// `viewflow-display-topology.service` active (it owns reconciliation).
    pub topology_supervised: bool,
    /// Runtime readiness of the cursor peer for the current input route.
    pub peer_ready: bool,
    /// QML `busy` ("Switching HDMI / Viewflow").
    pub busy: bool,
    pub last_error: Option<String>,
}
impl Default for ViewflowState {
    fn default() -> Self {
        Self {
            target: Target::Windows,
            hdmi_selected: None,
            input_route: Target::Windows,
            topology_supervised: false,
            peer_ready: false,
            busy: false,
            last_error: None,
        }
    }
}
topic_key!(ViewflowTopic, ViewflowState);

#[derive(Clone, Debug)]
pub enum ViewflowAction {
    Set(Target),
}
pub type ViewflowHandle = Handle<ViewflowState, ViewflowAction>;

/// Resolves the peer endpoint (`desktop_peer_discovery.py`). Implementations
/// must not spawn helpers; see README for the SSH identity-verification gap.
pub trait PeerResolver: Send + Sync {
    fn resolve<'a>(
        &'a self,
        config: &'a Value,
        start_windows: bool,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<String, ServiceError>> + Send + 'a>,
    >;
}
/// Configs without `peer_discovery` resolve to `remote`; identified peers need
/// SSH host-key verification, which has no native adapter yet.
pub struct NativeResolver;
impl PeerResolver for NativeResolver {
    fn resolve<'a>(
        &'a self,
        config: &'a Value,
        start_windows: bool,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<String, ServiceError>> + Send + 'a>,
    > {
        Box::pin(async move {
            if config
                .get("peer_discovery")
                .is_some_and(|p| !p.is_null() && p != &Value::Bool(false))
                || start_windows
            {
                return Err(ServiceError::unsupported(
                    "peer discovery requires SSH identity verification (no native adapter)",
                ));
            }
            config["remote"]
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| ServiceError::new("invalid", "config has no remote"))
        })
    }
}

#[derive(Clone)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
    pub runtime: PathBuf,
}
impl Paths {
    pub fn from_env(env: &crate::Env) -> Self {
        Self {
            config_dir: env.config_home.join("viewflow"),
            state_dir: env.state_home.join("hyprv"),
            runtime: env.runtime_dir.join("viewflow"),
        }
    }
    fn target_config(&self, t: Target) -> PathBuf {
        self.config_dir.join(match t {
            Target::Windows => "windows-input-source.json",
            Target::MacOs => "macos-input-source.json",
        })
    }
}
pub fn unit_for(t: Target) -> &'static str {
    match t {
        Target::Windows => "viewflow-windows-input.service",
        Target::MacOs => "viewflow-macos-input.service",
    }
}

pub fn spawn(ctx: &Ctx, ir: ir::IrHandle) -> ViewflowHandle {
    spawn_with(ctx, ir, Box::new(NativeResolver))
}
pub fn spawn_with(ctx: &Ctx, ir: ir::IrHandle, resolver: Box<dyn PeerResolver>) -> ViewflowHandle {
    let topic = crate::topic::<ViewflowTopic>(ctx, ViewflowState::default());
    let (handle, mut rx) = Handle::<ViewflowState, ViewflowAction>::new(topic.clone());
    let paths = Paths::from_env(&ctx.env);
    let mut shutdown = ctx.shutdown.clone();
    ctx.rt.spawn(async move {
        let mgr = UserManager::connect().await.ok();
        if let Some(m) = &mgr {
            let _ = m.manager.subscribe().await;
        }
        let mut state = ViewflowState::default();
        // Created once: property streams yield their current value on creation.
        let mut units = unit_changes(mgr.as_ref()).await;
        if let Some(u) = units.as_mut() {
            while tokio::time::timeout(Duration::from_millis(50), u.next())
                .await
                .is_ok_and(|v| v.is_some())
            {}
        }
        // One long-lived watch; it re-arms itself after replaces.
        let mut specs = vec![
            (
                paths.state_dir.clone(),
                &["hdmi-target", "viewflow-target"][..],
            ),
            (
                paths.config_dir.clone(),
                &["windows-input-source.json", "macos-input-source.json"][..],
            ),
        ];
        specs.extend(runtime_specs(&paths.runtime));
        let watch = util::Watch::new(&specs).ok();
        loop {
            observe(&paths, mgr.as_ref(), &mut state).await;
            topic.publish(state.clone());
            let changed = async {
                match &watch {
                    Some(w) => {
                        let _ = util::settle(w, Duration::from_millis(30)).await;
                    }
                    None => std::future::pending().await,
                }
            };
            let unit_event = async {
                match units.as_mut() {
                    Some(s) if !s.is_empty() => {
                        s.next().await;
                    }
                    _ => std::future::pending().await,
                }
            };
            tokio::select! {
                _ = changed => {}
                _ = unit_event => {}
                msg = rx.recv() => {
                    let Some(env) = msg else { return };
                    state.busy = true;
                    topic.publish(state.clone());
                    let ViewflowAction::Set(t) = env.action.clone();
                    let result = match &mgr {
                        Some(m) => set(&paths, m, &ir, resolver.as_ref(), t).await,
                        None => Err(ServiceError::new("dbus", "systemd user manager unavailable")),
                    };
                    if let Err(e) = &result {
                        util::notify("Viewflow Apple/Windows", &e.message, false, None).await;
                    }
                    state.busy = false;
                    state.last_error = result.as_ref().err().map(|e| e.message.clone());
                    env.respond(result);
                }
                _ = shutdown.changed() => if *shutdown.borrow() { return },
            }
        }
    });
    handle
}

const WATCHED_UNITS: &[&str] = &[
    TOPOLOGY_UNIT,
    DESKTOP_UNIT,
    "viewflow-windows-input.service",
    "viewflow-macos-input.service",
];
async fn unit_changes(
    mgr: Option<&UserManager>,
) -> Option<futures_util::stream::SelectAll<futures_util::stream::BoxStream<'static, ()>>> {
    let mgr = mgr?;
    let mut streams = Vec::new();
    for unit in WATCHED_UNITS {
        let Ok(path) = mgr.manager.load_unit(unit).await else {
            continue;
        };
        let Ok(proxy) = UnitProxy::builder(&mgr.conn).path(path).ok()?.build().await else {
            continue;
        };
        let s = proxy
            .receive_active_state_changed()
            .await
            .map(|_| ())
            .boxed();
        streams.push(s);
    }
    Some(select_all(streams))
}
async fn observe(paths: &Paths, mgr: Option<&UserManager>, state: &mut ViewflowState) {
    state.hdmi_selected =
        util::read_string(&paths.state_dir.join("hdmi-target")).and_then(|t| Target::parse(&t));
    state.input_route = current_target(paths, mgr).await;
    state.target = match util::read_string(&paths.state_dir.join("hdmi-target")) {
        // `cat` of an unreadable/garbage file: QML ignores non-target text.
        Some(t) => Target::parse(&t).unwrap_or(state.target),
        None => state.input_route,
    };
    state.topology_supervised = match mgr {
        Some(m) => m.is_active(TOPOLOGY_UNIT).await,
        None => false,
    };
    let template = std::fs::read(paths.target_config(state.input_route))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    state.peer_ready = template.is_some_and(|t| ready(&paths.runtime, &t, None));
}
pub async fn current_target(paths: &Paths, mgr: Option<&UserManager>) -> Target {
    if let Some(m) = mgr {
        if m.is_active(unit_for(Target::MacOs)).await {
            return Target::MacOs;
        }
        if m.is_active(unit_for(Target::Windows)).await || m.is_active(DESKTOP_UNIT).await {
            return Target::Windows;
        }
    }
    util::read_string(&paths.state_dir.join("viewflow-target"))
        .and_then(|t| Target::parse(&t))
        .unwrap_or(Target::Windows)
}

/// The `wait_ready` predicate: cursor peer PID/exe, compositor socket, cursor-ready
/// PID, and desktop-drag config matching remote/pointer.devices/server_name.
pub fn ready(runtime: &Path, template: &Value, expected_remote: Option<&str>) -> bool {
    let check = || -> Option<bool> {
        let pid: i32 = util::read_trim(&runtime.join("desktop-drag/daemon.pid"))?
            .parse()
            .ok()?;
        let exe = std::fs::canonicalize(format!("/proc/{pid}/exe")).ok()?;
        let mut valid =
            exe.file_name()? == "vf-cursor-peer" && runtime.join("hyprland.sock").exists();
        valid = valid
            && util::read_trim(&runtime.join("cursor-ready"))?
                .parse::<i32>()
                .ok()?
                == pid;
        let config: Value =
            serde_json::from_str(&util::read_string(&runtime.join("desktop-drag/config"))?).ok()?;
        if let Some(remote) = expected_remote {
            valid = valid && config.get("remote")?.as_str()? == remote;
        }
        valid = valid
            && config.get("pointer")?.get("devices")? == template.get("pointer")?.get("devices")?;
        valid = valid && config.get("server_name")? == template.get("server_name")?;
        Some(valid)
    };
    check().unwrap_or(false)
}
fn runtime_specs(runtime: &Path) -> Vec<(PathBuf, &'static [&'static str])> {
    vec![
        (
            runtime.to_path_buf(),
            &["hyprland.sock", "cursor-ready", "desktop-drag"][..],
        ),
        (runtime.join("desktop-drag"), &["daemon.pid", "config"][..]),
    ]
}
async fn wait_ready(paths: &Paths, target: Target, remote: &str) -> bool {
    let Some(template) = std::fs::read(paths.target_config(target))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
    else {
        return false;
    };
    tokio::time::timeout(READY_DEADLINE, async {
        let watch = util::Watch::new(&runtime_specs(&paths.runtime)).ok();
        loop {
            if ready(&paths.runtime, &template, Some(remote)) {
                return true;
            }
            match &watch {
                Some(w) => {
                    let _ = w.changed().await;
                }
                None => tokio::time::sleep(Duration::from_millis(200)).await,
            }
        }
    })
    .await
    .unwrap_or(false)
}
fn write_selector(paths: &Paths, name: &str, t: Target) -> Result<(), ServiceError> {
    util::atomic_write(
        &paths.state_dir.join(name),
        format!("{}\n", t.as_str()).as_bytes(),
        None,
    )?;
    Ok(())
}

async fn set(
    paths: &Paths,
    mgr: &UserManager,
    ir: &ir::IrHandle,
    resolver: &dyn PeerResolver,
    requested: Target,
) -> ActionResult {
    std::fs::create_dir_all(&paths.config_dir)?;
    std::fs::create_dir_all(&paths.state_dir)?;
    let _lock = util::FileLock::exclusive(paths.config_dir.join("target-switch.lock")).await?;
    // HDMI must remain usable even when a Viewflow peer cannot connect.
    if ir
        .request(ir::IrAction::Send(requested.ir_button().into()))
        .await
        .is_err()
    {
        return Err(ServiceError::new(
            "ir",
            format!("HDMI 切换到 {} 失败。", requested.as_str()),
        ));
    }
    write_selector(paths, "hdmi-target", requested)?;
    let hdmi = || util::read_string(&paths.state_dir.join("hdmi-target")).unwrap_or_default();
    // The topology supervisor reconciles after reading the selector.
    if mgr.is_active(TOPOLOGY_UNIT).await {
        return Ok(hdmi());
    }
    let previous = current_target(paths, Some(mgr)).await;
    let (requested_unit, previous_unit) = (unit_for(requested), unit_for(previous));
    let config: Value = std::fs::read(paths.target_config(requested))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null);
    let Ok(expected) = resolver
        .resolve(&config, requested == Target::Windows)
        .await
    else {
        return Err(ServiceError::new(
            "peer",
            "HDMI 已切换；未找到目标电脑，保留当前控制通道。",
        ));
    };
    let runtime_remote = util::read_string(&paths.runtime.join("desktop-drag/config"))
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|c| c.get("remote").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default();
    if requested != previous || runtime_remote != expected || !mgr.is_active(requested_unit).await {
        let _ = mgr.stop(previous_unit).await;
        let ok = mgr.restart(requested_unit).await.is_ok()
            && wait_ready(paths, requested, &expected).await;
        if !ok {
            let _ = mgr.stop(requested_unit).await;
            let _ = mgr.start(previous_unit).await;
            return Err(ServiceError::new(
                "ready",
                "HDMI 已切换；Viewflow 连接失败，已尝试恢复原控制目标。",
            ));
        }
    }
    // Remember the selected input route across login/reboot.
    mgr.manager
        .enable_unit_files(&[requested_unit], false, false)
        .await?;
    if previous_unit != requested_unit {
        let _ = mgr
            .manager
            .disable_unit_files(&[previous_unit], false)
            .await;
    }
    write_selector(paths, "viewflow-target", requested)?;
    Ok(hdmi())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn readiness_predicate() {
        let dir = std::env::temp_dir().join(format!("hyprv-vf-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("desktop-drag")).unwrap();
        let template = json!({"pointer":{"devices":["a"]},"server_name":"S"});
        assert!(!ready(&dir, &template, None));
        // Our own PID's exe is not vf-cursor-peer, so readiness stays false.
        std::fs::write(
            dir.join("desktop-drag/daemon.pid"),
            std::process::id().to_string(),
        )
        .unwrap();
        std::fs::write(dir.join("cursor-ready"), std::process::id().to_string()).unwrap();
        std::fs::write(dir.join("hyprland.sock"), "").unwrap();
        std::fs::write(
            dir.join("desktop-drag/config"),
            json!({"remote":"r","pointer":{"devices":["a"]},"server_name":"S"}).to_string(),
        )
        .unwrap();
        assert!(!ready(&dir, &template, Some("r")));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test(flavor = "current_thread")]
    async fn resolver_contract() {
        let r = NativeResolver;
        assert_eq!(
            r.resolve(&json!({"remote":"1.2.3.4:5"}), false)
                .await
                .unwrap(),
            "1.2.3.4:5"
        );
        assert_eq!(
            r.resolve(&json!({"remote":"x:1","peer_discovery":{"mac":"m"}}), false)
                .await
                .unwrap_err()
                .code,
            "unsupported"
        );
    }
}
