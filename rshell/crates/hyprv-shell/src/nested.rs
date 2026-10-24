//! Explicit test harness. Owns a private headless compositor and private D-Bus.
//! No production polling or helper process is introduced by this module.
use std::{
    error::Error,
    fs,
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Child, Command},
    time::{Duration, Instant},
};
struct RuntimeDirectory(PathBuf);
impl Drop for RuntimeDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Compositor(Child);
impl Drop for Compositor {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
pub fn run(directory: PathBuf) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(&directory)?;
    let directory = directory.canonicalize()?;
    let config = directory.join("bus.conf");
    fs::write(
        &config,
        r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>"#,
    )?;
    // Pin the executing inode before launching children. Parallel workspace builds
    // may replace target/debug/hyprv-shell while this long test is running.
    let binary = directory.join(format!("hyprv-shell-test-{}", std::process::id()));
    let executing = fs::metadata("/proc/self/exe")?;
    let linked = std::env::current_exe()
        .ok()
        .is_some_and(|source| fs::hard_link(source, &binary).is_ok());
    let correct = linked
        && fs::metadata(&binary)
            .is_ok_and(|m| m.ino() == executing.ino() && m.dev() == executing.dev());
    if !correct {
        let _ = fs::remove_file(&binary);
        fs::copy("/proc/self/exe", &binary)?;
    }
    let fake_system_bus = std::env::var("HYPRV_FAKE_CONTROLS_LUA").ok()
        .and_then(|_| std::env::var("DBUS_SYSTEM_BUS_ADDRESS").ok().or_else(|| std::env::var("DBUS_SESSION_BUS_ADDRESS").ok()));
    let mut driver = Command::new("dbus-run-session");
    if let Some(address) = fake_system_bus { driver.env("HYPRV_TEST_SYSTEM_BUS_ADDRESS", address); }
    let status = driver
        .arg(format!("--config-file={}", config.display()))
        .arg("--")
        .arg(&binary)
        .arg("--nested-driver")
        .arg(&directory)
        .status()?;
    if !status.success() {
        return Err(format!(
            "nested test failed: {status}; logs: {}",
            directory.display()
        )
        .into());
    }
    println!("nested test PNGs and manifest: {}", directory.display());
    Ok(())
}
pub fn driver(directory: &Path) -> Result<(), Box<dyn Error>> {
    let runtime = std::env::temp_dir().join(format!("hv-{}", std::process::id()));
    fs::create_dir_all(&runtime)?;
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700))?;
    let _runtime_cleanup = RuntimeDirectory(runtime.clone());
    let fixture_mode = std::env::var_os("HYPRV_NESTED_FIXTURE").is_some();
    let scale = std::env::var("HYPRV_NESTED_SCALE")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(1.);
    let width = std::env::var("HYPRV_NESTED_WIDTH")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(1280.);
    let height = std::env::var("HYPRV_NESTED_HEIGHT")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(720.);
    let physical_width = ((width * scale).round() as u32).to_string();
    let physical_height = ((height * scale).round() as u32).to_string();
    let config = runtime.join("hyprland.lua");
    fs::write(
        &config,
        r#"hl.monitor({output="",mode="1280x720@60",position="auto",scale=1})
hl.config({general={gaps_in=0,gaps_out=0,border_size=0},animations={enabled=false},decoration={rounding=0,dim_inactive=false,shadow={enabled=false},blur={enabled=false}},misc={disable_hyprland_logo=true,disable_splash_rendering=true,force_default_wallpaper=0,background_color=0xff000000},cursor={invisible=true},xwayland={enabled=false}})
"#.replace("1280x720",&format!("{physical_width}x{physical_height}")).replace("scale=1",&format!("scale={scale}")),
    )?;
    if let Some(dir) = std::env::var_os("HYPRV_FAKE_CONTROLS_LUA") {
        let prefix = PathBuf::from(dir).join("?.lua").to_string_lossy().into_owned() + ";";
        let original = fs::read_to_string(&config)?;
        fs::write(&config, format!("package.path = {} .. package.path\n{}\nrequire(\"hypr.controls\")\n", serde_json::to_string(&prefix)?, original))?;
    }
    let home = runtime.join("home");
    fs::create_dir_all(home.join(".config"))?;
    if fixture_mode {
        fs::write(home.join(".config/kwinrulesrc"),format!("[General]\ncount=1\n\n[1]\nDescription=Isolated fixture viewport\nwmclass=aquamarine\nwmclassmatch=1\nsize={physical_width},{physical_height}\nsizerule=2\nposition=0,0\npositionrule=2\nnoborder=true\nnoborderrule=2\n"))?;
    }
    let bus = std::env::var("DBUS_SESSION_BUS_ADDRESS")?;
    let system_bus = std::env::var("HYPRV_TEST_SYSTEM_BUS_ADDRESS").unwrap_or_else(|_| bus.clone());
    let parent_socket = runtime.join("parent");
    let mut parent = Compositor(
        Command::new("kwin_wayland")
            .args([
                "--virtual",
                "--socket",
                "parent",
                "--width",
                physical_width.as_str(),
                "--height",
                physical_height.as_str(),
                "--no-lockscreen",
                "--no-global-shortcuts",
                "--no-kactivities",
            ])
            .env("XDG_RUNTIME_DIR", &runtime)
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_CACHE_HOME", home.join(".cache"))
            .env("XDG_DATA_HOME", home.join(".local/share"))
            .env("XDG_STATE_HOME", home.join(".local/state"))
            .env("QT_QPA_PLATFORM", "offscreen")
            .env("DBUS_SYSTEM_BUS_ADDRESS", &system_bus)
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("DISPLAY")
            .stdout(fs::File::create(directory.join("parent.log"))?)
            .stderr(fs::File::create(directory.join("parent.err"))?)
            .spawn()?,
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while !parent_socket.exists() {
        if let Some(status) = parent.0.try_wait()? {
            return Err(format!("virtual KWin exited: {status}").into());
        }
        if Instant::now() > deadline {
            return Err("virtual KWin timeout".into());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let shim_source = runtime.join("nested-flush.c");
    let shim = runtime.join("nested-flush.so");
    fs::write(&shim_source, include_str!("nested-flush.c"))?;
    let compiled = Command::new("cc")
        .args(["-shared", "-fPIC"])
        .arg(&shim_source)
        .arg("-o")
        .arg(&shim)
        .arg("-ldl")
        .output()?;
    if !compiled.status.success() {
        return Err(format!(
            "nested flush shim: {}",
            String::from_utf8_lossy(&compiled.stderr)
        )
        .into());
    }
    let mut command = Command::new("Hyprland");
    command
        .args(["--config"])
        .arg(&config)
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("AQ_BACKEND", "headless")
        .env("LD_PRELOAD", &shim)
        .env("WAYLAND_DISPLAY", &parent_socket)
        .env("DBUS_SYSTEM_BUS_ADDRESS", &system_bus)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("HYPRLAND_NO_SD_VARS", "1")
        .env_remove("DISPLAY")
        .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
        .stdout(fs::File::create(directory.join("compositor.log"))?)
        .stderr(fs::File::create(directory.join("compositor.err"))?);
    if !fixture_mode {
        command.env("LIBGL_ALWAYS_SOFTWARE", "1");
    }
    let mut compositor = Compositor(command.spawn()?);
    let deadline = Instant::now() + Duration::from_secs(15);
    let socket = loop {
        if let Some(socket) = fs::read_dir(&runtime)?
            .filter_map(Result::ok)
            .find(|entry| {
                entry.file_name().to_string_lossy().starts_with("wayland-")
                    && entry.file_type().is_ok_and(|t| t.is_socket())
            })
        {
            break socket.path();
        }
        if let Some(status) = compositor.0.try_wait()? {
            return Err(format!("private compositor exited: {status}").into());
        }
        if Instant::now() > deadline {
            return Err("private compositor socket timeout".into());
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let signature = fs::read_dir(runtime.join("hypr"))?
        .filter_map(Result::ok)
        .find(|p| p.file_type().is_ok_and(|t| t.is_dir()))
        .map(|p| p.file_name())
        .ok_or("private compositor instance missing")?;
    use std::io::{Read, Write};
    let request_socket = runtime.join("hypr").join(&signature).join(".socket.sock");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let ready = (|| -> Option<bool> {
            let mut stream = std::os::unix::net::UnixStream::connect(&request_socket).ok()?;
            stream
                .set_read_timeout(Some(Duration::from_millis(200)))
                .ok()?;
            stream.write_all(b"j/monitors").ok()?;
            let mut data = String::new();
            stream.read_to_string(&mut data).ok()?;
            let value: serde_json::Value = serde_json::from_str(&data).ok()?;
            Some(!value.as_array()?.is_empty())
        })();
        if ready == Some(true) {
            break;
        }
        if Instant::now() > deadline {
            return Err("nested Hyprland did not publish an output; see compositor logs".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    if fixture_mode {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let mut stream = std::os::unix::net::UnixStream::connect(&request_socket)?;
            stream.set_read_timeout(Some(Duration::from_secs(2)))?;
            stream.write_all(b"j/monitors")?;
            let mut bytes = Vec::new();
            stream.read_to_end(&mut bytes)?;
            let monitors: serde_json::Value = serde_json::from_slice(&bytes)?;
            fs::write(
                directory.join("private-outputs.json"),
                serde_json::to_vec_pretty(&monitors)?,
            )?;
            let ready = monitors.as_array().is_some_and(|list| {
                list.len() == 1
                    && list[0]["width"].as_u64() == physical_width.parse().ok()
                    && list[0]["height"].as_u64() == physical_height.parse().ok()
                    && list[0]["scale"]
                        .as_f64()
                        .is_some_and(|s| (s - scale).abs() < 0.0001)
            });
            if ready {
                break;
            }
            if Instant::now() > deadline {
                let _ = fs::copy(
                    runtime.join("hypr").join(&signature).join("hyprland.log"),
                    directory.join("hyprland-debug.log"),
                );
                return Err("private compositor did not acknowledge exact fixture size/DPR; see private-outputs.json".into());
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
    let icon_root = std::env::var_os("HYPRV_ICON_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share/icons")
        });
    let (icon_paths, icon_fallback) = hyprv_widgets::icon_theme::search_paths();
    let run_name = format!("run-{}", std::process::id());
    let run = directory.join(&run_name);
    fs::create_dir_all(&run)?;
    let mut scenes = Vec::new();
    let phases = if fixture_mode {
        vec!["fixture"]
    } else {
        vec!["bar", "calendar", "control", "wifi", "stats", "volume"]
    };
    for phase in phases {
        let output = run.join(phase);
        fs::create_dir_all(&output)?;
        let mut child_command = Command::new(std::env::current_exe()?);
        let mut child = child_command
            .arg("--nested-child")
            .arg(&output)
            .arg(phase)
            .env("HYPRV_NESTED_TEST", "1")
            .env("HYPRV_ICON_ROOT", &icon_root)
            // Theme icons resolve against the live shell's search roots (read-only), as
            // the original shell sees them; the child's own HOME/XDG dirs are private.
            .env(
                "HYPRV_ICON_SEARCH_PATHS",
                std::env::join_paths(&icon_paths)?,
            )
            .env(
                "HYPRV_ICON_FALLBACK_PATHS",
                std::env::join_paths(&icon_fallback)?,
            )
            .env_remove("HYPRV_STARTUP_APPROVAL")
            .env("XDG_RUNTIME_DIR", &runtime)
            .env("WAYLAND_DISPLAY", &socket)
            .env("HYPRLAND_INSTANCE_SIGNATURE", &signature)
            .env("HYPRV_TEST_PRIVATE_WAYLAND", "1")
            .env("DBUS_SYSTEM_BUS_ADDRESS", &system_bus)
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_CACHE_HOME", home.join(".cache"))
            .env("XDG_STATE_HOME", home.join(".local/state"))
            .env("XDG_DATA_HOME", home.join(".local/share"))
            .env("HYPRV_CONFIG_DIR", home.join(".config/HyprV"))
            .env_remove("DISPLAY")
            .stdout(fs::File::create(output.join("shell.log"))?)
            .stderr(fs::File::create(output.join("shell.err"))?)
            .spawn()?;
        let deadline = Instant::now()
            + Duration::from_secs(
                if std::env::var("HYPRV_IDLE_TEST").as_deref() == Ok("1") {
                    60
                } else if std::env::var("HYPRV_FUNCTIONAL_TEST").as_deref() == Ok("1") {
                    1230
                } else if fixture_mode {
                    // Real black/white/black screencopy for every animation sample.
                    180
                } else {
                    12
                },
            );
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{phase}: shell timed out").into());
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        if !status.success() {
            return Err(format!(
                "{phase} shell exited {status}; see {}",
                output.join("shell.err").display()
            )
            .into());
        }
        let manifest_path = output.join("manifest.json");
        let mut manifest: serde_json::Value = if manifest_path.exists() {
            serde_json::from_slice(&fs::read(manifest_path)?)?
        } else if std::env::var("HYPRV_FUNCTIONAL_TEST").as_deref() == Ok("1") {
            let mut checkpoint: serde_json::Value =
                serde_json::from_slice(&fs::read(output.join("checkpoint.json"))?)?;
            checkpoint["error"] =
                serde_json::json!("Suite ended before completion; retained checkpoint only");
            checkpoint["pass"] = serde_json::json!(false);
            checkpoint
        } else {
            return Err("nested shell did not write a manifest".into());
        };
        if std::env::var("HYPRV_FUNCTIONAL_TEST").as_deref() == Ok("1") {
            let acknowledgement: serde_json::Value = fs::read(output.join("shutdown-ack.json"))
                .ok().and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or(serde_json::json!({"acknowledged":false,"error":"shutdown acknowledgement missing"}));
            let clean = acknowledgement["acknowledged"] == true
                && !runtime.join("hyprv/shell.sock").exists();
            if let Some(cases) = manifest["cases"].as_array_mut() {
                cases.push(serde_json::json!({"id":"ipc.shell.kill","source":"ui-bar.md §4 / hyprland-bind.lua:34","status":if clean {"PASS"} else {"FAIL"},"detail":{"acknowledgement":acknowledgement,"ipc_removed":!runtime.join("hyprv/shell.sock").exists(),"exit_code":status.code()}}));
            }
            manifest["pass"] = serde_json::json!(manifest["pass"] == true && clean);
            if !crate::laptop::active() && (std::env::var_os("HYPRV_FUNCTIONAL_FILTER").is_none()
                || std::env::var("HYPRV_FUNCTIONAL_LIFECYCLE").as_deref() == Ok("1"))
            {
                let rows = functional_lifecycle::run(
                    &mut compositor,
                    &mut command,
                    &mut child_command,
                    &runtime,
                    &socket,
                    &signature,
                    &output,
                );
                if let Some(cases) = manifest["cases"].as_array_mut() {
                    cases.extend(rows);
                }
                manifest["pass"] = serde_json::json!(
                    manifest["error"].is_null()
                        && manifest["cases"].as_array().is_some_and(
                            |v| !v.is_empty() && v.iter().all(|r| r["status"] == "PASS")
                        )
                );
            }
            write_functional_report(&manifest, directory)?;
            fs::write(
                directory.join("manifest.json"),
                serde_json::to_vec_pretty(&manifest)?,
            )?;
            if manifest["pass"] != true {
                return Err("functional matrix contains failures; see manifest.json".into());
            }
            return Ok(());
        }
        if fixture_mode {
            fs::write(
                directory.join("manifest.json"),
                serde_json::to_vec_pretty(
                    &serde_json::json!({"pass":manifest["pass"],"error":manifest["error"],"capture_manifest":format!("{run_name}/{phase}/manifest.json"),"captures":manifest["captures"]}),
                )?,
            )?;
            if !manifest["error"].is_null() {
                return Err(format!("fixture replay failed: {}", manifest["error"]).into());
            }
            return Ok(());
        }
        if manifest["surfaces"].as_array().is_none_or(|s| {
            s.len()
                < if matches!(phase, "bar" | "calendar") {
                    2
                } else {
                    3
                }
        }) {
            return Err(format!("{phase}: missing rendered surfaces").into());
        }
        scenes.push(serde_json::json!({"scene":phase,"manifest":format!("{run_name}/{phase}/manifest.json")}));
    }
    fs::write(
        directory.join("manifest.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"compositor":"Hyprland nested in private virtual KWin","provider_bus":"private D-Bus, actions disabled","scenes":scenes}),
        )?,
    )?;
    Ok(())
}

/// Fixture matrix for the actual Assembly. Display discovery is read-only; all
/// configured modes/scales below belong only to the private child compositor.
pub fn fixture_matrix(
    directory: PathBuf,
    fixtures: Vec<PathBuf>,
    theme: Option<String>,
    scale_override: Option<f64>,
    references: Option<PathBuf>,
    capture_only: bool,
) -> Result<(), Box<dyn Error>> {
    let monitors = real_outputs().unwrap_or_default();
    let mut jobs = Vec::new();
    for path in fixtures {
        let path = path.canonicalize()?;
        let fixture: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        let screens = fixture["services"]["screens"]
            .as_array()
            .ok_or("fixture has no output screens")?;
        for screen in screens {
            let name = screen["name"].as_str().ok_or("screen has no name")?;
            let width = screen["width"].as_f64().ok_or("screen width")?;
            let height = screen["height"].as_f64().ok_or("screen height")?;
            let real = monitors.iter().find(|v| v["name"] == name);
            let scale=scale_override.or_else(||real.and_then(|m|m["scale"].as_f64())).ok_or_else(||format!("output {name} not found in live read-only discovery; specify --scale for a synthetic diagnostic"))?;
            if !scale.is_finite()
                || scale <= 0.
                || !width.is_finite()
                || !height.is_finite()
                || width <= 0.
                || height <= 0.
                || width * height * scale * scale > 64. * 1024. * 1024.
            {
                return Err("invalid fixture output extent/scale".into());
            }
            let themes: Vec<&str> = theme
                .as_deref()
                .map_or_else(|| vec!["light", "dark"], |v| vec![v]);
            for theme in themes {
                if theme != "light" && theme != "dark" {
                    return Err("theme must be light or dark".into());
                }
                let stem = path.file_stem().unwrap().to_string_lossy();
                let safe_name: String = name
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || c == '-' {
                            c
                        } else {
                            '_'
                        }
                    })
                    .collect();
                jobs.push((
                    path.clone(),
                    directory.join(format!("{stem}--{theme}--{safe_name}@{scale}")),
                    name.to_string(),
                    width,
                    height,
                    scale,
                    theme.to_string(),
                ));
            }
        }
    }
    if jobs.is_empty() {
        return Err("no full-screen fixture jobs".into());
    }
    fs::create_dir_all(&directory)?;
    let references = references.unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../parity/fullscreen/reference")
    });
    let references = if references.is_absolute() {
        references
    } else {
        std::env::current_dir()?.join(references)
    };
    let mut results = Vec::new();
    for (fixture, output, name, width, height, scale, theme) in jobs {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .arg("--nested-test")
            .arg(&output)
            .env("HYPRV_NESTED_FIXTURE", &fixture)
            .env("HYPRV_NESTED_THEME", &theme)
            .env("HYPRV_NESTED_SCALE", scale.to_string())
            .env("HYPRV_NESTED_WIDTH", width.to_string())
            .env("HYPRV_NESTED_HEIGHT", height.to_string())
            .env("HYPRV_NESTED_OUTPUT", &name)
            .env("HYPRV_NESTED_REFERENCES", &references);
        let status = command.status()?;
        let manifest = fs::read(output.join("manifest.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
        let pass = status.success() && manifest.as_ref().is_some_and(|m| m["pass"] == true);
        let error = manifest
            .as_ref()
            .and_then(|m| m["error"].as_str())
            .map(str::to_owned);
        results.push(serde_json::json!({"fixture":fixture,"theme":theme,"output":name,"scale":scale,"scale_origin":if scale_override.is_some(){"explicit diagnostic override"}else{"live read-only monitor discovery"},"manifest":output.join("manifest.json"),"pass":pass,"runtime_ok":status.success()&&error.is_none(),"error":error}));
    }
    let pass = results.iter().all(|v| v["pass"] == true);
    let runtime_ok = results.iter().all(|v| v["runtime_ok"] == true);
    fs::write(
        directory.join("fullscreen-results.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"pass":pass,"capture_only":capture_only,"real_outputs":monitors,"jobs":results}),
        )?,
    )?;
    if !runtime_ok || (!capture_only && !pass) {
        return Err(format!(
            "full-screen parity did not pass; see {}",
            directory.join("fullscreen-results.json").display()
        )
        .into());
    }
    Ok(())
}
fn real_outputs() -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
    use std::io::{Read, Write};
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            let status = fs::read_to_string("/proc/self/status").ok()?;
            let uid = status
                .lines()
                .find(|v| v.starts_with("Uid:"))?
                .split_whitespace()
                .nth(1)?;
            Some(PathBuf::from(format!("/run/user/{uid}")))
        })
        .ok_or("runtime unavailable")?;
    let root = runtime.join("hypr");
    let mut sockets: Vec<_> =
        if let Some(signature) = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE") {
            vec![root.join(signature).join(".socket.sock")]
        } else {
            fs::read_dir(root)?
                .filter_map(Result::ok)
                .map(|p| p.path().join(".socket.sock"))
                .collect()
        };
    sockets.sort();
    let mut outputs = Vec::new();
    for socket in sockets {
        let Ok(mut stream) = std::os::unix::net::UnixStream::connect(socket) else {
            continue;
        };
        stream.set_read_timeout(Some(Duration::from_millis(200)))?;
        stream.write_all(b"j/monitors")?;
        let mut data = Vec::new();
        if stream.read_to_end(&mut data).is_err() {
            continue;
        }

        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&data) {
            if let Some(monitors) = value.as_array() {
                outputs.extend(monitors.iter().cloned());
            }
        }
    }
    Ok(outputs)
}

