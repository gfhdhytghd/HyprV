//! Clash/Mihomo controller API. Secrets stay in private transport configuration.
use crate::{
    support::*,
    systemd::{BusPool, Manager, Signals, UnitOperation, UnitScope, UnitState},
};
use futures_util::{
    stream::{BoxStream, SelectAll},
    StreamExt,
};
use hyprv_core::store::TopicKey;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{path::PathBuf, time::Duration};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{client::IntoClientRequest, Message},
};
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub online: bool,
    pub unit: Option<UnitState>,
    pub legacy_service_scope: String,
    pub version: String,
    pub mode: String,
    pub connections: u64,
    pub upload_bytes_per_s: u64,
    pub download_bytes_per_s: u64,
    pub dashboard_url: String,
    pub last_error: Option<Error>,
}
impl State {
    pub fn legacy_json(&self) -> Value {
        json!({"online":self.online,"service":self.legacy_service_scope,"version":self.version,"mode":self.mode,"connections":self.connections,"upload":self.upload_bytes_per_s,"download":self.download_bytes_per_s,"dashboard":self.dashboard_url})
    }
}
impl Model for State {
    fn failure(&mut self, e: Error) {
        self.online = false;
        self.upload_bytes_per_s = 0;
        self.download_bytes_per_s = 0;
        self.last_error = Some(e);
    }
    fn action_failure(&mut self, error: Error) {
        self.last_error = Some(error);
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Action {
    Refresh,
    Mode(String),
    Service(UnitOperation),
    OpenDashboard,
}
#[derive(Clone, Debug)]
pub struct Config {
    pub file: PathBuf,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            file: PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                .join(".config/mihomo/config.yaml"),
        }
    }
}
pub struct Key;
impl TopicKey for Key {
    type Value = State;
}
struct Controller {
    base: String,
    secret: String,
}
fn controller(bytes: &[u8]) -> Result<Controller> {
    let v: serde_yaml::Value =
        serde_yaml::from_slice(bytes).map_err(|_| Error::new("config", "invalid Mihomo YAML"))?;
    let raw = v["external-controller"]
        .as_str()
        .unwrap_or("127.0.0.1:9090")
        .trim_end_matches('/');
    let raw = if raw.starts_with(':') {
        format!("127.0.0.1{raw}")
    } else if raw.starts_with("0.0.0.0:") || raw.starts_with("[::]:") {
        format!("127.0.0.1:{}", raw.rsplit(':').next().unwrap())
    } else {
        raw.into()
    };
    let base = if raw.starts_with("http://") || raw.starts_with("https://") {
        raw
    } else {
        format!("http://{raw}")
    };
    let url =
        reqwest::Url::parse(&base).map_err(|_| Error::new("config", "invalid controller URL"))?;
    if !url.username().is_empty() || url.password().is_some() || url.query().is_some() {
        return Err(Error::new(
            "config",
            "controller URL must not embed credentials/query",
        ));
    }
    Ok(Controller {
        base,
        secret: v["secret"].as_str().unwrap_or_default().into(),
    })
}
struct Native {
    buses: BusPool,
    state: State,
    config: Config,
    controller: Option<Controller>,
    http: reqwest::Client,
    watch: Option<FileWatch>,
    streams: SelectAll<BoxStream<'static, (bool, Result<Value>)>>,
    visible: bool,
    deadline: Option<tokio::time::Instant>,
    manager: Option<Manager>,
    unit_events: Option<Signals>,
    sleep_events: Option<zbus::MessageStream>,
    sleeping: bool,
}
impl Native {
    fn req(&self, path: &str) -> reqwest::RequestBuilder {
        let c = self.controller.as_ref().unwrap();
        let request = self.http.get(format!("{}{path}", c.base));
        if c.secret.is_empty() {
            request
        } else {
            request.bearer_auth(&c.secret)
        }
    }
    async fn get(&mut self, path: &str) -> Result<Value> {
        let response = self
            .req(path)
            .timeout(Duration::from_secs(3))
            .send()
            .await
            .map_err(|_| Error::new("http", "Mihomo request failed"))?
            .error_for_status()
            .map_err(|_| Error::new("http", "Mihomo returned error status"))?;
        json_response(response).await
    }
    async fn snapshot(&mut self) -> Result<()> {
        let version = self.get("/version").await?;
        let config = self.get("/configs").await?;
        self.state.version = version["version"].as_str().unwrap_or_default().into();
        self.state.mode = config["mode"].as_str().unwrap_or_default().into();
        self.state.online = true;
        self.state.last_error = None;
        if let Some(m) = &self.manager {
            self.state.unit = m.unit("mihomo.service").await.ok();
        }
        Ok(())
    }
    async fn streams(&mut self) -> Result<()> {
        self.streams = SelectAll::new();
        for (traffic, path) in [(true, "/traffic"), (false, "/connections")] {
            let stream = self.stream(path).await?;
            self.streams.push(
                stream
                    .chain(futures_util::stream::once(async {
                        Err(Error::new("stream", "Mihomo endpoint stream closed"))
                    }))
                    .map(move |v| (traffic, v))
                    .boxed(),
            );
        }
        Ok(())
    }
    async fn stream(&mut self, path: &str) -> Result<BoxStream<'static, Result<Value>>> {
        let c = self.controller.as_ref().unwrap();
        let ws = format!("{}{path}", c.base.replacen("http", "ws", 1));
        let mut request = ws
            .into_client_request()
            .map_err(|_| Error::new("websocket", "invalid websocket URL"))?;
        if !c.secret.is_empty() {
            request.headers_mut().insert(
                "Authorization",
                format!("Bearer {}", c.secret)
                    .parse()
                    .map_err(|_| Error::new("config", "invalid auth header"))?,
            );
        }
        if let Ok(Ok((socket, _))) = tokio::time::timeout(
            Duration::from_secs(3),
            connect_async_with_config(
                request,
                Some(
                    tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
                        .max_message_size(Some(1024 * 1024))
                        .max_frame_size(Some(1024 * 1024)),
                ),
                false,
            ),
        )
        .await
        {
            return Ok(
                futures_util::stream::unfold(socket, |mut socket| async move {
                    loop {
                        match socket.next().await {
                            Some(Ok(Message::Text(t))) => {
                                let value = if t.len() <= 1024 * 1024 {
                                    serde_json::from_str(&t).map_err(Error::from)
                                } else {
                                    Err(Error::new("frame", "Mihomo websocket frame too large"))
                                };
                                return Some((value, socket));
                            }
                            Some(Ok(Message::Close(_))) | None => return None,
                            Some(Ok(_)) => continue,
                            Some(Err(_)) => {
                                return Some((
                                    Err(Error::new("websocket", "Mihomo stream disconnected")),
                                    socket,
                                ))
                            }
                        }
                    }
                })
                .boxed(),
            );
        }
        let response = tokio::time::timeout(Duration::from_secs(3), self.req(path).send())
            .await
            .map_err(|_| Error::new("timeout", "Mihomo stream headers timed out"))?
            .map_err(|_| Error::new("http", "Mihomo stream unavailable"))?
            .error_for_status()
            .map_err(|_| Error::new("http", "Mihomo stream rejected"))?;
        let body = response.bytes_stream().boxed();
        Ok(futures_util::stream::unfold(
            (body, Vec::<u8>::new()),
            |(mut body, mut buffer)| async move {
                loop {
                    if let Some(pos) = buffer.iter().position(|b| *b == b'\n') {
                        if pos > 1024 * 1024 {
                            return Some((
                                Err(Error::new("frame", "Mihomo stream frame too large")),
                                (body, vec![]),
                            ));
                        }
                        let bytes: Vec<u8> = buffer.drain(..=pos).collect();
                        return Some((
                            serde_json::from_slice(&bytes).map_err(Error::from),
                            (body, buffer),
                        ));
                    }
                    if buffer.len() > 1024 * 1024 {
                        return Some((
                            Err(Error::new("frame", "Mihomo stream frame too large")),
                            (body, vec![]),
                        ));
                    }
                    match body.next().await {
                        Some(Ok(bytes)) => buffer.extend_from_slice(&bytes),
                        Some(Err(_)) => {
                            return Some((
                                Err(Error::new("http", "Mihomo stream failed")),
                                (body, vec![]),
                            ))
                        }
                        None => return None,
                    }
                }
            },
        )
        .boxed())
    }
}
async fn json_response(response: reqwest::Response) -> Result<Value> {
    if response.content_length().is_some_and(|n| n > 1024 * 1024) {
        return Err(Error::new("frame", "HTTP response too large"));
    }
    let mut body = response.bytes_stream();
    let mut bytes = vec![];
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|_| Error::new("http", "body read failed"))?;
        if bytes.len() + chunk.len() > 1024 * 1024 {
            return Err(Error::new("frame", "HTTP response too large"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(serde_json::from_slice(&bytes)?)
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
    async fn connect(&mut self) -> Result<()> {
        self.streams = SelectAll::new();
        self.watch = Some(FileWatch::new(vec![self.config.file.clone()])?);
        let bytes = std::fs::read(&self.config.file).unwrap_or_default();
        self.controller = Some(controller(if bytes.is_empty() { b"{}" } else { &bytes })?);
        self.state.dashboard_url = format!("{}/ui", self.controller.as_ref().unwrap().base);
        self.manager = None;
        self.unit_events = None;
        self.state.legacy_service_scope = "none".into();
        for scope in [UnitScope::User, UnitScope::System] {
            if let Ok(m) = self.buses.manager(scope).await {
                if let Ok(unit) = m.unit("mihomo.service").await {
                    self.state.unit = Some(unit);
                    self.state.legacy_service_scope = if scope == UnitScope::User {
                        "user"
                    } else {
                        "system"
                    }
                    .into();
                    let events = m.signals_for_unit("mihomo.service").await?;
                    
                    self.unit_events = Some(events);
                    self.manager = Some(m);
                    break;
                }
            }
        }
        let manager = self.buses.manager(UnitScope::System).await?;
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender("org.freedesktop.login1")?
            .interface("org.freedesktop.login1.Manager")?
            .member("PrepareForSleep")?
            .build();
        self.sleep_events =
            Some(zbus::MessageStream::for_match_rule(rule, &manager.connection, Some(16)).await?);
        if let Err(error) = self.snapshot().await {
            self.state.failure(error);
        }
        Ok(())
    }
    async fn demand(&mut self, active: bool) -> Result<()> {
        self.visible = active;
        self.deadline = (active && !self.sleeping)
            .then(|| tokio::time::Instant::now() + Duration::from_secs(2));
        if active && !self.sleeping && self.state.online {
            if let Err(error) = self.streams().await {
                self.state.failure(error);
                self.streams = SelectAll::new();
            }
        } else {
            self.streams = SelectAll::new();
        }
        Ok(())
    }
    async fn next(&mut self) -> Result<()> {
        tokio::select! {
            event=async {match self.sleep_events.as_mut(){Some(stream)=>stream.next().await,None=>std::future::pending().await}}=>{
                let message=event.ok_or_else(||Error::new("bus","logind stream closed"))??;
                let(sleeping,):(bool,)=message.body().deserialize()?;self.sleeping=sleeping;
                if !sleeping {if let Err(error)=self.snapshot().await{self.state.failure(error);}}
                self.demand(self.visible).await
            },
            event=async { match self.unit_events.as_mut() {Some(stream)=>stream.next().await,None=>std::future::pending().await} }=>{
                let message=event.ok_or_else(||Error::new("bus","Mihomo service bus closed"))??;
                let header=message.header();
                if header.member().is_some_and(|member|member.as_str()=="NameOwnerChanged") {return Err(Error::new("owner","Mihomo systemd owner changed"));}
                if header.path().is_some_and(|p|p.as_str().starts_with("/org/freedesktop/systemd1/")) {
                    if let Some(manager)=&self.manager {self.state.unit=manager.unit("mihomo.service").await.ok();}
                }
                Ok(())
            },
            result=self.watch.as_mut().unwrap().changed()=>{
                result?;
                let controller=controller(&std::fs::read(&self.config.file)?)?;
                let old=self.controller.as_ref().unwrap();
                if controller.base!=old.base || controller.secret!=old.secret {return Err(Error::new("config","controller configuration changed"));}
                Ok(())
            },
            frame=self.streams.next(),if !self.streams.is_empty()=>{
                let result:Result<()>=(||{
                    let (traffic,value)=frame.ok_or_else(||Error::new("stream","Mihomo streams ended"))?;
                    let value=value?;
                    if traffic {
                        self.state.upload_bytes_per_s=value["up"].as_u64().ok_or_else(||Error::new("schema","traffic up missing"))?;
                        self.state.download_bytes_per_s=value["down"].as_u64().ok_or_else(||Error::new("schema","traffic down missing"))?;
                    } else {
                        self.state.connections=value["connections"].as_array().ok_or_else(||Error::new("schema","connections missing"))?.len() as u64;
                    }
                    Ok(())
                })();
                if let Err(error)=result {self.state.failure(error);self.streams=SelectAll::new();}
                Ok(())
            },
            _=async {if let Some(deadline)=self.deadline {tokio::time::sleep_until(deadline).await} else {std::future::pending().await}}=>{
                match self.snapshot().await {
                    Ok(())=>if self.streams.is_empty() {if let Err(error)=self.streams().await {self.state.failure(error);self.streams=SelectAll::new();}},
                    Err(error)=>{self.state.failure(error);self.streams=SelectAll::new();}
                }
                self.deadline=Some(tokio::time::Instant::now()+Duration::from_secs(2));
                Ok(())
            }
        }
    }
    async fn action(&mut self, _id: u64, a: Action) -> Result<Value> {
        match a {
            Action::Refresh => self.snapshot().await?,
            Action::Mode(mode) => {
                if !["rule", "global", "direct"].contains(&mode.as_str()) {
                    return Err(Error::new("mode", "expected rule/global/direct"));
                }
                let c = self.controller.as_ref().unwrap();
                self.http
                    .patch(format!("{}/configs", c.base))
                    .bearer_auth(&c.secret)
                    .json(&json!({"mode":mode}))
                    .timeout(Duration::from_secs(3))
                    .send()
                    .await
                    .map_err(|_| Error::new("http", "mode request failed"))?
                    .error_for_status()
                    .map_err(|_| Error::new("http", "mode rejected"))?;
                self.snapshot().await?;
            }
            Action::Service(op) => {
                let manager = self
                    .manager
                    .as_ref()
                    .ok_or_else(|| Error::new("service", "Mihomo service unavailable"))?;
                return Ok(json!({"job":manager.operate("mihomo.service",op).await?}));
            }
            Action::OpenDashboard => return portal_open(&self.state.dashboard_url).await,
        }
        Ok(Value::Null)
    }
}
pub fn spawn(ctx: Context) -> Handle<State, Action> {
    spawn_with_config(ctx, Config::default())
}
pub fn spawn_with_config(ctx: Context, config: Config) -> Handle<State, Action> {
    let buses = BusPool::get(&ctx);
    launch::<Key, _>(
        ctx,
        Native {
            buses,
            state: State::default(),
            config,
            controller: None,
            http: reqwest::Client::new(),
            watch: None,
            streams: SelectAll::new(),
            visible: false,
            deadline: None,
            manager: None,
            unit_events: None,
            sleep_events: None,
            sleeping: false,
        },
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quoted_yaml_secret_is_private() {
        let c = controller(b"external-controller: ':9090'\nsecret: 'a # b'\n").unwrap();
        assert_eq!(c.base, "http://127.0.0.1:9090");
        assert_eq!(c.secret, "a # b");
        assert!(!serde_json::to_string(&State::default())
            .unwrap()
            .contains("secret"));
    }
    #[test]
    fn wildcard_and_credentials() {
        assert_eq!(
            controller(b"external-controller: '[::]:9090'")
                .unwrap()
                .base,
            "http://127.0.0.1:9090"
        );
        assert!(controller(b"external-controller: 'http://user:pass@host'").is_err());
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    fn native(base: String) -> Native {
        Native {
            buses: BusPool::default(),
            state: State::default(),
            config: Config::default(),
            controller: Some(Controller {
                base,
                secret: "test-secret".into(),
            }),
            http: reqwest::Client::new(),
            watch: None,
            streams: SelectAll::new(),
            visible: false,
            deadline: None,
            manager: None,
            unit_events: None,
            sleep_events: None,
            sleeping: false,
        }
    }
    #[tokio::test]
    async fn websocket_capability_falls_back_to_ndjson() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for attempt in 0..2 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = vec![0; 8192];
                let count = socket.read(&mut bytes).await.unwrap();
                let request = String::from_utf8_lossy(&bytes[..count]).to_lowercase();
                assert!(request.contains("authorization: bearer test-secret"));
                if attempt == 0 {
                    assert!(request.contains("upgrade: websocket"));
                    socket.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
                } else {
                    let body = "{\"up\":123,\"down\":456}\n";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    socket.write_all(response.as_bytes()).await.unwrap();
                }
            }
        });
        let mut native = native(format!("http://{address}"));
        let mut stream = native.stream("/traffic").await.unwrap();
        let frame = stream.next().await.unwrap().unwrap();
        assert_eq!(frame["up"], 123);
        assert_eq!(frame["down"], 456);
        server.await.unwrap();
    }
    #[tokio::test]
    async fn websocket_transport_and_last_subscriber_cleanup() {
        use futures_util::SinkExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
            ws.send(Message::Text("{\"connections\":[{},{}]}".into()))
                .await
                .unwrap();
        });
        let mut native = native(format!("http://{address}"));
        let mut stream = native.stream("/connections").await.unwrap();
        assert_eq!(
            stream.next().await.unwrap().unwrap()["connections"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        server.await.unwrap();
        native.streams.push(stream.map(|v| (false, v)).boxed());
        native.deadline = Some(tokio::time::Instant::now());
        native.demand(false).await.unwrap();
        assert!(native.streams.is_empty());
        assert!(native.deadline.is_none());
    }
}
