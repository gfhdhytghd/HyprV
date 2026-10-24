//! Paint cost of an animating rounded rect: QtQuick (CPU raster) vs Skia rect mode (docs/verify/ui-perf.md).
use hyprv_ui::{geometry::Rect, render::Renderer, scene::{Node, Scene, Content}, skia as sk};
use hyprv_ui_island::util::qt_rect;
use hyprv_ui::animation::Color;
use std::time::Instant;
fn main() {
    for mode in ["qt", "skia"] {
        let mut r = Renderer::new();
        if mode == "skia" { r.set_rect_mode(hyprv_ui::qt_rect::RectMode::Skia); }
        let mut surf = sk::surfaces::raster_n32_premul((2400, 1700)).unwrap();
        for (label, w0, h0, w1, h1) in [("calendar card 200x38→1000x720", 200., 38., 1000., 720.), ("pill 120x38→420x200", 120., 38., 420., 200.)] {
            let mut total = 0.;
            let n = 14;
            for k in 0..n {
                let t = k as f32 / n as f32 + 0.013;
                let (w, h) = (w0 + (w1 - w0) * t, h0 + (h1 - h0) * t);
                let mut root = Node::new(Rect::new(0., 0., 1200., 850.), Content::Group);
                root.add(qt_rect(Rect::new(10.3, 10., w, h), 24. + 6. * t, Color { r: 0.95, g: 0.95, b: 0.97, a: 0.92 }, Some((1., Color { r: 0., g: 0., b: 0., a: 0.09 }))));
                let scene = Scene::new(root);
                let t0 = Instant::now();
                r.paint(surf.canvas(), &scene, 2., Rect::new(0., 0., 1200., 850.)).unwrap();
                total += t0.elapsed().as_secs_f64();
            }
            println!("{mode:5} {label}: {:7.0} µs/frame", total / n as f64 * 1e6);
        }
    }
}
