//! Crate-private stopgaps for engine APIs that are missing or not yet Qt-exact.
//! Each item maps to a request in `docs/requests/widgets.md` (R1–R8) and will be
//! replaced by the engine's foundations/primitives once they land.
use crate::foundations::{with_alpha, Palette, BASE_FONT};
use hyprv_ui::{
    animation::{Color, Direction, Easing, Family},
    geometry::Rect,
    scene::{Content, Node, Text},
    skia::{self as sk, FontMgr, FontStyle},
};

pub(crate) const CJK_FALLBACK: &str = "HarmonyOS Sans SC";

// ---------------------------------------------------------------- geometry (R2)

/// Qt anchor half-size (`foundations::hcenter`, R2 done).
pub(crate) fn hc(v: f32) -> f32 {
    crate::foundations::hcenter(v)
}

/// Qt `anchors.centerIn` of a `w×h` item inside `parent` (parent-local result).
pub(crate) fn center_in(pw: f32, ph: f32, w: f32, h: f32) -> (f32, f32) {
    (hc(pw) - hc(w), hc(ph) - hc(h))
}

pub(crate) fn vcenter(ph: f32, h: f32) -> f32 {
    hc(ph) - hc(h)
}

// ---------------------------------------------------------------- easing

pub(crate) fn out_cubic() -> Easing {
    Easing::new(Family::Cubic, Direction::Out)
}
pub(crate) fn in_out_quad() -> Easing {
    Easing::new(Family::Quad, Direction::InOut)
}

/// QML `Behavior on prop { NumberAnimation/ColorAnimation }`: snaps on first assignment
/// (Behaviors don't run at creation), restarts from the current animated value with the
/// full duration when the bound target changes, applies instantly while disabled.
#[derive(Clone, Debug)]
pub struct Anim<T: hyprv_ui::animation::Interpolate> {
    b: Option<hyprv_ui::animation::Behavior<T>>,
    target: Option<T>,
    duration: std::time::Duration,
    easing: Easing,
    until: std::time::Duration,
    pub enabled: bool,
}

impl<T: hyprv_ui::animation::Interpolate> Anim<T> {
    pub fn new(ms: u64, easing: Easing) -> Self {
        Self {
            b: None,
            target: None,
            duration: std::time::Duration::from_millis(ms),
            easing,
            until: std::time::Duration::ZERO,
            enabled: true,
        }
    }
    /// Assign the bound value (QML binding re-evaluation) at time `now`.
    pub fn set(&mut self, v: T, now: std::time::Duration) {
        if self.target.as_ref() == Some(&v) {
            return;
        }
        match (&mut self.b, self.enabled) {
            (Some(b), true) => {
                b.retarget(v.clone(), now);
                self.until = now + self.duration;
            }
            _ => {
                self.b = Some(hyprv_ui::animation::Behavior::new(
                    v.clone(),
                    self.duration,
                    self.easing.clone(),
                ));
                self.until = std::time::Duration::ZERO;
            }
        }
        self.target = Some(v);
    }
    pub fn get_or(&self, now: std::time::Duration, fallback: T) -> T {
        self.b.as_ref().map_or(fallback, |b| b.value(now))
    }
    pub fn running(&self, now: std::time::Duration) -> bool {
        self.b.is_some() && now < self.until
    }
}

// ---------------------------------------------------------------- colours (R4, R5)

/// Theme tokens for controls/viz: `foundations::Palette` (R4 done) plus the two chip
/// states that only WifiActionChip uses.
#[derive(Clone, Debug, PartialEq)]
pub struct Tok {
    pub pal: Palette,
    pub island_soft_fill: Color,
    pub island_stroke: Color,
    pub island_hover_fill: Color,
    pub island_muted_text: Color,
    pub accent_soft: Color,
    pub accent_fill: Color,
    pub accent_stroke: Color,
    pub input_fill: Color,
    pub chip_fill: Color,
    pub chip_stroke: Color,
    pub chip_disabled: Color,
}

impl Tok {
    pub fn new(dark: bool) -> Self {
        let pal = Palette::new(dark);
        let p = pal.primary_text;
        Self {
            island_soft_fill: pal.island_soft_fill,
            island_stroke: pal.island_stroke,
            island_hover_fill: pal.island_hover_fill,
            island_muted_text: pal.island_muted_text,
            accent_soft: pal.accent_soft,
            accent_fill: pal.accent_fill,
            accent_stroke: pal.accent_stroke,
            input_fill: pal.input_fill,
            chip_fill: pal.chip_fill,
            chip_stroke: with_alpha(p, 0.10),
            chip_disabled: with_alpha(p, 0.06),
            pal,
        }
    }
    pub fn dark(&self) -> bool {
        self.pal.dark
    }
    /// `iconWellFill` of tiles/toggles (ControlPanelToggle.qml:32-34).
    pub fn icon_well_fill(&self, active: bool) -> Color {
        self.pal.icon_well_fill(active)
    }
}

/// `Qt.lighter(c, factor)` (`foundations::lighter`, Qt 16-bit HSV; R5 done).
pub(crate) fn qt_lighter(c: Color, factor: f64) -> Color {
    crate::foundations::lighter(c, factor)
}

