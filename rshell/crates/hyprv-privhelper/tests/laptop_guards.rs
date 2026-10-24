//! Read-only subprocess probes against isolated fake hardware; never run as root.
use std::{fs, path::PathBuf, process::Command};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("hyprv-laptop-guard-{}", std::process::id()));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn put(&self, name: &str, value: &str) {
        let p = self.0.join(name);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, value).unwrap();
    }
    fn command(&self, binary: &str) -> Command {
        let mut c = Command::new(binary);
        c.env("HYPRV_SYSFS_ROOT", &self.0)
            .env("HYPRV_PROFILE", "laptop");
        c
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn laptop_probe_and_failsafe_never_touch_even_a_fake_nct_chip() {
    assert_ne!(
        unsafe { libc::geteuid() },
        0,
        "this test must never run privileged"
    );
    let f = Fixture::new();
    f.put("sys/class/dmi/id/product_name", "XPS 13 9350");
    f.put("sys/class/hwmon/hwmon0/name", "nct6799");
    f.put("sys/class/hwmon/hwmon0/pwm1", "19");
    f.put("sys/class/hwmon/hwmon0/pwm1_enable", "2");
    let status = f
        .command(env!("CARGO_BIN_EXE_hyprv-privhelper"))
        .arg("--eligible-cooling")
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(1));
    let automatic = f
        .command(env!("CARGO_BIN_EXE_hyprv-privhelper"))
        .env_remove("HYPRV_PROFILE")
        .arg("--eligible-cooling")
        .status()
        .unwrap();
    assert_eq!(
        automatic.code(),
        Some(1),
        "DMI autodetect must also skip cooling"
    );

    assert!(f
        .command(env!("CARGO_BIN_EXE_hyprv-cooling-failsafe"))
        .status()
        .unwrap()
        .success());
    assert_eq!(
        fs::read_to_string(f.0.join("sys/class/hwmon/hwmon0/pwm1")).unwrap(),
        "19"
    );
    assert_eq!(
        fs::read_to_string(f.0.join("sys/class/hwmon/hwmon0/pwm1_enable")).unwrap(),
        "2"
    );
    let power = f.0.join("sys/class/powercap/intel-rapl:0");
    f.put("sys/class/powercap/intel-rapl:0/name", "package-0");
    f.put(
        "sys/class/powercap/intel-rapl:0/max_energy_range_uj",
        "1000000",
    );
    f.put("sys/class/powercap/intel-rapl:0/energy_uj", "123");
    assert!(f
        .command(env!("CARGO_BIN_EXE_hyprv-cpu-power"))
        .arg("--eligible")
        .status()
        .unwrap()
        .success());
    assert_eq!(fs::read_to_string(power.join("energy_uj")).unwrap(), "123");
    fs::remove_dir_all(power).unwrap();
    assert_eq!(
        f.command(env!("CARGO_BIN_EXE_hyprv-cpu-power"))
            .arg("--eligible")
            .status()
            .unwrap()
            .code(),
        Some(1)
    );
}
