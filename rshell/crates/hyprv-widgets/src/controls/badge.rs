//! W6 Badge — `WifiSecurityBadge.qml` (ui-panels.md §2.5).
use super::kit::{self, Font, Tok};
use super::{Control, PointerEvent, Response};
use crate::foundations::with_alpha;
use hyprv_ui::{geometry::Rect, scene::Node};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Badge {
    pub frame: Rect,
    pub glyph: String,
    pub active: bool,
    pub corner_radius: f32,
}

impl Badge {
    /// Lock badge as used in the Wi-Fi list (cornerRadius 9 from the caller).
    pub fn security(active: bool) -> Self {
        Self {
            frame: Rect::new(0., 0., 22., 20.),
            glyph: "\u{f023}".into(),
            active,
            corner_radius: 10.,
        }
    }
}

impl Control for Badge {
    type Action = ();

    fn implicit_size(&self, _: &Tok) -> (f32, f32) {
        (22., 20.)
    }

    fn frame(&self) -> Rect {
        self.frame
    }

    fn build(&self, tok: &Tok, _: Duration) -> Node {
        let f = self.frame;
        let p = tok.pal.primary_text;
        let mut root = kit::group(f);
        root.visible = self.active;
        root.add(kit::qt_rect(
            Rect::new(0., 0., f.width, f.height),
            self.corner_radius,
            with_alpha(p, if tok.dark() { 0.08 } else { 0.06 }),
            Some((1., with_alpha(p, 0.08))),
        ));
        root.add(kit::centered_text(
            &self.glyph,
            f.width,
            f.height,
            &Font {
                px: 11.,
                weight: kit::BOLD,
            },
            with_alpha(p, 0.82),
        ));
        root
    }

    fn pointer(&mut self, _: PointerEvent, _: Duration) -> Response<()> {
        Response::default()
    }
}
