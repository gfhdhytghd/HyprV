//! External GPU inventory and removal: port of `egpu-disconnect.sh`.
//!
//! Inventory is read from sysfs and refreshed on kernel `pci`/`thunderbolt`
//! uevents (no polling). Removal is delegated to the privileged helper
//! (`RemoveExternalGpu`, see README) and confirmed by the device disappearing.
//! The two-step confirmation (`ControlPanelPopup.qml:400`) stays in the UI.
use crate::{topic_key, uevent::UeventSocket, Ctx, Handle, ServiceError};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub const HELPER_BUS: &str = "org.hyprv.Shell.PrivHelper";
pub const HELPER_PATH: &str = "/org/hyprv/Shell/PrivHelper";
pub const HELPER_IFACE: &str = "org.hyprv.Shell.PrivHelper1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EgpuDevice {
    pub bdf: String,
    /// `vendor:device` PCI IDs plus sysfs `label` when present.
    pub label: String,
    /// Thunderbolt ancestry or label proved; false = bus>=3 heuristic only.
    pub externally_verified: bool,
    /// Removal order: slot functions (descending), then non-root parent bridges.
    pub removal_chain: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemovalResult {
    pub bdf: String,
    /// `removed` | `error`
    pub status: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EgpuState {
    /// Candidates in sysfs order; the panel action removes the first.
    pub devices: Vec<EgpuDevice>,
    pub busy: bool,
    pub last_results: Vec<RemovalResult>,
    pub last_error: Option<String>,
}
topic_key!(EgpuTopic, EgpuState);

#[derive(Clone, Debug)]
pub enum EgpuAction {
    /// `None`: first candidate, like the script. `Some(bdf)` must be a current candidate.
    Remove(Option<String>),
}
pub type EgpuHandle = Handle<EgpuState, EgpuAction>;

pub fn spawn(ctx: &Ctx) -> EgpuHandle {
    spawn_with(ctx, PathBuf::from("/sys"))
}
pub fn spawn_with(ctx: &Ctx, sys: PathBuf) -> EgpuHandle {
    let topic = crate::topic::<EgpuTopic>(ctx, EgpuState::default());
    let (handle, mut rx) = Handle::<EgpuState, EgpuAction>::new(topic.clone());
    let mut shutdown = ctx.shutdown.clone();
    ctx.rt.spawn(async move {
        let uevents = UeventSocket::open().ok();
        let mut state = EgpuState::default();
        loop {
            state.devices = inventory(&sys);
            topic.publish(state.clone());
            let hotplug = async {
                match &uevents {
                    Some(u) => loop {
                        match u.next().await {
                            Ok(ev) if ev.subsystem == "pci" || ev.subsystem == "thunderbolt" => {
                                break
                            }
                            Ok(_) => {}
                            Err(_) => std::future::pending::<()>().await,
                        }
                    },
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                _ = hotplug => {}
                msg = rx.recv() => {
                    let Some(env) = msg else { return };
                    state.busy = true;
                    topic.publish(state.clone());
                    let EgpuAction::Remove(bdf) = env.action.clone();
                    let result = remove(&sys, &state.devices, bdf).await;
                    state.busy = false;
                    match &result {
                        Ok(results) => {
                            state.last_results = results.clone();
                            state.last_error = None;
                        }
                        Err(e) => state.last_error = Some(e.message.clone()),
                    }
                    env.respond(result.map(|r| render(&r)));
                }
                _ = shutdown.changed() => if *shutdown.borrow() { return },
            }
        }
    });
    handle
}

/// Legacy stdout: `device=BDF` + `status=...` per device, or `status=not_found`.
pub fn render(results: &[RemovalResult]) -> String {
    if results.is_empty() {
        return "status=not_found\n".into();
    }
    results
        .iter()
        .map(|r| format!("device={}\nstatus={}\n", r.bdf, r.status))
        .collect()
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}
pub fn is_bdf(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 12
        && b[4] == b':'
        && b[7] == b':'
        && b[10] == b'.'
        && [0, 1, 2, 3, 5, 6, 8, 9, 11]
            .iter()
            .all(|i| b[*i].is_ascii_hexdigit())
}
/// `is_thunderbolt_device`: returns (verified, heuristic).
fn thunderbolt(sys: &Path, dev: &Path) -> (bool, bool) {
    let real = std::fs::canonicalize(dev).unwrap_or_else(|_| dev.to_path_buf());
    let mut current = real.clone();
    loop {
        let s = current.to_string_lossy();
        if s == "/" || current == sys || s == "/sys" {
            break;
        }
        if current.join("domain0").is_dir()
            || current
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|n| n == "thunderbolt")
        {
            return (true, false);
        }
        let label = read(&current.join("label"));
        if label.contains("Thunderbolt") || label.contains("thunderbolt") {
            return (true, false);
        }
        if !current.pop() {
            break;
        }
    }
    let text = real.to_string_lossy();
    let pattern = text.split('/').any(|c| c.starts_with("pci")) && text.matches(':').count() >= 2;
    let bus = dev
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.split(':').nth(1))
        .and_then(|b| u32::from_str_radix(b, 16).ok());
    (false, pattern && bus.is_some_and(|b| b >= 3))
}
fn version_key(s: &str) -> Vec<u32> {
    s.split([':', '.'])
        .map(|p| u32::from_str_radix(p, 16).unwrap_or(0))
        .collect()
}
fn chain(sys: &Path, dev: &Path) -> Vec<String> {
    let devices = sys.join("bus/pci/devices");
    let bdf = dev
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let slot = bdf
        .rsplit_once('.')
        .map(|(s, _)| s.to_string())
        .unwrap_or_default();
    let mut out: Vec<String> = std::fs::read_dir(&devices)
        .map(|it| {
            it.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with(&format!("{slot}.")))
                .collect()
        })
        .unwrap_or_default();
    out.sort_by_key(|n| std::cmp::Reverse(version_key(n)));
    let real = std::fs::canonicalize(dev).unwrap_or_else(|_| dev.to_path_buf());
    let mut current = real.parent().map(Path::to_path_buf);
    while let Some(c) = current {
        if c == sys || c == sys.join("devices") || c.as_os_str().len() <= 1 {
            break;
        }
        let name = c
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let parent = c.parent().map(Path::to_path_buf);
        if is_bdf(&name) {
            let class = read(&c.join("class"));
            let root_port = parent
                .as_ref()
                .and_then(|p| p.file_name())
                .is_some_and(|n| n.to_string_lossy().starts_with("pci0000:"));
            if (class == "0x060400" || class == "0x060401") && !root_port {
                out.push(name);
            }
        }
        current = parent;
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|b| seen.insert(b.clone()));
    out
}
pub fn inventory(sys: &Path) -> Vec<EgpuDevice> {
    let devices = sys.join("bus/pci/devices");
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&devices)
        .map(|it| it.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    entries.sort();
    let mut out = Vec::new();
    for dev in entries {
        let class = read(&dev.join("class"));
        if class != "0x030000" && class != "0x030200" {
            continue;
        }
        let (verified, heuristic) = thunderbolt(sys, &dev);
        if !verified && !heuristic {
            continue;
        }
        let ids = format!(
            "{}:{}",
            read(&dev.join("vendor")).trim_start_matches("0x"),
            read(&dev.join("device")).trim_start_matches("0x")
        );
        let label = read(&dev.join("label"));
        out.push(EgpuDevice {
            bdf: dev.file_name().unwrap().to_string_lossy().into_owned(),
            label: if label.is_empty() {
                ids
            } else {
                format!("{label} ({ids})")
            },
            externally_verified: verified,
            removal_chain: chain(sys, &dev),
        });
    }
    out
}

