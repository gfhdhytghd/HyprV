//! Offline-baked QtSvg rasters (tools/bake.py, the bar's svg_bake.cpp): the textures
//! QQuickImage loads for shell SVGs (`sourceSize × devicePixelRatio`), so icons match
//! QtSvg without Qt at runtime. Sampling (mipmaps, trilinear) and MultiEffects are the
//! engine's (`Content::Image` + `Node.image_sampling`, `hyprv_ui::effects::MultiEffect`).
use hyprv_ui::skia as sk;
use std::{cell::RefCell, collections::HashMap};

include!("baked_data.rs");

thread_local! {
    static DECODED: RefCell<HashMap<(String, u32, u32), Option<sk::Image>>> = RefCell::new(HashMap::new());
}

/// The baked texture for (icon key, sourceSize in logical px, scale), decoded once.
pub fn raster(icon: &str, source_size: f32, scale: f32) -> Option<sk::Image> {
    let key = (icon.to_string(), source_size.round() as u32, (scale * 1000.).round() as u32);
    if let Some(hit) = DECODED.with(|c| c.borrow().get(&key).cloned()) {
        return hit;
    }
    let img = bytes(icon, key.1, scale, "source").and_then(|b| sk::Image::from_encoded(sk::Data::new_copy(b)));
    DECODED.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 256 {
            c.clear();
        }
        c.insert(key, img.clone());
    });
    img
}

/// Whether a baked raster exists for (icon, sourceSize, scale).
pub fn has(icon: &str, source_size: f32, scale: f32) -> bool {
    bytes(icon, source_size.round() as u32, scale, "source").is_some()
}
