//! System D-Bus availability (docs/requests/hyprv-shell.md, 2026-10-23).
//!
//! Like Quickshell, the shell keeps running without a system bus:
//! - System-bus services get a private placeholder connection. It is an in-process
//!   peer-to-peer link with no names on it, so every call fails and those services
//!   report themselves unavailable, while the UI and session-bus services work normally.
//! - A supervisor retries the real bus with capped exponential backoff. These are the
//!   only wakeups, and they happen only while degraded.
//! - The supervisor also notices when a working connection is lost (dbus-broker
//!   restart): the bus's own NameOwnerChanged stream ends.
//! - zbus connections cannot be re-targeted, and every system-bus actor holds its
//!   connection. Once the bus is reachable again, the shell therefore re-executes itself:
//!   same PID and argv, so the systemd unit stays active. It does this only when
//!   [`Recovery::ReExec`] was selected (the real app); tests observe [`SystemBusTopic`].
use hyprv_core::{
    runtime::ServiceRuntime,
    store::{StateStore, TopicKey},
    tokio,
};
use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SystemBusState {
    /// A real system bus connection is in use.
    pub available: bool,
    /// A previously working connection went away.
    pub lost: bool,
    /// Reconnect attempts since the bus became unavailable.
    pub attempts: u32,
    /// The bus is reachable again (the shell re-executes right after, see module docs).
    pub recovered: bool,
}
pub struct SystemBusTopic;
impl TopicKey for SystemBusTopic {
    type Value = SystemBusState;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recovery {
    /// Publish [`SystemBusState::recovered`] only (default: tests, embedders).
    Report,
    /// Re-execute the shell to rebuild every system-bus service on the new bus.
    ReExec,
}
static RECOVERY: OnceLock<Recovery> = OnceLock::new();
/// Selected once by the real shell before [`crate::services::Hub::spawn`].
pub fn set_recovery(recovery: Recovery) {
    let _ = RECOVERY.set(recovery);
}

/// 0.5 s, 1 s, 2 s … capped at 30 s.
pub fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(500u64.saturating_mul(1 << attempt.min(6))).min(Duration::from_secs(30))
}

/// The placeholder: a connected peer-to-peer pair whose server side serves nothing.
/// The server half is kept alive for the process lifetime so calls fail fast with
/// an error instead of hanging or seeing a disconnect loop.
async fn placeholder() -> zbus::Result<zbus::Connection> {
    static SERVER: OnceLock<zbus::Connection> = OnceLock::new();
    let (a, b) =
        tokio::net::UnixStream::pair().map_err(|e| zbus::Error::InputOutput(Arc::new(e)))?;
    let guid = zbus::Guid::generate();
    let (server, client) = tokio::join!(
        zbus::connection::Builder::unix_stream(a)
            .server(guid)?
            .p2p()
            .build(),
        zbus::connection::Builder::unix_stream(b).p2p().build()
    );
    let _ = SERVER.set(server?);
    client
}

/// The connection system-bus services should use, and whether it is the real bus.
pub fn connect(
    runtime: &ServiceRuntime,
    store: &StateStore,
) -> Result<(zbus::Connection, bool), String> {
    let topic = store.topic::<SystemBusTopic>(SystemBusState::default());
    match runtime.block_on(zbus::Connection::system()) {
        Ok(connection) => {
            topic.publish(SystemBusState {
                available: true,
                ..Default::default()
            });
            Ok((connection, true))
        }
        Err(e) => {
            eprintln!("system bus unavailable ({e}): system services report unavailable; retrying with backoff");
            let connection = runtime.block_on(placeholder()).map_err(|e| e.to_string())?;
            topic.publish(SystemBusState::default());
            Ok((connection, false))
        }
    }
}

/// Watch for loss (when `available`) and for reappearance (when degraded).
pub fn supervise(
    runtime: &ServiceRuntime,
    store: &StateStore,
    connection: zbus::Connection,
    available: bool,
) {
    let topic = store.topic::<SystemBusTopic>(SystemBusState::default());
    let mut shutdown = runtime.shutdown_signal();
    runtime.spawn(async move {
        if available {
            // Only this bus-owned name is matched, so a healthy bus causes no wakeups;
            // the stream ends when the connection closes.
            use zbus::export::futures_core::Stream;
            let stream = async {
                let proxy = zbus::fdo::DBusProxy::new(&connection).await?;
                proxy
                    .receive_name_owner_changed_with_args(&[(0, "org.freedesktop.DBus")])
                    .await
            };
            let Ok(stream) = stream.await else { return };
            let mut stream = Box::pin(stream);
            let ended = async {
                while std::future::poll_fn(|cx| stream.as_mut().poll_next(cx))
                    .await
                    .is_some()
                {}
            };
            tokio::select! {
                _ = ended => {}
                _ = shutdown.changed() => return,
            }
            eprintln!("system bus connection lost: system services unavailable until it returns");
            topic.publish(SystemBusState {
                lost: true,
                ..Default::default()
            });
        }
        let mut attempt = 0;
        loop {
            tokio::select! {
                _ = tokio::time::sleep(backoff(attempt)) => {}
                _ = shutdown.changed() => return,
            }
            attempt += 1;
            let mut state = topic.snapshot().value;
            state.attempts = attempt;
            if zbus::Connection::system().await.is_ok() {
                state.recovered = true;
                topic.publish(state);
                eprintln!("system bus available again after {attempt} attempt(s)");
                if RECOVERY.get() == Some(&Recovery::ReExec) {
                    reexec();
                }
                return;
            }
            topic.publish(state);
        }
    });
}

fn reexec() {
    use std::os::unix::process::CommandExt;
    let mut args = std::env::args_os();
    let arg0 = args.next().unwrap_or_else(|| "hyprv-shell".into());
    // /proc/self/exe still works when the binary on disk was replaced by an upgrade.
    let error = std::process::Command::new("/proc/self/exe")
        .arg0(arg0)
        .args(args)
        .exec();
    eprintln!("system bus recovery: re-exec failed: {error}; restart hyprv-shell to restore system services");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backoff_is_capped_exponential() {
        let s: Vec<_> = (0..9).map(|a| backoff(a).as_millis()).collect();
        assert_eq!(s, [500, 1000, 2000, 4000, 8000, 16000, 30000, 30000, 30000]);
    }
}
