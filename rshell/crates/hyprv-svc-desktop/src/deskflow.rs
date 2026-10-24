//! Deskflow right-edge target: port of `deskflow-target.sh`.
//!
//! Status is derived from the server config (inotify on its directory instead of
//! the 2 s panel poll). `Set` rewrites the config (0600, atomic), reloads the
//! service-managed `deskflow-core` with SIGHUP through a pidfd, and waits for the
//! appended "reloaded configuration" log line (IN_MODIFY) under one 2 s deadline;
//! failure restores the backup. The HDMI IR switch is queued separately on the
//! IR actor so a slow serial device never blocks the button.
use crate::{
    hypr::Hypr, ir, systemd::UserManager, topic_key, util, ActionResult, Ctx, Handle, ServiceError,
    Target,
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub const WINDOWS_SCREEN: &str = "WindowsVM";
pub const MACOS_SCREEN: &str = "linhaikuodeMac-mini.local";
const TEMPLATE: &str = include_str!("deskflow-server.conf.in");
const RELOAD_DEADLINE: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeskflowState {
    /// QML `target`: "macos" iff the config links right to the Mac, else "windows".
    pub target: Target,
    /// QML `busy` (label "Switching").
    pub busy: bool,
    pub last_error: Option<String>,
}
impl Default for DeskflowState {
    fn default() -> Self {
        Self {
            target: Target::Windows,
            busy: false,
            last_error: None,
        }
    }
}
topic_key!(DeskflowTopic, DeskflowState);

#[derive(Clone, Debug)]
pub enum DeskflowAction {
    /// `set windows|macos`; the UI's toggle sends the opposite of `target`.
    Set(Target),
    /// `reflow`: regenerate portal ranges for the current target, no IR switch.
    Reflow,
    /// `display-switch TARGET`: IR only.
    DisplaySwitch(Target),
}
pub type DeskflowHandle = Handle<DeskflowState, DeskflowAction>;

#[derive(Clone)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub config_file: PathBuf,
    pub log_file: PathBuf,
    pub transform_file: PathBuf,
}
impl Paths {
    pub fn from_env(env: &crate::Env) -> Self {
        let config_dir = env.config_home.join("Deskflow");
        Self {
            config_file: config_dir.join("deskflow-server.conf"),
            config_dir,
            log_file: env
                .var("DESKFLOW_LOG_FILE")
                .map(PathBuf::from)
                .unwrap_or_else(|| env.home.join("deskflow.log")),
            transform_file: env.state_home.join("hyprv/monitor-transform"),
        }
    }
}

