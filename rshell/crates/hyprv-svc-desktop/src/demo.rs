//! Demo mode: port of `demo-mode.sh` as a transaction over the Hyprland request
//! socket with the same on-disk rollback journal
//! (`$XDG_STATE_HOME/hyprv/quickshell/demo-mode/{active,session,source,headless,
//! created-headless,prevent-sleep-was-enabled}`), scoped by instance signature.
//!
//! Sleep prevention goes through [`SleepInhibitor`] so the shell can wire the
//! shared `inhibit` actor (svc-power); the default holds its own logind
//! `Inhibit("idle:sleep")` FD, released only if this module acquired it.
use crate::{hypr::Hypr, topic_key, util, ActionResult, Ctx, Handle, ServiceError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DemoState {
    /// QML `demoModeEnabled` / status `enabled=`.
    pub enabled: bool,
    pub session: String,
    pub source: String,
    pub headless: String,
    pub created_headless: bool,
    pub acquired_inhibitor: bool,
    pub busy: bool,
    pub last_error: Option<String>,
}
topic_key!(DemoTopic, DemoState);

#[derive(Clone, Copy, Debug)]
pub enum DemoAction {
    Enable,
    Disable,
    Toggle,
}
pub type DemoHandle = Handle<DemoState, DemoAction>;

type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
/// Narrow port onto the shared sleep-inhibit owner.
pub trait SleepInhibitor: Send + Sync {
    fn is_enabled(&self) -> BoxFut<'_, bool>;
    fn enable(&self) -> BoxFut<'_, Result<(), ServiceError>>;
    fn disable(&self) -> BoxFut<'_, ()>;
}
/// Default: logind `Inhibit("idle:sleep", "HyprV", "HyprV prevent sleep", "block")`.
#[derive(Default)]
pub struct LogindInhibitor(Mutex<Option<zvariant::OwnedFd>>);
impl SleepInhibitor for LogindInhibitor {
    fn is_enabled(&self) -> BoxFut<'_, bool> {
        Box::pin(async move { self.0.lock().unwrap().is_some() })
    }
    fn enable(&self) -> BoxFut<'_, Result<(), ServiceError>> {
        Box::pin(async move {
            if self.0.lock().unwrap().is_some() {
                return Ok(());
            }
            let conn = zbus::Connection::system().await?;
            let reply = conn
                .call_method(
                    Some("org.freedesktop.login1"),
                    "/org/freedesktop/login1",
                    Some("org.freedesktop.login1.Manager"),
                    "Inhibit",
                    &("idle:sleep", "HyprV", "HyprV prevent sleep", "block"),
                )
                .await?;
            let fd: zvariant::OwnedFd = reply.body().deserialize()?;
            *self.0.lock().unwrap() = Some(fd);
            Ok(())
        })
    }
    fn disable(&self) -> BoxFut<'_, ()> {
        Box::pin(async move {
            self.0.lock().unwrap().take();
        })
    }
}
pub fn default_inhibitor() -> Arc<dyn SleepInhibitor> {
    Arc::new(LogindInhibitor::default())
}

