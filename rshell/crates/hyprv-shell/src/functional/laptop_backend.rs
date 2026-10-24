//! Real native power actors against a per-case bus/sysfs fixture, never the host.
use super::*;
use hyprv_svc_power as power;
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::Arc,
};
pub(super) struct Backend {
    daemon: Child,
    fake: Child,
    pub runtime: Arc<hyprv_core::runtime::ServiceRuntime>,
    pub ctx: power::Context,
    pub power: power::Handle<power::power::State, power::power::Action>,
    _power_subscription: hyprv_core::store::Subscription<power::power::State>,
    root: std::path::PathBuf,
    log: std::path::PathBuf,
}
impl Backend {
    pub fn new(s: &Suite, id: &str) -> Result<Self, Box<dyn Error>> {
        let root = s.directory.join(format!("native-{id}"));
        std::fs::create_dir_all(&root)?;
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../parity/laptop/fakes");
        let scenario = script.join("scenarios/discharging-57.json");
        let status = Command::new("python3")
            .arg("-I")
            .arg(script.join("make-sysfs.py"))
            .arg(&scenario)
            .arg(root.join("sys"))
            .stdout(Stdio::null())
            .status()?;
        check(status.success(), "fixture generation")?;
        let mut daemon = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()?;
        let mut address = String::new();
        BufReader::new(daemon.stdout.take().ok_or("bus stdout")?).read_line(&mut address)?;
        let log = root.join("bus.ndjson");
        let mut fake = Command::new("python3")
            .arg("-I")
            .arg(script.join("fake-system-bus.py"))
            .arg("--address")
            .arg(address.trim())
            .arg("--scenario")
            .arg(&scenario)
            .arg("--log")
            .arg(&log)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let mut ready = String::new();
        BufReader::new(fake.stdout.take().ok_or("fake stdout")?).read_line(&mut ready)?;
        check(ready.starts_with("ready"), "fake bus readiness")?;
        let runtime = Arc::new(hyprv_core::runtime::ServiceRuntime::new()?);
        let bus = runtime.block_on(async {
            zbus::connection::Builder::address(address.trim())?
                .build()
                .await
        })?;
        let ctx = power::Context {
            runtime: Arc::downgrade(&runtime),
            shutdown: runtime.shutdown_signal(),
            bus,
            paths: power::Paths {
                sys: root.join("sys"),
                proc: root.join("proc"),
                state: root.join("state"),
                runtime: root.join("run"),
            },
            store: Arc::new(hyprv_core::store::StateStore::default()),
            sleeping: hyprv_core::tokio::sync::watch::channel(false).0,
        };
        let power = power::power::spawn_for_profile(ctx.clone(), power::Profile::Laptop);
        let power_subscription = power.subscribe();
        let mut result = Self {
            daemon,
            fake,
            runtime,
            ctx,
            power,
            _power_subscription: power_subscription,
            root,
            log,
        };
        result.wait(|p| p.available && p.battery_present)?;
        Ok(result)
    }
    pub fn step(&mut self, v: Value) -> Result<(), Box<dyn Error>> {
        let input = self.fake.stdin.as_mut().ok_or("fake stdin")?;
        writeln!(input, "{v}")?;
        input.flush()?;
        Ok(())
    }
    pub fn wait(
        &mut self,
        p: impl Fn(&power::power::State) -> bool,
    ) -> Result<power::power::State, Box<dyn Error>> {
        let deadline = std::time::Instant::now() + Duration::from_secs(4);
        loop {
            let state = self.power.state.snapshot().value;
            if p(&state) {
                return Ok(state);
            };
            if std::time::Instant::now() > deadline {
                return Err(format!("native power state timed out: {state:?}").into());
            };
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    pub fn profile_set(&mut self) -> Result<Value, Box<dyn Error>> {
        self.runtime
            .block_on(self.power.action(power::power::Action::ProfileSet(
                power::power::PowerProfile::PowerSaver,
            )))
            .map_err(|e| e.to_string())?;
        let state = self.wait(|p| p.profile == power::power::PowerProfile::PowerSaver)?;
        let lines = std::fs::read_to_string(&self.log)?;
        let writes: Vec<Value> = lines
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter(|r| r["event"] == "set" && r["member"] == "ActiveProfile")
            .collect();
        check(writes.len() == 1, "duplicate/missing PPD write")?;
        Ok(json!({"state":state,"writes":writes}))
    }
    pub fn no_egpu(&self) -> Result<Value, Box<dyn Error>> {
        let mut ctx=hyprv_svc_desktop::Ctx::new(self.ctx.store.clone(),self.runtime.enter(||hyprv_core::tokio::runtime::Handle::current()),self.runtime.shutdown_signal());
        ctx.env=Arc::new(hyprv_svc_desktop::Env::rooted(&self.root.join("desktop")));
        let graph=crate::services::spawn_desktop(&ctx,power::inhibit::spawn(self.ctx.clone()));
        let state=graph.egpu.state();
        check(state.devices.is_empty(),"laptop eGPU inventory was populated")?;
        let result=self.runtime.block_on(graph.egpu.request(hyprv_svc_desktop::egpu::EgpuAction::Remove(None)));
        check(result.is_err(),"inactive eGPU command was accepted")?;
        Ok(json!({"egpu":state,"inactive_channel_rejected_action":true,"ui":"active laptop CP has no eGPU card; unavailable state only"}))
    }
    pub fn thermal(&self) -> Result<Value, Box<dyn Error>> {
        let stats = power::sysstats::spawn_for_profile(self.ctx.clone(), power::Profile::Laptop);
        let guard = stats
            .lease(
                vec![power::sysstats::Metric::Temperature],
                Duration::from_millis(100),
            )
            .map_err(|e| e.to_string())?;
        let wait = |want: Option<f64>,
                    available: bool|
         -> Result<power::sysstats::State, Box<dyn Error>> {
            let until = std::time::Instant::now() + Duration::from_secs(3);
            loop {
                let v = stats.state.snapshot().value;
                if v.temperature_available == Some(available)
                    && (!available || v.temperature_c == want)
                {
                    return Ok(v);
                };
                if std::time::Instant::now() > until {
                    return Err(format!("thermal source: {v:?}").into());
                };
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        let pkg = wait(Some(45.), true)?;
        std::fs::remove_dir_all(self.ctx.paths.sys.join("class/thermal"))?;
        std::fs::write(
            self.ctx.paths.sys.join("class/hwmon/hwmon0/temp1_input"),
            "47000\n",
        )?;
        let hwmon = wait(Some(47.), true)?;
        std::fs::remove_dir_all(self.ctx.paths.sys.join("class/hwmon"))?;
        let none = wait(None, false)?;
        drop(guard);
        Ok(json!({"pkg":pkg,"hwmon":hwmon,"none":none}))
    }
    pub fn inert_fan(&self) -> Result<Value, Box<dyn Error>> {
        let fan = power::fan::spawn_for_profile(self.ctx.clone(), power::Profile::Laptop);
        let _subscription = fan.subscribe();
        std::thread::sleep(Duration::from_millis(150));
        let state = fan.state.snapshot().value;
        check(!state.available, "laptop fan available")?;
        let names = self.runtime.block_on(async {
            let dbus = zbus::fdo::DBusProxy::new(&self.ctx.bus).await?;
            dbus.list_names().await
        })?;
        check(
            !names.iter().any(|n| n.as_str().contains("hyprv.Priv")),
            "helper name activated",
        )?;
        Ok(
            json!({"fan":state,"bus_names":names.iter().map(|n|n.as_str()).collect::<Vec<_>>(),"sysfs":self.root.join("sys")}),
        )
    }
}
impl Drop for Backend {
    fn drop(&mut self) {
        self.runtime.request_shutdown();
        let _ = self.fake.kill();
        let _ = self.fake.wait();
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}
