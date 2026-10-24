//! Distance-field (Text.QtRendering) text against Qt: experiments/text/df/df-scene.qml
//! rendered by experiments/text/qt-ref at dpr 2 and dpr 1 (QT_SCALE_FACTOR=0.5).
mod common;
use hyprv_ui::{animation::Color, geometry::Rect, scene::{Content, Node, Scene}};
use hyprv_ui_panels::qtext::{self, Font, TextBox};

/// Max alpha diff per sample (GPU bilinear weight rounding leaves ≤ 1).
const MAX_DIFF: i32 = 1;

fn run(dpr: f32, file: &str) -> Vec<(String, i32, f64)> {
    let dir = common::root().join("experiments/text/df");
    let (qw, qh, qt) = common::read_png(&dir.join(file)).unwrap_or_else(|| panic!("missing {file} (render experiments/text/df/df-scene.qml)"));
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("df-scene.json")).unwrap()).unwrap();
    let (lw, lh) = (qw as f32 / dpr, qh as f32 / dpr);
    let mut root = Node::new(Rect::new(0., 0., lw, lh), Content::Group);
    let mut boxes = Vec::new();
    for s in v["samples"].as_array().unwrap() {
        let fam: &'static str = Box::leak(s["family"].as_str().unwrap().to_string().into_boxed_str());
        let font = Font::qt(s["px"].as_f64().unwrap() as f32, s["weight"].as_i64().unwrap() as i32).family(fam).spacing(s["letterSpacing"].as_f64().unwrap() as f32);
        let tb = TextBox::new(s["text"].as_str().unwrap(), font, Color::rgba(0x2b as f32 / 255., 0x2b as f32 / 255., 0x2c as f32 / 255., 1.));
        let (x, y) = (s["x"].as_f64().unwrap() as f32, s["y"].as_f64().unwrap() as f32);
        let (w, h) = qtext::item_size(&tb);
        root.add(qtext::node(&tb, Rect::new(x, y, w, h), (x, y), dpr));
        boxes.push((s["name"].as_str().unwrap().to_string(), x, y, w, h));
    }
    let (rw, rh, ours) = common::render(&Scene::new(root), lw, lh, dpr as f64);
    assert_eq!((rw, rh), (qw, qh));
    common::write_rgba(&common::out_dir().join(format!("df-ours-{file}")), rw, rh, &ours);
    let mut out = Vec::new();
    for (name, x, y, w, h) in boxes {
        let (x0, y0) = (((x - 3.) * dpr).max(0.) as usize, ((y - 3.) * dpr).max(0.) as usize);
        let (x1, y1) = ((((x + w + 3.) * dpr) as usize).min(qw), (((y + h + 3.) * dpr) as usize).min(qh));
        let (mut max, mut sum, mut n) = (0i32, 0f64, 0usize);
        for yy in y0..y1 {
            for xx in x0..x1 {
                let i = (yy * qw + xx) * 4 + 3;
                let d = (qt[i] as i32 - ours[i] as i32).abs();
                max = max.max(d);
                if qt[i] > 0 || ours[i] > 0 {
                    sum += d as f64;
                    n += 1;
                }
            }
        }
        out.push((name, max, sum / n.max(1) as f64));
    }
    out
}

#[test]
fn df_samples_match_qt() {
    let mut fails = Vec::new();
    for (dpr, file) in [(2., "qt@2.png"), (1., "qt@1.png")] {
        for (name, max, mean) in run(dpr, file) {
            eprintln!("@{dpr} {name:12} max {max:3} mean {mean:.3}");
            if max > MAX_DIFF {
                fails.push(format!("@{dpr} {name}: {max}"));
            }
        }
    }
    assert!(fails.is_empty(), "{fails:?}");
}
