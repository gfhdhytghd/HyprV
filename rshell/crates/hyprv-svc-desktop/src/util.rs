//! Small shared helpers: atomic files, /proc parsing, pidfds, flock, notifications.
use std::{
    collections::HashMap,
    ffi::CString,
    io,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::{ffi::OsStrExt, fs::PermissionsExt},
    },
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::io::unix::AsyncFd;

pub fn expand_tilde(path: &Path, home: &Path) -> PathBuf {
    match path.strip_prefix("~") {
        Ok(rest) => home.join(rest),
        Err(_) => path.to_path_buf(),
    }
}

pub fn read_string(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}
pub fn read_trim(path: &Path) -> Option<String> {
    read_string(path).map(|s| s.trim().to_string())
}

/// What the target must still contain at commit time (compare-and-swap).
#[derive(Clone, Copy, Debug)]
pub enum Expect<'a> {
    /// Unconditional replace.
    Any,
    /// The target must not exist.
    Absent,
    /// The target must still hold exactly these bytes.
    Bytes(&'a [u8]),
}
/// Error payload for a lost compare-and-swap.
#[derive(Debug)]
pub struct Conflict(pub PathBuf);
impl std::fmt::Display for Conflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} changed concurrently; not overwritten",
            self.0.display()
        )
    }
}
impl std::error::Error for Conflict {}
pub fn is_conflict(e: &io::Error) -> bool {
    e.get_ref().is_some_and(|i| i.is::<Conflict>())
}
fn cstr(p: &std::ffi::OsStr) -> io::Result<CString> {
    CString::new(p.as_bytes()).map_err(io::Error::other)
}
fn cvt(r: libc::c_int) -> io::Result<libc::c_int> {
    if r < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(r)
    }
}
fn random_hex() -> String {
    let mut b = [0u8; 8];
    unsafe { libc::getrandom(b.as_mut_ptr().cast(), b.len(), 0) };
    b.iter().map(|x| format!("{x:02x}")).collect()
}
/// Create a new file in `dir` exclusively (`O_CREAT|O_EXCL|O_NOFOLLOW`) under a
/// random name; never opens or truncates an existing entry or symlink.
fn create_exclusive(
    dir: &OwnedFd,
    prefix: &str,
    mode: u32,
) -> io::Result<(std::fs::File, CString)> {
    for _ in 0..32 {
        let name = cstr(std::ffi::OsStr::new(&format!("{prefix}{}", random_hex())))?;
        let fd = unsafe {
            libc::openat(
                dir.as_raw_fd(),
                name.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                mode as libc::c_uint,
            )
        };
        if fd >= 0 {
            return Ok((unsafe { std::fs::File::from_raw_fd(fd) }, name));
        }
        let e = io::Error::last_os_error();
        if e.kind() != io::ErrorKind::AlreadyExists {
            return Err(e);
        }
    }
    Err(io::Error::other("could not create a unique temporary file"))
}
fn open_dir(dir: &Path) -> io::Result<OwnedFd> {
    let c = cstr(dir.as_os_str())?;
    let fd = cvt(unsafe {
        libc::open(
            c.as_ptr(),
            libc::O_DIRECTORY | libc::O_RDONLY | libc::O_CLOEXEC,
        )
    })?;
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}
fn split(path: &Path) -> io::Result<(PathBuf, std::ffi::OsString)> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("path has no file name"))?
        .to_os_string();
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    Ok((parent, name))
}
/// Mode of an existing regular target (symlinks are not followed), else 0600.
fn inherited_mode(path: &Path) -> u32 {
    std::fs::symlink_metadata(path)
        .ok()
        .filter(|m| m.file_type().is_file())
        .map(|m| m.permissions().mode() & 0o7777)
        .unwrap_or(0o600)
}
fn current_matches(path: &Path, expect: Expect<'_>) -> io::Result<bool> {
    match expect {
        Expect::Any => Ok(true),
        Expect::Absent => Ok(std::fs::symlink_metadata(path).is_err()),
        Expect::Bytes(b) => match std::fs::read(path) {
            Ok(now) => Ok(now == b),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        },
    }
}
/// Durable atomic replace: exclusive sibling temp (mode set before any data),
/// fsync, then a compare-and-swap that cannot lose a concurrent writer's
/// version. `mode = None` keeps the existing file's permissions (0600 for new
/// files).
///
/// - `Expect::Bytes`: `renameat2(RENAME_EXCHANGE)` swaps the new file in and
///   the displaced one out under the temp name, then the displaced bytes are
///   compared with what the caller read. If another writer got in between the
///   caller's read and the swap, the exchange is reversed (their newer content
///   is the target again) and [`Conflict`] is returned so the caller rebases.
///   A generation that is neither ours nor the expected one is never deleted:
///   it is kept as `<name>.hyprv-conflict.<ms>[.<n>]`.
/// - `Expect::Absent`: `RENAME_NOREPLACE`, so a file created meanwhile wins.
/// - `Expect::Any`: plain rename.
///
/// Filesystems without `renameat2` flags fall back to check-then-rename.
pub fn replace_checked(
    path: &Path,
    data: &[u8],
    mode: Option<u32>,
    expect: Expect<'_>,
) -> io::Result<()> {
    use std::io::Write;
    let (parent, name) = split(path)?;
    std::fs::create_dir_all(&parent)?;
    let dir = open_dir(&parent)?;
    let mode = mode.unwrap_or_else(|| inherited_mode(path));
    let prefix = format!(".{}.", name.to_string_lossy());
    let (mut file, temp) = create_exclusive(&dir, &prefix, 0o600)?;
    let target = cstr(&name)?;
    // Whether the temp name still holds our own bytes (only then may it go).
    let mut ours_at_temp = true;
    let result = (|| -> io::Result<()> {
        cvt(unsafe { libc::fchmod(file.as_raw_fd(), mode as libc::mode_t) })?;
        file.write_all(data)?;
        file.sync_all()?;
        if !current_matches(path, expect)? {
            return Err(io::Error::other(Conflict(path.to_path_buf())));
        }
        let swap = |flags: libc::c_uint| renameat2(&dir, &temp, &target, flags);
        match expect {
            Expect::Any => swap(0)?,
            Expect::Absent => match swap(libc::RENAME_NOREPLACE) {
                Err(e) if e.raw_os_error() == Some(libc::EEXIST) => {
                    return Err(io::Error::other(Conflict(path.to_path_buf())))
                }
                Err(e) if unsupported(&e) => swap(0)?,
                r => r?,
            },
            Expect::Bytes(expected) => {
                match swap(libc::RENAME_EXCHANGE) {
                    Err(e) if e.raw_os_error() == Some(libc::ENOENT) => {
                        return Err(io::Error::other(Conflict(path.to_path_buf())))
                    }
                    Err(e) if unsupported(&e) => swap(0)?,
                    Err(e) => return Err(e),
                    Ok(()) => {
                        ours_at_temp = false;
                        let displaced = read_at(&dir, &temp)?;
                        if displaced == expected {
                            // Exactly the generation the caller read (and
                            // backed up): nothing is lost by dropping it.
                            unsafe { libc::unlinkat(dir.as_raw_fd(), temp.as_ptr(), 0) };
                        } else {
                            // A newer foreign version: give it back.
                            swap(libc::RENAME_EXCHANGE)?;
                            if read_at(&dir, &temp)? == data {
                                ours_at_temp = true;
                            } else {
                                // Yet another writer replaced ours meanwhile;
                                // keep that generation instead of deleting it.
                                keep_conflict(&dir, &temp, &name)?;
                            }
                            return Err(io::Error::other(Conflict(path.to_path_buf())));
                        }
                    }
                }
            }
        }
        let _ = unsafe { libc::fsync(dir.as_raw_fd()) };
        Ok(())
    })();
    if result.is_err() && ours_at_temp {
        // Only ever remove the entry this writer created, holding its bytes.
        unsafe { libc::unlinkat(dir.as_raw_fd(), temp.as_ptr(), 0) };
    }
    result
}
fn renameat2(dir: &OwnedFd, from: &CString, to: &CString, flags: libc::c_uint) -> io::Result<()> {
    // The libc wrapper (not a raw syscall), so tests can interpose it.
    cvt(unsafe {
        libc::renameat2(
            dir.as_raw_fd(),
            from.as_ptr(),
            dir.as_raw_fd(),
            to.as_ptr(),
            flags,
        )
    })
    .map(|_| ())
}
fn unsupported(e: &io::Error) -> bool {
    matches!(
        e.raw_os_error(),
        Some(libc::EINVAL | libc::ENOSYS | libc::ENOTSUP)
    )
}
fn read_at(dir: &OwnedFd, name: &CString) -> io::Result<Vec<u8>> {
    use std::io::Read;
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    let mut file = std::fs::File::from(unsafe { OwnedFd::from_raw_fd(cvt(fd)?) });
    let mut out = Vec::new();
    file.read_to_end(&mut out)?;
    Ok(out)
}
/// Moves an unexpected displaced generation to `<name>.hyprv-conflict.<ms>`.
fn keep_conflict(dir: &OwnedFd, temp: &CString, name: &std::ffi::OsStr) -> io::Result<()> {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    for n in 0..1000 {
        let suffix = if n == 0 {
            String::new()
        } else {
            format!(".{n}")
        };
        let kept = cstr(std::ffi::OsStr::new(&format!(
            "{}.hyprv-conflict.{ms}{suffix}",
            name.to_string_lossy()
        )))?;
        match renameat2(dir, temp, &kept, libc::RENAME_NOREPLACE) {
            Err(e) if e.raw_os_error() == Some(libc::EEXIST) => continue,
            Err(e) if unsupported(&e) => return renameat2(dir, temp, &kept, 0),
            r => return r,
        }
    }
    Err(io::Error::other("no free conflict name"))
}
/// Unconditional durable atomic replace (see [`replace_checked`]).
pub fn atomic_write(path: &Path, data: &[u8], mode: Option<u32>) -> io::Result<()> {
    replace_checked(path, data, mode, Expect::Any)
}
/// Write a versioned backup `NAME.hyprv-bak.<epoch-ms>` next to `path`,
/// created exclusively (never follows or overwrites an existing entry).
pub fn versioned_backup(path: &Path, data: &[u8], mode: u32) -> io::Result<PathBuf> {
    use std::io::Write;
    let (parent, name) = split(path)?;
    let dir = open_dir(&parent)?;
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let prefix = format!("{}.hyprv-bak.{ms}.", name.to_string_lossy());
    let (mut file, created) = create_exclusive(&dir, &prefix, 0o600)?;
    cvt(unsafe { libc::fchmod(file.as_raw_fd(), mode as libc::mode_t) })?;
    file.write_all(data)?;
    file.sync_all()?;
    let _ = unsafe { libc::fsync(dir.as_raw_fd()) };
    Ok(parent.join(std::ffi::OsStr::from_bytes(created.as_bytes())))
}

