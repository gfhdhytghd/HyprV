//! Optional laptop telemetry service. No controller, fan/eGPU interface or writes.
use crate::rapl::{self, State, Telemetry};
use futures_util::StreamExt;
use std::{collections::HashMap, path::PathBuf, time::Duration};
use zbus::{fdo, message::Header, zvariant::Value};

pub const BUS: &str = "org.hyprv.Shell.CpuPower";
pub const PATH: &str = "/org/hyprv/Shell/CpuPower";
pub const INTERFACE: &str = "org.hyprv.Shell.CpuPower1";
pub const ACTION: &str = "org.hyprv.shell.cpu-power";
pub const VERSION: u32 = 1;

struct Helper {
    uid: u32,
    telemetry: Telemetry,
}
#[zbus::interface(name = "org.hyprv.Shell.CpuPower1")]
impl Helper {
    #[zbus(property)]
    fn protocol_version(&self) -> u32 {
        VERSION
    }
    async fn subscribe_cpu_power(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let sender = header
            .sender()
            .ok_or_else(|| fdo::Error::AccessDenied("missing sender".into()))?;
        let bus = fdo::DBusProxy::new(connection).await?;
        let uid = bus.get_connection_unix_user(sender.clone().into()).await?;
        if uid != 0 && uid != self.uid {
            return Err(fdo::Error::AccessDenied(
                "caller is not configured owner".into(),
            ));
        }
        let authority = zbus::Proxy::new(
            connection,
            "org.freedesktop.PolicyKit1",
            "/org/freedesktop/PolicyKit1/Authority",
            "org.freedesktop.PolicyKit1.Authority",
        )
        .await
        .map_err(|e| fdo::Error::AccessDenied(e.to_string()))?;
        let mut subject = HashMap::new();
        subject.insert("name", Value::from(sender.as_str()));
        let ((allowed, _, _),): ((bool, bool, HashMap<String, String>),) = authority
            .call(
                "CheckAuthorization",
                &(
                    ("system-bus-name", subject),
                    ACTION,
                    HashMap::<String, String>::new(),
                    0u32,
                    "",
                ),
            )
            .await
            .map_err(|e| fdo::Error::AccessDenied(e.to_string()))?;
        if !allowed {
            return Err(fdo::Error::AccessDenied(
                "active local owner authorization denied".into(),
            ));
        }
        let state = self.telemetry.subscribe(sender.as_str());
        match bus.name_has_owner(sender.clone().into()).await {
            Ok(true) => {
                serde_json::to_string(&state).map_err(|e| fdo::Error::Failed(e.to_string()))
            }
            result => {
                self.telemetry.unsubscribe(sender.as_str());
                Err(fdo::Error::AccessDenied(format!(
                    "caller disconnected: {result:?}"
                )))
            }
        }
    }
    async fn unsubscribe_cpu_power(&self, #[zbus(header)] header: Header<'_>) -> fdo::Result<()> {
        let sender = header
            .sender()
            .ok_or_else(|| fdo::Error::AccessDenied("missing sender".into()))?;
        self.telemetry.unsubscribe(sender.as_str());
        Ok(())
    }
    #[zbus(signal)]
    async fn cpu_power_changed(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        state: &str,
    ) -> zbus::Result<()>;
}

/// Native root capability probe; the first sample reads counters but never writes.
pub fn eligible(root: PathBuf) -> std::io::Result<()> {
    rapl::Sampler::new(root)
        .sample(std::time::Instant::now())
        .map(|_| ())
}
pub fn owner_uid() -> std::io::Result<u32> {
    std::fs::read_to_string("/etc/passwd")?
        .lines()
        .find_map(|line| {
            let fields: Vec<_> = line.split(':').collect();
            (fields.first() == Some(&"wilf"))
                .then(|| fields.get(2)?.parse().ok())
                .flatten()
        })
        .ok_or_else(|| std::io::Error::other("configured owner wilf absent"))
}

