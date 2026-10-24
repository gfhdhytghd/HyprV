//! Private bus and filesystem only. Never consult the host system bus/sysfs.
use crate::{Context, Paths};
use std::{
    io::BufRead,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
pub struct Bus(std::process::Child, String);
impl Bus {
    pub fn new() -> Self {
        let mut child = std::process::Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--nopidfile", "--print-address=1"])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("private test bus");
        let mut address = String::new();
        std::io::BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self(child, address.trim().into())
    }
    pub async fn connection(&self) -> zbus::Connection {
        zbus::connection::Builder::address(self.1.as_str())
            .unwrap()
            .build()
            .await
            .unwrap()
    }
}
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
static SERIAL: AtomicU64 = AtomicU64::new(0);
pub struct Tree(pub PathBuf);
impl Tree {
    pub fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "hyprv-laptop-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    pub fn write(&self, path: &str, value: &str) {
        let p = self.0.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, value).unwrap();
    }
    pub fn paths(&self) -> Paths {
        Paths {
            sys: self.0.join("sys"),
            proc: self.0.join("proc"),
            state: self.0.join("state"),
            runtime: self.0.join("runtime/hyprv"),
        }
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub fn context(
    bus: zbus::Connection,
    paths: Paths,
    runtime: Option<&Arc<hyprv_core::runtime::ServiceRuntime>>,
) -> Context {
    let (_stop, shutdown) = tokio::sync::watch::channel(false);
    let (sleeping, _) = tokio::sync::watch::channel(false);
    Context {
        runtime: runtime.map(Arc::downgrade).unwrap_or_default(),
        shutdown: runtime.map(|r| r.shutdown_signal()).unwrap_or(shutdown),
        bus,
        paths,
        store: Arc::new(hyprv_core::store::StateStore::default()),
        sleeping,
    }
}
pub async fn settle() {
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
}
