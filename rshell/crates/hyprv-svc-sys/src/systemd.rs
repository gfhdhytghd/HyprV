//! systemd Manager Subscribe/Unit/Job lifecycle, following Plasma's service jobs.
use crate::support::*;
use futures_util::{
    stream::{BoxStream, SelectAll},
    StreamExt,
};
use hyprv_core::store::TopicKey;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum UnitScope {
    User,
    System,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitState {
    pub name: String,
    pub scope: UnitScope,
    pub active_state: String,
    pub sub_state: String,
    pub job: Option<u32>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub connected: bool,
    pub units: BTreeMap<String, UnitState>,
    pub jobs: BTreeMap<String, String>,
    pub last_error: Option<Error>,
}
impl Model for State {
    fn failure(&mut self, e: Error) {
        self.connected = false;
        self.last_error = Some(e);
    }
    fn action_failure(&mut self, error: Error) {
        self.last_error = Some(error);
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum UnitOperation {
    Start,
    Stop,
    Restart,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Action {
    Watch {
        scope: UnitScope,
        name: String,
    },
    Operate {
        scope: UnitScope,
        name: String,
        operation: UnitOperation,
    },
    Refresh,
}
pub struct Key;
impl TopicKey for Key {
    type Value = State;
}
/// One subscribed connection per scope, owned by the shared core store.
#[derive(Clone, Default)]
pub(crate) struct BusPool(std::sync::Arc<tokio::sync::Mutex<BTreeMap<UnitScope, Manager>>>);
impl PartialEq for BusPool {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}
struct PoolKey;
impl TopicKey for PoolKey {
    type Value = BusPool;
}
impl BusPool {
    pub fn get(ctx: &Context) -> Self {
        ctx.store.topic::<PoolKey>(Self::default()).snapshot().value
    }
    pub async fn manager(&self, scope: UnitScope) -> Result<Manager> {
        let mut managers = self.0.lock().await;
        if let Some(manager) = managers.get(&scope) {
            if !manager.connection.is_closed() {
                manager.subscribe().await?;
                return Ok(manager.clone());
            }
        }
        let manager = Manager::connect(scope).await?;
        managers.insert(scope, manager.clone());
        Ok(manager)
    }
}
pub type Signals = SelectAll<BoxStream<'static, zbus::Result<zbus::Message>>>;
/// Independently drains raw zbus receivers so they cannot block RPC replies on
/// the pooled connection. Property/unit invalidations coalesce; job completions
/// are queued separately and never silently evicted (overflow is an error).
struct SignalBuffer {
    jobs: std::collections::VecDeque<zbus::Message>,
    dirty: Option<zbus::Message>,
    owner: Option<zbus::Message>,
    error: Option<zbus::Error>,
    closed: bool,
}
struct SignalShared {
    buffer: std::sync::Mutex<SignalBuffer>,
    wake: futures_util::task::AtomicWaker,
}
struct DrainedSignals {
    shared: std::sync::Arc<SignalShared>,
    task: tokio::task::JoinHandle<()>,
}
impl futures_util::Stream for DrainedSignals {
    type Item = zbus::Result<zbus::Message>;
    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.shared.wake.register(cx.waker());
        let mut buffer = self.shared.buffer.lock().unwrap();
        if let Some(error) = buffer.error.take() {
            return std::task::Poll::Ready(Some(Err(error)));
        }
        if let Some(message) = buffer
            .owner
            .take()
            .or_else(|| buffer.jobs.pop_front())
            .or_else(|| buffer.dirty.take())
        {
            return std::task::Poll::Ready(Some(Ok(message)));
        }
        if buffer.closed {
            std::task::Poll::Ready(None)
        } else {
            std::task::Poll::Pending
        }
    }
}
impl Drop for DrainedSignals {
    fn drop(&mut self) {
        self.task.abort();
    }
}
fn drain_signals(
    mut raw: zbus::MessageStream,
    job_unit: Option<String>,
) -> BoxStream<'static, zbus::Result<zbus::Message>> {
    let shared = std::sync::Arc::new(SignalShared {
        buffer: std::sync::Mutex::new(SignalBuffer {
            jobs: Default::default(),
            dirty: None,
            owner: None,
            error: None,
            closed: false,
        }),
        wake: Default::default(),
    });
    let output = shared.clone();
    let task = tokio::spawn(async move {
        while let Some(message) = raw.next().await {
            {
                let mut buffer = output.buffer.lock().unwrap();
                match message {
                    Ok(message) => {
                        let header = message.header();
                        let member = header.member().map(|name| name.as_str());
                        if member == Some("JobRemoved") {
                            if let Some(unit) = &job_unit {
                                let completion: std::result::Result<
                                    (u32, zbus::zvariant::OwnedObjectPath, String, String),
                                    _,
                                > = message.body().deserialize();
                                if completion
                                    .as_ref()
                                    .is_ok_and(|(_, _, name, _)| name != unit)
                                {
                                    continue;
                                }
                            }
                            if buffer.jobs.len() < 256 {
                                buffer.jobs.push_back(message);
                            } else {
                                buffer.error = Some(zbus::Error::Failure(
                                    "systemd job completion overflow; outcomes require resnapshot"
                                        .into(),
                                ));
                            }
                        } else if member == Some("NameOwnerChanged") {
                            buffer.owner = Some(message);
                        } else {
                            buffer.dirty = Some(message);
                        }
                    }
                    Err(error) => {
                        buffer.error = Some(error);
                        buffer.closed = true;
                    }
                }
            }
            output.wake.wake();
        }
        output.buffer.lock().unwrap().closed = true;
        output.wake.wake();
    });
    DrainedSignals { shared, task }.boxed()
}
#[derive(Clone)]
pub struct Manager {
    pub scope: UnitScope,
    pub connection: zbus::Connection,
}
impl Manager {
    pub async fn connect(scope: UnitScope) -> Result<Self> {
        let connection = match scope {
            UnitScope::System => zbus::Connection::system().await?,
            UnitScope::User => zbus::Connection::session().await?,
        };
        let this = Self { scope, connection };
        this.subscribe().await?;
        Ok(this)
    }
    async fn subscribe(&self) -> Result<()> {
        match self.proxy().await?.call::<_, _, ()>("Subscribe", &()).await {
            Ok(()) => Ok(()),
            Err(zbus::Error::MethodError(name, _, _))
                if name.as_str().ends_with(".AlreadySubscribed") =>
            {
                Ok(())
            }
            Err(error) => Err(error.into()),
        }
    }
    async fn proxy(&self) -> Result<zbus::Proxy<'_>> {
        Ok(zbus::Proxy::new(
            &self.connection,
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
        )
        .await?)
    }
    pub async fn unit(&self, name: &str) -> Result<UnitState> {
        validate_unit(name)?;
        let path: zbus::zvariant::OwnedObjectPath =
            self.proxy().await?.call("LoadUnit", &(name,)).await?;
        let p = zbus::Proxy::new(
            &self.connection,
            "org.freedesktop.systemd1",
            path,
            "org.freedesktop.systemd1.Unit",
        )
        .await?;
        let load_state: String = p.get_property("LoadState").await?;
        if load_state == "not-found" {
            return Err(Error::new("unit", "unit not found"));
        }
        let active_state = p.get_property("ActiveState").await?;
        let sub_state = p.get_property("SubState").await?;
        let (id, _): (u32, zbus::zvariant::OwnedObjectPath) = p.get_property("Job").await?;
        Ok(UnitState {
            name: name.into(),
            scope: self.scope,
            active_state,
            sub_state,
            job: (id != 0).then_some(id),
        })
    }
    pub async fn operate(&self, name: &str, operation: UnitOperation) -> Result<String> {
        validate_unit(name)?;
        let method = match operation {
            UnitOperation::Start => "StartUnit",
            UnitOperation::Stop => "StopUnit",
            UnitOperation::Restart => "RestartUnit",
        };
        // ALLOW_INTERACTIVE_AUTHORIZATION hands polkit to the session agent.
        let path: Option<zbus::zvariant::OwnedObjectPath> = self
            .proxy()
            .await?
            .call_with_flags(
                method,
                zbus::proxy::MethodFlags::AllowInteractiveAuth.into(),
                &(name, "replace"),
            )
            .await?;
        let path = path.ok_or_else(|| Error::new("job", "missing job reply"))?;
        Ok(path.to_string())
    }
    pub async fn signals(&self) -> Result<Signals> {
        self.signal_streams(None).await
    }
    pub async fn signals_for_unit(&self, unit: &str) -> Result<Signals> {
        validate_unit(unit)?;
        self.signal_streams(Some(unit.into())).await
    }
    async fn signal_streams(&self, job_unit: Option<String>) -> Result<Signals> {
        
        // Subscribe to ownership before resolving the provider so replacement is
        // observed even when it races subscription setup.
        let owner = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender("org.freedesktop.DBus")?
            .interface("org.freedesktop.DBus")?
            .member("NameOwnerChanged")?
            .arg(0, "org.freedesktop.systemd1")?
            .build();
        let mut streams = SelectAll::new();
        streams.push(drain_signals(
            zbus::MessageStream::for_match_rule(owner, &self.connection, Some(256)).await?,
            job_unit.clone(),
        ));
        let sender = if self.connection.is_bus() {
            Some(
                zbus::fdo::DBusProxy::new(&self.connection)
                    .await?
                    .get_name_owner(
                        zbus::names::BusName::from_static_str("org.freedesktop.systemd1")
                            .expect("constant bus name"),
                    )
                    .await
                    .map_err(zbus::Error::from)?,
            )
        } else {
            None
        };
        let base = || -> zbus::Result<_> {
            let rule = zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal);
            // A unique sender also filters locally on shared connections.
            Ok(if let Some(sender) = &sender {
                rule.sender(sender.clone())?
            } else {
                rule
            })
        };
        let rules = if let Some(unit) = &job_unit {
            let path: zbus::zvariant::OwnedObjectPath =
                self.proxy().await?.call("LoadUnit", &(unit,)).await?;
            vec![
                base()?
                    .path(path)?
                    .interface("org.freedesktop.DBus.Properties")?
                    .member("PropertiesChanged")?
                    .build(),
                base()?
                    .path("/org/freedesktop/systemd1")?
                    .interface("org.freedesktop.systemd1.Manager")?
                    .member("JobRemoved")?
                    .arg(2, unit.as_str())?
                    .build(),
                base()?
                    .path("/org/freedesktop/systemd1")?
                    .interface("org.freedesktop.systemd1.Manager")?
                    .member("UnitNew")?
                    .arg(0, unit.as_str())?
                    .build(),
                base()?
                    .path("/org/freedesktop/systemd1")?
                    .interface("org.freedesktop.systemd1.Manager")?
                    .member("UnitRemoved")?
                    .arg(0, unit.as_str())?
                    .build(),
            ]
        } else {
            vec![base()?.path_namespace("/org/freedesktop/systemd1")?.build()]
        };
        for rule in rules {
            let raw =
                zbus::MessageStream::for_match_rule(rule, &self.connection, Some(256)).await?;
            streams.push(drain_signals(raw, job_unit.clone()));
        }
        Ok(streams)
    }
}
fn validate_unit(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 256 || name.bytes().any(|c| c <= 32 || c == b'/') {
        Err(Error::new("unit", "invalid unit name"))
    } else {
        Ok(())
    }
}
struct Native {
    buses: BusPool,
    state: State,
    managers: Vec<Manager>,
    events:
        SelectAll<BoxStream<'static, (UnitScope, std::result::Result<zbus::Message, zbus::Error>)>>,
    watched: Vec<(UnitScope, String)>,
}
impl Native {
    async fn refresh(&mut self) -> Result<()> {
        let mut error = None;
        for (scope, name) in &self.watched {
            let manager = self
                .managers
                .iter()
                .find(|manager| manager.scope == *scope)
                .ok_or_else(|| Error::new("bus", "scope unavailable"))?;
            let key = format!("{scope:?}:{name}");
            match manager.unit(name).await {
                Ok(unit) => {
                    self.state.units.insert(key, unit);
                }
                Err(e) => {
                    self.state.units.remove(&key);
                    error = Some(e);
                }
            }
        }
        self.state.last_error = error;
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
        self.events = SelectAll::new();
        self.managers.clear();
        for scope in [UnitScope::System, UnitScope::User] {
            let m = self.buses.manager(scope).await?;
            let events = m.signals().await?;
            self.events.push(events.map(move |e| (scope, e)).boxed());
            self.managers.push(m);
        }
        self.refresh().await?;
        self.state.connected = true;
        self.state.last_error = None;
        Ok(())
    }
    async fn next(&mut self) -> Result<()> {
        loop {
            let (scope, msg) = self
                .events
                .next()
                .await
                .ok_or_else(|| Error::new("bus", "signal stream closed"))?;
            let msg = msg?;
            let h = msg.header();
            if h.interface()
                .is_some_and(|i| i.as_str() == "org.freedesktop.systemd1.Manager")
            {
                if h.member().is_some_and(|m| m.as_str() == "JobRemoved") {
                    let (id, _, unit, result): (
                        u32,
                        zbus::zvariant::OwnedObjectPath,
                        String,
                        String,
                    ) = msg.body().deserialize()?;
                    self.state
                        .jobs
                        .insert(format!("{scope:?}:{id}"), format!("{unit}:{result}"));
                    while self.state.jobs.len() > 128 {
                        let first = self.state.jobs.keys().next().unwrap().clone();
                        self.state.jobs.remove(&first);
                    }
                }
                return self.refresh().await;
            }
            if h.interface()
                .is_some_and(|i| i.as_str() == "org.freedesktop.DBus.Properties")
                && h.path()
                    .is_some_and(|p| p.as_str().starts_with("/org/freedesktop/systemd1/unit/"))
            {
                return self.refresh().await;
            }
            if h.member().is_some_and(|m| m.as_str() == "NameOwnerChanged") {
                let (name, _, _): (String, String, String) = msg.body().deserialize()?;
                if name == "org.freedesktop.systemd1" {
                    return Err(Error::new("owner", "systemd owner changed"));
                }
            }
        }
    }
    async fn action(&mut self, _id: u64, a: Action) -> Result<serde_json::Value> {
        match a {
            Action::Watch { scope, name } => {
                validate_unit(&name)?;
                let manager = self
                    .managers
                    .iter()
                    .find(|manager| manager.scope == scope)
                    .ok_or_else(|| Error::new("scope", "bus unavailable"))?;
                let unit = manager.unit(&name).await?;
                if !self.watched.contains(&(scope, name.clone())) {
                    self.watched.push((scope, name.clone()));
                }
                self.state.units.insert(format!("{scope:?}:{name}"), unit);
                Ok(serde_json::Value::Null)
            }
            Action::Operate {
                scope,
                name,
                operation,
            } => {
                let m = self
                    .managers
                    .iter()
                    .find(|m| m.scope == scope)
                    .ok_or_else(|| Error::new("scope", "bus unavailable"))?;
                Ok(serde_json::json!({"job":m.operate(&name,operation).await?}))
            }
            Action::Refresh => {
                self.refresh().await?;
                Ok(serde_json::Value::Null)
            }
        }
    }
}
pub fn spawn(ctx: Context) -> Handle<State, Action> {
    let buses = BusPool::get(&ctx);
    launch::<Key, _>(
        ctx,
        Native {
            buses,
            state: State::default(),
            managers: vec![],
            events: SelectAll::new(),
            watched: vec![],
        },
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scopes_and_validation() {
        assert!(validate_unit("windows-autoshop.service").is_ok());
        assert!(validate_unit("../evil").is_err());
        assert_ne!(UnitScope::System, UnitScope::User);
    }
}

#[cfg(test)]
pub(crate) mod mock {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    const PATH: &str = "/org/freedesktop/systemd1/unit/windows_2dautoshop_2eservice";
    struct MockManager {
        running: Arc<AtomicBool>,
        calls: tokio::sync::mpsc::UnboundedSender<String>,
    }
    #[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
    impl MockManager {
        async fn subscribe(&self) {}
        async fn load_unit(
            &self,
            name: &str,
        ) -> zbus::fdo::Result<zbus::zvariant::OwnedObjectPath> {
            if name != crate::vm::UNIT {
                return Err(zbus::fdo::Error::FileNotFound("unknown unit".into()));
            }
            Ok(PATH.try_into().unwrap())
        }
        async fn start_unit(
            &self,
            name: &str,
            mode: &str,
            #[zbus(header)] header: zbus::message::Header<'_>,
            #[zbus(connection)] connection: &zbus::Connection,
        ) -> zbus::zvariant::OwnedObjectPath {
            assert_eq!(name, crate::vm::UNIT);
            assert_eq!(mode, "replace");
            assert!(header
                .primary()
                .flags()
                .contains(zbus::message::Flags::AllowInteractiveAuth));
            self.running.store(true, Ordering::SeqCst);
            self.calls.send("start".into()).unwrap();
            let path: zbus::zvariant::OwnedObjectPath =
                "/org/freedesktop/systemd1/job/1".try_into().unwrap();
            connection
                .emit_signal(
                    None::<&str>,
                    "/org/freedesktop/systemd1",
                    "org.freedesktop.systemd1.Manager",
                    "JobRemoved",
                    &(1u32, &path, name, "done"),
                )
                .await
                .unwrap();
            path
        }
        async fn stop_unit(&self, name: &str, mode: &str) -> zbus::zvariant::OwnedObjectPath {
            assert_eq!(name, crate::vm::UNIT);
            assert_eq!(mode, "replace");
            self.running.store(false, Ordering::SeqCst);
            self.calls.send("stop".into()).unwrap();
            "/org/freedesktop/systemd1/job/2".try_into().unwrap()
        }
    }
    struct MockUnit(Arc<AtomicBool>);
    #[zbus::interface(name = "org.freedesktop.systemd1.Unit")]
    impl MockUnit {
        #[zbus(property)]
        fn load_state(&self) -> &str {
            "loaded"
        }
        #[zbus(property)]
        fn active_state(&self) -> &str {
            if self.0.load(Ordering::SeqCst) {
                "active"
            } else {
                "inactive"
            }
        }
        #[zbus(property)]
        fn sub_state(&self) -> &str {
            if self.0.load(Ordering::SeqCst) {
                "running"
            } else {
                "dead"
            }
        }
        #[zbus(property)]
        fn job(&self) -> (u32, zbus::zvariant::OwnedObjectPath) {
            (0, "/".try_into().unwrap())
        }
    }
    pub async fn manager() -> (
        Manager,
        zbus::Connection,
        tokio::sync::mpsc::UnboundedReceiver<String>,
    ) {
        let (a, b) = tokio::net::UnixStream::pair().unwrap();
        let (calls, receive) = tokio::sync::mpsc::unbounded_channel();
        let running = Arc::new(AtomicBool::new(false));
        let server = zbus::connection::Builder::unix_stream(a)
            .server(zbus::Guid::generate())
            .unwrap()
            .p2p()
            .serve_at(
                "/org/freedesktop/systemd1",
                MockManager {
                    running: running.clone(),
                    calls,
                },
            )
            .unwrap()
            .serve_at(PATH, MockUnit(running))
            .unwrap()
            .build();
        let client = zbus::connection::Builder::unix_stream(b).p2p().build();
        let (server, client) = tokio::join!(server, client);
        (
            Manager {
                scope: UnitScope::System,
                connection: client.unwrap(),
            },
            server.unwrap(),
            receive,
        )
    }
    #[tokio::test]
    async fn typed_systemd_properties_and_interactive_jobs() {
        let (manager, _server, mut calls) = manager().await;
        let unit = manager.unit(crate::vm::UNIT).await.unwrap();
        assert_eq!(unit.active_state, "inactive");
        assert_eq!(unit.job, None);
        assert_eq!(
            manager
                .operate(crate::vm::UNIT, UnitOperation::Start)
                .await
                .unwrap(),
            "/org/freedesktop/systemd1/job/1"
        );
        assert_eq!(calls.recv().await.unwrap(), "start");
        assert_eq!(
            manager.unit(crate::vm::UNIT).await.unwrap().active_state,
            "active"
        );
    }
}

#[cfg(test)]
mod adversarial_review {
    use super::*;
    #[tokio::test]
    async fn review_sys_failed_watch_does_not_poison_existing_units() {
        let (manager, _server, _calls) = mock::manager().await;
        let mut native = Native {
            buses: BusPool::default(),
            state: State::default(),
            managers: vec![manager],
            events: SelectAll::new(),
            watched: vec![],
        };
        native
            .action(
                1,
                Action::Watch {
                    scope: UnitScope::System,
                    name: crate::vm::UNIT.into(),
                },
            )
            .await
            .unwrap();
        assert!(native
            .action(
                2,
                Action::Watch {
                    scope: UnitScope::System,
                    name: "missing.service".into()
                }
            )
            .await
            .is_err());
        assert!(
            native.refresh().await.is_ok(),
            "failed Watch remains in watched, so every later refresh/connect fails"
        );
    }
}

#[cfg(test)]
mod adversarial_signal_review {
    use super::*;
    #[tokio::test]
    async fn review_sys_unpolled_signals_do_not_block_snapshot_replies() {
        let (manager, server, _calls) = mock::manager().await;
        manager.unit(crate::vm::UNIT).await.unwrap();
        let mut signals = manager.signals().await.unwrap();
        for _ in 0..300 {
            server
                .emit_signal(
                    None::<&str>,
                    "/org/freedesktop/systemd1/unit/other",
                    "org.freedesktop.DBus.Properties",
                    "PropertiesChanged",
                    &(
                        "org.freedesktop.systemd1.Unit",
                        std::collections::HashMap::<String, zbus::zvariant::OwnedValue>::new(),
                        Vec::<String>::new(),
                    ),
                )
                .await
                .unwrap();
        }
        for _ in 0..50 {
            tokio::task::yield_now().await;
        }
        let reply = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            manager.unit(crate::vm::UNIT),
        )
        .await;
        let stalled = reply.is_err();
        let drainer = tokio::spawn(async move { while signals.next().await.is_some() {} });
        let recovered = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            manager.unit(crate::vm::UNIT),
        )
        .await
        .is_ok();
        drainer.abort();
        let _ = drainer.await;
        println!("unpolled stalled={stalled}; independent draining restored replies={recovered}");
        assert!(recovered, "control: draining did not restore replies");
        assert!(!stalled,"unpolled 256-entry provider stream blocked the shared connection's socket reader and the snapshot reply");
    }
}

