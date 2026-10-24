//! The shell cannot read root-only RAPL. This client owns a demand-scoped lease
//! on the existing privileged helper, never a fallback user-side energy read.
use crate::actor::{self, Result};
use futures_util::StreamExt;
use hyprv_privhelper::{rapl::State, BUS, INTERFACE, PATH};

#[derive(Clone, Copy)]
struct Endpoint {
    bus: &'static str,
    path: &'static str,
    interface: &'static str,
}
impl Endpoint {
    fn for_profile(profile: crate::Profile) -> Self {
        match profile {
            crate::Profile::Desktop => Self {
                bus: BUS,
                path: PATH,
                interface: INTERFACE,
            },
            crate::Profile::Laptop => Self {
                bus: hyprv_privhelper::cpu_power::BUS,
                path: hyprv_privhelper::cpu_power::PATH,
                interface: hyprv_privhelper::cpu_power::INTERFACE,
            },
        }
    }
}

pub(super) struct Client {
    bus: zbus::Connection,
    endpoint: Endpoint,
    stream: Option<actor::SignalInbox>,
    subscribed: bool,
    attempted: bool,
    acquisition: Option<tokio::task::JoinHandle<Result<String>>>,
    release: Option<tokio::task::JoinHandle<()>>,
    pub state: State,
    last_error: Option<String>,
}
impl Client {
    #[cfg(test)]
    pub fn new(bus: zbus::Connection) -> Self {
        Self::for_profile(bus, crate::Profile::Desktop)
    }
    pub fn for_profile(bus: zbus::Connection, profile: crate::Profile) -> Self {
        Self {
            bus,
            endpoint: Endpoint::for_profile(profile),
            stream: None,
            subscribed: false,
            attempted: false,
            acquisition: None,
            release: None,
            state: State::default(),
            last_error: None,
        }
    }
    /// Balance a remote acquisition while the sampled actor itself is dormant.
    /// Retain the release barrier: a fast reopen on the same D-Bus unique name
    /// must not let an old Unsubscribe remove the new Subscribe's lease.
    pub fn suspend(&mut self) {
        self.stream = None;
        self.attempted = false;
        self.state = State::default();
        if !self.subscribed {
            return;
        }
        self.subscribed = false;
        let bus = self.bus.clone();
        let endpoint = self.endpoint;
        let acquisition = self.acquisition.take();
        let previous = self.release.take();
        self.release = Some(tokio::spawn(async move {
            if let Some(job) = previous {
                let _ = job.await;
            }
            if let Some(job) = acquisition {
                let _ = job.await;
            }
            let result: Result<()> = async {
                actor::proxy(&bus, endpoint.bus, endpoint.path, endpoint.interface)
                    .await?
                    .call::<_, _, ()>("UnsubscribeCpuPower", &())
                    .await?;
                Ok(())
            }
            .await;
            if let Err(e) = result {
                eprintln!("sysstats CPU package power release failed: {e}");
            }
        }));
    }
    pub async fn connect(&mut self) -> Result<()> {
        self.demand(false).await;
        self.attempted = false;
        self.state = State::default();
        self.stream = Some(
            actor::signals_member(
                &self.bus,
                self.endpoint.bus,
                self.endpoint.path,
                self.endpoint.interface,
                "CpuPowerChanged",
            )
            .await?,
        );
        Ok(())
    }
    fn report(&mut self, error: Option<String>) {
        if self.last_error != error {
            if let Some(e) = &error {
                eprintln!("sysstats CPU package power unavailable: {e}");
            } else if self.last_error.is_some() {
                eprintln!("sysstats CPU package power source recovered");
            }
            self.last_error = error;
        }
    }
    fn accept(&mut self, json: &str) -> Result<()> {
        if json.len() > 4096 {
            return Err("CPU power response exceeds limit".into());
        }
        let state: State = serde_json::from_str(json)?;
        if state.watts.is_some_and(|w| !w.is_finite() || w < 0.)
            || (state.watts.is_some() && state.error.is_some())
        {
            return Err("invalid CPU power telemetry".into());
        }
        self.report(state.error.clone());
        self.state = state;
        Ok(())
    }
    pub async fn demand(&mut self, wanted: bool) {
        if let Some(job) = self.release.as_mut() {
            let _ = job.await;
            self.release = None;
        }
        if wanted && !self.attempted {
            // Laptop compact/hidden consumers never install a telemetry stream.
            if self.stream.is_none() {
                match actor::signals_member(
                    &self.bus,
                    self.endpoint.bus,
                    self.endpoint.path,
                    self.endpoint.interface,
                    "CpuPowerChanged",
                )
                .await
                {
                    Ok(stream) => self.stream = Some(stream),
                    Err(e) => {
                        self.attempted = true;
                        self.state = State {
                            watts: None,
                            error: Some(e.to_string()),
                        };
                        self.report(Some(e.to_string()));
                        return;
                    }
                }
            }
            // Cancellation during signal setup must remain retryable. Only mark
            // an attempt once no await separates it from spawning the RPC.
            self.attempted = true;
            // Keep the RPC alive across actor select cancellation. Dropping a
            // local call future after it was sent can otherwise leak a lease.
            self.subscribed = true;
            let bus = self.bus.clone();
            let endpoint = self.endpoint;
            self.acquisition = Some(tokio::spawn(async move {
                let p = actor::proxy(&bus, endpoint.bus, endpoint.path, endpoint.interface).await?;
                Ok(p.call("SubscribeCpuPower", &()).await?)
            }));
        }
        if let Some(job) = self.acquisition.as_mut() {
            let result = job.await;
            self.acquisition = None;
            match result {
                Ok(Ok(json)) => {
                    if let Err(e) = self.accept(&json) {
                        self.state = State {
                            watts: None,
                            error: Some(e.to_string()),
                        };
                        self.report(Some(e.to_string()));
                    }
                }
                result => {
                    // Even a timeout may have acquired remotely. Balance the
                    // intent on release; retry only on demand/provider change.
                    let error = format!("privileged CPU power subscription failed: {result:?}");
                    self.state = State {
                        watts: None,
                        error: Some(error.clone()),
                    };
                    self.report(Some(error));
                }
            }
        }
        if !wanted {
            self.attempted = false;
            self.state = State::default();
            if self.subscribed {
                let result: Result<()> = async {
                    actor::proxy(
                        &self.bus,
                        self.endpoint.bus,
                        self.endpoint.path,
                        self.endpoint.interface,
                    )
                    .await?
                    .call::<_, _, ()>("UnsubscribeCpuPower", &())
                    .await?;
                    Ok(())
                }
                .await;
                match result {
                    Ok(()) => self.subscribed = false,
                    Err(e) => self.report(Some(e.to_string())),
                }
            }
            if self.endpoint.bus == hyprv_privhelper::cpu_power::BUS {
                self.stream = None;
            }
        }
    }
    pub async fn event(&mut self) -> Result<()> {
        loop {
            let Some(stream) = self.stream.as_mut() else {
                return std::future::pending().await;
            };
            let message = stream
                .next()
                .await
                .ok_or("CPU power signal stream ended")??;
            let header = message.header();
            if header
                .interface()
                .is_some_and(|i| i.as_str() == self.endpoint.interface)
                && header
                    .path()
                    .is_some_and(|p| p.as_str() == self.endpoint.path)
                && header
                    .member()
                    .is_some_and(|m| m.as_str() == "CpuPowerChanged")
                && self.subscribed
            {
                let json: String = message.body().deserialize()?;
                if let Err(e) = self.accept(&json) {
                    self.report(Some(e.to_string()));
                    self.state = State {
                        watts: None,
                        error: Some(e.to_string()),
                    };
                }
                return Ok(());
            }
        }
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        if self.subscribed {
            let bus = self.bus.clone();
            let acquisition = self.acquisition.take();
            let endpoint = self.endpoint;
            // Normal shutdown releases explicitly. A killed shell is covered
            // by the helper's NameOwnerChanged unique-name lease cleanup.
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    if let Some(job) = acquisition {
                        let _ = job.await;
                    }
                    if let Ok(p) =
                        actor::proxy(&bus, endpoint.bus, endpoint.path, endpoint.interface).await
                    {
                        let _ = p.call::<_, _, ()>("UnsubscribeCpuPower", &()).await;
                    }
                });
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_and_invalid_readings_are_explicit() {
        let bad: std::result::Result<State, _> =
            serde_json::from_str(r#"{"watts":null,"error":"permission denied"}"#);
        assert_eq!(bad.unwrap().error.as_deref(), Some("permission denied"));
    }
}

#[cfg(test)]
mod bus_regressions {
    use super::*;
    use std::{
        io::BufRead,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::Duration,
    };
    struct Bus(std::process::Child, String);
    impl Bus {
        fn new() -> Self {
            let mut child = std::process::Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--nopidfile", "--print-address=1"])
                .stdout(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            let mut address = String::new();
            std::io::BufReader::new(child.stdout.take().unwrap())
                .read_line(&mut address)
                .unwrap();
            Self(child, address.trim().into())
        }
        async fn connect(&self) -> zbus::Connection {
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
    struct Fake {
        acquired: Arc<AtomicUsize>,
        released: Arc<AtomicUsize>,
        delay: bool,
    }
    #[zbus::interface(name = "org.hyprv.Shell.PrivHelper1")]
    impl Fake {
        async fn subscribe_cpu_power(&self) -> String {
            self.acquired.fetch_add(1, Ordering::SeqCst);
            if self.delay {
                tokio::time::sleep(Duration::from_millis(80)).await;
            }
            r#"{"watts":12.35,"error":null}"#.into()
        }
        fn unsubscribe_cpu_power(&self) {
            self.released.fetch_add(1, Ordering::SeqCst);
        }
    }
    struct LaptopFake {
        acquired: Arc<AtomicUsize>,
        released: Arc<AtomicUsize>,
    }
    #[zbus::interface(name = "org.hyprv.Shell.CpuPower1")]
    impl LaptopFake {
        fn subscribe_cpu_power(&self) -> String {
            self.acquired.fetch_add(1, Ordering::SeqCst);
            r#"{"watts":7.25,"error":null}"#.into()
        }
        fn unsubscribe_cpu_power(&self) {
            self.released.fetch_add(1, Ordering::SeqCst);
        }
    }
    #[test]
    fn laptop_optional_endpoint_is_visible_only_and_never_uses_combined_helper() {
        use crate::{
            sysstats::{self, Metric},
            test_support::{context, Tree},
        };
        let rt = Arc::new(hyprv_core::runtime::ServiceRuntime::new().unwrap());
        rt.block_on(async {
            let (_bus, server, bus, desktop_calls, _) = setup(false).await;
            let acquired = Arc::new(AtomicUsize::new(0));
            let released = Arc::new(AtomicUsize::new(0));
            server
                .object_server()
                .at(
                    hyprv_privhelper::cpu_power::PATH,
                    LaptopFake {
                        acquired: acquired.clone(),
                        released: released.clone(),
                    },
                )
                .await
                .unwrap();
            server
                .request_name(hyprv_privhelper::cpu_power::BUS)
                .await
                .unwrap();
            let tree = Tree::new();
            tree.write("proc/stat", "cpu 10 0 0 10\n");
            tree.write("proc/meminfo", "MemTotal: 1000 kB\nMemAvailable: 500 kB\n");
            let handle = sysstats::spawn_for_profile(
                context(bus, tree.paths(), Some(&rt)),
                crate::Profile::Laptop,
            );
            let compact = handle.laptop_strip(true).unwrap().unwrap();
            handle.action(sysstats::Action::Refresh).await.unwrap();
            assert_eq!(acquired.load(Ordering::SeqCst), 0);
            let watts = handle
                .lease(vec![Metric::CpuPackagePower], Duration::from_secs(1))
                .unwrap();
            handle.action(sysstats::Action::Refresh).await.unwrap();
            tokio::time::timeout(Duration::from_secs(2), async {
                while handle.state.snapshot().value.package_power_w != Some(7.25) {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap_or_else(|e| {
                panic!(
                    "{e}: acquired={} state={:?} error={:?}",
                    acquired.load(Ordering::SeqCst),
                    handle.state.snapshot().value,
                    handle.error.snapshot().value
                )
            });
            assert_eq!(acquired.load(Ordering::SeqCst), 1);
            drop(watts);
            tokio::time::timeout(Duration::from_secs(2), async {
                while released.load(Ordering::SeqCst) != 1 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert_eq!(desktop_calls.load(Ordering::SeqCst), 0);
            drop(compact);
            handle.action(sysstats::Action::Refresh).await.unwrap();
            assert_eq!(acquired.load(Ordering::SeqCst), 1);
        });
    }
    #[tokio::test]
    async fn cancelled_lazy_signal_setup_remains_retryable() {
        let (_bus, _server, bus, acquired, released) = setup(false).await;
        let mut client = Client::new(bus);
        let mut pending = Box::pin(client.demand(true));
        assert!(futures_util::poll!(pending.as_mut()).is_pending());
        drop(pending);
        assert!(
            !client.attempted,
            "cancelled signal setup suppressed the acquisition"
        );
        client.demand(true).await;
        assert_eq!(acquired.load(Ordering::SeqCst), 1);
        assert_eq!(client.state.watts, Some(12.35));
        client.demand(false).await;
        assert_eq!(released.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn rapid_hide_reopen_waits_for_late_acquisition_release() {
        let (_bus, _server, bus, acquired, released) = setup(true).await;
        let mut client = Client::new(bus);
        client.connect().await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(20), client.demand(true))
                .await
                .is_err()
        );
        assert_eq!(acquired.load(Ordering::SeqCst), 1);
        client.suspend();
        assert!(client.stream.is_none());
        // Reopen cannot issue Subscribe #2 until Unsubscribe #1 completed.
        assert!(
            tokio::time::timeout(Duration::from_millis(20), client.demand(true))
                .await
                .is_err()
        );
        assert_eq!(acquired.load(Ordering::SeqCst), 1);
        assert_eq!(released.load(Ordering::SeqCst), 0);
        client.demand(true).await;
        assert_eq!(acquired.load(Ordering::SeqCst), 2);
        assert_eq!(released.load(Ordering::SeqCst), 1);
        client.demand(false).await;
        assert_eq!(released.load(Ordering::SeqCst), 2);
    }
    #[tokio::test]
    async fn missing_optional_laptop_helper_does_not_fall_back_to_desktop() {
        let (_bus, _server, bus, desktop_calls, _) = setup(false).await;
        let mut client = Client::for_profile(bus, crate::Profile::Laptop);
        client.demand(false).await;
        assert!(client.stream.is_none());
        client.demand(true).await;
        assert!(client.state.watts.is_none());
        assert!(client.state.error.is_some());
        assert_eq!(desktop_calls.load(Ordering::SeqCst), 0);
        client.demand(false).await;
        assert!(client.stream.is_none());
    }
    async fn setup(
        delay: bool,
    ) -> (
        Bus,
        zbus::Connection,
        zbus::Connection,
        Arc<AtomicUsize>,
        Arc<AtomicUsize>,
    ) {
        let bus = Bus::new();
        let server = bus.connect().await;
        let client = bus.connect().await;
        let acquired = Arc::new(AtomicUsize::new(0));
        let released = Arc::new(AtomicUsize::new(0));
        server
            .object_server()
            .at(
                PATH,
                Fake {
                    acquired: acquired.clone(),
                    released: released.clone(),
                    delay,
                },
            )
            .await
            .unwrap();
        server.request_name(BUS).await.unwrap();
        (bus, server, client, acquired, released)
    }
    #[tokio::test]
    async fn unreadable_user_energy_still_displays_helper_watts_and_releases_on_widget_hide() {
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(
            unsafe { libc::geteuid() },
            0,
            "permission regression must run as the unprivileged shell user"
        );
        let (_bus, server, connection, acquired, released) = setup(false).await;
        let root = std::env::temp_dir().join(format!("hyprv-power-eacces-{}", std::process::id()));
        let energy = root.join("class/powercap/intel-rapl:0");
        std::fs::create_dir_all(&energy).unwrap();
        std::fs::write(energy.join("name"), "package-0\n").unwrap();
        std::fs::write(energy.join("max_energy_range_uj"), "100000000\n").unwrap();
        let path = energy.join("energy_uj");
        std::fs::write(&path, "12345\n").unwrap();
        // Same effective EACCES as root:root 0400 on the live host, without
        // chown/sudo or changing any real kernel file.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap_err().kind(),
            std::io::ErrorKind::PermissionDenied
        );
        let runtime = Arc::new(hyprv_core::runtime::ServiceRuntime::new().unwrap());
        let (sleeping, _) = tokio::sync::watch::channel(false);
        let ctx = crate::actor::Context {
            runtime: Arc::downgrade(&runtime),
            shutdown: runtime.shutdown_signal(),
            bus: connection,
            paths: crate::actor::Paths {
                sys: root.clone(),
                ..Default::default()
            },
            store: Arc::new(hyprv_core::store::StateStore::default()),
            sleeping: sleeping.clone(),
        };
        let handle = super::super::spawn(ctx);
        let lease = handle
            .lease(
                vec![
                    super::super::Metric::CpuPackagePower,
                    super::super::Metric::CpuTotal,
                    super::super::Metric::Memory,
                ],
                Duration::from_secs(3600),
            )
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if handle.state.snapshot().value.package_power_w == Some(12.35) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("root-only energy must not hide privileged watts");
        assert_eq!(acquired.load(Ordering::SeqCst), 1);
        tokio::time::sleep(Duration::from_millis(30)).await;
        let before = handle.state.snapshot().value;
        server
            .emit_signal(
                None::<&str>,
                PATH,
                INTERFACE,
                "CpuPowerChanged",
                &(r#"{"watts":23.45,"error":null}"#,),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            while handle.state.snapshot().value.package_power_w != Some(23.45) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let after = handle.state.snapshot().value;
        assert_eq!(
            before.cpu_history, after.cpu_history,
            "power signal must not resample CPU history"
        );
        assert_eq!(
            before.memory_history, after.memory_history,
            "power signal must not resample memory history"
        );
        assert_eq!(before.elapsed, after.elapsed);
        sleeping.send_replace(true);
        tokio::time::timeout(Duration::from_secs(2), async {
            while released.load(Ordering::SeqCst) < 1 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        sleeping.send_replace(false);
        tokio::time::timeout(Duration::from_secs(2), async {
            while acquired.load(Ordering::SeqCst) < 2 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let cpu_only = handle
            .lease(
                vec![super::super::Metric::CpuTotal],
                Duration::from_secs(3600),
            )
            .unwrap();
        drop(lease);
        tokio::time::timeout(Duration::from_secs(2), async {
            while released.load(Ordering::SeqCst) < 2 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let before = acquired.load(Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(before, acquired.load(Ordering::SeqCst));
        drop(cpu_only);
        runtime.request_shutdown();
        drop(handle);
        drop(runtime);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn cancelled_acquisition_is_balanced_after_late_reply() {
        let (_bus, _server, connection, acquired, released) = setup(true).await;
        let mut client = Client::new(connection);
        client.connect().await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(30), client.demand(true))
                .await
                .is_err()
        );
        assert_eq!(acquired.load(Ordering::SeqCst), 1);
        client.demand(false).await;
        assert_eq!(released.load(Ordering::SeqCst), 1);
        assert_eq!(client.state.watts, None);
    }
    #[tokio::test]
    async fn signals_update_watts_and_explicit_errors_without_rpc_polling() {
        let (_bus, server, connection, acquired, _released) = setup(false).await;
        let mut client = Client::new(connection);
        client.connect().await.unwrap();
        client.demand(true).await;
        for _ in 0..240 {
            server
                .emit_signal(
                    None::<&str>,
                    PATH,
                    INTERFACE,
                    "FanStateChanged",
                    &("unrelated",),
                )
                .await
                .unwrap();
        }
        server
            .emit_signal(
                None::<&str>,
                PATH,
                INTERFACE,
                "CpuPowerChanged",
                &(r#"{"watts":23.45,"error":null}"#,),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), client.event())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(client.state.watts, Some(23.45));
        server
            .emit_signal(
                None::<&str>,
                PATH,
                INTERFACE,
                "CpuPowerChanged",
                &(r#"{"watts":null,"error":"energy_uj: permission denied"}"#,),
            )
            .await
            .unwrap();
        client.event().await.unwrap();
        assert_eq!(client.state.watts, None);
        assert!(client.last_error.is_some());
        client.demand(true).await;
        assert_eq!(acquired.load(Ordering::SeqCst), 1);
        client.demand(false).await;
    }
}
