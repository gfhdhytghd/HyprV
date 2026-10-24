//! Demand-scoped, read-only package energy telemetry. No caller-controlled paths.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::watch;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub watts: Option<f64>,
    pub error: Option<String>,
}
#[derive(Default)]
struct Inner {
    subscribers: BTreeSet<String>,
    state: State,
}
#[derive(Clone)]
pub struct Telemetry {
    inner: Arc<Mutex<Inner>>,
    demand: watch::Sender<(bool, u64)>,
}
impl Default for Telemetry {
    fn default() -> Self {
        Self {
            inner: Arc::default(),
            demand: watch::channel((false, 0)).0,
        }
    }
}
impl Telemetry {
    pub fn subscribe(&self, sender: &str) -> State {
        let mut inner = self.inner.lock().unwrap();
        if inner.subscribers.is_empty() {
            inner.state = State::default();
        }
        inner.subscribers.insert(sender.into());
        self.demand.send_if_modified(|active| {
            if !active.0 {
                *active = (true, active.1.wrapping_add(1));
                true
            } else {
                false
            }
        });
        inner.state.clone()
    }
    pub fn unsubscribe(&self, sender: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.subscribers.remove(sender);
        if inner.subscribers.is_empty() {
            inner.state = State::default();
            self.demand.send_if_modified(|active| {
                if active.0 {
                    *active = (false, active.1.wrapping_add(1));
                    true
                } else {
                    false
                }
            });
        }
    }
    pub fn clear(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.subscribers.clear();
        inner.state = State::default();
        self.demand
            .send_modify(|d| *d = (false, d.1.wrapping_add(1)));
    }
    pub fn demand(&self) -> watch::Receiver<(bool, u64)> {
        self.demand.subscribe()
    }
    pub fn publish(&self, state: State) -> bool {
        let mut inner = self.inner.lock().unwrap();
        if inner.subscribers.is_empty() {
            return false;
        }
        inner.state = state;
        true
    }
}
struct Package {
    energy: PathBuf,
    range: u64,
    before: Option<(u64, Instant)>,
}
pub struct Sampler {
    root: PathBuf,
    packages: Vec<Package>,
}
fn integer(path: &Path) -> io::Result<u64> {
    std::fs::read_to_string(path)
        .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", path.display())))?
        .trim()
        .parse()
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}: {e}", path.display()),
            )
        })
}
pub fn power(before: u64, after: u64, range: u64, elapsed: Duration) -> Option<f64> {
    // A suspended/stalled interval can span multiple wraps. Rebaseline instead
    // of publishing a plausible but incorrect number; normal cadence is 1 Hz.
    if range == 0
        || before >= range
        || after >= range
        || elapsed.is_zero()
        || elapsed > Duration::from_secs(5)
    {
        return None;
    }
    let delta = if after >= before {
        after - before
    } else {
        range - before + after
    };
    Some(delta as f64 / 1_000_000. / elapsed.as_secs_f64())
}
impl Sampler {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            packages: Vec::new(),
        }
    }
    fn discover(&mut self) -> io::Result<()> {
        let mut paths = std::fs::read_dir(&self.root)?
            .map(|e| e.map(|e| e.path()))
            .collect::<io::Result<Vec<_>>>()?;
        // MSR and MMIO drivers may export the SAME physical package. Prefer
        // the MSR domain (turbostat source); never add both package-0 entries.
        paths.sort_by_key(|p| {
            (
                !p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("intel-rapl:")),
                p.clone(),
            )
        });
        let mut seen = BTreeSet::new();
        let mut packages = Vec::new();
        for path in paths {
            // Only top-level package domains, never core/uncore/DRAM children.
            let Ok(name) = std::fs::read_to_string(path.join("name")) else {
                continue;
            };
            if !name
                .trim()
                .strip_prefix("package-")
                .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            {
                continue;
            }
            if !seen.insert(name.trim().to_owned()) {
                continue;
            }
            let range = integer(&path.join("max_energy_range_uj"))?;
            if range == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "RAPL range is zero",
                ));
            }
            packages.push(Package {
                energy: path.join("energy_uj"),
                range,
                before: None,
            });
        }
        if packages.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "no package RAPL domains",
            ));
        }
        self.packages = packages;
        Ok(())
    }
    pub fn sample(&mut self, now: Instant) -> io::Result<Option<f64>> {
        if self.packages.is_empty() {
            self.discover()?;
        }
        let result = (|| {
            let mut total = Some(0.);
            for p in &mut self.packages {
                let value = integer(&p.energy)?;
                if value >= p.range {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "RAPL energy outside range",
                    ));
                }
                let watts = p.before.and_then(|(before, at)| {
                    power(before, value, p.range, now.saturating_duration_since(at))
                });
                p.before = Some((value, now));
                total = total.zip(watts).map(|(a, b)| a + b);
            }
            // turbostat's PkgWatt column has two fractional digits. Preserve
            // that source precision before the existing UI's .1/.0 formatter.
            Ok(total.map(|w| (w * 100.).round() / 100.))
        })();
        if result.is_err() {
            self.packages.clear();
        }
        result
    }
}
/// No interval/timer/read exists without demand. Dropping this future (bus
/// disconnect/shutdown) also destroys the energy baseline and timer.
pub async fn run(telemetry: Telemetry, root: PathBuf, updates: watch::Sender<State>) {
    let mut demand = telemetry.demand();
    loop {
        while !demand.borrow_and_update().0 {
            if demand.changed().await.is_err() {
                return;
            }
        }
        let epoch = demand.borrow().1;
        let mut sampler = Sampler::new(root.clone());
        let mut last_error = None;
        loop {
            if *demand.borrow_and_update() != (true, epoch) {
                break;
            }
            let state = match sampler.sample(Instant::now()) {
                Ok(watts) => State { watts, error: None },
                Err(e) => State {
                    watts: None,
                    error: Some(format!("CPU package power unavailable: {e}")),
                },
            };
            if state.error != last_error {
                if let Some(error) = &state.error {
                    eprintln!("{error}");
                } else if last_error.is_some() {
                    eprintln!("CPU package power source recovered");
                }
                last_error = state.error.clone();
            }
            if telemetry.publish(state.clone()) {
                updates.send_replace(state);
            }
            tokio::select! {
                _=tokio::time::sleep(Duration::from_secs(1))=>(),
                result=demand.changed()=>if result.is_err() {return;},
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wraps_and_rejects_stale_or_invalid_samples() {
        assert_eq!(
            power(900000, 100000, 1000000, Duration::from_secs(1)),
            Some(0.2)
        );
        assert_eq!(power(1, 2, 0, Duration::from_secs(1)), None);
        assert_eq!(power(1, 2, 10, Duration::ZERO), None);
        assert_eq!(power(1, 2, 10, Duration::from_secs(6)), None);
        assert_eq!(power(10, 2, 10, Duration::from_secs(1)), None);
    }
    #[test]
    fn subscriptions_are_idempotent_owner_scoped_and_reset() {
        let t = Telemetry::default();
        let d = t.demand();
        t.subscribe(":1.1");
        t.subscribe(":1.1");
        t.subscribe(":1.2");
        t.unsubscribe(":1.1");
        assert!(d.borrow().0);
        t.unsubscribe(":1.2");
        assert!(!d.borrow().0);
        assert!(!t.publish(State {
            watts: Some(12.),
            error: None
        }));
        assert_eq!(t.subscribe(":1.3"), State::default());
        t.clear();
        assert!(!d.borrow().0);
    }
    #[tokio::test]
    async fn no_demand_means_no_io_or_timer_and_drop_stops_sampling() {
        let t = Telemetry::default();
        let (tx, mut rx) = watch::channel(State::default());
        let job = tokio::spawn(run(t.clone(), "/definitely-absent-rapl".into(), tx));
        assert!(
            tokio::time::timeout(Duration::from_millis(40), rx.changed())
                .await
                .is_err()
        );
        t.subscribe(":1.1");
        rx.changed().await.unwrap();
        assert!(rx
            .borrow_and_update()
            .error
            .as_ref()
            .unwrap()
            .contains("unavailable"));
        t.unsubscribe(":1.1");
        tokio::task::yield_now().await;
        assert!(
            tokio::time::timeout(Duration::from_millis(1100), rx.changed())
                .await
                .is_err()
        );
        job.abort();
    }
    #[test]
    fn package_only_rounding_wrap_and_read_error_rebaseline() {
        let root = std::env::temp_dir().join(format!("hyprv-rapl-{}", std::process::id()));
        let p = root.join("intel-rapl:0");
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join("name"), "package-0\n").unwrap();
        std::fs::write(p.join("max_energy_range_uj"), "100000000\n").unwrap();
        std::fs::write(p.join("energy_uj"), "99000000\n").unwrap();
        let duplicate = root.join("intel-rapl-mmio:0");
        std::fs::create_dir_all(&duplicate).unwrap();
        std::fs::write(duplicate.join("name"), "package-0\n").unwrap();
        // This duplicate would fail discovery if we didn't prefer/de-duplicate
        // the MSR package. The live host exports both names for package-0.
        std::fs::write(duplicate.join("max_energy_range_uj"), "0\n").unwrap();
        let at = Instant::now();
        let mut s = Sampler::new(root.clone());
        assert_eq!(s.sample(at).unwrap(), None);
        std::fs::write(p.join("energy_uj"), "11345678\n").unwrap();
        assert_eq!(s.sample(at + Duration::from_secs(1)).unwrap(), Some(12.35));
        std::fs::remove_file(p.join("energy_uj")).unwrap();
        assert!(s.sample(at + Duration::from_secs(2)).is_err());
        std::fs::write(p.join("energy_uj"), "22345678\n").unwrap();
        assert_eq!(s.sample(at + Duration::from_secs(3)).unwrap(), None);
        std::fs::remove_dir_all(root).unwrap();
    }
}
