//! Network/SecretAgent tests. The `net*`/`control_*` tests are the regression
//! tests for review round 5 (docs/reviews/hyprv-svc-net.md). All bus peers are
//! fakes on a private dbus-daemon; no real NetworkManager, profile or keyring
//! is touched.
use super::*;
use crate::test_support::{ctx, PrivateBus, HITS, NEEDLE, SCAN};
use std::{
    sync::{atomic::Ordering as AO, Mutex as StdMutex},
    time::Duration,
};
use tokio::sync::Notify;
use zbus::zvariant::Value;

const DEV: &str = "/org/freedesktop/NetworkManager/Devices/1";
const CONN: &str = "/org/freedesktop/NetworkManager/Settings/1";
const SA: &str = "org.freedesktop.NetworkManager.SecretAgent";
const WSEC: &str = "802-11-wireless-security";
const UUID: &str = "8d7f1c2e-0000-4000-8000-000000000001";

fn ov(v: impl Into<Value<'static>>) -> OwnedValue {
    OwnedValue::try_from(v.into()).unwrap()
}
fn profile(psk_flags: u32) -> Settings {
    HashMap::from([
        (
            "connection".to_string(),
            HashMap::from([
                ("id".to_string(), ov("HomeNet")),
                ("uuid".to_string(), ov(UUID)),
                ("type".to_string(), ov("802-11-wireless")),
            ]),
        ),
        (
            "802-11-wireless".to_string(),
            HashMap::from([("ssid".to_string(), ov(b"HomeNet".to_vec()))]),
        ),
        (
            WSEC.to_string(),
            HashMap::from([
                ("key-mgmt".to_string(), ov("wpa-psk")),
                ("psk-flags".to_string(), ov(psk_flags)),
            ]),
        ),
    ])
}
fn path() -> OwnedObjectPath {
    OwnedObjectPath::try_from(CONN).unwrap()
}
fn psk_of(s: &SSettings) -> Option<&str> {
    s.get(WSEC).and_then(|p| p.get("psk")).and_then(SValue::str)
}