#[cfg(test)]
mod durable_signal_tests {
    use super::*;
    #[tokio::test]
    async fn property_flood_retains_job_completion_without_blocking_rpc() {
        let (manager, server, mut calls) = mock::manager().await;
        let mut events = manager.signals_for_unit(crate::vm::UNIT).await.unwrap();
        let _another = manager.signals().await.unwrap();
        for _ in 0..600 {
            server
                .emit_signal(
                    None::<&str>,
                    "/org/freedesktop/systemd1/unit/other",
                    "org.freedesktop.DBus.Properties",
                    "PropertiesChanged",
                    &(
                        "org.freedesktop.systemd1.Unit",
                        std::collections::HashMap::<String, zbus::zvariant::OwnedValue>::new(),
                        Vec::<String>::new(),
                    ),
                )
                .await
                .unwrap();
        }
        let job = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            manager.operate(crate::vm::UNIT, UnitOperation::Start),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(calls.recv().await.unwrap(), "start");
        let unit = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            manager.unit(crate::vm::UNIT),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(unit.active_state, "active");
        let path = tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while let Some(event) = events.next().await {
                let message = event.unwrap();
                if message
                    .header()
                    .member()
                    .is_some_and(|member| member.as_str() == "JobRemoved")
                {
                    let (_, path, name, result): (
                        u32,
                        zbus::zvariant::OwnedObjectPath,
                        String,
                        String,
                    ) = message.body().deserialize().unwrap();
                    assert_eq!(name, crate::vm::UNIT);
                    assert_eq!(result, "done");
                    return path.to_string();
                }
            }
            panic!("completion stream closed")
        })
        .await
        .unwrap();
        assert_eq!(path, job);
    }
}

