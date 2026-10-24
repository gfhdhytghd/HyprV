//! Render panel states to PNGs: `cargo run --example shots -- <out-dir> [scale]`.
use hyprv_ui::{render::Renderer, scene::Scene};
use hyprv_ui_panels::{control_panel::ControlPanel, fixtures, model::*};
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = std::path::PathBuf::from(args.get(1).cloned().unwrap_or("target-png".into()));
    let scale: f64 = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(2.);
    std::fs::create_dir_all(&out).unwrap();
    for dark in [false, true] {
        let theme = if dark { "dark" } else { "light" };
        for (name, model) in [("control-idle", fixtures::control_idle(dark)), ("control-off", fixtures::control_off(dark))] {
            let mut cp = ControlPanel::new();
            let mut m = model.clone();
            m.output.scale = scale as f32;
            cp.open_for(Anchor { x: 3000., y: 10., width: 28., height: 38. }, "DP-4", Duration::ZERO);
            m.now = Duration::from_millis(1000);
            cp.tick(m.now);
            let root = cp.build(&m);
            let card = root.children[0].frame;
            // crop to the card
            let mut wrap = hyprv_ui::scene::Node::new(hyprv_ui::geometry::Rect::new(0., 0., card.width + 20., card.height + 20.), hyprv_ui::scene::Content::Group);
            let mut r = root;
            r.frame.x = -card.x + 10.;
            r.frame.y = -card.y + 10.;
            wrap.children.push(r);
            let scene = Scene::new(wrap);
            Renderer::new().png(&scene, card.width + 20., card.height + 20., scale, out.join(format!("{name}--{theme}@{scale}.png"))).unwrap();
        }
    }
}
