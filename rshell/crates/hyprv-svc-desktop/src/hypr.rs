//! Minimal Hyprland request-socket client (`hyprctl` wire format: `FLAGS/COMMAND`).
//! The full compositor actor lives elsewhere; desktop actions only need one-shot
//! requests at action time and a socket2 reader for readiness waits.
use crate::{Env, ServiceError};
use serde::de::DeserializeOwned;
use std::{
    io,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug)]
pub struct Hypr {
    runtime_dir: PathBuf,
    signature: Option<String>,
}
impl Hypr {
    pub fn new(env: &Env) -> Self {
        Self {
            runtime_dir: env.runtime_dir.join("hypr"),
            signature: env.var("HYPRLAND_INSTANCE_SIGNATURE").map(str::to_string),
        }
    }
    /// The configured instance, else the instance whose lock file names a live PID.
    pub fn instance_dir(&self) -> Option<PathBuf> {
        if let Some(sig) = &self.signature {
            let dir = self.runtime_dir.join(sig);
            // A stale signature (shell started from an older session) falls through.
            let reachable =
                || std::os::unix::net::UnixStream::connect(dir.join(".socket.sock")).is_ok();
            if lock_alive(&dir) || (!has_lock(&dir) && reachable()) {
                return Some(dir);
            }
        }
        let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
        for entry in std::fs::read_dir(&self.runtime_dir).ok()?.flatten() {
            let dir = entry.path();
            if !lock_alive(&dir) {
                continue;
            }
            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            if best.as_ref().is_none_or(|(t, _)| modified > *t) {
                best = Some((modified, dir));
            }
        }
        best.map(|(_, d)| d)
    }
    pub fn signature(&self) -> Option<String> {
        self.instance_dir()
            .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
    }
    pub async fn request(&self, command: &str) -> Result<String, ServiceError> {
        let dir = self
            .instance_dir()
            .ok_or_else(|| ServiceError::new("hyprland", "Hyprland instance not found"))?;
        tokio::time::timeout(
            REQUEST_TIMEOUT,
            raw_request(&dir.join(".socket.sock"), command),
        )
        .await
        .map_err(|_| ServiceError::new("timeout", "Hyprland request timed out"))?
        .map_err(|e| ServiceError::new("hyprland", e.to_string()))
    }
    /// `hyprctl -j COMMAND`.
    pub async fn json<T: DeserializeOwned>(&self, command: &str) -> Result<T, ServiceError> {
        let text = self.request(&format!("j/{command}")).await?;
        serde_json::from_str(&text).map_err(|e| ServiceError::new("hyprland", e.to_string()))
    }
    /// `hyprctl COMMAND`; plain reply text.
    pub async fn plain(&self, command: &str) -> Result<String, ServiceError> {
        self.request(&format!("/{command}")).await
    }
    /// `hyprctl eval CODE` with the source's error-word heuristic
    /// (`demo-mode.sh:71`): output naming error/invalid/failed/cannot fails.
    pub async fn eval(&self, code: &str) -> Result<String, ServiceError> {
        let out = self.plain(&format!("eval {code}")).await?;
        if reply_is_error(&out) {
            return Err(ServiceError::new("hyprland", out.trim()));
        }
        Ok(out)
    }
    /// `hyprctl dispatch ARGS`; Hyprland replies `ok` on success.
    pub async fn dispatch(&self, args: &str) -> Result<(), ServiceError> {
        let out = self.plain(&format!("dispatch {args}")).await?;
        if out.trim() == "ok" {
            Ok(())
        } else {
            Err(ServiceError::new("hyprland", out.trim()))
        }
    }
    /// Lua dispatcher first, legacy dispatcher fallback (`apply-ui-state.sh:276`).
    pub async fn focus_address(&self, address: &str) -> Result<(), ServiceError> {
        if !valid_address(address) {
            return Err(ServiceError::new("invalid", "bad window address"));
        }
        let lua = format!("hl.dsp.focus({{ window = 'address:{address}' }})");
        if self.dispatch(&lua).await.is_ok() {
            return Ok(());
        }
        self.dispatch(&format!("focuswindow address:{address}"))
            .await
    }
    pub async fn events(&self) -> Result<EventStream, ServiceError> {
        let dir = self
            .instance_dir()
            .ok_or_else(|| ServiceError::new("hyprland", "Hyprland instance not found"))?;
        let stream = UnixStream::connect(dir.join(".socket2.sock")).await?;
        Ok(EventStream(BufReader::new(stream)))
    }
}
pub struct EventStream(BufReader<UnixStream>);
impl EventStream {
    /// Next `(event, data)` from socket2.
    pub async fn next(&mut self) -> io::Result<(String, String)> {
        let mut line = String::new();
        if self.0.read_line(&mut line).await? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        let line = line.trim_end();
        let (e, d) = line.split_once(">>").unwrap_or((line, ""));
        Ok((e.to_string(), d.to_string()))
    }
}