#[cfg(test)]
mod idle_subscription_tests {
    use super::*;
    #[tokio::test]
    async fn unit_stream_ignores_other_units_on_shared_connection() {
        let (manager, server, _) = mock::manager().await;
        let _broad = manager.signals().await.unwrap();
        let mut narrow = manager.signals_for_unit(crate::vm::UNIT).await.unwrap();
        let body = (
            "org.freedesktop.systemd1.Unit",
            std::collections::HashMap::<String, zbus::zvariant::OwnedValue>::new(),
            Vec::<String>::new(),
        );
        server
            .emit_signal(
                None::<&str>,
                "/org/freedesktop/systemd1/unit/other",
                "org.freedesktop.DBus.Properties",
                "PropertiesChanged",
                &body,
            )
            .await
            .unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(30), narrow.next())
                .await
                .is_err()
        );
        let path = manager
            .proxy()
            .await
            .unwrap()
            .call::<_, _, zbus::zvariant::OwnedObjectPath>("LoadUnit", &(crate::vm::UNIT,))
            .await
            .unwrap();
        server
            .emit_signal(
                None::<&str>,
                path,
                "org.freedesktop.DBus.Properties",
                "PropertiesChanged",
                &body,
            )
            .await
            .unwrap();
        let m = tokio::time::timeout(std::time::Duration::from_secs(1), narrow.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(m.header().member().unwrap().as_str(), "PropertiesChanged");
    }
}
