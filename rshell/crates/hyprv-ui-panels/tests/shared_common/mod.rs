//! Replay of `parity/fixtures/panels/*.json` against this crate's public API, mirroring
//! `parity/host/main.cpp`: virtual 60 Hz clock (or the fixture's `tick_ms`), animations
//! read floored ms, initial props then `settle_ms`, pointer parked in the output corner,
//! then the steps. Output: the fixture's `crop` of the composited output, compared with
//! `parity/reference/panels`. See docs/requests/hyprv-ui-panels.md ("Approved") for the
//! mapping rules (bar-level triggers, target lookup, calendar → piece B).
#![allow(dead_code)]
pub mod translate;

// Production GPU path (parity QA's EGL helper, HYPRV_PARITY_BACKEND=gl); CPU otherwise.
use hyprv_ui::offscreen;
mod qt_png;
use qt_png::qt_unpremultiply_rows;
use hyprv_ui::{
    geometry::Rect,
    render::Renderer,
    scene::{Content, Node, Scene},
    skia::{self as sk},
};
use hyprv_ui_panels::{
    control_panel::Page,
    model::{Action, Anchor, HostAction, Model, PanelKind},
    startup::StartupConfirm,
    ui::{Button, Pointer},
    Panels,
};
use serde_json::Value;
use std::{path::{Path, PathBuf}, time::Duration};
use translate::Translator;

pub fn rshell() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
pub fn fixtures() -> Vec<PathBuf> {
    fixtures_in("parity/fixtures/panels")
}
pub fn fixtures_in(dir: &str) -> Vec<PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(rshell().join(dir)).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")).collect();
    v.sort();
    v
}

/// Fixtures whose crop is the bar's centre morph (island + calendar): validated by
/// piece B (`hyprv-ui-island/tests/calendar_morph.rs`), which owns the island.
pub fn is_calendar_fixture(stem: &str) -> bool {
    stem.starts_with("calendar-")
}

/// The bar pill a popup opens from (`GroupPill`/bar items: y 10, height 38). Only the
/// bottom matters to the popups (`below_or_above(bottom + 10)` → cards at y 58).
const BAR_ANCHOR: Anchor = Anchor { x: 0., y: 10., width: 0., height: 38. };

pub struct Img {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}
impl Img {
    pub fn load(path: &Path) -> Option<Img> {
        let data = std::fs::read(path).ok()?;
        let image = sk::Image::from_encoded(sk::Data::new_copy(&data))?;
        Some(to_img(&image))
    }
    pub fn save(&self, path: &Path) {
        let info = sk::ImageInfo::new((self.w as i32, self.h as i32), sk::ColorType::RGBA8888, sk::AlphaType::Unpremul, None);
        let image = sk::images::raster_from_data(&info, sk::Data::new_copy(&self.px), self.w * 4).unwrap();
        let data = image.encode(None, sk::EncodedImageFormat::PNG, None).unwrap();
        std::fs::write(path, data.as_bytes()).unwrap();
    }
}
fn to_img(image: &sk::Image) -> Img {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let info = sk::ImageInfo::new((w as i32, h as i32), sk::ColorType::RGBA8888, sk::AlphaType::Premul, None);
    let mut px = vec![0u8; w * h * 4];
    assert!(image.read_pixels(&info, &mut px, w * 4, (0, 0), sk::image::CachingHint::Allow));
    qt_unpremultiply_rows(&mut px, w);
    Img { w, h, px }
}

