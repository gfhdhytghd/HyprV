//! Agent island: read-only observation of Claude Code / Codex sessions.
//!
//! Event sources: inotify on the hook state directory (hooks write `state.json`),
//! pidfds for every PID referenced by a session, one next-expiry deadline, and
//! process discovery that only runs while a scan lease is held. Discovery is the
//! legacy scan (every 1.5 s, [`model::ScanClock`]) over an incremental `/proc`
//! table ([`model::ProcTable`]: revalidated identities, pidfds for agent PIDs, a full
//! sweep every 60 s). The scan's effect (discovered sessions stamped with the
//! scan time) is recomputed and published only when the legacy view would
//! observably change; the published order is legacy's ([`model::LegacyOrder`]).
//! The single UI action is `FocusSession(id)`; there is deliberately no
//! approve/answer/type.
pub mod appserver;
pub mod focus;
pub mod incremental;
pub mod install;
pub mod model;
pub mod ojson;
mod probe_worker;
pub mod store;

use crate::{
    hypr::Hypr, topic_key, util, ActionResult, Ctx, Envelope, Handle, Lease, ServiceError,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, time::Duration};
use tokio::sync::mpsc;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: String,
    pub source: String,
    pub source_label: String,
    pub title: String,
    pub project: String,
    /// idle | processing | running | waitingApproval | waitingQuestion | completed
    pub status: String,
    pub current_tool: String,
    pub detail: String,
    /// Persisted `progress`; use [`AgentSession::progress_at`] for the ring.
    pub base_progress: f64,
    pub activity_epoch_s: f64,
    /// Process-discovered running row: `progress_at` is `base_progress`.
    #[serde(default)]
    pub pinned: bool,
}
impl AgentSession {
    /// The QML `progress` role: 12 s synthetic phase for running/processing.
    /// The UI samples this only while the ring animates.
    pub fn progress_at(&self, now_epoch_s: f64) -> f64 {
        if self.pinned {
            return self.base_progress;
        }
        model::display_progress(
            &self.status,
            self.base_progress,
            self.activity_epoch_s,
            now_epoch_s,
        )
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AgentIslandState {
    pub active: bool,
    pub count: u32,
    pub sessions: Vec<AgentSession>,
    pub pending_count: u32,
}
topic_key!(AgentIslandTopic, AgentIslandState);

#[derive(Clone, Debug)]
pub enum AgentAction {
    FocusSession(String),
}

#[derive(Clone)]
pub struct AgentIslandHandle {
    pub handle: Handle<AgentIslandState, AgentAction>,
    /// Hold `scan_lease.acquire()` to discover unhooked agents (1.5 s scan).
    pub scan_lease: Lease,
}
impl std::ops::Deref for AgentIslandHandle {
    type Target = Handle<AgentIslandState, AgentAction>;
    fn deref(&self) -> &Self::Target {
        &self.handle
    }
}

/// Read-only observer (default): never writes the hook directory.
pub fn spawn(ctx: &Ctx) -> AgentIslandHandle {
    spawn_with(ctx, false)
}
/// `write_back = true` reproduces the legacy observer, which persisted
/// process-scan sessions and cleanup into `state.json` under `state.lock`.
pub fn spawn_with(ctx: &Ctx, write_back: bool) -> AgentIslandHandle {
    let topic = crate::topic::<AgentIslandTopic>(ctx, AgentIslandState::default());
    let (handle, rx) = Handle::new(topic.clone());
    let scan_lease = Lease::new();
    let actor = Actor {
        dir: store::state_dir(&ctx.env),
        topic,
        hypr: Hypr::new(&ctx.env),
        proc: util::Proc::default(),
        lease: scan_lease.clone(),
        write_back,
    };
    // Codex app-server notifications (attach-only; see appserver.rs).
    let (app_tx, app_rx) = mpsc::unbounded_channel();
    ctx.rt.spawn(appserver::run(
        appserver::endpoint(&ctx.env),
        app_tx,
        ctx.shutdown.clone(),
    ));
    ctx.rt.spawn(actor.run(rx, app_rx, ctx.shutdown.clone()));
    AgentIslandHandle { handle, scan_lease }
}

struct Actor {
    dir: PathBuf,
    topic: hyprv_core::store::Topic<AgentIslandState>,
    hypr: Hypr,
    proc: util::Proc,
    lease: Lease,
    write_back: bool,
}
struct Refresh {
    rows: Vec<model::PublicSession>,
    pending: usize,
    expiry: Option<f64>,
    pids: Vec<i64>,
    /// Surviving sessions: file ones in file order, then overlay-only ones.
    file_live: Vec<String>,
    overlay_live: Vec<String>,
    /// See [`model::ScanClock`]; `None` when no scan was applied.
    resync_after: Option<f64>,
}
impl Refresh {
    /// Rows in legacy order, as published.
    fn state(mut self, order: &mut model::LegacyOrder) -> AgentIslandState {
        order.update(&self.file_live, &self.overlay_live);
        model::sort_rows_by(&mut self.rows, &|id| order.rank(id));
        to_state(self.rows, self.pending)
    }
}
/// Profiling hook: one read-only refresh of `dir` (no scan, no messages).
#[doc(hidden)]
pub fn profile_refresh(dir: &std::path::Path) -> std::io::Result<usize> {
    let mut overlay = Synthetic::new();
    refresh(
        dir,
        std::path::Path::new("/proc"),
        None,
        &mut overlay,
        false,
        &[],
        util::epoch_f64(),
    )
    .map(|r| r.rows.len())
}
/// Overlay kept by a read-only actor: process-scan sessions live in memory.
type Synthetic = serde_json::Map<String, serde_json::Value>;
fn refresh(
    dir: &std::path::Path,
    proc_root: &std::path::Path,
    scan: Option<(&model::ProcTable, f64)>,
    synthetic: &mut Synthetic,
    write_back: bool,
    app_messages: &[serde_json::Value],
    now: f64,
) -> std::io::Result<Refresh> {
    let proc = &util::Proc {
        root: proc_root.to_path_buf(),
    };
    let compute = |state: &mut serde_json::Value, synthetic: &mut Synthetic| {
        let before = if write_back {
            model::content_key(state)
        } else {
            String::new()
        };
        if !write_back {
            if let Some(sessions) = state["sessions"].as_object_mut() {
                for (k, v) in synthetic.iter() {
                    sessions.entry(k.clone()).or_insert_with(|| v.clone());
                }
            }
        }
        for message in app_messages {
            model::apply_app_message(state, message, now);
        }
        if let Some((table, at)) = scan {
            model::sync_process_sessions(state, &table.procs, &|pid| proc.cwd(pid as i32), at);
        }
        let mut alive = |pid: i64| proc.exists(pid as i32);
        model::cleanup_state(state, now, &mut alive);
        let (rows, pending) = model::public_sessions(state, now);
        let expiry = model::next_expiry(state, &mut alive);
        let pids = model::referenced_pids(state).into_iter().collect();
        if !write_back {
            *synthetic = state["sessions"]
                .as_object()
                .map(|s| {
                    s.iter()
                        .filter(|(_, v)| {
                            v["provider"] == "process-scan" || v["provider"] == "app-server"
                        })
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect()
                })
                .unwrap_or_default();
        }
        let changed = write_back && model::content_key(state) != before;
        let resync_after = scan.and_then(|(_, at)| model::resync_after(state, at));
        let live: Vec<String> = state["sessions"]
            .as_object()
            .map(|s| s.keys().cloned().collect())
            .unwrap_or_default();
        (
            changed,
            Refresh {
                rows,
                pending,
                expiry,
                pids,
                file_live: Vec::new(),
                overlay_live: live,
                resync_after,
            },
        )
    };
    let path = dir.join("state.json");
    let mut r = if write_back {
        store::with_locked(dir, now, |state| compute(state, synthetic))?
    } else {
        // Read-only: hooks replace state.json atomically, so no lock is needed
        // and the actor never touches the hook directory.
        let mut state = store::read_state(dir, now);
        compute(&mut state, synthetic).1
    };
    // Split the surviving ids into file order and overlay-only ones.
    let keys = model::session_keys(&std::fs::read(path).unwrap_or_default());
    let live: std::collections::HashSet<String> = r.overlay_live.drain(..).collect();
    r.file_live = keys.into_iter().filter(|k| live.contains(k)).collect();
    let in_file: std::collections::HashSet<&String> = r.file_live.iter().collect();
    let mut rest: Vec<String> = live
        .iter()
        .filter(|k| !in_file.contains(k))
        .cloned()
        .collect();
    rest.sort();
    r.overlay_live = rest;
    Ok(r)
}
/// At most one refresh + publication per frame-equivalent.
const COALESCE: Duration = Duration::from_millis(16);
/// Equal except for the scan time re-stamped on pinned rows. Nothing shows it
/// (their progress is `base_progress`; the order is already applied), and
/// legacy's UI payload never carried `last_activity`, so it is not published
/// on its own.
fn only_scan_time_changed(old: &AgentIslandState, new: &AgentIslandState) -> bool {
    old.active == new.active
        && old.count == new.count
        && old.pending_count == new.pending_count
        && old.sessions.len() == new.sessions.len()
        && old.sessions.iter().zip(&new.sessions).all(|(a, b)| {
            a == b
                || (a.pinned
                    && b.pinned
                    && AgentSession {
                        activity_epoch_s: b.activity_epoch_s,
                        ..a.clone()
                    } == *b)
        })
}
fn to_state(rows: Vec<model::PublicSession>, pending: usize) -> AgentIslandState {
    let sessions: Vec<AgentSession> = rows
        .into_iter()
        .map(|r| AgentSession {
            id: r.id,
            source: r.source,
            source_label: r.source_label,
            title: r.title,
            project: r.project,
            status: r.status,
            current_tool: r.current_tool,
            detail: r.detail,
            base_progress: r.progress,
            activity_epoch_s: r.last_activity,
            pinned: r.pinned,
        })
        .collect();
    AgentIslandState {
        active: !sessions.is_empty(),
        count: sessions.len() as u32,
        sessions,
        pending_count: pending as u32,
    }
}
impl Actor {
    /// Read-only refresh on the reactor: one file read, borrowed top-level
    /// parse, re-parse of changed sessions only. No lock, no directory scan.
    fn refresh_fast(
        &self,
        engine: &mut incremental::Incremental,
        overlay: &mut Synthetic,
        known_alive: &HashMap<i64, tokio::task::JoinHandle<()>>,
    ) -> Option<Refresh> {
        let now = util::epoch_f64();
        let bytes = std::fs::read(self.dir.join("state.json")).unwrap_or_default();
        let proc = &self.proc;
        let mut alive = |pid: i64| known_alive.contains_key(&pid) || proc.exists(pid as i32);
        let view = engine.refresh(&bytes, overlay, now, &mut alive)?;
        Some(Refresh {
            rows: view.rows,
            pending: view.pending,
            expiry: view.expiry,
            pids: view.pids,
            file_live: view.file_live,
            overlay_live: view.overlay_live,
            resync_after: None,
        })
    }
    async fn refresh_full(
        &self,
        scan: Option<(&mut model::ProcTable, f64)>,
        synthetic: &mut Synthetic,
        messages: Vec<serde_json::Value>,
    ) -> std::io::Result<Refresh> {
        // flock + full normalization run on the blocking pool.
        let (dir, root, write_back) = (self.dir.clone(), self.proc.root.clone(), self.write_back);
        let mut overlay = std::mem::take(synthetic);
        let (mut slot, at) = match scan {
            Some((t, at)) => (Some(t), at),
            None => (None, 0.0),
        };
        let table = slot.as_deref_mut().map(std::mem::take);
        let (r, overlay, table) = tokio::task::spawn_blocking(move || {
            let r = refresh(
                &dir,
                &root,
                table.as_ref().map(|t| (t, at)),
                &mut overlay,
                write_back,
                &messages,
                util::epoch_f64(),
            );
            (r, overlay, table)
        })
        .await
        .map_err(std::io::Error::other)?;
        if let (Some(slot), Some(table)) = (slot, table) {
            *slot = table;
        }
        *synthetic = overlay;
        r
    }
    /// Discovery revalidates cached /proc identities as well as new IDs.
    /// Keep all filesystem reads on a sleeping worker, including range probes.
    async fn probe(
        &self,
        table: &mut model::ProcTable,
        worker: &mut Option<probe_worker::ProbeWorker>,
    ) -> bool {
        let now = util::epoch_f64();
        if worker.is_none() {
            *worker = probe_worker::ProbeWorker::new(&self.proc.root).ok();
        }
        if let Some(worker) = worker {
            return worker.probe(table, now).await;
        }
        // Resource-exhaustion fallback retains discovery correctness.
        let root = self.proc.root.clone();
        let mut moved = std::mem::take(table);
        match tokio::task::spawn_blocking(move || {
            let changed = moved.refresh(&util::Proc { root }, now);
            (moved, changed)
        })
        .await
        {
            Ok((t, changed)) => {
                *table = t;
                changed
            }
            Err(_) => true,
        }
    }
    async fn run(
        self,
        mut rx: mpsc::Receiver<Envelope<AgentAction>>,
        mut app_rx: mpsc::UnboundedReceiver<serde_json::Value>,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) {
        if self.write_back {
            let _ = store::ensure_dirs(&self.dir);
        }
        let mut synthetic = Synthetic::new();
        let mut engine = incremental::Incremental::default();
        let mut app_pending: Vec<serde_json::Value> = Vec::new();
        let (exit_tx, mut exits) = mpsc::unbounded_channel::<i64>();
        let mut watchers: HashMap<i64, tokio::task::JoinHandle<()>> = HashMap::new();
        let mut demand = self.lease.demand();
        let mut table = model::ProcTable::default();
        let mut probe_worker = None;
        let mut clock = model::ScanClock::default();
        let mut order = model::LegacyOrder::default();
        let mut probe_due = self.lease.held();
        let mut refresh_due = true;
        // Lease released: apply the last scan time once, so discovered rows
        // then expire 900 s after it, as legacy's did when it stopped scanning.
        let mut release_scan = false;
        // Lease (re)acquired: its first scan re-creates rows that expired
        // while nothing scanned, even though the discovered set is unchanged.
        let mut resume_scan = true;
        let mut last_scan = tokio::time::Instant::now();
        // One long-lived watch (re-arms itself after atomic replaces).
        let watch = util::Watch::new(&[(self.dir.clone(), &["state.json"][..])]).ok();
        let mut flush_at: Option<tokio::time::Instant> = Some(tokio::time::Instant::now());
        let mut expiry: Option<f64> = None;
        loop {
            let now = tokio::time::Instant::now();
            if flush_at.is_some_and(|t| t <= now) {
                flush_at = None;
                let held = self.lease.held();
                let mut discovered = false;
                let mut refresh = std::mem::take(&mut refresh_due);
                if probe_due && held {
                    // A legacy scan: probe, advance the scan clock. Unless the
                    // legacy view would observably change, nothing is redone.
                    last_scan = now;
                    discovered = self.probe(&mut table, &mut probe_worker).await;
                    refresh |= clock.scanned(util::epoch_f64(), discovered);
                    refresh |= std::mem::take(&mut resume_scan);
                }
                probe_due = false;
                let release = std::mem::take(&mut release_scan);
                // While agents are discovered every refresh re-applies the last
                // scan (the read-only actor does not persist it); otherwise
                // only the scan that saw them go needs to.
                let scan_at = clock
                    .at
                    .filter(|_| (held || release) && (discovered || !table.agents().is_empty()));
                let messages = std::mem::take(&mut app_pending);
                let result = if !refresh {
                    None
                } else if self.write_back || scan_at.is_some() {
                    let scan = scan_at.map(|at| (&mut table, at));
                    self.refresh_full(scan, &mut synthetic, messages).await.ok()
                } else {
                    if !messages.is_empty() {
                        // App-server sessions live only in the overlay here.
                        let mut wrapper =
                            serde_json::json!({"sessions": std::mem::take(&mut synthetic)});
                        let t = util::epoch_f64();
                        for m in &messages {
                            model::apply_app_message(&mut wrapper, m, t);
                        }
                        if let serde_json::Value::Object(mut w) = wrapper {
                            if let Some(serde_json::Value::Object(s)) = w.remove("sessions") {
                                synthetic = s;
                            }
                        }
                    }
                    match self.refresh_fast(&mut engine, &mut synthetic, &watchers) {
                        Some(r) => Some(r),
                        None => self
                            .refresh_full(None, &mut synthetic, Vec::new())
                            .await
                            .ok(),
                    }
                };
                if let Some(mut r) = result {
                    clock.refreshed(r.resync_after);
                    expiry = r.expiry;
                    let pids = std::mem::take(&mut r.pids);
                    let next = r.state(&mut order);
                    if !only_scan_time_changed(&self.topic.snapshot().value, &next) {
                        self.topic.publish(next);
                    }
                    // pidfds: observe exits of referenced processes only.
                    watchers.retain(|pid, task| {
                        let keep = pids.contains(pid);
                        if !keep {
                            task.abort();
                        }
                        keep
                    });
                    for pid in pids {
                        if watchers.contains_key(&pid) {
                            continue;
                        }
                        if let Ok(fd) = util::Pidfd::open(pid as i32) {
                            let tx = exit_tx.clone();
                            watchers.insert(
                                pid,
                                tokio::spawn(async move {
                                    fd.exited().await;
                                    let _ = tx.send(pid);
                                }),
                            );
                        }
                    }
                }
            }
            // `dirty` arms a flush; `refresh_due` / `probe_due` say what it does.
            let mut dirty = false;
            let flush = async {
                match flush_at {
                    Some(t) => tokio::time::sleep_until(t).await,
                    None => std::future::pending().await,
                }
            };
            let expiry_sleep = async {
                match expiry {
                    Some(at) if flush_at.is_none() => {
                        let wait = (at - util::epoch_f64()).max(0.0) + 0.05;
                        tokio::time::sleep(Duration::from_secs_f64(wait)).await
                    }
                    _ => std::future::pending().await,
                }
            };
            // The legacy scan cadence: an incremental probe every 1.5 s.
            let probe_tick = async {
                if self.lease.held() && !probe_due {
                    tokio::time::sleep_until(
                        last_scan + Duration::from_secs_f64(model::PROCESS_SCAN_INTERVAL_SECONDS),
                    )
                    .await
                } else {
                    std::future::pending().await
                }
            };
            let file_event = async {
                match &watch {
                    Some(w) => {
                        let _ = w.changed().await;
                    }
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                _ = flush => {}
                _ = file_event => { refresh_due = true; dirty = true; }
                _ = expiry_sleep => { refresh_due = true; dirty = true; }
                _ = probe_tick => { probe_due = true; dirty = true; }
                r = demand.changed() => {
                    if r.is_err() { return; }
                    if *demand.borrow_and_update() > 0 {
                        // Resume scanning at the legacy rate.
                        resume_scan = true;
                        probe_due = clock.due(util::epoch_f64());
                        dirty = probe_due;
                    } else {
                        release_scan = clock.at.is_some();
                        refresh_due |= release_scan;
                        dirty = release_scan;
                    }
                }
                Some(pid) = exits.recv() => {
                    watchers.remove(&pid);
                    refresh_due = true;
                    dirty = true;
                }
                Some(message) = app_rx.recv() => {
                    app_pending.push(message);
                    while let Ok(m) = app_rx.try_recv() {
                        app_pending.push(m);
                    }
                    refresh_due = true;
                    dirty = true;
                }
                msg = rx.recv() => match msg {
                    Some(env) => self.handle(env, synthetic.clone()),
                    None => return,
                },
                _ = shutdown.changed() => {
                    if *shutdown.borrow() { break; }
                }
            }
            // Coalesce: the first event of a burst arms one flush 16 ms later;
            // further events inside the window ride along.
            if dirty && flush_at.is_none() {
                flush_at = Some(tokio::time::Instant::now() + COALESCE);
            }
        }
        for (_, task) in watchers {
            task.abort();
        }
    }
    fn handle(&self, env: Envelope<AgentAction>, overlay: Synthetic) {
        let AgentAction::FocusSession(id) = env.action.clone();
        let (dir, hypr, write_back) = (self.dir.clone(), self.hypr.clone(), self.write_back);
        // Focus runs detached so a slow compositor never blocks observation.
        tokio::spawn(async move {
            let result = focus(dir, hypr, id, overlay, write_back).await;
            env.respond(result);
        });
    }
}
async fn focus(
    dir: PathBuf,
    hypr: Hypr,
    id: String,
    overlay: Synthetic,
    write_back: bool,
) -> ActionResult {
    // Copy the actor-private record (file, then in-memory overlay), then focus.
    let session =
        tokio::task::spawn_blocking(move || -> std::io::Result<Option<serde_json::Value>> {
            let now = util::epoch_f64();
            let from_file = if write_back {
                store::with_locked(&dir, now, |state| {
                    (false, state["sessions"].get(&id).cloned())
                })?
            } else {
                store::read_state(&dir, now)["sessions"].get(&id).cloned()
            };
            Ok(from_file.or_else(|| overlay.get(&id).cloned()))
        })
        .await
        .map_err(ServiceError::io)??;
    match session {
        Some(s) if s.is_object() => focus::focus_session(&hypr, &s).await.map(|_| String::new()),
        _ => Err(ServiceError::new("not_found", "unknown agent session")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fake_proc(root: &std::path::Path, pid: i32, comm: &str, ppid: i32, argv: &[&str]) {
        let d = root.join(pid.to_string());
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join("stat"),
            format!("{pid} ({comm}) S {ppid} 1 1 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 77 0 0"),
        )
        .unwrap();
        std::fs::write(d.join("cmdline"), argv.join("\0") + "\0").unwrap();
    }
    /// Replays a recorded sequence (tests/fixtures/agent_parity) through the
    /// actor's decisions and compares every tick's published rows with the
    /// legacy observer's. If the legacy module is present, the recording is
    /// first re-derived from it (tests/agent_parity_legacy.py).
    fn legacy_parity(name: &str) -> (usize, usize) {
        use serde_json::Value;
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
        let fixture = base.join(format!("fixtures/agent_parity/{name}.json"));
        let recorded: Value = serde_json::from_str(
            &std::fs::read_to_string(
                base.join(format!("fixtures/agent_parity/{name}.legacy.json")),
            )
            .unwrap(),
        )
        .unwrap();
        let legacy = base.join("../../../../quickshell/scripts/agent_island.py");
        if legacy.exists() {
            let out = std::process::Command::new("python3")
                .args(["-I", "-B"])
                .arg(base.join("agent_parity_legacy.py"))
                .arg(&fixture)
                .arg(&legacy)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let live: Value = serde_json::from_slice(&out.stdout).unwrap();
            assert_eq!(
                live, recorded,
                "recording is stale against the legacy module"
            );
        } else {
            eprintln!("legacy module absent; comparing against the recording only");
        }
        let scenario: Value =
            serde_json::from_str(&std::fs::read_to_string(&fixture).unwrap()).unwrap();
        let root = std::env::temp_dir().join(format!("hyprv-parity-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (dir, proc) = (root.join("agent"), root.join("proc"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(&proc).unwrap();
        let procfs = util::Proc { root: proc.clone() };
        let (mut table, mut clock, mut order) = (
            model::ProcTable::default(),
            model::ScanClock::default(),
            model::LegacyOrder::default(),
        );
        let (mut overlay, mut engine) = (Synthetic::new(), incremental::Incremental::default());
        let (mut lease, mut published) = (false, AgentIslandState::default());
        let mut resume = false;
        let (mut scans, mut refreshes) = (0, 0);
        let (mut expiry, mut pids): (Option<f64>, Vec<i64>) = (None, Vec::new());
        let mut file = String::new();
        for (input, want) in scenario["ticks"]
            .as_array()
            .unwrap()
            .iter()
            .zip(recorded["ticks"].as_array().unwrap())
        {
            let t = input["t"].as_f64().unwrap();
            if let Some(list) = input["procs"].as_array() {
                std::fs::remove_dir_all(&proc).unwrap();
                for p in list {
                    let pid = p["pid"].as_i64().unwrap();
                    let argv: Vec<&str> = p["argv"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|a| a.as_str().unwrap())
                        .collect();
                    fake_proc(
                        &proc,
                        pid as i32,
                        p["comm"].as_str().unwrap(),
                        p["ppid"].as_i64().unwrap() as i32,
                        &argv,
                    );
                    let start = p["start"].as_u64().unwrap();
                    std::fs::write(
                        proc.join(format!("{pid}/stat")),
                        format!(
                            "{pid} ({}) S {} 1 1 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 {start} 0 0",
                            p["comm"].as_str().unwrap(),
                            p["ppid"]
                        ),
                    )
                    .unwrap();
                    if let Some(cwd) = p["cwd"].as_str() {
                        std::os::unix::fs::symlink(cwd, proc.join(format!("{pid}/cwd"))).unwrap();
                    }
                }
            }
            // The actor's triggers at this instant (see `Actor::run`).
            let mut refresh = false;
            let text = want["file"].as_str().unwrap();
            if text != file {
                file = text.to_string();
                std::fs::write(dir.join("state.json"), &file).unwrap();
                refresh = true; // inotify
            }
            refresh |= pids.iter().any(|p| !procfs.exists(*p as i32)); // pidfd
            refresh |= expiry.is_some_and(|e| t >= e + 0.05); // expiry deadline
            let held = input["lease"].as_bool().unwrap_or(lease);
            let release = lease && !held && clock.at.is_some();
            refresh |= release;
            resume |= held && !lease;
            lease = held;
            let mut discovered = false;
            if held && clock.due(t) {
                discovered = table.refresh(&procfs, t);
                refresh |= clock.scanned(t, discovered);
                refresh |= std::mem::take(&mut resume);
                scans += 1;
            }
            let scan_at = clock
                .at
                .filter(|_| (held || release) && (discovered || !table.agents().is_empty()));
            if refresh {
                refreshes += 1;
                let r = match scan_at {
                    Some(at) => refresh_at(&dir, &proc, Some((&table, at)), &mut overlay, t),
                    None => {
                        let bytes = std::fs::read(dir.join("state.json")).unwrap();
                        let mut alive = |p: i64| procfs.exists(p as i32);
                        let v = engine.refresh(&bytes, &mut overlay, t, &mut alive).unwrap();
                        Refresh {
                            rows: v.rows,
                            pending: v.pending,
                            expiry: v.expiry,
                            pids: v.pids,
                            file_live: v.file_live,
                            overlay_live: v.overlay_live,
                            resync_after: None,
                        }
                    }
                };
                clock.refreshed(r.resync_after);
                expiry = r.expiry;
                pids = r.pids.clone();
                published = r.state(&mut order);
            }
            let got: Vec<Value> = published
                .sessions
                .iter()
                .map(|s| serde_json::json!([s.id, s.status, s.detail]))
                .collect();
            assert_eq!(
                Value::Array(got),
                want["legacy"],
                "{name}: published rows differ from legacy at t={t}"
            );
        }
        std::fs::remove_dir_all(root).unwrap();
        (scans, refreshes)
    }
    fn refresh_at(
        dir: &std::path::Path,
        proc: &std::path::Path,
        scan: Option<(&model::ProcTable, f64)>,
        overlay: &mut Synthetic,
        now: f64,
    ) -> Refresh {
        refresh(dir, proc, scan, overlay, false, &[], now).unwrap()
    }
    #[test]
    fn legacy_parity_ordering() {
        legacy_parity("ordering");
    }
    #[test]
    fn legacy_parity_expiry_with_and_without_lease() {
        let (scans, refreshes) = legacy_parity("expiry");
        // The CPU property: unchanged scans recompute nothing. ~640 scans
        // over the held stretch; refreshes are the start, the hook row's
        // expiry, the release and the resume.
        assert!(scans > 600, "{scans}");
        assert!(refreshes <= 8, "{refreshes} refreshes for {scans} scans");
    }
    #[test]
    fn scan_time_alone_is_not_published() {
        let row = |id: &str, pinned: bool, t: f64| AgentSession {
            id: id.into(),
            source: "claude".into(),
            source_label: "Claude".into(),
            title: String::new(),
            project: String::new(),
            status: "running".into(),
            current_tool: String::new(),
            detail: String::new(),
            base_progress: 0.38,
            activity_epoch_s: t,
            pinned,
        };
        let state = |rows: Vec<AgentSession>| AgentIslandState {
            active: true,
            count: rows.len() as u32,
            sessions: rows,
            pending_count: 0,
        };
        let a = state(vec![row("p", true, 10.0), row("h", false, 5.0)]);
        assert!(only_scan_time_changed(
            &a,
            &state(vec![row("p", true, 11.5), row("h", false, 5.0)])
        ));
        // An unpinned row's time drives its progress ring: published.
        assert!(!only_scan_time_changed(
            &a,
            &state(vec![row("p", true, 10.0), row("h", false, 6.0)])
        ));
        // A reorder is published.
        assert!(!only_scan_time_changed(
            &a,
            &state(vec![row("h", false, 5.0), row("p", true, 11.5)])
        ));
    }
    #[test]
    fn read_only_overlay_keeps_synthetic_sessions_without_writing() {
        let root = std::env::temp_dir().join(format!("hyprv-agent-ro-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (dir, proc) = (root.join("agent"), root.join("proc"));
        fake_proc(&proc, 10, "codex", 1, &["/usr/bin/codex"]);
        fake_proc(
            &proc,
            11,
            "systemd-inhibit",
            10,
            &[
                "systemd-inhibit",
                "--who",
                "codex",
                "--why",
                "Codex is running an active turn",
            ],
        );
        let mut overlay = Synthetic::new();
        let mut table = model::ProcTable::default();
        let procfs = util::Proc { root: proc.clone() };
        assert!(table.refresh(&procfs, 100.0));
        let mut order = model::LegacyOrder::default();
        let r = refresh(
            &dir,
            &proc,
            Some((&table, 100.0)),
            &mut overlay,
            false,
            &[],
            100.0,
        )
        .unwrap()
        .state(&mut order);
        assert_eq!(r.count, 1);
        assert_eq!(r.sessions[0].status, "running");
        // Legacy parity: a discovered running row sits at its floor.
        let row = &r.sessions[0];
        assert!(row.pinned);
        assert_eq!(row.progress_at(row.activity_epoch_s + 7.0), 0.46);
        assert!(overlay.contains_key("codexproc-10-77"));
        std::fs::remove_dir_all(proc.join("11")).unwrap();
        assert!(
            table.refresh(&procfs, 101.5),
            "inhibitor exit changes discovery"
        );
        let r = refresh(
            &dir,
            &proc,
            Some((&table, 101.5)),
            &mut overlay,
            false,
            &[],
            101.5,
        )
        .unwrap()
        .state(&mut order);
        assert_eq!(r.sessions[0].detail, "Done");
        // Between scans (file event only) the overlay still shows the session.
        let r = refresh(&dir, &proc, None, &mut overlay, false, &[], 102.0)
            .unwrap()
            .state(&mut order);
        assert_eq!(r.sessions[0].status, "completed");
        assert!(
            !dir.exists(),
            "read-only actor must not create the hook directory"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
