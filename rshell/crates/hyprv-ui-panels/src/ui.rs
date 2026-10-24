//! Shared scaffolding: theme tokens, node helpers that know their absolute position (Qt
//! text snapping needs it), interaction zones with QML MouseArea semantics, pointer and
//! key events, surface requirements, and a Behavior wrapper.
use crate::qtext::{self, TextBox};
use hyprv_ui::{
    animation::{Color, Direction, Easing, Family, Interpolate},
    geometry::Rect,
    scene::{Border, Content, Node, Rectangle, Fill},
};
pub use hyprv_widgets::foundations::{darker, lighter, rgb, with_alpha, Palette};
use std::time::Duration;

/// `panelColor(c)`: alpha × 0.82 (ui-panels.md §1.2).
pub fn panel_color(c: Color) -> Color {
    with_alpha(c, c.a * 0.82)
}

/// Qt 6.12 anchor centring (qquickanchors.cpp:46-64): with the default
/// `anchors.alignWhenCentered: true` both the parent's and the item's centre are
/// `qRound(size / 2)` (round half away from zero). (The odd-integer rule in older specs is
/// Qt 5's.)
pub fn hc(v: f32) -> f32 {
    let h = v as f64 / 2.;
    (if h >= 0. { (h + 0.5).floor() } else { (h - 0.5).ceil() }) as f32
}
/// Offset of a child of size `c` centred in a parent of size `p`.
pub fn center(p: f32, c: f32) -> f32 {
    hc(p) - hc(c)
}

// ------------------------------------------------------------------ build context

/// Build context: output scale for text snapping, theme tokens, and the zone list.
pub struct Ctx<'a, Z> {
    pub dpr: f32,
    pub pal: &'a Palette,
    pub zones: &'a mut Vec<Zone<Z>>,
}