#[derive(Clone, Copy, Debug)]
pub struct Diff {
    pub mean: f64,
    pub bad: f64,
    pub max: u8,
    /// Mean of the per-pixel max channel delta over painted pixels (either alpha > 0):
    /// the docs/ARCHITECTURE.md gate metric.
    pub painted_mean: f64,
}
/// Premultiplied mean channel delta (0..255) and fraction of pixels whose largest delta > 48.
pub fn diff(a: &Img, b: &Img, heat: Option<&Path>) -> Diff {
    let n = a.w * a.h;
    let (mut sum, mut bad, mut max) = (0f64, 0usize, 0u8);
    let (mut psum, mut painted) = (0f64, 0usize);
    let mut out = vec![0u8; n * 4];
    for i in 0..n {
        let (pa, pb) = (&a.px[i * 4..i * 4 + 4], &b.px[i * 4..i * 4 + 4]);
        let (aa, ab) = (pa[3] as f64 / 255., pb[3] as f64 / 255.);
        let mut m = 0f64;
        for c in 0..4 {
            let (x, y) = if c == 3 { (pa[3] as f64, pb[3] as f64) } else { (pa[c] as f64 * aa, pb[c] as f64 * ab) };
            let d = (x - y).abs();
            sum += d;
            m = m.max(d);
        }
        if m > 48. {
            bad += 1;
        }
        if pa[3] > 0 || pb[3] > 0 {
            psum += m;
            painted += 1;
        }
        max = max.max(m.round() as u8);
        let v = (m * 4.).min(255.) as u8;
        out[i * 4..i * 4 + 4].copy_from_slice(&[v, v, v, 255]);
    }
    if let Some(p) = heat {
        Img { w: a.w, h: a.h, px: out }.save(p);
    }
    Diff { mean: sum / (n * 4) as f64, bad: bad as f64 / n as f64, max, painted_mean: psum / painted.max(1) as f64 }
}

pub struct Capture {
    pub file: String,
    pub img: Img,
}

/// One surface that can receive input, with its built tree (output coordinates).
struct Surface {
    kind: PanelKind,
    origin: (f32, f32),
    node: Node,
}

pub struct Runner {
    pub tr: Translator,
    pub panels: Panels,
    pub startup: Option<StartupConfirm>,
    pub t: f64,
    pub tick_ms: f64,
    pub scale: f32,
    pub crop: [f32; 4],
    pub state: String,
    pub theme: &'static str,
    pub mouse: Option<(f32, f32)>,
    /// Surface that received the last press (implicit grab).
    grab: Option<PanelKind>,
    /// Mapping order (layer-shell creation order inside Overlay).
    order: Vec<PanelKind>,
    pub captures: Vec<Capture>,
    pub errors: Vec<String>,
    /// Laptop-profile fixture (`source_ref` from the laptop branch): laptop panels and
    /// the bar's `exclusiveZone` (25 compact, 58 after an IPC `bar toggle`).
    pub laptop: bool,
    /// Qt-resolved in-panel targets (`targets.json`, from `gen_targets.py`) and the
    /// index of the step being replayed.
    targets: serde_json::Map<String, Value>,
    step_index: usize,
    /// Laptop brightness probe (laptop-quickadjust.md §3, PROBE_UNGUARDED): 90 ms after a
    /// quickAdjust brightness IPC the canned `brightness_get` level replaces the preview.
    probe_at: Option<f64>,
    hw_brightness: Option<i32>,
}

fn now(t: f64) -> Duration {
    Duration::from_millis(t.floor() as u64)
}

