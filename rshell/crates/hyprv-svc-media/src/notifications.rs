//! SwayNC state and actions over its native D-Bus API (replaces `swaync-client -swb`
//! `shell.qml:3279`, `-t -sw`/`-d -sw` `shell.qml:4350`, `ControlPanelPopup.qml:212`,
//! and `-rs`). Verified against deployed swaync 0.12.6 introspection (2026-10-23):
//! name/path `org.erikreider.swaync.cc` `/org/erikreider/swaync/cc`, methods
//! `GetSubscribeData() -> (bbub)` = (dnd, cc_open, count, inhibited), `ToggleDnd() -> b`,
//! `SetDnd(b)`, `ToggleVisibility()`, `SetVisibility(b)`, `ReloadCss() -> b`, signals
//! `SubscribeV2(u count, b dnd, b cc_open, b inhibited)` and legacy `Subscribe(ubb)`.
//! Snapshot on owner appearance, then signals only. No polling.
use crate::common::{action_queue, err, Backoff, Context, Error, Handle, Reply, Result};
use futures_util::StreamExt;
use hyprv_core::store::{Topic, TopicKey};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::Duration};
use tokio::sync::{mpsc, watch};
use zbus::{fdo, message::Type as MessageType, names::BusName, zvariant::Value, MatchRule, MessageStream};

pub const SWAYNC_NAME: &str = "org.erikreider.swaync.cc";
pub const SWAYNC_PATH: &str = "/org/erikreider/swaync/cc";
const CALL_TIMEOUT: Duration = Duration::from_secs(5);

/// QML: `notificationAlt`, `notificationTooltip`, `dndEnabled` (`shell.qml:1965`),
/// `notificationHasDot` (`shell.qml:501`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NotificationState {
    /// swaync owns its bus name.
    pub available: bool,
    /// waybar `alt`: `[dnd-][inhibited-](notification|none)`; `"none"` before any data.
    pub alt: String,
    /// waybar `tooltip`: `"N Notification(s)"` or empty.
    pub tooltip: String,
    /// waybar `text`.
    pub text: String,
    pub dnd: bool,
    /// Control center visible.
    pub cc_open: bool,
    pub count: u32,
    pub inhibited: bool,
    /// `alt.indexOf("notification") >= 0`
    pub has_dot: bool,
}
impl Default for NotificationState {
    fn default() -> Self {
        Self { available: false, alt: "none".into(), tooltip: String::new(), text: String::new(), dnd: false, cc_open: false, count: 0, inhibited: false, has_dot: false }
    }
}

