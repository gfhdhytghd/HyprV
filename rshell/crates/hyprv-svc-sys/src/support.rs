use futures_util::StreamExt;
use hyprv_core::{
    runtime::ServiceRuntime,
    store::{StateStore, Topic, TopicKey},
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
};
use tokio_util::codec::{FramedRead, LinesCodec};
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Error {
    pub code: String,
    pub message: String,
}
impl Error {
    pub fn new(code: &str, message: impl ToString) -> Self {
        Self {
            code: code.into(),
            message: message.to_string(),
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::new("io", e)
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::new("json", e)
    }
}
impl From<zbus::Error> for Error {
    fn from(e: zbus::Error) -> Self {
        Self::new("dbus", e)
    }
}
#[derive(Clone)]
pub struct Context {
    pub runtime: Arc<ServiceRuntime>,
    pub store: Arc<StateStore>,
}
pub struct Envelope<A> {
    pub id: u64,
    pub action: A,
    pub reply: oneshot::Sender<Result<serde_json::Value>>,
}
pub struct Handle<S: Clone, A> {
    pub state: Topic<S>,
    sender: mpsc::Sender<Envelope<A>>,
    ids: AtomicU64,
    pub task: JoinHandle<()>,
    stop: watch::Sender<bool>,
    stopped: bool,
}
impl<S: Clone + PartialEq, A> Handle<S, A> {
    pub async fn dispatch(&self, action: A) -> Result<serde_json::Value> {
        let (reply, receive) = oneshot::channel();
        self.sender
            .send(Envelope {
                id: self.ids.fetch_add(1, Ordering::Relaxed),
                action,
                reply,
            })
            .await
            .map_err(|_| Error::new("stopped", "service stopped"))?;
        receive
            .await
            .map_err(|_| Error::new("stopped", "service stopped"))?
    }
}
/// spawn() starts the actor; Service::start validates that it is still running.
/// stop() cancels IO and releases watches, streams and timers, and joins the actor.
impl<S, A> hyprv_core::service::Service for Handle<S, A>
where
    S: Clone + PartialEq + Send + Sync + 'static,
    A: Send + 'static,
{
    fn start(&mut self) -> hyprv_core::service::ServiceFuture<'_> {
        Box::pin(async {
            if self.stopped || self.task.is_finished() {
                Err(
                    Box::new(Error::new("stopped", "spawn a new service after stop"))
                        as Box<dyn std::error::Error + Send + Sync>,
                )
            } else {
                Ok(())
            }
        })
    }
    fn stop(&mut self) -> hyprv_core::service::ServiceFuture<'_> {
        Box::pin(async {
            if !self.stopped {
                self.stop.send_replace(true);
                (&mut self.task).await?;
                self.stopped = true;
            }
            Ok(())
        })
    }
}
pub trait Model: Clone + PartialEq + Send + Sync + 'static {
    fn failure(&mut self, error: Error);
    fn action_failure(&mut self, _error: Error) {}
}
#[async_trait::async_trait]
pub trait Backend: Send + 'static {
    type State: Model;
    type Action: Send + 'static;
    fn state(&self) -> &Self::State;
    fn state_mut(&mut self) -> &mut Self::State;
    /// False when only an explicit configuration action can make connection possible.
    fn reconnect_on_timer(&self) -> bool { true }
    /// An external provider/discovery event can interrupt offline backoff.
    async fn reconnect_event(&mut self) -> Result<()> { std::future::pending().await }
    async fn connect(&mut self) -> Result<()>;
    async fn next(&mut self) -> Result<()>;
    async fn action(&mut self, id: u64, action: Self::Action) -> Result<serde_json::Value>;
    async fn offline_action(
        &mut self,
        _id: u64,
        _action: Self::Action,
    ) -> Result<serde_json::Value> {
        Err(Error::new("unavailable", "provider disconnected"))
    }
    async fn demand(&mut self, _active: bool) -> Result<()> {
        Ok(())
    }
}
fn reconnect_delay(attempt: u32) -> Duration {
    let base = (400u64 << attempt.min(5)).min(12000);
    let jitter = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as u64
        % (base / 4);
    Duration::from_millis(base + jitter)
}
pub fn launch<K, B>(ctx: Context, mut backend: B) -> Handle<B::State, B::Action>
where
    K: TopicKey<Value = B::State>,
    B: Backend,
{
    let topic = ctx.store.topic::<K>(backend.state().clone());
    let state = topic.clone();
    let mut demand = topic.demand();
    let mut shutdown = ctx.runtime.shutdown_signal();
    let (stop, mut stopping) = watch::channel(false);
    let (sender, mut actions) = mpsc::channel::<Envelope<B::Action>>(32);
    let task = ctx.runtime.spawn(async move {
        let mut connected = false;
        let mut attempt = 0u32;
        let mut active_demand = false;
        let mut reconnect_at:Option<tokio::time::Instant>=None;
        loop {
            if *shutdown.borrow() || *stopping.borrow() {break;}
            if !connected {
                if attempt > 0 && !backend.reconnect_on_timer() {
                    reconnect_at = None;
                    tokio::select! {
                        _ = shutdown.changed() => break,
                        _ = stopping.changed() => break,
                        result = backend.reconnect_event() => {
                            if let Err(error)=result { backend.state_mut().failure(error); state.publish(backend.state().clone()); }
                            attempt=0;
                        },
                        item = actions.recv() => match item {
                            Some(item) => {
                                let result = backend.offline_action(item.id, item.action).await;
                                let _ = item.reply.send(result);
                            },
                            None => break,
                        }
                    }
                    continue;
                }
                if let Some(deadline)=reconnect_at {
                    tokio::select! {
                        _=shutdown.changed()=>break,
                        _=stopping.changed()=>break,
                        _=tokio::time::sleep_until(deadline)=>{reconnect_at=None;},
                        result=backend.reconnect_event()=>{
                            if let Err(error)=result { backend.state_mut().failure(error); state.publish(backend.state().clone()); }
                            reconnect_at=None;attempt=0;
                        },
                        item=actions.recv()=>match item {
                            Some(item)=>{let result=backend.offline_action(item.id,item.action).await;let _=item.reply.send(result);},
                            None=>break,
                        }
                    }
                    if reconnect_at.is_some(){continue;}
                }
                let result = tokio::select! {
                    biased;
                    _ = shutdown.changed() => break,
                    _ = stopping.changed() => break,
                    result = tokio::time::timeout(Duration::from_secs(10),backend.connect()) => {
                        result.unwrap_or_else(|_|Err(Error::new("timeout","provider connect timed out")))
                    },
                };
                match result {
                    Ok(()) => {
                        connected=true;

                        active_demand=*demand.borrow_and_update()>0;
                        let result=tokio::select! {
                            _ = shutdown.changed() => break,
                            _ = stopping.changed() => break,
                            result=backend.demand(active_demand)=>result,
                        };
                        if let Err(error)=result {backend.state_mut().failure(error);connected=false;}
                    },
                    Err(error) => {
                        backend.state_mut().failure(error);
                        state.publish(backend.state().clone());
                        reconnect_at=Some(tokio::time::Instant::now()+reconnect_delay(attempt));
                        attempt=attempt.saturating_add(1);
                        continue;
                    },
                }
                if !connected {
                    state.publish(backend.state().clone());
                    reconnect_at=Some(tokio::time::Instant::now()+reconnect_delay(attempt));
                    attempt=attempt.saturating_add(1);
                    continue;
                }
            }
            state.publish(backend.state().clone());
            tokio::select! {
                biased;
                _ = shutdown.changed() => break,
                _ = stopping.changed() => break,
                item = actions.recv() => match item {
                    Some(item) => {
                        let result=tokio::select! {
                            _ = shutdown.changed() => break,
                            _ = stopping.changed() => break,
                            result = backend.action(item.id,item.action)=>result,
                        };
                        if let Err(ref error)=result {
                            if matches!(error.code.as_str(),"io"|"disconnected"|"frame"|"timeout") {
                                backend.state_mut().failure(error.clone());connected=false;
                            } else {backend.state_mut().action_failure(error.clone());}
                        }
                        let _=item.reply.send(result);
                    },
                    None=>break,
                },
                changed=demand.changed()=>{
                    if changed.is_err(){break;}
                    let active=*demand.borrow_and_update()>0;
                    if active!=active_demand {
                        active_demand=active;
                        let result=tokio::select! {
                            _ = shutdown.changed() => break,
                            _ = stopping.changed() => break,
                            result=backend.demand(active)=>result,
                        };
                        if let Err(error)=result {backend.state_mut().failure(error);connected=false;}
                    }
                },
                result=backend.next()=>{
                    match result {Ok(())=>attempt=0,Err(error)=>{backend.state_mut().failure(error);connected=false;}}
                },
            }
            if !connected {
                state.publish(backend.state().clone());
                reconnect_at=Some(tokio::time::Instant::now()+reconnect_delay(attempt));
                attempt=attempt.saturating_add(1);
            }
        }
        let _=backend.demand(false).await;
    });
    Handle {
        state: topic,
        sender,
        ids: AtomicU64::new(1),
        task,
        stop,
        stopped: false,
    }
}
pub type Lines = FramedRead<tokio::net::unix::OwnedReadHalf, LinesCodec>;
pub fn lines(read: tokio::net::unix::OwnedReadHalf) -> Lines {
    FramedRead::new(read, LinesCodec::new_with_max_length(1024 * 1024))
}
pub async fn line(lines: &mut Lines) -> Result<String> {
    lines
        .next()
        .await
        .ok_or_else(|| Error::new("disconnected", "socket closed"))?
        .map_err(|e| Error::new("frame", e))
}
pub async fn portal_open(uri: &str) -> Result<serde_json::Value> {
    let bus = zbus::Connection::session().await?;
    let proxy = zbus::Proxy::new(
        &bus,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.OpenURI",
    )
    .await?;
    let options: std::collections::HashMap<&str, zbus::zvariant::Value<'_>> = Default::default();
    let path: zbus::zvariant::OwnedObjectPath = proxy.call("OpenURI", &("", uri, options)).await?;
    Ok(serde_json::json!({"request":path.as_str()}))
}
/// inotify watches the closest existing ancestor, rearming after atomic replacement.
/// This stays asleep in AsyncFd until a filesystem event occurs.
pub struct FileWatch {
    fd: tokio::io::unix::AsyncFd<std::os::fd::OwnedFd>,
    paths: Vec<PathBuf>,
    directory: Option<(PathBuf, String, String)>,
    watches: std::collections::BTreeMap<i32, (PathBuf, Vec<std::ffi::OsString>)>,
}
impl FileWatch {
    pub fn new(paths: Vec<PathBuf>) -> Result<Self> {
        use std::os::fd::FromRawFd;
        let raw = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if raw < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: AsyncFd owns this fixed descriptor until drop.
        let fd =
            unsafe { tokio::io::unix::AsyncFd::register(std::os::fd::OwnedFd::from_raw_fd(raw)) }
                .map_err(|error| Error::new("inotify", error))?;
        let mut this = Self {
            fd,
            paths,
            directory: None,
            watches: Default::default(),
        };
        this.rearm()?;
        Ok(this)
    }
    /// Watch only matching child names, sleeping through unrelated sibling events.
    pub fn directory(path: PathBuf, prefix: &str, suffix: &str) -> Result<Self> {
        let mut watcher=Self::new(vec![path.clone()])?;
        watcher.directory=Some((path,prefix.into(),suffix.into()));
        watcher.rearm()?;
        Ok(watcher)
    }
    fn rearm(&mut self) -> Result<()> {
        use std::os::{fd::AsRawFd, unix::ffi::OsStrExt};
        let mut desired: std::collections::BTreeMap<PathBuf, Vec<std::ffi::OsString>> =
            Default::default();
        let watching_directory=self.directory.as_ref().is_some_and(|(path,_,_)|path.is_dir());
        if watching_directory { desired.entry(self.directory.as_ref().unwrap().0.clone()).or_default(); }
        for path in self.paths.iter().filter(|_|!watching_directory) {
            let path = if path.is_absolute() {
                path.clone()
            } else {
                std::env::current_dir()?.join(path)
            };
            let mut parent = path.parent().unwrap_or(Path::new("/"));
            while !parent.is_dir() {
                parent = parent.parent().unwrap_or(Path::new("/"));
            }
            if let Some(component) = path
                .strip_prefix(parent)
                .ok()
                .and_then(|p| p.components().next())
            {
                desired
                    .entry(parent.into())
                    .or_default()
                    .push(component.as_os_str().to_owned());
            }
        }
        let fd = self.fd.get_ref().as_raw_fd();
        let stale: Vec<_> = self
            .watches
            .iter()
            .filter(|(_, (path, _))| !desired.contains_key(path))
            .map(|(wd, _)| *wd)
            .collect();
        for wd in stale {
            self.watches.remove(&wd);
            unsafe { libc::inotify_rm_watch(fd, wd) };
        }
        for (path, names) in desired {
            if let Some((_, (_, current))) = self.watches.iter_mut().find(|(_, (p, _))| *p == path)
            {
                *current = names;
                continue;
            }
            let name = std::ffi::CString::new(path.as_os_str().as_bytes())
                .map_err(|error| Error::new("path", error))?;
            let mask = libc::IN_CLOSE_WRITE
                | libc::IN_MOVED_TO
                | libc::IN_MOVED_FROM
                | libc::IN_CREATE
                | libc::IN_DELETE
                | libc::IN_ATTRIB
                | libc::IN_DELETE_SELF
                | libc::IN_MOVE_SELF;
            let wd = unsafe { libc::inotify_add_watch(fd, name.as_ptr(), mask) };
            if wd < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            self.watches.insert(wd, (path, names));
        }
        Ok(())
    }
    pub async fn changed(&mut self) -> Result<()> {
        use std::os::{fd::AsRawFd, unix::ffi::OsStrExt};
        loop {
            let mut ready = self.fd.readable().await?;
            let mut bytes = [0u8; 16384];
            let count = match ready.try_io(|fd| {
                let count = unsafe {
                    libc::read(
                        fd.get_ref().as_raw_fd(),
                        bytes.as_mut_ptr().cast(),
                        bytes.len(),
                    )
                };
                if count < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(count as usize)
                }
            }) {
                Ok(result) => result?,
                Err(_) => continue,
            };
            let mut offset = 0;
            let mut relevant = false;
            while offset + std::mem::size_of::<libc::inotify_event>() <= count {
                // SAFETY: the bounded slice contains a complete, possibly unaligned header.
                let event = unsafe {
                    std::ptr::read_unaligned(bytes[offset..].as_ptr().cast::<libc::inotify_event>())
                };
                let header = std::mem::size_of::<libc::inotify_event>();
                let end = offset + header + event.len as usize;
                if end > count {
                    return Err(Error::new("inotify", "truncated event"));
                }
                if event.mask & libc::IN_Q_OVERFLOW != 0 {
                    relevant = true;
                }
                if let Some((_, names)) = self.watches.get(&event.wd) {
                    if event.mask & (libc::IN_DELETE_SELF | libc::IN_MOVE_SELF | libc::IN_IGNORED)
                        != 0
                    {
                        relevant = true;
                        self.watches.remove(&event.wd);
                        unsafe { libc::inotify_rm_watch(self.fd.get_ref().as_raw_fd(), event.wd) };
                    } else {
                        let raw = &bytes[offset + header..end];
                        let raw = &raw[..raw.iter().position(|b| *b == 0).unwrap_or(raw.len())];
                        let name = std::ffi::OsStr::from_bytes(raw);
                        relevant |= names.iter().any(|expected| expected == name)
                            || (names.is_empty() && self.directory.as_ref().is_some_and(|(_,prefix,suffix)|
                                raw.starts_with(prefix.as_bytes()) && raw.ends_with(suffix.as_bytes())));
                    }
                }
                offset = end;
            }
            if relevant {
                self.rearm()?;
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
pub(crate) struct TestDir(pub PathBuf, std::fs::File);
#[cfg(test)]
impl TestDir {
    pub fn socket_dir(&self) -> PathBuf {
        use std::os::fd::AsRawFd;
        PathBuf::from(format!("/proc/self/fd/{}", self.1.as_raw_fd()))
    }
    pub fn new(name: &str) -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(1);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".check/test-data")
            .join(format!(
                "{name}-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&path).unwrap();
        let directory = std::fs::File::open(&path).unwrap();
        Self(path, directory)
    }
}
#[cfg(test)]
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Clone, PartialEq)]
    struct MockState(u64);
    impl Model for MockState {
        fn failure(&mut self, _: Error) {}
    }
    struct Key;
    impl TopicKey for Key {
        type Value = MockState;
    }
    struct Mock {
        state: MockState,
        events: mpsc::UnboundedSender<bool>,
    }
    #[async_trait::async_trait]
    impl Backend for Mock {
        type State = MockState;
        type Action = bool;
        fn state(&self) -> &MockState {
            &self.state
        }
        fn state_mut(&mut self) -> &mut MockState {
            &mut self.state
        }
        async fn connect(&mut self) -> Result<()> {
            Ok(())
        }
        async fn next(&mut self) -> Result<()> {
            std::future::pending().await
        }
        async fn demand(&mut self, active: bool) -> Result<()> {
            self.events.send(active).unwrap();
            Ok(())
        }
        async fn action(&mut self, _: u64, a: bool) -> Result<serde_json::Value> {
            if !a {
                return Err(Error::new("mock", "rejected"));
            }
            self.state.0 += 1;
            Ok(serde_json::json!(self.state.0))
        }
    }
    #[test]
    fn subscriber_transitions_and_shutdown() {
        let runtime = Arc::new(ServiceRuntime::new().unwrap());
        let store = Arc::new(StateStore::default());
        let (send, mut recv) = mpsc::unbounded_channel();
        let handle = launch::<Key, _>(
            Context {
                runtime: runtime.clone(),
                store,
            },
            Mock {
                state: MockState(0),
                events: send,
            },
        );
        runtime.block_on(async {
            assert!(!recv.recv().await.unwrap());
            let first = handle.state.subscribe();
            assert!(recv.recv().await.unwrap());
            let second = handle.state.subscribe();
            assert!(handle.dispatch(false).await.is_err());
            assert_eq!(handle.dispatch(true).await.unwrap(), 1);
            assert!(recv.try_recv().is_err());
            drop(first);
            assert_eq!(handle.dispatch(true).await.unwrap(), 2);
            assert!(recv.try_recv().is_err());
            drop(second);
            assert!(!recv.recv().await.unwrap());
            runtime.request_shutdown();
            handle.task.await.unwrap();
        });
    }
    #[test]
    fn core_service_stop_cancels_and_is_idempotent() {
        use hyprv_core::service::Service;
        let runtime = Arc::new(ServiceRuntime::new().unwrap());
        let (send, mut receive) = mpsc::unbounded_channel();
        let mut handle = launch::<Key, _>(
            Context {
                runtime: runtime.clone(),
                store: Arc::new(StateStore::default()),
            },
            Mock {
                state: MockState(0),
                events: send,
            },
        );
        runtime.block_on(async {
            handle.start().await.unwrap();
            assert!(!receive.recv().await.unwrap());
            let _visible = handle.state.subscribe();
            assert!(receive.recv().await.unwrap());
            handle.stop().await.unwrap();
            assert!(!receive.recv().await.unwrap());
            handle.stop().await.unwrap();
            assert!(handle.start().await.is_err());
            assert!(handle.dispatch(true).await.is_err());
        });
    }
    #[tokio::test]
    async fn inotify_atomic_replacement_and_frame_limit() {
        let dir = TestDir::new("inotify");
        let path = dir.0.join("file");
        let mut watch = FileWatch::new(vec![path.clone()]).unwrap();
        std::fs::write(dir.0.join("swap"), "{}").unwrap();
        std::fs::rename(dir.0.join("swap"), &path).unwrap();
        tokio::time::timeout(Duration::from_secs(1), watch.changed())
            .await
            .unwrap()
            .unwrap();
        let (a, mut b) = tokio::net::UnixStream::pair().unwrap();
        let (r, _) = a.into_split();
        let mut read = lines(r);
        let writer = tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            b.write_all(&vec![b'x'; 1024 * 1024 + 2]).await.unwrap();
        });
        assert_eq!(line(&mut read).await.unwrap_err().code, "frame");
        writer.await.unwrap();
    }
}

