//! Keyring tests against a fake Secret Service on a private bus (server side
//! of the DH session included). The real keyring is never contacted:
//! `session_bus()` refuses the user bus in test builds.
use super::*;
use crate::test_support::PrivateBus;
use std::sync::{Arc, Mutex};
use zbus::object_server::ObjectServer;

/// (attributes, plaintext)
type FakeItemData = (HashMap<String, String>, Vec<u8>);
#[derive(Clone, Default)]
struct Seen {
    algorithms: Arc<Mutex<Vec<String>>>,
    searches: Arc<Mutex<Vec<HashMap<String, String>>>>,
    /// Raw `value` bytes received on the bus by CreateItem.
    wire_values: Arc<Mutex<Vec<Vec<u8>>>>,
    /// item path → (attributes, plaintext)
    items: Arc<Mutex<HashMap<String, FakeItemData>>>,
    key: Arc<Mutex<Option<[u8; 16]>>>,
    refuse_dh: bool,
}
impl Seen {
    fn add_item(&self, attrs: &[(&str, &str)], secret: &str) -> String {
        let mut items = self.items.lock().unwrap();
        let path = format!(
            "/org/freedesktop/secrets/collection/login/{}",
            items.len() + 1
        );
        let attrs = attrs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        items.insert(path.clone(), (attrs, secret.as_bytes().to_vec()));
        path
    }
}
#[derive(zbus::DBusError, Debug)]
#[zbus(prefix = "org.freedesktop.DBus.Error")]
enum FakeError {
    NotSupported(String),
}
struct FakeService(Seen);
#[zbus::interface(name = "org.freedesktop.Secret.Service")]
impl FakeService {
    fn open_session(
        &self,
        algorithm: String,
        input: OwnedValue,
    ) -> std::result::Result<(OwnedValue, OwnedObjectPath), FakeError> {
        self.0.algorithms.lock().unwrap().push(algorithm.clone());
        let session = OwnedObjectPath::try_from("/org/freedesktop/secrets/session/s1").unwrap();
        if algorithm == dh::ALGORITHM {
            if self.0.refuse_dh {
                return Err(FakeError::NotSupported("dh refused".into()));
            }
            let server = dh::KeyPair::generate();
            let client = Vec::<u8>::try_from(input).unwrap();
            *self.0.key.lock().unwrap() = Some(*server.derive(&client).unwrap());
            Ok((
                OwnedValue::try_from(Value::from(server.public.clone())).unwrap(),
                session,
            ))
        } else {
            Ok((OwnedValue::try_from(Value::from("")).unwrap(), session))
        }
    }
    fn search_items(
        &self,
        attributes: HashMap<String, String>,
    ) -> (Vec<OwnedObjectPath>, Vec<OwnedObjectPath>) {
        self.0.searches.lock().unwrap().push(attributes.clone());
        let items = self.0.items.lock().unwrap();
        let mut found: Vec<OwnedObjectPath> = items
            .iter()
            .filter(|(_, (a, _))| attributes.iter().all(|(k, v)| a.get(k) == Some(v)))
            .map(|(p, _)| OwnedObjectPath::try_from(p.as_str()).unwrap())
            .collect();
        found.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        (found, vec![])
    }
    fn get_secrets(
        &self,
        items: Vec<OwnedObjectPath>,
        session: OwnedObjectPath,
    ) -> HashMap<OwnedObjectPath, (OwnedObjectPath, Vec<u8>, Vec<u8>, String)> {
        let store = self.0.items.lock().unwrap();
        let key = *self.0.key.lock().unwrap();
        items
            .into_iter()
            .filter_map(|p| {
                let (_, plain) = store.get(p.as_str())?;
                let (params, value) = match key {
                    Some(k) => dh::encrypt(&k, plain),
                    None => (vec![], plain.clone()),
                };
                Some((p, (session.clone(), params, value, "text/plain".into())))
            })
            .collect()
    }
    fn read_alias(&self, _name: String) -> OwnedObjectPath {
        OwnedObjectPath::try_from("/org/freedesktop/secrets/collection/login").unwrap()
    }
    fn unlock(&self, objects: Vec<OwnedObjectPath>) -> (Vec<OwnedObjectPath>, OwnedObjectPath) {
        (objects, OwnedObjectPath::try_from("/").unwrap())
    }
}
struct FakeCollection(Seen);
#[zbus::interface(name = "org.freedesktop.Secret.Collection")]
impl FakeCollection {
    async fn create_item(
        &self,
        properties: HashMap<String, OwnedValue>,
        secret: (OwnedObjectPath, Vec<u8>, Vec<u8>, String),
        _replace: bool,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> (OwnedObjectPath, OwnedObjectPath) {
        let attrs: HashMap<String, String> = properties["org.freedesktop.Secret.Item.Attributes"]
            .try_clone()
            .unwrap()
            .try_into()
            .unwrap();
        self.0.wire_values.lock().unwrap().push(secret.2.clone());
        let key = *self.0.key.lock().unwrap();
        let plain = match key {
            Some(k) => dh::decrypt(&k, &secret.1, &secret.2).unwrap().to_vec(),
            None => secret.2,
        };
        let pairs: Vec<(&str, &str)> = attrs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let path = self
            .0
            .add_item(&pairs, std::str::from_utf8(&plain).unwrap());
        server
            .at(path.as_str(), FakeItem(self.0.clone(), path.clone()))
            .await
            .unwrap();
        (
            OwnedObjectPath::try_from(path).unwrap(),
            OwnedObjectPath::try_from("/").unwrap(),
        )
    }
}
struct FakeItem(Seen, String);
#[zbus::interface(name = "org.freedesktop.Secret.Item")]
impl FakeItem {
    #[zbus(property)]
    fn attributes(&self) -> HashMap<String, String> {
        self.0
            .items
            .lock()
            .unwrap()
            .get(&self.1)
            .map(|(a, _)| a.clone())
            .unwrap_or_default()
    }
    fn delete(&self) -> OwnedObjectPath {
        self.0.items.lock().unwrap().remove(&self.1);
        OwnedObjectPath::try_from("/").unwrap()
    }
}
async fn fake_secret_service(bus: &PrivateBus, seen: Seen) -> Connection {
    let c = bus
        .builder()
        .name(SERVICE)
        .unwrap()
        .serve_at(SERVICE_PATH, FakeService(seen.clone()))
        .unwrap()
        .serve_at(
            "/org/freedesktop/secrets/collection/login",
            FakeCollection(seen.clone()),
        )
        .unwrap()
        .build()
        .await
        .unwrap();
    // Items created before the agent runs (e.g. by libnma/nm-applet).
    let paths: Vec<String> = seen.items.lock().unwrap().keys().cloned().collect();
    for p in paths {
        c.object_server()
            .at(p.as_str(), FakeItem(seen.clone(), p.clone()))
            .await
            .unwrap();
    }
    c
}
fn fields(pairs: &[(&str, &str)]) -> Fields {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), Zeroizing::new(v.to_string())))
        .collect()
}
const UUID: &str = "8d7f1c2e-0000-4000-8000-000000000001";
const WSEC: &str = "802-11-wireless-security";