/// Exactly what `swaync-client -swb` prints for one subscribe event.
pub fn waybar_state(count: u32, dnd: bool, cc_open: bool, inhibited: bool) -> NotificationState {
    let alt = format!(
        "{}{}{}",
        if dnd { "dnd-" } else { "" },
        if inhibited { "inhibited-" } else { "" },
        if count > 0 { "notification" } else { "none" }
    );
    let tooltip = if count > 0 { format!("{count} Notification{}", if count > 1 { "s" } else { "" }) } else { String::new() };
    NotificationState {
        available: true,
        has_dot: alt.contains("notification"),
        text: if count > 0 { count.to_string() } else { String::new() },
        alt,
        tooltip,
        dnd,
        cc_open,
        count,
        inhibited,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum NotificationAction {
    /// `swaync-client -t -sw`
    ToggleVisibility,
    SetVisibility(bool),
    /// `swaync-client -d -sw`
    ToggleDnd,
    SetDnd(bool),
    /// `swaync-client -rs`
    ReloadCss,
    /// freedesktop `Notify` for shell messages (like `notify-send`). Returns the id.
    Notify {
        app_name: String,
        summary: String,
        body: String,
        replaces_id: u32,
        expire_timeout_ms: i32,
        /// `x-canonical-private-synchronous` hint (e.g. `"sys-notify"`).
        synchronous: Option<String>,
    },
}

pub struct NotificationTopic;
impl TopicKey for NotificationTopic { type Value = NotificationState; }
/// Action result: the notification id for `Notify`, otherwise None.
pub type NotificationHandle = Handle<NotificationState, NotificationAction, Option<u32>>;

pub fn spawn(ctx: &Context) -> NotificationHandle {
    let topic = ctx.store.topic::<NotificationTopic>(NotificationState::default());
    let (actions, queue) = action_queue::<NotificationAction, Option<u32>>();
    let buses = ctx.buses();
    let mut shutdown = ctx.runtime.shutdown_signal();
    let publish = topic.clone();
    let task = ctx.runtime.spawn(async move {
        let mut queue = queue;
        let mut backoff = Backoff::new();
        loop {
            if let Ok(conn) = buses.session().await {
                backoff.reset();
                if run(&conn, &publish, &mut queue, &mut shutdown, buses.read_only).await.is_ok() {
                    return;
                }
                mark_unavailable(&publish);
            }
            let sleep = tokio::time::sleep(backoff.next_delay());
            tokio::pin!(sleep);
            loop {
                tokio::select! {
                    _ = &mut sleep => break,
                    r = queue.recv() => match r {
                        Some((_, reply)) => { let _ = reply.send(err("session bus unavailable")); }
                        None => return,
                    },
                    _ = shutdown.changed() => return,
                }
            }
        }
    });
    Handle::new(topic, actions, task)
}

/// Owner loss keeps the last alt/tooltip, like the QML stream that just stops emitting.
fn mark_unavailable(topic: &Topic<NotificationState>) {
    let mut s = topic.snapshot().value;
    s.available = false;
    topic.publish(s);
}

/// A `GetSubscribeData` completion, tagged with the owner epoch it was requested for.
struct Snapshot {
    epoch: u64,
    result: std::result::Result<(bool, bool, u32, bool), Error>,
}

/// The SwayNC owner this actor trusts (MEDIA-4/5): signals are accepted only from
/// `unique`, snapshots only for `epoch`. Every NameOwnerChanged starts a new epoch.
#[derive(Default)]
struct Owner {
    unique: Option<String>,
    epoch: u64,
    /// A signal of this epoch already superseded the snapshot.
    has_signal: bool,
    seen_v2: bool,
}

pub(crate) async fn run(
    conn: &zbus::Connection,
    topic: &Topic<NotificationState>,
    queue: &mut mpsc::Receiver<(NotificationAction, Reply<Option<u32>>)>,
    shutdown: &mut watch::Receiver<bool>,
    read_only: bool,
) -> Result<()> {
    let signal_rule = |member: &'static str| -> Result<MatchRule<'static>> {
        Ok(MatchRule::builder().msg_type(MessageType::Signal).interface(SWAYNC_NAME)?.member(member)?.path(SWAYNC_PATH)?.build())
    };
    // Subscribe first, then resolve the owner and snapshot (subscribe-before-snapshot).
    let mut v2 = MessageStream::for_match_rule(signal_rule("SubscribeV2")?, conn, Some(32)).await?;
    let mut v1 = MessageStream::for_match_rule(signal_rule("Subscribe")?, conn, Some(32)).await?;
    let owner_rule = MatchRule::builder().msg_type(MessageType::Signal).sender("org.freedesktop.DBus")?
        .interface("org.freedesktop.DBus")?.member("NameOwnerChanged")?.arg(0, SWAYNC_NAME)?.build();
    let mut owner_changes = MessageStream::for_match_rule(owner_rule, conn, Some(8)).await?;
    let (tx, mut rx) = mpsc::unbounded_channel::<Snapshot>();
    let snapshot = |tx: mpsc::UnboundedSender<Snapshot>, epoch: u64, unique: String| {
        let conn = conn.clone();
        tokio::spawn(async move {
            // Addressed to the unique name: a replacement daemon cannot answer for it.
            let fut = async {
                let reply = conn.call_method(Some(unique.as_str()), SWAYNC_PATH, Some(SWAYNC_NAME), "GetSubscribeData", &()).await?;
                Ok(reply.body().deserialize::<(bool, bool, u32, bool)>()?)
            };
            let result = tokio::time::timeout(CALL_TIMEOUT, fut).await.unwrap_or_else(|_| err("swaync did not answer"));
            let _ = tx.send(Snapshot { epoch, result });
        });
    };
    let mut owner = Owner::default();
    let initial = fdo::DBusProxy::new(conn).await?.get_name_owner(BusName::try_from(SWAYNC_NAME)?).await.ok().map(|o| o.to_string());
    owner.epoch = 1;
    match initial {
        Some(unique) => {
            owner.unique = Some(unique.clone());
            snapshot(tx.clone(), owner.epoch, unique);
        }
        None => mark_unavailable(topic),
    }
    loop {
        tokio::select! {
            m = v2.next() => match m {
                Some(Ok(m)) => if from_owner(&m, &owner) {
                    if let Ok((count, dnd, cc, inhibited)) = m.body().deserialize::<(u32, bool, bool, bool)>() {
                        owner.seen_v2 = true;
                        owner.has_signal = true;
                        topic.publish(waybar_state(count, dnd, cc, inhibited));
                    }
                },
                _ => return err("session bus closed"),
            },
            m = v1.next() => match m {
                Some(Ok(m)) => if from_owner(&m, &owner) && !owner.seen_v2 {
                    if let Ok((count, dnd, cc)) = m.body().deserialize::<(u32, bool, bool)>() {
                        owner.has_signal = true;
                        let inhibited = topic.snapshot().value.inhibited;
                        topic.publish(waybar_state(count, dnd, cc, inhibited));
                    }
                },
                _ => return err("session bus closed"),
            },
            m = owner_changes.next() => match m {
                Some(Ok(m)) => if let Ok((_, _, new)) = m.body().deserialize::<(String, String, String)>() {
                    owner = Owner { unique: (!new.is_empty()).then(|| new.clone()), epoch: owner.epoch + 1, ..Default::default() };
                    if new.is_empty() { mark_unavailable(topic) } else { snapshot(tx.clone(), owner.epoch, new) }
                },
                _ => return err("session bus closed"),
            },
            Some(s) = rx.recv() => if s.epoch == owner.epoch && owner.unique.is_some() {
                match s.result {
                    // A newer signal of this epoch wins over the snapshot (success or error).
                    Ok((dnd, cc, count, inhibited)) => if !owner.has_signal {
                        topic.publish(waybar_state(count, dnd, cc, inhibited));
                    },
                    Err(_) => if !owner.has_signal { mark_unavailable(topic) },
                }
            },
            r = queue.recv() => match r {
                Some((action, reply)) => {
                    if read_only { let _ = reply.send(err("read-only context")); continue }
                    let conn = conn.clone();
                    tokio::spawn(async move {
                        let r = tokio::time::timeout(CALL_TIMEOUT, perform(&conn, action)).await.unwrap_or_else(|_| err("notification daemon did not answer"));
                        let _ = reply.send(r);
                    });
                }
                None => return Ok(()),
            },
            _ = shutdown.changed() => if *shutdown.borrow() { return Ok(()) },
        }
    }
}

/// MEDIA-4: only the current unique owner may drive the state.
fn from_owner(message: &zbus::Message, owner: &Owner) -> bool {
    match (&owner.unique, message.header().sender()) {
        (Some(unique), Some(sender)) => sender.as_str() == unique,
        _ => false,
    }
}

async fn perform(conn: &zbus::Connection, action: NotificationAction) -> Result<Option<u32>> {
    let call = |method: &'static str| async move {
        conn.call_method(Some(SWAYNC_NAME), SWAYNC_PATH, Some(SWAYNC_NAME), method, &()).await.map(|_| None).map_err(Error::from)
    };
    match action {
        NotificationAction::ToggleVisibility => call("ToggleVisibility").await,
        NotificationAction::ToggleDnd => call("ToggleDnd").await,
        NotificationAction::ReloadCss => call("ReloadCss").await,
        NotificationAction::SetDnd(b) => conn.call_method(Some(SWAYNC_NAME), SWAYNC_PATH, Some(SWAYNC_NAME), "SetDnd", &(b,)).await.map(|_| None).map_err(Error::from),
        NotificationAction::SetVisibility(b) => conn.call_method(Some(SWAYNC_NAME), SWAYNC_PATH, Some(SWAYNC_NAME), "SetVisibility", &(b,)).await.map(|_| None).map_err(Error::from),
        NotificationAction::Notify { app_name, summary, body, replaces_id, expire_timeout_ms, synchronous } => {
            let mut hints: HashMap<&str, Value> = HashMap::new();
            if let Some(tag) = &synchronous {
                hints.insert("x-canonical-private-synchronous", Value::from(tag.as_str()));
            }
            let reply = conn.call_method(
                Some("org.freedesktop.Notifications"),
                "/org/freedesktop/Notifications",
                Some("org.freedesktop.Notifications"),
                "Notify",
                &(app_name.as_str(), replaces_id, "", summary.as_str(), body.as_str(), Vec::<&str>::new(), hints, expire_timeout_ms),
            ).await?;
            Ok(Some(reply.body().deserialize::<u32>()?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn waybar_format_matches_swaync_client() {
        // Live capture: {"text": "151", "alt": "dnd-notification", "tooltip": "151 Notifications", ...}
        let s = waybar_state(151, true, false, false);
        assert_eq!((s.alt.as_str(), s.tooltip.as_str(), s.text.as_str()), ("dnd-notification", "151 Notifications", "151"));
        assert!(s.has_dot && s.dnd);
        let s = waybar_state(1, false, true, true);
        assert_eq!((s.alt.as_str(), s.tooltip.as_str()), ("inhibited-notification", "1 Notification"));
        let s = waybar_state(0, false, false, false);
        assert_eq!((s.alt.as_str(), s.tooltip.as_str(), s.has_dot), ("none", "", false));
        assert_eq!(NotificationState::default().alt, "none");
    }
}
