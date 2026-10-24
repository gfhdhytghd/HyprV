//! Theme steps with no native API, executed exactly as `apply-ui-state.sh`
//! did (policy: one-shot exec for a user-initiated action, legacy parity).
//! Each runs only during an explicit theme action; nothing is polled.
//!
//! | step | legacy lines | exec |
//! |---|---|---|
//! | xsettingsd restart | `restart_xsettingsd` | `pkill -x xsettingsd` (signal, native) + detached `xsettingsd` when `$DISPLAY` |
//! | KDE color scheme | `plasma-apply-colorscheme` | `plasma-apply-colorscheme SCHEME` (5 s timeout) |
//! | wallpaper | `awww-daemon` / `awww img` | detached `awww-daemon` if absent (+0.2 s), `awww img BG --transition-fps 180 --transition-type wipe --transition-duration 2` |
//! | WeChat | `sync_wechat_appearance` | `xdotool`/`xwininfo` one-shots; Hyprland queries/dispatch via the socket |
use crate::{hypr::Hypr, util, Env, ServiceError};
use serde_json::Value;
use std::{path::Path, process::Stdio, time::Duration};
use tokio::process::Command;

/// One-shot exec with a deadline (`timeout N cmd >/dev/null 2>&1 || true`).
pub async fn run(argv: &[&str], deadline: Duration) -> Result<String, ServiceError> {
    let child = Command::new(argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| ServiceError::new("exec", format!("{}: {e}", argv[0])))?;
    let out = tokio::time::timeout(deadline, child.wait_with_output())
        .await
        .map_err(|_| ServiceError::new("timeout", format!("{} timed out", argv[0])))?
        .map_err(ServiceError::from)?;
    if !out.status.success() {
        return Err(ServiceError::new(
            "exec",
            format!("{} exited with {}", argv[0], out.status),
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
/// `nohup CMD >/dev/null 2>&1 &`: own process group, reaped by a task.
pub fn spawn_detached(argv: &[&str]) -> Result<(), ServiceError> {
    let mut child = Command::new(argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|e| ServiceError::new("exec", format!("{}: {e}", argv[0])))?;
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    Ok(())
}
fn pids_named(comm: &str) -> Vec<i32> {
    let proc = util::Proc::default();
    proc.pids()
        .into_iter()
        .filter(|p| proc.stat(*p).is_some_and(|s| s.comm == comm))
        .collect()
}
fn signal_all(comm: &str, sig: i32) {
    for pid in pids_named(comm) {
        if let Ok(fd) = util::Pidfd::open(pid) {
            let _ = fd.signal(sig);
        }
    }
}

/// `restart_xsettingsd`.
pub fn restart_xsettingsd(env: &Env) -> Result<(), ServiceError> {
    if util::which("xsettingsd", env.var("PATH")).is_none() {
        return Ok(());
    }
    signal_all("xsettingsd", libc::SIGTERM);
    if env.var("DISPLAY").is_some() {
        spawn_detached(&["xsettingsd"])?;
    }
    Ok(())
}
/// `run_with_timeout 5 plasma-apply-colorscheme SCHEME` (failures ignored).
pub async fn plasma_colorscheme(env: &Env, scheme: &str) -> Result<(), ServiceError> {
    if util::which("plasma-apply-colorscheme", env.var("PATH")).is_none() {
        return Ok(());
    }
    let _ = run(
        &["plasma-apply-colorscheme", scheme],
        Duration::from_secs(5),
    )
    .await;
    Ok(())
}
/// awww wallpaper with daemon bootstrap.
pub async fn wallpaper(env: &Env, background: &Path) -> Result<(), ServiceError> {
    if !background.is_file() || util::which("awww", env.var("PATH")).is_none() {
        return Ok(());
    }
    if pids_named("awww-daemon").is_empty() {
        spawn_detached(&["awww-daemon"])?;
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let bg = background.display().to_string();
    // `|| true` in the source; the transition itself runs in the daemon.
    let _ = run(
        &[
            "awww",
            "img",
            &bg,
            "--transition-fps",
            "180",
            "--transition-type",
            "wipe",
            "--transition-duration",
            "2",
        ],
        Duration::from_secs(30),
    )
    .await;
    Ok(())
}

// ------------------------------------------------------------------ WeChat

async fn xdotool(args: &[&str]) -> Option<String> {
    let mut argv = vec!["xdotool"];
    argv.extend_from_slice(args);
    run(&argv, Duration::from_secs(5)).await.ok()
}
async fn wechat_wids() -> Vec<String> {
    xdotool(&["search", "--class", "wechat"])
        .await
        .map(|o| {
            o.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}
async fn window_name(wid: &str) -> String {
    xdotool(&["getwindowname", wid])
        .await
        .map(|s| s.trim_end_matches('\n').to_string())
        .unwrap_or_default()
}
/// `wechat_xwindow_by_title`: last X window of class wechat with that name.
async fn xwindow_by_title(title: &str) -> Option<String> {
    let mut found = None;
    for wid in wechat_wids().await {
        if window_name(&wid).await == title {
            found = Some(wid);
        }
    }
    found
}
/// `wechat_popup_xwindow`: name "wechat", 250..420 x 150..300 per xwininfo.
async fn popup_xwindow() -> Option<String> {
    let mut found = None;
    for wid in wechat_wids().await {
        if window_name(&wid).await != "wechat" {
            continue;
        }
        let Ok(info) = run(&["xwininfo", "-id", &wid], Duration::from_secs(5)).await else {
            continue;
        };
        let field = |k: &str| {
            info.lines()
                .find_map(|l| l.trim().strip_prefix(k).map(|v| v.trim().to_string()))
                .and_then(|v| {
                    v.split_whitespace()
                        .next()
                        .and_then(|n| n.parse::<i64>().ok())
                })
        };
        if let (Some(w), Some(h)) = (field("Width:"), field("Height:")) {
            if (250..=420).contains(&w) && (150..=300).contains(&h) {
                found = Some(wid);
            }
        }
    }
    found
}
async fn clients(hypr: &Hypr) -> Vec<Value> {
    hypr.json("clients").await.unwrap_or_default()
}
/// `wechat_hypr_client_address_by_title` (last match).
async fn client_by_title(hypr: &Hypr, title: &str) -> Option<String> {
    clients(hypr)
        .await
        .iter()
        .filter(|c| c["class"] == "wechat" && c["title"] == title)
        .filter_map(|c| c["address"].as_str().map(str::to_string))
        .next_back()
}
/// `wechat_popup_hypr_address`: floating "wechat" 140..180 x 90..130 (last).
async fn popup_client(hypr: &Hypr) -> Option<String> {
    clients(hypr)
        .await
        .iter()
        .filter(|c| {
            let (w, h) = (
                c["size"][0].as_i64().unwrap_or(0),
                c["size"][1].as_i64().unwrap_or(0),
            );
            c["class"] == "wechat"
                && c["title"] == "wechat"
                && c["floating"] == true
                && (140..=180).contains(&w)
                && (90..=130).contains(&h)
        })
        .filter_map(|c| c["address"].as_str().map(str::to_string))
        .next_back()
}
async fn move_hidden(hypr: &Hypr, addr: &str, workspace: &str) {
    if !crate::hypr::valid_address(addr) {
        return;
    }
    let (Some(w), Some(ws)) = (
        crate::hypr::lua_quote(&format!("address:{addr}")),
        crate::hypr::lua_quote(workspace),
    ) else {
        return;
    };
    let lua = format!("hl.dsp.window.move({{ workspace = {ws}, window = {w}, follow = false }})");
    if hypr.dispatch(&lua).await.is_err() {
        let _ = hypr
            .dispatch(&format!("movetoworkspacesilent {workspace},address:{addr}"))
            .await;
    }
}
async fn cleanup(hypr: &Hypr, active: Option<&str>) {
    if let Some(wid) = xwindow_by_title("设置").await {
        let _ = xdotool(&["windowclose", &wid]).await;
    }
    if let Some(a) = active {
        let _ = hypr.focus_address(a).await;
    }
}
/// `sync_wechat_appearance THEME`: drive WeChat's settings UI (no appearance API).
pub async fn sync_wechat(env: &Env, hypr: &Hypr, dark: bool) -> Result<(), ServiceError> {
    let path = env.var("PATH");
    if util::which("xdotool", path).is_none() || util::which("xwininfo", path).is_none() {
        return Ok(());
    }
    let Some(main) = xwindow_by_title("微信").await else {
        return Ok(());
    };
    let hidden = "special:wechat-ui";
    let active: Option<String> = hypr.json::<Value>("activewindow").await.ok().and_then(|w| {
        w["address"]
            .as_str()
            .filter(|a| !a.is_empty())
            .map(str::to_string)
    });
    let active = active.as_deref();

    let mut settings_wid = xwindow_by_title("设置").await;
    let mut settings_addr = client_by_title(hypr, "设置").await;
    if settings_wid.is_none() || settings_addr.is_none() {
        if xdotool(&["key", "--window", &main, "ctrl+comma"])
            .await
            .is_none()
        {
            return Ok(());
        }
        settings_addr = None;
        for _ in 0..30 {
            settings_addr = client_by_title(hypr, "设置").await;
            if settings_addr.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        settings_wid = xwindow_by_title("设置").await;
    }
    let (Some(settings_wid), Some(settings_addr)) = (settings_wid, settings_addr) else {
        return Ok(());
    };
    move_hidden(hypr, &settings_addr, hidden).await;
    if let Some(a) = active {
        let _ = hypr.focus_address(a).await;
    }
    // Coordinates are XWayland window pixels from the current WeChat settings UI.
    let _ = xdotool(&[
        "mousemove",
        "--window",
        &settings_wid,
        "120",
        "225",
        "click",
        "1",
    ])
    .await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let _ = xdotool(&[
        "mousemove",
        "--window",
        &settings_wid,
        "910",
        "490",
        "click",
        "1",
    ])
    .await;

    let mut popup_addr = None;
    for _ in 0..20 {
        popup_addr = popup_client(hypr).await;
        if popup_addr.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let Some(popup_addr) = popup_addr else {
        cleanup(hypr, active).await;
        return Ok(());
    };
    move_hidden(hypr, &popup_addr, hidden).await;
    if let Some(a) = active {
        let _ = hypr.focus_address(a).await;
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    let Some(popup_wid) = popup_xwindow().await else {
        cleanup(hypr, active).await;
        return Ok(());
    };
    let menu_y = if dark { "150" } else { "80" };
    let _ = xdotool(&[
        "mousemove",
        "--window",
        &popup_wid,
        "150",
        menu_y,
        "click",
        "1",
    ])
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    // Close the hidden settings window so repeated applications start cleanly.
    cleanup(hypr, active).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(flavor = "current_thread")]
    async fn oneshot_exec_semantics() {
        assert_eq!(
            run(&["printf", "x"], Duration::from_secs(5)).await.unwrap(),
            "x"
        );
        assert_eq!(
            run(&["false"], Duration::from_secs(5))
                .await
                .unwrap_err()
                .code,
            "exec"
        );
        assert_eq!(
            run(&["sleep", "5"], Duration::from_millis(50))
                .await
                .unwrap_err()
                .code,
            "timeout"
        );
        assert_eq!(
            run(&["/nonexistent-hyprv"], Duration::from_secs(1))
                .await
                .unwrap_err()
                .code,
            "exec"
        );
    }
}