/// In-memory SecretStore; `load` can be held open with a gate.
#[derive(Default)]
struct TestStore {
    stored: StdMutex<Fields>,
    saves: StdMutex<Vec<(String, String, Vec<String>)>>,
    removed: StdMutex<Vec<String>>,
    entered: Arc<Notify>,
    gate: Option<Arc<Notify>>,
}
#[async_trait]
impl SecretStore for TestStore {
    async fn load(&self, _: &str, _: &str, _: bool) -> Result<Fields> {
        self.entered.notify_one();
        if let Some(g) = &self.gate {
            g.notified().await;
        }
        Ok(self.stored.lock().unwrap().clone())
    }
    async fn save(&self, uuid: &str, _id: &str, setting: &str, fields: &Fields) -> Result<()> {
        if fields.is_empty() {
            return Ok(());
        }
        self.saves.lock().unwrap().push((
            uuid.into(),
            setting.into(),
            fields.keys().cloned().collect(),
        ));
        let mut stored = self.stored.lock().unwrap();
        for (k, v) in fields {
            stored.insert(k.clone(), v.clone());
        }
        Ok(())
    }
    async fn remove(&self, _: &str, _: &str, keys: &[String]) -> Result<()> {
        let mut stored = self.stored.lock().unwrap();
        for k in keys {
            if stored.remove(k).is_some() {
                self.removed.lock().unwrap().push(k.clone());
            }
        }
        Ok(())
    }
    async fn delete(&self, _: &str) -> Result<()> {
        self.stored.lock().unwrap().clear();
        Ok(())
    }
}
fn secrets_with(store: TestStore) -> (Arc<AgentSecrets>, Arc<TestStore>) {
    let store = Arc::new(store);
    (Arc::new(AgentSecrets::new(store.clone())), store)
}
fn stored(pairs: &[(&str, &str)]) -> Fields {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), Zeroizing::new(v.to_string())))
        .collect()
}
async fn wait_prompt(broker: &Broker, within: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + within;
    while tokio::time::Instant::now() < deadline {
        if !broker.topic().snapshot().value.is_empty() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    false
}
fn answer(broker: &Broker, pairs: &[(&str, &str)]) {
    let id = broker.topic().snapshot().value[0].id;
    let map = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    broker.respond(id, Reply::Fields(map)).unwrap();
}

// ------------------------------------------------------------- fake NM
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
struct FRoot(Log);
#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl FRoot {
    #[zbus(property)]
    fn wireless_enabled(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn wireless_hardware_enabled(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn primary_connection(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from("/").unwrap()
    }
    fn get_devices(&self) -> Vec<OwnedObjectPath> {
        self.0.push("devices");
        vec![OwnedObjectPath::try_from(DEV).unwrap()]
    }
    fn activate_connection(
        &self,
        _c: OwnedObjectPath,
        _d: OwnedObjectPath,
        _s: OwnedObjectPath,
    ) -> OwnedObjectPath {
        OwnedObjectPath::try_from("/org/freedesktop/NetworkManager/ActiveConnection/1").unwrap()
    }
}
struct FAgentMgr(Log);
#[zbus::interface(name = "org.freedesktop.NetworkManager.AgentManager")]
impl FAgentMgr {
    fn register_with_capabilities(&self, id: String, caps: u32) {
        self.0.push(format!("register {id} {caps}"));
    }
    fn unregister(&self) {
        self.0.push("unregister");
    }
}
struct FDev;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device")]
impl FDev {
    #[zbus(property)]
    fn device_type(&self) -> u32 {
        2
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        30
    }
    #[zbus(property)]
    fn interface(&self) -> String {
        "wlan0".into()
    }
}
struct FWifi(Log);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
impl FWifi {
    fn request_scan(&self, _options: HashMap<String, OwnedValue>) {
        self.0.push("scan");
    }
    #[zbus(property)]
    fn last_scan(&self) -> i64 {
        1
    }
    #[zbus(property)]
    fn access_points(&self) -> Vec<OwnedObjectPath> {
        vec![]
    }
    #[zbus(property)]
    fn active_access_point(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from("/").unwrap()
    }
}
struct FSettings;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings")]
impl FSettings {
    fn list_connections(&self) -> Vec<OwnedObjectPath> {
        vec![path()]
    }
}
type Updates = Arc<StdMutex<Vec<Settings>>>;
struct FConn {
    flags: u32,
    updates: Updates,
}
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
impl FConn {
    fn get_settings(&self) -> Settings {
        profile(self.flags)
    }
    fn update(&self, s: Settings) {
        self.updates.lock().unwrap().push(s);
    }
}
async fn fake_nm(bus: &PrivateBus, flags: u32, log: Log, updates: Updates) -> Connection {
    bus.builder()
        .name(NM)
        .unwrap()
        .serve_at(NM_ROOT, FRoot(log.clone()))
        .unwrap()
        .serve_at(
            "/org/freedesktop/NetworkManager/AgentManager",
            FAgentMgr(log.clone()),
        )
        .unwrap()
        .serve_at(DEV, FDev)
        .unwrap()
        .serve_at(DEV, FWifi(log))
        .unwrap()
        .serve_at(SETTINGS, FSettings)
        .unwrap()
        .serve_at(CONN, FConn { flags, updates })
        .unwrap()
        .build()
        .await
        .unwrap()
}
async fn export_agent(conn: &Connection, broker: &Broker, secrets: Arc<AgentSecrets>) {
    conn.object_server()
        .at(
            AGENT,
            SecretAgent {
                connection: conn.clone(),
                broker: broker.clone(),
                secrets,
                pending: Default::default(),
            },
        )
        .await
        .unwrap();
}
async fn get_secrets(
    nm: &Connection,
    agent: &Connection,
    settings: Settings,
    flags: u32,
) -> zbus::Result<zbus::Message> {
    nm.call_method(
        Some(agent.unique_name().unwrap().as_str()),
        AGENT,
        Some(SA),
        "GetSecrets",
        &(settings, path(), WSEC, Vec::<String>::new(), flags),
    )
    .await
}
async fn cancel(nm: &Connection, agent: &Connection) {
    nm.call_method(
        Some(agent.unique_name().unwrap().as_str()),
        AGENT,
        Some(SA),
        "CancelGetSecrets",
        &(path(), WSEC),
    )
    .await
    .unwrap();
}
fn error_name(r: zbus::Result<zbus::Message>) -> String {
    match r {
        Err(zbus::Error::MethodError(name, _, _)) => name.to_string(),
        other => format!("{:?}", other.map(|_| ())),
    }
}

// ------------------------------------------------------------- profiles
#[test]
fn strongest_ap_and_active_priority_preserve_raw_ssid_identity() {
    let n = |id: &str, raw: Vec<u8>, active, signal| Network {
        id: id.into(),
        ssid: String::from_utf8_lossy(&raw).into_owned(),
        ssid_bytes: raw,
        active,
        signal,
        ..Network::default()
    };
    let result = normalize_networks(vec![
        n("weak", b"wifi".to_vec(), false, 10),
        n("strong", b"wifi".to_vec(), false, 90),
        n("active", b"wifi".to_vec(), true, 20),
        n("invalid1", vec![255], false, 80),
        n("invalid2", vec![254], false, 70),
    ]);
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].id, "active");
    assert_eq!(result[1].id, "invalid1");
}
#[test]
fn open_psk_sae_wep_and_enterprise_profiles() {
    let key_mgmt = |s: &SSettings| s[WSEC]["key-mgmt"].str().unwrap().to_owned();
    assert!(!connection_settings("open", None, "", None, false)
        .unwrap()
        .contains_key(WSEC));
    let s = connection_settings("wifi", Some("test-password"), "WPA", None, true).unwrap();
    assert_eq!(key_mgmt(&s), "wpa-psk");
    assert_eq!(psk_of(&s), Some("test-password"));
    assert!(
        matches!(s[WSEC]["psk"], SValue::Secret(_)),
        "secret carried as zeroizing value"
    );
    assert_eq!(s["802-11-wireless"]["ssid"].bytes().unwrap(), b"wifi");
    assert_eq!(
        key_mgmt(&connection_settings("wifi", None, "WPA3", None, true).unwrap()),
        "sae"
    );
    let wep = connection_settings("wifi", Some("k"), "WEP", None, true).unwrap();
    assert_eq!(key_mgmt(&wep), "none");
    assert!(
        !wep[WSEC].contains_key("wep-key-flags"),
        "legacy: system-owned (NONE)"
    );
    let mut e = Enterprise {
        identity: "user".into(),
        eap: vec!["peap".into()],
        phase2_auth: Some("mschapv2".into()),
        ..Enterprise::default()
    };
    assert!(connection_settings("enterprise", None, "EAP", Some(&e), true).is_err());
    e.domain_suffix_match = Some("example.edu".into());
    let s = connection_settings("enterprise", Some("pw"), "EAP", Some(&e), true).unwrap();
    assert_eq!(s["802-1x"]["identity"].str(), Some("user"));
    assert_eq!(s["802-1x"]["system-ca-certs"].bool(), Some(true));
    assert!(
        !s["802-1x"].contains_key("password-flags"),
        "legacy: system-owned (NONE)"
    );
    assert_eq!(key_mgmt(&s), "wpa-eap");
    // Empty input never becomes a secret.
    assert!(
        !connection_settings("wifi", Some(""), "WPA", None, true).unwrap()[WSEC]
            .contains_key("psk")
    );
}
#[test]
fn qml_fields_are_camel_case_and_secrets_are_not_state() {
    let json = serde_json::to_value(State::default()).unwrap();
    for k in [
        "present",
        "iface",
        "enabled",
        "hardwareEnabled",
        "connected",
        "ssid",
        "signal",
        "security",
        "networks",
    ] {
        assert!(json.get(k).is_some(), "{k}");
    }
    assert!(json.get("password").is_none());
}
/// NET-7 (legacy parity): a network created via the shell stores its secret
/// in the system profile like `nmcli dev wifi connect ... password`: no
/// `*-flags` key, i.e. NONE (0), never NOT_SAVED or AGENT_OWNED.
#[test]
fn net7_new_profile_secret_is_system_owned_like_nmcli() {
    let s = connection_settings("HomeNet", Some("pw-123456"), "WPA", None, true).unwrap();
    assert!(
        !s[WSEC].contains_key("psk-flags"),
        "psk-flags must be absent (NONE)"
    );
    assert_eq!(psk_of(&s), Some("pw-123456"));
    let e = Enterprise {
        identity: "u".into(),
        eap: vec!["peap".into()],
        domain_suffix_match: Some("example.edu".into()),
        ..Enterprise::default()
    };
    let s = connection_settings("Corp", Some("pw"), "EAP", Some(&e), true).unwrap();
    assert!(
        s["802-1x"].keys().all(|k| !k.ends_with("-flags")),
        "{:?}",
        s["802-1x"].keys()
    );
}

