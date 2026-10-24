//! QuickAdjust show: per-frame damage and top-level node frames (PANELS-PERF-5 probe).
use hyprv_ui::{geometry::Rect, scene::{Node, Scene}};
use hyprv_ui_panels::{fixtures, model::PanelKind, Panels};
use std::time::Duration;
fn dump(n: &Node, depth: usize) {
    if depth > 2 { return; }
    println!("{}frame {:?} eff_outset? multi={} shadow={} children {}", "  ".repeat(depth), n.frame, n.effects.multi_effect.is_some(), n.effects.shadow.is_some(), n.children.len());
    for c in &n.children { dump(c, depth + 1); }
}
fn main() {
    let mut model = fixtures::control_idle(false);
    model.output.scale = 2.;
    let mut p = Panels::default();
    let _ = p.ipc_quick_adjust("showVolume", &model);
    let _ = p.tick(&model);
    let mut scene = Scene::new(p.build(PanelKind::QuickAdjust, &model));
    let _ = scene.take_damage(Rect::new(0., 0., 3072., 1728.));
    dump(scene.root(), 0);
    for k in 1..20u64 {
        model.now = Duration::from_millis(k * 16);
        let _ = p.tick(&model);
        p.update(PanelKind::QuickAdjust, &mut scene, &model);
        if let Some(d) = scene.take_damage(Rect::new(0., 0., 3072., 1728.)) {
            println!("t{:>3}: damage {:?} = {:.0} px²", k * 16, d, d.width * d.height);
        }
    }
}
