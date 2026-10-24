use futures_util::StreamExt;
use hyprv_core::{
    runtime::ServiceRuntime,
    service::{run_sampled, Service, ServiceFuture},
    store::{StateStore, Subscription, Topic, TopicKey},
};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::{mpsc, oneshot, watch};
pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone)]
pub struct Paths {
    pub sys: PathBuf,
    pub proc: PathBuf,
    pub state: PathBuf,
    pub runtime: PathBuf,
}
impl Default for Paths {
    fn default() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/home/wilf"));
        Self {
            sys: hyprv_core::profile::sys_path("sys"),
            proc: "/proc".into(),
            state: std::env::var_os("XDG_STATE_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".local/state"))
                .join("hyprv"),
            runtime: std::env::var_os("XDG_RUNTIME_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join("hyprv"),
        }
    }
}
impl Paths {
    /// Explicit fixture contexts and the shared override use the same boundary.
    pub fn isolated_sysfs(&self) -> bool {
        self.sys != std::path::Path::new("/sys")
    }
    pub fn guard_sysfs(&self, path: impl AsRef<std::path::Path>) -> Result<PathBuf> {
        confined_sysfs(&self.sys, path.as_ref())
    }
}
/// Native class links may resolve into devices/, but never outside an injected
/// tree. Missing/denied/broken fixture paths are errors, never host fallbacks.
pub(crate) fn confined_sysfs(root: &std::path::Path, path: &std::path::Path) -> Result<PathBuf> {
    if root == std::path::Path::new("/sys") {
        return Ok(path.to_path_buf());
    }
    let canonical = path.canonicalize()?;
    if !canonical.starts_with(root.canonicalize()?) {
        return Err("sysfs fixture symlink escapes selected root".into());
    }
    Ok(canonical)
}
pub(crate) fn sysfs_entries(
    root: &std::path::Path,
    path: impl AsRef<std::path::Path>,
) -> Vec<PathBuf> {
    let path = path.as_ref();
    if confined_sysfs(root, path).is_err() {
        return Vec::new();
    }
    entries(path)
        .into_iter()
        .filter(|p| confined_sysfs(root, p).is_ok())
        .collect()
}
pub(crate) fn sysfs_text(
    root: &std::path::Path,
    path: impl AsRef<std::path::Path>,
) -> std::io::Result<String> {
    let path =
        confined_sysfs(root, path.as_ref()).map_err(|e| std::io::Error::other(e.to_string()))?;
    std::fs::read_to_string(path)
}
#[derive(Clone)]
pub struct Context {
    pub runtime: std::sync::Weak<ServiceRuntime>,
    pub shutdown: watch::Receiver<bool>,
    pub bus: zbus::Connection,
    pub paths: Paths,
    pub store: Arc<StateStore>,
    pub sleeping: watch::Sender<bool>,
}
impl Context {
    pub async fn system(runtime: Arc<ServiceRuntime>) -> Result<Self> {
        let (sleeping, _) = watch::channel(false);
        Ok(Self {
            shutdown: runtime.shutdown_signal(),
            runtime: Arc::downgrade(&runtime),
            bus: zbus::Connection::system().await?,
            paths: Paths::default(),
            store: Arc::new(StateStore::default()),
            sleeping,
        })
    }
}
pub struct Command<A> {
    pub action: A,
    pub reply: oneshot::Sender<Result<()>>,
}
pub(crate) type ActionObserver<A> = Arc<dyn Fn(&A) + Send + Sync>;
pub struct Handle<S: Clone, A> {
    pub state: Topic<S>,
    pub error: Topic<Option<String>>,
    pub(crate) commands: mpsc::Sender<Command<A>>,
    pub(crate) action_observer: Option<ActionObserver<A>>,
    pub(crate) period: watch::Sender<Duration>,
}
impl<S: Clone, A> Clone for Handle<S, A> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            error: self.error.clone(),
            commands: self.commands.clone(),
            action_observer: self.action_observer.clone(),
            period: self.period.clone(),
        }
    }
}
impl<S: Clone + PartialEq, A> Handle<S, A> {
    /// Core's subscription is the sampling lease; dropping it releases demand.
    pub fn subscribe(&self) -> Subscription<S> {
        self.state.subscribe()
    }
    pub async fn action(&self, action: A) -> Result<()> {
        if let Some(observer) = &self.action_observer {
            observer(&action);
        }
        let (reply, rx) = oneshot::channel();
        self.commands
            .send(Command { action, reply })
            .await
            .map_err(|_| "actor stopped")?;
        rx.await.map_err(|_| "actor stopped")?
    }
    pub fn set_period(&self, period: Duration) -> Result<()> {
        if period < Duration::from_millis(100) || period > Duration::from_secs(3600) {
            return Err("sampling period must be 100ms..3600s".into());
        }
        self.period.send_replace(period);
        Ok(())
    }
}
pub trait Backend: Send + 'static {
    type State: Clone + PartialEq + Default + Send + Sync + 'static;
    type Action: Send + 'static;
    fn connect(&mut self) -> impl std::future::Future<Output = Result<()>> + Send;
    fn snapshot(&mut self) -> impl std::future::Future<Output = Result<Self::State>> + Send;
    fn apply(
        &mut self,
        action: Self::Action,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
    fn event(&mut self) -> impl std::future::Future<Output = Result<()>> + Send;
    fn reset(&mut self) {}
    /// Opt-in: no provider connection/inbox/snapshot while observation is hidden.
    /// Explicit actions remain available. Reset must drop observation resources.
    fn observe_on_demand(&self) -> bool {
        false
    }
    fn unobserve(&mut self) {
        self.reset();
    }
    fn initial_snapshot(&self) -> bool {
        true
    }
    fn event_samples(&self) -> bool {
        true
    }
    fn bind(&mut self, _period: watch::Sender<Duration>) {}
    fn demand_changed(&mut self, _count: usize) {}
    fn providers(&self) -> &'static [&'static str] {
        &[]
    }
}
struct Key<B>(std::marker::PhantomData<B>);
impl<B: Backend> TopicKey for Key<B> {
    type Value = B::State;
}
struct Sampler {
    tx: mpsc::Sender<()>,
    period: watch::Receiver<Duration>,
    sleeping: watch::Receiver<bool>,
    task: Option<tokio::task::JoinHandle<()>>,
}
impl Service for Sampler {
    fn start(&mut self) -> ServiceFuture<'_> {
        Box::pin(async {
            let tx = self.tx.clone();
            let mut period = self.period.clone();
            let mut sleeping = self.sleeping.clone();
            self.task = Some(tokio::spawn(async move {
                loop {
                    if *sleeping.borrow() {
                        if sleeping.changed().await.is_err() {
                            break;
                        }
                        continue;
                    }
                    let _ = tx.try_send(());
                    let duration = *period.borrow_and_update();
                    tokio::select! { _=tokio::time::sleep(duration)=>(), r=period.changed()=>if r.is_err(){break;},r=sleeping.changed()=>if r.is_err(){break;} }
                }
            }));
            Ok(())
        })
    }
    fn stop(&mut self) -> ServiceFuture<'_> {
        Box::pin(async {
            if let Some(task) = self.task.take() {
                task.abort();
                let _ = task.await;
            }
            Ok(())
        })
    }
}
impl Drop for Sampler {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}
pub fn spawn<B: Backend>(ctx: Context, backend: B, sampled: bool) -> Handle<B::State, B::Action> {
    spawn_controlled(ctx, backend, sampled, None)
}
/// Explicit sampling authority for metric leases; passive topic observers do not
/// participate when an override is supplied.
pub(crate) fn spawn_controlled<B: Backend>(
    ctx: Context,
    mut backend: B,
    sampled: bool,
    authority: Option<watch::Receiver<usize>>,
) -> Handle<B::State, B::Action> {
    let runtime = ctx
        .runtime
        .upgrade()
        .expect("service runtime owner was dropped");
    let state = ctx.store.topic::<Key<B>>(B::State::default());
    let error = Topic::new(None);
    let (commands, mut receive) = mpsc::channel::<Command<B::Action>>(32);
    let activation = authority.unwrap_or_else(|| state.demand());
    let (tick, mut ticks) = mpsc::channel(1);
    let (period, period_rx) = watch::channel(Duration::from_secs(1));
    if sampled {
        let sampler = Sampler {
            tx: tick.clone(),
            period: period_rx,
            sleeping: ctx.sleeping.subscribe(),
            task: None,
        };
        let shutdown = ctx.shutdown.clone();
        let demand = activation.clone();
        runtime.spawn(async move {
            let _ = run_sampled(sampler, demand, shutdown).await;
        });
    }
    backend.bind(period.clone());
    let public = Handle {
        state: state.clone(),
        error: error.clone(),
        commands,
        action_observer: None,
        period,
    };
    let mut shutdown = ctx.shutdown.clone();
    let mut sleeping = ctx.sleeping.subscribe();
    runtime.spawn(async move {
        let _keep_tick = tick;
        let mut ready = false;
        let mut commands_open = true;
        let mut demand = activation;
        let count = *demand.borrow_and_update();
        backend.demand_changed(count);
        let mut demand_active = count > 0;
        let mut owners = None;
        let mut retry = Duration::from_millis(250);
        let mut cooldown = None;
        let mut connected_at = tokio::time::Instant::now();
        'actor: loop {
            if *shutdown.borrow() { break; }
            if backend.observe_on_demand() && (!demand_active || *sleeping.borrow()) {
                if ready { backend.unobserve(); ready = false; }
                owners = None;
                cooldown = None;
                tokio::select! {
                    _ = shutdown.changed() => break,
                    r = demand.changed() => {
                        if r.is_err() { break; }
                        let count = *demand.borrow_and_update();
                        backend.demand_changed(count);
                        demand_active = count > 0;
                    },
                    r = sleeping.changed() => if r.is_err() { break; },
                    c = receive.recv(), if commands_open => match c {
                        Some(c) => {
                            let result = tokio::time::timeout(Duration::from_secs(15), backend.apply(c.action)).await
                                .unwrap_or_else(|_| Err("action timed out; hardware may have changed".into()));
                            let mut failure = result.as_ref().err().map(ToString::to_string);
                            if result.is_ok() && (!sampled || demand_active && !*sleeping.borrow()) {
                                match backend.snapshot().await {
                                    Ok(next) => { state.publish(next); },
                                    Err(e) => { failure=Some(e.to_string()); },
                                }
                            }
                            error.publish(failure);
                            let _ = c.reply.send(result);
                        },
                        None => commands_open = false,
                    },
                }
                continue;
            }
            if let Some(deadline) = cooldown.take() {
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep_until(deadline) => break,
                        _ = shutdown.changed() => return,
                        c=receive.recv(),if commands_open=>match c {
                            Some(c)=>{let _=c.reply.send(Err("provider unavailable".into()));},
                            None=>commands_open=false,
                        },
                        r = demand.changed(), if sampled || backend.observe_on_demand() => {
                            if r.is_err() { return; }
                            let count = *demand.borrow_and_update();
                            backend.demand_changed(count);
                            demand_active = count > 0;
                            cooldown = Some(deadline);
                            continue 'actor;
                        },
                    }
                }
            }
            if !ready {
                if owners.is_none() && !backend.providers().is_empty() {
                    owners = owner_signals(&ctx.bus, backend.providers()).await.ok();
                }
                match backend.connect().await {
                    Ok(()) => { ready = true; connected_at = tokio::time::Instant::now(); },
                    Err(e) => {
                        state.publish(B::State::default()); error.publish(Some(e.to_string()));
                        cooldown = Some(tokio::time::Instant::now() + retry);
                        retry = (retry * 2).min(Duration::from_secs(30)); continue;
                    },
                }
                if backend.initial_snapshot() && (!sampled || demand_active) {
                    match backend.snapshot().await {
                        Ok(next) => { state.publish(next); error.publish(None); },
                        Err(e) => { error.publish(Some(e.to_string())); },
                    }
                }
            }
            let mut action_error = None;
            let provider_names = backend.providers();
            let refresh = tokio::select! {
                biased;
                _ = shutdown.changed() => break,
                c = receive.recv(), if commands_open => match c {
                    Some(c) => {
                        let result = tokio::time::timeout(Duration::from_secs(15), backend.apply(c.action)).await
                            .unwrap_or_else(|_| Err("action timed out; hardware may have changed".into()));
                        action_error = result.as_ref().err().map(ToString::to_string);
                        if action_error.is_some() { error.publish(action_error.clone()); }
                        let _ = c.reply.send(result); true
                    },
                    None => { commands_open = false; false },
                },
                r = sleeping.changed() => {
                    if r.is_err() { break; }
                    if sampled || backend.observe_on_demand() { backend.reset(); }
                    true
                },
                r = demand.changed(), if sampled || backend.observe_on_demand() => {
                    if r.is_err() { break; }
                    let count = *demand.borrow_and_update(); backend.demand_changed(count);
                    let active = count > 0;
                    if active != demand_active { backend.reset(); demand_active = active; }
                    false
                },
                r = owner_change(&mut owners, provider_names) => {
                    if r.is_err() {
                        owners = None; cooldown = Some(tokio::time::Instant::now() + retry);
                        retry = (retry * 2).min(Duration::from_secs(30));
                    }
                    backend.reset(); ready = false; false
                },
                _ = ticks.recv(), if sampled => !*sleeping.borrow() && demand_active,
                result = backend.event() => match result {
                    Ok(()) => {
                        if connected_at.elapsed() >= Duration::from_secs(30) { retry = Duration::from_millis(250); }
                        backend.event_samples()
                    },
                    Err(e) => {
                        error.publish(Some(e.to_string())); state.publish(B::State::default());
                        backend.reset(); ready = false;
                        cooldown = Some(tokio::time::Instant::now() + retry);
                        retry = (retry * 2).min(Duration::from_secs(30)); false
                    },
                },
            };
            if refresh && (!sampled || demand_active) && (!sampled || !*sleeping.borrow())
                && (!backend.observe_on_demand() || !*sleeping.borrow()) {
                match backend.snapshot().await {
                    Ok(next) => { state.publish(next); error.publish(action_error); },
                    Err(e) => { error.publish(Some(e.to_string())); state.publish(B::State::default()); backend.reset(); },
                }
            }
        }
    });
    public
}
/// The dedicated reader never waits for a consumer/RPC. Overflow is explicit:
/// callers reconnect and reconcile rather than silently retaining stale state.
pub struct SignalInbox {
    rx: mpsc::Receiver<zbus::Result<zbus::Message>>,
    overflow: Arc<std::sync::atomic::AtomicBool>,
    reader: tokio::task::JoinHandle<()>,
}
impl futures_util::Stream for SignalInbox {
    type Item = zbus::Result<zbus::Message>;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        if self
            .overflow
            .swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            return std::task::Poll::Ready(Some(Err(zbus::Error::Failure(
                "provider signal overflow; resnapshot required".into(),
            ))));
        }
        self.rx.poll_recv(cx)
    }
}
impl Drop for SignalInbox {
    fn drop(&mut self) {
        self.reader.abort();
    }
}
pub async fn signals(bus: &zbus::Connection, sender: &str) -> Result<SignalInbox> {
    signals_filtered(bus, sender, None).await
}
/// Broker-side filtering avoids waking a dormant sensor client for unrelated
/// fan/controller signals on the same provider connection.
pub async fn signals_member(
    bus: &zbus::Connection,
    sender: &str,
    path: &str,
    interface: &str,
    member: &str,
) -> Result<SignalInbox> {
    signals_filtered(bus, sender, Some((path, interface, member))).await
}
async fn signals_filtered(
    bus: &zbus::Connection,
    sender: &str,
    filter: Option<(&str, &str, &str)>,
) -> Result<SignalInbox> {
    // MessageStream's local matcher cannot resolve well-known senders. Other
    // match rules on this shared connection would leak their traffic here.
    // Owner watches are installed before connect; replacement triggers a fresh
    // resolution/snapshot. An absent provider waits for that owner event.
    let sender = if bus.is_bus() {
        match zbus::fdo::DBusProxy::new(bus)
            .await?
            .get_name_owner(sender.try_into()?)
            .await
        {
            Ok(owner) => owner.to_string(),
            Err(zbus::fdo::Error::NameHasNoOwner(_)) => {
                return Ok(signal_inbox(futures_util::stream::pending()));
            }
            Err(error) => return Err(error.into()),
        }
    } else {
        sender.to_owned()
    };
    let mut rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(sender.as_str())?;
    if let Some((path, interface, member)) = filter {
        rule = rule.path(path)?.interface(interface)?.member(member)?;
    }
    let rule = rule.build();
    let stream = zbus::MessageStream::for_match_rule(rule, bus, Some(128)).await?;
    Ok(signal_inbox(stream))
}
async fn owner_signals(bus: &zbus::Connection, names: &[&str]) -> Result<SignalInbox> {
    let mut streams = futures_util::stream::SelectAll::new();
    for name in names {
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender("org.freedesktop.DBus")?
            .path("/org/freedesktop/DBus")?
            .interface("org.freedesktop.DBus")?
            .member("NameOwnerChanged")?
            .arg(0, *name)?
            .build();
        streams.push(zbus::MessageStream::for_match_rule(rule, bus, Some(128)).await?);
    }
    Ok(signal_inbox(streams))
}
fn signal_inbox(
    mut stream: impl futures_util::Stream<Item = zbus::Result<zbus::Message>> + Send + Unpin + 'static,
) -> SignalInbox {
    let (tx, rx) = mpsc::channel(128);
    let overflow = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = overflow.clone();
    let reader = tokio::spawn(async move {
        while let Some(message) = stream.next().await {
            match tx.try_send(message) {
                Ok(()) => (),
                Err(mpsc::error::TrySendError::Full(_)) => {
                    flag.store(true, std::sync::atomic::Ordering::Release);
                }
                Err(mpsc::error::TrySendError::Closed(_)) => break,
            }
        }
    });
    SignalInbox {
        rx,
        overflow,
        reader,
    }
}
pub async fn next(stream: &mut Option<SignalInbox>) -> Result<()> {
    match stream.as_mut() {
        Some(s) => match s.next().await {
            Some(Ok(_)) => Ok(()),
            Some(Err(e)) => Err(e.into()),
            None => Err("signal stream ended".into()),
        },
        None => std::future::pending().await,
    }
}
pub async fn has_owner(bus: &zbus::Connection, name: &str) -> Result<bool> {
    if !bus.is_bus() {
        return Ok(true);
    }
    Ok(zbus::fdo::DBusProxy::new(bus)
        .await?
        .name_has_owner(name.try_into()?)
        .await?)
}
pub async fn proxy<'a>(
    bus: &'a zbus::Connection,
    destination: &'a str,
    path: &'a str,
    interface: &'a str,
) -> Result<zbus::Proxy<'a>> {
    Ok(zbus::Proxy::new(bus, destination, path, interface).await?)
}
pub fn read_number(path: impl AsRef<std::path::Path>) -> Option<f64> {
    std::fs::read_to_string(path)
        .ok()?
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|x| x.is_finite())
}
pub fn entries(path: impl AsRef<std::path::Path>) -> Vec<PathBuf> {
    let mut p: Vec<_> = std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|x| x.path())
        .collect();
    p.sort();
    p
}
pub fn atomic(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let parent = path.parent().ok_or("no parent")?;
    std::fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".{}-{}.tmp",
        path.file_name().ok_or("no name")?.to_string_lossy(),
        std::process::id()
    ));
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}