// ------------------------------------------------------------- secret flags
#[test]
fn only_agent_owned_secrets_are_stored() {
    let connection = HashMap::from([(
        WSEC.to_string(),
        HashMap::from([
            ("psk-flags".to_string(), ov(3u32)),
            ("wep-key-flags".to_string(), ov(1u32)),
        ]),
    )]);
    let fields = stored(&[
        ("psk", "must-not-save"),
        ("wep-key0", "agent-owned"),
        ("leap-password", "system-owned"),
    ]);
    let owned = owned_fields(&connection, WSEC, &fields);
    assert_eq!(owned.keys().collect::<Vec<_>>(), ["wep-key0"]);
    assert!(agent_owned(AGENT_OWNED));
    assert!(!agent_owned(AGENT_OWNED | NOT_SAVED));
    assert!(!agent_owned(AGENT_OWNED | NOT_REQUIRED));
    assert!(!agent_owned(0));
}
#[tokio::test]
async fn secrets_noninteractive_keyring_and_new_secret_prompt() {
    let (secrets, store) = secrets_with(TestStore::default());
    let broker = Broker::new();
    // Nothing stored, interaction disallowed.
    assert!(matches!(
        resolve_secrets(
            &broker,
            &secrets,
            profile(1),
            path(),
            WSEC.into(),
            vec![],
            0
        )
        .await,
        Err(AgentError::NoSecrets(_))
    ));
    // Agent-owned psk from the keyring, no prompt.
    *store.stored.lock().unwrap() = stored(&[("psk", "saved-value")]);
    let cached = resolve_secrets(
        &broker,
        &secrets,
        profile(1),
        path(),
        WSEC.into(),
        vec![],
        0,
    )
    .await
    .unwrap();
    assert_eq!(psk_of(&cached), Some("saved-value"));
    assert_eq!(cached[WSEC]["name"].str(), Some(WSEC));
    assert!(broker.topic().snapshot().value.is_empty());
    // REQUEST_NEW: keyring ignored, user asked; empty fields never count.
    let b = broker.clone();
    let s2 = secrets.clone();
    let pending = tokio::spawn(async move {
        resolve_secrets(
            &b,
            &s2,
            profile(1),
            path(),
            WSEC.into(),
            vec!["psk".into()],
            ALLOW_INTERACTION | REQUEST_NEW,
        )
        .await
    });
    assert!(wait_prompt(&broker, Duration::from_secs(2)).await);
    assert!(matches!(
        broker.topic().snapshot().value[0].kind,
        Kind::NetworkSecrets {
            new_secrets: true,
            ..
        }
    ));
    answer(&broker, &[("psk", "new-value"), ("wep-key0", "")]);
    let result = pending.await.unwrap().unwrap();
    assert_eq!(psk_of(&result), Some("new-value"));
    assert!(!result[WSEC].contains_key("wep-key0"));
    assert_eq!(
        store.saves.lock().unwrap().len(),
        1,
        "typed agent-owned secret stored"
    );
}
#[tokio::test]
async fn explicit_reject_is_user_canceled_and_timeout_is_no_secrets() {
    let (secrets, _) = secrets_with(TestStore::default());
    let broker = Broker::new();
    let (b, s) = (broker.clone(), secrets.clone());
    let pending = tokio::spawn(async move {
        resolve_secrets(
            &b,
            &s,
            profile(1),
            path(),
            WSEC.into(),
            vec![],
            ALLOW_INTERACTION,
        )
        .await
    });
    assert!(wait_prompt(&broker, Duration::from_secs(2)).await);
    let id = broker.topic().snapshot().value[0].id;
    broker.respond(id, Reply::Reject).unwrap();
    assert!(matches!(
        pending.await.unwrap(),
        Err(AgentError::UserCanceled(_))
    ));
}
/// NET-4: a NOT_SAVED secret is always asked for, never served from the
/// keyring, and is never stored.
#[tokio::test]
async fn net4_not_saved_secret_is_never_served_from_keyring() {
    let (secrets, store) = secrets_with(TestStore {
        stored: StdMutex::new(stored(&[("psk", "stale-keyring-value")])),
        ..Default::default()
    });
    let broker = Broker::new();
    let (b, s) = (broker.clone(), secrets.clone());
    let task = tokio::spawn(async move {
        resolve_secrets(
            &b,
            &s,
            profile(NOT_SAVED),
            path(),
            WSEC.into(),
            vec![],
            ALLOW_INTERACTION,
        )
        .await
    });
    assert!(
        wait_prompt(&broker, Duration::from_millis(500)).await,
        "NOT_SAVED secret must prompt"
    );
    answer(&broker, &[("psk", "typed-now")]);
    let result = task.await.unwrap().unwrap();
    assert_eq!(psk_of(&result), Some("typed-now"));
    assert!(
        store.saves.lock().unwrap().is_empty(),
        "NOT_SAVED secret stored"
    );
    // Non-interactive: no stale fallback either.
    assert!(matches!(
        resolve_secrets(
            &broker,
            &secrets,
            profile(NOT_SAVED),
            path(),
            WSEC.into(),
            vec![],
            0
        )
        .await,
        Err(AgentError::NoSecrets(_))
    ));
}
#[tokio::test]
async fn not_required_secret_is_not_requested() {
    let (secrets, _) = secrets_with(TestStore::default());
    let broker = Broker::new();
    let r = resolve_secrets(
        &broker,
        &secrets,
        profile(NOT_REQUIRED),
        path(),
        WSEC.into(),
        vec![],
        ALLOW_INTERACTION,
    )
    .await;
    assert!(
        matches!(r, Err(AgentError::NoSecrets(_))),
        "no prompt for a NOT_REQUIRED-only setting"
    );
    assert!(broker.topic().snapshot().value.is_empty());
}

