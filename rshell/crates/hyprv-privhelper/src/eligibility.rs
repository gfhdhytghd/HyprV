//! Guard every cooling entry point, including recovery outside the daemon.
use hyprv_core::profile::{profile, sys_path, Profile};
use std::io;

/// A production root service never accepts caller-selected hardware or bus roots.
/// Non-root fixture processes can exercise exactly the same read-only probe.
pub fn trusted_environment() -> io::Result<()> {
    if unsafe { libc::geteuid() } == 0
        && [
            "HYPRV_PROFILE",
            "HYPRV_SYSFS_ROOT",
            "DBUS_SYSTEM_BUS_ADDRESS",
        ]
        .iter()
        .any(|key| std::env::var_os(key).is_some())
    {
        return Err(io::Error::other(
            "root helper refuses user-selected profile/hardware/bus overrides",
        ));
    }
    Ok(())
}

pub fn cooling_for(selected: Profile) -> bool {
    selected == Profile::Desktop
}
pub fn cooling() -> bool {
    // Do not require the chip to exist at ExecCondition time: desktop's original
    // controller must retain boot-time late-module discovery/recovery behavior.
    // Controller/failsafe still validate the exact nct6799 hardware before writes.
    cooling_for(profile())
}
pub fn guarded_failsafe() -> io::Result<()> {
    trusted_environment()?;
    if cooling() {
        crate::cooling::failsafe(&sys_path("sys/class/hwmon"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn laptop_never_enters_controller_or_failsafe_paths() {
        assert!(!cooling_for(Profile::Laptop));
    }
    #[test]
    fn desktop_preserves_late_chip_discovery() {
        assert!(cooling_for(Profile::Desktop));
    }
}
