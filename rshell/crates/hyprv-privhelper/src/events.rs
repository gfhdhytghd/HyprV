//! Native kernel event sources, without a subprocess or periodic rediscovery.
use std::{
    io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
};
use tokio::io::unix::AsyncFd;
pub struct KernelEvents(AsyncFd<OwnedFd>);
impl KernelEvents {
    fn new(protocol: i32, groups: u32) -> io::Result<Self> {
        let raw = unsafe {
            libc::socket(
                libc::AF_NETLINK,
                libc::SOCK_DGRAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                protocol,
            )
        };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        let mut address: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
        address.nl_family = libc::AF_NETLINK as u16;
        address.nl_groups = groups;
        if unsafe {
            libc::bind(
                raw,
                (&address as *const libc::sockaddr_nl).cast(),
                std::mem::size_of_val(&address) as u32,
            )
        } < 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(
            unsafe { AsyncFd::register(fd) }.map_err(io::Error::other)?,
        ))
    }
    pub fn udev() -> io::Result<Self> {
        Self::new(libc::NETLINK_KOBJECT_UEVENT, 1)
    }
    pub fn routes() -> io::Result<Self> {
        Self::new(libc::NETLINK_ROUTE, 1 | 0x40 | 0x400)
    }
    pub async fn next(&self) -> io::Result<Vec<u8>> {
        loop {
            let mut ready = self.0.readable().await?;
            let result = ready.try_io(|fd| {
                let mut bytes = vec![0u8; 65536];
                let mut sender: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
                let mut len = std::mem::size_of_val(&sender) as u32;
                let n = unsafe {
                    libc::recvfrom(
                        fd.get_ref().as_raw_fd(),
                        bytes.as_mut_ptr().cast(),
                        bytes.len(),
                        libc::MSG_TRUNC,
                        (&mut sender as *mut libc::sockaddr_nl).cast(),
                        &mut len,
                    )
                };
                if n < 0 {
                    return Err(io::Error::last_os_error());
                }
                if n as usize > bytes.len() || sender.nl_pid != 0 {
                    return Ok(Vec::new());
                }
                bytes.truncate(n as usize);
                Ok(bytes)
            });
            if let Ok(result) = result {
                let bytes = result?;
                if !bytes.is_empty() {
                    return Ok(bytes);
                }
            }
        }
    }
    pub async fn subsystem(&self, subsystems: &[&str]) -> io::Result<()> {
        loop {
            let bytes = self.next().await?;
            if bytes.split(|b| *b == 0).any(|part| {
                part.strip_prefix(b"SUBSYSTEM=")
                    .is_some_and(|name| subsystems.iter().any(|s| name == s.as_bytes()))
            }) {
                return Ok(());
            }
        }
    }
}
