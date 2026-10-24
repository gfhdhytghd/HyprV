//! V4 TrendChart — `SystemTrendChart.qml` (ui-panels-stats.md §3).
use super::{paint, picture, stroke_paint};
use crate::controls::{kit, Control, PointerEvent, Response, Tok};
use crate::foundations::{rgb, with_alpha};
use hyprv_ui::{animation::Color, geometry::Rect, scene::Node, skia as sk};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct TrendChart {
    pub frame: Rect,
    pub samples: Vec<f64>,
    pub max_value: f64,
    pub auto_scale: bool,
    pub line_width: f32,
    pub grid_line_count: usize,
    pub surface_opacity: f32,
    /// `None` = systemChartAccent.
    pub accent: Option<Color>,
    /// `active` (repaint gate): when false the last painting is kept by QML; here it only
    /// documents intent — the host simply doesn't rebuild an inactive chart.
    pub active: bool,
}

impl TrendChart {
    pub fn new(w: f32, h: f32) -> Self {
        Self {
            frame: Rect::new(0., 0., w, h),
            samples: Vec::new(),
            max_value: 100.,
            auto_scale: false,
            line_width: 2.25,
            grid_line_count: 4,
            surface_opacity: 0.82,
            accent: None,
            active: true,
        }
    }
    /// `resolvedMaxValue` (`SystemTrendChart.qml:30-40`).
    pub fn resolved_max(&self) -> f64 {
        let mut upper = if self.auto_scale { 0. } else { self.max_value.max(1.) };
        for v in &self.samples {
            upper = upper.max(v.max(0.));
        }
        if self.auto_scale {
            upper *= 1.15;
        }
        upper.max(1.)
    }
}

impl Control for TrendChart {
    type Action = ();

    fn implicit_size(&self, _: &Tok) -> (f32, f32) {
        (280., 116.)
    }

    fn frame(&self) -> Rect {
        self.frame
    }

    fn build(&self, tok: &Tok, _: Duration) -> Node {
        let f = self.frame;
        let d = tok.dark();
        let p = tok.pal.primary_text;
        let accent = self.accent.unwrap_or(tok.pal.chart_accent);
        let frame_c = with_alpha(p, if d { 0.12 } else { 0.10 });
        let grid_c = with_alpha(p, if d { 0.10 } else { 0.08 });
        let panel = with_alpha(rgb("#ffffff"), if d { 0.055 } else { 0.032 });
        let overlay = with_alpha(rgb("#ffffff"), 0.018);
        let area_top = with_alpha(accent, if d { 0.18 } else { 0.14 });
        let area_bottom = with_alpha(accent, 0.02);

        let mut root = kit::group(f);
        // Frame Rectangle (opacity 0.82 applies per node: fill, border, overlay child).
        let mut frame = kit::group(Rect::new(0., 0., f.width, f.height));
        frame.add(kit::qt_rect(
            Rect::new(0., 0., f.width, f.height),
            10.,
            panel,
            Some((1., frame_c)),
        ));
        if d {
            frame.add(kit::qt_rect(Rect::new(0., 0., f.width, f.height), 10., overlay, None));
        }
        root.add(kit::fade(frame, self.surface_opacity));

        // Canvas: anchors.margins 2.
        let (cw, ch) = (f.width - 4., f.height - 4.);
        let max = self.resolved_max();
        let samples = self.samples.clone();
        let (grid_n, lw) = (self.grid_line_count, self.line_width);
        root.add(picture(2., 2., cw, ch, move |c| {
            let (left, top) = (10f32, 10f32);
            let chart_w = (cw - 20.).max(1.);
            let chart_h = (ch - 20.).max(1.);
            let grid = stroke_paint(grid_c, 1., false);
            for i in 0..grid_n {
                let ratio = if grid_n <= 1 {
                    0.
                } else {
                    i as f32 / (grid_n - 1) as f32
                };
                let y = top + chart_h * ratio;
                c.draw_line((left, y), (left + chart_w, y), &grid);
            }
            if samples.is_empty() {
                return;
            }
            let step = if samples.len() > 1 {
                chart_w / (samples.len() - 1) as f32
            } else {
                0.
            };
            let mut pts: Vec<(f32, f32)> = samples
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let s = v.clamp(0., max);
                    (left + step * i as f32, top + chart_h - (s / max) as f32 * chart_h)
                })
                .collect();
            if pts.len() == 1 {
                pts.push((left + chart_w, pts[0].1));
            }
            let bottom = top + chart_h;
            let mut area = sk::Path::new();
            area.move_to((pts[0].0, bottom));
            for &(x, y) in &pts {
                area.line_to((x, y));
            }
            area.line_to((pts[pts.len() - 1].0, bottom));
            area.close();
            let mut fill = sk::Paint::default();
            fill.set_anti_alias(true);
            let col = |c: Color| sk::Color4f::new(c.r, c.g, c.b, c.a);
            fill.set_shader(sk::shader::Shader::linear_gradient(
                ((0., top), (0., bottom)),
                sk::gradient_shader::GradientShaderColors::ColorsInSpace(&[col(area_top), col(area_bottom)], None),
                None,
                sk::TileMode::Clamp,
                None,
                None,
            ));
            c.draw_path(&area, &fill);
            let mut line = sk::Path::new();
            line.move_to(pts[0]);
            for &pt in &pts[1..] {
                line.line_to(pt);
            }
            c.draw_path(&line, &stroke_paint(accent, lw, true));
            let last = pts[pts.len() - 1];
            c.draw_circle(last, 3., &paint(accent));
        }));
        root
    }

    fn pointer(&mut self, _: PointerEvent, _: Duration) -> Response<()> {
        Response::default()
    }
}
