//! W7 ToggleTile — `ControlPanelToggle.qml` (ui-panels-control.md §2.2).
use super::kit::{self, Font, Tok};
use super::{Button, Control, Cursor, Interaction, PointerEvent, Response};
use crate::foundations::with_alpha;
use hyprv_ui::{
    animation::Color,
    geometry::Rect,
    scene::{Content, Node},
    skia::{self as sk, svg},
};
use std::time::Duration;

#[derive(Clone)]
pub enum ToggleIcon {
    /// Nerd Font glyph.
    Glyph(String),
    /// SVG drawn colourised with the icon colour (MultiEffect colorization: 1).
    Svg(svg::Dom),
}

impl std::fmt::Debug for ToggleIcon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Glyph(g) => write!(f, "Glyph({g:?})"),
            Self::Svg(_) => write!(f, "Svg(..)"),
        }
    }
}

impl ToggleIcon {
    /// Parse SVG bytes (e.g. `assets/control-panel/light-dark.svg`).
    pub fn svg(bytes: &[u8]) -> Option<Self> {
        svg::Dom::from_bytes(bytes, sk::FontMgr::default()).ok().map(Self::Svg)
    }
}

#[derive(Clone, Debug)]
pub struct ToggleTile {
    pub frame: Rect,
    pub icon: ToggleIcon,
    pub label: String,
    pub icon_only: bool,
    pub active: bool,
    pub destructive: bool,
    pub toggle_enabled: bool,
    pub state: Interaction,
}

impl ToggleTile {
    /// Control-panel tile: 62×62, icon-only (as every toggle in ControlPanelPopup.qml).
    pub fn new(icon: ToggleIcon) -> Self {
        Self {
            frame: Rect::new(0., 0., 62., 62.),
            icon,
            label: String::new(),
            icon_only: true,
            active: false,
            destructive: false,
            toggle_enabled: true,
            state: Interaction::default(),
        }
    }
    fn sizes(&self) -> (f32, f32, f32, f32) {
        let (w, h) = (self.frame.width, self.frame.height);
        let bx = if self.icon_only {
            (w.min(h) - 12.).clamp(22., 30.)
        } else {
            (h - 16.).clamp(28., 34.)
        };
        let radius = (bx * 0.34).round();
        let px = if self.icon_only {
            (bx * 0.5).clamp(15., 18.)
        } else {
            (bx * 0.46).clamp(15., 18.)
        };
        let img = if self.icon_only {
            (bx * 0.74).clamp(18., 22.)
        } else {
            (bx * 0.64).clamp(18., 20.)
        };
        (bx, radius, px, img)
    }
    fn label_visible(&self) -> bool {
        !self.icon_only && !self.label.is_empty()
    }
}

/// Qt MultiEffect `colorization: 1` as a recorded picture. MultiEffect does NOT tint to a
/// flat colour: `rgb = mix(rgb, colorizationColor.rgb * gray, colorization)` with
/// `gray = dot(rgb, (0.299, 0.587, 0.114))` — so a black SVG stays black whatever the
/// colour (verified: qml-ref/reference/toggle-svg-active--*.png shows a black icon on the
/// active tile, i.e. the light-dark icon never turns white in the live panel either).
pub(crate) fn tinted_svg(dom: &svg::Dom, size: f32, color: Color) -> Node {
    let mut rec = sk::PictureRecorder::new();
    let canvas = rec.begin_recording(sk::Rect::from_wh(size, size), false);
    let mut layer = sk::Paint::default();
    let (r, g, b) = (color.r, color.g, color.b);
    let (wr, wg, wb) = (0.299, 0.587, 0.114);
    #[rustfmt::skip]
    let m = sk::ColorMatrix::new(
        r * wr, r * wg, r * wb, 0., 0.,
        g * wr, g * wg, g * wb, 0., 0.,
        b * wr, b * wg, b * wb, 0., 0.,
        0., 0., 0., 1., 0.,
    );
    layer.set_color_filter(sk::color_filters::matrix(&m, None));
    canvas.save_layer(&sk::canvas::SaveLayerRec::default().paint(&layer));
    let mut dom = dom.clone();
    dom.set_container_size((size, size));
    dom.render(canvas);
    canvas.restore();
    let picture = rec.finish_recording_as_picture(None).expect("svg picture");
    Node::new(Rect::new(0., 0., size, size), Content::Picture(picture))
}

impl Control for ToggleTile {
    type Action = ();

    fn implicit_size(&self, _: &Tok) -> (f32, f32) {
        (self.frame.width, 72.)
    }

