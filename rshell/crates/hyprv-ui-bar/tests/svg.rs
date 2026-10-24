use hyprv_ui::skia;
use hyprv_ui_bar::svg::{self, Backend};
fn pixels(image: &skia::Image) -> Vec<u8> {
    let info = skia::ImageInfo::new(
        (image.width(), image.height()),
        skia::ColorType::RGBA8888,
        skia::AlphaType::Premul,
        None,
    );
    let mut pixels = vec![0u8; image.width() as usize * image.height() as usize * 4];
    assert!(image.read_pixels(
        &info,
        &mut pixels,
        image.width() as usize * 4,
        (0, 0),
        skia::image::CachingHint::Allow
    ));
    pixels
}
#[test]
fn dynamic_solid_paths_match_independent_qtsvg_control_goldens() {
    for (source, size, png) in [
        (
            include_bytes!("../assets/control-panel-dark.svg").as_slice(),
            24,
            include_bytes!("data/svg/control-dark-24.png").as_slice(),
        ),
        (
            include_bytes!("../assets/control-panel-dark.svg").as_slice(),
            48,
            include_bytes!("data/svg/control-dark-48.png").as_slice(),
        ),
        (
            include_bytes!("../assets/network-wired-light.svg").as_slice(),
            24,
            include_bytes!("data/svg/wired-24.png").as_slice(),
        ),
        (
            include_bytes!("../assets/network-wired-light.svg").as_slice(),
            48,
            include_bytes!("data/svg/wired-48.png").as_slice(),
        ),
    ] {
        let result = svg::decode(source, size, size).unwrap();
        assert_eq!(result.backend, Backend::GrayOutline);
        let expected = skia::Image::from_encoded(skia::Data::new_copy(png)).unwrap();
        let a = pixels(&result.image);
        let b = pixels(&expected);
        let mut max = 0;
        let mut error = 0usize;
        let mut painted = 0;
        for (a, b) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
            if a[3] != 0 || b[3] != 0 {
                painted += 1;
                for (&a, &b) in a.iter().zip(b) {
                    let d = a.abs_diff(b);
                    max = max.max(d);
                    error += d as usize;
                }
            }
        }
        assert!(max <= 8);
        assert!(error as f64 / (painted * 4) as f64 <= 0.5);
    }
}
#[test]
fn memory_only_decoder_handles_gradients_and_rejects_external_hrefs() {
    let svg=br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><defs><linearGradient id="g"><stop stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff"/></linearGradient></defs><rect width="16" height="16" fill="url(#g)"/></svg>"##;
    let decoded = svg::decode(svg, 16, 16).unwrap();
    assert_eq!(decoded.backend, Backend::Resvg);
    assert!(pixels(&decoded.image).chunks_exact(4).all(|p| p[3] == 255));
    let external=br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><image href="file:///etc/hostname" width="16" height="16"/></svg>"#;
    assert!(pixels(&svg::decode(external, 16, 16).unwrap().image)
        .iter()
        .all(|v| *v == 0));
    assert!(svg::decode(b"not an SVG", 16, 16).is_none());
    assert!(svg::decode(svg, 0, 16).is_none());
    assert!(svg::decode(svg, 10000, 10000).is_none());
}