/// Atomically replace `link` with a symlink to `target` (`ln -sfn`).
pub fn atomic_symlink(target: &Path, link: &Path) -> io::Result<()> {
    let (parent, name) = split(link)?;
    std::fs::create_dir_all(&parent)?;
    let dir = open_dir(&parent)?;
    let t = cstr(target.as_os_str())?;
    for _ in 0..32 {
        let temp = cstr(std::ffi::OsStr::new(&format!(
            ".{}.{}",
            name.to_string_lossy(),
            random_hex()
        )))?;
        // symlinkat never follows or replaces an existing entry (EEXIST).
        if unsafe { libc::symlinkat(t.as_ptr(), dir.as_raw_fd(), temp.as_ptr()) } < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::AlreadyExists {
                continue;
            }
            return Err(e);
        }
        let final_name = cstr(&name)?;
        let r = cvt(unsafe {
            libc::renameat(
                dir.as_raw_fd(),
                temp.as_ptr(),
                dir.as_raw_fd(),
                final_name.as_ptr(),
            )
        });
        if r.is_err() {
            unsafe { libc::unlinkat(dir.as_raw_fd(), temp.as_ptr(), 0) };
        }
        return r.map(|_| ());
    }
    Err(io::Error::other("could not create a unique temporary link"))
}

