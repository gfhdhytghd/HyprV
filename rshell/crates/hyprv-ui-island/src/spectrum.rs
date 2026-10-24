//! AudioSpectrum.qml:31-105: JS double arithmetic and QPainter Canvas fills.
//! Keep island-local until the widget owner can adopt the same Canvas builder.
use hyprv_ui::{canvas::{CanvasBatch, CanvasPaint, CanvasPath}, geometry::Rect, scene::Node};
use hyprv_widgets::controls::Tok;

pub(crate) fn build(frame: Rect, values: &[f32], active: bool, phase: f64, tok: &Tok, expanded: bool) -> Node {
    let (count, minimum, gain) = if expanded { (8, 0.10, 1.75) } else { (6, 0.18, 2.05) };
    let (w, h) = (frame.width as f64, frame.height as f64);
    let energy = values.iter().any(|v| *v > 0.);
    let gap = (w / 36.).clamp(2., 4.);
    let bw = ((w - gap * (count - 1) as f64) / count as f64).max(2.);
    let mut operations = Vec::with_capacity(count);
    for i in 0..count {
        let ratio = if !active || !energy {
            minimum + 0.34 * (((phase + i as f64 * 0.72).sin() + 1.) / 2.).powf(1.45)
        } else {
            let source = (values.len() - 1).min(i * values.len() / count);
            minimum.max(((values[source].max(0.) as f64 / 100.).min(1.).powf(0.62) * gain).min(1.))
        };
        let ratio = minimum + (ratio.min(1.) - minimum) * 0.65;
        let bh = ((h * 0.98).max(2.) * ratio).max(3.);
        let x = i as f64 * (bw + gap);
        let y = h / 2. - bh / 2.;
        let r = (bw / 2.).min(bh / 2.).max(0.);
        let color = if !active { tok.island_muted_text } else if energy && ratio > 0.74 { tok.pal.primary_text } else { tok.pal.accent };
        let mut path = CanvasPath::new();
        path.move_to(x + r, y).line_to(x + bw - r, y)
            .quadratic_curve_to(x + bw, y, x + bw, y + r)
            .line_to(x + bw, y + bh - r)
            .quadratic_curve_to(x + bw, y + bh, x + bw - r, y + bh)
            .line_to(x + r, y + bh)
            .quadratic_curve_to(x, y + bh, x, y + bh - r)
            .line_to(x, y + r)
            .quadratic_curve_to(x, y, x + r, y).close();
        operations.push(CanvasPaint { path, fill: Some(color), gradient: None, stroke: None });
    }
    CanvasBatch { operations }.node(frame)
}
