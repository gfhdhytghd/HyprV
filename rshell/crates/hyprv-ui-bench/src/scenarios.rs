//! Benchmark scenarios on real surface sizes for DP-4 (3072×1728 logical):
//! bar surface 3072×58 (3072×820 while the island is expanded / calendar presented),
//! full-output popup surfaces 3072×1728.
#[path = "../../hyprv-ui-bar/tests/common/mod.rs"]
#[allow(unused_imports)]
mod bar_common;

use crate::Bench;
use hyprv_ui::{geometry::Rect, scene::{Content, Node, Scene}, skia as sk};
use hyprv_ui_island::{util::{out_cubic, qt_rect, Anim}, Agent, Inputs, Island, Media, Pointer};
use hyprv_ui_panels::{calendar::CalendarPanel, fixtures, model::{Anchor, Model, PanelKind}, Panels};
use hyprv_widgets::foundations::{hcenter, with_alpha, Palette};
use std::time::Duration;

const OUT_W: f32 = 3072.;
const OUT_H: f32 = 1728.;
const BAR_H: f32 = 58.;
const TALL_H: f32 = 820.;
const BAR_ANCHOR: Anchor = Anchor { x: 0., y: 10., width: 0., height: 38. };

fn rshell() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn all(b: &mut Bench) {
    island(b);
    bar(b);
    panels(b);
    calendar(b);
}

// ------------------------------------------------------------------ island

fn spectrum_at(k: usize) -> Vec<f32> {
    (0..16).map(|i| (50. + 40. * ((k as f32) * 0.7 + i as f32 * 0.5).sin()).max(0.)).collect()
}

fn music(playing: bool) -> Inputs {
    // UI_BENCH_NO_ART isolates the image sampler cost.
    let art = std::fs::read(rshell().join("parity/fixtures/assets/cover-warm.png")).ok().filter(|_| std::env::var_os("UI_BENCH_NO_ART").is_none()).and_then(|b| sk::Image::from_encoded(sk::Data::new_copy(&b)));
    Inputs {
        now_hh_mm: "09:41".into(),
        media: Media { available: true, playing, title: "Midnight City".into(), artist: "M83".into(), player: "spotify".into(), art_url: "file:///cover-warm.png".into(), position_s: 83., length_s: 225. },
        spectrum: if playing { spectrum_at(0) } else { Vec::new() },
        art,
        ..Default::default()
    }
}

/// The island inside the bar surface, positioned like the bar host does.
fn compose_island(island: &Island, h: f32) -> Node {
    let mut root = Node::new(Rect::new(0., 0., OUT_W, h), Content::Group);
    let g = island.geometry();
    let mut wrap = Node::new(g.rect, Content::Group);
    let mut inner = island.build();
    inner.frame.x = 0.;
    inner.frame.y = 0.;
    wrap.add(inner);
    root.add(wrap);
    root
}

fn island_case(b: &mut Bench, name: &str, h: f32, frames: usize, inputs: Inputs, note: &str, mut each: impl FnMut(&mut Island, Duration, usize)) {
    let mut island = Island::new(inputs, OUT_W, Duration::ZERO);
    island.clip_hover_to_input_region = false;
    each(&mut island, Duration::ZERO, usize::MAX);
    for k in 0..90 {
        island.frame(Duration::from_millis(k * 16));
    }
    let base = Duration::from_millis(90 * 16);
    let mut scene = Scene::new(compose_island(&island, h));
    let root = scene.root().id();
    // Host policy: frame() only while animating, at a due deadline or after new input;
    // rebuild only when `visual_rev` changed (None = animating).
    let mut built: Option<Option<u64>> = None;
    b.run(name, (OUT_W, h), &mut scene, frames, |scene, now, i| {
        let now = base + now;
        let (rev0, anim0) = (island.visual_rev(), island.animating(now));
        each(&mut island, now, i);
        if anim0 || island.visual_rev() != rev0 || island.next_deadline().is_some_and(|d| d <= now) {
            island.frame(now);
        }
        let rev = island.visual_rev();
        if rev.is_none() || built != Some(rev) {
            hyprv_ui_island::reconcile(scene, root, compose_island(&island, h));
            built = Some(rev);
        }
    }, note);
}