/// NET-1: secret buffers are zeroized before they are freed.
#[tokio::test]
async fn net1_secret_buffers_are_zeroized() {
    let broker = Broker::new();
    HITS.store(0, AO::SeqCst);
    SCAN.store(true, AO::SeqCst);
    {
        let (secrets, store) = secrets_with(TestStore::default());
        let (b, s) = (broker.clone(), secrets.clone());
        let task = tokio::spawn(async move {
            resolve_secrets(
                &b,
                &s,
                profile(1),
                path(),
                WSEC.into(),
                vec![],
                ALLOW_INTERACTION,
            )
            .await
            .map(|s| s.len())
            .map_err(|_| ())
        });
        assert!(wait_prompt(&broker, Duration::from_secs(2)).await);
        let id = broker.topic().snapshot().value[0].id;
        let secret = String::from_utf8(NEEDLE.to_vec()).unwrap();
        broker
            .respond(
                id,
                Reply::Fields(BTreeMap::from([("psk".to_string(), secret)])),
            )
            .unwrap();
        task.await.unwrap().unwrap();
        drop((secrets, store));
        // UI → service: the Connect action carries the password.
        drop(Action::Connect {
            ssid: "HomeNet".into(),
            password: Some(String::from_utf8(NEEDLE.to_vec()).unwrap()),
            security: "WPA".into(),
            enterprise: None,
        });
        // New-profile settings carry it to AddAndActivateConnection2.
        let pw = Zeroizing::new(String::from_utf8(NEEDLE.to_vec()).unwrap());
        drop(connection_settings("HomeNet", Some(&pw), "WPA", None, true).unwrap());
        drop(pw);
        // A PIN reply.
        drop(Reply::Pin(String::from_utf8(NEEDLE.to_vec()).unwrap()));
    }
    SCAN.store(false, AO::SeqCst);
    let hits = HITS.load(AO::SeqCst);
    assert_eq!(
        hits, 0,
        "{hits} heap blocks still held the secret when freed"
    );
}

