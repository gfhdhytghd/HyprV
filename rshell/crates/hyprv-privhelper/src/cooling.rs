use crate::{FanChannel, FanReading, FanState};
use std::{
    ffi::CString,
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

// User decision 2026-10-24: enter strictly above this raw temperature.
pub const EMERGENCY_MC: u32 = 105000;
pub const EMERGENCY_EXIT_MC: u32 = 100000;
pub const FAN_START_MC: u32 = 72000;
pub const FAN_STOP_MC: u32 = 65000;
pub const FAN_FULL_MC: u32 = 88000;
pub const MIN_START: u8 = 77;
pub const TEMPERATURE_EMA_SECONDS: f64 = 10.;
pub const PWM_RISE_PER_SECOND: f64 = 10.;
pub const PWM_FALL_PER_SECOND: f64 = 2.;
pub const STATE_PATH: &str = "/var/lib/hyprv-privhelper";
/// Legacy boot/non-desktop policy. Desktop operation uses DesktopCurve below.
pub fn target_pwm(limit: u8, temperature_mc: Option<u32>) -> u8 {
    let cap = u32::from(limit.min(100)) * 255 / 100;
    match temperature_mc {
        None => 255,
        Some(t) if t > EMERGENCY_MC => 255,
        Some(t) if t <= 70000 => 0,
        Some(t) if t <= 88000 => (cap * (t - 70000) / 18000) as u8,
        _ => cap as u8,
    }
}
#[derive(Default)]
struct DesktopCurve {
    temperature_mc: Option<f64>,
    running: [bool; 2],
    emergency: bool,
    channel_fault: [bool; 2],
    output: [Option<f64>; 2],
}
impl DesktopCurve {
    fn step(
        &mut self,
        raw: Option<u32>,
        limits: [u8; 2],
        fault: bool,
        elapsed: Duration,
        actual: [u8; 2],
    ) -> [u8; 2] {
        let dt = elapsed.as_secs_f64();
        if let Some(raw) = raw {
            let raw = f64::from(raw);
            let alpha = -(-dt / TEMPERATURE_EMA_SECONDS).exp_m1();
            self.temperature_mc = Some(
                self.temperature_mc
                    .map_or(raw, |old| old + alpha * (raw - old)),
            );
        } else {
            self.temperature_mc = None;
        }
        if fault || raw.is_none_or(|t| t > EMERGENCY_MC) {
            self.emergency = true;
        } else if raw.is_some_and(|t| t < EMERGENCY_EXIT_MC) {
            self.emergency = false;
            self.channel_fault = [false; 2];
        }
        std::array::from_fn(|i| {
            let cap = u32::from(limits[i].min(100)) * 255 / 100;
            if cap == 0 {
                self.running[i] = false;
            } else if let Some(t) = self.temperature_mc {
                if t >= f64::from(FAN_START_MC) {
                    self.running[i] = true;
                } else if t < f64::from(FAN_STOP_MC) {
                    self.running[i] = false;
                }
            }
            let floor = f64::from(u32::from(MIN_START).min(cap));
            let next = if self.emergency || self.channel_fault[i] {
                255.
            } else if cap == 0 || !self.running[i] {
                0.
            } else {
                let cap = f64::from(cap);
                let fraction = ((self.temperature_mc.unwrap_or(f64::from(FAN_FULL_MC))
                    - f64::from(FAN_START_MC))
                    / f64::from(FAN_FULL_MC - FAN_START_MC))
                .clamp(0., 1.);
                let target = floor + fraction * (cap - floor);
                // A fault handler may have raised hardware duty independently
                // of this curve. Keep fractional credit only when it still
                // describes the actual output; otherwise slew from hardware.
                let previous = self.output[i]
                    .filter(|v| v.floor() as u8 == actual[i])
                    .unwrap_or(f64::from(actual[i]));
                // Starting/stopping must not traverse the stall region. Hard
                // caps (including a newly reduced cap) override downward slew.
                if previous < floor {
                    floor
                } else {
                    target
                        .clamp(
                            previous - PWM_FALL_PER_SECOND * dt,
                            previous + PWM_RISE_PER_SECOND * dt,
                        )
                        .clamp(floor, cap)
                }
            };
            self.output[i] = Some(next);
            next.floor() as u8
        })
    }
}
fn number(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}
fn entries(path: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();
    paths.sort();
    paths
}
#[derive(Clone, Debug, Default)]
pub struct Hardware {
    pub chip: Option<PathBuf>,
    pub temperatures: Vec<PathBuf>,
}
impl Hardware {
    pub fn discover(hwmon_root: &Path) -> Self {
        let mut result = Self::default();
        for hwmon in entries(hwmon_root) {
            let name = std::fs::read_to_string(hwmon.join("name")).unwrap_or_default();
            match name.trim() {
                "nct6799" => result.chip = Some(hwmon.clone()),
                "coretemp" | "k10temp" | "zenpower" => {
                    for input in entries(&hwmon) {
                        let name = input.file_name().unwrap_or_default().to_string_lossy();
                        if !name.starts_with("temp") || !name.ends_with("_input") {
                            continue;
                        }
                        let label = std::fs::read_to_string(
                            input.with_file_name(name.replace("_input", "_label")),
                        )
                        .unwrap_or_default();
                        if label.trim().starts_with("Package id ")
                            || matches!(label.trim(), "Tctl" | "Tdie" | "CPU Package" | "CPU")
                        {
                            result.temperatures.push(input);
                        }
                    }
                }
                _ => (),
            }
        }
        result
    }
    pub fn temperature(&self) -> Option<u32> {
        // A failed sensor must not turn a multi-package hot system into a cool one.
        // Preserve over-range readings: the emergency policy handles them.
        self.temperatures
            .iter()
            .map(|p| number(p).filter(|v| *v > 0))
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .max()
    }
}
/// Open each directory component without following symlinks. All file operations
/// remain relative to the held directory FD even if its path is renamed.
struct StateDirectory {
    fd: OwnedFd,
    uid: u32,
}
impl StateDirectory {
    fn open(path: &Path) -> io::Result<Self> {
        Self::open_owned(path, 0, true)
    }
    fn open_owned(path: &Path, uid: u32, check_ancestors: bool) -> io::Result<Self> {
        if !path.is_absolute() {
            return Err(io::Error::other("state path must be absolute"));
        }
        let root = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open("/")?;
        let mut fd: OwnedFd = root.into();
        for component in path.components() {
            let std::path::Component::Normal(name) = component else {
                if component == std::path::Component::RootDir {
                    continue;
                }
                return Err(io::Error::other("invalid state component"));
            };
            let name = CString::new(name.as_encoded_bytes())?;
            let raw = unsafe {
                libc::openat(
                    fd.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            };
            if raw < 0 {
                return Err(io::Error::last_os_error());
            }
            fd = unsafe { OwnedFd::from_raw_fd(raw) };
            if check_ancestors {
                trusted(&File::from(fd.try_clone()?), uid)?;
            }
        }
        trusted(&File::from(fd.try_clone()?), uid)?;
        Ok(Self { fd, uid })
    }
    fn file(&self, name: &str, flags: i32) -> io::Result<File> {
        let name = CString::new(name)?;
        let raw = unsafe {
            libc::openat(
                self.fd.as_raw_fd(),
                name.as_ptr(),
                flags | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
                0o600,
            )
        };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        let file = unsafe { File::from_raw_fd(raw) };
        if !file.metadata()?.is_file() {
            return Err(io::Error::other("state attribute is not a regular file"));
        }
        trusted(&file, self.uid)?;
        if file.metadata()?.nlink() != 1 {
            return Err(io::Error::other("state file has multiple links"));
        }
        Ok(file)
    }
    fn save(&self, limits: [u8; 2]) -> io::Result<()> {
        let name = format!("fan-percent.{}.tmp", std::process::id());
        let mut file = self.file(&name, libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL)?;
        let result = (|| {
            writeln!(file, "chassis={}\npump={}", limits[0], limits[1])?;
            file.sync_all()?;
            let old = CString::new(name.clone())?;
            let new = CString::new("fan-percent")?;
            if unsafe {
                libc::renameat(
                    self.fd.as_raw_fd(),
                    old.as_ptr(),
                    self.fd.as_raw_fd(),
                    new.as_ptr(),
                )
            } < 0
            {
                return Err(io::Error::last_os_error());
            }
            if unsafe { libc::fsync(self.fd.as_raw_fd()) } < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        })();
        if result.is_err() {
            let name = CString::new(name)?;
            unsafe {
                libc::unlinkat(self.fd.as_raw_fd(), name.as_ptr(), 0);
            }
        }
        result
    }
}
fn trusted(file: &File, uid: u32) -> io::Result<()> {
    let m = file.metadata()?;
    if m.uid() != uid || m.mode() & 0o022 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "untrusted state ownership or mode",
        ));
    }
    Ok(())
}
fn lock(file: &File, nonblock: bool) -> io::Result<()> {
    if unsafe {
        libc::flock(
            file.as_raw_fd(),
            libc::LOCK_EX | if nonblock { libc::LOCK_NB } else { 0 },
        )
    } < 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
/// A single independently owned controller, with injectable roots for tests only.
pub struct Controller {
    hwmon_root: PathBuf,
    hardware: Hardware,
    directory: StateDirectory,
    _singleton: File,
    state_lock: File,
    pub saved_limits: [u8; 2],
    state_fault: Option<String>,
    desktop_curve: DesktopCurve,
    last_tick: Option<Instant>,
}
impl Controller {
    pub fn open(hwmon_root: PathBuf, state_dir: &Path) -> io::Result<Self> {
        // Even a persistence/setup error must leave hardware in a safe state.
        failsafe(&hwmon_root)?;
        let directory = StateDirectory::open(state_dir)?;
        Self::from_directory(hwmon_root, directory)
    }
    #[cfg(test)]
    fn open_test(hwmon_root: PathBuf, state_dir: &Path) -> io::Result<Self> {
        Self::from_directory(
            hwmon_root,
            StateDirectory::open_owned(state_dir, unsafe { libc::geteuid() }, false)?,
        )
    }
    fn from_directory(hwmon_root: PathBuf, directory: StateDirectory) -> io::Result<Self> {
        let singleton = directory.file("fan-control.lock", libc::O_RDWR | libc::O_CREAT)?;
        lock(&singleton, true)?;
        let state_lock = directory.file("fan-percent.lock", libc::O_RDWR | libc::O_CREAT)?;
        let hardware = Hardware::discover(&hwmon_root);
        let (saved_limits, state_fault) = match directory.file("fan-percent", libc::O_RDONLY) {
            Ok(file) => match parse_limits(file) {
                Ok(limits) => (limits, None),
                Err(e) => ([80, 80], Some(format!("invalid saved limits: {e}"))),
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => ([80, 80], None),
            Err(e) => (
                [80, 80],
                Some(format!("untrusted/unreadable saved limits: {e}")),
            ),
        };
        Ok(Self {
            hwmon_root,
            hardware,
            directory,
            _singleton: singleton,
            state_lock,
            saved_limits,
            state_fault,
            desktop_curve: DesktopCurve::default(),
            last_tick: None,
        })
    }
    pub fn rediscover(&mut self) {
        let next = Hardware::discover(&self.hwmon_root);
        if next.chip != self.hardware.chip || next.temperatures != self.hardware.temperatures {
            // New paths invalidate filter/output history, not a safety latch:
            // only a healthy raw reading below the exit threshold clears it.
            self.desktop_curve = DesktopCurve {
                emergency: self.desktop_curve.emergency,
                channel_fault: self.desktop_curve.channel_fault,
                ..DesktopCurve::default()
            };
            self.last_tick = None;
        }
        self.hardware = next;
    }
    pub fn available(&self) -> bool {
        self.hardware.chip.is_some()
    }
    pub fn set_limit(
        &mut self,
        channel: FanChannel,
        percent: u8,
        desktop: bool,
    ) -> io::Result<FanState> {
        if percent > 100 {
            return Err(io::Error::other("percentage outside 0..100"));
        }
        if !self.available() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "nct6799 unavailable",
            ));
        }
        lock(&self.state_lock, true)?;
        let index = match channel {
            FanChannel::Chassis => 0,
            FanChannel::Pump => 1,
        };
        let mut next = self.saved_limits;
        next[index] = percent;
        let result = self.directory.save(next);
        unsafe {
            libc::flock(self.state_lock.as_raw_fd(), libc::LOCK_UN);
        }
        result?;
        self.saved_limits = next;
        self.state_fault = None;
        self.tick(desktop).map_err(|e| {
            io::Error::new(e.kind(), format!("fan limit saved; PWM update failed: {e}"))
        })
    }
    fn validate_chip(&self) -> io::Result<&Path> {
        let chip = self
            .hardware
            .chip
            .as_deref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "nct6799 unavailable"))?;
        if std::fs::read_to_string(chip.join("name"))?.trim() != "nct6799" {
            return Err(io::Error::other("hwmon identity changed"));
        }
        Ok(chip)
    }
    pub fn read(&self, desktop: bool) -> io::Result<FanState> {
        if !self.available() {
            return Ok(FanState::default());
        }
        let chip = self.validate_chip()?;
        let temperature = self.hardware.temperature();
        let mut channels = Vec::new();
        for (i, channel) in [FanChannel::Chassis, FanChannel::Pump].iter().enumerate() {
            let index = channel.index();
            let pwm = if number(&chip.join(format!("pwm{index}_enable"))) == Some(0) {
                255
            } else {
                number(&chip.join(format!("pwm{index}")))
                    .filter(|p| *p <= 255)
                    .ok_or_else(|| io::Error::other("PWM unavailable"))? as u8
            };
            channels.push(FanReading {
                channel: *channel,
                limit_percent: self.saved_limits[i],
                pwm,
                rpm: number(&chip.join(format!("fan{index}_input"))),
            });
        }
        Ok(FanState {
            available: true,
            chip: "nct6799".into(),
            temperature_c: temperature.map(|v| v as f64 / 1000.),
            desktop_policy: desktop,
            channels,
            last_error: self.state_fault.clone().or_else(|| match temperature {
                None => Some("CPU temperature unavailable; maximum cooling".into()),
                Some(t) if t > EMERGENCY_MC => {
                    Some("emergency CPU temperature; maximum cooling".into())
                }
                _ if desktop
                    && (self.desktop_curve.emergency
                        || self.desktop_curve.channel_fault.iter().any(|v| *v)) =>
                {
                    Some(format!(
                        "emergency cooling latched until raw CPU temperature below {}C",
                        EMERGENCY_EXIT_MC / 1000
                    ))
                }
                _ => None,
            }),
        })
    }
    pub fn tick(&mut self, desktop: bool) -> io::Result<FanState> {
        self.tick_at(desktop, Instant::now())
    }
    fn tick_at(&mut self, desktop: bool, now: Instant) -> io::Result<FanState> {
        if !self.available() {
            return Ok(FanState::default());
        }
        let chip = Chip::open(self.validate_chip()?)?;
        let temperature = self.hardware.temperature();
        let elapsed = self
            .last_tick
            .map_or(Duration::ZERO, |last| now.saturating_duration_since(last));
        self.last_tick = Some(now);
        let targets = if desktop {
            let actual = [1, 7].map(|index| {
                number(
                    &self
                        .hardware
                        .chip
                        .as_ref()
                        .unwrap()
                        .join(format!("pwm{index}")),
                )
                .filter(|v| *v <= 255)
                .unwrap_or(255) as u8
            });
            self.desktop_curve.step(
                temperature,
                self.saved_limits,
                self.state_fault.is_some(),
                elapsed,
                actual,
            )
        } else {
            self.desktop_curve = DesktopCurve::default();
            [target_pwm(80, temperature); 2]
        };
        let mut errors = Vec::new();
        for (i, index) in [1, 7].iter().enumerate() {
            let target = targets[i];
            let fault = self.state_fault.is_some()
                || temperature.is_none()
                || temperature.is_some_and(|t| t > EMERGENCY_MC)
                || (desktop
                    && (self.desktop_curve.emergency || self.desktop_curve.channel_fault[i]));
            let result = if fault {
                chip.safe(*index)
            } else {
                chip.manual(*index, target)
            };
            if let Err(e) = result {
                let recovery = chip.safe(*index);
                errors.push(format!("pwm{index}: {e}; recovery: {recovery:?}"));
                self.desktop_curve.output[i] = Some(255.);
                self.desktop_curve.channel_fault[i] = true;
            }
        }
        if !errors.is_empty() {
            return Err(io::Error::other(errors.join("; ")));
        }
        self.read(desktop)
    }
    pub fn fail_safe(&self) -> io::Result<()> {
        match &self.hardware.chip {
            Some(path) => Chip::open(path)?.safe_all(),
            None => Ok(()),
        }
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        if let Err(e) = self.fail_safe() {
            eprintln!("cooling handoff failed: {e}");
        }
    }
}

