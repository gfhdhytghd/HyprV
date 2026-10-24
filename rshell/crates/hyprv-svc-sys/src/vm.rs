//! windows-autoshop.service. "Hibernate" is StopUnit, never libvirt suspend.
use crate::{
    support::*,
    systemd::{BusPool, Manager, Signals, UnitOperation, UnitScope, UnitState},
};
use futures_util::StreamExt;
use hyprv_core::store::{Topic, TopicKey};
use serde::{Deserialize, Serialize};
pub const UNIT: &str = "windows-autoshop.service";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    AwaitingUiRelease,
    AwaitingAuthorization,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub unit: UnitState,
    pub running: bool,
    pub busy: bool,
    pub action: Option<u64>,
    pub phase: Option<Phase>,
    pub connected: bool,
    pub last_error: Option<Error>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            unit: UnitState {
                name: UNIT.into(),
                scope: UnitScope::System,
                active_state: "unknown".into(),
                sub_state: "unknown".into(),
                job: None,
            },
            running: false,
            busy: false,
            action: None,
            phase: None,
            connected: false,
            last_error: None,
        }
    }
}
impl Model for State {
    fn failure(&mut self, e: Error) {
        self.connected = false;
        self.last_error = Some(e);
        self.busy = false;
        self.phase = Some(Phase::Failed);
    }
    fn action_failure(&mut self, e: Error) {
        self.last_error = Some(e);
        if matches!(
            self.phase,
            Some(Phase::AwaitingAuthorization | Phase::Running)
        ) {
            self.busy = false;
            self.phase = Some(Phase::Failed);
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Action {
    Toggle,
    Start,
    Hibernate,
    InputReleased { request: u64 },
    Cancel { request: u64 },
    Refresh,
}
pub struct Key;
impl TopicKey for Key {
    type Value = State;
}
struct Native {
    buses: BusPool,
    state: State,
    updates: Topic<State>,
    manager: Option<Manager>,
    events: Option<Signals>,
    pending: Option<UnitOperation>,
}
impl Native {
    async fn refresh(&mut self) -> Result<()> {
        self.state.unit = self.manager.as_ref().unwrap().unit(UNIT).await?;
        self.state.running = self.state.unit.active_state == "active";
        self.state.connected = true;
        Ok(())
    }
    fn publish(&self) {
        self.updates.publish(self.state.clone());
    }
    fn prepare(&mut self, id: u64, op: UnitOperation) -> Result<serde_json::Value> {
        if self.state.busy {
            return Err(Error::new("busy", "VM action already in progress"));
        }
        self.pending = Some(op);
        self.state.action = Some(id);
        self.state.busy = true;
        self.state.phase = Some(Phase::AwaitingUiRelease);
        self.state.last_error = None;
        Ok(serde_json::json!({"request":id,"release_input":true}))
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
    async fn connect(&mut self) -> Result<()> {
        self.state.connected = false;
        let m = self.buses.manager(UnitScope::System).await?;
        self.events = Some(m.signals_for_unit(UNIT).await?);
        self.manager = Some(m);
        self.refresh().await
    }
    async fn next(&mut self) -> Result<()> {
        loop {
            let msg = self
                .events
                .as_mut()
                .unwrap()
                .next()
                .await
                .ok_or_else(|| Error::new("bus", "VM bus disconnected"))??;
            let h = msg.header();
            if h.path()
                .is_some_and(|p| p.as_str().starts_with("/org/freedesktop/systemd1/"))
            {
                return self.refresh().await;
            }
            if h.member().is_some_and(|m| m.as_str() == "NameOwnerChanged") {
                let (name, _, _): (String, String, String) = msg.body().deserialize()?;
                if name == "org.freedesktop.systemd1" {
                    self.state.connected = false;
                    return Err(Error::new("owner", "systemd owner changed"));
                }
            }
        }
    }
    async fn action(&mut self, id: u64, a: Action) -> Result<serde_json::Value> {
        match a {
            Action::Toggle => self.prepare(
                id,
                if self.state.running {
                    UnitOperation::Stop
                } else {
                    UnitOperation::Start
                },
            ),
            Action::Start => self.prepare(id, UnitOperation::Start),
            Action::Hibernate => self.prepare(id, UnitOperation::Stop),
            Action::Cancel { request } => {
                if self.state.action != Some(request)
                    || self.state.phase != Some(Phase::AwaitingUiRelease)
                {
                    return Err(Error::new("request", "action cannot be cancelled"));
                }
                self.pending = None;
                self.state.busy = false;
                self.state.phase = Some(Phase::Cancelled);
                Ok(serde_json::Value::Null)
            }
            Action::InputReleased { request } => {
                if self.state.action != Some(request)
                    || self.state.phase != Some(Phase::AwaitingUiRelease)
                {
                    return Err(Error::new("request", "no matching release handoff"));
                }
                let op = self
                    .pending
                    .take()
                    .ok_or_else(|| Error::new("request", "no pending operation"))?;
                self.state.phase = Some(Phase::AwaitingAuthorization);
                self.state.last_error = None;
                self.publish();
                let job = self.manager.as_ref().unwrap().operate(UNIT, op).await?;
                self.state.phase = Some(Phase::Running);
                self.publish();
                let result = tokio::time::timeout(std::time::Duration::from_secs(120), async {
                    loop {
                        let msg = self
                            .events
                            .as_mut()
                            .unwrap()
                            .next()
                            .await
                            .ok_or_else(|| Error::new("bus", "job bus disconnected"))??;
                        if msg
                            .header()
                            .member()
                            .is_some_and(|m| m.as_str() == "JobRemoved")
                        {
                            let (_, path, unit, result): (
                                u32,
                                zbus::zvariant::OwnedObjectPath,
                                String,
                                String,
                            ) = msg.body().deserialize()?;
                            if path.as_str() == job && unit == UNIT {
                                if result != "done" {
                                    return Err(Error::new("job", result));
                                }
                                break;
                            }
                        }
                    }
                    self.refresh().await
                })
                .await
                .map_err(|_| Error::new("timeout", "VM job not completed in 120s"))?;
                self.state.busy = false;
                result?;
                self.state.phase = Some(Phase::Succeeded);
                Ok(serde_json::json!({"job":job}))
            }
            Action::Refresh => {
                self.refresh().await?;
                Ok(serde_json::Value::Null)
            }
        }
    }
}
pub fn spawn(ctx: Context) -> Handle<State, Action> {
    let buses = BusPool::get(&ctx);
    let state = State::default();
    let updates = ctx.store.topic::<Key>(state.clone());
    launch::<Key, _>(
        ctx,
        Native {
            buses,
            state,
            updates,
            manager: None,
            events: None,
            pending: None,
        },
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn release_is_required_and_busy_is_retained() {
        let state = State::default();
        let mut n = Native {
            buses: BusPool::default(),
            updates: Topic::new(state.clone()),
            state,
            manager: None,
            events: None,
            pending: None,
        };
        assert_eq!(
            n.prepare(3, UnitOperation::Start).unwrap()["release_input"],
            true
        );
        assert_eq!(n.state.phase, Some(Phase::AwaitingUiRelease));
        assert!(n.prepare(4, UnitOperation::Stop).is_err());
        assert!(n.manager.is_none());
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    use futures_util::{stream::SelectAll, StreamExt};
    #[tokio::test]
    async fn auth_handoff_precedes_job_and_unit_confirmation() {
        let (manager, _server, mut calls) = crate::systemd::mock::manager().await;
        let mut signals = SelectAll::new();
        signals.push(zbus::MessageStream::from(manager.connection.clone()).boxed());
        let state = State::default();
        let mut native = Native {
            buses: BusPool::default(),
            updates: Topic::new(state.clone()),
            state,
            manager: Some(manager),
            events: Some(signals),
            pending: None,
        };
        native.refresh().await.unwrap();
        native.action(10, Action::Start).await.unwrap();
        assert!(calls.try_recv().is_err());
        assert!(native.action(11, Action::Start).await.is_err());
        assert!(native.state.busy);
        assert_eq!(native.state.action, Some(10));
        assert!(native
            .action(12, Action::InputReleased { request: 99 })
            .await
            .is_err());
        assert!(calls.try_recv().is_err());
        native
            .action(13, Action::InputReleased { request: 10 })
            .await
            .unwrap();
        assert_eq!(calls.recv().await.unwrap(), "start");
        assert!(native.state.running);
        assert!(!native.state.busy);
        assert_eq!(native.state.phase, Some(Phase::Succeeded));
    }
}

#[cfg(test)]
mod adversarial_review {
    use super::*;
    #[test]
    fn review_sys_vm_failure_invalidates_connected() {
        let mut state = State {
            connected: true,
            ..State::default()
        };
        state.failure(Error::new("bus", "connection lost"));
        assert!(
            !state.connected,
            "unavailable VM keeps connected=true during reconnect failures"
        );
    }
}
