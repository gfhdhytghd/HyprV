use hyprv_ui::skia as sk;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let image =
        sk::Image::from_encoded(sk::Data::new_copy(&std::fs::read(&args[1]).unwrap())).unwrap();
    let info = sk::ImageInfo::new(
        image.dimensions(),
        sk::ColorType::RGBA8888,
        sk::AlphaType::Premul,
        None,
    );
    let mut bytes = vec![0; image.width() as usize * image.height() as usize * 4];
    assert!(image.read_pixels(
        &info,
        &mut bytes,
        image.width() as usize * 4,
        (0, 0),
        sk::image::CachingHint::Allow
    ));
    std::fs::write(&args[2], bytes).unwrap();
}
