//! Piece B: the island ↔ calendar morph of the bar centre (`shell.qml:3604-3710,
//! 3879-3970`), validated against the SHARED panels captures `calendar-*`
//! (`parity/reference/panels`, crop of the full 3072×1728 desktop). Composes:
//! - the island (this crate), fed from the same fixture;
//! - the host `centerSection` morph (width/height/radius Behaviors enabled only while
//!   presented, `calendarProgress` 210 ms OutCubic started in the same event-loop turn,
//!   210 ms close hold) — the canonical behaviour the bar/shell host must implement;
//! - `hyprv_ui_panels::calendar::CalendarPanel` content, through the panels' shared
//!   fixture translator (`hyprv-ui-panels/tests/shared_common/translate.rs`).
//!
//! A missing reference or an un-replayable step FAILS.
#[path = "../../hyprv-ui-panels/tests/shared_common/translate.rs"]
mod translate;
#[path = "../../hyprv-ui-panels/tests/shared_common/qt_png.rs"]
mod qt_png;

// Production GPU path (parity QA's EGL helper, HYPRV_PARITY_BACKEND=gl); CPU otherwise.
use hyprv_ui::{geometry::Rect, render::Renderer, scene::{Content, Node, Scene}, skia as sk};
use hyprv_ui_island::{util::{out_cubic, qt_rect, Anim}, Inputs, Island, Media};
use hyprv_ui_panels::{calendar::CalendarPanel, model::Model, ui::Pointer as PanelPointer};
use hyprv_widgets::foundations::{with_alpha, Palette};
// centerSection, the island and the calendar Loader are `anchors.horizontalCenter` items:
// QQuickAnchors' alignWhenCentered (default true) rounds both halves, as the bar view does.
use hyprv_ui::geometry::qt_center_half as hcenter;
use serde_json::Value;
use std::{path::{Path, PathBuf}, time::Duration};

const MAX_MEAN: f64 = 2.0;
const MAX_BAD: f64 = 0.02;
const OUTPUT_W: f32 = 3072.;
const TICK_MS: f64 = 1000. / 60.;

fn rshell() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn now(t: f64) -> Duration {
    Duration::from_millis(t.floor() as u64)
}

struct Host {
    visible: bool,
    presented: bool,
    progress: Anim,
    close_at: Option<f64>,
    cw: Anim,
    ch: Anim,
    cr: Anim,
}

struct Run {
    tr: translate::Translator,
    island: Island,
    panel: CalendarPanel,
    host: Host,
    t: f64,
    scale: f32,
    crop: [f32; 4],
    targets: serde_json::Map<String, Value>,
    step: usize,
    captures: Vec<(String, Vec<u8>, usize, usize)>,
    state: String,
    theme: &'static str,
}

fn island_inputs(fixture: &Value, model: &Model) -> Inputs {
    let mut length = 0.;
    let mut position = 0.;
    if let Some(p) = fixture["processes"].as_array().and_then(|a| a.iter().find(|p| p["match"] == "media-status.sh")) {
        for l in p["stdout"].as_str().unwrap_or("").lines() {
            if let Some(v) = l.strip_prefix("length=") { length = v.trim().parse().unwrap_or(0.) }
            if let Some(v) = l.strip_prefix("position=") { position = v.trim().parse().unwrap_or(0.) }
        }
    }
    let spectrum = fixture["processes"].as_array().and_then(|a| a.iter().find(|p| p["match"] == "audio-spectrum.sh"))
        .and_then(|p| p["lines"].as_array()?.last()?["data"].as_str().map(str::to_owned))
        .map(|l| l.split(';').filter(|s| !s.trim().is_empty()).map(|s| s.trim().parse::<f32>().unwrap_or(0.).max(0.)).collect())
        .unwrap_or_default();
    let m = &model.media;
    Inputs {
        dark: model.theme.dark,
        now_hh_mm: model.wall.format("%H:%M").to_string(),
        media: Media {
            available: m.available, playing: m.playing, title: m.title.clone(), artist: m.artist.clone(),
            player: m.player_name.clone(), art_url: m.art_url.clone(), position_s: position, length_s: length,
        },
        spectrum: if m.available && m.playing { spectrum } else { Vec::new() },
        art: m.art.clone(),
        ..Default::default()
    }
}

