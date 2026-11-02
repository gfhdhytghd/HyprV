//! System-bus event loop of the privileged helper (signals, lease cleanup,
//! desktop-session tracking). Method calls are served by zbus' object server.
use crate::{rapl, FanState, INTERFACE, PATH};
use futures_util::StreamExt;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tokio::sync::{watch, Notify};

pub async fn graphical(connection: &zbus::Connection, uid: u32) -> zbus::Result<bool> {
    let manager = zbus::Proxy::new(
        connection,
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )
    .await?;
    let sessions: Vec<(String, u32, String, String, zbus::zvariant::OwnedObjectPath)> =
        manager.call("ListSessions", &()).await?;
    for (_, owner, _, _, path) in sessions {
        if owner != uid {
            continue;
        }
        let session = zbus::Proxy::new(
            connection,
            "org.freedesktop.login1",
            path,
            "org.freedesktop.login1.Session",
        )
        .await?;
        let kind: String = session.get_property("Type").await?;
        if kind == "wayland" || kind == "x11" {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Upper bound for one logind desktop query; an error or timeout counts as
/// "no graphical session", like any other failed query.
pub const DESKTOP_QUERY_TIMEOUT: Duration = Duration::from_secs(5);

/// Runs until the bus connection is lost (`Ok`, caller reconnects) or a fatal
/// error occurs (`Err`).
///
/// zbus has ONE socket reader per connection, and it blocks while any bounded
/// `MessageStream` of this connection is full. Method replies and incoming
/// method calls are read by that same reader. So nothing in this select loop
/// may await a D-Bus round-trip while the streams below go unpolled: a burst of
/// NameOwnerChanged signals during such an await fills `owners`, the reader
/// stalls before the awaited reply, and the whole connection deadlocks (no
/// method call is ever answered again). The logind query therefore runs as a
/// separate future polled alongside the streams, and refresh requests
/// coalesce through `Notify`.
pub async fn run(
    connection: &zbus::Connection,
    uid: u32,
    desktop: &AtomicBool,
    power: &rapl::Telemetry,
    updates: &mut watch::Receiver<FanState>,
    powercap: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.login1")?
        .build();
    let mut sessions = zbus::MessageStream::for_match_rule(rule, connection, Some(64)).await?;
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.DBus")?
        .member("NameOwnerChanged")?
        .build();
    let mut owners = zbus::MessageStream::for_match_rule(rule, connection, Some(16)).await?;
    // Initial query happens after both subscriptions exist, as before.
    let refresh = Notify::new();
    refresh.notify_one();
    let tracking = async {
        loop {
            refresh.notified().await;
            let graphical =
                tokio::time::timeout(DESKTOP_QUERY_TIMEOUT, graphical(connection, uid)).await;
            desktop.store(matches!(graphical, Ok(Ok(true))), Ordering::Release);
        }
    };
    tokio::pin!(tracking);
    let emitter = zbus::object_server::SignalEmitter::new(connection, PATH)?;
    let (power_updates, mut watts) = watch::channel(rapl::State::default());
    // Kept as a pinned future rather than a detached task: bus loss and
    // service exit synchronously drop all sampling/timer state.
    let sampling = rapl::run(power.clone(), powercap, power_updates);
    tokio::pin!(sampling);

    loop {
        tokio::select! {
            () = &mut tracking => {},
            _=&mut sampling=>return Err("CPU power sampler unexpectedly stopped".into()),
            result=watts.changed()=>{if result.is_err(){break;}let next=watts.borrow_and_update().clone();if emitter.emit(INTERFACE,"CpuPowerChanged",&serde_json::to_string(&next)?).await.is_err(){break;}},
            result=updates.changed()=>{if result.is_err(){return Err("cooling state channel closed".into());}let next=updates.borrow_and_update().clone();if emitter.emit(INTERFACE,"FanStateChanged",&serde_json::to_string(&next)?).await.is_err(){break;}},
            // zbus cannot filter a well-known sender locally, so this stream
            // also sees the daemon's own signals; owner changes of logind are
            // handled by the `owners` arm.
            event=sessions.next()=>{match event{Some(Ok(message))=>{
                if message.header().sender().is_none_or(|s| s != "org.freedesktop.DBus") {refresh.notify_one();}
            },_=>break}},
            event=owners.next()=>{match event{Some(Ok(message))=>{
                let (name,_,new):(String,String,String)=message.body().deserialize()?;
                if name.starts_with(':') && new.is_empty() {power.unsubscribe(&name);}
                if name=="org.freedesktop.login1" {refresh.notify_one();}
            },_=>break}},
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::BufRead,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        },
    };
    use zbus::zvariant::OwnedObjectPath;

    struct Bus(std::process::Child, String);
    impl Bus {
        fn new() -> Self {
            let mut child = std::process::Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--nopidfile", "--print-address=1"])
                .stdout(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            let mut address = String::new();
            std::io::BufReader::new(child.stdout.take().unwrap())
                .read_line(&mut address)
                .unwrap();
            Self(child, address.trim().into())
        }
        async fn connect(&self) -> zbus::Connection {
            zbus::connection::Builder::address(self.1.as_str())
                .unwrap()
                .build()
                .await
                .unwrap()
        }
    }
    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    /// Fake logind whose ListSessions reply is held while the test owns `gate`.
    struct Manager {
        uid: u32,
        gate: Arc<tokio::sync::Mutex<()>>,
        calls: Arc<AtomicUsize>,
    }
    #[zbus::interface(name = "org.freedesktop.login1.Manager")]
    impl Manager {
        async fn list_sessions(&self) -> Vec<(String, u32, String, String, OwnedObjectPath)> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            drop(self.gate.lock().await);
            vec![(
                "1".into(),
                self.uid,
                "wilf".into(),
                "seat0".into(),
                OwnedObjectPath::try_from("/org/freedesktop/login1/session/_31").unwrap(),
            )]
        }
        #[zbus(signal)]
        async fn session_new(
            emitter: &zbus::object_server::SignalEmitter<'_>,
            id: &str,
            path: zbus::zvariant::ObjectPath<'_>,
        ) -> zbus::Result<()>;
    }
    struct Session(Arc<Mutex<String>>);
    #[zbus::interface(name = "org.freedesktop.login1.Session")]
    impl Session {
        #[zbus(property, name = "Type")]
        fn kind(&self) -> String {
            self.0.lock().unwrap().clone()
        }
    }
    struct Probe;
    #[zbus::interface(name = "org.hyprv.Test.Probe")]
    impl Probe {
        fn ping(&self) -> u32 {
            7
        }
    }

    async fn eventually(what: &str, mut check: impl FnMut() -> bool) {
        tokio::time::timeout(Duration::from_secs(3), async {
            while !check() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
    }

    /// Many unique names appear and vanish on the bus; returns once `owners`
    /// saw the last disconnect, i.e. the daemon already queued every one of
    /// these NameOwnerChanged signals for the helper too.
    async fn churn(bus: &Bus, owners: &mut zbus::MessageStream, clients: usize) {
        let mut last = String::new();
        for _ in 0..clients {
            let c = bus.connect().await;
            last = c.unique_name().unwrap().to_string();
            c.close().await.unwrap();
        }
        tokio::time::timeout(Duration::from_secs(3), async {
            while let Some(Ok(m)) = owners.next().await {
                let (name, _, new): (String, String, String) = m.body().deserialize().unwrap();
                if name == last && new.is_empty() {
                    return;
                }
            }
        })
        .await
        .expect("churn not observed");
    }

    async fn helper_answers(client: &zbus::Connection, server: &str) -> bool {
        let probe = zbus::Proxy::new(client, server.to_owned(), PATH, "org.hyprv.Test.Probe")
            .await
            .unwrap();
        matches!(
            tokio::time::timeout(Duration::from_secs(2), probe.call::<_, _, u32>("Ping", &()))
                .await,
            Ok(Ok(7))
        )
    }

    /// Production hang: a burst of bus NameOwnerChanged signals (session crash,
    /// shell restarts) arriving while the loop awaited a logind round-trip
    /// filled a bounded MessageStream; zbus' only socket reader then blocked
    /// forever and never read the reply, silencing every D-Bus method.
    #[tokio::test]
    async fn name_owner_burst_during_logind_query_cannot_wedge_the_bus() {
        let bus = Bus::new();
        let uid = 4242;
        let gate = Arc::new(tokio::sync::Mutex::new(()));
        let calls = Arc::new(AtomicUsize::new(0));
        let kind = Arc::new(Mutex::new("wayland".to_owned()));
        let logind = bus.connect().await;
        logind
            .object_server()
            .at(
                "/org/freedesktop/login1",
                Manager {
                    uid,
                    gate: gate.clone(),
                    calls: calls.clone(),
                },
            )
            .await
            .unwrap();
        logind
            .object_server()
            .at("/org/freedesktop/login1/session/_31", Session(kind.clone()))
            .await
            .unwrap();
        logind.request_name("org.freedesktop.login1").await.unwrap();

        let client = bus.connect().await;
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender("org.freedesktop.DBus")
            .unwrap()
            .member("NameOwnerChanged")
            .unwrap()
            .build();
        let mut owners = zbus::MessageStream::for_match_rule(rule, &client, Some(1024))
            .await
            .unwrap();
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface(INTERFACE)
            .unwrap()
            .member("FanStateChanged")
            .unwrap()
            .build();
        let mut fan_signals = zbus::MessageStream::for_match_rule(rule, &client, Some(16))
            .await
            .unwrap();

        let server = bus.connect().await;
        server.object_server().at(PATH, Probe).await.unwrap();
        let server_name = server.unique_name().unwrap().to_string();
        let desktop = Arc::new(AtomicBool::new(false));
        let power = rapl::Telemetry::default();
        let (fan_tx, mut fan_rx) = watch::channel(FanState::default());
        let held = gate.clone().lock_owned().await;
        let task = tokio::spawn({
            let (desktop, power) = (desktop.clone(), power.clone());
            async move {
                run(
                    &server,
                    uid,
                    &desktop,
                    &power,
                    &mut fan_rx,
                    "/no-such-powercap".into(),
                )
                .await
                .map_err(|e| e.to_string())
            }
        });

        // Phase 1: the startup desktop query is pending while names churn.
        eventually("initial ListSessions", || calls.load(Ordering::SeqCst) >= 1).await;
        churn(&bus, &mut owners, 40).await;
        drop(held);
        assert!(
            helper_answers(&client, &server_name).await,
            "helper stopped answering method calls after a NameOwnerChanged burst"
        );
        eventually("desktop session detected", || {
            desktop.load(Ordering::Acquire)
        })
        .await;

        // Phase 2: a logind signal triggers a refresh; churn while it waits.
        *kind.lock().unwrap() = "tty".into();
        let held = gate.clone().lock_owned().await;
        let before = calls.load(Ordering::SeqCst);
        let path =
            zbus::zvariant::ObjectPath::try_from("/org/freedesktop/login1/session/_31").unwrap();
        let emitter =
            zbus::object_server::SignalEmitter::new(&logind, "/org/freedesktop/login1").unwrap();
        Manager::session_new(&emitter, "1", path).await.unwrap();
        eventually("refresh ListSessions", || {
            calls.load(Ordering::SeqCst) > before
        })
        .await;
        churn(&bus, &mut owners, 40).await;
        drop(held);
        assert!(helper_answers(&client, &server_name).await);
        eventually("desktop session cleared", || {
            !desktop.load(Ordering::Acquire)
        })
        .await;

        // Lease cleanup and signal fan-out still work after both bursts.
        let leaseholder = bus.connect().await;
        let demand = power.demand();
        power.subscribe(leaseholder.unique_name().unwrap().as_str());
        assert!(demand.borrow().0);
        leaseholder.close().await.unwrap();
        eventually("lease released on disconnect", || !demand.borrow().0).await;
        fan_tx.send_replace(FanState {
            chip: "nct6798".into(),
            ..FanState::default()
        });
        let message = tokio::time::timeout(Duration::from_secs(2), fan_signals.next())
            .await
            .expect("FanStateChanged not emitted")
            .unwrap()
            .unwrap();
        let header = message.header();
        assert_eq!(header.path().unwrap().as_str(), PATH);
        let json: String = message.body().deserialize().unwrap();
        let state: FanState = serde_json::from_str(&json).unwrap();
        assert_eq!(state.chip, "nct6798");

        assert!(!task.is_finished(), "{:?}", task.await);
        task.abort();
    }
}
