//! `hyprv-shell desktop setVariant <v1|v2|v3|bt>` and `hyprv-shell desktop deskflowReflow`:
//! the entry points of the patched hypr scripts (cutover/hypr.patch).
//!
//! They prefer the running shell (IPC target `desktop`), so its service performs the action
//! and its state stays authoritative. When no shell is reachable (not running, crashed or
//! mid-restart), the same svc-desktop code runs once in-process, as the legacy
//! `set-variant.sh` / `deskflow-target.sh` did. So a monitor rotation never depends on the
//! shell process. A service error from a running shell is returned as is; it is never
//! retried locally.
use hyprv_core::{ipc, tokio};
use hyprv_svc_desktop as desktop;
use std::{error::Error, io::ErrorKind};

pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let request = ipc::Request {
        target: "desktop".into(),
        function: args.first().cloned().unwrap_or_default(),
        args: args.iter().skip(1).cloned().collect(),
    };
    request.validate()?;
    if crate::laptop::active() && request.function == "deskflowReflow" { return Ok(()); }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let reply = match ipc::socket_path() {
        Ok(socket) => runtime.block_on(ipc::call(&socket, &request)),
        Err(e) => Err(e),
    };
    match reply {
        Ok(response) if response.ok => {
            if !response.result.is_empty() {
                println!("{}", response.result);
            }
            Ok(())
        }
        Ok(response) => Err(response.result.into()),
        Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::ConnectionRefused) => {
            eprintln!(
                "hyprv-shell is not running; applying `desktop {}` directly",
                request.function
            );
            let env = desktop::Env::from_process();
            let result = runtime.block_on(async {
                match request.function.as_str() {
                    "setVariant" => {
                        let variant = desktop::theme::Variant::parse(&request.args[0])
                            .ok_or("variant must be v1, v2, v3 or bt")?;
                        Ok(desktop::theme::run_once(
                            &env,
                            desktop::theme::ThemeAction::SetVariant(variant),
                        )
                        .await?)
                    }
                    "deskflowReflow" => Ok(desktop::deskflow::reflow_once(&env).await?),
                    _ => Err::<String, Box<dyn Error>>("unknown desktop function".into()),
                }
            })?;
            if !result.trim().is_empty() {
                println!("{}", result.trim_end());
            }
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}
