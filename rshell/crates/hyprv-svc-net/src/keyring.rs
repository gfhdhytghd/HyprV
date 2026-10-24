//! freedesktop Secret Service persistence in NetworkManager's standard schema
//! (`org.freedesktop.NetworkManager.Connection`, the schema libnma/nm-applet
//! and plasma-nm's Secret Service backend use): one item per secret key with
//! attributes `connection-uuid`, `setting-name`, `setting-key`. Lookups do not
//! match the schema name (libnma's `SECRET_SCHEMA_DONT_MATCH_NAME`), so items
//! written by other NetworkManager agents are found. The transport session is
//! `dh-ietf1024-sha256-aes128-cbc-pkcs7`; `plain` is used only when the
//! service refuses it. No plaintext disk fallback.
use crate::{
    bus::{err, proxy},
    dh,
    secret::{Fields, Zeroizing},
    Result,
};
use async_trait::async_trait;
use futures_util::StreamExt;
use std::{collections::HashMap, time::Duration};
use zbus::{
    zvariant::{OwnedObjectPath, OwnedValue, Value},
    Connection,
};

pub const SCHEMA: &str = "org.freedesktop.NetworkManager.Connection";
const SERVICE: &str = "org.freedesktop.secrets";
const SERVICE_PATH: &str = "/org/freedesktop/secrets";
const SERVICE_IFACE: &str = "org.freedesktop.Secret.Service";

#[async_trait]
pub trait SecretStore: Send + Sync {
    /// Stored secrets of one setting (key → value); empty when none.
    async fn load(&self, uuid: &str, setting: &str, interactive: bool) -> Result<Fields>;
    /// Create or replace one item per key. `id` is the connection name (label).
    async fn save(&self, uuid: &str, id: &str, setting: &str, fields: &Fields) -> Result<()>;
    /// Remove the listed keys of one setting.
    async fn remove(&self, uuid: &str, setting: &str, keys: &[String]) -> Result<()>;
    /// Remove every secret of a connection.
    async fn delete(&self, uuid: &str) -> Result<()>;
}
pub struct Keyring;

enum Mode {
    Plain,
    Aes(Zeroizing<[u8; 16]>),
}
struct Session {
    c: Connection,
    path: OwnedObjectPath,
    mode: Mode,
}
/// Session bus for keyring traffic. Test builds refuse the real user bus so no
/// test can read or write the user's actual keyring.
async fn session_bus() -> Result<Connection> {
    #[cfg(test)]
    {
        let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_default();
        if address.is_empty() || address.contains("/run/user/") {
            return Err("tests must not use the real session keyring".into());
        }
    }
    Connection::session().await.map_err(err)
}
impl Session {
    async fn open() -> Result<Self> {
        let c = session_bus().await?;
        let svc = proxy(&c, SERVICE, SERVICE_PATH, SERVICE_IFACE).await?;
        let pair = dh::KeyPair::generate();
        let reply: zbus::Result<(OwnedValue, OwnedObjectPath)> = svc
            .call(
                "OpenSession",
                &(dh::ALGORITHM, Value::from(pair.public.clone())),
            )
            .await;
        let (mode, path) = match reply {
            Ok((output, path)) => {
                let peer = Vec::<u8>::try_from(output).map_err(err)?;
                let key = pair
                    .derive(&peer)
                    .ok_or("Secret Service returned an invalid DH public key")?;
                (Mode::Aes(key), path)
            }
            Err(zbus::Error::MethodError(name, _, _))
                if name.as_str() == "org.freedesktop.DBus.Error.NotSupported" =>
            {
                let (_, path): (OwnedValue, OwnedObjectPath) = svc
                    .call("OpenSession", &("plain", Value::from("")))
                    .await
                    .map_err(err)?;
                (Mode::Plain, path)
            }
            Err(e) => return Err(e.to_string()),
        };
        Ok(Self { c, path, mode })
    }
    /// `(parameters, value)` for a `(oayays)` Secret struct.
    fn encode(&self, secret: &[u8]) -> (Vec<u8>, Zeroizing<Vec<u8>>) {
        match &self.mode {
            Mode::Plain => (vec![], Zeroizing::new(secret.to_vec())),
            Mode::Aes(key) => {
                let (iv, ct) = dh::encrypt(key, secret);
                (iv, Zeroizing::new(ct))
            }
        }
    }
    fn decode(&self, params: &[u8], value: Zeroizing<Vec<u8>>) -> Result<Zeroizing<Vec<u8>>> {
        match &self.mode {
            Mode::Plain => Ok(value),
            Mode::Aes(key) => {
                dh::decrypt(key, params, &value).ok_or_else(|| "secret decryption failed".into())
            }
        }
    }
    async fn close(self) {
        let _ = self
            .c
            .call_method(
                Some(SERVICE),
                self.path.as_str(),
                Some("org.freedesktop.Secret.Session"),
                "Close",
                &(),
            )
            .await;
    }
}

