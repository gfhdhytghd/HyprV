//! Hyprland request IPC and socket2; subscribes before snapshotting.
use crate::support::*;
use hyprv_core::store::TopicKey;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{path::PathBuf, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DesktopWindow {
    pub address: String,
    pub pid: Option<u32>,
    pub class: String,
    pub title: String,
    pub monitor: i32,
    pub geometry: Rect,
    /// Exact Hyprland fullscreen mode 2; maximized windows (mode 1) keep the bar.
    pub fullscreen: bool,
    #[serde(default)]
    pub workspace: i32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DesktopMonitor {
    pub name: String,
    pub id: i32,
    pub geometry: Rect,
    pub scale: f64,
    pub transform: u8,
    /// The workspace this monitor currently shows (not the globally focused one).
    #[serde(default)]
    pub active_workspace: i32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: i32,
    pub name: String,
    pub monitor: String,
    pub active: bool,
    pub windows: u32,
    pub tooltip: String,
    pub state: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub connected: bool,
    pub active_window: Option<String>,
    pub windows: Vec<DesktopWindow>,
    pub monitors: Vec<DesktopMonitor>,
    pub workspaces: Vec<Workspace>,
    /// Workspaces holding a window that raised `urgent`, until that workspace is focused
    /// (Quickshell HyprlandWorkspace.urgent semantics).
    #[serde(default)]
    pub urgent_workspaces: Vec<i32>,
    pub keymap: String,
    pub move_duration_ms: u32,
    pub move_bezier: [f64; 6],
    pub curve_name: String,
    pub last_error: Option<Error>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            connected: false,
            active_window: None,
            windows: vec![],
            monitors: vec![],
            workspaces: vec![],
            urgent_workspaces: vec![],
            keymap: String::new(),
            move_duration_ms: 700,
            move_bezier: [0.1, 0.9, 0.1, 1.05, 1., 1.],
            curve_name: "myBezier".into(),
            last_error: None,
        }
    }
}
impl Model for State {
    fn failure(&mut self, e: Error) {
        self.connected = false;
        self.active_window = None;
        self.windows.clear();
        self.monitors.clear();
        self.workspaces.clear();
        self.last_error = Some(e);
    }
    fn action_failure(&mut self, error: Error) {
        self.last_error = Some(error);
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Action {
    Refresh,
    FocusWindow(String),
    WorkspaceActivate(i32),
    WorkspaceMoveWindow(i32),
}
#[derive(Clone, Debug)]
pub struct Config {
    pub directory: PathBuf,
}
impl Config {
    pub fn from_env() -> Result<Self> {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .ok_or_else(|| Error::new("environment", "XDG_RUNTIME_DIR missing"))?;
        let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")
            .map_err(|_| Error::new("environment", "HYPRLAND_INSTANCE_SIGNATURE missing"))?;
        if signature.contains('/') || signature.is_empty() {
            return Err(Error::new("environment", "invalid instance signature"));
        }
        Ok(Self {
            directory: PathBuf::from(runtime).join("hypr").join(signature),
        })
    }
}
pub struct Key;
impl TopicKey for Key {
    type Value = State;
}
pub async fn request(directory: &std::path::Path, command: &str) -> Result<String> {
    if command.len() > 65536 || command.contains('\0') {
        return Err(Error::new("command", "invalid compositor request"));
    }
    tokio::time::timeout(Duration::from_secs(3), async {
        let mut stream = tokio::net::UnixStream::connect(directory.join(".socket.sock")).await?;
        stream.write_all(command.as_bytes()).await?;
        let mut bytes = vec![];
        stream
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .await?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err(Error::new("frame", "compositor response too large"));
        }
        String::from_utf8(bytes).map_err(|e| Error::new("utf8", e))
    })
    .await
    .map_err(|_| Error::new("timeout", "Hyprland request timed out"))?
}
async fn query(c: &Config, name: &str) -> Result<Value> {
    Ok(serde_json::from_str(
        &request(&c.directory, &format!("j/{name}")).await?,
    )?)
}
fn text(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or_default().into()
}
fn num(v: &Value, k: &str) -> f64 {
    v[k].as_f64().unwrap_or_default()
}
fn motion(state: &mut State, v: &Value) {
    let Some(animations) = v[0].as_array() else {
        return;
    };
    let Some(curves) = v[1].as_array() else {
        return;
    };
    let config = ["windowsMove", "windows", "global"]
        .iter()
        .find_map(|name| {
            animations
                .iter()
                .find(|a| a["name"] == *name && a["overridden"] == true)
        });
    if let Some(a) = config {
        if let Some(c) = curves.iter().find(|c| c["name"] == a["bezier"]) {
            state.move_duration_ms = if a["enabled"] == true {
                (num(a, "speed") * 100.).round().max(0.) as u32
            } else {
                0
            };
            state.curve_name = text(a, "bezier");
            state.move_bezier = [
                num(c, "X0"),
                num(c, "Y0"),
                num(c, "X1"),
                num(c, "Y1"),
                1.,
                1.,
            ];
        }
    }
}
struct Native {
    state: State,
    config: Result<Config>,
    events: Option<Lines>,
}
impl Native {
    async fn snapshot(&mut self, with_motion: bool) -> Result<()> {
        let c = self.config.as_ref().map_err(Clone::clone)?;
        let windows = query(c, "clients").await?;
        let monitors = query(c, "monitors").await?;
        let spaces = query(c, "workspaces").await?;
        let active = query(c, "activewindow").await?;
        let focused_workspace = query(c, "activeworkspace").await?;
        let devices = query(c, "devices").await?;
        self.state.windows = windows
            .as_array()
            .ok_or_else(|| Error::new("schema", "clients must be array"))?
            .iter()
            .map(|w| DesktopWindow {
                address: text(w, "address"),
                pid: w["pid"].as_u64().map(|x| x as u32),
                class: text(w, "class"),
                title: text(w, "title"),
                monitor: num(w, "monitor") as i32,
                geometry: Rect {
                    x: w["at"][0].as_f64().unwrap_or_default(),
                    y: w["at"][1].as_f64().unwrap_or_default(),
                    width: w["size"][0].as_f64().unwrap_or_default(),
                    height: w["size"][1].as_f64().unwrap_or_default(),
                },
                fullscreen: num(w, "fullscreen") == 2.,
                workspace: num(&w["workspace"], "id") as i32,
            })
            .collect();
        self.state.monitors = monitors
            .as_array()
            .ok_or_else(|| Error::new("schema", "monitors must be array"))?
            .iter()
            .map(|m| {
                let scale = num(m, "scale").max(0.01);
                let transform = num(m, "transform") as u8;
                let (w, h) = if transform % 2 == 1 {
                    (num(m, "height"), num(m, "width"))
                } else {
                    (num(m, "width"), num(m, "height"))
                };
                DesktopMonitor {
                    name: text(m, "name"),
                    id: num(m, "id") as i32,
                    geometry: Rect {
                        x: num(m, "x"),
                        y: num(m, "y"),
                        width: w / scale,
                        height: h / scale,
                    },
                    scale,
                    transform,
                    active_workspace: num(&m["activeWorkspace"], "id") as i32,
                }
            })
            .collect();
        self.state.active_window = active["address"]
            .as_str()
            .filter(|s| !s.is_empty() && *s != "0x0")
            .map(str::to_string);
        let active_id = focused_workspace["id"].as_i64();
        let spaces = spaces
            .as_array()
            .ok_or_else(|| Error::new("schema", "workspaces must be array"))?;
        self.state.workspaces = spaces
            .iter()
            .map(|w| {
                let id = num(w, "id") as i32;
                let active = active_id == Some(id as i64);
                let windows = num(w, "windows") as u32;
                Workspace {
                    id,
                    name: text(w, "name"),
                    monitor: text(w, "monitor"),
                    active,
                    windows,
                    tooltip: text(w, "lastwindowtitle"),
                    state: if active {
                        "active"
                    } else if windows > 0 {
                        "occupied"
                    } else {
                        "empty"
                    }
                    .into(),
                }
            })
            .collect();
        // Only workspaces Hyprland actually has, as the legacy bar's Hyprland.workspaces;
        // no padding to a fixed 1..8 range.
        self.state.workspaces.sort_by_key(|w| w.id);
        let live: Vec<i32> = self.state.workspaces.iter().map(|w| w.id).collect();
        self.state
            .urgent_workspaces
            .retain(|id| Some(*id as i64) != active_id && live.contains(id));
        if let Some(k) = devices["keyboards"]
            .as_array()
            .and_then(|ks| ks.iter().find(|k| k["main"] == true).or_else(|| ks.first()))
        {
            self.state.keymap = text(k, "active_keymap");
        }
        if with_motion {
            motion(&mut self.state, &query(c, "animations").await?);
        }
        self.state.connected = true;
        self.state.last_error = None;
        Ok(())
    }
}
#[async_trait::async_trait]
impl Backend for Native {
    type State = State;
    type Action = Action;
    fn state(&self) -> &State {
        &self.state
    }
    fn state_mut(&mut self) -> &mut State {
        &mut self.state
    }
    async fn connect(&mut self) -> Result<()> {
        let c = self.config.as_ref().map_err(Clone::clone)?;
        let stream = tokio::net::UnixStream::connect(c.directory.join(".socket2.sock")).await?;
        let (read, _) = stream.into_split();
        self.events = Some(lines(read));
        self.snapshot(true).await
    }
    async fn next(&mut self) -> Result<()> {
        loop {
            let event = line(self.events.as_mut().unwrap()).await?;
            let name = event.split_once(">>").map(|x| x.0).unwrap_or("");
            if name == "urgent" {
                // `urgent>>ADDRESS` (hex without 0x); the window list carries its workspace.
                let address = format!("0x{}", event.split_once(">>").map(|x| x.1).unwrap_or(""));
                let focused = self.state.workspaces.iter().find(|w| w.active).map(|w| w.id);
                if let Some(id) = self.state.windows.iter().find(|w| w.address == address).map(|w| w.workspace) {
                    if Some(id) != focused && id > 0 && !self.state.urgent_workspaces.contains(&id) {
                        self.state.urgent_workspaces.push(id);
                        self.state.urgent_workspaces.sort_unstable();
                        return Ok(());
                    }
                }
                continue;
            }
            if name == "activelayout" {
                self.state.keymap = event
                    .split_once(',')
                    .map(|x| x.1)
                    .unwrap_or_default()
                    .into();
                return Ok(());
            }
            if [
                "workspace",
                "focusedmon",
                "activewindow",
                "openwindow",
                "closewindow",
                "movewindow",
                "createworkspace",
                "destroyworkspace",
                "monitoradded",
                "monitorremoved",
                "changefloatingmode",
                "fullscreen",
                "windowtitle",
                "configreloaded",
            ]
            .iter()
            .any(|prefix| name.starts_with(prefix))
            {
                return self.snapshot(name == "configreloaded").await;
            }
        }
    }
    async fn action(&mut self, _id: u64, a: Action) -> Result<Value> {
        let command = match a {
            Action::Refresh => {
                self.snapshot(true).await?;
                return Ok(Value::Null);
            }
            Action::FocusWindow(address) => {
                if !address.starts_with("0x")
                    || address.len() > 20
                    || !address[2..].bytes().all(|c| c.is_ascii_hexdigit())
                    || address.len() == 2
                {
                    return Err(Error::new("address", "invalid window address"));
                }
                format!("dispatch focuswindow address:{address}")
            }
            Action::WorkspaceActivate(id) => format!("dispatch workspace {id}"),
            Action::WorkspaceMoveWindow(id) => format!("dispatch movetoworkspace {id}"),
        };
        let reply = request(
            &self.config.as_ref().map_err(Clone::clone)?.directory,
            &command,
        )
        .await?;
        if reply.trim() != "ok" {
            return Err(Error::new("compositor", reply));
        }
        Ok(json!({"accepted":true}))
    }
}
pub fn spawn(ctx: Context) -> Handle<State, Action> {
    spawn_with_config(ctx, Config::from_env())
}
pub fn spawn_with_config(ctx: Context, config: Result<Config>) -> Handle<State, Action> {
    launch::<Key, _>(
        ctx,
        Native {
            state: State::default(),
            config,
            events: None,
        },
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inherited_motion() {
        let mut s = State::default();
        motion(
            &mut s,
            &json!([[{"name":"windowsMove","overridden":false},{"name":"windows","overridden":true,"speed":2.5,"enabled":true,"bezier":"x"}],[{"name":"x","X0":0.2,"Y0":0.4,"X1":0.8,"Y1":1.2}]]),
        );
        assert_eq!(s.move_duration_ms, 250);
        assert_eq!(s.move_bezier, [0.2, 0.4, 0.8, 1.2, 1., 1.]);
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    #[tokio::test]
    async fn native_request_socket_roundtrip() {
        let dir = crate::support::TestDir::new("hyprland");
        let listener =
            tokio::net::UnixListener::bind(dir.socket_dir().join(".socket.sock")).unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 128];
            let count = stream.read(&mut bytes).await.unwrap();
            assert_eq!(&bytes[..count], b"j/monitors");
            stream
                .write_all(b"[{\"id\":1,\"name\":\"TEST\"}]")
                .await
                .unwrap();
        });
        let reply = request(&dir.socket_dir(), "j/monitors").await.unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&reply).unwrap()[0]["name"],
            "TEST"
        );
        server.await.unwrap();
    }
}

