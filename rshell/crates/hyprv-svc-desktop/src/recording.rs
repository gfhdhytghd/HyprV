//! Screen recording status and toggle (`ControlPanelPopup.qml:223`,
//! `control-panel-status.sh` recording detection).
//!
//! Start activates the existing recorder script as a transient user unit
//! (systemd executes it; the shell never forks). Stop sends SIGINT through
//! pidfds in the source's `wf-recorder || wl-screenrec || gpu-screen-recorder`
//! order. Known recorders are tracked by pidfd (exit is immediate); foreign
//! recorders started elsewhere are found by a bounded /proc scan that only runs
//! while `scan_lease` is held (panel visible, 1.5 s) — an unavoidable fallback,
//! as no universal process-start signal exists for unprivileged users.
//!
//! The laptop profile uses the laptop script's `pgrep -x` semantics instead
//! ([`Match::Comm`]) and never polls: recorder binaries on `PATH` are watched for
//! exec with fanotify, their directories with inotify (installs and upgrades
//! re-mark them), exits with pidfds, and the panel lease takes one snapshot.
use crate::{systemd, topic_key, util, ActionResult, Ctx, Handle, Lease, ServiceError};
use hyprv_core::profile::Profile;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    time::Duration,
};
use tokio::{io::unix::AsyncFd, sync::mpsc, time::Instant};

pub const RECORDERS: &[&str] = &["wf-recorder", "wl-screenrec", "gpu-screen-recorder"];
const SCAN_PERIOD: Duration = Duration::from_millis(1500);
/// `FAN_OPEN_EXEC` arrives when the binary is opened, before the kernel sets the
/// new comm, so an exec event is answered by a scan now plus these bounded
/// follow-ups (one-shot timers per event, not a period).
const EXEC_FOLLOWUPS: [Duration; 3] = [
    Duration::from_millis(50),
    Duration::from_millis(250),
    Duration::from_secs(1),
];
const DEFAULT_PATH: &str = "/usr/local/bin:/usr/bin";