impl Run {
    fn new(path: &Path, dark: bool, scale: f32) -> Self {
        let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let state = path.file_stem().unwrap().to_string_lossy().to_string();
        let crop = fixture["crop"].as_array().map(|a| [0, 1, 2, 3].map(|i| a[i].as_f64().unwrap() as f32)).unwrap();
        let tr = translate::Translator::new(&fixture, dark, scale, &rshell(), hyprv_ui_panels::fixtures::default_assets());
        let island = Island::new(island_inputs(&fixture, &tr.model), OUTPUT_W, Duration::ZERO);
        let (w, h) = CalendarPanel::host_size(OUTPUT_W, 820.);
        let targets = serde_json::from_str::<Value>(include_str!("../../hyprv-ui-panels/tests/shared_common/targets.json")).unwrap()[state.as_str()]
            .as_object().cloned().unwrap_or_default();
        let a = || Anim::new(210, out_cubic());
        let mut r = Self {
            tr, island, panel: CalendarPanel::new(w, h),
            host: Host { visible: false, presented: false, progress: a(), close_at: None, cw: a(), ch: a(), cr: a() },
            t: 0., scale, crop, targets, step: 0, captures: Vec::new(), state, theme: if dark { "dark" } else { "light" },
        };
        r.host.progress.set(0., Duration::ZERO);
        r.island.clip_hover_to_input_region = false;
        r.tick();
        r.advance(fixture["settle_ms"].as_f64().unwrap_or(1500.));
        r.advance(TICK_MS);
        r
    }

    fn model(&self) -> Model {
        let mut m = self.tr.model.clone();
        m.now = now(self.t);
        m.calendar.bridge = self.host.presented;
        // Offline, CalendarBridge.lastError stays empty until a request is sent with the link
        // down (CalendarBridge.qml:45); no fixture step does, so the pill reads 「正在连接日历…」.
        m
    }

    fn sync_island(&mut self) {
        let mut i = self.island.inputs().clone();
        i.calendar_open = self.host.presented;
        i.calendar_progress = self.host.progress.get(now(self.t)) as f32;
        self.island.set_inputs(i, now(self.t));
    }

    /// centerSection bindings (Behaviors enabled only while presented).
    fn sync_container(&mut self) {
        let t = now(self.t);
        let g = self.island.geometry();
        let (w, h) = (self.panel.width, self.panel.height);
        let host = &mut self.host;
        for a in [&mut host.cw, &mut host.ch, &mut host.cr] {
            a.enabled = host.presented;
        }
        host.cw.set(if host.presented { w as f64 } else { g.width as f64 }, t);
        host.ch.set(if host.presented { h as f64 } else { g.height as f64 }, t);
        host.cr.set(if host.visible { 30. } else { 24. }, t);
    }

    fn tick(&mut self) {
        if self.host.close_at.is_some_and(|c| self.t + 1e-9 >= c) {
            self.host.close_at = None;
            if self.host.visible {
                self.host.progress.set(1., now(self.t));
            } else {
                self.host.progress.set(0., now(self.t));
                self.host.presented = false;
            }
        }
        self.tr.advance_to(self.t);
        self.sync_island();
        self.island.frame(now(self.t));
        self.sync_container();
        let m = self.model();
        let _ = self.panel.tick(&m);
    }

    fn advance(&mut self, ms: f64) {
        let target = self.t + ms;
        while self.t + 1e-9 < target {
            self.t = target.min(self.t + TICK_MS);
            self.tick();
        }
    }

    /// `toggleCalendar` on this screen (shell.qml onCalendarVisibleChanged).
    fn toggle(&mut self) {
        let t = now(self.t);
        self.host.visible = !self.host.visible;
        if self.host.visible {
            self.host.close_at = None;
            self.host.presented = true;
            self.host.progress.set(1., t); // Qt.callLater: same event-loop turn
        } else {
            self.host.progress.set(0., t);
            if self.host.presented {
                self.host.close_at = Some(self.t + 210.);
            }
        }
        self.sync_island();
        self.sync_container();
    }

    /// Container rect (output coords) and panel origin.
    fn layout(&self) -> (Rect, (f32, f32)) {
        let t = now(self.t);
        let g = self.island.geometry();
        let (cw, ch) = (self.host.cw.get(t) as f32, self.host.ch.get(t) as f32);
        let off = if self.host.presented { 0. } else { g.attached_center_offset };
        let x = hcenter(OUTPUT_W) + off - hcenter(cw);
        let c = Rect::new(x, 10., cw, ch);
        (c, (c.x + hcenter(cw) - hcenter(self.panel.width), 10.))
    }

