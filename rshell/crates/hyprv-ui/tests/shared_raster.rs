use hyprv_ui::{
    canvas::CanvasPath,
    geometry::{self, Rect},
    image::{self, ImageSampling},
    render::Renderer,
    scene::{Content, Node, Scene},
    skia as sk,
};
#[test]
fn canvas_arc_elements_and_center_positions_match_qt_runtime() {
    let oracle: serde_json::Value =
        serde_json::from_str(include_str!("data/layout-canvas.json")).unwrap();
    for sample in oracle["arcs"].as_array().unwrap() {
        let args: Vec<_> = sample["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        let mut path = CanvasPath::new();
        path.arc(args[0], args[1], args[2], args[3], args[4], args[5] != 0.);
        let expected = sample["elements"].as_array().unwrap();
        assert_eq!(path.elements().len(), expected.len(), "args={args:?}");
        for ((kind, p), e) in path.elements().iter().zip(expected) {
            assert_eq!(*kind, e[0].as_i64().unwrap() as i32);
            for i in 0..2 {
                assert!(
                    (p[i] - e[i + 1].as_f64().unwrap()).abs() < 1e-9,
                    "args={args:?}, ours={p:?}, Qt={e}"
                );
            }
        }
    }
    for sample in oracle["quadratics"].as_array().unwrap() {
        let a: Vec<_> = sample["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        let mut path = CanvasPath::new();
        path.move_to(a[0], a[1])
            .quadratic_curve_to(a[2], a[3], a[4], a[5]);
        let e = sample["elements"].as_array().unwrap();
        assert_eq!(path.elements().len(), e.len());
        for ((k, p), e) in path.elements().iter().zip(e) {
            assert_eq!(*k, e[0].as_i64().unwrap() as i32);
            for i in 0..2 {
                assert!((p[i] - e[i + 1].as_f64().unwrap()).abs() < 1e-9);
            }
        }
    }
    for sample in oracle["centers"].as_array().unwrap() {
        let p = sample["parent"].as_f64().unwrap() as f32;
        let c = sample["child"].as_f64().unwrap() as f32;
        assert_eq!(
            geometry::centered_position(p, c, true) as f64,
            sample["x"].as_f64().unwrap()
        );
        assert_eq!(
            geometry::centered_position(90.3, 21.3, true) as f64,
            sample["y"].as_f64().unwrap()
        );
    }
}
fn pixels(image: &sk::Image) -> Vec<u8> {
    let mut bytes = vec![0; image.width() as usize * image.height() as usize * 4];
    let info = sk::ImageInfo::new(
        image.dimensions(),
        sk::ColorType::RGBA8888,
        sk::AlphaType::Premul,
        None,
    );
    assert!(image.read_pixels(
        &info,
        &mut bytes,
        image.width() as usize * 4,
        (0, 0),
        sk::image::CachingHint::Allow
    ));
    bytes
}
#[test]
fn sampling_preserves_full_source_and_selects_each_filter() {
    let mut source = vec![];
    for y in 0..8 {
        for x in 0..8 {
            let c = if (x + y) % 2 == 0 { 0 } else { 255 };
            source.extend([c, c, c, 255]);
        }
    }
    let info = sk::ImageInfo::new((8, 8), sk::ColorType::RGBA8888, sk::AlphaType::Premul, None);
    let image = sk::images::raster_from_data(&info, sk::Data::new_copy(&source), 32).unwrap();
    let r = Rect::new(2.1875, 3.1875, 2., 2.);
    let nearest = image::sample(&image, r, ImageSampling::Nearest).unwrap();
    let linear = image::sample(&image, r, ImageSampling::Linear).unwrap();
    let mip = image::sample(&image, r, ImageSampling::Mipmap).unwrap();
    assert_ne!(pixels(&nearest.0), pixels(&linear.0));
    assert_ne!(pixels(&linear.0), pixels(&mip.0));
    assert!(pixels(&mip.0)
        .chunks_exact(4)
        .all(|p| p == [128, 128, 128, 255]));
    let shifted = image::sample(
        &image,
        Rect::new(22.1875, 33.1875, 2., 2.),
        ImageSampling::Mipmap,
    )
    .unwrap();
    assert_eq!(shifted.0.unique_id(), mip.0.unique_id());
    assert_eq!((shifted.1.x - mip.1.x, shifted.1.y - mip.1.y), (20., 30.));
    // The renderer consumes the node setting, rather than pre-resizing the source.
    let mut node = Node::new(Rect::new(0., 0., 2., 2.), Content::Image(image));
    node.image_sampling = ImageSampling::Mipmap;
    let mut surface = Renderer::new()
        .raster(&Scene::new(node), 2., 2., 1.)
        .unwrap();
    assert!(pixels(&surface.image_snapshot())
        .chunks_exact(4)
        .all(|p| p == [128, 128, 128, 255]));
}

#[test]
fn rectangle_mode_overrides_and_effect_cache_follow_renderer_changes() {
    use hyprv_ui::{
        animation::Color,
        qt_rect::RectMode,
        scene::{Fill, Rectangle},
    };
    let mut node = Node::new(
        Rect::new(2.25, 2.25, 22., 14.),
        Content::Rect(Rectangle {
            fill: Fill::Solid(Color::rgba(0.2, 0.4, 0.8, 0.8)),
            radii: [5.; 4],
            border: None,
        }),
    );
    node.effects.multi_effect = Some(Default::default());
    let scene = Scene::new(node);
    let mut renderer = Renderer::new();
    renderer.set_rect_mode(RectMode::Skia);
    let mut old = renderer.raster(&scene, 28., 20., 1.5).unwrap();
    let old = pixels(&old.image_snapshot());
    renderer.set_rect_mode(RectMode::QtQuick);
    let mut next = renderer.raster(&scene, 28., 20., 1.5).unwrap();
    let next = pixels(&next.image_snapshot());
    let mut fresh = Renderer::new().raster(&scene, 28., 20., 1.5).unwrap();
    assert_eq!(next, pixels(&fresh.image_snapshot()));
    assert_ne!(old, next);
    let mut node = Node::new(
        Rect::new(2.25, 2.25, 22., 14.),
        Content::Rect(Rectangle {
            fill: Fill::Solid(Color::rgba(0.2, 0.4, 0.8, 0.8)),
            radii: [5.; 4],
            border: None,
        }),
    );
    node.rect_mode = Some(RectMode::Skia);
    let scene = Scene::new(node);
    let mut a = Renderer::new().raster(&scene, 28., 20., 1.5).unwrap();
    let mut r = Renderer::new();
    r.set_rect_mode(RectMode::Skia);
    let mut b = r.raster(&scene, 28., 20., 1.5).unwrap();
    assert_eq!(pixels(&a.image_snapshot()), pixels(&b.image_snapshot()));
}
