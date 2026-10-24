//! Private dbus-daemon harness. Nothing here touches the real session or system bus.
#![allow(dead_code)]
use hyprv_core::{runtime::ServiceRuntime, store::{StateStore, Topic}};
use hyprv_svc_media::Context;
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

pub struct Bus {
    pub address: String,
    child: Child,
    dir: std::path::PathBuf,
}
impl Bus {
    pub fn start() -> Self {
        let dir = std::env::temp_dir().join(format!("hyprv-media-bus-{}-{}", std::process::id(), rand_suffix()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut child = Command::new("dbus-daemon")
            .args(["--config-file=/usr/share/dbus-1/session.conf", "--nofork", "--print-address=1"])
            .arg(format!("--address=unix:path={}/bus", dir.display()))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("dbus-daemon");
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
        Self { address: line.trim().to_owned(), child, dir }
    }
    pub async fn connect(&self) -> zbus::Connection {
        zbus::connection::Builder::address(self.address.as_str()).unwrap().build().await.unwrap()
    }
}
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn rand_suffix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().subsec_nanos() as u64
}

/// Actors live on the ServiceRuntime; peers on a separate multi-thread runtime.
pub struct Harness {
    pub bus: Bus,
    pub peers: tokio::runtime::Runtime,
    pub runtime: Arc<ServiceRuntime>,
    pub ctx: Context,
}
impl Harness {
    pub fn new(read_only: bool) -> Self {
        let bus = Bus::start();
        let peers = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
        let runtime = Arc::new(ServiceRuntime::new().unwrap());
        let address = bus.address.clone();
        let session = runtime.block_on(async move {
            zbus::connection::Builder::address(address.as_str()).unwrap().build().await.unwrap()
        });
        let mut ctx = Context::new(Arc::new(StateStore::default()), runtime.clone());
        ctx.session_bus = Some(session);
        ctx.read_only = read_only;
        Self { bus, peers, runtime, ctx }
    }
    pub fn peer(&self) -> zbus::Connection {
        self.peers.block_on(self.bus.connect())
    }
}

pub fn wait_for<T: Clone + PartialEq>(topic: &Topic<T>, what: &str, pred: impl Fn(&T) -> bool) -> T {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let v = topic.snapshot().value;
        if pred(&v) {
            return v;
        }
        if Instant::now() > deadline {
            panic!("timed out waiting for {what}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
