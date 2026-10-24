//! Codex app-server adapter: port of `agent-island-codex-appserver.py`.
//!
//! The Python spawned `codex app-server --listen stdio://`. The shell never
//! spawns it; it attaches to the running app-server control socket
//! (`$CODEX_HOME/app-server-control/app-server-control.sock`, a
//! `--listen unix://` endpoint speaking JSON-RPC over WebSocket). It sends the
//! same `initialize` request, then forwards every JSON object it receives;
//! the actor applies `thread/started`, `thread/status/changed` and
//! `thread/closed` exactly as the Python did.
//!
//! Reconnect: the endpoint path is watched with inotify (no timer while it is
//! absent). After a disconnect, or when the path exists but refuses connections,
//! retries start at the Python's 4 s and back off to 5 min. Any change to the
//! endpoint resets the backoff.
use crate::{util, Env};
use serde_json::{json, Value};
use std::{path::PathBuf, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    sync::mpsc,
};

const MAX_MESSAGE: usize = 16 << 20;

pub fn endpoint(env: &Env) -> PathBuf {
    if let Some(p) = env.var("HYPRV_CODEX_APP_SERVER_SOCK") {
        return util::expand_tilde(std::path::Path::new(p), &env.home);
    }
    env.var("CODEX_HOME")
        .map(|p| util::expand_tilde(std::path::Path::new(p), &env.home))
        .unwrap_or_else(|| env.home.join(".codex"))
        .join("app-server-control/app-server-control.sock")
}

pub fn initialize_request() -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"clientInfo": {"name": "HyprV", "version": "agent-island"}, "capabilities": null}})
}

