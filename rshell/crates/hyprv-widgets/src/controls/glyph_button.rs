//! W4 GlyphButton — glyph-only button without chrome.
//! Covers `ControlPanelMediaCard.MediaButton` (`ControlPanelMediaCard.qml:53-93`), the tray
//! overflow trigger `⋯` (ui-bar-layout.md §8.5) and the notification trigger with its red
//! dot (ui-bar-layout.md §8.7).
use super::kit::{self, Font, Tok};
use super::{Button, Control, Cursor, Interaction, PointerEvent, Response};
use hyprv_ui::{animation::Color, geometry::Rect, scene::Node};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GlyphAnchor {
    /// `anchors.centerIn: parent`
    Center,
    /// `anchors.horizontalCenter` + `anchors.bottom` with this bottom margin.
    Bottom(f32),
}

/// A small text badge anchored top-right (`topMargin`, `rightMargin`).
#[derive(Clone, Debug)]
pub struct GlyphBadge {
    pub glyph: String,
    pub px: f32,
    pub color: Color,
    pub top: f32,
    pub right: f32,
}

#[derive(Clone, Debug)]
pub struct GlyphButton {
    pub frame: Rect,
    pub glyph: String,
    pub px: f32,
    pub weight: i32,
    /// `None` = primaryText.
    pub color: Option<Color>,
    pub anchor: GlyphAnchor,
    pub enabled: bool,
    /// Opacity applied when `!enabled` (MediaButton: 0.6).
    pub disabled_opacity: f32,
    pub buttons: Vec<Button>,
    pub badge: Option<GlyphBadge>,
    pub state: Interaction,
}

impl GlyphButton {
    pub fn new(glyph: impl Into<String>, px: f32, frame: Rect) -> Self {
        Self {
            frame,
            glyph: glyph.into(),
            px,
            weight: kit::BOLD,
            color: None,
            anchor: GlyphAnchor::Center,
            enabled: true,
            disabled_opacity: 1.,
            buttons: vec![Button::Left],
            badge: None,
            state: Interaction::default(),
        }
    }

    /// Media card button (`ControlPanelMediaCard.qml:21-52`): primary 41×34 glyph 31 px black,
    /// secondary 34×34 glyph 26 px primaryText; bottom margins 3 / 6.
    pub fn media(glyph: impl Into<String>, primary: bool) -> Self {
        let (w, px, margin) = if primary { (41., 31., 3.) } else { (34., 26., 6.) };
        let mut b = Self::new(glyph, px, Rect::new(0., 0., w, 34.));
        b.anchor = GlyphAnchor::Bottom(margin);
        b.disabled_opacity = 0.6;
        if primary {
            b.color = Some(crate::foundations::rgb("#000000"));
        }
        b
    }

    /// Bar notification trigger: glyph 14 px, width glyph + 10, red U+F444 dot when `dot`.
    pub fn notification(dnd: bool, dot: bool) -> Self {
        let glyph = if dnd { "\u{f009b}" } else { "\u{f0a2}" };
        let w = kit::measure(glyph, 14., kit::BOLD).width + 10.;
        let mut b = Self::new(glyph, 14., Rect::new(0., 0., w, 38.));
        b.buttons = vec![Button::Left, Button::Right];
        if dot {
            b.badge = Some(GlyphBadge {
                glyph: "\u{f444}".into(),
                px: 10.,
                color: crate::foundations::rgb("#ff0000"),
                top: 4.,
                right: 2.,
            });
        }
        b
    }

    /// Tray overflow trigger: 22×38, `⋯` 18 px centred.
    pub fn tray_overflow() -> Self {
        Self::new("\u{22ef}", 18., Rect::new(0., 0., 22., 38.))
    }
}

impl Control for GlyphButton {
    type Action = Button;

    fn implicit_size(&self, _: &Tok) -> (f32, f32) {
        (self.frame.width, self.frame.height)
    }

    fn frame(&self) -> Rect {
        self.frame
    }

    fn build(&self, tok: &Tok, _: Duration) -> Node {
        let f = self.frame;
        let mut root = kit::group(f);
        let font = Font {
            px: self.px,
            weight: self.weight,
        };
        let color = self.color.unwrap_or(tok.pal.primary_text);
        let m = kit::measure(&self.glyph, self.px, self.weight);
        let (x, y) = match self.anchor {
            GlyphAnchor::Center => kit::center_in(f.width, f.height, m.width, m.height),
            GlyphAnchor::Bottom(margin) => (kit::hc(f.width) - kit::hc(m.width), f.height - margin - m.height),
        };
        root.add(kit::text(&self.glyph, x, y, None, &font, color));
        if let Some(b) = &self.badge {
            let bm = kit::measure(&b.glyph, b.px, kit::BOLD);
            root.add(kit::text(
                &b.glyph,
                f.width - b.right - bm.width,
                b.top,
                None,
                &Font {
                    px: b.px,
                    weight: kit::BOLD,
                },
                b.color,
            ));
        }
        if self.enabled {
            root
        } else {
            kit::fade(root, self.disabled_opacity)
        }
    }

    fn update(&mut self, _: &Tok, _: Duration) {
        if !self.enabled {
            self.state = Interaction::default();
        }
    }
    fn pointer(&mut self, ev: PointerEvent, _: Duration) -> Response<Button> {
        if !self.enabled {
            self.state = Interaction::default();
            return Response::default();
        }
        let feed = self
            .state
            .feed(ev, (self.frame.width, self.frame.height), &self.buttons);
        let mut r = Response::default().with_cursor(Cursor::Pointer);
        if let Some(b) = feed.clicked {
            r = r.act(b);
        }
        r
    }
}
