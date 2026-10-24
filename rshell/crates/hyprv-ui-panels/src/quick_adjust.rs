//! QuickAdjustPopup — brightness/volume OSD (ui-panels-stats.md §1).
use crate::{
    control_panel::{laptop_profile, snap_brightness_for, volume_icon},
    model::*,
    ui::*,
    widgets::{self, Slider},
};
use hyprv_ui::{
    animation::{Direction, Family},
    geometry::Rect,
    scene::{Node, NodeId, Scene},
    skia::Matrix,
};
use std::time::Duration;

/// `adjustmentFinished` is not connected in QuickAdjustPopup.qml, so drags here never call
/// `brightness_finish`. Flip to send [`Action::BrightnessFinish`] on release.
pub const QUIRK_NO_BRIGHTNESS_FINISH: bool = true;
/// Laptop (laptop-quickadjust.md §1): `brightnessUiStepPercent` 1, `snap = clamp(round(v))`.
pub const QUIRK_LAPTOP_BRIGHTNESS_STEP_1: bool = true;
/// Laptop (§5): `updatePopupPosition` runs on map, size or screen change only, so a bar
/// toggle while the OSD is up leaves the card at its old y until it remaps.
pub const QUIRK_QA_POSITION_FROZEN_WHILE_MAPPED: bool = true;

pub const WIDTH: f32 = 62. * 4. + 10. * 3.;
pub const HEIGHT: f32 = 62.;
const HIDDEN_SCALE: f64 = 0.96;
const AUTO_HIDE_MS: u64 = 3000;
const CLOSE_FINALIZE_MS: u64 = 140;
const ICON_LEFT_INSET: f32 = 24.;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Brightness,
    Volume,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Z {
    Slider,
}

#[derive(Clone, Debug, Default)]
struct Overrides {
    brightness: Option<(i32, i32)>, // (value, model value when set)
    volume: Option<(i32, i32)>,
    muted: Option<(bool, bool)>,
}

pub struct QuickAdjust {
    pub mapped: bool,
    pub mode: Mode,
    closing: bool,
    hide_at: Option<Duration>,
    finalize_at: Option<Duration>,
    opacity: Anim<f64>,
    scale: Anim<f64>,
    adjusting: bool,
    preview: f32,
    ov: Overrides,
    inter: Interaction<Z>,
    zones: Vec<Zone<Z>>,
    root: Option<NodeId>,
    card: Option<NodeId>,
    key: String,
    /// (opacity, scale) baked into the current tree.
    baked_frame: (f64, f64),
    /// Laptop profile (laptop-quickadjust.md). Latched from [`laptop_profile`] at
    /// construction; tests and fixture replay may set it.
    pub laptop: bool,
    /// Laptop: card rect latched at map, keyed by the output it was computed for.
    frozen: std::cell::Cell<Option<(Rect, (u32, u32))>>,
}

impl Default for QuickAdjust {
    fn default() -> Self {
        Self::new()
    }
}

impl QuickAdjust {
    pub fn new() -> Self {
        Self::with_profile(laptop_profile())
    }
    pub fn with_profile(laptop: bool) -> Self {
        Self {
            laptop,
            frozen: std::cell::Cell::new(None),
            mapped: false,
            mode: Mode::Brightness,
            closing: false,
            hide_at: None,
            finalize_at: None,
            opacity: Anim::new(0., 120, ease(Family::Quad, Direction::Out)),
            scale: Anim::new(HIDDEN_SCALE, 160, ease(Family::Cubic, Direction::Out)),
            adjusting: false,
            preview: 0.,
            ov: Overrides::default(),
            inter: Interaction::default(),
            zones: Vec::new(),
            root: None,
            card: None,
            key: String::new(),
            baked_frame: (f64::NAN, f64::NAN),
        }
    }

    /// `show(kind)` (IPC `quickAdjust.*` and the brightness probe).
    pub fn show(&mut self, mode: Mode, now: Duration) -> Vec<Action> {
        self.mode = mode;
        self.closing = false;
        self.finalize_at = None;
        self.hide_at = Some(now + ms(AUTO_HIDE_MS));
        let mut out = Vec::new();
        if !self.mapped {
            self.opacity.snap(0.);
            self.scale.snap(HIDDEN_SCALE);
            self.mapped = true;
            out.push(Action::Host(HostAction::Mapped(PanelKind::QuickAdjust)));
        }
        self.opacity.set(1., now);
        self.scale.set(1., now);
        out
    }

