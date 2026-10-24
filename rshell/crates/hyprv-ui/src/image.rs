//! Qt Quick Image sampling in premultiplied RGBA at device pixel centres.
//! Promoted from the bar's independently validated GL sampler. No file I/O.
use crate::{geometry::Rect, skia};
use std::{cell::RefCell, collections::BTreeMap};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ImageSampling {
    Nearest,
    #[default]
    Linear,
    Mipmap,
    /// Four vertically packed, square A8 Canvas images. Match Qt's padded
    /// screen-sized atlas coordinates; `corner` selects TL, TR, BL, BR.
    QtCanvasAtlas { size: (u32, u32), corner: u8 },
}
impl ImageSampling {
    pub fn qt(smooth: bool, mipmap: bool) -> Self {
        if !smooth {
            Self::Nearest
        } else if mipmap {
            Self::Mipmap
        } else {
            Self::Linear
        }
    }
    pub fn skia(self) -> skia::SamplingOptions {
        match self {
            Self::Nearest => {
                skia::SamplingOptions::new(skia::FilterMode::Nearest, skia::MipmapMode::None)
            }
            Self::Linear | Self::QtCanvasAtlas { .. } => {
                skia::SamplingOptions::new(skia::FilterMode::Linear, skia::MipmapMode::None)
            }
            Self::Mipmap => {
                skia::SamplingOptions::new(skia::FilterMode::Linear, skia::MipmapMode::Linear)
            }
        }
    }
}
// Image source mips, sampled F32 results and affine mips share one of the
// renderer's four raster-budget shares. Charge F32 samples at 16 bytes/pixel.
const CACHE_SHARE: usize = crate::render::RASTER_CACHE_SHARE / 3;
type Key = (u32, ImageSampling, [u32; 6]);
#[derive(Default)]
struct Cache {
    entries: BTreeMap<Key, (skia::Image, Rect, u64)>,
    clock: u64,
}
thread_local! {static CACHE:RefCell<Cache>=RefCell::new(Cache::default());}
/// Draws the exact device-centre sampler for positive axis-aligned transforms.
/// Returns false for affine/perspective transforms; the renderer uses Skia's GL
/// sampler with an explicitly generated mip chain for that case.
pub fn draw(
    canvas: &skia::Canvas,
    image: &skia::Image,
    bounds: Rect,
    sampling: ImageSampling,
    opacity: f32,
) -> bool {
    let matrix = canvas.local_to_device_as_3x3();
    if matrix.has_perspective()
        || matrix.skew_x() != 0.
        || matrix.skew_y() != 0.
        || matrix.scale_x() <= 0.
        || matrix.scale_y() <= 0.
    {
        return false;
    }
    let r = Rect::new(
        bounds.x * matrix.scale_x() + matrix.translate_x(),
        bounds.y * matrix.scale_y() + matrix.translate_y(),
        bounds.width * matrix.scale_x(),
        bounds.height * matrix.scale_y(),
    );
    let Some((image, rect)) = sample(image, r, sampling) else {
        return false;
    };
    let _guard = skia::AutoCanvasRestore::guard(canvas, true);
    canvas.reset_matrix();
    let mut paint = skia::Paint::default();
    paint.set_alpha_f(opacity);
    canvas.draw_image(image, (rect.x, rect.y), Some(&crate::render::exact_alpha(paint)));
    true
}
pub fn sample(
    image: &skia::Image,
    r: Rect,
    sampling: ImageSampling,
) -> Option<(skia::Image, Rect)> {
    if let ImageSampling::QtCanvasAtlas {corner,..}=sampling {
        let side=image.width();
        if corner>=4 || image.height()!=side*4 {return None;}
        let part=image.make_subset(None,skia::IRect::from_xywh(0,side*corner as i32,side,side),Default::default())?;
        return sample(&part,r,ImageSampling::Linear);
    }
    if !r.x.is_finite() || !r.y.is_finite() {
        return None;
    }
    let (ox, oy) = (r.x.floor(), r.y.floor());
    let phase = Rect::new(r.x - ox, r.y - oy, r.width, r.height);
    let key = (
        image.unique_id(),
        sampling,
        [
            phase.x.to_bits(),
            phase.y.to_bits(),
            phase.width.to_bits(),
            phase.height.to_bits(),
            image.width() as u32,
            image.height() as u32,
        ],
    );
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.clock = cache.clock.wrapping_add(1);
        let used = cache.clock;
        let (image, bounds) = if let Some((image, bounds, last)) = cache.entries.get_mut(&key) {
            *last = used;
            (image.clone(), *bounds)
        } else {
            let result = sample_uncached(image, phase, sampling)?;
            while cache.entries.len() >= 512
                || (!cache.entries.is_empty()
                    && cache
                        .entries
                        .values()
                        .map(|v| v.0.width() as usize * v.0.height() as usize * 16)
                        .sum::<usize>()
                        + result.0.width() as usize * result.0.height() as usize * 16
                        > CACHE_SHARE)
            {
                if let Some(oldest) = cache
                    .entries
                    .iter()
                    .min_by_key(|(_, v)| v.2)
                    .map(|(k, _)| *k)
                {
                    cache.entries.remove(&oldest);
                }
            }
            if result.0.width() as usize * result.0.height() as usize * 16 <= CACHE_SHARE {
                cache.entries.insert(key, (result.0.clone(), result.1, used));
            }
            result
        };
        Some((
            image,
            Rect::new(bounds.x + ox, bounds.y + oy, bounds.width, bounds.height),
        ))
    })
}
fn sample_uncached(
    image: &skia::Image,
    r: Rect,
    sampling: ImageSampling,
) -> Option<(skia::Image, Rect)> {
    if r.width <= 0. || r.height <= 0. || !r.x.is_finite() || !r.y.is_finite() {
        return None;
    }
    // Pixel centres exactly on an edge follow GL's top-left rule in Qt's y-up window
    // space (qt_rect::TIE_RULE): left and bottom edges are inside, right and top are not
    // (ENG-IMG-1: a quad at y + 0.5 device px).
    let (x0, y0) = ((r.x - 0.5).ceil() as i32, (r.y - 0.5).floor() as i32 + 1);
    let (x1, y1) = (
        ((r.x + r.width) - 0.5).ceil() as i32,
        ((r.y + r.height) - 0.5).floor() as i32 + 1,
    );
    let (w, h) = (x1.checked_sub(x0)?, y1.checked_sub(y0)?);
    if w <= 0 || h <= 0 || w > 4096 || h > 4096 {
        return None;
    }
    let (sw, sh) = (image.width(), image.height());
    if sw <= 0 || sh <= 0 || sw > 4096 || sh > 4096 {
        return None;
    }
    let info = |w, h| {
        skia::ImageInfo::new(
            (w, h),
            skia::ColorType::RGBA8888,
            skia::AlphaType::Premul,
            None,
        )
    };
    let levels = source_levels(image, sampling == ImageSampling::Mipmap)?;
    let lod = if sampling == ImageSampling::Mipmap {
        ((sw as f32 / r.width).max(sh as f32 / r.height))
            .log2()
            .max(0.)
    } else {
        0.
    };
    let lower = lod.floor() as usize;
    let blend = lod - lower as f32;
    let lower = lower.min(levels.len() - 1);
    let upper = (lower + 1).min(levels.len() - 1);
    let mut pixels = vec![0f32; w as usize * h as usize * 4];
    // Per level, the sample coordinates of every column and row (same expressions as
    // per pixel, hoisted out of the channel loop: identical output, PERF-UI-1).
    struct Axis {
        i: Vec<i32>,
        f: Vec<f32>,
        nearest: Vec<i32>,
    }
    let axis = |n: i32, origin: i32, r0: f32, len: f32, size: i32| {
        let mut a = Axis { i: Vec::with_capacity(n as usize), f: Vec::with_capacity(n as usize), nearest: Vec::with_capacity(n as usize) };
        for d in 0..n {
            let t = ((origin + d) as f32 + 0.5 - r0) / len * size as f32 - 0.5;
            a.i.push(t.floor() as i32);
            a.f.push(t - t.floor());
            a.nearest.push((t + 0.5).floor() as i32);
        }
        a
    };
    let axes: Vec<(Axis, Axis)> = [lower, upper]
        .iter()
        .map(|&l| {
            let (_, sw, sh) = &levels[l];
            (axis(w, x0, r.x, r.width, *sw), axis(h, y0, r.y, r.height, *sh))
        })
        .collect();
    let read = |k: usize, dx: usize, dy: usize| -> [f32; 4] {
        let (source, sw, sh) = &levels[if k == 0 { lower } else { upper }];
        let (sw, sh) = (*sw, *sh);
        let (ax, ay) = &axes[k];
        let at = |x: i32, y: i32| ((y.clamp(0, sh - 1) * sw + x.clamp(0, sw - 1)) * 4) as usize;
        if sampling == ImageSampling::Nearest {
            let i = at(ax.nearest[dx], ay.nearest[dy]);
            return std::array::from_fn(|c| source[i + c] as f32);
        }
        let (ix, iy, fx, fy) = (ax.i[dx], ay.i[dy], ax.f[dx], ay.f[dy]);
        let (p00, p10, p01, p11) = (at(ix, iy), at(ix + 1, iy), at(ix, iy + 1), at(ix + 1, iy + 1));
        std::array::from_fn(|c| {
            let get = |i: usize| source[i + c] as f32;
            let top = get(p00) * (1. - fx) + get(p10) * fx;
            let bottom = get(p01) * (1. - fx) + get(p11) * fx;
            top * (1. - fy) + bottom * fy
        })
    };
    for dy in 0..h as usize {
        for dx in 0..w as usize {
            let (lo, hi) = (read(0, dx, dy), read(1, dx, dy));
            let o = (dy * w as usize + dx) * 4;
            for c in 0..4 {
                // unrounded, as a GPU texture sample reaches the blender (ENG-BLEND-1)
                pixels[o + c] = ((lo[c] * (1. - blend) + hi[c] * blend) / 255.).clamp(0., 1.);
            }
        }
    }
    let bytes: Vec<u8> = pixels.iter().flat_map(|v| v.to_ne_bytes()).collect();
    let f32_info = skia::ImageInfo::new((w, h), skia::ColorType::RGBAF32, skia::AlphaType::Premul, None);
    let image = skia::images::raster_from_data(&f32_info, skia::Data::new_copy(&bytes), w as usize * 16)?;
    let _ = info;
    Some((image, Rect::new(x0 as f32, y0 as f32, w as f32, h as f32)))
}

