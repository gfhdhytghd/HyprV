//! V3 SpectrumBars — `AudioSpectrum.qml` (ui-island.md §7).
use super::{paint, picture};
use crate::controls::{Control, PointerEvent, Response, Tok};
use hyprv_ui::{animation::Color, geometry::Rect, scene::Node, skia as sk};
use std::time::Duration;

/// Idle-wave timer interval (`AudioSpectrum.qml:62`).
pub const SPECTRUM_TICK: Duration = Duration::from_millis(66);

#[derive(Clone, Debug)]
pub struct SpectrumBars {
    pub frame: Rect,
    pub values: Vec<f32>,
    pub bar_count: usize,
    pub active: bool,
    /// `None` = island accent / textColor / mutedTextColor (the island's bindings).
    pub bar_color: Option<Color>,
    pub peak_color: Option<Color>,
    pub quiet_color: Option<Color>,
    pub minimum_bar_ratio: f32,
    pub amplitude_gain: f32,
    pub height_range_scale: f32,
    pub phase: f32,
    /// Item visibility (the wave only ticks while visible).
    pub visible: bool,
}

impl SpectrumBars {
    /// QML defaults (barCount 14, min 0.12, gain 1.7, range 0.65).
    pub fn new(w: f32, h: f32) -> Self {
        Self {
            frame: Rect::new(0., 0., w, h),
            values: Vec::new(),
            bar_count: 14,
            active: false,
            bar_color: None,
            peak_color: None,
            quiet_color: None,
            minimum_bar_ratio: 0.12,
            amplitude_gain: 1.7,
            height_range_scale: 0.65,
            phase: 0.,
            visible: true,
        }
    }
    /// Island compact instance: 22×22, 6 bars, min 0.18, gain 2.05.
    pub fn island_compact() -> Self {
        Self {
            bar_count: 6,
            minimum_bar_ratio: 0.18,
            amplitude_gain: 2.05,
            ..Self::new(22., 22.)
        }
    }
    /// Island expanded instance: 70×56, 8 bars, min 0.10, gain 1.75.
    pub fn island_expanded() -> Self {
        Self {
            bar_count: 8,
            minimum_bar_ratio: 0.10,
            amplitude_gain: 1.75,
            ..Self::new(70., 56.)
        }
    }
    pub fn has_energy(&self) -> bool {
        self.values.iter().any(|v| *v > 0.)
    }
    /// The host runs a 66 ms timer while this is true (`Timer.running`).
    pub fn wants_tick(&self) -> bool {
        self.visible && (!self.active || !self.has_energy())
    }
    /// One idle-wave timer tick: `phase += 0.28`.
    pub fn tick(&mut self) {
        self.phase += 0.28;
    }
    fn sample(&self, i: usize) -> f32 {
        let min = self.minimum_bar_ratio;
        let ratio = if !self.active || !self.has_energy() {
            min + 0.34 * (((self.phase + i as f32 * 0.72).sin() + 1.) / 2.).powf(1.45)
        } else {
            let len = self.values.len();
            let src = (len - 1).min(i * len / self.bar_count.max(1));
            let raw = self.values[src].max(0.);
            min.max(1f32.min((raw / 100.).min(1.).powf(0.62) * self.amplitude_gain))
        };
        min + (ratio.min(1.) - min) * self.height_range_scale
    }
}

impl Control for SpectrumBars {
    type Action = ();

    fn implicit_size(&self, _: &Tok) -> (f32, f32) {
        (self.frame.width, self.frame.height)
    }

    fn frame(&self) -> Rect {
        self.frame
    }

    fn build(&self, tok: &Tok, _: Duration) -> Node {
        let f = self.frame;
        let bar = self.bar_color.unwrap_or(tok.pal.accent);
        let peak = self.peak_color.unwrap_or(tok.pal.primary_text);
        let quiet = self.quiet_color.unwrap_or(tok.island_muted_text);
        let (w, h) = (f.width, f.height);
        let mut node = picture(f.x, f.y, w, h, |c| {
            let count = self.bar_count.max(1);
            let gap = (w / 36.).clamp(2., 4.);
            let bw = ((w - gap * (count as f32 - 1.)) / count as f32).max(2.);
            let cy = h / 2.;
            let max_h = (h * 0.98).max(2.);
            for i in 0..count {
                let ratio = self.sample(i);
                let bh = (max_h * ratio).max(3.);
                let x = i as f32 * (bw + gap);
                let y = cy - bh / 2.;
                let is_peak = self.active && self.has_energy() && ratio > 0.74;
                let color = if self.active {
                    if is_peak {
                        peak
                    } else {
                        bar
                    }
                } else {
                    quiet
                };
                // roundedRect() with quadratic-curve corners (AudioSpectrum.qml:46-59).
                let r = (bw / 2.).min(bw / 2.).min(bh / 2.).max(0.);
                let mut p = sk::Path::new();
                p.move_to((x + r, y));
                p.line_to((x + bw - r, y));
                p.quad_to((x + bw, y), (x + bw, y + r));
                p.line_to((x + bw, y + bh - r));
                p.quad_to((x + bw, y + bh), (x + bw - r, y + bh));
                p.line_to((x + r, y + bh));
                p.quad_to((x, y + bh), (x, y + bh - r));
                p.line_to((x, y + r));
                p.quad_to((x, y), (x + r, y));
                p.close();
                c.draw_path(&p, &paint(color));
            }
        });
        node.visible = self.visible;
        node
    }

    fn pointer(&mut self, _: PointerEvent, _: Duration) -> Response<()> {
        Response::default()
    }
}
