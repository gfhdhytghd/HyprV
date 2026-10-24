//! W3 CircleGlyphButton — island `MediaGlyphButton` (`DynamicIsland.qml:446-493`,
//! ui-island.md §3.4).
use super::kit::{self, Anim, Font, Tok};
use super::{Button, Control, Cursor, Interaction, PointerEvent, Response};
use hyprv_ui::{geometry::Rect, scene::Node};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct CircleGlyphButton {
    pub frame: Rect,
    pub glyph: String,
    pub primary: bool,
    pub state: Interaction,
    scale: Anim<f64>,
}

impl CircleGlyphButton {
    pub fn new(glyph: impl Into<String>, primary: bool) -> Self {
        let d = if primary { 44. } else { 40. };
        let mut s = Self {
            frame: Rect::new(0., 0., d, d),
            glyph: glyph.into(),
            primary,
            state: Interaction::default(),
            scale: Anim::new(120, kit::out_cubic()),
        };
        s.update(&Tok::new(false), Duration::ZERO);
        s
    }
    fn target_scale(&self) -> f64 {
        if self.state.pressed.is_some() {
            0.92
        } else if self.state.hovered {
            1.06
        } else {
            1.
        }
    }
}

impl Control for CircleGlyphButton {
    type Action = ();

    fn implicit_size(&self, _: &Tok) -> (f32, f32) {
        let d = if self.primary { 44. } else { 40. };
        (d, d)
    }

    fn frame(&self) -> Rect {
        self.frame
    }

    fn update(&mut self, _: &Tok, now: Duration) {
        let t = self.target_scale();
        self.scale.set(t, now);
    }

    fn build(&self, tok: &Tok, now: Duration) -> Node {
        let f = self.frame;
        let mut root = kit::group(f);
        let (fill, border) = if self.primary {
            (tok.accent_soft, None)
        } else {
            (tok.island_soft_fill, Some((1., tok.island_stroke)))
        };
        let circle = kit::qt_rect(Rect::new(0., 0., f.width, f.height), f.height / 2., fill, border);
        let opacity = if self.state.hovered || self.primary { 1. } else { 0.82 };
        root.add(kit::fade(circle, opacity));
        let font = Font {
            px: if self.primary { 21. } else { 17. },
            weight: kit::BOLD,
        };
        let color = if self.primary {
            crate::foundations::rgb("#ffffff")
        } else {
            tok.pal.primary_text
        };
        let mut glyph = kit::centered_text(&self.glyph, f.width, f.height, &font, color);
        if self.primary {
            glyph.frame.y += 1.; // anchors.verticalCenterOffset: 1
        }
        root.add(glyph);
        let s = self.scale.get_or(now, 1.) as f32;
        if (s - 1.).abs() > 1e-6 {
            kit::scale_about_center(&mut root, s);
        }
        root
    }

    fn pointer(&mut self, ev: PointerEvent, now: Duration) -> Response<()> {
        let feed = self
            .state
            .feed(ev, (self.frame.width, self.frame.height), &[Button::Left]);
        self.update(&Tok::new(false), now);
        let mut r = Response::redraw(feed.changed).with_cursor(Cursor::Pointer);
        if feed.clicked == Some(Button::Left) {
            r = r.act(());
        }
        r
    }

    fn animating(&self, now: Duration) -> bool {
        self.scale.running(now)
    }
}
