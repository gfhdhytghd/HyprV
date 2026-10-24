//! Static package checks only: no unit installation or privileged execution.
#[test]
fn root_packaging_uses_the_authoritative_helper_contract() {
    let crate_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo = crate_dir.parent().unwrap().parent().unwrap();
    for name in [
        "hyprv-privhelper.service",
        "org.hyprv.Shell.PrivHelper.conf",
        "org.hyprv.Shell.PrivHelper.service",
        "org.hyprv.shell.fan-control.policy",
    ] {
        let root = repo.join("packaging").join(name);
        let canonical = crate_dir.join("packaging").join(name);
        assert_eq!(
            root.canonicalize().unwrap(),
            canonical.canonicalize().unwrap()
        );
    }
    let unit = include_str!("../packaging/hyprv-privhelper.service");
    for directive in [
        "StateDirectory=hyprv-privhelper",
        "StateDirectoryMode=0700",
        "Type=notify",
        "WatchdogSec=5s",
        "ExecStartPre=/usr/libexec/hyprv-cooling-failsafe",
        "ExecStopPost=/usr/libexec/hyprv-cooling-failsafe",
        "AF_UNIX AF_NETLINK",
        "Restart=always",
    ] {
        assert!(unit.contains(directive), "missing {directive}");
    }
    assert!(!unit.contains("/home/wilf"));
    let policy = include_str!("../packaging/org.hyprv.Shell.PrivHelper.conf");
    for member in [
        "GetFans",
        "SetFanLimit",
        "SubscribeCpuPower",
        "UnsubscribeCpuPower",
        "Get",
        "GetAll",
    ] {
        assert!(policy.contains(&format!("send_member=\"{member}\"")));
    }
    let main = include_str!("../src/main.rs");
    let fan = main
        .split("async fn set_fan_limit(")
        .nth(1)
        .unwrap()
        .split("async fn ")
        .next()
        .unwrap();
    assert!(
        fan.find("authorize(connection, &header, ACTION, 1).await?")
            .unwrap()
            < fan.find(".set_limit(").unwrap()
    );
}

#[test]
fn cpu_only_files_are_authoritative_and_never_install_cooling_directives() {
    let crate_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo = crate_dir.parent().unwrap().parent().unwrap();
    for name in [
        "hyprv-cpu-power-only.service",
        "org.hyprv.Shell.CpuPower.service",
        "org.hyprv.Shell.CpuPower.conf",
        "org.hyprv.shell.cpu-power.policy",
    ] {
        assert_eq!(
            repo.join("packaging").join(name).canonicalize().unwrap(),
            crate_dir
                .join("packaging")
                .join(name)
                .canonicalize()
                .unwrap()
        );
    }
    let unit = include_str!("../packaging/hyprv-cpu-power-only.service");
    for forbidden in [
        "ExecStartPre=",
        "ExecStopPost=",
        "ReadWritePaths=",
        "WantedBy=",
        "WatchdogSec=",
    ] {
        assert!(!unit.contains(forbidden), "{forbidden}");
    }
    assert!(unit.contains("ExecCondition=/usr/libexec/hyprv-cpu-power --eligible"));
    let policy = include_str!("../packaging/org.hyprv.shell.cpu-power.policy");
    assert!(policy.contains("org.hyprv.shell.cpu-power"));
    assert!(!policy.contains("fan-control"));
    assert!(!policy.contains("egpu"));
}
