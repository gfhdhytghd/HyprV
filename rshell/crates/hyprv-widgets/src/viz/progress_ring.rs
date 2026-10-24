//! V2 ProgressRing — island `AgentProgressRing` (`DynamicIsland.qml:495-535`, ui-island.md §3.5).
use crate::controls::{Control, PointerEvent, Response, Tok};
use hyprv_ui::{
    animation::Color,
    canvas::{CanvasBatch, CanvasPaint, CanvasPath},
    geometry::Rect,
    scene::{Node, StrokeStyle},
    skia as sk,
};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct ProgressRing {
    pub frame: Rect,
    /// 0..1 (clamped); jumps, no animation.
    pub value: f64,
    /// `None` = island accentColor (launchColor).
    pub ring_color: Option<Color>,
    /// `None` = island softFill.
    pub track_color: Option<Color>,
}

impl ProgressRing {
    pub fn new(size: f32, value: f64) -> Self {
        Self {
            frame: Rect::new(0., 0., size, size),
            value,
            ring_color: None,
            track_color: None,
        }
    }
}

impl Control for ProgressRing {
    type Action = ();

    fn implicit_size(&self, _: &Tok) -> (f32, f32) {
        (25., 25.)
    }

    fn frame(&self) -> Rect {
        self.frame
    }

    fn build(&self, tok: &Tok, _: Duration) -> Node {
        let f = self.frame;
        let track = self.track_color.unwrap_or(tok.island_soft_fill);
        let ring = self.ring_color.unwrap_or(tok.pal.accent);
        let p = self.value.clamp(0., 1.);
        // Canvas { onPaint: lineWidth 3; lineCap "round"; arc track then progress } —
        // the engine's QQuickContext2D arc + QPainter stroker/gray-raster port, both
        // paints in one Canvas image (per-draw QPainter rounding).
        let size = f.width.min(f.height) as f64;
        let center = size / 2.;
        let r = (center - 3.).max(1.);
        let stroke = |color: Color| StrokeStyle {
            width: 3.,
            color,
            cap: sk::paint::Cap::Round,
            // context2d default lineJoin "miter" (miterLimit 10)
            join: sk::paint::Join::Miter,
        };
        let mut track_path = CanvasPath::new();
        track_path.arc(center, center, r, 0., std::f64::consts::TAU, false);
        // Equal start/end angles are a no-op in QQuickContext2D::arc: no dot at 0.
        let mut ring_path = CanvasPath::new();
        let start = -std::f64::consts::FRAC_PI_2;
        ring_path.arc(center, center, r, start, start + std::f64::consts::TAU * p, false);
        CanvasBatch {
            operations: vec![
                CanvasPaint { path: track_path, fill: None, gradient: None, stroke: Some(stroke(track)) },
                CanvasPaint { path: ring_path, fill: None, gradient: None, stroke: Some(stroke(ring)) },
            ],
        }
        .node(f)
    }

    fn pointer(&mut self, _: PointerEvent, _: Duration) -> Response<()> {
        Response::default()
    }
}
