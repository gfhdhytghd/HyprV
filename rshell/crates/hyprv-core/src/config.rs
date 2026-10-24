use crate::store::Topic;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    path::{Path, PathBuf},
};
use tokio::io::unix::AsyncFd;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub theme_file: PathBuf,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            theme_file: "/tmp/qs_colors.json".into(),
        }
    }
}
impl Config {
    pub fn path() -> io::Result<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "HOME/XDG_CONFIG_HOME missing")
            })?;
        Ok(base.join("hyprv-shell/config.json"))
    }
    pub fn load(path: &Path) -> io::Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme(pub BTreeMap<String, String>);
impl Default for Theme {
    fn default() -> Self {
        // quickshell/hypr-comp/MatugenColors.qml:9-30
        let names = [
            "base", "mantle", "crust", "text", "subtext0", "subtext1", "surface0", "surface1",
            "surface2", "overlay0", "overlay1", "overlay2", "blue", "sapphire", "peach", "green",
            "red", "mauve", "pink", "yellow", "maroon", "teal",
        ];
        let colors = [
            "#1e1e2e", "#181825", "#11111b", "#cdd6f4", "#a6adc8", "#bac2de", "#313244", "#45475a",
            "#585b70", "#6c7086", "#7f849c", "#9399b2", "#89b4fa", "#74c7ec", "#fab387", "#a6e3a1",
            "#f38ba8", "#cba6f7", "#f5c2e7", "#f9e2af", "#eba0ac", "#94e2d5",
        ];
        Self(
            names
                .into_iter()
                .zip(colors)
                .map(|(n, c)| (n.into(), c.into()))
                .collect(),
        )
    }
}
impl Theme {
    /// Partial updates retain previous values, matching MatugenColors.qml:44-65.
    pub fn reload(&self, path: &Path) -> io::Result<Self> {
        // CORE-4: unknown keys of any JSON type are ignored before validation; known
        // colours must be valid strings or the whole update is rejected (atomic).
        let values: BTreeMap<String, serde_json::Value> =
            serde_json::from_slice(&std::fs::read(path)?).map_err(io::Error::other)?;
        let mut next = self.clone();
        for (key, value) in values {
            if let std::collections::btree_map::Entry::Occupied(mut entry) = next.0.entry(key) {
                let serde_json::Value::String(value) = value else {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "colour must be a string"));
                };
                if !matches!(value.len(), 7 | 9)
                    || !value.starts_with('#')
                    || !value[1..].bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "expected #RRGGBB or #AARRGGBB",
                    ));
                }
                entry.insert(value);
            }
        }
        Ok(next)
    }
}
/// Watches parents so atomic renames and a file created after startup both work.
/// CORE-3: events are decoded and filtered by watch descriptor and name, so activity on
/// unrelated siblings (e.g. other files in `/tmp`) never reports a change.
pub struct FileWatch {
    fd: AsyncFd<OwnedFd>,
    /// watch descriptor → names (next path component below the watched directory).
    targets: std::collections::HashMap<i32, Vec<std::ffi::OsString>>,
}
const DIR_EVENTS: u32 = libc::IN_CLOSE_WRITE
    | libc::IN_MOVED_TO
    | libc::IN_MOVED_FROM
    | libc::IN_CREATE
    | libc::IN_DELETE
    | libc::IN_DELETE_SELF
    | libc::IN_MOVE_SELF;
