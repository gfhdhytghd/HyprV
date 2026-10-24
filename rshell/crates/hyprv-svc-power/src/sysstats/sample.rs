use super::{cpu_delta, memory, network, Cpu, Metric, State};
use hyprv_core::sample_file::SampleFile;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

#[derive(Clone, Copy, Default)]
struct Cpus {
    total: Option<Cpu>,
    cores: [Option<(u32, Cpu)>; 8],
}
fn row(fields: impl Iterator<Item = impl AsRef<str>>, laptop: bool) -> Option<Cpu> {
    let (mut count, mut total, mut idle) = (0, 0u64, 0u64);
    for (i, f) in fields.enumerate() {
        let n = f.as_ref().parse::<u64>().ok()?;
        if i < 8 || laptop {
            total = total.checked_add(n)?;
        }
        if i == 3 || i == 4 {
            idle = idle.checked_add(n)?;
        }
        count += 1;
    }
    (count >= 4).then_some(Cpu { total, idle })
}
#[cfg(test)]
fn cpus(text: &str, cores: bool) -> Cpus {
    cpus_for_profile(text, cores, false)
}
fn cpus_for_profile(text: &str, cores: bool, laptop: bool) -> Cpus {
    let mut out = Cpus::default();
    for line in text.lines() {
        let mut f = line.split_ascii_whitespace();
        let Some(name) = f.next() else { continue };
        if name == "cpu" {
            out.total = row(f, laptop);
            if !cores {
                break;
            }
            continue;
        }
        let Some(id) = name.strip_prefix("cpu").and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let Some(cpu) = row(f, laptop) else { continue };
        if let Some(slot) = out
            .cores
            .iter_mut()
            .find(|s| s.is_some_and(|(n, _)| n == id))
        {
            *slot = Some((id, cpu));
            continue;
        }
        if let Some(pos) = out.cores.iter().position(|s| s.is_none_or(|(n, _)| id < n)) {
            out.cores.copy_within(pos..7, pos + 1);
            out.cores[pos] = Some((id, cpu));
        }
    }
    out
}
pub(super) fn route(text: &str) -> Option<&str> {
    text.lines().skip(1).find_map(|l| {
        let mut f = l.split_ascii_whitespace();
        let name = f.next()?;
        let dest = f.next()?;
        f.next()?;
        let flags = u32::from_str_radix(f.next()?, 16).ok()?;
        (dest == "00000000" && flags & 1 != 0).then_some(name)
    })
}
fn laptop_route(text: &str) -> Option<&str> {
    text.lines().skip(1).find_map(|l| {
        let mut f = l.split_ascii_whitespace();
        let name = f.next()?;
        let dest = f.next()?;
        let mask = f.nth(5)?;
        (dest == "00000000" && mask == "00000000").then_some(name)
    })
}
fn laptop_memory(text: &str) -> Option<(u64, u64)> {
    let mut v = [0; 5];
    for l in text.lines() {
        let Some((k, raw)) = l.split_once(':') else {
            continue;
        };
        if let Some(i) = ["MemTotal", "MemAvailable", "MemFree", "Buffers", "Cached"]
            .iter()
            .position(|x| *x == k)
        {
            v[i] = raw
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()?
                .checked_mul(1024)?;
        }
    }
    if v[0] == 0 {
        return None;
    }
    Some((
        v[0],
        if v[1] > 0 {
            v[1]
        } else {
            v[2].checked_add(v[3])?.checked_add(v[4])?
        },
    ))
}
struct Baseline {
    at: Instant,
    cpu: Cpus,
    net: Option<(Arc<str>, u64, u64)>,
}
pub(super) struct Reader {
    pub laptop: bool,
    pub missing_temperature: bool,
    stat: SampleFile,
    mem: SampleFile,
    route: SampleFile,
    net: SampleFile,
    sys: Option<PathBuf>,
    route_dirty: bool,
    interface: Option<Arc<str>>,
    counters: Option<(SampleFile, SampleFile)>,
    pub thermal: Vec<SampleFile>,
    pub hwmon: Vec<SampleFile>,
    before: Option<Baseline>,
}
impl Reader {
    pub fn new(proc: &Path) -> Self {
        Self {
            laptop: false,
            missing_temperature: false,
            stat: SampleFile::new(proc.join("stat"), 4096),
            mem: SampleFile::new(proc.join("meminfo"), 16384),
            route: SampleFile::new(proc.join("net/route"), 256 * 1024),
            net: SampleFile::new(proc.join("net/dev"), 256 * 1024),
            sys: None,
            route_dirty: true,
            interface: None,
            counters: None,
            thermal: Vec::new(),
            hwmon: Vec::new(),
            before: None,
        }
    }
    pub fn with_sys(mut self, sys: &Path) -> Self {
        if sys != Path::new("/sys") {
            self.stat = self.stat.verify_unlinked(true);
            self.mem = self.mem.verify_unlinked(true);
            self.route = self.route.verify_unlinked(true);
            self.net = self.net.verify_unlinked(true);
        }
        self.sys = Some(sys.to_owned());
        self
    }
    // Selection is exactly the first UP IPv4 default route in /proc/net/route,
    // independent of metric. Only the subscribed rtnetlink events invalidate it.
    fn network(&mut self) -> super::Result<Option<(Arc<str>, u64, u64)>> {
        if self.route_dirty {
            let laptop = self.laptop;
            let text = self.route.text()?;
            let selected = if laptop {
                laptop_route(text)
            } else {
                route(text)
            };
            if self.interface.as_deref() != selected {
                self.interface = selected.map(Arc::from);
            }
            self.counters = self
                .sys
                .as_ref()
                .zip(self.interface.as_deref())
                .filter(|(_, name)| {
                    !name.is_empty()
                        && *name != "."
                        && *name != ".."
                        && !name.bytes().any(|b| b == b'/' || b == 0)
                })
                .and_then(|(sys, name)| {
                    let statistics = sys.join("class/net").join(name).join("statistics");
                    Some((
                        SampleFile::new(
                            crate::actor::confined_sysfs(sys, &statistics.join("rx_bytes")).ok()?,
                            64,
                        )
                        .verify_unlinked(sys != Path::new("/sys")),
                        SampleFile::new(
                            crate::actor::confined_sysfs(sys, &statistics.join("tx_bytes")).ok()?,
                            64,
                        )
                        .verify_unlinked(sys != Path::new("/sys")),
                    ))
                });
            self.route_dirty = false;
        }
        let Some(iface) = &self.interface else {
            return Ok(None);
        };
        // Both sysfs attributes and /proc/net/dev expose rtnl_link_stats64 bytes.
        // A missing/malformed/removed device must use the original proc parser.
        let sys = self.sys.as_deref();
        let laptop = self.laptop;
        let fast = self.counters.as_mut().and_then(|(rx, tx)| {
            if laptop {
                if let Some(sys) = sys {
                    crate::actor::confined_sysfs(sys, rx.path()).ok()?;
                    crate::actor::confined_sysfs(sys, tx.path()).ok()?;
                }
            }
            Some((
                rx.prefix().ok()?.trim().parse::<u64>().ok()?,
                tx.prefix().ok()?.trim().parse::<u64>().ok()?,
            ))
        });
        let values = fast.or_else(|| network(self.net.text().ok()?, iface));
        Ok(values.map(|(rx, tx)| (iface.clone(), rx, tx)))
    }
    pub fn reset(&mut self) {
        self.before = None;
    }
    pub fn reset_network(&mut self) {
        self.route_dirty = true;
        self.route.invalidate();
        self.counters = None;
        if let Some(b) = &mut self.before {
            b.net = None;
        }
    }
    pub fn invalidate(&mut self) {
        self.stat.invalidate();
        self.mem.invalidate();
        self.route.invalidate();
        self.net.invalidate();
        self.reset_network();
        self.reset();
    }
    pub fn sample(
        &mut self,
        state: &mut State,
        metrics: &[Metric],
        at: Instant,
    ) -> super::Result<()> {
        let elapsed = self
            .before
            .as_ref()
            .map(|b| at.duration_since(b.at))
            .unwrap_or_default();
        let cpu = if metrics.contains(&Metric::CpuTotal) || metrics.contains(&Metric::CpuCores) {
            cpus_for_profile(
                self.stat.prefix()?,
                metrics.contains(&Metric::CpuCores),
                self.laptop,
            )
        } else {
            Cpus::default()
        };
        state.elapsed = elapsed;
        let old_cpu = state.cpu_percent;
        state.cpu_percent = cpu
            .total
            .zip(self.before.as_ref().and_then(|b| b.cpu.total))
            .and_then(|(a, b)| cpu_delta(b, a));
        if self.laptop && state.cpu_percent.is_none() {
            state.cpu_percent = old_cpu.or(Some(0.));
        }
        state.core_percent.clear();
        if metrics.contains(&Metric::CpuCores) {
            for (id, now) in cpu.cores.iter().flatten() {
                let previous = self
                    .before
                    .as_ref()
                    .and_then(|b| b.cpu.cores.iter().flatten().find(|(n, _)| n == id))
                    .map(|(_, c)| *c);
                state
                    .core_percent
                    .push_back(previous.and_then(|b| cpu_delta(b, *now)));
            }
        }
        if metrics.contains(&Metric::Memory) {
            let text = self.mem.text()?;
            let (total, available) = if self.laptop {
                laptop_memory(text)
            } else {
                memory(text)
            }
            .ok_or("invalid meminfo")?;
            state.memory_total_bytes = total;
            state.memory_available_bytes = available;
            state.memory_percent = Some(if self.laptop {
                (total as f64 - available as f64) / total as f64 * 100.
            } else {
                (total - available) as f64 / total as f64 * 100.
            });
        }
        let old_temperature = state.temperature_c;
        let sys = self.sys.as_deref();
        state.temperature_c = if metrics.contains(&Metric::Temperature) {
            self.thermal
                .iter_mut()
                .find_map(|f| {
                    number_scoped(f, sys)
                        .filter(|v| {
                            (1. ..=125000.).contains(v) && (!self.laptop || v.fract() == 0.)
                        })
                        .map(|v| {
                            if self.laptop && v <= 1000. {
                                v
                            } else {
                                v / 1000.
                            }
                        })
                })
                .or_else(|| {
                    if self.laptop {
                        return self.hwmon.iter_mut().find_map(|f| {
                            number_scoped(f, sys)
                                .filter(|v| (1. ..=125000.).contains(v) && v.fract() == 0.)
                                .map(|v| if v > 1000. { v / 1000. } else { v })
                        });
                    }
                    let mut maximum = None;
                    for f in &mut self.hwmon {
                        let v = f
                            .prefix()
                            .ok()?
                            .trim()
                            .parse::<u32>()
                            .ok()
                            .filter(|v| *v > 0)?;
                        maximum = Some(maximum.unwrap_or(0).max(v));
                    }
                    maximum.map(|v| v as f64 / 1000.)
                })
        } else {
            None
        };
        self.missing_temperature =
            metrics.contains(&Metric::Temperature) && state.temperature_c.is_none();
        if self.laptop && state.temperature_c.is_none() {
            state.temperature_c = old_temperature;
        }
        // CPU package power is supplied exclusively by the privileged D-Bus client.
        let net = if metrics.contains(&Metric::Network) {
            let net = self.network()?;
            state.iface = net.as_ref().map(|(iface, _, _)| iface.clone());
            net
        } else {
            state.iface = None;
            None
        };
        state.rx_bytes_per_s = None;
        state.tx_bytes_per_s = None;
        if let Some((iface, rx, tx)) = &net {
            state.rx_bytes = *rx;
            state.tx_bytes = *tx;
            if let Some((before, brx, btx)) = self.before.as_ref().and_then(|b| b.net.as_ref()) {
                if before == iface && elapsed.as_secs_f64() > 0. {
                    state.rx_bytes_per_s = if self.laptop {
                        Some(rx.saturating_sub(*brx) as f64 / elapsed.as_secs_f64().max(0.001))
                    } else {
                        rx.checked_sub(*brx)
                            .map(|v| v as f64 / elapsed.as_secs_f64())
                    };
                    state.tx_bytes_per_s = if self.laptop {
                        Some(tx.saturating_sub(*btx) as f64 / elapsed.as_secs_f64().max(0.001))
                    } else {
                        tx.checked_sub(*btx)
                            .map(|v| v as f64 / elapsed.as_secs_f64())
                    };
                }
            }
        }
        for (metric, history, value) in [
            (Metric::CpuTotal, &mut state.cpu_history, state.cpu_percent),
            (
                Metric::Memory,
                &mut state.memory_history,
                state.memory_percent,
            ),
            (
                Metric::Network,
                &mut state.network_history,
                state
                    .rx_bytes_per_s
                    .zip(state.tx_bytes_per_s)
                    .map(|(r, t)| r + t),
            ),
        ] {
            if !self.laptop || metrics.contains(&metric) {
                history.push_back(value);
            }
        }
        if let Some(before) = &self.before {
            let cpu_reset = before
                .cpu
                .total
                .zip(cpu.total)
                .is_some_and(|(b, a)| a.total < b.total || a.idle < b.idle);
            let net_reset = match (&before.net, &net) {
                (Some((a, ar, at)), Some((b, br, bt))) => a != b || br < ar || bt < at,
                (None, None) => false,
                _ => true,
            };
            if cpu_reset || net_reset {
                state.generation += 1;
            }
        }
        self.before = Some(Baseline { at, cpu, net });
        Ok(())
    }
}
fn number_scoped(f: &mut SampleFile, root: Option<&Path>) -> Option<f64> {
    if let Some(root) = root.filter(|p| *p != Path::new("/sys")) {
        crate::actor::confined_sysfs(root, f.path()).ok()?;
    }
    number(f)
}
fn number(f: &mut SampleFile) -> Option<f64> {
    f.prefix()
        .ok()?
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_cpu_matches_reference() {
        let text="cpu 100 1 2 30 4 5 6 7 80 0\ncpu8 100 1 2 30\ncpu2 100 1 2 30\ncpu2 200 1 2 30\ncpu0 bad 1 2 30\ncpu1 1 2 3 4\n";
        let r = super::super::parse_cpu(text);
        let fast = cpus(text, true);
        assert_eq!(fast.total, Some(r["cpu"]));
        for (id, c) in fast.cores.into_iter().flatten() {
            assert_eq!(c, r[&format!("cpu{id}")]);
        }
    }
}

