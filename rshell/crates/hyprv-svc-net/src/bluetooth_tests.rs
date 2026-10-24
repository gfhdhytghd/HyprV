//! BlueZ agent tests against a fake BlueZ on a private bus.
use super::*;
use crate::test_support::{ctx, PrivateBus};
use std::{sync::Mutex as StdMutex, time::Duration};

#[derive(Clone, Default)]
struct Log(Arc<StdMutex<Vec<String>>>);
impl Log {
    fn push(&self, s: impl Into<String>) {
        self.0.lock().unwrap().push(s.into());
    }
    fn count(&self, p: &str) -> usize {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.starts_with(p))
            .count()
    }
}
struct FAgentManager(Log);
#[zbus::interface(name = "org.bluez.AgentManager1")]
impl FAgentManager {
    fn register_agent(&self, path: OwnedObjectPath, capability: String) {
        self.0
            .push(format!("register {} {capability}", path.as_str()));
    }
    fn request_default_agent(&self, _path: OwnedObjectPath) {
        self.0.push("default");
    }
    fn unregister_agent(&self, _path: OwnedObjectPath) {
        self.0.push("unregister");
    }
}
async fn fake_bluez(bus: &PrivateBus, log: Log) -> Connection {
    bus.builder()
        .name(BLUEZ)
        .unwrap()
        .serve_at("/org/bluez", FAgentManager(log))
        .unwrap()
        .serve_at("/", zbus::fdo::ObjectManager)
        .unwrap()
        .build()
        .await
        .unwrap()
}
async fn export(agent: &Connection, broker: &Broker) {
    agent
        .object_server()
        .at(
            AGENT,
            PairingAgent {
                connection: agent.clone(),
                broker: broker.clone(),
            },
        )
        .await
        .unwrap();
}
fn dev() -> OwnedObjectPath {
    OwnedObjectPath::try_from("/org/bluez/hci0/dev_00_11_22_33_44_55").unwrap()
}
async fn call<B>(
    from: &Connection,
    agent: &Connection,
    method: &str,
    body: &B,
) -> zbus::Result<zbus::Message>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    from.call_method(
        Some(agent.unique_name().unwrap().as_str()),
        AGENT,
        Some("org.bluez.Agent1"),
        method,
        body,
    )
    .await
}
async fn wait_for(log: &Log, p: &str, n: usize) {
    let t = tokio::time::Instant::now();
    while log.count(p) < n && t.elapsed() < Duration::from_secs(3) {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
const UUID_A2DP: &str = "0000110b-0000-1000-8000-00805f9b34fb";

/// Control: only BlueZ's current owner may call the agent.
#[tokio::test(flavor = "multi_thread")]
async fn control_foreign_caller_is_rejected() {
    let bus = PrivateBus::start();
    let _bz = fake_bluez(&bus, Log::default()).await;
    let agent = bus.connect().await;
    let attacker = bus.connect().await;
    let broker = Broker::new();
    export(&agent, &broker).await;
    let r = call(&attacker, &agent, "AuthorizeService", &(dev(), UUID_A2DP)).await;
    assert!(
        matches!(r, Err(zbus::Error::MethodError(ref n, _, _)) if n.as_str() == "org.bluez.Error.Rejected"),
        "{r:?}"
    );
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(broker.topic().snapshot().value.is_empty());
}

/// Control: authorization/confirmation wait for explicit consent; silence
/// never authorizes, Reject refuses, only Accept authorizes.
#[tokio::test(flavor = "multi_thread")]
async fn control_authorization_requires_explicit_consent() {
    let bus = PrivateBus::start();
    let bz = fake_bluez(&bus, Log::default()).await;
    let agent = bus.connect().await;
    let broker = Broker::new();
    export(&agent, &broker).await;
    for (reply, ok) in [(Reply::Reject, false), (Reply::Accept, true)] {
        let (b2, a2) = (bz.clone(), agent.clone());
        let pending = tokio::spawn(async move {
            call(&b2, &a2, "RequestConfirmation", &(dev(), 123456u32)).await
        });
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!pending.is_finished(), "confirmed without a user reply");
        let id = broker.topic().snapshot().value[0].id;
        broker.respond(id, reply).unwrap();
        let r = tokio::time::timeout(Duration::from_secs(2), pending)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(r.is_ok(), ok);
    }
}

/// RequestPinCode returns the typed PIN (serialized from a zeroizing buffer).
#[tokio::test(flavor = "multi_thread")]
async fn pin_code_reply_roundtrip() {
    let bus = PrivateBus::start();
    let bz = fake_bluez(&bus, Log::default()).await;
    let agent = bus.connect().await;
    let broker = Broker::new();
    export(&agent, &broker).await;
    let (b2, a2) = (bz.clone(), agent.clone());
    let pending = tokio::spawn(async move { call(&b2, &a2, "RequestPinCode", &(dev(),)).await });
    let t = tokio::time::Instant::now();
    while broker.topic().snapshot().value.is_empty() && t.elapsed() < Duration::from_secs(2) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let id = broker.topic().snapshot().value[0].id;
    broker.respond(id, Reply::Pin("0000".into())).unwrap();
    let reply = pending.await.unwrap().unwrap();
    assert_eq!(reply.body().deserialize::<String>().unwrap(), "0000");
}

/// BT-1: stopping bluetoothd never D-Bus-activates it again; the agent
/// registers once the daemon appears by itself.
#[tokio::test(flavor = "multi_thread")]
async fn bt1_daemon_stop_does_not_reactivate_bluez() {
    let dir = std::env::temp_dir().join(format!("hyprv-net-bt-act-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let marker = dir.join("bluez-activated");
    let bus = PrivateBus::with_activatable(BLUEZ, &marker);
    let log = Log::default();
    let bz = fake_bluez(&bus, log.clone()).await;
    let agent = bus.connect().await;
    let handle = spawn(ctx(agent));
    wait_for(&log, "default", 1).await;
    assert_eq!(log.count("register"), 1, "initial registration");
    // The daemon is stopped (systemctl stop bluetooth).
    bz.close().await.unwrap();
    tokio::time::sleep(Duration::from_millis(1000)).await;
    assert!(
        !marker.exists(),
        "stopping bluetoothd caused a D-Bus activation request for org.bluez"
    );
    let s = handle.state.snapshot().value;
    assert!(!s.available, "{s:?}");
    // It comes back on its own: the agent registers with the new instance.
    let log2 = Log::default();
    let _bz2 = fake_bluez(&bus, log2.clone()).await;
    wait_for(&log2, "default", 1).await;
    assert_eq!((log2.count("register"), log2.count("default")), (1, 1));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A shell started while bluetoothd is stopped sends nothing to org.bluez
/// (no activation) and registers when it starts.
#[tokio::test(flavor = "multi_thread")]
async fn startup_without_daemon_does_not_activate() {
    let dir = std::env::temp_dir().join(format!("hyprv-net-bt-start-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let marker = dir.join("bluez-activated");
    let bus = PrivateBus::with_activatable(BLUEZ, &marker);
    let agent = bus.connect().await;
    let _handle = spawn(ctx(agent));
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert!(!marker.exists(), "startup activated org.bluez");
    let log = Log::default();
    let _bz = fake_bluez(&bus, log.clone()).await;
    wait_for(&log, "default", 1).await;
    assert_eq!(log.count("register"), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

struct LaptopAdapter(Log);
#[zbus::interface(name = "org.bluez.Adapter1")]
impl LaptopAdapter {
    #[zbus(property)]
    fn powered(&self) -> bool {
        self.0.push("read");
        true
    }
    #[zbus(property)]
    fn discovering(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn pairable(&self) -> bool {
        true
    }
    fn start_discovery(&self) {
        self.0.push("start-discovery");
    }
    fn stop_discovery(&self) {
        self.0.push("stop-discovery");
    }
}
async fn laptop_wait(predicate: impl Fn() -> bool) {
    let start = std::time::Instant::now();
    while !predicate() {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "private BlueZ actor stalled"
        );
        tokio::task::yield_now().await;
    }
}
async fn laptop_settle() {
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_millis(30) {
        tokio::task::yield_now().await;
    }
}
#[tokio::test]
async fn laptop_private_bluez_is_passive_and_explicit_discovery_expires_once() {
    // Bluetooth QML has no laptop delta. Exercise the same production actor.
    let bus = PrivateBus::start();
    let log = Log::default();
    let _bluez = bus
        .builder()
        .name(BLUEZ)
        .unwrap()
        .serve_at("/org/bluez", FAgentManager(log.clone()))
        .unwrap()
        .serve_at("/", zbus::fdo::ObjectManager)
        .unwrap()
        .serve_at("/org/bluez/hci0", LaptopAdapter(log.clone()))
        .unwrap()
        .build()
        .await
        .unwrap();
    let context = ctx(bus.connect().await);
    let broker = Broker::new();
    let native = Arc::new(Native::new(&context, broker.clone()).await.unwrap());
    let state = context.store.topic::<StateTopic>(State::default());
    let (actions, rx) = mpsc::channel(32);
    let handle = Handle {
        state: state.clone(),
        prompts: broker.topic(),
        broker: broker.clone(),
        actions,
    };
    tokio::spawn(run(
        native,
        state,
        rx,
        broker,
        false,
        context.runtime.shutdown_signal(),
        false,
    ));
    laptop_wait(|| handle.state.snapshot().value.present).await;
    laptop_settle().await;
    tokio::time::pause();
    let _observer = handle.state.subscribe();
    let reads = log.count("read");
    tokio::time::advance(Duration::from_secs(3600)).await;
    laptop_settle().await;
    assert_eq!(log.count("read"), reads);
    assert_eq!(log.count("start-discovery"), 0);
    let action = handle.dispatch(Action::Scan).await.unwrap();
    laptop_wait(|| log.count("start-discovery") == 1 && !handle.state.snapshot().value.busy).await;
    action.result().await.unwrap();
    laptop_settle().await;
    tokio::time::advance(Duration::from_millis(3999)).await;
    laptop_settle().await;
    assert_eq!(log.count("stop-discovery"), 0);
    tokio::time::advance(Duration::from_millis(2)).await;
    laptop_wait(|| log.count("stop-discovery") == 1 && !handle.state.snapshot().value.busy).await;
    laptop_settle().await;
    let reads = log.count("read");
    tokio::time::advance(Duration::from_secs(3600)).await;
    laptop_settle().await;
    assert_eq!(log.count("start-discovery"), 1);
    assert_eq!(log.count("stop-discovery"), 1);
    assert_eq!(log.count("read"), reads);
    context.runtime.request_shutdown();
    laptop_wait(|| log.count("unregister") == 1).await;
}
