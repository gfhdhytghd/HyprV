//! Kernel uevent netlink listener (NETLINK_KOBJECT_UEVENT, kernel multicast group).
//! Used for PCI/Thunderbolt and tty hotplug without libudev or polling.
use std::{
    collections::HashMap,
    io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
};
use tokio::io::unix::AsyncFd;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Uevent {
    pub action: String,
    pub devpath: String,
    pub subsystem: String,
    pub vars: HashMap<String, String>,
}
pub struct UeventSocket(AsyncFd<OwnedFd>);
impl UeventSocket {
    pub fn open() -> io::Result<Self> {
        let raw = unsafe {
            libc::socket(
                libc::AF_NETLINK,
                libc::SOCK_DGRAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                libc::NETLINK_KOBJECT_UEVENT,
            )
        };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        let mut addr: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
        addr.nl_family = libc::AF_NETLINK as u16;
        addr.nl_groups = 1;
        let r = unsafe {
            libc::bind(
                fd.as_raw_fd(),
                (&addr as *const libc::sockaddr_nl).cast(),
                std::mem::size_of::<libc::sockaddr_nl>() as u32,
            )
        };
        if r < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(unsafe { AsyncFd::register(fd) }?))
    }
    pub async fn next(&self) -> io::Result<Uevent> {
        loop {
            let mut guard = self.0.readable().await?;
            let mut buf = vec![0u8; 16384];
            let res = guard.try_io(|fd| {
                let n = unsafe {
                    libc::recv(
                        fd.get_ref().as_raw_fd(),
                        buf.as_mut_ptr().cast(),
                        buf.len(),
                        0,
                    )
                };
                if n < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(n as usize)
                }
            });
            match res {
                Ok(Ok(n)) => {
                    if let Some(ev) = parse(&buf[..n]) {
                        return Ok(ev);
                    }
                }
                Ok(Err(e)) if e.kind() == io::ErrorKind::Interrupted => {}
                Ok(Err(e)) => return Err(e),
                Err(_) => {}
            }
        }
    }
}
/// Kernel format: `ACTION@DEVPATH\0KEY=VALUE\0...`.
pub fn parse(buf: &[u8]) -> Option<Uevent> {
    let mut parts = buf.split(|b| *b == 0).filter(|p| !p.is_empty());
    let header = std::str::from_utf8(parts.next()?).ok()?;
    if header.starts_with("libudev") || !header.contains('@') {
        return None;
    }
    let mut vars = HashMap::new();
    for part in parts {
        if let Ok(s) = std::str::from_utf8(part) {
            if let Some((k, v)) = s.split_once('=') {
                vars.insert(k.to_string(), v.to_string());
            }
        }
    }
    Some(Uevent {
        action: vars.get("ACTION").cloned()?,
        devpath: vars.get("DEVPATH").cloned().unwrap_or_default(),
        subsystem: vars.get("SUBSYSTEM").cloned().unwrap_or_default(),
        vars,
    })
}
#[cfg(test)]
mod tests {
    #[test]
    fn parses_kernel_message() {
        let msg = b"remove@/devices/pci0000:00/0000:00:01.0\0ACTION=remove\0DEVPATH=/devices/pci0000:00/0000:00:01.0\0SUBSYSTEM=pci\0";
        let ev = super::parse(msg).unwrap();
        assert_eq!(ev.action, "remove");
        assert_eq!(ev.subsystem, "pci");
        assert!(super::parse(b"libudev\0x").is_none());
    }
}
