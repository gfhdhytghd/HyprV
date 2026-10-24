//! Nonblocking systemd notification; heartbeats are sent only by the cooling loop.
use std::{
    io,
    os::{
        linux::net::SocketAddrExt,
        unix::net::{SocketAddr, UnixDatagram},
    },
};
pub fn enabled() -> bool {
    std::env::var("WATCHDOG_USEC")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .is_some_and(|v| v > 0)
}
pub fn notify(message: &str) -> io::Result<()> {
    let Some(path) = std::env::var_os("NOTIFY_SOCKET") else {
        return Ok(());
    };
    use std::os::unix::ffi::OsStrExt;
    let bytes = path.as_os_str().as_bytes();
    let address = if let Some(name) = bytes.strip_prefix(b"@") {
        SocketAddr::from_abstract_name(name)?
    } else {
        SocketAddr::from_pathname(path)?
    };
    send(&address, message)
}
fn send(address: &SocketAddr, message: &str) -> io::Result<()> {
    let socket = UnixDatagram::unbound()?;
    socket.set_nonblocking(true)?;
    socket.send_to_addr(message.as_bytes(), address)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notify_uses_a_private_datagram_socket() {
        let name = format!("hyprv-watchdog-test-{}", std::process::id());
        let address = SocketAddr::from_abstract_name(name).unwrap();
        let receiver = UnixDatagram::bind_addr(&address).unwrap();
        receiver
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .unwrap();
        send(&address, "WATCHDOG=1").unwrap();
        let mut data = [0; 64];
        let n = receiver.recv(&mut data).unwrap();
        assert_eq!(&data[..n], b"WATCHDOG=1");
    }
}
