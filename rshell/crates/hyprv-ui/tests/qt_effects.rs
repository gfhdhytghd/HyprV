//! MultiEffect colorization and mask against Qt 6.12 (tests/data/qt_effects/scene.qml,
//! rendered at dpr 2 by experiments/text/qt-ref through an unmapped Wayland window).
//! The Image sources are the rasters QQuickImage loads: QtSvg at sourceSize × dpr
//! (light-dark-80.png, baked by the panels' bake tool) and the cover PNG.
use hyprv_ui::{
    animation::Color,
    effects::{Mask, MaskSource, MultiEffect},
    geometry::Rect,
    image::ImageSampling,
    render::Renderer,
    scene::{Content, Node, Scene},
    skia as sk,
};
use std::path::Path;

fn data(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/qt_effects").join(name)
}
fn image(name: &str) -> sk::Image {
    sk::Image::from_encoded(sk::Data::new_copy(&std::fs::read(data(name)).unwrap())).unwrap()
}
fn premul(path_or_px: &[u8], w: usize, x: usize, y: usize) -> [i32; 4] {
    let p = &path_or_px[(y * w + x) * 4..(y * w + x) * 4 + 4];
    let a = p[3] as i32;
    [(p[0] as i32 * a + 127) / 255, (p[1] as i32 * a + 127) / 255, (p[2] as i32 * a + 127) / 255, a]
}
fn pixels(img: &sk::Image) -> (usize, usize, Vec<u8>) {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let info = sk::ImageInfo::new((w as i32, h as i32), sk::ColorType::RGBA8888, sk::AlphaType::Unpremul, None);
    let mut px = vec![0u8; w * h * 4];
    img.read_pixels(&info, &mut px, w * 4, (0, 0), sk::image::CachingHint::Allow);
    (w, h, px)
}
fn effect_item(frame: Rect, effect: MultiEffect, child: Node) -> Node {
    let mut n = Node::new(frame, Content::Group);
    n.effects.multi_effect = Some(effect);
    n.children.push(child);
    n
}
fn icon(x: f32, color: Color, amount: f32) -> Node {
    let mut img = Node::new(Rect::new(0., 0., 20., 20.), Content::Image(image("light-dark-80.png")));
    img.image_sampling = ImageSampling::Mipmap;
    effect_item(Rect::new(x, 10., 20., 20.), MultiEffect { colorization: amount, colorization_color: color, ..Default::default() }, img)
}

#[test]
fn colorization_and_mask_match_qt() {
    let mut root = Node::new(Rect::new(0., 0., 200., 80.), Content::Group);
    root.children.push(icon(10., Color::rgba(1., 1., 1., 1.), 1.));
    root.children.push(icon(40., Color::rgba(0x2b as f32 / 255., 0x2b as f32 / 255., 0x2c as f32 / 255., 1.), 1.));
    root.children.push(icon(70., Color::rgba(0xcc as f32 / 255., 0x33 as f32 / 255., 0x11 as f32 / 255., 1.), 0.5));
    // album art: square cover → PreserveAspectCrop == fill
    let mut art = Node::new(Rect::new(0., 0., 44., 44.), Content::Image(image("cover-warm.png")));
    art.image_sampling = ImageSampling::Mipmap;
    let mask = Mask { source: MaskSource::Rect { radius: 11. }, threshold_min: 0.5, spread_at_min: 1., ..Default::default() };
    root.children.push(effect_item(Rect::new(110., 10., 44., 44.), MultiEffect { mask: Some(mask), ..Default::default() }, art));
    let mut surface = Renderer::new().raster(&Scene::new(root), 200., 80., 2.).unwrap();
    let (w, h, ours) = pixels(&surface.image_snapshot());
    let (qw, qh, qt) = pixels(&image("qt@2.png"));
    assert_eq!((w, h), (qw, qh));
    let mut worst = (0, 0, 0);
    for y in 0..h {
        for x in 0..w {
            let (a, b) = (premul(&qt, w, x, y), premul(&ours, w, x, y));
            let d = (0..4).map(|c| (a[c] - b[c]).abs()).max().unwrap();
            if d > worst.0 {
                worst = (d, x, y);
            }
        }
    }
    eprintln!("qt_effects worst premultiplied diff {worst:?}");
    assert!(worst.0 <= 2, "worst premultiplied diff {worst:?}");
}
