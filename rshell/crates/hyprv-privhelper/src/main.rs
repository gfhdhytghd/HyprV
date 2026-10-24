use futures_util::StreamExt;
use hyprv_privhelper::{cooling::Controller, events::KernelEvents, FanChannel, BUS, PATH, VERSION};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use zbus::{fdo, message::Header, zvariant::Value};
mod watchdog;
const ACTION: &str = "org.hyprv.shell.fan-control";
const POWER_ACTION: &str = "org.hyprv.shell.cpu-power";
const EGPU_ACTION: &str = "org.hyprv.shell.egpu-remove";
struct Helper {
    controller: Arc<tokio::sync::Mutex<Controller>>,
    desktop: Arc<AtomicBool>,
    uid: u32,
    power: hyprv_privhelper::rapl::Telemetry,
}
fn validate(channel: &str, percent: u32) -> fdo::Result<u8> {
    let channel = FanChannel::parse(channel)
        .ok_or_else(|| fdo::Error::InvalidArgs("channel must be chassis or pump".into()))?;
    if percent > 100 {
        return Err(fdo::Error::InvalidArgs("percent must be 0..100".into()));
    }
    Ok(channel.index())
}
async fn authorize(
    connection: &zbus::Connection,
    header: &Header<'_>,
    action: &str,
    flags: u32,
) -> fdo::Result<()> {
    let sender = header
        .sender()
        .ok_or_else(|| fdo::Error::AccessDenied("missing unique sender".into()))?;
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
                action,
                HashMap::<String, String>::new(),
                flags,
                "",
            ),
        )
        .await
        .map_err(|e| fdo::Error::AccessDenied(e.to_string()))?;
    if allowed {
        Ok(())
    } else {
        Err(fdo::Error::AccessDenied(
            "polkit authorization denied".into(),
        ))
    }
}