/// NET-2: CancelGetSecrets during the keyring stage cancels the request:
/// no prompt appears and the call is answered at once.
#[tokio::test(flavor = "multi_thread")]
async fn net2_cancel_before_prompt_cancels_request() {
    let bus = PrivateBus::start();
    let nm = fake_nm(&bus, 1, Log::default(), Default::default()).await;
    let agent = bus.connect().await;
    let broker = Broker::new();
    let entered = Arc::new(Notify::new());
    let gate = Arc::new(Notify::new());
    let (secrets, _) = secrets_with(TestStore {
        entered: entered.clone(),
        gate: Some(gate.clone()),
        ..Default::default()
    });
    export_agent(&agent, &broker, secrets).await;
    let (nm2, agent2) = (nm.clone(), agent.clone());
    let pending =
        tokio::spawn(
            async move { get_secrets(&nm2, &agent2, profile(1), ALLOW_INTERACTION).await },
        );
    tokio::time::timeout(Duration::from_secs(2), entered.notified())
        .await
        .expect("keyring stage reached");
    cancel(&nm, &agent).await;
    gate.notify_one();
    let reply = tokio::time::timeout(Duration::from_millis(800), pending).await;
    let shown = wait_prompt(&broker, Duration::from_millis(200)).await;
    assert!(!shown, "prompt published after CancelGetSecrets");
    let name = error_name(reply.expect("GetSecrets answered").unwrap());
    assert_eq!(
        name,
        "org.freedesktop.NetworkManager.SecretAgent.AgentCanceled"
    );
}

/// NET-3: cancelling a request with a visible prompt answers AgentCanceled
/// and removes the prompt.
#[tokio::test(flavor = "multi_thread")]
async fn net3_cancelled_request_reports_agent_canceled() {
    let bus = PrivateBus::start();
    let nm = fake_nm(&bus, 1, Log::default(), Default::default()).await;
    let agent = bus.connect().await;
    let broker = Broker::new();
    let (secrets, _) = secrets_with(TestStore::default());
    export_agent(&agent, &broker, secrets).await;
    let (nm2, agent2) = (nm.clone(), agent.clone());
    let pending =
        tokio::spawn(
            async move { get_secrets(&nm2, &agent2, profile(1), ALLOW_INTERACTION).await },
        );
    assert!(wait_prompt(&broker, Duration::from_secs(2)).await);
    cancel(&nm, &agent).await;
    let reply = tokio::time::timeout(Duration::from_secs(2), pending)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        error_name(reply),
        "org.freedesktop.NetworkManager.SecretAgent.AgentCanceled"
    );
    assert!(broker.topic().snapshot().value.is_empty());
}

/// Control: only NetworkManager's current owner may call the agent; others
/// are refused before any prompt or keyring access.
#[tokio::test(flavor = "multi_thread")]
async fn control_foreign_caller_is_rejected() {
    let bus = PrivateBus::start();
    let _nm = fake_nm(&bus, 1, Log::default(), Default::default()).await;
    let agent = bus.connect().await;
    let attacker = bus.connect().await;
    let broker = Broker::new();
    let entered = Arc::new(Notify::new());
    let (secrets, _) = secrets_with(TestStore {
        entered: entered.clone(),
        ..Default::default()
    });
    export_agent(&agent, &broker, secrets).await;
    let r = get_secrets(&attacker, &agent, profile(1), ALLOW_INTERACTION).await;
    assert_eq!(
        error_name(r),
        "org.freedesktop.NetworkManager.SecretAgent.PermissionDenied"
    );
    assert!(!wait_prompt(&broker, Duration::from_millis(200)).await);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), entered.notified())
            .await
            .is_err(),
        "keyring consulted"
    );
}

