//! `WAYLAND_DISPLAY=<private> cargo run --example selftest -- <out-dir>`:
//! one translucent rounded rect with a border, rendered on the CPU raster and on the
//! production GPU path; prints both and the max premultiplied channel delta.
use hyprv_ui::{
    geometry::Rect,
    render::Renderer,
    animation::Color,
    scene::{Border, Content, Fill, Node, Rectangle, Scene},
};

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&out).unwrap();
    let mut root = Node::new(Rect::new(0., 0., 160., 60.), Content::Group);
    root.add(Node::new(
        Rect::new(10.3, 10., 120.157, 38.),
        Content::Rect(Rectangle {
            fill: Fill::Solid(Color { r: 231. / 255., g: 231. / 255., b: 236. / 255., a: 0.8 }),
            radii: [19.; 4],
            border: Some(Border { width: 1., color: Color { r: 43. / 255., g: 43. / 255., b: 44. / 255., a: 0.10 } }),
        }),
    ));
    let scene = Scene::new(root);
    let mut renderer = Renderer::new();
    renderer.set_offscreen_backend(hyprv_ui::offscreen::Backend::Cpu);
    for scale in [1.0, 2.0] {
        renderer.png(&scene, 160., 60., scale, out.join(format!("cpu@{scale}.png"))).unwrap();
        let png = hyprv_parity_gpu::GpuTarget::new()
            .and_then(|mut t| t.render_png(&mut renderer, &scene, 160., 60., scale))
            .expect("GPU path");
        std::fs::write(out.join(format!("gpu@{scale}.png")), &png).unwrap();
        println!("scale {scale}: wrote cpu/gpu PNGs to {}", out.display());
    }
}