    fn frame(&self) -> Rect {
        self.frame
    }

    fn build(&self, tok: &Tok, _: Duration) -> Node {
        let f = self.frame;
        let p = &tok.pal;
        let accent = if self.destructive { p.critical } else { p.accent };
        let active_fill = with_alpha(accent, if tok.dark() { 0.28 } else { 0.22 });
        let active_stroke = with_alpha(accent, if tok.dark() { 0.42 } else { 0.28 });
        let (pressed, hovered) = (self.state.pressed.is_some(), self.state.hovered);
        let base = if self.active { active_fill } else { p.card_fill };
        let fill = if !self.toggle_enabled {
            Color { a: 0.48, ..base }
        } else if pressed {
            kit::qt_darker(base, 1.06)
        } else if hovered {
            kit::qt_lighter(base, 1.04)
        } else {
            base
        };
        let stroke = if !self.toggle_enabled {
            p.card_stroke
        } else if hovered {
            if self.active {
                kit::qt_lighter(active_stroke, 1.08)
            } else {
                kit::qt_lighter(p.card_stroke, 1.04)
            }
        } else if self.active {
            active_stroke
        } else {
            p.card_stroke
        };
        let mut root = kit::group(f);
        root.add(kit::qt_rect(
            Rect::new(0., 0., f.width, f.height),
            19.,
            fill,
            Some((1., stroke)),
        ));

        // Column { anchors.centerIn; spacing: iconOnly ? 0 : 6 } { iconBox; label }
        let (bx, radius, px, img) = self.sizes();
        let icon_color = if self.active {
            crate::foundations::rgb("#ffffff")
        } else {
            p.primary_text
        };
        let label_font = Font {
            px: 10.,
            weight: kit::BOLD,
        };
        let label_w = (f.width - 14.).max(40.);
        let lines = self.label.split('\n').count() as f32;
        let label_h = if self.label_visible() {
            (lines * 1.32 * kit::qt_px(10.)).ceil()
        } else {
            0.
        };
        let (col_w, col_h) = if self.label_visible() {
            (bx.max(label_w), bx + 6. + label_h)
        } else {
            (bx, bx)
        };
        let (cx, cy) = kit::center_in(f.width, f.height, col_w, col_h);
        let mut boxn = kit::group(Rect::new(cx + kit::hc(col_w) - kit::hc(bx), cy, bx, bx));
        if !self.icon_only {
            let well = tok.icon_well_fill(self.active);
            let well = if pressed {
                kit::qt_darker(well, 1.05)
            } else if hovered {
                kit::qt_lighter(well, 1.03)
            } else {
                well
            };
            boxn.add(kit::qt_rect(Rect::new(0., 0., bx, bx), radius, well, None));
        }
        match &self.icon {
            ToggleIcon::Glyph(g) => {
                boxn.add(kit::centered_text(
                    g,
                    bx,
                    bx,
                    &Font { px, weight: kit::BOLD },
                    icon_color,
                ));
            }
            ToggleIcon::Svg(dom) => {
                let (x, y) = kit::center_in(bx, bx, img, img);
                let mut n = tinted_svg(dom, img, icon_color);
                n.frame.x = x;
                n.frame.y = y;
                boxn.add(n);
            }
        }
        root.add(boxn);
        if self.label_visible() {
            let color = if self.active {
                crate::foundations::rgb("#ffffff")
            } else {
                p.primary_text
            };
            let lx = cx + kit::hc(col_w) - kit::hc(label_w);
            let line_h = 1.32 * kit::qt_px(10.);
            for (i, line) in self.label.split('\n').enumerate() {
                let m = kit::measure(line, 10., kit::BOLD);
                let x = lx + (label_w - m.width) / 2.;
                root.add(kit::text(
                    line,
                    x,
                    cy + bx + 6. + i as f32 * line_h,
                    None,
                    &label_font,
                    color,
                ));
            }
        }
        kit::fade(root, if self.toggle_enabled { 1. } else { 0.56 })
    }

    fn pointer(&mut self, ev: PointerEvent, _: Duration) -> Response<()> {
        if !self.toggle_enabled {
            self.state = Interaction::default();
            return Response::default();
        }
        let feed = self
            .state
            .feed(ev, (self.frame.width, self.frame.height), &[Button::Left]);
        let mut r = Response::redraw(feed.changed).with_cursor(Cursor::Pointer);
        if feed.clicked == Some(Button::Left) {
            r = r.act(());
        }
        r
    }
}
