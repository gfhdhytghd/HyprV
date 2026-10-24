//! Explicitly enabled live inspection. No agent registration or action dispatch.
use hyprv_core::{runtime::ServiceRuntime, store::StateStore};
use hyprv_svc_net::{
    bluetooth::{self, Transport as _},
    network::{self, Transport as _},
    prompts::Broker,
    Context,
};
use std::sync::Arc;
#[tokio::test]
#[ignore = "requires a readable system bus with NetworkManager and BlueZ"]
async fn inspect_system_bus_only() {
    let mut ctx = Context::new(
        Arc::new(StateStore::default()),
        Arc::new(ServiceRuntime::new().unwrap()),
    );
    ctx.read_only = true;
    let nm = network::Native::new(&ctx, Broker::new()).await.unwrap();
    let state = nm.snapshot().await.unwrap();
    assert!(state.available);
    eprintln!(
        "NetworkManager: {} APs, present={}",
        state.networks.len(),
        state.present
    );
    let bluez = bluetooth::Native::new(&ctx, Broker::new()).await.unwrap();
    let state = bluez.snapshot().await.unwrap();
    assert!(state.available);
    eprintln!(
        "BlueZ: {} devices, present={}",
        state.devices.len(),
        state.present
    );
    ctx.runtime.request_shutdown();
}
