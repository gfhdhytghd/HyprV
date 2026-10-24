//! Qt Quick building blocks used by the island: Behavior animation, anchoring, text
//! boxes and rectangles with the exact rules established in `hyprv-widgets`
//! (components.md §0.2: Qt advance/implicit-width metrics, odd-size anchor halves).
use hyprv_ui::{
    animation::{Behavior, Color, Direction, Easing, Family, Interpolate},
    geometry::Rect,
    scene::{Border, Content, Node, RenderType, Text},
    skia as sk,
};
use hyprv_widgets::{
    foundations::BASE_FONT,
    primitives::{rounded_rect, FontSpec, Label},
};
use std::time::Duration;

pub const CJK_FALLBACK: &str = "HarmonyOS Sans SC";
pub const BOLD: i32 = 700;
pub const DEMIBOLD: i32 = 600;
/// `Font.Black` (900) resolves to the ExtraBold face (no Black face installed).
pub const BLACK: i32 = 800;

pub fn out_cubic() -> Easing {
    Easing::new(Family::Cubic, Direction::Out)
}
pub fn out_quad() -> Easing {
    Easing::new(Family::Quad, Direction::Out)
}

/// QML `Behavior on x { NumberAnimation }` (ui-island.md §0.3): the first assignment
/// snaps, later target changes restart from the current animated value with the full
/// duration, equal targets are ignored, a disabled Behavior applies instantly.
#[derive(Clone, Debug)]
pub struct Anim<T: Interpolate = f64> {
    b: Option<Behavior<T>>,
    target: Option<T>,
    duration: Duration,
    easing: Easing,
    until: Duration,
    pub enabled: bool,
}
impl<T: Interpolate> Anim<T> {
    pub fn new(ms: u64, easing: Easing) -> Self {
        Self { b: None, target: None, duration: Duration::from_millis(ms), easing, until: Duration::ZERO, enabled: true }
    }
    pub fn set(&mut self, v: T, now: Duration) {
        if self.target.as_ref() == Some(&v) {
            return;
        }
        match (&mut self.b, self.enabled) {
            (Some(b), true) => {
                b.retarget(v.clone(), now);
                self.until = now + self.duration;
            }
            _ => {
                self.b = Some(Behavior::new(v.clone(), self.duration, self.easing.clone()));
                self.until = Duration::ZERO;
            }
        }
        self.target = Some(v);
    }
    /// QQuickBehavior retargets from the last property value actually presented.
    /// A binding that changes every frame must not advance its old job first.
    pub fn set_from_presented(&mut self, v: T, now: Duration, presented: Duration) {
        if self.target.as_ref() == Some(&v) { return; }
        if self.enabled {
            if let Some(old) = &self.b {
                let mut next = Behavior::new(old.value(presented.min(now)), self.duration, self.easing.clone());
                next.retarget(v.clone(), now);
                self.b = Some(next);
                self.target = Some(v);
                self.until = now + self.duration;
                return;
            }
        }
        self.set(v, now);
    }
    pub fn snap(&mut self, value: T) {
        self.b = Some(Behavior::new(value.clone(), self.duration, self.easing.clone()));
        self.target = Some(value);
        self.until = Duration::ZERO;
    }
    pub fn get(&self, now: Duration) -> T
    where
        T: Default,
    {
        self.b.as_ref().map_or_else(T::default, |b| b.value(now))
    }
    pub fn target(&self) -> Option<&T> {
        self.target.as_ref()
    }
    /// Whether this source animation already ran on an earlier presented tick.
    pub fn had_presented_tick(&self, presented: Duration) -> bool {
        self.until.checked_sub(self.duration).is_some_and(|start| start < presented && presented < self.until)
    }
    pub fn running(&self, now: Duration) -> bool {
        self.b.is_some() && now < self.until
    }
}