#[zbus::interface(name = "org.hyprv.Shell.PrivHelper1")]
impl Helper {
    #[zbus(property)]
    fn protocol_version(&self) -> u32 {
        VERSION
    }
    /// One idempotent lease per authenticated unique bus owner. No paths or
    /// sampling cadence can be supplied by the caller.
    async fn subscribe_cpu_power(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let sender = header
            .sender()
            .ok_or_else(|| fdo::Error::AccessDenied("missing sender".into()))?;
        let bus = zbus::fdo::DBusProxy::new(connection)
            .await
            .map_err(|e| fdo::Error::Failed(e.to_string()))?;
        let uid = bus.get_connection_unix_user(sender.clone().into()).await?;
        if uid != 0 && uid != self.uid {
            return Err(fdo::Error::AccessDenied(
                "caller is not configured desktop owner".into(),
            ));
        }
        // Telemetry must never open an authentication dialog on a timer.
        authorize(connection, &header, POWER_ACTION, 0).await?;
        // Install before the final liveness query. If the caller vanished
        // while polkit was pending, roll back; if it vanishes after the query,
        // NameOwnerChanged necessarily observes an already-installed lease.
        let state = self.power.subscribe(sender.as_str());
        match bus.name_has_owner(sender.clone().into()).await {
            Ok(true) => {
                serde_json::to_string(&state).map_err(|e| fdo::Error::Failed(e.to_string()))
            }
            result => {
                self.power.unsubscribe(sender.as_str());
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
        self.power.unsubscribe(sender.as_str());
        Ok(())
    }
    #[zbus(signal)]
    async fn cpu_power_changed(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        state: &str,
    ) -> zbus::Result<()>;
    async fn get_fans(&self) -> fdo::Result<String> {
        let state = self
            .controller
            .lock()
            .await
            .read(self.desktop.load(Ordering::Acquire))
            .map_err(|e| fdo::Error::Failed(e.to_string()))?;
        serde_json::to_string(&state).map_err(|e| fdo::Error::Failed(e.to_string()))
    }
    async fn set_fan_limit(
        &self,
        channel: &str,
        percent: u32,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: zbus::object_server::SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        validate(channel, percent)?;
        let sender = header
            .sender()
            .ok_or_else(|| fdo::Error::AccessDenied("missing sender".into()))?;
        let bus = zbus::fdo::DBusProxy::new(connection)
            .await
            .map_err(|e| fdo::Error::Failed(e.to_string()))?;
        let uid = bus.get_connection_unix_user(sender.clone().into()).await?;
        if uid != 0 && uid != self.uid {
            return Err(fdo::Error::AccessDenied(
                "caller is not configured desktop owner".into(),
            ));
        }
        authorize(connection, &header, ACTION, 1).await?;
        // No stale authorization across device replacement: tick rechecks chip identity.
        let state = self
            .controller
            .lock()
            .await
            .set_limit(
                FanChannel::parse(channel).unwrap(),
                percent as u8,
                self.desktop.load(Ordering::Acquire),
            )
            .map_err(|e| fdo::Error::Failed(e.to_string()))?;
        let json = serde_json::to_string(&state).map_err(|e| fdo::Error::Failed(e.to_string()))?;
        Self::fan_state_changed(&emitter, &json)
            .await
            .map_err(|e| fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }
    /// Remove a proven-external GPU and its slot/bridge chain. Only a PCI
    /// address token crosses the bus; everything else is re-derived from sysfs.
    async fn remove_external_gpu(
        &self,
        bdf: &str,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<Vec<(String, String)>> {
        let token = hyprv_privhelper::egpu::validate_bdf(bdf)
            .ok_or_else(|| fdo::Error::InvalidArgs("bdf must be DDDD:BB:DD.F".into()))?;
        let sender = header
            .sender()
            .ok_or_else(|| fdo::Error::AccessDenied("missing sender".into()))?;
        let bus = zbus::fdo::DBusProxy::new(connection)
            .await
            .map_err(|e| fdo::Error::Failed(e.to_string()))?;
        let uid = bus.get_connection_unix_user(sender.clone().into()).await?;
        if uid != 0 && uid != self.uid {
            return Err(fdo::Error::AccessDenied(
                "caller is not configured desktop owner".into(),
            ));
        }
        // Validate before prompting, then revalidate after authorization.
        let sys = hyprv_core::profile::sys_path("sys");
        hyprv_privhelper::egpu::plan(&sys, &token).map_err(fdo::Error::InvalidArgs)?;
        authorize(connection, &header, EGPU_ACTION, 1).await?;
        tokio::task::spawn_blocking(move || {
            let chain = hyprv_privhelper::egpu::plan(&sys, &token).map_err(fdo::Error::Failed)?;
            Ok(hyprv_privhelper::egpu::remove(&sys, &chain))
        })
        .await
        .map_err(|e| fdo::Error::Failed(e.to_string()))?
    }
    #[zbus(signal)]
    async fn fan_state_changed(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        state: &str,
    ) -> zbus::Result<()>;
}
async fn graphical(connection: &zbus::Connection, uid: u32) -> zbus::Result<bool> {
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
async fn safety_loop(
    controller: Arc<tokio::sync::Mutex<Controller>>,
    desktop: Arc<AtomicBool>,
    state: tokio::sync::watch::Sender<hyprv_privhelper::FanState>,
    udev: KernelEvents,
) -> std::io::Result<()> {
    let mut timer = tokio::time::interval(Duration::from_secs(1));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        let available = controller.lock().await.available();
        tokio::select! {
            _=timer.tick(),if available || watchdog::enabled()=>(),
            result=udev.subsystem(&["hwmon"])=>{result?;controller.lock().await.rediscover();},
        }
        let result = controller
            .lock()
            .await
            .tick(desktop.load(Ordering::Acquire));
        let next = match result {
            Ok(next) => next,
            Err(e) => {
                let _ = controller.lock().await.fail_safe();
                return Err(e);
            }
        };
        watchdog::notify("WATCHDOG=1")?;
        if *state.borrow() != next {
            state.send_replace(next);
        }
    }
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    hyprv_privhelper::eligibility::trusted_environment()?;
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args == ["--eligible-cooling"] {
        std::process::exit(if hyprv_privhelper::eligibility::cooling() {
            0
        } else {
            1
        });
    }
    if !hyprv_privhelper::eligibility::cooling() {
        eprintln!("cooling inactive: laptop profile");
        return Ok(());
    }
    if args == ["--check"] {
        hyprv_privhelper::cooling::check(&hyprv_core::profile::sys_path("sys/class/hwmon"))?;
        println!("helper cooling capability v2: single-store manual-enable then PWM 255");
        return Ok(());
    }
    if unsafe { libc::geteuid() } != 0 {
        return Err("hyprv-privhelper requires root system activation".into());
    }
    hyprv_privhelper::eligibility::guarded_failsafe()?;
    let uid: u32 = std::fs::read_to_string("/etc/passwd")?
        .lines()
        .find_map(|line| {
            let f: Vec<_> = line.split(':').collect();
            (f.first() == Some(&"wilf"))
                .then(|| f.get(2)?.parse().ok())
                .flatten()
        })
        .ok_or("desktop owner wilf absent")?;
    let mut controller = Controller::open(
        hyprv_core::profile::sys_path("sys/class/hwmon"),
        std::path::Path::new(hyprv_privhelper::cooling::STATE_PATH),
    )?;
    // Establish boot cooling before trying D-Bus: control survives absent/restarting bus.
    let initial = controller.tick(false)?;
    let controller = Arc::new(tokio::sync::Mutex::new(controller));
    let desktop = Arc::new(AtomicBool::new(false));
    let (state, mut updates) = tokio::sync::watch::channel(initial);
    let udev = KernelEvents::udev()?;
    let mut safety = tokio::spawn(safety_loop(
        controller.clone(),
        desktop.clone(),
        state,
        udev,
    ));
    watchdog::notify("READY=1")?;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut backoff = Duration::from_millis(250);
    'service: loop {
        let power = hyprv_privhelper::rapl::Telemetry::default();
        let build = async {
            zbus::connection::Builder::system()?
                .name(BUS)?
                .serve_at(
                    PATH,
                    Helper {
                        controller: controller.clone(),
                        desktop: desktop.clone(),
                        uid,
                        power: power.clone(),
                    },
                )?
                .build()
                .await
        };
        let connection = tokio::select! {
            result=build=>match result{Ok(c)=>c,Err(e)=>{
                eprintln!("helper bus unavailable: {e}; cooling remains active");
                tokio::select!{_=tokio::time::sleep(backoff)=>(),_=terminate.recv()=>break 'service,_=tokio::signal::ctrl_c()=>break 'service,result=&mut safety=>return Err(format!("cooling task stopped: {result:?}").into())}
                backoff=(backoff*2).min(Duration::from_secs(30));continue;
            }},
            result=&mut safety=>return Err(format!("cooling task stopped: {result:?}").into()),
            _=terminate.recv()=>break,
            _=tokio::signal::ctrl_c()=>break,
        };
        backoff = Duration::from_millis(250);
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender("org.freedesktop.login1")?
            .build();
        let mut sessions = zbus::MessageStream::for_match_rule(rule, &connection, Some(64)).await?;
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender("org.freedesktop.DBus")?
            .member("NameOwnerChanged")?
            .build();
        let mut owners = zbus::MessageStream::for_match_rule(rule, &connection, Some(16)).await?;
        desktop.store(
            graphical(&connection, uid).await.unwrap_or(false),
            Ordering::Release,
        );
        let emitter = zbus::object_server::SignalEmitter::new(&connection, PATH)?;
        let (power_updates, mut watts) =
            tokio::sync::watch::channel(hyprv_privhelper::rapl::State::default());
        // Kept as a pinned future rather than a detached task: bus loss and
        // service exit synchronously drop all sampling/timer state.
        let sampling = hyprv_privhelper::rapl::run(
            power.clone(),
            hyprv_core::profile::sys_path("sys/class/powercap"),
            power_updates,
        );
        tokio::pin!(sampling);

        loop {
            tokio::select! {
                _=&mut sampling=>return Err("CPU power sampler unexpectedly stopped".into()),
                result=watts.changed()=>{if result.is_err(){break;}let next=watts.borrow_and_update().clone();if Helper::cpu_power_changed(&emitter,&serde_json::to_string(&next)?).await.is_err(){break;}},
                result=&mut safety=>return Err(format!("cooling task stopped: {result:?}").into()),
                result=updates.changed()=>{if result.is_err(){return Err("cooling state channel closed".into());}let next=updates.borrow_and_update().clone();if Helper::fan_state_changed(&emitter,&serde_json::to_string(&next)?).await.is_err(){break;}},
                event=sessions.next()=>{match event{Some(Ok(_))=>desktop.store(graphical(&connection,uid).await.unwrap_or(false),Ordering::Release),_=>break}},
                event=owners.next()=>{match event{Some(Ok(message))=>{
                    let (name,_,new):(String,String,String)=message.body().deserialize()?;
                    if name.starts_with(':') && new.is_empty() {power.unsubscribe(&name);}
                    if name=="org.freedesktop.login1" {desktop.store(graphical(&connection,uid).await.unwrap_or(false),Ordering::Release);}
                },_=>break}},
                _=terminate.recv()=>break 'service,
                _=tokio::signal::ctrl_c()=>break 'service,
            }
        }
        power.clear();
        desktop.store(false, Ordering::Release);
    }
    safety.abort();
    let _ = safety.await;
    // Do not depend on the final D-Bus object reference being released first.
    controller.lock().await.fail_safe()?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn allowlist_is_strict() {
        assert_eq!(validate("chassis", 0).unwrap(), 1);
        assert_eq!(validate("pump", 100).unwrap(), 7);
        assert!(validate("cpu", 20).is_err());
        assert!(validate("chassis", 101).is_err());
    }
    #[test]
    fn polkit_reply_has_one_struct_output() {
        let message = zbus::Message::method_call("/org/test", "Reply")
            .unwrap()
            .build(&((true, false, HashMap::<String, String>::new()),))
            .unwrap();
        let ((allowed, challenge, _),): ((bool, bool, HashMap<String, String>),) =
            message.body().deserialize().unwrap();
        assert!(allowed);
        assert!(!challenge);
    }
}
