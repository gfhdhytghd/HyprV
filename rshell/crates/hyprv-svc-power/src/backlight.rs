use crate::actor::{self, Backend, Context, Handle, Result};
use async_trait::async_trait;
use hyprv_privhelper::events::KernelEvents;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BacklightBackend {
    Sysfs { device: PathBuf },
    Ddc { connector: String, bus: u32 },
}
impl Default for BacklightBackend {
    fn default() -> Self {
        Self::Sysfs {
            device: PathBuf::new(),
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OutputState {
    pub output: String,
    pub backend: BacklightBackend,
    pub level_percent: f64,
    pub raw: Option<u32>,
    pub raw_max: Option<u32>,
    pub reference_luminance_nits: f64,
    pub request_sequence: u64,
    pub confirmed_sequence: Option<u64>,
    pub dragging: bool,
    pub failed: bool,
    pub retries: u8,
    pub last_error: Option<String>,
}
pub type BrightnessState = OutputState;
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub outputs: Vec<OutputState>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Action {
    Refresh,
    Restore {
        output: String,
    },
    Begin {
        output: String,
    },
    Set {
        output: String,
        percent: f64,
        sequence: u64,
    },
    Finish {
        output: String,
        sequence: u64,
    },
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CurveKind {
    #[default]
    Desktop,
    Laptop,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutputConfig {
    pub output: String,
    pub mode: String,
    pub backend: BacklightBackend,
    pub desktop: f64,
    pub peak: f64,
    pub sdr_min: f64,
    pub monitor_fields: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub curve: CurveKind,
    #[serde(default)]
    pub auto_brightness: bool,
}
impl OutputConfig {
    pub fn validate(&self) -> Result<()> {
        if self.output.is_empty()
            || self.output.len() > 128
            || self
                .output
                .bytes()
                .any(|b| !b.is_ascii_alphanumeric() && !b"_.:-".contains(&b))
        {
            return Err("invalid output identity".into());
        }
        if !self.desktop.is_finite()
            || !self.peak.is_finite()
            || self.desktop <= 10.
            || self.peak <= self.desktop
            || !self.sdr_min.is_finite()
        {
            return Err("invalid luminance profile".into());
        }
        Ok(())
    }
}
pub fn hardware_percent(level: f64) -> f64 {
    let old = ((level.clamp(0., 100.) - 10.) * 70. / 60.).clamp(0., 70.);
    if old <= 10. {
        old
    } else {
        10. * 10f64.powf((old - 10.) / 60.)
    }
}
pub fn plan(level: f64, max: u32, backlight: bool) -> u32 {
    (max as f64 * hardware_percent(level) / 100.)
        .round()
        .clamp(if backlight { 1. } else { 0. }, max.max(1) as f64) as u32
}
pub fn plan_for_config(level: f64, max: u32, c: &OutputConfig) -> u32 {
    if c.curve == CurveKind::Desktop {
        return plan(
            level,
            max,
            matches!(c.backend, BacklightBackend::Sysfs { .. }),
        );
    }
    let u = level.clamp(0., 70.);
    let p = if u <= 10. {
        u
    } else {
        10. * 10f64.powf((u - 10.) / 60.)
    };
    (max as f64 * p / 100.).round().clamp(1., max.max(1) as f64) as u32
}
pub fn luminance(level: f64, c: &OutputConfig) -> f64 {
    if c.curve == CurveKind::Laptop {
        return (c.desktop + (c.peak - c.desktop) * ((level - 70.) / 30.).clamp(0., 1.)).round();
    }
    if level < 10. {
        (10. + (c.desktop - 10.) * (level / 10.).clamp(0., 1.)).round()
    } else {
        (c.desktop + (c.peak - c.desktop) * ((level - 70.) / 30.).clamp(0., 1.)).round()
    }
}
pub fn raw_level(raw: u32, max: u32, reference: f64, c: &OutputConfig) -> f64 {
    if max == 0 {
        return 70.;
    }
    if c.curve == CurveKind::Laptop {
        if raw < max {
            let p = 100. * raw as f64 / max as f64;
            return if p <= 10. {
                p.round()
            } else {
                (10. + 60. * (p / 10.).log10()).round()
            };
        }
        return (70. + 30. * ((reference - c.desktop) / (c.peak - c.desktop)).clamp(0., 1.))
            .round();
    }
    if raw == plan(0., max, matches!(c.backend, BacklightBackend::Sysfs { .. }))
        && reference < c.desktop
    {
        return (10. * ((reference - 10.) / (c.desktop - 10.)).clamp(0., 1.)).round();
    }
    if raw < max {
        let percent = 100. * raw as f64 / max as f64;
        let old = if percent <= 10. {
            percent
        } else {
            10. + 60. * (percent / 10.).log10()
        };
        (10. + old * 60. / 70.).round()
    } else {
        (70. + 30. * ((reference - c.desktop) / (c.peak - c.desktop)).clamp(0., 1.)).round()
    }
}
#[async_trait]
pub trait Transport: Send + Sync + 'static {
    async fn read(&self, config: &OutputConfig) -> Result<(u32, u32)>;
    async fn write(&self, config: &OutputConfig, raw: u32) -> Result<()>;
    async fn reference(&self, config: &OutputConfig, nits: f64) -> Result<()>;
    async fn persist(&self, config: &OutputConfig, level: f64) -> Result<()>;
    /// Laptop legacy path (laptop shell.qml): hand the level to `hypr/controls.lua`, which
    /// owns the backlight, the SDR reference and any display synced to it. `false` means
    /// unavailable, and the native curve below is used instead.
    async fn controls_set(&self, _level: f64) -> Result<bool> {
        Ok(false)
    }
    /// `brightness_get()` from `hypr/controls.lua`: the user's target while one is held.
    async fn controls_get(&self) -> Result<Option<f64>> {
        Ok(None)
    }
}
pub struct Engine<T: Transport> {
    pub transport: T,
    pub configs: Vec<OutputConfig>,
    pub state: State,
    saved: Vec<f64>,
    latest: Arc<std::sync::Mutex<BTreeMap<String, u64>>>,
}
impl<T: Transport> Engine<T> {
    pub fn new(transport: T, configs: Vec<OutputConfig>) -> Result<Self> {
        for c in &configs {
            c.validate()?;
        }
        let state = State {
            outputs: configs
                .iter()
                .map(|c| OutputState {
                    output: c.output.clone(),
                    backend: c.backend.clone(),
                    level_percent: 70.,
                    reference_luminance_nits: c.desktop,
                    ..Default::default()
                })
                .collect(),
        };
        let saved = vec![70.; configs.len()];
        Ok(Self {
            saved,
            latest: Arc::default(),
            transport,
            configs,
            state,
        })
    }
    pub async fn refresh(&mut self) -> Result<()> {
        for (i, c) in self.configs.iter().enumerate() {
            if c.curve != CurveKind::Laptop && self.state.outputs[i].dragging {
                continue;
            }
            if c.curve == CurveKind::Laptop {
                if let Some(level) = self.transport.controls_get().await? {
                    self.state.outputs[i].level_percent = level.round().clamp(0., 100.);
                    continue;
                }
            }
            match self.transport.read(c).await {
                Ok((raw, max)) => {
                    if max == 0 || raw > max {
                        return Err("invalid brightness range".into());
                    }
                    let s = &mut self.state.outputs[i];
                    let previous_raw = s.raw;
                    s.raw = Some(raw);
                    s.raw_max = Some(max);
                    // Laptop probes are accepted even during interaction (shell.qml
                    // quickAdjustBrightnessProbe); desktop retains serial guards.
                    if c.curve == CurveKind::Laptop
                        || s.request_sequence == 0
                        || (s.confirmed_sequence == Some(s.request_sequence)
                            && previous_raw != Some(raw))
                    {
                        s.level_percent = raw_level(raw, max, s.reference_luminance_nits, c);
                    }
                }
                Err(e) => {
                    self.state.outputs[i].last_error = Some(e.to_string());
                    return Err(e);
                }
            }
        }
        Ok(())
    }
    pub async fn apply(&mut self, a: Action) -> Result<()> {
        let (output, level, sequence, finish) = match a {
            Action::Refresh => return self.refresh().await,
            Action::Restore { output } => {
                let i = self
                    .configs
                    .iter()
                    .position(|c| c.output == output)
                    .ok_or("unknown output")?;
                let c = &self.configs[i];
                if c.curve == CurveKind::Laptop && c.auto_brightness {
                    self.transport.reference(c, luminance(70., c)).await?;
                    self.transport.persist(c, 70.).await?;
                    self.saved[i] = 70.;
                    let s = &mut self.state.outputs[i];
                    s.reference_luminance_nits = luminance(70., c);
                    s.level_percent = raw_level(
                        s.raw.unwrap_or(1),
                        s.raw_max.unwrap_or(400),
                        s.reference_luminance_nits,
                        c,
                    );
                    return Ok(());
                }
                (
                    output,
                    Some(self.saved[i]),
                    self.state.outputs[i].request_sequence + 1,
                    true,
                )
            }
            Action::Begin { output } => {
                let s = self
                    .state
                    .outputs
                    .iter_mut()
                    .find(|s| s.output == output)
                    .ok_or("unknown output")?;
                s.dragging = true;
                return Ok(());
            }
            Action::Set {
                output,
                percent,
                sequence,
            } => (output, Some(percent), sequence, false),
            Action::Finish { output, sequence } => (output, None, sequence, true),
        };
        let index = self
            .configs
            .iter()
            .position(|c| c.output == output)
            .ok_or("unknown output")?;
        let c = &self.configs[index];
        let latest = self.latest.clone();
        // A newer queued Set makes this one obsolete on both curves: a fast slider drag
        // must not replay every intermediate step through slow DDC writes and confirms.
        let superseded = || {
            latest
                .lock()
                .unwrap()
                .get(&output)
                .is_some_and(|n| *n > sequence)
        };
        if superseded() {
            return Ok(());
        }
        let s = &mut self.state.outputs[index];
        if sequence < s.request_sequence {
            return Err("stale brightness sequence".into());
        }
        let level = level.unwrap_or(s.level_percent);
        if !level.is_finite() {
            return Err("brightness must be finite".into());
        }
        let level = level.round().clamp(0., 100.);
        if c.curve == CurveKind::Laptop && self.transport.controls_set(level).await? {
            s.request_sequence = sequence;
            s.level_percent = level;
            s.confirmed_sequence = Some(sequence);
            s.failed = false;
            s.last_error = None;
            s.retries = 0;
            if finish {
                s.dragging = false;
            }
            self.saved[index] = level;
            return Ok(());
        }
        if s.raw_max.is_none() {
            let (raw, max) = self.transport.read(c).await?;
            if max == 0 || raw > max {
                return Err("invalid brightness range".into());
            }
            s.raw = Some(raw);
            s.raw_max = Some(max);
        }
        let max = s.raw_max.unwrap();
        let target = plan_for_config(level, max, c);
        let previous_state = s.clone();
        s.request_sequence = sequence;
        s.level_percent = level;
        s.confirmed_sequence = None;
        s.failed = false;
        s.last_error = None;
        s.retries = 0;
        if finish {
            s.dragging = false;
        }
        let result = async {
            if superseded() {
                return Ok(());
            }
            if c.curve == CurveKind::Desktop || s.raw != Some(target) {
                self.transport.write(c, target).await?;
            }
            if c.curve == CurveKind::Laptop {
                let (raw, actual_max) = self.transport.read(c).await?;
                s.raw = Some(raw);
                s.raw_max = Some(actual_max);
                if raw != target || actual_max != max {
                    return Err("brightness hardware confirmation failed".into());
                }
            }
            if superseded() {
                let raw = s.raw;
                let raw_max = s.raw_max;
                *s = previous_state;
                s.raw = raw;
                s.raw_max = raw_max;
                return Ok(());
            }
            let nits = luminance(level, c);
            if c.curve == CurveKind::Desktop || nits != s.reference_luminance_nits {
                self.transport.reference(c, nits).await?;
            }
            s.reference_luminance_nits = nits;
            if finish || !s.dragging {
                tokio::time::sleep(Duration::from_millis(90)).await;
                loop {
                    let (raw, actual_max) = self.transport.read(c).await?;
                    s.raw = Some(raw);
                    s.raw_max = Some(actual_max);
                    if raw == target && actual_max == max {
                        break;
                    }
                    if s.retries >= 3 {
                        return Err("brightness confirmation failed after 3 retries".into());
                    }
                    s.retries += 1;
                    self.transport.write(c, target).await?;
                    tokio::time::sleep(Duration::from_millis(90)).await;
                }
                if superseded() {
                    return Ok(());
                }
                s.confirmed_sequence = Some(sequence);
                self.transport.persist(c, level).await?;
                self.saved[index] = level;
            }
            Ok(())
        }
        .await;
        if let Err(ref e) = result {
            s.failed = true;
            s.last_error = Some(format!("{e}"));
        }
        result
    }
}
fn lua_string(s: &str) -> String {
    let mut out = String::from("\"");
    for byte in s.bytes() {
        out.push_str(&format!("\\{byte:03}"));
    }
    out.push('"');
    out
}
async fn hypr_request(ctx: &Context, command: String) -> Result<Vec<u8>> {
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")?;
    if signature.contains('/') || signature.contains("..") {
        return Err("invalid Hyprland signature".into());
    }
    let base = ctx.paths.runtime.parent().ok_or("no runtime")?;
    let path = base.join("hypr").join(signature).join(".socket.sock");
    let mut socket = tokio::net::UnixStream::connect(path).await?;
    socket.write_all(command.as_bytes()).await?;
    let mut bytes = Vec::new();
    socket.take(1024 * 1024 + 1).read_to_end(&mut bytes).await?;
    if bytes.len() > 1024 * 1024 {
        return Err("Hyprland reply too large".into());
    }
    Ok(bytes)
}
struct NativeTransport {
    ctx: Context,
    primary: String,
    hardware: Arc<std::sync::Mutex<()>>,
}
impl NativeTransport {
    async fn hardware<R: Send + 'static>(
        &self,
        job: impl FnOnce() -> Result<R> + Send + 'static,
    ) -> Result<R> {
        let lock = self.hardware.clone();
        tokio::time::timeout(
            Duration::from_secs(12),
            tokio::task::spawn_blocking(move || {
                let _guard = lock.lock().map_err(|_| "hardware worker poisoned")?;
                job()
            }),
        )
        .await
        .map_err(|_| "hardware timed out; operation may still complete, serialization retained")??
    }
}
fn ddc_open(bus: u32) -> Result<std::fs::File> {
    use std::os::{fd::AsRawFd, unix::fs::OpenOptionsExt};
    let f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(format!("/dev/i2c-{bus}"))?;
    if unsafe { libc::ioctl(f.as_raw_fd(), 0x0703, 0x37) } < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(f)
}
pub fn decode_ddc(bytes: &[u8]) -> Result<(u32, u32)> {
    if bytes.len() != 11
        || bytes[0] != 0x6e
        || bytes[1] != 0x88
        || bytes[2] != 2
        || bytes[3] != 0
        || bytes[4] != 0x10
        || bytes.iter().fold(0x50, |a, b| a ^ b) != 0
    {
        return Err("invalid DDC brightness response".into());
    }
    let max = u16::from_be_bytes([bytes[6], bytes[7]]) as u32;
    let raw = u16::from_be_bytes([bytes[8], bytes[9]]) as u32;
    if max == 0 || raw > max {
        return Err("invalid DDC range".into());
    }
    Ok((raw, max))
}
#[async_trait]
impl Transport for NativeTransport {
    async fn read(&self, c: &OutputConfig) -> Result<(u32, u32)> {
        let backend = c.backend.clone();
        let paths = self.ctx.paths.clone();
        self.hardware(move || match backend {
            BacklightBackend::Sysfs { device } => {
                let raw = actor::read_number(paths.guard_sysfs(device.join("brightness"))?)
                    .ok_or("backlight read failed")? as u32;
                let max = actor::read_number(paths.guard_sysfs(device.join("max_brightness"))?)
                    .ok_or("backlight range unavailable")? as u32;
                Ok((raw, max))
            }
            BacklightBackend::Ddc { bus, .. } => {
                if paths.isolated_sysfs() {
                    return Err("DDC hardware access disabled for isolated sysfs".into());
                }
                use std::io::{Read, Write};
                let mut f = ddc_open(bus)?;
                let mut req = vec![0x51, 0x82, 0x01, 0x10];
                req.push(req.iter().fold(0x6e, |a, b| a ^ b));
                f.write_all(&req)?;
                std::thread::sleep(Duration::from_millis(40));
                let mut reply = [0u8; 11];
                f.read_exact(&mut reply)?;
                decode_ddc(&reply)
            }
        })
        .await
    }
    async fn write(&self, c: &OutputConfig, raw: u32) -> Result<()> {
        let backend = c.backend.clone();
        let paths = self.ctx.paths.clone();
        self.hardware(move || match backend {
            BacklightBackend::Sysfs { device } => {
                std::fs::write(
                    paths.guard_sysfs(device.join("brightness"))?,
                    format!("{raw}\n"),
                )?;
                Ok(())
            }
            BacklightBackend::Ddc { bus, .. } => {
                if paths.isolated_sysfs() {
                    return Err("DDC hardware access disabled for isolated sysfs".into());
                }
                use std::io::Write;
                let mut f = ddc_open(bus)?;
                let raw = u16::try_from(raw)?;
                let mut req = vec![0x51, 0x84, 0x03, 0x10, (raw >> 8) as u8, raw as u8];
                req.push(req.iter().fold(0x6e, |a, b| a ^ b));
                f.write_all(&req)?;
                std::thread::sleep(Duration::from_millis(50));
                Ok(())
            }
        })
        .await
    }
    async fn reference(&self, c: &OutputConfig, nits: f64) -> Result<()> {
        if c.curve == CurveKind::Laptop {
            let command = laptop_reference_command(c, nits)?;
            let reply = hypr_request(&self.ctx, command).await?;
            let text = String::from_utf8_lossy(&reply);
            if text.starts_with("error:") || !text.contains("hyprv-brightness-ok") {
                return Err(format!("compositor rejected luminance: {text}").into());
            }
            return Ok(());
        }
        let mut fields=format!("output={},mode={},position=p.x..'x'..p.y,scale=m.scale,transform=m.transform,bitdepth=10,cm='hdr',sdr_min_luminance={},reference_luminance={nits}",lua_string(&c.output),lua_string(&c.mode),c.sdr_min);
        for (key, value) in &c.monitor_fields {
            if key.is_empty() || key.bytes().any(|b| !b.is_ascii_lowercase() && b != b'_') {
                return Err("invalid monitor field".into());
            }
            let value = match value {
                serde_json::Value::String(s) => lua_string(s),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                _ => return Err("invalid monitor field value".into()),
            };
            fields.push_str(&format!(",{key}={value}"));
        }
        let command=format!("/repl local m=assert(hl.get_monitor({}));local p=m.position;hl.monitor({{{fields}}});return 'hyprv-brightness-ok'",lua_string(&c.output));
        let reply = hypr_request(&self.ctx, command).await?;
        let text = String::from_utf8_lossy(&reply);
        if text.starts_with("error:") || !text.contains("hyprv-brightness-ok") {
            return Err(format!("compositor rejected luminance: {text}").into());
        }
        Ok(())
    }
    async fn controls_set(&self, level: f64) -> Result<bool> {
        let command = format!(
            "/repl require('hypr.controls').brightness_set({},false);return 'hyprv-brightness-ok'",
            level.round().clamp(0., 100.) as u32
        );
        // No reachable compositor: fall back to the native curve.
        let Ok(reply) = hypr_request(&self.ctx, command).await else {
            return Ok(false);
        };
        let text = String::from_utf8_lossy(&reply);
        if text.starts_with("error:") || !text.contains("hyprv-brightness-ok") {
            return Err(format!("hypr.controls rejected brightness: {text}").into());
        }
        Ok(true)
    }
    async fn controls_get(&self) -> Result<Option<f64>> {
        let Ok(reply) = hypr_request(
            &self.ctx,
            "/repl return tostring(require('hypr.controls').brightness_get())".into(),
        )
        .await
        else {
            return Ok(None);
        };
        Ok(String::from_utf8_lossy(&reply).trim().parse::<f64>().ok())
    }
    async fn persist(&self, config: &OutputConfig, level: f64) -> Result<()> {
        let path = self.ctx.paths.state.join("brightness-hdr.json");
        let mut value = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            .filter(|v| v.is_object())
            .unwrap_or_else(|| serde_json::json!({}));
        if config.output == self.primary {
            value["level"] = serde_json::json!(level as u32);
        }
        if !value.get("levels").is_some_and(|v| v.is_object()) {
            value["levels"] = serde_json::json!({});
        }
        value["levels"][&config.output] = serde_json::json!(level as u32);
        actor::atomic(&path, &serde_json::to_vec(&value)?)
    }
}
/// Parse the existing simple literal profile. Reject expressions/nested overrides
/// instead of guessing values; callers may provide a typed multi-output profile.
pub fn load_profile(ctx: &Context) -> Result<Vec<OutputConfig>> {
    load_for_profile(ctx, hyprv_core::profile::profile())
}
const LAPTOP_PROFILE: &str = "return {auto_brightness=true,mode=\"preferred\",output=\"eDP-1\",backend=\"backlight\",desktop=153,client=153,peak=400,monitor={supports_hdr=1,supports_wide_color=1,min_luminance=0.0005,max_luminance=530,max_avg_luminance=400},initial_raw=1,initial_max=400,sdr_min=0}";
fn load_for_profile(ctx: &Context, profile: crate::Profile) -> Result<Vec<OutputConfig>> {
    let path =
        PathBuf::from(std::env::var("HOME")?).join(".config/HyprV/hypr/controls-profile.lua");
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let configs = parse_profile(&ctx.paths.sys, &text, profile)?;
            if profile == crate::Profile::Laptop
                && configs
                    .iter()
                    .any(|c| matches!(c.backend, BacklightBackend::Ddc { .. }))
            {
                return parse_profile(&ctx.paths.sys, LAPTOP_PROFILE, profile);
            }
            Ok(configs)
        }
        Err(e) if profile == crate::Profile::Laptop && e.kind() == std::io::ErrorKind::NotFound => {
            parse_profile(&ctx.paths.sys, LAPTOP_PROFILE, profile)
        }
        Err(e) => Err(e.into()),
    }
}
/// Only literal root scalars and the literal monitor table are accepted.
/// Never evaluate code or shell substitutions from a profile.
pub fn parse_profile(
    sys: &std::path::Path,
    text: &str,
    profile: crate::Profile,
) -> Result<Vec<OutputConfig>> {
    let body = text
        .trim()
        .strip_prefix("return")
        .ok_or("expected literal return")?
        .trim();
    let fields = literal_fields(body)?;
    let get = |key: &str| {
        fields
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| format!("missing profile {key}"))
    };
    let scalar = |key: &str| -> Result<String> {
        let raw = get(key)?;
        Ok(raw.trim_matches(['\"', '\'']).into())
    };
    let output = scalar("output")?;
    let backend = match scalar("backend")?.as_str() {
        "backlight" => BacklightBackend::Sysfs {
            device: sys.join("class/backlight/intel_backlight"),
        },
        "ddc" => BacklightBackend::Ddc {
            connector: output.clone(),
            bus: get("ddc_bus")?.parse()?,
        },
        _ => return Err("unsupported brightness backend".into()),
    };
    let mut monitor_fields = BTreeMap::new();
    if let Some(raw) = fields.get("monitor") {
        for (key, value) in literal_fields(raw)? {
            monitor_fields.insert(key, serde_json::from_str(&value)?);
        }
    }
    let c = OutputConfig {
        output,
        mode: scalar("mode")?,
        backend,
        desktop: get("desktop")?.parse()?,
        peak: get("peak")?.parse()?,
        sdr_min: get("sdr_min")?.parse()?,
        monitor_fields,
        curve: if profile == crate::Profile::Laptop {
            CurveKind::Laptop
        } else {
            CurveKind::Desktop
        },
        auto_brightness: fields.get("auto_brightness").is_some_and(|v| v == "true"),
    };
    c.validate()?;
    Ok(vec![c])
}
fn literal_fields(table: &str) -> Result<BTreeMap<String, String>> {
    let body = table
        .trim()
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .ok_or("expected literal table")?;
    let mut depth = 0u32;
    let mut quoted = None;
    let mut start = 0;
    let mut entries = Vec::new();
    for (i, c) in body.char_indices() {
        if let Some(q) = quoted {
            if c == q {
                quoted = None;
            } else if c == '\\' {
                return Err("escaped literal not supported".into());
            }
            continue;
        }
        match c {
            '\"' | '\'' => quoted = Some(c),
            '{' => depth += 1,
            '}' => depth = depth.checked_sub(1).ok_or("unbalanced table")?,
            ',' if depth == 0 => {
                entries.push(&body[start..i]);
                start = i + 1;
            }
            _ => (),
        }
    }
    if depth != 0 || quoted.is_some() {
        return Err("unbalanced literal profile".into());
    }
    entries.push(&body[start..]);
    let mut out = BTreeMap::new();
    for entry in entries {
        if entry.trim().is_empty() {
            continue;
        }
        let (k, v) = entry.split_once('=').ok_or("expected literal assignment")?;
        let k = k.trim();
        let v = v.trim();
        if k.is_empty()
            || k.bytes().any(|b| !b.is_ascii_alphanumeric() && b != b'_')
            || out.contains_key(k)
        {
            return Err("invalid/duplicate profile key".into());
        }
        if !(v.starts_with('{')
            || v.starts_with('\"') && v.ends_with('\"')
            || v.starts_with('\'') && v.ends_with('\'')
            || matches!(v, "true" | "false")
            || v.parse::<f64>().ok().filter(|n| n.is_finite()).is_some())
        {
            return Err("nonliteral profile value".into());
        }
        out.insert(k.into(), v.into());
    }
    Ok(out)
}
pub fn laptop_saved_level(bytes: &[u8]) -> f64 {
    let Some(start) = bytes.windows(7).position(|s| s == b"\"level\"") else {
        return 70.;
    };
    let rest = &bytes[start + 7..];
    let mut it = rest.iter().copied().skip_while(u8::is_ascii_whitespace);
    if it.next() != Some(b':') {
        return 70.;
    }
    let digits: Vec<u8> = it
        .skip_while(u8::is_ascii_whitespace)
        .take_while(u8::is_ascii_digit)
        .collect();
    std::str::from_utf8(&digits)
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(70.)
}
pub fn laptop_reference_command(c: &OutputConfig, nits: f64) -> Result<String> {
    if !nits.is_finite() {
        return Err("invalid reference luminance".into());
    }
    let mut extra = String::new();
    for (k, v) in &c.monitor_fields {
        if k.is_empty() || k.bytes().any(|b| !b.is_ascii_lowercase() && b != b'_') {
            return Err("invalid monitor field".into());
        }
        if !v.is_number() && !v.is_boolean() {
            return Err("non-numeric monitor field".into());
        }
        extra.push_str(&format!(",{k}={v}"));
    }
    Ok(format!("/repl local p=assert(hl.plugin and hl.plugin.hypr_sdr_white,'hypr-sdr-white missing');assert(p.refresh);local m=assert(hl.get_monitor({output}),'Display unavailable');local pos=m.position;hl.config({{plugin={{hypr_sdr_white={{reference_luminance={nits}}}}}}});hl.monitor({{output={output},mode={mode},position=pos.x..'x'..pos.y,scale=m.scale,transform=m.transform,bitdepth=10,cm='hdr',sdr_min_luminance={min},sdr_max_luminance={nits}{extra}}});p.refresh();return 'hyprv-brightness-ok'",output=lua_string(&c.output),mode=lua_string(&c.mode),min=c.sdr_min))
}
pub fn spawn(ctx: Context) -> Handle<State, Action> {
    spawn_for_profile(ctx, hyprv_core::profile::profile())
}
pub fn spawn_with_config(ctx: Context, configs: Vec<OutputConfig>) -> Handle<State, Action> {
    spawn_inner(ctx, Ok(configs), false, hyprv_core::profile::profile())
}
pub fn spawn_for_profile(ctx: Context, profile: crate::Profile) -> Handle<State, Action> {
    let configs = load_for_profile(&ctx, profile);
    spawn_inner(ctx, configs, true, profile)
}
fn spawn_inner(
    ctx: Context,
    configs: Result<Vec<OutputConfig>>,
    dynamic_profile: bool,
    profile: crate::Profile,
) -> Handle<State, Action> {
    let hardware = Arc::new(std::sync::Mutex::new(()));
    let engine = configs
        .and_then(|configs| native_engine(&ctx, configs, hardware.clone()))
        .map_err(|e| e.to_string());
    let latest = engine.as_ref().ok().map(|e| e.latest.clone());
    let mut handle = actor::spawn(
        ctx.clone(),
        Native {
            ctx,
            engine,
            events: None,
            acquired: false,
            restore_saved: true,
            dynamic_profile,
            profile,
            latest: latest.clone(),
            files: None,
            hardware,
        },
        false,
    );
    {
        if let Some(latest) = latest {
            handle.action_observer = Some(Arc::new(move |action| {
                if let Action::Set {
                    output,
                    percent,
                    sequence,
                } = action
                {
                    if percent.is_finite() {
                        let mut map = latest.lock().unwrap();
                        let n = map.entry(output.clone()).or_default();
                        *n = (*n).max(*sequence);
                    }
                }
            }));
        }
    }
    handle
}
fn native_engine(
    ctx: &Context,
    configs: Vec<OutputConfig>,
    hardware: Arc<std::sync::Mutex<()>>,
) -> Result<Engine<NativeTransport>> {
    let primary = configs
        .first()
        .map(|c| c.output.clone())
        .unwrap_or_default();
    Engine::new(
        NativeTransport {
            ctx: ctx.clone(),
            primary,
            hardware,
        },
        configs,
    )
}
struct Native {
    ctx: Context,
    engine: std::result::Result<Engine<NativeTransport>, String>,
    events: Option<KernelEvents>,
    acquired: bool,
    restore_saved: bool,
    dynamic_profile: bool,
    profile: crate::Profile,
    latest: Option<Arc<std::sync::Mutex<BTreeMap<String, u64>>>>,
    files: Option<hyprv_core::config::FileWatch>,
    hardware: Arc<std::sync::Mutex<()>>,
}
impl Backend for Native {
    type State = State;
    type Action = Action;
    async fn connect(&mut self) -> Result<()> {
        if self.dynamic_profile {
            let configs = load_for_profile(&self.ctx, self.profile)?;
            if self.engine.as_ref().map_or(true, |e| e.configs != configs) {
                self.engine = native_engine(&self.ctx, configs, self.hardware.clone())
                    .map(|mut engine| {
                        if let Some(latest) = &self.latest {
                            engine.latest = latest.clone();
                        }
                        engine
                    })
                    .map_err(|e| e.to_string());
                self.acquired = false;
                self.restore_saved = true;
            }
        }
        if let Err(e) = &self.engine {
            return Err(e.clone().into());
        }
        let mut paths = vec![self.ctx.paths.state.join("brightness-hdr.json")];
        if self.dynamic_profile {
            paths.push(
                PathBuf::from(std::env::var("HOME")?)
                    .join(".config/HyprV/hypr/controls-profile.lua"),
            );
        }
        self.files = Some(hyprv_core::config::FileWatch::new(&paths)?);
        if self.ctx.paths.isolated_sysfs() {
            for config in &self.engine.as_ref().map_err(|e| e.clone())?.configs {
                if let BacklightBackend::Sysfs { device } = &config.backend {
                    paths.extend([device.join("brightness"), device.join("max_brightness")]);
                }
            }
            self.files = Some(hyprv_core::config::FileWatch::new(&paths)?);
            self.events = None;
        } else {
            self.events = Some(KernelEvents::udev()?);
        }
        Ok(())
    }
    async fn snapshot(&mut self) -> Result<State> {
        let e = self.engine.as_mut().map_err(|e| e.clone())?;
        if !self.acquired {
            if self.profile == crate::Profile::Laptop && self.restore_saved {
                let bytes = std::fs::read(self.ctx.paths.state.join("brightness-hdr.json"))
                    .unwrap_or_default();
                let level = laptop_saved_level(&bytes);
                for (i, c) in e.configs.iter().enumerate() {
                    e.saved[i] = level;
                    e.state.outputs[i].reference_luminance_nits = luminance(level, c);
                }
            }
            e.refresh().await?;
            if self.restore_saved && self.profile == crate::Profile::Desktop {
                if let Ok(bytes) = std::fs::read(self.ctx.paths.state.join("brightness-hdr.json")) {
                    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                        let primary = e.configs.first().map(|c| c.output.clone());
                        for (s, c) in e.state.outputs.iter_mut().zip(&e.configs) {
                            if s.dragging
                                || (s.request_sequence > 0
                                    && s.confirmed_sequence != Some(s.request_sequence))
                            {
                                continue;
                            }
                            let level = value
                                .get("levels")
                                .and_then(|v| v.get(&c.output))
                                .and_then(|v| v.as_f64())
                                .or_else(|| {
                                    (primary.as_deref() == Some(c.output.as_str()))
                                        .then(|| value.get("level").and_then(|v| v.as_f64()))
                                        .flatten()
                                });
                            if let Some(level) = level.filter(|v| v.is_finite()) {
                                s.level_percent = level.clamp(0., 100.);
                                s.reference_luminance_nits = luminance(level, c);
                            }
                        }
                    }
                }
            }
            self.restore_saved = false;
            self.acquired = true;
        }
        Ok(e.state.clone())
    }
    async fn apply(&mut self, a: Action) -> Result<()> {
        self.engine.as_mut().map_err(|e| e.clone())?.apply(a).await
    }
    async fn event(&mut self) -> Result<()> {
        tokio::select! {
            r=async {match self.events.as_ref(){Some(events)=>{events.subsystem(&["backlight","drm"]).await?;Ok::<_,crate::actor::Error>(())},None=>std::future::pending().await}}=>{r?;self.acquired=false;},
            r=self.files.as_ref().unwrap().changed()=>{r?;self.acquired=false;self.restore_saved=true;
                if self.dynamic_profile {let configs=load_for_profile(&self.ctx,self.profile)?;if self.engine.as_ref().map_or(true,|e|e.configs!=configs){self.engine=native_engine(&self.ctx,configs,self.hardware.clone()).map(|mut engine|{if let Some(latest)=&self.latest{engine.latest=latest.clone();}engine}).map_err(|e|e.to_string());}}
            },
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    struct Fake(Mutex<u32>);
    #[async_trait]
    impl Transport for Fake {
        async fn read(&self, _: &OutputConfig) -> Result<(u32, u32)> {
            Ok((*self.0.lock().unwrap(), 100))
        }
        async fn write(&self, _: &OutputConfig, r: u32) -> Result<()> {
            *self.0.lock().unwrap() = r;
            Ok(())
        }
        async fn reference(&self, _: &OutputConfig, _: f64) -> Result<()> {
            Ok(())
        }
        async fn persist(&self, _: &OutputConfig, _: f64) -> Result<()> {
            Ok(())
        }
    }
    fn config() -> OutputConfig {
        OutputConfig {
            output: "DP-4".into(),
            mode: "6144x3456@60.02".into(),
            backend: BacklightBackend::Ddc {
                connector: "DP-4".into(),
                bus: 9,
            },
            desktop: 173.,
            peak: 446.,
            sdr_min: 0.2,
            monitor_fields: BTreeMap::new(),
            curve: CurveKind::Desktop,
            auto_brightness: false,
        }
    }
    #[test]
    fn all_levels_follow_curve() {
        let c = config();
        assert_eq!(luminance(0., &c), 10.);
        assert_eq!(luminance(10., &c), 173.);
        assert_eq!(luminance(70., &c), 173.);
        assert_eq!(luminance(100., &c), 446.);
        let mut previous = 0;
        for level in 0..=100 {
            let raw = plan(level as f64, 100, false);
            assert!(raw >= previous && raw <= 100);
            previous = raw;
        }
        assert_eq!(plan(0., 100, false), 0);
        assert_eq!(plan(70., 100, false), 100);
        assert_eq!(plan(0., 1000, true), 1);
    }
    #[tokio::test]
    async fn drag_finish_and_stale_sequence() {
        let mut engine = Engine::new(Fake(Mutex::new(100)), vec![config()]).unwrap();
        engine.refresh().await.unwrap();
        engine
            .apply(Action::Begin {
                output: "DP-4".into(),
            })
            .await
            .unwrap();
        engine
            .apply(Action::Set {
                output: "DP-4".into(),
                percent: 50.,
                sequence: 2,
            })
            .await
            .unwrap();
        assert_eq!(engine.state.outputs[0].confirmed_sequence, None);
        engine
            .apply(Action::Finish {
                output: "DP-4".into(),
                sequence: 2,
            })
            .await
            .unwrap();
        assert_eq!(engine.state.outputs[0].confirmed_sequence, Some(2));
        assert!(engine
            .apply(Action::Set {
                output: "DP-4".into(),
                percent: 1.,
                sequence: 1
            })
            .await
            .is_err());
    }
    #[test]
    fn malformed_ddc_is_rejected() {
        assert!(decode_ddc(&[0; 11]).is_err());
        let mut reply = [0x6e, 0x88, 2, 0, 0x10, 0, 0, 100, 0, 50, 0];
        reply[10] = reply[..10].iter().fold(0x50, |a, b| a ^ b);
        assert_eq!(decode_ddc(&reply).unwrap(), (50, 100));
    }
    struct Stubborn(std::sync::atomic::AtomicUsize);
    #[async_trait]
    impl Transport for Stubborn {
        async fn read(&self, _: &OutputConfig) -> Result<(u32, u32)> {
            Ok((100, 100))
        }
        async fn write(&self, _: &OutputConfig, _: u32) -> Result<()> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        }
        async fn reference(&self, _: &OutputConfig, _: f64) -> Result<()> {
            Ok(())
        }
        async fn persist(&self, _: &OutputConfig, _: f64) -> Result<()> {
            panic!("unconfirmed target must not be persisted")
        }
    }
    #[tokio::test(start_paused = true)]
    async fn failed_readback_preserves_goal_and_exposes_error() {
        let mut e = Engine::new(
            Stubborn(std::sync::atomic::AtomicUsize::new(0)),
            vec![config()],
        )
        .unwrap();
        e.refresh().await.unwrap();
        assert!(e
            .apply(Action::Set {
                output: "DP-4".into(),
                percent: 50.,
                sequence: 1
            })
            .await
            .is_err());
        assert_eq!(e.state.outputs[0].level_percent, 50.);
        assert!(e.state.outputs[0].failed);
        assert_eq!(e.state.outputs[0].confirmed_sequence, None);
        assert_eq!(e.state.outputs[0].retries, 3);
        assert_eq!(e.transport.0.load(std::sync::atomic::Ordering::Relaxed), 4);
        e.refresh().await.unwrap();
        assert_eq!(e.state.outputs[0].level_percent, 50.);
    }
}

