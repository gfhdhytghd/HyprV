//! SystemStatsPopup (ui-panels-stats.md §2) with SystemTrendChart (§3).
use crate::{
    model::*,
    popup::{self, GlassIds, GlassStyle, Machine, PopupEvent},
    qtext::{BOLD, MEDIUM},
    ui::*,
    widgets,
};
use hyprv_ui::{
    animation::Color,
    geometry::Rect,
    scene::{Node, NodeId, Scene},
};
use hyprv_widgets::containers::RevealTiming;
use std::time::Duration;

/// `formatPercent(null)`: `Number(null)` is 0, so a missing core shows "0%", never "--".
pub const QUIRK_MISSING_CORE_SHOWS_ZERO: bool = true;

pub const WIDTH: f32 = 548.;
/// `x: Math.max(10, Math.min(width - card.width - 10, 10))` is always 10.
pub const QUIRK_X_ALWAYS_10: f32 = 10.;
const PADDING: f32 = 16.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Z {
    Card,
}

fn timing() -> RevealTiming {
    RevealTiming { open_pause: 20, open_reveal: 200, content_delay: 20, open_fade: 140, open_slide: 180, open_offset: -8., close_pause: 30, close_reveal: 180, close_fade: 90, close_slide: 150, close_offset: -8. }
}

/// `humanRate(b)` (shell.qml:1788-1800)
pub fn human_rate(b: f64) -> String {
    let v = b.max(0.);
    if v < 1024. {
        format!("{} B/s", js_round(v))
    } else if v < 1024. * 1024. {
        format!("{:.1}kB/s", v / 1024.)
    } else if v < 1024. * 1024. * 1024. {
        format!("{:.1}MB/s", v / (1024. * 1024.))
    } else {
        format!("{:.1}GB/s", v / (1024. * 1024. * 1024.))
    }
}

/// JS `Math.round` (half toward +∞).
pub fn js_round(v: f64) -> i64 {
    (v + 0.5).floor() as i64
}

pub fn severity(pal: &Palette, v: f64) -> Color {
    pal.severity(v)
}

pub struct StatsPopup {
    pub m: Machine,
    anchor: Anchor,
    output: String,
    root: Option<NodeId>,
    glass: Option<GlassIds>,
    /// Inputs of the built card, compared by value (no per-update formatting).
    key: Option<(Theme, Stats, Output, Anchor)>,
    last_card: Rect,
}

impl Default for StatsPopup {
    fn default() -> Self {
        Self::new()
    }
}

impl StatsPopup {
    pub fn new() -> Self {
        Self { m: Machine::new(timing()), anchor: Anchor::default(), output: String::new(), root: None, glass: None, key: None, last_card: Rect::default() }
    }
    /// Mapped but the content height is not known yet (no build since mapping); the
    /// QML binding has it immediately, so [`crate::Panels::tick`] measures first.
    pub fn needs_measure(&self) -> bool {
        self.m.mapped && self.m.reveal.full_height <= 0.
    }
    pub fn is_mapped(&self) -> bool {
        self.m.mapped
    }
    /// The sampler should run while this is true (subscriber-counted lease).
    pub fn wants_samples(&self) -> bool {
        self.m.requested || self.m.closing
    }
    pub fn open_for(&mut self, anchor: Anchor, output: &str, now: Duration) -> Vec<Action> {
        self.anchor = anchor;
        self.output = output.to_string();
        if self.m.open(now) {
            vec![Action::Host(HostAction::Mapped(PanelKind::Stats))]
        } else {
            vec![]
        }
    }
    pub fn toggle_for(&mut self, anchor: Anchor, output: &str, now: Duration) -> Vec<Action> {
        if self.m.mapped && self.anchor == anchor && self.output == output {
            self.m.close(now);
            vec![]
        } else {
            self.open_for(anchor, output, now)
        }
    }
    pub fn close(&mut self, now: Duration) {
        self.m.close(now);
    }
    pub fn tick(&mut self, now: Duration) -> Vec<Action> {
        let mut out = Vec::new();
        for ev in self.m.tick(now) {
            if ev == PopupEvent::CloseFinished && self.m.finish_close() {
                out.push(Action::Host(HostAction::Unmapped(PanelKind::Stats)));
            }
        }
        out
    }
    pub fn animating(&self) -> bool {
        self.m.mapped && self.m.animating()
    }
    pub fn next_deadline(&self) -> Option<Duration> {
        self.m.next_deadline()
    }
    pub fn surface(&self, model: &Model) -> Option<SurfaceSpec> {
        self.m.mapped.then(|| SurfaceSpec {
            namespace: "shell:hyprv-system-stats",
            layer: Layer::Overlay,
            anchors: [true; 4],
            exclusive_zone: -1,
            keyboard: KeyboardMode::OnDemand,
            size: None,
            input_region: None,
            output: if self.output.is_empty() { model.output.name.clone() } else { self.output.clone() },
        })
    }