async fn owner_change(stream: &mut Option<SignalInbox>, names: &[&str]) -> Result<()> {
    if names.is_empty() {
        return std::future::pending().await;
    }
    loop {
        let Some(stream) = stream.as_mut() else {
            return std::future::pending().await;
        };
        let message = stream.next().await.ok_or("bus disconnected")??;
        if message
            .header()
            .member()
            .is_some_and(|m| m.as_str() == "NameOwnerChanged")
        {
            let (name, _, _): (String, String, String) = message.body().deserialize()?;
            if names.contains(&name.as_str()) {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(start_paused = true)]
    async fn no_sampling_without_core_subscription_and_sleep_stops_samples() {
        let topic = Topic::new(0u8);
        let (tx, mut rx) = mpsc::channel(1);
        let (period, period_rx) = watch::channel(Duration::from_secs(1));
        let (sleeping, sleep_rx) = watch::channel(false);
        let (shutdown, stop) = watch::channel(false);
        let sampler = Sampler {
            tx,
            period: period_rx,
            sleeping: sleep_rx,
            task: None,
        };
        let task = tokio::spawn(run_sampled(sampler, topic.demand(), stop));
        tokio::time::advance(Duration::from_secs(3600)).await;
        assert!(rx.try_recv().is_err());
        let lease = topic.subscribe();
        rx.recv().await.unwrap();
        sleeping.send_replace(true);
        tokio::task::yield_now().await;
        while rx.try_recv().is_ok() {}
        tokio::time::advance(Duration::from_secs(3600)).await;
        assert!(rx.try_recv().is_err());
        sleeping.send_replace(false);
        rx.recv().await.unwrap();
        drop(lease);
        tokio::task::yield_now().await;
        while rx.try_recv().is_ok() {}
        tokio::time::advance(Duration::from_secs(3600)).await;
        assert!(rx.try_recv().is_err());
        shutdown.send_replace(true);
        task.await.unwrap().unwrap();
        drop(period);
    }
}

#[cfg(test)]
mod adversarial_review {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Broken {
        count: Arc<AtomicUsize>,
        started: Option<std::sync::mpsc::Sender<()>>,
    }
    impl Backend for Broken {
        type State = u8;
        type Action = ();
        async fn connect(&mut self) -> Result<()> {
            Ok(())
        }
        async fn snapshot(&mut self) -> Result<u8> {
            Ok(1)
        }
        async fn apply(&mut self, _: ()) -> Result<()> {
            Ok(())
        }
        async fn event(&mut self) -> Result<()> {
            let n = self.count.fetch_add(1, Ordering::SeqCst);
            if let Some(started) = self.started.take() {
                let _ = started.send(());
            }
            if n >= 1000 {
                std::future::pending::<()>().await;
            }
            Err("provider disconnected".into())
        }
    }
    #[test]
    fn review_power_event_failure_has_reconnect_backoff() {
        let rt = Arc::new(ServiceRuntime::new().unwrap());
        let bus = rt.block_on(async {
            let (a, b) = tokio::net::UnixStream::pair().unwrap();
            let guid = zbus::Guid::generate();
            let (a, b) = tokio::join!(
                zbus::connection::Builder::unix_stream(a)
                    .server(guid)
                    .unwrap()
                    .p2p()
                    .build(),
                zbus::connection::Builder::unix_stream(b).p2p().build()
            );
            (a.unwrap(), b.unwrap())
        });
        let (sleeping, _) = watch::channel(false);
        let ctx = Context {
            runtime: Arc::downgrade(&rt),
            shutdown: rt.shutdown_signal(),
            bus: bus.0,
            paths: Paths::default(),
            store: Arc::new(StateStore::default()),
            sleeping,
        };
        let _keep_sleep_sender = ctx.sleeping.clone();
        let count = Arc::new(AtomicUsize::new(0));
        let (started, ready) = std::sync::mpsc::channel();
        let _handle = spawn(
            ctx,
            Broken {
                count: count.clone(),
                started: Some(started),
            },
            false,
        );
        ready.recv_timeout(Duration::from_secs(2)).unwrap();
        std::thread::sleep(Duration::from_millis(30));
        let attempts = count.load(Ordering::SeqCst);
        rt.request_shutdown();
        drop(bus.1);
        drop(rt);
        assert!(attempts > 0, "backend was never polled");
        assert!(
            attempts <= 2,
            "{attempts} immediate event/connect failures in 30ms"
        );
    }
}
#[cfg(test)]
mod adversarial_signal_review {
    use super::*;
    struct Ping;
    #[zbus::interface(name = "org.hyprv.ReviewPing")]
    impl Ping {
        fn ping(&self) -> u32 {
            7
        }
    }
    #[tokio::test]
    async fn review_power_signal_queue_cannot_block_snapshot_rpc() {
        let (a, b) = tokio::net::UnixStream::pair().unwrap();
        let guid = zbus::Guid::generate();
        let (server, client) = tokio::join!(
            zbus::connection::Builder::unix_stream(a)
                .server(guid)
                .unwrap()
                .p2p()
                .serve_at("/review", Ping)
                .unwrap()
                .build(),
            zbus::connection::Builder::unix_stream(b).p2p().build()
        );
        let (server, client) = (server.unwrap(), client.unwrap());
        let mut stream = signals(&client, "org.freedesktop.UPower").await.unwrap();
        for _ in 0..180 {
            server
                .emit_signal(
                    None::<&str>,
                    "/review",
                    "org.freedesktop.DBus.Properties",
                    "PropertiesChanged",
                    &(),
                )
                .await
                .unwrap();
        }
        for _ in 0..50 {
            tokio::task::yield_now().await;
        }
        let request = || {
            client.call_method(
                None::<&str>,
                "/review",
                Some("org.hyprv.ReviewPing"),
                "Ping",
                &(),
            )
        };
        let stalled = tokio::time::timeout(Duration::from_millis(100), request())
            .await
            .is_err();
        let drainer = tokio::spawn(async move { while stream.next().await.is_some() {} });
        let recovered = matches!(
            tokio::time::timeout(Duration::from_secs(1), request()).await,
            Ok(Ok(_))
        );
        drainer.abort();
        let _ = drainer.await;
        assert!(
            recovered,
            "control: independent signal drainer must restore method replies"
        );
        assert!(
            !stalled,
            "128 undrained provider signals block the shared connection and snapshot response"
        );
    }
}

#[cfg(test)]
mod idle_sender_tests {
    use super::*;
    use std::io::BufRead;

    struct PrivateBus(std::process::Child, String);
    impl PrivateBus {
        fn new() -> Self {
            let mut child = std::process::Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--nopidfile", "--print-address=1"])
                .stdout(std::process::Stdio::piped())
                .spawn()
                .expect("idle signal regressions require dbus-daemon");
            let mut address = String::new();
            std::io::BufReader::new(child.stdout.take().unwrap())
                .read_line(&mut address)
                .unwrap();
            Self(child, address.trim().into())
        }
        async fn connection(&self) -> zbus::Connection {
            zbus::connection::Builder::address(self.1.as_str())
                .unwrap()
                .build()
                .await
                .unwrap()
        }
    }
    impl Drop for PrivateBus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    async fn emit(connection: &zbus::Connection) {
        connection
            .emit_signal(
                None::<&str>,
                "/idle",
                "org.freedesktop.DBus.Properties",
                "PropertiesChanged",
                &(),
            )
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn provider_inbox_ignores_other_rules_on_shared_connection() {
        let bus = PrivateBus::new();
        let client = bus.connection().await;
        let provider = bus.connection().await;
        provider
            .request_name("org.hyprv.IdleProvider")
            .await
            .unwrap();
        let noise = bus.connection().await;
        noise.request_name("org.hyprv.IdleNoise").await.unwrap();
        let mut inbox = signals(&client, "org.hyprv.IdleProvider").await.unwrap();
        // This second rule admits noise onto the same socket. A well-known
        // sender-only local match erroneously gives that noise to both inboxes.
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender("org.hyprv.IdleNoise")
            .unwrap()
            .build();
        let _noise_stream = zbus::MessageStream::for_match_rule(rule, &client, Some(128))
            .await
            .unwrap();
        for _ in 0..8 {
            emit(&noise).await;
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(40), inbox.next())
                .await
                .is_err(),
            "unrelated sender triggered a provider refresh"
        );
        emit(&provider).await;
        let message = tokio::time::timeout(Duration::from_secs(1), inbox.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(
            message.header().sender(),
            provider.unique_name().map(|v| &**v)
        );
    }
    #[tokio::test]
    async fn owner_watch_ignores_unrelated_names_and_recovers_absent_provider() {
        let bus = PrivateBus::new();
        let client = bus.connection().await;
        let mut owners = Some(
            owner_signals(&client, &["org.hyprv.IdleProvider"])
                .await
                .unwrap(),
        );
        let mut absent = signals(&client, "org.hyprv.IdleProvider").await.unwrap();
        let noise = bus.connection().await;
        noise.request_name("org.hyprv.IdleNoise").await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(40), owners.as_mut().unwrap().next())
                .await
                .is_err()
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(40), absent.next())
                .await
                .is_err(),
            "absent provider must wait, not report EOF/retry"
        );
        let provider = bus.connection().await;
        provider
            .request_name("org.hyprv.IdleProvider")
            .await
            .unwrap();
        tokio::time::timeout(
            Duration::from_secs(1),
            owner_change(&mut owners, &["org.hyprv.IdleProvider"]),
        )
        .await
        .unwrap()
        .unwrap();
        let mut current = signals(&client, "org.hyprv.IdleProvider").await.unwrap();
        emit(&provider).await;
        tokio::time::timeout(Duration::from_secs(1), current.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        provider
            .release_name("org.hyprv.IdleProvider")
            .await
            .unwrap();
        tokio::time::timeout(
            Duration::from_secs(1),
            owner_change(&mut owners, &["org.hyprv.IdleProvider"]),
        )
        .await
        .unwrap()
        .unwrap();
        let replacement = bus.connection().await;
        replacement
            .request_name("org.hyprv.IdleProvider")
            .await
            .unwrap();
        tokio::time::timeout(
            Duration::from_secs(1),
            owner_change(&mut owners, &["org.hyprv.IdleProvider"]),
        )
        .await
        .unwrap()
        .unwrap();
        let mut current = signals(&client, "org.hyprv.IdleProvider").await.unwrap();
        emit(&replacement).await;
        let message = tokio::time::timeout(Duration::from_secs(1), current.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(
            message.header().sender(),
            replacement.unique_name().map(|v| &**v)
        );
    }
    #[tokio::test]
    async fn fan_and_power_members_are_isolated_even_with_broad_shared_listener() {
        let bus = PrivateBus::new();
        let client = bus.connection().await;
        let provider = bus.connection().await;
        provider
            .request_name("org.hyprv.MemberIsolation")
            .await
            .unwrap();
        let mut broad = signals(&client, "org.hyprv.MemberIsolation").await.unwrap();
        let mut fan = signals_member(
            &client,
            "org.hyprv.MemberIsolation",
            "/org/hyprv/Shell/PrivHelper",
            "org.hyprv.Shell.PrivHelper1",
            "FanStateChanged",
        )
        .await
        .unwrap();
        let mut power = signals_member(
            &client,
            "org.hyprv.MemberIsolation",
            "/org/hyprv/Shell/PrivHelper",
            "org.hyprv.Shell.PrivHelper1",
            "CpuPowerChanged",
        )
        .await
        .unwrap();
        provider
            .emit_signal(
                None::<&str>,
                "/org/hyprv/Shell/PrivHelper",
                "org.hyprv.Shell.PrivHelper1",
                "CpuPowerChanged",
                &("watts",),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), power.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        broad.next().await.unwrap().unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(40), fan.next())
                .await
                .is_err(),
            "power signals must not trigger fan RPCs"
        );
        provider
            .emit_signal(
                None::<&str>,
                "/org/hyprv/Shell/PrivHelper",
                "org.hyprv.Shell.PrivHelper1",
                "FanStateChanged",
                &("fans",),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), fan.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        broad.next().await.unwrap().unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(40), power.next())
                .await
                .is_err(),
            "fan signals must not wake power clients"
        );
    }
}

#[cfg(test)]
mod fixture_boundary_tests {
    use super::*;
    #[test]
    fn injected_sysfs_rejects_escaped_and_missing_paths_but_accepts_internal_class_links() {
        let tree = crate::test_support::Tree::new();
        tree.write("sys/devices/fake/temp", "42000");
        tree.write("outside/temp", "99000");
        std::fs::create_dir_all(tree.0.join("sys/class/thermal")).unwrap();
        std::os::unix::fs::symlink("../../devices/fake", tree.0.join("sys/class/thermal/good"))
            .unwrap();
        std::os::unix::fs::symlink(
            tree.0.join("outside"),
            tree.0.join("sys/class/thermal/escape"),
        )
        .unwrap();
        let paths = tree.paths();
        assert_eq!(
            sysfs_text(&paths.sys, paths.sys.join("class/thermal/good/temp")).unwrap(),
            "42000"
        );
        assert!(sysfs_text(&paths.sys, paths.sys.join("class/thermal/escape/temp")).is_err());
        assert!(sysfs_text(&paths.sys, paths.sys.join("missing")).is_err());
        assert_eq!(
            sysfs_entries(&paths.sys, paths.sys.join("class/thermal")).len(),
            1
        );
    }
}