    fn capture(&mut self, name: &str) {
        let t = now(self.t);
        let [cx, cy, w, h] = self.crop;
        let pal = Palette::new(self.theme == "dark");
        let (c, panel_abs) = self.layout();
        let p = self.host.progress.get(t);
        // GL: render the whole output and crop, as the references were captured (Qt's
        // projection/scissor maths use the output-sized viewport).
        let full = hyprv_ui::offscreen::enabled();
        let (rw, rh, ox, oy) = if full { (OUTPUT_W, 1728., 0., 0.) } else { (w, h, -cx, -cy) };
        let mut root = Node::new(Rect::new(0., 0., rw, rh), Content::Group);
        let mut out = Node::new(Rect::new(ox, oy, OUTPUT_W, 1728.), Content::Group);
        let mut section = if p > 0.001 {
            qt_rect(c, self.host.cr.get(t) as f32, pal.module_background, Some((1., with_alpha(pal.primary_text, if pal.dark { 0.11 } else { 0.09 }))))
        } else {
            Node::new(c, Content::Group)
        };
        section.clip = Some([0.; 4]); // clip: true (rectangular)
        // Island centred in the section (same width while not presented).
        let g = self.island.geometry();
        let mut island = Node::new(Rect::new(hcenter(c.width) - hcenter(g.width), 0., g.width, g.height), Content::Group);
        island.opacity = (1. - p) as f32;
        island.visible = island.opacity > 0.01;
        let mut inner = self.island.build();
        inner.frame.x = 0.;
        inner.frame.y = 0.;
        island.add(inner);
        // DynamicIsland has `z: expanded ? 40 : 1` over the calendar Loader's z 0: the island
        // paints above the calendar (hyprv-ui-bar QUIRK_ISLAND_ABOVE_CALENDAR).
        if self.host.presented {
            let m = self.model();
            let mut cal = Node::new(Rect::new(panel_abs.0 - c.x, 0., self.panel.width, self.panel.height), Content::Group);
            cal.opacity = p as f32;
            cal.add(self.panel.build(&m, panel_abs));
            section.add(cal);
        }
        section.add(island);
        out.add(section);
        root.add(out);
        let scene = Scene::new(root);
        let mut r = Renderer::new();
        let image = r.image(&scene, rw, rh, self.scale as f64).expect("offscreen render");
        let k = self.scale as f64;
        let (x0, y0) = if full { ((cx as f64 * k).round() as i32, (cy as f64 * k).round() as i32) } else { (0, 0) };
        let (iw, ih) = if full { ((w as f64 * k).round() as usize, (h as f64 * k).round() as usize) } else { (image.width() as usize, image.height() as usize) };
        // encoded like parity-host's QImage::convertToFormat(RGBA8888) (panels qt_png)
        let info = sk::ImageInfo::new((iw as i32, ih as i32), sk::ColorType::RGBA8888, sk::AlphaType::Premul, None);
        let mut px = vec![0u8; iw * ih * 4];
        assert!(image.read_pixels(&info, &mut px, iw * 4, (x0, y0), sk::image::CachingHint::Allow));
        qt_png::qt_unpremultiply_rows(&mut px, iw);
        let mut file = self.state.clone();
        if !name.is_empty() {
            file += "--";
            file += name;
        }
        file += &format!("--{}@{}.png", self.theme, if self.scale == 1. { "1".into() } else { format!("{}", self.scale) });
        self.captures.push((file, px, iw, ih));
    }

    fn run(&mut self, steps: &Value) -> Result<(), String> {
        for (i, s) in steps.as_array().unwrap().iter().enumerate() {
            self.step = i;
            if let Some(set) = s.get("set") {
                match set.get("ipc").and_then(Value::as_array).map(|a| (a[0].as_str(), a[1].as_str())) {
                    Some((Some("calendar"), Some("toggle"))) => self.toggle(),
                    other => return Err(format!("unmapped set {other:?}")),
                }
            }
            if let Some(v) = s.get("hover") {
                let p = self.targets.get(&format!("{i}/hover")).and_then(Value::as_array).ok_or(format!("no target for hover {v}"))?;
                let (x, y) = (p[0].as_f64().unwrap() as f32, p[1].as_f64().unwrap() as f32);
                let (_, abs) = self.layout();
                let m = self.model();
                self.panel.pointer(PanelPointer::Motion { x, y }, &m, abs);
                self.island.pointer(hyprv_ui_island::Pointer::Motion { x, y }, now(self.t));
            }
            for k in ["press", "click", "drag", "release", "wheel", "invoke"] {
                if s.get(k).is_some() {
                    return Err(format!("step '{k}' not supported in calendar fixtures"));
                }
            }
            if let Some(ms) = s.get("advance").and_then(Value::as_f64) {
                self.advance(ms);
            }
            if let Some(name) = s.get("capture").and_then(Value::as_str) {
                let name = name.to_owned();
                self.capture(&name);
            }
            if let Some(f) = s.get("frames") {
                let start = self.t;
                let name = f["name"].as_str().unwrap_or("").to_owned();
                for at in f["at"].as_array().into_iter().flatten() {
                    let at = at.as_f64().unwrap();
                    if start + at > self.t {
                        self.advance(start + at - self.t);
                    }
                    self.capture(&format!("{name}-t{:04}", at as i64));
                }
            }
        }
        Ok(())
    }
}

