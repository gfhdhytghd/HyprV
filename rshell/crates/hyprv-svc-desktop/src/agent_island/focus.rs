//! `FocusSession(id)`: port of `agent-island-action.py focus`. Uses the
//! Hyprland request socket and Kitty's remote-control socket protocol instead of
//! `hyprctl`/`kitty @` subprocesses. No approve/answer/type command exists.
use super::model::is_agent_focus_client;
use crate::{hypr::Hypr, ServiceError};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

pub async fn focus_session(hypr: &Hypr, session: &Value) -> Result<(), ServiceError> {
    let terminal = session.get("terminal").cloned().unwrap_or(json!({}));
    let window = terminal.get("window").cloned().unwrap_or(json!({}));
    let clients: Vec<Value> = hypr.json("clients").await.unwrap_or_default();
    if let Some(address) = window
        .get("address")
        .and_then(Value::as_str)
        .filter(|a| !a.is_empty())
    {
        let matched = clients
            .iter()
            .find(|c| c.get("address").and_then(Value::as_str) == Some(address));
        if matched.is_some_and(is_agent_focus_client) {
            let _ = hypr.focus_address(address).await;
            let _ = kitty_focus(&terminal).await;
            return Ok(());
        }
    }
    if kitty_focus(&terminal).await {
        return Ok(());
    }
    let terms = session_terms(session);
    let mut scored: Vec<(i32, &str)> = clients
        .iter()
        .filter_map(|c| {
            let score = client_score(c, &terms);
            let addr = c.get("address").and_then(Value::as_str)?;
            (score >= 25).then_some((score, addr))
        })
        .collect();
    // Python sorts stably by score descending.
    scored.sort_by_key(|s| std::cmp::Reverse(s.0));
    if let Some((_, address)) = scored.first() {
        let _ = hypr.focus_address(address).await;
        let _ = kitty_focus(&terminal).await;
        return Ok(());
    }
    Err(ServiceError::new(
        "not_found",
        "no window for agent session",
    ))
}

pub fn session_terms(session: &Value) -> Vec<String> {
    let mut terms = Vec::new();
    for key in ["cwd", "project", "title", "source_label", "source"] {
        if let Some(v) = session
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            terms.push(v.to_string());
        }
    }
    if let Some(cwd) = session
        .get("cwd")
        .and_then(Value::as_str)
        .filter(|c| !c.trim().is_empty())
    {
        let name = crate::util::basename(cwd.trim_end_matches('/'));
        if !name.is_empty() {
            terms.push(name.to_string());
        }
    }
    let mut seen = std::collections::HashSet::new();
    terms.retain(|t| seen.insert(t.to_lowercase()));
    terms
}
pub fn client_score(client: &Value, terms: &[String]) -> i32 {
    if !is_agent_focus_client(client) {
        return -1;
    }
    let s = |k: &str| client.get(k).and_then(Value::as_str).unwrap_or("");
    let title = if s("title").is_empty() {
        s("initialTitle")
    } else {
        s("title")
    }
    .to_lowercase();
    let class_text = format!("{} {}", s("class"), s("initialClass")).to_lowercase();
    let mut score = 5;
    if [
        "ghostty",
        "alacritty",
        "kitty",
        "wezterm",
        "foot",
        "konsole",
        "terminal",
    ]
    .iter()
    .any(|t| class_text.contains(t))
    {
        score += 15;
    }
    for term in terms {
        let lowered = term.to_lowercase();
        if lowered.chars().count() < 3 {
            continue;
        }
        if title.contains(&lowered) {
            score += 40;
        }
        if class_text.contains(&lowered) {
            score += 10;
        }
    }
    score
}

/// `kitty @ --to LISTEN_ON focus-window --match id:N` over the RC protocol.
pub async fn kitty_focus(terminal: &Value) -> bool {
    let window = terminal
        .get("kitty_window")
        .and_then(Value::as_str)
        .map(str::trim);
    let listen = terminal
        .get("kitty_listen_on")
        .and_then(Value::as_str)
        .map(str::trim);
    let (Some(window), Some(listen)) = (window, listen) else {
        return false;
    };
    if window.is_empty() || listen.is_empty() {
        return false;
    }
    let payload = json!({"match": format!("id:{window}")});
    tokio::time::timeout(
        Duration::from_millis(800),
        kitty_command(listen, "focus-window", payload),
    )
    .await
    .ok()
    .and_then(Result::ok)
    .is_some_and(|reply| reply.get("ok").and_then(Value::as_bool) == Some(true))
}

/// Connect to a `unix:` Kitty listen address (path or `@abstract`).
pub async fn kitty_connect(listen_on: &str) -> std::io::Result<UnixStream> {
    let target = listen_on
        .strip_prefix("unix:")
        .ok_or_else(|| std::io::Error::other("unsupported kitty address"))?;
    if let Some(name) = target.strip_prefix('@') {
        use std::os::linux::net::SocketAddrExt;
        let addr = std::os::unix::net::SocketAddr::from_abstract_name(name.as_bytes())?;
        let std = std::os::unix::net::UnixStream::connect_addr(&addr)?;
        std.set_nonblocking(true)?;
        UnixStream::from_std(std)
    } else {
        UnixStream::connect(target).await
    }
}
/// Kitty RC framing: `ESC P @kitty-cmd JSON ESC \`; reply uses the same frame.
pub async fn kitty_command(listen_on: &str, cmd: &str, payload: Value) -> std::io::Result<Value> {
    let mut stream = kitty_connect(listen_on).await?;
    let body = json!({"cmd": cmd, "version": [0, 26, 0], "no_response": false, "payload": payload});
    let frame = format!("\x1bP@kitty-cmd{body}\x1b\\");
    stream.write_all(frame.as_bytes()).await?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.ends_with(b"\x1b\\") {
            break;
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let json_part = text
        .trim_start_matches("\x1bP@kitty-cmd")
        .trim_end_matches("\x1b\\");
    serde_json::from_str(json_part).map_err(std::io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scoring_matches_python() {
        let c = json!({"class":"kitty","title":"proj: vim","address":"0x1"});
        let terms = session_terms(&json!({"cwd":"/h/proj","project":"proj","source":"claude"}));
        assert_eq!(terms, ["/h/proj", "proj", "claude"]);
        assert_eq!(client_score(&c, &terms), 5 + 15 + 40);
        assert_eq!(client_score(&json!({"class":"firefox"}), &terms), -1);
    }
    #[tokio::test(flavor = "current_thread")]
    async fn kitty_rc_frame() {
        let dir = std::env::temp_dir().join(format!("hyprv-kitty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("k.sock");
        let listener = tokio::net::UnixListener::bind(&sock).unwrap();
        let server = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 512];
            let n = s.read(&mut buf).await.unwrap();
            let text = String::from_utf8_lossy(&buf[..n]).to_string();
            assert!(text.starts_with("\x1bP@kitty-cmd{"));
            assert!(text.contains("\"match\":\"id:3\""));
            s.write_all(b"\x1bP@kitty-cmd{\"ok\":true}\x1b\\")
                .await
                .unwrap();
        });
        let t = json!({"kitty_window":"3","kitty_listen_on":format!("unix:{}", sock.display())});
        assert!(kitty_focus(&t).await);
        server.await.unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
