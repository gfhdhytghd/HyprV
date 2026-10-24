//! Fixture replay mirroring `parity/host/main.cpp`: 60 Hz virtual ticks (floored ms for
//! animations), props applied then 1500 ms settle, pointer parked in the canvas corner,
//! then the fixture steps. Renders offscreen and diffs against `parity/reference/island`.
#![allow(dead_code)]
// Production GPU path (parity QA's EGL helper, HYPRV_PARITY_BACKEND=gl); CPU otherwise.
use hyprv_ui::{geometry::Rect, render::Renderer, scene::{Content, Node, Scene}, skia as sk};
use hyprv_ui_island::{Agent, Inputs, Island, Media, Pointer};
use serde_json::Value;
use std::{path::{Path, PathBuf}, time::Duration};

mod qt_png;

pub const TICK_MS: f64 = 1000. / 60.;

pub fn rshell() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
pub fn fixtures() -> Vec<PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(std::env::var_os("HYPRV_ISLAND_FIXTURE_ROOT").map(PathBuf::from).unwrap_or_else(||rshell().join("parity/fixtures/island"))).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")).collect();
    v.sort();
    v
}

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
    let info = sk::ImageInfo::new((w as i32, h as i32), sk::ColorType::RGBA8888, sk::AlphaType::Unpremul, None);
    let mut px = vec![0u8; w * h * 4];
    assert!(image.read_pixels(&info, &mut px, w * 4, (0, 0), sk::image::CachingHint::Allow));
    Img { w, h, px }
}

#[derive(Clone, Copy, Debug)]
pub struct Diff {
    pub mean: f64,
    pub bad: f64,
    pub max: u8,
}
/// Premultiplied mean absolute channel difference and fraction of pixels > 48.
pub fn diff(a: &Img, b: &Img, heat: Option<&Path>) -> Diff {
    assert_eq!((a.w, a.h), (b.w, b.h));
    let n = a.w * a.h;
    let (mut sum, mut bad, mut max) = (0f64, 0usize, 0u8);
    let mut out = vec![0u8; n * 4];
    for i in 0..n {
        let pa = &a.px[i * 4..i * 4 + 4];
        let pb = &b.px[i * 4..i * 4 + 4];
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
        max = max.max(m.round() as u8);
        let v = (m * 4.).min(255.) as u8;
        out[i * 4..i * 4 + 4].copy_from_slice(&[v, v, v, 255]);
    }
    if let Some(p) = heat {
        Img { w: a.w, h: a.h, px: out }.save(p);
    }
    Diff { mean: sum / (n * 4) as f64, bad: bad as f64 / n as f64, max }
}

pub struct Capture {
    pub file: String,
    pub img: Img,
}

pub struct Runner {
    pub island: Island,
    pub t: f64,
    pub w: f32,
    pub h: f32,
    pub scale: f64,
    pub mouse: Option<(f32, f32)>,
    pub state: String,
    pub theme: &'static str,
    pub captures: Vec<Capture>,
    pub calendar: Calendar,
}

/// Minimal host for the calendar morph (`parity/scenes/island.qml` centerSection).
pub struct Calendar {
    pub visible: bool,
    pub presented: bool,
    pub progress: hyprv_ui_island::util::Anim,
    pub progress_target_pending: Option<f64>,
    pub close_at: Option<f64>,
    /// centerSection width/height/radius Behaviors (enabled only while presented).
    pub cw: hyprv_ui_island::util::Anim,
    pub ch: hyprv_ui_island::util::Anim,
    pub cr: hyprv_ui_island::util::Anim,
}

impl Default for Calendar {
    fn default() -> Self {
        Self { visible: false, presented: false, progress: hyprv_ui_island::util::Anim::new(210, hyprv_ui_island::util::out_cubic()), progress_target_pending: None, close_at: None,
            cw: hyprv_ui_island::util::Anim::new(210, hyprv_ui_island::util::out_cubic()),
            ch: hyprv_ui_island::util::Anim::new(210, hyprv_ui_island::util::out_cubic()),
            cr: hyprv_ui_island::util::Anim::new(210, hyprv_ui_island::util::out_cubic()) }
    }
}

fn now(t: f64) -> Duration {
    Duration::from_millis(t.floor() as u64)
}

pub fn resolve(v: &str) -> String {
    match v.strip_prefix("@asset:") {
        Some(f) => format!("file://{}", rshell().join("parity/fixtures/assets").join(f).display()),
        None => v.to_owned(),
    }
}
fn load_art(url: &str) -> Option<sk::Image> {
    let path = url.strip_prefix("file://")?;
    sk::Image::from_encoded(sk::Data::new_copy(&std::fs::read(path).ok()?))
}

