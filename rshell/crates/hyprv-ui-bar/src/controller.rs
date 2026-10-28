//! Host-facing per-output UI state. All deadlines are supplied to the host's one-shot
//! event-loop timer; only active animations request presentation frames.
use crate::{
    input::{Input, InputResult},
    model::*,
    motion::Motion,
    view,
};
use hyprv_ui::scene::Scene;
use hyprv_widgets::controls::PointerEvent;
use std::time::Duration;
pub struct Controller {
    pub model: Model,
    pub motion: Motion,
    pub input: Input,
}
impl Controller {
    pub fn new(mut model: Model) -> Self {
        if let Some(w) = model.stats.package_power_w.filter(|w| w.is_finite()) {
            model.last_package_power_w = w;
        }
        let open = model.calendar_open;
        model.calendar_open = false;
        let mut motion = Motion::new(&model);
        model.calendar_open = open;
        motion.sync(&model, model.now);
        let now = model.now;
        motion.advance(&mut model, now);
        Self {
            motion,
            model,
            input: Input::default(),
        }
    }
    pub fn sync(&mut self, mut next: Model, now: Duration) {
        next.last_package_power_w = next
            .stats
            .package_power_w
            .filter(|w| w.is_finite())
            .unwrap_or(self.model.last_package_power_w);
        next.hovered = self.model.hovered.clone();
        next.tooltip_visible = self.model.tooltip_visible;
        self.motion.sync(&next, now);
        if !next.bar_toggle_is_noop() {
            next.reveal = self.motion.reveal(now);
        }
        self.model = next;
    }
    pub fn pointer(&mut self, event: PointerEvent) -> InputResult {
        let result = self.input.pointer(&mut self.model, event);
        if let PointerEvent::Move { x, y } | PointerEvent::Enter { x, y } = event {
            let l = crate::layout::compute(&self.model, &self.model.theme);
            self.motion.pointer_calendar(
                crate::layout::contains(
                    hyprv_ui::geometry::Rect::new(
                        l.center.x,
                        l.center.y + l.content_y,
                        l.center.width,
                        l.center.height,
                    ),
                    x,
                    y,
                ),
                &self.model,
                self.model.now,
            );
        } else if matches!(event, PointerEvent::Leave) {
            self.motion
                .pointer_calendar(false, &self.model, self.model.now);
        }
        result
    }
    /// Laptop compact strip events on this output's corner window (window = output
    /// logical coordinates). Desktop (or a settled full bar) never consumes them.
    pub fn pointer_compact(&mut self, event: PointerEvent) -> InputResult {
        self.input.strip_pointer(&mut self.model, event)
    }
    pub fn frame(&mut self, scene: &mut Scene, now: Duration) -> Vec<Action> {
        let mut actions = self.motion.advance(&mut self.model, now);
        actions.extend(self.input.advance(now));
        self.input.tooltip(&mut self.model, now);
        view::update(scene, &self.model);
        actions
    }
    pub fn next_deadline(&self) -> Option<Duration> {
        [self.motion.next_deadline(), self.input.next_deadline(), self.model.urgent_blink_deadline()]
            .into_iter()
            .flatten()
            .min()
    }
    pub fn animating(&self) -> bool {
        self.motion.animating()
    }
}
