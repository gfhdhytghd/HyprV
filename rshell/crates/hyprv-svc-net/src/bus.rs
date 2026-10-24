use crate::Result;
use futures_util::StreamExt;
use std::collections::HashMap;
use tokio::{
    sync::{mpsc, Mutex},
    task::JoinHandle,
};
use zbus::{
    zvariant::{OwnedObjectPath, OwnedValue},
    Connection, MatchRule, MessageStream, Proxy,
};
pub type Props = HashMap<String, OwnedValue>;
pub type Settings = HashMap<String, Props>;
pub type Objects = HashMap<OwnedObjectPath, HashMap<String, Props>>;
pub const NM: &str = "org.freedesktop.NetworkManager";
pub const NM_ROOT: &str = "/org/freedesktop/NetworkManager";
pub const BLUEZ: &str = "org.bluez";
#[derive(Clone, Debug)]
pub enum Event {
    Changed,
    /// The daemon (re)appeared under a new unique name.
    OwnerChanged,
    /// The daemon's name lost its owner. Nothing may be sent to it now:
    /// any call would D-Bus-activate a daemon the user just stopped.
    OwnerLost,
    Disconnected(String),
}
pub struct Watch {
    receiver: Mutex<mpsc::Receiver<Event>>,
    tasks: Vec<JoinHandle<()>>,
}
impl Watch {
    pub async fn new(connection: &Connection, destination: &'static str) -> Result<Self> {
        let (tx, rx) = mpsc::channel(64);
        let mut tasks = vec![];
        let rules = [
            MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .sender(destination)
                .map_err(err)?
                .build(),
            MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .sender("org.freedesktop.DBus")
                .map_err(err)?
                .interface("org.freedesktop.DBus")
                .map_err(err)?
                .member("NameOwnerChanged")
                .map_err(err)?
                .add_arg(destination)
                .map_err(err)?
                .build(),
        ];
        for (i, rule) in rules.into_iter().enumerate() {
            let mut stream = MessageStream::for_match_rule(rule, connection, Some(256))
                .await
                .map_err(err)?;
            let tx = tx.clone();
            tasks.push(tokio::spawn(async move {
                while let Some(message) = stream.next().await {
                    let event = match message {
                        Ok(m) if i == 1 => match m.body().deserialize::<(String, String, String)>()
                        {
                            Ok((_, _, new_owner)) if new_owner.is_empty() => Event::OwnerLost,
                            _ => Event::OwnerChanged,
                        },
                        Ok(_) => Event::Changed,
                        Err(e) => Event::Disconnected(e.to_string()),
                    };
                    if tx.send(event).await.is_err() {
                        return;
                    }
                }
                let _ = tx
                    .send(Event::Disconnected("D-Bus signal stream ended".into()))
                    .await;
            }));
        }
        Ok(Self {
            receiver: Mutex::new(rx),
            tasks,
        })
    }
    pub async fn next(&self) -> Event {
        self.receiver
            .lock()
            .await
            .recv()
            .await
            .unwrap_or(Event::Disconnected("signal channel closed".into()))
    }
}
impl Drop for Watch {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}
pub fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub async fn proxy<'a>(
    c: &'a Connection,
    dest: &'a str,
    path: &'a str,
    iface: &'a str,
) -> Result<Proxy<'a>> {
    Proxy::new(c, dest, path, iface).await.map_err(err)
}
pub async fn properties(c: &Connection, dest: &str, path: &str, iface: &str) -> Result<Props> {
    let p = proxy(c, dest, path, "org.freedesktop.DBus.Properties").await?;
    call(&p, "GetAll", &(iface,)).await
}
/// Method call with `NO_AUTO_START`: never D-Bus-activates the provider.
pub async fn call<B, R>(p: &Proxy<'_>, method: &str, body: &B) -> Result<R>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
    R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
{
    p.call_with_flags::<_, B, R>(method, zbus::proxy::MethodFlags::NoAutoStart.into(), body)
        .await
        .map_err(err)?
        .ok_or_else(|| format!("{method}: no reply"))
}
/// `NameHasOwner` (asking the bus itself never activates anything).
pub async fn has_owner(c: &Connection, name: &str) -> bool {
    match proxy(
        c,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .await
    {
        Ok(p) => p
            .call::<_, _, bool>("NameHasOwner", &(name,))
            .await
            .unwrap_or(false),
        Err(_) => false,
    }
}
pub async fn call_unit(
    c: &Connection,
    dest: &str,
    path: &str,
    iface: &str,
    method: &str,
) -> Result<()> {
    call(&proxy(c, dest, path, iface).await?, method, &()).await
}
pub fn boolean(p: &Props, k: &str) -> bool {
    p.get(k)
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false)
}
pub fn number(p: &Props, k: &str) -> u32 {
    p.get(k).and_then(|v| u32::try_from(v).ok()).unwrap_or(0)
}
pub fn text(p: &Props, k: &str) -> String {
    p.get(k)
        .and_then(|v| <&str>::try_from(v).ok())
        .unwrap_or("")
        .into()
}
pub fn path(p: &Props, k: &str) -> Option<String> {
    p.get(k)
        .and_then(|v| <&zbus::zvariant::ObjectPath>::try_from(v).ok())
        .map(|p| p.as_str().to_owned())
        .filter(|p| p != "/")
}
pub fn paths(p: &Props, k: &str) -> Vec<OwnedObjectPath> {
    p.get(k)
        .and_then(|v| v.try_clone().ok())
        .and_then(|v| Vec::<OwnedObjectPath>::try_from(v).ok())
        .unwrap_or_default()
}
pub fn bytes(p: &Props, k: &str) -> Vec<u8> {
    p.get(k)
        .and_then(|v| v.try_clone().ok())
        .and_then(|v| Vec::<u8>::try_from(v).ok())
        .unwrap_or_default()
}
/// Agent methods must not disclose secrets to an arbitrary system-bus client.
pub async fn authorize(
    c: &Connection,
    daemon: &str,
    header: &zbus::message::Header<'_>,
) -> Result<()> {
    let owner: String = proxy(
        c,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .await?
    .call("GetNameOwner", &(daemon,))
    .await
    .map_err(err)?;
    if header.sender().map(|s| s.as_str()) == Some(owner.as_str()) {
        Ok(())
    } else {
        Err("agent caller is not the daemon owner".into())
    }
}
#[cfg(test)]
pub fn value_string(s: &str) -> OwnedValue {
    zbus::zvariant::Str::from(s).into()
}