/// Dismisses an outstanding keyring prompt if the waiting request is dropped
/// (e.g. NetworkManager called CancelGetSecrets).
struct PromptGuard {
    c: Connection,
    path: Option<OwnedObjectPath>,
}
impl Drop for PromptGuard {
    fn drop(&mut self) {
        if let (Some(path), Ok(rt)) = (self.path.take(), tokio::runtime::Handle::try_current()) {
            let c = self.c.clone();
            rt.spawn(async move {
                let _ = c
                    .call_method(
                        Some(SERVICE),
                        path.as_str(),
                        Some("org.freedesktop.Secret.Prompt"),
                        "Dismiss",
                        &(),
                    )
                    .await;
            });
        }
    }
}
async fn complete(c: &Connection, path: &OwnedObjectPath) -> Result<Option<OwnedValue>> {
    if path.as_str() == "/" {
        return Ok(None);
    }
    let p = proxy(c, SERVICE, path.as_str(), "org.freedesktop.Secret.Prompt").await?;
    let mut signals = p.receive_signal("Completed").await.map_err(err)?;
    let mut guard = PromptGuard {
        c: c.clone(),
        path: Some(path.clone()),
    };
    p.call::<_, _, ()>("Prompt", &("",)).await.map_err(err)?;
    let signal = match tokio::time::timeout(Duration::from_secs(120), signals.next()).await {
        Ok(Some(s)) => s,
        _ => return Err("keyring prompt timed out".into()), // guard dismisses
    };
    guard.path = None;
    let (dismissed, value): (bool, OwnedValue) = signal.body().deserialize().map_err(err)?;
    if dismissed {
        Err("keyring prompt cancelled".into())
    } else {
        Ok(Some(value))
    }
}
async fn unlock(c: &Connection, paths: Vec<OwnedObjectPath>) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let (_, prompt): (Vec<OwnedObjectPath>, OwnedObjectPath) =
        proxy(c, SERVICE, SERVICE_PATH, SERVICE_IFACE)
            .await?
            .call("Unlock", &(paths,))
            .await
            .map_err(err)?;
    complete(c, &prompt).await?;
    Ok(())
}
/// `SearchItems` without `xdg:schema` (libnma `SECRET_SCHEMA_DONT_MATCH_NAME`).
async fn search(
    c: &Connection,
    attrs: &[(&str, &str)],
) -> Result<(Vec<OwnedObjectPath>, Vec<OwnedObjectPath>)> {
    let attrs: HashMap<&str, &str> = attrs.iter().copied().collect();
    proxy(c, SERVICE, SERVICE_PATH, SERVICE_IFACE)
        .await?
        .call("SearchItems", &(attrs,))
        .await
        .map_err(err)
}
async fn collection(c: &Connection) -> Result<OwnedObjectPath> {
    let p = proxy(c, SERVICE, SERVICE_PATH, SERVICE_IFACE).await?;
    let mut path: OwnedObjectPath = p.call("ReadAlias", &("default",)).await.map_err(err)?;
    if path.as_str() == "/" {
        let props = HashMap::from([(
            "org.freedesktop.Secret.Collection.Label",
            Value::from("Login"),
        )]);
        let (created, prompt): (OwnedObjectPath, OwnedObjectPath) = p
            .call("CreateCollection", &(props, "default"))
            .await
            .map_err(err)?;
        path = match complete(c, &prompt).await? {
            Some(v) => OwnedObjectPath::try_from(v).map_err(err)?,
            None => created,
        };
    }
    unlock(c, vec![path.clone()]).await?;
    Ok(path)
}
async fn delete_items(c: &Connection, items: Vec<OwnedObjectPath>) -> Result<()> {
    for item in items {
        let prompt: OwnedObjectPath =
            proxy(c, SERVICE, item.as_str(), "org.freedesktop.Secret.Item")
                .await?
                .call("Delete", &())
                .await
                .map_err(err)?;
        complete(c, &prompt).await?;
    }
    Ok(())
}
type SecretTuple = (OwnedObjectPath, Vec<u8>, Vec<u8>, String);

