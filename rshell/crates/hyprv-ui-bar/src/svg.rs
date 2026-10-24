// Portions adapted from Qt QBezier::addToPolygon (Qt Company Ltd).
// SPDX-License-Identifier: LGPL-3.0-only
//! In-memory, Qt-free dynamic SVG decoding. Solid paths use the FreeType gray
//! rasterizer (the lineage used by QPainter); complex SVGs use resvg. External
//! href resolution is disabled. Decoding is caller-driven, never a polling task.
use freetype::{ffi, Library};
use hyprv_ui::skia;
use resvg::{tiny_skia as tiny, usvg};

/// The backend used, so parity results never conflate the two paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    GrayOutline,
    Resvg,
}
pub struct Decoded {
    pub image: skia::Image,
    pub backend: Backend,
}
/// Decode to physical pixels. The host supplies bytes and preferred dimensions;
/// no filesystem access or Qt dependency is introduced by this API.
pub fn decode(bytes: &[u8], width: u32, height: u32) -> Option<Decoded> {
    decode_backend(bytes, width, height, Backend::GrayOutline)
}
/// Explicit backend selection for reproducible decoder parity probes.
pub fn decode_backend(
    bytes: &[u8],
    width: u32,
    height: u32,
    preferred: Backend,
) -> Option<Decoded> {
    if width == 0 || height == 0 || width > 4096 || height > 4096 || bytes.len() > 8 * 1024 * 1024 {
        return None;
    }
    let mut options = usvg::Options::default();
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    let tree = usvg::Tree::from_data(bytes, &options).ok()?;
    let transform = tiny::Transform::from_scale(
        width as f32 / tree.size().width(),
        height as f32 / tree.size().height(),
    );
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    let backend = if preferred == Backend::GrayOutline && supported(tree.root()) {
        let library = Library::init().ok()?;
        group(tree.root(), transform, width, height, &library, &mut pixels)?;
        Backend::GrayOutline
    } else {
        let mut pixmap = tiny::PixmapMut::from_bytes(&mut pixels, width, height)?;
        resvg::render(&tree, transform, &mut pixmap);
        Backend::Resvg
    };
    let info = skia::ImageInfo::new(
        (width as i32, height as i32),
        skia::ColorType::RGBA8888,
        skia::AlphaType::Premul,
        None,
    );
    let image =
        skia::images::raster_from_data(&info, skia::Data::new_copy(&pixels), width as usize * 4)?;
    Some(Decoded { image, backend })
}
fn supported(g: &usvg::Group) -> bool {
    g.clip_path().is_none()
        && g.mask().is_none()
        && g.filters().is_empty()
        && g.blend_mode() == usvg::BlendMode::Normal
        && g.children().iter().all(|node| match node {
            usvg::Node::Group(g) => supported(g),
            usvg::Node::Path(p) => {
                p.fill()
                    .is_none_or(|f| matches!(f.paint(), usvg::Paint::Color(_)))
                    && p.stroke()
                        .is_none_or(|s| matches!(s.paint(), usvg::Paint::Color(_)))
            }
            _ => false,
        })
}
fn mul(a: u8, b: u8) -> u8 {
    let x = a as u32 * b as u32 + 128;
    ((x + (x >> 8)) >> 8) as u8
}
fn over(dst: &mut [u8], src: &[u8], opacity: u8) {
    for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
        let a = mul(s[3], opacity);
        for i in 0..4 {
            d[i] = mul(s[i], opacity).saturating_add(mul(d[i], 255 - a));
        }
    }
}
fn group(
    g: &usvg::Group,
    t: tiny::Transform,
    w: u32,
    h: u32,
    lib: &Library,
    dst: &mut [u8],
) -> Option<()> {
    let mut layer = vec![0; dst.len()];
    for node in g.children() {
        match node {
            usvg::Node::Group(g) => group(g, t, w, h, lib, &mut layer)?,
            usvg::Node::Path(p) if p.is_visible() => {
                let transform = t.pre_concat(p.abs_transform());
                let mut fill = || -> Option<()> {
                    if let Some(f) = p.fill() {
                        let usvg::Paint::Color(c) = f.paint() else {
                            return None;
                        };
                        let path = p.data();
                        paint(
                            path,
                            transform,
                            f.rule() == usvg::FillRule::EvenOdd,
                            *c,
                            f.opacity().get(),
                            w,
                            h,
                            lib,
                            &mut layer,
                        )?;
                    }
                    Some(())
                };
                // Separate closures would borrow layer twice. Honor SVG paint order.
                if p.paint_order() == usvg::PaintOrder::FillAndStroke {
                    fill()?;
                    stroke(p, transform, w, h, lib, &mut layer)?;
                } else {
                    stroke(p, transform, w, h, lib, &mut layer)?;
                    if let Some(f) = p.fill() {
                        let usvg::Paint::Color(c) = f.paint() else {
                            return None;
                        };
                        paint(
                            p.data(),
                            transform,
                            f.rule() == usvg::FillRule::EvenOdd,
                            *c,
                            f.opacity().get(),
                            w,
                            h,
                            lib,
                            &mut layer,
                        )?;
                    }
                }
            }
            _ => {}
        }
    }
    over(dst, &layer, (g.opacity().get() * 255.).round() as u8);
    Some(())
}
fn stroke(
    p: &usvg::Path,
    t: tiny::Transform,
    w: u32,
    h: u32,
    lib: &Library,
    dst: &mut [u8],
) -> Option<()> {
    if let Some(s) = p.stroke() {
        let usvg::Paint::Color(c) = s.paint() else {
            return None;
        };
        let st = s.to_tiny_skia();
        let path = if let Some(dash) = &st.dash {
            p.data().dash(dash, 1.)?
        } else {
            p.data().clone()
        };
        let path = path.stroke(&st, 1.)?;
        paint(&path, t, false, *c, s.opacity().get(), w, h, lib, dst)?;
    }
    Some(())
}
#[allow(clippy::too_many_arguments)]
fn paint(
    path: &tiny::Path,
    t: tiny::Transform,
    even_odd: bool,
    c: usvg::Color,
    opacity: f32,
    w: u32,
    h: u32,
    lib: &Library,
    dst: &mut [u8],
) -> Option<()> {
    let mut points = Vec::<ffi::FT_Vector>::new();
    let mut tags = Vec::new();
    let mut contours = Vec::<i16>::new();
    let mut open = false;
    let point = |p: tiny::Point| ffi::FT_Vector {
        x: ((t.sx as f64 * p.x as f64 + t.kx as f64 * p.y as f64 + t.tx as f64) * 64.).round() as _,
        y: ((h as f64 - (t.ky as f64 * p.x as f64 + t.sy as f64 * p.y as f64 + t.ty as f64)) * 64.)
            .round() as _,
    };
    let tolerance = 0.25 / t.sx.hypot(t.ky).max(t.kx.hypot(t.sy)).max(1e-6) as f64;
    let mut last = tiny::Point::from_xy(0., 0.);
    for segment in path.segments() {
        use tiny::PathSegment::*;
        match segment {
            MoveTo(p) => {
                if open {
                    contours.push((points.len() - 1).try_into().ok()?);
                }
                points.push(point(p));
                tags.push(1);
                open = true;
                last = p;
            }
            LineTo(p) => {
                points.push(point(p));
                tags.push(1);
                last = p;
            }
            QuadTo(a, b) => {
                let c1 = tiny::Point::from_xy(
                    last.x + (a.x - last.x) * 2. / 3.,
                    last.y + (a.y - last.y) * 2. / 3.,
                );
                let c2 =
                    tiny::Point::from_xy(b.x + (a.x - b.x) * 2. / 3., b.y + (a.y - b.y) * 2. / 3.);
                for p in flatten(last, c1, c2, b, tolerance) {
                    points.push(point(p));
                    tags.push(1);
                }
                last = b;
            }
            CubicTo(a, b, c) => {
                for p in flatten(last, a, b, c, tolerance) {
                    points.push(point(p));
                    tags.push(1);
                }
                last = c;
            }
            Close => {
                if open {
                    contours.push((points.len() - 1).try_into().ok()?);
                    open = false;
                }
            }
        }
    }
    if open {
        contours.push((points.len() - 1).try_into().ok()?);
    }
    let mut outline = ffi::FT_Outline {
        n_contours: contours.len().try_into().ok()?,
        n_points: points.len().try_into().ok()?,
        points: points.as_mut_ptr(),
        tags: tags.as_mut_ptr(),
        contours: contours.as_mut_ptr(),
        flags: if even_odd {
            ffi::FT_OUTLINE_EVEN_ODD_FILL
        } else {
            0
        },
    };
    let mut mask = vec![0u8; w as usize * h as usize];
    let bitmap = ffi::FT_Bitmap {
        rows: h as _,
        width: w as _,
        pitch: w as _,
        buffer: mask.as_mut_ptr(),
        num_grays: 256,
        pixel_mode: ffi::FT_PIXEL_MODE_GRAY as _,
        palette_mode: 0,
        palette: std::ptr::null_mut(),
    };
    // SAFETY: all outline arrays and the positive-pitch bitmap are owned above,
    // dimension/count checked, and remain alive through this synchronous call.
    if unsafe { ffi::FT_Outline_Get_Bitmap(lib.raw(), &mut outline, &bitmap) } != 0 {
        return None;
    }
    let alpha = (opacity * 255.).round() as u8;
    for (d, coverage) in dst.chunks_exact_mut(4).zip(mask) {
        let a = mul(alpha, coverage);
        for (i, color) in [c.red, c.green, c.blue, 255].into_iter().enumerate() {
            d[i] = mul(color, a).saturating_add(mul(d[i], 255 - a));
        }
    }
    Some(())
}