#[cfg(test)]
mod adversarial_review {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    #[derive(Clone, PartialEq)]
    struct State;
    impl Model for State {
        fn failure(&mut self, _: Error) {}
    }
    struct Key;
    impl TopicKey for Key {
        type Value = State;
    }
    struct Flapping {
        state: State,
        attempts: Arc<AtomicUsize>,
    }
    #[async_trait::async_trait]
    impl Backend for Flapping {
        type State = State;
        type Action = ();
        fn state(&self) -> &State {
            &self.state
        }
        fn state_mut(&mut self) -> &mut State {
            &mut self.state
        }
        async fn connect(&mut self) -> Result<()> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn next(&mut self) -> Result<()> {
            Err(Error::new(
                "disconnected",
                "provider closes immediately after snapshot",
            ))
        }
        async fn action(&mut self, _: u64, _: ()) -> Result<Value> {
            Ok(Value::Null)
        }
    }
    use serde_json::Value;
    #[test]
    fn review_sys_flapping_provider_has_reconnect_backoff() {
        let runtime = Arc::new(ServiceRuntime::new().unwrap());
        let attempts = Arc::new(AtomicUsize::new(0));
        let handle = launch::<Key, _>(
            Context {
                runtime: runtime.clone(),
                store: Arc::new(StateStore::default()),
            },
            Flapping {
                state: State,
                attempts: attempts.clone(),
            },
        );
        std::thread::sleep(Duration::from_millis(40));
        runtime.request_shutdown();
        runtime.block_on(handle.task).unwrap();
        assert!(attempts.load(Ordering::SeqCst)<=1,"{} reconnects in 40ms; successful connect resets retry history and next() errors skip backoff",attempts.load(Ordering::SeqCst));
    }
    #[tokio::test]
    async fn review_sys_watch_ignores_unrelated_sibling() {
        let dir = TestDir::new("review-sys-watch");
        let mut watcher = FileWatch::new(vec![dir.0.join("config.yaml")]).unwrap();
        std::fs::write(dir.0.join("unrelated.log"), "log write").unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(50), watcher.changed())
                .await
                .is_err(),
            "unrelated sibling woke all provider file consumers"
        );
    }
}

