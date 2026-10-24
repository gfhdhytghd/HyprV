//! IPC used by the patched hypr scripts (cutover/hypr.patch): `desktop setVariant` and
//! `desktop deskflowReflow` reach the desktop services (here: the mocked service graph,
//! which records actions instead of performing them).
use hyprv_core::{ipc::Request, runtime::ServiceRuntime, store::StateStore};
use std::sync::Arc;

fn request(function: &str, args: &[&str]) -> Request {
    Request {
        target: "desktop".into(),
        function: function.into(),
        args: args.iter().map(|s| s.to_string()).collect(),
    }
}

#[test]
fn desktop_ipc_routes_variant_and_reflow_to_services() {
    let runtime = Arc::new(ServiceRuntime::new().unwrap());
    let hub = hyprv_shell::services::Hub::mocked(runtime.clone(), Arc::new(StateStore::default()));
    let ok = runtime.block_on(hub.desktop_ipc(request("setVariant", &["bt"])));
    assert!(ok.ok, "{}", ok.result);
    let ok = runtime.block_on(hub.desktop_ipc(request("deskflowReflow", &[])));
    assert!(ok.ok, "{}", ok.result);
    let bad = runtime.block_on(hub.desktop_ipc(request("setVariant", &["daily"])));
    assert!(!bad.ok);
    let log: Vec<String> = hub.recorded(true).iter().map(|v| v.to_string()).collect();
    assert_eq!(log.len(), 2, "{log:?}");
    assert!(
        log[0].contains("theme") && log[0].contains("SetVariant(Bt)"),
        "{log:?}"
    );
    assert!(
        log[1].contains("deskflow") && log[1].contains("Reflow"),
        "{log:?}"
    );
}
