use super::*;
#[cfg(test)]
mod adversarial_review {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new(label: &str) -> Self {
            let p = std::env::temp_dir()
                .join(format!("hyprv-regression-{}-{label}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            for d in ["state", "hwmon/chip", "hwmon/cpu"] {
                std::fs::create_dir_all(p.join(d)).unwrap();
            }
            for (n, v) in [
                ("name", "nct6799"),
                ("pwm1", "0"),
                ("pwm7", "0"),
                ("pwm1_enable", "2"),
                ("pwm7_enable", "2"),
                ("fan1_input", "0"),
                ("fan7_input", "0"),
            ] {
                std::fs::write(p.join("hwmon/chip").join(n), v).unwrap();
            }
            for (n, v) in [
                ("name", "coretemp"),
                ("temp1_label", "Package id 0"),
                ("temp1_input", "60000"),
            ] {
                std::fs::write(p.join("hwmon/cpu").join(n), v).unwrap();
            }
            std::fs::write(p.join("state/fan-percent"), "chassis=80\npump=80\n").unwrap();
            Self(p.canonicalize().unwrap())
        }
        fn open(&self) -> Controller {
            Controller::open_test(self.0.join("hwmon"), &self.0.join("state")).unwrap()
        }
        fn value(&self, n: &str) -> u32 {
            number(&self.0.join("hwmon/chip").join(n)).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn review_helper_shutdown_restores_autonomous_cooling() {
        let f = Fixture::new("helper-shutdown");
        let mut c = f.open();
        c.tick(true).unwrap();
        assert_eq!(f.value("pwm1"), 0);
        drop(c);
        assert_eq!(f.value("pwm1_enable"), 1);
        assert_eq!(f.value("pwm1"), 255);
    }
    #[test]
    fn recovery_ignores_corrupt_state_and_held_controller_locks() {
        let f = Fixture::new("crash-recovery");
        let mut c = f.open();
        c.tick(true).unwrap();
        std::fs::write(f.0.join("state/fan-percent"), [0xff]).unwrap();
        failsafe(&f.0.join("hwmon")).unwrap();
        assert_eq!(f.value("pwm1_enable"), 1);
        assert_eq!(f.value("pwm1"), 255);
        assert_eq!(f.value("pwm7_enable"), 1);
        assert_eq!(f.value("pwm7"), 255);
    }
    #[test]
    fn one_failed_sensor_cannot_be_hidden_by_a_cool_package() {
        let f = Fixture::new("partial-sensor");
        std::fs::write(f.0.join("hwmon/cpu/temp2_label"), "Package id 1").unwrap();
        std::fs::write(f.0.join("hwmon/cpu/temp2_input"), "invalid").unwrap();
        let mut c = f.open();
        let state = c.tick(true).unwrap();
        assert_eq!(state.temperature_c, None);
        assert_eq!(state.channels[0].pwm, 255);
        assert_eq!(state.channels[1].pwm, 255);
        assert!(state.last_error.is_some());
    }
    #[test]
    fn review_helper_untrusted_state_cannot_bypass_authorization() {
        let f = Fixture::new("helper-state");
        std::fs::write(f.0.join("state/fan-percent"), "chassis=0\npump=0\n").unwrap();
        std::fs::write(f.0.join("hwmon/cpu/temp1_input"), "105001").unwrap();
        let mut c = f.open();
        let s = c.tick(true).unwrap();
        assert!(
            Controller::open(f.0.join("hwmon"), &f.0.join("state")).is_err(),
            "production must reject user-owned persistence"
        );
        assert_eq!(
            s.channels[0].pwm, 255,
            "emergency must override even test-injected zero limits"
        );
    }
    #[test]
    fn review_helper_bad_state_does_not_prevent_boot_cooling() {
        let f = Fixture::new("helper-invalid");
        std::fs::write(f.0.join("state/fan-percent"), [0xff]).unwrap();
        let mut c = f.open();
        let s = c.tick(false).unwrap();
        assert_eq!(s.channels[0].pwm, 255);
        assert!(s.last_error.is_some());
    }
    #[test]
    fn review_helper_one_channel_error_does_not_skip_other_channel() {
        let f = Fixture::new("helper-partial");
        let mut c = f.open();
        std::fs::write(f.0.join("hwmon/cpu/temp1_input"), "105001").unwrap();
        std::fs::remove_file(f.0.join("hwmon/chip/pwm1")).unwrap();
        std::fs::remove_file(f.0.join("hwmon/chip/pwm1_enable")).unwrap();
        assert!(c.tick(true).is_err());
        assert_eq!(
            f.value("pwm7"),
            255,
            "healthy pwm7 must enter maximum-speed mode"
        );
    }
    #[test]
    fn review_helper_overrange_hot_sensor_does_not_choose_cold_sensor() {
        let f = Fixture::new("helper-hot");
        std::fs::write(f.0.join("hwmon/cpu/temp2_label"), "Package id 1").unwrap();
        std::fs::write(f.0.join("hwmon/cpu/temp2_input"), "130000").unwrap();
        let mut c = f.open();
        let s = c.tick(true).unwrap();
        assert!(
            s.channels[0].pwm > 0,
            "130C package silently discarded in favor of 60C package: {s:?}"
        );
    }
    #[test]
    fn review_helper_critical_temperature_overrides_zero_cap() {
        assert!(
            target_pwm(0, Some(120000)) > 0,
            "critical heat retains zero PWM cap"
        );
    }
}
#[cfg(test)]
mod adversarial_lock_review {
    use super::*;
    #[test]
    fn review_helper_user_cannot_replace_singleton_lock() {
        let p = std::env::temp_dir().join(format!("hyprv-untrusted-lock-{}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        assert!(
            StateDirectory::open(&p).is_err(),
            "production must reject a replaceable user-directory lock namespace"
        );
        std::fs::remove_dir_all(p).unwrap();
    }
}
#[test]
fn protected_namespace_rejects_unsafe_mode_and_file_replacement() {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("hyprv-state-modes-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o777)).unwrap();
    assert!(StateDirectory::open_owned(&root, unsafe { libc::geteuid() }, false).is_err());
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let dir = StateDirectory::open_owned(&root, unsafe { libc::geteuid() }, false).unwrap();
    std::fs::write(root.join("fan-percent"), "chassis=0\npump=0\n").unwrap();
    std::fs::set_permissions(
        root.join("fan-percent"),
        std::fs::Permissions::from_mode(0o666),
    )
    .unwrap();
    assert!(dir.file("fan-percent", libc::O_RDONLY).is_err());
    std::fs::set_permissions(
        root.join("fan-percent"),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    std::fs::hard_link(root.join("fan-percent"), root.join("other")).unwrap();
    assert!(dir.file("fan-percent", libc::O_RDONLY).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn malformed_limits_are_atomic_and_bounded() {
    let path = std::env::temp_dir().join(format!("hyprv-limit-parser-{}", std::process::id()));
    for bytes in [
        b"chassis=50\npump=101".to_vec(),
        b"chassis=0\nchassis=1\npump=2".to_vec(),
        vec![b'x'; 4097],
        vec![0xff],
        b"chassis=0".to_vec(),
    ] {
        std::fs::write(&path, bytes).unwrap();
        assert!(parse_limits(File::open(&path).unwrap()).is_err());
    }
    std::fs::remove_file(path).unwrap();
}

// Stateful fake sysfs stores, rather than permissive regular files. Like
// kstrtoul/store_pwm_enable/store_pwm, every write is a separate store.
#[cfg(test)]
mod nct6775_sysfs {
    use super::*;
    use std::{cell::RefCell, rc::Rc};
    #[derive(Default)]
    struct Driver {
        modes: [u32; 2],
        duty: [u32; 2],
        stores: Vec<(String, Vec<u8>)>,
    }
    struct Attribute {
        chip: Rc<RefCell<Driver>>,
        name: String,
    }
    impl Write for Attribute {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let mut chip = self.chip.borrow_mut();
            chip.stores.push((self.name.clone(), bytes.to_vec()));
            let value: u32 = std::str::from_utf8(bytes)
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .ok_or_else(|| io::Error::from_raw_os_error(libc::EINVAL))?;
            let i = usize::from(self.name.starts_with("pwm7"));
            if self.name.ends_with("_enable") {
                // nct6799: SmartFan III (4) invalid, IV (5) depends on trip
                // points. This fake's firmware has inconsistent points.
                if ![0, 1, 2, 3].contains(&value) {
                    return Err(io::Error::from_raw_os_error(libc::EINVAL));
                }
                chip.modes[i] = value;
                if value == 0 {
                    chip.duty[i] = 255;
                }
            } else {
                if chip.modes[i] > 1 {
                    return Err(io::Error::from_raw_os_error(libc::EBUSY));
                }
                chip.duty[i] = value.min(255);
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn old_formatted_store_reproduces_einval_after_side_effect() {
        let chip = Rc::new(RefCell::new(Driver::default()));
        let mut attr = Attribute {
            chip: chip.clone(),
            name: "pwm1_enable".into(),
        };
        let mode = 1u32;
        let error = writeln!(attr, "{mode}").unwrap_err();
        assert_eq!(error.raw_os_error(), Some(libc::EINVAL));
        assert_eq!(chip.borrow().modes[0], 1, "first write already took effect");
        assert_eq!(chip.borrow().stores.last().unwrap().1, b"\n");
    }
    #[test]
    fn failsafe_handles_all_supported_initial_modes_with_single_ordered_stores() {
        for initial in [0, 1, 2, 3, 5] {
            let chip = Rc::new(RefCell::new(Driver {
                modes: [initial; 2],
                duty: [0; 2],
                stores: vec![],
            }));
            for i in [1, 7] {
                maximum_speed(
                    |name, value| {
                        write_attribute(
                            &mut Attribute {
                                chip: chip.clone(),
                                name: name.into(),
                            },
                            value,
                        )
                    },
                    i,
                )
                .unwrap();
            }
            let chip = chip.borrow();
            assert_eq!(chip.modes, [1; 2]);
            assert_eq!(chip.duty, [255; 2]);
            assert_eq!(
                chip.stores,
                vec![
                    ("pwm1_enable".into(), b"1\n".to_vec()),
                    ("pwm1".into(), b"255\n".to_vec()),
                    ("pwm7_enable".into(), b"1\n".to_vec()),
                    ("pwm7".into(), b"255\n".to_vec())
                ]
            );
        }
    }
    #[test]
    fn automatic_duty_and_invalid_auto_mode_are_rejected() {
        let chip = Rc::new(RefCell::new(Driver {
            modes: [2, 5],
            ..Default::default()
        }));
        let mut duty = Attribute {
            chip: chip.clone(),
            name: "pwm7".into(),
        };
        assert_eq!(
            write_attribute(&mut duty, 255).unwrap_err().raw_os_error(),
            Some(libc::EBUSY)
        );
        for value in [4, 5, 6] {
            assert_eq!(
                write_attribute(
                    &mut Attribute {
                        chip: chip.clone(),
                        name: "pwm7_enable".into()
                    },
                    value
                )
                .unwrap_err()
                .raw_os_error(),
                Some(libc::EINVAL)
            );
        }
    }
    #[test]
    fn partial_store_is_an_error_and_never_retried_as_a_second_sysfs_store() {
        struct Short(usize);
        impl Write for Short {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                self.0 += 1;
                Ok(1)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut out = Short(0);
        assert!(write_attribute(&mut out, 255).is_err());
        assert_eq!(out.0, 1);
    }
}
