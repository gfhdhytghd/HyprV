//! Nested-compositor regression for output rotation (docs/requests/hyprv-ui.md, 2026-10-23:
//! `wp_viewport` "Box doesn't fit" crash on a wl_output transform change).
//!
//! For scales 1, 1.5 and 2 it maps a top bar (anchored left+right), a full-output
//! overlay and two popup-like overlays. It then steps the output through all eight
//! wl_output transforms (90/180/270 and the flipped ones) and back to normal. After
//! each step it forces a full repaint and asserts:
//! - no Wayland protocol error (the connection stays up);
//! - the engine's configured logical sizes match the rotated output, with width and
//!   height swapped for 90/270;
//! - the compositor reports the same layer sizes (`hyprctl -j layers`), so the viewport
//!   destination and buffer agree with the latest configure.
//!
//! Isolation: a private runtime dir, a private D-Bus (session and "system"), a
//! `kwin_wayland --virtual` parent and a nested Hyprland. The live session is never
//! contacted.
//!
//! Run: `cargo test -p hyprv-ui --test nested_rotation -- --ignored --nocapture`
//! (needs kwin_wayland, Hyprland, dbus-daemon and a GPU render node).
use hyprv_ui::{
    animation::Color,
    geometry::Rect,
    scene::{Content, Fill, Node, Rectangle, Scene},
    wayland::{output_logical_size, Anchor, Application, Engine, KeyboardInteractivity, Layer, LayerConfig, UiHandler, WlOutput},
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs,
    os::unix::fs::{FileTypeExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    rc::Rc,
    time::{Duration, Instant},
};

const PHYSICAL: (u32, u32) = (1920, 1080);
const BAR_H: u32 = 32;
const POPUP_A: (u32, u32) = (300, 200);
const POPUP_B: (u32, u32) = (360, 480);

struct Proc(Child);
impl Drop for Proc {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Nested {
    runtime: PathBuf,
    signature: String,
    output: String,
    scale: f64,
    _procs: Vec<Proc>,
}
impl Drop for Nested {
    fn drop(&mut self) {
        self._procs.clear();
        let _ = fs::remove_dir_all(&self.runtime);
    }
}
impl Nested {
    fn hyprctl(&self, args: &[&str]) -> String {
        let out = Command::new("hyprctl")
            .args(args)
            .env("XDG_RUNTIME_DIR", &self.runtime)
            .env("HYPRLAND_INSTANCE_SIGNATURE", &self.signature)
            .output()
            .expect("hyprctl");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }
    fn set_transform(&self, transform: u32) {
        let lua = format!(
            "hl.monitor({{ output = \"{}\", mode = \"{}x{}@60\", position = \"0x0\", scale = \"{}\", transform = {} }})",
            self.output, PHYSICAL.0, PHYSICAL.1, self.scale, transform
        );
        let reply = self.hyprctl(&["eval", &lua]);
        assert!(reply.trim() == "ok", "hyprctl eval transform {transform}: {reply}");
    }
    /// (transform, logical size) as Hyprland reports it.
    fn monitor(&self) -> Option<(u32, (u32, u32))> {
        let list: serde_json::Value = serde_json::from_str(&self.hyprctl(&["-j", "monitors"])).ok()?;
        let m = list.as_array()?.iter().find(|m| m["name"] == self.output.as_str())?;
        let (w, h) = (m["width"].as_f64()?, m["height"].as_f64()?);
        let t = m["transform"].as_u64()? as u32;
        let (lw, lh) = ((w / self.scale).round() as u32, (h / self.scale).round() as u32);
        Some((t, if t % 2 == 1 { (lh, lw) } else { (lw, lh) }))
    }
    /// namespace → (w, h) of our layers as the compositor sees them.
    fn layers(&self) -> BTreeMap<String, (u32, u32)> {
        let v: serde_json::Value = serde_json::from_str(&self.hyprctl(&["-j", "layers"])).unwrap_or_default();
        let mut out = BTreeMap::new();
        for (_, mon) in v.as_object().into_iter().flatten() {
            for (_, level) in mon["levels"].as_object().into_iter().flatten() {
                for l in level.as_array().into_iter().flatten() {
                    let ns = l["namespace"].as_str().unwrap_or_default();
                    if ns.starts_with("rot-") {
                        out.insert(ns.into(), (l["w"].as_u64().unwrap_or(0) as u32, l["h"].as_u64().unwrap_or(0) as u32));
                    }
                }
            }
        }
        out
    }
}
fn wait_socket(path: &Path, child: &mut Child, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !path.exists() {
        assert!(child.try_wait().unwrap().is_none(), "{what} exited early");
        assert!(Instant::now() < deadline, "{what} socket timeout");
        std::thread::sleep(Duration::from_millis(25));
    }
}
fn start(scale: f64) -> Nested {
    let runtime = std::env::temp_dir().join(format!("hvrot-{}-{}", std::process::id(), (scale * 100.) as u32));
    let _ = fs::remove_dir_all(&runtime);
    fs::create_dir_all(runtime.join("home/.config")).unwrap();
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700)).unwrap();
    let home = runtime.join("home");
    // Pin the nested compositor's window to the full virtual output (as hyprv-shell's
    // nested harness does), so the nested output really is PHYSICAL.
    fs::write(home.join(".config/kwinrulesrc"), format!("[General]\ncount=1\n\n[1]\nDescription=Nested Hyprland viewport\nwmclass=aquamarine\nwmclassmatch=1\nsize={},{}\nsizerule=2\nposition=0,0\npositionrule=2\nnoborder=true\nnoborderrule=2\n", PHYSICAL.0, PHYSICAL.1)).unwrap();
    let logs = |name: &str| fs::File::create(runtime.join(name)).unwrap();
    let bus_conf = runtime.join("bus.conf");
    fs::write(&bus_conf, format!(r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd"><busconfig><type>session</type><listen>unix:path={}/bus</listen><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>"#, runtime.display())).unwrap();
    let mut bus = Command::new("dbus-daemon").arg(format!("--config-file={}", bus_conf.display())).arg("--nofork").stdout(Stdio::null()).stderr(logs("bus.err")).spawn().expect("dbus-daemon");
    wait_socket(&runtime.join("bus"), &mut bus, "private D-Bus");
    let bus_address = format!("unix:path={}/bus", runtime.display());
    let env = |c: &mut Command| {
        c.env("XDG_RUNTIME_DIR", &runtime)
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_CACHE_HOME", home.join(".cache"))
            .env("XDG_DATA_HOME", home.join(".local/share"))
            .env("XDG_STATE_HOME", home.join(".local/state"))
            .env("DBUS_SESSION_BUS_ADDRESS", &bus_address)
            .env("DBUS_SYSTEM_BUS_ADDRESS", &bus_address)
            .env_remove("DISPLAY")
            .env_remove("HYPRLAND_INSTANCE_SIGNATURE");
    };
    let mut kwin = Command::new("kwin_wayland");
    kwin.args(["--virtual", "--socket", "parent", "--width", &PHYSICAL.0.to_string(), "--height", &PHYSICAL.1.to_string(), "--no-lockscreen", "--no-global-shortcuts", "--no-kactivities"])
        .env("QT_QPA_PLATFORM", "offscreen")
        .env_remove("WAYLAND_DISPLAY")
        .stdout(logs("kwin.log"))
        .stderr(logs("kwin.err"));
    env(&mut kwin);
    let mut kwin = kwin.spawn().expect("kwin_wayland");
    wait_socket(&runtime.join("parent"), &mut kwin, "virtual KWin");
    let config = runtime.join("hyprland.lua");
    fs::write(&config, format!(
        "hl.monitor({{output=\"\",mode=\"{}x{}@60\",position=\"0x0\",scale=\"{scale}\"}})\nhl.config({{general={{gaps_in=0,gaps_out=0,border_size=0}},animations={{enabled=false}},decoration={{rounding=0,shadow={{enabled=false}},blur={{enabled=false}}}},misc={{disable_hyprland_logo=true,disable_splash_rendering=true,disable_watchdog_warning=true}},xwayland={{enabled=false}}}})\n",
        PHYSICAL.0, PHYSICAL.1)).unwrap();
    let mut hypr = Command::new("Hyprland");
    hypr.arg("--config").arg(&config).env("WAYLAND_DISPLAY", runtime.join("parent")).env("HYPRLAND_NO_SD_VARS", "1").stdout(logs("hyprland.log")).stderr(logs("hyprland.err"));
    env(&mut hypr);
    let mut hypr = hypr.spawn().expect("Hyprland");
    let deadline = Instant::now() + Duration::from_secs(20);
    let signature = loop {
        assert!(hypr.try_wait().unwrap().is_none(), "nested Hyprland exited; see {}", runtime.display());
        assert!(Instant::now() < deadline, "nested Hyprland instance timeout");
        if let Some(sig) = fs::read_dir(runtime.join("hypr")).ok().and_then(|d| d.filter_map(Result::ok).find(|e| e.path().join(".socket.sock").exists())) {
            break sig.file_name().to_string_lossy().into_owned();
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut nested = Nested { runtime: runtime.clone(), signature, output: String::new(), scale, _procs: vec![Proc(bus), Proc(kwin), Proc(hypr)] };
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let list: serde_json::Value = serde_json::from_str(&nested.hyprctl(&["-j", "monitors"])).unwrap_or_default();
        if let Some(m) = list.as_array().and_then(|l| l.first()) {
            nested.output = m["name"].as_str().unwrap_or_default().into();
            if m["width"].as_u64() == Some(PHYSICAL.0 as u64) && m["height"].as_u64() == Some(PHYSICAL.1 as u64) && m["scale"].as_f64().is_some_and(|s| (s - scale).abs() < 1e-3) {
                break;
            }
        }
        assert!(Instant::now() < deadline, "nested output never reached {PHYSICAL:?}@{scale}: {list}");
        std::thread::sleep(Duration::from_millis(50));
    }
    let socket = fs::read_dir(&runtime).unwrap().filter_map(Result::ok)
        .find(|e| e.file_name().to_string_lossy().starts_with("wayland-") && e.file_type().is_ok_and(|t| t.is_socket()))
        .expect("nested Wayland socket").path();
    // The client below connects through these; both point into the private runtime dir.
    std::env::set_var("XDG_RUNTIME_DIR", &runtime);
    std::env::set_var("WAYLAND_DISPLAY", socket);
    nested
}

#[derive(Default)]
struct Probe {
    mapped: bool,
    /// namespace → (logical size, scale) from the engine after the latest configure.
    sizes: BTreeMap<String, ((u32, u32), f64)>,
    /// `output_logical_size` of the engine's latest MonitorInfo (rotation-aware).
    output: Option<(i32, i32)>,
}
struct Handler(Rc<RefCell<Probe>>);
fn filled(color: Color) -> Scene {
    Scene::new(Node::new(
        Rect::new(0., 0., 4096., 4096.),
        Content::Rect(Rectangle { fill: Fill::Solid(color), radii: [6.; 4], border: None }),
    ))
}
impl UiHandler for Handler {
    fn update_after_frame(&self) -> bool { false }
    fn immediate_static_frames(&self) -> bool { true }
    fn output_added(&mut self, _: &WlOutput, _: Option<hyprv_ui::wayland::MonitorInfo>) -> Vec<(LayerConfig, Scene)> {
        let mut p = self.0.borrow_mut();
        if p.mapped {
            return Vec::new();
        }
        p.mapped = true;
        let bar = LayerConfig { namespace: "rot-bar".into(), layer: Layer::Top, anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT, size: (0, BAR_H), exclusive_zone: BAR_H as i32, ..Default::default() };
        let overlay = LayerConfig { namespace: "rot-overlay".into(), layer: Layer::Overlay, anchor: Anchor::all(), size: (0, 0), exclusive_zone: -1, input_passthrough: true, ..Default::default() };
        let popup_a = LayerConfig { namespace: "rot-popup-a".into(), ..LayerConfig::popup(POPUP_A) };
        let popup_b = LayerConfig { namespace: "rot-popup-b".into(), anchor: Anchor::TOP | Anchor::RIGHT, margins: (BAR_H as i32 + 8, 8, 0, 0), keyboard: KeyboardInteractivity::None, ..LayerConfig::popup(POPUP_B) };
        vec![
            (bar, filled(Color::rgba(0.1, 0.1, 0.2, 0.9))),
            (overlay, filled(Color::rgba(0., 0., 0., 0.05))),
            (popup_a, filled(Color::rgba(0.2, 0.4, 0.8, 0.95))),
            (popup_b, filled(Color::rgba(0.8, 0.4, 0.2, 0.95))),
        ]
    }
    fn update(&mut self, engine: &mut Engine) {
        let mut p = self.0.borrow_mut();
        p.output = engine.outputs().first().and_then(|(_, info)| info.as_ref().and_then(output_logical_size));
        for (id, config) in engine.surfaces() {
            if let Some(g) = engine.surface_geometry(id) {
                p.sizes.insert(config.namespace, g);
            }
        }
    }
}
fn expected(logical: (u32, u32)) -> BTreeMap<String, (u32, u32)> {
    BTreeMap::from([
        ("rot-bar".into(), (logical.0, BAR_H)),
        ("rot-overlay".into(), logical),
        ("rot-popup-a".into(), POPUP_A),
        ("rot-popup-b".into(), POPUP_B),
    ])
}
fn pump(app: &mut Application, for_: Duration, context: &str) {
    let end = Instant::now() + for_;
    while Instant::now() < end {
        if let Err(e) = app.dispatch(Some(Duration::from_millis(20))) {
            panic!("Wayland error {context}: {e}");
        }
    }
}

fn rotate_all(scale: f64) -> Vec<String> {
    let nested = start(scale);
    let probe = Rc::new(RefCell::new(Probe::default()));
    let mut app = Application::connect(Handler(probe.clone())).expect("connect to nested Hyprland");
    let mut report = Vec::new();
    // 0 → 1 … 7 (90, 180, 270, flipped, flipped-90/180/270) → 0
    for transform in [0u32, 1, 2, 3, 4, 5, 6, 7, 0] {
        if transform != 0 || !report.is_empty() {
            nested.set_transform(transform);
        }
        let context = format!("at scale {scale}, transform {transform}");
        let deadline = Instant::now() + Duration::from_secs(5);
        let logical = loop {
            pump(&mut app, Duration::from_millis(50), &context);
            if let Some((t, logical)) = nested.monitor() {
                let sizes: BTreeMap<_, _> = probe.borrow().sizes.iter().map(|(k, v)| (k.clone(), v.0)).collect();
                let output = probe.borrow().output;
                if t == transform && sizes == expected(logical) && output == Some((logical.0 as i32, logical.1 as i32)) {
                    break logical;
                }
            }
            assert!(Instant::now() < deadline, "sizes never matched {context}: engine {:?}, engine output {:?}, monitor {:?}", probe.borrow().sizes, probe.borrow().output, nested.monitor());
        };
        // A full repaint at the new size, then give the compositor time to reject it.
        app.engine.render_now();
        pump(&mut app, Duration::from_millis(250), &context);
        assert!(app.engine.is_running(), "engine stopped {context}");
        let frames = app.engine.frames;
        let layers = nested.layers();
        assert_eq!(layers, expected(logical), "compositor layer sizes {context}");
        let scales: Vec<f64> = probe.borrow().sizes.values().map(|v| v.1).collect();
        assert!(scales.iter().all(|s| (s - scale).abs() < 1e-3), "surface scales {scales:?} {context}");
        report.push(format!("scale {scale} transform {transform}: logical {}x{}, layers ok, frames {frames}", logical.0, logical.1));
    }
    drop(app);
    report
}

#[test]
#[ignore = "nested compositor: needs kwin_wayland, Hyprland, dbus-daemon and a GPU; run with --ignored"]
fn rotation_through_all_transforms_with_popups_open() {
    let mut all = Vec::new();
    for scale in [1., 1.5, 2.] {
        all.extend(rotate_all(scale));
    }
    for line in &all {
        println!("{line}");
    }
    if let Some(out) = std::env::var_os("HYPRV_ROTATION_REPORT") {
        fs::write(out, all.join("\n") + "\n").unwrap();
    }
}

/// The same native EGL pixels, with one less callback/empty commit for each
/// static data update. Animation callbacks must still use the original clock.
#[test]
#[ignore = "private compositor; run tests/run-nested-rotation.sh"]
fn static_data_frames_preserve_pixels_and_animation_clock() {
    use hyprv_ui::{diagnostics, scene::Text, wayland::{FrameInfo, SurfaceId}};
    use std::cell::Cell;
    struct StaticHandler {
        immediate: bool,
        remaining: Rc<Cell<u32>>,
        hold: Rc<Cell<bool>>,
        times: Rc<RefCell<Vec<(Duration, Instant)>>>,
    }
    impl UiHandler for StaticHandler {
        fn update_after_frame(&self) -> bool { false }
        fn immediate_static_frames(&self) -> bool { self.immediate }
        fn resource_transition_active(&self) -> bool { self.hold.get() }
        fn presentation_feedback(&self, _: SurfaceId) -> bool { false }
        fn frame(&mut self, _: SurfaceId, _: &mut Scene, t: FrameInfo) -> bool {
            self.times.borrow_mut().push((t.time, Instant::now()));
            let next = self.remaining.get().saturating_sub(1);
            self.remaining.set(next);
            next > 0
        }
    }
    assert!(diagnostics::enabled(), "run with HYPRV_UI_PROFILE=1 in the private launcher");
    let nested = start(1.5);
    let mut captures = Vec::new();
    let mut requests = Vec::new();
    let mut screens = Vec::new();
    for (immediate,direct,retire) in [(false,false,false),(true,false,false),(true,true,false),(true,true,true)] {
        let remaining = Rc::new(Cell::new(0));
        let hold = Rc::new(Cell::new(false));
        let times = Rc::new(RefCell::new(Vec::new()));
        let mut app = Application::connect(StaticHandler {
            immediate, remaining: remaining.clone(), hold: hold.clone(), times: times.clone(),
        }).unwrap();
        if !retire { app.engine.retain_idle_drawables_for_test(); }
        let mut root = Node::new(Rect::new(0., 0., 320., 64.), Content::Rect(Rectangle {
            fill: Fill::Solid(Color::rgba(0.15, 0.17, 0.2, 0.8)), radii: [18.; 4], border: None,
        }));
        root.native_materials = Some(true);
        let mut text = Node::new(Rect::new(12., 12., 290., 40.), Content::Text(Text {
            text: "Value 10".into(), size: 16., ..Default::default()
        }));
        text.native_text_mode = hyprv_ui::scene::NativeTextMode::QtGl;
        let label = root.add(text);
        let id = app.engine.add_surface(LayerConfig {
            namespace: "static-frame-test".into(), size: (320, 64), full_redraw: direct,
            input_passthrough: true, ..Default::default()
        }, Scene::new(root));
        pump(&mut app, Duration::from_millis(350), "static frame initial configure");
        remaining.set(3);
        app.engine.animate(id);
        pump(&mut app, Duration::from_millis(200), "first animation");
        assert_eq!(remaining.get(), 0);
        diagnostics::clear();
        let mut images = Vec::new();
        let mut composed = Vec::new();
        let start = app.engine.frames;
        for value in 11..=18 {
            app.engine.scene(id).unwrap().edit(label, |n| {
                let Content::Text(text) = &mut n.content else { unreachable!() };
                text.text = format!("Value {value}");
            });
            pump(&mut app, Duration::from_millis(80), "static data update");
            let dir = nested.runtime.join(format!("capture-{immediate}-{direct}-{retire}-{value}"));
            let capture = app.engine.dump_surfaces(&dir).unwrap().remove(0);
            images.push(fs::read(dir.join(capture.file)).unwrap());
            // Read the compositor's last attached buffer after drawable retirement,
            // not merely a redraw/readback of our retained scene.
            let shot = dir.join("composed.png");
            let socket = fs::read_dir(&nested.runtime).unwrap().filter_map(Result::ok)
                .find(|e| e.file_name().to_string_lossy().starts_with("wayland-")
                    && e.file_type().is_ok_and(|t|t.is_socket())).unwrap().path();
            assert!(Command::new("grim").args(["-o",&nested.output]).arg(&shot)
                .env("XDG_RUNTIME_DIR",&nested.runtime).env("WAYLAND_DISPLAY",socket)
                .status().unwrap().success());
            composed.push(fs::read(shot).unwrap());
        }
        assert_eq!(app.engine.frames - start, 8, "no repeated static paints");
        let csv = diagnostics::csv();
        let callbacks = csv.lines().find_map(|line| {
            let fields: Vec<_> = line.split(',').collect();
            (fields.get(1) == Some(&"engine.frame_callback_requested"))
                .then(|| fields[4].parse::<u64>().unwrap())
        }).unwrap_or(0);
        assert_eq!(callbacks, if immediate { 0 } else { 8 });
        requests.push(callbacks);
        captures.push(images);
        screens.push(composed);
        if retire {
            assert!(csv.lines().any(|line|line.starts_with("counter,egl.idle_drawable_retired,")),
                "static direct windows must retire their swapchain without a commit");
        }
        if retire {
            fn retired() -> u64 {
                diagnostics::csv().lines().find_map(|l| {
                    let p:Vec<_>=l.split(',').collect();
                    (p.get(1)==Some(&"egl.idle_drawable_retired")).then(||p[4].parse().unwrap())
                }).unwrap_or(0)
            }
            hold.set(true);
            app.engine.scene(id).unwrap().invalidate();
            pump(&mut app,Duration::from_millis(80),"host transition holds direct drawable");
            let before=retired(); let frames=app.engine.frames;
            hold.set(false);
            pump(&mut app,Duration::from_millis(40),"transition ends without new damage");
            assert_eq!(retired(),before+1,"release the held drawable at the transition boundary");
            assert_eq!(app.engine.frames,frames,"retirement itself must not paint or request a frame");
        }
        remaining.set(3);
        app.engine.animate(id);
        pump(&mut app, Duration::from_millis(200), "animation after static updates");
        assert_eq!(remaining.get(), 0);
        for pair in times.borrow().windows(2) {
            let clock = pair[1].0.checked_sub(pair[0].0).expect("monotonic frame clock");
            let elapsed = pair[1].1.duration_since(pair[0].1);
            assert!(clock.abs_diff(elapsed) < Duration::from_millis(35),
                "static drawing moved the animation callback anchor: clock={clock:?}, elapsed={elapsed:?}");
        }
    }
    assert_eq!(captures[0], captures[1], "callback removal changed native EGL pixels");
    assert_eq!(captures[2], captures[3], "idle drawable retirement changed EGL pixels");
    fn pixels(png: &[u8]) -> Vec<u8> {
        let image = hyprv_ui::skia::Image::from_encoded(hyprv_ui::skia::Data::new_copy(png)).unwrap();
        let mut bytes = vec![0;image.width() as usize*image.height() as usize*4];
        assert!(image.read_pixels(&hyprv_ui::skia::ImageInfo::new(image.dimensions(),
            hyprv_ui::skia::ColorType::RGBA8888,hyprv_ui::skia::AlphaType::Premul,None),
            &mut bytes,image.width() as usize*4,(0,0),hyprv_ui::skia::image::CachingHint::Disallow));
        bytes
    }
    for (index,(a,b)) in screens[2].iter().zip(&screens[3]).enumerate() {
        let (a,b) = (pixels(a),pixels(b));
        let changed = a.chunks_exact(4).zip(b.chunks_exact(4)).enumerate()
            .filter_map(|(i,(a,b))|(a!=b).then_some(i)).collect::<Vec<_>>();
        let max = a.iter().zip(&b).map(|(a,b)|a.abs_diff(*b)).max().unwrap();
        if !changed.is_empty() {
            if let Some(path) = std::env::var_os("HYPRV_ROTATION_REPORT") {
                fs::write(path,serde_json::to_vec(&serde_json::json!({
                    "expected_png":screens[2][index],"actual_png":screens[3][index],
                    "changed":changed.len(),"max":max
                })).unwrap()).unwrap();
            }
        }
        assert!(changed.is_empty(),"retired drawable compositor sample {index}: {} changed pixels, max {max}, first {:?}",
            changed.len(),changed.first().map(|i|(i%PHYSICAL.0 as usize,i/PHYSICAL.0 as usize)));
    }
    println!("static frames: 8/8 native PNGs byte-identical; callback requests {} -> {}; animation clock preserved", requests[0], requests[1]);
}
