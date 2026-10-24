//! W2 PillButton — animated text button.
//! - `PillStyle::Island`: `IslandTextButton` (`DynamicIsland.qml:322-393`, ui-island.md §3.3)
//! - `PillStyle::Header`: calendar `HeaderButton` (`WeekCalendarPanel.qml:44-79`,
//!   ui-panels-calendar.md §2.2)
use super::kit::{self, Anim, Font, Tok};
use super::{Button, Control, Cursor, Interaction, PointerEvent, Response};
use hyprv_ui::{
    animation::{Color, Easing},
    geometry::Rect,
    scene::Node,
};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub enum PillStyle {
    /// 38 high (collapsedHeight), hover fill fades (opacity 120 ms OutCubic), press scale
    /// 0.96 (120 ms OutCubic). Width = label + paddings. Label 16 px Bold.
    Island { padding_left: f32, padding_right: f32 },
    /// 34 high, radius 17, width max(36, label + 22). Fill colour animates (120 ms linear),
    /// press scale 0.96 (90 ms linear). Label 13 px DemiBold, centred.
    Header { emphasized: bool },
}

#[derive(Clone, Debug)]
pub struct PillButton {
    pub frame: Rect,
    pub label: String,
    pub style: PillStyle,
    /// Island style only: MouseArea.enabled (the clock is non-interactive: no hover, no cursor).
    pub interactive: bool,
    /// `None` = theme text colour (`island.textColor` / `root.textColor`).
    pub label_color: Option<Color>,
    pub state: Interaction,
    /// Theme from the last host `update`; `None` until the first one (WIDGETS-2: the fill
    /// colour must snap to the first real theme, not animate from a constructor default).
    tok: Option<Tok>,
    scale: Anim<f64>,
    hover: Anim<f64>,
    fill: Anim<Color>,
}

impl PillButton {
    pub fn island(label: impl Into<String>, padding_left: f32, padding_right: f32) -> Self {
        Self::new(
            label,
            PillStyle::Island {
                padding_left,
                padding_right,
            },
        )
    }
    pub fn header(label: impl Into<String>, emphasized: bool) -> Self {
        Self::new(label, PillStyle::Header { emphasized })
    }
    fn new(label: impl Into<String>, style: PillStyle) -> Self {
        let header = matches!(style, PillStyle::Header { .. });
        let mut s = Self {
            frame: Rect::default(),
            label: label.into(),
            style,
            interactive: true,
            label_color: None,
            state: Interaction::default(),
            tok: None,
            scale: Anim::new(
                if header { 90 } else { 120 },
                if header { Easing::Linear } else { kit::out_cubic() },
            ),
            hover: Anim::new(120, kit::out_cubic()),
            fill: Anim::new(120, Easing::Linear),
        };
        let (w, h) = s.implicit_size(&Tok::new(false));
        s.frame = Rect::new(0., 0., w, h);
        s.retarget_interaction(Duration::ZERO);
        s
    }

    /// Theme-independent Behaviors (press scale, hover fade).
    fn retarget_interaction(&mut self, now: Duration) {
        let pressed = self.state.pressed.is_some();
        self.scale.set(if pressed { 0.96 } else { 1. }, now);
        self.hover.set(if self.state.hovered { 1. } else { 0. }, now);
    }
    fn font(&self) -> Font {
        match self.style {
            PillStyle::Island { .. } => Font {
                px: 16.,
                weight: kit::BOLD,
            },
            PillStyle::Header { .. } => Font {
                px: 13.,
                weight: kit::DEMIBOLD,
            },
        }
    }
    fn mouse_enabled(&self) -> bool {
        match self.style {
            PillStyle::Island { .. } => self.interactive,
            PillStyle::Header { .. } => true,
        }
    }
    fn header_fill(&self, tok: &Tok) -> Color {
        let PillStyle::Header { emphasized } = self.style else {
            return Color::default();
        };
        if emphasized {
            tok.pal.accent
        } else if self.state.hovered {
            // elevatedColor: #fff α .055 D / .30 L (WeekCalendarPanel.qml:32)
            kit::mul_alpha(
                crate::foundations::rgb("#ffffff"),
                if tok.dark() { 0.055 } else { 0.30 },
            )
        } else {
            Color::rgba(0., 0., 0., 0.) // QML "transparent"
        }
    }
}

impl Control for PillButton {
    type Action = ();

    fn implicit_size(&self, _: &Tok) -> (f32, f32) {
        let f = self.font();
        let lw = kit::measure(&self.label, f.px, f.weight).width;
        match self.style {
            PillStyle::Island {
                padding_left,
                padding_right,
            } => (lw + padding_left.max(0.) + padding_right.max(0.), 38.),
            PillStyle::Header { .. } => ((lw + 22.).max(36.), 34.),
        }
    }

    fn frame(&self) -> Rect {
        self.frame
    }

    fn update(&mut self, tok: &Tok, now: Duration) {
        if !self.mouse_enabled() {
            self.state = Interaction::default();
        }
        self.tok = Some(tok.clone());
        self.retarget_interaction(now);
        let fill = self.header_fill(tok);
        self.fill.set(fill, now);
    }

    fn build(&self, tok: &Tok, now: Duration) -> Node {
        let f = self.frame;
        let mut root = kit::group(f);
        let font = self.font();
        match self.style {
            PillStyle::Island {
                padding_left,
                padding_right,
            } => {
                let hover = self.hover.get_or(now, 0.) as f32;
                if hover > 0. {
                    let bg = kit::qt_rect(Rect::new(0., 0., f.width, f.height), 19., tok.island_hover_fill, None);
                    root.add(kit::fade(bg, hover));
                }
                let (pl, pr) = (padding_left.max(0.), padding_right.max(0.));
                root.add(kit::hcentered_text(
                    &self.label,
                    pl,
                    f.width - pl - pr,
                    f.height,
                    &font,
                    self.label_color.unwrap_or(tok.pal.primary_text),
                ));
            }
            PillStyle::Header { emphasized } => {
                // lineColor: primaryText α .11 D / .09 L
                let line = crate::foundations::with_alpha(tok.pal.primary_text, if tok.dark() { 0.11 } else { 0.09 });
                let fill = self.fill.get_or(now, self.header_fill(tok));
                root.add(kit::qt_rect(
                    Rect::new(0., 0., f.width, f.height),
                    17.,
                    fill,
                    if emphasized { None } else { Some((1., line)) },
                ));
                let color = if emphasized {
                    crate::foundations::rgb("#ffffff")
                } else {
                    tok.pal.primary_text
                };
                root.add(kit::centered_text(&self.label, f.width, f.height, &font, color));
            }
        }
        let s = self.scale.get_or(now, 1.) as f32;
        if (s - 1.).abs() > 1e-6 {
            kit::scale_about_center(&mut root, s);
        }
        root
    }

    fn pointer(&mut self, ev: PointerEvent, now: Duration) -> Response<()> {
        if !self.mouse_enabled() {
            self.state = Interaction::default();
            return Response::default();
        }
        let feed = self
            .state
            .feed(ev, (self.frame.width, self.frame.height), &[Button::Left]);
        match self.tok.clone() {
            Some(tok) => self.update(&tok, now),
            // No theme yet: only the theme-independent Behaviors may retarget.
            None => self.retarget_interaction(now),
        }
        let mut r = Response::redraw(feed.changed).with_cursor(Cursor::Pointer);
        if feed.clicked == Some(Button::Left) {
            r = r.act(());
        }
        r
    }

    fn animating(&self, now: Duration) -> bool {
        self.scale.running(now) || self.hover.running(now) || self.fill.running(now)
    }
}
