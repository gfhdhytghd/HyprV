//! AnimatedGlassPanel popups (ui-panels.md §2.1, §3): map/open/close/unmap state machine
//! with the 16 ms open timer, glass chrome with stable node ids (per-frame updates only
//! touch the reveal clip, content opacity and card position), and placement rules.
use crate::{model::Anchor, ui::*};
use hyprv_ui::{
    animation::Color,
    geometry::Rect,
    scene::{Node, NodeId, Scene},
};
use hyprv_widgets::containers::{Reveal, RevealTiming, Transition};
use std::time::Duration;

/// AnimatedGlassPanel's content has `anchors.fill` *and* `y: contentOffset`; anchors win,
/// so the slide is a no-op (ui-panels.md §2.1). Flip to apply the offset.
pub const QUIRK_GLASS_CONTENT_SLIDE_IS_NOOP: bool = true;

pub const OPEN_TIMER_MS: u64 = 16;

#[derive(Clone, Debug, PartialEq)]
pub enum PopupEvent {
    OpenFinished,
    CloseFinished,
    /// The window was unmapped (close animation finished while closing).
    Unmapped,
}

/// Shared popup state (`popupRequested`, `animatingClose`, `openAnimationPending`, the
/// window's `visible`, the `popupOpenTimer`).
#[derive(Clone, Debug)]
pub struct Machine {
    pub mapped: bool,
    pub requested: bool,
    pub closing: bool,
    pub pending: bool,
    open_timer: Option<Duration>,
    /// Restart the open timer at the first tick after this time (see
    /// [`QUIRK_PAGE_SWITCH_TIMER_RESTARTS_NEXT_FRAME`]).
    restart_after: Option<Duration>,
    pub reveal: Reveal,
}

/// After a page switch the new page's Loader item settles its implicit height one frame
/// later; `onFullPanelHeightChanged` (openAnimationPending branch) then restarts
/// popupOpenTimer, so the reopen starts one frame later than a plain open. Measured
/// against parity/reference/panels cp-page-switch (exactly one frame).
pub const QUIRK_PAGE_SWITCH_TIMER_RESTARTS_NEXT_FRAME: bool = true;