pub fn apply_props(inputs: &mut Inputs, props: &Value) {
    let obj = props.as_object().unwrap();
    for (k, v) in obj {
        let s = || v.as_str().unwrap_or_default().to_owned();
        let f = || v.as_f64().unwrap_or(0.);
        let md: &mut Media = &mut inputs.media;
        match k.as_str() {
            "now" => inputs.now_hh_mm = s(),
            "mediaAvailable" => md.available = v.as_bool().unwrap_or(false),
            "mediaPlaying" => md.playing = v.as_bool().unwrap_or(false),
            "mediaTitle" => md.title = s(),
            "mediaArtist" => md.artist = s(),
            "mediaPlayerName" => md.player = s(),
            "mediaArtUrl" => {
                md.art_url = resolve(&s());
                inputs.art = load_art(&md.art_url);
            }
            "mediaLengthSeconds" => md.length_s = f(),
            "mediaPositionSeconds" => md.position_s = f(),
            "spectrumValues" => inputs.spectrum = v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap_or(0.) as f32).collect(),
            "agentSessions" => {
                inputs.agents = v.as_array().unwrap().iter().map(|a| {
                    let g = |k: &str| a.get(k).and_then(Value::as_str).unwrap_or_default().to_owned();
                    Agent {
                        id: g("id"), source: g("source"), source_label: g("source_label"), title: g("title"),
                        project: g("project"), status: g("status"), current_tool: g("current_tool"), detail: g("detail"),
                        progress: a.get("progress").and_then(Value::as_f64),
                    }
                }).collect()
            }
            _ => {}
        }
    }
}

impl Runner {
    pub fn new(fixture: &Value, dark: bool, scale: f64, state: &str) -> Self {
        let canvas = &fixture["canvas"];
        let w = canvas["width"].as_f64().unwrap_or(640.) as f32;
        let h = canvas["height"].as_f64().unwrap_or(80.) as f32;
        let mut inputs = Inputs { dark, ..Default::default() };
        apply_props(&mut inputs, &fixture["props"]);
        let mut island = Island::new(inputs, w, Duration::ZERO);
        island.clip_hover_to_input_region = false; // the Qt harness has no input region
        let mut r = Self {
            island, t: 0., w, h, scale, mouse: None, state: state.into(), theme: if dark { "dark" } else { "light" },
            captures: Vec::new(), calendar: Calendar::default(),
        };
        let settle = fixture.get("settle_ms").and_then(Value::as_f64).unwrap_or(1500.);
        // Qt host's initial frame() polishes/renders without stepping its driver.
        r.advance(settle);
        r.move_mouse((w - 1., h - 1.));
        r.advance(TICK_MS);
        r
    }

    fn tick(&mut self) {
        let t = self.t;
        // Host calendar (scene island.qml): callLater(progress = 1) lands on the next turn.
        if let Some(target) = self.calendar.progress_target_pending.take() {
            self.calendar.progress.set(target, now(t));
        }
        if self.calendar.close_at.is_some_and(|c| t + 1e-9 >= c) {
            self.calendar.close_at = None;
            if self.calendar.visible {
                self.calendar.progress.set(1., now(t));
            } else {
                self.calendar.progress.set(0., now(t));
                self.calendar.presented = false;
            }
        }
        self.sync_calendar();
        self.island.frame(now(t));
        self.sync_container();
    }

    /// centerSection size/radius bindings (`parity/scenes/island.qml`).
    fn sync_container(&mut self) {
        let t = now(self.t);
        let g = self.island.geometry();
        let c = &mut self.calendar;
        for a in [&mut c.cw, &mut c.ch, &mut c.cr] {
            a.enabled = c.presented;
        }
        c.cw.set(if c.presented { 520f64.min(3072. - 48.) } else { g.width as f64 }, t);
        c.ch.set(if c.presented { 500_f64 } else { g.height as f64 }, t);
        c.cr.set(if c.visible { 30. } else { 24. }, t);
    }

    fn sync_calendar(&mut self) {
        let mut inputs = self.island.inputs().clone();
        let p = self.calendar.progress.get(now(self.t)) as f32;
        if inputs.calendar_open != self.calendar.presented || inputs.calendar_progress != p {
            inputs.calendar_open = self.calendar.presented;
            inputs.calendar_progress = p;
            self.island.set_inputs(inputs, now(self.t));
        }
    }