// Qt flattens cubics before its gray rasterizer, at 0.25 device-pixel tolerance.
fn flatten(
    p0: tiny::Point,
    p1: tiny::Point,
    p2: tiny::Point,
    p3: tiny::Point,
    tolerance: f64,
) -> Vec<tiny::Point> {
    let p = |p: tiny::Point| [p.x as f64, p.y as f64];
    let mut stack = vec![([p(p0), p(p1), p(p2), p(p3)], 9)];
    let mut out = vec![];
    while let Some((b, level)) = stack.pop() {
        let dx = b[3][0] - b[0][0];
        let dy = b[3][1] - b[0][1];
        let mut l = dx.abs() + dy.abs();
        // Qt receives qreal points. usvg stores f32, which can turn an exact
        // unit-length subdivision into 1 + a few ULPs. Keep that tie on Qt's
        // short-segment branch instead of changing the subdivision topology.
        let d = if l > 1. + 1e-5 {
            (dx * (b[0][1] - b[1][1]) - dy * (b[0][0] - b[1][0])).abs()
                + (dx * (b[0][1] - b[2][1]) - dy * (b[0][0] - b[2][0])).abs()
        } else {
            l = 1.;
            (b[0][0] - b[1][0]).abs()
                + (b[0][1] - b[1][1]).abs()
                + (b[0][0] - b[2][0]).abs()
                + (b[0][1] - b[2][1]).abs()
        };
        if d < tolerance * l || level == 0 {
            out.push(tiny::Point::from_xy(b[3][0] as f32, b[3][1] as f32));
        } else {
            let mid = |a: [f64; 2], b: [f64; 2]| [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
            let a = mid(b[0], b[1]);
            let c = mid(b[1], b[2]);
            let e = mid(b[2], b[3]);
            let f = mid(a, c);
            let g = mid(c, e);
            let h = mid(f, g);
            stack.push(([h, g, e, b[3]], level - 1));
            stack.push(([b[0], a, f, h], level - 1));
        }
    }
    out
}
