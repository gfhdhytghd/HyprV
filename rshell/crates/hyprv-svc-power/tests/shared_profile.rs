//! Fresh processes exercise OnceLock and zbus system construction without
//! mutating the environment of parallel unit tests or using the host bus/sysfs.
use hyprv_core::{
    profile::{self, Profile},
    runtime::ServiceRuntime,
};
use hyprv_svc_power::{backlight, battery_history, fan, power, sysstats, Context, Paths};
use std::{
    io::BufRead,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
struct Bus(std::process::Child, String);
impl Bus {
    fn new() -> Self {
        let mut p = std::process::Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--nopidfile", "--print-address=1"])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut address = String::new();
        std::io::BufReader::new(p.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self(p, address.trim().into())
    }
}
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn put(root: &Path, name: &str, value: &str) {
    let p = root.join(name);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, value).unwrap();
}
struct Upower;
#[zbus::interface(name = "org.freedesktop.UPower")]
impl Upower {
    fn get_display_device(&self) -> zbus::zvariant::OwnedObjectPath {
        "/org/freedesktop/UPower/devices/DisplayDevice"
            .try_into()
            .unwrap()
    }
    #[zbus(property)]
    fn on_battery(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn lid_is_present(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn lid_is_closed(&self) -> bool {
        false
    }
}
struct Battery;
#[zbus::interface(name = "org.freedesktop.UPower.Device")]
impl Battery {
    #[zbus(property, name = "Type")]
    fn kind(&self) -> u32 {
        2
    }
    #[zbus(property)]
    fn power_supply(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn is_present(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        2
    }
    #[zbus(property)]
    fn percentage(&self) -> f64 {
        57.
    }
}
struct Helper(Arc<AtomicUsize>);
#[zbus::interface(name = "org.hyprv.Shell.PrivHelper1")]
impl Helper {
    #[zbus(property)]
    fn protocol_version(&self) -> u32 {
        hyprv_privhelper::VERSION
    }
    fn get_fans(&self) -> String {
        self.0.fetch_add(1, Ordering::SeqCst);
        serde_json::to_string(&fan::State {
            available: true,
            chip: "private fixture".into(),
            ..Default::default()
        })
        .unwrap()
    }
}
#[test]
fn default_constructors_follow_shared_latched_profile_and_private_roots() {
    let bus = Bus::new();
    let base = std::env::temp_dir().join(format!("hyprv-power-shared-{}", std::process::id()));
    for mode in ["desktop", "laptop", "auto", "bad-bus"] {
        let root = base.join(mode);
        std::fs::create_dir_all(root.join("home")).unwrap();
        let mut c = std::process::Command::new(std::env::current_exe().unwrap());
        c.args(["--exact", "shared_profile_child", "--nocapture"])
            .env("HYPRV_POWER_CONTRACT_CHILD", mode)
            .env("HYPRV_SYSFS_ROOT", &root)
            .env("HOME", root.join("home"))
            .env("XDG_RUNTIME_DIR", root.join("run"))
            .env("XDG_STATE_HOME", root.join("state"))
            .env(
                "DBUS_SYSTEM_BUS_ADDRESS",
                if mode == "bad-bus" {
                    "unix:path=/nonexistent/hyprv-private-bus"
                } else {
                    &bus.1
                },
            );
        if mode == "auto" {
            c.env_remove("HYPRV_PROFILE");
        } else {
            c.env(
                "HYPRV_PROFILE",
                if mode == "desktop" {
                    "desktop"
                } else {
                    "laptop"
                },
            );
        }
        let result = c.output().unwrap();
        assert!(
            result.status.success(),
            "{mode}\n{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    std::fs::remove_dir_all(base).unwrap();
}
#[test]
fn shared_profile_child() {
    let Ok(mode) = std::env::var("HYPRV_POWER_CONTRACT_CHILD") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("HYPRV_SYSFS_ROOT").unwrap());
    for (name, value) in [
        ("sys/class/dmi/id/product_name", "XPS 13 9350"),
        ("sys/class/power_supply/BAT0/type", "Battery"),
        ("sys/class/power_supply/BAT0/scope", "System"),
        ("sys/class/power_supply/BAT0/present", "1"),
        ("sys/class/power_supply/BAT0/status", "Discharging"),
        ("sys/class/power_supply/BAT0/capacity", "57"),
        ("sys/class/power_supply/BAT0/energy_now", "10000000"),
        ("sys/class/power_supply/BAT0/power_now", "2000000"),
        ("sys/class/backlight/intel_backlight/brightness", "120"),
        ("sys/class/backlight/intel_backlight/max_brightness", "400"),
        ("sys/class/thermal/thermal_zone0/type", "x86_pkg_temp"),
        ("sys/class/thermal/thermal_zone0/temp", "45678"),
    ] {
        put(&root, name, value);
    }
    let selected = profile::profile();
    assert_eq!(
        selected,
        if mode == "desktop" {
            Profile::Desktop
        } else {
            Profile::Laptop
        }
    );
    let _: hyprv_svc_power::Profile = selected; // shared type, not a local policy enum
    assert_eq!(Paths::default().sys, profile::sys_path("sys"));
    assert_eq!(Paths::default().sys, root.join("sys"));
    let rt = Arc::new(ServiceRuntime::new().unwrap());
    rt.block_on(async {
        if mode == "bad-bus" {
            assert!(
                Context::system(rt.clone()).await.is_err(),
                "must not retry host system bus"
            );
            return;
        }
        let server = zbus::connection::Builder::address(
            std::env::var("DBUS_SYSTEM_BUS_ADDRESS").unwrap().as_str(),
        )
        .unwrap()
        .build()
        .await
        .unwrap();
        server.request_name("org.freedesktop.UPower").await.unwrap();
        server.request_name(hyprv_privhelper::BUS).await.unwrap();
        server
            .object_server()
            .at("/org/freedesktop/UPower", Upower)
            .await
            .unwrap();
        server
            .object_server()
            .at("/org/freedesktop/UPower/devices/DisplayDevice", Battery)
            .await
            .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        server
            .object_server()
            .at(hyprv_privhelper::PATH, Helper(calls.clone()))
            .await
            .unwrap();
        let ctx = Context::system(rt.clone()).await.unwrap();
        let id = zbus::fdo::DBusProxy::new(&server)
            .await
            .unwrap()
            .get_id()
            .await
            .unwrap();
        assert_eq!(
            id,
            zbus::fdo::DBusProxy::new(&ctx.bus)
                .await
                .unwrap()
                .get_id()
                .await
                .unwrap()
        );
        let p = power::spawn(ctx.clone());
        let f = fan::spawn(ctx.clone());
        let _fan_visible = f.subscribe();
        tokio::time::timeout(Duration::from_secs(3), async {
            while !p.state.snapshot().value.available {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(p.state.snapshot().value.battery_percent, Some(57.));
        if selected == Profile::Desktop {
            tokio::time::timeout(Duration::from_secs(3), async {
                while !f.state.snapshot().value.available {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert_eq!(p.state.snapshot().value.lid_present, None);
            assert!(calls.load(Ordering::SeqCst) > 0);
        } else {
            assert_eq!(p.state.snapshot().value.lid_present, Some(true));
            assert!(f.action(fan::Action::Refresh).await.is_err());
            assert_eq!(calls.load(Ordering::SeqCst), 0);
            let b = backlight::spawn(ctx.clone());
            let mut seen = b.subscribe();
            tokio::time::timeout(Duration::from_secs(3), async {
                while seen.current().value.outputs.is_empty() {
                    seen.changed().await.unwrap();
                }
            })
            .await
            .unwrap();
            let output = &seen.current().value.outputs[0];
            assert_eq!(output.output, "eDP-1");
            assert_eq!(output.raw, Some(120));
            assert_eq!(output.raw_max, Some(400));
            let detail = battery_history::spawn(ctx.clone());
            assert!(!ctx.paths.runtime.join("battery-power-history.tsv").exists());
            let mut popup = detail.subscribe();
            tokio::time::timeout(Duration::from_secs(3), popup.changed())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(popup.current().value.capacity_percent, Some(57.));
            drop(popup);
            tokio::time::sleep(Duration::from_millis(50)).await;
            let history =
                std::fs::read(ctx.paths.runtime.join("battery-power-history.tsv")).unwrap();
            tokio::time::sleep(Duration::from_millis(1100)).await;
            assert_eq!(
                history,
                std::fs::read(ctx.paths.runtime.join("battery-power-history.tsv")).unwrap()
            );
            let stats = sysstats::spawn(ctx.clone());
            let mut thermal = stats
                .lease(vec![sysstats::Metric::Temperature], Duration::from_secs(1))
                .unwrap();
            tokio::time::timeout(Duration::from_secs(3), thermal.subscription.changed())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                thermal.subscription.current().value.temperature_c,
                Some(45.678)
            );
            assert_eq!(
                calls.load(Ordering::SeqCst),
                0,
                "no laptop cooling/root telemetry calls"
            );
        }
        rt.request_shutdown();
    });
    rt.request_shutdown();
}