impl Runner {
    pub fn new(path: &Path, dark: bool, scale: f32) -> Self {
        let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let state = path.file_stem().unwrap().to_string_lossy().to_string();
        let crop = fixture["crop"].as_array().map(|a| [0, 1, 2, 3].map(|i| a[i].as_f64().unwrap() as f32)).unwrap_or([0., 0., 3072., 1728.]);
        let assets = hyprv_ui_panels::fixtures::default_assets();
        let mut tr = Translator::new(&fixture, dark, scale, &rshell(), assets);
        let laptop = fixture["source_ref"].as_str().is_some_and(|r| r.contains("laptop"));
        if laptop {
            tr.model.shell.bar_exclusive_zone = 25.;
        }
        let startup = (fixture["entry"].as_str() == Some("StartupConfirm.qml")).then(|| {
            let mut s = StartupConfirm::new(dark, Duration::ZERO);
            // The offscreen QML window is unfocused: Qt draws no focus ring.
            s.set_focus_ring(false);
            s
        });
        let mut r = Self {
            tr,
            panels: Panels::with_profile(laptop),
            startup,
            t: 0.,
            tick_ms: fixture.get("tick_ms").and_then(Value::as_f64).unwrap_or(1000. / 60.),
            scale,
            crop,
            state,
            theme: if dark { "dark" } else { "light" },
            mouse: None,
            grab: None,
            order: Vec::new(),
            captures: Vec::new(),
            errors: Vec::new(),
            laptop,
            targets: serde_json::from_str::<Value>(include_str!("targets.json")).unwrap()[path.file_stem().unwrap().to_str().unwrap()].clone()
                .as_object().cloned().unwrap_or_default(),
            step_index: 0,
            probe_at: None,
            hw_brightness: fixture["processes"].as_array().into_iter().flatten()
                .find(|p| p["match"].as_str() == Some("brightness_get"))
                .and_then(|p| p["stdout"].as_str()?.trim().parse().ok()),
        };
        r.frame();
        let settle = fixture.get("settle_ms").and_then(Value::as_f64).unwrap_or(1500.);
        r.advance(settle);
        let out = &r.tr.model.output;
        let corner = (out.width - 1., out.height - 1.);
        r.mouse = Some(corner);
        r.advance(r.tick_ms);
        r
    }

    fn model(&self) -> Model {
        let mut m = self.tr.model.clone();
        m.now = now(self.t);
        m
    }

    fn frame(&mut self) {
        self.tr.advance_to(self.t);
        if self.probe_at.is_some_and(|t| self.t + 1e-9 >= t) {
            self.probe_at = None;
            if let Some(v) = self.hw_brightness {
                self.tr.model.controls.brightness_percent = v.clamp(0, 100);
            }
        }
        let m = self.model();
        let actions = self.panels.tick(&m);
        self.handle(actions);
        if let Some(s) = &mut self.startup {
            s.tick(now(self.t));
        }
        // A host builds/updates every mapped surface each presented frame; the panels
        // measure content (reveal full height) while building.
        let _ = self.surfaces();
    }

    pub fn advance(&mut self, ms: f64) {
        let target = self.t + ms;
        while self.t + 1e-9 < target {
            self.t = target.min(self.t + self.tick_ms);
            self.frame();
        }
    }

    fn handle(&mut self, actions: Vec<Action>) {
        for a in actions {
            match a {
                Action::Host(HostAction::Mapped(k)) => {
                    self.order.retain(|o| *o != k);
                    self.order.push(k);
                }
                Action::Host(HostAction::Unmapped(k)) => self.order.retain(|o| *o != k),
                Action::Host(HostAction::OpenWifiPanel) => {
                    let m = self.model();
                    self.panels.wifi.set_anchor(BAR_ANCHOR, &m.output.name);
                    let more = self.panels.wifi.open(&m);
                    self.handle(more);
                }
                _ => {}
            }
        }
    }

    /// Mapped panel surfaces in stacking order (bottom → top).
    fn surfaces(&mut self) -> Vec<Surface> {
        let m = self.model();
        let mut mapped: Vec<PanelKind> = self.panels.surfaces(&m).into_iter().map(|(k, _)| k).collect();
        mapped.sort_by_key(|k| self.order.iter().position(|o| o == k).unwrap_or(usize::MAX));
        let mut out: Vec<Surface> = mapped.into_iter().map(|k| Surface { kind: k, origin: (0., 0.), node: self.panels.build(k, &m) }).collect();
        if let Some(s) = &mut self.startup {
            let (w, h) = s.surface().size.unwrap();
            let x = ((m.output.width - w) / 2.).round();
            let y = ((m.output.height - h) / 2.).round();
            out.push(Surface { kind: PanelKind::Startup, origin: (x, y), node: s.build(self.scale) });
        }
        out
    }

