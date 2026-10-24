//! `hyprv_ui::image::draw` cost for a static vs size-animating 512² image (docs/verify/ui-perf.md).
use hyprv_ui::{geometry::Rect, image::{draw, ImageSampling}, skia as sk};
use std::time::Instant;
fn main() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../parity/fixtures/assets/cover-warm.png");
    let img = sk::Image::from_encoded(sk::Data::new_copy(&std::fs::read(path).unwrap())).unwrap();
    let mut surf = sk::surfaces::raster_n32_premul((800, 800)).unwrap();
    let c = surf.canvas();
    c.scale((2., 2.));
    for (label, animate) in [("static 64px", false), ("morph 24→64px", true)] {
        let mut total = 0.;
        let n = 36;
        for k in 0..n {
            let size = if animate { 24. + 40. * k as f32 / n as f32 } else { 64. };
            let t = Instant::now();
            draw(c, &img, Rect::new(13.3, 7.7 + k as f32 * if animate { 0.37 } else { 0. }, size, size), ImageSampling::Linear, 1.);
            total += t.elapsed().as_secs_f64();
        }
        println!("{label}: {:.1} µs/frame", total / n as f64 * 1e6);
    }
}