fn premul_diff(a: &[u8], b: &[u8]) -> (f64, f64) {
    let n = a.len() / 4;
    let (mut sum, mut bad) = (0f64, 0usize);
    for i in 0..n {
        let (pa, pb) = (&a[i * 4..i * 4 + 4], &b[i * 4..i * 4 + 4]);
        let mut m = 0f64;
        for c in 0..4 {
            let (x, y) = if c == 3 { (pa[3] as f64, pb[3] as f64) } else { (pa[c] as f64 * pa[3] as f64 / 255., pb[c] as f64 * pb[3] as f64 / 255.) };
            sum += (x - y).abs();
            m = m.max((x - y).abs());
        }
        if m > 48. {
            bad += 1;
        }
    }
    (sum / (n * 4) as f64, bad as f64 / n as f64)
}

fn load(path: &Path) -> Option<(Vec<u8>, usize, usize)> {
    let image = sk::Image::from_encoded(sk::Data::new_copy(&std::fs::read(path).ok()?))?;
    let (w, h) = (image.width() as usize, image.height() as usize);
    let info = sk::ImageInfo::new((w as i32, h as i32), sk::ColorType::RGBA8888, sk::AlphaType::Unpremul, None);
    let mut px = vec![0u8; w * h * 4];
    image.read_pixels(&info, &mut px, w * 4, (0, 0), sk::image::CachingHint::Allow).then_some((px, w, h))
}

#[test]
fn calendar_morph_matches_shared_references() {
    // parity/verify/verify.py sets HYPRV_CALENDAR_MORPH_OUT (and HYPRV_ISLAND_PARITY_OUT).
    let out = ["HYPRV_CALENDAR_PARITY_OUT", "HYPRV_CALENDAR_MORPH_OUT", "HYPRV_ISLAND_PARITY_OUT"]
        .iter()
        .find_map(std::env::var_os)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("calendar-parity"));
    std::fs::create_dir_all(&out).unwrap();
    let refs = rshell().join("parity/reference/panels");
    let manifest: Value = serde_json::from_str(&std::fs::read_to_string(refs.join("manifest.json")).unwrap()).unwrap();
    let mut report = String::from("file\tresult\tmean\tbad\n");
    let mut failures = Vec::new();
    let mut fixtures: Vec<PathBuf> = std::fs::read_dir(rshell().join("parity/fixtures/panels")).unwrap().flatten().map(|e| e.path())
        .filter(|p| p.file_stem().unwrap().to_string_lossy().starts_with("calendar-")).collect();
    fixtures.sort();
    assert!(!fixtures.is_empty(), "no calendar fixtures");
    for path in fixtures {
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let want: Vec<String> = manifest["images"].as_array().unwrap().iter().filter(|i| i["state"] == stem.as_str()).map(|i| i["file"].as_str().unwrap().to_owned()).collect();
        if want.is_empty() {
            failures.push(format!("{stem}: no references"));
        }
        let mut produced = Vec::new();
        for dark in [false, true] {
            for scale in [1f32, 2.] {
                let mut run = Run::new(&path, dark, scale);
                let fixture: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
                if let Err(e) = run.run(&fixture["steps"]) {
                    failures.push(format!("{stem}: {e}"));
                }
                for (file, px, w, h) in run.captures {
                    produced.push(file.clone());
                    let Some((rpx, rw, rh)) = load(&refs.join(&file)) else {
                        failures.push(format!("{file}: missing reference"));
                        report += &format!("{file}\tMISSING-REFERENCE\t\t\n");
                        continue;
                    };
                    if (rw, rh) != (w, h) {
                        failures.push(format!("{file}: size"));
                        continue;
                    }
                    let (mean, bad) = premul_diff(&px, &rpx);
                    let ok = mean <= MAX_MEAN && bad <= MAX_BAD;
                    report += &format!("{file}\t{}\t{mean:.3}\t{bad:.4}\n", if ok { "PASS" } else { "FAIL" });
                    if !ok {
                        failures.push(format!("{file}: mean {mean:.3} bad {bad:.4}"));
                    }
                    let info = sk::ImageInfo::new((w as i32, h as i32), sk::ColorType::RGBA8888, sk::AlphaType::Unpremul, None);
                    if let Some(img) = sk::images::raster_from_data(&info, sk::Data::new_copy(&px), w * 4) {
                        if let Some(png) = img.encode(None, sk::EncodedImageFormat::PNG, None) {
                            let _ = std::fs::write(out.join(format!("mine-{file}")), png.as_bytes());
                        }
                    }
                }
            }
        }
        for f in &want {
            if !produced.contains(f) {
                failures.push(format!("{f}: not produced"));
                report += &format!("{f}\tNOT-PRODUCED\t\t\n");
            }
        }
    }
    std::fs::write(out.join("report.tsv"), &report).unwrap();
    assert!(failures.is_empty(), "{} failure(s) (report {}):\n{}", failures.len(), out.display(), failures.join("\n"));
}
