//! Frame-timestamp driven Qt-compatible interpolation. No timers or worker threads.
use std::{
    f64::consts::{FRAC_PI_2, PI},
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Quad,
    Cubic,
    Quart,
    Quint,
    Sine,
    Expo,
    Circ,
    Back,
    Elastic,
    Bounce,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    In,
    Out,
    InOut,
    OutIn,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier {
    pub control1: (f64, f64),
    pub control2: (f64, f64),
    pub end: (f64, f64),
}
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Easing {
    #[default]
    Linear,
    Curve {
        family: Family,
        direction: Direction,
        overshoot: f64,
        amplitude: f64,
        period: f64,
    },
    /// Continuous cubic segments from (0,0), ending at (1,1); x must be monotonic.
    BezierSpline(Vec<CubicBezier>),
}
impl Easing {
    pub fn new(family: Family, direction: Direction) -> Self {
        Self::Curve {
            family,
            direction,
            overshoot: 1.70158,
            amplitude: 1.,
            period: 0.3,
        }
    }
    pub fn value(&self, progress: f64) -> f64 {
        let t = progress.clamp(0., 1.);
        match self {
            Self::Linear => t,
            Self::BezierSpline(segments) => {
                if t == 0. || t == 1. || segments.last().is_none_or(|b| b.end != (1., 1.)) {
                    return t;
                }
                let mut start = (0., 0.);
                for b in segments {
                    if t <= b.end.0 {
                        let u = crate::qt_bezier::parameter(
                            start.0,
                            b.control1.0,
                            b.control2.0,
                            b.end.0,
                            t,
                        );
                        return cubic(start.1, b.control1.1, b.control2.1, b.end.1, u);
                    }
                    start = b.end;
                }
                t
            }
            Self::Curve {
                family: f,
                direction: d,
                overshoot: s,
                amplitude: a,
                period: p,
            } => {
                use Direction::*;
                use Family::*;
                let s = if *s < 0. { 1.70158 } else { *s };
                let a = if *a < 0. { 1. } else { *a };
                let p = if *p <= 0. { 0.3 } else { *p };
                // Qt Expo is intentionally not symmetric under 1-f(1-t).
                if *f == Expo {
                    return match d {
                        In => expo_in(t),
                        Out => expo_out(t),
                        InOut => {
                            if t == 0. || t == 1. {
                                t
                            } else if t < 0.5 {
                                0.5 * 2f64.powf(10. * (2. * t - 1.)) - 0.0005
                            } else {
                                0.5 * 1.0005 * (-2f64.powf(-10. * (2. * t - 1.)) + 2.)
                            }
                        }
                        OutIn => {
                            if t < 0.5 {
                                expo_out(2. * t) * 0.5
                            } else {
                                expo_in(2. * t - 1.) * 0.5 + 0.5
                            }
                        }
                    };
                }
                if *f == Elastic {
                    return match d {
                        In => elastic(t, 1., a, p, true),
                        Out => elastic(t, 1., a, p, false),
                        InOut => {
                            if t == 0. || t == 1. {
                                t
                            } else if t < 0.5 {
                                elastic(t * 2., 1., a, p, true) * 0.5
                            } else {
                                elastic(t * 2. - 1., 1., a, p, false) * 0.5 + 0.5
                            }
                        }
                        OutIn => {
                            if t < 0.5 {
                                elastic(t * 2., 0.5, a, p, false)
                            } else {
                                elastic(t * 2. - 1., 0.5, a, p, true) + 0.5
                            }
                        }
                    };
                }
                if *f == Bounce && *d == OutIn {
                    return if t < 0.5 {
                        bounce(t * 2., 0.5, a)
                    } else {
                        1. - bounce(2. - 2. * t, 0.5, a)
                    };
                }
                let ss = if *f == Back && *d == InOut {
                    s * (1.525f32 as f64)
                } else {
                    s
                };
                let input = |x: f64| match f {
                    Quad => x * x,
                    Cubic => x * x * x,
                    Quart => x.powi(4),
                    Quint => x.powi(5),
                    Sine => {
                        if x == 1. {
                            1.
                        } else {
                            1. - (x * FRAC_PI_2).cos()
                        }
                    }
                    Circ => 1. - (1. - x * x).sqrt(),
                    Back => x * x * ((ss + 1.) * x - ss),
                    Bounce => 1. - bounce(1. - x, 1., a),
                    _ => unreachable!(),
                };
                match d {
                    In => input(t),
                    Out => 1. - input(1. - t),
                    InOut => {
                        if t < 0.5 {
                            input(2. * t) * 0.5
                        } else {
                            1. - input(2. - 2. * t) * 0.5
                        }
                    }
                    OutIn => {
                        if t < 0.5 {
                            (1. - input(1. - 2. * t)) * 0.5
                        } else {
                            input(2. * t - 1.) * 0.5 + 0.5
                        }
                    }
                }
            }
        }
    }
}
fn cubic(a: f64, b: f64, c: f64, d: f64, t: f64) -> f64 {
    let u = 1. - t;
    u * u * u * a + 3. * u * u * t * b + 3. * u * t * t * c + t * t * t * d
}
fn expo_in(t: f64) -> f64 {
    if t == 0. || t == 1. {
        t
    } else {
        2f64.powf(10. * (t - 1.)) - 0.001
    }
}
fn expo_out(t: f64) -> f64 {
    if t == 1. {
        1.
    } else {
        1.001 * (1. - 2f64.powf(-10. * t))
    }
}
fn elastic(t: f64, c: f64, mut a: f64, p: f64, input: bool) -> f64 {
    if t == 0. {
        return 0.;
    }
    if t == 1. {
        return c;
    }
    let s = if a < c {
        a = c;
        p / 4.
    } else {
        p / (2. * PI) * (c / a).asin()
    };
    if input {
        -a * 2f64.powf(10. * (t - 1.)) * ((t - 1. - s) * 2. * PI / p).sin()
    } else {
        a * 2f64.powf(-10. * t) * ((t - s) * 2. * PI / p).sin() + c
    }
}
fn bounce(mut t: f64, c: f64, a: f64) -> f64 {
    if t == 1. {
        c
    } else if t < 4. / 11. {
        c * 7.5625 * t * t
    } else if t < 8. / 11. {
        t -= 6. / 11.;
        c - a * (1. - (7.5625 * t * t + 0.75))
    } else if t < 10. / 11. {
        t -= 9. / 11.;
        c - a * (1. - (7.5625 * t * t + 0.9375))
    } else {
        t -= 21. / 22.;
        c - a * (1. - (7.5625 * t * t + 0.984375))
    }
}

pub trait Interpolate: Clone + PartialEq {
    fn interpolate(&self, to: &Self, progress: f64) -> Self;
}
impl Interpolate for f64 {
    fn interpolate(&self, to: &Self, t: f64) -> Self {
        self + (to - self) * t
    }
}
/// Unpremultiplied RGBA, matching QColorAnimation (no gamma conversion).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}
impl Color {
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
}
impl Interpolate for Color {
    fn interpolate(&self, to: &Self, t: f64) -> Self {
        let mix = |a: f32, b: f32| (a + (b - a) * t as f32).clamp(0., 1.);
        Self::rgba(
            mix(self.r, to.r),
            mix(self.g, to.g),
            mix(self.b, to.b),
            mix(self.a, to.a),
        )
    }
}
/// Behavior semantics: retarget at `now` samples the old animation before replacing it.
#[derive(Clone, Debug)]
pub struct Behavior<T: Interpolate> {
    from: T,
    target: T,
    start: Duration,
    duration: Duration,
    easing: Easing,
    running: bool,
    pub enabled: bool,
}
impl<T: Interpolate> Behavior<T> {
    pub fn new(value: T, duration: Duration, easing: Easing) -> Self {
        Self {
            from: value.clone(),
            target: value,
            start: Duration::ZERO,
            duration,
            easing,
            running: false,
            enabled: true,
        }
    }
    pub fn value(&self, now: Duration) -> T {
        if !self.running || self.duration.is_zero() {
            self.target.clone()
        } else {
            self.from.interpolate(
                &self.target,
                self.easing.value(
                    now.saturating_sub(self.start).as_secs_f64() / self.duration.as_secs_f64(),
                ),
            )
        }
    }
    pub fn retarget(&mut self, to: T, now: Duration) {
        if self.target == to {
            return;
        }
        self.from = self.value(now);
        self.target = to;
        self.start = now;
        self.running = self.enabled && !self.duration.is_zero();
    }
    pub fn advance(&mut self, now: Duration) -> T {
        let value = self.value(now);
        if now.saturating_sub(self.start) >= self.duration {
            self.running = false;
        }
        value
    }
    pub fn is_running(&self) -> bool {
        self.running
    }
    pub fn finish(&mut self) -> T {
        self.running = false;
        self.target.clone()
    }
}
/// Qt Quick's semi-implicit, fixed 16ms spring integrator. Retarget preserves velocity.
#[derive(Clone, Debug)]
pub struct Spring {
    pub value: f64,
    pub velocity: f64,
    pub target: f64,
    pub spring: f64,
    pub damping: f64,
    pub mass: f64,
    pub epsilon: f64,
    /// Units/second; zero means unlimited (or track mode when spring is zero).
    pub max_velocity: f64,
    pub modulus: Option<f64>,
    last_ms: u64,
    start_ms: u64,
    duration_ms: u64,
    running: bool,
}
impl Spring {
    pub fn new(value: f64) -> Self {
        Self {
            value,
            velocity: 0.,
            target: value,
            spring: 0.,
            damping: 0.,
            mass: 1.,
            epsilon: 0.01,
            max_velocity: 0.,
            modulus: None,
            last_ms: 0,
            start_ms: 0,
            duration_ms: 0,
            running: false,
        }
    }
    pub fn retarget(&mut self, to: f64, now: Duration) {
        self.target = to;
        self.last_ms = now.as_millis() as u64;
        self.start_ms = self.last_ms;
        self.duration_ms = (self.diff().abs() / self.max_velocity * 1000.) as u64;
        self.running = true;
        if self.spring == 0. && self.max_velocity == 0. {
            self.value = to;
            self.running = false;
        }
    }
    fn diff(&self) -> f64 {
        let mut d = self.target - self.value;
        if let Some(m) = self.modulus {
            d %= m;
            if d.abs() > m / 2. {
                d -= m * d.signum();
            }
        }
        d
    }
    pub fn advance(&mut self, now: Duration) -> f64 {
        if !self.running {
            return self.value;
        }
        let time = now.as_millis() as u64;
        let elapsed = time.saturating_sub(self.last_ms);
        if self.spring > 0. {
            if elapsed < 16 {
                return self.value;
            }
            let destination = self.modulus.map_or(self.target, |m| self.target % m);
            let steps = elapsed / 16;
            self.last_ms += steps * 16;
            for _ in 0..steps {
                self.velocity = if self.mass == 1. {
                    self.velocity + self.spring * self.diff() - self.damping * self.velocity
                } else {
                    self.velocity
                        + (self.spring * self.diff() - self.damping * self.velocity) / self.mass
                };
                if self.max_velocity > 0. {
                    self.velocity = self.velocity.clamp(-self.max_velocity, self.max_velocity);
                }
                self.value += self.velocity * 0.016;
                if let Some(m) = self.modulus {
                    self.value = self.value.rem_euclid(m);
                }
            }
            if self.velocity.abs() < self.epsilon && (destination - self.value).abs() < self.epsilon
            {
                self.value = destination;
                self.velocity = 0.;
                self.running = false;
            }
        } else {
            self.value += self.diff().signum() * elapsed as f64 * self.max_velocity / 1000.;
            self.last_ms = time;
            if let Some(m) = self.modulus {
                self.value = self.value.rem_euclid(m);
            }
            if time.saturating_sub(self.start_ms) >= self.duration_ms {
                self.value = self.target;
                self.running = false;
            }
        }
        self.value
    }
    pub fn is_running(&self) -> bool {
        self.running
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn qt_reference_samples() {
        // Fixture generated by the installed Qt QEasingCurve, not by this implementation.
        for line in include_str!("../tests/qt-easing.csv")
            .lines()
            .filter(|l| !l.starts_with('#'))
        {
            let v: Vec<_> = line.split(',').collect();
            let family = match v[0] {
                "Quad" => Family::Quad,
                "Cubic" => Family::Cubic,
                "Quart" => Family::Quart,
                "Quint" => Family::Quint,
                "Sine" => Family::Sine,
                "Expo" => Family::Expo,
                "Circ" => Family::Circ,
                "Back" => Family::Back,
                "Elastic" => Family::Elastic,
                "Bounce" => Family::Bounce,
                _ => panic!(),
            };
            let direction = match v[1] {
                "In" => Direction::In,
                "Out" => Direction::Out,
                "InOut" => Direction::InOut,
                "OutIn" => Direction::OutIn,
                _ => panic!(),
            };
            let t = v[2].parse().unwrap();
            let expected: f64 = v[3].parse().unwrap();
            let actual = Easing::new(family, direction).value(t);
            assert!(
                (actual - expected).abs() < 1e-12,
                "{} {} at {t}: {actual} != {expected}",
                v[0],
                v[1]
            );
        }
    }
    #[test]
    fn retarget_is_continuous() {
        let mut b = Behavior::new(0., Duration::from_secs(1), Easing::Linear);
        b.retarget(100., Duration::ZERO);
        b.retarget(200., Duration::from_millis(400));
        assert_eq!(b.value(Duration::from_millis(400)), 40.);
        assert_eq!(b.value(Duration::from_millis(900)), 120.);
    }
    #[test]
    fn qt_spring_steps() {
        let mut s = Spring::new(0.);
        s.spring = 2.;
        s.damping = 0.2;
        s.retarget(100., Duration::ZERO);
        assert_eq!(s.advance(Duration::from_millis(15)), 0.);
        assert_eq!(s.advance(Duration::from_millis(16)), 3.2);
        assert!((s.advance(Duration::from_millis(32)) - 8.8576).abs() < 1e-12);
    }
    #[test]
    fn colors_are_straight_rgba() {
        assert_eq!(
            Color::rgba(1., 0., 0., 0.).interpolate(&Color::rgba(0., 0., 1., 1.), 0.5),
            Color::rgba(0.5, 0., 0.5, 0.5)
        );
    }
    #[test]
    fn bezier_inverts_x() {
        let e = Easing::BezierSpline(vec![CubicBezier {
            control1: (0.25, 0.1),
            control2: (0.25, 1.),
            end: (1., 1.),
        }]);
        assert!((e.value(0.5) - 0.802403387584857).abs() < 1e-3);
    }
}

#[cfg(test)]
mod custom_tests {
    use super::*;
    #[test]
    fn qt_custom_curve_reference() {
        for line in include_str!("../tests/qt-special.csv")
            .lines()
            .filter(|l| !l.starts_with('#'))
        {
            let v: Vec<_> = line.split(',').collect();
            let curve = match v[0] {
                "Back2" => Easing::Curve {
                    family: Family::Back,
                    direction: Direction::Out,
                    overshoot: 2.,
                    amplitude: 1.,
                    period: 0.3,
                },
                "Elastic2" => Easing::Curve {
                    family: Family::Elastic,
                    direction: Direction::InOut,
                    overshoot: 1.70158,
                    amplitude: 2.,
                    period: 0.4,
                },
                "Bounce03" => Easing::Curve {
                    family: Family::Bounce,
                    direction: Direction::OutIn,
                    overshoot: 1.70158,
                    amplitude: 0.3,
                    period: 0.3,
                },
                "BarBezier" => Easing::BezierSpline(vec![CubicBezier {
                    control1: (0.1, 0.9),
                    control2: (0.1, 1.05),
                    end: (1., 1.),
                }]),
                "MultiBezier" => Easing::BezierSpline(vec![
                    CubicBezier {
                        control1: (0.1, 0.3),
                        control2: (0.3, 0.6),
                        end: (0.5, 0.5),
                    },
                    CubicBezier {
                        control1: (0.6, 0.4),
                        control2: (0.8, 1.2),
                        end: (1., 1.),
                    },
                ]),
                _ => panic!(),
            };
            let t = v[1].parse().unwrap();
            let expected: f64 = v[2].parse().unwrap();
            let actual = curve.value(t);
            assert!(
                (actual - expected).abs() < 1e-12,
                "{} at {t}: {actual} != {expected}",
                v[0]
            );
        }
    }
    #[test]
    fn spring_settles_and_retargets_with_velocity() {
        let mut s = Spring::new(0.);
        s.spring = 2.;
        s.damping = 0.25;
        s.retarget(100., Duration::ZERO);
        s.advance(Duration::from_millis(32));
        let velocity = s.velocity;
        s.retarget(-20., Duration::from_millis(32));
        assert_eq!(s.velocity, velocity);
        for ms in (48..20000).step_by(16) {
            s.advance(Duration::from_millis(ms));
        }
        assert_eq!(s.value, -20.);
        assert!(!s.is_running());
    }
}

#[cfg(test)]
mod behavior_tests {
    use super::*;
    #[test]
    fn unchanged_target_does_not_restart() {
        let mut b = Behavior::new(0., Duration::from_secs(1), Easing::Linear);
        b.retarget(100., Duration::ZERO);
        b.retarget(100., Duration::from_millis(500));
        assert_eq!(b.advance(Duration::from_secs(1)), 100.);
        assert!(!b.is_running());
    }
    #[test]
    fn modular_spring_settles_at_qt_remainder() {
        let mut s = Spring::new(350.);
        s.spring = 2.;
        s.damping = 0.25;
        s.modulus = Some(360.);
        s.retarget(370., Duration::ZERO);
        for ms in (16..20000).step_by(16) {
            s.advance(Duration::from_millis(ms));
        }
        assert_eq!(s.value, 10.);
        assert!(!s.is_running());
    }
}
