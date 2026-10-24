//! Startup application-restore prompt (`StartupConfirm.qml`).
//!
//! Only an explicit affirmative decision writes exactly `launch\n` to
//! `$HYPRV_STARTUP_APPROVAL`; cancel, close, crash or a failed write never
//! approve. One decision per process; no polling.
use crate::{topic_key, ActionResult, Ctx, Handle, ServiceError};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartupState {
    pub approval_path: Option<PathBuf>,
    pub decided: bool,
    /// True only after `launch\n` was written successfully.
    pub approved: bool,
    pub write_error: Option<String>,
}
topic_key!(StartupTopic, StartupState);

#[derive(Clone, Copy, Debug)]
pub enum StartupAction {
    /// `finish(launch)`: the UI quits after this returns.
    Decide(bool),
}
pub type StartupHandle = Handle<StartupState, StartupAction>;

pub fn spawn(ctx: &Ctx) -> StartupHandle {
    let path = ctx.env.var("HYPRV_STARTUP_APPROVAL").map(PathBuf::from);
    let topic = crate::topic::<StartupTopic>(
        ctx,
        StartupState {
            approval_path: path.clone(),
            ..Default::default()
        },
    );
    let (handle, mut rx) = Handle::<StartupState, StartupAction>::new(topic.clone());
    let mut shutdown = ctx.shutdown.clone();
    ctx.rt.spawn(async move {
        loop {
            tokio::select! {
                msg = rx.recv() => {
                    let Some(env) = msg else { return };
                    let mut state = topic.snapshot().value;
                    let StartupAction::Decide(launch) = env.action;
                    let result = decide(&mut state, launch);
                    topic.publish(state);
                    env.respond(result);
                }
                _ = shutdown.changed() => if *shutdown.borrow() { return },
            }
        }
    });
    handle
}
pub fn decide(state: &mut StartupState, launch: bool) -> ActionResult {
    if state.decided {
        return Err(ServiceError::new("decided", "decision already made"));
    }
    state.decided = true;
    match (&state.approval_path, launch) {
        (Some(path), true) => match std::fs::write(path, "launch\n") {
            Ok(()) => {
                state.approved = true;
                Ok("launch".into())
            }
            Err(e) => {
                state.write_error = Some(e.to_string());
                Err(e.into())
            }
        },
        _ => Ok(String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_explicit_launch_approves() {
        let path = std::env::temp_dir().join(format!("hyprv-approval-{}", std::process::id()));
        std::fs::write(&path, "").unwrap();
        let mut s = StartupState {
            approval_path: Some(path.clone()),
            ..Default::default()
        };
        decide(&mut s, false).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
        assert!(decide(&mut s, true).is_err(), "second decision ignored");
        let mut s = StartupState {
            approval_path: Some(path.clone()),
            ..Default::default()
        };
        decide(&mut s, true).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "launch\n");
        assert!(s.approved);
        let mut s = StartupState::default();
        decide(&mut s, true).unwrap();
        assert!(!s.approved);
        std::fs::remove_file(path).unwrap();
    }
}
