use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
struct Instance {
    child: Child,
    dir: PathBuf,
}
impl Drop for Instance {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
#[test]
fn single_instance_cli_and_shutdown_acknowledgement() {
    let dir = std::env::temp_dir().join(format!("hyprv-shell-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let command = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_hyprv-shell"));
        command
            .env("XDG_RUNTIME_DIR", &dir)
            .env("XDG_CONFIG_HOME", dir.join("config"));
        command
    };
    let child = command()
        .arg("--headless")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut instance = Instance {
        child,
        dir: dir.clone(),
    };
    let socket = dir.join("hyprv/shell.sock");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !socket.exists() {
        assert!(Instant::now() < deadline, "IPC socket did not appear");
        assert!(
            instance.child.try_wait().unwrap().is_none(),
            "shell exited before bind"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let duplicate = command().arg("--headless").output().unwrap();
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("already running"));
    let pending_ui = command()
        .args(["ipc", "call", "calendar", "toggle"])
        .output()
        .unwrap();
    assert!(
        pending_ui.status.success(),
        "{}",
        String::from_utf8_lossy(&pending_ui.stderr)
    );
    let layout = command()
        .args(["ipc", "call", "bar", "layoutStatus", "HEADLESS-TEST"])
        .output()
        .unwrap();
    assert!(layout.status.success());
    let data: serde_json::Value = serde_json::from_slice(&layout.stdout).unwrap();
    assert_eq!(data["expanded"], true);
    let invalid = command()
        .args(["ipc", "call", "quickAdjust", "showBrightnessLevel", "oops"])
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    let killed = command().arg("kill").output().unwrap();
    assert!(
        killed.status.success(),
        "{}",
        String::from_utf8_lossy(&killed.stderr)
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = instance.child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline, "shell did not exit");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!socket.exists());
}

/// Exercises popup creation after fractional-scale feedback, including zero-size
/// initial surface state. Uses an isolated IPC runtime and never reserves space.
#[test]
#[ignore = "bounded live Wayland host smoke; run explicitly with WAYLAND_DISPLAY"]
fn live_popups_and_calendar_survive_fractional_scale_startup() {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").unwrap();
    let display = PathBuf::from(std::env::var_os("WAYLAND_DISPLAY").unwrap());
    let display = if display.is_absolute() {
        display
    } else {
        PathBuf::from(runtime).join(display)
    };
    let dir = std::env::temp_dir().join(format!("hyprv-shell-live-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let command = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_hyprv-shell"));
        command
            .env("XDG_RUNTIME_DIR", &dir)
            .env("WAYLAND_DISPLAY", &display);
        command
    };
    let child = command()
        .args(["--demo-host", "--no-exclusive", "--exit-after-ms", "2500"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut instance = Instance {
        child,
        dir: dir.clone(),
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    while !dir.join("hyprv/shell.sock").exists() {
        assert!(Instant::now() < deadline);
        assert!(instance.child.try_wait().unwrap().is_none());
        std::thread::sleep(Duration::from_millis(10));
    }
    // Let output globals and the initial five surfaces configure first.
    std::thread::sleep(Duration::from_millis(250));
    for (target, function) in [
        ("calendar", "toggle"),
        ("controlPanel", "toggle"),
        ("controlPanel", "toggle"),
        ("calendar", "toggle"),
        ("quickAdjust", "showVolume"),
    ] {
        let output = command()
            .args(["ipc", "call", target, function])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let status = instance.child.wait().unwrap();
    let mut stderr = String::new();
    use std::io::Read;
    instance
        .child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(status.success(), "{stderr}");
    assert!(!dir.join("hyprv/shell.sock").exists());
}

#[test]
#[ignore = "isolated compositor smoke; requires Hyprland, kwin_wayland and dbus-run-session"]
fn nested_test_captures_real_gpu_surfaces() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/nested-test-regression");
    let result = Command::new(env!("CARGO_BIN_EXE_hyprv-shell"))
        .arg("--nested-test")
        .arg(&dir)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["scenes"].as_array().unwrap().len(), 6);
    for scene in manifest["scenes"].as_array().unwrap() {
        let path = dir.join(scene["manifest"].as_str().unwrap());
        let surfaces: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        for surface in surfaces["surfaces"].as_array().unwrap() {
            let bytes = std::fs::read(
                path.parent()
                    .unwrap()
                    .join(surface["file"].as_str().unwrap()),
            )
            .unwrap();
            assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
            assert_eq!(surface["source"], "retained GPU target");
            let image = hyprv_ui::skia::Image::from_encoded(hyprv_ui::skia::Data::new_copy(&bytes))
                .unwrap();
            let ns = surface["namespace"].as_str().unwrap();
            if surface["input_only"] == true {
                assert_eq!(
                    (image.width(), image.height()),
                    (1, 1),
                    "{ns} must not allocate an output-sized GPU target"
                );
            } else if ns.starts_with("shell:") {
                assert!(
                    image.width() < 1280 && image.height() < 720,
                    "{ns} is not card-sized"
                );
                let info = hyprv_ui::skia::ImageInfo::new(
                    (image.width(), image.height()),
                    hyprv_ui::skia::ColorType::RGBA8888,
                    hyprv_ui::skia::AlphaType::Premul,
                    None,
                );
                let mut pixels = vec![0u8; image.width() as usize * image.height() as usize * 4];
                assert!(image.read_pixels(
                    &info,
                    &mut pixels,
                    image.width() as usize * 4,
                    (0, 0),
                    hyprv_ui::skia::image::CachingHint::Allow
                ));
                assert!(
                    pixels.chunks_exact(4).any(|p| p[3] > 0),
                    "{ns} captured an empty panel"
                );
            }
        }
    }
}

#[test]
#[ignore = "isolated assembled fixture GPU replay; requires Hyprland, KWin, dbus-run-session and cc"]
fn nested_fixture_replays_full_outputs_and_does_not_pass_missing_references() {
    let dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/nested-fixture-regression");
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/assembled-desktop.json");
    let references = dir.join(format!("missing-references-{}", std::process::id()));
    let result = Command::new(env!("CARGO_BIN_EXE_hyprv-shell"))
        .arg("--nested-test")
        .arg(&dir)
        .arg("--fixture")
        .arg(fixture)
        .args([
            "--theme",
            "dark",
            "--scale",
            "1",
            "--capture-only",
            "--references",
        ])
        .arg(references)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let matrix: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("fullscreen-results.json")).unwrap())
            .unwrap();
    assert_eq!(matrix["pass"], false);
    let jobs = matrix["jobs"].as_array().unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0]["runtime_ok"], true);
    let manifest = PathBuf::from(jobs[0]["manifest"].as_str().unwrap());
    let data: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    let captures = data["captures"].as_array().unwrap();
    assert_eq!(captures.len(), 10);
    for capture in captures {
        assert_eq!(capture["pixel_size"], serde_json::json!([3072, 1728]));
        assert_eq!(capture["comparison"]["status"], "missing_reference");
        assert_eq!(capture["comparison"]["pass"], false);
        assert!(capture["renderer"].as_str().is_some_and(|r| !r.is_empty()));
        assert_eq!(capture["format"], "RGBA8");
    }
    let capture_manifest = manifest
        .parent()
        .unwrap()
        .join(data["capture_manifest"].as_str().unwrap());
    let image = |index: usize| {
        std::fs::read(
            capture_manifest
                .parent()
                .unwrap()
                .join(captures[index]["file"].as_str().unwrap()),
        )
        .unwrap()
    };
    assert_ne!(
        image(1),
        image(5),
        "control panel opening animation must advance"
    );
    assert_ne!(
        image(0),
        image(6),
        "calendar must be assembled into the output"
    );
}