fn island(b: &mut Bench) {
    island_case(b, "island idle (clock only, no change)", BAR_H, 120, Inputs { now_hh_mm: "09:41".into(), ..Default::default() },
        "host would not render: expect 0 damage", |_, _, _| {});
    island_case(b, "island compact, spectrum 24 Hz", BAR_H, 180, music(true), "spectrum input every 41.7 ms", |isl, now, i| {
        if i == usize::MAX { return }
        let k = (now.as_secs_f64() * 24.) as usize;
        let mut inp = isl.inputs().clone();
        let s = spectrum_at(k);
        if inp.spectrum != s {
            inp.spectrum = s;
            isl.set_inputs(inp, now);
        }
    });
    island_case(b, "island compact, paused (66 ms idle wave)", BAR_H, 120, music(false), "QML wave timer", |_, _, _| {});
    // Expand morph: hover the left side hotspot at t0.
    island_case(b, "island music expand morph (0→600 ms)", TALL_H, 36, music(true), "hover side hotspot", |isl, now, i| {
        if i == 0 {
            let (ox, oy) = isl.origin();
            isl.pointer(Pointer::Motion { x: ox + 6., y: oy + 19. }, now);
        }
    });
    island_case(b, "island expanded, spectrum 24 Hz + 1 Hz position", TALL_H, 180, music(true), "steady expanded view", |isl, now, i| {
        if i == usize::MAX {
            let (ox, oy) = isl.origin();
            isl.pointer(Pointer::Motion { x: ox + 6., y: oy + 19. }, now);
            return;
        }
        let mut inp = isl.inputs().clone();
        let s = spectrum_at((now.as_secs_f64() * 24.) as usize);
        let pos = 83. + now.as_secs_f64().floor();
        if inp.spectrum != s || inp.media.position_s != pos {
            inp.spectrum = s;
            inp.media.position_s = pos;
            isl.set_inputs(inp, now);
        }
    });
    let agents: Vec<Agent> = (0..9).map(|k| Agent {
        id: format!("s{k}"), source: if k % 2 == 0 { "codex" } else { "claude" }.into(), source_label: "Codex".into(),
        project: format!("project-{k}"), status: "running".into(), current_tool: "Bash".into(), progress: Some(0.4), ..Default::default()
    }).collect();
    island_case(b, "island agent expanded n=9, ring step 650 ms", TALL_H, 180, Inputs { now_hh_mm: "09:41".into(), agents, ..Default::default() }, "agent observer tick", |isl, now, i| {
        if i == usize::MAX {
            let (ox, oy) = isl.origin();
            isl.pointer(Pointer::Motion { x: ox + 6., y: oy + 19. }, now);
            return;
        }
        let step = (now.as_millis() / 650) as f64;
        let mut inp = isl.inputs().clone();
        let p = 0.22 + (step * 0.05) % 0.7;
        if inp.agents[0].progress != Some(p) {
            inp.agents[0].progress = Some(p);
            isl.set_inputs(inp, now);
        }
    });
}

// ------------------------------------------------------------------ bar

fn bar(b: &mut Bench) {
    let mut m = bar_common::baseline();
    m.output.scale = b.scale as f32;
    m.assets = bar_common::fixture_assets(false, b.scale as f32);
    let mut c = hyprv_ui_bar::Controller::new(m.clone());
    let mut scene = Scene::new(hyprv_ui_bar::build(&c.model, &c.model.theme));
    let h = c.model.surface_height();
    b.run("bar static, stats sample 1 Hz", (c.model.output.width, h), &mut scene, 180, |scene, now, _| {
        let mut next = c.model.clone();
        next.now = now;
        let cpu = 7. + (now.as_secs() % 5) as f64;
        if next.stats.cpu_percent != Some(cpu) {
            next.stats.cpu_percent = Some(cpu);
            c.sync(next, now);
        }
        c.frame(scene, now);
    }, "CPU % changes once per second");
    let mut c = hyprv_ui_bar::Controller::new(m.clone());
    let mut scene = Scene::new(hyprv_ui_bar::build(&c.model, &c.model.theme));
    b.run("bar fullscreen hide → reveal", (c.model.output.width, h), &mut scene, 120, |scene, now, i| {
        if i == 0 || i == 60 {
            let mut next = c.model.clone();
            next.fullscreen = i == 0;
            c.sync(next, now);
        }
        c.frame(scene, now);
    }, "BarScreenState bezier motion");
}

// ------------------------------------------------------------------ panels

fn panel_case(b: &mut Bench, name: &str, panel: (PanelKind, Model), frames: usize, note: &str,
    open: impl FnOnce(&mut Panels, &Model), mut each: impl FnMut(&mut Model, Duration, usize)) {
    let (kind, mut model) = panel;
    model.output.scale = b.scale as f32;
    let mut p = Panels::default();
    open(&mut p, &model);
    let _ = p.tick(&model);
    let mut scene = Scene::new(p.build(kind, &model));
    b.run(name, (OUT_W, OUT_H), &mut scene, frames, |scene, now, i| {
        each(&mut model, now, i);
        model.now = now;
        let _ = p.tick(&model);
        p.update(kind, scene, &model);
    }, note);
}

