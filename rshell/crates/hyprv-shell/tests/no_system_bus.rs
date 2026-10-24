//! The shell must start and keep running without a system D-Bus (docs/requests/hyprv-shell.md,
//! 2026-10-23). Everything runs on private dbus-daemons in a temp dir: the user's session
//! and system buses are never contacted (their addresses are overridden before any service
//! starts, and this test binary has nothing else in it).
use hyprv_core::{runtime::ServiceRuntime, store::StateStore};
use hyprv_shell::system_bus::{SystemBusState, SystemBusTopic};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

struct Daemon(Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn daemon(dir: &Path, name: &str, kind: &str) -> Option<Daemon> {
    let socket = dir.join(name);
    let _ = std::fs::remove_file(&socket);
    let config = dir.join(format!("{name}.conf"));
    std::fs::write(
        &config,
        format!(
            "<!DOCTYPE busconfig PUBLIC \"-//freedesktop//DTD D-BUS Bus Configuration 1.0//EN\" \
             \"http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd\"><busconfig><type>{kind}</type>\
             <listen>unix:path={}</listen><policy context=\"default\"><allow send_destination=\"*\" eavesdrop=\"true\"/>\
             <allow eavesdrop=\"true\"/><allow own=\"*\"/></policy></busconfig>",
            socket.display()
        ),
    )
    .ok()?;
    let child = Command::new("dbus-daemon")
        .arg(format!("--config-file={}", config.display()))
        .arg("--nofork")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let start = Instant::now();
    while !socket.exists() {
        if start.elapsed() > Duration::from_secs(5) {
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Some(Daemon(child))
}
fn wait_for(
    topic: &hyprv_core::store::Topic<SystemBusState>,
    what: &str,
    f: impl Fn(&SystemBusState) -> bool,
) -> SystemBusState {
    let start = Instant::now();
    loop {
        let s = topic.snapshot().value;
        if f(&s) {
            return s;
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "timed out waiting for {what}: {s:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}
fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hyprv-no-system-bus-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("run")).unwrap();
    std::fs::create_dir_all(dir.join("home")).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir.join("run"), std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

#[test]
fn shell_services_start_without_system_bus_and_recover() {
    if Command::new("dbus-daemon")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipped: dbus-daemon not installed");
        return;
    }
    let dir = scratch();
    // Isolation before anything connects: private session bus, absent system bus.
    let Some(_session) = daemon(&dir, "session", "session") else {
        panic!("private session bus did not start")
    };
    std::env::set_var(
        "DBUS_SESSION_BUS_ADDRESS",
        format!("unix:path={}", dir.join("session").display()),
    );
    std::env::set_var(
        "DBUS_SYSTEM_BUS_ADDRESS",
        format!("unix:path={}", dir.join("system").display()),
    );
    std::env::set_var("XDG_RUNTIME_DIR", dir.join("run"));
    std::env::set_var("HOME", dir.join("home"));
    for key in ["HYPRLAND_INSTANCE_SIGNATURE", "WAYLAND_DISPLAY", "DISPLAY"] {
        std::env::remove_var(key);
    }

    // 1. The shell's service hub starts with no system bus (this used to abort startup).
    let runtime = Arc::new(ServiceRuntime::new().unwrap());
    let store = Arc::new(StateStore::default());
    let (ping, _source) = hyprv_core::calloop::ping::make_ping().unwrap();
    let hub = hyprv_shell::services::Hub::spawn(runtime.clone(), store.clone(), ping, true)
        .expect("shell services must start without a system bus");
    let topic = store.topic::<SystemBusTopic>(SystemBusState::default());
    assert!(!topic.snapshot().value.available);
    std::thread::sleep(Duration::from_millis(700));
    assert!(
        topic.snapshot().value.attempts >= 1,
        "no reconnect attempt: {:?}",
        topic.snapshot().value
    );

    // 2. The bus appears: the supervisor notices on its next backoff attempt.
    let system = daemon(&dir, "system", "system").expect("private system bus");
    let s = wait_for(&topic, "recovery", |s| s.recovered);
    assert!(s.attempts >= 1);
    drop(hub);

    // 3. A working connection that is lost (dbus-broker restart) is detected, then recovered.
    let store2 = Arc::new(StateStore::default());
    let (bus, up) = hyprv_shell::system_bus::connect(&runtime, &store2).unwrap();
    assert!(up, "connects to the (private) system bus");
    hyprv_shell::system_bus::supervise(&runtime, &store2, bus, true);
    let topic2 = store2.topic::<SystemBusTopic>(SystemBusState::default());
    std::thread::sleep(Duration::from_millis(200));
    drop(system);
    wait_for(&topic2, "loss", |s| s.lost);
    let _system = daemon(&dir, "system", "system").expect("private system bus again");
    wait_for(&topic2, "recovery after loss", |s| s.recovered);
    let _ = std::fs::remove_dir_all(&dir);
}