    fn send(&mut self, kind: PanelKind, origin: (f32, f32), ev: Pointer) {
        let m = self.model();
        let reply = match kind {
            PanelKind::Startup => {
                let ev = shift(ev, origin);
                self.startup.as_mut().map(|s| s.pointer(ev)).map(|r| r.actions).unwrap_or_default()
            }
            k => self.panels.pointer(k, ev, &m).actions,
        };
        self.handle(reply);
    }

    /// Deliver a pointer event to the topmost surface (or the grabbing one).
    fn pointer(&mut self, ev: Pointer) {
        let surfaces = self.surfaces();
        let target = match (ev, self.grab) {
            (Pointer::Release { .. }, Some(g)) | (Pointer::Motion { .. }, Some(g)) => surfaces.iter().find(|s| s.kind == g),
            _ => surfaces.last(),
        };
        let Some(s) = target.map(|s| (s.kind, s.origin)) else { return };
        match ev {
            Pointer::Press { .. } => self.grab = Some(s.0),
            Pointer::Release { .. } => self.grab = None,
            _ => {}
        }
        self.send(s.0, s.1, ev);
    }

    fn move_mouse(&mut self, p: (f32, f32)) {
        self.mouse = Some(p);
        self.pointer(Pointer::Motion { x: p.0, y: p.1 });
    }

