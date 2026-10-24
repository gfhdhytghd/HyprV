mod laptop;
mod package_power;
mod sample;
mod series;
use crate::actor::{self, Backend, Context, Handle, Result};
use hyprv_privhelper::{cooling::Hardware, events::KernelEvents};
use serde::{Deserialize, Serialize};
pub use series::Series;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Metric {
    CpuTotal,
    CpuCores,
    Memory,
    Network,
    Temperature,
    CpuPackagePower,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct State {
    pub cpu_percent: Option<f64>,
    pub core_percent: Series<8, false>,
    pub memory_total_bytes: u64,
    pub memory_available_bytes: u64,
    pub memory_percent: Option<f64>,
    pub temperature_c: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature_available: Option<bool>,
    pub package_power_w: Option<f64>,
    pub iface: Option<std::sync::Arc<str>>,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_bytes_per_s: Option<f64>,
    pub tx_bytes_per_s: Option<f64>,
    pub elapsed: Duration,
    pub generation: u64,
    pub cpu_history: Series<120>,
    pub memory_history: Series<120>,
    pub network_history: Series<120>,
}
// Elapsed is sample metadata, already reflected in the derived rates/power.
// Advancing it alone is not a new value; ordered graph history still compares.
impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        self.cpu_percent == other.cpu_percent
            && self.core_percent == other.core_percent
            && self.memory_total_bytes == other.memory_total_bytes
            && self.memory_available_bytes == other.memory_available_bytes
            && self.memory_percent == other.memory_percent
            && self.temperature_c == other.temperature_c
            && self.temperature_available == other.temperature_available
            && self.package_power_w == other.package_power_w
            && self.iface == other.iface
            && self.rx_bytes == other.rx_bytes
            && self.tx_bytes == other.tx_bytes
            && self.rx_bytes_per_s == other.rx_bytes_per_s
            && self.tx_bytes_per_s == other.tx_bytes_per_s
            && self.generation == other.generation
            && self.cpu_history == other.cpu_history
            && self.memory_history == other.memory_history
            && self.network_history == other.network_history
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Action {
    Refresh,
    ResetBaseline,
    Metrics(Vec<Metric>),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cpu {
    pub total: u64,
    pub idle: u64,
}
pub fn parse_cpu(text: &str) -> BTreeMap<String, Cpu> {
    text.lines()
        .filter_map(|line| {
            let mut f = line.split_whitespace();
            let name = f.next()?;
            if name != "cpu"
                && !name
                    .strip_prefix("cpu")
                    .is_some_and(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
            {
                return None;
            }
            let v: Vec<u64> = f
                .map(str::parse)
                .collect::<std::result::Result<_, _>>()
                .ok()?;
            if v.len() < 4 {
                return None;
            }
            Some((
                name.into(),
                Cpu {
                    // guest/guest_nice (columns 9/10) are already included in
                    // user/nice. Only user through steal contribute to total.
                    total: v.iter().take(8).try_fold(0u64, |a, b| a.checked_add(*b))?,
                    idle: v[3].checked_add(*v.get(4).unwrap_or(&0))?,
                },
            ))
        })
        .collect()
}
pub fn cpu_delta(before: Cpu, after: Cpu) -> Option<f64> {
    let total = after.total.checked_sub(before.total)?;
    let idle = after.idle.checked_sub(before.idle)?;
    if total == 0 || idle > total {
        None
    } else {
        Some((1. - idle as f64 / total as f64) * 100.)
    }
}
pub fn memory(text: &str) -> Option<(u64, u64)> {
    let mut values = [None; 5];
    for line in text.lines() {
        let Some((key, raw)) = line.split_once(':') else {
            continue;
        };
        let Some(i) = ["MemTotal", "MemAvailable", "MemFree", "Buffers", "Cached"]
            .iter()
            .position(|k| *k == key)
        else {
            continue;
        };
        if let Some(n) = raw
            .split_whitespace()
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .and_then(|n| n.checked_mul(1024))
        {
            values[i] = Some(n);
        }
    }
    let total = values[0]?;
    let available =
        values[1].or_else(|| values[2]?.checked_add(values[3]?)?.checked_add(values[4]?))?;
    (total > 0).then_some((total, available.min(total)))
}
pub fn default_iface(text: &str) -> Option<String> {
    sample::route(text).map(str::to_owned)
}
pub fn network(text: &str, iface: &str) -> Option<(u64, u64)> {
    text.lines().find_map(|line| {
        let (name, values) = line.split_once(':')?;
        if name.trim() != iface {
            return None;
        }
        let mut fields = values.split_ascii_whitespace();
        let rx = fields.next()?.parse().ok()?;
        let tx = fields.nth(7)?.parse().ok()?;
        Some((rx, tx))
    })
}
pub fn rapl_power(before: u64, after: u64, range: u64, elapsed: f64) -> Option<f64> {
    if range == 0 || before >= range || after >= range || elapsed <= 0. {
        return None;
    }
    let energy = if after >= before {
        after - before
    } else {
        range - before + after
    };
    Some(energy as f64 / 1e6 / elapsed)
}
#[derive(Clone)]
pub struct ServiceHandle {
    pub state: hyprv_core::store::Topic<State>,
    pub error: hyprv_core::store::Topic<Option<String>>,
    inner: Handle<State, Action>,
    registry: std::sync::Arc<Registry>,
}
struct Registry {
    entries: std::sync::Mutex<BTreeMap<u64, (Vec<Metric>, Duration)>>,
    notify: tokio::sync::watch::Sender<u64>,
    serial: std::sync::atomic::AtomicU64,
    demand: tokio::sync::watch::Sender<usize>,
    profile: crate::Profile,
}
pub struct SamplingLease {
    pub subscription: hyprv_core::store::Subscription<State>,
    registry: std::sync::Arc<Registry>,
    id: u64,
}
impl Drop for SamplingLease {
    fn drop(&mut self) {
        let mut entries = self.registry.entries.lock().unwrap();
        entries.remove(&self.id);
        self.registry.demand.send_replace(entries.len());
        drop(entries);
        self.registry.notify.send_modify(|n| *n += 1);
    }
}
fn all_metrics() -> Vec<Metric> {
    vec![
        Metric::CpuTotal,
        Metric::CpuCores,
        Metric::Memory,
        Metric::Network,
        Metric::Temperature,
        Metric::CpuPackagePower,
    ]
}
impl ServiceHandle {
    /// The core subscription owns activation; this wrapper only negotiates metrics/cadence.
    pub fn lease(&self, metrics: Vec<Metric>, period: Duration) -> Result<SamplingLease> {
        if metrics.is_empty()
            || period < Duration::from_millis(100)
            || period > Duration::from_secs(3600)
        {
            return Err("lease requires metrics and 100ms..3600s period".into());
        }
        let id = self
            .registry
            .serial
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        {
            let mut entries = self.registry.entries.lock().unwrap();
            if entries.len() >= 128 {
                return Err("too many sampling leases".into());
            }
            entries.insert(id, (metrics, period));
            self.registry.demand.send_replace(entries.len());
        }
        self.registry.notify.send_modify(|n| *n += 1);
        Ok(SamplingLease {
            subscription: self.state.subscribe(),
            registry: self.registry.clone(),
            id,
        })
    }
    /// Compact strip's real painted visibility owns this guard. None means no
    /// timer even if state/error observers remain subscribed.
    pub fn laptop_strip(&self, painted: bool) -> Result<Option<SamplingLease>> {
        if self.registry.profile != crate::Profile::Laptop {
            return Err("laptop_strip requires laptop policy".into());
        }
        painted
            .then(|| {
                self.lease(
                    vec![Metric::CpuTotal, Metric::Memory],
                    Duration::from_secs(5),
                )
            })
            .transpose()
    }
    pub fn subscribe(&self) -> SamplingLease {
        self.lease(
            if self.registry.profile == crate::Profile::Laptop {
                vec![Metric::CpuTotal, Metric::Memory]
            } else {
                all_metrics()
            },
            if self.registry.profile == crate::Profile::Laptop {
                Duration::from_secs(5)
            } else {
                Duration::from_secs(1)
            },
        )
        .expect("default lease capacity")
    }
    pub async fn action(&self, action: Action) -> Result<()> {
        self.inner.action(action).await
    }
}
pub fn spawn(ctx: Context) -> ServiceHandle {
    spawn_for_profile(ctx, hyprv_core::profile::profile())
}
pub fn spawn_for_profile(ctx: Context, profile: crate::Profile) -> ServiceHandle {
    let (notify, changes) = tokio::sync::watch::channel(0);
    let (demand, authority) = tokio::sync::watch::channel(0);
    let registry = std::sync::Arc::new(Registry {
        demand,
        profile,
        entries: std::sync::Mutex::new(BTreeMap::new()),
        notify,
        serial: std::sync::atomic::AtomicU64::new(1),
    });
    let native = Native {
        profile,
        laptop: (profile == crate::Profile::Laptop)
            .then(|| laptop::Sampler::new(ctx.paths.clone())),
        ctx: ctx.clone(),
        sampler: sample::Reader::new(&ctx.paths.proc).with_sys(&ctx.paths.sys),
        hardware: Hardware::default(),
        thermal: Vec::new(),
        power: package_power::Client::for_profile(ctx.bus.clone(), profile),
        missing_temperature: false,
        missing_network: false,
        sample_error: None,
        power_event: false,
        udev: None,
        routes: None,
        state: State::default(),
        metrics: all_metrics(),
        registry: registry.clone(),
        changes,
        cadence: None,
        core_demand: 0,
    };
    let inner = actor::spawn_controlled(
        ctx,
        native,
        true,
        (profile == crate::Profile::Laptop).then_some(authority),
    );
    ServiceHandle {
        state: inner.state.clone(),
        error: inner.error.clone(),
        inner,
        registry,
    }
}

struct Native {
    profile: crate::Profile,
    laptop: Option<laptop::Sampler>,
    ctx: Context,
    sampler: sample::Reader,
    hardware: Hardware,
    thermal: Vec<PathBuf>,
    power: package_power::Client,
    missing_temperature: bool,
    missing_network: bool,
    sample_error: Option<String>,
    power_event: bool,
    udev: Option<KernelEvents>,
    routes: Option<KernelEvents>,
    state: State,
    metrics: Vec<Metric>,
    registry: std::sync::Arc<Registry>,
    changes: tokio::sync::watch::Receiver<u64>,
    cadence: Option<tokio::sync::watch::Sender<Duration>>,
    core_demand: usize,
}
fn kernel_demand(profile: crate::Profile, count: usize, metrics: &[Metric]) -> (bool, bool) {
    if profile == crate::Profile::Desktop {
        return (true, true);
    }
    (
        count > 0 && metrics.contains(&Metric::Temperature),
        count > 0 && metrics.contains(&Metric::Network),
    )
}
async fn kernel_event(events: &Option<KernelEvents>, subsystems: Option<&[&str]>) -> Result<()> {
    let Some(events) = events else {
        return std::future::pending().await;
    };
    if let Some(subsystems) = subsystems {
        events.subsystem(subsystems).await?;
    } else {
        events.next().await?;
    }
    Ok(())
}
impl Native {
    fn sync_kernel_watches(&mut self) -> Result<()> {
        if self.ctx.paths.isolated_sysfs() {
            self.udev = None;
            self.routes = None;
            return Ok(());
        }
        let (thermal, network) = kernel_demand(self.profile, self.core_demand, &self.metrics);
        if thermal {
            if self.udev.is_none() {
                self.udev = Some(KernelEvents::udev()?);
            }
        } else {
            self.udev = None;
        }
        if network {
            if self.routes.is_none() {
                self.routes = Some(KernelEvents::routes()?);
            }
        } else {
            self.routes = None;
        }
        Ok(())
    }
    fn negotiate(&mut self) {
        let entries = self.registry.entries.lock().unwrap();
        let mut metrics = std::collections::BTreeSet::new();
        let mut period = Duration::from_secs(3600);
        let mut plan = BTreeMap::<Metric, Duration>::new();
        for (requested, cadence) in entries.values() {
            for metric in requested {
                plan.entry(*metric)
                    .and_modify(|p| *p = (*p).min(*cadence))
                    .or_insert(*cadence);
            }
            metrics.extend(requested.iter().copied());
            period = period.min(*cadence);
        }
        if self.profile == crate::Profile::Desktop
            && (self.core_demand > entries.len() || entries.is_empty())
        {
            metrics.extend(all_metrics());
            period = period.min(Duration::from_secs(1));
        }
        drop(entries);
        let previous = std::mem::replace(&mut self.metrics, metrics.into_iter().collect());
        if let Some(laptop) = &mut self.laptop {
            laptop.plan(plan);
            if self.metrics.contains(&Metric::Temperature)
                && !previous.contains(&Metric::Temperature)
            {
                laptop.discover();
            }
        }
        if let Some(cadence) = &self.cadence {
            if *cadence.borrow() != period {
                cadence.send_replace(period);
            }
        }
    }
    fn discover(&mut self) {
        if let Some(laptop) = &mut self.laptop {
            laptop.discover();
            return;
        }
        let hwmon = self.ctx.paths.sys.join("class/hwmon");
        let safe = !self.ctx.paths.isolated_sysfs()
            || self.ctx.paths.guard_sysfs(&hwmon).is_ok()
                && actor::entries(&hwmon).iter().all(|entry| {
                    self.ctx.paths.guard_sysfs(entry).is_ok()
                        && actor::entries(entry)
                            .iter()
                            .all(|field| self.ctx.paths.guard_sysfs(field).is_ok())
                });
        self.hardware = if safe {
            Hardware::discover(&hwmon)
        } else {
            Hardware::default()
        };
        self.thermal.clear();
        for preferred in [
            "x86_pkg_temp",
            "TCPU",
            "cpu_thermal",
            "cpu-thermal",
            "soc_thermal",
        ] {
            for zone in actor::sysfs_entries(
                &self.ctx.paths.sys,
                self.ctx.paths.sys.join("class/thermal"),
            ) {
                if actor::sysfs_text(&self.ctx.paths.sys, zone.join("type"))
                    .is_ok_and(|x| x.trim() == preferred)
                {
                    self.thermal.push(zone.join("temp"));
                }
            }
        }
        use hyprv_core::sample_file::SampleFile;
        self.sampler.thermal = self
            .thermal
            .iter()
            .filter_map(|p| {
                self.ctx
                    .paths
                    .guard_sysfs(p)
                    .ok()
                    .map(|p| SampleFile::new(p, 64))
            })
            .collect();
        self.sampler.hwmon = self
            .hardware
            .temperatures
            .iter()
            .filter_map(|p| {
                self.ctx
                    .paths
                    .guard_sysfs(p)
                    .ok()
                    .map(|p| SampleFile::new(p, 64))
            })
            .collect();
    }
}
impl Backend for Native {
    fn observe_on_demand(&self) -> bool {
        self.profile == crate::Profile::Laptop
    }
    fn initial_snapshot(&self) -> bool {
        self.profile == crate::Profile::Desktop
    }
    fn unobserve(&mut self) {
        self.udev = None;
        self.routes = None;
        if self.profile == crate::Profile::Laptop {
            self.power.suspend();
        }
        self.reset();
    }
    fn bind(&mut self, period: tokio::sync::watch::Sender<Duration>) {
        self.cadence = Some(period);
    }
    fn demand_changed(&mut self, count: usize) {
        self.core_demand = count;
        self.negotiate();
    }
    type State = State;
    type Action = Action;
    fn providers(&self) -> &'static [&'static str] {
        if self.profile == crate::Profile::Desktop {
            &[hyprv_privhelper::BUS]
        } else {
            &[hyprv_privhelper::cpu_power::BUS]
        }
    }
    async fn connect(&mut self) -> Result<()> {
        self.sync_kernel_watches()?;
        self.discover();
        if self.profile == crate::Profile::Desktop
            || self.metrics.contains(&Metric::CpuPackagePower)
        {
            self.power.connect().await?;
        }
        Ok(())
    }
    async fn snapshot(&mut self) -> Result<State> {
        self.power
            .demand(
                self.core_demand > 0
                    && !*self.ctx.sleeping.borrow()
                    && self.metrics.contains(&Metric::CpuPackagePower),
            )
            .await;
        self.state.package_power_w = if self.metrics.contains(&Metric::CpuPackagePower) {
            self.power.state.watts
        } else {
            None
        };
        // A helper signal changes only power, not the local 1-Hz CPU/memory/
        // network histories. Do not turn two independently phased clocks into
        // a 2-Hz sampler or overwrite their baselines with tiny deltas.
        if std::mem::take(&mut self.power_event) {
            return Ok(self.state.clone());
        }
        let result = if let Some(laptop) = &mut self.laptop {
            laptop.sample(&mut self.state, tokio::time::Instant::now().into_std())
        } else {
            self.sampler
                .sample(&mut self.state, &self.metrics, Instant::now())
        };
        let error = result.as_ref().err().map(ToString::to_string);
        if error != self.sample_error {
            if let Some(e) = &error {
                eprintln!("sysstats sample unavailable: {e}");
            }
            self.sample_error = error;
        }
        result?;
        self.state.package_power_w = if self.metrics.contains(&Metric::CpuPackagePower) {
            self.power.state.watts
        } else {
            None
        };
        let missing = self.metrics.contains(&Metric::Temperature)
            && (self.state.temperature_c.is_none()
                || self.state.temperature_available == Some(false));
        if missing && !self.missing_temperature {
            eprintln!("sysstats CPU temperature unavailable: no readable valid thermal/hwmon source under {}",self.ctx.paths.sys.display());
        }
        self.missing_temperature = missing;
        let missing = self.metrics.contains(&Metric::Network) && self.state.iface.is_none();
        if missing && !self.missing_network {
            eprintln!("sysstats network unavailable: no readable UP IPv4 default-route counters");
        }
        self.missing_network = missing;
        Ok(self.state.clone())
    }
    async fn apply(&mut self, a: Action) -> Result<()> {
        match a {
            Action::Refresh => (),
            Action::ResetBaseline => self.reset(),
            Action::Metrics(metrics) => {
                if self.profile == crate::Profile::Laptop {
                    return Err("laptop metrics require an explicit sampling lease".into());
                }
                self.metrics = metrics;
                self.reset();
            }
        }
        Ok(())
    }
    async fn event(&mut self) -> Result<()> {
        self.sync_kernel_watches()?;
        self.power
            .demand(
                self.core_demand > 0
                    && !*self.ctx.sleeping.borrow()
                    && self.metrics.contains(&Metric::CpuPackagePower),
            )
            .await;
        let mut sleeping = self.ctx.sleeping.subscribe();
        tokio::select! {
        _=self.changes.changed()=>{self.negotiate();Ok(())},_=sleeping.changed()=>Ok(()),r=self.power.event()=>{r?;self.power_event=true;Ok(())},r=kernel_event(&self.udev,Some(&["hwmon","thermal","powercap"]))=>{r?;self.discover();self.reset();Ok(())},r=kernel_event(&self.routes,None)=>{r?;self.sampler.reset_network();if let Some(laptop)=&mut self.laptop{laptop.reset_network();}Ok(())}}
    }
    fn reset(&mut self) {
        self.power_event = false;
        self.sampler.invalidate();
        if let Some(laptop) = &mut self.laptop {
            laptop.reset();
        }
        self.state.generation += 1;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compact_laptop_does_not_subscribe_to_hidden_kernel_sources() {
        assert_eq!(
            kernel_demand(
                crate::Profile::Laptop,
                1,
                &[Metric::CpuTotal, Metric::Memory]
            ),
            (false, false)
        );
        assert_eq!(
            kernel_demand(
                crate::Profile::Laptop,
                0,
                &[Metric::Network, Metric::Temperature]
            ),
            (false, false)
        );
        assert_eq!(
            kernel_demand(crate::Profile::Laptop, 1, &[Metric::Network]),
            (false, true)
        );
        assert_eq!(
            kernel_demand(crate::Profile::Laptop, 1, &[Metric::Temperature]),
            (true, false)
        );
        assert_eq!(kernel_demand(crate::Profile::Desktop, 0, &[]), (true, true));
    }
    #[test]
    fn resets_and_wrap() {
        assert_eq!(
            cpu_delta(
                Cpu {
                    total: 100,
                    idle: 50
                },
                Cpu {
                    total: 200,
                    idle: 100
                }
            ),
            Some(50.)
        );
        assert_eq!(
            cpu_delta(
                Cpu {
                    total: 200,
                    idle: 100
                },
                Cpu {
                    total: 100,
                    idle: 50
                }
            ),
            None
        );
        assert_eq!(rapl_power(900000, 100000, 1000000, 1.), Some(0.2));
        assert_eq!(
            memory("MemTotal: 1000 kB\nMemFree: 100 kB\nBuffers: 20 kB\nCached: 30 kB"),
            Some((1024000, 153600))
        );
    }
    #[test]
    fn network_fixture() {
        assert_eq!(
            default_iface("Iface Destination Gateway Flags\neth0 00000000 01010101 0003"),
            Some("eth0".into())
        );
        assert_eq!(network("eth0: 1 0 0 0 0 0 0 0 2 0", "eth0"), Some((1, 2)));
    }
}

#[cfg(test)]
mod adversarial_review {
    use super::*;
    #[test]
    fn review_power_guest_time_is_not_counted_twice() {
        let before = parse_cpu("cpu 0 0 0 0 0 0 0 0 0 0");
        let after = parse_cpu("cpu 100 0 0 100 0 0 0 0 100 0");
        assert!(
            (cpu_delta(before["cpu"], after["cpu"]).unwrap() - 50.).abs() < 0.001,
            "guest time already included in user time"
        );
    }
}

#[cfg(test)]
mod laptop_actor_tests {
    use super::*;
    use crate::test_support::{context, settle, Bus, Tree};
    #[test]
    fn passive_observers_do_not_sample_and_compact_lease_has_no_root_or_hidden_metrics() {
        let rt = std::sync::Arc::new(hyprv_core::runtime::ServiceRuntime::new().unwrap());
        rt.block_on(async {
            let bus = Bus::new();
            let tree = Tree::new();
            tree.write("proc/stat", "cpu 100 0 0 100 0 0 0 0 0 0\n");
            tree.write("proc/meminfo", "MemTotal: 1000 kB\nMemAvailable: 500 kB\n");
            let handle = spawn_for_profile(
                context(bus.connection().await, tree.paths(), Some(&rt)),
                crate::Profile::Laptop,
            );
            let _observer = handle.state.subscribe();
            handle.action(Action::Refresh).await.unwrap();
            settle().await;
            assert!(handle.state.snapshot().value.cpu_history.is_empty());
            assert_eq!(handle.inner.period.borrow().as_secs(), 3600);

            let mut lease = handle.laptop_strip(true).unwrap().unwrap();
            tokio::time::timeout(Duration::from_secs(2), lease.subscription.changed())
                .await
                .unwrap()
                .unwrap();
            let state = lease.subscription.current().value;
            assert_eq!(handle.inner.period.borrow().as_secs(), 5);
            assert_eq!(state.cpu_history.len(), 1);
            assert!(state.core_percent.is_empty());
            assert!(state.network_history.is_empty());
            assert_eq!(state.temperature_c, None);
            assert_eq!(state.package_power_w, None);
            drop(lease);
            settle().await;
            assert_eq!(*handle.registry.demand.borrow(), 0);
            assert!(handle.registry.entries.lock().unwrap().is_empty());
            assert!(handle.laptop_strip(false).unwrap().is_none());
            assert!(!zbus::fdo::DBusProxy::new(&bus.connection().await)
                .await
                .unwrap()
                .name_has_owner(hyprv_privhelper::BUS.try_into().unwrap())
                .await
                .unwrap());
            rt.request_shutdown();
            settle().await;
        });
    }
}
