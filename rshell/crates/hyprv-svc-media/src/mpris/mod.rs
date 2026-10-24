//! MPRIS actor (replaces `media-status.sh` polling, `playerctl` actions and the
//! `BarServices.qml:86-205` media path). KDE mpris2-engine semantics:
//!
//! * subscribe first (`NameOwnerChanged` arg0namespace, `PropertiesChanged` and `Seeked`
//!   on `/org/mpris/MediaPlayer2`), then snapshot with `ListNames` + `GetAll`;
//! * position is a local anchor (position, CLOCK_MONOTONIC instant, rate) extrapolated by
//!   the UI only while it presents media; it is re-read (one `Get Position`) on playback,
//!   rate or track change, re-anchored by `Seeked`, and re-snapshotted after resume;
//! * no periodic D-Bus reads. The only timers are the 200 ms Cider repair debounce and the
//!   30 s Cider fallback, which runs only while media is presented
//!   ([`MediaPresentationTopic`] has subscribers) and a confirmed Cider player is playing.
pub mod model;

use crate::cider::{CiderAction, CiderTrack};
use crate::common::{action_queue, bump, err, monotonic_us, Context, Counters, Error, Handle, Reply, Requester, Result};
use futures_util::StreamExt;
use hyprv_core::{clock::{SleepState, SleepTopic}, store::{StateStore, Subscription, Topic, TopicKey}};
pub use model::{Capabilities, Metadata, MprisPlayer, MprisState, Playback, PositionAnchor};
use model::{Correction, PREFIX};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, pin::Pin, sync::Arc, time::Duration};
use tokio::sync::{mpsc, watch};
use zbus::{fdo, message::Type as MessageType, names::BusName, zvariant::OwnedValue, MatchRule, MessageStream};