// ---------------------------------------------------------------- /proc

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcStat {
    pub comm: String,
    pub ppid: i32,
    pub start_ticks: u64,
}
pub(crate) fn stat_fields(text: &str) -> Option<(&str, i32, u64)> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    let mut fields = text.get(close + 2..)?.split_whitespace();
    fields.next()?;
    let ppid = fields.next()?.parse().ok()?;
    let start = fields.nth(17)?.parse().ok()?;
    Some((text.get(open + 1..close)?, ppid, start))
}
/// The task is running or in uninterruptible sleep (`R`/`D` in /proc/PID/stat).
pub(crate) fn stat_busy(text: &str) -> bool {
    text.rfind(')')
        .and_then(|close| text.as_bytes().get(close + 2))
        .is_some_and(|s| matches!(s, b'R' | b'D'))
}
pub fn parse_stat(text: &str) -> Option<ProcStat> {
    let (comm, ppid, start_ticks) = stat_fields(text)?;
    Some(ProcStat {
        comm: comm.to_owned(),
        ppid,
        start_ticks,
    })
}
pub struct Proc {
    pub root: PathBuf,
}
impl Default for Proc {
    fn default() -> Self {
        Self {
            root: "/proc".into(),
        }
    }
}
impl Proc {
    pub fn pids(&self) -> Vec<i32> {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        entries
            .filter_map(|e| e.ok()?.file_name().to_str()?.parse().ok())
            .collect()
    }
    pub fn stat(&self, pid: i32) -> Option<ProcStat> {
        parse_stat(&std::fs::read_to_string(self.root.join(format!("{pid}/stat"))).ok()?)
    }
    pub fn cmdline(&self, pid: i32) -> Vec<String> {
        std::fs::read(self.root.join(format!("{pid}/cmdline")))
            .map(|raw| {
                raw.split(|b| *b == 0)
                    .filter(|p| !p.is_empty())
                    .map(|p| String::from_utf8_lossy(p).into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn exe(&self, pid: i32) -> Option<PathBuf> {
        let link = std::fs::read_link(self.root.join(format!("{pid}/exe"))).ok()?;
        let text = link.to_string_lossy();
        Some(PathBuf::from(
            text.strip_suffix(" (deleted)").unwrap_or(&text),
        ))
    }
    pub fn cwd(&self, pid: i32) -> Option<String> {
        std::fs::read_link(self.root.join(format!("{pid}/cwd")))
            .ok()
            .map(|p| p.display().to_string())
    }
    pub fn exists(&self, pid: i32) -> bool {
        pid > 0 && self.root.join(pid.to_string()).exists()
    }
    /// Last PID (or TID) allocated in this PID namespace, from `loadavg`.
    pub fn last_pid(&self) -> Option<i32> {
        std::fs::read_to_string(self.root.join("loadavg"))
            .ok()?
            .split_whitespace()
            .nth(4)?
            .parse()
            .ok()
    }
    pub fn pid_max(&self) -> Option<i32> {
        std::fs::read_to_string(self.root.join("sys/kernel/pid_max"))
            .ok()?
            .trim()
            .parse()
            .ok()
    }
    /// Thread-group id: `/proc/<tid>` also resolves threads, readdir does not.
    pub fn tgid(&self, pid: i32) -> Option<i32> {
        let status = std::fs::read_to_string(self.root.join(format!("{pid}/status"))).ok()?;
        status
            .lines()
            .find_map(|l| l.strip_prefix("Tgid:"))?
            .trim()
            .parse()
            .ok()
    }
    /// One bounded scan returning stat plus argv for processes named in `argv_for`.
    pub fn scan(&self, argv_for: &[&str]) -> HashMap<i32, (ProcStat, Vec<String>)> {
        let mut out = HashMap::new();
        for pid in self.pids() {
            let Some(stat) = self.stat(pid) else { continue };
            let argv = if argv_for.contains(&stat.comm.as_str()) {
                self.cmdline(pid)
            } else {
                Vec::new()
            };
            out.insert(pid, (stat, argv));
        }
        out
    }
}
pub fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

// ---------------------------------------------------------------- pidfd

/// Process handle that becomes readable when the process exits.
pub struct Pidfd {
    fd: AsyncFd<OwnedFd>,
    pub pid: i32,
}
impl Pidfd {
    pub fn open(pid: i32) -> io::Result<Self> {
        let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, libc::PIDFD_NONBLOCK) };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw as i32) };
        Ok(Self {
            fd: unsafe { AsyncFd::register(fd) }?,
            pid,
        })
    }
    pub async fn exited(&self) {
        let _ = self.fd.readable().await.map(|mut g| g.retain_ready());
    }
    pub fn signal(&self, signal: i32) -> io::Result<()> {
        let r = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                self.fd.get_ref().as_raw_fd(),
                signal,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        };
        if r < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

/// A pidfd without reactor registration, for batch polling with [`exited`].
pub fn pidfd(pid: i32) -> io::Result<OwnedFd> {
    let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, libc::PIDFD_NONBLOCK) };
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { OwnedFd::from_raw_fd(raw as i32) })
}
/// Exec notifications for a few executables: unprivileged fanotify
/// (`FAN_REPORT_FID`, inode marks) with `FAN_OPEN_EXEC`, which unlike an inotify
/// `IN_OPEN` does not fire when a file is merely read. Unprivileged listeners
/// get no PID, so this only says "one of the marked binaries was executed".
pub struct ExecWatch {
    fd: OwnedFd,
    marked: std::collections::HashSet<PathBuf>,
}
impl ExecWatch {
    pub fn new() -> io::Result<Self> {
        let flags =
            libc::FAN_CLASS_NOTIF | libc::FAN_CLOEXEC | libc::FAN_NONBLOCK | libc::FAN_REPORT_FID;
        let raw = unsafe { libc::fanotify_init(flags, (libc::O_RDONLY | libc::O_CLOEXEC) as u32) };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            fd: unsafe { OwnedFd::from_raw_fd(raw) },
            marked: Default::default(),
        })
    }
    /// Marks `path` (symlinks followed) once; a replaced binary is a new inode,
    /// so callers re-mark resolved paths periodically.
    pub fn mark(&mut self, path: &Path) -> bool {
        if self.marked.contains(path) {
            return true;
        }
        let Ok(c) = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()) else {
            return false;
        };
        let r = unsafe {
            libc::fanotify_mark(
                self.fd.as_raw_fd(),
                libc::FAN_MARK_ADD,
                libc::FAN_OPEN_EXEC,
                libc::AT_FDCWD,
                c.as_ptr(),
            )
        };
        if r == 0 {
            self.marked.insert(path.to_path_buf());
        }
        r == 0
    }
    /// Forgets marked paths so the next [`ExecWatch::mark`] re-resolves them.
    pub fn remark(&mut self) {
        self.marked.clear();
    }
    /// Drains pending events without blocking; true if any arrived.
    pub fn drain(&self) -> bool {
        let mut buf = [0u8; 4096];
        let mut any = false;
        loop {
            let n = unsafe { libc::read(self.fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
            if n <= 0 {
                return any;
            }
            any = true;
        }
    }
}
/// For waiting on events through `tokio::io::unix::AsyncFd` instead of polling.
impl AsRawFd for ExecWatch {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.fd.as_raw_fd()
    }
}
/// Indices of `fds` whose process has exited: one non-waiting poll(2).
pub fn exited(fds: &[&OwnedFd]) -> Vec<usize> {
    let mut polls: Vec<libc::pollfd> = fds
        .iter()
        .map(|fd| libc::pollfd {
            fd: fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        })
        .collect();
    if polls.is_empty() || unsafe { libc::poll(polls.as_mut_ptr(), polls.len() as _, 0) } <= 0 {
        return Vec::new();
    }
    polls
        .iter()
        .enumerate()
        .filter(|(_, p)| p.revents != 0)
        .map(|(i, _)| i)
        .collect()
}