    fn hide(&mut self, now: Duration) {
        self.closing = true;
        self.hide_at = None;
        self.opacity.set(0., now);
        self.scale.set(HIDDEN_SCALE, now);
        self.finalize_at = Some(now + ms(CLOSE_FINALIZE_MS));
    }

    pub fn tick(&mut self, now: Duration) -> Vec<Action> {
        let mut out = Vec::new();
        if self.hide_at.is_some_and(|t| now >= t) {
            self.hide(now);
        }
        if self.finalize_at.is_some_and(|t| now >= t) {
            self.finalize_at = None;
            if self.closing {
                self.closing = false;
                self.mapped = false;
                self.frozen.set(None);
                self.adjusting = false;
                self.inter = Interaction::default();
                out.push(Action::Host(HostAction::Unmapped(PanelKind::QuickAdjust)));
            }
        }
        out
    }

    pub fn animating(&self, now: Duration) -> bool {
        self.mapped && (self.opacity.running(now) || self.scale.running(now))
    }
    pub fn next_deadline(&self) -> Option<Duration> {
        [self.hide_at, self.finalize_at].into_iter().flatten().min()
    }

    fn card_rect(&self, model: &Model) -> Rect {
        if !(self.laptop && QUIRK_QA_POSITION_FROZEN_WHILE_MAPPED) {
            return Self::live_rect(model);
        }
        let key = (model.output.width.to_bits(), model.output.height.to_bits());
        match self.frozen.get() {
            Some((rect, k)) if self.mapped && k == key => rect,
            _ => {
                let rect = Self::live_rect(model);
                if self.mapped {
                    self.frozen.set(Some((rect, key)));
                }
                rect
            }
        }
    }
    /// Card y is the live primary-bar exclusive zone: 58 full, 25 compact on laptop.
    fn live_rect(model: &Model) -> Rect {
        let w = model.output.width;
        let bar_bottom = model.shell.bar_exclusive_zone.max(0.);
        let x = match model.shell.quick_adjust_anchor_right {
            Some(right) => 10f32.max((w - WIDTH - 10.).min(right.round() - WIDTH)),
            None => 10f32.max(w - WIDTH - 10.),
        };
        Rect::new(x, bar_bottom, WIDTH, HEIGHT)
    }

    pub fn surface(&self, model: &Model) -> Option<SurfaceSpec> {
        self.mapped.then(|| SurfaceSpec {
            namespace: "shell:hyprv-quick-adjust",
            layer: Layer::Overlay,
            anchors: [true; 4],
            exclusive_zone: -1,
            keyboard: KeyboardMode::None,
            size: None,
            input_region: Some(self.card_rect(model)),
            output: model.output.name.clone(),
        })
    }

    fn values(&mut self, model: &Model) -> (i32, Audio) {
        let mut b = model.controls.brightness_percent;
        if let Some((v, seen)) = self.ov.brightness {
            if seen != model.controls.brightness_percent {
                self.ov.brightness = None;
            } else {
                b = v;
            }
        }
        let mut a = model.audio.clone();
        if let Some((v, seen)) = self.ov.volume {
            if seen != model.audio.volume_percent {
                self.ov.volume = None;
            } else {
                a.volume_percent = v;
                a.available = true;
            }
        }
        if let Some((v, seen)) = self.ov.muted {
            if seen != model.audio.muted {
                self.ov.muted = None;
            } else {
                a.muted = v;
            }
        }
        (b, a)
    }

    fn content(&mut self, model: &Model, card: Rect) -> Node {
        let pal = Palette::new(model.theme.dark);
        let (b, a) = self.values(model);
        let mut zones = Vec::new();
        let mut c = Ctx { dpr: model.output.scale, pal: &pal, zones: &mut zones };
        let value = if self.adjusting {
            self.preview
        } else if self.mode == Mode::Volume {
            a.volume_percent as f32
        } else {
            b as f32
        };
        let icon = if self.mode == Mode::Volume { volume_icon(&a) } else { "\u{F00DF}" };
        let s = Slider { icon: icon.into(), value, icon_left_inset: ICON_LEFT_INSET };
        let n = widgets::slider(&mut c, Rect::new(0., 0., WIDTH, HEIGHT), (card.x, card.y), &s, Z::Slider, true);
        self.zones = zones;
        n
    }