    /// `pointFor`: `[x,y]`, or the n-th visible Text/Image (top-to-bottom, left-to-right)
    /// in the composited Rust scene, centre + (x, y), floored. Bar-level items return None.
    fn point(&mut self, v: &Value) -> Result<Option<(f32, f32)>, String> {
        if let Some(a) = v.as_array() {
            return Ok(Some((a[0].as_f64().unwrap() as f32, a[1].as_f64().unwrap() as f32)));
        }
        let (dx, dy) = (v["x"].as_f64().unwrap_or(0.) as f32, v["y"].as_f64().unwrap_or(0.) as f32);
        let index = v["index"].as_u64().unwrap_or(0) as usize;
        let m = self.model();
        let mut hits = Vec::new();
        for s in self.surfaces() {
            let base = sk::Matrix::translate((s.origin.0, s.origin.1));
            collect(&s.node, base, 1., &mut |node, rect| {
                let hit = match (&node.content, v) {
                    (Content::Text(t), v) if v.get("text").is_some() => v["text"].as_str() == Some(t.text.as_str()),
                    (Content::Text(t), v) if v.get("contains").is_some() => t.text.contains(v["contains"].as_str().unwrap_or("\u{0}")),
                    (Content::Svg(dom), v) if v.get("image").is_some() => {
                        let needle = v["image"].as_str().unwrap_or("");
                        m.assets.svgs.iter().any(|(k, d)| k.contains(needle) && same_dom(d, dom))
                    }
                    _ => false,
                };
                if hit {
                    hits.push(rect);
                }
            });
        }
        hits.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap().then(a.x.partial_cmp(&b.x).unwrap()));
        match hits.get(index) {
            Some(r) => Ok(Some(((r.x + r.width / 2. + dx).floor(), (r.y + r.height / 2. + dy).floor()))),
            None => Ok(None),
        }
    }

    /// Bar-level targets (the bar is not part of this crate): open the matching popup.
    fn bar_trigger(&mut self, v: &Value) -> Result<(), String> {
        let m = self.model();
        let out = m.output.name.clone();
        let t = now(self.t);
        let image = v.get("image").and_then(Value::as_str).unwrap_or("");
        let text = v.get("text").or(v.get("contains")).and_then(Value::as_str).unwrap_or("");
        let actions = if image == "control-panel-" {
            self.panels.control.toggle_for(BAR_ANCHOR, &out, t)
        } else if image == "nm-signal-" {
            self.panels.wifi.set_anchor(BAR_ANCHOR, &out);
            self.panels.wifi.toggle(&m)
        } else if text.contains('%') || text == "nocon" {
            self.panels.stats.toggle_for(BAR_ANCHOR, &out, t)
        } else {
            return Err(format!("no visible target {v} in the panels scene and no bar mapping"));
        };
        self.handle(actions);
        Ok(())
    }

    fn target(&mut self, key: &str, v: &Value) -> Result<Option<(f32, f32)>, String> {
        // In-panel Text/Image: the point Qt's pointFor resolved in the reference run.
        if let Some(p) = self.targets.get(&format!("{}/{key}", self.step_index)).and_then(Value::as_array) {
            return Ok(Some((p[0].as_f64().unwrap() as f32, p[1].as_f64().unwrap() as f32)));
        }
        match self.point(v)? {
            Some(p) => Ok(Some(p)),
            None if key == "click" => self.bar_trigger(v).map(|_| None),
            None => Err(format!("step '{key}': no visible target {v}")),
        }
    }

    pub fn run_steps(&mut self, steps: &Value) {
        for (i, step) in steps.as_array().unwrap().iter().enumerate() {
            self.step_index = i;
            if let Err(e) = self.step(step) {
                self.errors.push(format!("{}: {e}", self.state));
                return;
            }
        }
    }

    fn step(&mut self, step: &Value) -> Result<(), String> {
        let left = Button::Left;
        if let Some(set) = step.get("set") {
            if let Some(ipc) = set.get("ipc").and_then(Value::as_array) {
                let (target, function) = (ipc[0].as_str().unwrap_or(""), ipc[1].as_str().unwrap_or(""));
                let m = self.model();
                let actions = match target {
                    "quickAdjust" => {
                        // Host-side legacy OSD preview (hyprv-shell `preview_brightness`):
                        // laptop step 1 with no evening, desktop step 2 (laptop-quickadjust.md §4).
                        let step = if self.laptop { 1 } else { 2 };
                        let cur = self.tr.model.controls.brightness_percent;
                        let level = match function {
                            "showBrightnessLevel" => ipc.get(2).and_then(Value::as_i64).map(|v| v as i32),
                            "showBrightnessIncrease" => Some(cur + step),
                            "showBrightnessDecrease" => Some(cur - step),
                            _ => None,
                        };
                        if let Some(v) = level.filter(|_| self.laptop) {
                            self.tr.model.controls.brightness_percent = ((v.clamp(0, 100) + step / 2) / step * step).min(100);
                        }
                        if self.laptop && function.starts_with("showBrightness") {
                            self.probe_at = Some(self.t + 90.);
                        }
                        self.panels.ipc_quick_adjust(function, &m)
                    }
                    "controlPanel" => self.panels.ipc_control_panel_toggle(&m.output.name, &m),
                    "calendar" => return Err("calendar host is piece B (hyprv-ui-island/tests/calendar_morph.rs)".into()),
                    // laptop adaptive bar: only its exclusive zone reaches the panels
                    "bar" if self.laptop && function == "toggle" => {
                        let z = &mut self.tr.model.shell.bar_exclusive_zone;
                        *z = if *z > 25. { 25. } else { 58. };
                        vec![]
                    }
                    other => return Err(format!("unmapped ipc target {other}")),
                };
                self.handle(actions);
            }
            if set.get("services").is_some() || set.get("root").is_some() {
                return Err("set services/root not supported".into());
            }
        }
        if let Some(inv) = step.get("invoke") {
            match (inv["type"].as_str(), inv["method"].as_str()) {
                (Some("ControlPanelPopup"), Some("toggleMihomoExpanded")) => self.panels.control.show_page(Page::Mihomo, now(self.t)),
                _ => return Err(format!("unmapped invoke {inv}")),
            }
        }
        if let Some(v) = step.get("hover") {
            if let Some(p) = self.target("hover", v)? {
                self.move_mouse(p);
            }
        }
        if let Some(v) = step.get("press") {
            if let Some(p) = self.target("press", v)? {
                self.move_mouse(p);
                self.pointer(Pointer::Press { x: p.0, y: p.1, button: left });
            }
        }
        if let Some(v) = step.get("click") {
            if let Some(p) = self.target("click", v)? {
                self.move_mouse(p);
                self.pointer(Pointer::Press { x: p.0, y: p.1, button: left });
                self.pointer(Pointer::Release { x: p.0, y: p.1, button: left });
            }
        }
        if let Some(v) = step.get("drag") {
            let to = self.target("drag", v)?.ok_or("drag target missing")?;
            let from = self.mouse.unwrap_or(to);
            for i in 1..=12 {
                let k = i as f32 / 12.;
                let p = (from.0 + (to.0 - from.0) * k, from.1 + (to.1 - from.1) * k);
                self.mouse = Some(p);
                self.pointer(Pointer::Motion { x: p.0, y: p.1 });
                self.advance(self.tick_ms);
            }
        }
        if let Some(v) = step.get("release") {
            let p = if v.is_boolean() { self.mouse.ok_or("release without pointer")? } else { self.target("release", v)?.ok_or("release target")? };
            if Some(p) != self.mouse {
                self.move_mouse(p);
            }
            self.pointer(Pointer::Release { x: p.0, y: p.1, button: left });
        }
        if let Some(ms) = step.get("advance").and_then(Value::as_f64) {
            self.advance(ms);
        }
        if let Some(name) = step.get("capture").and_then(Value::as_str) {
            let name = name.to_owned();
            self.capture(&name);
        }
        if let Some(f) = step.get("frames") {
            let start = self.t;
            let name = f["name"].as_str().unwrap_or("").to_owned();
            for at in f["at"].as_array().into_iter().flatten() {
                let at = at.as_f64().unwrap_or(0.);
                if start + at > self.t {
                    self.advance(start + at - self.t);
                }
                self.capture(&format!("{name}-t{:04}", at as i64));
            }
        }
        Ok(())
    }

    pub fn capture(&mut self, name: &str) {
        let [cx, cy, cw, ch] = self.crop;
        // The GL path renders the whole output and crops afterwards, as the references were
        // captured: Qt's projection/scissor maths use the output-sized viewport.
        let full = offscreen::enabled();
        let (ow, oh) = (self.tr.model.output.width, self.tr.model.output.height);
        let (rw, rh, ox, oy) = if full { (ow, oh, 0., 0.) } else { (cw, ch, -cx, -cy) };
        let mut root = Node::new(Rect::new(0., 0., rw, rh), Content::Group);
        let mut layer = Node::new(Rect::new(ox, oy, ow, oh), Content::Group);
        for s in self.surfaces() {
            let mut g = Node::new(Rect::new(s.origin.0, s.origin.1, 0., 0.), Content::Group);
            g.add(s.node);
            layer.add(g);
        }
        root.add(layer);
        let scene = Scene::new(root);
        let mut r = Renderer::new();
        if let Some(image) = offscreen::render_image_if_enabled(&mut r, &scene, rw, rh, self.scale as f64).expect("GPU render") {
            let k = self.scale as f64;
            let (x0, y0) = ((cx as f64 * k).round() as i32, (cy as f64 * k).round() as i32);
            let (w, h) = ((cw as f64 * k).round() as i32, (ch as f64 * k).round() as i32);
            let info = sk::ImageInfo::new((w, h), sk::ColorType::RGBA8888, sk::AlphaType::Premul, None);
            let mut px = vec![0u8; w as usize * h as usize * 4];
            assert!(image.read_pixels(&info, &mut px, w as usize * 4, (x0, y0), sk::image::CachingHint::Allow));
            qt_unpremultiply_rows(&mut px, w as usize);
            let img = Img { w: w as usize, h: h as usize, px };
            let mut file = self.state.clone();
            if !name.is_empty() {
                file += "--";
                file += name;
            }
            file += &format!("--{}@{}.png", self.theme, if self.scale == 1. { "1".into() } else { format!("{}", self.scale) });
            self.captures.push(Capture { file, img });
            return;
        }
        // CPU harness: `Renderer::raster` blends like the GL framebuffer (float blend, one
        // 8-bit store per draw; hyprv-ui ENG-BLEND-1). PANELS_RASTER=f16 keeps the old
        // RGBAF16 approximation for comparison.
        let mut surface = if std::env::var("PANELS_RASTER").as_deref() == Ok("f16") {
            let (w, h) = ((cw as f64 * self.scale as f64).round() as i32, (ch as f64 * self.scale as f64).round() as i32);
            let info = sk::ImageInfo::new((w, h), sk::ColorType::RGBAF16, sk::AlphaType::Premul, None);
            let mut s = sk::surfaces::raster(&info, None, None).expect("f16");
            r.paint(s.canvas(), &scene, self.scale as f64, Rect::new(0., 0., cw, ch)).expect("paint");
            s
        } else {
            r.raster(&scene, cw, ch, self.scale as f64).expect("raster")
        };
        let img = to_img(&surface.image_snapshot());
        let mut file = self.state.clone();
        if !name.is_empty() {
            file += "--";
            file += name;
        }
        file += &format!("--{}@{}.png", self.theme, if self.scale == 1. { "1".into() } else { format!("{}", self.scale) });
        self.captures.push(Capture { file, img });
    }
}