#[cfg(test)]
mod laptop_tests {
    use super::*;
    use crate::test_support::Tree;
    use std::sync::Mutex;
    fn config() -> OutputConfig {
        parse_profile(
            std::path::Path::new("/fake-sys"),
            include_str!("../tests/fixtures/laptop-controls-profile.lua"),
            crate::Profile::Laptop,
        )
        .unwrap()
        .remove(0)
    }
    #[test]
    fn all_levels_match_pinned_lua_and_profile_keeps_monitor_overrides() {
        let tree = Tree::new();
        tree.write(
            "controls.lua",
            include_str!("../tests/fixtures/laptop-controls.lua"),
        );
        tree.write(
            "profile.lua",
            include_str!("../tests/fixtures/laptop-controls-profile.lua"),
        );
        tree.write("oracle.lua","local c=dofile(arg[2]);c.initial_max=tonumber(arg[3]);package.preload['hypr.controls-profile']=function()return c end;hl={timer=function()return{}end};local m=dofile(arg[1]);for i=0,100 do local r,d,k=m.plan(i);print(i,r,d,k)end\n");
        let c = config();
        assert!(c.auto_brightness);
        assert_eq!(c.monitor_fields.len(), 5);
        assert_eq!(c.monitor_fields["max_luminance"], 530);
        for max in [1, 100, 400, 937, 65535] {
            let output = std::process::Command::new("lua")
                .arg(tree.0.join("oracle.lua"))
                .arg(tree.0.join("controls.lua"))
                .arg(tree.0.join("profile.lua"))
                .arg(max.to_string())
                .env("HOME", &tree.0)
                .env("LC_ALL", "C")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            for line in String::from_utf8(output.stdout).unwrap().lines() {
                let n: Vec<f64> = line
                    .split_whitespace()
                    .map(|v| v.parse().unwrap())
                    .collect();
                assert_eq!(
                    plan_for_config(n[0], max, &c),
                    n[1] as u32,
                    "L={} M={max}",
                    n[0]
                );
                assert_eq!(luminance(n[0], &c), n[2]);
                assert_eq!(n[2], n[3]);
            }
        }
        assert_eq!(raw_level(1, 400, 153., &c), 0.);
        assert_eq!(raw_level(400, 400, 400., &c), 100.);
        assert_eq!(luminance(0., &c), 153.);
        assert_eq!(luminance(85., &c), 277.);
        let command = laptop_reference_command(&c, 277.).unwrap();
        for required in [
            "hypr_sdr_white",
            "p.refresh()",
            "sdr_max_luminance=277",
            "reference_luminance=277",
            "max_luminance=530",
            "scale=m.scale",
            "transform=m.transform",
        ] {
            assert!(command.contains(required), "{required}");
        }
        // Execute only the generated Lua against a private API fake. This
        // verifies syntax, current geometry, both HDR anchors and refresh order.
        tree.write("transport.lua", "local seen={};hl={plugin={hypr_sdr_white={refresh=function()assert(seen.monitor);seen.refresh=true end}},get_monitor=function(output)assert(output=='eDP-1');return{position={x=17,y=19},scale=2,transform=1}end,config=function(c)assert(c.plugin.hypr_sdr_white.reference_luminance==277);seen.config=true end,monitor=function(m)assert(seen.config);assert(m.position=='17x19' and m.scale==2 and m.transform==1);assert(m.sdr_max_luminance==277 and m.max_luminance==530 and m.max_avg_luminance==400 and m.cm=='hdr' and m.bitdepth==10);seen.monitor=true end};local f=assert(load(arg[1]));assert(f()=='hyprv-brightness-ok');assert(seen.refresh)\n");
        let output = std::process::Command::new("lua")
            .arg(tree.0.join("transport.lua"))
            .arg(command.strip_prefix("/repl ").unwrap())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(parse_profile(
            std::path::Path::new("/fake"),
            "return {output=os.execute('bad')}",
            crate::Profile::Laptop
        )
        .is_err());
    }
    struct Fake {
        raw: Mutex<u32>,
        events: Mutex<Vec<String>>,
        deny: bool,
    }
    #[async_trait]
    impl Transport for Fake {
        async fn read(&self, _: &OutputConfig) -> Result<(u32, u32)> {
            Ok((*self.raw.lock().unwrap(), 400))
        }
        async fn write(&self, _: &OutputConfig, raw: u32) -> Result<()> {
            if self.deny {
                return Err("permission denied".into());
            }
            self.events.lock().unwrap().push(format!("raw={raw}"));
            *self.raw.lock().unwrap() = raw;
            Ok(())
        }
        async fn reference(&self, _: &OutputConfig, n: f64) -> Result<()> {
            self.events.lock().unwrap().push(format!("hdr={n}"));
            Ok(())
        }
        async fn persist(&self, _: &OutputConfig, n: f64) -> Result<()> {
            self.events.lock().unwrap().push(format!("save={n}"));
            Ok(())
        }
    }
    #[tokio::test(start_paused = true)]
    async fn auto_restore_never_writes_hardware_and_external_als_readback_uses_curve() {
        let mut e = Engine::new(
            Fake {
                raw: Mutex::new(40),
                events: Mutex::new(Vec::new()),
                deny: false,
            },
            vec![config()],
        )
        .unwrap();
        // Startup adoption changes local SDR knowledge, never hardware.
        e.state.outputs[0].reference_luminance_nits = 400.;
        e.refresh().await.unwrap();
        assert_eq!(e.state.outputs[0].level_percent, 10.);
        assert!(e.transport.events.lock().unwrap().is_empty());
        e.apply(Action::Restore {
            output: "eDP-1".into(),
        })
        .await
        .unwrap();
        assert_eq!(*e.transport.raw.lock().unwrap(), 40);
        assert_eq!(
            *e.transport.events.lock().unwrap(),
            vec!["hdr=153", "save=70"]
        );
        e.transport.events.lock().unwrap().clear();
        e.apply(Action::Set {
            output: "eDP-1".into(),
            percent: 10.,
            sequence: 1,
        })
        .await
        .unwrap();
        assert_eq!(
            *e.transport.events.lock().unwrap(),
            vec!["save=10"],
            "same raw/SDR skips hardware writes"
        );
        e.transport.events.lock().unwrap().clear();
        *e.transport.raw.lock().unwrap() = 400;
        e.refresh().await.unwrap();
        assert_eq!(e.state.outputs[0].level_percent, 70.);
        e.apply(Action::Set {
            output: "eDP-1".into(),
            percent: 85.,
            sequence: 2,
        })
        .await
        .unwrap();
        assert_eq!(
            *e.transport.events.lock().unwrap(),
            vec!["hdr=277", "save=85"]
        );
        // A request already superseded in the actor queue cannot apply old HDR/state.
        e.transport.events.lock().unwrap().clear();
        e.latest.lock().unwrap().insert("eDP-1".into(), 4);
        e.apply(Action::Set {
            output: "eDP-1".into(),
            percent: 99.,
            sequence: 3,
        })
        .await
        .unwrap();
        assert!(e.transport.events.lock().unwrap().is_empty());
        e.apply(Action::Set {
            output: "eDP-1".into(),
            percent: 100.,
            sequence: 4,
        })
        .await
        .unwrap();
        assert_eq!(
            *e.transport.events.lock().unwrap(),
            vec!["hdr=400", "save=100"]
        );
    }
    #[tokio::test]
    async fn laptop_probe_accepts_readback_during_drag_and_unconfirmed_request() {
        let mut e = Engine::new(
            Fake {
                raw: Mutex::new(40),
                events: Mutex::new(Vec::new()),
                deny: false,
            },
            vec![config()],
        )
        .unwrap();
        e.refresh().await.unwrap();
        let state = &mut e.state.outputs[0];
        state.dragging = true;
        state.request_sequence = 5;
        state.confirmed_sequence = Some(4);
        state.level_percent = 90.;
        // Same hardware value as the previous probe still replaces UI intent.
        e.refresh().await.unwrap();
        assert_eq!(e.state.outputs[0].level_percent, 10.);
        *e.transport.raw.lock().unwrap() = 400;
        e.refresh().await.unwrap();
        assert_eq!(e.state.outputs[0].level_percent, 70.);
        assert!(e.transport.events.lock().unwrap().is_empty());
        // Desktop retains its interaction and pending-request guards.
        e.configs[0].curve = CurveKind::Desktop;
        e.state.outputs[0].level_percent = 90.;
        e.refresh().await.unwrap();
        assert_eq!(e.state.outputs[0].level_percent, 90.);
        e.state.outputs[0].dragging = false;
        e.refresh().await.unwrap();
        assert_eq!(e.state.outputs[0].level_percent, 90.);
    }
    #[tokio::test]
    async fn laptop_write_denial_does_not_apply_hdr_or_persist() {
        let mut e = Engine::new(
            Fake {
                raw: Mutex::new(40),
                events: Mutex::new(Vec::new()),
                deny: true,
            },
            vec![config()],
        )
        .unwrap();
        e.refresh().await.unwrap();
        let result = e
            .apply(Action::Set {
                output: "eDP-1".into(),
                percent: 100.,
                sequence: 1,
            })
            .await;
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("permission denied"));
        assert!(e.state.outputs[0].failed);
        assert!(e.transport.events.lock().unwrap().is_empty());
    }
}