    fn style(pal: &Palette) -> GlassStyle {
        GlassStyle {
            radius: 19.,
            fill: pal.glass_fill,
            stroke: pal.glass_stroke,
            surface_opacity: 0.82,
            shadow: if pal.dark { with_alpha(rgb("#000000"), 0.45) } else { with_alpha(rgb("#111111"), 0.18) },
        }
    }

    fn card_rect(&self, model: &Model, h: f32) -> Rect {
        let x = QUIRK_X_ALWAYS_10;
        Rect::new(x, popup::below_or_above(self.anchor.bottom(), h, model.output.height), WIDTH, h)
    }

    /// Content (card-local) and the card height.
    pub fn content(model: &Model, card_abs: (f32, f32)) -> (Vec<Node>, f32) {
        let mut r = Self::content_raw(model, card_abs);
        for n in &mut r.0 {
            crate::qrect::convert(n, card_abs, model.output.scale);
        }
        r
    }
    fn content_raw(model: &Model, card_abs: (f32, f32)) -> (Vec<Node>, f32) {
        let pal = Palette::new(model.theme.dark);
        let s = &model.stats;
        let dpr = model.output.scale;
        let w = WIDTH - 2. * PADDING;
        let cabs = (card_abs.0 + PADDING, card_abs.1 + PADDING);
        let muted = with_alpha(pal.primary_text, if pal.dark { 0.72 } else { 0.68 });
        let metric = if pal.dark { pal.primary_text } else { rgb("#000000") };
        let mut col = Vec::new();
        let mut y = 0.;
        // header
        {
            let t = widgets::label("系统资源", 15., BOLD, pal.primary_text);
            let r = widgets::label("过去 120s · 1s", 11., MEDIUM, muted);
            let (tw, th) = text_size(&t);
            let (rw, rh) = text_size(&r);
            let h = th.max(rh);
            col.push(text(&t, Rect::new(0., y + center(h, th), tw, th), cabs, dpr));
            col.push(text(&r, Rect::new(w - rw, y + center(h, rh), rw, rh), cabs, dpr));
            y += h + 18.;
        }
        let title_row = |col: &mut Vec<Node>, y: f32, title: &str, value: &str, detail: Option<&str>| -> f32 {
            let t = widgets::label(title, 16., BOLD, pal.primary_text);
            let v = widgets::label(value, 16., BOLD, metric);
            let (tw, th) = text_size(&t);
            let (vw, vh) = text_size(&v);
            let d = detail.map(|d| widgets::label(d, 11., MEDIUM, metric));
            let dh = d.as_ref().map(|d| text_size(d).1).unwrap_or(0.);
            let h = th.max(vh).max(dh);
            col.push(text(&t, Rect::new(0., y + center(h, th), tw, th), cabs, dpr));
            col.push(text(&v, Rect::new(tw + 8., y + center(h, vh), vw, vh), cabs, dpr));
            if let Some(d) = d {
                let (dw, dh) = text_size(&d);
                col.push(text(&d, Rect::new(w - dw, y + center(h, dh), dw, dh), cabs, dpr));
            }
            h
        };
        // CPU
        {
            let h = title_row(&mut col, y, "CPU:", &format!("{}%", js_round(s.cpu_usage)), None);
            y += h + 8.;
            let grid_w = 74. * 2. + 10.;
            let grid_h = 32. * 4. + 8. * 3.;
            let chart_w = 240f32.max(w - grid_w - 16.);
            let chart_h = 112f32.max(grid_h);
            let row_h = 116f32.max(grid_h);
            col.push(widgets::trend_chart(&pal, Rect::new(0., y, chart_w, chart_h), &s.cpu_history, 100., false, severity(&pal, s.cpu_usage)));
            let gx = w - grid_w;
            let gy = y + center(row_h, grid_h);
            for i in 0..8 {
                let (cx, cy) = ((i % 2) as f32 * (74. + 10.), (i / 2) as f32 * (32. + 8.));
                let f = Rect::new(gx + cx, gy + cy, 74., 32.);
                let mut tile = rect(f, 10., panel_color(with_alpha(pal.primary_text, if pal.dark { 0.06 } else { 0.04 })), Some((1., pal.glass_stroke)));
                let v = s.cpu_core_usages.get(i).copied();
                let label = match v {
                    Some(v) if v.is_finite() => format!("C{} {}%", i + 1, js_round(v)),
                    Some(_) => format!("C{} --", i + 1),
                    None if QUIRK_MISSING_CORE_SHOWS_ZERO => format!("C{} 0%", i + 1),
                    None => format!("C{} --", i + 1),
                };
                tile.children.push(text_center(&widgets::label(&label, 13., BOLD, metric), 74., 32., at(cabs, f), dpr));
                col.push(tile);
            }
            y += row_h + 18.;
        }
        // RAM
        {
            let h = title_row(&mut col, y, "RAM:", &format!("{}%", js_round(s.memory_usage)), None);
            y += h + 8.;
            col.push(widgets::trend_chart(&pal, Rect::new(0., y, w, 112.), &s.memory_history, 100., false, severity(&pal, s.memory_usage)));
            y += 112. + 18.;
        }
        // Network
        {
            let value = if s.default_interface.is_empty() { "未连接".to_string() } else { human_rate(s.rx_rate + s.tx_rate) };
            let detail = format!("↓ {}   ↑ {}", human_rate(s.rx_rate), human_rate(s.tx_rate));
            let h = title_row(&mut col, y, "Network:", &value, Some(&detail));
            y += h + 8.;
            col.push(widgets::trend_chart(&pal, Rect::new(0., y, w, 112.), &s.network_history, 100., true, pal.accent));
            y += 112.;
        }
        (vec![group_with(Rect::new(PADDING, PADDING, w, y), col)], y + 2. * PADDING)
    }