    fn transform(&self, now: Duration) -> (f32, Matrix) {
        let s = self.scale.get(now) as f32;
        // transformOrigin: TopRight
        let mut m = Matrix::translate((WIDTH, 0.));
        m.pre_scale((s, s), None);
        m.pre_translate((-WIDTH, 0.));
        (self.opacity.get(now) as f32, m)
    }

    pub fn build(&mut self, model: &Model) -> Node {
        let mut n = self.build_raw(model);
        // Native text under the card's scale is baked for the current frame (glyph quads
        // snap in device space); `update` rebuilds every frame while scale/opacity run.
        push_opacity(&mut n, 1.);
        crate::qrect::convert(&mut n, (0., 0.), model.output.scale);
        self.baked_frame = (self.opacity.get(model.now), self.scale.get(model.now));
        n
    }
    fn build_raw(&mut self, model: &Model) -> Node {
        let mut root = group(Rect::new(0., 0., model.output.width, model.output.height));
        self.root = Some(root.id());
        let card = self.card_rect(model);
        let slider = self.content(model, card);
        let (op, m) = self.transform(model.now);
        let mut cn = group_with(card, vec![slider]);
        cn.opacity = op;
        cn.transform = m;
        self.card = Some(cn.id());
        root.children.push(cn);
        self.key = self.key_of(model);
        root
    }

    fn key_of(&self, model: &Model) -> String {
        format!("{:?}{:?}{:?}{:?}{}{}{:?}{:?}", model.theme, model.controls.brightness_percent, model.audio, self.mode, self.adjusting, self.preview, model.output, model.shell)
    }

    pub fn update(&mut self, scene: &mut Scene, model: &Model) -> bool {
        let changed = self.update_raw(scene, model);
        crate::qrect::finalize(scene, model.output.scale);
        changed
    }
    fn update_raw(&mut self, scene: &mut Scene, model: &Model) -> bool {
        let (Some(root), Some(card)) = (self.root, self.card) else { return false };
        if scene.root().find(root).is_none() {
            return false;
        }
        let rev = scene.revision();
        let key = self.key_of(model);
        let frame = (self.opacity.get(model.now), self.scale.get(model.now));
        if key != self.key || frame != self.baked_frame {
            let mut n = self.build(model);
            self.root = Some(root);
            // Replace the card in place (edit keeps its id): damage is the card's old ∪ new
            // bounds, not the full-output root (PANELS-PERF-5, svc-media).
            if n.children.len() == 1 && scene.root().find(card).is_some() {
                let new_card = n.children.pop().unwrap();
                scene.edit(card, |c| *c = new_card);
                self.card = Some(card);
            } else {
                let ch = n.children;
                scene.edit(root, |r| r.children = ch);
            }
        } else {
            let (op, m) = self.transform(model.now);
            let need = scene.root().find(card).is_some_and(|n| n.opacity != op || n.transform != m);
            if need {
                scene.edit(card, |n| {
                    n.opacity = op;
                    n.transform = m;
                });
            }
        }
        scene.revision() != rev
    }

    fn snap(&self, v: f32) -> i32 {
        snap_brightness_for(self.laptop && QUIRK_LAPTOP_BRIGHTNESS_STEP_1, v)
    }
    /// `applyValue(v)`. Laptop: one `BrightnessSet(n)` per distinct value, never a finish
    /// (`applyBrightnessPercent`, laptop shell.qml:2877-2882).
    fn apply(&mut self, v: f32, model: &Model) -> Vec<Action> {
        let v = v.round().clamp(0., 100.) as i32;
        self.hide_at = Some(model.now + ms(AUTO_HIDE_MS));
        match self.mode {
            Mode::Brightness => {
                let s = self.snap(v as f32);
                self.ov.brightness = Some((s, model.controls.brightness_percent));
                vec![Action::BrightnessSet(s)]
            }
            Mode::Volume => {
                let mut out = Vec::new();
                self.ov.volume = Some((v, model.audio.volume_percent));
                // QML only clears the local `audioMuted` flag; `volume("set")` does the rest.
                if model.audio.muted && v > 0 {
                    self.ov.muted = Some((false, model.audio.muted));
                }
                out.push(Action::SetVolume(v));
                out
            }
        }
    }

