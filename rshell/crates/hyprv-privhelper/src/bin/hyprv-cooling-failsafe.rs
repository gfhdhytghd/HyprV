//! Fixed-path crash/startup recovery. No bus, persistence, or caller arguments.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    hyprv_privhelper::eligibility::trusted_environment()?;
    if !hyprv_privhelper::eligibility::cooling() {
        return Ok(());
    }
    if std::env::args_os().skip(1).collect::<Vec<_>>() == ["--check"] {
        hyprv_privhelper::cooling::check(&hyprv_core::profile::sys_path("sys/class/hwmon"))?;
        println!("cooling capability v2: single-store manual-enable then PWM 255");
        return Ok(());
    }
    if unsafe { libc::geteuid() } != 0 {
        return Err("cooling recovery requires root".into());
    }
    if std::env::args_os().len() != 1 {
        return Err("no arguments accepted".into());
    }
    hyprv_privhelper::eligibility::guarded_failsafe()?;
    Ok(())
}