// ---------------------------------------------------------------- flock

/// Exclusive advisory lock (`flock LOCK_EX`), released on drop.
pub struct FileLock(std::fs::File);
impl FileLock {
    pub fn exclusive_blocking(path: &Path) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Read-only when present: closing never emits IN_CLOSE_WRITE, so
        // watchers of the directory are not woken by lock traffic.
        let file = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(_) => std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .read(true)
                .open(path)?,
        };
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(file))
    }
    pub async fn exclusive(path: PathBuf) -> io::Result<Self> {
        tokio::task::spawn_blocking(move || Self::exclusive_blocking(&path))
            .await
            .map_err(io::Error::other)?
    }
}
impl Drop for FileLock {
    fn drop(&mut self) {
        unsafe { libc::flock(self.0.as_raw_fd(), libc::LOCK_UN) };
    }
}

// ---------------------------------------------------------------- inotify

/// Name-filtered directory watch. Each spec is `(dir, names)`; an empty name
/// list accepts every entry. Missing directories are watched through their
/// nearest existing ancestor (filtered to the missing child), so creation is
/// seen; callers recreate the watch after each wake (subscribe, then snapshot).
pub struct Watch {
    ino: Inotify,
    specs: Vec<(PathBuf, Vec<String>)>,
    filters: std::sync::Mutex<HashMap<i32, Option<Vec<std::ffi::OsString>>>>,
}
const WATCH_MASK: u32 = libc::IN_CLOSE_WRITE
    | libc::IN_MOVED_TO
    | libc::IN_MOVED_FROM
    | libc::IN_CREATE
    | libc::IN_DELETE
    | libc::IN_DELETE_SELF
    | libc::IN_MOVE_SELF;
