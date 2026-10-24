//! W8 SplitTile — `ControlPanelSplitTile.qml` (ui-panels-control.md §2.1).
use super::kit::{self, Font, Tok};
use super::{Button, Control, Cursor, Interaction, PointerEvent, Response};
use crate::foundations::with_alpha;
use hyprv_ui::{animation::Color, geometry::Rect, scene::Node};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitZone {
    /// Icon well area (`leftClicked`).
    Left,
    /// Text area (`rightClicked`).
    Right,
}

#[derive(Clone, Debug)]
pub struct SplitTile {
    pub frame: Rect,
    pub icon: String,
    pub title: String,
    pub subtitle: String,
    pub active: bool,
    pub destructive: bool,
    pub left_enabled: bool,
    pub right_enabled: bool,
    pub expand_indicator_visible: bool,
    pub expanded: bool,
    /// `None` = height (QML default `iconAreaWidth: height`).
    pub icon_area_width: Option<f32>,
    pub left: Interaction,
    pub right: Interaction,
}

impl SplitTile {
    pub fn new(icon: impl Into<String>, title: impl Into<String>, subtitle: impl Into<String>) -> Self {
        Self {
            frame: Rect::new(0., 0., 134., 62.),
            icon: icon.into(),
            title: title.into(),
            subtitle: subtitle.into(),
            active: false,
            destructive: false,
            left_enabled: true,
            right_enabled: true,
            expand_indicator_visible: false,
            expanded: false,
            icon_area_width: None,
            left: Interaction::default(),
            right: Interaction::default(),
        }
    }
    /// (inset, wellW, wellH, iconPx, contentStartX) per ControlPanelSplitTile.qml:36-42.
    fn geometry(&self) -> (f32, f32, f32, f32, f32) {
        let h = self.frame.height;
        let area = self.icon_area_width.unwrap_or(h);
        let inset = (h * 0.14).round().max(6.);
        let ww = (area - inset * 2.).max(28.);
        let wh = (h - inset * 2.).max(28.);
        let px = (ww.min(wh) * 0.48).clamp(16., 20.);
        (inset, ww, wh, px, inset + ww + 9.)
    }
}

impl Control for SplitTile {
    type Action = SplitZone;

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
        let (inset, ww, wh, px, start) = self.geometry();
        let mut root = kit::group(f);
        let (fill, stroke) = if self.active {
            (
                with_alpha(accent, if tok.dark() { 0.28 } else { 0.22 }),
                with_alpha(accent, if tok.dark() { 0.42 } else { 0.28 }),
            )
        } else {
            (p.card_fill, p.card_stroke)
        };
        root.add(kit::qt_rect(
            Rect::new(0., 0., f.width, f.height),
            19.,
            fill,
            Some((1., stroke)),
        ));
        let well = tok.icon_well_fill(self.active);
        let well = if self.left.pressed.is_some() {
            kit::qt_darker(well, 1.08)
        } else if self.left.hovered {
            kit::qt_lighter(well, 1.04)
        } else {
            well
        };
        root.add(kit::qt_rect(Rect::new(inset, inset, ww, wh), 11., well, None));
        let icon_color = if self.active {
            crate::foundations::rgb("#fff7ef")
        } else {
            p.primary_text
        };
        let mut icon = kit::centered_text(&self.icon, ww, wh, &Font { px, weight: kit::BOLD }, icon_color);
        icon.frame.x += inset;
        icon.frame.y += inset;
        root.add(icon);

        // Text block: x = contentStartX … width − 10, vertically centred.
        let block_w = f.width - start - 10.;
        let title_font = Font {
            px: 11.,
            weight: kit::BOLD,
        };
        let sub_font = Font {
            px: 9.,
            weight: kit::MEDIUM,
        };
        let th = kit::measure(&self.title, 11., kit::BOLD).height;
        let sub_vis = !self.subtitle.is_empty();
        let sh = if sub_vis {
            kit::measure(&self.subtitle, 9., kit::MEDIUM).height
        } else {
            0.
        };
        let col_h = th + if sub_vis { 2. + sh } else { 0. };
        let block_y = kit::vcenter(f.height, col_h);
        let chevron = if self.expanded { "\u{f077}" } else { "\u{f078}" };
        let chev_m = kit::measure(chevron, 11., kit::BOLD);
        let col_w = if self.expand_indicator_visible {
            block_w - chev_m.width - 6.
        } else {
            block_w
        };
        let white = crate::foundations::rgb("#ffffff");
        let title_color = if self.active { white } else { p.primary_text };
        let sub_color: Color = if self.active {
            with_alpha(white, 0.82)
        } else {
            p.muted_text
        };
        root.add(kit::text(
            &self.title,
            start,
            block_y,
            Some(col_w),
            &title_font,
            title_color,
        ));
        if sub_vis {
            root.add(kit::text(
                &self.subtitle,
                start,
                block_y + th + 2.,
                Some(col_w),
                &sub_font,
                sub_color,
            ));
        }
        if self.expand_indicator_visible {
            let y = block_y + kit::vcenter(col_h, chev_m.height);
            root.add(kit::text(
                chevron,
                start + block_w - chev_m.width,
                y,
                None,
                &title_font,
                if self.active { white } else { p.muted_text },
            ));
        }
        root
    }

    fn update(&mut self, _: &Tok, _: Duration) {
        // Binding-driven disable mid-gesture: drop the grab even if no pointer event follows.
        if !self.left_enabled {
            self.left = Interaction::default();
        }
        if !self.right_enabled {
            self.right = Interaction::default();
        }
    }

    fn pointer(&mut self, ev: PointerEvent, _: Duration) -> Response<SplitZone> {
        let (.., start) = self.geometry();
        let h = self.frame.height;
        let shifted = |ev: PointerEvent, dx: f32| match ev {
            PointerEvent::Enter { x, y } => PointerEvent::Enter { x: x - dx, y },
            PointerEvent::Move { x, y } => PointerEvent::Move { x: x - dx, y },
            PointerEvent::Press { x, y, button } => PointerEvent::Press { x: x - dx, y, button },
            PointerEvent::Release { x, y, button } => PointerEvent::Release { x: x - dx, y, button },
            other => other,
        };
        let mut r = Response::default();
        if !self.left_enabled {
            self.left = Interaction::default();
        }
        if !self.right_enabled {
            self.right = Interaction::default();
        }
        if self.left_enabled {
            let f = self.left.feed(ev, (start, h), &[Button::Left]);
            r.redraw |= f.changed;
            if f.clicked.is_some() {
                r.action = Some(SplitZone::Left);
            }
        }
        if self.right_enabled {
            let f = self
                .right
                .feed(shifted(ev, start), (self.frame.width - start, h), &[Button::Left]);
            if f.clicked.is_some() {
                r.action = Some(SplitZone::Right);
            }
        }
        let over_left = self.left.hovered && self.left_enabled;
        let over_right = self.right.hovered && self.right_enabled;
        r.cursor = Some(if over_left || over_right {
            Cursor::Pointer
        } else {
            Cursor::Default
        });
        if r.action.is_some() {
            r.redraw = true;
        }
        r
    }
}
