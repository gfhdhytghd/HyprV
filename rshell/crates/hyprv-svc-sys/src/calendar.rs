//! dcal NDJSON with native, event-driven discovery; never executes the dcal CLI.
#[path = "calendar_discovery.rs"]
mod discovery;
use crate::support::*;
use chrono::{DateTime, Datelike, Days, Local, NaiveDate, Utc};
use hyprv_core::store::{Topic, TopicKey};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{path::PathBuf, time::Duration};
use tokio::io::AsyncWriteExt;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CalendarInfo {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub visible: bool,
    pub hidden: bool,
    pub holds_tasks: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub uid: String,
    pub calendar_id: String,
    pub title: String,
    pub start_iso: String,
    pub end_iso: String,
    pub all_day: bool,
    pub color: Option<String>,
    pub location: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub socket: Option<PathBuf>,
    pub connected: bool,
    pub subscribed: bool,
    pub loading: bool,
    pub week_start_local: String,
    pub request_serial: u64,
    pub calendars: Vec<CalendarInfo>,
    pub events: Vec<CalendarEvent>,
    pub last_error: Option<Error>,
    pub child_window_requested: Option<u64>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            socket: None,
            connected: false,
            subscribed: false,
            loading: false,
            week_start_local: monday(Local::now().date_naive()).to_string(),
            request_serial: 0,
            calendars: vec![],
            events: vec![],
            last_error: None,
            child_window_requested: None,
        }
    }
}
impl Model for State {
    fn failure(&mut self, e: Error) {
        self.connected = false;
        self.subscribed = false;
        self.loading = false;
        self.last_error = Some(e);
    }
    fn action_failure(&mut self, error: Error) {
        self.last_error = Some(error);
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Action {
    Week(String),
    PreviousWeek,
    NextWeek,
    Today,
    Refresh,
    OpenEvent { uid: String, start_iso: String },
    NewEvent,
    Show,
    ConfigureSocket(PathBuf),
}
#[derive(Clone, Debug, Default)]
pub struct Config {
    pub socket: Option<PathBuf>,
    pub discovery_file: Option<PathBuf>,
    /// Override the runtime directory for isolated tests; normally XDG_RUNTIME_DIR.
    pub runtime_dir: Option<PathBuf>,
}
impl Config {
    pub fn from_env() -> Self {
        Self {
            socket: std::env::var_os("HYPRV_DCAL_SOCKET").map(PathBuf::from),
            discovery_file: std::env::var_os("HYPRV_DCAL_DISCOVERY_FILE").map(PathBuf::from),
            runtime_dir: None,
        }
    }
}
pub struct Key;
impl TopicKey for Key {
    type Value = State;
}
fn monday(d: NaiveDate) -> NaiveDate {
    d - Days::new(d.weekday().num_days_from_monday() as u64)
}
fn range(date: &str) -> Result<(String, String)> {
    let d = NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|e| Error::new("date", e))?;
    let local = |d: NaiveDate| -> Result<String> {
        let dt = d
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(Local)
            .earliest()
            .ok_or_else(|| Error::new("date", "local midnight unavailable"))?;
        Ok(dt
            .with_timezone(&Utc)
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
    };
    Ok((local(d - Days::new(7))?, local(d + Days::new(14))?))
}
fn string(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().into()
}
fn event(v: &Value) -> Result<CalendarEvent> {
    let start = string(v, "start");
    let end = string(v, "end");
    for date in [&start, &end] {
        if NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err()
            && DateTime::parse_from_rfc3339(date).is_err()
        {
            return Err(Error::new("date", "invalid calendar event date"));
        }
    }
    Ok(CalendarEvent {
        uid: string(v, "uid"),
        calendar_id: string(v, "calendarId"),
        title: string(v, "summary"),
        start_iso: start,
        end_iso: end,
        all_day: v["allDay"].as_bool().unwrap_or(false),
        color: v["color"].as_str().map(str::to_owned),
        location: v["location"].as_str().map(str::to_owned),
    })
}
struct Channel {
    read: Lines,
    write: tokio::net::unix::OwnedWriteHalf,
}
impl Channel {
    async fn connect(path: &std::path::Path) -> Result<Self> {
        let (r, w) = tokio::net::UnixStream::connect(path).await?.into_split();
        Ok(Self {
            read: lines(r),
            write: w,
        })
    }
    async fn send(&mut self, v: Value) -> Result<()> {
        let mut bytes = serde_json::to_vec(&v)?;
        bytes.push(b'\n');
        self.write.write_all(&bytes).await?;
        Ok(())
    }
}
struct PendingRequest {
    id: u64,
    method: String,
    params: Value,
    wire: Vec<u8>,
    written: usize,
    deadline: tokio::time::Instant,
}
struct Native {
    state: State,
    updates: Topic<State>,
    config: Config,
    request: Option<Channel>,
    subscription: Option<Channel>,
    id: u64,
    watch: Option<FileWatch>,
    socket_identity: Option<(u64,u64)>,
    visible: bool,
    dirty: bool,
    refresh_at: Option<tokio::time::Instant>,
    pending: Option<PendingRequest>,
    calendars_dirty: bool,
}
impl Native {
    fn runtime_dir(&self) -> PathBuf {
        self.config.runtime_dir.clone().unwrap_or_else(discovery::runtime_dir)
    }
    async fn path(&self) -> Result<PathBuf> {
        if let Some(ref p) = self.config.socket {
            return Ok(p.clone());
        }
        if let Some(ref p) = self.config.discovery_file {
            let v: Value = serde_json::from_slice(&std::fs::read(p)?)?;
            return v["path"]
                .as_str()
                .filter(|p| p.starts_with('/'))
                .map(PathBuf::from)
                .ok_or_else(|| Error::new("discovery", "discovery JSON requires absolute path"));
        }
        discovery::find(&self.runtime_dir()).await
    }
    async fn rpc(&mut self, method: &str, params: Value) -> Result<Value> {
        if let Some(pending) = &self.pending {
            if pending.method != method || pending.params != params {
                return Err(Error::new("busy", "another calendar request is pending"));
            }
        } else {
            self.id += 1;
            let mut wire =
                serde_json::to_vec(&json!({"id":self.id,"method":method,"params":params}))?;
            if wire.len() > 1024 * 1024 {
                return Err(Error::new("frame", "calendar request too large"));
            }
            wire.push(b'\n');
            self.pending = Some(PendingRequest {
                id: self.id,
                method: method.into(),
                params,
                wire,
                written: 0,
                deadline: tokio::time::Instant::now() + Duration::from_secs(10),
            });
        }
        let pending = self.pending.as_mut().unwrap();
        let channel = self
            .request
            .as_mut()
            .ok_or_else(|| Error::new("socket", "request channel absent"))?;
        let result = tokio::time::timeout_at(pending.deadline, async {
            while pending.written < pending.wire.len() {
                let count = channel
                    .write
                    .write(&pending.wire[pending.written..])
                    .await?;
                if count == 0 {
                    return Err(Error::new("disconnected", "calendar request socket closed"));
                }
                pending.written += count;
            }
            loop {
                let value: Value = serde_json::from_str(&line(&mut channel.read).await?)?;
                if value["id"] != pending.id {
                    continue;
                }
                if let Some(error) = value.get("error") {
                    return Err(Error::new("rpc", error));
                }
                return Ok(value["result"].clone());
            }
        })
        .await
        .map_err(|_| Error::new("timeout", "dcal request expired"));
        // The request/offset/deadline survive cancellation of this future. Only
        // an actual reply, timeout, disconnect or explicit reconnect clears it.
        self.pending = None;
        result?
    }
    async fn resume_updates(&mut self) -> Result<bool> {
        if self.calendars_dirty {
            self.calendars().await?;
            return Ok(true);
        }
        if self
            .pending
            .as_ref()
            .is_some_and(|request| request.method == "events.list")
        {
            self.week().await?;
            return Ok(true);
        }
        Ok(false)
    }
    async fn calendars(&mut self) -> Result<()> {
        self.calendars_dirty = true;
        let v = self.rpc("calendars.list", json!({})).await?;
        let a = v
            .as_array()
            .ok_or_else(|| Error::new("schema", "calendars.list must return array"))?;
        if a.len() > 4096 {
            return Err(Error::new("limit", "too many calendars"));
        }
        self.state.calendars = a
            .iter()
            .map(|c| {
                let hidden = c["hidden"].as_bool().unwrap_or(false);
                let holds_tasks = c["holdsTasks"].as_bool().unwrap_or(false);
                CalendarInfo {
                    id: string(c, "id"),
                    name: c["summary"]
                        .as_str()
                        .or_else(|| c["name"].as_str())
                        .unwrap_or_default()
                        .into(),
                    color: c["color"].as_str().map(str::to_owned),
                    visible: !hidden && !holds_tasks,
                    hidden,
                    holds_tasks,
                }
            })
            .collect();
        self.calendars_dirty = false;
        Ok(())
    }
    async fn week(&mut self) -> Result<()> {
        if self
            .pending
            .as_ref()
            .is_none_or(|request| request.method != "events.list")
        {
            self.state.request_serial += 1;
        }
        let serial = self.state.request_serial;
        let (from, to) = range(&self.state.week_start_local)?;
        self.state.loading = true;
        self.updates.publish(self.state.clone());
        let result = self
            .rpc("events.list", json!({"from":from,"to":to,"limit":2000}))
            .await;
        self.state.loading = false;
        let result = result?;
        if serial == self.state.request_serial {
            let events = result["events"]
                .as_array()
                .ok_or_else(|| Error::new("schema", "events.list requires events array"))?;
            if events.len() > 2000 {
                return Err(Error::new("limit", "too many events"));
            }
            self.state.events = events.iter().map(event).collect::<Result<_>>()?;
            self.state.last_error = None;
        }
        Ok(())
    }
}
#[async_trait::async_trait]
impl Backend for Native {
    type State = State;
    type Action = Action;
    fn state(&self) -> &State {
        &self.state
    }
    fn state_mut(&mut self) -> &mut State {
        &mut self.state
    }
    fn reconnect_on_timer(&self) -> bool {
        self.config.socket.is_some() || self.config.discovery_file.is_some() || self.state.socket.is_some()
    }
    async fn reconnect_event(&mut self) -> Result<()> {
        match self.watch.as_mut() { Some(watch)=>watch.changed().await, None=>std::future::pending().await }
    }
    async fn connect(&mut self) -> Result<()> {
        self.state.connected = false;
        self.state.socket = None;
        self.socket_identity = None;
        self.request = None;
        self.subscription = None;
        self.pending = None;
        self.calendars_dirty = false;
        // Arm discovery before the scan so a startup/restart cannot be lost.
        self.watch = Some(if let Some(path)=self.config.socket.as_ref().or(self.config.discovery_file.as_ref()) {
            FileWatch::new(vec![path.clone()])?
        } else {
            FileWatch::directory(self.runtime_dir(), "dankcal-", ".sock")?
        });
        let p = self.path().await?;
        if self.config.socket.is_none() {
            if let Some(file)=&self.config.discovery_file {
                self.watch=Some(FileWatch::new(vec![file.clone(),p.clone()])?);
                if self.path().await?!=p { return Err(Error::new("discovery","discovery file changed while arming socket watch")); }
            }
        }
        self.state.socket = Some(p.clone());
        self.socket_identity = Some(discovery::identity(&p)?);
        self.subscription = Some(Channel::connect(&p).await?);
        self.id += 1;
        self.subscription.as_mut().unwrap().send(json!({"id":self.id,"method":"subscribe","params":{"topics":["accounts","calendars","events","sync"]}})).await?;
        self.request = Some(Channel::connect(&p).await?);
        self.state.socket = Some(p);
        self.state.connected = true;
        self.state.subscribed = true;
        self.calendars().await?;
        self.state.last_error = None;
        self.dirty = true;
        self.refresh_at = Some(tokio::time::Instant::now());
        Ok(())
    }
    async fn demand(&mut self, active: bool) -> Result<()> {
        self.visible = active;
        if !active {
            self.state.loading = false;
        }
        if active {
            self.resume_updates().await?;
            self.week().await?;
            self.dirty = false;
        }
        Ok(())
    }
    async fn next(&mut self) -> Result<()> {
        if self.resume_updates().await? {
            return Ok(());
        }
        loop {
            let deadline = if self.visible && self.dirty {
                self.refresh_at
            } else {
                None
            };
            tokio::select! {
                _ = async { if let Some(deadline)=deadline { tokio::time::sleep_until(deadline).await } else { std::future::pending().await } } => {
                    self.week().await?; self.dirty=false; self.refresh_at=None; return Ok(());
                },
                result=line(&mut self.subscription.as_mut().unwrap().read)=>{
                    let value:Value=serde_json::from_str(&result?)?;
                    if let Some(error)=value.get("error") { return Err(Error::new("subscribe",error)); }
                    match value["event"].as_str() {
                        Some("accounts"|"calendars")=>{self.dirty=true;self.refresh_at=Some(tokio::time::Instant::now()+Duration::from_millis(350));self.calendars().await?;return Ok(());},
                        Some("events"|"sync")=>{self.dirty=true;self.refresh_at=Some(tokio::time::Instant::now()+Duration::from_millis(350));return Ok(());},
                        _=>{}
                    }
                },
                result=self.watch.as_mut().unwrap().changed()=>{result?;if (self.config.socket.is_none() && self.config.discovery_file.is_none()) || self.path().await?!=self.state.socket.clone().unwrap() || Some(discovery::identity(self.state.socket.as_ref().unwrap())?)!=self.socket_identity {return Err(Error::new("discovery","dcal socket changed"));}}
            }
        }
    }
    async fn offline_action(&mut self, _id: u64, action: Action) -> Result<Value> {
        match action {
            Action::ConfigureSocket(path) => {
                self.config.socket = Some(path);
                Ok(json!({"reconnecting":true}))
            }
            _ => Err(Error::new("unavailable", "calendar disconnected")),
        }
    }
    async fn action(&mut self, id: u64, a: Action) -> Result<Value> {
        if !matches!(a, Action::ConfigureSocket(_)) {
            self.resume_updates().await?;
        }
        match a {
            Action::Week(d) => {
                self.state.week_start_local = monday(
                    NaiveDate::parse_from_str(&d, "%Y-%m-%d").map_err(|e| Error::new("date", e))?,
                )
                .to_string();
                self.week().await?;
            }
            Action::PreviousWeek | Action::NextWeek => {
                let d =
                    NaiveDate::parse_from_str(&self.state.week_start_local, "%Y-%m-%d").unwrap();
                self.state.week_start_local = if matches!(a, Action::PreviousWeek) {
                    d - Days::new(7)
                } else {
                    d + Days::new(7)
                }
                .to_string();
                self.week().await?;
            }
            Action::Today => {
                self.state.week_start_local = monday(Local::now().date_naive()).to_string();
                self.week().await?;
            }
            Action::Refresh => {
                self.calendars().await?;
                self.week().await?;
            }
            Action::OpenEvent { uid, start_iso } => {
                if uid.is_empty() {
                    return Err(Error::new("event", "empty UID"));
                }
                self.state.child_window_requested = Some(id);
                self.updates.publish(self.state.clone());
                return self
                    .rpc(
                        "ui.openEvent",
                        json!({"uid":uid,"start":start_iso,"editorOnly":true}),
                    )
                    .await;
            }
            Action::NewEvent => {
                self.state.child_window_requested = Some(id);
                self.updates.publish(self.state.clone());
                return self.rpc("ui.newEvent", json!({"editorOnly":true})).await;
            }
            Action::Show => return self.rpc("ui.show", json!({"view":"week"})).await,
            Action::ConfigureSocket(path) => {
                self.config.socket = Some(path);
                self.connect().await?;
            }
        }
        Ok(Value::Null)
    }
}
pub fn spawn(ctx: Context) -> Handle<State, Action> {
    spawn_with_config(ctx, Config::from_env())
}
pub fn spawn_with_config(ctx: Context, config: Config) -> Handle<State, Action> {
    let state = State::default();
    let updates = ctx.store.topic::<Key>(state.clone());
    launch::<Key, _>(
        ctx,
        Native {
            state,
            updates,
            config,
            request: None,
            subscription: None,
            id: 0,
            watch: None,
            socket_identity: None,
            visible: false,
            dirty: false,
            refresh_at: None,
            pending: None,
            calendars_dirty: false,
        },
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wire_dates_and_recurring_identity() {
        let v = json!({"uid":"x","calendarId":"cal","summary":"All day","start":"2026-10-23","end":"2026-10-24","allDay":true});
        let e = event(&v).unwrap();
        assert_eq!(e.calendar_id, "cal");
        assert_eq!(e.start_iso, "2026-10-23");
        assert!(e.all_day);
        assert_eq!(
            monday(NaiveDate::from_ymd_opt(2026, 10, 23).unwrap()).to_string(),
            "2026-10-19"
        );
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    #[tokio::test]
    async fn two_channels_correlate_responses_and_refresh_events() {
        let dir = crate::support::TestDir::new("dcal");
        let path = dir.socket_dir().join("calendar.sock");
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let (events, ready) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (sub, _) = listener.accept().await.unwrap();
            let (read, write) = sub.into_split();
            let mut reader = lines(read);
            let subscribe: Value = serde_json::from_str(&line(&mut reader).await.unwrap()).unwrap();
            assert_eq!(subscribe["method"], "subscribe");
            assert_eq!(
                subscribe["params"]["topics"],
                json!(["accounts", "calendars", "events", "sync"])
            );
            events.send(write).unwrap();
            let (req, _) = listener.accept().await.unwrap();
            let (read, mut write) = req.into_split();
            let mut reader = lines(read);
            while let Ok(text) = line(&mut reader).await {
                let request: Value = serde_json::from_str(&text).unwrap();
                let result = match request["method"].as_str().unwrap() {
                    "calendars.list" => {
                        json!([{"id":"c","summary":"Personal","color":"#112233","hidden":false,"holdsTasks":false}])
                    }
                    "events.list" => {
                        assert_eq!(request["params"]["limit"], 2000);
                        json!({"events":[{"uid":"recurring","calendarId":"c","summary":"Holiday","start":"2026-10-23","end":"2026-10-24","allDay":true}]})
                    }
                    "ui.openEvent" => {
                        assert_eq!(request["params"]["editorOnly"], true);
                        assert_eq!(request["params"]["start"], "2026-10-23");
                        json!({"opened":true})
                    }
                    method => panic!("unexpected {method}"),
                };
                // Old/unknown IDs may arrive after cancellation and cannot win.
                write
                    .write_all(b"{\"id\":0,\"result\":{\"wrong\":true}}\n")
                    .await
                    .unwrap();
                let reply = format!("{}\n", json!({"id":request["id"],"result":result}));
                write.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        let state = State::default();
        let mut native = Native {
            updates: Topic::new(state.clone()),
            state,
            config: Config {
                socket: Some(path),
                discovery_file: None,
                runtime_dir: None,
            },
            request: None,
            subscription: None,
            id: 0,
            watch: None,
            socket_identity: None,
            visible: false,
            dirty: false,
            refresh_at: None,
            pending: None,
            calendars_dirty: false,
        };
        native.connect().await.unwrap();
        let mut events = ready.await.unwrap();
        assert!(native.state.subscribed);
        assert_eq!(native.state.calendars[0].name, "Personal");
        native.demand(true).await.unwrap();
        assert_eq!(native.state.events[0].start_iso, "2026-10-23");
        assert!(!native.state.loading);
        let serial = native.state.request_serial;
        events.write_all(b"{\"event\":\"events\"}\n").await.unwrap();
        native.next().await.unwrap();
        assert!(native.dirty);
        native.next().await.unwrap();
        assert!(native.state.request_serial > serial);
        let result = native
            .action(
                77,
                Action::OpenEvent {
                    uid: "recurring".into(),
                    start_iso: "2026-10-23".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(result["opened"], true);
        assert_eq!(native.state.child_window_requested, Some(77));
        drop(native);
        drop(events);
        server.await.unwrap();
    }
}

#[cfg(test)]
mod adversarial_review {
    use super::*;
    #[tokio::test]
    async fn review_sys_calendar_refresh_survives_next_cancellation() {
        let dir = crate::support::TestDir::new("review-calendar");
        let (request, peer) = tokio::net::UnixStream::pair().unwrap();
        let (read, write) = request.into_split();
        let (subscription, mut changes) = tokio::net::UnixStream::pair().unwrap();
        let (sr, sw) = subscription.into_split();
        let mut native = Native {
            state: State::default(),
            updates: Topic::new(State::default()),
            config: Config {
                socket: Some(dir.0.join("dcal.sock")),
                discovery_file: None,
                runtime_dir: None,
            },
            request: Some(Channel {
                read: lines(read),
                write,
            }),
            subscription: Some(Channel {
                read: lines(sr),
                write: sw,
            }),
            id: 0,
            watch: Some(FileWatch::new(vec![dir.0.join("dcal.sock")]).unwrap()),
            socket_identity: None,
            visible: false,
            dirty: false,
            refresh_at: None,
            pending: None,
            calendars_dirty: false,
        };
        native.state.socket = native.config.socket.clone();
        native.state.calendars = vec![CalendarInfo {
            id: "1".into(),
            name: "old".into(),
            color: None,
            visible: true,
            hidden: false,
            holds_tasks: false,
        }];
        let (announced, wait) = tokio::sync::oneshot::channel();
        let (release, released) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (r, mut w) = peer.into_split();
            let mut r = lines(r);
            let request: Value = serde_json::from_str(&line(&mut r).await.unwrap()).unwrap();
            assert_eq!(request["method"], "calendars.list");
            announced.send(()).unwrap();
            released.await.unwrap();
            w.write_all(
                format!(
                    "{}\n",
                    json!({"id":request["id"],"result":[{"id":"1","name":"new"}]})
                )
                .as_bytes(),
            )
            .await
            .unwrap();
            w
        });
        changes
            .write_all(b"{\"event\":\"calendars\"}\n")
            .await
            .unwrap();
        let mut pending = Box::pin(native.next());
        tokio::select! {_=wait=>{},r=&mut pending=>panic!("refresh unexpectedly completed: {r:?}")};
        drop(pending);
        native.demand(false).await.unwrap();
        release.send(()).unwrap();
        let _keep_request_peer = server.await.unwrap();
        let _ = tokio::time::timeout(Duration::from_millis(50), native.next()).await;
        assert_eq!(
            native.state.calendars[0].name, "new",
            "a demand transition cancelled next() after it consumed the only calendar notification"
        );
    }
}

#[cfg(test)]
mod discovery_actor_tests {
    use super::*;
    use hyprv_core::{runtime::ServiceRuntime, service::Service, store::StateStore};
    use std::sync::Arc;

    fn serve(listener: tokio::net::UnixListener, label: &'static str) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                let (socket,_) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let (read,mut write)=socket.into_split();let mut read=lines(read);
                    // Real dcal sends capabilities before requests; discovery probes close immediately.
                    if write.write_all(b"{\"apiVersion\":1,\"capabilities\":[\"calendars\",\"events\"]}\n").await.is_err(){return;}
                    while let Ok(line)=line(&mut read).await {
                        let request:Value=serde_json::from_str(&line).unwrap();
                        let result=match request["method"].as_str().unwrap() {
                            "subscribe"=>json!({"topics":["calendars","events"]}),
                            "calendars.list"=>json!([{"id":"calendar","summary":label}]),
                            "events.list"=>json!({"events":[]}),
                            other=>panic!("unexpected non-read-only request: {other}"),
                        };
                        if write.write_all(format!("{}\n",json!({"id":request["id"],"result":result})).as_bytes()).await.is_err(){return;}
                    }
                });
            }
        })
    }
    async fn wait_for(handle:&Handle<State,Action>, label:&str) {
        tokio::time::timeout(Duration::from_secs(3),async {
            loop {
                let state=handle.state.snapshot().value;
                if state.connected && state.subscribed && !state.loading && state.request_serial>0 && state.calendars.first().is_some_and(|v|v.name==label){break;}
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }).await.unwrap();
    }
    #[tokio::test]
    async fn absent_provider_parks_then_socket_creation_and_same_path_restart_reconnect() {
        let dir=crate::support::TestDir::new("calendar-actor-discovery");let root=dir.socket_dir();
        let runtime=Arc::new(ServiceRuntime::new().unwrap());
        let ctx=Context{runtime:runtime.clone(),store:Arc::new(StateStore::default())};
        let mut actor=spawn_with_config(ctx,Config{runtime_dir:Some(root.clone()),..Config::default()});
        let _demand=actor.state.subscribe();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!actor.state.snapshot().value.connected);
        let revision=actor.state.snapshot().revision;
        for n in 1..20 {std::fs::write(root.join(format!("dankcal-{n}.session")),b"stale").unwrap();}
        tokio::time::sleep(Duration::from_millis(550)).await;
        assert_eq!(actor.state.snapshot().revision,revision,"absent dcal must not poll or respond to stale session writes");
        let path=root.join(format!("dankcal-{}.sock",std::process::id()));
        let first=serve(tokio::net::UnixListener::bind(&path).unwrap(),"first");
        wait_for(&actor,"first").await;
        // The old accepted channels remain alive: reconnect must come from the
        // runtime-directory event, not only EOF or an environment override.
        first.abort();let _=first.await;std::fs::remove_file(&path).unwrap();
        let second=serve(tokio::net::UnixListener::bind(&path).unwrap(),"restarted");
        wait_for(&actor,"restarted").await;
        actor.stop().await.unwrap();second.abort();
        runtime.request_shutdown();
    }
    #[tokio::test]
    async fn explicit_socket_and_discovery_file_overrides_keep_precedence() {
        let dir=crate::support::TestDir::new("calendar-config-hooks");let root=dir.socket_dir();
        let socket=root.join("test-hook.sock");let server=serve(tokio::net::UnixListener::bind(&socket).unwrap(),"override");
        let json_file=root.join("discovery.json");std::fs::write(&json_file,json!({"path":socket}).to_string()).unwrap();
        let runtime=Arc::new(ServiceRuntime::new().unwrap());
        for config in [Config{socket:Some(socket.clone()),discovery_file:Some(root.join("missing")),runtime_dir:Some(root.clone())},Config{discovery_file:Some(json_file.clone()),runtime_dir:Some(root.clone()),..Config::default()}] {
            let ctx=Context{runtime:runtime.clone(),store:Arc::new(StateStore::default())};
            let mut actor=spawn_with_config(ctx,config);let _demand=actor.state.subscribe();
            wait_for(&actor,"override").await;actor.stop().await.unwrap();
        }
        server.abort();runtime.request_shutdown();
    }
}
