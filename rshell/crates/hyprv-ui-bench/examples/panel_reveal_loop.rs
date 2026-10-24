//! CPU-raster replay of the control panel reveal, repeated (for sampling profilers):
//! `REPEAT=50 cargo run --release --example panel_reveal_loop`.
use hyprv_ui::{geometry::Rect, render::Renderer, scene::Scene, skia as sk};
use hyprv_ui_panels::{fixtures, model::{Anchor, PanelKind}, Panels};
use std::time::{Duration, Instant};

fn main() {
    let repeat: usize = std::env::var("REPEAT").ok().and_then(|v| v.parse().ok()).unwrap_or(20);
    let anchor = Anchor { x: 2600., y: 10., width: 38., height: 38. };
    let (out_w, out_h, scale) = (3072f32, 1728f32, 2f32);
    let mut surface = sk::surfaces::raster_n32_premul(((out_w * scale) as i32, (out_h * scale) as i32)).unwrap();
    let mut renderer = Renderer::new();
    if std::env::var("UI_BENCH_RECT").is_ok_and(|m| m == "skia") {
        renderer.set_rect_mode(hyprv_ui::qt_rect::RectMode::Skia);
    }
    let (mut total, mut frames) = (Duration::ZERO, 0);
    for _ in 0..repeat {
        let mut model = fixtures::control_idle(false);
        model.output.scale = scale;
        let mut p = Panels::default();
        let _ = p.control.toggle_for(anchor, &model.output.name, model.now);
        let _ = p.tick(&model);
        let mut scene = Scene::new(p.build(PanelKind::ControlPanel, &model));
        let _ = scene.take_damage(Rect::new(0., 0., out_w, out_h));
        renderer.paint(surface.canvas(), &scene, scale as f64, Rect::new(0., 0., out_w, out_h)).unwrap();
        for k in 1..30u64 {
            model.now = Duration::from_millis(k * 16);
            let _ = p.tick(&model);
            p.update(PanelKind::ControlPanel, &mut scene, &model);
            if let Some(d) = scene.take_damage(Rect::new(0., 0., out_w, out_h)) {
                let t = Instant::now();
                renderer.paint(surface.canvas(), &scene, scale as f64, d).unwrap();
                total += t.elapsed();
                frames += 1;
            }
        }
    }
    println!("{frames} damaged frames, {:.0} µs/frame CPU paint", total.as_secs_f64() / frames as f64 * 1e6);
}