type Levels = Vec<(Vec<u8>, i32, i32)>;
type Sources = BTreeMap<(u32, bool), std::rc::Rc<Levels>>;
thread_local! {static SOURCES:RefCell<Sources>=const{RefCell::new(BTreeMap::new())};static AFFINE_MIPS:RefCell<BTreeMap<u32,skia::Image>>=const{RefCell::new(BTreeMap::new())};}
fn source_levels(image: &skia::Image, mipmap: bool) -> Option<std::rc::Rc<Levels>> {
    let key = (image.unique_id(), mipmap);
    if let Some(hit) = SOURCES.with(|cache| cache.borrow().get(&key).cloned()) {
        return Some(hit);
    }
    let (sw, sh) = (image.width(), image.height());
    let mut source = vec![0u8; sw as usize * sh as usize * 4];
    let info = |w, h| {
        skia::ImageInfo::new(
            (w, h),
            skia::ColorType::RGBA8888,
            skia::AlphaType::Premul,
            None,
        )
    };
    if !image.read_pixels(
        &info(sw, sh),
        &mut source,
        sw as usize * 4,
        (0, 0),
        skia::image::CachingHint::Allow,
    ) {
        return None;
    }
    let mut levels = vec![(source, sw, sh)];
    while mipmap && levels.last().is_some_and(|(_, w, h)| *w > 1 || *h > 1) {
        let (previous, sw, sh) = levels.last()?;
        if *sw <= 1 && *sh <= 1 {
            break;
        }
        let (nw, nh) = ((sw / 2).max(1), (sh / 2).max(1));
        let mut next = vec![0u8; nw as usize * nh as usize * 4];
        // glGenerateMipmap to floor(n/2): each texel averages its exact source footprint
        // [i·k, (i+1)·k) per axis, area-weighted. For even sizes this is the 2×2 average;
        // for odd sizes it matches the reference GPU's mips (ENG-IMG-1, ≤ 2/255).
        let (sw, sh) = (*sw, *sh);
        let (kx, ky) = (sw as f32 / nw as f32, sh as f32 / nh as f32);
        let at = |x: i32, y: i32, c: usize| previous[((y.clamp(0, sh - 1) * sw + x.clamp(0, sw - 1)) * 4) as usize + c] as f32;
        let weights = |n: i32, k: f32, src: i32| -> Vec<Vec<(i32, f32)>> {
            (0..n).map(|i| {
                let (a, b) = (i as f32 * k, (i + 1) as f32 * k);
                let mut v = Vec::new();
                let mut t = a.floor() as i32;
                while (t as f32) < b {
                    let w = (b.min(t as f32 + 1.) - a.max(t as f32)) / k;
                    if w > 0. { v.push((t.min(src - 1), w)); }
                    t += 1;
                }
                v
            }).collect()
        };
        let (wx, wy) = (weights(nw, kx, sw), weights(nh, ky, sh));
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let mut acc = 0f32;
                    for &(sy, fy) in &wy[y as usize] {
                        for &(sx, fx) in &wx[x as usize] {
                            acc += at(sx, sy, c) * fx * fy;
                        }
                    }
                    next[((y * nw + x) * 4) as usize + c] = acc.round_ties_even() as u8;
                }
            }
        }
        levels.push((next, nw, nh));
    }

    let levels = std::rc::Rc::new(levels);
    SOURCES.with(|cache| {
        let mut cache = cache.borrow_mut();
        let bytes: usize = cache
            .values()
            .flat_map(|v| v.iter())
            .map(|v| v.0.len())
            .sum();
        let new: usize = levels.iter().map(|v| v.0.len()).sum();
        if cache.len() >= 64 || bytes + new > CACHE_SHARE {
            cache.clear();
        }
        if new <= CACHE_SHARE { cache.insert(key, levels.clone()); }
    });
    Some(levels)
}
/// Cached native mip chain for non-axis-aligned Skia/GPU sampling.
pub fn mipmapped(image: &skia::Image) -> skia::Image {
    AFFINE_MIPS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(hit) = cache.get(&image.unique_id()) {
            return hit.clone();
        }
        let result = image
            .with_default_mipmaps()
            .unwrap_or_else(|| image.clone());
        // Conservatively charge a full F32 mip chain; this also covers RGBA8
        // and A8 sources without depending on lazy Skia backing allocation.
        let charge = |image: &skia::Image| image.width() as usize * image.height() as usize * 32;
        if cache.len() >= 32 || cache.values().map(charge).sum::<usize>() + charge(&result) > CACHE_SHARE {
            cache.clear();
        }
        if charge(&result) <= CACHE_SHARE { cache.insert(image.unique_id(), result.clone()); }
        result
    })
}
/// Release decoded/sample caches; the UI thread calls this before EGL teardown.
pub fn clear_caches() {
    CACHE.with(|c| c.borrow_mut().entries.clear());
    SOURCES.with(|c| c.borrow_mut().clear());
    AFFINE_MIPS.with(|c| c.borrow_mut().clear());
}

