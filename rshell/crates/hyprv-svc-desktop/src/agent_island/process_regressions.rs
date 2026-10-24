use super::*;
fn pid(root: &std::path::Path, n: i64, comm: &str, parent: i64, start: u64, argv: &[&str]) {
    pid_in(root, n, comm, parent, start, argv, 'S');
}
fn pid_in(
    root: &std::path::Path,
    n: i64,
    comm: &str,
    parent: i64,
    start: u64,
    argv: &[&str],
    state: char,
) {
    let p = root.join(n.to_string());
    std::fs::create_dir_all(&p).unwrap();
    std::fs::write(
        p.join("stat"),
        format!("{n} ({comm}) {state} {parent} 1 1 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 {start} 0 0"),
    )
    .unwrap();
    std::fs::write(p.join("cmdline"), argv.join("\0") + "\0").unwrap();
    std::fs::write(p.join("status"), format!("Tgid:\t{n}\n")).unwrap();
}
fn last(root: &std::path::Path, n: i64) {
    std::fs::write(root.join("loadavg"), format!("0 0 0 1/1 {n}")).unwrap();
}
#[test]
fn initial_and_late_exec_are_observed_without_pid_allocation() {
    let root = std::env::temp_dir().join(format!("hyprv-initial-exec-{}", std::process::id()));
    let proc = Proc { root: root.clone() };
    pid(&root, 10, "bash", 1, 5, &["bash"]);
    last(&root, 10);
    let mut table = ProcTable::default();
    assert!(!table.refresh(&proc, 100.));
    pid(&root, 10, "claude", 1, 5, &["claude"]);
    assert!(table.refresh(&proc, 101.5));
    assert_eq!(table.agents(), &[(10, 5, "claude")]);
    pid(&root, 10, "bash", 1, 5, &["bash"]);
    assert!(table.refresh(&proc, 103.));
    assert!(!table.refresh(&proc, 104.5));
    // A late exec in an old non-agent PID is announced by ExecWatch
    // (fanotify FAN_OPEN_EXEC on the agent binaries) and found at once.
    pid(&root, 10, "claude", 1, 5, &["claude"]);
    table.note_exec();
    assert!(table.refresh(&proc, 106.));
    // A same-name exec can change classification through argv, too.
    pid(&root, 10, "claude", 1, 5, &["claude", "--version"]);
    assert!(table.refresh(&proc, 107.5));
    assert!(table.agents().is_empty());
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn unsignalled_late_exec_waits_for_the_sweep_and_signals_are_rate_limited() {
    let root = std::env::temp_dir().join(format!("hyprv-late-exec-{}", std::process::id()));
    let proc = Proc { root: root.clone() };
    pid(&root, 10, "bash", 1, 5, &["bash"]);
    pid(&root, 11, "bash", 1, 6, &["bash"]);
    last(&root, 11);
    let mut table = ProcTable::default();
    assert!(!table.refresh(&proc, 100.));
    assert!(!table.refresh(&proc, 101.5)); // initial-snapshot recheck
                                           // Unsignalled (e.g. an agent started from an unmarked path): no per-probe
                                           // full re-stat any more, the periodic sweep finds it.
    pid(&root, 10, "claude", 1, 5, &["claude"]);
    for t in [103., 130., 159.] {
        assert!(!table.refresh(&proc, t), "found before the sweep at {t}");
    }
    assert!(table.refresh(&proc, 160.5));
    // Signals: the first is served at once, the next waits out the rate limit.
    pid(&root, 11, "claude", 1, 6, &["claude"]);
    table.note_exec();
    assert!(table.refresh(&proc, 162.));
    pid(&root, 11, "bash", 1, 6, &["bash"]);
    pid(&root, 10, "bash", 1, 5, &["bash"]);
    table.note_exec();
    // PID 10/11 are agent-named, so leaving the agent set needs no signal.
    assert!(table.refresh(&proc, 163.5));
    pid(&root, 10, "claude", 1, 5, &["claude"]);
    table.note_exec();
    assert!(!table.refresh(&proc, 165.), "rate limited");
    assert!(!table.refresh(&proc, 175.), "rate limited");
    assert!(
        table.refresh(&proc, 177.),
        "pending signal served after 15 s"
    );
    std::fs::remove_dir_all(root).unwrap();
}
/// FAN_OPEN_EXEC is queued when execve opens the binary, before the new comm
/// and argv are visible. A signalled pass that lands in that window reads the
/// old image; the PID is running (`R`) or in disk sleep (`D`) then, so it is
/// read again at the next probe even though no further signal comes.
#[test]
fn exec_in_flight_during_the_signalled_pass_is_seen_at_the_next_probe() {
    let root = std::env::temp_dir().join(format!("hyprv-inflight-exec-{}", std::process::id()));
    let proc = Proc { root: root.clone() };
    pid(&root, 10, "bash", 1, 5, &["bash"]);
    pid(&root, 11, "bash", 1, 6, &["bash"]);
    last(&root, 11);
    let mut table = ProcTable::default();
    assert!(!table.refresh(&proc, 100.));
    assert!(!table.refresh(&proc, 101.5));
    for (state, t) in [('R', 103.), ('D', 120.)] {
        pid_in(&root, 10, "bash", 1, 5, &["bash"], state);
        table.note_exec();
        assert!(!table.refresh(&proc, t), "the pass sees the old image");
        pid(&root, 10, "claude", 1, 5, &["claude"]);
        assert!(
            table.refresh(&proc, t + 1.5),
            "exec finished after the pass ({state})"
        );
        pid(&root, 10, "bash", 1, 5, &["bash"]);
        assert!(table.refresh(&proc, t + 3.));
    }
    // A sleeping PID is not inside execve: no extra stat, the sweep finds it.
    pid(&root, 11, "claude", 1, 6, &["claude"]);
    assert!(!table.refresh(&proc, 125.));
    assert!(table.refresh(&proc, 160.5));
    std::fs::remove_dir_all(root).unwrap();
}
/// Real /proc and a real exec in an old PID: a copy of `sleep` named `claude`
/// is marked, a shell waits, then execs it. Read-only towards the system.
///
/// The table also marks the host's agent binaries (and those of agent-named
/// processes other tests run), and their execs share the
/// [`EXEC_RESCAN_SECONDS`] budget. So the contract checked is: the signal is
/// raised, it is served at the next probe when the budget is free, and
/// otherwise at the first probe the limit allows, before the sweep.
#[test]
fn real_late_exec_is_signalled_by_fanotify() {
    struct Cleanup(std::process::Child, std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
            let _ = std::fs::remove_dir_all(&self.1);
        }
    }
    let dir = std::env::temp_dir().join(format!("hyprv-fanotify-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("claude");
    std::fs::copy("/usr/bin/sleep", &exe).unwrap();
    let proc = Proc::default();
    let mut table = ProcTable::default();
    if !table.watch_exec(&exe) {
        eprintln!("fanotify unavailable (kernel < 5.13?); skipped");
        return;
    }
    // Only this private copy: nothing else on the host execs it.
    let mut own = crate::util::ExecWatch::new().unwrap();
    assert!(own.mark(&exe));
    use std::io::Write;
    let mut shell = Cleanup(
        std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(format!("read x; exec '{}' 30", exe.display()))
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap(),
        dir.clone(),
    );
    let pid = shell.0.id() as i64;
    let t0 = crate::util::epoch_f64();
    table.refresh(&proc, t0);
    table.refresh(&proc, t0 + 1.5); // the shell is now an old, known PID
    assert!(!table.agents().iter().any(|a| a.0 == pid));
    assert!(!own.drain(), "no exec of the copy yet");
    shell.0.stdin.take().unwrap().write_all(b"go\n").unwrap();
    let comm = format!("/proc/{pid}/comm");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !std::fs::read_to_string(&comm).is_ok_and(|c| c.trim() == "claude") {
        assert!(
            std::time::Instant::now() < deadline,
            "the shell never exec'd the copy"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        own.drain(),
        "FAN_OPEN_EXEC raised for the exec in an old PID"
    );
    let budget_free = table.last_exec_scan.is_none();
    table.refresh(&proc, t0 + 3.0);
    let next_probe = table.agents().iter().any(|a| a.0 == pid);
    assert!(
        next_probe || !budget_free,
        "found at the next probe when the budget is free"
    );
    if !next_probe {
        assert!(table.exec_pending, "the signal waits out the limit");
        let due = table.last_exec_scan.unwrap() + EXEC_RESCAN_SECONDS;
        assert!(due < t0 + PROCESS_SWEEP_SECONDS);
        table.refresh(&proc, due);
        assert!(
            table.agents().iter().any(|a| a.0 == pid),
            "found once the rate limit allows, before the sweep"
        );
    }
}
#[test]
fn reused_and_reparented_ancestors_match_a_fresh_scan() {
    for reused in [false, true] {
        let root =
            std::env::temp_dir().join(format!("hyprv-ancestor-{reused}-{}", std::process::id()));
        let proc = Proc { root: root.clone() };
        pid(&root, 10, "codex", 1, 5, &["codex"]);
        pid(&root, 20, "bash", 10, 6, &["bash"]);
        last(&root, 20);
        let mut table = ProcTable::default();
        assert!(!table.refresh(&proc, 100.));
        pid(&root, 20, "bash", 1, if reused { 99 } else { 6 }, &["bash"]);
        pid(
            &root,
            30,
            "systemd-inhibit",
            20,
            100,
            &[
                "systemd-inhibit",
                "--who",
                "codex",
                "--why",
                "Codex is running an active turn",
            ],
        );
        last(&root, 30);
        table.refresh(&proc, 101.5);
        let mut fresh = ProcTable::default();
        fresh.refresh(&proc, 101.5);
        assert_eq!(table.agents(), fresh.agents());
        assert!(table.agents().is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
}
