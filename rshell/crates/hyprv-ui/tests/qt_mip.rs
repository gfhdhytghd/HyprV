//! `ImageSampling::Mipmap` against Qt 6.12 (tests/data/qt_mip/scene.qml rendered by
//! experiments/text/qt-ref at dpr 1 and 2): power-of-two (256) and odd-mip (816) sources,
//! fractional positions incl. y + 0.5 device px (edge tie rule) — ENG-IMG-1.
use hyprv_ui::{geometry::Rect, image::ImageSampling, render::Renderer, scene::{Content, Node, Scene}, skia as sk};
use std::path::Path;

fn data(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/qt_mip").join(name)
}
fn image(name: &str) -> sk::Image {
    sk::Image::from_encoded(sk::Data::new_copy(&std::fs::read(data(name)).unwrap())).unwrap()
}
fn premul(img: &sk::Image) -> (usize, Vec<u8>) {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let info = sk::ImageInfo::new((w as i32, h as i32), sk::ColorType::RGBA8888, sk::AlphaType::Premul, None);
    let mut v = vec![0; w * h * 4];
    img.read_pixels(&info, &mut v, w * 4, (0, 0), sk::image::CachingHint::Allow);
    (w, v)
}

#[test]
fn mipmapped_images_match_qt() {
    let (claude, codex) = (image("claude.png"), image("codex.png"));
    let frac = [0., 0.25, 0.5, 0.75, 0., 0.25, 0.5, 0.75];
    let mut items = Vec::new();
    for (i, f) in frac.iter().enumerate() {
        // Item { x: 6 + i·28 + frac; y: 6 (+0.5); 24×24 } → Image margins 3
        items.push((Rect::new(9. + i as f32 * 28. + f, 9. + if i >= 4 { 0.5 } else { 0. }, 18., 18.), claude.clone()));
    }
    for (i, side) in [24f32, 30., 17., 64.].into_iter().enumerate() {
        items.push((Rect::new(9. + i as f32 * 28., 43., side - 6., side - 6.), codex.clone()));
    }
    let mut root = Node::new(Rect::new(0., 0., 240., 120.), Content::Group);
    for (r, img) in &items {
        let mut n = Node::new(*r, Content::Image(img.clone()));
        n.image_sampling = ImageSampling::Mipmap;
        root.children.push(n);
    }
    let scene = Scene::new(root);
    for scale in [1u32, 2] {
        let mut surface = Renderer::new().raster(&scene, 240., 120., scale as f64).unwrap();
        let (w, ours) = premul(&surface.image_snapshot());
        let (qw, qt) = premul(&image(&format!("qt@{scale}.png")));
        assert_eq!(w, qw);
        let worst = ours.iter().zip(&qt).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap();
        assert!(worst <= 2, "scale {scale}: worst premultiplied channel diff {worst}");
    }
}