// IN_DONT_FOLLOW: a watched symlink is replaced, not its target.
const FILE_MASK: u32 = libc::IN_CLOSE_WRITE
    | libc::IN_ATTRIB
    | libc::IN_DELETE_SELF
    | libc::IN_MOVE_SELF
    | libc::IN_DONT_FOLLOW;
impl Watch {
    /// The watch is long-lived: after an atomic replace (the watched inode goes
    /// away) or the creation of a missing name, it re-arms itself on the next
    /// event, so callers never rebuild it or rescan directories per event.
    pub fn new(specs: &[(PathBuf, &[&str])]) -> io::Result<Self> {
        let watch = Self {
            ino: Inotify::new()?,
            specs: specs
                .iter()
                .map(|(d, n)| (d.clone(), n.iter().map(|s| s.to_string()).collect()))
                .collect(),
            filters: std::sync::Mutex::new(HashMap::new()),
        };
        watch.arm()?;
        Ok(watch)
    }
    fn arm(&self) -> io::Result<()> {
        let mut filters = self.filters.lock().unwrap();
        let mut add =
            |path: &Path, mask: u32, names: Option<Vec<std::ffi::OsString>>| -> io::Result<()> {
                let wd = self.ino.add_wd(path, mask)?;
                // Re-adding an inode returns its existing wd; rebuild its filter.
                let entry = filters.entry(wd).or_insert(Some(Vec::new()));
                match (entry.as_mut(), names) {
                    (Some(list), Some(n)) => {
                        for name in n {
                            if !list.contains(&name) {
                                list.push(name);
                            }
                        }
                    }
                    (_, None) => *entry = None,
                    (None, Some(_)) => {}
                }
                Ok(())
            };
        for (dir, names) in &self.specs {
            // Existing named files are watched directly, so unrelated traffic
            // in a busy directory (e.g. a 1 Hz lock file) never wakes us.
            let mut missing: Vec<&str> = Vec::new();
            if dir.is_dir() {
                for name in names {
                    let file = dir.join(name);
                    if std::fs::symlink_metadata(&file).is_ok() {
                        add(&file, FILE_MASK, None)?;
                    } else {
                        missing.push(name);
                    }
                }
                if !names.is_empty() && missing.is_empty() {
                    continue;
                }
            }
            let mut target = dir.clone();
            let mut filter: Option<Vec<std::ffi::OsString>> = if names.is_empty() {
                None
            } else if dir.is_dir() {
                Some(missing.iter().map(Into::into).collect())
            } else {
                Some(names.iter().map(Into::into).collect())
            };
            while !target.is_dir() {
                let child = target.file_name().map(|n| n.to_os_string());
                if !target.pop() {
                    break;
                }
                filter = child.map(|c| vec![c]);
            }
            add(&target, WATCH_MASK, filter)?;
        }
        Ok(())
    }
    /// Resolve when an event matches a filter (overflow/self events always do).
    pub async fn changed(&self) -> io::Result<()> {
        loop {
            let mut hit = false;
            let mut rearm = false;
            for (wd, mask, name) in self.ino.events().await? {
                if mask & libc::IN_IGNORED != 0 {
                    self.filters.lock().unwrap().remove(&wd);
                    rearm = true;
                    continue;
                }
                if mask & (libc::IN_Q_OVERFLOW | libc::IN_DELETE_SELF | libc::IN_MOVE_SELF) != 0 {
                    hit = true;
                    rearm = true;
                    continue;
                }
                let matched = match self.filters.lock().unwrap().get(&wd) {
                    Some(None) => true,
                    Some(Some(names)) => names.contains(&name),
                    None => false,
                };
                if matched {
                    hit = true;
                    // A watched name appeared in its directory: switch to a file watch.
                    rearm |= mask & (libc::IN_CREATE | libc::IN_MOVED_TO) != 0;
                }
            }
            if rearm {
                let _ = self.arm();
            }
            if hit {
                return Ok(());
            }
        }
    }
}
pub fn dir_watch(dirs: &[PathBuf]) -> io::Result<Watch> {
    let specs: Vec<(PathBuf, &[&str])> = dirs.iter().map(|d| (d.clone(), &[][..])).collect();
    Watch::new(&specs)
}
/// Coalesce a burst of events: wait for the first, then absorb more for `quiet`.
pub async fn settle(watch: &Watch, quiet: Duration) -> io::Result<()> {
    watch.changed().await?;
    while tokio::time::timeout(quiet, watch.changed()).await.is_ok() {}
    Ok(())
}

