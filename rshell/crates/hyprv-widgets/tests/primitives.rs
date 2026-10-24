use hyprv_ui::{animation::Color, geometry::Rect, render::Renderer, scene::Scene};
use hyprv_widgets::{containers::*, foundations::*, primitives::*};
use std::time::Duration;
#[test]
fn measured_qt_metrics_and_anchor_rules() {
    for (text, px, width, height) in [
        ("12:34", 16., 47.96875, 22.),
        ("\u{f028}", 16., 18.59375, 22.),
        ("\u{f303}", 17., 17.1875, 23.),
        ("系统资源", 15., 60., 20.),
        ("\u{f00df}", 21.08, 19.59375, 28.),
    ] {
        let l = Label::new(
            text,
            FontSpec {
                pixel_size: px,
                ..Default::default()
            },
            rgb("#ffffff"),
        );
        let m = l.measure();
        assert!((m.width - width).abs() < 0.001, "{text}: {:?}", m);
        assert_eq!(m.height, height);
    }
    assert_eq!(hcenter(29.3), 15.15);
    assert_eq!(hcenter(120.16), 60.08);
}
#[test]
fn qt_colors_and_positioner_visibility() {
    let dark = darker(rgb("#8e90cb"), 1.05);
    assert_eq!((dark.r * 65535.).round() as i32, 34756);
    assert_eq!((dark.g * 65535.).round() as i32, 35246);
    assert_eq!((dark.b * 65535.).round() as i32, 49686);
    assert_eq!(lighter(rgb("#ffffff"), 1.06), rgb("#ffffff"));
    assert_eq!(with_alpha(rgb("#80112233"), 0.25).a, 0.25);
    let items = [
        LayoutItem {
            width: 10.,
            height: 10.,
            visible: true,
        },
        LayoutItem {
            width: 30.,
            height: 0.,
            visible: true,
        },
        LayoutItem {
            width: 10.,
            height: 10.,
            visible: true,
        },
    ];
    assert_eq!(row(&items, 9.5)[2].unwrap().x, 19.5);
}
#[test]
fn reveal_retargets_and_png_export() {
    let output =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/widget-samples");
    std::fs::create_dir_all(&output).unwrap();
    for dark in [false, true] {
        let palette = Palette::new(dark);
        let mut reveal = Reveal::new(100.);
        reveal.prepare_open();
        reveal.play_open(Duration::ZERO);
        reveal.advance(Duration::from_millis(100));
        assert!((reveal.values().height - 87.5).abs() < 0.001);
        let label = Label::new("12:34", FontSpec::default(), palette.primary_text)
            .node(Rect::new(16., 14., 100., 22.));
        let root = glass_panel(
            Rect::new(0., 0., 160., 100.),
            &reveal,
            &GlassStyle {
                fill: palette.glass_fill,
                stroke: palette.glass_stroke,
                ..Default::default()
            },
            vec![label],
        );
        let scene = Scene::new(root);
        let path = output.join(format!(
            "glass-{}@1.25.png",
            if dark { "dark" } else { "light" }
        ));
        Renderer::new()
            .png(&scene, 160., 100., 1.25, &path)
            .unwrap();
        let image = hyprv_ui::skia::Image::from_encoded(hyprv_ui::skia::Data::new_copy(
            &std::fs::read(path).unwrap(),
        ))
        .unwrap();
        assert_eq!((image.width(), image.height()), (200, 125));
        reveal.play_close(Duration::from_millis(100));
        reveal.advance(Duration::from_millis(280));
        assert_eq!(reveal.values().height, 2.);
        assert!(!reveal.is_running());
    }
    let mut section = ExpandableSection::new(0., false);
    section.set_expanded(true, Duration::ZERO);
    assert!(!section.reveal.is_running());
    section.set_full_height(50., Duration::from_secs(1));
    assert!(section.reveal.is_running());
}
#[test]
fn asynchronous_asset_tickets_ignore_stale_results() {
    let mut icon = ImageIcon::default();
    let first = icon.request();
    let second = icon.request();
    let svg=br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#fff"/></svg>"##;
    assert!(!icon.complete(first, svg, true).unwrap());
    assert!(!icon.ready.get());
    assert!(icon.complete(second, svg, true).unwrap());
    assert!(icon.ready.get());
    let node = icon.rounded(Rect::new(0., 0., 32., 32.), 8., Color::default(), None);
    let mut renderer = Renderer::new();
    assert!(renderer.raster(&Scene::new(node), 32., 32., 1.5).is_ok());
}