impl FileWatch {
    pub fn new(paths: &[PathBuf]) -> io::Result<Self> {
        let raw = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        let mut targets: std::collections::HashMap<i32, Vec<std::ffi::OsString>> = Default::default();
        for path in paths {
            let mut parent = path.parent().unwrap_or(Path::new("/"));
            while !parent.exists() {
                parent = parent.parent().unwrap_or(Path::new("/"));
            }
            // The relevant entry in `parent` is the first component of the remainder:
            // the file itself, or the missing ancestor directory that will contain it.
            let Some(name) = path
                .strip_prefix(parent)
                .ok()
                .and_then(|rest| rest.components().next())
                .map(|c| c.as_os_str().to_owned())
            else {
                continue;
            };
            let dir = std::ffi::CString::new(parent.as_os_str().as_encoded_bytes())
                .map_err(io::Error::other)?;
            let wd = unsafe { libc::inotify_add_watch(fd.as_raw_fd(), dir.as_ptr(), DIR_EVENTS) };
            if wd < 0 {
                return Err(io::Error::last_os_error());
            }
            targets.entry(wd).or_default().push(name);
        }
        Ok(Self {
            fd: unsafe { AsyncFd::register(fd) }.map_err(io::Error::other)?,
            targets,
        })
    }
    /// Is this decoded event about one of the watched paths?
    fn relevant(&self, wd: i32, mask: u32, name: &[u8]) -> bool {
        if mask & (libc::IN_Q_OVERFLOW | libc::IN_DELETE_SELF | libc::IN_MOVE_SELF | libc::IN_IGNORED) != 0 {
            // Overflow, or the watched directory itself went away: caller must rebuild.
            return mask & libc::IN_Q_OVERFLOW != 0 || self.targets.contains_key(&wd);
        }
        self.targets
            .get(&wd)
            .is_some_and(|names| names.iter().any(|n| n.as_encoded_bytes() == name))
    }
    pub async fn changed(&self) -> io::Result<()> {
        let mut buffer = [0u8; 16384];
        loop {
            let mut ready = self.fd.readable().await?;
            let read = ready.try_io(|fd| {
                let n = unsafe {
                    libc::read(fd.get_ref().as_raw_fd(), buffer.as_mut_ptr().cast(), buffer.len())
                };
                if n < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(n as usize)
                }
            });
            let n = match read {
                Ok(result) => result?,
                Err(_would_block) => continue,
            };
            let header = std::mem::size_of::<libc::inotify_event>();
            let mut offset = 0;
            let mut hit = false;
            while offset + header <= n {
                // SAFETY: the kernel writes complete inotify_event records.
                let event: libc::inotify_event =
                    unsafe { std::ptr::read_unaligned(buffer[offset..].as_ptr().cast()) };
                let name_start = offset + header;
                let name_end = (name_start + event.len as usize).min(n);
                let raw_name = &buffer[name_start..name_end];
                let name = &raw_name[..raw_name.iter().position(|b| *b == 0).unwrap_or(raw_name.len())];
                hit |= self.relevant(event.wd, event.mask, name);
                offset = name_end;
            }
            if hit {
                return Ok(());
            }
        }
    }
}
pub async fn watch_config(
    path: PathBuf,
    config: Topic<Config>,
    theme: Topic<Theme>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> io::Result<()> {
    loop {
        let current = config.snapshot().value;
        let watcher = FileWatch::new(&[path.clone(), current.theme_file.clone()])?;
        if let Ok(next) = Config::load(&path) {
            config.publish(next);
        }
        let updated = config.snapshot().value;
        if updated.theme_file != current.theme_file {
            continue;
        }
        let current = updated;
        if let Ok(next) = theme.snapshot().value.reload(&current.theme_file) {
            theme.publish(next);
        }
        // Recreate watches when the configured source or its parent changes.
        if *shutdown.borrow() {
            return Ok(());
        }
        tokio::select! {
            result = watcher.changed() => result?,
            _ = shutdown.changed() => return Ok(()),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_match_qml() {
        assert_eq!(Theme::default().0["base"], "#1e1e2e");
        assert_eq!(Theme::default().0.len(), 22);
    }
    #[tokio::test(flavor = "current_thread")]
    async fn watches_atomic_replacement_and_retains_partial_colors() {
        let dir = std::env::temp_dir().join(format!("hyprv-theme-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("colors.json");
        let watcher = FileWatch::new(std::slice::from_ref(&path)).unwrap();
        std::fs::write(dir.join("new"), r##"{"base":"#112233"}"##).unwrap();
        std::fs::rename(dir.join("new"), &path).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), watcher.changed())
            .await
            .unwrap()
            .unwrap();
        let theme = Theme::default().reload(&path).unwrap();
        assert_eq!(theme.0["base"], "#112233");
        assert_eq!(theme.0["text"], Theme::default().0["text"]);
        std::fs::write(&path, r##"{"base":"invalid"}"##).unwrap();
        assert!(theme.reload(&path).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn unknown_theme_metadata_is_ignored() {
        let dir = std::env::temp_dir().join(format!("hyprv-theme-meta-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("colors.json");
        std::fs::write(&path, r##"{"base":"#112233","metadata":{"generator":"test"},"n":3,"s":"x"}"##).unwrap();
        assert_eq!(Theme::default().reload(&path).unwrap().0["base"], "#112233");
        std::fs::write(&path, r##"{"base":5}"##).unwrap();
        assert!(Theme::default().reload(&path).is_err(), "known key with wrong type stays atomic");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test(flavor = "current_thread")]
    async fn watch_ignores_unrelated_sibling_but_sees_target_and_missing_parent() {
        let dir = std::env::temp_dir().join(format!("hyprv-watch-sib-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let watcher = FileWatch::new(std::slice::from_ref(&path)).unwrap();
        std::fs::write(dir.join("unrelated.log"), "noise").unwrap();
        let quiet = tokio::time::timeout(std::time::Duration::from_millis(200), watcher.changed()).await;
        assert!(quiet.is_err(), "sibling activity must not count as a change");
        std::fs::write(&path, "{}").unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), watcher.changed()).await.unwrap().unwrap();
        // Missing parent: watch the nearest ancestor for the missing directory's creation.
        let nested = dir.join("later/sub/colors.json");
        let watcher = FileWatch::new(std::slice::from_ref(&nested)).unwrap();
        std::fs::write(dir.join("other.txt"), "noise").unwrap();
        assert!(tokio::time::timeout(std::time::Duration::from_millis(200), watcher.changed()).await.is_err());
        std::fs::create_dir(dir.join("later")).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), watcher.changed()).await.unwrap().unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