async fn raw_request(socket: &Path, command: &str) -> io::Result<String> {
    let mut stream = UnixStream::connect(socket).await?;
    stream.write_all(command.as_bytes()).await?;
    let mut out = Vec::new();
    stream.read_to_end(&mut out).await?;
    Ok(String::from_utf8_lossy(&out).into_owned())
}
fn has_lock(dir: &Path) -> bool {
    dir.join("hyprland.lock").exists()
}
fn lock_alive(dir: &Path) -> bool {
    let Some(text) = crate::util::read_string(&dir.join("hyprland.lock")) else {
        return false;
    };
    text.lines()
        .next()
        .and_then(|l| l.trim().parse::<i32>().ok())
        .is_some_and(|pid| pid > 0 && Path::new(&format!("/proc/{pid}")).exists())
        && dir.join(".socket.sock").exists()
}
pub fn reply_is_error(out: &str) -> bool {
    let lower = out.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    for word in ["error", "invalid", "failed", "cannot"] {
        let mut start = 0;
        while let Some(i) = lower[start..].find(word) {
            let at = start + i;
            let end = at + word.len();
            let before = at == 0 || !bytes[at - 1].is_ascii_lowercase();
            let after = end == bytes.len() || !bytes[end].is_ascii_lowercase();
            if before && after {
                return true;
            }
            start = at + 1;
        }
    }
    false
}
pub fn valid_address(address: &str) -> bool {
    address
        .strip_prefix("0x")
        .is_some_and(|h| !h.is_empty() && h.bytes().all(|b| b.is_ascii_hexdigit()))
}
/// Single-quoted Lua string; refuses embedded quotes (`apply-ui-state.sh:262`).
pub fn lua_quote(value: &str) -> Option<String> {
    (!value.contains('\'') && !value.contains('\\') && !value.contains('\n'))
        .then(|| format!("'{value}'"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn error_heuristic_matches_grep() {
        assert!(reply_is_error("Error: bad"));
        assert!(reply_is_error("ok\nfailed to apply"));
        assert!(!reply_is_error("ok"));
        assert!(!reply_is_error("errors?")); // grep needs a non-letter after the word
        assert!(reply_is_error("x invalid."));
    }
    #[test]
    fn quoting() {
        assert_eq!(lua_quote("DP-4").as_deref(), Some("'DP-4'"));
        assert!(lua_quote("a'b").is_none());
        assert!(valid_address("0x55ab"));
        assert!(!valid_address("0x"));
    }
    #[tokio::test(flavor = "current_thread")]
    async fn request_wire_format() {
        let dir = std::env::temp_dir().join(format!("hyprv-hypr-{}", std::process::id()));
        let inst = dir.join("hypr/sig");
        std::fs::create_dir_all(&inst).unwrap();
        let listener = tokio::net::UnixListener::bind(inst.join(".socket.sock")).unwrap();
        let server = tokio::spawn(async move {
            // The first connection may be the reachability probe (no payload).
            loop {
                let (mut s, _) = listener.accept().await.unwrap();
                let mut buf = vec![0u8; 64];
                let n = s.read(&mut buf).await.unwrap();
                if n == 0 {
                    continue;
                }
                assert_eq!(&buf[..n], b"j/monitors all");
                s.write_all(b"[]").await.unwrap();
                break;
            }
        });
        let mut vars = std::collections::BTreeMap::new();
        vars.insert("XDG_RUNTIME_DIR".into(), dir.display().to_string());
        vars.insert("HYPRLAND_INSTANCE_SIGNATURE".into(), "sig".into());
        let h = Hypr::new(&Env::from_vars(vars));
        let v: Vec<serde_json::Value> = h.json("monitors all").await.unwrap();
        assert!(v.is_empty());
        server.await.unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
