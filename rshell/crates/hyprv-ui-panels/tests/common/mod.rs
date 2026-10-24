#![allow(dead_code)]
//! Shared helpers for offscreen PNG tests.
use hyprv_ui::{render::Renderer, scene::Scene, skia as sk};
use std::path::{Path, PathBuf};

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

/// RGBA8888 unpremultiplied pixels of a PNG.
pub fn read_png(path: &Path) -> Option<(usize, usize, Vec<u8>)> {
    let data = sk::Data::new_copy(&std::fs::read(path).ok()?);
    let img = sk::Image::from_encoded(data)?;
    let info = sk::ImageInfo::new((img.width(), img.height()), sk::ColorType::RGBA8888, sk::AlphaType::Unpremul, None);
    let mut px = vec![0u8; (img.width() * img.height() * 4) as usize];
    img.read_pixels(&info, &mut px, img.width() as usize * 4, (0, 0), sk::image::CachingHint::Allow);
    Some((img.width() as usize, img.height() as usize, px))
}

/// Render a scene at `scale` and return premultiplied→unpremultiplied RGBA pixels.
pub fn render(scene: &Scene, w: f32, h: f32, scale: f64) -> (usize, usize, Vec<u8>) {
    let mut r = Renderer::new();
    let mut surface = r.raster(scene, w, h, scale).expect("raster");
    let img = surface.image_snapshot();
    let info = sk::ImageInfo::new((img.width(), img.height()), sk::ColorType::RGBA8888, sk::AlphaType::Unpremul, None);
    let mut px = vec![0u8; (img.width() * img.height() * 4) as usize];
    img.read_pixels(&info, &mut px, img.width() as usize * 4, (0, 0), sk::image::CachingHint::Allow);
    (img.width() as usize, img.height() as usize, px)
}

pub fn save_png(scene: &Scene, w: f32, h: f32, scale: f64, path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    Renderer::new().png(scene, w, h, scale, path).expect("png");
}

pub fn out_dir() -> PathBuf {
    let d = Path::new(env!("CARGO_MANIFEST_DIR")).join("target-png");
    std::fs::create_dir_all(&d).unwrap();
    d
}

pub fn write_rgba(path: &Path, w: usize, h: usize, px: &[u8]) {
    let info = sk::ImageInfo::new((w as i32, h as i32), sk::ColorType::RGBA8888, sk::AlphaType::Unpremul, None);
    let img = sk::images::raster_from_data(&info, sk::Data::new_copy(px), w * 4).unwrap();
    let png = img.encode(None, sk::EncodedImageFormat::PNG, None).unwrap();
    std::fs::write(path, png.as_bytes()).unwrap();
}
