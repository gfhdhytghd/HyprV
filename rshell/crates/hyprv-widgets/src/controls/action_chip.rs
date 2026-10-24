//! W5 ActionChip — `WifiActionChip.qml` (ui-panels.md §2.4) and the legacy
//! `ControlPanelWifiDetails.InlineChip` (ui-panels-control.md §8) as `ActionChip::inline`.
use super::kit::{self, Font, Tok};
use super::{Button, Control, Cursor, Interaction, PointerEvent, Response};
use hyprv_ui::{animation::Color, geometry::Rect, scene::Node};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub enum ChipContent {
    /// 13 px Bold baseFont (inline variant: 11 px Bold).
    Label(String),
    /// Icon glyph, `icon_pixel_size` Bold iconFont.
    Icon(String),
}

#[derive(Clone, Debug)]
pub struct ActionChip {
    pub frame: Rect,
    pub content: ChipContent,
    pub disabled: bool,
    /// `None` = primaryText α0.08.
    pub fill: Option<Color>,
    /// `None` = primaryText.
    pub foreground: Option<Color>,
    /// `None` = primaryText α0.10.
    pub stroke: Option<Color>,
    pub minimum_width: f32,
    pub icon_pixel_size: f32,
    pub corner_radius: f32,
    /// Legacy InlineChip look (h 30, r 15, width text+22, 11 px, other hover/press factors).
    pub inline: bool,
    pub state: Interaction,
}

impl ActionChip {
    pub fn label(text: impl Into<String>) -> Self {
        Self::with(ChipContent::Label(text.into()))
    }
    pub fn icon(glyph: impl Into<String>) -> Self {
        Self::with(ChipContent::Icon(glyph.into()))
    }
    pub fn inline(text: impl Into<String>) -> Self {
        let mut c = Self::with(ChipContent::Label(text.into()));
        c.inline = true;
        c.corner_radius = 15.;
        c
    }
    fn with(content: ChipContent) -> Self {
        Self {
            frame: Rect::default(),
            content,
            disabled: false,
            fill: None,
            foreground: None,
            stroke: None,
            minimum_width: 0.,
            icon_pixel_size: 16.,
            corner_radius: 10.,
            inline: false,
            state: Interaction::default(),
        }
    }
    /// Builder: set the frame origin and adopt the implicit size.
    pub fn at(mut self, x: f32, y: f32, tok: &Tok) -> Self {
        let (w, h) = self.implicit_size(tok);
        self.frame = Rect::new(x, y, w, h);
        self
    }
    fn font(&self) -> Font {
        match (&self.content, self.inline) {
            (_, true) => Font {
                px: 11.,
                weight: kit::BOLD,
            },
            (ChipContent::Label(_), false) => Font {
                px: 13.,
                weight: kit::BOLD,
            },
            (ChipContent::Icon(_), false) => Font {
                px: self.icon_pixel_size,
                weight: kit::BOLD,
            },
        }
    }
    fn text(&self) -> &str {
        match &self.content {
            ChipContent::Label(s) | ChipContent::Icon(s) => s,
        }
    }
}

impl Control for ActionChip {
    type Action = ();

    fn implicit_size(&self, _: &Tok) -> (f32, f32) {
        let f = self.font();
        let w = kit::measure(self.text(), f.px, f.weight).width;
        if self.inline {
            (w + 22., 30.)
        } else {
            (self.minimum_width.max(w + 24.), 34.)
        }
    }

    fn frame(&self) -> Rect {
        self.frame
    }

    fn build(&self, tok: &Tok, _: Duration) -> Node {
        let f = self.frame;
        let p = tok.pal.primary_text;
        let fill = self.fill.unwrap_or(tok.chip_fill);
        let fg = self.foreground.unwrap_or(p);
        let stroke = self.stroke.unwrap_or(tok.chip_stroke);
        let (pressed, hovered) = (self.state.pressed.is_some(), self.state.hovered);
        let (bg, border, opacity) = if self.inline {
            // ControlPanelWifiDetails.qml:38-43
            let bg = if self.disabled {
                kit::mul_alpha(Color { a: 1., ..fill }, 0.45)
            } else if pressed {
                kit::qt_darker(fill, 1.05)
            } else if hovered {
                kit::qt_lighter(fill, 1.04)
            } else {
                fill
            };
            (bg, stroke, if self.disabled { 0.56 } else { 1. })
        } else {
            // WifiActionChip.qml:22-33
            let bg = if self.disabled {
                tok.chip_disabled
            } else if pressed {
                kit::qt_darker(fill, 1.08)
            } else if hovered {
                kit::qt_lighter(fill, 1.06)
            } else {
                fill
            };
            let border = if self.disabled {
                tok.chip_disabled
            } else if hovered {
                crate::foundations::with_alpha(fg, 0.22)
            } else {
                stroke
            };
            (bg, border, if self.disabled { 0.56 } else { 1. })
        };
        let mut root = kit::group(f);
        root.add(kit::qt_rect(
            Rect::new(0., 0., f.width, f.height),
            self.corner_radius,
            bg,
            Some((1., border)),
        ));
        root.add(kit::centered_text(self.text(), f.width, f.height, &self.font(), fg));
        kit::fade(root, opacity)
    }

    fn pointer(&mut self, ev: PointerEvent, _: Duration) -> Response<()> {
        if self.disabled {
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
