//! Qt 6.12 Wayland wheel conversion. Coordinates are logical pixels; angle units
//! are eighths of a degree. Relative direction is metadata, never a second sign flip.
//! Derived from qtbase v6.12.0 qwaylandinputdevice.cpp (LGPL-3.0-only).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollPhase {
    #[default]
    None,
    Begin,
    Update,
    End,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollSource {
    #[default]
    Wheel,
    Finger,
    Continuous,
    WheelTilt,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollEvent {
    pub pixel_delta: (f64, f64),
    pub angle_delta: (f64, f64),
    pub phase: ScrollPhase,
    pub synthesized: bool,
    pub inverted: bool,
}
#[derive(Default, Debug)]
pub(crate) struct ScrollState {
    pub delta: [f64; 2],
    pub delta120: [i32; 2],
    pub source: ScrollSource,
    pub inverted: [bool; 2],
    pub stop: bool,
    begun: bool,
    remainder: [f64; 2],
}
impl ScrollState {
    pub fn finish(&mut self) -> Vec<ScrollEvent> {
        let angle = if self.delta120 == [0, 0] {
            self.delta.map(|v| (-12. * v).round())
        } else {
            self.delta120.map(|v| -(v as f64))
        };
        let synthesized = matches!(self.source, ScrollSource::Finger | ScrollSource::Continuous);
        let inverted = self.inverted.iter().any(|v| *v);
        let mut out = Vec::with_capacity(3);
        if angle != [0., 0.] {
            if self.source == ScrollSource::Finger && !self.begun {
                self.begun = true;
                self.remainder = [0., 0.];
                out.push(ScrollEvent {
                    phase: ScrollPhase::Begin,
                    ..Default::default()
                });
            }
            let mut pixel = [0.; 2];
            for (i, p) in pixel.iter_mut().enumerate() {
                let rounded = (self.delta[i] + self.remainder[i]).round();
                self.remainder[i] += self.delta[i] - rounded;
                *p = -rounded;
            }
            out.push(ScrollEvent {
                pixel_delta: (pixel[0], pixel[1]),
                angle_delta: (angle[0], angle[1]),
                phase: if self.begun {
                    ScrollPhase::Update
                } else {
                    ScrollPhase::None
                },
                synthesized,
                inverted,
            });
        }
        if self.stop && self.begun {
            out.push(ScrollEvent {
                phase: ScrollPhase::End,
                synthesized,
                inverted,
                ..Default::default()
            });
            self.begun = false;
            self.remainder = [0., 0.];
        }
        self.delta = [0., 0.];
        self.delta120 = [0, 0];
        self.source = ScrollSource::Wheel;
        self.inverted = [false; 2];
        self.stop = false;
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finger_begin_fractional_remainder_and_axis_stop() {
        let mut s = ScrollState::default();
        let mut sum = 0.;
        for i in 0..10 {
            s.source = ScrollSource::Finger;
            s.delta[1] = 0.2;
            let out = s.finish();
            assert_eq!(out.len(), if i == 0 { 2 } else { 1 });
            if i == 0 {
                assert_eq!(out[0].phase, ScrollPhase::Begin);
            }
            let e = out.last().unwrap();
            assert_eq!(e.phase, ScrollPhase::Update);
            assert!(e.synthesized);
            sum += e.pixel_delta.1;
        }
        assert_eq!(sum, -2.);
        s.stop = true;
        let end = s.finish();
        assert_eq!(end.len(), 1);
        assert_eq!(end[0].phase, ScrollPhase::End);
        assert!(s.finish().is_empty());
    }
    #[test]
    fn high_resolution_wheel_and_inversion_metadata() {
        let mut s = ScrollState::default();
        s.delta120[1] = 15;
        s.delta[1] = 1.25;
        s.inverted[1] = true;
        let e = s.finish()[0];
        assert_eq!(e.angle_delta.1, -15.);
        assert_eq!(e.pixel_delta.1, -1.);
        assert_eq!(e.phase, ScrollPhase::None);
        assert!(!e.synthesized);
        assert!(e.inverted);
    }
    #[test]
    fn continuous_has_pixels_without_invented_phases() {
        let mut s = ScrollState {
            source: ScrollSource::Continuous,
            delta: [-1.5, 2.5],
            ..Default::default()
        };
        let e = s.finish()[0];
        assert_eq!(e.angle_delta, (18., -30.));
        assert_eq!(e.pixel_delta, (2., -3.));
        assert_eq!(e.phase, ScrollPhase::None);
        s.stop = true;
        assert!(s.finish().is_empty());
    }
}
