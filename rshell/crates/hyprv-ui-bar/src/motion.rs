use crate::model::*;
use hyprv_ui::animation::{Behavior, CubicBezier, Direction, Easing, Family};
use std::time::Duration;
fn out_cubic() -> Easing {
    Easing::new(Family::Cubic, Direction::Out)
}
/// Per-output clock. Time is supplied by presentation callbacks; no timer or I/O.
pub struct Motion {
    /// Laptop BarScreenState.reveal (laptop-bar.md §1); pinned to 1 on desktop.
    reveal: Behavior<f64>,
    expanded: bool,
    hide: Behavior<f64>,
    fullscreen: bool,
    progress: Behavior<f64>,
    width: Behavior<f64>,
    height: Behavior<f64>,
    radius: Behavior<f64>,
    calendar_open: bool,
    presented: bool,
    close_at: Option<Duration>,
    island_release: Option<Duration>,
    island_expanded: bool,
    leave_at: Option<Duration>,
    arm_at: Option<Duration>,
    armed: bool,
    pointer_inside: bool,
    child_active: bool,
}
impl Motion {
    pub fn new(m: &Model) -> Self {
        let morph = Duration::from_millis(210);
        Self {
            // The initial value is assigned without animation: startup draws compact.
            reveal: Behavior::new(
                if m.bar_toggle_is_noop() || m.expanded { 1. } else { 0. },
                Duration::from_millis(m.desktop.move_duration_ms as u64),
                curve(m.desktop.move_bezier),
            ),
            expanded: m.expanded,
            hide: Behavior::new(
                if m.fullscreen { 1. } else { m.fullscreen_hide },
                Duration::from_millis(m.desktop.move_duration_ms as u64),
                curve(m.desktop.move_bezier),
            ),
            fullscreen: m.fullscreen,
            progress: Behavior::new(m.calendar_progress, morph, out_cubic()),
            width: Behavior::new(m.calendar_width as f64, morph, out_cubic()),
            height: Behavior::new(m.calendar_height as f64, morph, out_cubic()),
            radius: Behavior::new(m.calendar_radius as f64, morph, out_cubic()),
            calendar_open: m.calendar_open,
            presented: m.calendar_presented,
            close_at: None,
            island_release: None,
            island_expanded: m.island.expanded,
            leave_at: None,
            arm_at: None,
            armed: m.calendar_pointer_armed,
            pointer_inside: false,
            child_active: m.calendar_child_active || m.calendar_child_opening,
        }
    }
    pub fn sync(&mut self, m: &Model, now: Duration) {
        if !m.bar_toggle_is_noop() && m.expanded != self.expanded {
            let current = self.reveal.value(now);
            self.reveal = Behavior::new(
                current,
                Duration::from_millis(m.desktop.move_duration_ms as u64),
                curve(m.desktop.move_bezier),
            );
            self.reveal.retarget(if m.expanded { 1. } else { 0. }, now);
            self.expanded = m.expanded;
        }
        if m.fullscreen != self.fullscreen {
            let current = self.hide.value(now);
            self.hide = Behavior::new(
                current,
                Duration::from_millis(m.desktop.move_duration_ms as u64),
                curve(m.desktop.move_bezier),
            );
            self.hide.retarget(if m.fullscreen { 1. } else { 0. }, now);
            self.fullscreen = m.fullscreen;
        }
        if m.calendar_open != self.calendar_open {
            self.calendar_open = m.calendar_open;
            self.leave_at = None;
            self.armed = false;
            self.arm_at = if m.calendar_open {
                Some(now + Duration::from_millis(260))
            } else {
                None
            };
            self.progress
                .retarget(if m.calendar_open { 1. } else { 0. }, now);
            if m.calendar_open {
                if !self.presented {
                    self.width = Behavior::new(
                        m.island.width as f64,
                        Duration::from_millis(210),
                        out_cubic(),
                    );
                    self.height = Behavior::new(
                        m.island.height as f64,
                        Duration::from_millis(210),
                        out_cubic(),
                    );
                    self.radius = Behavior::new(24., Duration::from_millis(210), out_cubic());
                }
                self.presented = true;
                self.close_at = None;
                self.width
                    .retarget((m.output.width - 48.).clamp(0., 520.) as f64, now);
                self.height.retarget(
                    (m.output.height.min(820.) - 78.).clamp(0., 500.) as f64,
                    now,
                );
                self.radius.retarget(30., now);
            } else {
                self.radius.retarget(24., now);
                self.close_at = Some(now + Duration::from_millis(210));
            }
        }
        if m.calendar_open {
            self.width
                .retarget((m.output.width - 48.).clamp(0., 520.) as f64, now);
            self.height.retarget(
                (m.output.height.min(820.) - 78.).clamp(0., 500.) as f64,
                now,
            );
        }
        let child = m.calendar_child_active || m.calendar_child_opening;
        if self.child_active && !child && m.calendar_open && !self.pointer_inside {
            self.leave_at = Some(now + Duration::from_millis(220));
        }
        if child {
            self.leave_at = None;
        }
        self.child_active = child;
        if m.island.expanded {
            self.island_expanded = true;
            self.island_release = None;
        } else if self.island_expanded && self.island_release.is_none() {
            self.island_release = Some(now + Duration::from_millis(230));
        }
    }
    pub fn pointer_calendar(&mut self, inside: bool, m: &Model, now: Duration) {
        let left = self.pointer_inside && !inside;
        self.pointer_inside = inside;
        if inside || m.calendar_child_active || m.calendar_child_opening {
            self.leave_at = None;
        } else if m.calendar_open && (self.armed || left) {
            self.leave_at = Some(now + Duration::from_millis(220));
        }
    }
    pub fn advance(&mut self, m: &mut Model, now: Duration) -> Vec<Action> {
        m.now = now;
        if self.arm_at.is_some_and(|deadline| now >= deadline) {
            self.arm_at = None;
            self.armed = m.calendar_open;
        }
        m.calendar_pointer_armed = self.armed;
        m.fullscreen_hide = self.hide.advance(now);
        m.reveal = if m.bar_toggle_is_noop() {
            1.
        } else {
            self.reveal.advance(now)
        };
        m.calendar_progress = self.progress.advance(now);
        let mut actions = Vec::new();
        if self.close_at.is_some_and(|deadline| now >= deadline) {
            self.presented = false;
            self.close_at = None;
        }
        if self.island_release.is_some_and(|deadline| now >= deadline) {
            self.island_expanded = false;
            self.island_release = None;
        }
        m.island.expanded = self.island_expanded;
        m.calendar_presented = self.presented;
        if self.presented {
            m.calendar_width = self.width.advance(now) as f32;
            m.calendar_height = self.height.advance(now) as f32;
            m.calendar_radius = self.radius.advance(now) as f32;
        } else {
            m.calendar_width = m.island.width;
            m.calendar_height = m.island.height;
            m.calendar_radius = 24.;
        }
        if self.leave_at.is_some_and(|deadline| now >= deadline) {
            self.leave_at = None;
            if m.calendar_open && !m.calendar_child_active && !m.calendar_child_opening {
                actions.push(Action::Host(HostAction::CloseCalendar { force: false }));
            }
        }
        actions
    }
    pub fn next_deadline(&self) -> Option<Duration> {
        [
            self.close_at,
            self.island_release,
            self.leave_at,
            self.arm_at,
        ]
        .into_iter()
        .flatten()
        .min()
    }
    /// Current reveal without advancing (hosts carry it across model rebuilds).
    pub fn reveal(&self, now: Duration) -> f64 {
        self.reveal.value(now)
    }
    pub fn animating(&self) -> bool {
        self.reveal.is_running()
            || self.hide.is_running()
            || self.progress.is_running()
            || (self.presented
                && (self.width.is_running()
                    || self.height.is_running()
                    || self.radius.is_running()))
    }
}
fn curve(v: [f64; 6]) -> Easing {
    Easing::BezierSpline(vec![CubicBezier {
        control1: (v[0], v[1]),
        control2: (v[2], v[3]),
        end: (v[4], v[5]),
    }])
}

/// Flip this output's adaptive-bar target (laptop `toggleBar`, after the debounce).
/// Desktop keeps the QML no-op. Motion animates `reveal` on the next sync.
pub fn toggle_bar(m: &mut Model) {
    if !m.bar_toggle_is_noop() {
        m.expanded = !m.expanded;
    }
}
