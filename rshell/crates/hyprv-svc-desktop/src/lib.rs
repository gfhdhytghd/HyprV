//! Desktop-domain service actors: agent island, Deskflow/Viewflow target routing,
//! serial IR, eGPU removal, demo mode, recording, theme, applications and the
//! startup approval prompt. Every actor is event-driven: inotify, D-Bus signals,
//! kernel uevents, pidfds and sockets. Sampling (process scans) only runs while
//! a lease subscription is held.
use hyprv_core::store::{StateStore, Subscription, Topic, TopicKey};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tokio::sync::{mpsc, oneshot, watch};

pub mod agent_island;
pub mod applications;
pub mod demo;
pub mod deskflow;
pub mod egpu;
pub mod hypr;
pub mod ir;
pub mod recording;
pub mod startup;
pub mod systemd;
pub mod theme;
pub mod uevent;
pub mod util;
pub mod viewflow;

/// Paths and environment, resolved once. Tests construct rooted variants.
#[derive(Clone, Debug)]
pub struct Env {
    pub home: PathBuf,
    pub config_home: PathBuf,
    pub state_home: PathBuf,
    pub cache_home: PathBuf,
    pub data_home: PathBuf,
    pub runtime_dir: PathBuf,
    /// `HYPRV_CONFIG_DIR`, default `~/.config/HyprV` (hardcoded HOME path in source).
    pub hyprv_config_dir: PathBuf,
    pub vars: BTreeMap<String, String>,
}
impl Env {
    pub fn from_process() -> Self {
        let vars: BTreeMap<String, String> = std::env::vars().collect();
        Self::from_vars(vars)
    }
    pub fn from_vars(vars: BTreeMap<String, String>) -> Self {
        let get = |k: &str| vars.get(k).filter(|v| !v.is_empty()).map(PathBuf::from);
        let home = get("HOME").unwrap_or_else(|| PathBuf::from("/"));
        let uid = unsafe { libc::getuid() };
        Self {
            config_home: get("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config")),
            state_home: get("XDG_STATE_HOME").unwrap_or_else(|| home.join(".local/state")),
            cache_home: get("XDG_CACHE_HOME").unwrap_or_else(|| home.join(".cache")),
            data_home: get("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share")),
            runtime_dir: get("XDG_RUNTIME_DIR")
                .unwrap_or_else(|| PathBuf::from(format!("/run/user/{uid}"))),
            hyprv_config_dir: get("HYPRV_CONFIG_DIR")
                .map(|p| util::expand_tilde(&p, &home))
                .unwrap_or_else(|| home.join(".config/HyprV")),
            home,
            vars,
        }
    }
    /// Every XDG directory below `root`; used by tests.
    pub fn rooted(root: &std::path::Path) -> Self {
        let mut vars = BTreeMap::new();
        vars.insert("HOME".into(), root.join("home").display().to_string());
        vars.insert(
            "XDG_RUNTIME_DIR".into(),
            root.join("run").display().to_string(),
        );
        // Never drive the real RAM LEDs from a test tree.
        vars.insert("HYPRV_RAM_RGB".into(), "off".into());
        Self::from_vars(vars)
    }
    pub fn var(&self, key: &str) -> Option<&str> {
        self.vars
            .get(key)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }
}

/// Shared construction context for every actor in this crate.
#[derive(Clone)]
pub struct Ctx {
    pub store: Arc<StateStore>,
    pub rt: tokio::runtime::Handle,
    pub shutdown: watch::Receiver<bool>,
    pub env: Arc<Env>,
}
impl Ctx {
    pub fn new(
        store: Arc<StateStore>,
        rt: tokio::runtime::Handle,
        shutdown: watch::Receiver<bool>,
    ) -> Self {
        Self {
            store,
            rt,
            shutdown,
            env: Arc::new(Env::from_process()),
        }
    }
    pub fn with_env(mut self, env: Env) -> Self {
        self.env = Arc::new(env);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RequestId(pub u64);
static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);
impl RequestId {
    pub fn next() -> Self {
        Self(NEXT_REQUEST.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}
impl ServiceError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable: false,
        }
    }
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new("unsupported", message)
    }
    pub fn io(e: impl std::fmt::Display) -> Self {
        Self::new("io", e.to_string())
    }
}
impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for ServiceError {}
impl From<std::io::Error> for ServiceError {
    fn from(e: std::io::Error) -> Self {
        Self::io(e)
    }
}
impl From<zbus::fdo::Error> for ServiceError {
    fn from(e: zbus::fdo::Error) -> Self {
        Self::new("dbus", e.to_string())
    }
}
impl From<zbus::Error> for ServiceError {
    fn from(e: zbus::Error) -> Self {
        Self::new("dbus", e.to_string())
    }
}

/// Success carries the legacy helper's human-readable stdout where one existed.
pub type ActionResult = Result<String, ServiceError>;