#[async_trait]
impl SecretStore for Keyring {
    async fn load(&self, uuid: &str, setting: &str, interactive: bool) -> Result<Fields> {
        let session = Session::open().await?;
        let c = session.c.clone();
        let result = async {
            let query = [("connection-uuid", uuid), ("setting-name", setting)];
            let (mut items, locked) = search(&c, &query).await?;
            if !locked.is_empty() {
                if interactive {
                    unlock(&c, locked).await?;
                    items = search(&c, &query).await?.0;
                } else if items.is_empty() {
                    return Err("keyring locked and interaction disallowed".into());
                }
            }
            let mut fields = Fields::new();
            if items.is_empty() {
                return Ok(fields);
            }
            let secrets: HashMap<OwnedObjectPath, SecretTuple> =
                proxy(&c, SERVICE, SERVICE_PATH, SERVICE_IFACE)
                    .await?
                    .call("GetSecrets", &(items, session.path.clone()))
                    .await
                    .map_err(err)?;
            for (item, (_, params, value, _)) in secrets {
                let value = Zeroizing::new(value);
                let attrs: HashMap<String, String> =
                    proxy(&c, SERVICE, item.as_str(), "org.freedesktop.Secret.Item")
                        .await?
                        .get_property("Attributes")
                        .await
                        .map_err(err)?;
                let Some(key) = attrs.get("setting-key").filter(|k| !k.is_empty()) else {
                    continue;
                };
                let plain = session.decode(&params, value)?;
                if let Ok(text) = std::str::from_utf8(&plain) {
                    fields.insert(key.clone(), Zeroizing::new(text.to_owned()));
                }
            }
            Ok(fields)
        }
        .await;
        session.close().await;
        result
    }
    async fn save(&self, uuid: &str, id: &str, setting: &str, fields: &Fields) -> Result<()> {
        if fields.is_empty() {
            return Ok(());
        }
        let session = Session::open().await?;
        let c = session.c.clone();
        let result = async {
            let collection = collection(&c).await?;
            let coll = proxy(
                &c,
                SERVICE,
                collection.as_str(),
                "org.freedesktop.Secret.Collection",
            )
            .await?;
            for (key, value) in fields {
                let attrs = HashMap::from([
                    ("xdg:schema", SCHEMA),
                    ("connection-uuid", uuid),
                    ("setting-name", setting),
                    ("setting-key", key.as_str()),
                ]);
                let mut props = HashMap::<&str, Value<'_>>::new();
                props.insert(
                    "org.freedesktop.Secret.Item.Label",
                    Value::from(format!("Network secret for {id}/{setting}/{key}")),
                );
                props.insert("org.freedesktop.Secret.Item.Attributes", Value::from(attrs));
                let (params, encoded) = session.encode(value.as_bytes());
                let secret = (session.path.clone(), params, &encoded[..], "text/plain");
                let (_item, prompt): (OwnedObjectPath, OwnedObjectPath) = coll
                    .call("CreateItem", &(props, secret, true))
                    .await
                    .map_err(err)?;
                complete(&c, &prompt).await?;
            }
            Ok(())
        }
        .await;
        session.close().await;
        result
    }
    async fn remove(&self, uuid: &str, setting: &str, keys: &[String]) -> Result<()> {
        if keys.is_empty() {
            return Ok(());
        }
        let c = session_bus().await?;
        for key in keys {
            let (mut items, locked) = search(
                &c,
                &[
                    ("connection-uuid", uuid),
                    ("setting-name", setting),
                    ("setting-key", key),
                ],
            )
            .await?;
            unlock(&c, locked.clone()).await?;
            items.extend(locked);
            delete_items(&c, items).await?;
        }
        Ok(())
    }
    async fn delete(&self, uuid: &str) -> Result<()> {
        let c = session_bus().await?;
        let (mut items, locked) = search(&c, &[("connection-uuid", uuid)]).await?;
        unlock(&c, locked.clone()).await?;
        items.extend(locked);
        delete_items(&c, items).await
    }
}

#[cfg(test)]
#[path = "keyring_tests.rs"]
mod tests;
