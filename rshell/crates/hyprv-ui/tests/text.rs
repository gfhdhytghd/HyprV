//! Qt 6.12 RHI reference samples supplied in experiments/text (unmapped capture).
use hyprv_ui::{
    animation::Color,
    geometry::Rect,
    render::Renderer,
    scene::{Content, Node, RenderType, Scene, Text},
    skia as sk,
};
use serde_json::Value;
fn alpha(image: &sk::Image) -> Vec<u8> {
    let info = sk::ImageInfo::new(
        (image.width(), image.height()),
        sk::ColorType::RGBA8888,
        sk::AlphaType::Premul,
        None,
    );
    let mut pixels = vec![0; image.width() as usize * image.height() as usize * 4];
    assert!(image.read_pixels(
        &info,
        &mut pixels,
        image.width() as usize * 4,
        (0, 0),
        sk::image::CachingHint::Allow
    ));
    pixels.chunks_exact(4).map(|p| p[3]).collect()
}
#[test]
fn qt_text_reference_regression() {
    let scene: Value = serde_json::from_str(include_str!("text/scene.json")).unwrap();
    for (dpr, dark, png) in [
        (2., false, include_bytes!("text/qt-light@2.png").as_slice()),
        (
            1.5,
            false,
            include_bytes!("text/qt-light@1.5.png").as_slice(),
        ),
        (2., true, include_bytes!("text/qt-dark@2.png").as_slice()),
    ] {
        let reference = sk::Image::from_encoded(sk::Data::new_copy(png)).unwrap();
        let qt = alpha(&reference);
        let mut root = Node::new(Rect::new(0., 0., 320., 250.), Content::Group);
        let mut metrics = Vec::new();
        for s in scene["samples"].as_array().unwrap() {
            let t = Text {
                text: s["text"].as_str().unwrap().into(),
                size: s["px"].as_f64().unwrap() as f32,
                weight: s["weight"].as_i64().unwrap() as i32,
                families: vec!["JetBrainsMono Nerd Font".into(), "HarmonyOS Sans SC".into()],
                letter_spacing: s["letterSpacing"].as_f64().unwrap_or(0.) as f32,
                render_type: if s["mode"] == "native" {
                    RenderType::NativeRendering
                } else {
                    RenderType::QtRendering
                },
                color: if dark {
                    Color::rgba(205. / 255., 214. / 255., 244. / 255., 1.)
                } else {
                    Color::rgba(43. / 255., 43. / 255., 44. / 255., 1.)
                },
                elide: s.get("width").is_some(),
                ..Default::default()
            };
            let m = hyprv_ui::text::measure(&t).unwrap();
            metrics.push(m);
            root.children.push(Node::new(
                Rect::new(
                    s["x"].as_f64().unwrap() as f32,
                    s["y"].as_f64().unwrap() as f32,
                    s["width"].as_f64().unwrap_or(m.implicit_width as f64) as f32,
                    m.implicit_height,
                ),
                Content::Text(t),
            ));
        }
        let mut surface = Renderer::new()
            .raster(&Scene::new(root), 320., 250., dpr)
            .unwrap();
        let snapshot = surface.image_snapshot();
        let ours = alpha(&snapshot);
        let w = reference.width() as usize;
        for (i, s) in scene["samples"].as_array().unwrap().iter().enumerate() {
            let m = metrics[i];
            let x = s["x"].as_f64().unwrap();
            let y = s["y"].as_f64().unwrap();
            let width = s["width"].as_f64().unwrap_or(m.implicit_width as f64);
            let (x0, y0, x1, y1) = (
                ((x - 3.) * dpr).floor().max(0.) as usize,
                ((y - 3.) * dpr).floor().max(0.) as usize,
                (((x + width + 3.) * dpr).ceil() as usize).min(w),
                (((y + m.implicit_height as f64 + 3.) * dpr).ceil() as usize)
                    .min(reference.height() as usize),
            );
            let (mut delta, mut ink, mut max, mut changed) = (0u64, 0u64, 0u8, 0);
            for py in y0..y1 {
                for px in x0..x1 {
                    let d = qt[py * w + px].abs_diff(ours[py * w + px]);
                    delta += d as u64;
                    ink += qt[py * w + px] as u64;
                    max = max.max(d);
                    if d > 0 {
                        changed += 1;
                    }
                }
            }
            eprintln!("{} scale={dpr} dark={dark}: delta={delta}, changed={changed}, max={max}, ink={:.3}%",s["name"],100.*delta as f64/ink.max(1) as f64);
            if s["mode"] == "native" {
                assert_eq!(delta, 0, "native {} dpr={dpr} dark={dark}", s["name"]);
            } else {
                assert!(
                    max <= 1 && (delta as f64 / ink.max(1) as f64) < 0.002,
                    "SDF {} dpr={dpr}",
                    s["name"]
                );
            }
        }
    }
}
