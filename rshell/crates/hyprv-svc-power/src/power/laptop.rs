//! Signals mutate a local property cache. Snapshot never performs provider IO.
use super::*;
use std::collections::HashMap;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
type Properties = HashMap<String, OwnedValue>;
type CacheKey = (String, String, String, String);
type CacheFloors = std::sync::Mutex<HashMap<CacheKey, zbus::message::Sequence>>;
const UPOWER: &str = "org.freedesktop.UPower";
const ROOT: &str = "/org/freedesktop/UPower";
const DEVICE: &str = "org.freedesktop.UPower.Device";
const LOGIN: &str = "org.freedesktop.login1";
const LOGIN_PATH: &str = "/org/freedesktop/login1";
const LOGIN_IFACE: &str = "org.freedesktop.login1.Manager";
const MODERN: (&str, &str) = (
    "org.freedesktop.UPower.PowerProfiles",
    "/org/freedesktop/UPower/PowerProfiles",
);
const OLD: (&str, &str) = ("net.hadess.PowerProfiles", "/net/hadess/PowerProfiles");

pub(super) struct Native {
    ctx: Context,
    floors: CacheFloors,
    root: Properties,
    device: Properties,
    profile: Properties,
    device_path: Option<String>,
    provider: Option<(&'static str, &'static str)>,
    upower: Option<actor::SignalInbox>,
    profiles: Option<actor::SignalInbox>,
    login: Option<actor::SignalInbox>,
    fallback_lid: Option<bool>,
    reported_upower_missing: bool,
    reported_profiles_missing: bool,
}
impl Native {
    pub fn new(ctx: Context) -> Self {
        Self {
            ctx,
            floors: std::sync::Mutex::default(),
            root: Properties::new(),
            device: Properties::new(),
            profile: Properties::new(),
            device_path: None,
            provider: None,
            upower: None,
            profiles: None,
            login: None,
            fallback_lid: None,
            reported_upower_missing: false,
            reported_profiles_missing: false,
        }
    }
    async fn get_all(&self, bus: &str, path: &str, interface: &str) -> Result<Properties> {
        let p = actor::proxy(&self.ctx.bus, bus, path, "org.freedesktop.DBus.Properties").await?;
        let reply = p.call_method("GetAll", &(interface,)).await?;
        self.floors.lock().unwrap().insert(
            (bus.into(), path.into(), interface.into(), String::new()),
            reply.recv_position(),
        );
        Ok(reply.body().deserialize()?)
    }
    fn accepts(
        &self,
        message: &zbus::Message,
        bus: &str,
        path: &str,
        interface: &str,
        property: &str,
    ) -> bool {
        let floors = self.floors.lock().unwrap();
        ["", property].iter().all(|property| {
            floors
                .get(&(
                    bus.into(),
                    path.into(),
                    interface.into(),
                    (*property).into(),
                ))
                .is_none_or(|floor| message.recv_position() > *floor)
        })
    }
    async fn display(&mut self) -> Result<()> {
        self.device.clear();
        self.device_path = None;
        let p = actor::proxy(&self.ctx.bus, UPOWER, ROOT, UPOWER).await?;
        let path: OwnedObjectPath = p.call("GetDisplayDevice", &()).await?;
        self.device = self.get_all(UPOWER, path.as_str(), DEVICE).await?;
        self.device_path = Some(path.to_string());
        Ok(())
    }
    async fn changed(
        &self,
        message: &zbus::Message,
        bus: &str,
        path: &str,
        interface: &str,
    ) -> Result<Option<Properties>> {
        if message.header().path().is_none_or(|p| p.as_str() != path)
            || message
                .header()
                .interface()
                .is_none_or(|i| i.as_str() != "org.freedesktop.DBus.Properties")
            || message
                .header()
                .member()
                .is_none_or(|m| m.as_str() != "PropertiesChanged")
        {
            return Ok(None);
        }
        let (iface, mut changed, invalid): (String, Properties, Vec<String>) =
            message.body().deserialize()?;
        if iface != interface {
            return Ok(None);
        }
        changed.retain(|name, _| self.accepts(message, bus, path, interface, name));
        let invalid: Vec<_> = invalid
            .into_iter()
            .filter(|name| self.accepts(message, bus, path, interface, name))
            .collect();
        if changed.is_empty() && invalid.is_empty() {
            return Ok(None);
        }
        if !invalid.is_empty() {
            let p =
                actor::proxy(&self.ctx.bus, bus, path, "org.freedesktop.DBus.Properties").await?;
            for name in invalid {
                let reply = p.call_method("Get", &(interface, &name)).await?;
                let value: OwnedValue = reply.body().deserialize()?;
                self.floors.lock().unwrap().insert(
                    (bus.into(), path.into(), interface.into(), name.clone()),
                    reply.recv_position(),
                );
                changed.insert(name, value);
            }
        }
        Ok(Some(changed))
    }
    fn project(&self) -> State {
        let on_battery = boolean(&self.root, "OnBattery").unwrap_or(false);
        let present = boolean(&self.device, "IsPresent") == Some(true)
            && boolean(&self.device, "PowerSupply") == Some(true)
            && integer(&self.device, "Type") == Some(2);
        State {
            available: self.root.contains_key("OnBattery"),
            on_battery,
            battery_present: present,
            battery_percent: present
                .then(|| real(&self.device, "Percentage").map(|p| p.clamp(0., 100.)))
                .flatten(),
            battery_mode: battery_mode(
                present,
                integer(&self.device, "State").unwrap_or(0),
                on_battery,
            ),
            lid_present: boolean(&self.root, "LidIsPresent"),
            lid_closed: boolean(&self.root, "LidIsClosed").or(self.fallback_lid),
            sleeping: *self.ctx.sleeping.borrow(),
            profile: string(&self.profile, "ActiveProfile")
                .map(PowerProfile::parse)
                .unwrap_or_default(),
            profiles: self
                .profile
                .get("Profiles")
                .and_then(|p| {
                    <Vec<HashMap<String, OwnedValue>>>::try_from(p.try_clone().ok()?).ok()
                })
                .unwrap_or_default()
                .iter()
                .filter_map(|p| string(p, "Profile").map(PowerProfile::parse))
                .collect(),
            performance_degraded: string(&self.profile, "PerformanceDegraded")
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
            energy_now_wh: present.then(|| real(&self.device, "Energy")).flatten(),
            energy_full_wh: present.then(|| real(&self.device, "EnergyFull")).flatten(),
            energy_rate_w: present.then(|| real(&self.device, "EnergyRate")).flatten(),
            time_to_empty_s: present
                .then(|| signed(&self.device, "TimeToEmpty"))
                .flatten(),
            time_to_full_s: present
                .then(|| signed(&self.device, "TimeToFull"))
                .flatten(),
        }
    }
}
fn boolean(p: &Properties, k: &str) -> Option<bool> {
    bool::try_from(p.get(k)?).ok()
}
fn integer(p: &Properties, k: &str) -> Option<u32> {
    u32::try_from(p.get(k)?).ok()
}
fn signed(p: &Properties, k: &str) -> Option<i64> {
    i64::try_from(p.get(k)?).ok()
}
fn real(p: &Properties, k: &str) -> Option<f64> {
    f64::try_from(p.get(k)?).ok().filter(|n| n.is_finite())
}
fn string<'a>(p: &'a Properties, k: &str) -> Option<&'a str> {
    <&str>::try_from(p.get(k)?).ok()
}
impl Backend for Native {
    type State = State;
    type Action = Action;
    fn providers(&self) -> &'static [&'static str] {
        &[UPOWER, LOGIN, MODERN.0, OLD.0]
    }
    async fn connect(&mut self) -> Result<()> {
        self.floors.lock().unwrap().clear();
        self.root.clear();
        self.device.clear();
        self.profile.clear();
        self.device_path = None;
        self.provider = None;
        self.fallback_lid = None;
        self.upower = Some(actor::signals(&self.ctx.bus, UPOWER).await?);
        self.login = Some(actor::signals(&self.ctx.bus, LOGIN).await?);
        self.profiles = None;
        if actor::has_owner(&self.ctx.bus, UPOWER).await? {
            match self.get_all(UPOWER, ROOT, UPOWER).await {
                Ok(p) => {
                    self.root = p;
                    if let Err(e) = self.display().await {
                        eprintln!("laptop UPower display unavailable: {e}");
                    }
                }
                Err(e) => eprintln!("laptop UPower unavailable: {e}"),
            }
        }
        if actor::has_owner(&self.ctx.bus, LOGIN).await? {
            if let Ok(p) = self.get_all(LOGIN, LOGIN_PATH, LOGIN_IFACE).await {
                self.fallback_lid = boolean(&p, "LidClosed");
                if let Some(sleep) = boolean(&p, "PreparingForSleep") {
                    self.ctx.sleeping.send_replace(sleep);
                }
            }
        }
        for (name, path) in [MODERN, OLD] {
            if !actor::has_owner(&self.ctx.bus, name).await? {
                continue;
            }
            // Subscribe before snapshot. No reads of the other profile provider
            // on battery, lid or sleep events.
            self.profiles = Some(
                actor::signals_member(
                    &self.ctx.bus,
                    name,
                    path,
                    "org.freedesktop.DBus.Properties",
                    "PropertiesChanged",
                )
                .await?,
            );
            if let Ok(p) = self.get_all(name, path, name).await {
                self.profile = p;
                self.provider = Some((name, path));
                break;
            }
            self.profiles = None;
        }
        let missing = !self.root.contains_key("OnBattery");
        if missing != self.reported_upower_missing {
            eprintln!(
                "laptop UPower {}",
                if missing {
                    "unavailable: no usable provider snapshot; waiting for owner/device events"
                } else {
                    "source recovered"
                }
            );
            self.reported_upower_missing = missing;
        }
        let missing = self.provider.is_none();
        if missing != self.reported_profiles_missing {
            eprintln!(
                "laptop power profiles {}",
                if missing {
                    "unavailable: no usable provider; waiting for owner events"
                } else {
                    "source recovered"
                }
            );
            self.reported_profiles_missing = missing;
        }
        Ok(())
    }
    async fn snapshot(&mut self) -> Result<State> {
        Ok(self.project())
    }
    async fn apply(&mut self, action: Action) -> Result<()> {
        match action {
            Action::Refresh => self.connect().await,
            Action::Suspend => {
                actor::proxy(&self.ctx.bus, LOGIN, LOGIN_PATH, LOGIN_IFACE)
                    .await?
                    .call::<_, _, ()>("Suspend", &(true,))
                    .await?;
                Ok(())
            }
            action => {
                let state = self.project();
                let target = match action {
                    Action::ProfileSet(p) => p,
                    Action::ProfileToggle => next_profile(&state.profile),
                    Action::ProfileCyclePanel => next_panel_profile(&state.profile),
                    _ => unreachable!(),
                };
                if !state.profiles.contains(&target) {
                    return Err("profile not offered by provider".into());
                }
                let (name, path) = self.provider.ok_or("power profile provider absent")?;
                actor::proxy(&self.ctx.bus, name, path, name)
                    .await?
                    .set_property("ActiveProfile", target.name())
                    .await?;
                // Confirm accepted value once; PropertiesChanged also reconciles
                // external/held profile changes. No post-action polling timer.
                let p = actor::proxy(&self.ctx.bus, name, path, "org.freedesktop.DBus.Properties")
                    .await?;
                let reply = p.call_method("Get", &(name, "ActiveProfile")).await?;
                let value: OwnedValue = reply.body().deserialize()?;
                self.floors.lock().unwrap().insert(
                    (
                        name.into(),
                        path.into(),
                        name.into(),
                        "ActiveProfile".into(),
                    ),
                    reply.recv_position(),
                );
                self.profile.insert("ActiveProfile".into(), value);
                Ok(())
            }
        }
    }
    async fn event(&mut self) -> Result<()> {
        loop {
            let (source, message) = tokio::select! {
                m=receive(&mut self.upower)=>(0,m?),
                m=receive(&mut self.profiles)=>(1,m?),
                m=receive(&mut self.login)=>(2,m?),
            };
            match source {
                0 => {
                    if let Some(p) = self.changed(&message, UPOWER, ROOT, UPOWER).await? {
                        self.root.extend(p);
                        return Ok(());
                    }
                    if let Some(path) = &self.device_path {
                        if let Some(p) = self.changed(&message, UPOWER, path, DEVICE).await? {
                            self.device.extend(p);
                            return Ok(());
                        }
                    }
                    if message.header().path().is_some_and(|p| p.as_str() == ROOT)
                        && message
                            .header()
                            .interface()
                            .is_some_and(|i| i.as_str() == UPOWER)
                        && message
                            .header()
                            .member()
                            .is_some_and(|m| matches!(m.as_str(), "DeviceAdded" | "DeviceRemoved"))
                    {
                        self.display().await?;
                        return Ok(());
                    }
                }
                1 => {
                    if let Some((name, path)) = self.provider {
                        if let Some(p) = self.changed(&message, name, path, name).await? {
                            self.profile.extend(p);
                            return Ok(());
                        }
                    }
                }
                _ => {
                    if message
                        .header()
                        .path()
                        .is_some_and(|p| p.as_str() == LOGIN_PATH)
                        && message
                            .header()
                            .interface()
                            .is_some_and(|i| i.as_str() == LOGIN_IFACE)
                        && message
                            .header()
                            .member()
                            .is_some_and(|m| m.as_str() == "PrepareForSleep")
                    {
                        if !self.accepts(
                            &message,
                            LOGIN,
                            LOGIN_PATH,
                            LOGIN_IFACE,
                            "PreparingForSleep",
                        ) {
                            continue;
                        }
                        let (sleep,): (bool,) = message.body().deserialize()?;
                        self.ctx.sleeping.send_replace(sleep);
                        return Ok(());
                    }
                    if let Some(p) = self
                        .changed(&message, LOGIN, LOGIN_PATH, LOGIN_IFACE)
                        .await?
                    {
                        if let Some(lid) = boolean(&p, "LidClosed") {
                            self.fallback_lid = Some(lid);
                            return Ok(());
                        }
                    }
                }
            }
        }
    }
}
async fn receive(stream: &mut Option<actor::SignalInbox>) -> Result<zbus::Message> {
    match stream.as_mut() {
        Some(s) => Ok(s.next().await.ok_or("power provider disconnected")??),
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{context, Bus, Tree};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    struct Root(Arc<AtomicUsize>);
    #[zbus::interface(name = "org.freedesktop.UPower")]
    impl Root {
        fn get_display_device(&self) -> OwnedObjectPath {
            self.0.fetch_add(1, Ordering::SeqCst);
            OwnedObjectPath::try_from("/org/freedesktop/UPower/devices/DisplayDevice").unwrap()
        }
        #[zbus(property)]
        fn on_battery(&self) -> bool {
            self.0.fetch_add(1, Ordering::SeqCst);
            true
        }
        #[zbus(property)]
        fn lid_is_present(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn lid_is_closed(&self) -> bool {
            false
        }
    }
    struct Device;
    #[zbus::interface(name = "org.freedesktop.UPower.Device")]
    impl Device {
        #[zbus(property)]
        fn is_present(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn power_supply(&self) -> bool {
            true
        }
        #[zbus(property, name = "Type")]
        fn kind(&self) -> u32 {
            2
        }
        #[zbus(property)]
        fn state(&self) -> u32 {
            2
        }
        #[zbus(property)]
        fn percentage(&self) -> f64 {
            20.1
        }
    }
    // Explicit PropertiesChanged fixtures make invalidation/state and API call
    // counters independent of zbus property caching in a fake UI client.
    #[tokio::test]
    async fn signals_change_cache_without_refresh_and_lid_never_acts() {
        let bus = Bus::new();
        let server = bus.connection().await;
        let client = bus.connection().await;
        server.request_name(UPOWER).await.unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        server
            .object_server()
            .at(ROOT, Root(calls.clone()))
            .await
            .unwrap();
        server
            .object_server()
            .at("/org/freedesktop/UPower/devices/DisplayDevice", Device)
            .await
            .unwrap();
        let tree = Tree::new();
        let mut native = Native::new(context(client, tree.paths(), None));
        native.connect().await.unwrap();
        let initial = native.snapshot().await.unwrap();
        assert!(initial.battery_present);
        assert_eq!(initial.battery_percent, Some(20.1));
        assert_eq!(initial.lid_present, Some(true));
        assert_eq!(initial.lid_closed, Some(false));
        let before = calls.load(Ordering::SeqCst);
        for _ in 0..1000 {
            native.snapshot().await.unwrap();
        }
        let changed = HashMap::from([
            ("Percentage", OwnedValue::from(19.9f64)),
            ("State", OwnedValue::from(5u32)),
        ]);
        server
            .emit_signal(
                None::<&str>,
                native.device_path.as_deref().unwrap(),
                "org.freedesktop.DBus.Properties",
                "PropertiesChanged",
                &(DEVICE, changed, Vec::<String>::new()),
            )
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), native.event())
            .await
            .unwrap()
            .unwrap();
        let state = native.snapshot().await.unwrap();
        assert_eq!(state.battery_percent, Some(19.9));
        assert_eq!(state.battery_mode, BatteryMode::Charging);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            before,
            "battery signal must not snapshot providers"
        );
        native.device.insert("PowerSupply".into(), false.into());
        assert!(!native.project().battery_present);
        native.device.insert("PowerSupply".into(), true.into());
        native.device.insert("Type".into(), 3u32.into());
        assert!(!native.project().battery_present);
        native.root.insert("LidIsClosed".into(), true.into());
        assert_eq!(native.project().lid_closed, Some(true));
        assert!(native.apply(Action::ProfileCyclePanel).await.is_err());
    }
    #[test]
    fn panel_and_cli_orders_are_distinct() {
        assert_eq!(
            next_panel_profile(&PowerProfile::Balanced),
            PowerProfile::Performance
        );
        assert_eq!(
            next_profile(&PowerProfile::Balanced),
            PowerProfile::PowerSaver
        );
        assert_eq!(
            next_panel_profile(&PowerProfile::Other("unknown".into())),
            PowerProfile::Balanced
        );
    }
}