    pub fn pointer(&mut self, ev: Pointer, model: &Model) -> Reply<Action> {
        let mut r = Reply::default();
        if !self.mapped {
            return r;
        }
        let zones = self.zones.clone();
        let card = self.card_rect(model);
        match self.inter.feed(ev, &zones) {
            Hit::Hover => r.redraw = true,
            Hit::Pressed(_, Button::Left, x, y) => {
                r.redraw = true;
                let s = Slider { icon: String::new(), value: 0., icon_left_inset: ICON_LEFT_INSET };
                if self.mode == Mode::Volume && widgets::slider_icon_hit(&s, WIDTH, HEIGHT, x - card.x, y - card.y) {
                    // toggleAudioMute()
                    self.hide_at = Some(model.now + ms(AUTO_HIDE_MS));
                    let (_, a) = self.values(model);
                    self.ov.muted = Some((!a.muted, model.audio.muted));
                    r.actions.push(Action::ToggleMute);
                    self.inter.pressed = None;
                } else {
                    self.adjusting = true;
                    self.preview = widgets::slider_value(x - card.x, WIDTH);
                    r.actions = self.apply(self.preview, model);
                }
            }
            Hit::Dragged(_, x, _) if self.adjusting => {
                self.preview = widgets::slider_value(x - card.x, WIDTH);
                r.actions = self.apply(self.preview, model);
                r.redraw = true;
            }
            // ControlPanelSlider onCanceled; QuickAdjust ignores adjustmentFinished
            Hit::Canceled { .. } if self.adjusting => {
                self.adjusting = false;
                if !QUIRK_NO_BRIGHTNESS_FINISH && !self.laptop && self.mode == Mode::Brightness {
                    r.actions.push(Action::BrightnessFinish(self.snap(self.preview)));
                }
                r.redraw = true;
            }
            Hit::Released { x, .. } if self.adjusting => {
                self.preview = widgets::slider_value(x - card.x, WIDTH);
                r.actions = self.apply(self.preview, model);
                self.adjusting = false;
                if !QUIRK_NO_BRIGHTNESS_FINISH && !self.laptop && self.mode == Mode::Brightness {
                    r.actions.push(Action::BrightnessFinish(self.snap(self.preview)));
                }
                r.redraw = true;
            }
            _ => {}
        }
        r.cursor = Some(self.inter.cursor(&self.zones));
        r
    }

    /// Card at origin at full opacity/scale (offscreen tests).
    pub fn preview(&mut self, model: &Model, mode: Mode) -> (Node, f32, f32) {
        let mut r = self.preview_raw(model, mode);
        crate::qrect::convert(&mut r.0, (0., 0.), model.output.scale);
        r
    }
    fn preview_raw(&mut self, model: &Model, mode: Mode) -> (Node, f32, f32) {
        self.mode = mode;
        let n = self.content(model, Rect::new(0., 0., WIDTH, HEIGHT));
        (group_with(Rect::new(0., 0., WIDTH, HEIGHT), vec![n]), WIDTH, HEIGHT)
    }
}

/// Leaf rectangles under a fading card become single-leaf opacity layers (see below).
pub const QUIRK_LEAF_RECT_OPACITY_LAYERS: bool = true;