/// Same SVG DOM object (the asset the host supplied). `svg::Dom` is skia's `RCHandle`, a
/// single non-null pointer; `NativeAccess` is not re-exported through `hyprv_ui`.
fn same_dom(a: &sk::svg::Dom, b: &sk::svg::Dom) -> bool {
    const _: () = assert!(std::mem::size_of::<sk::svg::Dom>() == std::mem::size_of::<usize>());
    // SAFETY: reads the handle's pointer value only; size checked above.
    unsafe { std::mem::transmute_copy::<sk::svg::Dom, usize>(a) == std::mem::transmute_copy::<sk::svg::Dom, usize>(b) }
}

fn shift(ev: Pointer, (ox, oy): (f32, f32)) -> Pointer {
    match ev {
        Pointer::Motion { x, y } => Pointer::Motion { x: x - ox, y: y - oy },
        Pointer::Press { x, y, button } => Pointer::Press { x: x - ox, y: y - oy, button },
        Pointer::Release { x, y, button } => Pointer::Release { x: x - ox, y: y - oy, button },
        Pointer::Wheel { x, y, angle_x, angle_y, pixel_x, pixel_y } => Pointer::Wheel { x: x - ox, y: y - oy, angle_x, angle_y, pixel_x, pixel_y },
        Pointer::Leave => Pointer::Leave,
        Pointer::Cancel => Pointer::Cancel,
    }
}

/// Walk visible nodes (accumulated opacity > 0.01) with their output-space bounds.
fn collect(node: &Node, parent: sk::Matrix, opacity: f32, f: &mut dyn FnMut(&Node, Rect)) {
    let op = opacity * node.opacity;
    if !node.visible || op <= 0.01 {
        return;
    }
    let m = sk::Matrix::concat(&parent, &sk::Matrix::concat(&sk::Matrix::translate((node.frame.x, node.frame.y)), &node.transform));
    let (r, _) = m.map_rect(sk::Rect::from_xywh(0., 0., node.frame.width, node.frame.height));
    f(node, Rect::new(r.left, r.top, r.width(), r.height()));
    for c in &node.children {
        collect(c, m, op, f);
    }
}

pub fn run_fixture(path: &Path, dark: bool, scale: f32) -> (Vec<Capture>, Vec<String>) {
    let mut r = Runner::new(path, dark, scale);
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    r.run_steps(&fixture["steps"]);
    (r.captures, r.errors)
}