    pub fn build(&mut self, model: &Model) -> Node {
        let mut n = self.build_raw(model);
        crate::qrect::convert(&mut n, (0., 0.), model.output.scale);
        n
    }
    fn build_raw(&mut self, model: &Model) -> Node {
        let mut root = group(Rect::new(0., 0., model.output.width, model.output.height));
        self.root = Some(root.id());
        let probe = self.card_rect(model, 0.);
        let (_, h) = Self::content(model, (probe.x, probe.y));
        self.m.set_full_height(h);
        let card = self.card_rect(model, h);
        let (nodes, _) = Self::content(model, (card.x, card.y));
        self.last_card = card;
        let pal = Palette::new(model.theme.dark);
        let v = self.m.reveal.values();
        let (cn, g) = popup::glass(card, h, v.height, v.opacity, v.offset, &Self::style(&pal), nodes, model.output.scale);
        self.glass = Some(g);
        root.children.push(cn);
        self.key = Some((model.theme, model.stats.clone(), model.output.clone(), self.anchor));
        root
    }
    fn key_matches(&self, model: &Model) -> bool {
        self.key.as_ref().is_some_and(|(t, s, o, a)| *t == model.theme && *s == model.stats && *o == model.output && *a == self.anchor)
    }
    pub fn update(&mut self, scene: &mut Scene, model: &Model) -> bool {
        let changed = self.update_raw(scene, model);
        crate::qrect::finalize(scene, model.output.scale);
        changed
    }
    fn update_raw(&mut self, scene: &mut Scene, model: &Model) -> bool {
        let (Some(root), Some(g)) = (self.root, self.glass) else { return false };
        if scene.root().find(root).is_none() {
            return false;
        }
        let rev = scene.revision();
        if !self.key_matches(model) {
            let n = self.build(model);
            self.root = Some(root);
            let ch = n.children;
            scene.edit(root, |r| r.children = ch);
        } else {
            let card = self.card_rect(model, self.m.reveal.full_height);
            self.last_card = card;
            let v = self.m.reveal.values();
            popup::glass_update(scene, &g, (card.x, card.y), v.height, v.opacity);
        }
        scene.revision() != rev
    }
    pub fn key(&mut self, ev: &KeyEvent, model: &Model) -> Reply<Action> {
        let mut r = Reply::default();
        if ev.pressed && ev.key == Key::Escape && self.m.mapped {
            self.m.close(model.now);
            r.redraw = true;
        }
        r
    }
    /// Any button outside the card closes; the card absorbs clicks.
    pub fn pointer(&mut self, ev: Pointer, model: &Model) -> Reply<Action> {
        let mut r = Reply { cursor: Some(Cursor::Default), ..Default::default() };
        if let Pointer::Press { x, y, .. } = ev {
            if self.m.mapped && !self.last_card.contains(x, y) {
                self.m.close(model.now);
                r.redraw = true;
            }
        }
        r
    }
    pub fn preview(model: &Model) -> (Node, f32, f32) {
        let mut r = Self::preview_raw(model);
        crate::qrect::convert(&mut r.0, (0., 0.), model.output.scale);
        r
    }
    fn preview_raw(model: &Model) -> (Node, f32, f32) {
        let (nodes, h) = Self::content(model, (0., 0.));
        let pal = Palette::new(model.theme.dark);
        let (n, _) = popup::glass(Rect::new(0., 0., WIDTH, h), h, h, 1., 0., &Self::style(&pal), nodes, model.output.scale);
        (n, WIDTH, h)
    }
}
