//! Reused FreeType faces must not retain the preceding glyph's transform/size.
use hyprv_ui::{
    geometry::Rect,
    offscreen::Backend,
    render::Renderer,
    scene::{Content, Node, RenderType, Scene, Text},
    skia as sk,
    text::TextEngine,
};
#[test]
fn mixed_masks_leave_metrics_and_pixels_stable() {
    let texts: Vec<_> = [500, 600, 700]
        .into_iter()
        .flat_map(|weight| {
            [11., 13., 17.].into_iter().map(move |size| Text {
                text: "AV电池 19.7 W".into(),
                size,
                weight,
                render_type: RenderType::NativeRendering,
                families: vec!["JetBrainsMono Nerd Font".into(), "HarmonyOS Sans SC".into()],
                elide: false,
                ..Default::default()
            })
        })
        .collect();
    let expected: Vec<_> = texts
        .iter()
        .map(|t| TextEngine::new().unwrap().measure(t).unwrap())
        .collect();
    let mut renderer = Renderer::new();
    renderer.set_offscreen_backend(Backend::Cpu);
    let mut snapshots = vec![];
    for round in 0..2 {
        for (index, text) in texts.iter().enumerate() {
            let mut node = Node::new(
                Rect::new(0.25, 0.75, 180., 40.),
                Content::Text(text.clone()),
            );
            node.transform = sk::Matrix::scale((1.5, 1.5));
            let image = renderer.image(&Scene::new(node), 300., 80., 1.25).unwrap();
            let info = sk::ImageInfo::new_n32_premul(image.dimensions(), None);
            let mut pixels = vec![0; image.width() as usize * image.height() as usize * 4];
            assert!(image.read_pixels(
                &info,
                &mut pixels,
                image.width() as usize * 4,
                (0, 0),
                sk::image::CachingHint::Disallow
            ));
            if round == 0 {
                snapshots.push(pixels);
            } else {
                assert_eq!(pixels, snapshots[index]);
            }
            for (text, expected) in texts.iter().zip(&expected) {
                // A fresh shaping cache forces metrics to consult the shared FT
                // face after rasterizing masks at another scale/subpixel origin.
                let metrics = TextEngine::new().unwrap().measure(text).unwrap();
                assert_eq!(metrics, *expected);
            }
        }
        renderer.clear_caches();
    }
}