// ---------------------------------------------------------------- time

/// `datetime.now(timezone.utc).astimezone().isoformat()`: local time, µs, ±HH:MM.
pub fn local_iso_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&secs, &mut tm) };
    let off = tm.tm_gmtoff;
    let sign = if off < 0 { '-' } else { '+' };
    let off = off.abs();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:06}{}{:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec,
        now.subsec_micros(),
        sign,
        off / 3600,
        (off % 3600) / 60
    )
}
pub fn epoch_f64() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

// ---------------------------------------------------------------- D-Bus helpers

/// `notify-send [-u low] [-h string:x-canonical-private-synchronous:TAG] SUMMARY BODY`.
pub async fn notify(summary: &str, body: &str, low: bool, sync_tag: Option<&str>) {
    let Ok(conn) = zbus::Connection::session().await else {
        return;
    };
    let mut hints: HashMap<&str, zvariant::Value<'_>> = HashMap::new();
    if low {
        hints.insert("urgency", zvariant::Value::U8(0));
    }
    if let Some(tag) = sync_tag {
        hints.insert(
            "x-canonical-private-synchronous",
            zvariant::Value::from(tag),
        );
    }
    let _ = conn
        .call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                "hyprv-shell",
                0u32,
                "",
                summary,
                body,
                Vec::<&str>::new(),
                hints,
                -1i32,
            ),
        )
        .await;
}