impl Machine {
    pub fn new(timing: RevealTiming) -> Self {
        let mut reveal = Reveal::new(0.);
        reveal.timing = timing;
        Self { mapped: false, requested: false, closing: false, pending: false, open_timer: None, restart_after: None, reveal }
    }
    /// The 16 ms `popupOpenTimer` (content height is known synchronously here).
    fn start_open_timer(&mut self, now: Duration) {
        self.open_timer = Some(now + ms(OPEN_TIMER_MS));
    }
    /// `onVisibleChanged(true)`: prepare and arm the open timer (unless closing).
    pub fn map(&mut self, now: Duration) {
        self.mapped = true;
        if !self.closing {
            self.pending = true;
            self.reveal.prepare_open();
            self.start_open_timer(now);
        }
    }
    /// openFor when already mapped (e.g. mid-close): restart the open sequence.
    pub fn reopen(&mut self, now: Duration) {
        self.closing = false;
        self.requested = true;
        self.reveal.prepare_open();
        self.pending = true;
        self.start_open_timer(now);
    }
    /// `animateCurrentPageOpen()` after a page switch.
    pub fn reopen_after_page_switch(&mut self, now: Duration) {
        self.reopen(now);
        if QUIRK_PAGE_SWITCH_TIMER_RESTARTS_NEXT_FRAME {
            self.restart_after = Some(now);
        }
    }
    /// Generic `openFor`: map or restart. Returns true if the window must be mapped.
    pub fn open(&mut self, now: Duration) -> bool {
        self.requested = true;
        if self.mapped {
            self.reopen(now);
            false
        } else {
            self.closing = false;
            self.map(now);
            true
        }
    }
    /// `closePopup()`.
    pub fn close(&mut self, now: Duration) {
        if (!self.requested && !self.closing) || !self.mapped {
            self.requested = false;
            self.closing = false;
            return;
        }
        if self.closing {
            return;
        }
        self.requested = false;
        self.closing = true;
        self.open_timer = None;
        self.reveal.play_close(now);
    }
    /// Immediate unmap (Wi-Fi tile, Windows VM toggle).
    pub fn unmap_now(&mut self) {
        self.requested = false;
        self.closing = false;
        self.unmapped();
    }
    fn unmapped(&mut self) {
        self.mapped = false;
        self.pending = false;
        self.open_timer = None;
        self.restart_after = None;
        self.reveal.reset();
    }
    /// Advance timers and animations; returns what happened.
    pub fn tick(&mut self, now: Duration) -> Vec<PopupEvent> {
        let mut ev = Vec::new();
        if let Some(t) = self.restart_after {
            if now > t {
                self.restart_after = None;
                if self.open_timer.is_some() {
                    self.start_open_timer(now);
                }
            }
        }
        if let Some(t) = self.open_timer {
            if now >= t {
                self.open_timer = None;
                if self.mapped && self.requested && !self.closing {
                    if self.reveal.full_height <= 0. {
                        // content not measured yet: retry in 16 ms (popupOpenTimer)
                        self.open_timer = Some(t + ms(OPEN_TIMER_MS));
                    } else {
                        self.pending = false;
                        // Qt starts the animation at the animation driver time of the frame
                        // in which the timer fired, not at the timer deadline.
                        self.reveal.play_open(now);
                    }
                } else {
                    self.pending = false;
                }
            }
        }
        match self.reveal.advance(now) {
            Some(Transition::Open) => ev.push(PopupEvent::OpenFinished),
            Some(Transition::Close) => {
                ev.push(PopupEvent::CloseFinished);
            }
            None => {}
        }
        ev
    }
    /// Close-finished default handling: unmap if closing and not requested.
    pub fn finish_close(&mut self) -> bool {
        if self.closing && !self.requested {
            self.closing = false;
            self.unmapped();
            true
        } else {
            false
        }
    }
    pub fn animating(&self) -> bool {
        self.reveal.is_running() || self.open_timer.is_some()
    }
    pub fn next_deadline(&self) -> Option<Duration> {
        self.open_timer
    }
    /// `onFullPanelHeightChanged`: snap when idle (instant resize, no animation).
    pub fn set_full_height(&mut self, h: f32) {
        if (self.reveal.full_height - h).abs() < 1e-4 {
            return;
        }
        if self.mapped && !self.closing && !self.reveal.is_running() && !self.pending {
            self.reveal.full_height = h;
            self.reveal.reset();
        } else {
            self.reveal.full_height = h;
            if !self.reveal.is_running() && !self.pending {
                self.reveal.reset();
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct GlassStyle {
    pub radius: f32,
    pub fill: Color,
    pub stroke: Color,
    pub surface_opacity: f32,
    pub shadow: Color,
}

/// Stable node ids of a glass card.
#[derive(Clone, Copy, Debug)]
pub struct GlassIds {
    pub card: NodeId,
    pub viewport: NodeId,
    pub chrome: NodeId,
    pub content: NodeId,
}

/// Build the card: card(frame) → viewport clip (h = reveal) → panelFrame clip (full) →
/// [chrome (fill/border × surfaceOpacity, optional MultiEffect shadow), content (opacity)].
#[allow(clippy::too_many_arguments)]
pub fn glass(frame: Rect, full_h: f32, reveal_h: f32, opacity: f32, offset: f32, style: &GlassStyle, content: Vec<Node>, dpr: f32) -> (Node, GlassIds) {
    let full = Rect::new(0., 0., frame.width, full_h);
    // shadowLayer (layer.effect MultiEffect when shadowColor.a > 0) → Rectangle with
    // opacity surfaceOpacity. The effect wraps the translucent rectangle, so the shadow
    // keeps full strength under the fill; panelFrame's clip hides it outside the card.
    let mut chrome = if style.shadow.a > 0. && style.surface_opacity > 0. {
        // shadowLayer: layer.textureSize = round(size × dpr) while MultiEffect auto
        // padding enlarges the captured region (engine `source_texture_size`)
        let mut fill = rect(full, style.radius, style.fill, Some((1., style.stroke)));
        fill.opacity = style.surface_opacity;
        let mut layer = group_with(full, vec![fill]);
        layer.effects.multi_effect = Some(hyprv_ui::effects::MultiEffect {
            shadow_enabled: true,
            shadow_blur: 10. / 64.,
            shadow_color: style.shadow,
            source_texture_size: Some(((frame.width * dpr).round().max(1.) as u32, (full_h * dpr).round().max(1.) as u32)),
            ..Default::default()
        });
        layer
    } else {
        let mut fill = rect(full, style.radius, style.fill, Some((1., style.stroke)));
        fill.opacity = style.surface_opacity;
        group_with(full, vec![fill])
    };
    if style.surface_opacity <= 0. {
        chrome.visible = false;
    }
    let y = if QUIRK_GLASS_CONTENT_SLIDE_IS_NOOP { 0. } else { offset };
    let mut c = group_with(Rect::new(0., y, frame.width, full_h), content);
    c.opacity = opacity;
    let ids_chrome = chrome.id();
    let ids_content = c.id();
    let panel = clip(full, vec![chrome, c]);
    let viewport = clip(Rect::new(0., 0., frame.width, reveal_h.max(0.)), vec![panel]);
    let vid = viewport.id();
    let card = group_with(Rect::new(frame.x, frame.y, frame.width, full_h), vec![viewport]);
    let ids = GlassIds { card: card.id(), viewport: vid, chrome: ids_chrome, content: ids_content };
    (card, ids)
}

/// Per-frame update of reveal values and position only.
pub fn glass_update(scene: &mut Scene, ids: &GlassIds, pos: (f32, f32), reveal_h: f32, opacity: f32) {
    scene.set_position(ids.card, pos.0, pos.1);
    let need = scene.root().find(ids.viewport).is_some_and(|n| (n.frame.height - reveal_h.max(0.)).abs() > 1e-4);
    if need {
        scene.edit(ids.viewport, |n| n.frame.height = reveal_h.max(0.));
    }
    let need = scene.root().find(ids.content).is_some_and(|n| (n.opacity - opacity).abs() > 1e-5);
    if need {
        scene.edit(ids.content, |n| n.opacity = opacity);
    }
}

/// Vertical placement below the anchor with above-fallback and 8 px screen margin
/// (`updatePopupPosition` in ControlPanel/Wi-Fi/Stats/tray popups).
pub fn below_or_above(anchor_bottom: f32, card_h: f32, out_h: f32) -> f32 {
    let below = (anchor_bottom + 10.).round();
    let above = (anchor_bottom - card_h - 10.).round();
    if below + card_h <= out_h - 8. || above < 8. {
        below.min(out_h - card_h - 8.).max(8.)
    } else {
        above.max(8.)
    }
}

pub fn anchor_rect(a: &Anchor) -> Rect {
    Rect::new(a.x, a.y, a.width, a.height)
}
