//! SystemTrendChart's Canvas (engine CanvasBatch / qt_painter) against Qt (qml-ref/canvas/scene.qml rendered by
//! experiments/text/qt-ref at dpr 2 through an unmapped Wayland window).
mod common;
use hyprv_ui::{animation::Color, geometry::Rect, scene::{Content, Node, Scene}};
use hyprv_ui_panels::widgets::trend_canvas;
use std::path::Path;

#[test]
fn trend_canvas_matches_qt() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("qml-ref/canvas");
    let (qw, qh, qt) = common::read_png(&dir.join("trend@2.png")).expect("Qt reference");
    let cases: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("cases.json")).unwrap()).unwrap();
    let mut root = Node::new(Rect::new(0., 0., 600., 350.), Content::Group);
    for c in cases.as_array().unwrap() {
        let f = |k: &str| c[k].as_f64().unwrap() as f32;
        let samples: Vec<f64> = c["samples"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let accent = Color::rgba(0xb9 as f32 / 255., 0x78 as f32 / 255., 0x2f as f32 / 255., 1.);
        let batch = trend_canvas(
            (f("w") as f64, f("h") as f64),
            &samples,
            100.,
            c["auto"].as_bool().unwrap(),
            accent,
            Color::rgba(0.2, 0.2, 0.2, 0.08),
            Color::rgba(0.73, 0.47, 0.18, 0.14),
            Color::rgba(0.73, 0.47, 0.18, 0.02),
        );
        root.children.push(batch.node(Rect::new(f("x"), f("y"), f("w"), f("h"))));
    }
    let (w, h, ours) = common::render(&Scene::new(root), 600., 350., 2.);
    assert_eq!((w, h), (qw, qh));
    common::write_rgba(&common::out_dir().join("trend-ours@2.png"), w, h, &ours);
    let pm = |p: &[u8], c: usize| if c == 3 { p[3] as i32 } else { (p[c] as i32 * p[3] as i32 + 127) / 255 };
    let (mut worst, mut bad) = ((0, 0, 0), 0);
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let d = (0..4).map(|c| (pm(&qt[i..i + 4], c) - pm(&ours[i..i + 4], c)).abs()).max().unwrap();
            if d > 2 {
                bad += 1;
            }
            if d > worst.0 {
                worst = (d, x, y);
            }
        }
    }
    eprintln!("worst {worst:?}, {bad} px > 2");
    assert!(worst.0 <= 2, "worst premultiplied diff {worst:?} ({bad} px > 2)");
}