async fn remove(
    sys: &Path,
    devices: &[EgpuDevice],
    bdf: Option<String>,
) -> Result<Vec<RemovalResult>, ServiceError> {
    // Revalidate against a fresh inventory, never a caller-supplied path.
    let fresh = inventory(sys);
    let target = match bdf {
        None => fresh.first(),
        Some(b) => fresh
            .iter()
            .find(|d| d.bdf == b && devices.iter().any(|o| o.bdf == b)),
    };
    let Some(target) = target.cloned() else {
        return Ok(Vec::new());
    };
    let uevents = UeventSocket::open().ok();
    let conn = zbus::Connection::system().await?;
    let reply = conn
        .call_method(
            Some(HELPER_BUS),
            HELPER_PATH,
            Some(HELPER_IFACE),
            "RemoveExternalGpu",
            &(target.bdf.as_str(),),
        )
        .await;
    let helper: Vec<(String, String)> = match reply {
        Ok(msg) => msg.body().deserialize().unwrap_or_default(),
        Err(zbus::Error::MethodError(name, _, _))
            if name.as_str() == "org.freedesktop.DBus.Error.UnknownMethod"
                || name.as_str() == "org.freedesktop.DBus.Error.ServiceUnknown"
                || name.as_str() == "org.freedesktop.DBus.Error.NotSupported" =>
        {
            return Err(ServiceError::unsupported(
                "privileged helper lacks RemoveExternalGpu",
            ));
        }
        Err(e) => return Err(e.into()),
    };
    // Confirm by disappearance (uevent remove), bounded at action time.
    let gone = |b: &str| !sys.join("bus/pci/devices").join(b).exists();
    if let Some(u) = &uevents {
        let _ = tokio::time::timeout(Duration::from_secs(3), async {
            while !target.removal_chain.iter().all(|b| gone(b)) {
                if u.next().await.is_err() {
                    break;
                }
            }
        })
        .await;
    }
    let order: Vec<String> = if helper.is_empty() {
        target.removal_chain.clone()
    } else {
        helper.iter().map(|(b, _)| b.clone()).collect()
    };
    Ok(order
        .into_iter()
        .map(|b| RemovalResult {
            status: if gone(&b) { "removed" } else { "error" }.into(),
            bdf: b,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mkdev(root: &Path, rel: &str, class: &str) -> PathBuf {
        let p = root.join(rel);
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join("class"), format!("{class}\n")).unwrap();
        std::fs::write(p.join("vendor"), "0x10de\n").unwrap();
        std::fs::write(p.join("device"), "0x2684\n").unwrap();
        let name = p.file_name().unwrap().to_owned();
        std::os::unix::fs::symlink(&p, root.join("bus/pci/devices").join(name)).unwrap();
        p
    }
    #[test]
    fn inventory_topology() {
        let root = std::env::temp_dir().join(format!("hyprv-egpu-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("bus/pci/devices")).unwrap();
        // iGPU on bus 0: never a candidate.
        mkdev(&root, "devices/pci0000:00/0000:00:02.0", "0x030000");
        // Root port -> TB bridge -> GPU (two functions).
        mkdev(&root, "devices/pci0000:00/0000:00:07.0", "0x060400");
        mkdev(
            &root,
            "devices/pci0000:00/0000:00:07.0/0000:05:00.0",
            "0x060400",
        );
        std::fs::write(
            root.join("devices/pci0000:00/0000:00:07.0/0000:05:00.0/label"),
            "Thunderbolt 4\n",
        )
        .unwrap();
        mkdev(
            &root,
            "devices/pci0000:00/0000:00:07.0/0000:05:00.0/0000:06:00.0",
            "0x030000",
        );
        mkdev(
            &root,
            "devices/pci0000:00/0000:00:07.0/0000:05:00.0/0000:06:00.1",
            "0x040300",
        );
        let inv = inventory(&root);
        assert_eq!(inv.len(), 1);
        let d = &inv[0];
        assert_eq!(d.bdf, "0000:06:00.0");
        assert!(d.externally_verified);
        assert_eq!(
            d.removal_chain,
            ["0000:06:00.1", "0000:06:00.0", "0000:05:00.0"]
        );
        assert_eq!(render(&[]), "status=not_found\n");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn bdf_validation() {
        assert!(is_bdf("0000:06:00.0"));
        assert!(!is_bdf("0000:06:00.0/../x"));
    }
}