/// NET-8 + NET-9 in one test: it is the only test touching the session-bus
/// environment variable.
#[tokio::test(flavor = "multi_thread")]
async fn net8_net9_networkmanager_schema_over_encrypted_session() {
    // Real-keyring guard: the user's bus address is refused outright.
    std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus");
    assert!(Keyring.load(UUID, WSEC, false).await.is_err());

    let bus = PrivateBus::start();
    let seen = Seen::default();
    // A secret written earlier by libnma (no xdg:schema on old items).
    seen.add_item(
        &[
            ("connection-uuid", UUID),
            ("setting-name", WSEC),
            ("setting-key", "psk"),
        ],
        "from-libnma",
    );
    let _svc = fake_secret_service(&bus, seen.clone()).await;
    std::env::set_var("DBUS_SESSION_BUS_ADDRESS", &bus.address);

    // NET-8: found through NetworkManager's schema attributes.
    let loaded = Keyring.load(UUID, WSEC, false).await.unwrap();
    assert_eq!(loaded.get("psk").map(|v| v.as_str()), Some("from-libnma"));
    let search = seen.searches.lock().unwrap()[0].clone();
    assert_eq!(
        search,
        HashMap::from([
            ("connection-uuid".into(), UUID.into()),
            ("setting-name".into(), WSEC.into())
        ])
    );
    // NET-9: the session was DH-negotiated.
    assert_eq!(seen.algorithms.lock().unwrap()[0], dh::ALGORITHM);

    // Saving writes one NetworkManager-schema item per key, encrypted on the wire.
    Keyring
        .save(
            UUID,
            "HomeNet",
            WSEC,
            &fields(&[("psk", "new-secret-value")]),
        )
        .await
        .unwrap();
    let items = seen.items.lock().unwrap().clone();
    let (attrs, plain) = items
        .values()
        .find(|(_, p)| p == b"new-secret-value")
        .expect("item stored");
    assert_eq!(attrs.get("xdg:schema").map(String::as_str), Some(SCHEMA));
    assert_eq!(attrs.get("connection-uuid").map(String::as_str), Some(UUID));
    assert_eq!(attrs.get("setting-name").map(String::as_str), Some(WSEC));
    assert_eq!(attrs.get("setting-key").map(String::as_str), Some("psk"));
    assert!(!attrs.contains_key("application"));
    assert_eq!(plain, b"new-secret-value");
    let wire = seen.wire_values.lock().unwrap()[0].clone();
    assert!(
        !wire
            .windows(b"new-secret-value".len())
            .any(|w| w == b"new-secret-value"),
        "plaintext on the bus"
    );

    // Remove one key, then delete the connection's remaining secrets.
    Keyring.remove(UUID, WSEC, &["psk".into()]).await.unwrap();
    assert!(seen.items.lock().unwrap().is_empty());
    Keyring
        .save(
            UUID,
            "HomeNet",
            "802-1x",
            &fields(&[("password", "x"), ("private-key-password", "y")]),
        )
        .await
        .unwrap();
    assert_eq!(seen.items.lock().unwrap().len(), 2);
    Keyring.delete(UUID).await.unwrap();
    assert!(seen.items.lock().unwrap().is_empty());

    // A service that refuses DH falls back to the plain transport.
    let bus2 = PrivateBus::start();
    let plain_seen = Seen {
        refuse_dh: true,
        ..Default::default()
    };
    plain_seen.add_item(
        &[
            ("connection-uuid", UUID),
            ("setting-name", WSEC),
            ("setting-key", "psk"),
        ],
        "p",
    );
    let _svc3 = fake_secret_service(&bus2, plain_seen.clone()).await;
    std::env::set_var("DBUS_SESSION_BUS_ADDRESS", &bus2.address);
    assert_eq!(
        Keyring.load(UUID, WSEC, false).await.unwrap()["psk"].as_str(),
        "p"
    );
    assert_eq!(
        *plain_seen.algorithms.lock().unwrap(),
        [dh::ALGORITHM, "plain"]
    );
    std::env::remove_var("DBUS_SESSION_BUS_ADDRESS");
}
