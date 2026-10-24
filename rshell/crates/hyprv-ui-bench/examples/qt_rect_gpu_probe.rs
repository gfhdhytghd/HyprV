//! Single-case GPU vs CPU dump for `qt_rect::Cache` (debugging aid).
#[path = "../src/gpu.rs"]
#[allow(dead_code)]
mod gpu;
use hyprv_ui::{animation::Color, geometry::Rect, qt_rect::Cache, scene::{Fill, Rectangle}, skia as sk};
fn read(surface: &mut sk::Surface) -> Vec<u8> {
    let info = sk::ImageInfo::new(surface.image_info().dimensions(), sk::ColorType::RGBA8888, sk::AlphaType::Premul, None);
    let mut out = vec![0u8; (info.width() * info.height() * 4) as usize];
    surface.read_pixels(&info, &mut out, info.width() as usize * 4, (0, 0));
    out
}
fn main() {
    let mut g = gpu::Gpu::new().unwrap();
    let args: Vec<f32> = std::env::args().skip(1).map(|a| a.parse().unwrap()).collect();
    let (radius, opacity, alpha) = (args.first().copied().unwrap_or(4.), args.get(1).copied().unwrap_or(1.), args.get(2).copied().unwrap_or(1.));
    let rect = Rectangle { fill: Fill::Solid(Color { r: 0.1, g: 0.4, b: 0.9, a: alpha }), radii: [radius; 4], border: None };
    let mut gs = g.render_target(24, 12).unwrap();
    gs.canvas().clear(sk::Color::TRANSPARENT);
    gs.canvas().translate((2., 2.));
    Cache::default().draw(gs.canvas(), Rect::new(0., 0., 18., 8.), &rect, opacity, radius > 0.);
    g.skia.flush_and_submit();
    let mut cs = sk::surfaces::raster_n32_premul((24, 12)).unwrap();
    cs.canvas().translate((2., 2.));
    Cache::default().draw(cs.canvas(), Rect::new(0., 0., 18., 8.), &rect, opacity, radius > 0.);
    let (a, b) = (read(&mut gs), read(&mut cs));
    for y in [2usize, 3, 6] {
        println!("row {y}");
        for x in 0..12 {
            let i = (y * 24 + x) * 4;
            println!("  x{x}: gpu {:?} cpu {:?}", &a[i..i + 4], &b[i..i + 4]);
        }
    }
}