fn panels(b: &mut Bench) {
    panel_case(b, "control panel open (reveal) + steady", (PanelKind::ControlPanel, fixtures::control_idle(false)), 90, "open at t0",
        |p, m| { let _ = p.control.toggle_for(BAR_ANCHOR, &m.output.name, m.now); }, |_, _, _| {});
    panel_case(b, "Wi-Fi popup open + steady", (PanelKind::Wifi, fixtures::wifi_many(false)), 90, "open at t0",
        |p, m| { p.wifi.set_anchor(BAR_ANCHOR, &m.output.name); let _ = p.wifi.open(m); }, |_, _, _| {});
    panel_case(b, "QuickAdjust OSD show", (PanelKind::QuickAdjust, fixtures::control_idle(false)), 60, "showVolume at t0",
        |p, m| { let _ = p.ipc_quick_adjust("showVolume", m); }, |_, _, _| {});
    panel_case(b, "stats popup, sample 1 Hz", (PanelKind::Stats, fixtures::stats_full(false)), 180, "chart history push per second",
        |p, m| { let _ = p.stats.toggle_for(BAR_ANCHOR, &m.output.name, m.now); }, {
            let mut last_sec = None;
            move |m, now, _| {
            let sec = now.as_secs();
            if last_sec != Some(sec) {
                last_sec = Some(sec);
                let v = 20. + (sec as f64 * 7.) % 60.;
                m.stats.cpu_usage = v;
                for h in [&mut m.stats.cpu_history, &mut m.stats.memory_history, &mut m.stats.network_history] {
                    h.push(v);
                    if h.len() > 120 { h.remove(0); }
                }
            }
        }});
}

// ------------------------------------------------------------------ calendar morph (bar centre)

fn calendar(b: &mut Bench) {
    let mut model = fixtures::calendar_week(false);
    model.output.scale = b.scale as f32;
    let mut island = Island::new(music(true), OUT_W, Duration::ZERO);
    let (pw, ph) = CalendarPanel::host_size(OUT_W, TALL_H);
    let mut panel = CalendarPanel::new(pw, ph);
    let pal = Palette::new(false);
    let a = || Anim::new(210, out_cubic());
    let (mut progress, mut cw, mut ch, mut cr) = (a(), a(), a(), a());
    progress.set(0., Duration::ZERO);
    let mut presented = false;
    let mut visible = false;
    let mut close_at: Option<Duration> = None;
    let compose = |island: &mut Island, panel: &mut CalendarPanel, model: &Model, now: Duration,
                       presented: bool, visible: bool, progress: &Anim, cw: &mut Anim, ch: &mut Anim, cr: &mut Anim| {
        let mut inp = island.inputs().clone();
        inp.calendar_open = presented;
        inp.calendar_progress = progress.get(now) as f32;
        island.set_inputs(inp, now);
        island.frame(now);
        let g = island.geometry();
        for x in [&mut *cw, &mut *ch, &mut *cr] { x.enabled = presented; }
        cw.set(if presented { pw as f64 } else { g.width as f64 }, now);
        ch.set(if presented { ph as f64 } else { g.height as f64 }, now);
        cr.set(if visible { 30. } else { 24. }, now);
        let (w, h) = (cw.get(now) as f32, ch.get(now) as f32);
        let x = hcenter(OUT_W) + if presented { 0. } else { g.attached_center_offset } - hcenter(w);
        let p = progress.get(now);
        let mut root = Node::new(Rect::new(0., 0., OUT_W, TALL_H), Content::Group);
        let mut sec = if p > 0.001 {
            qt_rect(Rect::new(x, 10., w, h), cr.get(now) as f32, pal.module_background, Some((1., with_alpha(pal.primary_text, 0.09))))
        } else {
            Node::new(Rect::new(x, 10., w, h), Content::Group)
        };
        sec.clip = Some([0.; 4]);
        let mut isl = Node::new(Rect::new(hcenter(w) - hcenter(g.width), 0., g.width, g.height), Content::Group);
        isl.opacity = (1. - p) as f32;
        isl.visible = isl.opacity > 0.01;
        let mut inner = island.build();
        inner.frame.x = 0.;
        inner.frame.y = 0.;
        isl.add(inner);
        sec.add(isl);
        // UI_BENCH_CAL_BISECT=nopanel|noisland isolates the morph's parts.
        let bisect = std::env::var("UI_BENCH_CAL_BISECT").unwrap_or_default();
        if bisect == "noisland" {
            sec.children.clear();
        }
        if presented && bisect != "nopanel" {
            let abs = (x + hcenter(w) - hcenter(pw), 10.);
            let mut cal = Node::new(Rect::new(abs.0 - x, 0., pw, ph), Content::Group);
            cal.opacity = p as f32;
            let mut m = model.clone();
            m.now = now;
            cal.add(panel.build(&m, abs));
            sec.add(cal);
        }
        root.add(sec);
        root
    };
    let n0 = compose(&mut island, &mut panel, &model, Duration::ZERO, presented, visible, &progress, &mut cw, &mut ch, &mut cr);
    let mut scene = Scene::new(n0);
    let root = scene.root().id();
    b.run("calendar morph open (0–450 ms) → close", (OUT_W, TALL_H), &mut scene, 60, |scene, now, i| {
        if i == 0 {
            visible = true;
            presented = true;
            progress.set(1., now);
        }
        if i == 27 {
            visible = false;
            progress.set(0., now);
            close_at = Some(now + Duration::from_millis(210));
        }
        if close_at.is_some_and(|c| now >= c) {
            close_at = None;
            presented = false;
        }
        let node = compose(&mut island, &mut panel, &model, now, presented, visible, &progress, &mut cw, &mut ch, &mut cr);
        hyprv_ui_island::reconcile(scene, root, node);
    }, "ipc calendar toggle twice");
}
