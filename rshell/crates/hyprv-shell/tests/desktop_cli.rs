//! `hyprv-shell desktop …` (the patched hypr scripts' entry point) falls back to running
//! the action in-process when no shell is reachable. Uses deskflowReflow with no Deskflow
//! config, which fails before any side effect; the environment is a temp HOME and runtime dir.
use std::process::Command;

fn run(args: &[&str], dir: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_hyprv-shell"))
        .args(args)
        .env_clear()
        .env("HOME", dir.join("home"))
        .env("XDG_RUNTIME_DIR", dir.join("run"))
        .env("PATH", "/usr/bin")
        .output()
        .unwrap()
}

#[test]
fn desktop_cli_falls_back_in_process_when_no_shell_is_running() {
    let dir = std::env::temp_dir().join(format!("hyprv-desktop-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("home")).unwrap();
    std::fs::create_dir_all(dir.join("run")).unwrap();
    let out = run(&["desktop", "deskflowReflow"], &dir);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(
        err.contains("not running; applying `desktop deskflowReflow` directly"),
        "{err}"
    );
    assert!(
        err.contains("cannot stat") && err.contains("deskflow-server.conf"),
        "{err}"
    );
    let out = run(&["desktop", "setVariant", "daily"], &dir);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("variant must be"),
        "rejected before any action"
    );
    assert!(!dir.join("home/.local/state").exists(), "nothing written");
    let _ = std::fs::remove_dir_all(&dir);
}
