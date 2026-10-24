//! Process-wide runtime policy. Selection and fixture roots are latched before actors start.
use std::{
    env, fs,
    path::{Component, Path, PathBuf},
    sync::OnceLock,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Profile {
    Desktop,
    Laptop,
}

static ROOT: OnceLock<PathBuf> = OnceLock::new();
static PROFILE: OnceLock<Profile> = OnceLock::new();

/// Filesystem root for native sysfs readers. A supplied fixture never falls back to `/`.
pub fn sys_root() -> &'static Path {
    ROOT.get_or_init(|| {
        env::var_os("HYPRV_SYSFS_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"))
    })
    .as_path()
}

/// Resolve a path relative to the selected filesystem root (leading `/` is accepted).
/// Parent traversal is rejected, so fixture readers cannot accidentally escape their root.
pub fn sys_path(rel: impl AsRef<Path>) -> PathBuf {
    let mut path = sys_root().to_path_buf();
    for part in rel.as_ref().components() {
        match part {
            Component::Normal(p) => path.push(p),
            Component::RootDir | Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) => {
                panic!("sys_path requires a root-relative path without parent traversal")
            }
        }
    }
    path
}

/// Select once, before creating services or UI. Invalid explicit overrides are fatal.
/// AC, hotplug and legacy color-variant changes never alter the selected policy.
pub fn profile() -> Profile {
    *PROFILE.get_or_init(|| {
        let selected = match env::var("HYPRV_PROFILE") {
            Ok(value) => override_profile(&value).unwrap_or_else(|e| panic!("{e}")),
            Err(env::VarError::NotPresent) => detect(sys_root()),
            Err(env::VarError::NotUnicode(_)) => {
                panic!("HYPRV_PROFILE must be desktop or laptop (invalid Unicode)")
            }
        };
        eprintln!(
            "hyprv-shell: runtime profile {selected:?} (latched; root {})",
            sys_root().display()
        );
        selected
    })
}

fn override_profile(value: &str) -> Result<Profile, String> {
    match value {
        "desktop" => Ok(Profile::Desktop),
        "laptop" => Ok(Profile::Laptop),
        _ => Err(format!(
            "HYPRV_PROFILE must be desktop or laptop; got {value:?}"
        )),
    }
}

fn read(root: &Path, rel: &Path) -> Option<String> {
    let path = root.join(rel);
    // sys/class entries are normally links. Permit them inside the selected root only.
    let canonical = path.canonicalize().ok()?;
    if !canonical.starts_with(root.canonicalize().ok()?) {
        return None;
    }
    fs::read_to_string(canonical)
        .ok()
        .map(|v| v.trim().to_owned())
}

fn detect(root: &Path) -> Profile {
    let product = read(root, Path::new("sys/class/dmi/id/product_name"));
    if product
        .as_deref()
        .is_some_and(|p| matches!(p.to_ascii_lowercase().as_str(), "xps 13 9350" | "xps 9350"))
    {
        eprintln!(
            "hyprv-shell: portable DMI identity {}",
            product.as_deref().unwrap_or_default()
        );
        return Profile::Laptop;
    }
    if let Ok(entries) = fs::read_dir(root.join("sys/class/power_supply")) {
        for entry in entries.flatten() {
            let rel = Path::new("sys/class/power_supply").join(entry.file_name());
            if read(root, &rel.join("type")).as_deref() != Some("Battery")
                || read(root, &rel.join("present")).as_deref() != Some("1")
            {
                continue;
            }
            // Explicit System scope is the kernel's system-power-supply identity.
            // Missing scope is ambiguous: never mistake a mouse/UPS for a laptop.
            if read(root, &rel.join("scope")).as_deref() == Some("System") {
                eprintln!(
                    "hyprv-shell: present System-scope battery {}",
                    entry.file_name().to_string_lossy()
                );
                return Profile::Laptop;
            }
        }
    }
    eprintln!("hyprv-shell: no recognized portable DMI/internal battery; using desktop");
    Profile::Desktop
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let p = env::temp_dir().join(format!(
                "hyprv-profile-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn put(&self, p: &str, value: &str) {
            let p = self.0.join(p);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, value).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn selection_is_latched_in_a_fresh_process() {
        const CHILD: &str = "HYPRV_PROFILE_LATCH_TEST";
        if env::var_os(CHILD).is_some() {
            assert_eq!(profile(), Profile::Laptop);
            let original_root = sys_root().to_path_buf();
            env::set_var("HYPRV_PROFILE", "desktop");
            env::set_var("HYPRV_SYSFS_ROOT", "/this-must-not-be-read");
            assert_eq!(profile(), Profile::Laptop);
            assert_eq!(sys_root(), original_root);
            assert_eq!(
                sys_path("/sys/class/thermal"),
                original_root.join("sys/class/thermal")
            );
            return;
        }
        let f = Fixture::new();
        let status = std::process::Command::new(env::current_exe().unwrap())
            .args([
                "--exact",
                "profile::tests::selection_is_latched_in_a_fresh_process",
            ])
            .env(CHILD, "1")
            .env("HYPRV_PROFILE", "laptop")
            .env("HYPRV_SYSFS_ROOT", &f.0)
            .status()
            .unwrap();
        assert!(status.success());
    }
    #[test]
    fn overrides_are_exact() {
        assert_eq!(override_profile("desktop").unwrap(), Profile::Desktop);
        assert_eq!(override_profile("laptop").unwrap(), Profile::Laptop);
        for s in ["", "Laptop", "auto", " laptop"] {
            assert!(override_profile(s).is_err());
        }
    }
    #[test]
    fn portable_dmi_never_matches_a_tower() {
        let f = Fixture::new();
        for (name, want) in [
            ("XPS 13 9350\n", Profile::Laptop),
            ("XPS 9350", Profile::Laptop),
            ("XPS 8930", Profile::Desktop),
            ("unknown", Profile::Desktop),
        ] {
            f.put("sys/class/dmi/id/product_name", name);
            assert_eq!(detect(&f.0), want);
        }
    }
    #[test]
    fn only_present_system_batteries_count() {
        let f = Fixture::new();
        let b = "sys/class/power_supply/BAT0/";
        for (kind, present, scope, want) in [
            ("Battery", "1", "System", Profile::Laptop),
            ("Battery", "0", "System", Profile::Desktop),
            ("Battery", "1", "Device", Profile::Desktop),
            ("UPS", "1", "System", Profile::Desktop),
            ("Battery", "1", "", Profile::Desktop),
        ] {
            for (p, v) in [("type", kind), ("present", present), ("scope", scope)] {
                f.put(&format!("{b}{p}"), v);
            }
            assert_eq!(detect(&f.0), want);
        }
    }
    #[test]
    fn missing_and_escaped_fixture_never_use_host() {
        let f = Fixture::new();
        assert_eq!(detect(&f.0), Profile::Desktop);
        fs::create_dir_all(f.0.join("sys/class/dmi")).unwrap();
        std::os::unix::fs::symlink("/sys/class/dmi/id", f.0.join("sys/class/dmi/id")).unwrap();
        assert_eq!(detect(&f.0), Profile::Desktop);
    }
}