/// Injectable transport/hardware for non-root private-bus tests. No test auth bypass.
pub async fn serve(
    connection: zbus::Connection,
    uid: u32,
    root: PathBuf,
    idle: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    let telemetry = Telemetry::default();
    connection
        .object_server()
        .at(
            PATH,
            Helper {
                uid,
                telemetry: telemetry.clone(),
            },
        )
        .await?;
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.DBus")?
        .member("NameOwnerChanged")?
        .build();
    // Register disconnect cleanup BEFORE advertising our well-known name.
    let mut owners = zbus::MessageStream::for_match_rule(rule, &connection, Some(64)).await?;
    connection.request_name(BUS).await?;
    let (updates, mut watts) = tokio::sync::watch::channel(State::default());
    let mut demand = telemetry.demand();
    let sampling = rapl::run(telemetry.clone(), root, updates);
    tokio::pin!(sampling);
    let deadline = tokio::time::sleep(idle);
    tokio::pin!(deadline);
    let emitter = zbus::object_server::SignalEmitter::new(&connection, PATH)?;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    loop {
        tokio::select! {
            _=&mut deadline, if !demand.borrow().0 => break,
            changed=demand.changed()=>{
                changed?;
                deadline.as_mut().reset(tokio::time::Instant::now()+idle);
            },
            _=&mut sampling=>return Err("CPU-only sampler stopped".into()),
            changed=watts.changed()=>{
                changed?;
                let json=serde_json::to_string(&*watts.borrow_and_update())?;
                Helper::cpu_power_changed(&emitter,&json).await?;
            },
            event=owners.next()=>match event {
                Some(Ok(message))=>{
                    let (name,_,new):(String,String,String)=message.body().deserialize()?;
                    if name.starts_with(':') && new.is_empty() { telemetry.unsubscribe(&name); }
                },
                _=>return Err("CPU-only bus disconnected".into())
            },
            _=terminate.recv()=>break,
        }
    }
    telemetry.clear();
    connection.release_name(BUS).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;
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
    struct Policy(bool);
    #[zbus::interface(name = "org.freedesktop.PolicyKit1.Authority")]
    impl Policy {
        fn check_authorization(
            &self,
            subject: (String, HashMap<String, zbus::zvariant::OwnedValue>),
            action: &str,
            _details: HashMap<String, String>,
            flags: u32,
            _cancel: &str,
        ) -> (bool, bool, HashMap<String, String>) {
            assert_eq!(subject.0, "system-bus-name");
            assert!(subject.1.contains_key("name"));
            assert_eq!(action, ACTION);
            assert_eq!(flags, 0, "telemetry must not prompt");
            (self.0, false, HashMap::new())
        }
    }
    async fn wait_name(client: &zbus::Connection) {
        let p = fdo::DBusProxy::new(client).await.unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while !p.name_has_owner(BUS.try_into().unwrap()).await.unwrap() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn cpu_only_interface_denies_mutators_and_denied_authorization() {
        let bus = Bus::new();
        let auth = bus.connect().await;
        auth.object_server()
            .at("/org/freedesktop/PolicyKit1/Authority", Policy(false))
            .await
            .unwrap();
        auth.request_name("org.freedesktop.PolicyKit1")
            .await
            .unwrap();
        let server = bus.connect().await;
        let client = bus.connect().await;
        let task = tokio::spawn(async move {
            serve(
                server,
                unsafe { libc::geteuid() },
                "/no-such-fixture".into(),
                Duration::from_millis(300),
            )
            .await
            .map_err(|e| e.to_string())
        });
        wait_name(&client).await;
        let p = zbus::Proxy::new(&client, BUS, PATH, INTERFACE)
            .await
            .unwrap();
        assert!(p
            .call::<_, _, String>("SubscribeCpuPower", &())
            .await
            .is_err());
        for method in ["GetFans", "SetFanLimit", "RemoveExternalGpu"] {
            assert!(p.call::<_, _, String>(method, &()).await.is_err());
        }
        let introspect =
            zbus::Proxy::new(&client, BUS, PATH, "org.freedesktop.DBus.Introspectable")
                .await
                .unwrap();
        let xml: String = introspect.call("Introspect", &()).await.unwrap();
        assert!(xml.contains(INTERFACE));
        assert!(!xml.contains("SetFanLimit"));
        assert!(!xml.contains("RemoveExternalGpu"));
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
    #[tokio::test]
    async fn owner_disconnect_releases_lease_and_service_exits_idle() {
        let bus = Bus::new();
        let auth = bus.connect().await;
        auth.object_server()
            .at("/org/freedesktop/PolicyKit1/Authority", Policy(true))
            .await
            .unwrap();
        auth.request_name("org.freedesktop.PolicyKit1")
            .await
            .unwrap();
        let server = bus.connect().await;
        let client = bus.connect().await;
        let task = tokio::spawn(async move {
            serve(
                server,
                unsafe { libc::geteuid() },
                "/no-such-fixture".into(),
                Duration::from_millis(120),
            )
            .await
            .map_err(|e| e.to_string())
        });
        wait_name(&client).await;
        let p = zbus::Proxy::new(&client, BUS, PATH, INTERFACE)
            .await
            .unwrap();
        let json: String = p.call("SubscribeCpuPower", &()).await.unwrap();
        let state: State = serde_json::from_str(&json).unwrap();
        assert!(state.watts.is_none());
        tokio::time::sleep(Duration::from_millis(180)).await;
        assert!(!task.is_finished(), "live lease prevents idle exit");
        drop(p);
        client.close().await.unwrap();
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
    #[test]
    fn absent_counters_are_ineligible_without_writes() {
        assert!(eligible("/no-such-fixture".into()).is_err());
    }
}