pub async fn run(
    path: PathBuf,
    tx: mpsc::UnboundedSender<Value>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let first = Duration::from_secs(4);
    let mut backoff = first;
    loop {
        if *shutdown.borrow() || tx.is_closed() {
            return;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let parent = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let watch = util::Watch::new(&[(parent, &[name.as_str()][..])]).ok();
        let exists = std::fs::symlink_metadata(&path).is_ok();
        let mut connected = false;
        if exists {
            if let Ok(stream) = UnixStream::connect(&path).await {
                connected = true;
                tokio::select! {
                    _ = session(stream, &tx) => {}
                    _ = shutdown.changed() => {}
                }
                if *shutdown.borrow() {
                    return;
                }
            }
        }
        // Absent endpoint: wait for it to appear, no timer.
        // Present but failing / just disconnected: bounded backoff.
        let retry = async {
            if exists {
                tokio::time::sleep(backoff).await;
                true
            } else {
                std::future::pending().await
            }
        };
        let changed = async {
            match &watch {
                Some(w) => {
                    let _ = w.changed().await;
                }
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            timed = retry => {
                if timed && !connected {
                    backoff = (backoff * 2).min(Duration::from_secs(300));
                }
            }
            _ = changed => backoff = first,
            _ = shutdown.changed() => {}
        }
        if connected {
            backoff = first;
        }
    }
}

async fn session(mut stream: UnixStream, tx: &mpsc::UnboundedSender<Value>) -> std::io::Result<()> {
    handshake(&mut stream).await?;
    send_text(&mut stream, &initialize_request().to_string()).await?;
    let mut message: Vec<u8> = Vec::new();
    loop {
        let (fin, opcode, payload) = read_frame(&mut stream).await?;
        match opcode {
            0x0..=0x2 => {
                message.extend_from_slice(&payload);
                if message.len() > MAX_MESSAGE {
                    return Err(std::io::Error::other("message too large"));
                }
                if fin {
                    if let Ok(v @ Value::Object(_)) = serde_json::from_slice::<Value>(&message) {
                        if tx.send(v).is_err() {
                            return Ok(());
                        }
                    }
                    message.clear();
                }
            }
            0x8 => {
                let _ = write_frame(&mut stream, 0x8, &payload).await;
                return Ok(());
            }
            0x9 => write_frame(&mut stream, 0xA, &payload).await?,
            _ => {}
        }
    }
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}
fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    unsafe { libc::getrandom(b.as_mut_ptr().cast(), N, 0) };
    b
}
async fn handshake(stream: &mut UnixStream) -> std::io::Result<()> {
    let key = base64(&random::<16>());
    let req = format!(
        "GET / HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).await?;
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).await? == 0 || head.len() > 8192 {
            return Err(std::io::Error::other("bad websocket handshake"));
        }
        head.push(byte[0]);
    }
    let status = String::from_utf8_lossy(&head);
    if !status.starts_with("HTTP/1.1 101") {
        return Err(std::io::Error::other("websocket upgrade refused"));
    }
    Ok(())
}
async fn write_frame(stream: &mut UnixStream, opcode: u8, data: &[u8]) -> std::io::Result<()> {
    let mut frame = vec![0x80 | opcode];
    let n = data.len();
    if n < 126 {
        frame.push(0x80 | n as u8);
    } else if n < 65536 {
        frame.push(0x80 | 126);
        frame.extend_from_slice(&(n as u16).to_be_bytes());
    } else {
        frame.push(0x80 | 127);
        frame.extend_from_slice(&(n as u64).to_be_bytes());
    }
    let mask = random::<4>();
    frame.extend_from_slice(&mask);
    frame.extend(data.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
    stream.write_all(&frame).await
}
async fn send_text(stream: &mut UnixStream, text: &str) -> std::io::Result<()> {
    write_frame(stream, 0x1, text.as_bytes()).await
}
async fn read_frame(stream: &mut UnixStream) -> std::io::Result<(bool, u8, Vec<u8>)> {
    let mut h = [0u8; 2];
    stream.read_exact(&mut h).await?;
    let fin = h[0] & 0x80 != 0;
    let opcode = h[0] & 0x0F;
    let masked = h[1] & 0x80 != 0;
    let mut len = (h[1] & 0x7F) as u64;
    if len == 126 {
        let mut b = [0u8; 2];
        stream.read_exact(&mut b).await?;
        len = u16::from_be_bytes(b) as u64;
    } else if len == 127 {
        let mut b = [0u8; 8];
        stream.read_exact(&mut b).await?;
        len = u64::from_be_bytes(b);
    }
    if len as usize > MAX_MESSAGE {
        return Err(std::io::Error::other("frame too large"));
    }
    let mut mask = [0u8; 4];
    if masked {
        stream.read_exact(&mut mask).await?;
    }
    let mut payload = vec![0u8; len as usize];
    stream.read_exact(&mut payload).await?;
    if masked {
        for (i, b) in payload.iter_mut().enumerate() {
            *b ^= mask[i % 4];
        }
    }
    Ok((fin, opcode, payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn b64() {
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"f"), "Zg==");
    }
    /// Fake app-server: WebSocket over a Unix socket, fragmented + ping frames.
    #[tokio::test(flavor = "current_thread")]
    async fn attaches_initializes_and_forwards_notifications() {
        let dir = std::env::temp_dir().join(format!("hyprv-appsrv-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("app-server-control.sock");
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (_stop, shutdown) = tokio::sync::watch::channel(false);
        // Start the client before the endpoint exists: it must wait via inotify.
        let client = tokio::spawn(run(sock.clone(), tx, shutdown));
        tokio::time::sleep(Duration::from_millis(50)).await;
        let listener = tokio::net::UnixListener::bind(&sock).unwrap();
        let (mut s, _) = listener.accept().await.unwrap();
        let mut head = Vec::new();
        let mut b = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            s.read_exact(&mut b).await.unwrap();
            head.push(b[0]);
        }
        assert!(String::from_utf8_lossy(&head).contains("Upgrade: websocket"));
        s.write_all(b"HTTP/1.1 101 Switching Protocols\r\nupgrade: websocket\r\n\r\n")
            .await
            .unwrap();
        let (fin, op, init) = read_frame(&mut s).await.unwrap();
        assert!(fin && op == 1);
        let init: Value = serde_json::from_slice(&init).unwrap();
        assert_eq!(init, initialize_request());
        // Server frames are unmasked; send a ping, then a fragmented notification.
        s.write_all(&[0x89, 0x02, b'h', b'i']).await.unwrap();
        let (_, op, data) = read_frame(&mut s).await.unwrap();
        assert_eq!((op, data.as_slice()), (0xA, &b"hi"[..]));
        let msg = br#"{"method":"thread/closed","params":{"threadId":"t"}}"#;
        let (a, z) = msg.split_at(10);
        let mut f = vec![0x01, a.len() as u8];
        f.extend_from_slice(a);
        f.extend_from_slice(&[0x80, z.len() as u8]);
        f.extend_from_slice(z);
        s.write_all(&f).await.unwrap();
        let got = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got["method"], "thread/closed");
        drop(rx);
        drop(s);
        client.abort();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
