//! Explicit opt-in host tests: property reads only. No unit loading or operations.
#[tokio::test]
#[ignore = "requires real user and system buses; only reads Version"]
async fn read_only_systemd_version_both_scopes() {
    for system in [true, false] {
        let connection = if system {
            zbus::Connection::system().await
        } else {
            zbus::Connection::session().await
        }
        .unwrap();
        let manager = zbus::Proxy::new(
            &connection,
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
        )
        .await
        .unwrap();
        let version: String = manager.get_property("Version").await.unwrap();
        assert!(!version.is_empty());
    }
}