impl<'a, Z: Clone> Ctx<'a, Z> {
    pub fn zone(&mut self, id: Z, abs: Rect, cursor: Cursor) {
        self.zones.push(Zone { id, rect: abs, cursor, enabled: true });
    }
    pub fn zone_if(&mut self, enabled: bool, id: Z, abs: Rect, cursor: Cursor) {
        self.zones.push(Zone { id, rect: abs, cursor, enabled });
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Zone<Z> {
    pub id: Z,
    /// Absolute (surface) logical rect.
    pub rect: Rect,
    pub cursor: Cursor,
    /// `MouseArea.enabled`: disabled zones neither react nor change the cursor nor
    /// block events (they fall through to zones below).
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Cursor {
    #[default]
    Default,
    Pointer,
}

/// Rect `r` relative to an absolute origin.
pub fn abs(origin: (f32, f32), r: Rect) -> Rect {
    Rect::new(origin.0 + r.x, origin.1 + r.y, r.width, r.height)
}
pub fn at(origin: (f32, f32), r: Rect) -> (f32, f32) {
    (origin.0 + r.x, origin.1 + r.y)
}

pub fn group(frame: Rect) -> Node {
    Node::new(frame, Content::Group)
}
pub fn group_with(frame: Rect, children: Vec<Node>) -> Node {
    let mut n = group(frame);
    n.children = children;
    n
}
/// Qt Rectangle: radius clamped to min(w,h)/2 at paint time; border inside the bounds
/// replaces the fill under it (the engine draws inner fill + ring).
pub fn rect(frame: Rect, radius: f32, fill: Color, border: Option<(f32, Color)>) -> Node {
    let border = border.filter(|b| b.0 > 0.);
    let r = radius.min(frame.width / 2.).min(frame.height / 2.).max(0.);
    Node::new(
        frame,
        Content::Rect(Rectangle {
            fill: Fill::Solid(fill),
            radii: [r; 4],
            border: border.map(|(width, color)| Border { width, color }),
        }),
    )
}
/// `clip: true` — rectangular clip of the children (Qt clips to the bounding rect).
pub fn clip(frame: Rect, children: Vec<Node>) -> Node {
    let mut n = group_with(frame, children);
    n.clip = Some([0.; 4]);
    n
}
/// Text item at `frame` (relative to the parent whose absolute origin is `parent_abs`).
pub fn text(tb: &TextBox, frame: Rect, parent_abs: (f32, f32), dpr: f32) -> Node {
    qtext::node(tb, frame, at(parent_abs, frame), dpr)
}
/// Size Qt gives the Text item.
pub fn text_size(tb: &TextBox) -> (f32, f32) {
    qtext::item_size(tb)
}
/// Text vertically centred in a parent of height `ph` at x.
pub fn text_vc(tb: &TextBox, x: f32, ph: f32, parent_abs: (f32, f32), dpr: f32) -> Node {
    let (w, h) = text_size(tb);
    text(tb, Rect::new(x, center(ph, h), w, h), parent_abs, dpr)
}
/// Text centred in a parent rect (`anchors.centerIn`).
pub fn text_center(tb: &TextBox, pw: f32, ph: f32, parent_abs: (f32, f32), dpr: f32) -> Node {
    let (w, h) = text_size(tb);
    text(tb, Rect::new(center(pw, w), center(ph, h), w, h), parent_abs, dpr)
}

/// Per-node opacity (Qt semantics: each node's alpha multiplied, no isolation).
pub fn faded(mut n: Node, opacity: f32) -> Node {
    n.opacity *= opacity;
    n
}

impl<A> Default for Reply<A> {
    fn default() -> Self {
        Self { actions: Vec::new(), cursor: None, redraw: false }
    }
}

// ------------------------------------------------------------------ events

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
}

/// Surface-local pointer events (logical px).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pointer {
    Motion { x: f32, y: f32 },
    Leave,
    /// The compositor broke the implicit grab (wl_pointer cancel/leave mid-press): like
    /// QtWayland's ungrab, every grab ends without a release (MouseArea `canceled`).
    Cancel,
    Press { x: f32, y: f32, button: Button },
    Release { x: f32, y: f32, button: Button },
    /// `angle_*` = Qt angleDelta (120 per notch); `pixel_*` = smooth-scroll pixel delta.
    Wheel { x: f32, y: f32, angle_x: f32, angle_y: f32, pixel_x: f32, pixel_y: f32 },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Key {
    Escape,
    Return,
    Enter,
    Space,
    Tab,
    BackTab,
    Left,
    Right,
    Backspace,
    Delete,
    Home,
    End,
    /// Text input (already composed by the IME/xkb layer).
    Text(String),
    AltF4,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyEvent {
    pub key: Key,
    pub pressed: bool,
    pub autorepeat: bool,
}

/// What the host should do after an event.
#[derive(Clone, Debug, PartialEq)]
pub struct Reply<A> {
    pub actions: Vec<A>,
    pub cursor: Option<Cursor>,
    /// Pixels changed or an animation started: call `update` and request a frame.
    pub redraw: bool,
}

/// MouseArea press/hover bookkeeping over a list of zones (topmost = last).
#[derive(Clone, Debug)]
pub struct Interaction<Z> {
    pub hovered: Option<Z>,
    pub pressed: Option<(Z, Button)>,
    pub pointer: Option<(f32, f32)>,
}
impl<Z> Default for Interaction<Z> {
    fn default() -> Self {
        Self { hovered: None, pressed: None, pointer: None }
    }
}

pub enum Hit<Z> {
    None,
    /// Hover changed.
    Hover,
    Pressed(Z, Button, f32, f32),
    Dragged(Z, f32, f32),
    /// Release (inside = click).
    Released { id: Z, button: Button, x: f32, y: f32, inside: bool },
    /// The grab ended without a release (Leave/Cancel while pressed): MouseArea
    /// `onCanceled`, never a click.
    Canceled { id: Z, button: Button },
}

impl<Z: Clone + PartialEq> Interaction<Z> {
    pub fn top(zones: &[Zone<Z>], x: f32, y: f32) -> Option<&Zone<Z>> {
        zones.iter().rev().find(|z| z.enabled && z.rect.contains(x, y))
    }
    pub fn cursor(&self, zones: &[Zone<Z>]) -> Cursor {
        let Some((x, y)) = self.pointer else { return Cursor::Default };
        if let Some((p, _)) = &self.pressed {
            if let Some(z) = zones.iter().rev().find(|z| &z.id == p) {
                return z.cursor;
            }
        }
        Self::top(zones, x, y).map(|z| z.cursor).unwrap_or_default()
    }
    pub fn feed(&mut self, ev: Pointer, zones: &[Zone<Z>]) -> Hit<Z> {
        match ev {
            Pointer::Motion { x, y } => {
                self.pointer = Some((x, y));
                if let Some((id, _)) = self.pressed.clone() {
                    // QML: hover stays with the grabbing area; containsMouse follows the pointer
                    let inside = zones.iter().rev().find(|z| z.id == id).is_some_and(|z| z.rect.contains(x, y));
                    let h = if inside { Some(id.clone()) } else { None };
                    let changed = h != self.hovered;
                    self.hovered = h;
                    let _ = changed;
                    return Hit::Dragged(id, x, y);
                }
                let h = Self::top(zones, x, y).map(|z| z.id.clone());
                if h != self.hovered {
                    self.hovered = h;
                    Hit::Hover
                } else {
                    Hit::None
                }
            }
            Pointer::Leave | Pointer::Cancel => {
                self.pointer = None;
                // QtWayland resets its button state on pointer leave: the grab is lost
                if let Some((id, button)) = self.pressed.take() {
                    self.hovered = None;
                    return Hit::Canceled { id, button };
                }
                if self.hovered.take().is_some() {
                    Hit::Hover
                } else {
                    Hit::None
                }
            }
            Pointer::Press { x, y, button } => {
                self.pointer = Some((x, y));
                if self.pressed.is_some() {
                    return Hit::None;
                }
                match Self::top(zones, x, y) {
                    Some(z) => {
                        let id = z.id.clone();
                        self.pressed = Some((id.clone(), button));
                        self.hovered = Some(id.clone());
                        Hit::Pressed(id, button, x, y)
                    }
                    None => Hit::None,
                }
            }
            Pointer::Release { x, y, button } => {
                self.pointer = Some((x, y));
                match self.pressed.clone() {
                    Some((id, b)) if b == button => {
                        self.pressed = None;
                        let inside = zones.iter().rev().find(|z| z.id == id).is_some_and(|z| z.rect.contains(x, y));
                        self.hovered = Self::top(zones, x, y).map(|z| z.id.clone());
                        Hit::Released { id, button, x, y, inside }
                    }
                    _ => Hit::None,
                }
            }
            Pointer::Wheel { .. } => Hit::None,
        }
    }
    pub fn is_hovered(&self, id: &Z) -> bool {
        self.hovered.as_ref() == Some(id)
    }
    pub fn is_pressed(&self, id: &Z) -> bool {
        self.pressed.as_ref().is_some_and(|(p, _)| p == id)
    }
}

// ------------------------------------------------------------------ surfaces

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Overlay,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardMode {
    None,
    OnDemand,
    Exclusive,
}

/// Layer-shell requirements for the host (ui-panels.md §3).
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceSpec {
    pub namespace: &'static str,
    pub layer: Layer,
    /// top, left, right, bottom
    pub anchors: [bool; 4],
    pub exclusive_zone: i32,
    pub keyboard: KeyboardMode,
    /// Explicit size (unanchored surfaces); `None` = follow anchors (full output).
    pub size: Option<(f32, f32)>,
    /// Input region (surface-local); `None` = whole surface.
    pub input_region: Option<Rect>,
    /// Output name the surface must be on.
    pub output: String,
}

// ------------------------------------------------------------------ animation

pub fn ease(f: Family, d: Direction) -> Easing {
    Easing::new(f, d)
}
pub fn ms(v: u64) -> Duration {
    Duration::from_millis(v)
}

/// QML `Behavior on p { … }`: retargets from the current animated value with the full
/// duration; disabled → jumps; no animation at creation; `running(now)` drives frames.
#[derive(Clone, Debug)]
pub struct Anim<T: Interpolate> {
    from: T,
    to: T,
    start: Duration,
    duration: Duration,
    easing: Easing,
    pub enabled: bool,
    active: bool,
}
impl<T: Interpolate> Anim<T> {
    pub fn new(v: T, duration_ms: u64, easing: Easing) -> Self {
        Self { from: v.clone(), to: v, start: Duration::ZERO, duration: ms(duration_ms), easing, enabled: true, active: false }
    }
    pub fn set(&mut self, v: T, now: Duration) {
        if v == self.to {
            return;
        }
        if !self.enabled || self.duration.is_zero() {
            self.snap(v);
            return;
        }
        self.from = self.get(now);
        self.to = v;
        self.start = now;
        self.active = true;
    }
    pub fn target(&self) -> &T {
        &self.to
    }
    pub fn get(&self, now: Duration) -> T {
        if !self.active {
            return self.to.clone();
        }
        let p = now.saturating_sub(self.start).as_secs_f64() / self.duration.as_secs_f64();
        if p >= 1. {
            return self.to.clone();
        }
        self.from.interpolate(&self.to, self.easing.value(p))
    }
    pub fn running(&self, now: Duration) -> bool {
        self.active && now < self.start + self.duration
    }
    pub fn snap(&mut self, v: T) {
        self.from = v.clone();
        self.to = v;
        self.active = false;
    }
    pub fn set_duration(&mut self, duration_ms: u64) {
        self.duration = ms(duration_ms);
    }
}