/// `Qt.darker(c, factor)` (`foundations::darker`).
pub(crate) fn qt_darker(c: Color, factor: f64) -> Color {
    crate::foundations::darker(c, factor)
}

pub(crate) fn mul_alpha(mut c: Color, k: f32) -> Color {
    c.a *= k;
    c
}

// ---------------------------------------------------------------- text (R1, R3)

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Metrics {
    /// Qt `Text.implicitWidth` (advances + ceil'd ink overflow of the last glyph).
    pub width: f32,
    /// Qt `Text.implicitHeight` = ceil(1.32 * int px).
    pub height: f32,
    /// Sum of advances only.
    pub advance: f32,
}

/// Application default font for QML Texts without `font.family` (fontconfig sans-serif).
pub(crate) const DEFAULT_FAMILY: &str = "sans-serif";

/// Qt `font.pixelSize` is an int (QML ToInt32 truncation).
pub(crate) fn qt_px(px: f32) -> f32 {
    px.trunc()
}

/// Measure like Qt 6.12 NativeRendering on this machine (components.md §0.2) — engine
/// `primitives::Label::measure` (R1 done; verified against the measured table below).
pub(crate) fn measure(text: &str, px: f32, weight: i32) -> Metrics {
    measure_in(BASE_FONT, text, px, weight, 0.)
}

pub(crate) fn measure_in(family: &str, text: &str, px: f32, weight: i32, spacing: f32) -> Metrics {
    let mut label = crate::primitives::Label::new(
        text,
        crate::primitives::FontSpec {
            family: family.into(),
            pixel_size: px,
            weight,
        },
        Color::rgba(0., 0., 0., 1.),
    );
    label.letter_spacing = spacing;
    let m = label.measure();
    Metrics {
        width: m.width,
        height: m.height,
        advance: m.advance,
    }
}

/// Unrounded ascent + descent (what Qt's AlignVCenter centres inside an explicit height).
pub(crate) fn line_height_exact(family: &str, px: f32, weight: i32) -> f32 {
    let style = FontStyle::new(
        sk::font_style::Weight::from(weight),
        sk::font_style::Width::NORMAL,
        sk::font_style::Slant::Upright,
    );
    FontMgr::default()
        .match_family_style(family, style)
        .map_or(px * 1.32, |tf| {
            let mut f = sk::Font::from_typeface(tf, qt_px(px));
            f.set_linear_metrics(true);
            let (_, m) = f.metrics();
            -m.ascent + m.descent
        })
}

/// Text style for nodes produced by controls/viz.
#[derive(Clone, Debug)]
pub(crate) struct Font {
    pub px: f32,
    pub weight: i32,
}

pub(crate) const BOLD: i32 = 700;
pub(crate) const DEMIBOLD: i32 = 600;
pub(crate) const MEDIUM: i32 = 500;
pub(crate) const NORMAL: i32 = 400;