/// `command -v NAME` without spawning a shell.
pub fn which(name: &str, path_var: Option<&str>) -> Option<PathBuf> {
    let path = path_var.unwrap_or("/usr/local/bin:/usr/bin:/bin");
    for dir in path.split(':').filter(|d| !d.is_empty()) {
        let candidate = Path::new(dir).join(name);
        let Ok(c) = CString::new(candidate.as_os_str().as_bytes()) else {
            continue;
        };
        if candidate.is_file() && unsafe { libc::access(c.as_ptr(), libc::X_OK) } == 0 {
            return Some(candidate);
        }
    }
    None
}

/// Stable FNV-1a; replaces Python's per-process randomized `hash()`.
pub fn fnv1a(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Direct inotify watch on specific paths with a caller-chosen mask
/// (e.g. `IN_MODIFY` on an append-only log, which parent-dir watches miss).
pub struct Inotify(AsyncFd<OwnedFd>);
impl Inotify {
    pub fn new() -> io::Result<Self> {
        let raw = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        Ok(Self(unsafe { AsyncFd::register(fd) }?))
    }
    pub fn add(&self, path: &Path, mask: u32) -> io::Result<()> {
        let c = CString::new(path.as_os_str().as_bytes()).map_err(io::Error::other)?;
        if unsafe { libc::inotify_add_watch(self.0.get_ref().as_raw_fd(), c.as_ptr(), mask) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    pub fn add_wd(&self, path: &Path, mask: u32) -> io::Result<i32> {
        let c = CString::new(path.as_os_str().as_bytes()).map_err(io::Error::other)?;
        let wd = unsafe { libc::inotify_add_watch(self.0.get_ref().as_raw_fd(), c.as_ptr(), mask) };
        if wd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(wd)
    }
    /// Next batch of `(wd, mask, name)`.
    pub async fn events(&self) -> io::Result<Vec<(i32, u32, std::ffi::OsString)>> {
        loop {
            let mut guard = self.0.readable().await?;
            let mut buf = [0u8; 8192];
            let r = guard.try_io(|fd| {
                let n = unsafe {
                    libc::read(fd.get_ref().as_raw_fd(), buf.as_mut_ptr().cast(), buf.len())
                };
                if n < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(n as usize)
                }
            });
            let Ok(r) = r else { continue };
            let n = r?;
            let mut out = Vec::new();
            let mut off = 0;
            let header = std::mem::size_of::<libc::inotify_event>();
            while off + header <= n {
                let ev: libc::inotify_event =
                    unsafe { std::ptr::read_unaligned(buf[off..].as_ptr().cast()) };
                let name_bytes = &buf[off + header..off + header + ev.len as usize];
                let end = name_bytes
                    .iter()
                    .position(|b| *b == 0)
                    .unwrap_or(name_bytes.len());
                out.push((
                    ev.wd,
                    ev.mask,
                    std::ffi::OsStr::from_bytes(&name_bytes[..end]).to_os_string(),
                ));
                off += header + ev.len as usize;
            }
            return Ok(out);
        }
    }
    /// Wait for at least one event and discard the batch.
    pub async fn changed(&self) -> io::Result<()> {
        loop {
            let mut guard = self.0.readable().await?;
            let r = guard.try_io(|fd| {
                let mut buf = [0u8; 8192];
                let n = unsafe {
                    libc::read(fd.get_ref().as_raw_fd(), buf.as_mut_ptr().cast(), buf.len())
                };
                if n < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
            if let Ok(r) = r {
                return r;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stat_with_parens_in_comm() {
        let s = "42 (a) b) S 7 1 1 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 999 0 0";
        let st = parse_stat(s).unwrap();
        assert_eq!(st.comm, "a) b");
        assert_eq!(st.ppid, 7);
        assert_eq!(st.start_ticks, 999);
    }
    #[test]
    fn iso_shape() {
        let s = local_iso_now();
        assert_eq!(s.len(), 32, "{s}");
        assert_eq!(&s[10..11], "T");
    }
    #[test]
    fn temp_and_backup_never_follow_symlinks_and_mode_is_kept() {
        let dir = std::env::temp_dir().join(format!("hyprv-util-safe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let victim = dir.join("victim");
        std::fs::write(&victim, "keep").unwrap();
        let target = dir.join("settings.ini");
        std::fs::write(&target, "old").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        // Plant symlinks at every predictable name an older writer would use.
        for n in 0..8 {
            let _ = std::os::unix::fs::symlink(
                &victim,
                dir.join(format!(".settings.ini.{}.{n}", std::process::id())),
            );
        }
        let _ = std::os::unix::fs::symlink(&victim, dir.join("settings.ini.hyprv-bak"));
        let old = unsafe { libc::umask(0o022) };
        atomic_write(&target, b"new", None).unwrap();
        let backup = versioned_backup(&target, b"old", 0o600).unwrap();
        unsafe { libc::umask(old) };
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        assert!(!std::fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o600,
            "DESKTOP-7"
        );
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "old");
        assert!(backup
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("settings.ini.hyprv-bak."));
        // New files default to private.
        atomic_write(&dir.join("fresh"), b"x", None).unwrap();
        assert_eq!(
            std::fs::metadata(dir.join("fresh"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn compare_and_swap_rejects_concurrent_change() {
        let dir = std::env::temp_dir().join(format!("hyprv-util-cas-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("doc");
        std::fs::write(&f, "v2-from-someone-else").unwrap();
        let e = replace_checked(&f, b"mine", None, Expect::Bytes(b"v1")).unwrap_err();
        assert!(is_conflict(&e));
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "v2-from-someone-else");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "temp removed");
        assert!(is_conflict(
            &replace_checked(&f, b"x", None, Expect::Absent).unwrap_err()
        ));
        replace_checked(&f, b"ok", None, Expect::Bytes(b"v2-from-someone-else")).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn atomic_write_and_symlink() {
        let dir = std::env::temp_dir().join(format!("hyprv-util-{}", std::process::id()));
        let f = dir.join("a/b.txt");
        atomic_write(&f, b"x", Some(0o600)).unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"x");
        let l = dir.join("link");
        atomic_symlink(&f, &l).unwrap();
        atomic_symlink(&f, &l).unwrap();
        assert_eq!(std::fs::read_link(&l).unwrap(), f);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
#[cfg(test)]
mod watch_tests {
    use super::*;
    #[tokio::test(flavor = "current_thread")]
    async fn one_watch_survives_repeated_atomic_replaces() {
        let root = std::env::temp_dir().join(format!("hyprv-rearm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let w = Watch::new(&[(root.clone(), &["state.json"][..])]).unwrap();
        for i in 0..5 {
            atomic_write(&root.join("state.json"), format!("{i}").as_bytes(), None).unwrap();
            tokio::time::timeout(Duration::from_secs(1), w.changed())
                .await
                .unwrap()
                .unwrap();
            // Drain the follow-up events of the same replace.
            while tokio::time::timeout(Duration::from_millis(20), w.changed())
                .await
                .is_ok()
            {}
        }
        std::fs::write(root.join("noise"), "x").unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(100), w.changed())
                .await
                .is_err()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test(flavor = "current_thread")]
    async fn filters_by_name_and_sees_missing_dirs() {
        let root = std::env::temp_dir().join(format!("hyprv-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let w = Watch::new(&[
            (root.clone(), &["want"][..]),
            (root.join("later/sub"), &[][..]),
        ])
        .unwrap();
        std::fs::write(root.join("noise.lock"), "x").unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(100), w.changed())
                .await
                .is_err()
        );
        std::fs::write(root.join("want"), "x").unwrap();
        tokio::time::timeout(Duration::from_secs(1), w.changed())
            .await
            .unwrap()
            .unwrap();
        std::fs::create_dir(root.join("later")).unwrap();
        tokio::time::timeout(Duration::from_secs(1), w.changed())
            .await
            .unwrap()
            .unwrap();
        // `want` now exists: watched as a file; noise stays silent, atomic replace wakes.
        let w = Watch::new(&[(root.clone(), &["want"][..])]).unwrap();
        std::fs::write(root.join("noise.lock"), "y").unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(100), w.changed())
                .await
                .is_err()
        );
        atomic_write(&root.join("want"), b"z", None).unwrap();
        tokio::time::timeout(Duration::from_secs(1), w.changed())
            .await
            .unwrap()
            .unwrap();
        // Read-only lock reopen produces no IN_CLOSE_WRITE.
        let w = Watch::new(&[(root.clone(), &[][..])]).unwrap();
        let lock = root.join("state.lock");
        std::fs::write(&lock, "").unwrap();
        tokio::time::timeout(Duration::from_secs(1), w.changed())
            .await
            .unwrap()
            .unwrap();
        drop(FileLock::exclusive_blocking(&lock).unwrap());
        assert!(
            tokio::time::timeout(Duration::from_millis(100), w.changed())
                .await
                .is_err()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