#[test]
#[ignore = "full native-input functional matrix; private Hyprland/KWin, virtual input protocols, cc, and XKB required"]
fn nested_functional_matrix_uses_native_input_and_mocked_service_commands() {
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/functional-test-regression");
    let result = Command::new(env!("CARGO_BIN_EXE_hyprv-shell"))
        .arg("--nested-functional-test")
        .arg(&directory)
        .env_remove("HYPRV_FUNCTIONAL_FILTER")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["pass"], true);
    let cases = manifest["cases"].as_array().unwrap();
    assert!(cases.len() >= 150);
    for row in cases {
        assert_eq!(row["status"], "PASS", "{}", row["id"]);
    }
    for required in [
        "ipc.shell.kill",
        "ipc.desktop.deskflowReflow",
        "bar.hover.Battery",
        "bar.hover.Overflow",
        "mihomo.start",
        "wifi.secure-password-return",
        "island.music.seek",
        "control.slider-drag.brightness",
        "calendar.open-event.timed",
        "outputs.input-second",
        "outputs.scale-1.5",
        "outputs.scale-2",
        "compositor.disconnect",
        "compositor.restart-fresh-reconnect",
    ] {
        assert!(
            cases.iter().any(|row| row["id"] == required),
            "missing matrix row {required}"
        );
    }
    assert_eq!(
        manifest["services"],
        "mock sink; zero native service actors"
    );
}

#[test]
#[ignore = "isolated full-screen Wi-Fi focus/Qt tray regression; private Hyprland/KWin and GPU required"]
fn nested_fixture_stationary_pointer_leaves_bar_when_popup_covers_it() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join("crates/hyprv-shell/artifacts/fullscreen-core/wifi-regression");
    let result = Command::new(env!("CARGO_BIN_EXE_hyprv-shell"))
        .arg("--nested-test")
        .arg(&dir)
        .arg("--fixture")
        .arg(root.join("parity/fullscreen/fixtures/wifi.json"))
        .arg("--references")
        .arg(root.join("parity/fullscreen/reference"))
        .args(["--scale", "2"])
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
        .env("TMPDIR", "/tmp")
        .env("LIBSEAT_BACKEND", "seatd")
        .env("AQ_DRM_DEVICES", "/nonexistent")
        .output()
        .unwrap();
    let matrix: serde_json::Value = serde_json::from_slice(
        &std::fs::read(dir.join("fullscreen-results.json")).unwrap(),
    ).unwrap();
    assert!(result.status.success(), "{}\n{matrix}", String::from_utf8_lossy(&result.stderr));
    let jobs = matrix["jobs"].as_array().unwrap();
    assert_eq!(jobs.len(), 2);
    for job in jobs {
        let manifest: serde_json::Value = serde_json::from_slice(
            &std::fs::read(job["manifest"].as_str().unwrap()).unwrap(),
        ).unwrap();
        assert_eq!(manifest["captures"][0]["comparison"]["pass"], true, "{manifest}");
    }
}