pub fn hc(v: f32) -> f32 {
    hyprv_ui::geometry::qt_center_half(v)
}
/// `anchors.verticalCenter` of an `h`-tall item in a `ph`-tall parent.
pub fn vcenter(ph: f32, h: f32) -> f32 {
    hc(ph) - hc(h)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Metrics {
    /// Qt `Text.implicitWidth` (advances + ceil'd ink overflow of the last glyph).
    pub width: f32,
    /// Qt `Text.implicitHeight`.
    pub height: f32,
    /// Sum of advances (what `AlignHCenter` centres).
    pub advance: f32,
}

pub fn measure(text: &str, px: f32, weight: i32) -> Metrics {
    let m = Label::new(text, FontSpec { family: BASE_FONT.into(), pixel_size: px, weight }, Color::rgba(0., 0., 0., 1.)).measure();
    Metrics { width: m.width, height: m.height, advance: m.advance }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

/// A one-line native-rendered `Text` whose Qt box is `(x, y, box_w, implicitHeight)`.
/// `box_w = None` = implicit width. Elides (…) only when the text does not fit.
#[allow(clippy::too_many_arguments)] // mirrors QML Text properties
pub fn text(s: &str, x: f32, y: f32, box_w: Option<f32>, align: HAlign, px: f32, weight: i32, color: Color) -> Node {
    let m = measure(s, px, weight);
    let bw = box_w.unwrap_or(m.width);
    let fits = bw + 0.01 >= m.advance;
    // Preserve Text item origin: QSG quantizes local line offsets separately
    // from the item transform (baking alignment into x loses glyph subpixel phase).
    let elide = !fits;
    Node::new(
        Rect::new(x, y, bw.max(0.), m.height),
        Content::Text(Text {
            render_type: RenderType::NativeRendering,
            text: s.to_owned(),
            families: vec![BASE_FONT.into(), CJK_FALLBACK.into()],
            size: px.trunc(),
            weight,
            color,
            letter_spacing: 0.,
            max_lines: Some(1),
            elide,
            align: match align { HAlign::Left => sk::textlayout::TextAlign::Left, HAlign::Center => sk::textlayout::TextAlign::Center, HAlign::Right => sk::textlayout::TextAlign::Right },
            hinting: sk::FontHinting::Slight,
            subpixel: true,
        }),
    )
}

/// `anchors.centerIn` text (implicit size) inside a `pw×ph` parent at `(px0, py0)`.
#[allow(clippy::too_many_arguments)]
pub fn centered_text(s: &str, x0: f32, y0: f32, pw: f32, ph: f32, px: f32, weight: i32, color: Color) -> Node {
    let m = measure(s, px, weight);
    text(s, x0 + hc(pw) - hc(m.width), y0 + vcenter(ph, m.height), None, HAlign::Left, px, weight, color)
}

/// Qt Quick `Rectangle` (radius clamped to min(w,h)/2 at paint time, inner border).
pub fn qt_rect(frame: Rect, radius: f32, fill: Color, border: Option<(f32, Color)>) -> Node {
    rounded_rect(
        frame,
        radius,
        fill,
        border.filter(|(w, c)| *w > 0. && c.a > 0.).map(|(width, color)| Border { width, color }),
    )
}

pub fn group(frame: Rect) -> Node {
    Node::new(frame, Content::Group)
}

/// Item `clip: true` (rectangular).
pub fn clip_group(frame: Rect) -> Node {
    let mut n = group(frame);
    n.clip = Some([0.; 4]);
    n
}

/// Scale about the item centre (`transformOrigin: Item.Center`).
pub fn scale_about_center(node: &mut Node, s: f32) {
    if (s - 1.).abs() < 1e-6 {
        return;
    }
    let (cx, cy) = (node.frame.width / 2., node.frame.height / 2.);
    let mut m = sk::Matrix::translate((cx, cy));
    m.pre_scale((s, s), None);
    m.pre_translate((-cx, -cy));
    node.transform = m;
}

pub fn contains(r: Rect, x: f32, y: f32) -> bool {
    x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height
}

/// QML jobs created during frame polish begin on the next event-loop turn.
#[derive(Clone, Debug)]
pub(crate) struct FrameBehavior {
    animation: Anim,
    target: f64,
    pending: Option<f64>,
}
impl FrameBehavior {
    pub fn new() -> Self {
        let mut animation=Anim::new(120,out_cubic()); animation.snap(1.);
        Self {animation,target:1.,pending:None}
    }
    pub fn request(&mut self,target:f64,now:Duration,during_frame:bool) {
        if self.target==target {return;}
        self.target=target;
        if during_frame {
            self.animation.snap(self.animation.get(now));
            self.pending=Some(target);
        } else {
            self.pending=None;
            self.animation.set(target,now);
        }
    }
    pub fn begin_frame(&mut self,now:Duration) {
        if let Some(target)=self.pending.take() {self.animation.set(target,now);}
    }
    pub fn value(&self,now:Duration)->f64 {self.animation.get(now)}
    pub fn running(&self,now:Duration)->bool {self.pending.is_some() || self.animation.running(now)}
}