pub fn spawn(ctx: &Ctx, ir: ir::IrHandle) -> DeskflowHandle {
    let topic = crate::topic::<DeskflowTopic>(ctx, DeskflowState::default());
    let (handle, mut rx) = Handle::new(topic.clone());
    let paths = Paths::from_env(&ctx.env);
    let hypr = Hypr::new(&ctx.env);
    let mut shutdown = ctx.shutdown.clone();
    ctx.rt.spawn(async move {
        let mut state = DeskflowState::default();
        // One long-lived watch; it re-arms itself after replaces.
        let watch =
            util::Watch::new(&[(paths.config_dir.clone(), &["deskflow-server.conf"][..])]).ok();
        loop {
            state.target = current_target(&paths.config_file);
            topic.publish(state.clone());
            let changed = async {
                match &watch {
                    Some(w) => {
                        let _ = util::settle(w, Duration::from_millis(30)).await;
                    }
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                _ = changed => {}
                msg = rx.recv() => {
                    let Some(env) = msg else { return };
                    state.busy = true;
                    topic.publish(state.clone());
                    let result = handle_action(&paths, &hypr, &ir, &env.action).await;
                    if let Err(e) = &result {
                        util::notify("Win/macOS switch", &e.message, false, None).await;
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

/// `grep -Eq "right\([0-9]+,[0-9]+\) = linhaikuodeMac-mini.local\(0,100\)"`.
pub fn config_targets_macos(text: &str) -> bool {
    let mut rest = text;
    while let Some(i) = rest.find("right(") {
        let tail = &rest[i + 6..];
        rest = &rest[i + 1..];
        let digits = |s: &str| s.bytes().take_while(u8::is_ascii_digit).count();
        let a = digits(tail);
        if a == 0 || tail.as_bytes().get(a) != Some(&b',') {
            continue;
        }
        let t2 = &tail[a + 1..];
        let b = digits(t2);
        if b == 0 {
            continue;
        }
        let Some(t3) = t2[b..].strip_prefix(") = linhaikuodeMac-mini") else {
            continue;
        };
        let mut chars = t3.chars();
        if chars.next().is_some() && chars.as_str().starts_with("local(0,100)") {
            return true;
        }
    }
    false
}
pub fn current_target(config_file: &Path) -> Target {
    match std::fs::read_to_string(config_file) {
        Ok(text) if config_targets_macos(&text) => Target::MacOs,
        _ => Target::Windows,
    }
}
pub fn portal_range(transform: Option<i64>) -> (u32, u32) {
    match transform {
        Some(1) | Some(3) => (38, 71),
        _ => (28, 87),
    }
}
pub fn render_config(target: Target, (start, end): (u32, u32)) -> String {
    let screen = match target {
        Target::Windows => WINDOWS_SCREEN,
        Target::MacOs => MACOS_SCREEN,
    };
    TEMPLATE
        .replace("__TARGET_SCREEN__", screen)
        .replace("__PORTAL_START__", &start.to_string())
        .replace("__PORTAL_END__", &end.to_string())
}
async fn read_transform(paths: &Paths, hypr: &Hypr) -> Option<i64> {
    if let Some(text) = util::read_string(&paths.transform_file) {
        return text.lines().next().and_then(|l| l.trim().parse().ok());
    }
    let monitors: Vec<serde_json::Value> = hypr.json("monitors all").await.ok()?;
    monitors
        .iter()
        .find(|m| m["name"] == "DP-4")
        .and_then(|m| m["transform"].as_i64())
}
async fn write_config(paths: &Paths, hypr: &Hypr, target: Target) -> Result<(), ServiceError> {
    let range = portal_range(read_transform(paths, hypr).await);
    util::atomic_write(
        &paths.config_file,
        render_config(target, range).as_bytes(),
        Some(0o600),
    )?;
    Ok(())
}

enum Reload {
    Confirmed,
    NotManaged,
    Failed,
}
/// Locate the service-managed core (unit cgroup first, then a unique server process).
pub async fn find_core_pid() -> Option<i32> {
    let proc = util::Proc::default();
    if let Ok(mgr) = UserManager::connect().await {
        if mgr.is_active("deskflow.service").await {
            if let Some(cg) = mgr
                .control_group("deskflow.service")
                .await
                .filter(|c| !c.is_empty())
            {
                let procs = std::fs::read_to_string(format!("/sys/fs/cgroup{cg}/cgroup.procs"))
                    .unwrap_or_default();
                for pid in procs.lines().filter_map(|l| l.trim().parse::<i32>().ok()) {
                    if proc
                        .exe(pid)
                        .is_some_and(|e| e.file_name().is_some_and(|n| n == "deskflow-core"))
                    {
                        return Some(pid);
                    }
                }
            }
        }
    }
    let candidates: Vec<i32> = proc
        .pids()
        .into_iter()
        .filter(|pid| proc.stat(*pid).is_some_and(|s| s.comm == "deskflow-core"))
        .filter(|pid| {
            proc.exe(*pid)
                .is_some_and(|e| e.file_name().is_some_and(|n| n == "deskflow-core"))
                && format!(" {} ", proc.cmdline(*pid).join(" ")).contains(" server ")
        })
        .collect();
    (candidates.len() == 1).then(|| candidates[0])
}
async fn reload(paths: &Paths) -> Reload {
    let Some(pid) = find_core_pid().await else {
        return Reload::NotManaged;
    };
    let Ok(pidfd) = util::Pidfd::open(pid) else {
        return Reload::NotManaged;
    };
    let offset = std::fs::metadata(&paths.log_file)
        .map(|m| m.len())
        .unwrap_or(0);
    // Subscribe to log growth before signalling.
    let watch = util::Inotify::new().ok();
    if let Some(w) = &watch {
        let parent = paths.log_file.parent().unwrap_or(Path::new("/"));
        let _ = w.add(parent, libc::IN_CREATE | libc::IN_MOVED_TO);
        let _ = w.add(&paths.log_file, libc::IN_MODIFY);
    }
    if pidfd.signal(libc::SIGHUP).is_err() {
        return Reload::Failed;
    }
    let confirmed = tokio::time::timeout(RELOAD_DEADLINE, async {
        loop {
            if log_since(&paths.log_file, offset).contains("reloaded configuration") {
                return true;
            }
            tokio::select! {
                _ = async { match &watch { Some(w) => { let _ = w.changed().await; } None => tokio::time::sleep(Duration::from_millis(100)).await } } => {
                    if let Some(w) = &watch { let _ = w.add(&paths.log_file, libc::IN_MODIFY); }
                }
                _ = pidfd.exited() => return log_since(&paths.log_file, offset).contains("reloaded configuration"),
            }
        }
    })
    .await
    .unwrap_or(false);
    if confirmed {
        Reload::Confirmed
    } else {
        Reload::Failed
    }
}
fn log_since(path: &Path, offset: u64) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let mut out = Vec::new();
    if let Ok(mut f) = std::fs::File::open(path) {
        let _ = f.seek(SeekFrom::Start(offset));
        let _ = f.take(1 << 20).read_to_end(&mut out);
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// One-shot `deskflow-target.sh reflow` without a running actor, for
/// `hyprv-shell desktop deskflowReflow` when no shell is reachable: regenerate the portal
/// ranges for the current target and reload. A failed reload restores the previous config.
pub async fn reflow_once(env: &crate::Env) -> ActionResult {
    let paths = Paths::from_env(env);
    apply(&paths, &Hypr::new(env), current_target(&paths.config_file)).await?;
    Ok(String::new())
}

async fn handle_action(
    paths: &Paths,
    hypr: &Hypr,
    ir: &ir::IrHandle,
    action: &DeskflowAction,
) -> ActionResult {
    match action {
        DeskflowAction::DisplaySwitch(t) => {
            display_switch(ir, *t).await?;
            Ok(String::new())
        }
        DeskflowAction::Set(target) => {
            apply(paths, hypr, *target).await?;
            queue_display_switch(ir.clone(), *target);
            Ok(format!("{}\n", current_target(&paths.config_file).as_str()))
        }
        DeskflowAction::Reflow => {
            apply(paths, hypr, current_target(&paths.config_file)).await?;
            Ok(String::new())
        }
    }
}
async fn apply(paths: &Paths, hypr: &Hypr, target: Target) -> Result<(), ServiceError> {
    let backup = std::fs::read(&paths.config_file).map_err(|e| {
        ServiceError::new(
            "io",
            format!("cp: cannot stat '{}': {e}", paths.config_file.display()),
        )
    })?;
    write_config(paths, hypr, target).await?;
    match reload(paths).await {
        Reload::Confirmed => Ok(()),
        // Not service-managed: configuration applies on its next start.
        Reload::NotManaged => Ok(()),
        Reload::Failed => {
            util::atomic_write(&paths.config_file, &backup, Some(0o600))?;
            let _ = reload(paths).await;
            Err(ServiceError::new(
                "reload",
                "Restored the previous Deskflow configuration",
            ))
        }
    }
}
async fn display_switch(ir: &ir::IrHandle, target: Target) -> Result<(), ServiceError> {
    ir.request(ir::IrAction::Send(target.ir_button().into()))
        .await
        .map(|_| ())
}
/// Detached like the transient `hyprv-display-switch-*` unit.
fn queue_display_switch(ir: ir::IrHandle, target: Target) {
    tokio::spawn(async move {
        if display_switch(&ir, target).await.is_err() {
            util::notify(
                "Win/macOS switch",
                &format!("HDMI switch to {} failed", target.as_str()),
                false,
                None,
            )
            .await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_regex() {
        let mac = render_config(Target::MacOs, (28, 87));
        assert!(config_targets_macos(&mac));
        assert!(
            mac.contains("\tSuperPower:\n\t\tright(28,87) = linhaikuodeMac-mini.local(0,100)\n")
        );
        assert!(mac.contains(
            "keystroke(control+alt+BracketR) = switchToScreen(linhaikuodeMac-mini.local)"
        ));
        let win = render_config(Target::Windows, portal_range(Some(1)));
        assert!(!config_targets_macos(&win));
        assert!(win.contains("right(38,71) = WindowsVM(0,100)"));
        assert!(config_targets_macos(
            "right(1,2) = linhaikuodeMac-miniXlocal(0,100)"
        ));
        assert!(!config_targets_macos(
            "right(,2) = linhaikuodeMac-mini.local(0,100)"
        ));
    }
    #[test]
    fn template_matches_live_config_layout() {
        // Byte-for-byte equality with the script's rendering of the live file.
        if let Ok(live) = std::fs::read_to_string(
            std::path::Path::new(&std::env::var("HOME").unwrap_or_default())
                .join(".config/Deskflow/deskflow-server.conf"),
        ) {
            let t = current_target_text(&live);
            for range in [(28, 87), (38, 71)] {
                if render_config(t, range) == live {
                    return;
                }
            }
            panic!("rendered config differs from the live script output");
        }
    }
    fn current_target_text(text: &str) -> Target {
        if config_targets_macos(text) {
            Target::MacOs
        } else {
            Target::Windows
        }
    }
}