/// Recorder detection semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Match {
    /// Desktop (`control-panel-status.sh` on main): `pgrep -f` on the command line.
    Cmdline,
    /// Laptop: `pgrep -x`, the exact 15-byte kernel comm. A comm truncated from a
    /// longer name (`gpu-screen-recorder` → `gpu-screen-reco`) never matches, and
    /// argv is never consulted.
    Comm,
}
impl Match {
    pub fn for_profile(profile: Profile) -> Self {
        match profile {
            Profile::Desktop => Self::Cmdline,
            Profile::Laptop => Self::Comm,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recorder {
    pub pid: u32,
    pub name: String,
    /// Started by this shell (inside one of our transient units).
    pub managed: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordingState {
    /// QML `screenRecording` / status `recording=`.
    pub active: bool,
    pub recorders: Vec<Recorder>,
    pub busy: bool,
    pub last_error: Option<String>,
}
topic_key!(RecordingTopic, RecordingState);

#[derive(Clone, Copy, Debug)]
pub enum RecordingAction {
    /// QML `toggleRecording`: start when not recording, else stop.
    Toggle,
    Start,
    Stop,
}
#[derive(Clone)]
pub struct RecordingHandle {
    pub handle: Handle<RecordingState, RecordingAction>,
    pub scan_lease: Lease,
}
impl std::ops::Deref for RecordingHandle {
    type Target = Handle<RecordingState, RecordingAction>;
    fn deref(&self) -> &Self::Target {
        &self.handle
    }
}

/// `pgrep -f '(^|/)(wf-recorder|wl-screenrec|gpu-screen-recorder)( |$)'`.
pub fn cmdline_matches(cmdline: &str) -> Option<&'static str> {
    for name in RECORDERS {
        let mut start = 0;
        while let Some(i) = cmdline[start..].find(name) {
            let at = start + i;
            let end = at + name.len();
            let before = at == 0 || cmdline.as_bytes()[at - 1] == b'/';
            let after = end == cmdline.len() || cmdline.as_bytes()[end] == b' ';
            if before && after {
                return Some(name);
            }
            start = at + 1;
        }
    }
    None
}
/// `pgrep -x NAME` against `/proc/PID/comm`.
pub fn comm_matches(comm: &str) -> Option<&'static str> {
    RECORDERS.iter().copied().find(|n| *n == comm)
}
fn scan(proc: &util::Proc, m: Match) -> Vec<(i32, &'static str)> {
    let me = std::process::id() as i32;
    proc.pids()
        .into_iter()
        .filter(|p| *p != me)
        .filter_map(|p| match m {
            Match::Cmdline => cmdline_matches(&proc.cmdline(p).join(" ")).map(|n| (p, n)),
            Match::Comm => comm_matches(&proc.stat(p)?.comm).map(|n| (p, n)),
        })
        .collect()
}

/// Laptop exec source: fanotify marks on every `PATH` entry named like a
/// recorder, plus an inotify watch on those directories for later installs.
struct ExecSource {
    exec: AsyncFd<util::ExecWatch>,
    dirs: Option<util::Watch>,
    path: Vec<PathBuf>,
}
impl ExecSource {
    fn new(path_var: &str) -> std::io::Result<Self> {
        let path: Vec<PathBuf> = path_var
            .split(':')
            .filter(|d| d.starts_with('/'))
            .map(PathBuf::from)
            .collect();
        let specs: Vec<(PathBuf, &[&str])> = path.iter().map(|d| (d.clone(), RECORDERS)).collect();
        let mut source = Self {
            // SAFETY: the AsyncFd owns the watch, so the fd outlives its registration.
            exec: unsafe { AsyncFd::register(util::ExecWatch::new()?) }?,
            dirs: util::Watch::new(&specs).ok(),
            path,
        };
        source.mark();
        Ok(source)
    }
    /// (Re)marks the current inodes; replaced binaries are new inodes.
    fn mark(&mut self) {
        let watch = self.exec.get_mut();
        watch.remark();
        for dir in &self.path {
            for name in RECORDERS {
                let bin = dir.join(name);
                if bin.is_file() {
                    watch.mark(&bin);
                }
            }
        }
    }
    /// Resolves once one or more marked binaries were executed.
    async fn exec(&self) -> std::io::Result<()> {
        use std::os::fd::AsRawFd;
        loop {
            let mut guard = self.exec.readable().await?;
            let mut any = false;
            // Readiness is cleared only on EAGAIN, so no event is lost in between.
            let r = guard.try_io(|fd| loop {
                let mut buf = [0u8; 4096];
                let n = unsafe {
                    libc::read(fd.get_ref().as_raw_fd(), buf.as_mut_ptr().cast(), buf.len())
                };
                if n <= 0 {
                    return Err::<(), _>(std::io::Error::last_os_error());
                }
                any = true;
            });
            match r {
                Ok(Err(e)) if e.kind() != std::io::ErrorKind::WouldBlock => return Err(e),
                _ if any => return Ok(()),
                _ => {}
            }
        }
    }
}
async fn maybe<F: std::future::Future>(f: Option<F>) -> F::Output {
    match f {
        Some(f) => f.await,
        None => std::future::pending().await,
    }
}

/// Detection follows the process-latched runtime profile.
pub fn spawn(ctx: &Ctx) -> RecordingHandle {
    spawn_with(ctx, Match::for_profile(hyprv_core::profile::profile()))
}
pub fn spawn_with(ctx: &Ctx, matching: Match) -> RecordingHandle {
    let topic = crate::topic::<RecordingTopic>(ctx, RecordingState::default());
    let (handle, mut rx) = Handle::<RecordingState, RecordingAction>::new(topic.clone());
    let lease = Lease::new();
    let script: PathBuf = ctx.env.home.join(".config/hypr/scripts/record-script.sh");
    let lease2 = lease.clone();
    let mut shutdown = ctx.shutdown.clone();
    let path_var = ctx.env.var("PATH").unwrap_or(DEFAULT_PATH).to_string();
    ctx.rt.spawn(async move {
        let proc = util::Proc::default();
        let mut execs = match matching {
            Match::Cmdline => None,
            Match::Comm => ExecSource::new(&path_var)
                .inspect_err(|e| eprintln!("hyprv recording: exec watch unavailable: {e}"))
                .ok(),
        };
        let mut followups: VecDeque<Instant> = VecDeque::new();
        let (exit_tx, mut exits) = mpsc::unbounded_channel::<i32>();
        let mut known: HashMap<i32, (Recorder, tokio::task::JoinHandle<()>)> = HashMap::new();
        let mut units: Vec<String> = Vec::new();
        let mut state = RecordingState::default();
        let mut demand = lease2.demand();
        let mut do_scan = true; // one initial snapshot
        loop {
            if do_scan {
                let found = scan(&proc, matching);
                for (pid, name) in found {
                    if known.contains_key(&pid) {
                        continue;
                    }
                    if let Ok(fd) = util::Pidfd::open(pid) {
                        let tx = exit_tx.clone();
                        let managed = in_units(pid, &units).await;
                        let task = tokio::spawn(async move {
                            fd.exited().await;
                            let _ = tx.send(pid);
                        });
                        known.insert(
                            pid,
                            (
                                Recorder {
                                    pid: pid as u32,
                                    name: name.into(),
                                    managed,
                                },
                                task,
                            ),
                        );
                    }
                }
                do_scan = false;
            }
            let mut recs: Vec<Recorder> = known.values().map(|(r, _)| r.clone()).collect();
            recs.sort_by_key(|r| r.pid);
            state.active = !recs.is_empty();
            state.recorders = recs;
            topic.publish(state.clone());
            let tick = async {
                if matching == Match::Cmdline && lease2.held() {
                    tokio::time::sleep(SCAN_PERIOD).await
                } else {
                    std::future::pending().await
                }
            };
            tokio::select! {
                Some(pid) = exits.recv() => {
                    known.remove(&pid);
                    if known.values().all(|(r, _)| !r.managed) && !units.is_empty() {
                        // Recorder finalized: retire our RemainAfterExit units.
                        if let Ok(m) = systemd::UserManager::connect().await {
                            for u in units.drain(..) {
                                let _ = m.manager.stop_unit(&u, "replace").await;
                            }
                        }
                    }
                }
                _ = tick => do_scan = true,
                r = maybe(execs.as_ref().map(|x| x.exec())) => {
                    if r.is_err() { execs = None; }
                    let now = Instant::now();
                    followups = EXEC_FOLLOWUPS.iter().map(|d| now + *d).collect();
                    do_scan = true;
                }
                _ = maybe(followups.front().map(|t| tokio::time::sleep_until(*t))) => {
                    followups.pop_front();
                    do_scan = true;
                }
                r = maybe(execs.as_ref().and_then(|x| x.dirs.as_ref()).map(|w| w.changed())) => {
                    match (r, execs.as_mut()) {
                        (Ok(()), Some(x)) => x.mark(),
                        (Err(_), Some(x)) => x.dirs = None,
                        _ => {}
                    }
                    do_scan = true;
                }
                r = demand.changed() => {
                    if r.is_err() { return; }
                    do_scan = *demand.borrow_and_update() > 0;
                }
                msg = rx.recv() => {
                    let Some(env) = msg else { return };
                    state.busy = true;
                    topic.publish(state.clone());
                    let start = match env.action {
                        RecordingAction::Start => true,
                        RecordingAction::Stop => false,
                        RecordingAction::Toggle => known.is_empty(),
                    };
                    let result = if start {
                        start_recorder(&script, &mut units).await
                    } else {
                        stop_recorders(&proc)
                    };
                    // Pick up the new recorder (or confirm exits) right away.
                    if start && result.is_ok() {
                        wait_for_recorder(&proc, matching).await;
                    }
                    do_scan = true;
                    state.busy = false;
                    state.last_error = result.as_ref().err().map(|e| e.message.clone());
                    env.respond(result);
                }
                _ = shutdown.changed() => if *shutdown.borrow() { return },
            }
        }
    });
    RecordingHandle {
        handle,
        scan_lease: lease,
    }
}

async fn in_units(pid: i32, units: &[String]) -> bool {
    if units.is_empty() {
        return false;
    }
    let cgroup = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).unwrap_or_default();
    units.iter().any(|u| cgroup.contains(&format!("/{u}")))
}
async fn start_recorder(script: &std::path::Path, units: &mut Vec<String>) -> ActionResult {
    let mgr = systemd::UserManager::connect().await?;
    let unit = format!("hyprv-recording-{}.service", systemd::unique_suffix());
    let argv = vec![
        script.display().to_string(),
        "--fullscreen-sound".to_string(),
    ];
    mgr.run_transient(
        &unit,
        &argv,
        "HyprV screen recording",
        vec![
            // The script backgrounds the recorder; keep its cgroup alive.
            ("RemainAfterExit", true.into()),
            ("KillSignal", libc::SIGINT.into()),
        ],
    )
    .await?;
    units.push(unit);
    Ok(String::new())
}
/// Recorder appears asynchronously after the script runs; bounded action wait.
async fn wait_for_recorder(proc: &util::Proc, matching: Match) {
    let _ = tokio::time::timeout(Duration::from_secs(3), async {
        while scan(proc, matching).is_empty() {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;
}
fn stop_recorders(proc: &util::Proc) -> ActionResult {
    for name in RECORDERS {
        // pkill matches the 15-byte kernel comm.
        let comm: String = name.chars().take(15).collect();
        let pids: Vec<i32> = proc
            .pids()
            .into_iter()
            .filter(|p| proc.stat(*p).is_some_and(|s| s.comm == comm))
            .collect();
        let mut signalled = false;
        for pid in pids {
            if let Ok(fd) = util::Pidfd::open(pid) {
                signalled |= fd.signal(libc::SIGINT).is_ok();
            }
        }
        if signalled {
            return Ok(String::new());
        }
    }
    Err(ServiceError::new("not_found", "no recorder process"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pgrep_pattern() {
        assert_eq!(
            cmdline_matches("wf-recorder -o DP-4 -f x.mp4"),
            Some("wf-recorder")
        );
        assert_eq!(
            cmdline_matches("/usr/bin/wl-screenrec"),
            Some("wl-screenrec")
        );
        assert_eq!(
            cmdline_matches("/opt/gpu-screen-recorder -w x"),
            Some("gpu-screen-recorder")
        );
        assert_eq!(cmdline_matches("sh -c gpu-screen-recorder"), None);
        assert_eq!(cmdline_matches("my-wf-recorder"), None);
        assert_eq!(cmdline_matches("wf-recorderx"), None);
        assert_eq!(cmdline_matches("vim notes/wf-recorder.txt"), None);
    }

    // ---- laptop exact-comm semantics (`pgrep -x`) ----

    use std::{os::unix::fs::PermissionsExt, path::Path, process::Command, sync::Arc};

    fn stat_line(pid: i32, comm: &str) -> String {
        format!("{pid} ({comm}) S 1 {pid} {pid} 0 -1 0{}\n", " 0".repeat(40))
    }
    fn fake_proc(root: &Path, rows: &[(i32, &str, &str)]) -> util::Proc {
        let _ = std::fs::remove_dir_all(root);
        for (pid, comm, cmdline) in rows {
            let d = root.join(pid.to_string());
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("stat"), stat_line(*pid, comm)).unwrap();
            std::fs::write(d.join("comm"), format!("{comm}\n")).unwrap();
            std::fs::write(d.join("cmdline"), cmdline.replace(' ', "\0") + "\0").unwrap();
        }
        util::Proc { root: root.into() }
    }

    #[test]
    fn comm_is_exact_and_never_argv() {
        assert_eq!(comm_matches("wf-recorder"), Some("wf-recorder"));
        assert_eq!(comm_matches("wl-screenrec"), Some("wl-screenrec"));
        // The kernel keeps 15 bytes: `gpu-screen-recorder` can never match `-x`.
        assert_eq!(comm_matches("gpu-screen-reco"), None);
        for c in [
            "wf-recorder ",
            "wf-recorderx",
            "my-wf-recorder",
            "sh",
            "",
            "WF-RECORDER",
        ] {
            assert_eq!(comm_matches(c), None, "{c:?}");
        }
    }

    #[test]
    fn profiles_select_their_own_semantics_over_one_proc_tree() {
        let root = std::env::temp_dir().join(format!("hyprv-rec-proc-{}", std::process::id()));
        let proc = fake_proc(
            &root,
            &[
                (100, "wf-recorder", "wf-recorder -f out.mp4"),
                // Wrapper whose argv names a recorder: desktop true, laptop false.
                (101, "sh", "sh -c wf-recorder"),
                (102, "bash", "/usr/bin/wf-recorder -a"),
                // Long name truncated by the kernel: desktop true, laptop false.
                (
                    103,
                    "gpu-screen-reco",
                    "/usr/bin/gpu-screen-recorder -w screen",
                ),
                (104, "wl-screenrec", "/usr/bin/wl-screenrec"),
                // Renamed recorder (prctl / exec of another name): laptop true.
                (105, "wf-recorder", "python3 fake.py"),
                (106, "vim", "vim wf-recorder.txt"),
            ],
        );
        let pids = |m| {
            let mut v: Vec<i32> = scan(&proc, m).into_iter().map(|(p, _)| p).collect();
            v.sort();
            v
        };
        assert_eq!(pids(Match::Comm), vec![100, 104, 105]);
        assert_eq!(pids(Match::Cmdline), vec![100, 102, 103, 104]);
        assert_eq!(Match::for_profile(Profile::Laptop), Match::Comm);
        assert_eq!(Match::for_profile(Profile::Desktop), Match::Cmdline);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Scratch `PATH` directory with copies of `sleep` named like recorders.
    struct Scratch(PathBuf);
    impl Scratch {
        fn new(tag: &str) -> Self {
            let d = std::env::temp_dir().join(format!("hyprv-rec-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(d.join("bin")).unwrap();
            std::fs::create_dir_all(d.join("elsewhere")).unwrap();
            Self(d)
        }
        fn bin(&self) -> PathBuf {
            self.0.join("bin")
        }
        /// Installs atomically (new inode), as a package manager does.
        fn install(&self, dir: &Path, name: &str) -> PathBuf {
            let tmp = dir.join(format!(".{name}.tmp"));
            std::fs::copy("/usr/bin/sleep", &tmp).unwrap();
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).unwrap();
            let bin = dir.join(name);
            std::fs::rename(&tmp, &bin).unwrap();
            bin
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    struct Kill(std::process::Child);
    impl Drop for Kill {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn run(bin: &Path) -> Kill {
        Kill(Command::new(bin).arg("30").spawn().unwrap())
    }
    fn ours(pids: &[u32]) -> impl Fn(&RecordingState) -> Vec<u32> + '_ {
        move |s| {
            s.recorders
                .iter()
                .map(|r| r.pid)
                .filter(|p| pids.contains(p))
                .collect()
        }
    }
    async fn wait_until(
        topic: &hyprv_core::store::Topic<RecordingState>,
        within: Duration,
        pred: impl Fn(&RecordingState) -> bool,
    ) -> bool {
        let mut sub = topic.subscribe();
        let deadline = tokio::time::Instant::now() + within;
        loop {
            if pred(&sub.current().value) {
                return true;
            }
            if tokio::time::timeout_at(deadline, sub.changed())
                .await
                .is_err()
            {
                return false;
            }
        }
    }

    #[test]
    fn real_processes_exact_comm_wrapper_and_truncation() {
        let s = Scratch::new("real");
        let exact = run(&s.install(&s.bin(), "wf-recorder"));
        let long = run(&s.install(&s.bin(), "gpu-screen-recorder"));
        let wrapper = Kill(
            Command::new("bash")
                .args(["-c", "exec -a wf-recorder sleep 30"])
                .spawn()
                .unwrap(),
        );
        let ids = [
            exact.0.id() as i32,
            long.0.id() as i32,
            wrapper.0.id() as i32,
        ];
        let proc = util::Proc::default();
        // Exec completes asynchronously; wait until each child shows its final image.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while ids
            .iter()
            .any(|p| proc.cmdline(*p).first().is_none_or(|a| a == "bash"))
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        let found = |m| -> Vec<i32> {
            let mut v: Vec<i32> = scan(&proc, m)
                .into_iter()
                .map(|(p, _)| p)
                .filter(|p| ids.contains(p))
                .collect();
            v.sort();
            v
        };
        assert_eq!(proc.stat(ids[1]).unwrap().comm, "gpu-screen-reco");
        assert_eq!(proc.stat(ids[2]).unwrap().comm, "sleep");
        assert_eq!(found(Match::Comm), vec![ids[0]]);
        let mut desktop = ids.to_vec();
        desktop.sort();
        assert_eq!(found(Match::Cmdline), desktop);
    }

    /// Lets the bounded post-exec follow-up scans run out, so the next step can
    /// only be satisfied by its own event.
    async fn quiet() {
        tokio::time::sleep(EXEC_FOLLOWUPS[2] + Duration::from_millis(200)).await;
    }
    #[tokio::test(flavor = "current_thread")]
    async fn laptop_detection_is_event_driven() {
        if util::ExecWatch::new().is_err() {
            eprintln!("fanotify unavailable (kernel < 5.13?); skipped");
            return;
        }
        let s = Scratch::new("events");
        let wf = s.install(&s.bin(), "wf-recorder");
        let mut vars = std::collections::BTreeMap::new();
        vars.insert("HOME".to_string(), s.0.join("home").display().to_string());
        vars.insert("PATH".to_string(), s.bin().display().to_string());
        let (_stop, shutdown) = tokio::sync::watch::channel(false);
        let ctx = Ctx::new(
            Arc::new(hyprv_core::store::StateStore::default()),
            tokio::runtime::Handle::current(),
            shutdown,
        )
        .with_env(crate::Env::from_vars(vars));
        let h = spawn_with(&ctx, Match::Comm);
        let t = &h.topic;
        tokio::time::sleep(Duration::from_millis(100)).await; // initial snapshot

        // 1. Exec of a marked PATH binary is announced; exit is seen by pidfd.
        let a = run(&wf);
        let pid = a.0.id();
        assert!(
            wait_until(t, Duration::from_secs(5), |st| ours(&[pid])(st) == [pid]).await,
            "exec not detected"
        );
        drop(a);
        assert!(wait_until(t, Duration::from_secs(5), |st| ours(&[pid])(st).is_empty()).await);

        quiet().await;
        // 2. A recorder installed after start (and an upgrade = new inode) is marked
        //    through the PATH directory watch.
        let wl = s.install(&s.bin(), "wl-screenrec");
        tokio::time::sleep(Duration::from_millis(200)).await;
        let b = run(&wl);
        let pid = b.0.id();
        assert!(
            wait_until(t, Duration::from_secs(5), |st| ours(&[pid])(st) == [pid]).await,
            "install not marked"
        );
        drop(b);
        quiet().await;
        let wf = s.install(&s.bin(), "wf-recorder");
        tokio::time::sleep(Duration::from_millis(200)).await;
        let c = run(&wf);
        let pid = c.0.id();
        assert!(
            wait_until(t, Duration::from_secs(5), |st| ours(&[pid])(st) == [pid]).await,
            "upgraded binary not re-marked"
        );
        drop(c);
        quiet().await;

        // 3. No polling: an exact-comm recorder outside PATH stays unseen without
        //    an event, even past several legacy poll periods ...
        let other = run(&s.install(&s.0.join("elsewhere"), "wf-recorder"));
        let pid = other.0.id();
        assert!(
            !wait_until(t, Duration::from_millis(4500), |st| !ours(&[pid])(st)
                .is_empty())
            .await,
            "found without an event: something polls"
        );
        // ... and opening the panel (lease) takes one snapshot that finds it.
        let lease = h.scan_lease.acquire();
        assert!(wait_until(t, Duration::from_secs(2), |st| ours(&[pid])(st) == [pid]).await);
        drop(lease);
        drop(other);
        assert!(wait_until(t, Duration::from_secs(5), |st| ours(&[pid])(st).is_empty()).await);

        // 4. The exact comm can appear after the exec event (FAN_OPEN_EXEC is queued
        //    at open). Here the marked binary is a script interpreter; the script
        //    sleeps, then execs an unmarked exact-name recorder: only the bounded
        //    follow-up scans can see it.
        quiet().await;
        let shell = s.0.join("elsewhere/sh");
        std::fs::copy("/usr/bin/bash", &shell).unwrap();
        let interp = s.bin().join("wl-screenrec");
        std::fs::rename(&shell, &interp).unwrap();
        let late = s.0.join("elsewhere/wf-recorder");
        let script = s.0.join("elsewhere/prep");
        std::fs::write(
            &script,
            format!(
                "#!{}\nsleep 0.4\nexec {} 30\n",
                interp.display(),
                late.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await; // re-mark after replace
        let d = Kill(Command::new(&script).spawn().unwrap());
        let pid = d.0.id();
        assert!(
            wait_until(t, Duration::from_secs(3), |st| ours(&[pid])(st) == [pid]).await,
            "comm set after the exec event was missed"
        );
        drop(d);
    }
}
