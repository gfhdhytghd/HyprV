//! Qt 6.12 RHI golden: explicit layer.textureSize survives MultiEffect padding.
//! Diagnostic QML/provenance: fixtures/effect-texture/README.md.
use hyprv_ui::{
    animation::Color,
    effects::MultiEffect,
    geometry::Rect,
    render::{RenderError, Renderer},
    scene::{Border, Content, Fill, Node, Rectangle, Scene},
    skia as sk,
};
fn scene(size: Option<(u32, u32)>) -> Scene {
    let mut root = Node::new(Rect::new(0., 0., 428., 358.), Content::Group);
    let mut layer = Node::new(Rect::new(64., 64., 300., 230.), Content::Group);
    layer.effects.multi_effect = Some(MultiEffect {
        source_texture_size: size,
        shadow_enabled: true,
        shadow_blur: 10. / 64.,
        shadow_color: Color::rgba(0., 0., 0., 0.45),
        ..Default::default()
    });
    let mut rect = Node::new(
        Rect::new(0., 0., 300., 230.),
        Content::Rect(Rectangle {
            fill: Fill::Solid(Color::rgba(16. / 255., 18. / 255., 20. / 255., 0.42)),
            radii: [19.; 4],
            border: Some(Border {
                width: 1.,
                color: Color::rgba(205. / 255., 214. / 255., 244. / 255., 0.14),
            }),
        }),
    );
    rect.opacity = 0.82;
    layer.add(rect);
    root.add(layer);
    Scene::new(root)
}
fn pixels(image: &sk::Image) -> Vec<u8> {
    let info = sk::ImageInfo::new(
        image.dimensions(),
        sk::ColorType::RGBA8888,
        sk::AlphaType::Premul,
        None,
    );
    let mut bytes = vec![0u8; image.width() as usize * image.height() as usize * 4];
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
fn explicit_source_texture_matches_independent_qt_rhi_at_both_dprs() {
    for (scale, bytes) in [
        (
            1.,
            include_bytes!("fixtures/effect-texture/qt@1.png").as_slice(),
        ),
        (
            2.,
            include_bytes!("fixtures/effect-texture/qt@2.png").as_slice(),
        ),
    ] {
        let reference = sk::Image::from_encoded(sk::Data::new_copy(bytes)).unwrap();
        let expected = pixels(&reference);
        let mut output = Renderer::new()
            .raster(
                &scene(Some(((300. * scale) as u32, (230. * scale) as u32))),
                428.,
                358.,
                scale,
            )
            .unwrap();
        let actual = pixels(&output.image_snapshot());
        let mut max = 0u8;
        let mut error = 0u64;
        let mut painted = 0;
        for (a, b) in actual.chunks_exact(4).zip(expected.chunks_exact(4)) {
            if a[3] > 0 || b[3] > 0 {
                painted += 1;
                for (i, j) in a.iter().zip(b) {
                    let d = i.abs_diff(*j);
                    max = max.max(d);
                    error += d as u64;
                }
            }
        }
        assert!(max <= 1, "DPR{scale}: max{max}");
        assert!(
            (error as f64 / (painted * 4) as f64) < 0.01,
            "DPR{scale}: mean {}",
            error as f64 / (painted * 4) as f64
        );
    }
}
#[test]
fn explicit_source_dimensions_are_validated_before_allocation() {
    for size in [
        (0, 230),
        (300, 0),
        (32769, 230),
        (u32::MAX, 1),
        (32768, 32768),
    ] {
        assert!(
            matches!(
                Renderer::new().raster(&scene(Some(size)), 428., 358., 1.),
                Err(RenderError::InvalidSize)
            ),
            "{size:?}"
        );
    }
}
#[test]
fn source_texture_edits_invalidate_cached_effect_without_changing_output_extent() {
    let mut scene = scene(None);
    let id = scene.root().children[0].id();
    let mut r = Renderer::new();
    let mut before = r.raster(&scene, 428., 358., 1.).unwrap();
    let before = pixels(&before.image_snapshot());
    scene.edit(id, |n| {
        n.effects.multi_effect.as_mut().unwrap().source_texture_size = Some((300, 230))
    });
    let mut after = r.raster(&scene, 428., 358., 1.).unwrap();
    assert_eq!((after.width(), after.height()), (428, 358));
    assert_ne!(before, pixels(&after.image_snapshot()));
    scene.edit(id, |n| {
        n.effects.multi_effect.as_mut().unwrap().source_texture_size = None
    });
    let mut restored = r.raster(&scene, 428., 358., 1.).unwrap();
    assert_eq!(before, pixels(&restored.image_snapshot()));
}
