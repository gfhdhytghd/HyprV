//! CPU-power-only activation: intentionally never constructs a cooling controller.
use hyprv_privhelper::{cpu_power, eligibility};
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    eligibility::trusted_environment()?;
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let root = hyprv_core::profile::sys_path("sys/class/powercap");
    if args == ["--eligible"] {
        std::process::exit(if cpu_power::eligible(root).is_ok() {
            0
        } else {
            1
        });
    }
    if !args.is_empty() {
        return Err("only --eligible is accepted".into());
    }
    if unsafe { libc::geteuid() } != 0 {
        return Err("CPU-only helper requires system activation".into());
    }
    // Builder::system honors DBUS_SYSTEM_BUS_ADDRESS for non-root fixture servers;
    // production root launches reject all caller-supplied path/bus overrides above.
    cpu_power::serve(
        zbus::Connection::system().await?,
        cpu_power::owner_uid()?,
        root,
        std::time::Duration::from_secs(5),
    )
    .await
}