fn parse_limits(file: File) -> io::Result<[u8; 2]> {
    let mut bytes = Vec::new();
    file.take(4097).read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        return Err(io::Error::other("saved limits too large"));
    }
    let text = std::str::from_utf8(&bytes).map_err(io::Error::other)?;
    let mut values = [None, None];
    for line in text.lines() {
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| io::Error::other("invalid limit line"))?;
        let index = match key {
            "chassis" => 0,
            "pump" => 1,
            _ => return Err(io::Error::other("unknown channel")),
        };
        let value = value
            .parse::<u8>()
            .ok()
            .filter(|v| *v <= 100)
            .ok_or_else(|| io::Error::other("invalid percent"))?;
        if values[index].replace(value).is_some() {
            return Err(io::Error::other("duplicate channel"));
        }
    }
    Ok([
        values[0].ok_or_else(|| io::Error::other("missing chassis"))?,
        values[1].ok_or_else(|| io::Error::other("missing pump"))?,
    ])
}

/// Held chip directory pins identity across attribute opens. Only fixed attribute
/// names generated from the two allowlisted channels are ever written.
struct Chip(File);
impl Chip {
    fn open(path: &Path) -> io::Result<Self> {
        let chip = Self(
            OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
                .open(path)?,
        );
        let mut name = String::new();
        chip.attribute("name", libc::O_RDONLY)?
            .read_to_string(&mut name)?;
        if name.trim() != "nct6799" {
            return Err(io::Error::other("hwmon identity changed"));
        }
        Ok(chip)
    }
    fn attribute(&self, name: &str, flags: i32) -> io::Result<File> {
        let name = CString::new(name)?;
        let fd = unsafe {
            libc::openat(
                self.0.as_raw_fd(),
                name.as_ptr(),
                flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }
    fn write(&self, name: &str, value: u32) -> io::Result<()> {
        let mut current = String::new();
        if self
            .attribute(name, libc::O_RDONLY)?
            .read_to_string(&mut current)
            .is_ok()
            && current.trim().parse::<u32>().ok() == Some(value)
        {
            return Ok(());
        }
        write_attribute(
            &mut self.attribute(name, libc::O_WRONLY | libc::O_TRUNC)?,
            value,
        )
    }
    fn safe(&self, index: u8) -> io::Result<()> {
        // Manual mode is unconditional on nct6799; auto modes depend on firmware
        // trip points. store_pwm rejects duty changes in automatic modes.
        maximum_speed(|name, value| self.write(name, value), index)
    }
    fn safe_all(&self) -> io::Result<()> {
        let errors: Vec<_> = [1, 7]
            .into_iter()
            .filter_map(|i| self.safe(i).err().map(|e| format!("pwm{i}: {e}")))
            .collect();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(io::Error::other(errors.join("; ")))
        }
    }
    fn manual(&self, index: u8, value: u8) -> io::Result<()> {
        // Open both before changing either; transition via maximum speed instead
        // of exposing a stale low manual duty between the two writes.
        let pwm = format!("pwm{index}");
        let mode = format!("pwm{index}_enable");
        self.attribute(&pwm, libc::O_WRONLY)?;
        self.attribute(&mode, libc::O_WRONLY)?;
        let mut current = String::new();
        self.attribute(&mode, libc::O_RDONLY)?
            .read_to_string(&mut current)?;
        if current.trim() == "1" {
            return self.write(&pwm, value.into());
        }
        self.safe(index)?;
        self.write(&pwm, value.into())
    }
}
// A sysfs store is one syscall, not a stream: write_fmt may split "255" and
// "\n", and the second store returns EINVAL after the first already took effect.
fn write_attribute(out: &mut impl Write, value: u32) -> io::Result<()> {
    let bytes = format!("{value}\n");
    match out.write(bytes.as_bytes())? {
        n if n == bytes.len() => Ok(()),
        _ => Err(io::Error::new(
            io::ErrorKind::WriteZero,
            "short sysfs store",
        )),
    }
}
fn maximum_speed(mut write: impl FnMut(&str, u32) -> io::Result<()>, index: u8) -> io::Result<()> {
    write(&format!("pwm{index}_enable"), 1)?;
    write(&format!("pwm{index}"), 255)
}
/// Read/open-only capability check. Never changes hardware, state files or locks.
/// Root callers also check write-open permission without issuing a store.
pub fn check(hwmon_root: &Path) -> io::Result<()> {
    let chips: Vec<_> = entries(hwmon_root)
        .into_iter()
        .filter(|p| std::fs::read_to_string(p.join("name")).is_ok_and(|n| n.trim() == "nct6799"))
        .collect();
    if chips.len() > 1 {
        return Err(io::Error::other("ambiguous nct6799 chips"));
    }
    let Some(path) = chips.first() else {
        return Ok(());
    };
    let chip = Chip::open(path)?;
    for index in [1, 7] {
        for suffix in ["", "_enable"] {
            let name = format!("pwm{index}{suffix}");
            let mut value = String::new();
            chip.attribute(&name, libc::O_RDONLY)?
                .read_to_string(&mut value)?;
            let value: u32 = value.trim().parse().map_err(io::Error::other)?;
            if (suffix.is_empty() && value > 255)
                || (!suffix.is_empty() && ![0, 1, 2, 3, 5].contains(&value))
            {
                return Err(io::Error::other("invalid nct6799 PWM state"));
            }
            if unsafe { libc::geteuid() } == 0 {
                chip.attribute(&name, libc::O_WRONLY)?;
            }
        }
    }
    Ok(())
}
/// Crash/startup recovery deliberately does not depend on state files or locks.
pub fn failsafe(hwmon_root: &Path) -> io::Result<()> {
    if entries(hwmon_root)
        .iter()
        .filter(|p| std::fs::read_to_string(p.join("name")).is_ok_and(|n| n.trim() == "nct6799"))
        .count()
        > 1
    {
        return Err(io::Error::other("ambiguous nct6799 chips"));
    }
    match Hardware::discover(hwmon_root).chip {
        Some(path) => Chip::open(&path)?.safe_all(),
        None => Ok(()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn curve_is_capped_and_floored() {
        for limit in 0..=100 {
            for t in [0, 70000, 70001, 79000, 88000] {
                assert!(target_pwm(limit, Some(t)) <= (u32::from(limit) * 255 / 100) as u8);
            }
        }
        assert_eq!(target_pwm(80, Some(70000)), 0);
        assert_eq!(target_pwm(80, Some(79000)), 102);
        assert_eq!(target_pwm(80, None), 255);
    }
}

#[cfg(test)]
mod controller_tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new(label: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("hyprv-cooling-{}-{label}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            for path in ["state", "hwmon/hwmon0", "hwmon/hwmon1"] {
                std::fs::create_dir_all(root.join(path)).unwrap();
            }
            for (name, value) in [
                ("name", "nct6799"),
                ("pwm1", "255"),
                ("pwm7", "255"),
                ("pwm1_enable", "2"),
                ("pwm7_enable", "2"),
                ("fan1_input", "500"),
                ("fan7_input", "600"),
            ] {
                std::fs::write(root.join("hwmon/hwmon0").join(name), value).unwrap();
            }
            for (name, value) in [
                ("name", "coretemp"),
                ("temp1_input", "79000"),
                ("temp1_label", "Package id 0"),
            ] {
                std::fs::write(root.join("hwmon/hwmon1").join(name), value).unwrap();
            }
            std::fs::write(root.join("state/fan-percent"), "chassis=40\npump=60\n").unwrap();
            Self(root)
        }
        fn open(&self) -> Controller {
            Controller::open_test(self.0.join("hwmon"), &self.0.join("state")).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn boot_desktop_saved_limits_and_missing_telemetry() {
        let fixture = Fixture::new("policy");
        let mut c = fixture.open();
        assert_eq!(c.saved_limits, [40, 60]);
        let boot = c.tick(false).unwrap();
        assert_eq!(boot.channels[0].limit_percent, 40);
        assert_eq!(boot.channels[0].pwm, 102);
        c.set_limit(FanChannel::Chassis, 100, false).unwrap();
        assert_eq!(c.tick(false).unwrap().channels[0].pwm, 102);
        let desktop = c.tick(true).unwrap();
        assert!((MIN_START..=255).contains(&desktop.channels[0].pwm));
        assert!((MIN_START..=153).contains(&desktop.channels[1].pwm));
        assert_eq!(
            std::fs::read_to_string(fixture.0.join("state/fan-percent")).unwrap(),
            "chassis=100\npump=60\n"
        );
        std::fs::remove_file(fixture.0.join("hwmon/hwmon1/temp1_input")).unwrap();
        let missing = c.tick(true).unwrap();
        assert_eq!(missing.temperature_c, None);
        assert_eq!(missing.channels[0].pwm, 255);
        assert_eq!(missing.channels[1].pwm, 255);
    }
    #[test]
    fn quiet_desktop_controller_uses_clock_hard_caps_and_raw_emergency_but_boot_stays_legacy() {
        let fixture = Fixture::new("quiet-clock");
        let temp = fixture.0.join("hwmon/hwmon1/temp1_input");
        for i in [1, 7] {
            std::fs::write(fixture.0.join(format!("hwmon/hwmon0/pwm{i}")), "0").unwrap();
            std::fs::write(fixture.0.join(format!("hwmon/hwmon0/pwm{i}_enable")), "1").unwrap();
        }
        let mut c = fixture.open();
        c.saved_limits = [80, 10];
        let now = Instant::now();
        std::fs::write(&temp, "72000").unwrap();
        let state = c.tick_at(true, now).unwrap();
        assert_eq!([state.channels[0].pwm, state.channels[1].pwm], [77, 25]);
        std::fs::write(&temp, "88000").unwrap();
        assert_eq!(
            c.tick_at(true, now + Duration::from_secs(1))
                .unwrap()
                .channels[0]
                .pwm,
            87
        );
        assert_eq!(
            c.tick_at(true, now + Duration::from_secs(1))
                .unwrap()
                .channels[0]
                .pwm,
            87
        );
        std::fs::write(&temp, "105001").unwrap();
        assert_eq!(
            c.tick_at(true, now + Duration::from_secs(2))
                .unwrap()
                .channels[0]
                .pwm,
            255
        );
        std::fs::write(&temp, "100000").unwrap();
        let latched = c.tick_at(true, now + Duration::from_secs(3)).unwrap();
        assert_eq!(latched.channels[1].pwm, 255);
        assert!(latched.last_error.unwrap().contains("latched"));
        std::fs::write(&temp, "99999").unwrap();
        let clear = c.tick_at(true, now + Duration::from_secs(4)).unwrap();
        assert!(clear.channels[0].pwm <= 204);
        assert_eq!(clear.channels[1].pwm, 25);
        std::fs::write(&temp, "79000").unwrap();
        let boot = c.tick_at(false, now + Duration::from_secs(5)).unwrap();
        assert_eq!([boot.channels[0].pwm, boot.channels[1].pwm], [102; 2]);
        assert_eq!(c.saved_limits, [80, 10]);
        assert!(c.desktop_curve.temperature_mc.is_none());
    }

    #[test]
    fn zero_cap_controller_obeys_strict_emergency_boundaries_and_sensor_faults() {
        let f = Fixture::new("strict-emergency-boundaries");
        let temp = f.0.join("hwmon/hwmon1/temp1_input");
        let mut c = f.open();
        c.saved_limits = [0; 2];
        let now = Instant::now();
        for (t, expected) in [
            (105000, 0),
            (105001, 255),
            (105000, 255),
            (100000, 255),
            (99999, 0),
        ] {
            std::fs::write(&temp, t.to_string()).unwrap();
            let state = c.tick_at(true, now).unwrap();
            assert_eq!(
                [state.channels[0].pwm, state.channels[1].pwm],
                [expected; 2],
                "T={t}"
            );
            assert_eq!(state.last_error.is_some(), expected == 255, "T={t}");
        }
        std::fs::write(&temp, "unreadable").unwrap();
        let state = c.tick_at(true, now).unwrap();
        assert_eq!([state.channels[0].pwm, state.channels[1].pwm], [255; 2]);
        assert!(state.last_error.is_some());
    }

    #[test]
    fn rediscovery_preserves_ema_and_emergency_until_raw_temperature_is_safe() {
        let f = Fixture::new("quiet-hotplug-latch");
        let temp = f.0.join("hwmon/hwmon1/temp1_input");
        let mut c = f.open();
        let now = Instant::now();
        std::fs::write(&temp, "105001").unwrap();
        c.tick_at(true, now).unwrap();
        let filtered = c.desktop_curve.temperature_mc;
        c.rediscover();
        assert_eq!(c.desktop_curve.temperature_mc, filtered);
        assert!(c.desktop_curve.emergency);
        std::fs::write(&temp, "100000").unwrap();
        assert_eq!(
            c.tick_at(true, now + Duration::from_secs(1))
                .unwrap()
                .channels[0]
                .pwm,
            255
        );
        std::fs::rename(f.0.join("hwmon/hwmon0"), f.0.join("hwmon/hwmon2")).unwrap();
        c.rediscover();
        assert!(c.desktop_curve.temperature_mc.is_none());
        assert!(c.desktop_curve.emergency);
        assert_eq!(
            c.tick_at(true, now + Duration::from_secs(2))
                .unwrap()
                .channels[0]
                .pwm,
            255
        );
        std::fs::write(&temp, "99999").unwrap();
        assert!(
            c.tick_at(true, now + Duration::from_secs(3))
                .unwrap()
                .channels[0]
                .pwm
                <= 102
        );
    }

    #[test]
    fn singleton_identity_and_hotplug() {
        let fixture = Fixture::new("identity");
        let mut c = fixture.open();
        assert!(Controller::open_test(fixture.0.join("hwmon"), &fixture.0.join("state")).is_err());
        std::fs::write(fixture.0.join("hwmon/hwmon0/name"), "other").unwrap();
        assert!(c.tick(true).is_err());
        c.rediscover();
        assert!(!c.available());
        assert!(!c.tick(true).unwrap().available);
        std::fs::write(fixture.0.join("hwmon/hwmon0/name"), "nct6799").unwrap();
        c.rediscover();
        assert!(c.tick(true).unwrap().available);
    }
    #[test]
    fn symlinked_state_directory_is_rejected() {
        let fixture = Fixture::new("symlink");
        std::os::unix::fs::symlink(fixture.0.join("state"), fixture.0.join("redirect")).unwrap();
        assert!(
            Controller::open_test(fixture.0.join("hwmon"), &fixture.0.join("redirect")).is_err()
        );
    }
    #[test]
    fn write_permission_failure_is_reported() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 {
            return;
        } // Root's DAC override cannot exercise a denied write.
        let fixture = Fixture::new("denied");
        let mut controller = fixture.open();
        std::fs::set_permissions(
            fixture.0.join("hwmon/hwmon0/pwm1"),
            std::fs::Permissions::from_mode(0o400),
        )
        .unwrap();
        let error = controller.tick(true).unwrap_err();
        assert!(error.to_string().contains("Permission denied"));
    }
}

#[cfg(test)]
#[path = "cooling_regression.rs"]
mod regression;

#[cfg(test)]
mod quiet_curve_tests {
    use super::*;
    fn step(c: &mut DesktopCurve, t: u32, limits: [u8; 2], seconds: f64) -> [u8; 2] {
        let actual = c.output.map(|v| v.unwrap_or(0.).floor() as u8);
        c.step(
            Some(t),
            limits,
            false,
            Duration::from_secs_f64(seconds),
            actual,
        )
    }
    #[test]
    fn boot_emergency_entry_is_strict_and_faults_override_zero_cap() {
        assert_eq!(EMERGENCY_MC, 105000);
        assert_eq!(EMERGENCY_EXIT_MC, 100000);
        for t in [90000, 100000, 104999, 105000] {
            assert_eq!(target_pwm(0, Some(t)), 0);
            assert_eq!(target_pwm(80, Some(t)), 204);
        }
        assert_eq!(target_pwm(0, Some(105001)), 255);
        assert_eq!(target_pwm(0, None), 255);
    }
    #[test]
    fn startup_floor_caps_and_hysteresis() {
        let mut c = DesktopCurve::default();
        assert_eq!(step(&mut c, 71999, [80, 80], 1.), [0; 2]);
        assert_eq!(step(&mut c, FAN_START_MC, [80, 80], 1000.), [MIN_START; 2]);
        assert_eq!(step(&mut c, 66000, [80, 80], 1000.), [MIN_START; 2]);
        assert_eq!(step(&mut c, FAN_STOP_MC, [80, 80], 1000.), [MIN_START; 2]);
        assert_eq!(step(&mut c, FAN_STOP_MC - 1, [80, 80], 1000.), [0; 2]);
        assert_eq!(step(&mut c, 70000, [80, 80], 1000.), [0; 2]);
        assert_eq!(step(&mut c, FAN_START_MC, [10, 0], 1000.), [25, 0]);
    }
    #[test]
    fn steady_curve_is_linear_to_cap_at_88_and_never_stalls() {
        for limit in 0..=100 {
            let cap = u32::from(limit) * 255 / 100;
            for t in [72000, 76000, 80000, 84000, 88000, 89999] {
                let mut c = DesktopCurve::default();
                step(&mut c, t, [limit; 2], 1.);
                let out = step(&mut c, t, [limit; 2], 1000.);
                let floor = u32::from(MIN_START).min(cap);
                let want = floor
                    + (cap - floor) * (t - FAN_START_MC).min(FAN_FULL_MC - FAN_START_MC)
                        / (FAN_FULL_MC - FAN_START_MC);
                assert_eq!(out, [want as u8; 2], "limit={limit}, T={t}");
                assert!(out[0] == 0 || u32::from(out[0]) >= floor);
            }
        }
    }
    #[test]
    fn ema_suppresses_short_package_spikes_and_uses_elapsed_time() {
        let mut c = DesktopCurve::default();
        step(&mut c, 60000, [80; 2], 1.);
        assert_eq!(step(&mut c, 88000, [80; 2], 1.), [0; 2]);
        let want = 60000. + (88000. - 60000.) * (1. - (-0.1f64).exp());
        assert!((c.temperature_mc.unwrap() - want).abs() < 0.0001);
        let previous = c.temperature_mc;
        step(&mut c, 88000, [80; 2], 0.);
        assert_eq!(
            c.temperature_mc, previous,
            "extra D-Bus calls do not accelerate EMA"
        );
        let mut a = DesktopCurve::default();
        let mut b = DesktopCurve::default();
        step(&mut a, 60000, [80; 2], 0.);
        step(&mut b, 60000, [80; 2], 0.);
        step(&mut a, 88000, [80; 2], 10.);
        for _ in 0..10 {
            step(&mut b, 88000, [80; 2], 1.);
        }
        assert!((a.temperature_mc.unwrap() - b.temperature_mc.unwrap()).abs() < 0.0001);
    }
    #[test]
    fn slew_is_per_second_in_both_directions_and_preserves_subsecond_credit() {
        let mut c = DesktopCurve::default();
        assert_eq!(step(&mut c, 88000, [100; 2], 0.), [MIN_START; 2]);
        assert_eq!(step(&mut c, 88000, [100; 2], 1.), [87; 2]);
        assert_eq!(step(&mut c, 88000, [100; 2], 0.5), [92; 2]);
        assert_eq!(step(&mut c, 88000, [100; 2], 0.), [92; 2]);
        step(&mut c, 88000, [100; 2], 30.);
        c.temperature_mc = Some(72000.);
        assert_eq!(step(&mut c, 72000, [100; 2], 1.), [253; 2]);
        for _ in 0..10 {
            step(&mut c, 72000, [100; 2], 0.1);
        }
        assert!((c.output[0].unwrap() - 251.).abs() < 0.0001);
    }
    #[test]
    fn raw_emergency_bypasses_filter_slew_and_zero_cap_until_below_100() {
        let mut c = DesktopCurve::default();
        step(&mut c, 60000, [0, 10], 1.);
        for t in [90000, 100000, 104999, 105000] {
            let out = step(&mut c, t, [0, 10], 0.);
            assert!(out[0] == 0 && out[1] <= 25, "raw T={t}: {out:?}");
            assert!(!c.emergency);
        }
        assert_eq!(step(&mut c, 105001, [0, 10], 0.), [255; 2]);
        for t in [105000, 102000, 100000] {
            assert_eq!(step(&mut c, t, [0, 10], 1.), [255; 2]);
        }
        let out = step(&mut c, 99999, [0, 10], 1.);
        assert!(out[0] == 0 && out[1] <= 25);
        assert!(!c.emergency);
        assert_eq!(
            c.step(None, [0; 2], false, Duration::ZERO, [0; 2]),
            [255; 2]
        );
        assert_eq!(step(&mut c, 102000, [0; 2], 1.), [255; 2]);
        assert_eq!(step(&mut c, 99999, [0; 2], 1.), [0; 2]);
        assert_eq!(
            c.step(Some(60000), [0; 2], true, Duration::ZERO, [0; 2]),
            [255; 2]
        );
    }
    #[test]
    fn external_failsafe_duty_is_used_instead_of_stale_curve_output() {
        let mut c = DesktopCurve::default();
        assert_eq!(step(&mut c, 72000, [100; 2], 0.), [77; 2]);
        assert_eq!(
            c.step(
                Some(72000),
                [100; 2],
                false,
                Duration::from_secs(1),
                [255; 2]
            ),
            [253; 2]
        );
    }

    #[test]
    fn reenabled_channel_must_reach_start_threshold_again() {
        let mut c = DesktopCurve::default();
        assert_eq!(step(&mut c, 72000, [0, 80], 0.), [0, 77]);
        assert_eq!(step(&mut c, 70000, [0, 80], 1000.), [0, 77]);
        assert_eq!(step(&mut c, 70000, [80; 2], 0.), [0, 77]);
        assert_eq!(step(&mut c, 72000, [80; 2], 1000.), [77; 2]);
    }

    #[test]
    fn channel_fault_latch_does_not_force_healthy_channel() {
        let mut c = DesktopCurve::default();
        step(&mut c, 102000, [10; 2], 0.);
        c.channel_fault[0] = true;
        c.output[0] = Some(255.);
        assert_eq!(step(&mut c, 102000, [10; 2], 1.), [255, 25]);
        assert_eq!(step(&mut c, 100000, [10; 2], 1.), [255, 25]);
        assert_eq!(step(&mut c, 99999, [10; 2], 1.), [25; 2]);
    }

    #[test]
    fn cap_reduction_is_immediate_and_channels_are_independent() {
        let mut c = DesktopCurve::default();
        step(&mut c, 88000, [100; 2], 1.);
        step(&mut c, 88000, [100; 2], 1000.);
        assert_eq!(step(&mut c, 88000, [10, 80], 0.), [25, 204]);
        assert_eq!(step(&mut c, 88000, [0, 80], 0.), [0, 204]);
    }
}