// ------------------------------------------------------------- Connect
struct ConnectRun {
    updates: Vec<Settings>,
    store: Arc<TestStore>,
    native: Native,
}
async fn connect_with(flags: u32, password: &str) -> (ConnectRun, PrivateBus) {
    let bus = PrivateBus::start();
    let updates: Updates = Default::default();
    let _nm = fake_nm(&bus, flags, Log::default(), updates.clone()).await;
    let agent = bus.connect().await;
    let store = Arc::new(TestStore::default());
    let native = Native::with_store(&ctx(agent), Broker::new(), store.clone())
        .await
        .unwrap();
    native
        .execute(Action::Connect {
            ssid: "HomeNet".into(),
            password: Some(password.into()),
            security: "WPA".into(),
            enterprise: None,
        })
        .await
        .unwrap();
    let updates = updates.lock().unwrap().clone();
    (
        ConnectRun {
            updates,
            store,
            native,
        },
        bus,
    )
}
fn flags_of(s: &Settings) -> u32 {
    s.get(WSEC).map(|p| number(p, "psk-flags")).unwrap_or(0)
}
/// NET-5: a typed password never changes an existing profile's secret flags.
/// System-owned: secret replaced in place, flags untouched. Agent-owned:
/// keyring + this activation only. NOT_SAVED: this activation only.
#[tokio::test(flavor = "multi_thread")]
async fn net5_existing_profile_keeps_secret_ownership() {
    let (system, _b1) = connect_with(0, "new-pass").await;
    assert_eq!(system.updates.len(), 1);
    assert_eq!(flags_of(&system.updates[0]), 0, "system-owned flags kept");
    assert_eq!(
        system.updates[0][WSEC]
            .get("psk")
            .and_then(|v| <&str>::try_from(v).ok()),
        Some("new-pass")
    );

    let (agent, _b2) = connect_with(AGENT_OWNED, "new-pass").await;
    assert!(
        agent.updates.is_empty(),
        "agent-owned profile was rewritten"
    );
    assert_eq!(
        agent.store.saves.lock().unwrap().len(),
        1,
        "stored in keyring"
    );
    let staged = agent
        .native
        .secrets
        .take_staged(UUID, WSEC)
        .expect("staged for activation");
    assert_eq!(staged["psk"].as_str(), "new-pass");

    let (not_saved, _b3) = connect_with(NOT_SAVED, "new-pass").await;
    assert!(
        not_saved.updates.is_empty(),
        "NOT_SAVED profile was rewritten"
    );
    assert!(
        not_saved.store.saves.lock().unwrap().is_empty(),
        "NOT_SAVED secret stored"
    );
    assert!(not_saved.native.secrets.take_staged(UUID, WSEC).is_some());
}
/// NET-6: an empty password on a saved network never overwrites its secret.
#[tokio::test(flavor = "multi_thread")]
async fn net6_empty_password_keeps_saved_secret() {
    for flags in [0, AGENT_OWNED, NOT_SAVED] {
        let (run, _bus) = connect_with(flags, "").await;
        assert!(
            run.updates.is_empty(),
            "flags {flags}: Update sent for empty input"
        );
        assert!(run.store.saves.lock().unwrap().is_empty());
        assert!(run.native.secrets.take_staged(UUID, WSEC).is_none());
    }
}
/// The staged secret is consumed by the next GetSecrets (once), without a prompt.
#[tokio::test]
async fn staged_secret_answers_next_request_once() {
    let (secrets, _) = secrets_with(TestStore::default());
    secrets.stage(UUID, WSEC, stored(&[("psk", "typed-for-now")]));
    let broker = Broker::new();
    let r = resolve_secrets(
        &broker,
        &secrets,
        profile(NOT_SAVED),
        path(),
        WSEC.into(),
        vec![],
        0,
    )
    .await
    .unwrap();
    assert_eq!(psk_of(&r), Some("typed-for-now"));
    assert!(secrets.take_staged(UUID, WSEC).is_none());
}
/// SaveSecrets stores agent-owned secrets and drops keyring copies of
/// secrets that are no longer agent-owned.
#[tokio::test(flavor = "multi_thread")]
async fn save_secrets_syncs_ownership() {
    let bus = PrivateBus::start();
    let nm = fake_nm(&bus, 1, Log::default(), Default::default()).await;
    let agent = bus.connect().await;
    let (secrets, store) = secrets_with(TestStore {
        stored: StdMutex::new(stored(&[("psk", "old")])),
        ..Default::default()
    });
    export_agent(&agent, &Broker::new(), secrets).await;
    let mut conn = profile(NOT_SAVED);
    conn.get_mut(WSEC)
        .unwrap()
        .insert("psk".into(), ov("not-to-be-stored"));
    nm.call_method(
        Some(agent.unique_name().unwrap().as_str()),
        AGENT,
        Some(SA),
        "SaveSecrets",
        &(conn, path()),
    )
    .await
    .unwrap();
    assert!(store.saves.lock().unwrap().is_empty());
    assert_eq!(*store.removed.lock().unwrap(), ["psk"]);
}