/// A one-line text node whose *Qt text box* (implicit size) starts at (x, y).
/// `box_w` is the layout width (Qt `width`); `None` = implicit (no elision).
pub(crate) fn text(s: &str, x: f32, y: f32, box_w: Option<f32>, font: &Font, color: Color) -> Node {
    text_full(s, x, y, box_w, font, color, 0., true)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn text_full(
    s: &str,
    x: f32,
    y: f32,
    box_w: Option<f32>,
    font: &Font,
    color: Color,
    letter_spacing: f32,
    elide: bool,
) -> Node {
    text_in(BASE_FONT, s, x, y, box_w, font, color, letter_spacing, elide)
}

/// Like [`text_full`] in an explicit family (fallback chain appended).
#[allow(clippy::too_many_arguments)]
pub(crate) fn text_in(
    family: &str,
    s: &str,
    x: f32,
    y: f32,
    box_w: Option<f32>,
    font: &Font,
    color: Color,
    letter_spacing: f32,
    elide: bool,
) -> Node {
    let m = measure_in(family, s, font.px, font.weight, letter_spacing);
    let (w, elide) = match box_w {
        Some(bw) if bw + 0.01 < m.advance => (bw.max(0.), elide),
        // Room to spare: never let the shaper wrap/elide on rounding slack.
        _ => (m.width.max(box_w.unwrap_or(0.)) + 2., false),
    };
    Node::new(
        Rect::new(x, y, w, m.height),
        Content::Text(Text {
            render_type: hyprv_ui::scene::RenderType::NativeRendering,
            text: s.to_owned(),
            families: vec![family.into(), CJK_FALLBACK.into()],
            size: qt_px(font.px),
            weight: font.weight,
            color,
            letter_spacing,
            max_lines: Some(1),
            elide,
            align: sk::textlayout::TextAlign::Left,
            hinting: sk::FontHinting::Slight,
            subpixel: true,
        }),
    )
}

/// Switch every text node in the subtree to Qt's default (distance-field) rendering, for
/// QML Texts that set no `renderType`.
pub(crate) fn qt_rendering(mut node: Node) -> Node {
    fn walk(n: &mut Node) {
        if let Content::Text(t) = &mut n.content {
            t.render_type = hyprv_ui::scene::RenderType::QtRendering;
        }
        n.children.iter_mut().for_each(walk);
    }
    walk(&mut node);
    node
}

/// Text centred in a `pw×ph` parent the way `anchors.centerIn` does.
pub(crate) fn centered_text(s: &str, pw: f32, ph: f32, font: &Font, color: Color) -> Node {
    let m = measure(s, font.px, font.weight);
    let (x, y) = center_in(pw, ph, m.width, m.height);
    text(s, x, y, None, font, color)
}

/// Text box between `left`/`right` margins with Qt `AlignHCenter`, vertically centred.
/// Qt centres the line's *advance* width; the last glyph's ink overflow is ignored here
/// (verified: tm-launcher reference, U+F303@17 sits (17.19 − 10.19)/2 = 3.5 px right).
pub(crate) fn hcentered_text(s: &str, left: f32, box_w: f32, ph: f32, font: &Font, color: Color) -> Node {
    let m = measure(s, font.px, font.weight);
    let x = left + ((box_w - m.advance) / 2.);
    text(s, x, vcenter(ph, m.height), None, font, color)
}

// ---------------------------------------------------------------- shapes (R6, R7)

/// Qt Quick Rectangle (border ring replaces the fill under it; R7 done in the engine).
pub(crate) fn qt_rect(frame: Rect, radius: f32, fill: Color, border: Option<(f32, Color)>) -> Node {
    crate::primitives::rounded_rect(
        frame,
        radius,
        fill,
        border
            .filter(|(w, c)| *w > 0. && c.a > 0.)
            .map(|(width, color)| hyprv_ui::scene::Border { width, color }),
    )
}

/// QML item opacity: the engine's default `OpacityMode::PerNode` multiplies each node like
/// Qt's scene graph (R8 done), so this just scales `Node.opacity`.
pub(crate) fn fade(mut node: Node, k: f32) -> Node {
    node.opacity *= k;
    node
}

/// Scale a node about its own centre (Qt `transformOrigin: Item.Center`).
pub(crate) fn scale_about_center(node: &mut Node, s: f32) {
    let (cx, cy) = (node.frame.width / 2., node.frame.height / 2.);
    let mut m = sk::Matrix::translate((cx, cy));
    m.pre_scale((s, s), None);
    m.pre_translate((-cx, -cy));
    node.transform = m;
}

pub(crate) fn group(frame: Rect) -> Node {
    Node::new(frame, Content::Group)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hcenter_matches_qt_for_fractional_sizes() {
        assert_eq!(hc(29.0), 15.0);
        assert!((hc(29.3) - 15.15).abs() < 1e-5);
        assert!((hc(120.16) - 60.08).abs() < 1e-5);
        assert_eq!(hc(62.0), 31.0);
    }

    #[test]
    fn lighter_on_white_is_noop_and_darker_matches_qt() {
        let w = Color::rgba(1., 1., 1., 0.22);
        assert_eq!(qt_lighter(w, 1.04), w);
        let d = qt_darker(w, 1.08);
        // 65535*100/108 = 60680 → 60680/65535
        assert!((d.r - 60680. / 65535.).abs() < 1e-6, "{d:?}");
        assert_eq!(d.a, 0.22);
        // activeWorkspaceBackground: Qt.darker("#8e90cb", 1.05) == #8789c1 (ui-bar.md §5.1)
        let c = qt_darker(crate::foundations::rgb("#8e90cb"), 1.05);
        let to8 = |x: f32| ((x * 65535.).round() as u32 + 128 - (((x * 65535.).round() as u32 + 128) >> 8)) >> 8;
        assert_eq!((to8(c.r), to8(c.g), to8(c.b)), (0x87, 0x89, 0xc1));
    }

    #[test]
    fn measured_qt_metrics() {
        // components.md §0.2 — measured in Qt 6.12
        let cases: &[(&str, f32, f32, f32)] = &[
            ("0000000000", 16., 95.9375, 22.),
            ("0000000000", 17., 101.875, 23.),
            ("0000000000", 11., 65.9375, 15.),
            ("\u{f028}", 16., 18.59375, 22.),
            ("\u{f023}", 16., 12.59375, 22.),
            ("\u{f011}", 16., 13.59375, 22.),
            ("\u{f303}", 17., 17.1875, 23.),
            ("\u{f0a2}", 14., 12.390625, 19.),
            ("\u{f04ae}", 26., 15.59375, 35.),
            ("\u{f00df}", 21.08, 19.59375, 28.),
            ("12:34", 16., 47.96875, 22.),
            ("Close", 13., 38.984375, 18.),
        ];
        for (s, px, w, h) in cases {
            let m = measure(s, *px, BOLD);
            assert!((m.width - w).abs() < 0.02, "{s:?}@{px}: {} vs {w}", m.width);
            assert_eq!(m.height, *h, "{s:?}@{px}");
        }
    }

    #[test]
    fn cjk_fallback_is_one_em() {
        let m = measure("系统资源", 15., BOLD);
        assert!((m.width - 60.).abs() < 0.5, "{}", m.width);
    }
}
