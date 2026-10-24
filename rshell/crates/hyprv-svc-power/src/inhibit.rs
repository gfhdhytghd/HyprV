use crate::actor::{self, Backend, Context, Handle, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Owner {
    Manual,
    Demo,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub owners: Vec<Owner>,
    pub held: bool,
    pub enabled: bool,
    pub pid: Option<u32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Action {
    Enable(Owner),
    Disable(Owner),
    Toggle(Owner),
    Status,
}
#[async_trait]
pub trait Transport: Send + 'static {
    type Lease: Send;
    async fn acquire(&self) -> Result<Self::Lease>;
}
pub struct Inhibitor<T: Transport> {
    transport: T,
    owners: BTreeSet<Owner>,
    fd: Option<T::Lease>,
}
impl<T: Transport> Inhibitor<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            owners: BTreeSet::new(),
            fd: None,
        }
    }
    pub fn state(&self) -> State {
        State {
            owners: self.owners.iter().copied().collect(),
            held: self.fd.is_some(),
            enabled: self.fd.is_some(),
            pid: None,
        }
    }
    pub async fn recover(&mut self) -> Result<()> {
        if self.fd.is_none() && !self.owners.is_empty() {
            self.fd = Some(self.transport.acquire().await?);
        }
        Ok(())
    }
    pub fn invalidate(&mut self) {
        self.fd.take();
    }
    pub async fn apply(&mut self, a: Action) -> Result<()> {
        let (owner, enable) = match a {
            Action::Status => return Ok(()),
            Action::Enable(o) => (o, true),
            Action::Disable(o) => (o, false),
            Action::Toggle(o) => (o, !self.owners.contains(&o)),
        };
        if enable {
            if self.fd.is_none() {
                self.fd = Some(self.transport.acquire().await?);
            }
            self.owners.insert(owner);
        } else {
            self.owners.remove(&owner);
            if self.owners.is_empty() {
                self.fd.take();
            }
        }
        Ok(())
    }
}
struct BusTransport(zbus::Connection);
#[async_trait]
impl Transport for BusTransport {
    type Lease = zbus::zvariant::OwnedFd;
    async fn acquire(&self) -> Result<Self::Lease> {
        let p = actor::proxy(
            &self.0,
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
        )
        .await?;
        Ok(p.call(
            "Inhibit",
            &("idle:sleep", "HyprV", "HyprV prevent sleep", "block"),
        )
        .await?)
    }
}
pub fn spawn(ctx: Context) -> Handle<State, Action> {
    let inhibitor = Inhibitor::new(BusTransport(ctx.bus.clone()));
    actor::spawn(ctx, Native(inhibitor), false)
}
struct Native(Inhibitor<BusTransport>);
impl Backend for Native {
    type State = State;
    type Action = Action;
    fn providers(&self) -> &'static [&'static str] {
        &["org.freedesktop.login1"]
    }
    async fn connect(&mut self) -> Result<()> {
        Ok(())
    }
    async fn snapshot(&mut self) -> Result<State> {
        self.0.recover().await?;
        Ok(self.0.state())
    }
    async fn apply(&mut self, a: Action) -> Result<()> {
        self.0.apply(a).await
    }
    fn reset(&mut self) {
        self.0.invalidate();
    }
    async fn event(&mut self) -> Result<()> {
        std::future::pending().await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    struct Fake {
        held: Arc<AtomicUsize>,
        calls: Arc<AtomicUsize>,
        deny: bool,
    }
    struct Token(Arc<AtomicUsize>);
    impl Drop for Token {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::Relaxed);
        }
    }
    #[async_trait]
    impl Transport for Fake {
        type Lease = Token;
        async fn acquire(&self) -> Result<Token> {
            if self.deny {
                return Err("authorization denied".into());
            }
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.held.fetch_add(1, Ordering::Relaxed);
            Ok(Token(self.held.clone()))
        }
    }
    #[tokio::test]
    async fn manual_and_demo_own_independent_leases() {
        let held = Arc::new(AtomicUsize::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let mut i = Inhibitor::new(Fake {
            held: held.clone(),
            calls: calls.clone(),
            deny: false,
        });
        i.apply(Action::Enable(Owner::Manual)).await.unwrap();
        i.apply(Action::Enable(Owner::Demo)).await.unwrap();
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        i.apply(Action::Disable(Owner::Demo)).await.unwrap();
        assert!(i.state().held);
        assert_eq!(i.state().owners, vec![Owner::Manual]);
        i.invalidate();
        assert_eq!(held.load(Ordering::Relaxed), 0);
        i.recover().await.unwrap();
        assert_eq!(calls.load(Ordering::Relaxed), 2);
        i.apply(Action::Disable(Owner::Manual)).await.unwrap();
        assert_eq!(held.load(Ordering::Relaxed), 0);
    }
    #[tokio::test]
    async fn denied_acquisition_never_publishes_enabled() {
        let mut i = Inhibitor::new(Fake {
            held: Arc::new(AtomicUsize::new(0)),
            calls: Arc::new(AtomicUsize::new(0)),
            deny: true,
        });
        assert!(i.apply(Action::Enable(Owner::Manual)).await.is_err());
        assert!(!i.state().held);
        assert!(i.state().owners.is_empty());
    }
}
