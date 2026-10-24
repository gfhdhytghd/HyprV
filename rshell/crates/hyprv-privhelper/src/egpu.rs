//! `RemoveExternalGpu(bdf)`: privileged half of `egpu-disconnect.sh`.
//!
//! The caller supplies only a PCI address token. The helper validates its exact
//! syntax, re-derives everything from sysfs (no caller paths), requires the GPU
//! to be *proven* external (Thunderbolt ancestry/label, or the kernel's
//! `removable` attribute) — the legacy bus>=3 heuristic is never accepted as
//! proof by root — and removes slot functions (descending) then non-root
//! parent bridges, like the script.
use std::path::{Path, PathBuf};

/// `DDDD:BB:DD.F`, hex, function 0-7. Returns the lowercase canonical form.
pub fn validate_bdf(token: &str) -> Option<String> {
    let b = token.as_bytes();
    let hex = |i: usize| b[i].is_ascii_hexdigit();
    let ok = b.len() == 12
        && b[4] == b':'
        && b[7] == b':'
        && b[10] == b'.'
        && [0, 1, 2, 3, 5, 6, 8, 9].iter().all(|i| hex(*i))
        && (b'0'..=b'7').contains(&b[11]);
    ok.then(|| token.to_ascii_lowercase())
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}
fn is_gpu(dev: &Path) -> bool {
    matches!(read(&dev.join("class")).as_str(), "0x030000" | "0x030200")
}
/// External proof walking the resolved sysfs path upwards.
pub fn proven_external(sys: &Path, dev: &Path) -> bool {
    if read(&dev.join("removable")) == "removable" {
        return true;
    }
    let Ok(mut current) = std::fs::canonicalize(dev) else {
        return false;
    };
    let stop = std::fs::canonicalize(sys).unwrap_or_else(|_| sys.to_path_buf());
    while current.starts_with(&stop) && current != stop {
        if current.join("domain0").is_dir()
            || current
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|n| n == "thunderbolt")
        {
            return true;
        }
        let label = read(&current.join("label"));
        if label.contains("Thunderbolt") || label.contains("thunderbolt") {
            return true;
        }
        if !current.pop() {
            break;
        }
    }
    false
}
fn version_key(s: &str) -> Vec<u32> {
    s.split([':', '.'])
        .map(|p| u32::from_str_radix(p, 16).unwrap_or(0))
        .collect()
}
/// Removal chain for a validated, proven-external GPU.
pub fn plan(sys: &Path, token: &str) -> Result<Vec<String>, String> {
    let bdf = validate_bdf(token).ok_or("invalid PCI address")?;
    let devices = sys.join("bus/pci/devices");
    let dev = devices.join(&bdf);
    if !dev.exists() {
        return Err("device not present".into());
    }
    if !is_gpu(&dev) {
        return Err("device is not a display controller".into());
    }
    if !proven_external(sys, &dev) {
        return Err("device is not proven external (Thunderbolt/removable)".into());
    }
    let slot = &bdf[..10];
    let mut chain: Vec<String> = std::fs::read_dir(&devices)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter_map(|e| validate_bdf(&e.file_name().to_string_lossy()))
        .filter(|n| n.starts_with(slot) && n.as_bytes()[10] == b'.')
        .collect();
    chain.sort_by_key(|n| std::cmp::Reverse(version_key(n)));
    let real = std::fs::canonicalize(&dev).map_err(|e| e.to_string())?;
    let stop = std::fs::canonicalize(sys).unwrap_or_else(|_| sys.to_path_buf());
    let mut current = real.parent().map(Path::to_path_buf);
    while let Some(c) = current {
        if !c.starts_with(&stop) || c == stop {
            break;
        }
        let parent = c.parent().map(Path::to_path_buf);
        if let Some(name) = c
            .file_name()
            .and_then(|n| validate_bdf(&n.to_string_lossy()))
        {
            let class = read(&c.join("class"));
            let root_port = parent
                .as_ref()
                .and_then(|p| p.file_name())
                .is_some_and(|n| n.to_string_lossy().starts_with("pci0000:"));
            if (class == "0x060400" || class == "0x060401") && !root_port {
                chain.push(name);
            }
        }
        current = parent;
    }
    let mut seen = std::collections::HashSet::new();
    chain.retain(|b| seen.insert(b.clone()));
    Ok(chain)
}
/// Execute: `echo 1 > /sys/bus/pci/devices/BDF/remove` per element.
/// Returns `(bdf, "removed"|"error")`; absent devices are skipped like the script.
pub fn remove(sys: &Path, chain: &[String]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for bdf in chain {
        let Some(bdf) = validate_bdf(bdf) else {
            continue;
        };
        let file: PathBuf = sys.join("bus/pci/devices").join(&bdf).join("remove");
        if !file.is_file() {
            continue;
        }
        let status = match std::fs::write(&file, "1") {
            Ok(()) => "removed",
            Err(_) => "error",
        };
        out.push((bdf, status.to_string()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_bdf() {
        assert_eq!(
            validate_bdf("0000:06:00.0").as_deref(),
            Some("0000:06:00.0")
        );
        assert_eq!(
            validate_bdf("0000:0A:00.1").as_deref(),
            Some("0000:0a:00.1")
        );
        for bad in [
            "0000:06:00.8",
            "0000:06:00.0/",
            "../06:00.0",
            "0000:06:00",
            "0000-06:00.0",
            "0000:06:00.0\n",
            "g000:06:00.0",
            "",
        ] {
            assert!(validate_bdf(bad).is_none(), "{bad:?}");
        }
    }
    fn mk(root: &Path, rel: &str, class: &str) -> PathBuf {
        let p = root.join(rel);
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join("class"), format!("{class}\n")).unwrap();
        std::fs::write(p.join("remove"), "").unwrap();
        std::os::unix::fs::symlink(
            &p,
            root.join("bus/pci/devices").join(p.file_name().unwrap()),
        )
        .unwrap();
        p
    }
    #[test]
    fn plan_requires_proof_and_orders_chain() {
        let root = std::env::temp_dir().join(format!("hyprv-ph-egpu-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("bus/pci/devices")).unwrap();
        mk(&root, "devices/pci0000:00/0000:00:07.0", "0x060400");
        let br = mk(
            &root,
            "devices/pci0000:00/0000:00:07.0/0000:05:00.0",
            "0x060400",
        );
        mk(
            &root,
            "devices/pci0000:00/0000:00:07.0/0000:05:00.0/0000:06:00.0",
            "0x030000",
        );
        mk(
            &root,
            "devices/pci0000:00/0000:00:07.0/0000:05:00.0/0000:06:00.1",
            "0x040300",
        );
        // Heuristic bus>=3 alone is refused by root.
        assert!(plan(&root, "0000:06:00.0")
            .unwrap_err()
            .contains("not proven external"));
        std::fs::write(br.join("label"), "Thunderbolt 4\n").unwrap();
        assert_eq!(
            plan(&root, "0000:06:00.0").unwrap(),
            ["0000:06:00.1", "0000:06:00.0", "0000:05:00.0"]
        );
        assert!(plan(&root, "0000:06:00.1")
            .unwrap_err()
            .contains("not a display"));
        assert!(plan(&root, "0000:09:00.0").is_err());
        let results = remove(&root, &plan(&root, "0000:06:00.0").unwrap());
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|(_, s)| s == "removed"));
        assert_eq!(std::fs::read_to_string(br.join("remove")).unwrap(), "1");
        std::fs::remove_dir_all(root).unwrap();
    }
}
