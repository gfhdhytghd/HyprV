//! HDMI-switch IR transmitter: port of `hdmi-switch-ir.py`.
//!
//! MicroPython Pico on `/dev/serial/by-id`, raw 115200 baud, newline-terminated
//! UTF-8 commands and newline JSON reply records. Device discovery and database
//! changes are inotify-driven; commands run one at a time (exclusive queue),
//! each with one readiness-driven deadline (no 250 ms select loop).
use crate::{topic_key, util, ActionResult, Ctx, Handle, ServiceError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::io::unix::AsyncFd;

pub const SERIAL_DIR: &str = "/dev/serial/by-id";
const SERIAL_PATTERNS: &[&str] = &["MicroPython", "Raspberry_Pi_Pico"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NecDecoded {
    pub protocol: String,
    pub bytes_hex: Vec<String>,
    pub address: i64,
    pub command: i64,
    pub address_inverse_valid: bool,
    pub command_inverse_valid: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IrButton {
    pub name: String,
    pub captured_at: String,
    pub source_pin: Option<i64>,
    pub polarity: String,
    pub durations_us: Vec<i64>,
    pub decoded: Option<NecDecoded>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct IrState {
    pub database: PathBuf,
    /// Exactly one matching Pico; `None` when absent or ambiguous.
    pub device: Option<PathBuf>,
    /// "MicroPython Pico serial device not found" / "multiple Pico ..." when no device.
    pub device_error: Option<String>,
    pub carrier_hz: i64,
    pub duty_cycle: f64,
    /// Sorted by name (`list` output order).
    pub buttons: Vec<IrButton>,
    pub busy: bool,
    pub last_error: Option<String>,
}
topic_key!(IrTopic, IrState);

#[derive(Clone, Debug)]
pub enum IrAction {
    /// `send NAME`; result is the device's `transmit` record plus `button`.
    Send(String),
    /// `learn NAME [--force]`.
    Learn { name: String, force: bool },
    /// `scan [--seconds N]` (default 5).
    Scan { seconds: f64 },
    /// `set-pin PIN`.
    SetPin(i64),
    /// `status`.
    Status,
    /// `show NAME` (database only, no device access).
    Show(String),
    /// `list` (database only).
    List,
}
pub type IrHandle = Handle<IrState, IrAction>;

pub struct IrConfig {
    pub database: PathBuf,
    pub serial_dir: PathBuf,
    /// `--device` override.
    pub device: Option<PathBuf>,
}
impl IrConfig {
    pub fn from_env(env: &crate::Env) -> Self {
        Self {
            database: env.data_home.join("hyprv-ir/hdmi-switch.json"),
            serial_dir: SERIAL_DIR.into(),
            device: None,
        }
    }
}

pub fn spawn(ctx: &Ctx) -> IrHandle {
    spawn_with(ctx, IrConfig::from_env(&ctx.env))
}
pub fn spawn_with(ctx: &Ctx, config: IrConfig) -> IrHandle {
    let topic = crate::topic::<IrTopic>(ctx, IrState::default());
    let (handle, mut rx) = Handle::new(topic.clone());
    let mut shutdown = ctx.shutdown.clone();
    ctx.rt.spawn(async move {
        let mut state = IrState::default();
        // One long-lived watch; it re-arms itself after replaces.
        let db_name = config
            .database
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let watch = util::Watch::new(&[
            (config.serial_dir.clone(), &[][..]),
            (
                config
                    .database
                    .parent()
                    .unwrap_or(Path::new("/"))
                    .to_path_buf(),
                &[db_name.as_str()][..],
            ),
        ])
        .ok();
        loop {
            refresh(&config, &mut state);
            topic.publish(state.clone());
            let changed = async {
                match &watch {
                    Some(w) => {
                        let _ = util::settle(w, Duration::from_millis(50)).await;
                    }
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                _ = changed => {}
                msg = rx.recv() => {
                    let Some(env) = msg else { return };
                    state.busy = true;
                    topic.publish(state.clone());
                    let result = run(&config, &env.action).await;
                    state.busy = false;
                    state.last_error = result.as_ref().err().map(|e| e.message.clone());
                    env.respond(result);
                }
                _ = shutdown.changed() => if *shutdown.borrow() { return },
            }
        }
    });
    handle
}

fn refresh(config: &IrConfig, state: &mut IrState) {
    state.database = config.database.clone();
    match find_device(config) {
        Ok(d) => {
            state.device = Some(d);
            state.device_error = None;
        }
        Err(e) => {
            state.device = None;
            state.device_error = Some(e.message);
        }
    }
    let db = load_database(&config.database).unwrap_or_else(|_| default_database());
    state.carrier_hz = db
        .get("carrier_hz")
        .and_then(Value::as_i64)
        .unwrap_or(38000);
    state.duty_cycle = db.get("duty_cycle").and_then(Value::as_f64).unwrap_or(0.33);
    state.buttons = buttons(&db);
}
pub fn buttons(db: &Value) -> Vec<IrButton> {
    let mut out: Vec<IrButton> = db
        .get("buttons")
        .and_then(Value::as_object)
        .map(|b| {
            b.iter()
                .map(|(name, e)| IrButton {
                    name: name.clone(),
                    captured_at: e["captured_at"].as_str().unwrap_or("").into(),
                    source_pin: e["source_pin"].as_i64(),
                    polarity: e["polarity"].as_str().unwrap_or("").into(),
                    durations_us: e["durations_us"]
                        .as_array()
                        .map(|a| a.iter().filter_map(Value::as_i64).collect())
                        .unwrap_or_default(),
                    decoded: serde_json::from_value(e["decoded"].clone()).ok().flatten(),
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn find_device(config: &IrConfig) -> Result<PathBuf, ServiceError> {
    if let Some(d) = &config.device {
        return Ok(d.clone());
    }
    let mut matches: Vec<PathBuf> = std::fs::read_dir(&config.serial_dir)
        .map(|it| {
            it.flatten()
                .filter(|e| {
                    let n = e.file_name().to_string_lossy().into_owned();
                    SERIAL_PATTERNS.iter().any(|p| n.contains(p))
                })
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default();
    matches.sort();
    matches.dedup();
    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err(ServiceError::new(
            "not_found",
            "MicroPython Pico serial device not found",
        )),
        _ => Err(ServiceError::new(
            "ambiguous",
            "multiple Pico serial devices found; pass --device",
        )),
    }
}

pub fn default_database() -> Value {
    json!({"schema": 1, "carrier_hz": 38000, "duty_cycle": 0.33, "buttons": {}})
}
pub fn load_database(path: &Path) -> Result<Value, ServiceError> {
    match std::fs::read_to_string(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(default_database()),
        Err(e) => Err(e.into()),
        Ok(t) => serde_json::from_str(&t).map_err(|e| ServiceError::new("invalid", e.to_string())),
    }
}
/// `indent=2`, newline, `.json.tmp` then replace (unknown fields retained).
pub fn save_database(path: &Path, db: &Value) -> Result<(), ServiceError> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(
        &tmp,
        serde_json::to_string_pretty(db).map_err(ServiceError::io)? + "\n",
    )?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

fn near(actual: i64, expected: f64) -> bool {
    let a = actual as f64;
    expected * 0.65 <= a && a <= expected * 1.35
}
/// Drop an inter-frame gap and any repeat frame that follows it.
pub fn single_frame(durations: &[i64]) -> Vec<i64> {
    let mut i = 1;
    while i < durations.len() {
        if durations[i] > 10000 {
            return durations[..i].to_vec();
        }
        i += 2;
    }
    durations.to_vec()
}
pub fn decode_nec(d: &[i64]) -> Option<NecDecoded> {
    if d.len() < 66 || !near(d[0], 9000.0) || !near(d[1], 4500.0) {
        return None;
    }
    let mut bits = Vec::with_capacity(32);
    for offset in (2..66).step_by(2) {
        let (mark, space) = (d[offset], d[offset + 1]);
        if !near(mark, 560.0) {
            return None;
        }
        if near(space, 560.0) {
            bits.push(0u8);
        } else if near(space, 1690.0) {
            bits.push(1);
        } else {
            return None;
        }
    }
    let octets: Vec<i64> = (0..4)
        .map(|o| (0..8).map(|b| (bits[o * 8 + b] as i64) << b).sum())
        .collect();
    Some(NecDecoded {
        protocol: "NEC".into(),
        bytes_hex: octets.iter().map(|v| format!("0x{v:02X}")).collect(),
        address: octets[0],
        command: octets[2],
        address_inverse_valid: octets[0] ^ octets[1] == 0xFF,
        command_inverse_valid: octets[2] ^ octets[3] == 0xFF,
    })
}

async fn run(config: &IrConfig, action: &IrAction) -> ActionResult {
    match action {
        IrAction::List => {
            let db = load_database(&config.database)?;
            Ok(buttons(&db)
                .iter()
                .map(|b| match &b.decoded {
                    Some(d) => format!("{}  {} command={}\n", b.name, d.protocol, d.command),
                    None => format!("{}  raw\n", b.name),
                })
                .collect())
        }
        IrAction::Show(name) => {
            let db = load_database(&config.database)?;
            db["buttons"]
                .get(name)
                .map(pretty)
                .ok_or_else(|| ServiceError::new("not_found", format!("button not found: {name}")))
        }
        IrAction::Status => {
            let mut dev = Device::open(&find_device(config)?)?;
            Ok(pretty(&dev.command("status", &["status"], 2.0).await?))
        }
        IrAction::Scan { seconds } => {
            let mut dev = Device::open(&find_device(config)?)?;
            let ms = ((seconds * 1000.0) as i64).max(1000);
            Ok(pretty(
                &dev.command(&format!("scan {ms}"), &["scan"], seconds + 3.0)
                    .await?,
            ))
        }
        IrAction::SetPin(pin) => {
            let mut dev = Device::open(&find_device(config)?)?;
            Ok(pretty(
                &dev.command(&format!("pin {pin}"), &["pin"], 2.0).await?,
            ))
        }
        IrAction::Learn { name, force } => {
            let mut db = load_database(&config.database)?;
            if db["buttons"].get(name).is_some() && !force {
                return Err(ServiceError::new(
                    "exists",
                    format!("'{name}' already exists; use --force to replace it"),
                ));
            }
            let mut dev = Device::open(&find_device(config)?)?;
            let capture = dev
                .command("capture 10000", &["capture", "capture_timeout"], 13.0)
                .await?;
            if capture["event"] == "capture_timeout" {
                return Err(ServiceError::new("timeout", "10 秒内没有收到红外信号"));
            }
            let raw: Vec<i64> = capture["durations_us"]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_i64).collect())
                .ok_or_else(|| ServiceError::new("protocol", "capture without durations_us"))?;
            let durations = single_frame(&raw);
            let entry = json!({
                "captured_at": util::local_iso_now(),
                "source_pin": capture["pin"],
                "polarity": capture["polarity"],
                "durations_us": durations,
                "decoded": decode_nec(&durations),
            });
            if !db["buttons"].is_object() {
                db["buttons"] = json!({});
            }
            db["buttons"][name] = entry.clone();
            save_database(&config.database, &db)?;
            Ok(pretty(&entry))
        }
        IrAction::Send(name) => {
            let db = load_database(&config.database)?;
            let entry = db["buttons"].get(name).ok_or_else(|| {
                ServiceError::new("not_found", format!("button not found: {name}"))
            })?;
            let carrier = db
                .get("carrier_hz")
                .and_then(Value::as_f64)
                .unwrap_or(38000.0) as i64;
            let duty = (db.get("duty_cycle").and_then(Value::as_f64).unwrap_or(0.33) * 10000.0)
                .round() as i64;
            let durations: Vec<String> = entry["durations_us"]
                .as_array()
                .map(|a| a.iter().map(|v| v.to_string()).collect())
                .unwrap_or_default();
            let mut dev = Device::open(&find_device(config)?)?;
            let mut result = dev
                .command(
                    &format!("transmit {carrier} {duty} {}", durations.join(",")),
                    &["transmit"],
                    5.0,
                )
                .await?;
            result["button"] = json!(name);
            Ok(pretty(&result))
        }
    }
}

/// Raw 115200-baud serial line device.
pub struct Device {
    fd: AsyncFd<OwnedFd>,
    buffer: Vec<u8>,
}
impl Device {
    pub fn open(path: &Path) -> Result<Self, ServiceError> {
        let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
            .map_err(ServiceError::io)?;
        let raw = unsafe {
            libc::open(
                c.as_ptr(),
                libc::O_RDWR | libc::O_NOCTTY | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        if raw < 0 {
            return Err(io::Error::last_os_error().into());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        unsafe {
            let mut t: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(raw, &mut t) == 0 {
                libc::cfmakeraw(&mut t);
                libc::cfsetispeed(&mut t, libc::B115200);
                libc::cfsetospeed(&mut t, libc::B115200);
                libc::tcsetattr(raw, libc::TCSANOW, &t);
            }
        }
        Self::from_fd(fd)
    }
    pub fn from_fd(fd: OwnedFd) -> Result<Self, ServiceError> {
        Ok(Self {
            fd: unsafe { AsyncFd::register(fd) }.map_err(ServiceError::io)?,
            buffer: Vec::new(),
        })
    }
    fn read_now(&self, buf: &mut [u8]) -> io::Result<usize> {
        let n = unsafe {
            libc::read(
                self.fd.get_ref().as_raw_fd(),
                buf.as_mut_ptr().cast(),
                buf.len(),
            )
        };
        if n < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(n as usize)
        }
    }
    fn drain(&mut self) {
        let mut buf = [0u8; 4096];
        while matches!(self.read_now(&mut buf), Ok(n) if n > 0) {}
        self.buffer.clear();
    }
    async fn write_all(&self, mut data: &[u8]) -> io::Result<()> {
        while !data.is_empty() {
            let mut guard = self.fd.writable().await?;
            match guard.try_io(|fd| {
                let n = unsafe {
                    libc::write(fd.get_ref().as_raw_fd(), data.as_ptr().cast(), data.len())
                };
                if n < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(n as usize)
                }
            }) {
                Ok(Ok(n)) => data = &data[n..],
                Ok(Err(e)) => return Err(e),
                Err(_) => {}
            }
        }
        Ok(())
    }
    /// Send one command and wait for a final event record; `error` events fail.
    pub async fn command(
        &mut self,
        command: &str,
        finals: &[&str],
        timeout_s: f64,
    ) -> Result<Value, ServiceError> {
        self.drain();
        self.write_all(format!("{command}\n").as_bytes()).await?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs_f64(timeout_s);
        loop {
            while let Some(pos) = self.buffer.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = self.buffer.drain(..=pos).collect();
                let Ok(msg) = serde_json::from_slice::<Value>(line.trim_ascii()) else {
                    continue;
                };
                let event = msg.get("event").and_then(Value::as_str).unwrap_or("");
                if event == "error" {
                    let m = msg
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("device error");
                    return Err(ServiceError::new("device", m));
                }
                if finals.contains(&event) {
                    return Ok(msg);
                }
            }
            let ready = if tokio::time::Instant::now() >= deadline {
                None
            } else {
                tokio::time::timeout_at(deadline, self.fd.readable())
                    .await
                    .ok()
            };
            let Some(guard) = ready else {
                return Err(ServiceError::new(
                    "timeout",
                    format!("Pico did not answer within {}s", timeout_s.round() as i64),
                ));
            };
            let mut guard = guard?;
            let mut buf = [0u8; 4096];
            match guard.try_io(|fd| {
                let n = unsafe {
                    libc::read(fd.get_ref().as_raw_fd(), buf.as_mut_ptr().cast(), buf.len())
                };
                if n < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(n as usize)
                }
            }) {
                Ok(Ok(0)) => {
                    // EOF on a tty means hangup; wait for the deadline like the source.
                    guard.clear_ready();
                    tokio::time::sleep_until(deadline).await;
                }
                Ok(Ok(n)) => self.buffer.extend_from_slice(&buf[..n]),
                Ok(Err(e)) => return Err(e.into()),
                Err(_) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn nec(addr: u8, cmd: u8) -> Vec<i64> {
        let mut d = vec![9000, 4500];
        for byte in [addr, !addr, cmd, !cmd] {
            for b in 0..8 {
                d.push(560);
                d.push(if byte >> b & 1 == 1 { 1690 } else { 560 });
            }
        }
        d.push(560);
        d.extend([40000, 9000, 2250, 560]);
        d
    }
    #[test]
    fn nec_decode_and_frame() {
        let raw = nec(0x04, 0x08);
        let frame = single_frame(&raw);
        assert_eq!(frame.len(), 67);
        let d = decode_nec(&frame).unwrap();
        assert_eq!(d.address, 4);
        assert_eq!(d.command, 8);
        assert_eq!(d.bytes_hex, ["0x04", "0xFB", "0x08", "0xF7"]);
        assert!(d.address_inverse_valid && d.command_inverse_valid);
        assert!(decode_nec(&[1, 2, 3]).is_none());
    }
    #[test]
    fn real_database_shape() {
        let db = json!({"schema":1,"carrier_hz":38000,"duty_cycle":0.33,"buttons":{
            "input2":{"captured_at":"t","source_pin":0,"polarity":"active_low","durations_us":[1,2],"decoded":null},
            "input1":{"captured_at":"t","source_pin":0,"polarity":"active_low","durations_us":[3],"decoded":null,"extra":true}}});
        let b = buttons(&db);
        assert_eq!(b[0].name, "input1");
        assert_eq!(b[1].durations_us, [1, 2]);
    }
    #[tokio::test(flavor = "current_thread")]
    async fn serial_protocol_over_socketpair() {
        let mut fds = [0; 2];
        assert_eq!(
            unsafe {
                libc::socketpair(
                    libc::AF_UNIX,
                    libc::SOCK_STREAM | libc::SOCK_NONBLOCK,
                    0,
                    fds.as_mut_ptr(),
                )
            },
            0
        );
        let ours = unsafe { OwnedFd::from_raw_fd(fds[0]) };
        let peer = unsafe { std::os::unix::net::UnixStream::from_raw_fd(fds[1]) };
        let peer = tokio::net::UnixStream::from_std(peer).unwrap();
        let mut dev = Device::from_fd(ours).unwrap();
        let fake = tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
            let (r, mut w) = peer.into_split();
            let mut lines = BufReader::new(r).lines();
            let cmd = lines.next_line().await.unwrap().unwrap();
            assert_eq!(cmd, "transmit 38000 3300 1,2");
            w.write_all(
                b"garbage\n{\"event\":\"progress\"}\n{\"event\":\"transmit\",\"ok\":true}\n",
            )
            .await
            .unwrap();
            let cmd = lines.next_line().await.unwrap().unwrap();
            assert_eq!(cmd, "status");
            w.write_all(b"{\"event\":\"error\",\"message\":\"bad pin\"}\n")
                .await
                .unwrap();
            let _ = lines.next_line().await;
        });
        let r = dev
            .command("transmit 38000 3300 1,2", &["transmit"], 2.0)
            .await
            .unwrap();
        assert_eq!(r["ok"], true);
        let e = dev.command("status", &["status"], 2.0).await.unwrap_err();
        assert_eq!(e.message, "bad pin");
        let t = dev.command("pin 1", &["pin"], 0.05).await.unwrap_err();
        assert_eq!(t.code, "timeout");
        drop(dev);
        fake.await.unwrap();
    }
}
