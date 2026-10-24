//! Observation only. Deliberately never invokes a service action API.
use hyprv_core::{
    runtime::ServiceRuntime,
    store::{StateStore, Topic},
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    any::Any,
    io::{self, Write},
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{mpsc, watch};
type AnyError = Box<dyn std::error::Error + Send + Sync>;
fn epoch() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
struct Probe {
    runtime: Arc<ServiceRuntime>,
    store: Arc<StateStore>,
    output: mpsc::UnboundedSender<Value>,
    release: watch::Receiver<bool>,
    keep: Vec<Box<dyn Any>>,
    state_dir: PathBuf,
    freeze_agent_input: bool,
    exclude: std::collections::BTreeSet<String>,
}
impl Probe {
    fn topic<S>(&self, name: &'static str, topic: Topic<S>, sampled: bool)
    where
        S: Clone + PartialEq + Serialize + Send + Sync + 'static,
    {
        let mut subscription = topic.subscribe();
        let output = self.output.clone();
        let mut release = self.release.clone();
        let mut stop = self.runtime.shutdown_signal();
        let _=output.send(json!({"topic":name,"epoch":epoch(),"revision":subscription.current().revision,"state":subscription.current().value}));
        self.runtime.spawn(async move {
            loop {tokio::select! {
                value=subscription.changed()=>match value{Ok(value)=>{let _=output.send(json!({"topic":name,"epoch":epoch(),"revision":value.revision,"state":value.value}));},Err(_)=>break},
                _=release.changed(),if sampled=>{if *release.borrow(){let _=output.send(json!({"topic":name,"epoch":epoch(),"kind":"sampling_released"}));break;}},
                _=stop.changed()=>break,
            }}
        });
    }
    fn unavailable(&self, name: &str, reason: &str) {
        let _ = self
            .output
            .send(json!({"topic":name,"epoch":epoch(),"kind":"unavailable","reason":reason}));
    }
    #[cfg(feature = "net")]
    fn net(&mut self) {
        use hyprv_svc_net as svc;
        let mut ctx = svc::Context::new(self.store.clone(), self.runtime.clone());
        ctx.read_only = true;
        let n = svc::network::spawn(ctx.clone());
        self.topic("network", n.state.clone(), false);
        self.keep.push(Box::new(n));
        let b = svc::bluetooth::spawn(ctx);
        self.topic("bluetooth", b.state.clone(), false);
        self.keep.push(Box::new(b));
    }
    #[cfg(feature = "media")]
    fn media(&mut self) {
        use hyprv_svc_media as svc;
        let mut ctx = svc::Context::new(self.store.clone(), self.runtime.clone());
        ctx.read_only = true;
        let audio = svc::audio::spawn(&ctx);
        self.topic("audio", audio.topic.clone(), false);
        self.keep.push(Box::new(audio));
        let cider = svc::cider::spawn(&ctx);
        self.topic("cider", cider.topic.clone(), false);
        // No internal Lookup actions: the verification task forbids every dispatch.
        let mpris = svc::mpris::spawn(&ctx, None);
        self.topic("mpris", mpris.topic.clone(), false);
        self.keep.push(Box::new(mpris));
        self.keep.push(Box::new(cider));
        // Do not open passive audio capture in a read-only idle observer.
        let (gate, rx) = watch::channel(false);
        let spectrum = svc::spectrum::spawn(&ctx, rx);
        self.topic("spectrum", spectrum.topic.clone(), true);
        self.keep.push(Box::new((gate, spectrum)));
        #[cfg(has_media_notifications)]
        {
            let notifications = svc::notifications::spawn(&ctx);
            self.topic("notifications", notifications.topic.clone(), false);
            self.keep.push(Box::new(notifications));
        }
        let tray = svc::tray::spawn(&ctx);
        self.topic("tray", tray.topic.clone(), false);
        self.keep.push(Box::new(tray));
    }
    #[cfg(feature = "power")]
    async fn power(&mut self) -> Result<(), AnyError> {
        use hyprv_svc_power as svc;
        let mut ctx = svc::Context::system(self.runtime.clone()).await?;
        ctx.store = self.store.clone();
        // Persistent sampler writes are confined to probe-owned artifacts.
        ctx.paths.runtime = self.state_dir.join("power-runtime");
        let p = svc::power::spawn(ctx.clone());
        self.topic("power", p.state.clone(), false);
        self.topic("power.error", p.error.clone(), false);
        self.keep.push(Box::new(p));
        let b = svc::battery_history::spawn(ctx.clone());
        self.topic("battery_history", b.state.clone(), true);
        self.topic("battery_history.error", b.error.clone(), false);
        self.keep.push(Box::new(b));
        let stats = svc::sysstats::spawn(ctx.clone());
        self.topic("sysstats", stats.state.clone(), true);
        self.topic("sysstats.error", stats.error.clone(), false);
        self.keep.push(Box::new(stats));
        // refresh() issues only sysfs reads or DDC Get VCP (0x01), never Set VCP.
        // Restoring saved metadata updates local State only; no actions are sent.
        let backlight = svc::backlight::spawn(ctx.clone());
        self.topic("backlight", backlight.state.clone(), false);
        self.topic("backlight.error", backlight.error.clone(), false);
        self.keep.push(Box::new(backlight));
        let inhibitor = svc::inhibit::spawn(ctx.clone());
        self.topic("inhibit", inhibitor.state.clone(), false);
        self.keep.push(Box::new(inhibitor));
        let dbus = zbus::fdo::DBusProxy::new(&ctx.bus).await?;
        if dbus
            .name_has_owner(hyprv_privhelper::BUS.try_into()?)
            .await?
        {
            let fan = svc::fan::spawn(ctx);
            self.topic("fan", fan.state.clone(), false);
            self.topic("fan.error", fan.error.clone(), false);
            self.keep.push(Box::new(fan));
        } else {
            self.unavailable(
                "fan",
                "helper absent; actor skipped to prevent D-Bus autoactivation",
            );
        }
        Ok(())
    }
    #[cfg(feature = "sys")]
    fn sys(&mut self) {
        use hyprv_svc_sys as svc;
        let ctx = svc::Context {
            runtime: self.runtime.clone(),
            store: self.store.clone(),
        };
        macro_rules! start {
            ($module:ident,$sampled:expr) => {
                let h = svc::$module::spawn(ctx.clone());
                self.topic(stringify!($module), h.state.clone(), $sampled);
                self.keep.push(Box::new(h));
            };
        }
        start!(systemd, false);
        start!(vm, false);
        start!(mihomo, true);
        start!(hyprland, false);
        start!(calendar, true);
        let config = svc::weather::Config {
            cache_dir: self.state_dir.join("weather-cache"),
            ..Default::default()
        };
        let weather = svc::weather::spawn_with_config(ctx, config);
        self.topic("weather", weather.state.clone(), true);
        self.keep.push(Box::new(weather));
    }
    #[cfg(feature = "desktop")]
    fn desktop(&mut self) {
        use hyprv_svc_desktop as svc;
        let env = svc::Env::from_process();
        let rt = self.runtime.enter(tokio::runtime::Handle::current);
        let ctx = svc::Ctx::new(self.store.clone(), rt, self.runtime.shutdown_signal())
            .with_env(env.clone());
        if !self.exclude.contains("agent_island") {
            let target = self.state_dir.join("agent-island");
            let source = svc::agent_island::store::state_dir(&env);
            let _ = std::fs::create_dir_all(&target);
            if let Ok(data) = std::fs::read(source.join("state.json")) {
                let _ = std::fs::write(target.join("state.json"), data);
            }
            if !self.freeze_agent_input {
                let from = source.join("state.json");
                let to = target.join("state.json");
                let mut shutdown = self.runtime.shutdown_signal();
                self.runtime.spawn(async move {
                let Ok(watch)=hyprv_core::config::FileWatch::new(std::slice::from_ref(&from))else{return;};
                let mut previous=std::fs::read(&from).ok();
                loop{tokio::select!{
                    result=watch.changed()=>{if result.is_err(){break;}
                        if let Ok(data)=std::fs::read(&from){if previous.as_ref()!=Some(&data){let _=std::fs::write(&to,&data);previous=Some(data);}}},
                    _=shutdown.changed()=>break,
                }}
            });
            }
            let mut agent_env = env.clone();
            if self.freeze_agent_input {
                // Explicit idle fixture: keep the actor, freeze external hook
                // input and point app-server observation at an absent endpoint.
                agent_env.vars.insert(
                    "HYPRV_CODEX_APP_SERVER_SOCK".into(),
                    target
                        .join("absent-app-server.sock")
                        .to_string_lossy()
                        .into_owned(),
                );
            }
            agent_env.vars.insert(
                "HYPRV_AGENT_ISLAND_DIR".into(),
                target.to_string_lossy().into_owned(),
            );
            let agent_ctx = ctx.clone().with_env(agent_env);
            let agent = svc::agent_island::spawn(&agent_ctx);
            self.topic("agent_island", agent.topic.clone(), false);
            self.keep.push(Box::new(agent));
        } else {
            self.unavailable("agent_island", "excluded for isolation measurement");
        }
        let ir = svc::ir::spawn(&ctx);
        self.topic("ir", ir.topic.clone(), false);
        let deskflow = svc::deskflow::spawn(&ctx, ir.clone());
        self.topic("deskflow", deskflow.topic.clone(), false);
        self.keep.push(Box::new(deskflow));
        #[cfg(has_desktop_viewflow)]
        {
            if !self.exclude.contains("viewflow") {
                let h = svc::viewflow::spawn(&ctx, ir.clone());
                self.topic("viewflow", h.topic.clone(), false);
                self.keep.push(Box::new(h));
            }
        }
        #[cfg(has_desktop_egpu)]
        {
            if !self.exclude.contains("egpu") {
                let h = svc::egpu::spawn(&ctx);
                self.topic("egpu", h.topic.clone(), false);
                self.keep.push(Box::new(h));
            }
        }
        #[cfg(has_desktop_recording)]
        {
            if !self.exclude.contains("recording") {
                let h = svc::recording::spawn(&ctx);
                self.topic("recording", h.topic.clone(), false);
                self.keep.push(Box::new(h));
            }
        }
        #[cfg(has_desktop_demo)]
        {
            if !self.exclude.contains("demo") {
                let mut demo_env = env.clone();
                demo_env.state_home = self.state_dir.join("desktop-state");
                let source = env.state_home.join("hyprv/quickshell/demo-mode");
                let target = demo_env.state_home.join("hyprv/quickshell/demo-mode");
                let _ = std::fs::create_dir_all(&target);
                for name in [
                    "active",
                    "session",
                    "source",
                    "headless",
                    "created-headless",
                    "prevent-sleep-was-enabled",
                ] {
                    if let Ok(data) = std::fs::read(source.join(name)) {
                        let _ = std::fs::write(target.join(name), data);
                    }
                }
                let demo_ctx = ctx.clone().with_env(demo_env);
                let h = svc::demo::spawn(&demo_ctx, svc::demo::default_inhibitor());
                self.topic("demo", h.topic.clone(), false);
                self.keep.push(Box::new(h));
            }
        }
        #[cfg(has_desktop_applications)]
        {
            if !self.exclude.contains("applications") {
                let h = svc::applications::spawn(&ctx);
                self.topic("applications", h.topic.clone(), false);
                self.keep.push(Box::new(h));
            }
        }
        #[cfg(has_desktop_startup)]
        {
            let h = svc::startup::spawn(&ctx);
            self.topic("startup", h.topic.clone(), false);
            self.keep.push(Box::new(h));
        }
        #[cfg(has_desktop_theme)]
        {
            if !self.exclude.contains("theme") {
                let mut theme_env = env.clone();
                theme_env.state_home = self.state_dir.join("desktop-state");
                let source = svc::theme::state_file(&env);
                let target = svc::theme::state_file(&theme_env);
                if let Some(parent) = target.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Ok(data) = std::fs::read(source) {
                    let _ = std::fs::write(&target, data);
                }
                let h = svc::theme::spawn(&ctx.clone().with_env(theme_env));
                self.topic("theme", h.topic.clone(), false);
                self.keep.push(Box::new(h));
            }
        }
        self.keep.push(Box::new(ir));
    }
}
fn main() -> Result<(), AnyError> {
    let mut duration = 20u64;
    let mut idle_after = None;
    let mut freeze_agent_input = false;
    let mut state_dir = PathBuf::from("crates/hyprv-svc-probe/.check/runtime");
    let mut args = std::env::args().skip(1);
    let mut exclude = std::collections::BTreeSet::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--freeze-agent-input" => freeze_agent_input = true,
            "--exclude" => {
                exclude.insert(args.next().ok_or("missing exclude")?);
            }
            "--duration" => duration = args.next().ok_or("missing duration")?.parse()?,
            "--idle-after" => {
                idle_after = Some(args.next().ok_or("missing idle-after")?.parse::<u64>()?)
            }
            "--state-dir" => state_dir = args.next().ok_or("missing state-dir")?.into(),
            "--help" => {
                println!("hyprv-svc-probe [--duration SECONDS] [--idle-after SECONDS] [--state-dir PATH] [--exclude agent_island] [--freeze-agent-input]");
                return Ok(());
            }
            _ => return Err(format!("unknown argument {arg}").into()),
        }
    }
    std::fs::create_dir_all(&state_dir)?;
    let state_dir = std::fs::canonicalize(state_dir)?;
    let runtime = Arc::new(ServiceRuntime::new()?);
    let (output, mut receiver) = mpsc::unbounded_channel();
    let (release, rx) = watch::channel(false);
    let mut probe = Probe {
        runtime: runtime.clone(),
        store: Arc::new(StateStore::default()),
        output,
        release: rx,
        keep: vec![],
        state_dir,
        freeze_agent_input,
        exclude,
    };
    #[cfg(feature = "net")]
    if !probe.exclude.contains("net") {
        probe.net();
    }
    #[cfg(feature = "media")]
    if !probe.exclude.contains("media") {
        probe.media();
    }
    #[cfg(feature = "power")]
    if !probe.exclude.contains("power") {
        if let Err(error) = runtime.block_on(probe.power()) {
            probe.unavailable("power.crate", &error.to_string());
        }
    }
    #[cfg(feature = "sys")]
    if !probe.exclude.contains("sys") {
        probe.sys();
    }
    #[cfg(feature = "desktop")]
    if !probe.exclude.contains("desktop") {
        probe.desktop();
    }
    runtime.block_on(async{
        let stdout=io::stdout();let mut out=stdout.lock();writeln!(out,"{}",json!({"kind":"ready","epoch":epoch(),"pid":std::process::id(),"idle_after_s":idle_after,"actions_dispatched":0,"agent_inputs_frozen":freeze_agent_input,"compiled":{"net":cfg!(feature="net"),"media":cfg!(feature="media"),"power":cfg!(feature="power"),"sys":cfg!(feature="sys"),"desktop":cfg!(feature="desktop")}}))?;
        let deadline=tokio::time::sleep(Duration::from_secs(duration));tokio::pin!(deadline);let idle=async{if let Some(seconds)=idle_after{tokio::time::sleep(Duration::from_secs(seconds)).await}else{std::future::pending().await}};tokio::pin!(idle);let mut released=false;
        loop{tokio::select!{_= &mut deadline=>break,_=&mut idle,if !released=>{release.send_replace(true);released=true;writeln!(out,"{}",json!({"kind":"idle","epoch":epoch()}))?;out.flush()?;},Some(value)=receiver.recv()=>{writeln!(out,"{value}")?;out.flush()?;},_=tokio::signal::ctrl_c()=>break}}
        runtime.request_shutdown();tokio::time::sleep(Duration::from_millis(300)).await;Ok::<_,io::Error>(())
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idle_releases_sampling_and_keeps_event_subscriptions() {
        let runtime = Arc::new(ServiceRuntime::new().unwrap());
        let (output, mut receive) = mpsc::unbounded_channel();
        let (release, rx) = watch::channel(false);
        let probe = Probe {
            runtime: runtime.clone(),
            store: Arc::new(StateStore::default()),
            output,
            release: rx,
            keep: vec![],
            state_dir: PathBuf::new(),
            freeze_agent_input: false,
            exclude: Default::default(),
        };
        let sampled = Topic::new(0u8);
        let events = Topic::new(0u8);
        probe.topic("sampled", sampled.clone(), true);
        probe.topic("events", events.clone(), false);
        assert_eq!(sampled.subscriber_count(), 1);
        assert_eq!(events.subscriber_count(), 1);
        runtime.block_on(async {
            receive.recv().await.unwrap();
            receive.recv().await.unwrap();
            release.send_replace(true);
            let message = tokio::time::timeout(Duration::from_secs(1), receive.recv())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(message["kind"], "sampling_released");
            assert_eq!(sampled.subscriber_count(), 0);
            assert_eq!(events.subscriber_count(), 1);
            events.publish(1);
            let message = receive.recv().await.unwrap();
            assert_eq!(message["state"], 1);
            runtime.request_shutdown();
        });
    }
}