#[path = "functional/lifecycle.rs"]
mod functional_lifecycle;

fn write_functional_report(
    manifest: &serde_json::Value,
    directory: &Path,
) -> Result<(), Box<dyn Error>> {
    let cases = manifest["cases"]
        .as_array()
        .ok_or("functional manifest has no cases")?;
    let passed = cases.iter().filter(|r| r["status"] == "PASS").count();
    let mut report=format!("# Nested functional E2E — 2026-10-23\n\nCommand: `hyprv-shell --nested-functional-test target/functional-test`.\n\n{} / {} cases pass. Overall: **{}**. Raw manifest: `{}/manifest.json`.\n\nInput goes through zwlr_virtual_pointer_v1 / zwp_virtual_keyboard_v1 and native\nseat dispatch. Service actors are absent; typed production service commands are\ncaptured by a mock sink before execution. Exact ordered command arrays reject\nextra, missing, duplicate or wrong-payload actions. Local actions assert UI state\nand zero commands. Passwords below use only synthetic test credentials.\n\nOnly private KWin/Hyprland, private XDG paths and private D-Bus are used.\nOutput lifecycle mutations are sent to that instance's native IPC socket.\nMihomo is unreachable in the legacy shell (spec §7); its setup hook mounts the\npage, then native input tests its controls. Reconnect policy is explicit: a lost\nWayland connection exits cleanly; a fresh supervised process reconnects after\ncompositor restart. Automatic in-process reconnect is not implemented.\n\n| Action | Result | Spec / evidence |\n| --- | --- | --- |\n",passed,cases.len(),if manifest["pass"]==true{"PASS"}else{"FAIL"},directory.display());
    for row in cases {
        let detail = row["error"]
            .as_str()
            .unwrap_or(row["source"].as_str().unwrap_or(""));
        report.push_str(&format!(
            "| `{}` | {} | {} |\n",
            row["id"].as_str().unwrap_or("unknown"),
            row["status"].as_str().unwrap_or("FAIL"),
            detail.replace('|', "\\|").replace('\n', " ")
        ));
    }
    report.push_str("\nSource index: bar hit areas/actions `shell.qml:3771`, `shell.qml:4032`, `TrayButton.qml:46`; tray backdrop/rows `shell.qml:914`, `shell.qml:1060`; main controls `ControlPanelPopup.qml:518`, `ControlPanelPopup.qml:683`; Bluetooth `ControlPanelBluetoothDetails.qml:194`; Wi-Fi form `WifiFallback.qml:776`, `WifiFallback.qml:921`; calendar `WeekCalendarPanel.qml:365`, `WeekCalendarPanel.qml:420`, `WeekCalendarPanel.qml:520`, `WeekCalendarPanel.qml:645`; island `DynamicIsland.qml:808`, `DynamicIsland.qml:932`, `DynamicIsland.qml:1228`; IPC `shell.qml:44`, `shell.qml:3403`, `shell.qml:3423`. Inactive legacy components (`hypr-comp`, DeskflowTargetControl, ControlPanelWifiDetails) are excluded as specified.\n");
    let path = if crate::laptop::active() {
        directory.join("laptop-e2e.md")
    } else if std::env::var_os("HYPRV_FUNCTIONAL_FILTER").is_some() {
        directory.join("e2e.md")
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/verify/e2e.md")
    };
    if crate::laptop::active() { report.push_str("\nLaptop native-provider cases create additional per-case private bus/sysfs worlds and run real power actors from the suite driver. UI fixture/action-sink cases are explicitly named; they do not claim native provider integration.\n"); }
    // Engine regression matrices kept in their own files stay part of the matrix.
    if let Ok(rotation) = fs::read_to_string(path.with_file_name("e2e-rotation.md")) {
        report.push('\n');
        report.push_str(&rotation);
    }
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(path, report)?;
    Ok(())
}