/// Push PerNode group opacity down to the drawing leaves (Qt multiplies the accumulated
/// opacity into each draw). Leaf rectangles become single-leaf opacity layers — the same
/// result as PerNode for one leaf — so the engine rasterises them at alpha 1 and applies
/// the alpha when compositing, like Qt's shader (no 8-bit rounding before the blend;
/// docs/requests/hyprv-ui.md ENG opacity-at-composite). Safe because `update` rebuilds
/// the tree whenever the card's opacity changes.
fn push_opacity(node: &mut Node, inherited: f32) {
    use hyprv_ui::scene::{Content, OpacityMode};
    let eff = inherited * node.opacity;
    if node.effects.multi_effect.is_some() || node.opacity_mode == OpacityMode::Group {
        node.opacity = eff;
        return;
    }
    match node.content {
        Content::Group => {
            node.opacity = 1.;
            for c in &mut node.children {
                push_opacity(c, eff);
            }
        }
        Content::Rect(_) if eff < 1. && QUIRK_LEAF_RECT_OPACITY_LAYERS => {
            if !node.children.is_empty() {
                // own painting into a first child leaf so the layer covers it alone
                let f = node.frame;
                let own = std::mem::replace(&mut node.content, Content::Group);
                let mut leaf = Node::new(hyprv_ui::geometry::Rect::new(0., 0., f.width, f.height), own);
                leaf.rect_mode = node.rect_mode;
                leaf.rect_antialias = node.rect_antialias;
                node.children.insert(0, leaf);
                let o = node.opacity;
                node.opacity = 1.;
                for c in &mut node.children {
                    push_opacity(c, inherited * o);
                }
            } else {
                node.opacity = eff;
                node.opacity_mode = OpacityMode::Group;
            }
        }
        _ => {
            node.opacity = eff;
            for c in &mut node.children {
                push_opacity(c, 1.);
            }
        }
    }
}

#[cfg(test)]
mod laptop_tests {
    use super::*;
    fn model() -> Model {
        let mut m = Model::default();
        m.output.width = 1440.;
        m.output.height = 900.;
        m.shell.bar_exclusive_zone = 25.;
        m
    }
    fn drag(q: &mut QuickAdjust, m: &Model, values: &[f32]) -> Vec<Action> {
        let mut out = Vec::new();
        q.build(m);
        let card = q.card_rect(m);
        let x = |v: f32| card.x + v / 100. * WIDTH;
        out.extend(q.pointer(Pointer::Motion { x: x(values[0]), y: card.y + 30. }, m).actions);
        out.extend(q.pointer(Pointer::Press { x: x(values[0]), y: card.y + 30., button: Button::Left }, m).actions);
        for v in &values[1..] {
            out.extend(q.pointer(Pointer::Motion { x: x(*v), y: card.y + 30. }, m).actions);
        }
        let last = *values.last().unwrap();
        out.extend(q.pointer(Pointer::Release { x: x(last), y: card.y + 30., button: Button::Left }, m).actions);
        out
    }
    #[test]
    fn laptop_sends_odd_levels_and_never_finishes() {
        let m = model();
        let mut q = QuickAdjust::with_profile(true);
        q.show(Mode::Brightness, m.now);
        let actions = drag(&mut q, &m, &[37., 41.]);
        assert!(actions.iter().any(|a| matches!(a, Action::BrightnessSet(37))), "{actions:?}");
        assert!(actions.iter().any(|a| matches!(a, Action::BrightnessSet(41))), "{actions:?}");
        assert!(!actions.iter().any(|a| matches!(a, Action::BrightnessFinish(_))));
        let mut d = QuickAdjust::with_profile(false);
        d.show(Mode::Brightness, m.now);
        let actions = drag(&mut d, &m, &[37.]);
        assert!(actions.iter().all(|a| !matches!(a, Action::BrightnessSet(37))), "desktop evens: {actions:?}");
    }
    #[test]
    fn laptop_card_position_frozen_while_mapped() {
        let mut m = model();
        let mut q = QuickAdjust::with_profile(true);
        q.show(Mode::Volume, m.now);
        assert_eq!(q.card_rect(&m).y, 25.);
        m.shell.bar_exclusive_zone = 58.;
        assert_eq!(q.card_rect(&m).y, 25., "QUIRK_QA_POSITION_FROZEN_WHILE_MAPPED");
        q.hide(m.now);
        q.tick(m.now + ms(CLOSE_FINALIZE_MS));
        q.show(Mode::Volume, m.now + ms(1000));
        assert_eq!(q.card_rect(&m).y, 58.);
        let mut d = QuickAdjust::with_profile(false);
        d.show(Mode::Volume, m.now);
        m.shell.bar_exclusive_zone = 25.;
        assert_eq!(d.card_rect(&m).y, 25., "desktop keeps live placement");
    }
}