#[cfg(test)]
mod budget_tests {
    use super::*;
    #[test]
    fn f32_sample_and_mip_caches_do_not_retain_oversized_results() {
        clear_caches();
        let source = skia::images::raster_from_data(
            &skia::ImageInfo::new((512,512),skia::ColorType::RGBA8888,skia::AlphaType::Premul,None),
            skia::Data::new_copy(&vec![255;512*512*4]),512*4).unwrap();
        let (sampled,_) = sample(&source,Rect::new(0.,0.,260.,180.),ImageSampling::Nearest).unwrap();
        assert_eq!(sampled.color_type(),skia::ColorType::RGBAF32);
        assert!(sampled.width() as usize*sampled.height() as usize*16 > CACHE_SHARE);
        assert!(CACHE.with(|c|c.borrow().entries.is_empty()));
        assert!(SOURCES.with(|c|c.borrow().is_empty()));
        let _ = mipmapped(&source);
        assert!(AFFINE_MIPS.with(|c|c.borrow().is_empty()));
        let mut pixel = [0u8;4];
        assert!(sampled.read_pixels(&skia::ImageInfo::new((1,1),skia::ColorType::RGBA8888,
            skia::AlphaType::Premul,None),&mut pixel,4,(0,0),skia::image::CachingHint::Disallow));
        assert_eq!(pixel,[255;4],"uncached oversize sampling must preserve its output");
        clear_caches();
    }
}
