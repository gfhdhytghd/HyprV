//! Shared actor plumbing: context, action handles, monotonic time and reconnect backoff.
use hyprv_core::{runtime::ServiceRuntime, store::{StateStore, Topic}};
use std::{fmt, sync::{atomic::{AtomicU64, Ordering}, Arc}, time::Duration};
use tokio::{sync::{mpsc, oneshot}, task::JoinHandle};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
}
impl std::error::Error for Error {}
impl From<zbus::Error> for Error {
    fn from(e: zbus::Error) -> Self { Self(e.to_string()) }
}
impl From<zbus::fdo::Error> for Error {
    fn from(e: zbus::fdo::Error) -> Self { Self(e.to_string()) }
}
impl From<zbus::names::Error> for Error {
    fn from(e: zbus::names::Error) -> Self { Self(e.to_string()) }
}
impl From<zbus::zvariant::Error> for Error {
    fn from(e: zbus::zvariant::Error) -> Self { Self(e.to_string()) }
}
pub type Result<T> = std::result::Result<T, Error>;
pub(crate) fn err<T>(message: impl Into<String>) -> Result<T> { Err(Error(message.into())) }

/// Everything a media actor needs. Buses are injectable for tests and containers.
#[derive(Clone)]
pub struct Context {
    pub store: Arc<StateStore>,
    pub runtime: Arc<ServiceRuntime>,
    /// None connects to the session bus on first use.
    pub session_bus: Option<zbus::Connection>,
    /// Read-only mode: actors observe but refuse all actions and never own bus names.
    pub read_only: bool,
    /// Work attribution for the minimal-state budget (shared by every actor of this context).
    pub counters: Arc<Counters>,
}
impl Context {
    pub fn new(store: Arc<StateStore>, runtime: Arc<ServiceRuntime>) -> Self {
        Self { store, runtime, session_bus: None, read_only: false, counters: Arc::default() }
    }
    /// Bus access for actor tasks. Tasks must not capture the Context itself: it holds
    /// the `ServiceRuntime`, whose last reference must never drop on its own thread.
    pub(crate) fn buses(&self) -> Buses {
        Buses { session: self.session_bus.clone(), read_only: self.read_only }
    }
}

#[derive(Clone)]
pub(crate) struct Buses {
    session: Option<zbus::Connection>,
    pub read_only: bool,
}
impl Buses {
    pub async fn session(&self) -> Result<zbus::Connection> {
        match &self.session {
            Some(bus) => Ok(bus.clone()),
            None => Ok(zbus::Connection::session().await?),
        }
    }
}

/// Monotonic work counters. Each names one source of media wakeups or reads, so a
/// minimal-state run can attribute (and assert) zero media work while nothing changes.
#[derive(Debug, Default)]
pub struct Counters {
    /// MPRIS actor loop turns after its initial snapshot (bus signals, actions, timers).
    pub mpris_wakeups: AtomicU64,
    /// `Get Position` calls issued (never periodic: playback/rate/track change only).
    pub position_reads: AtomicU64,
    /// Cider repair debounce/fallback firings.
    pub compat_refreshes: AtomicU64,
    /// Spectrum capture starts (cava stream + 24 Hz frame timer).
    pub spectrum_starts: AtomicU64,
}
/// Plain-data copy of [`Counters`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct CounterSnapshot {
    pub mpris_wakeups: u64,
    pub position_reads: u64,
    pub compat_refreshes: u64,
    pub spectrum_starts: u64,
}
impl Counters {
    pub fn snapshot(&self) -> CounterSnapshot {
        let get = |c: &AtomicU64| c.load(Ordering::Relaxed);
        CounterSnapshot {
            mpris_wakeups: get(&self.mpris_wakeups),
            position_reads: get(&self.position_reads),
            compat_refreshes: get(&self.compat_refreshes),
            spectrum_starts: get(&self.spectrum_starts),
        }
    }
}
pub(crate) fn bump(counter: &AtomicU64) { counter.fetch_add(1, Ordering::Relaxed); }

pub(crate) type Reply<R> = oneshot::Sender<Result<R>>;
pub(crate) type Request<A, R> = (A, Reply<R>);