#[cfg(test)]
mod filtered_watch_tests {
    use super::*;
    #[tokio::test]
    async fn missing_parent_narrows_and_atomic_replacement_does_not_self_wake() {
        let directory = TestDir::new("filtered-watch");
        let nested = directory.0.join("nested");
        let path = nested.join("config.yaml");
        let mut watch = FileWatch::new(vec![path.clone()]).unwrap();
        std::fs::write(directory.0.join("unrelated.log"), "unrelated").unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(30), watch.changed())
                .await
                .is_err()
        );
        std::fs::create_dir(&nested).unwrap();
        std::fs::write(&path, "first").unwrap();
        tokio::time::timeout(Duration::from_secs(1), watch.changed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(watch.watches.len(), 1);
        assert_eq!(watch.watches.values().next().unwrap().0, nested);
        std::fs::write(directory.0.join("unrelated.log"), "another ancestor change").unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(30), watch.changed())
                .await
                .is_err()
        );
        std::fs::write(nested.join("swap"), "second").unwrap();
        std::fs::rename(nested.join("swap"), &path).unwrap();
        tokio::time::timeout(Duration::from_secs(1), watch.changed())
            .await
            .unwrap()
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(30), watch.changed())
                .await
                .is_err()
        );
    }
    #[test]
    fn reconnect_backoff_grows_and_caps() {
        for attempt in 0..12 {
            let delay = reconnect_delay(attempt).as_millis();
            let base = (400u128 << attempt.min(5)).min(12000);
            assert!(delay >= base && delay < base + base / 4);
        }
    }
}

