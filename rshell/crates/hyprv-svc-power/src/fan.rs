use crate::actor::{self, Backend, Context, Handle, Result};
pub use hyprv_privhelper::{FanChannel, FanReading, FanState as State};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Action {
    Refresh,
    SetLimit { channel: FanChannel, percent: u8 },
}
pub fn spawn(ctx: Context) -> Handle<State, Action> {
    spawn_for_profile(ctx, hyprv_core::profile::profile())
}
/// Laptop has no cooling provider; this actor never connects or activates it.
pub fn spawn_for_profile(ctx: Context, profile: crate::Profile) -> Handle<State, Action> {
    actor::spawn(
        ctx.clone(),
        Native {
            ctx,
            stream: None,
            enabled: profile == crate::Profile::Desktop,
        },
        false,
    )
}
struct Native {
    enabled: bool,
    ctx: Context,
    stream: Option<actor::SignalInbox>,
}
impl Backend for Native {
    fn observe_on_demand(&self) -> bool {
        true
    }
    fn reset(&mut self) {
        self.stream = None;
    }
    fn providers(&self) -> &'static [&'static str] {
        if self.enabled {
            &["org.hyprv.Shell.PrivHelper"]
        } else {
            &[]
        }
    }
    type State = State;
    type Action = Action;
    async fn connect(&mut self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        self.stream = Some(
            actor::signals_member(
                &self.ctx.bus,
                hyprv_privhelper::BUS,
                hyprv_privhelper::PATH,
                hyprv_privhelper::INTERFACE,
                "FanStateChanged",
            )
            .await?,
        );
        Ok(())
    }
    async fn snapshot(&mut self) -> Result<State> {
        if !self.enabled {
            return Ok(State::default());
        }
        if !actor::has_owner(&self.ctx.bus, hyprv_privhelper::BUS).await? {
            return Err("fan helper absent; observation does not activate it".into());
        }
        let p = actor::proxy(
            &self.ctx.bus,
            hyprv_privhelper::BUS,
            hyprv_privhelper::PATH,
            hyprv_privhelper::INTERFACE,
        )
        .await?;
        let version: u32 = p.get_property("ProtocolVersion").await?;
        if version != hyprv_privhelper::VERSION {
            return Err("helper protocol version mismatch".into());
        }
        let json: String = p.call("GetFans", &()).await?;
        if json.len() > 65536 {
            return Err("fan state exceeds limit".into());
        }
        let state = serde_json::from_str(&json)?;
        Ok(state)
    }
    async fn apply(&mut self, a: Action) -> Result<()> {
        if !self.enabled {
            return Err("cooling unsupported by laptop profile".into());
        }
        match a {
            Action::Refresh => Ok(()),
            Action::SetLimit { channel, percent } => {
                if percent > 100 {
                    return Err("fan percent must be 0..100".into());
                }
                let _ = self.snapshot().await?;
                let name = match channel {
                    FanChannel::Chassis => "chassis",
                    FanChannel::Pump => "pump",
                };
                actor::proxy(
                    &self.ctx.bus,
                    hyprv_privhelper::BUS,
                    hyprv_privhelper::PATH,
                    hyprv_privhelper::INTERFACE,
                )
                .await?
                .call::<_, _, ()>("SetFanLimit", &(name, u32::from(percent)))
                .await?;
                Ok(())
            }
        }
    }
    async fn event(&mut self) -> Result<()> {
        actor::next(&mut self.stream).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{context, settle, Bus, Tree};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    struct Helper(Arc<AtomicUsize>);
    #[zbus::interface(name = "org.hyprv.Shell.PrivHelper1")]
    impl Helper {
        #[zbus(property)]
        fn protocol_version(&self) -> u32 {
            hyprv_privhelper::VERSION
        }
        fn get_fans(&self) -> String {
            self.0.fetch_add(1, Ordering::SeqCst);
            serde_json::to_string(&State {
                available: true,
                ..Default::default()
            })
            .unwrap()
        }
    }
    async fn emit(server: &zbus::Connection, member: &str) {
        server
            .emit_signal(
                None::<&str>,
                hyprv_privhelper::PATH,
                hyprv_privhelper::INTERFACE,
                member,
                &(),
            )
            .await
            .unwrap();
    }
    #[test]
    fn hidden_fan_has_no_rpc_and_rehydrates_after_owner_replacement() {
        let rt = Arc::new(hyprv_core::runtime::ServiceRuntime::new().unwrap());
        rt.block_on(async {
            let bus = Bus::new();
            let server = bus.connection().await;
            let client = bus.connection().await;
            server.request_name(hyprv_privhelper::BUS).await.unwrap();
            let calls = Arc::new(AtomicUsize::new(0));
            server
                .object_server()
                .at(hyprv_privhelper::PATH, Helper(calls.clone()))
                .await
                .unwrap();
            let tree = Tree::new();
            let ctx = context(client, tree.paths(), Some(&rt));
            let handle = spawn(ctx);
            for _ in 0..60 {
                emit(&server, "FanStateChanged").await;
                emit(&server, "CpuPowerChanged").await;
            }
            settle().await;
            assert_eq!(calls.load(Ordering::SeqCst), 0);
            let mut visible = handle.subscribe();
            tokio::time::timeout(std::time::Duration::from_secs(2), visible.changed())
                .await
                .unwrap()
                .unwrap();
            assert!(visible.current().value.available);
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            for _ in 0..60 {
                emit(&server, "CpuPowerChanged").await;
            }
            settle().await;
            assert_eq!(
                calls.load(Ordering::SeqCst),
                1,
                "CPU helper signals must not refresh fans"
            );
            emit(&server, "FanStateChanged").await;
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                while calls.load(Ordering::SeqCst) < 2 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            drop(visible);
            settle().await;
            for _ in 0..60 {
                emit(&server, "FanStateChanged").await;
            }
            settle().await;
            assert_eq!(calls.load(Ordering::SeqCst), 2);
            server.release_name(hyprv_privhelper::BUS).await.unwrap();
            let replacement = bus.connection().await;
            replacement
                .request_name(hyprv_privhelper::BUS)
                .await
                .unwrap();
            replacement
                .object_server()
                .at(hyprv_privhelper::PATH, Helper(calls.clone()))
                .await
                .unwrap();
            settle().await;
            assert_eq!(calls.load(Ordering::SeqCst), 2);
            let visible = handle.subscribe();
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                while calls.load(Ordering::SeqCst) < 3 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            drop(visible);
            // Explicit hidden refresh remains available and read-only.
            handle.action(Action::Refresh).await.unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                while calls.load(Ordering::SeqCst) < 4 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert_eq!(calls.load(Ordering::SeqCst), 4);
            rt.request_shutdown();
            settle().await;
        });
    }
    #[test]
    #[ignore = "isolated 10-second private-bus fan idle receipt"]
    fn private_bus_hidden_fan_idle_receipt() {
        let rt = Arc::new(hyprv_core::runtime::ServiceRuntime::new().unwrap());
        let bus = Bus::new();
        let tree = Tree::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let (_server, handle) = rt.block_on(async {
            let server = bus.connection().await;
            server.request_name(hyprv_privhelper::BUS).await.unwrap();
            server
                .object_server()
                .at(hyprv_privhelper::PATH, Helper(calls.clone()))
                .await
                .unwrap();
            let handle = spawn(context(bus.connection().await, tree.paths(), Some(&rt)));
            for _ in 0..60 {
                emit(&server, "FanStateChanged").await;
                emit(&server, "CpuPowerChanged").await;
            }
            handle.action(Action::Refresh).await.unwrap(); // actor startup barrier
            (server, handle)
        });
        std::thread::sleep(std::time::Duration::from_millis(100));
        let tid = std::fs::read_dir("/proc/self/task")
            .unwrap()
            .flatten()
            .find_map(|e| {
                std::fs::read_to_string(e.path().join("comm"))
                    .ok()
                    .filter(|s| s.trim() == "hyprv-services")
                    .map(|_| e.path())
            })
            .unwrap();
        let read = || {
            let sched = std::fs::read_to_string(tid.join("schedstat")).unwrap();
            let ns = sched
                .split_whitespace()
                .next()
                .unwrap()
                .parse::<u64>()
                .unwrap();
            let status = std::fs::read_to_string(tid.join("status")).unwrap();
            let vcs = status
                .lines()
                .find_map(|s| s.strip_prefix("voluntary_ctxt_switches:"))
                .unwrap()
                .trim()
                .parse::<u64>()
                .unwrap();
            (ns, vcs)
        };
        let before = read();
        let rpc_before = calls.load(Ordering::SeqCst);
        let start = std::time::Instant::now();
        std::thread::sleep(std::time::Duration::from_secs(10));
        let after = read();
        let seconds = start.elapsed().as_secs_f64();
        let rpc = calls.load(Ordering::SeqCst) - rpc_before;
        println!("hidden_fan seconds={seconds:.6} service_cpu_ns={} cpu_percent={:.8} voluntary_switches={} GetFans={rpc}",after.0-before.0,(after.0-before.0) as f64/1e9/seconds*100.,after.1-before.1);
        assert_eq!(rpc, 0);
        drop(handle);
        rt.request_shutdown();
    }
    #[test]
    fn laptop_fan_is_inert_even_with_subscribers_or_explicit_actions() {
        let rt = Arc::new(hyprv_core::runtime::ServiceRuntime::new().unwrap());
        rt.block_on(async {
            let bus = Bus::new();
            let tree = Tree::new();
            let handle = spawn_for_profile(
                context(bus.connection().await, tree.paths(), Some(&rt)),
                crate::Profile::Laptop,
            );
            let _visible = handle.subscribe();
            settle().await;
            assert!(!handle.state.snapshot().value.available);
            assert!(handle
                .action(Action::SetLimit {
                    channel: FanChannel::Chassis,
                    percent: 40
                })
                .await
                .is_err());
            assert!(!zbus::fdo::DBusProxy::new(&bus.connection().await)
                .await
                .unwrap()
                .name_has_owner(hyprv_privhelper::BUS.try_into().unwrap())
                .await
                .unwrap());
            rt.request_shutdown();
            settle().await;
        });
    }
}