#[cfg(test)]
#[path = "../../../hyprv-core/src/alloc_probe.rs"]
mod alloc_probe;
#[cfg(test)]
mod allocation_regressions {
    use super::*;
    use std::{fs, path::PathBuf, time::Duration};
    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn fixture() -> Fixture {
        let p = std::env::temp_dir().join(format!(
            "hyprv-sample-{}-{}",
            std::process::id(),
            alloc_probe::cpu_ns()
        ));
        fs::create_dir_all(p.join("net")).unwrap();
        for (name, text) in [
            (
                "stat",
                "cpu 100 1 2 30 4 5 6 7 80 0\ncpu0 100 1 2 30 4 5 6 7 80 0\n",
            ),
            ("meminfo", "MemTotal: 100 kB\nMemAvailable: 50 kB\n"),
            (
                "net/route",
                "Iface Destination Gateway Flags\neth0 00000000 01010101 0003\n",
            ),
            ("net/dev", "eth0: 100 0 0 0 0 0 0 0 200 0\n"),
            ("temp", "42000\n"),
            ("energy", "100000\n"),
        ] {
            fs::write(p.join(name), text).unwrap();
        }
        Fixture(p)
    }
    #[test]
    fn warm_read_clone_and_publish_are_allocation_free_and_metadata_is_quiet() {
        let fixture = fixture();
        let mut reader = Reader::new(&fixture.0);
        reader
            .thermal
            .push(SampleFile::new(fixture.0.join("temp"), 64));
        let metrics = super::super::all_metrics();
        let mut state = State::default();
        let base = Instant::now();
        for i in 0..3 {
            reader
                .sample(&mut state, &metrics, base + Duration::from_secs(i))
                .unwrap();
        }
        let topic = hyprv_core::store::Topic::new(state.clone());
        let count = alloc_probe::allocations(|| {
            for i in 3..125 {
                reader
                    .sample(&mut state, &metrics, base + Duration::from_secs(i))
                    .unwrap();
                std::hint::black_box(topic.publish(state.clone()));
            }
        });
        assert_eq!(count, 0, "warm sampling and publication allocated");
        reader
            .sample(&mut state, &metrics, base + Duration::from_secs(127))
            .unwrap();
        assert!(
            topic.publish(state.clone()).is_none(),
            "elapsed alone is metadata"
        );
        let positive = alloc_probe::allocations(|| {
            std::hint::black_box(super::super::parse_cpu("cpu 1 2 3 4\ncpu0 1 2 3 4\n"));
        });
        assert!(positive > 0);
    }
    #[test]
    fn unleased_sensors_are_not_opened_and_failures_are_unavailable() {
        let fixture = fixture();
        let mut reader = Reader::new(&fixture.0);
        reader
            .thermal
            .push(SampleFile::new(fixture.0.join("missing"), 64));
        let mut state = State::default();
        reader
            .sample(
                &mut state,
                &[Metric::CpuTotal, Metric::Memory],
                Instant::now(),
            )
            .unwrap();
        assert_eq!(state.temperature_c, None);
        assert_eq!(state.package_power_w, None);
        assert!(state.core_percent.is_empty());
        reader
            .sample(&mut state, &[Metric::Temperature], Instant::now())
            .unwrap();
        assert_eq!(state.temperature_c, None);
    }
    #[test]
    fn counters_reset_interface_loss_and_sensor_failure_preserve_unavailable() {
        let fixture = fixture();
        let mut reader = Reader::new(&fixture.0);
        let mut state = State::default();
        let t = Instant::now();
        reader
            .hwmon
            .push(SampleFile::new(fixture.0.join("temp"), 64));
        reader
            .sample(&mut state, &super::super::all_metrics(), t)
            .unwrap();
        assert_eq!(state.temperature_c, Some(42.));
        fs::write(fixture.0.join("stat"), "cpu 50 1 2 10 0 0 0 0\n").unwrap();
        fs::write(fixture.0.join("net/dev"), "eth0: 1 0 0 0 0 0 0 0 2 0\n").unwrap();
        fs::write(fixture.0.join("temp"), "bad\n").unwrap();
        let old = state.generation;
        reader
            .sample(
                &mut state,
                &super::super::all_metrics(),
                t + Duration::from_secs(1),
            )
            .unwrap();
        assert_eq!(state.cpu_percent, None);
        assert_eq!(state.rx_bytes_per_s, None);
        assert!(state.generation > old);
        assert_eq!(state.temperature_c, None);
        fs::write(fixture.0.join("net/dev"), "other0: 5 0 0 0 0 0 0 0 8 0\n").unwrap();
        reader
            .sample(
                &mut state,
                &super::super::all_metrics(),
                t + Duration::from_secs(2),
            )
            .unwrap();
        assert!(state.iface.is_none());
        assert_eq!(state.rx_bytes, 1);
    }
    #[test]
    fn sysfs_equals_legacy_counters_and_rates_and_route_selection_is_event_driven() {
        let fixture = fixture();
        let sys = fixture.0.join("sys");
        let dir = sys.join("class/net/eth0/statistics");
        fs::create_dir_all(&dir).unwrap();
        let mut fast = Reader::new(&fixture.0).with_sys(&sys);
        let mut legacy = Reader::new(&fixture.0);
        let (mut a, mut b) = (State::default(), State::default());
        let at = Instant::now();
        // Ignore DOWN defaults, preserve file order even if later metrics are lower.
        fs::write(fixture.0.join("net/route"), "Iface Destination Gateway Flags RefCnt Use Metric\ndown0 00000000 0 0000 0 0 0\neth0 00000000 0 0003 0 0 999\nother0 00000000 0 0003 0 0 1\n").unwrap();
        for (i, (rx, tx)) in [(100u64, 200u64), (900, 1600), (5, 6)]
            .into_iter()
            .enumerate()
        {
            fs::write(dir.join("rx_bytes"), rx.to_string()).unwrap();
            fs::write(dir.join("tx_bytes"), tx.to_string()).unwrap();
            fs::write(
                fixture.0.join("net/dev"),
                format!("eth0: {rx} 0 0 0 0 0 0 0 {tx} 0\n"),
            )
            .unwrap();
            let now = at + Duration::from_secs(i as u64);
            fast.sample(&mut a, &[Metric::Network], now).unwrap();
            legacy.sample(&mut b, &[Metric::Network], now).unwrap();
            assert_eq!(
                a, b,
                "sysfs must equal legacy, including counter resets and rates"
            );
        }
        // Steady sampling allocates nothing and never polls the route file.
        fs::write(
            fixture.0.join("net/route"),
            "Iface Destination Gateway Flags\nother0 00000000 0 0003\n",
        )
        .unwrap();
        assert_eq!(
            alloc_probe::allocations(|| {
                fast.sample(&mut a, &[Metric::Network], at + Duration::from_secs(3))
                    .unwrap();
            }),
            0
        );
        assert_eq!(a.iface.as_deref(), Some("eth0"));
        // Same invalidation hook used by Native's rtnetlink branch; no sysfs for other0.
        fs::write(fixture.0.join("net/dev"), "other0: 77 0 0 0 0 0 0 0 88 0\n").unwrap();
        fast.reset_network();
        fast.sample(&mut a, &[Metric::Network], at + Duration::from_secs(4))
            .unwrap();
        assert_eq!(a.iface.as_deref(), Some("other0"));
        assert_eq!((a.rx_bytes, a.tx_bytes), (77, 88));
        assert_eq!(a.rx_bytes_per_s, None);
    }
    #[test]
    fn bad_sysfs_falls_back_to_proc_and_names_cannot_escape_sys_root() {
        let fixture = fixture();
        let sys = fixture.0.join("sys");
        let dir = sys.join("class/net/eth0/statistics");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("rx_bytes"), "123\n").unwrap();
        fs::write(dir.join("tx_bytes"), "bad\n").unwrap();
        let mut reader = Reader::new(&fixture.0).with_sys(&sys);
        let mut state = State::default();
        reader
            .sample(&mut state, &[Metric::Network], Instant::now())
            .unwrap();
        assert_eq!((state.rx_bytes, state.tx_bytes), (100, 200));
        assert_eq!(
            alloc_probe::allocations(|| {
                reader
                    .sample(&mut state, &[Metric::Network], Instant::now())
                    .unwrap();
            }),
            0
        );
        fs::write(
            fixture.0.join("net/route"),
            "Iface Destination Gateway Flags\n../escape 00000000 0 0003\n",
        )
        .unwrap();
        fs::write(
            fixture.0.join("net/dev"),
            "../escape: 3 0 0 0 0 0 0 0 4 0\n",
        )
        .unwrap();
        reader.reset_network();
        reader
            .sample(&mut state, &[Metric::Network], Instant::now())
            .unwrap();
        assert!(reader.counters.is_none());
        assert_eq!((state.rx_bytes, state.tx_bytes), (3, 4));
    }
    #[test]
    #[ignore = "manual read-only kernel/sensor CPU benchmark"]
    fn real_sample_cpu() {
        let paths = crate::Paths::default();
        let mut reader = Reader::new(&paths.proc).with_sys(&paths.sys);
        // Same root as production, including an explicitly selected fixture.
        let hardware =
            hyprv_privhelper::cooling::Hardware::discover(&paths.sys.join("class/hwmon"));
        for preferred in [
            "x86_pkg_temp",
            "TCPU",
            "cpu_thermal",
            "cpu-thermal",
            "soc_thermal",
        ] {
            for zone in crate::actor::sysfs_entries(&paths.sys, paths.sys.join("class/thermal")) {
                if fs::read_to_string(zone.join("type")).is_ok_and(|s| s.trim() == preferred) {
                    reader.thermal.push(SampleFile::new(zone.join("temp"), 64));
                }
            }
        }
        reader.hwmon = hardware
            .temperatures
            .iter()
            .map(|p| SampleFile::new(p, 64))
            .collect();
        let mut state = State::default();
        let metrics = [
            Metric::CpuTotal,
            Metric::Memory,
            Metric::Network,
            Metric::Temperature,
            Metric::CpuPackagePower,
        ];
        for _ in 0..3 {
            reader.sample(&mut state, &metrics, Instant::now()).unwrap();
        }
        let topic = hyprv_core::store::Topic::new(state.clone());
        let paced = std::env::var_os("HYPRV_SAMPLE_PACED").is_some();
        let count = if paced { 60 } else { 500 };
        let mut times = [0u64; 500];
        let mut allocations = 0;
        for time in &mut times[..count] {
            if paced {
                std::thread::sleep(Duration::from_secs(1));
            }
            let begin = alloc_probe::cpu_ns();
            allocations += alloc_probe::allocations(|| {
                reader.sample(&mut state, &metrics, Instant::now()).unwrap();
                std::hint::black_box(topic.publish(state.clone()));
            });
            *time = alloc_probe::cpu_ns() - begin;
        }
        let ns: u64 = times[..count].iter().sum();
        times[..count].sort_unstable();
        println!(
            "sample_cpu_ns={} p50_ns={} p99_ns={} samples={} paced={} allocations={} sensors_thermal={} sensors_hwmon={}",
            ns / count as u64, times[count/2], times[(count-1)*99/100], count, paced,
            allocations, reader.thermal.len(), reader.hwmon.len()
        );
        assert_eq!(allocations, 0);
    }
}
