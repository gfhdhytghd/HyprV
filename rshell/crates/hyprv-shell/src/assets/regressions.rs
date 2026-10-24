use super::*;
#[test]
fn oversized_icon_is_rejected_before_decoding() {
    let p = std::env::temp_dir().join(format!("hyprv-oversized-icon-{}", std::process::id()));
    let f = std::fs::File::create(&p).unwrap();
    f.set_len(128 * 1024 * 1024).unwrap();
    drop(f);
    assert!(icon_file(&p, 36).is_none());
    std::fs::remove_file(p).unwrap();
}
#[test]
fn encoded_images_have_a_decoded_dimension_budget() {
    let mut surface = sk::surfaces::raster_n32_premul((8192, 1)).unwrap();
    surface.canvas().clear(sk::Color::RED);
    let data = surface
        .image_snapshot()
        .encode(None, sk::EncodedImageFormat::PNG, None)
        .unwrap();
    assert!(data.size() < MAX_ICON_BYTES);
    assert!(decode(data.as_bytes()).is_none());
    let mut small = sk::surfaces::raster_n32_premul((32, 16)).unwrap();
    small.canvas().clear(sk::Color::RED);
    let data = small
        .image_snapshot()
        .encode(None, sk::EncodedImageFormat::PNG, None)
        .unwrap();
    assert!(decode(data.as_bytes()).is_some());
}
#[test]
fn fixture_cache_includes_qicon_screen_scale_at_fractional_scales() {
    let p = std::env::temp_dir().join(format!(
        "hyprv-icon-dpr-regression-{}.svg",
        std::process::id()
    ));
    std::fs::write(&p,"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"100\"><rect width=\"100\" height=\"100\" fill=\"red\"/></svg>").unwrap();
    let mut item = TrayItem {
        key: "fixture-dpr".into(),
        icon_name: p.display().to_string(),
        ..Default::default()
    };
    assert_eq!(fixture_tray(&item, 1.25).unwrap().width(), 46);
    let cached = fixture_tray(&item, 1.3).unwrap();
    item.key = "fixture-fresh".into();
    let fresh = fixture_tray(&item, 1.3).unwrap();
    assert_eq!(cached.width(), 46);
    assert_eq!(cached.dimensions(), fresh.dimensions());
    std::fs::remove_file(p).unwrap();
}