#[cfg(test)]
mod supersession_tests {
    use super::*;
    struct Blocking {
        raw: std::sync::Mutex<u32>,
        entered: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
        effects: std::sync::Mutex<Vec<String>>,
    }
    #[async_trait]
    impl Transport for Blocking {
        async fn read(&self, _: &OutputConfig) -> Result<(u32, u32)> {
            Ok((*self.raw.lock().unwrap(), 400))
        }
        async fn write(&self, _: &OutputConfig, raw: u32) -> Result<()> {
            self.entered.notify_one();
            self.release.notified().await;
            *self.raw.lock().unwrap() = raw;
            Ok(())
        }
        async fn reference(&self, _: &OutputConfig, _: f64) -> Result<()> {
            self.effects.lock().unwrap().push("HDR".into());
            Ok(())
        }
        async fn persist(&self, _: &OutputConfig, _: f64) -> Result<()> {
            self.effects.lock().unwrap().push("persist".into());
            Ok(())
        }
    }
    #[tokio::test]
    async fn queued_goal_supersedes_slow_hardware_before_hdr_and_persist() {
        let config = parse_profile(
            std::path::Path::new("/fake"),
            include_str!("../tests/fixtures/laptop-controls-profile.lua"),
            crate::Profile::Laptop,
        )
        .unwrap();
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let mut e = Engine::new(
            Blocking {
                raw: std::sync::Mutex::new(400),
                entered: entered.clone(),
                release: release.clone(),
                effects: std::sync::Mutex::new(Vec::new()),
            },
            config,
        )
        .unwrap();
        e.refresh().await.unwrap();
        let latest = e.latest.clone();
        let work = tokio::spawn(async move {
            e.apply(Action::Set {
                output: "eDP-1".into(),
                percent: 10.,
                sequence: 1,
            })
            .await
            .unwrap();
            e
        });
        entered.notified().await;
        latest.lock().unwrap().insert("eDP-1".into(), 2);
        release.notify_one();
        let e = work.await.unwrap();
        assert!(e.transport.effects.lock().unwrap().is_empty());
        assert_eq!(
            e.state.outputs[0].raw,
            Some(40),
            "retain actual hardware knowledge for the next request"
        );
        assert_eq!(
            e.state.outputs[0].level_percent, 70.,
            "obsolete public intent must not roll back the latest UI goal"
        );
    }
}
