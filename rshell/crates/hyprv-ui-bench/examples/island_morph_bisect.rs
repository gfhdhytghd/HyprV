//! Island expand morph paint cost, bisected by content kind; WARM=1 JITTER=7 ART=1 (docs/verify/ui-perf.md).
use hyprv_ui::{geometry::Rect, render::Renderer, scene::{Content, Node, Scene}, skia as sk};
use hyprv_ui_island::{Inputs, Island, Media, Pointer};
use std::time::{Duration, Instant};

fn kind(c: &Content) -> &'static str {
    match c {
        Content::Group => "group",
        Content::Picture(_) => "picture",
        Content::StrokedPath { .. } => "stroked",
        Content::Rect(_) => "rect",
        Content::Text(_) => "text",
        Content::Image(_) => "image",
        Content::Svg(_) => "svg",
        Content::Path { .. } => "path",
        #[allow(unreachable_patterns)]
        _ => "other",
    }
}
fn strip(n: &mut Node, drop: &str, counts: &mut std::collections::BTreeMap<&'static str, usize>) {
    *counts.entry(kind(&n.content)).or_default() += 1;
    if kind(&n.content) == drop {
        n.content = Content::Group;
    }
    for c in &mut n.children {
        strip(c, drop, counts);
    }
}
fn main() {
    let art = std::env::var_os("ART").map(|_| {
        let b = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../parity/fixtures/assets/cover-warm.png")).unwrap();
        sk::Image::from_encoded(sk::Data::new_copy(&b)).unwrap()
    });
    let inputs = Inputs {
        now_hh_mm: "09:41".into(),
        media: Media { available: true, playing: true, title: "Midnight City".into(), artist: "M83".into(), player: "spotify".into(), art_url: "x".into(), position_s: 83., length_s: 225. },
        spectrum: (0..16).map(|i| 30. + i as f32 * 3.).collect(),
        art,
        ..Default::default()
    };
    for drop in ["none", "text", "rect"] { for mode in ["qt", "skia", "static-text"] {
        let mut island = Island::new(inputs.clone(), 1000., Duration::ZERO);
        island.clip_hover_to_input_region = false;
        for k in 0..60 { island.frame(Duration::from_millis(k * 16)); }
        let t0 = 60 * 16;
        let (ox, oy) = island.origin();
        island.pointer(Pointer::Motion { x: ox + 6., y: oy + 19. }, Duration::from_millis(t0));
        let mut r = Renderer::new();
        let warm = std::env::var_os("WARM").is_some();
        if mode == "skia" { r.set_rect_mode(hyprv_ui::qt_rect::RectMode::Skia); }
        let mut surf = sk::surfaces::raster_n32_premul((1000, 900)).unwrap();
        let mut total = 0.;
        let mut counts = Default::default();
        let passes = if warm { 2 } else { 1 };
        for pass in 0..passes {
        if pass == 1 {
            total = 0.;
            island = Island::new(inputs.clone(), 1000., Duration::ZERO);
            island.clip_hover_to_input_region = false;
            for k in 0..60 { island.frame(Duration::from_millis(k * 16)); }
            island.pointer(Pointer::Motion { x: ox + 6., y: oy + 19. }, Duration::from_millis(t0));
        }
        for k in 1..=14u64 {
            let jitter = if pass == 1 { std::env::var("JITTER").ok().and_then(|v| v.parse().ok()).unwrap_or(0) } else { 0 };
            let now = Duration::from_millis(t0 + k * 16 + jitter);
            island.frame(now);
            let mut n = island.build();
            strip(&mut n, drop, &mut counts);
            if mode == "static-text" { freeze_text(&mut n); }
            let scene = Scene::new(n);
            let t = Instant::now();
            r.paint(surf.canvas(), &scene, 2., Rect::new(0., 0., 500., 450.)).unwrap();
            total += t.elapsed().as_secs_f64();
        }
        }
        let _ = &counts;
        println!("drop {drop:6} {mode:12}: {:7.0} µs/frame", total / 14. * 1e6);
    }}
}

/// Text frames rounded to whole logical px and fixed width (separates reshaping from placement).
fn freeze_text(n: &mut Node) {
    if let Content::Text(_) = n.content {
        n.frame.x = n.frame.x.round();
        n.frame.y = n.frame.y.round();
        n.frame.width = 300.;
    }
    for c in &mut n.children { freeze_text(c); }
}
