//! Each leased metric owns its own cadence and delta baseline. Passive observers
//! never create these readers; dropping the metric releases cached file handles.
use super::*;
struct Source {
    period: Duration,
    next: Option<Instant>,
    reader: sample::Reader,
    state: State,
}
pub(super) struct Sampler {
    paths: crate::Paths,
    sources: BTreeMap<Metric, Source>,
}
impl Sampler {
    pub fn new(paths: crate::Paths) -> Self {
        Self {
            paths,
            sources: BTreeMap::new(),
        }
    }
    pub fn plan(&mut self, plan: BTreeMap<Metric, Duration>) {
        self.sources.retain(|m, _| plan.contains_key(m));
        for (metric, period) in plan {
            // Package watts use the distinct demand-scoped CPU-only D-Bus
            // client, never a user-side RAPL reader or combined desktop helper.
            if metric == Metric::CpuPackagePower {
                continue;
            }
            let source = self.sources.entry(metric).or_insert_with(|| {
                let mut reader = sample::Reader::new(&self.paths.proc).with_sys(&self.paths.sys);
                reader.laptop = true;
                Source {
                    period,
                    next: None,
                    reader,
                    state: State::default(),
                }
            });
            if source.period != period {
                source.period = period;
                source.next = None;
            }
        }
    }
    pub fn discover(&mut self) {
        let Some(source) = self.sources.get_mut(&Metric::Temperature) else {
            return;
        };
        use hyprv_core::sample_file::SampleFile;
        source.reader.thermal.clear();
        source.reader.hwmon.clear();
        for preferred in [
            "x86_pkg_temp",
            "TCPU",
            "cpu_thermal",
            "cpu-thermal",
            "soc_thermal",
        ] {
            for zone in actor::sysfs_entries(&self.paths.sys, self.paths.sys.join("class/thermal"))
            {
                if actor::sysfs_text(&self.paths.sys, zone.join("type"))
                    .is_ok_and(|s| s.trim() == preferred)
                {
                    if let Ok(path) = self.paths.guard_sysfs(zone.join("temp")) {
                        source
                            .reader
                            .thermal
                            .push(SampleFile::new(path, 64).verify_unlinked(true));
                    }
                }
            }
        }
        for hwmon in actor::sysfs_entries(&self.paths.sys, self.paths.sys.join("class/hwmon")) {
            if !actor::sysfs_text(&self.paths.sys, hwmon.join("name"))
                .is_ok_and(|s| matches!(s.trim(), "coretemp" | "k10temp" | "zenpower"))
            {
                continue;
            }
            for input in actor::sysfs_entries(&self.paths.sys, &hwmon) {
                let Some(name) = input.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                let Some(base) = name
                    .strip_suffix("_input")
                    .filter(|n| n.starts_with("temp"))
                else {
                    continue;
                };
                if actor::sysfs_text(&self.paths.sys, hwmon.join(format!("{base}_label")))
                    .is_ok_and(|s| {
                        matches!(s.trim(), "Package id 0" | "Tctl" | "CPU Package" | "CPU")
                    })
                {
                    if let Ok(path) = self.paths.guard_sysfs(input) {
                        source
                            .reader
                            .hwmon
                            .push(SampleFile::new(path, 64).verify_unlinked(true));
                    }
                }
            }
        }
    }
    pub fn reset(&mut self) {
        for s in self.sources.values_mut() {
            s.reader.invalidate();
            s.next = None;
        }
    }
    pub fn reset_network(&mut self) {
        if let Some(s) = self.sources.get_mut(&Metric::Network) {
            s.reader.reset_network();
            s.next = None;
        }
    }
    pub fn sample(&mut self, state: &mut State, now: Instant) -> Result<()> {
        // Fake trees have no kernel uevents. Refresh only on an actually due
        // visible temperature tick; never watch the host's udev/netlink sockets.
        if self.paths.isolated_sysfs()
            && self
                .sources
                .get(&Metric::Temperature)
                .is_some_and(|s| s.next.is_none_or(|next| now >= next))
        {
            self.discover();
        }
        for (metric, source) in &mut self.sources {
            if source.next.is_some_and(|next| now < next) {
                continue;
            }
            source.reader.sample(&mut source.state, &[*metric], now)?;
            source.next = Some(now + source.period);
            let s = &source.state;
            match metric {
                Metric::CpuTotal => {
                    state.cpu_percent = s.cpu_percent;
                    state.cpu_history = s.cpu_history.clone();
                }
                Metric::CpuCores => state.core_percent = s.core_percent.clone(),
                Metric::Memory => {
                    state.memory_total_bytes = s.memory_total_bytes;
                    state.memory_available_bytes = s.memory_available_bytes;
                    state.memory_percent = s.memory_percent;
                    state.memory_history = s.memory_history.clone();
                }
                Metric::Network => {
                    state.iface = s.iface.clone();
                    state.rx_bytes = s.rx_bytes;
                    state.tx_bytes = s.tx_bytes;
                    state.rx_bytes_per_s = s.rx_bytes_per_s;
                    state.tx_bytes_per_s = s.tx_bytes_per_s;
                    state.network_history = s.network_history.clone();
                }
                Metric::Temperature => {
                    state.temperature_c = s.temperature_c;
                    state.temperature_available = Some(!source.reader.missing_temperature);
                }
                Metric::CpuPackagePower => unreachable!(),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::Tree;
    #[test]
    fn five_second_strip_is_independent_of_fast_visible_temperature() {
        let tree = Tree::new();
        tree.write(
            "proc/stat",
            "cpu 100 0 0 100 0 0 0 0 100 0\ncpu0 100 0 0 100 0 0 0 0 100 0\n",
        );
        tree.write("proc/meminfo","MemTotal: 1000 kB\nMemAvailable: 0 kB\nMemFree: 100 kB\nBuffers: 20 kB\nCached: 30 kB\n");
        tree.write("sys/class/thermal/thermal_zone9/type", "x86_pkg_temp");
        tree.write("sys/class/thermal/thermal_zone9/temp", "42000");
        let mut sampler = Sampler::new(tree.paths());
        let mut state = State::default();
        let now = Instant::now();
        // No readers/hidden files are needed at all, even for an hour of attempts.
        sampler
            .sample(&mut state, now + Duration::from_secs(3600))
            .unwrap();
        assert!(state.cpu_history.is_empty());
        sampler.plan(BTreeMap::from([
            (Metric::CpuTotal, Duration::from_secs(5)),
            (Metric::Memory, Duration::from_secs(5)),
            (Metric::Temperature, Duration::from_secs(1)),
        ]));
        sampler.discover();
        sampler.sample(&mut state, now).unwrap();
        assert_eq!(state.memory_percent, Some(85.));
        assert_eq!(state.temperature_c, Some(42.));
        assert!(state.core_percent.is_empty());
        assert!(state.network_history.is_empty());
        assert_eq!(state.package_power_w, None);
        // Poison proc files between strip ticks: fast temperature never reads them.
        tree.write("proc/stat", "bad");
        tree.write("proc/meminfo", "bad");
        for t in 1..5 {
            sampler
                .sample(&mut state, now + Duration::from_secs(t))
                .unwrap();
        }
        assert_eq!(state.cpu_history.len(), 1);
        assert_eq!(state.memory_history.len(), 1);
        tree.write("proc/stat", "cpu 200 0 0 200 0 0 0 0 200 0\n");
        tree.write("proc/meminfo", "MemTotal: 1000 kB\nMemAvailable: 500 kB\n");
        sampler
            .sample(&mut state, now + Duration::from_secs(5))
            .unwrap();
        assert_eq!(state.cpu_history.len(), 2);
        assert!(
            (state.cpu_percent.unwrap() - 200. / 3.).abs() < 1e-8,
            "laptop includes guest columns"
        );
        sampler.plan(BTreeMap::new());
        assert!(sampler.sources.is_empty());
        tree.write("proc/stat", "bad");
        tree.write("proc/meminfo", "bad");
        let before = state.clone();
        sampler
            .sample(&mut state, now + Duration::from_secs(7200))
            .unwrap();
        assert_eq!(state, before);
        // Showing again primes, rather than reporting a delta across hidden time.
        sampler.plan(BTreeMap::from([(Metric::CpuTotal, Duration::from_secs(5))]));
        tree.write("proc/stat", "cpu 999 0 0 999 0 0 0 0 999 0\n");
        sampler
            .sample(&mut state, now + Duration::from_secs(7201))
            .unwrap();
        assert_eq!(state.cpu_percent, Some(0.));
    }
    #[test]
    fn thermal_priority_label_filter_and_conversion_quirk() {
        let tree = Tree::new();
        for (zone, kind, temp) in [
            ("0", "TCPU", "70000"),
            ("8", "x86_pkg_temp", "1000"),
            ("9", "x86_pkg_temp", "50000"),
        ] {
            tree.write(&format!("sys/class/thermal/thermal_zone{zone}/type"), kind);
            tree.write(&format!("sys/class/thermal/thermal_zone{zone}/temp"), temp);
        }
        tree.write("sys/class/hwmon/hwmon0/name", "dell_smm");
        tree.write("sys/class/hwmon/hwmon0/temp1_label", "CPU");
        tree.write("sys/class/hwmon/hwmon0/temp1_input", "99000");
        tree.write("sys/class/hwmon/hwmon1/name", "coretemp");
        for (n, label, temp) in [
            ("1", "Core 0", "99000"),
            ("2", "Package id 0", "44000"),
            ("3", "CPU", "88000"),
        ] {
            tree.write(&format!("sys/class/hwmon/hwmon1/temp{n}_label"), label);
            tree.write(&format!("sys/class/hwmon/hwmon1/temp{n}_input"), temp);
        }
        let mut sampler = Sampler::new(tree.paths());
        let mut state = State::default();
        let now = Instant::now();
        sampler.plan(BTreeMap::from([(
            Metric::Temperature,
            Duration::from_secs(1),
        )]));
        sampler.discover();
        sampler.sample(&mut state, now).unwrap();
        assert_eq!(state.temperature_c, Some(1000.));
        tree.write("sys/class/thermal/thermal_zone8/temp", "1001");
        sampler
            .sample(&mut state, now + Duration::from_secs(1))
            .unwrap();
        assert_eq!(state.temperature_c, Some(1.001));
        for zone in ["0", "8", "9"] {
            tree.write(
                &format!("sys/class/thermal/thermal_zone{zone}/temp"),
                "125001",
            );
        }
        sampler
            .sample(&mut state, now + Duration::from_secs(2))
            .unwrap();
        assert_eq!(state.temperature_c, Some(44.));
        tree.write("sys/class/hwmon/hwmon1/temp2_input", "bad");
        sampler
            .sample(&mut state, now + Duration::from_secs(3))
            .unwrap();
        assert_eq!(state.temperature_c, Some(88.));
    }
    #[test]
    fn network_lease_releases_files_hidden_and_reprimes_after_reopen() {
        let tree = Tree::new();
        tree.write(
            "proc/net/route",
            "Iface Destination Gateway Flags RefCnt Use Metric Mask\neth0 00000000 0 0003 0 0 0 00000000\n",
        );
        tree.write("proc/net/dev", "eth0: 100 0 0 0 0 0 0 0 200 0\n");
        let mut sampler = Sampler::new(tree.paths());
        let mut state = State::default();
        let now = Instant::now();
        sampler.plan(BTreeMap::from([(Metric::Network, Duration::from_secs(1))]));
        sampler.sample(&mut state, now).unwrap();
        tree.write("proc/net/dev", "eth0: 200 0 0 0 0 0 0 0 400 0\n");
        sampler
            .sample(&mut state, now + Duration::from_secs(1))
            .unwrap();
        assert_eq!(state.rx_bytes_per_s, Some(100.));
        sampler.plan(BTreeMap::new());
        assert!(sampler.sources.is_empty());
        std::fs::remove_dir_all(tree.0.join("proc/net")).unwrap();
        let before = state.clone();
        sampler.reset_network(); // A route event while hidden creates no reader.
        sampler
            .sample(&mut state, now + Duration::from_secs(3600))
            .unwrap();
        assert_eq!(state, before);
        tree.write(
            "proc/net/route",
            "Iface Destination Gateway Flags RefCnt Use Metric Mask\nwlan0 00000000 0 0003 0 0 0 00000000\n",
        );
        tree.write("proc/net/dev", "wlan0: 9000 0 0 0 0 0 0 0 9000 0\n");
        sampler.plan(BTreeMap::from([(Metric::Network, Duration::from_secs(1))]));
        sampler
            .sample(&mut state, now + Duration::from_secs(3601))
            .unwrap();
        assert_eq!(state.iface.as_deref(), Some("wlan0"));
        assert_eq!(state.rx_bytes_per_s, None);
        assert_eq!(state.tx_bytes_per_s, None);
    }
    #[test]
    fn laptop_route_ignores_up_flag_and_preserves_file_order() {
        let tree = Tree::new();
        tree.write("proc/net/route","Iface Destination Gateway Flags RefCnt Use Metric Mask\ndown0 00000000 0 0000 0 0 999 00000000\nup0 00000000 0 0003 0 0 0 00000000\n");
        tree.write("proc/net/dev", "down0: 100 0 0 0 0 0 0 0 200 0\n");
        let mut sampler = Sampler::new(tree.paths());
        sampler.plan(BTreeMap::from([(Metric::Network, Duration::from_secs(1))]));
        let mut state = State::default();
        let now = Instant::now();
        sampler.sample(&mut state, now).unwrap();
        assert_eq!(state.iface.as_deref(), Some("down0"));
        tree.write("proc/net/dev", "down0: 90 0 0 0 0 0 0 0 210 0\n");
        sampler
            .sample(&mut state, now + Duration::from_secs(1))
            .unwrap();
        assert_eq!(state.rx_bytes_per_s, Some(0.));
        assert_eq!(state.tx_bytes_per_s, Some(10.));
    }
}

#[cfg(test)]
mod atomic_fixture_tests {
    use super::*;
    use crate::test_support::Tree;
    #[test]
    fn isolated_thermal_hotplug_and_atomic_updates_never_read_escaped_values() {
        let tree = Tree::new();
        tree.write("sys/class/thermal/thermal_zone0/type", "x86_pkg_temp");
        tree.write("sys/class/thermal/thermal_zone0/temp", "42000");
        let mut sampler = Sampler::new(tree.paths());
        sampler.plan(BTreeMap::from([(
            Metric::Temperature,
            Duration::from_secs(1),
        )]));
        let now = Instant::now();
        let mut state = State::default();
        sampler.sample(&mut state, now).unwrap();
        assert_eq!(state.temperature_c, Some(42.));
        tree.write("replacement", "43000");
        std::fs::rename(
            tree.0.join("replacement"),
            tree.0.join("sys/class/thermal/thermal_zone0/temp"),
        )
        .unwrap();
        sampler
            .sample(&mut state, now + Duration::from_secs(1))
            .unwrap();
        assert_eq!(state.temperature_c, Some(43.));
        std::fs::remove_file(tree.0.join("sys/class/thermal/thermal_zone0/temp")).unwrap();
        tree.write("outside/temp", "99000");
        std::os::unix::fs::symlink(
            tree.0.join("outside/temp"),
            tree.0.join("sys/class/thermal/thermal_zone0/temp"),
        )
        .unwrap();
        sampler
            .sample(&mut state, now + Duration::from_secs(2))
            .unwrap();
        assert_eq!(state.temperature_c, Some(43.));
        assert_eq!(state.temperature_available, Some(false));
    }
}
