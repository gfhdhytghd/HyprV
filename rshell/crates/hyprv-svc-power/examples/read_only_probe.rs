//! Intentionally no spawn/Action API: running the probe cannot change hardware.
use hyprv_svc_power::actor::{proxy, Result};
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let bus = zbus::Connection::system().await?;
    let dbus = zbus::fdo::DBusProxy::new(&bus).await?;
    for (name, path, interface, property) in [
        (
            "org.freedesktop.UPower",
            "/org/freedesktop/UPower",
            "org.freedesktop.UPower",
            "OnBattery",
        ),
        (
            "net.hadess.PowerProfiles",
            "/net/hadess/PowerProfiles",
            "net.hadess.PowerProfiles",
            "ActiveProfile",
        ),
        (
            "org.freedesktop.UPower.PowerProfiles",
            "/org/freedesktop/UPower/PowerProfiles",
            "org.freedesktop.UPower.PowerProfiles",
            "ActiveProfile",
        ),
    ] {
        if !dbus
            .name_has_owner(zbus::names::BusName::try_from(name)?)
            .await?
        {
            println!("{name}: absent (not activated)");
            continue;
        }
        let p = proxy(&bus, name, path, interface).await?;
        let value: zbus::zvariant::OwnedValue = p.get_property(property).await?;
        println!("{name}.{property}: {value:?}");
    }
    if dbus
        .name_has_owner(zbus::names::BusName::try_from(hyprv_privhelper::BUS)?)
        .await?
    {
        let p = proxy(
            &bus,
            hyprv_privhelper::BUS,
            hyprv_privhelper::PATH,
            hyprv_privhelper::INTERFACE,
        )
        .await?;
        let version: u32 = p.get_property("ProtocolVersion").await?;
        println!("helper version {version}");
        let state: String = p.call("GetFans", &()).await?;
        println!("{state}");
    } else {
        println!("helper: absent (not activated)");
    }
    let runtime = std::sync::Arc::new(hyprv_core::runtime::ServiceRuntime::new()?);
    let context = hyprv_svc_power::Context::system(runtime.clone()).await?;
    let power = hyprv_svc_power::power::spawn(context);
    let mut subscription = power.subscribe();
    let snapshot =
        tokio::time::timeout(std::time::Duration::from_secs(5), subscription.changed()).await??;
    println!(
        "power actor snapshot: {}",
        serde_json::to_string(&snapshot.value)?
    );
    runtime.request_shutdown();
    Ok(())
}