#[cfg(test)]
mod urgent_tests {
    use super::*;
    fn native(windows: &[(&str, i32)], focused: i32) -> Native {
        let mut state = State::default();
        state.windows = windows
            .iter()
            .map(|(a, ws)| DesktopWindow {
                address: a.to_string(),
                pid: None,
                class: String::new(),
                title: String::new(),
                monitor: 0,
                geometry: Rect { x: 0., y: 0., width: 0., height: 0. },
                fullscreen: false,
                workspace: *ws,
            })
            .collect();
        state.workspaces = [1, 3]
            .iter()
            .map(|&id| Workspace {
                id,
                name: id.to_string(),
                monitor: "M".into(),
                active: id == focused,
                windows: 1,
                tooltip: String::new(),
                state: String::new(),
            })
            .collect();
        Native { state, config: Err(Error::new("test", "no compositor")), events: None }
    }
    async fn feed(n: &mut Native, lines_in: &str) {
        let (mut w, r) = tokio::net::UnixStream::pair().unwrap();
        tokio::io::AsyncWriteExt::write_all(&mut w, lines_in.as_bytes()).await.unwrap();
        let (read, _) = r.into_split();
        n.events = Some(lines(read));
        n.next().await.unwrap();
    }
    #[tokio::test]
    async fn urgent_marks_the_window_workspace_but_not_the_focused_one() {
        let mut n = native(&[("0xaa", 3), ("0xbb", 1)], 1);
        feed(&mut n, "urgent>>bb\nurgent>>aa\n").await;
        assert_eq!(n.state.urgent_workspaces, vec![3]);
    }
}
