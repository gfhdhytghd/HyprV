//! W9 SliderTile — `ControlPanelSlider.qml` (ui-panels-control.md §2.3; QuickAdjust uses it
//! at 278×62 with `icon_left_inset = 24`, ui-panels-stats.md §1.3).
use super::kit::{self, Font, Tok};
use super::{Button, Control, Cursor, Interaction, PointerEvent, Response};
use crate::foundations::with_alpha;
use hyprv_ui::{geometry::Rect, scene::Node};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SliderAction {
    /// `valueChangeRequested(newValue)` (0..=100, integer-valued).
    ValueChangeRequested(f32),
    /// `adjustmentFinished(finalValue)`.
    AdjustmentFinished(f32),
    /// `iconClicked()` (only when `icon_clickable`).
    IconClicked,
}

#[derive(Clone, Debug)]
pub struct SliderTile {
    pub frame: Rect,
    pub icon: String,
    /// Bound value 0..100 (displayed unless dragging).
    pub value: f32,
    pub icon_clickable: bool,
    /// `iconLeftInset` (< 0 = centre rule).
    pub icon_left_inset: f32,
    /// QML `enabled` (fan sliders: false when fans unavailable).
    pub enabled: bool,
    /// Item opacity set by the caller (fan sliders: `enabled ? 1 : 0.48`).
    pub opacity: f32,
    pub adjusting: bool,
    pub preview_value: f32,
    pub state: Interaction,
    canceled_finish: Option<f32>,
}

impl SliderTile {
    pub fn new(icon: impl Into<String>, value: f32) -> Self {
        Self {
            frame: Rect::new(0., 0., 134., 62.),
            icon: icon.into(),
            value,
            icon_clickable: false,
            icon_left_inset: -1.,
            enabled: true,
            opacity: 1.,
            adjusting: false,
            preview_value: 0.,
            state: Interaction::default(),
            canceled_finish: None,
        }
    }
    /// Drain the completion caused by a disabled binding during `update`.
    /// A host may dispatch it immediately; otherwise the next pointer event
    /// reports it once before accepting another gesture.
    pub fn take_canceled_adjustment(&mut self) -> Option<SliderAction> {
        self.canceled_finish.take().map(SliderAction::AdjustmentFinished)
    }
    fn display(&self) -> f32 {
        (if self.adjusting { self.preview_value } else { self.value }).clamp(0., 100.)
    }
    fn icon_px(&self) -> f32 {
        (self.frame.height * 0.34).clamp(18., 22.)
    }
    fn icon_center_x(&self) -> f32 {
        let (w, h) = (self.frame.width, self.frame.height);
        (w - h / 2.).min((h / 2.).max(w * 0.24))
    }
    fn compute(&self, x: f32) -> f32 {
        ((x / self.frame.width).clamp(0., 1.) * 100.).round()
    }
    fn inside_icon(&self, x: f32, y: f32) -> bool {
        let hit = (self.frame.height * 0.3).max(18.);
        let cx = if self.icon_left_inset >= 0. {
            self.icon_left_inset + self.icon_px() / 2.
        } else {
            self.icon_center_x()
        };
        (x - cx).abs() <= hit && (y - self.frame.height / 2.).abs() <= hit
    }
}

impl Control for SliderTile {
    type Action = SliderAction;

    fn implicit_size(&self, _: &Tok) -> (f32, f32) {
        (self.frame.width, 62.)
    }

    fn frame(&self) -> Rect {
        self.frame
    }

    fn build(&self, tok: &Tok, _: Duration) -> Node {
        let f = self.frame;
        let p = &tok.pal;
        let fill_w = f.width * self.display() / 100.;
        let progress = with_alpha(p.accent, if tok.dark() { 0.28 } else { 0.22 });
        let mut root = kit::group(f);
        root.add(kit::qt_rect(
            Rect::new(0., 0., f.width, f.height),
            19.,
            p.card_fill,
            None,
        ));
        // Progress: full-width rounded rect clipped to [0, fillWidth].
        let mut clip = crate::primitives::clip_box(Rect::new(0., 0., fill_w, f.height), vec![]);
        clip.add(kit::qt_rect(Rect::new(0., 0., f.width, f.height), 19., progress, None));
        root.add(clip);
        // Two-tone icon: white over the fill, translucent accent over the rest.
        let font = Font {
            px: self.icon_px(),
            weight: kit::BOLD,
        };
        let m = kit::measure(&self.icon, font.px, font.weight);
        let ix = if self.icon_left_inset >= 0. {
            self.icon_left_inset
        } else {
            self.icon_center_x() - m.width / 2.
        };
        let iy = kit::vcenter(f.height, m.height);
        let white = crate::foundations::rgb("#ffffff");
        let mut on = crate::primitives::clip_box(Rect::new(0., 0., fill_w, f.height), vec![]);
        on.add(kit::text(&self.icon, ix, iy, None, &font, white));
        root.add(on);
        let mut off = crate::primitives::clip_box(Rect::new(fill_w, 0., (f.width - fill_w).max(0.), f.height), vec![]);
        off.add(kit::text(&self.icon, ix - fill_w, iy, None, &font, progress));
        root.add(off);
        root.add(kit::qt_rect(
            Rect::new(0., 0., f.width, f.height),
            19.,
            hyprv_ui::animation::Color::rgba(0., 0., 0., 0.),
            Some((1., p.card_stroke)),
        ));
        kit::fade(root, self.opacity)
    }

    fn update(&mut self, _: &Tok, _: Duration) {
        if !self.enabled {
            self.state = Interaction::default();
            if self.adjusting {
                self.adjusting = false;
                self.canceled_finish = Some(self.preview_value);
            }
        }
    }
    fn pointer(&mut self, ev: PointerEvent, _: Duration) -> Response<SliderAction> {
        if let Some(finish) = self.take_canceled_adjustment() {
            return Response::default().act(finish);
        }
        if !self.enabled {
            let canceled = self
                .state
                .feed(PointerEvent::Cancel, (self.frame.width, self.frame.height), &[]);
            let mut reply = Response::redraw(canceled.changed);
            if self.adjusting {
                self.adjusting = false;
                reply = reply.act(SliderAction::AdjustmentFinished(self.preview_value));
            }
            return reply;
        }
        let feed = self
            .state
            .feed(ev, (self.frame.width, self.frame.height), &[Button::Left]);
        let r = Response::default().with_cursor(Cursor::Pointer);
        if let Some((x, y, _)) = feed.pressed_at {
            if self.icon_clickable && self.inside_icon(x, y) {
                return r.act(SliderAction::IconClicked);
            }
            self.adjusting = true;
            self.preview_value = self.compute(x);
            return r.act(SliderAction::ValueChangeRequested(self.preview_value));
        }
        if let Some((x, _)) = feed.moved_to {
            if self.adjusting {
                self.preview_value = self.compute(x);
                return r.act(SliderAction::ValueChangeRequested(self.preview_value));
            }
        }
        if let Some((x, _, _)) = feed.released_at {
            if self.adjusting {
                self.preview_value = self.compute(x);
                self.adjusting = false;
                // QML emits valueChangeRequested then adjustmentFinished; the host gets the
                // final value through the latter (both carry the same value).
                return r.act(SliderAction::AdjustmentFinished(self.preview_value));
            }
        }
        if feed.canceled && self.adjusting {
            self.adjusting = false;
            return r.act(SliderAction::AdjustmentFinished(self.preview_value));
        }
        r
    }
}
