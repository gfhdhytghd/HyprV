//! Isolated subprocess: environment never mutates another test's bus selection.
use std::{
    io::BufRead,
    process::{Command, Stdio},
};
#[test]
fn private_system_bus_child() {
    let Ok(expected) = std::env::var("HYPRV_TEST_BUS_ID") else {
        return;
    };
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let connection = zbus::Connection::system().await.unwrap();
            let proxy = zbus::fdo::DBusProxy::new(&connection).await.unwrap();
            assert_eq!(proxy.get_id().await.unwrap().as_str(), expected);
        });
}
#[test]
fn default_system_connector_honors_private_address() {
    let mut daemon = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--nopidfile", "--print-address=1"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    struct Cleanup(std::process::Child);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut address = String::new();
    std::io::BufReader::new(daemon.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    let _cleanup = Cleanup(daemon);
    let address = address.trim();
    let id = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let connection = zbus::connection::Builder::address(address)
                .unwrap()
                .build()
                .await
                .unwrap();
            zbus::fdo::DBusProxy::new(&connection)
                .await
                .unwrap()
                .get_id()
                .await
                .unwrap()
                .to_string()
        });
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "private_system_bus_child"])
        .env("DBUS_SYSTEM_BUS_ADDRESS", address)
        .env("HYPRV_TEST_BUS_ID", id)
        .status()
        .unwrap();
    assert!(status.success());
}