#[cfg(test)]
mod profile_bus_tests {
    use super::*;
    use crate::test_support::{context, Bus, Tree};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    };
    struct Profiles {
        current: Mutex<String>,
        writes: Arc<Mutex<Vec<String>>>,
        denied: Arc<AtomicBool>,
    }
    fn offered() -> Vec<HashMap<String, OwnedValue>> {
        ["power-saver", "balanced", "performance"]
            .into_iter()
            .map(|p| {
                HashMap::from([(
                    "Profile".into(),
                    OwnedValue::from(zbus::zvariant::Str::from(p)),
                )])
            })
            .collect()
    }
    #[zbus::interface(name = "org.freedesktop.UPower.PowerProfiles")]
    impl Profiles {
        #[zbus(property)]
        fn active_profile(&self) -> String {
            self.current.lock().unwrap().clone()
        }
        #[zbus(property)]
        fn set_active_profile(&mut self, value: String) -> zbus::fdo::Result<()> {
            if self.denied.load(Ordering::SeqCst) {
                return Err(zbus::fdo::Error::AccessDenied("fake policy denial".into()));
            }
            self.writes.lock().unwrap().push(value.clone());
            *self.current.lock().unwrap() = value;
            Ok(())
        }
        #[zbus(property)]
        fn profiles(&self) -> Vec<HashMap<String, OwnedValue>> {
            offered()
        }
    }
    struct Old;
    #[zbus::interface(name = "net.hadess.PowerProfiles")]
    impl Old {
        #[zbus(property)]
        fn active_profile(&self) -> String {
            "power-saver".into()
        }
        #[zbus(property)]
        fn profiles(&self) -> Vec<HashMap<String, OwnedValue>> {
            offered()
        }
    }
    #[tokio::test]
    async fn modern_preference_distinct_cycles_denial_and_signal_updates() {
        let bus = Bus::new();
        let server = bus.connection().await;
        let client = bus.connection().await;
        server.request_name(MODERN.0).await.unwrap();
        server.request_name(OLD.0).await.unwrap();
        let writes = Arc::new(Mutex::new(Vec::new()));
        let denied = Arc::new(AtomicBool::new(false));
        server
            .object_server()
            .at(
                MODERN.1,
                Profiles {
                    current: Mutex::new("balanced".into()),
                    writes: writes.clone(),
                    denied: denied.clone(),
                },
            )
            .await
            .unwrap();
        server.object_server().at(OLD.1, Old).await.unwrap();
        let tree = Tree::new();
        let mut native = Native::new(context(client, tree.paths(), None));
        native.connect().await.unwrap();
        assert_eq!(native.provider, Some(MODERN));
        assert_eq!(native.project().profile, PowerProfile::Balanced);
        native.apply(Action::ProfileCyclePanel).await.unwrap();
        assert_eq!(native.project().profile, PowerProfile::Performance);
        native.apply(Action::ProfileToggle).await.unwrap();
        assert_eq!(native.project().profile, PowerProfile::Balanced);
        assert_eq!(*writes.lock().unwrap(), vec!["performance", "balanced"]);
        // Setter signals were queued before the confirming Get replies. A
        // later unrelated property signal is a barrier: old ActiveProfile must
        // not roll confirmed balanced state back to performance.
        let marker = HashMap::from([(
            "PerformanceDegraded",
            OwnedValue::from(zbus::zvariant::Str::from("fake-hot")),
        )]);
        server
            .emit_signal(
                None::<&str>,
                MODERN.1,
                "org.freedesktop.DBus.Properties",
                "PropertiesChanged",
                &(MODERN.0, marker, Vec::<String>::new()),
            )
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), native.event())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            native.project().profile,
            PowerProfile::Balanced,
            "queued old signal overwrote a newer confirmed profile"
        );
        denied.store(true, Ordering::SeqCst);
        assert!(native.apply(Action::ProfileCyclePanel).await.is_err());
        assert_eq!(native.project().profile, PowerProfile::Balanced);
        // Drain previously queued setter signals, then an external profile change.
        let changed = HashMap::from([(
            "ActiveProfile",
            OwnedValue::from(zbus::zvariant::Str::from("power-saver")),
        )]);
        server
            .emit_signal(
                None::<&str>,
                MODERN.1,
                "org.freedesktop.DBus.Properties",
                "PropertiesChanged",
                &(MODERN.0, changed, Vec::<String>::new()),
            )
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while native.project().profile != PowerProfile::PowerSaver {
                native.event().await.unwrap();
            }
        })
        .await
        .unwrap();
        server.release_name(MODERN.0).await.unwrap();
        native.connect().await.unwrap();
        assert_eq!(native.provider, Some(OLD));
        assert_eq!(native.project().profile, PowerProfile::PowerSaver);
    }
}

