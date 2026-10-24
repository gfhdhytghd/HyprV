//! All mutations here hit in-memory transports. No desktop daemon is controlled.
use async_trait::async_trait;
use hyprv_core::{runtime::ServiceRuntime, store::StateStore};
use hyprv_svc_net::{
    bluetooth, network,
    prompts::{Broker, Kind, Reply},
    Context, Event, Result,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::{mpsc, Mutex};
struct NetworkMock {
    value: std::sync::Mutex<network::State>,
    events: Mutex<mpsc::Receiver<Event>>,
    reads: AtomicUsize,
    changes: mpsc::Sender<Event>,
    actions: AtomicUsize,
    closed: AtomicUsize,
    broker: Broker,
}
#[async_trait]
impl network::Transport for NetworkMock {
    async fn snapshot(&self) -> Result<network::State> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(self.value.lock().unwrap().clone())
    }
    async fn execute(&self, action: network::Action) -> Result<String> {
        self.actions.fetch_add(1, Ordering::SeqCst);
        match action {
            network::Action::SetRadio(enabled) => self.value.lock().unwrap().enabled = enabled,
            network::Action::Connect { .. } => {
                let reply = self
                    .broker
                    .request(Kind::NetworkSecrets {
                        connection: "profile".into(),
                        setting: "802-1x".into(),
                        hints: vec!["password".into()],
                        new_secrets: false,
                    })
                    .await
                    .map_err(|e| e.to_string())?;
                if !matches!(reply, Reply::Fields(_)) {
                    return Err("cancelled".into());
                }
                self.value.lock().unwrap().connected = true;
            }
            _ => return Err("mock request rejected".into()),
        }
        self.changes.send(Event::Changed).await.unwrap();
        Ok("ok".into())
    }
    async fn next_event(&self) -> Event {
        self.events.lock().await.recv().await.unwrap()
    }
    async fn shutdown(&self) {
        self.closed.fetch_add(1, Ordering::SeqCst);
    }
}
fn ctx(read_only: bool) -> Context {
    let mut ctx = Context::new(
        Arc::new(StateStore::default()),
        Arc::new(ServiceRuntime::new().unwrap()),
    );
    ctx.read_only = read_only;
    ctx
}
fn network_mock(broker: Broker) -> Arc<NetworkMock> {
    let (tx, rx) = mpsc::channel(16);
    Arc::new(NetworkMock {
        value: std::sync::Mutex::new(network::State {
            available: true,
            present: true,
            ..network::State::default()
        }),
        events: Mutex::new(rx),
        reads: AtomicUsize::new(0),
        changes: tx,
        actions: AtomicUsize::new(0),
        closed: AtomicUsize::new(0),
        broker,
    })
}
async fn until(predicate: impl Fn() -> bool) {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while !predicate() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn network_idle_diffs_errors_and_shutdown() {
    let ctx = ctx(false);
    let broker = Broker::new();
    let mock = network_mock(broker.clone());
    let h = network::spawn_with_transport(ctx.clone(), mock.clone(), broker);
    until(|| h.state.snapshot().value.available).await;
    let reads = mock.reads.load(Ordering::SeqCst);
    for _ in 0..100 {
        tokio::task::yield_now().await;
    }
    assert_eq!(mock.reads.load(Ordering::SeqCst), reads);
    h.dispatch(network::Action::SetRadio(true))
        .await
        .unwrap()
        .result()
        .await
        .unwrap();
    until(|| h.state.snapshot().value.enabled && !h.state.snapshot().value.busy).await;
    let revision = h.state.snapshot().revision;
    mock.changes.send(Event::Changed).await.unwrap();
    until(|| mock.reads.load(Ordering::SeqCst) > reads + 1).await;
    assert_eq!(revision, h.state.snapshot().revision);
    assert!(h
        .dispatch(network::Action::Disconnect)
        .await
        .unwrap()
        .result()
        .await
        .is_err());
    until(|| h.state.snapshot().value.error.is_some()).await;
    mock.value.lock().unwrap().available = false;
    mock.changes.send(Event::OwnerChanged).await.unwrap();
    until(|| !h.state.snapshot().value.available).await;
    ctx.runtime.request_shutdown();
    until(|| mock.closed.load(Ordering::SeqCst) == 1).await;
}
#[tokio::test]
async fn network_prompt_does_not_block_observation_and_cancels() {
    let ctx = ctx(false);
    let broker = Broker::new();
    let mock = network_mock(broker.clone());
    let h = network::spawn_with_transport(ctx.clone(), mock.clone(), broker);
    until(|| h.state.snapshot().value.available).await;
    let action = h
        .dispatch(network::Action::Connect {
            ssid: "enterprise".into(),
            password: None,
            security: "EAP".into(),
            enterprise: None,
        })
        .await
        .unwrap();
    until(|| !h.prompts.snapshot().value.is_empty()).await;
    mock.value.lock().unwrap().ssid = "other network".into();
    mock.changes.send(Event::Changed).await.unwrap();
    until(|| h.state.snapshot().value.ssid == "other network").await;
    let id = h.prompts.snapshot().value[0].id;
    h.respond(id, Reply::Reject).unwrap();
    assert!(action.result().await.is_err());
    assert!(h.prompts.snapshot().value.is_empty());
    ctx.runtime.request_shutdown();
    until(|| mock.closed.load(Ordering::SeqCst) == 1).await;
}
#[tokio::test]
async fn read_only_rejects_before_transport_mutation() {
    let ctx = ctx(true);
    let broker = Broker::new();
    let mock = network_mock(broker.clone());
    let h = network::spawn_with_transport(ctx.clone(), mock.clone(), broker);
    until(|| h.state.snapshot().value.available).await;
    assert!(h
        .dispatch(network::Action::SetRadio(false))
        .await
        .unwrap()
        .result()
        .await
        .is_err());
    assert_eq!(mock.actions.load(Ordering::SeqCst), 0);
    ctx.runtime.request_shutdown();
    until(|| mock.closed.load(Ordering::SeqCst) == 1).await;
}
struct BluetoothMock {
    state: std::sync::Mutex<bluetooth::State>,
    rx: Mutex<mpsc::Receiver<Event>>,
    tx: mpsc::Sender<Event>,
    closed: AtomicUsize,
}
#[async_trait]
impl bluetooth::Transport for BluetoothMock {
    async fn snapshot(&self) -> Result<bluetooth::State> {
        Ok(self.state.lock().unwrap().clone())
    }
    async fn execute(&self, action: bluetooth::Action) -> Result<String> {
        match action {
            bluetooth::Action::Scan => self.state.lock().unwrap().discovering = true,
            bluetooth::Action::StopScan => self.state.lock().unwrap().discovering = false,
            _ => return Err("mock action failed".into()),
        }
        self.tx.send(Event::Changed).await.unwrap();
        Ok("ok".into())
    }
    async fn next_event(&self) -> Event {
        self.rx.lock().await.recv().await.unwrap()
    }
    async fn shutdown(&self) {
        self.closed.fetch_add(1, Ordering::SeqCst);
    }
}
#[tokio::test]
async fn bluetooth_scan_has_explicit_stop_and_shutdown_releases_ownership() {
    let ctx = ctx(false);
    let (tx, rx) = mpsc::channel(16);
    let mock = Arc::new(BluetoothMock {
        state: std::sync::Mutex::new(bluetooth::State {
            available: true,
            present: true,
            enabled: true,
            ..bluetooth::State::default()
        }),
        rx: Mutex::new(rx),
        tx,
        closed: AtomicUsize::new(0),
    });
    let h = bluetooth::spawn_with_transport(ctx.clone(), mock.clone(), Broker::new());
    until(|| h.state.snapshot().value.available).await;
    h.dispatch(bluetooth::Action::Scan)
        .await
        .unwrap()
        .result()
        .await
        .unwrap();
    until(|| h.state.snapshot().value.discovering && !h.state.snapshot().value.busy).await;
    h.dispatch(bluetooth::Action::StopScan)
        .await
        .unwrap()
        .result()
        .await
        .unwrap();
    until(|| !h.state.snapshot().value.discovering && !h.state.snapshot().value.busy).await;
    h.dispatch(bluetooth::Action::Scan)
        .await
        .unwrap()
        .result()
        .await
        .unwrap();
    until(|| h.state.snapshot().value.discovering && !h.state.snapshot().value.busy).await;
    ctx.runtime.request_shutdown();
    until(|| mock.closed.load(Ordering::SeqCst) == 1).await;
    assert!(!mock.state.lock().unwrap().discovering);
}
