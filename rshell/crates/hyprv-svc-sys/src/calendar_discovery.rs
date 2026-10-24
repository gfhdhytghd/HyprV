//! dcal's bytewise filename order, with owned live-server and peer validation.
//! `.pid` describes the UI child; `.session` and `dankcal.path` are not IPC endpoints.
use crate::support::{Error, Result};
use std::{
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::{Path, PathBuf},
    time::Duration,
};

pub(super) fn runtime_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    if unsafe { libc::geteuid() } == 0 {
        PathBuf::from("/run/dankcal")
    } else {
        std::env::temp_dir()
    }
}

fn socket_pid(name: &std::ffi::OsStr) -> Option<i32> {
    let text = name
        .to_str()?
        .strip_prefix("dankcal-")?
        .strip_suffix(".sock")?;
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<i32>().ok().filter(|pid| *pid > 0)
}

fn candidates(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = std::fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| socket_pid(&entry.file_name()).is_some())
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    paths.sort(); // os.ReadDir in dcal is lexicographic, not newest/highest PID.
    Ok(paths)
}

fn owned_live_process(pid: i32, uid: u32) -> bool {
    let proc = PathBuf::from(format!("/proc/{pid}"));
    if !std::fs::metadata(&proc).is_ok_and(|m| m.uid() == uid) {
        return false;
    }
    if unsafe { libc::kill(pid, 0) } != 0 {
        return false;
    }
    std::fs::read_to_string(proc.join("stat"))
        .ok()
        .and_then(|text| {
            text.rsplit_once(')')
                .map(|(_, rest)| rest.trim_start().starts_with(['Z', 'X']))
        })
        == Some(false)
}

pub(super) fn identity(path: &Path) -> Result<(u64, u64)> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_socket() {
        return Err(Error::new("discovery", "calendar endpoint is not a socket"));
    }
    Ok((metadata.dev(), metadata.ino()))
}

pub(super) async fn find(dir: &Path) -> Result<PathBuf> {
    find_for_uid(dir, unsafe { libc::geteuid() }).await
}

async fn find_for_uid(dir: &Path, uid: u32) -> Result<PathBuf> {
    for path in candidates(dir)? {
        let pid = socket_pid(path.file_name().unwrap()).unwrap();
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if !metadata.file_type().is_socket()
            || metadata.uid() != uid
            || !owned_live_process(pid, uid)
        {
            continue;
        }
        let before = (metadata.dev(), metadata.ino());
        let Ok(Ok(stream)) = tokio::time::timeout(
            Duration::from_millis(500),
            tokio::net::UnixStream::connect(&path),
        )
        .await
        else {
            continue;
        };
        let Ok(peer) = stream.peer_cred() else {
            continue;
        };
        // A listening inode is insufficient: reject another user's socket, a
        // stale/reused PID, or a same-user endpoint misnamed for a different process.
        if peer.uid() == uid
            && peer.pid() == Some(pid)
            && owned_live_process(pid, uid)
            && identity(&path).ok() == Some(before)
        {
            return Ok(path);
        }
    }
    Err(Error::new(
        "discovery",
        "no live user-owned dcal socket in runtime directory",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::{FileWatch, TestDir};
    #[tokio::test]
    async fn ignores_stale_session_pid_path_files_and_dead_or_nonlistening_sockets() {
        let dir = TestDir::new("calendar-discovery");
        let root = dir.socket_dir();
        for n in 1..200 {
            std::fs::write(root.join(format!("dankcal-{n}.session")), b"wayland-1").unwrap();
        }
        std::fs::write(root.join("dankcal.path"), b"/not/an/ipc/path").unwrap();
        std::fs::write(root.join("dankcal-1.pid"), b"1").unwrap();
        let dead = root.join("dankcal-2147483647.sock");
        let _dead = tokio::net::UnixListener::bind(&dead).unwrap();
        assert!(
            find(&root).await.is_err(),
            "listening but dead/mismatched filename PID must not win"
        );
        let real = root.join(format!("dankcal-{}.sock", std::process::id()));
        {
            let _stale = tokio::net::UnixListener::bind(&real).unwrap();
        }
        assert!(
            find(&root).await.is_err(),
            "stale socket inode must be ignored"
        );
        std::fs::remove_file(&real).unwrap();
        let _server = tokio::net::UnixListener::bind(&real).unwrap();
        assert_eq!(find(&root).await.unwrap(), real);
        assert!(find_for_uid(&root, unsafe { libc::geteuid() } + 1)
            .await
            .is_err());
    }
    #[test]
    fn selection_is_lexicographic_and_never_uses_pid_or_session_markers() {
        let dir = TestDir::new("calendar-order");
        for name in [
            "dankcal-20.sock",
            "dankcal-100.sock",
            "dankcal-1.pid",
            "dankcal-1.session",
            "dankcal.path",
            "dankcal-x.sock",
            "dankcal-0.sock",
        ] {
            std::fs::write(dir.0.join(name), b"").unwrap();
        }
        assert_eq!(
            candidates(&dir.0)
                .unwrap()
                .iter()
                .map(|p| p.file_name().unwrap().to_str().unwrap())
                .collect::<Vec<_>>(),
            ["dankcal-100.sock", "dankcal-20.sock"]
        );
    }
    #[tokio::test]
    async fn directory_watch_sleeps_through_unrelated_files_and_wakes_for_socket_replacement() {
        let dir = TestDir::new("calendar-watch");
        let root = dir.socket_dir();
        let mut watch = FileWatch::directory(root.clone(), "dankcal-", ".sock").unwrap();
        std::fs::write(root.join("dankcal-1.session"), b"wayland-1").unwrap();
        std::fs::write(root.join("other-service.sock"), b"unrelated").unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(50), watch.changed())
                .await
                .is_err()
        );
        let path = root.join(format!("dankcal-{}.sock", std::process::id()));
        let first = tokio::net::UnixListener::bind(&path).unwrap();
        tokio::time::timeout(Duration::from_secs(1), watch.changed())
            .await
            .unwrap()
            .unwrap();
        drop(first);
        std::fs::remove_file(&path).unwrap();
        let _second = tokio::net::UnixListener::bind(&path).unwrap();
        tokio::time::timeout(Duration::from_secs(1), watch.changed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(find(&root).await.unwrap(), path);
    }
    #[tokio::test]
    async fn socket_symlinks_are_not_discovered() {
        let dir = TestDir::new("calendar-symlink");
        let root = dir.socket_dir();
        let target = root.join("actual.sock");
        let _listener = tokio::net::UnixListener::bind(&target).unwrap();
        std::os::unix::fs::symlink(
            target,
            root.join(format!("dankcal-{}.sock", std::process::id())),
        )
        .unwrap();
        assert!(find(&root).await.is_err());
    }
}
