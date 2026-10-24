//! The panel text path (qtext Picture nodes through hyprv-ui's renderer) must stay
//! pixel-exact against Qt for NativeRendering samples (experiments/text references).
mod common;
use hyprv_ui::{animation::Color, geometry::Rect, scene::{Content, Node, Scene}};
use hyprv_ui_panels::qtext::{self, Font, TextBox};

#[test]
fn native_samples_match_qt_reference() {
    let dir = common::root().join("experiments/text");
    let Some((qw, qh, qt)) = common::read_png(&dir.join("out/qt-light@2.png")) else {
        eprintln!("skip: no Qt reference (run experiments/text/run.sh)");
        return;
    };
    let scene: serde_json_like::Scene = serde_json_like::load(&dir.join("scene.json"));
    let mut root = Node::new(Rect::new(0., 0., qw as f32 / 2., qh as f32 / 2.), Content::Group);
    let mut checked = Vec::new();
    for s in &scene.samples {
        if s.mode != "native" {
            continue;
        }
        let mut font = Font::native(s.px, s.weight).spacing(s.letter_spacing);
        font.render = qtext::Render::Native;
        let mut tb = TextBox::new(s.text.clone(), font, Color::rgba(0x2b as f32 / 255., 0x2b as f32 / 255., 0x2c as f32 / 255., 1.));
        if let Some(w) = s.width {
            tb = tb.width(w).elide();
        }
        let (w, h) = qtext::item_size(&tb);
        root.add(qtext::node(&tb, Rect::new(s.x, s.y, w, h), (s.x, s.y), 2.));
        checked.push((s.name.clone(), s.x, s.y, w, h));
    }
    let (rw, rh, ours) = common::render(&Scene::new(root), qw as f32 / 2., qh as f32 / 2., 2.);
    assert_eq!((rw, rh), (qw, qh));
    for (name, x, y, w, h) in checked {
        let (x0, y0) = (((x - 3.) * 2.).max(0.) as usize, ((y - 3.) * 2.).max(0.) as usize);
        let (x1, y1) = ((((x + w + 3.) * 2.) as usize).min(qw), (((y + h + 3.) * 2.) as usize).min(qh));
        let mut max = 0i32;
        for yy in y0..y1 {
            for xx in x0..x1 {
                let i = (yy * qw + xx) * 4 + 3;
                max = max.max((qt[i] as i32 - ours[i] as i32).abs());
            }
        }
        assert!(max <= 1, "{name}: max alpha delta {max}");
    }
}

/// Minimal reader for experiments/text/scene.json (no serde derive in this crate).
mod serde_json_like {
    pub struct Sample { pub name: String, pub text: String, pub px: f32, pub weight: i32, pub x: f32, pub y: f32, pub mode: String, pub letter_spacing: f32, pub width: Option<f32> }
    pub struct Scene { pub samples: Vec<Sample> }
    pub fn load(p: &std::path::Path) -> Scene {
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        Scene {
            samples: v["samples"].as_array().unwrap().iter().map(|s| Sample {
                name: s["name"].as_str().unwrap().into(),
                text: s["text"].as_str().unwrap().into(),
                px: s["px"].as_f64().unwrap() as f32,
                weight: s["weight"].as_i64().unwrap() as i32,
                x: s["x"].as_f64().unwrap() as f32,
                y: s["y"].as_f64().unwrap() as f32,
                mode: s["mode"].as_str().unwrap().into(),
                letter_spacing: s["letterSpacing"].as_f64().unwrap_or(0.) as f32,
                width: s["width"].as_f64().map(|w| w as f32),
            }).collect(),
        }
    }
}
