//! ENG-PQ8 transport contracts. GPU checks require a private Wayland compositor.
use hyprv_ui::{
    animation::Color,
    geometry::Rect,
    offscreen::{self, Backend, GpuTarget},
    render::{RenderError, Renderer},
    scene::{Border, Content, Fill, NativeTextMode, Node, Rectangle, Scene, Text},
    skia as sk,
};
fn scene() -> Scene {
    let mut root = Node::new(Rect::new(0., 0., 180., 90.), Content::Group);
    root.native_materials = Some(true);
    root.add(Node::new(
        Rect::new(2.25, 3.5, 171.5, 78.25),
        Content::Rect(Rectangle {
            fill: Fill::Solid(Color::rgba(0.17, 0.23, 0.31, 0.7)),
            radii: [11.5; 4],
            border: Some(Border {
                width: 1.,
                color: Color::rgba(0.7, 0.6, 0.9, 0.23),
            }),
        }),
    ));
    let mut label = Node::new(
        Rect::new(12., 15., 150., 40.),
        Content::Text(Text {
            text: "Native 电池".into(),
            families: vec!["JetBrainsMono Nerd Font".into(), "HarmonyOS Sans SC".into()],
            size: 17.,
            weight: 600,
            render_type: hyprv_ui::scene::RenderType::NativeRendering,
            color: Color::rgba(0.8, 0.6, 0.9, 0.83),
            ..Default::default()
        }),
    );
    label.native_text_mode = NativeTextMode::QtGl;
    root.add(label);
    Scene::new(root)
}
fn pixels(image: &sk::Image) -> Vec<u8> {
    let info = sk::ImageInfo::new_n32_premul(image.dimensions(), None);
    let mut bytes = vec![0; image.width() as usize * image.height() as usize * 4];
    assert!(image.read_pixels(
        &info,
        &mut bytes,
        image.width() as usize * 4,
        (0, 0),
        sk::image::CachingHint::Disallow
    ));
    bytes
}
#[test]
fn explicit_cpu_transport_needs_no_display_and_rejects_invalid_extents() {
    let mut renderer = Renderer::new();
    renderer.set_offscreen_backend(Backend::Cpu);
    let image = renderer.image(&scene(), 180., 90., 1.5).unwrap();
    assert_eq!(image.dimensions(), sk::ISize::new(270, 135));
    assert!(matches!(
        renderer.raster(&scene(), f32::NAN, 90., 1.),
        Err(RenderError::InvalidSize)
    ));
    renderer.set_offscreen_backend(Backend::Gl);
    assert!(matches!(
        renderer.raster(&scene(), 0., 90., 1.),
        Err(RenderError::InvalidSize)
    ));
}
#[test]
#[ignore = "private Weston + HYPRV_PARITY_BACKEND=gl; production EGL transport"]
fn renderer_entry_points_match_production_target_and_survive_recreation() {
    assert!(offscreen::enabled());
    let scene = scene();
    let mut oracle = GpuTarget::new().unwrap();
    let mut r = Renderer::new();
    for scale in [1., 1.5, 2., 1.] {
        let expected = oracle.render_png(&mut r, &scene, 180., 90., scale).unwrap();
        let image = sk::Image::from_encoded(sk::Data::new_copy(&expected)).unwrap();
        let actual = r.image(&scene, 180., 90., scale).unwrap();
        assert_eq!(pixels(&image), pixels(&actual));
        let actual = r.raster(&scene, 180., 90., scale).unwrap().image_snapshot();
        assert_eq!(pixels(&image), pixels(&actual));
        let path = std::env::temp_dir().join(format!("hyprv-offscreen-{}.png", std::process::id()));
        r.png(&scene, 180., 90., scale, &path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), expected);
        std::fs::remove_file(path).unwrap();
        offscreen::clear_target();
    }
    r.set_offscreen_backend(Backend::Cpu);
    assert_eq!(r.offscreen_backend().unwrap(), Backend::Cpu);
    assert!(
        offscreen::render_png_if_enabled(&mut r, &scene, 180., 90., 1.)
            .unwrap()
            .is_none()
    );
    offscreen::clear_target();
}
