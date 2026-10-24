//! Installed, content-addressed runtime assets. No source-checkout fallback.
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

const CATALOG: &str = include_str!("runtime-assets.json");
#[derive(serde::Deserialize)]
struct Catalog {
    schema: u32,
    files: BTreeMap<String, String>,
}

fn resolve(
    explicit: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Result<PathBuf, String> {
    let root = explicit.map(PathBuf::from).or_else(|| home.map(|p| PathBuf::from(p).join(".local/share/hyprv-shell")))
        .ok_or("runtime assets: HOME and HYPRV_ASSET_ROOT are unset; run packaging/install-user.sh --apply")?;
    if !root.is_absolute() {
        return Err("runtime assets: HYPRV_ASSET_ROOT must be an absolute directory".into());
    }
    Ok(root)
}

/// Check the catalog embedded in this ELF against the installed catalog and every payload.
pub fn validate(root: &Path) -> Result<usize, String> {
    let check = |relative: &str, expected: &str| -> Result<(), String> {
        let path = root.join(relative);
        let info = std::fs::symlink_metadata(&path).map_err(|e| {
            format!(
                "runtime assets: {}: {e}; run packaging/install-user.sh --apply",
                path.display()
            )
        })?;
        if !info.is_file() {
            return Err(format!(
                "runtime assets: {} is not a regular file",
                path.display()
            ));
        }
        let bytes =
            std::fs::read(&path).map_err(|e| format!("runtime assets: {}: {e}", path.display()))?;
        if format!("{:x}", Sha256::digest(bytes)) != expected {
            return Err(format!(
                "runtime assets: digest mismatch: {}; reinstall the matching release",
                path.display()
            ));
        }
        Ok(())
    };
    check(
        "runtime-assets.json",
        &format!("{:x}", Sha256::digest(CATALOG.as_bytes())),
    )?;
    let catalog: Catalog = serde_json::from_str(CATALOG).map_err(|e| e.to_string())?;
    if catalog.schema != 1 || catalog.files.is_empty() {
        return Err("invalid embedded runtime asset catalog".into());
    }
    for (name, digest) in &catalog.files {
        check(name, digest)?;
    }
    Ok(catalog.files.len())
}

pub fn installed() -> Result<PathBuf, String> {
    let root = resolve(
        std::env::var_os("HYPRV_ASSET_ROOT"),
        std::env::var_os("HOME"),
    )?;
    validate(&root)?;
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_is_installed_data_and_relative_override_is_rejected() {
        assert_eq!(
            resolve(None, Some("/scratch/home".into())).unwrap(),
            PathBuf::from("/scratch/home/.local/share/hyprv-shell")
        );
        assert!(resolve(Some("relative".into()), Some("/home".into())).is_err());
        assert!(resolve(None, None).is_err());
    }
    #[test]
    fn catalog_requires_every_file_and_rejects_corruption() {
        let root =
            std::env::temp_dir().join(format!("hyprv-runtime-assets-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        {
            assert!(validate(&root).unwrap_err().contains("runtime-assets.json"));
            std::fs::write(root.join("runtime-assets.json"), CATALOG).unwrap();
            let catalog: Catalog = serde_json::from_str(CATALOG).unwrap();
            let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging/assets");
            for name in catalog.files.keys() {
                let dest = root.join(name);
                std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
                std::fs::copy(source.join(name), dest).unwrap();
            }
            assert_eq!(validate(&root).unwrap(), catalog.files.len());
            let name = catalog.files.keys().next().unwrap();
            std::fs::write(root.join(name), b"corrupt").unwrap();
            assert!(validate(&root).unwrap_err().contains("digest mismatch"));
            std::fs::remove_file(root.join(name)).unwrap();
            assert!(validate(&root).unwrap_err().contains(name));
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
