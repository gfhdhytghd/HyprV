//! Host-side decoding; service actors transfer only bytes across the UI bridge.
use hyprv_svc_media::tray::TrayItem;
use hyprv_ui::skia as sk;
use std::time::Duration;
#[path = "assets/tray_loader.rs"]
mod tray_loader;
pub use tray_loader::{TrayLoader, TrayReady};
pub enum Ready { Art(Art), Tray(TrayReady) }

pub struct Art {
    pub url: String,
    pub bytes: Vec<u8>,
}
pub async fn art_bytes(url: &str) -> Vec<u8> {
    const LIMIT: usize = 16 * 1024 * 1024;
    let Ok(url) = reqwest::Url::parse(url) else {
        return vec![];
    };
    if url.scheme() == "file" {
        let Ok(path) = url.to_file_path() else {
            return vec![];
        };
        return hyprv_core::tokio::task::spawn_blocking(move ||
            hyprv_widgets::icon_theme::read_bounded_file(&path, LIMIT).unwrap_or_default())
            .await.unwrap_or_default();
    }
    if !matches!(url.scheme(), "http" | "https") {
        return vec![];
    }
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    else {
        return vec![];
    };
    let Ok(response) = client.get(url).send().await else {
        return vec![];
    };
    let Ok(mut response) = response.error_for_status() else {
        return vec![];
    };
    let mut bytes = Vec::new();
    while let Ok(Some(chunk)) = response.chunk().await {
        if bytes.len() + chunk.len() > LIMIT {
            return vec![];
        }
        bytes.extend_from_slice(&chunk);
    }
    bytes
}
const MAX_ICON_BYTES: usize = 16 * 1024 * 1024;
const MAX_PIXELS: usize = 4096 * 4096;
pub fn decode(bytes: &[u8]) -> Option<sk::Image> {
    if bytes.len() > MAX_ICON_BYTES { return None; }
    let image = sk::Image::from_encoded(sk::Data::new_copy(bytes))?;
    valid_dimensions(image.width(), image.height()).then_some(image)
}
fn valid_dimensions(w: i32, h: i32) -> bool {
    w > 0 && h > 0 && w <= 4096 && h <= 4096
        && (w as usize).checked_mul(h as usize).is_some_and(|n| n <= MAX_PIXELS)
}
type TrayCache = std::collections::HashMap<(String, tray_loader::Sizes), (TrayItem, Option<sk::Image>)>;
thread_local! {static TRAY:std::cell::RefCell<TrayCache>=Default::default();}
fn same_tray_icon(a: &TrayItem, b: &TrayItem) -> bool {
    fn parts(item: &TrayItem) -> (&Vec<hyprv_svc_media::tray::TrayPixmap>, &String) {
        let attention = item.status == "NeedsAttention";
        (if attention && !item.attention_pixmaps.is_empty() { &item.attention_pixmaps } else { &item.icon_pixmaps },
         if attention && !item.attention_icon_name.is_empty() { &item.attention_icon_name } else { &item.icon_name })
    }
    let (ap, an) = parts(a); let (bp, bn) = parts(b);
    ap == bp && an == bn && a.icon_theme_path == b.icon_theme_path
}
fn icon_input_bytes(item: &TrayItem) -> Option<usize> {
    if item.key.len() > 4096 || item.status.len() > 64 || item.icon_name.len() > 4096
        || item.attention_icon_name.len() > 4096 || item.icon_theme_path.len() > 4096 { return None; }
    let bytes = item.icon_pixmaps.iter().chain(&item.attention_pixmaps)
        .try_fold(0usize, |n, p| n.checked_add(p.argb.len()))?;
    (bytes <= 64 * 1024 * 1024).then_some(bytes)
}
fn icon_identity(item: &TrayItem) -> TrayItem {
    TrayItem { key: item.key.clone(), status: item.status.clone(),
        icon_name: item.icon_name.clone(), attention_icon_name: item.attention_icon_name.clone(),
        icon_theme_path: item.icon_theme_path.clone(), icon_pixmaps: item.icon_pixmaps.clone(),
        attention_pixmaps: item.attention_pixmaps.clone(), ..Default::default() }
}
// Only isolated fixture replay blocks for completion. File I/O still runs on
// a worker; live Assembly uses TrayLoader's asynchronous byte-only delivery.
pub(crate) fn fixture_tray(item: &TrayItem, scale: f32) -> Option<sk::Image> {
    // IconImage requests 18*DPR, then QIcon::pixmap uses the screen DPR again.
    // Independent Qt 6.12 oracle: tools/qt_tray_probe.cpp.
    let sizes = tray_loader::Sizes::new(scale)?;
    let input_bytes = icon_input_bytes(item)?;
    TRAY.with(|cache| {
        let key = (item.key.clone(), sizes);
        if let Some((old, image)) = cache.borrow().get(&key) {
            if same_tray_icon(old, item) {
                return image.clone();
            }
        }
        let image = std::thread::scope(|scope| scope.spawn(||
            tray_pixmap(item, scale).or_else(|| themed_icon(item, sizes)))
            .join().ok().flatten());
        let mut cache = cache.borrow_mut();
        let image_bytes = image.as_ref().map_or(0, |i| i.width() as usize * i.height() as usize * 4);
        let bytes = input_bytes + image_bytes;
        if bytes > 64 * 1024 * 1024 { return image; }
        let retained: usize = cache.values().map(|(item, image)|
            icon_input_bytes(item).unwrap_or(0) + image.as_ref().map_or(0, |i| i.width() as usize * i.height() as usize * 4)).sum();
        if cache.len() >= 128 || retained + bytes > 64 * 1024 * 1024 { cache.clear(); }
        cache.insert(key, (icon_identity(item), image.clone()));
        image
    })
}
#[cfg(test)]
use fixture_tray as tray;
/// Quickshell's `image://icon/<name>`: `QIcon::fromTheme(name).pixmap(size)` through
/// Qt's QIconLoader (see `hyprv_widgets::icon_theme`), or Quickshell's missing-icon
/// pixmap when the name resolves to nothing. Only an empty name has no image (the bar
/// then draws its letter fallback). QIcon resolves at the screen scale, and returns
/// up to request*scale physical pixels (TrayButton.qml:15-24 uses mipmap sampling).
fn themed_icon(item: &TrayItem, sizes: tray_loader::Sizes) -> Option<sk::Image> {
    let request = sizes.request;
    let physical = request * sizes.theme_scale;
    let name = if item.status == "NeedsAttention" && !item.attention_icon_name.is_empty() {
        &item.attention_icon_name
    } else {
        &item.icon_name
    };
    if name.is_empty() {
        return None;
    }
    let custom = std::path::Path::new(&item.icon_theme_path);
    let custom = (!item.icon_theme_path.is_empty())
        .then(|| {
            [
                custom.join(name),
                custom.join(format!("{name}.png")),
                custom.join(format!("{name}.svg")),
            ]
            .into_iter()
            .find(|p| p.is_file())
        })
        .flatten();
    let direct = std::path::PathBuf::from(name);
    let direct = custom.or_else(|| (direct.is_absolute() && direct.is_file()).then_some(direct));
    let image = if let Some(file) = direct {
        icon_file(&file, physical)
    } else {
        hyprv_widgets::icon_theme::lookup(name, request, sizes.theme_scale)
            .and_then(|entry| icon_file(&entry.file, physical))
    };
    // Quickshell's missingPixmap also uses the device-pixel request itself.
    image.or_else(|| Some(missing_icon(request)))
}
/// One theme-icon file as QIcon renders it at `size` device px: raster files larger
/// than the request are smooth-downscaled (Qt's area averaging, aspect kept), smaller
/// ones are returned as they are (never upscaled); SVGs render at the requested size.
fn icon_file(path: &std::path::Path, size: u32) -> Option<sk::Image> {
    let bytes = hyprv_widgets::icon_theme::read_bounded_file(path, MAX_ICON_BYTES)?;
    if let Some(image) = decode(&bytes) {
        let (w, h) = (image.width() as u32, image.height() as u32);
        let (dw, dh) = hyprv_widgets::icon_theme::adjust_size((size, size), (w, h));
        if (dw, dh) == (w, h) {
            return Some(image);
        }
        let info = sk::ImageInfo::new(
            (w as i32, h as i32),
            sk::ColorType::RGBA8888,
            sk::AlphaType::Premul,
            None,
        );
        let mut src = vec![0u8; (w * h * 4) as usize];
        if !image.read_pixels(
            &info,
            &mut src,
            w as usize * 4,
            (0, 0),
            sk::image::CachingHint::Allow,
        ) {
            return None;
        }
        let out = hyprv_widgets::icon_theme::smooth_scale_down(
            &src,
            w as usize,
            h as usize,
            dw as usize,
            dh as usize,
        );
        let info = sk::ImageInfo::new(
            (dw as i32, dh as i32),
            sk::ColorType::RGBA8888,
            sk::AlphaType::Premul,
            None,
        );
        return sk::images::raster_from_data(&info, sk::Data::new_copy(&out), dw as usize * 4);
    }
    // SVG DOM construction can amplify input; bound it separately.
    (bytes.len() <= 1024 * 1024).then(|| svg_icon(&bytes, size)).flatten()
}
/// `QSvgIconEngine::pixmap`: the document's default size (absolute width/height, else
/// the viewBox) scaled to fit `size` keeping its aspect ratio, rendered into that rect.
/// Skia's DOM would otherwise draw an absolutely sized document unscaled.
fn svg_icon(bytes: &[u8], size: u32) -> Option<sk::Image> {
    if size == 0 || size > 4096 || bytes.len() > 1024 * 1024 { return None; }
    // QtSvg-faithful raster first (Qt's coverage, gradients and opacity semantics).
    if let Some((w, h)) = hyprv_widgets::qt_svg::default_size(bytes) {
        let (pw, ph) = hyprv_widgets::icon_theme::fit_size(
            (size, size),
            (w.round().max(1.) as u32, h.round().max(1.) as u32),
        );
        if let Some(r) = hyprv_widgets::qt_svg::render(bytes, pw as usize, ph as usize) {
            let info = sk::ImageInfo::new(
                (r.w as i32, r.h as i32),
                sk::ColorType::RGBA8888,
                sk::AlphaType::Premul,
                None,
            );
            return sk::images::raster_from_data(&info, sk::Data::new_copy(&r.rgba), r.w * 4);
        }
    }
    let mut dom = hyprv_ui_panels::assets::load_svg(bytes, sk::FontMgr::default())?;
    let square = sk::Size::new(size as f32, size as f32);
    dom.set_container_size(square);
    let natural = dom.root().intrinsic_size();
    let (w, h) = if natural.width > 0. && natural.height > 0. {
        (natural.width, natural.height)
    } else {
        (size as f32, size as f32)
    };
    let (pw, ph) = hyprv_widgets::icon_theme::fit_size(
        (size, size),
        (w.round().max(1.) as u32, h.round().max(1.) as u32),
    );
    dom.set_container_size(sk::Size::new(w, h));
    let mut surface = sk::surfaces::raster_n32_premul((pw as i32, ph as i32))?;
    let canvas = surface.canvas();
    canvas.scale((pw as f32 / w, ph as f32 / h));
    dom.render(canvas);
    Some(surface.image_snapshot())
}
/// Quickshell's missing-icon pixmap (`hyprv_widgets::icon_theme::missing_icon`).
fn missing_icon(size: u32) -> sk::Image {
    let (side, px) = hyprv_widgets::icon_theme::missing_icon(size);
    let info = sk::ImageInfo::new(
        (side as i32, side as i32),
        sk::ColorType::RGBA8888,
        sk::AlphaType::Premul,
        None,
    );
    sk::images::raster_from_data(&info, sk::Data::new_copy(&px), side as usize * 4)
        .expect("missing-icon raster")
}
pub(crate) fn tray_pixmap(item: &TrayItem, scale: f32) -> Option<sk::Image> {
    let target = (18. * scale).ceil() as u32;
    let pixmaps = if item.status == "NeedsAttention" && !item.attention_pixmaps.is_empty() {
        &item.attention_pixmaps
    } else {
        &item.icon_pixmaps
    };
    let p = pixmaps
        .iter()
        .filter(|p| {
            p.width > 0
                && p.height > 0
                && p.width <= 4096
                && p.height <= 4096
                && p.argb.len() == p.width as usize * p.height as usize * 4
        })
        .min_by_key(|p| p.width.abs_diff(target))?;
    let rgba: Vec<u8> = p
        .argb
        .chunks_exact(4)
        .flat_map(|p| [p[1], p[2], p[3], p[0]])
        .collect();
    let info = sk::ImageInfo::new(
        (p.width as i32, p.height as i32),
        sk::ColorType::RGBA8888,
        sk::AlphaType::Unpremul,
        None,
    );
    sk::images::raster_from_data(&info, sk::Data::new_copy(&rgba), p.width as usize * 4)
}
// SHELL-R2-1/R2-2 regressions (size and decode limits, DPR cache key).
#[cfg(test)]
mod regressions;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tray_metadata_keeps_image_identity_but_icon_pixels_invalidate() {
        let mut item = TrayItem { key: "metadata-cache-test".into(),
            icon_pixmaps: vec![hyprv_svc_media::tray::TrayPixmap {
                width: 1, height: 1, argb: vec![255, 1, 2, 3],
            }], ..Default::default() };
        let first = tray(&item, 1.).unwrap();
        item.tooltip_description = "CPU 12%".into(); item.title = "updated".into();
        assert_eq!(tray(&item, 1.).unwrap().unique_id(), first.unique_id());
        item.icon_pixmaps[0].argb[1] = 4;
        assert_ne!(tray(&item, 1.).unwrap().unique_id(), first.unique_id());
    }
    #[test]
    fn tray_argb_is_decoded_without_losing_alpha() {
        let item = TrayItem {
            icon_pixmaps: vec![hyprv_svc_media::tray::TrayPixmap {
                width: 1,
                height: 1,
                argb: vec![128, 200, 100, 50],
            }],
            ..Default::default()
        };
        let image = tray(&item, 1.).unwrap();
        let info = sk::ImageInfo::new(
            (1, 1),
            sk::ColorType::RGBA8888,
            sk::AlphaType::Unpremul,
            None,
        );
        let mut pixels = [0u8; 4];
        assert!(image.read_pixels(&info, &mut pixels, 4, (0, 0), sk::image::CachingHint::Allow));
        assert_eq!(pixels, [200, 100, 50, 128]);
    }
    fn png(path: &std::path::Path, w: i32, h: i32) {
        let mut surface = sk::surfaces::raster_n32_premul((w, h)).unwrap();
        surface
            .canvas()
            .clear(sk::Color::from_argb(255, 10, 200, 30));
        let data = surface
            .image_snapshot()
            .encode(None, sk::EncodedImageFormat::PNG, None)
            .unwrap();
        std::fs::write(path, data.as_bytes()).unwrap();
    }
    fn rgba(image: &sk::Image, x: i32, y: i32) -> [u8; 4] {
        let info = sk::ImageInfo::new((1, 1), sk::ColorType::RGBA8888, sk::AlphaType::Premul, None);
        let mut px = [0u8; 4];
        assert!(image.read_pixels(&info, &mut px, 4, (x, y), sk::image::CachingHint::Allow));
        px
    }
    #[test]
    fn unresolvable_icon_names_draw_quickshells_missing_icon_not_a_letter() {
        let item = TrayItem {
            key: "t".into(),
            icon_name: "hyprv-test-no-such-icon-anywhere".into(),
            ..Default::default()
        };
        let image = tray(&item, 2.).expect("placeholder, not None (letter)");
        assert_eq!((image.width(), image.height()), (36, 36));
        assert_eq!(rgba(&image, 0, 0), [0, 0, 0, 255]);
        assert_eq!(rgba(&image, 35, 0), [0xd9, 0, 0xd8, 255]);
        assert_eq!(rgba(&image, 0, 35), [0xd9, 0, 0xd8, 255]);
        assert_eq!(rgba(&image, 35, 35), [0, 0, 0, 255]);
        // Only an empty name keeps the letter fallback.
        let empty = TrayItem {
            key: "e".into(),
            ..Default::default()
        };
        assert!(tray(&empty, 2.).is_none());
    }
    #[test]
    fn raster_icons_are_downscaled_qt_style_but_never_upscaled() {
        let dir = std::env::temp_dir().join(format!("hyprv-assets-icons-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let big = dir.join("big.png");
        let small = dir.join("small.png");
        png(&big, 256, 128);
        png(&small, 32, 32);
        let big_img = icon_file(&big, 36).unwrap();
        assert_eq!((big_img.width(), big_img.height()), (36, 18));
        assert_eq!(rgba(&big_img, 10, 9), [10, 200, 30, 255]);
        let small_img = icon_file(&small, 36).unwrap();
        assert_eq!((small_img.width(), small_img.height()), (32, 32));
        // Absolute IconName paths load directly.
        let item = TrayItem {
            key: "abs".into(),
            icon_name: big.to_string_lossy().into_owned(),
            ..Default::default()
        };
        assert_eq!(tray(&item, 2.).unwrap().width(), 72, "QIcon pixmap36 at screen DPR2 returns up to72 physical pixels");
        // An absolutely sized SVG without viewBox scales to the request (QSvgIconEngine).
        let svg = dir.join("abs.svg");
        std::fs::write(&svg, r##"<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128"><rect x="96" y="96" width="32" height="32" fill="#ff0000"/></svg>"##).unwrap();
        let img = icon_file(&svg, 36).unwrap();
        assert_eq!((img.width(), img.height()), (36, 36));
        assert_eq!(
            rgba(&img, 33, 33),
            [255, 0, 0, 255],
            "bottom-right square scaled into the 36 px icon"
        );
        assert_eq!(rgba(&img, 20, 20)[3], 0);
        std::fs::remove_dir_all(dir).unwrap();
    }
    /// Diagnostic: `HYPRV_DUMP_ICON=<file>:<size>:<out.png> cargo test -p hyprv-shell --lib
    /// dump_icon_file -- --ignored` writes the pixmap `icon_file` produces (premultiplied
    /// RGBA, PNG-encoded), for comparison with Qt's QIcon pixmaps.
    #[test]
    #[ignore]
    fn dump_icon_file() {
        let Ok(spec) = std::env::var("HYPRV_DUMP_ICON") else { return };
        for one in spec.split(',') {
            let mut parts = one.split(':');
            let (file, size, out) = (parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap());
            let image = icon_file(std::path::Path::new(file), size.parse().unwrap()).unwrap();
            let data = image.encode(None, sk::EncodedImageFormat::PNG, None).unwrap();
            std::fs::write(out, data.as_bytes()).unwrap();
        }
    }
    /// Diagnostic: `HYPRV_DUMP_TRAY=<icon name>:<dpr>:<out.png>` writes `fixture_tray`'s image.
    #[test]
    #[ignore]
    fn dump_fixture_tray() {
        let Ok(spec) = std::env::var("HYPRV_DUMP_TRAY") else { return };
        let mut it = spec.split(':');
        let (name, dpr, out) = (it.next().unwrap(), it.next().unwrap(), it.next().unwrap());
        let item = TrayItem { key: name.into(), id: name.into(), icon: name.into(), icon_name: name.into(), ..Default::default() };
        let image = fixture_tray(&item, dpr.parse().unwrap()).expect("image");
        let data = image.encode(None, sk::EncodedImageFormat::PNG, None).unwrap();
        std::fs::write(out, data.as_bytes()).unwrap();
        println!("{name}: {}x{}", image.width(), image.height());
    }
}