    pub fn advance(&mut self, ms: f64) {
        let target = self.t + ms;
        while self.t + 1e-9 < target {
            self.t = target.min(self.t + TICK_MS);
            self.tick();
        }
    }

    fn send(&mut self, ev: Pointer) -> Vec<hyprv_ui_island::IslandAction> {
        let out = self.island.pointer(ev, now(self.t));
        for a in &out.actions {
            match a {
                hyprv_ui_island::IslandAction::ToggleCalendar => self.set_calendar(!self.calendar.visible),
                hyprv_ui_island::IslandAction::PlayPause => {
                    let mut i = self.island.inputs().clone();
                    i.media.playing = !i.media.playing;
                    self.island.set_inputs(i, now(self.t));
                }
                hyprv_ui_island::IslandAction::Seek(s) => {
                    let mut i = self.island.inputs().clone();
                    let len = i.media.length_s;
                    i.media.position_s = if len > 0. { s.clamp(0., len) } else { s.max(0.) };
                    self.island.set_inputs(i, now(self.t));
                }
                _ => {}
            }
        }
        out.actions
    }

    fn set_calendar(&mut self, on: bool) {
        if self.calendar.visible == on {
            return;
        }
        self.calendar.visible = on;
        if self.calendar.progress.target().is_none() {
            self.calendar.progress.set(0., now(self.t));
        }
        if on {
            self.calendar.close_at = None;
            self.calendar.presented = true;
            // Qt.callLater runs before the next frame (same processEvents turn).
            self.calendar.progress.set(1., now(self.t));
        } else {
            self.calendar.progress.set(0., now(self.t));
            if self.calendar.presented {
                self.calendar.close_at = Some(self.t + 210.);
            }
        }
        self.sync_calendar();
        self.sync_container();
    }

    pub fn move_mouse(&mut self, p: (f32, f32)) {
        self.mouse = Some(p);
        self.send(Pointer::Motion { x: p.0, y: p.1 });
    }

    /// `{item: "island", x, y}` → floored canvas coordinates; `[x, y]` canvas.
    fn point(&self, v: &Value) -> (f32, f32) {
        if let Some(a) = v.as_array() {
            return (a[0].as_f64().unwrap() as f32, a[1].as_f64().unwrap() as f32);
        }
        let (ox, oy) = self.island.origin();
        let x = v["x"].as_f64().unwrap_or(0.) as f32;
        let y = v["y"].as_f64().unwrap_or(0.) as f32;
        ((ox + x).floor(), (oy + y).floor())
    }

    pub fn run_steps(&mut self, steps: &Value) {
        for step in steps.as_array().unwrap() {
            if let Some(set) = step.get("set") {
                if let Some(c) = set.get("calendar") {
                    self.set_calendar(c.as_bool().unwrap_or(false));
                }
                let mut i = self.island.inputs().clone();
                apply_props(&mut i, set);
                self.island.set_inputs(i, now(self.t));
            }
            if let Some(v) = step.get("hover") {
                let p = self.point(v);
                self.move_mouse(p);
            }
            if let Some(v) = step.get("press") {
                let p = self.point(v);
                self.move_mouse(p);
                self.send(Pointer::Press { x: p.0, y: p.1 });
            }
            if let Some(v) = step.get("click") {
                let p = self.point(v);
                self.move_mouse(p);
                self.send(Pointer::Press { x: p.0, y: p.1 });
                self.send(Pointer::Release { x: p.0, y: p.1 });
            }
            if let Some(v) = step.get("drag") {
                let to = self.point(v);
                let from = self.mouse.unwrap_or(to);
                for i in 1..=12 {
                    let k = i as f32 / 12.;
                    let p = (from.0 + (to.0 - from.0) * k, from.1 + (to.1 - from.1) * k);
                    self.mouse = Some(p);
                    self.send(Pointer::Motion { x: p.0, y: p.1 });
                    self.advance(TICK_MS);
                }
            }
            if let Some(v) = step.get("wheel") {
                let p = self.point(v);
                self.move_mouse(p);
                let dy = v.get("dy").and_then(Value::as_f64).unwrap_or(-120.) as f32;
                self.send(Pointer::Wheel { x: p.0, y: p.1, angle_y: dy });
            }
            if let Some(v) = step.get("release") {
                let p = if v.is_boolean() { self.mouse.unwrap() } else { self.point(v) };
                if Some(p) != self.mouse {
                    self.move_mouse(p);
                }
                self.send(Pointer::Release { x: p.0, y: p.1 });
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
                let name = f["name"].as_str().unwrap().to_owned();
                for at in f["at"].as_array().unwrap() {
                    let at = at.as_f64().unwrap();
                    if start + at > self.t {
                        self.advance(start + at - self.t);
                    }
                    self.capture(&format!("{name}-t{:04}", at as i64));
                }
            }
        }
    }

