use crate::{
    actor::{self, Backend, Context, Handle, Result},
    power::BatteryMode,
};
use hyprv_privhelper::events::KernelEvents;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};
pub const WINDOW_S: i64 = 1800;
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub available: bool,
    pub status: String,
    pub mode: BatteryMode,
    pub capacity_percent: Option<f64>,
    pub power_w: Option<f64>,
    pub average_power_w: Option<f64>,
    pub samples: u32,
    pub window_s: u32,
    pub window_complete: bool,
    pub estimate_s: Option<f64>,
    pub estimate_basis: String,
    pub energy_now_wh: Option<f64>,
    pub energy_full_wh: Option<f64>,
}
impl State {
    pub fn legacy_json_for_profile(&self, profile: crate::Profile) -> serde_json::Value {
        if profile == crate::Profile::Laptop
            && !self.available
            && self.status.is_empty()
            && self.mode == BatteryMode::Unknown
        {
            serde_json::json!({"available":false})
        } else {
            self.legacy_json()
        }
    }
    pub fn legacy_json(&self) -> serde_json::Value {
        serde_json::json!({"available":self.available,"status":self.status,"mode":self.mode,"capacity":self.capacity_percent,"powerW":self.power_w,"averagePowerW":self.average_power_w,"sampleCount":self.samples,"sampleWindowSeconds":self.window_s,"windowComplete":self.window_complete,"estimateSeconds":self.estimate_s,"estimateBasis":self.estimate_basis,"energyNowWh":self.energy_now_wh,"energyFullWh":self.energy_full_wh})
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Action {
    Refresh,
    ResetBaseline,
}
#[derive(Clone, Debug, Default)]
pub struct Reading {
    pub present: bool,
    pub status: String,
    pub capacity: Option<f64>,
    pub energy_now_uwh: Option<f64>,
    pub energy_full_uwh: Option<f64>,
    pub power_uw: Option<f64>,
}
pub fn mode(status: &str, capacity: Option<f64>) -> BatteryMode {
    match status {
        "Charging" | "Pending charge" => BatteryMode::Charging,
        "Discharging" | "Pending discharge" => BatteryMode::Discharging,
        "Full" => BatteryMode::Full,
        "Not charging" if capacity.unwrap_or(0.) >= 99. => BatteryMode::Full,
        "Not charging" => BatteryMode::Plugged,
        _ => BatteryMode::Idle,
    }
}
#[derive(Default)]
pub struct History(pub VecDeque<(i64, i64)>);
impl History {
    pub fn import(text: &str) -> Self {
        Self(
            text.lines()
                .filter_map(|line| {
                    let mut f = line.split('\t');
                    let ts = f.next()?.parse::<i64>().ok()?;
                    let power = f.next()?.parse::<i64>().ok()?;
                    (ts >= 0).then_some((ts, power))
                })
                .take(100000)
                .collect(),
        )
    }
    /// Laptop defaults missing numeric inputs to zero and records a BAT row even
    /// when present=0. The pinned script has a real TAB IFS; means are valid.
    pub fn record_for_profile(
        &mut self,
        now: i64,
        mut r: Reading,
        profile: crate::Profile,
    ) -> State {
        if profile == crate::Profile::Desktop {
            return self.record(now, r);
        }
        let available = r.present;
        r.capacity = Some(r.capacity.unwrap_or(0.));
        r.energy_now_uwh = Some(r.energy_now_uwh.unwrap_or(0.));
        r.energy_full_uwh = Some(r.energy_full_uwh.unwrap_or(0.));
        r.power_uw = Some(r.power_uw.unwrap_or(0.));
        r.present = true;
        let mut state = self.record(now, r);
        state.available = available;
        for value in [
            &mut state.power_w,
            &mut state.average_power_w,
            &mut state.energy_now_wh,
            &mut state.energy_full_wh,
        ]
        .into_iter()
        .flatten()
        {
            *value = (*value * 1000.).round_ties_even() / 1000.;
        }
        state
    }
    pub fn import_for_profile(text: &str, profile: crate::Profile) -> Self {
        if profile == crate::Profile::Desktop {
            return Self::import(text);
        }
        let valid = text
            .lines()
            .filter(|line| {
                let mut f = line.split('\t');
                let ts = f.next().unwrap_or("");
                let power = f.next().unwrap_or("");
                let power = power.strip_prefix('-').unwrap_or(power);
                !ts.is_empty()
                    && ts.bytes().all(|b| b.is_ascii_digit())
                    && !power.is_empty()
                    && power.bytes().all(|b| b.is_ascii_digit())
            })
            .collect::<Vec<_>>()
            .join("\n");
        Self::import(&valid)
    }
    pub fn tsv(&self) -> String {
        self.0.iter().map(|(t, p)| format!("{t}\t{p}\n")).collect()
    }
    pub fn record(&mut self, now: i64, r: Reading) -> State {
        self.0.retain(|(ts, _)| *ts >= now - WINDOW_S);
        let mode = mode(&r.status, r.capacity);
        let signed = r.power_uw.map(|v| match mode {
            BatteryMode::Charging => v.abs(),
            BatteryMode::Discharging => -v.abs(),
            _ => 0.,
        });
        if r.present {
            if let Some(power) = signed {
                self.0.push_back((now, power.round() as i64));
            }
        }
        while self.0.len() > 100000 {
            self.0.pop_front();
        }
        let included: Vec<_> = self
            .0
            .iter()
            .filter(|(_, v)| match mode {
                BatteryMode::Charging => *v > 0,
                BatteryMode::Discharging => *v < 0,
                _ => true,
            })
            .collect();
        let average = (!included.is_empty()).then(|| {
            (included.iter().map(|(_, v)| *v as f64).sum::<f64>() / included.len() as f64)
                .round_ties_even()
        });
        let span = included
            .first()
            .map(|(ts, _)| (now - *ts).max(0) as u32)
            .unwrap_or(0);
        let mut basis = "none";
        let power = average
            .filter(|p| p.abs() > 0.)
            .map(|p| {
                basis = "average";
                p.abs()
            })
            .or_else(|| {
                r.power_uw.filter(|p| p.abs() > 0.).map(|p| {
                    basis = "current";
                    p.abs()
                })
            });
        let remaining = match mode {
            BatteryMode::Charging => r
                .energy_full_uwh
                .zip(r.energy_now_uwh)
                .map(|(f, n)| (f - n).max(0.)),
            BatteryMode::Discharging => r.energy_now_uwh,
            _ => None,
        };
        let estimate = match mode {
            BatteryMode::Full => {
                basis = "none";
                Some(0.)
            }
            BatteryMode::Charging if remaining == Some(0.) => {
                basis = "none";
                Some(0.)
            }
            BatteryMode::Charging | BatteryMode::Discharging => remaining
                .zip(power)
                .filter(|(e, _)| *e > 0.)
                .map(|(e, p)| (e / p * 3600.).round_ties_even()),
            _ => {
                basis = "none";
                None
            }
        };
        if estimate.is_none() {
            basis = "none";
        }
        State {
            available: r.present,
            status: r.status,
            mode,
            capacity_percent: r.capacity,
            power_w: signed.map(|x| x / 1e6),
            average_power_w: average.map(|x| x / 1e6),
            samples: included.len() as u32,
            window_s: span,
            window_complete: span >= WINDOW_S as u32,
            estimate_s: estimate,
            estimate_basis: basis.into(),
            energy_now_wh: r.energy_now_uwh.map(|x| x / 1e6),
            energy_full_wh: r.energy_full_uwh.map(|x| x / 1e6),
        }
    }
}
fn number(path: PathBuf) -> Option<f64> {
    actor::read_number(path)
}
pub fn read_battery(path: &Path) -> Reading {
    let voltage = number(path.join("voltage_now"));
    let energy = |kind: &str| {
        number(path.join(format!("energy_{kind}"))).or_else(|| {
            number(path.join(format!("charge_{kind}")))
                .zip(voltage)
                .map(|(c, v)| (c * v / 1e6).round_ties_even())
        })
    };
    Reading {
        present: number(path.join("present")).map(|v| v > 0.).unwrap_or(true),
        status: std::fs::read_to_string(path.join("status"))
            .unwrap_or_else(|_| "Unknown".into())
            .trim()
            .into(),
        capacity: number(path.join("capacity")),
        energy_now_uwh: energy("now"),
        energy_full_uwh: energy("full"),
        power_uw: number(path.join("power_now")).map(f64::abs).or_else(|| {
            number(path.join("current_now"))
                .zip(voltage)
                .map(|(c, v)| (c.abs() * v / 1e6).round_ties_even())
        }),
    }
}
/// Fakeable sysfs reading with the script's explicit-zero and positive fallback rules.
pub fn read_laptop_battery(path: &Path) -> Reading {
    let num = |name: &str| {
        std::fs::read_to_string(path.join(name))
            .ok()
            .filter(|v| !v.trim().is_empty())
            .map(|v| v.trim().parse::<i64>().unwrap_or(0) as f64)
    };
    let voltage = num("voltage_now").unwrap_or(0.);
    let energy = |kind: &str| {
        num(&format!("energy_{kind}")).unwrap_or_else(|| {
            let charge = num(&format!("charge_{kind}")).unwrap_or(0.);
            if charge > 0. && voltage > 0. {
                (charge * voltage / 1e6).round_ties_even()
            } else {
                0.
            }
        })
    };
    let power = num("power_now").map(f64::abs).unwrap_or_else(|| {
        let current = num("current_now").unwrap_or(0.).abs();
        if current > 0. && voltage > 0. {
            (current * voltage / 1e6).round_ties_even()
        } else {
            0.
        }
    });
    Reading {
        present: num("present").unwrap_or(1.) > 0.,
        status: std::fs::read_to_string(path.join("status"))
            .unwrap_or_else(|_| "Unknown".into())
            .replace('\n', ""),
        capacity: Some(num("capacity").unwrap_or(0.)),
        energy_now_uwh: Some(energy("now")),
        energy_full_uwh: Some(energy("full")),
        power_uw: Some(power),
    }
}
/// Runtime first, /tmp fallback. Probe only on detail acquisition, never hidden
/// startup. Fakes supply both directories explicitly to this boundary.
pub fn select_history_storage(primary: &Path, fallback: &Path) -> Result<PathBuf> {
    use std::{
        io::Write,
        os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    };
    static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut last = None;
    for dir in [primary, fallback] {
        let result = (|| -> Result<()> {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)?;
            let meta = std::fs::symlink_metadata(dir)?;
            if !meta.is_dir()
                || meta.uid() != unsafe { libc::geteuid() }
                || meta.permissions().mode() & 0o022 != 0
            {
                return Err("unsafe battery history directory".into());
            }
            let probe = dir.join(format!(
                ".battery-write-test-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&probe)?;
            let result = file.write_all(b"probe");
            drop(file);
            let _ = std::fs::remove_file(probe);
            result?;
            Ok(())
        })();
        match result {
            Ok(()) => return Ok(dir.to_path_buf()),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| "no battery history storage".into()))
}
pub fn spawn(ctx: Context) -> Handle<State, Action> {
    spawn_for_profile(ctx, hyprv_core::profile::profile())
}
pub fn spawn_for_profile(ctx: Context, profile: crate::Profile) -> Handle<State, Action> {
    let legacy = ctx.paths.runtime.join("battery-power-history.tsv");
    let history = if profile == crate::Profile::Desktop {
        History::import(&std::fs::read_to_string(&legacy).unwrap_or_default())
    } else {
        History::default()
    };
    actor::spawn(
        ctx.clone(),
        Native {
            ctx,
            profile,
            history,
            device: None,
            events: None,
            legacy,
            loaded: false,
            missing: false,
        },
        true,
    )
}
struct Native {
    ctx: Context,
    profile: crate::Profile,
    history: History,
    device: Option<PathBuf>,
    events: Option<KernelEvents>,
    legacy: PathBuf,
    loaded: bool,
    missing: bool,
}
// scripts/lib/battery.sh:22-29: first directory from BAT*, regardless of type,
// scope or presence. A peripheral reporting type=Battery is not a fallback.
fn select_battery(root: &Path) -> Option<PathBuf> {
    actor::entries(root).into_iter().find(|p| {
        p.file_name()
            .is_some_and(|name| name.as_encoded_bytes().starts_with(b"BAT"))
            && p.is_dir()
    })
}
impl Native {
    fn discover(&mut self) {
        let root = self.ctx.paths.sys.join("class/power_supply");
        self.device = if self.ctx.paths.isolated_sysfs() {
            actor::sysfs_entries(&self.ctx.paths.sys, &root)
                .into_iter()
                .find(|p| {
                    p.file_name()
                        .is_some_and(|n| n.as_encoded_bytes().starts_with(b"BAT"))
                        && p.is_dir()
                })
        } else {
            select_battery(&root)
        };
    }
}
impl Backend for Native {
    type State = State;
    type Action = Action;
    fn observe_on_demand(&self) -> bool {
        self.profile == crate::Profile::Laptop
    }
    fn initial_snapshot(&self) -> bool {
        self.profile == crate::Profile::Desktop
    }
    fn event_samples(&self) -> bool {
        self.profile == crate::Profile::Desktop
    }
    fn unobserve(&mut self) {
        self.events = None;
        self.device = None;
    }
    async fn connect(&mut self) -> Result<()> {
        if self.profile == crate::Profile::Laptop && !self.loaded {
            let directory = select_history_storage(
                &self.ctx.paths.runtime,
                &std::env::temp_dir().join("hyprv"),
            )?;
            self.legacy = directory.join("battery-power-history.tsv");
            self.history = History::import_for_profile(
                &std::fs::read_to_string(&self.legacy).unwrap_or_default(),
                self.profile,
            );
            self.loaded = true;
        }
        self.events = if self.ctx.paths.isolated_sysfs() {
            None
        } else {
            Some(KernelEvents::udev()?)
        };
        self.discover();
        Ok(())
    }
    async fn snapshot(&mut self) -> Result<State> {
        if self.profile == crate::Profile::Laptop {
            self.discover();
        }
        let Some(device) = &self.device else {
            if self.profile == crate::Profile::Laptop && !self.missing {
                eprintln!(
                    "laptop battery details unavailable: no BAT directory under {}",
                    self.ctx.paths.sys.display()
                );
            }
            self.missing = true;
            return Ok(State::default());
        };
        if self.ctx.paths.isolated_sysfs() {
            self.ctx.paths.guard_sysfs(device)?;
            for field in [
                "status",
                "present",
                "capacity",
                "voltage_now",
                "energy_now",
                "energy_full",
                "charge_now",
                "charge_full",
                "power_now",
                "current_now",
            ] {
                let path = device.join(field);
                // Missing attributes retain script defaults, but an existing
                // symlink (including a broken link) may not escape the fixture.
                if std::fs::symlink_metadata(&path).is_ok() {
                    self.ctx.paths.guard_sysfs(path)?;
                }
            }
        }
        let missing = std::fs::read_to_string(device.join("status")).is_err();
        if self.profile == crate::Profile::Laptop && missing != self.missing {
            if missing {
                eprintln!(
                    "laptop battery details missing/unreadable status: {}",
                    device.display()
                );
            } else if self.missing {
                eprintln!(
                    "laptop battery detail source recovered: {}",
                    device.display()
                );
            }
        }
        self.missing = missing;
        let reading = if self.profile == crate::Profile::Laptop {
            read_laptop_battery(device)
        } else {
            read_battery(device)
        };
        let state = self.history.record_for_profile(
            hyprv_core::clock::unix_seconds(),
            reading,
            self.profile,
        );
        actor::atomic(&self.legacy, self.history.tsv().as_bytes())?;
        Ok(state)
    }
    async fn apply(&mut self, a: Action) -> Result<()> {
        match a {
            Action::Refresh => self.discover(),
            Action::ResetBaseline => (),
        }
        Ok(())
    }
    async fn event(&mut self) -> Result<()> {
        let Some(events) = self.events.as_ref() else {
            return std::future::pending().await;
        };
        events.subsystem(&["power_supply"]).await?;
        self.discover();
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_matches_legacy_bat_glob() {
        let root =
            std::env::temp_dir().join(format!("hyprv-battery-selector-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("aaa-peripheral")).unwrap();
        std::fs::write(root.join("aaa-peripheral/type"), "Battery\n").unwrap();
        assert_eq!(select_battery(&root), None);
        // BAT* means a directory, not a type or scope filter, and sorts by name.
        std::fs::write(root.join("BAT0"), "not a directory").unwrap();
        std::fs::create_dir(root.join("BAT2")).unwrap();
        std::fs::create_dir(root.join("BAT10")).unwrap();
        std::fs::write(root.join("BAT10/type"), "Unknown\n").unwrap();
        std::fs::write(root.join("BAT10/present"), "0\n").unwrap();
        assert_eq!(select_battery(&root), Some(root.join("BAT10")));
        // Sysfs class entries are symlinks; follow directory symlinks like -d.
        std::os::unix::fs::symlink(root.join("BAT2"), root.join("BAT01")).unwrap();
        assert_eq!(select_battery(&root), Some(root.join("BAT01")));
        std::fs::remove_file(root.join("BAT01")).unwrap();
        std::fs::remove_dir_all(root.join("BAT10")).unwrap();
        assert_eq!(select_battery(&root), Some(root.join("BAT2")));
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(select_battery(&root), None);
    }
    #[test]
    fn sign_filter_window_and_estimate() {
        let mut h = History::import("0\t-1000000\n10\t2000000\ninvalid\n");
        let s = h.record(
            1800,
            Reading {
                present: true,
                status: "Discharging".into(),
                capacity: Some(50.),
                energy_now_uwh: Some(10000000.),
                power_uw: Some(3000000.),
                ..Default::default()
            },
        );
        assert_eq!(s.average_power_w, Some(-2.));
        assert_eq!(s.samples, 2);
        assert!(s.window_complete);
        assert_eq!(s.estimate_s, Some(18000.));
        assert_eq!(s.estimate_basis, "average");
        assert!(h.tsv().contains("1800\t-3000000"));
        let s = h.record(
            1801,
            Reading {
                present: true,
                status: "Full".into(),
                ..Default::default()
            },
        );
        assert_eq!(s.estimate_s, Some(0.));
        assert!(!h.0.iter().any(|(t, _)| *t == 0));
    }
    #[test]
    fn missing_power_is_not_a_zero_sample() {
        let mut h = History::default();
        let s = h.record(
            1,
            Reading {
                present: true,
                status: "Discharging".into(),
                ..Default::default()
            },
        );
        assert_eq!(s.power_w, None);
        assert_eq!(s.samples, 0);
    }
}

#[cfg(test)]
mod laptop_tests {
    use super::*;
    use crate::test_support::Tree;
    #[test]
    fn script_oracle_confirms_real_tab_ifs_and_average_estimate() {
        let tree = Tree::new();
        for (name, value) in [
            ("present", "1"),
            ("status", "Discharging"),
            ("capacity", "50"),
            ("energy_now", "10000000"),
            ("energy_full", "20000000"),
            ("power_now", "3000000"),
        ] {
            tree.write(&format!("sys/class/power_supply/BAT0/{name}"), value);
        }
        let script = include_str!("../tests/fixtures/laptop-battery-info.sh");
        let lib = include_str!("../tests/fixtures/laptop-battery-lib.sh").replace(
            "/sys/class/power_supply/BAT*",
            "\"$HYPRV_FAKE_SYS\"/class/power_supply/BAT*",
        );
        tree.write("quickshell/scripts/battery-info.sh", script);
        tree.write("scripts/lib/battery.sh", &lib);
        tree.write("bin/date", "#!/bin/sh\nprintf '1800001\\n'\n");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            tree.0.join("bin/date"),
            std::fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        tree.write(
            "runtime/hyprv/battery-power-history.tsv",
            "1798201\t-1000000\n1799000\t-2000000\n",
        );
        let output = std::process::Command::new("sh")
            .arg(tree.0.join("quickshell/scripts/battery-info.sh"))
            .env("HYPRV_FAKE_SYS", tree.0.join("sys"))
            .env("XDG_RUNTIME_DIR", tree.0.join("runtime"))
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    tree.0.join("bin").display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .env("LC_ALL", "C")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let oracle: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let mut history = History::import("1798201\t-1000000\n1799000\t-2000000\n");
        let state = history.record_for_profile(
            1800001,
            read_laptop_battery(&tree.0.join("sys/class/power_supply/BAT0")),
            crate::Profile::Laptop,
        );
        let actual = state.legacy_json();
        for (key, value) in oracle.as_object().unwrap() {
            let got = &actual[key];
            if value.is_number() {
                assert_eq!(got.as_f64(), value.as_f64(), "{key}");
            } else {
                assert_eq!(got, value, "{key}");
            }
        }
        assert_eq!(state.samples, 3);
        assert_eq!(state.average_power_w, Some(-2.));
        assert_eq!(state.estimate_basis, "average");
        assert_eq!(state.estimate_s, Some(18000.));
        assert_eq!(
            std::fs::read_to_string(tree.0.join("runtime/hyprv/battery-power-history.tsv"))
                .unwrap(),
            history.tsv()
        );
    }
    #[test]
    fn laptop_explicit_zero_and_missing_inputs_match_script_defaults() {
        let tree = Tree::new();
        tree.write("BAT0/voltage_now", "10000000");
        tree.write("BAT0/charge_now", "1000000");
        tree.write("BAT0/current_now", "-2000000");
        let p = tree.0.join("BAT0");
        let r = read_laptop_battery(&p);
        assert_eq!(r.energy_now_uwh, Some(10000000.));
        assert_eq!(r.power_uw, Some(20000000.));
        tree.write("BAT0/energy_now", "0");
        tree.write("BAT0/power_now", "0");
        let r = read_laptop_battery(&p);
        assert_eq!(r.energy_now_uwh, Some(0.));
        assert_eq!(r.power_uw, Some(0.));
        let mut history = History::default();
        let state = history.record_for_profile(1800, r, crate::Profile::Laptop);
        assert_eq!(state.capacity_percent, Some(0.));
        assert_eq!(state.power_w, Some(0.));
        assert!(history.tsv().contains("1800\t0"));
        tree.write("BAT0/present", "0");
        let _ = history.record_for_profile(1801, read_laptop_battery(&p), crate::Profile::Laptop);
        assert_eq!(history.0.len(), 2, "present=0 still records a BAT row");
    }
}

#[cfg(test)]
mod popup_lifetime_tests {
    use super::*;
    use crate::test_support::{context, settle, Bus, Tree};
    use std::{sync::Arc, time::Duration};
    #[test]
    fn popup_request_owns_one_hz_and_close_stops_before_surface_teardown() {
        let rt = Arc::new(hyprv_core::runtime::ServiceRuntime::new().unwrap());
        rt.block_on(async {
            let bus = Bus::new();
            let tree = Tree::new();
            for (name, value) in [
                ("status", "Discharging"),
                ("power_now", "2000000"),
                ("energy_now", "10000000"),
            ] {
                tree.write(&format!("sys/class/power_supply/BAT0/{name}"), value);
            }
            let ctx = context(bus.connection().await, tree.paths(), Some(&rt));
            let handle = spawn_for_profile(ctx.clone(), crate::Profile::Laptop);
            settle().await;
            assert!(!ctx.paths.runtime.join("battery-power-history.tsv").exists());
            handle.action(Action::Refresh).await.unwrap();
            assert!(
                !ctx.paths.runtime.join("battery-power-history.tsv").exists(),
                "explicit hidden refresh must not sample detail"
            );
            let mut open = handle.subscribe();
            tokio::time::timeout(Duration::from_secs(2), open.changed())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(open.current().value.samples, 1);
            let second = open.current().value.clone();
            // A second output/card shares the same timer. It does not record a row.
            let other = handle.subscribe();
            settle().await;
            assert_eq!(handle.state.snapshot().value, second);
            tokio::time::timeout(Duration::from_secs(2), open.changed())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(open.current().value.samples, 2);
            drop(open);
            drop(other);
            settle().await;
            let path = ctx.paths.runtime.join("battery-power-history.tsv");
            let frozen = std::fs::read_to_string(&path).unwrap();
            tokio::time::sleep(Duration::from_millis(1100)).await;
            assert_eq!(
                std::fs::read_to_string(&path).unwrap(),
                frozen,
                "close requested releases sampling even with the actor/surface alive"
            );
            rt.request_shutdown();
            settle().await;
        });
    }
}

#[cfg(test)]
mod storage_tests {
    use super::*;
    use crate::test_support::Tree;
    #[test]
    fn fallback_is_owned_private_and_no_bat_projection_has_missing_estimate() {
        let tree = Tree::new();
        tree.write("not-a-directory", "blocked");
        let selected = select_history_storage(
            &tree.0.join("not-a-directory/hyprv"),
            &tree.0.join("fallback"),
        )
        .unwrap();
        assert_eq!(selected, tree.0.join("fallback"));
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&selected).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::read_dir(&selected).unwrap().count(),
            0,
            "probe is removed"
        );
        std::os::unix::fs::symlink(&selected, tree.0.join("link")).unwrap();
        assert!(select_history_storage(
            &tree.0.join("link"),
            &tree.0.join("not-a-directory/hyprv")
        )
        .is_err());
        let history = History::import_for_profile("+1\t-2\n1\t+2\n1\t-2\n", crate::Profile::Laptop);
        assert_eq!(history.tsv(), "1\t-2\n");
        assert_eq!(
            State::default().legacy_json_for_profile(crate::Profile::Laptop),
            serde_json::json!({"available":false})
        );
    }
}