#[cfg(test)]
mod lid_tests {
    use super::*;
    use crate::test_support::{context, Bus, Tree};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    struct Manager(Arc<AtomicUsize>);
    #[zbus::interface(name = "org.freedesktop.login1.Manager")]
    impl Manager {
        #[zbus(property)]
        fn lid_closed(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn preparing_for_sleep(&self) -> bool {
            false
        }
        fn suspend(&self, _interactive: bool) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    #[tokio::test]
    async fn lid_fallback_and_sleep_signals_do_not_trigger_suspend_policy() {
        let bus = Bus::new();
        let server = bus.connection().await;
        server.request_name(LOGIN).await.unwrap();
        let actions = Arc::new(AtomicUsize::new(0));
        server
            .object_server()
            .at(LOGIN_PATH, Manager(actions.clone()))
            .await
            .unwrap();
        let tree = Tree::new();
        let mut native = Native::new(context(bus.connection().await, tree.paths(), None));
        native.connect().await.unwrap();
        assert_eq!(native.project().lid_present, None);
        assert_eq!(native.project().lid_closed, Some(true));
        server
            .emit_signal(
                None::<&str>,
                LOGIN_PATH,
                LOGIN_IFACE,
                "PrepareForSleep",
                &(true,),
            )
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), native.event())
            .await
            .unwrap()
            .unwrap();
        assert!(native.project().sleeping);
        server
            .emit_signal(
                None::<&str>,
                LOGIN_PATH,
                LOGIN_IFACE,
                "PrepareForSleep",
                &(false,),
            )
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), native.event())
            .await
            .unwrap()
            .unwrap();
        assert!(!native.project().sleeping);
        assert_eq!(actions.load(Ordering::SeqCst), 0);
    }
}
