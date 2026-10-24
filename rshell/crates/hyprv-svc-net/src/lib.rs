//! Native NetworkManager and BlueZ actors. No subprocesses or periodic sampling.
pub mod bluetooth;
mod bus;
mod dh;
mod keyring;
pub mod network;
pub mod prompts;
pub mod rfkill;
pub mod secret;
mod wifi_scan;

use hyprv_core::{
    runtime::ServiceRuntime,
    service::{Service, ServiceFuture},
    store::StateStore,
};
use std::{future::Future, sync::Arc};
use tokio::{sync::oneshot, task::JoinHandle};

pub type Result<T> = std::result::Result<T, String>;
#[derive(Clone)]
pub struct Context {
    pub store: Arc<StateStore>,
    pub runtime: Arc<ServiceRuntime>,
    /// Injectable bus for tests/containers. None connects to the system bus.
    pub system_bus: Option<zbus::Connection>,
    /// True enables read-only inspection: no agent registration or actions.
    pub read_only: bool,
}
impl Context {
    pub fn new(store: Arc<StateStore>, runtime: Arc<ServiceRuntime>) -> Self {
        Self {
            store,
            runtime,
            system_bus: None,
            read_only: false,
        }
    }
}

/// Each action gets a completion future; closing the initiating widget does not cancel it.
pub struct PendingAction(oneshot::Receiver<Result<()>>);
impl PendingAction {
    pub async fn result(self) -> Result<()> {
        self.0.await.map_err(|_| "service stopped".to_owned())?
    }
}

struct ActorService {
    future: Option<std::pin::Pin<Box<dyn Future<Output = ()> + Send>>>,
    worker: Option<JoinHandle<()>>,
}
impl Service for ActorService {
    fn start(&mut self) -> ServiceFuture<'_> {
        Box::pin(async {
            if let Some(f) = self.future.take() {
                self.worker = Some(tokio::spawn(f));
            }
            Ok(())
        })
    }
    fn stop(&mut self) -> ServiceFuture<'_> {
        Box::pin(async {
            if let Some(mut worker) = self.worker.take() {
                if tokio::time::timeout(std::time::Duration::from_secs(3), &mut worker)
                    .await
                    .is_err()
                {
                    worker.abort();
                    let _ = worker.await;
                }
            }
            Ok(())
        })
    }
}
fn supervise(ctx: &Context, future: impl Future<Output = ()> + Send + 'static) -> JoinHandle<()> {
    let mut shutdown = ctx.runtime.shutdown_signal();
    ctx.runtime.spawn(async move {
        let mut service = ActorService {
            future: Some(Box::pin(future)),
            worker: None,
        };
        if *shutdown.borrow() {
            return;
        }
        if service.start().await.is_err() {
            return;
        }
        loop {
            if *shutdown.borrow() || shutdown.changed().await.is_err() {
                break;
            }
        }
        let _ = service.stop().await;
    })
}

/// Transport change events, also used by injectable mocks.
pub use bus::Event;

/// Test infrastructure: private buses and a freed-memory scanning allocator.
#[cfg(test)]
pub(crate) mod test_support {
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        io::BufRead,
        sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    /// Secret marker used by the zeroization tests (never a real credential).
    pub const NEEDLE: &[u8] = b"Zr0iZ3-p4ss!";
    pub static SCAN: AtomicBool = AtomicBool::new(false);
    pub static HITS: AtomicUsize = AtomicUsize::new(0);
    /// Counts heap blocks that still contain NEEDLE when they are freed,
    /// i.e. secret bytes released without zeroization.
    pub struct ScanningAlloc;
    unsafe impl GlobalAlloc for ScanningAlloc {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            System.alloc(l)
        }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
            if SCAN.load(Ordering::Relaxed) && l.size() >= NEEDLE.len() && l.size() <= 1 << 20 {
                let block = std::slice::from_raw_parts(p, l.size());
                if block.windows(NEEDLE.len()).any(|w| w == NEEDLE) {
                    HITS.fetch_add(1, Ordering::Relaxed);
                }
            }
            System.dealloc(p, l)
        }
        unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
            // Model realloc as alloc+copy+dealloc so moved-out old blocks are scanned.
            let new = self.alloc(Layout::from_size_align_unchecked(n, l.align()));
            if !new.is_null() {
                std::ptr::copy_nonoverlapping(p, new, l.size().min(n));
                self.dealloc(p, l);
            }
            new
        }
    }
    #[global_allocator]
    static GLOBAL: ScanningAlloc = ScanningAlloc;

    /// Private dbus-daemon for fake NetworkManager/BlueZ/Secret Service peers.
    /// Tests never touch the real system or session bus.
    pub struct PrivateBus {
        child: std::process::Child,
        pub address: String,
    }
    impl PrivateBus {
        pub fn start() -> Self {
            Self::start_with("/usr/share/dbus-1/session.conf")
        }
        /// Private bus whose service directory activates `name` by running
        /// `touch marker` (records an activation attempt; never owns the name).
        pub fn with_activatable(name: &str, marker: &std::path::Path) -> Self {
            let dir = marker
                .parent()
                .unwrap()
                .join(format!("services-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join(format!("{name}.service")),
                format!(
                    "[D-BUS Service]\nName={name}\nExec=/usr/bin/touch {}\n",
                    marker.display()
                ),
            )
            .unwrap();
            let conf = dir.join("bus.conf");
            std::fs::write(
                &conf,
                format!(
                    "<!DOCTYPE busconfig PUBLIC \"-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN\" \"http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd\">\n<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><servicedir>{}</servicedir><policy context=\"default\"><allow send_destination=\"*\" eavesdrop=\"true\"/><allow eavesdrop=\"true\"/><allow own=\"*\"/></policy></busconfig>\n",
                    dir.display()
                ),
            )
            .unwrap();
            Self::start_with(conf.to_str().unwrap())
        }
        fn start_with(config: &str) -> Self {
            let config = format!("--config-file={config}");
            let mut child = std::process::Command::new("dbus-daemon")
                .args([config.as_str(), "--nofork", "--print-address=1"])
                .stdout(std::process::Stdio::piped())
                .spawn()
                .expect("dbus-daemon");
            let mut line = String::new();
            std::io::BufReader::new(child.stdout.take().unwrap())
                .read_line(&mut line)
                .unwrap();
            Self {
                child,
                address: line.trim().to_string(),
            }
        }
        pub async fn connect(&self) -> zbus::Connection {
            zbus::connection::Builder::address(self.address.as_str())
                .unwrap()
                .build()
                .await
                .unwrap()
        }
        pub fn builder(&self) -> zbus::connection::Builder<'static> {
            zbus::connection::Builder::address(self.address.as_str()).unwrap()
        }
    }
    impl Drop for PrivateBus {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
    pub fn ctx(bus: zbus::Connection) -> crate::Context {
        let mut c = crate::Context::new(
            std::sync::Arc::new(hyprv_core::store::StateStore::default()),
            std::sync::Arc::new(hyprv_core::runtime::ServiceRuntime::new().unwrap()),
        );
        c.system_bus = Some(bus);
        c
    }
}