    pub fn scene(&self) -> Node {
        let mut root = Node::new(Rect::new(0., 0., self.w, self.h), Content::Group);
        root.native_materials = Some(true);
        let g = self.island.geometry();
        let mut wrap = Node::new(g.rect, Content::Group);
        let p = self.island.inputs().calendar_progress;
        wrap.opacity = 1. - p;
        wrap.visible = wrap.opacity > 0.01;
        let mut inner = self.island.build();
        inner.frame.x = 0.;
        inner.frame.y = 0.;
        wrap.add(inner);
        root.add(calendar_container(self));
        root.add(wrap);
        root
    }

    pub fn capture(&mut self, name: &str) {
        if std::env::var_os("ISLAND_TRACE_STATE").is_some() {
            println!("ISLAND_HOST {name} t={} progress={:.17} cw={:.17} ch={:.17} radius={:.17}",self.t,self.calendar.progress.get(now(self.t)),self.calendar.cw.get(now(self.t)),self.calendar.ch.get(now(self.t)),self.calendar.cr.get(now(self.t)));
        }
        let scene = Scene::new(self.scene());
        if std::env::var_os("ISLAND_TRACE_GEOMETRY").is_some() {
            fn trace(n:&Node,x:f32,y:f32,alpha:f32,name:&str) {
                let(x,y,alpha)=(x+n.frame.x,y+n.frame.y,alpha*n.opacity);
                if let Content::Text(t)=&n.content {println!("RUST_TEXT {name} {} x{x:.17} y{y:.17} w{} h{} px{} alpha{alpha}",t.text,n.frame.width,n.frame.height,t.size);}
                if let Content::Rect(r)=&n.content {if n.frame.width>=90. && n.frame.height>=38. {println!("RUST_FRAME {name} x={x:.17} y={y:.17} w={:.17} h={:.17} radius={:.17} opacity={alpha:.17} visible={}",n.frame.width,n.frame.height,r.radii[0],n.visible);}}
                for c in &n.children {trace(c,x,y,alpha,name);}
            }
            trace(scene.root(),0.,0.,1.,name);
        }
        let mut r = Renderer::new();
        let img = qt_png::capture(&r.image(&scene, self.w, self.h, self.scale).expect("offscreen render"));
        let mut file = self.state.clone();
        if !name.is_empty() {
            file += "--";
            file += name;
        }
        let tag = if self.scale == 1. { "1".to_string() } else { format!("{}", self.scale) };
        file += &format!("--{}@{}.png", self.theme, tag);
        self.captures.push(Capture { file, img });
    }
}

/// centerSection fill/border while `calendarProgress > 0.001` (host morph, owned by the
/// bar crate; emulated here so the calendar fixtures check the island fade in context).
fn calendar_container(r: &Runner) -> Node {
    let c = &r.calendar;
    let t = now(r.t);
    let p = c.progress.get(t);
    if p <= 0.001 {
        return Node::new(Rect::new(0., 0., 0., 0.), Content::Group);
    }
    let pal = hyprv_widgets::foundations::Palette::new(r.theme == "dark");
    let (cw, ch) = (c.cw.get(t) as f32, c.ch.get(t) as f32);
    let off = if c.presented { 0. } else { r.island.geometry().attached_center_offset };
    let x = hyprv_ui::geometry::qt_center_half(r.w) + off - hyprv_ui::geometry::qt_center_half(cw);
    let border = hyprv_widgets::foundations::with_alpha(pal.primary_text, if pal.dark { 0.11 } else { 0.09 });
    let mut node=hyprv_ui_island::util::qt_rect(Rect::new(x, 10., cw, ch), c.cr.get(t) as f32, pal.module_background, Some((1., border)));
    node.clip=Some([0.;4]); // centerSection.clip: true also clips its own AA fringe.
    node
}

pub fn run_fixture(path: &Path, dark: bool, scale: f64) -> Vec<Capture> {
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let state = path.file_stem().unwrap().to_string_lossy().to_string();
    let mut r = Runner::new(&fixture, dark, scale, &state);
    r.run_steps(&fixture["steps"]);
    if !fixture["steps"].as_array().unwrap().iter().any(|s| s.get("capture").is_some() || s.get("frames").is_some()) {
        r.capture("");
    }
    r.captures
}
