//! Immutable offline QtSvg cache. No Qt linkage at runtime.
include!("baked_data.rs");
/// Decode immutable offline-baked pixels; the host may cache the returned image.
pub fn image(icon: &str, size: u32, scale: f32, color: &str) -> Option<hyprv_ui::skia::Image> {
    use hyprv_ui::skia;
    thread_local! {static CACHE:std::cell::RefCell<std::collections::BTreeMap<(String,u32,u32,String),skia::Image>>=Default::default();}
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let key = (
            icon.to_string(),
            size,
            (scale * 1000.).round() as u32,
            color.to_string(),
        );
        if let Some(image) = cache.get(&key) {
            return Some(image.clone());
        }
        let image =
            skia::Image::from_encoded(skia::Data::new_copy(bytes(icon, size, scale, color)?))?;
        cache.insert(key, image.clone());
        Some(image)
    })
}
