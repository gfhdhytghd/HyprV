// SPDX-License-Identifier: LGPL-3.0-only
//! Vertical Qt Quick 6.12 Flickable physics for the shell's menu and agent ListView.
//!
//! Algorithm port of QQuickFlickable/QQuickTimeLine (The Qt Company, LGPL-3.0-only).
//! Coordinates, velocity and time are logical pixels, pixels/second and monotonic
//! frame time. There is no clock, timer, I/O or frame request inside this controller.
//! The host presents while `animating()`; at rest it schedules nothing. Child click
//! handlers must be canceled when `drag_to` reports that the scroll grab was taken.
use std::{collections::VecDeque, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundsBehavior {
    StopAtBounds,
    DragOverBounds,
    OvershootBounds,
    DragAndOvershootBounds,
}
impl BoundsBehavior {
    fn drag(self) -> bool {
        matches!(self, Self::DragOverBounds | Self::DragAndOvershootBounds)
    }
    fn flick(self) -> bool {
        matches!(self, Self::OvershootBounds | Self::DragAndOvershootBounds)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundsMovement {
    FollowBoundsBehavior,
    StopAtBounds,
}
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub bounds_behavior: BoundsBehavior,
    pub bounds_movement: BoundsMovement,
    pub start_drag_distance: f64,
    pub flick_deceleration: f64,
    pub maximum_flick_velocity: f64,
    pub wheel_deceleration: f64,
    pub wheel_scroll_lines: f64,
    pub device_pixel_ratio: f64,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            bounds_behavior: BoundsBehavior::DragAndOvershootBounds,
            bounds_movement: BoundsMovement::FollowBoundsBehavior,
            start_drag_distance: 10.,
            flick_deceleration: 1500.,
            maximum_flick_velocity: 2500.,
            wheel_deceleration: 15000.,
            wheel_scroll_lines: 3.,
            device_pixel_ratio: 1.,
        }
    }
}
impl Settings {
    pub fn list_view() -> Self {
        Self {
            bounds_behavior: BoundsBehavior::StopAtBounds,
            ..Self::default()
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollPhase {
    #[default]
    None,
    Begin,
    Update,
    Momentum,
    End,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Wheel {
    pub angle_delta: f64,
    pub pixel_delta: f64,
    pub phase: ScrollPhase,
    /// Qt MouseEventSynthesizedBySystem (touchpad), rather than a physical wheel.
    pub synthesized: bool,
}
impl Wheel {
    /// Adapter for unphased shell input. Discrete angles take priority over
    /// absolute axis distances; pixel-only input is a touchpad update. Callers
    /// with native phase/source information should supply a full `Wheel`.
    pub fn deltas(angle_delta: f64, pixel_delta: f64) -> Self {
        Self {
            angle_delta,
            pixel_delta,
            synthesized: angle_delta == 0. && pixel_delta != 0.,
            phase: if angle_delta == 0. && pixel_delta != 0. {
                ScrollPhase::Update
            } else {
                ScrollPhase::None
            },
        }
    }
}
#[derive(Clone, Debug)]
struct Drag {
    pointer: f64,
    content: f64,
    maximum_y: f64,
    last_pointer: f64,
    last_time: u64,
    press_time: u64,
    offset: f64,
    previous_delta: f64,
    steal: bool,
    moved: bool,
    filtered_velocity: f64,
}
#[derive(Clone, Copy, Debug)]
enum Curve {
    InQuad,
    OutExpo,
}
#[derive(Clone, Copy, Debug)]
enum Segment {
    Accel {
        from: f64,
        velocity: f64,
        acceleration: f64,
        duration: u64,
    },
    Ease {
        from: f64,
        to: f64,
        curve: Curve,
        duration: u64,
    },
}
impl Segment {
    fn duration(self) -> u64 {
        match self {
            Self::Accel { duration, .. } | Self::Ease { duration, .. } => duration,
        }
    }
    fn value(self, ms: u64) -> f64 {
        match self {
            Self::Accel {
                from,
                velocity,
                acceleration,
                ..
            } => {
                let t = ms as f64 / 1000.;
                from + velocity * t + 0.5 * acceleration * t * t
            }
            Self::Ease {
                from,
                to,
                curve,
                duration,
            } => {
                let t = (ms as f64 / duration.max(1) as f64).min(1.);
                let p = match curve {
                    Curve::InQuad => t * t,
                    Curve::OutExpo => {
                        if t == 0. {
                            0.
                        } else if t == 1. {
                            1.
                        } else {
                            1.001 * (1. - 2f64.powf(-10. * t))
                        }
                    }
                };
                from + (to - from) * p
            }
        }
    }
}
#[derive(Clone, Debug)]
struct Timeline {
    queue: VecDeque<Segment>,
    elapsed: u64,
    last_frame: Option<u64>,
    flick: bool,
    overshoot: bool,
    fixing: bool,
    // On release Qt fixes the unused horizontal axis with a 1 ms no-op.
    // QQuickTimeLine visits that boundary before the rest of the first frame;
    // preserve the split because overshoot velocity is measured at that visit.
    first_flick_step: bool,
}
#[derive(Clone, Debug)]
pub struct Flickable {
    pub settings: Settings,
    raw_y: f64,
    viewport: f64,
    content: f64,
    drag: Option<Drag>,
    timeline: Option<Timeline>,
    samples: VecDeque<f64>,
    velocity: f64,
    wheel_time: Option<u64>,
    wheel_pixels: f64,
    wheel_phase: bool,
    wheel_drag: bool,
    wheel_rebound: bool,
    wheel_peak: f64,
    now: u64,
    last_flick_velocity: f64,
    boost: f64,
    fixup_immediate: bool,
}
impl Default for Flickable {
    fn default() -> Self {
        Self::new(Settings::default())
    }
}
fn millis(now: Duration) -> u64 {
    now.as_millis().min(u64::MAX as u128) as u64
}
fn finite(v: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        0.
    }
}
impl Flickable {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            raw_y: 0.,
            viewport: 0.,
            content: 0.,
            drag: None,
            timeline: None,
            samples: VecDeque::new(),
            velocity: 0.,
            wheel_time: None,
            wheel_pixels: 0.,
            wheel_phase: false,
            wheel_drag: false,
            wheel_rebound: false,
            wheel_peak: 0.,
            now: 0,
            last_flick_velocity: 0.,
            boost: 1.,
            fixup_immediate: true,
        }
    }
    pub fn content_y(&self) -> f64 {
        if self.settings.bounds_movement == BoundsMovement::StopAtBounds {
            self.raw_y.clamp(0., self.maximum_y())
        } else {
            self.raw_y
        }
    }
    pub fn maximum_y(&self) -> f64 {
        (self.content - self.viewport).max(0.)
    }
    pub fn dragging(&self) -> bool {
        self.drag.as_ref().is_some_and(|d| d.moved) && (!self.wheel_drag || self.wheel_phase)
    }
    pub fn grabbed(&self) -> bool {
        self.drag.as_ref().is_some_and(|d| d.steal)
    }
    pub fn animating(&self) -> bool {
        self.timeline.is_some()
    }
    pub fn moving(&self) -> bool {
        self.animating() || self.dragging()
    }
    /// Geometry changes are applied immediately at rest, as in QQuickFlickable.
    pub fn set_extents(&mut self, viewport: f64, content: f64) {
        let old = self.maximum_y();
        let content_changed = self.content != finite(content).max(0.);
        self.viewport = finite(viewport).max(0.);
        self.content = finite(content).max(0.);
        if old != self.maximum_y() && self.drag.is_none() {
            if self.timeline.is_none() {
                self.raw_y = self.raw_y.clamp(0., self.maximum_y());
            } else if content_changed
                && self.timeline.as_ref().is_some_and(|t| t.fixing)
                && self.raw_y > self.maximum_y()
            {
                // setContentHeight during a fixup uses ExtentChanged: retain the
                // presentation clock and finish with OutExpo, without InQuad.
                self.queue(
                    VecDeque::from([Segment::Ease {
                        from: self.raw_y,
                        to: self.maximum_y(),
                        curve: Curve::OutExpo,
                        duration: 300,
                    }]),
                    false,
                    true,
                    false,
                );
            }
            // A running flick keeps its original acceleration. viewportAxisMoved
            // observes the new edge on the next frame and brakes/rebounds even
            // under StopAtBounds (the extent moved under the running animation).
        }
    }
    /// Qt disabling interactive cancels a held press, but leaves a released flick
    /// running. Owners call this when an overflowing list becomes noninteractive.
    pub fn cancel_interaction(&mut self) {
        if self.drag.is_some() && (!self.wheel_drag || self.wheel_phase) {
            self.cancel();
        }
    }
    pub fn set_content_y(&mut self, y: f64) {
        self.timeline = None;
        let y = finite(y);
        if let Some(d) = &mut self.drag {
            d.content += y - self.raw_y;
        }
        self.raw_y = y;
    }
    /// Owner unmap/hide: cancel the gesture and all presentation work.
    pub fn reset(&mut self, y: f64) {
        self.drag = None;
        self.timeline = None;
        self.wheel_phase = false;
        self.wheel_drag = false;
        self.wheel_rebound = false;
        self.wheel_peak = 0.;
        self.wheel_pixels = 0.;
        self.samples.clear();
        self.velocity = 0.;
        self.raw_y = finite(y).clamp(0., self.maximum_y());
    }
    fn sample(&mut self, v: f64) {
        let max = self.settings.maximum_flick_velocity;
        self.samples
            .push_back(if max < 0. { v } else { v.clamp(-max, max) });
        while self.samples.len() > 3 {
            self.samples.pop_front();
        }
    }
    fn average(&mut self) -> f64 {
        self.velocity = self.samples.iter().sum::<f64>() / self.samples.len().max(1) as f64;
        self.velocity
    }
    pub fn press(&mut self, pointer_y: f64, now: Duration) {
        self.frame(now);
        self.fixup_immediate = false;
        self.wheel_drag = false;
        if (self.content - self.viewport).abs().floor() == 0. {
            self.drag = None;
            return;
        }
        let time = millis(now);
        let retain = self
            .timeline
            .as_ref()
            .is_some_and(|t| t.flick && !t.fixing && !t.overshoot)
            && self.velocity.abs() > 100.;
        let flick_time = self.timeline.as_ref().map_or(0, |t| t.elapsed);
        if !retain || flick_time > 600 {
            self.boost = 1.;
            self.last_flick_velocity = 0.;
        } else {
            self.last_flick_velocity = self.velocity;
            if flick_time > 300 {
                self.boost = (self.boost - 0.5).max(1.);
            }
        }
        self.timeline = None;
        self.samples.clear();
        self.velocity = 0.;
        self.drag = Some(Drag {
            pointer: finite(pointer_y),
            content: self.raw_y,
            maximum_y: self.maximum_y(),
            last_pointer: finite(pointer_y),
            last_time: time,
            press_time: time,
            offset: 0.,
            previous_delta: 0.,
            steal: retain,
            moved: false,
            filtered_velocity: 0.,
        });
    }
    /// Move under a child MouseArea. `velocity` is optional device-local velocity;
    /// without one, use Qt's QEventPoint 0.7-gain filter on timestamped positions.
    /// Returns true once the Flickable steals the child's press.
    pub fn drag_to(&mut self, pointer_y: f64, velocity: Option<f64>, now: Duration) -> bool {
        self.drag_axis(pointer_y, velocity, now, None)
    }

    /// Qt HorizontalAndVerticalFlick shares its threshold and mouse grab across axes.
    /// Call once per pointer move, rather than dragging two independent controllers.
    pub fn drag_both(
        x: &mut Self,
        y: &mut Self,
        pointer: (f64, f64),
        velocity: Option<(f64, f64)>,
        now: Duration,
    ) -> bool {
        let over = [(&*x, pointer.0), (&*y, pointer.1)].iter().any(|(f, p)| {
            f.drag
                .as_ref()
                .is_some_and(|d| (*p - d.pointer).abs() > f.settings.start_drag_distance || d.moved)
        });
        let steal = x.grabbed() || y.grabbed();
        for f in [&mut *x, &mut *y] {
            if let Some(d) = &mut f.drag {
                d.steal = steal;
            }
        }
        let gx = x.drag_axis(pointer.0, velocity.map(|v| v.0), now, Some(over));
        let gy = y.drag_axis(pointer.1, velocity.map(|v| v.1), now, Some(over));
        let grabbed = gx || gy;
        for f in [x, y] {
            if let Some(d) = &mut f.drag {
                d.steal = grabbed;
            }
        }
        grabbed
    }

    fn drag_axis(
        &mut self,
        pointer_y: f64,
        velocity: Option<f64>,
        now: Duration,
        over: Option<bool>,
    ) -> bool {
        let time = millis(now);
        self.now = time;
        if self.wheel_drag {
            return false;
        }
        let Some(mut d) = self.drag.take() else {
            return false;
        };
        let y = finite(pointer_y);
        let delta = y - d.pointer;
        let dt = time.saturating_sub(d.last_time);
        if dt > 0 {
            d.filtered_velocity =
                ((y - d.last_pointer) / dt as f64 * 1000.) * 0.7 + d.filtered_velocity * 0.3;
        }
        let v = finite(velocity.unwrap_or(d.filtered_velocity));
        let over = over.unwrap_or(delta.abs() > self.settings.start_drag_distance || d.moved);
        let mut reject = false;
        if over || time.saturating_sub(d.press_time) > 200 {
            if !d.moved {
                d.offset = delta;
            }
            let mut target = d.content - (delta - d.offset);
            if self.settings.bounds_behavior.drag() {
                if target < 0. {
                    target *= 0.5;
                } else if target > d.maximum_y {
                    target = d.maximum_y + (target - d.maximum_y) * 0.5;
                }
            } else {
                target = target.clamp(0., d.maximum_y);
                reject = (d.content == 0. && self.raw_y == 0. && delta > 0.)
                    || (d.content == d.maximum_y && self.raw_y == d.maximum_y && delta < 0.);
            }
            if !reject && d.steal && delta != d.previous_delta {
                self.timeline = None;
                self.raw_y = target;
                d.moved = true;
            }
            if !reject && over {
                d.steal = true;
            }
        }
        if reject {
            self.samples.clear();
            self.velocity = 0.;
        } else {
            self.sample(-v);
        }
        d.previous_delta = delta;
        d.last_pointer = y;
        d.last_time = time;
        let grabbed = d.steal;
        self.drag = Some(d);
        grabbed
    }
    /// Release returns whether the child's click was consumed by scrolling.
    pub fn release(&mut self, pointer_y: f64, now: Duration) -> bool {
        self.now = millis(now);
        if self.wheel_drag {
            return false;
        }
        let Some(d) = self.drag.take() else {
            return false;
        };
        let grabbed = d.steal;
        let mut velocity = self.average();
        if self.now.saturating_sub(d.last_time) >= 100 {
            velocity = 0.;
        }
        if (self.raw_y <= 0. && velocity < 0.) || (self.raw_y >= self.maximum_y() && velocity > 0.)
        {
            velocity *= 0.5;
        } else if self.content / self.viewport.max(1.) > 10.
            && velocity.abs() > 1250.
            && velocity * self.last_flick_velocity > 0.
        {
            self.boost = (self.boost + 0.25).min(3.);
        } else {
            self.boost = 1.;
        }
        velocity *= self.boost;
        if grabbed && velocity.abs() > 75. && (pointer_y - d.pointer).abs() > 15. {
            self.start_flick(velocity, false);
        } else {
            self.return_to_bounds();
        }
        grabbed
    }
    pub fn cancel(&mut self) {
        self.drag = None;
        self.wheel_drag = false;
        self.wheel_phase = false;
        self.return_to_bounds();
    }
    fn queue(
        &mut self,
        queue: VecDeque<Segment>,
        flick: bool,
        fixing: bool,
        first_flick_step: bool,
    ) {
        self.timeline = (!queue.is_empty()).then_some(Timeline {
            queue,
            elapsed: 0,
            last_frame: if self.timeline.is_some() {
                Some(self.now)
            } else {
                None
            },
            flick,
            overshoot: false,
            fixing,
            first_flick_step,
        });
    }
    fn accel(&self, velocity: f64, deceleration: f64, distance: Option<f64>) -> Option<Segment> {
        if velocity == 0. || !velocity.is_finite() {
            return None;
        }
        let mut a = deceleration.max(0.001);
        if let Some(d) = distance {
            if d <= 0. {
                return None;
            }
            a = a.max(velocity * velocity / (2. * d));
        }
        let duration = (1000. * velocity.abs() / a) as u64;
        (duration > 0).then_some(Segment::Accel {
            from: self.raw_y,
            velocity,
            acceleration: -a.copysign(velocity),
            duration,
        })
    }
    fn start_flick(&mut self, velocity: f64, wheel: bool) {
        let max = self.settings.maximum_flick_velocity;
        let v = if max < 0. {
            velocity
        } else {
            velocity.clamp(-max, max)
        };
        let distance = if v > 0. {
            (self.maximum_y() - self.raw_y).abs()
        } else {
            self.raw_y.abs()
        };
        if distance == 0. && !self.settings.bounds_behavior.flick() {
            self.return_to_bounds();
            return;
        }
        let a = if wheel {
            self.settings.wheel_deceleration
        } else {
            self.settings.flick_deceleration
        };
        let target = (self.raw_y + v.signum() * v * v / (2. * a.max(0.001))).round();
        let adjusted = v * v / (2. * (target - self.raw_y).abs());
        let segment = self.accel(
            v,
            adjusted,
            (!self.settings.bounds_behavior.flick()).then_some(distance),
        );
        self.last_flick_velocity = v;
        self.velocity = v;
        self.queue(segment.into_iter().collect(), true, false, !wheel);
    }
    fn rebound_queue(&self) -> VecDeque<Segment> {
        let target = self.raw_y.clamp(0., self.maximum_y());
        if target == self.raw_y {
            return VecDeque::new();
        }
        let mid = (self.raw_y + target) / 2.;
        VecDeque::from([
            Segment::Ease {
                from: self.raw_y,
                to: mid,
                curve: Curve::InQuad,
                duration: 100,
            },
            Segment::Ease {
                from: mid,
                to: target,
                curve: Curve::OutExpo,
                duration: 300,
            },
        ])
    }
    pub fn return_to_bounds(&mut self) {
        let q = self.rebound_queue();
        if q.is_empty() {
            self.raw_y = self.raw_y.round();
        }
        self.queue(q, false, true, false);
    }
    pub fn wheel(&mut self, event: Wheel, now: Duration) -> bool {
        self.frame(now);
        self.now = millis(now);
        match event.phase {
            ScrollPhase::Begin => {
                self.wheel_phase = true;
                self.wheel_rebound = false;
                self.wheel_peak = 0.;
                self.wheel_pixels = 0.;
                self.samples.clear();
                self.velocity = 0.;
                self.press(0., now);
                self.wheel_drag = true;
                self.wheel_time = None;
            }
            ScrollPhase::End => {
                self.drag = None;
                self.wheel_phase = false;
                self.wheel_drag = false;
                self.wheel_time = None;
                self.return_to_bounds();
                return false;
            }
            ScrollPhase::Momentum => {
                self.wheel_phase = false;
            }
            _ => {}
        }
        if (self.content - self.viewport).abs().floor() == 0. {
            return false;
        }
        let elapsed = self.wheel_time.map_or((self.now + 1) as f64 / 1000., |t| {
            self.now.saturating_sub(t) as f64 / 1000.
        });
        self.wheel_time = Some(self.now);
        if elapsed <= 0. {
            return false;
        }
        if !event.synthesized || event.phase == ScrollPhase::None || event.pixel_delta == 0. {
            let delta = finite(event.angle_delta);
            if delta == 0. {
                return false;
            }
            if (delta > 0. && self.raw_y <= 0.) || (delta < 0. && self.raw_y >= self.maximum_y()) {
                return false;
            }
            if self.settings.wheel_deceleration > 14999. {
                let mut to = self.raw_y - delta / 120. * self.settings.wheel_scroll_lines * 24.;
                if self.settings.bounds_behavior == BoundsBehavior::StopAtBounds {
                    to = to.clamp(0., self.maximum_y());
                }
                self.queue(
                    VecDeque::from([Segment::Ease {
                        from: self.raw_y,
                        to,
                        curve: Curve::OutExpo,
                        duration: 300,
                    }]),
                    false,
                    true,
                    false,
                );
            } else {
                let dt = if !self.moving() {
                    120. / (self.settings.wheel_deceleration
                        * 2.
                        * self.settings.wheel_scroll_lines
                        * 24.)
                        .sqrt()
                } else {
                    elapsed
                };
                let v = -delta / dt;
                if v * self.velocity < 0. {
                    self.samples.clear();
                }
                self.sample(v);
                let v = self.average();
                self.start_flick(v, true);
            }
            true
        } else {
            // A native ScrollUpdate without ScrollBegin has no pressed session
            // in QQuickFlickable; it cannot start a touchpad drag by itself.
            if event.phase == ScrollPhase::Update && !self.wheel_phase && self.drag.is_none() {
                return false;
            }
            self.wheel_drag = true;
            self.wheel_pixels += finite(event.pixel_delta);
            let attempted = self
                .drag
                .as_ref()
                .map_or(self.raw_y, |d| d.content - (self.wheel_pixels - d.offset));
            if event.phase == ScrollPhase::Momentum
                && ((self.raw_y <= 0. && attempted < 0.)
                    || (self.raw_y >= self.maximum_y() && attempted > self.maximum_y()))
            {
                if !self.wheel_rebound {
                    self.wheel_rebound = true;
                    self.return_to_bounds();
                }
                return self.moving();
            }
            if self.drag.is_none() {
                self.drag = Some(Drag {
                    pointer: 0.,
                    content: self.raw_y,
                    maximum_y: self.maximum_y(),
                    last_pointer: 0.,
                    last_time: self.now,
                    press_time: self.now,
                    offset: 0.,
                    previous_delta: 0.,
                    steal: false,
                    moved: false,
                    filtered_velocity: 0.,
                });
            }
            // Wheel ScrollBegin establishes the press; the first update acquires
            // the grab, the next establishes dragStartOffset, just like drag().
            let y = self.wheel_pixels;
            let v = event.pixel_delta / elapsed;
            let max = self.settings.maximum_flick_velocity.max(0.);
            let friction = (v / 8.).clamp(-max, max);
            if (friction > 0. && friction > self.wheel_peak)
                || (friction < 0. && friction < self.wheel_peak)
            {
                self.wheel_peak = friction;
            }
            let mut d = self.drag.take().unwrap();
            let target = d.content - (y - d.offset);
            if !d.moved {
                d.offset = y;
            }
            if d.steal && y != d.previous_delta {
                let mut raw = if d.moved { target } else { d.content };
                if !self.settings.bounds_behavior.drag() {
                    raw = raw.clamp(0., self.maximum_y());
                } else if raw < 0. || raw > self.maximum_y() {
                    let bound = raw.clamp(0., self.maximum_y());
                    let over = (raw - bound) * self.wheel_peak.abs() / max.max(0.001) / 8.;
                    let cap = 150. * self.settings.device_pixel_ratio.max(0.001);
                    raw = bound + cap * (over / cap).atan();
                }
                self.raw_y = raw;
                d.moved = true;
                self.timeline = None;
            }
            d.steal = true;
            d.previous_delta = y;
            self.drag = Some(d);
            self.raw_y > 0. && self.raw_y < self.maximum_y()
        }
    }
    /// Sample the local Qt-style timeline at the host's frame time. Starting a
    /// timeline uses its first presentation as t=0; subsequent frames integrate
    /// exactly, including integer-ms acceleration lengths and queued rebound.
    pub fn frame(&mut self, now: Duration) -> bool {
        self.now = millis(now);
        let before = self.content_y();
        let Some(mut t) = self.timeline.take() else {
            return false;
        };
        let Some(last) = t.last_frame.replace(self.now) else {
            self.timeline = Some(t);
            return false;
        };
        let frame_dt = self.now.saturating_sub(last);
        let mut remaining = frame_dt;
        let mut first = true;
        while remaining > 0 {
            let Some(segment) = t.queue.front().copied() else {
                break;
            };
            let mut step = remaining.min(segment.duration().saturating_sub(t.elapsed));
            if t.first_flick_step {
                step = step.min(1);
                t.first_flick_step = false;
            }
            let old = self.raw_y;
            t.elapsed += step;
            self.raw_y = segment.value(t.elapsed);
            remaining -= step;
            if first && frame_dt > 0 {
                self.velocity = (self.raw_y - old) * 1000. / frame_dt as f64;
                first = false;
            }
            if t.flick
                && !t.overshoot
                && !t.fixing
                && (self.raw_y < 0. || self.raw_y > self.maximum_y())
                && self.velocity.abs() > 10.
            {
                let beyond = (self.raw_y - self.raw_y.clamp(0., self.maximum_y())).abs();
                let distance = (self.velocity.abs() / 3.).min(150.) - beyond;
                t.queue = self
                    .accel(
                        self.velocity,
                        self.settings.flick_deceleration * 8.,
                        Some(distance),
                    )
                    .into_iter()
                    .collect();
                t.elapsed = 0;
                t.overshoot = true;
                if t.queue.is_empty() {
                    t.queue = self.rebound_queue();
                    t.fixing = true;
                    t.flick = false;
                }
                continue;
            }
            if t.elapsed >= segment.duration() {
                t.queue.pop_front();
                t.elapsed = 0;
                if t.queue.is_empty() {
                    if self.fixup_immediate {
                        self.raw_y = self.raw_y.clamp(0., self.maximum_y());
                    }
                    self.fixup_immediate = false;
                    t.queue = self.rebound_queue();
                    t.fixing = true;
                    t.flick = false;
                    if t.queue.is_empty() {
                        self.raw_y = if (self.raw_y.round() - self.raw_y).abs() < 0.25 {
                            self.raw_y.round()
                        } else if self.velocity > 0. {
                            self.raw_y.floor()
                        } else if self.velocity < 0. {
                            self.raw_y.ceil()
                        } else {
                            self.raw_y.round()
                        };
                    }
                }
            }
            if step == 0 {
                break;
            }
        }
        if !t.queue.is_empty() {
            self.timeline = Some(t);
        } else {
            self.velocity = 0.;
        }
        before != self.content_y()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    enum Event {
        Press(f64),
        Move(f64, Option<f64>),
        Release(f64),
        Wheel(Wheel),
        Set(&'static str, f64),
    }
    type Case = (&'static str, bool, Vec<(u64, Event)>);
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/flickable-implementation/cases.rs"
    ));
    #[test]
    fn qt_612_frame_traces() {
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/flickable-implementation");
        let mut failures = Vec::new();
        let mut traces = 0;
        let mut frames = 0;
        let mut maximum_error = 0f64;
        for (name, list, events) in cases() {
            let mut settings = if list {
                Settings::list_view()
            } else {
                Settings::default()
            };
            if name.contains("accel") {
                settings.wheel_deceleration = 5000.;
            }
            let mut f = Flickable::new(settings);
            f.set_extents(
                if list { 386. } else { 402. },
                if list { 1492. } else { 1049. },
            );
            let text = std::fs::read_to_string(
                base.join(format!("{}-{name}.csv", if list { "list" } else { "menu" })),
            )
            .unwrap();
            traces += 1;
            let mut worst = (0., 0, 0., 0.);
            for line in text.lines().skip(1) {
                frames += 1;
                let c: Vec<_> = line.split(',').collect();
                let time = c[0].parse::<u64>().unwrap();
                let expected = c[1].parse::<f64>().unwrap();
                let now = Duration::from_millis(time);
                f.frame(now);
                for (_, event) in events.iter().filter(|(t, _)| *t == time) {
                    match event {
                        Event::Press(y) => f.press(*y, now),
                        Event::Move(y, v) => {
                            f.drag_to(*y, *v, now);
                        }
                        Event::Release(y) => {
                            f.release(*y, now);
                        }
                        Event::Set(key, y) => match *key {
                            "contentY" => f.set_content_y(*y),
                            "contentHeight" => f.set_extents(f.viewport, *y),
                            "model" => f.set_extents(
                                f.viewport,
                                (*y * 42. + (*y - 1.).max(0.) * 8.).max(0.),
                            ),
                            "boundsMovement" => {
                                f.settings.bounds_movement = BoundsMovement::StopAtBounds
                            }
                            "boundsBehavior" => {
                                f.settings.bounds_behavior = if *y == 1. {
                                    BoundsBehavior::DragOverBounds
                                } else {
                                    BoundsBehavior::OvershootBounds
                                }
                            }
                            _ => panic!("unknown property"),
                        },
                        Event::Wheel(w) => {
                            f.wheel(*w, now);
                        }
                    }
                }
                let actual = f.content_y();
                let diff = (actual - expected).abs();
                if diff > worst.0 {
                    worst = (diff, time, expected, actual);
                }
            }
            maximum_error = maximum_error.max(worst.0);
            if worst.0 > 0.00001 {
                failures.push(format!(
                    "{}-{name}: max={} at {} Qt={} Rust={}",
                    if list { "list" } else { "menu" },
                    worst.0,
                    worst.1,
                    worst.2,
                    worst.3
                ));
            }
            assert!(!f.animating(), "{name}: timeline leaked at rest");
        }
        eprintln!("Qt 6.12: {traces} traces, {frames} frame samples, maximum contentY error {maximum_error:.12} logical px");
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
    #[test]
    fn exactly_fitting_content_does_not_steal_child_clicks() {
        let mut f = Flickable::default();
        f.set_extents(100., 100.);
        f.press(80., Duration::ZERO);
        assert!(!f.drag_to(40., None, Duration::from_millis(16)));
        assert!(!f.drag_to(0., None, Duration::from_millis(32)));
        assert!(!f.release(0., Duration::from_millis(48)));
        assert_eq!(f.content_y(), 0.);
        assert!(!f.animating());
    }

    #[test]
    fn discrete_angles_take_priority_in_unphased_axis_frames() {
        let w = Wheel::deltas(-120., -10.);
        assert!(!w.synthesized);
        assert_eq!(w.phase, ScrollPhase::None);
    }

    #[test]
    fn pixel_gesture_is_not_a_mouse_press() {
        let mut f = Flickable::default();
        f.set_extents(100., 1000.);
        for n in 0..4 {
            f.wheel(Wheel::deltas(0., -10.), Duration::from_millis(n * 16));
        }
        let y = f.content_y();
        assert!(!f.drag_to(500., None, Duration::from_millis(80)));
        assert_eq!(f.content_y(), y);
        assert!(!f.release(500., Duration::from_millis(96)));
        assert!(!f.animating());
        f.reset(y);
        assert!(!f.grabbed());
    }

    #[test]
    fn cancel_and_shrink_leave_no_wakeups() {
        let mut f = Flickable::default();
        f.set_extents(100., 1000.);
        f.wheel(Wheel::deltas(-120., 0.), Duration::ZERO);
        f.reset(500.);
        assert!(!f.animating());
        f.set_extents(100., 150.);
        assert_eq!(f.content_y(), 50.);
        assert!(!f.animating());
    }
}