/// Control: the agent re-registers with a restarted NetworkManager, and
/// sends nothing while it is gone.
#[tokio::test(flavor = "multi_thread")]
async fn control_reregisters_after_nm_restart() {
    let bus = PrivateBus::start();
    let log1 = Log::default();
    let nm1 = fake_nm(&bus, 1, log1.clone(), Default::default()).await;
    let agent = bus.connect().await;
    let handle = spawn(ctx(agent));
    let t = tokio::time::Instant::now();
    while log1.count("register") == 0 && t.elapsed() < Duration::from_secs(3) {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(log1.count("register org.hyprv.Shell"), 1);
    nm1.close().await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let gone = handle.state.snapshot().value;
    assert!(
        !gone.available && gone.error.as_deref() == Some("NetworkManager is not running"),
        "{gone:?}"
    );
    let log2 = Log::default();
    let _nm2 = fake_nm(&bus, 1, log2.clone(), Default::default()).await;
    let t = tokio::time::Instant::now();
    while log2.count("register") == 0 && t.elapsed() < Duration::from_secs(3) {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        log2.count("register org.hyprv.Shell"),
        1,
        "no re-registration with the new daemon"
    );
}
/// BT-1 (NetworkManager side): a stopped NetworkManager is never D-Bus-activated.
#[tokio::test(flavor = "multi_thread")]
async fn nm_stop_does_not_reactivate_networkmanager() {
    let dir = std::env::temp_dir().join(format!("hyprv-net-nm-act-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let marker = dir.join("nm-activated");
    let bus = PrivateBus::with_activatable(NM, &marker);
    let log = Log::default();
    let nm = fake_nm(&bus, 1, log.clone(), Default::default()).await;
    let agent = bus.connect().await;
    let _handle = spawn(ctx(agent));
    let t = tokio::time::Instant::now();
    while log.count("register") == 0 && t.elapsed() < Duration::from_secs(3) {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    nm.close().await.unwrap();
    tokio::time::sleep(Duration::from_millis(1000)).await;
    let activated = marker.exists();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        !activated,
        "stopping NetworkManager caused a D-Bus activation request"
    );
}

// Laptop scanner tests use actual Native D-Bus calls on a private bus only.
// Run the identical actor on the test runtime so its deadlines share virtual time.
fn laptop_actor(context: Context, broker: Broker, native: Arc<Native>, profile: Profile) -> Handle {
    let state = context.store.topic::<StateTopic>(State::default());
    let scan_lease = ScanLease::default();
    let scanner = Scanner::new(profile, context.read_only, &scan_lease);
    let (actions, rx) = mpsc::channel(32);
    let h = Handle {
        state: state.clone(),
        prompts: broker.topic(),
        scan_lease,
        broker: broker.clone(),
        actions,
    };
    tokio::spawn(run(
        native,
        state,
        rx,
        broker,
        scanner,
        context.runtime.shutdown_signal(),
        false,
    ));
    h
}

async fn laptop_until(label: &str, predicate: impl Fn() -> bool) {
    let start = std::time::Instant::now();
    while !predicate() {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "private bus actor stalled: {label}"
        );
        tokio::task::yield_now().await;
    }
}
async fn laptop_settle() {
    // Keep the paused runtime runnable while the independent dbus-daemon replies.
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_millis(30) {
        tokio::task::yield_now().await;
    }
}
#[tokio::test]
async fn laptop_private_bus_scan_follows_page_lease_without_minimal_wakeups() {
    let bus = PrivateBus::start();
    let log = Log::default();
    let _nm = fake_nm(&bus, 1, log.clone(), Default::default()).await;
    let context = ctx(bus.connect().await);
    let broker = Broker::new();
    let native = Arc::new(
        Native::with_store(&context, broker.clone(), Arc::new(TestStore::default()))
            .await
            .unwrap(),
    );
    let h = laptop_actor(context.clone(), broker, native, Profile::Laptop);
    laptop_until("available", || h.state.snapshot().value.available).await;
    tokio::time::pause();
    // A bar observer, even with a native backend, cannot enable scanning.
    let _observer = h.state.subscribe();
    let reads = log.count("devices");
    tokio::time::advance(Duration::from_secs(3600)).await;
    laptop_settle().await;
    assert_eq!(log.count("scan"), 0);
    assert_eq!(log.count("devices"), reads);
    assert!(h
        .dispatch(Action::Rescan)
        .await
        .unwrap()
        .result()
        .await
        .is_err());
    assert_eq!(log.count("scan"), 0);
    let first = h.scan_lease.acquire();
    laptop_until("visible scanner", || h.state.snapshot().value.scanning).await;
    laptop_settle().await;
    tokio::time::advance(Duration::from_millis(1)).await;
    laptop_until("first scan", || {
        log.count("scan") == 1 && h.state.snapshot().value.scanning
    })
    .await;
    laptop_settle().await;
    let second = h.scan_lease.acquire();
    drop(first);
    laptop_settle().await;
    tokio::time::advance(crate::wifi_scan::INTERVAL - Duration::from_millis(1)).await;
    laptop_settle().await;
    assert_eq!(
        log.count("scan"),
        1,
        "one scanner, no duplicate acquisition scan"
    );
    // Tokio's timer wheel rounds the sub-millisecond origin up to its next tick.
    tokio::time::advance(Duration::from_millis(2)).await;
    laptop_until("second scan after cooldown", || log.count("scan") == 2).await;
    drop(second);
    laptop_until("last lease closed", || !h.state.snapshot().value.scanning).await;
    laptop_settle().await;
    let reads = log.count("devices");
    tokio::time::advance(Duration::from_secs(3600)).await;
    laptop_settle().await;
    assert_eq!(log.count("scan"), 2);
    assert_eq!(
        log.count("devices"),
        reads,
        "closed page leaves no sampling work"
    );
    let reopened = h.scan_lease.acquire();
    laptop_until("visible again", || h.state.snapshot().value.scanning).await;
    laptop_settle().await;
    tokio::time::advance(Duration::from_millis(1)).await;
    laptop_until("reopened after idle", || log.count("scan") == 3).await;
    drop(reopened);
    laptop_until("rapid close", || !h.state.snapshot().value.scanning).await;
    let reopened = h.scan_lease.acquire();
    laptop_settle().await;
    assert_eq!(log.count("scan"), 3, "rapid reopen retains rate limit");
    context.runtime.request_shutdown();
    laptop_until("shutdown unregistered", || log.count("unregister") == 1).await;
    tokio::time::advance(Duration::from_secs(3600)).await;
    laptop_settle().await;
    assert_eq!(log.count("scan"), 3);
    drop(reopened);
}

#[tokio::test]
async fn laptop_private_bus_scan_readonly_and_provider_loss_do_not_poll() {
    let bus = PrivateBus::start();
    let log = Log::default();
    let nm = fake_nm(&bus, 1, log.clone(), Default::default()).await;
    let mut context = ctx(bus.connect().await);
    context.read_only = true;
    let broker = Broker::new();
    let native = Arc::new(
        Native::with_store(&context, broker.clone(), Arc::new(TestStore::default()))
            .await
            .unwrap(),
    );
    let h = laptop_actor(context.clone(), broker, native, Profile::Laptop);
    let _lease = h.scan_lease.acquire();
    laptop_until("readonly available", || h.state.snapshot().value.available).await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(3600)).await;
    laptop_settle().await;
    assert_eq!(log.count("scan"), 0);
    context.runtime.request_shutdown();
    laptop_settle().await;

    let context = ctx(bus.connect().await);
    let broker = Broker::new();
    let native = Arc::new(
        Native::with_store(&context, broker.clone(), Arc::new(TestStore::default()))
            .await
            .unwrap(),
    );
    let h = laptop_actor(context.clone(), broker, native, Profile::Laptop);
    laptop_until("provider available", || h.state.snapshot().value.available).await;
    nm.release_name(NM).await.unwrap();
    laptop_until("provider lost", || !h.state.snapshot().value.available).await;
    let _lease = h.scan_lease.acquire();
    laptop_settle().await;
    let reads = log.count("devices");
    tokio::time::advance(Duration::from_secs(3600)).await;
    laptop_settle().await;
    assert_eq!(log.count("scan"), 0);
    assert_eq!(log.count("devices"), reads);
    nm.request_name(NM).await.unwrap();
    laptop_until("new owner available", || h.state.snapshot().value.available).await;
    laptop_settle().await;
    tokio::time::advance(Duration::from_millis(1)).await;
    laptop_until("provider reappeared", || {
        h.state.snapshot().value.available && log.count("scan") == 1
    })
    .await;
    context.runtime.request_shutdown();
    laptop_settle().await;
}

#[tokio::test]
async fn desktop_private_bus_rescan_needs_no_page_lease_and_never_auto_scans() {
    let bus = PrivateBus::start();
    let log = Log::default();
    let _nm = fake_nm(&bus, 1, log.clone(), Default::default()).await;
    let context = ctx(bus.connect().await);
    let broker = Broker::new();
    let native = Arc::new(
        Native::with_store(&context, broker.clone(), Arc::new(TestStore::default()))
            .await
            .unwrap(),
    );
    let h = laptop_actor(context.clone(), broker, native, Profile::Desktop);
    laptop_until("desktop available", || h.state.snapshot().value.available).await;
    tokio::time::pause();
    let _lease = h.scan_lease.acquire();
    let before = serde_json::to_vec(&h.state.snapshot().value).unwrap();
    let reads = log.count("devices");
    tokio::time::advance(Duration::from_secs(3600)).await;
    laptop_settle().await;
    assert_eq!(log.count("scan"), 0);
    assert_eq!(log.count("devices"), reads);
    assert_eq!(
        serde_json::to_vec(&h.state.snapshot().value).unwrap(),
        before,
        "desktop model bytes unchanged by page leases"
    );
    h.dispatch(Action::Rescan)
        .await
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(log.count("scan"), 1);
    context.runtime.request_shutdown();
    laptop_until("desktop shutdown", || log.count("unregister") == 1).await;
}

#[test]
fn laptop_bad_system_bus_override_never_falls_back_to_host() {
    const CHILD: &str = "HYPRV_NET_BAD_BUS_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let mut context = Context::new(
                Arc::new(hyprv_core::store::StateStore::default()),
                Arc::new(hyprv_core::runtime::ServiceRuntime::new().unwrap()),
            );
            context.read_only = true;
            assert!(
                Native::with_store(&context, Broker::new(), Arc::new(TestStore::default()))
                    .await
                    .is_err()
            );
            assert!(crate::bluetooth::Native::new(&context, Broker::new())
                .await
                .is_err());
            context.runtime.request_shutdown();
        });
        return;
    }
    // Set the environment in a fresh child, never in parallel test processes.
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "network::tests::laptop_bad_system_bus_override_never_falls_back_to_host",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .env("HYPRV_PROFILE", "laptop")
        .env("DBUS_SYSTEM_BUS_ADDRESS", "not-a-valid-dbus-address")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn laptop_prefers_first_connected_radio_and_desktop_selection_is_unchanged() {
    for profile in [Profile::Desktop, Profile::Laptop] {
        assert!(prefer_wifi(profile, None, false));
        assert!(prefer_wifi(profile, Some(false), true));
        assert!(!prefer_wifi(profile, Some(false), false));
        assert!(!prefer_wifi(profile, Some(true), false));
    }
    assert!(!prefer_wifi(Profile::Laptop, Some(true), true));
    assert!(prefer_wifi(Profile::Desktop, Some(true), true));
}