pub struct Envelope<A> {
    pub id: RequestId,
    pub action: A,
    pub reply: Option<oneshot::Sender<ActionResult>>,
}
impl<A> Envelope<A> {
    pub fn respond(self, result: ActionResult) -> (A, RequestId) {
        if let Some(reply) = self.reply {
            let _ = reply.send(result);
        }
        (self.action, self.id)
    }
}

/// Per-module handle: typed state topic plus a bounded action queue.
pub struct Handle<S: Clone + PartialEq, A> {
    pub topic: Topic<S>,
    tx: mpsc::Sender<Envelope<A>>,
}
impl<S: Clone + PartialEq, A> Clone for Handle<S, A> {
    fn clone(&self) -> Self {
        Self {
            topic: self.topic.clone(),
            tx: self.tx.clone(),
        }
    }
}
impl<S: Clone + PartialEq, A: Send + 'static> Handle<S, A> {
    pub fn new(topic: Topic<S>) -> (Self, mpsc::Receiver<Envelope<A>>) {
        let (tx, rx) = mpsc::channel(32);
        (Self { topic, tx }, rx)
    }
    pub fn subscribe(&self) -> Subscription<S> {
        self.topic.subscribe()
    }
    pub fn state(&self) -> S {
        self.topic.snapshot().value
    }
    /// Fire-and-forget; the action outlives its initiating popup.
    pub fn dispatch(&self, action: A) -> Result<RequestId, ServiceError> {
        let id = RequestId::next();
        self.tx
            .try_send(Envelope {
                id,
                action,
                reply: None,
            })
            .map_err(|_| ServiceError::new("busy", "action queue full or actor stopped"))?;
        Ok(id)
    }
    pub async fn request(&self, action: A) -> ActionResult {
        let (reply, wait) = oneshot::channel();
        self.tx
            .send(Envelope {
                id: RequestId::next(),
                action,
                reply: Some(reply),
            })
            .await
            .map_err(|_| ServiceError::new("stopped", "actor stopped"))?;
        wait.await
            .map_err(|_| ServiceError::new("stopped", "actor dropped request"))?
    }
}

/// Topic marker helper: `topic_key!(Marker, ValueType)`.
#[macro_export]
macro_rules! topic_key {
    ($name:ident, $value:ty) => {
        pub struct $name;
        impl hyprv_core::store::TopicKey for $name {
            type Value = $value;
        }
    };
}

/// A demand-only lease. Holding the returned subscription enables sampling.
#[derive(Clone)]
pub struct Lease(Topic<()>);
impl Lease {
    pub fn new() -> Self {
        Self(Topic::new(()))
    }
    pub fn acquire(&self) -> Subscription<()> {
        self.0.subscribe()
    }
    pub fn demand(&self) -> watch::Receiver<usize> {
        self.0.demand()
    }
    pub fn held(&self) -> bool {
        self.0.subscriber_count() > 0
    }
}
impl Default for Lease {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn topic<K: TopicKey>(ctx: &Ctx, initial: K::Value) -> Topic<K::Value> {
    ctx.store.topic::<K>(initial)
}

/// Windows/macOS routing target; serialized as the legacy `windows`/`macos` text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    Windows,
    #[serde(rename = "macos")]
    MacOs,
}
impl Target {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::MacOs => "macos",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "windows" => Some(Self::Windows),
            "macos" => Some(Self::MacOs),
            _ => None,
        }
    }
    pub fn other(self) -> Self {
        match self {
            Self::Windows => Self::MacOs,
            Self::MacOs => Self::Windows,
        }
    }
    /// IR button learned for the HDMI switch input (`input1`/`input2`).
    pub fn ir_button(self) -> &'static str {
        match self {
            Self::Windows => "input1",
            Self::MacOs => "input2",
        }
    }
}

/// Every actor of this crate. IR is shared by Deskflow and Viewflow.
pub struct DesktopServices {
    pub agent_island: agent_island::AgentIslandHandle,
    pub ir: ir::IrHandle,
    pub deskflow: deskflow::DeskflowHandle,
    pub viewflow: viewflow::ViewflowHandle,
    pub egpu: egpu::EgpuHandle,
    pub demo: demo::DemoHandle,
    pub recording: recording::RecordingHandle,
    pub theme: theme::ThemeHandle,
    pub applications: applications::ApplicationsHandle,
    pub startup: startup::StartupHandle,
}
pub fn spawn_all(ctx: &Ctx) -> DesktopServices {
    let ir = ir::spawn(ctx);
    DesktopServices {
        agent_island: agent_island::spawn(ctx),
        deskflow: deskflow::spawn(ctx, ir.clone()),
        viewflow: viewflow::spawn(ctx, ir.clone()),
        ir,
        egpu: egpu::spawn(ctx),
        demo: demo::spawn(ctx, demo::default_inhibitor()),
        recording: recording::spawn(ctx),
        theme: theme::spawn(ctx),
        applications: applications::spawn(ctx),
        startup: startup::spawn(ctx),
    }
}