#[cfg(test)]
mod unconfigured_idle_tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    #[derive(Clone, PartialEq)]
    struct State;
    impl Model for State { fn failure(&mut self, _: Error) {} }
    struct Key;
    impl TopicKey for Key { type Value = State; }
    struct BackendFixture { state: State, configured: bool, attempts: Arc<AtomicUsize> }
    #[async_trait::async_trait]
    impl Backend for BackendFixture {
        type State = State;
        type Action = ();
        fn state(&self) -> &State { &self.state }
        fn state_mut(&mut self) -> &mut State { &mut self.state }
        fn reconnect_on_timer(&self) -> bool { self.configured }
        async fn connect(&mut self) -> Result<()> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            if self.configured { Ok(()) } else { Err(Error::new("discovery", "not configured")) }
        }
        async fn next(&mut self) -> Result<()> { std::future::pending().await }
        async fn action(&mut self, _: u64, _: ()) -> Result<serde_json::Value> { Ok(serde_json::Value::Null) }
        async fn offline_action(&mut self, _: u64, _: ()) -> Result<serde_json::Value> {
            self.configured = true;
            Ok(serde_json::Value::Null)
        }
    }
    #[test]
    fn no_retry_timer_until_configured_and_action_connects_immediately() {
        let runtime = Arc::new(ServiceRuntime::new().unwrap());
        let attempts = Arc::new(AtomicUsize::new(0));
        let handle = launch::<Key,_>(Context { runtime: runtime.clone(), store: Arc::new(StateStore::default()) }, BackendFixture { state: State, configured: false, attempts: attempts.clone() });
        runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(1), async {
                while attempts.load(Ordering::SeqCst) == 0 { tokio::task::yield_now().await; }
            }).await.unwrap();
            tokio::time::sleep(Duration::from_millis(650)).await;
            assert_eq!(attempts.load(Ordering::SeqCst), 1, "unconfigured provider must park, not retry");
            handle.dispatch(()).await.unwrap();
            tokio::time::timeout(Duration::from_secs(1), async {
                while attempts.load(Ordering::SeqCst) < 2 { tokio::task::yield_now().await; }
            }).await.unwrap();
            runtime.request_shutdown();
            handle.task.await.unwrap();
        });
    }
}
