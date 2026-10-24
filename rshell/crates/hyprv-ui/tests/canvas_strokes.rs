use hyprv_ui::offscreen;
use hyprv_ui::{
    animation::Color,
    canvas::{CanvasBatch, CanvasPaint, CanvasPath},
    scene::StrokeStyle,
    skia as sk,
};
fn bytes(image: &sk::Image) -> Vec<u8> {
    let info = sk::ImageInfo::new(
        (image.width(), image.height()),
        sk::ColorType::RGBA8888,
        sk::AlphaType::Premul,
        None,
    );
    let mut out = vec![0; image.width() as usize * image.height() as usize * 4];
    assert!(image.read_pixels(
        &info,
        &mut out,
        image.width() as usize * 4,
        (0, 0),
        sk::image::CachingHint::Allow
    ));
    out
}
#[test]
fn qt_stroke_coverage_goldens() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("data/canvas-strokes/cases.json")).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/canvas-strokes");
    let mut failures = Vec::new();
    for case in data["cases"].as_array().unwrap() {
        let mut p = CanvasPath::new();
        let el = case["elements"].as_array().unwrap();
        let mut i = 0;
        while i < el.len() {
            let x = el[i][1].as_f64().unwrap();
            let y = el[i][2].as_f64().unwrap();
            match el[i][0].as_i64().unwrap() {
                0 => {
                    p.move_to(x, y);
                }
                1 => {
                    p.line_to(x, y);
                }
                2 => {
                    p.cubic_to(
                        [x, y],
                        [
                            el[i + 1][1].as_f64().unwrap(),
                            el[i + 1][2].as_f64().unwrap(),
                        ],
                        [
                            el[i + 2][1].as_f64().unwrap(),
                            el[i + 2][2].as_f64().unwrap(),
                        ],
                    );
                    i += 2;
                }
                _ => panic!(),
            }
            i += 1;
        }
        let dpr = case["dpr"].as_f64().unwrap();
        let side = (64. * dpr).round() as u32;
        let paint = CanvasPaint {
            path: p,
            fill: None, gradient: None,
            stroke: Some(StrokeStyle {
                width: case["width"].as_f64().unwrap() as f32,
                color: Color::rgba(0.2, 0.4, 0.8, 0.7),
                cap: match case["cap"].as_u64().unwrap() {
                    0 => sk::paint::Cap::Butt,
                    1 => sk::paint::Cap::Square,
                    _ => sk::paint::Cap::Round,
                },
                join: sk::paint::Join::Round,
            }),
        };
        let batch = if case["batch"] == true {
            let mut track = CanvasPath::new();
            track.arc(29.3, 28.7, 19., 0., std::f64::consts::TAU, false);
            CanvasBatch {
                operations: vec![
                    CanvasPaint {
                        path: track,
                        fill: None, gradient: None,
                        stroke: Some(StrokeStyle {
                            width: 2.25,
                            color: Color::rgba(0.6, 0.3, 0.1, 0.35),
                            cap: sk::paint::Cap::Round,
                            join: sk::paint::Join::Round,
                        }),
                    },
                    paint,
                ],
            }
        } else {
            CanvasBatch {
                operations: vec![paint],
            }
        };
        let actual = if std::env::var("HYPRV_PARITY_BACKEND").as_deref() == Ok("gl") {
            let scene = hyprv_ui::scene::Scene::new(
                batch.node(hyprv_ui::geometry::Rect::new(0., 0., 64., 64.)),
            );
            offscreen::render_image_if_enabled(
                &mut hyprv_ui::render::Renderer::new(),
                &scene,
                64.,
                64.,
                dpr,
            )
            .unwrap()
            .unwrap()
        } else {
            batch.raster(side, side, dpr).unwrap()
        };
        let id = case["id"].as_str().unwrap();
        let reference = sk::Image::from_encoded(sk::Data::new_copy(
            &std::fs::read(root.join(format!("{id}.png"))).unwrap(),
        ))
        .unwrap();
        let a = bytes(&actual);
        let b = bytes(&reference);
        let max = a.iter().zip(&b).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
        if max != 0 {
            failures.push((id, max, a.iter().zip(b).filter(|(a, b)| **a != *b).count()));
        }
    }
    assert!(
        failures.is_empty(),
        "Qt stroke coverage mismatch: {failures:?}"
    );
}