const PATH: &str = "/org/mpris/MediaPlayer2";
const ROOT_IFACE: &str = "org.mpris.MediaPlayer2";
const PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";
/// `compatibilityRefresh.interval`
const COMPAT_DEBOUNCE: Duration = Duration::from_millis(200);
/// Cider fallback retry while visible and playing.
const COMPAT_FALLBACK: Duration = Duration::from_secs(30);
/// `mediaFollowupRefresh.interval` after a seek.
const SEEK_FOLLOWUP: Duration = Duration::from_millis(180);
const CALL_TIMEOUT: Duration = Duration::from_secs(5);
/// playerctld is a proxy that re-exports another player; listing it would duplicate one.
const IGNORED: &[&str] = &["playerctld"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MprisAction {
    /// `playerctl play-pause`
    PlayPause,
    Play,
    Pause,
    /// `playerctl next`
    Next,
    /// `playerctl previous`
    Previous,
    /// `seekMedia(seconds)`: clamp to [0, length] when length is known, absolute.
    Seek(f64),
    /// `focusMediaApp()`.
    Raise,
    /// `refreshMediaStatus()`: one re-snapshot of every player.
    Refresh,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MprisOutcome {
    Done,
    /// The player cannot `Raise()`: focus a compositor client instead (by PID, else by
    /// class like `shell.qml:2916`). Routed by the shell to the Hyprland actor.
    FocusFallback { pid: Option<u32>, class: Option<String> },
}

pub struct MprisTopic;
impl TopicKey for MprisTopic { type Value = MprisState; }
/// Subscribe while media UI is presented (QML `mediaVisible`/`mediaUiVisible`).
pub struct MediaPresentationTopic;
impl TopicKey for MediaPresentationTopic { type Value = (); }
pub type MprisHandle = Handle<MprisState, MprisAction, MprisOutcome>;

/// Held while media UI is presented: desktop `mediaVisible`, laptop `mediaUiVisible`
/// (a bar expanded or the control panel requested). Dropping it ends the demand.
/// Rising demand re-arms the Cider repair once (`onMediaVisibleChanged`); while held, a
/// confirmed playing Cider gets its 30 s fallback. Position needs no read on reveal:
/// the event-maintained anchor is current, so the first presented frame is correct.
pub struct Presentation(#[allow(dead_code)] Subscription<()>);
pub fn present(store: &StateStore) -> Presentation {
    Presentation(store.topic::<MediaPresentationTopic>(()).subscribe())
}
pub type CiderRequester = Requester<CiderAction, Option<CiderTrack>>;

/// Spectrum gate: selected player available and playing.
pub fn playing_gate(ctx: &Context, topic: &Topic<MprisState>) -> watch::Receiver<bool> {
    let (tx, rx) = watch::channel(false);
    // A dedicated subscription is fine: mpris sampling does not depend on its demand.
    let mut subscription = topic.subscribe();
    let mut shutdown = ctx.runtime.shutdown_signal();
    ctx.runtime.spawn(async move {
        loop {
            let s = subscription.current().value;
            tx.send_if_modified(|v| std::mem::replace(v, s.available && s.playing) != (s.available && s.playing));
            tokio::select! {
                r = subscription.changed() => if r.is_err() { break },
                _ = shutdown.changed() => break,
                _ = tx.closed() => break,
            }
        }
    });
    rx
}

pub fn spawn(ctx: &Context, cider: Option<CiderRequester>) -> MprisHandle {
    let topic = ctx.store.topic::<MprisTopic>(MprisState::default());
    let presentation = ctx.store.topic::<MediaPresentationTopic>(());
    let sleep = ctx.store.topic::<SleepTopic>(SleepState::default());
    let (actions, queue) = action_queue::<MprisAction, MprisOutcome>();
    let buses = ctx.buses();
    let counters = ctx.counters.clone();
    let mut shutdown = ctx.runtime.shutdown_signal();
    let publish = topic.clone();
    let task = ctx.runtime.spawn(async move {
        let mut queue = queue;
        let mut backoff = crate::common::Backoff::new();
        loop {
            if let Ok(conn) = buses.session().await {
                backoff.reset();
                let mut actor = Actor::new(conn, publish.clone(), cider.clone(), buses.read_only, counters.clone());
                match actor.run(&mut queue, presentation.demand(), sleep.subscribe(), &mut shutdown).await {
                    Ok(()) => return,
                    Err(_) => { publish.publish(MprisState::default()); }
                }
            }
            // Session bus lost: refuse actions while waiting.
            let delay = backoff.next_delay();
            let sleep = tokio::time::sleep(delay);
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

enum Internal {
    Fetched { bus_name: String, owner: String, player: Box<std::result::Result<MprisPlayer, Error>> },
    Position { bus_name: String, owner: String, generation: u64, position_us: i64, at_us: u64 },
    CiderDone { bus_name: String, key: String, track: Option<CiderTrack> },
}

#[derive(Default)]
struct Compat {
    debounce: Option<Pin<Box<tokio::time::Sleep>>>,
    fallback: Option<tokio::time::Interval>,
    in_flight: bool,
    queued: bool,
    last_key: Option<String>,
    cider_player: Option<String>,
    correction: Option<Correction>,
}

struct Actor {
    conn: zbus::Connection,
    topic: Topic<MprisState>,
    cider: Option<CiderRequester>,
    read_only: bool,
    players: HashMap<String, MprisPlayer>,
    /// bus name → owner for names discovered but not yet fetched, with buffered signals.
    pending: HashMap<String, (String, Vec<zbus::Message>)>,
    next_order: u64,
    internal_tx: mpsc::UnboundedSender<Internal>,
    internal_rx: mpsc::UnboundedReceiver<Internal>,
    compat: Compat,
    visible: bool,
    /// MEDIA-2: bus name → generation of its current position anchor. Every new
    /// authoritative anchor (Seeked, playback/rate/track rebase, local seek, snapshot)
    /// takes a fresh value from `next_anchor`; a Position read is applied only if the
    /// anchor it was requested for is still current.
    anchor_gen: HashMap<String, u64>,
    next_anchor: u64,
    counters: Arc<Counters>,
}

impl Actor {
    fn new(conn: zbus::Connection, topic: Topic<MprisState>, cider: Option<CiderRequester>, read_only: bool, counters: Arc<Counters>) -> Self {
        let (internal_tx, internal_rx) = mpsc::unbounded_channel();
        Self {
            conn, topic, cider, read_only,
            players: HashMap::new(), pending: HashMap::new(), next_order: 0,
            internal_tx, internal_rx, compat: Compat::default(), visible: false,
            anchor_gen: HashMap::new(), next_anchor: 0, counters,
        }
    }

    fn bump_anchor(&mut self, bus_name: &str) -> u64 {
        self.next_anchor += 1;
        self.anchor_gen.insert(bus_name.to_owned(), self.next_anchor);
        self.next_anchor
    }

    async fn run(
        &mut self,
        queue: &mut mpsc::Receiver<(MprisAction, Reply<MprisOutcome>)>,
        mut presentation: watch::Receiver<usize>,
        mut sleep: hyprv_core::store::Subscription<SleepState>,
        shutdown: &mut watch::Receiver<bool>,
    ) -> Result<()> {
        let owners_rule = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .sender("org.freedesktop.DBus")?
            .interface("org.freedesktop.DBus")?
            .member("NameOwnerChanged")?
            .arg0ns("org.mpris.MediaPlayer2")?
            .build();
        let props_rule = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .interface("org.freedesktop.DBus.Properties")?
            .member("PropertiesChanged")?
            .path(PATH)?
            .build();
        let seeked_rule = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .interface(PLAYER_IFACE)?
            .member("Seeked")?
            .path(PATH)?
            .build();
        let mut owners = MessageStream::for_match_rule(owners_rule, &self.conn, Some(64)).await?;
        let mut props = MessageStream::for_match_rule(props_rule, &self.conn, Some(256)).await?;
        let mut seeked = MessageStream::for_match_rule(seeked_rule, &self.conn, Some(64)).await?;
        self.snapshot_all().await?;
        self.visible = *presentation.borrow_and_update() > 0;
        self.publish();
        loop {
            let debounce = async {
                match self.compat.debounce.as_mut() { Some(s) => s.await, None => std::future::pending().await }
            };
            let fallback = async {
                match self.compat.fallback.as_mut() { Some(i) => { i.tick().await; } None => std::future::pending().await }
            };
            tokio::select! {
                m = owners.next() => match m {
                    Some(Ok(m)) => self.on_owner_changed(&m),
                    _ => return err("session bus closed"),
                },
                m = props.next() => match m {
                    Some(Ok(m)) => self.on_properties_changed(m),
                    _ => return err("session bus closed"),
                },
                m = seeked.next() => match m {
                    Some(Ok(m)) => self.on_seeked(&m),
                    _ => return err("session bus closed"),
                },
                Some(i) = self.internal_rx.recv() => self.on_internal(i),
                r = queue.recv() => match r {
                    Some((action, reply)) => self.on_action(action, reply),
                    None => return Ok(()),
                },
                _ = debounce => { self.compat.debounce = None; bump(&self.counters.compat_refreshes); self.refresh_compat(); }
                _ = fallback => { bump(&self.counters.compat_refreshes); self.refresh_compat() }
                r = presentation.changed() => {
                    if r.is_err() { return Ok(()) }
                    let visible = *presentation.borrow_and_update() > 0;
                    if visible && !self.visible { self.schedule_compat(); }
                    self.visible = visible;
                }
                r = sleep.changed_diff() => match r {
                    // Resume (core SleepTopic, published by the shell host): re-anchor.
                    Ok(d) if d.after.value.resume_generation != d.before.value.resume_generation => self.snapshot_all().await?,
                    Ok(_) => {}
                    Err(_) => return Ok(()),
                },
                _ = shutdown.changed() => if *shutdown.borrow() { return Ok(()) },
            }
            bump(&self.counters.mpris_wakeups);
            self.publish();
        }
    }

    async fn snapshot_all(&mut self) -> Result<()> {
        let dbus = fdo::DBusProxy::new(&self.conn).await?;
        for name in dbus.list_names().await? {
            let name = name.to_string();
            if !is_player_name(&name) { continue }
            if let Ok(owner) = dbus.get_name_owner(BusName::try_from(name.as_str())?).await {
                self.discover(name, owner.to_string());
            }
        }
        Ok(())
    }

    fn discover(&mut self, bus_name: String, owner: String) {
        self.pending.insert(bus_name.clone(), (owner.clone(), Vec::new()));
        let conn = self.conn.clone();
        let tx = self.internal_tx.clone();
        tokio::spawn(async move {
            let player = tokio::time::timeout(CALL_TIMEOUT, fetch_player(&conn, &bus_name, &owner)).await
                .unwrap_or_else(|_| err("player did not answer"));
            let _ = tx.send(Internal::Fetched { bus_name, owner, player: Box::new(player) });
        });
    }

    fn on_owner_changed(&mut self, message: &zbus::Message) {
        let Ok((name, old, new)) = message.body().deserialize::<(String, String, String)>() else { return };
        if !is_player_name(&name) { return }
        let _ = old;
        self.players.remove(&name);
        self.pending.remove(&name);
        self.anchor_gen.remove(&name);
        if !new.is_empty() {
            self.discover(name, new);
        }
    }

    fn names_for_owner(&self, owner: &str) -> Vec<String> {
        self.players.values().filter(|p| p.owner == owner).map(|p| p.bus_name.clone()).collect()
    }

    fn on_properties_changed(&mut self, message: zbus::Message) {
        let Some(sender) = message.header().sender().map(|s| s.to_string()) else { return };
        // Buffer for players still being fetched; replayed after insertion.
        let mut buffered = false;
        for (owner, queue) in self.pending.values_mut() {
            if *owner == sender { queue.push(message.clone()); buffered = true; }
        }
        if buffered && self.names_for_owner(&sender).is_empty() { return }
        let Ok((iface, changed, invalidated)) = message.body().deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>() else { return };
        for bus_name in self.names_for_owner(&sender) {
            self.apply_changes(&bus_name, &iface, &changed, &invalidated);
        }
    }

    fn apply_changes(&mut self, bus_name: &str, iface: &str, changed: &HashMap<String, OwnedValue>, invalidated: &[String]) {
        let now = monotonic_us();
        let Some(p) = self.players.get_mut(bus_name) else { return };
        if !invalidated.is_empty() {
            let (b, o) = (p.bus_name.clone(), p.owner.clone());
            self.players.remove(&b);
            self.discover(b, o);
            return;
        }
        let mut reread_position = false;
        let mut new_anchor = false;
        match iface {
            PLAYER_IFACE => {
                let old_track = (p.metadata.track_id.clone(), p.metadata.title.clone(), p.metadata.length_us);
                p.rebase(now);
                apply_player_props(p, changed);
                if changed.contains_key("PlaybackStatus") || changed.contains_key("Rate") {
                    reread_position = true;
                }
                if changed.contains_key("Metadata") && old_track != (p.metadata.track_id.clone(), p.metadata.title.clone(), p.metadata.length_us) {
                    reread_position = true;
                }
                if let Some(pos) = changed.get("Position").and_then(|v| i64::try_from(v).ok()) {
                    p.reanchor(pos, now);
                    reread_position = false;
                    new_anchor = true;
                } else {
                    p.rebase(now);
                }
            }
            ROOT_IFACE => apply_root_props(p, changed),
            _ => {}
        }
        let supported = p.position_supported;
        let (b, o) = (p.bus_name.clone(), p.owner.clone());
        if reread_position || new_anchor {
            self.bump_anchor(&b);
        }
        if reread_position && supported {
            self.read_position(b, o, Duration::ZERO);
        }
    }

    fn on_seeked(&mut self, message: &zbus::Message) {
        let Some(sender) = message.header().sender().map(|s| s.to_string()) else { return };
        let Ok((position,)) = message.body().deserialize::<(i64,)>() else { return };
        let now = monotonic_us();
        for name in self.names_for_owner(&sender) {
            if let Some(p) = self.players.get_mut(&name) {
                p.reanchor(position, now);
                self.bump_anchor(&name);
            }
        }
    }

    /// One `Get Position`, valid only for the anchor generation current at request time.
    fn read_position(&self, bus_name: String, owner: String, delay: Duration) {
        let generation = self.anchor_gen.get(&bus_name).copied().unwrap_or(0);
        bump(&self.counters.position_reads);
        let conn = self.conn.clone();
        let tx = self.internal_tx.clone();
        tokio::spawn(async move {
            if !delay.is_zero() { tokio::time::sleep(delay).await }
            let fut = async {
                let proxy = fdo::PropertiesProxy::builder(&conn).destination(bus_name.as_str())?.path(PATH)?.cache_properties(zbus::proxy::CacheProperties::No).build().await?;
                let v = proxy.get(zbus::names::InterfaceName::from_static_str_unchecked(PLAYER_IFACE), "Position").await?;
                i64::try_from(v).map_err(|e| Error(e.to_string()))
            };
            if let Ok(Ok(position_us)) = tokio::time::timeout(CALL_TIMEOUT, fut).await {
                let _ = tx.send(Internal::Position { bus_name, owner, generation, position_us, at_us: monotonic_us() });
            }
        });
    }

    fn on_internal(&mut self, event: Internal) {
        match event {
            Internal::Fetched { bus_name, owner, player } => {
                let Some((expected, buffered)) = self.pending.get(&bus_name) else { return };
                if *expected != owner { return }
                let buffered = buffered.clone();
                self.pending.remove(&bus_name);
                let Ok(mut player) = *player else { return };
                player.order = self.next_order;
                self.next_order += 1;
                self.players.insert(bus_name.clone(), player);
                self.bump_anchor(&bus_name);
                for message in buffered {
                    if let Ok((iface, changed, invalidated)) = message.body().deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>() {
                        self.apply_changes(&bus_name, &iface, &changed, &invalidated);
                    }
                }
            }
            Internal::Position { bus_name, owner, generation, position_us, at_us } => {
                if self.anchor_gen.get(&bus_name) != Some(&generation) {
                    return; // a newer Seeked/track/seek anchor superseded this read
                }
                if let Some(p) = self.players.get_mut(&bus_name).filter(|p| p.owner == owner) {
                    p.reanchor(position_us, at_us);
                }
            }
            Internal::CiderDone { bus_name, key, track } => {
                self.compat.in_flight = false;
                let current = model::select(&self.players.values().cloned().collect::<Vec<_>>()).cloned();
                if let Some(p) = current.filter(|p| p.bus_name == bus_name && p.snapshot_key() == key) {
                    self.compat.cider_player = Some(bus_name);
                    self.compat.correction = Some(model::merge_cider(&p, key, track.as_ref()));
                }
                if self.compat.queued {
                    self.compat.queued = false;
                    self.schedule_compat();
                }
            }
        }
    }

    fn schedule_compat(&mut self) {
        self.compat.debounce = Some(Box::pin(tokio::time::sleep(COMPAT_DEBOUNCE)));
    }

    /// `refreshCompatibility()`: one queued repair, keyed by the current snapshot.
    fn refresh_compat(&mut self) {
        let players: Vec<MprisPlayer> = self.players.values().cloned().collect();
        let Some(p) = model::select(&players).cloned() else { return };
        if !p.compat_candidate() { return }
        if self.compat.in_flight {
            self.compat.queued = true;
            return;
        }
        if !p.is_cider() { return }
        let key = p.snapshot_key();
        let length = p.metadata.length_us.unwrap_or(0) as f64 / 1e6;
        let tx = self.internal_tx.clone();
        let bus_name = p.bus_name.clone();
        match (&self.cider, p.comm.as_deref() == Some("Cider") && length > 0.0) {
            (Some(cider), true) => {
                self.compat.in_flight = true;
                let cider = cider.clone();
                let query = crate::cider::CiderQuery {
                    artist: p.metadata.artist(),
                    album: p.metadata.album.clone(),
                    length_seconds: (length * 1000.0).round() / 1000.0,
                    current_title: p.metadata.title.clone(),
                };
                tokio::spawn(async move {
                    let track = cider.request(CiderAction::Lookup(query)).await.ok().flatten();
                    let _ = tx.send(Internal::CiderDone { bus_name, key, track });
                });
            }
            _ => {
                self.compat.in_flight = true;
                let _ = tx.send(Internal::CiderDone { bus_name, key, track: None });
            }
        }
    }

    fn on_action(&mut self, action: MprisAction, reply: Reply<MprisOutcome>) {
        if self.read_only {
            let _ = reply.send(err("read-only context"));
            return;
        }
        if action == MprisAction::Refresh {
            let names: Vec<(String, String)> = self.players.values().map(|p| (p.bus_name.clone(), p.owner.clone())).collect();
            for (b, o) in names { self.read_position(b, o, Duration::ZERO) }
            let _ = reply.send(Ok(MprisOutcome::Done));
            return;
        }
        let players: Vec<MprisPlayer> = self.players.values().cloned().collect();
        let Some(p) = model::select(&players).cloned() else {
            let _ = reply.send(err("no media player"));
            return;
        };
        let caps = &p.capabilities;
        let (method, body): (&str, Option<SeekBody>) = match &action {
            MprisAction::PlayPause | MprisAction::Play | MprisAction::Pause if !caps.can_control => {
                let _ = reply.send(err("player cannot be controlled"));
                return;
            }
            MprisAction::PlayPause => ("PlayPause", None),
            MprisAction::Play => ("Play", None),
            MprisAction::Pause => ("Pause", None),
            MprisAction::Next if caps.can_go_next => ("Next", None),
            MprisAction::Previous if caps.can_go_previous => ("Previous", None),
            MprisAction::Next | MprisAction::Previous => {
                let _ = reply.send(err("player does not support this action"));
                return;
            }
            MprisAction::Seek(seconds) => {
                if !seconds.is_finite() || !caps.can_seek {
                    let _ = reply.send(err("player cannot seek"));
                    return;
                }
                let length = p.metadata.length_us.unwrap_or(0) as f64 / 1e6;
                let target = if length > 0.0 { seconds.clamp(0.0, length) } else { seconds.max(0.0) };
                let target_us = (target * 1e6).round() as i64;
                let now = monotonic_us();
                let current = p.position.position_us_at(now);
                // `seekMedia` sets mediaPositionSeconds immediately, then re-reads.
                if let Some(entry) = self.players.get_mut(&p.bus_name) { entry.reanchor(target_us, now) }
                self.bump_anchor(&p.bus_name);
                self.read_position(p.bus_name.clone(), p.owner.clone(), SEEK_FOLLOWUP);
                match &p.metadata.track_id {
                    Some(track) => ("SetPosition", Some(SeekBody::Absolute(track.clone(), target_us))),
                    None => ("Seek", Some(SeekBody::Relative(target_us - current))),
                }
            }
            MprisAction::Raise => {
                if !caps.can_raise {
                    let class = if p.name.to_lowercase().contains("cider") { Some("Cider".into()) }
                        else if p.name.to_lowercase().contains("spotify") { Some("Spotify".into()) } else { None };
                    let pid = p.pid.or_else(|| p.name.rsplit_once("instance").and_then(|(_, n)| n.parse().ok()));
                    let _ = reply.send(Ok(MprisOutcome::FocusFallback { pid, class }));
                    return;
                }
                ("Raise", None)
            }
            MprisAction::Refresh => unreachable!(),
        };
        let iface = if method == "Raise" { ROOT_IFACE } else { PLAYER_IFACE };
        let conn = self.conn.clone();
        let destination = p.bus_name.clone();
        tokio::spawn(async move {
            let call = async {
                let path = zbus::zvariant::ObjectPath::from_static_str_unchecked(PATH);
                match body {
                    None => conn.call_method(Some(destination.as_str()), PATH, Some(iface), method, &()).await,
                    Some(SeekBody::Absolute(track, us)) => {
                        let track = zbus::zvariant::ObjectPath::try_from(track.as_str()).unwrap_or(path);
                        conn.call_method(Some(destination.as_str()), PATH, Some(iface), method, &(track, us)).await
                    }
                    Some(SeekBody::Relative(us)) => conn.call_method(Some(destination.as_str()), PATH, Some(iface), method, &(us,)).await,
                }
            };
            let result = match tokio::time::timeout(CALL_TIMEOUT, call).await {
                Ok(Ok(_)) => Ok(MprisOutcome::Done),
                Ok(Err(e)) => Err(Error::from(e)),
                Err(_) => err("player did not answer"),
            };
            let _ = reply.send(result);
        });
    }

    fn publish(&mut self) {
        let players: Vec<MprisPlayer> = self.players.values().cloned().collect();
        let previous = self.topic.snapshot().value;
        let state = model::flatten(&players, self.compat.correction.as_ref(), &previous, monotonic_us());
        // Cider bookkeeping (BarServices: onMediaSnapshotChanged → compatibilityRefresh).
        let selected = state.selected_player().cloned();
        let key = selected.as_ref().map(|p| p.snapshot_key());
        if key != self.compat.last_key {
            self.compat.last_key = key.clone();
            if selected.is_some() { self.schedule_compat() }
        }
        if selected.is_none() {
            self.compat.correction = None;
            self.compat.cider_player = None;
        }
        let fallback_wanted = self.visible
            && selected.as_ref().is_some_and(|p| p.is_playing() && self.compat.cider_player.as_deref() == Some(p.bus_name.as_str()));
        match (fallback_wanted, self.compat.fallback.is_some()) {
            (true, false) => {
                let mut interval = tokio::time::interval_at(tokio::time::Instant::now() + COMPAT_FALLBACK, COMPAT_FALLBACK);
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                self.compat.fallback = Some(interval);
            }
            (false, true) => self.compat.fallback = None,
            _ => {}
        }
        self.topic.publish(state);
    }
}

enum SeekBody { Absolute(String, i64), Relative(i64) }

fn is_player_name(name: &str) -> bool {
    name.strip_prefix(PREFIX).is_some_and(|rest| !rest.is_empty() && !IGNORED.iter().any(|i| rest == *i || rest.starts_with(&format!("{i}."))))
}

fn bool_prop(map: &HashMap<String, OwnedValue>, key: &str) -> Option<bool> {
    map.get(key).and_then(|v| bool::try_from(v).ok())
}
fn string_prop(map: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| <&str>::try_from(v).ok().map(str::to_owned))
}

pub(crate) fn apply_player_props(p: &mut MprisPlayer, map: &HashMap<String, OwnedValue>) {
    if let Some(s) = string_prop(map, "PlaybackStatus") { p.playback = Playback::parse(&s) }
    if let Some(r) = map.get("Rate").and_then(|v| f64::try_from(v).ok()) { p.rate = r }
    if let Some(m) = map.get("Metadata").and_then(|v| HashMap::<String, OwnedValue>::try_from(v.try_clone().ok()?).ok()) {
        p.metadata = Metadata::parse(&m);
    }
    let c = &mut p.capabilities;
    if let Some(b) = bool_prop(map, "CanControl") { c.can_control = b }
    if let Some(b) = bool_prop(map, "CanPlay") { c.can_play = b }
    if let Some(b) = bool_prop(map, "CanPause") { c.can_pause = b }
    if let Some(b) = bool_prop(map, "CanGoNext") { c.can_go_next = b }
    if let Some(b) = bool_prop(map, "CanGoPrevious") { c.can_go_previous = b }
    if let Some(b) = bool_prop(map, "CanSeek") { c.can_seek = b }
}
pub(crate) fn apply_root_props(p: &mut MprisPlayer, map: &HashMap<String, OwnedValue>) {
    if let Some(s) = string_prop(map, "Identity") { p.identity = s }
    if let Some(s) = string_prop(map, "DesktopEntry") { p.desktop_entry = Some(s).filter(|s| !s.is_empty()) }
    if let Some(b) = bool_prop(map, "CanRaise") { p.capabilities.can_raise = b }
}

async fn fetch_player(conn: &zbus::Connection, bus_name: &str, owner: &str) -> Result<MprisPlayer> {
    let proxy = fdo::PropertiesProxy::builder(conn)
        .destination(bus_name.to_owned())?
        .path(PATH)?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await?;
    let root = proxy.get_all(zbus::names::InterfaceName::from_static_str_unchecked(ROOT_IFACE)).await.unwrap_or_default();
    let player = proxy.get_all(zbus::names::InterfaceName::from_static_str_unchecked(PLAYER_IFACE)).await?;
    let pid = match fdo::DBusProxy::new(conn).await {
        Ok(dbus) => dbus.get_connection_unix_process_id(BusName::try_from(owner)?).await.ok(),
        Err(_) => None,
    };
    let comm = pid.and_then(|pid| std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()).map(|s| s.trim().to_owned());
    let mut p = MprisPlayer {
        bus_name: bus_name.to_owned(),
        name: bus_name.trim_start_matches(PREFIX).to_owned(),
        owner: owner.to_owned(),
        pid,
        comm,
        rate: 1.0,
        ..Default::default()
    };
    let root: HashMap<String, OwnedValue> = root.into_iter().collect();
    let player: HashMap<String, OwnedValue> = player.into_iter().collect();
    apply_root_props(&mut p, &root);
    apply_player_props(&mut p, &player);
    let now = monotonic_us();
    match player.get("Position").and_then(|v| i64::try_from(v).ok()) {
        Some(pos) => { p.position_supported = true; p.reanchor(pos, now) }
        None => p.reanchor(0, now),
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Value;
    #[test]
    fn player_name_filter() {
        assert!(is_player_name("org.mpris.MediaPlayer2.spotify"));
        assert!(is_player_name("org.mpris.MediaPlayer2.chromium.instance123"));
        assert!(!is_player_name("org.mpris.MediaPlayer2.playerctld"));
        assert!(!is_player_name("org.mpris.MediaPlayer2."));
        assert!(!is_player_name("org.kde.foo"));
    }
    #[test]
    fn property_application() {
        let mut p = MprisPlayer::default();
        let mut map = HashMap::new();
        map.insert("PlaybackStatus".to_owned(), OwnedValue::try_from(Value::from("Playing")).unwrap());
        map.insert("Rate".to_owned(), OwnedValue::from(1.5f64));
        map.insert("CanSeek".to_owned(), OwnedValue::from(true));
        let mut md: HashMap<String, Value> = HashMap::new();
        md.insert("xesam:title".into(), Value::from("Song"));
        map.insert("Metadata".to_owned(), OwnedValue::try_from(Value::from(md)).unwrap());
        apply_player_props(&mut p, &map);
        assert_eq!((p.playback, p.rate, p.capabilities.can_seek, p.metadata.title.as_str()), (Playback::Playing, 1.5, true, "Song"));
        let mut root = HashMap::new();
        root.insert("Identity".to_owned(), OwnedValue::try_from(Value::from("Spotify")).unwrap());
        root.insert("CanRaise".to_owned(), OwnedValue::from(true));
        apply_root_props(&mut p, &root);
        assert!(p.capabilities.can_raise && p.identity == "Spotify");
    }
}
