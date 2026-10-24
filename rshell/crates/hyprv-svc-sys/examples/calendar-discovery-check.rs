//! Read-only proof of the production calendar actor with no discovery override.
use hyprv_core::{runtime::ServiceRuntime, service::Service, store::StateStore};
use hyprv_svc_sys::{calendar, Context};
use std::{sync::Arc, time::Duration};
fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if std::env::args().skip(1).collect::<Vec<_>>() != ["--read-only"] {
        return Err(
            "requires --read-only; only calendars.list/events.list/subscribe are used".into(),
        );
    }
    let runtime = Arc::new(ServiceRuntime::new()?);
    let mut calendar = calendar::spawn_with_config(
        Context {
            runtime: runtime.clone(),
            store: Arc::new(StateStore::default()),
        },
        calendar::Config::default(),
    );
    let _demand = calendar.state.subscribe();
    let result=runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(12),async {
            loop {
                let state=calendar.state.snapshot().value;
                if state.connected && state.subscribed && !state.loading && state.request_serial>0 {
                    return serde_json::json!({"socket":state.socket,"connected":state.connected,"subscribed":state.subscribed,"calendars":state.calendars.len(),"events":state.events.len(),"loading":state.loading,"request_serial":state.request_serial,"exec_discovery":false,"read_only":true});
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }).await
    });
    runtime.block_on(calendar.stop())?;
    runtime.request_shutdown();
    println!("{}", serde_json::to_string_pretty(&result?)?);
    Ok(())
}