/// Completion of one action. Dropping it does not cancel the action.
pub struct PendingAction<R = ()>(oneshot::Receiver<Result<R>>);
impl<R> PendingAction<R> {
    pub async fn result(self) -> Result<R> {
        self.0.await.unwrap_or_else(|_| err("service stopped"))
    }
}

/// Per-module handle: typed state topic, bounded action queue and the actor task.
pub struct Handle<S: Clone, A, R = ()> {
    pub topic: Topic<S>,
    actions: mpsc::Sender<Request<A, R>>,
    task: Option<JoinHandle<()>>,
}
impl<S: Clone + PartialEq, A, R> Handle<S, A, R> {
    pub(crate) fn new(topic: Topic<S>, actions: mpsc::Sender<Request<A, R>>, task: JoinHandle<()>) -> Self {
        Self { topic, actions, task: Some(task) }
    }
    /// Queue an action; the result arrives independently of the caller's lifetime.
    pub fn action(&self, action: A) -> PendingAction<R> {
        let (reply, wait) = oneshot::channel();
        if let Err(error) = self.actions.try_send((action, reply)) {
            let (_, reply) = match error {
                mpsc::error::TrySendError::Full(r) => r,
                mpsc::error::TrySendError::Closed(r) => r,
            };
            let _ = reply.send(err("action queue unavailable"));
        }
        PendingAction(wait)
    }
    /// Clonable action sender for other actors (e.g. mpris → cider).
    pub fn requester(&self) -> Requester<A, R> { Requester(self.actions.clone()) }
    /// Await actor termination (after `ServiceRuntime::request_shutdown`).
    pub async fn join(mut self) {
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

pub struct Requester<A, R = ()>(mpsc::Sender<Request<A, R>>);
impl<A, R> Clone for Requester<A, R> {
    fn clone(&self) -> Self { Self(self.0.clone()) }
}
impl<A, R> Requester<A, R> {
    pub async fn request(&self, action: A) -> Result<R> {
        let (reply, wait) = oneshot::channel();
        self.0.send((action, reply)).await.map_err(|_| Error("service stopped".into()))?;
        wait.await.unwrap_or_else(|_| err("service stopped"))
    }
}

pub(crate) const ACTION_QUEUE: usize = 32;
pub(crate) type Queue<A, R> = (mpsc::Sender<Request<A, R>>, mpsc::Receiver<Request<A, R>>);
pub(crate) fn action_queue<A, R>() -> Queue<A, R> {
    mpsc::channel(ACTION_QUEUE)
}

/// CLOCK_MONOTONIC in microseconds; position anchors use this timebase so the UI
/// thread can extrapolate without a service timer.
pub fn monotonic_us() -> u64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: valid pointer to a timespec; CLOCK_MONOTONIC is always available on Linux.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    ts.tv_sec as u64 * 1_000_000 + ts.tv_nsec as u64 / 1_000
}

/// Exponential reconnect backoff with jitter, used only while a provider is absent.
pub(crate) struct Backoff { attempt: u32 }
impl Backoff {
    pub fn new() -> Self { Self { attempt: 0 } }
    pub fn reset(&mut self) { self.attempt = 0; }
    pub fn next_delay(&mut self) -> Duration {
        let base = 500u64 << self.attempt.min(6); // 0.5s .. 32s
        self.attempt = self.attempt.saturating_add(1);
        let jitter = (monotonic_us() % 1000) * base / 4000; // up to +25%
        Duration::from_millis(base + jitter)
    }
}

/// Collapse whitespace like the legacy scripts' `single_line`.
pub(crate) fn single_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backoff_grows_and_caps() {
        let mut b = Backoff::new();
        let first = b.next_delay();
        assert!(first >= Duration::from_millis(500) && first <= Duration::from_millis(625));
        for _ in 0..20 { b.next_delay(); }
        assert!(b.next_delay() <= Duration::from_millis(40_000));
        b.reset();
        assert!(b.next_delay() <= Duration::from_millis(625));
    }
    #[test]
    fn single_line_collapses() {
        assert_eq!(single_line(" a\n b\r\tc  "), "a b c");
    }
}
