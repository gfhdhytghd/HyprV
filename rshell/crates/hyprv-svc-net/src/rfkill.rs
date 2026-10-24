//! Linux /dev/rfkill records are readiness-driven, not sampled sysfs attributes.
use crate::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{self, Read},
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
};
use tokio::io::unix::AsyncFd;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub available: bool,
    pub soft_blocked: bool,
    pub hard_blocked: bool,
    pub error: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record {
    pub index: u32,
    pub kind: u8,
    pub operation: u8,
    pub soft: bool,
    pub hard: bool,
}
impl Record {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 8 {
            return Err("short rfkill event".into());
        }
        Ok(Self {
            index: u32::from_ne_bytes(bytes[..4].try_into().unwrap()),
            kind: bytes[4],
            operation: bytes[5],
            soft: bytes[6] != 0,
            hard: bytes[7] != 0,
        })
    }
}
#[derive(Default)]
pub struct Reducer {
    entries: BTreeMap<u32, Record>,
}
impl Reducer {
    pub fn apply(&mut self, record: Record, kind: u8) -> State {
        match record.operation {
            0 | 2 => {
                self.entries.insert(record.index, record);
            }
            1 => {
                self.entries.remove(&record.index);
            }
            3 => {
                for item in self
                    .entries
                    .values_mut()
                    .filter(|r| record.kind == 0 || r.kind == record.kind)
                {
                    item.soft = record.soft;
                }
            }
            _ => {}
        }
        State {
            available: true,
            soft_blocked: self.entries.values().any(|r| r.kind == kind && r.soft),
            hard_blocked: self.entries.values().any(|r| r.kind == kind && r.hard),
            error: None,
        }
    }
}
pub struct Monitor {
    fd: AsyncFd<File>,
    reducer: Reducer,
    kind: u8,
}
impl Monitor {
    pub fn open(kind: u8) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open("/dev/rfkill")?;
        Ok(Self {
            fd: unsafe { AsyncFd::register(file) }?,
            reducer: Reducer::default(),
            kind,
        })
    }
    pub async fn next(&mut self) -> Result<State> {
        loop {
            let mut ready = self.fd.readable().await.map_err(|e| e.to_string())?;
            match ready.try_io(|fd| {
                let mut data = [0u8; 8];
                let n = fd.get_ref().read(&mut data)?;
                if n == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "rfkill closed",
                    ));
                }
                Ok((data, n))
            }) {
                Ok(Ok((bytes, n))) => {
                    return Ok(self.reducer.apply(Record::parse(&bytes[..n])?, self.kind))
                }
                Ok(Err(e)) => return Err(e.to_string()),
                Err(_) => continue,
            }
        }
    }
}
impl AsRawFd for Monitor {
    fn as_raw_fd(&self) -> i32 {
        self.fd.get_ref().as_raw_fd()
    }
}
/// Watches device creation/permission changes as well as rfkill records. No retry timer.
pub struct Watcher {
    directory: AsyncFd<std::os::fd::OwnedFd>,
    monitor: Option<Monitor>,
    kind: u8,
    initial: bool,
    open_error: Option<String>,
}
impl Watcher {
    pub fn new(kind: u8) -> io::Result<Self> {
        use std::os::fd::FromRawFd;
        let raw = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = unsafe { std::os::fd::OwnedFd::from_raw_fd(raw) };
        let path = std::ffi::CString::new("/dev").unwrap();
        if unsafe {
            libc::inotify_add_watch(
                fd.as_raw_fd(),
                path.as_ptr(),
                libc::IN_CREATE
                    | libc::IN_DELETE
                    | libc::IN_MOVED_TO
                    | libc::IN_MOVED_FROM
                    | libc::IN_ATTRIB,
            )
        } < 0
        {
            return Err(io::Error::last_os_error());
        }
        let (monitor, open_error) = match Monitor::open(kind) {
            Ok(m) => (Some(m), None),
            Err(e) => (None, Some(e.to_string())),
        };
        Ok(Self {
            directory: unsafe { AsyncFd::register(fd) }?,
            monitor,
            kind,
            initial: true,
            open_error,
        })
    }
    pub async fn next(&mut self) -> Result<State> {
        if self.initial {
            self.initial = false;
            return Ok(State {
                available: self.monitor.is_some(),
                error: self.open_error.take(),
                ..State::default()
            });
        }
        loop {
            tokio::select! {
                result=async{self.monitor.as_mut().unwrap().next().await},if self.monitor.is_some()=>{
                    match result{Ok(s)=>return Ok(s),Err(e)=>{self.monitor=None;return Ok(State{error:Some(e),..State::default()});}}
                },
                result=dev_changed(&self.directory)=>{
                    result?;
                    if self.monitor.is_none(){
                        match Monitor::open(self.kind){Ok(m)=>{self.monitor=Some(m);return Ok(State{available:true,..State::default()});},Err(e)=>return Ok(State{error:Some(e.to_string()),..State::default()})}
                    }
                },
            }
        }
    }
}
async fn dev_changed(fd: &AsyncFd<std::os::fd::OwnedFd>) -> Result<()> {
    loop {
        let mut ready = fd.readable().await.map_err(|e| e.to_string())?;
        let result = ready.try_io(|fd| {
            let mut buf = [0u8; 8192];
            let n =
                unsafe { libc::read(fd.get_ref().as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
            if n < 0 {
                return Err(io::Error::last_os_error());
            }
            let mut offset = 0usize;
            let mut changed = false;
            while offset + std::mem::size_of::<libc::inotify_event>() <= n as usize {
                let event = unsafe {
                    std::ptr::read_unaligned(buf.as_ptr().add(offset).cast::<libc::inotify_event>())
                };
                let start = offset + std::mem::size_of::<libc::inotify_event>();
                let end = start + event.len as usize;
                if end > n as usize {
                    break;
                }
                let name = &buf[start..end];
                let name = &name[..name.iter().position(|b| *b == 0).unwrap_or(name.len())];
                changed |= name == b"rfkill" || event.mask & libc::IN_Q_OVERFLOW != 0;
                offset = end;
            }
            Ok(changed)
        });
        match result {
            Ok(Ok(true)) => return Ok(()),
            Ok(Ok(false)) | Err(_) => {}
            Ok(Err(e)) => return Err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn change_delete_and_change_all() {
        let mut r = Reducer::default();
        assert!(
            r.apply(Record::parse(&[1, 0, 0, 0, 1, 0, 1, 0]).unwrap(), 1)
                .soft_blocked
        );
        assert!(
            !r.apply(Record::parse(&[0, 0, 0, 0, 0, 3, 0, 0]).unwrap(), 1)
                .soft_blocked
        );
        assert!(
            r.apply(Record::parse(&[1, 0, 0, 0, 1, 2, 0, 1]).unwrap(), 1)
                .hard_blocked
        );
        assert!(
            !r.apply(Record::parse(&[1, 0, 0, 0, 1, 1, 0, 0]).unwrap(), 1)
                .hard_blocked
        );
        assert!(Record::parse(&[0]).is_err());
    }
}