struct Journal {
    dir: PathBuf,
}
impl Journal {
    fn file(&self, n: &str) -> PathBuf {
        self.dir.join(n)
    }
    fn read(&self, n: &str) -> String {
        util::read_string(&self.file(n))
            .map(|s| s.strip_suffix('\n').unwrap_or(&s).to_string())
            .unwrap_or_default()
    }
    fn write(&self, n: &str, v: &str) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        std::fs::write(self.file(n), format!("{v}\n"))
    }
    fn belongs(&self, session: &str) -> bool {
        self.file("active").exists()
            && self.file("session").exists()
            && self.read("session") == session
    }
    fn cleanup_stale(&self, session: &str) {
        if self.file("active").exists() && !self.belongs(session) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
    fn clear(&self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub fn spawn(ctx: &Ctx, inhibitor: Arc<dyn SleepInhibitor>) -> DemoHandle {
    let topic = crate::topic::<DemoTopic>(ctx, DemoState::default());
    let (handle, mut rx) = Handle::new(topic.clone());
    let journal = Journal {
        dir: ctx.env.state_home.join("hyprv/quickshell/demo-mode"),
    };
    let hypr = Hypr::new(&ctx.env);
    let mut shutdown = ctx.shutdown.clone();
    ctx.rt.spawn(async move {
        let mut state = DemoState::default();
        // One long-lived watch; it re-arms itself after replaces.
            let watch = util::Watch::new(&[
            (journal.dir.clone(), &[][..]),
            (journal.dir.parent().unwrap().to_path_buf(), &["demo-mode"][..]),
        ])
        .ok();
        loop {
            let session = hypr.signature().unwrap_or_else(|| "unknown".into());
            observe(&journal, &session, &mut state);
            topic.publish(state.clone());
            let changed = async {
                match &watch {
                    Some(w) => {
                        let _ = util::settle(w, Duration::from_millis(50)).await;
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
                    let enable = match env.action {
                        DemoAction::Enable => true,
                        DemoAction::Disable => false,
                        DemoAction::Toggle => { journal.cleanup_stale(&session); !journal.belongs(&session) }
                    };
                    let result = if enable {
                        enable_demo(&journal, &hypr, inhibitor.as_ref(), &session).await
                    } else {
                        disable_demo(&journal, &hypr, inhibitor.as_ref(), &session).await
                    };
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

fn observe(j: &Journal, session: &str, state: &mut DemoState) {
    j.cleanup_stale(session);
    if j.belongs(session) {
        state.enabled = true;
        state.session = session.into();
        state.source = j.read("source");
        state.headless = j.read("headless");
        state.created_headless = !j.read("created-headless").is_empty();
        state.acquired_inhibitor = j.read("prevent-sleep-was-enabled") == "false";
    } else {
        *state = DemoState {
            busy: state.busy,
            last_error: state.last_error.clone(),
            ..Default::default()
        };
    }
}
fn status_text(j: &Journal, session: &str) -> String {
    if j.belongs(session) {
        format!(
            "enabled=true\nsource={}\nheadless={}\n",
            j.read("source"),
            j.read("headless")
        )
    } else {
        "enabled=false\nsource=\nheadless=\n".into()
    }
}

fn name(m: &Value) -> &str {
    m["name"].as_str().unwrap_or("")
}
fn enabled(m: &Value) -> bool {
    m["disabled"] != Value::Bool(true)
}
fn starts_ci(s: &str, prefixes: &[&str]) -> bool {
    let l = s.to_ascii_lowercase();
    prefixes
        .iter()
        .any(|p| l.starts_with(&p.to_ascii_lowercase()))
}
pub fn external_outputs(monitors: &[Value]) -> Vec<String> {
    monitors
        .iter()
        .filter(|m| {
            enabled(m)
                && !starts_ci(
                    name(m),
                    &["eDP", "LVDS", "DSI", "HEADLESS", "WAYLAND", "WL-"],
                )
        })
        .map(|m| name(m).to_string())
        .collect()
}
pub fn headless_output(monitors: &[Value]) -> Option<String> {
    monitors
        .iter()
        .find(|m| enabled(m) && name(m).starts_with("HEADLESS-"))
        .map(|m| name(m).to_string())
}
/// `portrait_mode_for`: native 1080x1920@59.x/60.x, else 1920x1080@60 + transform 1.
pub fn portrait_mode(monitor: &Value) -> (&'static str, i64) {
    let native = monitor["availableModes"].as_array().is_some_and(|modes| {
        modes.iter().filter_map(Value::as_str).any(|m| {
            m.strip_prefix("1080x1920@")
                .is_some_and(|r| r.starts_with("59.") || r.starts_with("60."))
        })
    });
    if native {
        ("1080x1920@60", 0)
    } else {
        ("1920x1080@60", 1)
    }
}
/// `external_position`: right edge (logical) of the right-most internal panel.
pub fn external_position(monitors: &[Value]) -> String {
    let internal: Vec<(f64, f64)> = monitors
        .iter()
        .filter(|m| enabled(m) && starts_ci(name(m), &["eDP", "LVDS", "DSI"]))
        .map(|m| {
            let f = |k: &str| m[k].as_f64().unwrap_or(0.0);
            let scale = if f("scale") == 0.0 { 1.0 } else { f("scale") };
            let rotated = m["transform"].as_i64().unwrap_or(0) % 2 == 1;
            let w = if rotated { f("height") } else { f("width") } / scale;
            (f("x") + w, f("y"))
        })
        .collect();
    match internal
        .iter()
        .cloned()
        .reduce(|a, b| if b.0 > a.0 { b } else { a })
    {
        None => "auto-right".into(),
        Some((right, top)) => format!("{}x{}", right.floor() as i64, top.floor() as i64),
    }
}
pub fn monitor_expr(
    output: &str,
    mode: &str,
    position: &str,
    transform: i64,
    mirror: Option<&str>,
) -> String {
    let mut e = format!(
        "hl.monitor({{ output = '{output}', mode = '{mode}', position = '{position}', scale = '1', transform = {transform}"
    );
    if let Some(m) = mirror.filter(|m| !m.is_empty()) {
        e.push_str(&format!(", mirror = '{m}'"));
    }
    e.push_str(" })");
    e
}
fn portrait_ok(m: &Value) -> bool {
    let (w, h, t) = (
        m["width"].as_i64(),
        m["height"].as_i64(),
        m["transform"].as_i64(),
    );
    (w == Some(1080) && h == Some(1920) && t == Some(0))
        || (w == Some(1920) && h == Some(1080) && t == Some(1))
}
pub fn verify(
    live: &[Value],
    source: &str,
    headless: &str,
    mirrors: &[String],
) -> Result<(), String> {
    let ok = live.iter().any(|m| name(m) == source && portrait_ok(m))
        && live
            .iter()
            .any(|m| name(m) == headless && m["mirrorOf"] == source);
    if !ok {
        return Err("demo monitor verification failed".into());
    }
    for out in mirrors {
        if !live.iter().any(|m| {
            name(m) == out && m["mirrorOf"] == source.to_string().as_str() && portrait_ok(m)
        }) {
            return Err(format!("external mirror verification failed for {out}"));
        }
    }
    Ok(())
}
fn safe(name: &str) -> Result<(), ServiceError> {
    if crate::hypr::lua_quote(name).is_some() {
        Ok(())
    } else {
        Err(ServiceError::new("invalid", "unsafe output name"))
    }
}

async fn notify(msg: &str) {
    util::notify("HyprV Demo mode", msg, false, None).await;
}
async fn rollback(j: &Journal, hypr: &Hypr, inh: &dyn SleepInhibitor, created: Option<&str>) {
    let _ = hypr.plain("reload").await;
    if let Some(h) = created {
        let _ = hypr.plain(&format!("output remove {h}")).await;
    }
    if j.read("prevent-sleep-was-enabled") == "false" {
        inh.disable().await;
    }
    j.clear();
}

async fn enable_demo(
    j: &Journal,
    hypr: &Hypr,
    inh: &dyn SleepInhibitor,
    session: &str,
) -> ActionResult {
    j.cleanup_stale(session);
    if j.belongs(session) {
        return Ok(status_text(j, session));
    }
    let monitors: Vec<Value> = hypr.json("monitors all").await?;
    let externals = external_outputs(&monitors);
    if externals.is_empty() {
        notify("没有检测到外接显示器，Demo mode 未启动").await;
        return Err(ServiceError::new(
            "no_external",
            "no external display is connected",
        ));
    }
    for e in &externals {
        safe(e)?;
    }
    let source = externals[0].clone();
    let source_json = monitors
        .iter()
        .find(|m| name(m) == source)
        .cloned()
        .unwrap_or(Value::Null);
    let (source_mode, source_transform) = portrait_mode(&source_json);
    let position = external_position(&monitors);
    let mut headless = headless_output(&monitors).unwrap_or_default();

    j.write("session", session)?;
    j.write("source", &source)?;
    j.write("headless", &headless)?;
    std::fs::write(j.file("active"), "")?;
    if inh.is_enabled().await {
        j.write("prevent-sleep-was-enabled", "true")?;
    } else {
        j.write("prevent-sleep-was-enabled", "false")?;
        if let Err(e) = inh.enable().await {
            rollback(j, hypr, inh, None).await;
            return Err(e);
        }
    }

    let mut created: Option<String> = None;
    if headless.is_empty() {
        let before: Vec<String> = monitors.iter().map(|m| name(m).to_string()).collect();
        // Subscribe to socket2 before creating, then wait for the new output.
        let mut events = hypr.events().await.ok();
        let _ = hypr.plain("output create headless").await;
        if let Some(ev) = events.as_mut() {
            let _ = tokio::time::timeout(Duration::from_secs(2), async {
                while let Ok((e, _)) = ev.next().await {
                    if e.starts_with("monitoradded") {
                        break;
                    }
                }
            })
            .await;
        } else {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let after: Vec<Value> = hypr.json("monitors all").await.unwrap_or_default();
        headless = after
            .iter()
            .map(name)
            .find(|n| n.starts_with("HEADLESS-") && !before.iter().any(|b| b == n))
            .unwrap_or("")
            .to_string();
        if headless.is_empty() || safe(&headless).is_err() {
            rollback(j, hypr, inh, None).await;
            notify("Headless 输出创建失败").await;
            return Err(ServiceError::new(
                "headless",
                "failed to create a headless output",
            ));
        }
        created = Some(headless.clone());
        j.write("headless", &headless)?;
        j.write("created-headless", &headless)?;
    }
    let created_ref = created.as_deref();
    if hypr
        .eval(&monitor_expr(
            &source,
            source_mode,
            &position,
            source_transform,
            None,
        ))
        .await
        .is_err()
    {
        rollback(j, hypr, inh, created_ref).await;
        notify("外接显示器配置失败，已恢复原布局").await;
        return Err(ServiceError::new(
            "monitor",
            "外接显示器配置失败，已恢复原布局",
        ));
    }
    for out in &externals[1..] {
        let mon = monitors
            .iter()
            .find(|m| name(m) == out)
            .cloned()
            .unwrap_or(Value::Null);
        let (mode, transform) = portrait_mode(&mon);
        if hypr
            .eval(&monitor_expr(
                out,
                mode,
                "auto-right",
                transform,
                Some(&source),
            ))
            .await
            .is_err()
        {
            rollback(j, hypr, inh, created_ref).await;
            notify("外接显示器配置失败，已恢复原布局").await;
            return Err(ServiceError::new(
                "monitor",
                "外接显示器配置失败，已恢复原布局",
            ));
        }
    }
    if hypr
        .eval(&monitor_expr(
            &headless,
            source_mode,
            "auto-right",
            source_transform,
            Some(&source),
        ))
        .await
        .is_err()
    {
        rollback(j, hypr, inh, created_ref).await;
        notify("Headless 复制配置失败，已恢复原布局").await;
        return Err(ServiceError::new(
            "mirror",
            "Headless 复制配置失败，已恢复原布局",
        ));
    }
    let devices: Value = hypr.json("devices").await.unwrap_or(Value::Null);
    for touch in devices["touch"].as_array().into_iter().flatten() {
        let Some(n) = touch["name"].as_str().filter(|n| !n.is_empty()) else {
            continue;
        };
        let ok = safe(n).is_ok()
            && hypr
                .eval(&format!("hl.device({{ name = '{n}', transform = 0 }})"))
                .await
                .is_ok();
        if !ok {
            rollback(j, hypr, inh, created_ref).await;
            notify("触控方向配置失败，已恢复原布局").await;
            return Err(ServiceError::new("touch", "触控方向配置失败，已恢复原布局"));
        }
    }
    // Bounded action-time settle before verification (source: 0.3 s).
    tokio::time::sleep(Duration::from_millis(300)).await;
    let live: Vec<Value> = hypr.json("monitors").await.unwrap_or_default();
    if let Err(msg) = verify(&live, &source, &headless, &externals[1..]) {
        rollback(j, hypr, inh, created_ref).await;
        let note = if msg.starts_with("external") {
            "Demo mode 逐屏验证失败，已恢复原布局"
        } else {
            "Demo mode 验证失败，已恢复原布局"
        };
        notify(note).await;
        return Err(ServiceError::new("verify", msg));
    }
    notify("已启用：防睡眠、1080×1920@60、外接复制、Headless 复制、横向触控").await;
    Ok(status_text(j, session))
}

async fn disable_demo(
    j: &Journal,
    hypr: &Hypr,
    inh: &dyn SleepInhibitor,
    session: &str,
) -> ActionResult {
    j.cleanup_stale(session);
    if !j.belongs(session) {
        return Ok(status_text(j, session));
    }
    let created = j.read("created-headless");
    let was_enabled = {
        let v = j.read("prevent-sleep-was-enabled");
        if v.is_empty() {
            "false".to_string()
        } else {
            v
        }
    };
    hypr.plain("reload").await?;
    if !created.is_empty() {
        let _ = hypr.plain(&format!("output remove {created}")).await;
    }
    if was_enabled != "true" {
        inh.disable().await;
    }
    j.clear();
    notify("已退出并恢复原显示器、触控与睡眠设置").await;
    Ok("enabled=false\nsource=\nheadless=\n".into())
}

pub fn journal_dir(state_home: &Path) -> PathBuf {
    state_home.join("hyprv/quickshell/demo-mode")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn monitor_selection() {
        let mons = vec![
            json!({"name":"eDP-1","x":0,"y":0,"width":2880,"height":1800,"scale":2.0,"transform":0}),
            json!({"name":"DP-4","availableModes":["3840x2160@60.00Hz","1080x1920@59.95Hz"]}),
            json!({"name":"HDMI-A-1","disabled":true}),
            json!({"name":"HEADLESS-2"}),
        ];
        assert_eq!(external_outputs(&mons), ["DP-4"]);
        assert_eq!(headless_output(&mons).as_deref(), Some("HEADLESS-2"));
        assert_eq!(portrait_mode(&mons[1]), ("1080x1920@60", 0));
        assert_eq!(portrait_mode(&json!({})), ("1920x1080@60", 1));
        assert_eq!(external_position(&mons), "1440x0");
        assert_eq!(external_position(&mons[1..]), "auto-right");
        assert_eq!(
            monitor_expr("H", "1920x1080@60", "auto-right", 1, Some("DP-4")),
            "hl.monitor({ output = 'H', mode = '1920x1080@60', position = 'auto-right', scale = '1', transform = 1, mirror = 'DP-4' })"
        );
    }
    #[test]
    fn verification() {
        let live = vec![
            json!({"name":"DP-4","width":1920,"height":1080,"transform":1}),
            json!({"name":"HEADLESS-1","mirrorOf":"DP-4"}),
        ];
        assert!(verify(&live, "DP-4", "HEADLESS-1", &[]).is_ok());
        assert!(verify(&live, "DP-4", "HEADLESS-1", &["X".into()]).is_err());
    }
    #[test]
    fn journal_scoping() {
        let dir = std::env::temp_dir().join(format!("hyprv-demo-{}", std::process::id()));
        let j = Journal { dir: dir.clone() };
        j.write("session", "A").unwrap();
        j.write("source", "DP-4").unwrap();
        std::fs::write(j.file("active"), "").unwrap();
        assert!(j.belongs("A"));
        assert_eq!(
            status_text(&j, "A"),
            "enabled=true\nsource=DP-4\nheadless=\n"
        );
        j.cleanup_stale("B");
        assert!(!dir.exists());
    }
}
