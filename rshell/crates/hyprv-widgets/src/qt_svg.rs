//! Icon SVGs rasterised the way the original shell's Qt renders them:
//! `QSvgIconEngine` → `QSvgRenderer::render(&painter, QRect(0, 0, w, h))` into an
//! ARGB32_Premultiplied QImage with an antialiased QPainter (QRasterPaintEngine).
//!
//! The SVG is parsed and normalised by usvg; painting follows QtSvg / qtbase 6.12:
//! - **Coverage**: every fill and stroke is mapped to device space and
//!   rasterised by hyprv-ui's port of Qt's raster path (QOutlineMapper → qgrayraster,
//!   QStroker for pens), so edges get Qt's exact antialiasing.
//! - **Paint**: solid colours as QSpanData (`qPremultiply(combineAlpha256(c, opacity))`);
//!   gradients through QGradientCache's 1024-entry table (component interpolation,
//!   premultiplied afterwards) and `qt_gradient_pixel` with pad/reflect/repeat. Linear
//!   `t` is the projection on (x1,y1)→(x2,y2); radial uses `qt_fetch_radial_gradient`
//!   with the focal point pulled 0.1 % inside the circle
//!   (`qt_radial_gradient_adapt_focal_point`). Pixels are sampled at their centres.
//! - **Blending**: `comp_func_SourceOver` with the span coverage as constant alpha
//!   (`BYTE_MUL` rounding). `clip-path` is ignored, as QtSvg 6.12 does (measured).
//! - **Opacity**: QtSvg multiplies `opacity` into the painter for every descendant (no
//!   group layer), and `fill-opacity` / `stroke-opacity` into the pen or brush draw.
//!   Painter opacity reaches the spans as `qRound(opacity * 256)`.
//!
//! Thin pens (≤ 1 device px) take QCosmeticStroker's curve subdivision, and CSS
//! `rgba()`/`hsl()` colours become opaque black as QtSvg's parser leaves them invalid.
//!
//! Unsupported content (images, text, masks, filters, patterns, even-odd fills) is
//! reported by [`render`] returning `None`, so callers can fall back to another renderer.
use hyprv_ui::animation::Color;
use hyprv_ui::qt_painter::{Brush, Canvas, Cap, Join, Path, Rgba64, P};

const TABLE: usize = 1024;

/// A rasterised icon: `w × h` premultiplied RGBA8.
pub struct Raster {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Copy)]
struct Affine {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}
impl Affine {
    fn from(t: usvg::Transform) -> Self {
        Self {
            a: t.sx as f64,
            b: t.ky as f64,
            c: t.kx as f64,
            d: t.sy as f64,
            e: t.tx as f64,
            f: t.ty as f64,
        }
    }
    fn scale(sx: f64, sy: f64) -> Self {
        Self {
            a: sx,
            b: 0.,
            c: 0.,
            d: sy,
            e: 0.,
            f: 0.,
        }
    }
    /// self ∘ other (apply `other` first).
    fn then(self, other: Affine) -> Self {
        Self {
            a: self.a * other.a + self.c * other.b,
            b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d,
            d: self.b * other.c + self.d * other.d,
            e: self.a * other.e + self.c * other.f + self.e,
            f: self.b * other.e + self.d * other.f + self.f,
        }
    }
    fn map(self, x: f64, y: f64) -> (f64, f64) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }
    fn invert(self) -> Option<Self> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-12 {
            return None;
        }
        let (a, b, c, d) = (self.d / det, -self.b / det, -self.c / det, self.a / det);
        Some(Self {
            a,
            b,
            c,
            d,
            e: -(a * self.e + c * self.f),
            f: -(b * self.e + d * self.f),
        })
    }
    /// Uniform scale factor (QPen widths transform with the painter).
    fn uniform_scale(self) -> Option<f64> {
        let sx = (self.a * self.a + self.b * self.b).sqrt();
        let sy = (self.c * self.c + self.d * self.d).sqrt();
        ((sx - sy).abs() <= 1e-6 * sx.max(sy)).then_some(sx)
    }
}

// ------------------------------------------------------------------ Qt pixel math

/// qtbase BYTE_MUL.
fn byte_mul(x: u32, a: u32) -> u32 {
    let mut t = (x & 0xff00ff).wrapping_mul(a);
    t = (t.wrapping_add((t >> 8) & 0xff00ff).wrapping_add(0x800080)) >> 8;
    t &= 0xff00ff;
    let mut y = ((x >> 8) & 0xff00ff).wrapping_mul(a);
    y = y.wrapping_add((y >> 8) & 0xff00ff).wrapping_add(0x800080);
    y &= 0xff00ff00;
    y | t
}
/// comp_func_SourceOver for one pixel with constant alpha.
fn src_over(dst: &mut u32, src: u32, const_alpha: u32) {
    let s = if const_alpha != 255 {
        byte_mul(src, const_alpha)
    } else {
        src
    };
    if s >> 24 == 255 {
        *dst = s;
    } else if s != 0 {
        *dst = s.wrapping_add(byte_mul(*dst, (!s) >> 24));
    }
}

/// QRgba64 (16-bit components, straight alpha).
#[derive(Clone, Copy)]
struct C64 {
    r: u32,
    g: u32,
    b: u32,
    a: u32,
}
impl C64 {
    fn from_svg(c: usvg::Color, opacity: f32) -> Self {
        let q = |v: u8| v as u32 * 257;
        Self {
            r: q(c.red),
            g: q(c.green),
            b: q(c.blue),
            a: (opacity.clamp(0., 1.) * 65535.).round() as u32,
        }
    }
    /// combineAlpha256: scale alpha by alpha256/256.
    fn combine_alpha256(self, alpha256: u32) -> Self {
        Self {
            a: (self.a * alpha256) >> 8,
            ..self
        }
    }
    fn mul256(self, k: u32) -> Self {
        Self {
            r: (self.r * k) >> 8,
            g: (self.g * k) >> 8,
            b: (self.b * k) >> 8,
            a: (self.a * k) >> 8,
        }
    }
    fn add(self, o: Self) -> Self {
        Self {
            r: self.r + o.r,
            g: self.g + o.g,
            b: self.b + o.b,
            a: self.a + o.a,
        }
    }
    /// QRgba64::premultiplied.
    fn premultiplied(self) -> Self {
        if self.a == 65535 {
            return self;
        }
        if self.a == 0 {
            return Self {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            };
        }
        let a = self.a as u64;
        let m = |c: u32| {
            let x = c as u64 * a;
            ((x + (x >> 16) + 0x8000) >> 16) as u32
        };
        Self {
            r: m(self.r),
            g: m(self.g),
            b: m(self.b),
            a: self.a,
        }
    }
    /// QRgba64::toArgb32.
    fn argb32(self) -> u32 {
        let d = |x: u32| {
            let x = x.min(65535) + 128;
            (x - (x >> 8)) >> 8
        };
        (d(self.a) << 24) | (d(self.r) << 16) | (d(self.g) << 8) | d(self.b)
    }
}

/// QGradientCache::generateGradientColorTable (ComponentInterpolation).
fn gradient_table(stops: &[(f64, C64)], alpha256: u32) -> Vec<u32> {
    let mut table = vec![0u32; TABLE];
    if stops.is_empty() {
        return table;
    }
    let incr = 1.0 / TABLE as f64;
    let mut fpos = 1.5 * incr;
    let mut pos = 0usize;
    let mut current = stops[0].1.combine_alpha256(alpha256);
    table[pos] = current.premultiplied().argb32();
    pos += 1;
    while fpos <= stops[0].0 && pos < TABLE {
        table[pos] = table[pos - 1];
        pos += 1;
        fpos += incr;
    }
    for i in 0..stops.len() - 1 {
        let next = stops[i + 1].1.combine_alpha256(alpha256);
        if stops[i + 1].0 == stops[i].0 {
            current = next;
            continue;
        }
        let delta = 1. / (stops[i + 1].0 - stops[i].0);
        while fpos < stops[i + 1].0 && pos < TABLE {
            let dist = (256. * ((fpos - stops[i].0) * delta)) as u32;
            let idist = 256 - dist.min(256);
            table[pos] = current
                .mul256(idist)
                .add(next.mul256(dist))
                .premultiplied()
                .argb32();
            pos += 1;
            fpos += incr;
        }
        current = next;
    }
    let last = stops[stops.len() - 1]
        .1
        .combine_alpha256(alpha256)
        .premultiplied()
        .argb32();
    for t in &mut table[pos..] {
        *t = last;
    }
    table[TABLE - 1] = last;
    table
}

/// qt_gradient_clamp + table lookup (`qt_gradient_pixel`).
fn gradient_pixel(table: &[u32], spread: usvg::SpreadMethod, t: f64) -> u32 {
    let mut ipos = (t * (TABLE as f64 - 1.) + 0.5) as i64;
    let n = TABLE as i64;
    match spread {
        usvg::SpreadMethod::Repeat => {
            ipos %= n;
            if ipos < 0 {
                ipos += n;
            }
        }
        usvg::SpreadMethod::Reflect => {
            let limit = n * 2;
            ipos %= limit;
            if ipos < 0 {
                ipos += limit;
            }
            if ipos >= n {
                ipos = limit - 1 - ipos;
            }
        }
        usvg::SpreadMethod::Pad => ipos = ipos.clamp(0, n - 1),
    }
    table[ipos as usize]
}

enum Source {
    Solid(u32),
    Linear {
        inv: Affine,
        dx: f64,
        dy: f64,
        off: f64,
        table: Vec<u32>,
        spread: usvg::SpreadMethod,
    },
    Radial {
        inv: Affine,
        fx: f64,
        fy: f64,
        dx: f64,
        dy: f64,
        dr: f64,
        a: f64,
        table: Vec<u32>,
        spread: usvg::SpreadMethod,
    },
}
impl Source {
    fn new(
        paint: &usvg::Paint,
        paint_opacity: f32,
        painter_opacity: f64,
        to_device: Affine,
    ) -> Option<Self> {
        let alpha256 = (painter_opacity * 256.).round().clamp(0., 256.) as u32;
        let stops = |g: &usvg::BaseGradient| -> Vec<(f64, C64)> {
            g.stops()
                .iter()
                .map(|s| {
                    (
                        s.offset().get() as f64,
                        C64::from_svg(s.color(), s.opacity().get() * paint_opacity),
                    )
                })
                .collect()
        };
        Some(match paint {
            usvg::Paint::Color(c) => Source::Solid(
                C64::from_svg(*c, paint_opacity)
                    .combine_alpha256(alpha256)
                    .premultiplied()
                    .argb32(),
            ),
            usvg::Paint::LinearGradient(g) => {
                let inv = to_device.then(Affine::from(g.transform())).invert()?;
                let (x1, y1) = (g.x1() as f64, g.y1() as f64);
                let (vx, vy) = (g.x2() as f64 - x1, g.y2() as f64 - y1);
                let l = vx * vx + vy * vy;
                let (dx, dy) = if l != 0. { (vx / l, vy / l) } else { (0., 0.) };
                Source::Linear {
                    inv,
                    dx,
                    dy,
                    off: -dx * x1 - dy * y1,
                    table: gradient_table(&stops(g), alpha256),
                    spread: g.spread_method(),
                }
            }
            usvg::Paint::RadialGradient(g) => {
                let inv = to_device.then(Affine::from(g.transform())).invert()?;
                let (cx, cy, r) = (g.cx() as f64, g.cy() as f64, g.r().get() as f64);
                let (mut fx, mut fy) = (g.fx() as f64, g.fy() as f64);
                // qt_radial_gradient_adapt_focal_point
                let comp = r - r * 0.001;
                let (lx, ly) = (fx - cx, fy - cy);
                let len = (lx * lx + ly * ly).sqrt();
                if len > comp {
                    fx = cx + lx * comp / len;
                    fy = cy + ly * comp / len;
                }
                let (dx, dy, dr) = (cx - fx, cy - fy, r);
                Source::Radial {
                    inv,
                    fx,
                    fy,
                    dx,
                    dy,
                    dr,
                    a: dr * dr - dx * dx - dy * dy,
                    table: gradient_table(&stops(g), alpha256),
                    spread: g.spread_method(),
                }
            }
            usvg::Paint::Pattern(_) => return None,
        })
    }
    fn at(&self, x: usize, y: usize) -> u32 {
        let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
        match self {
            Source::Solid(c) => *c,
            Source::Linear {
                inv,
                dx,
                dy,
                off,
                table,
                spread,
                ..
            } => {
                let (rx, ry) = inv.map(px, py);
                gradient_pixel(table, *spread, dx * rx + dy * ry + off)
            }
            Source::Radial {
                inv,
                fx,
                fy,
                dx,
                dy,
                dr,
                a,
                table,
                spread,
            } => {
                let (gx, gy) = inv.map(px, py);
                let (rx, ry) = (gx - fx, gy - fy);
                let b = 2. * (rx * dx + ry * dy);
                let det = b * b + 4. * a * (rx * rx + ry * ry);
                let t = if *a != 0. {
                    (-b + det.max(0.).sqrt()) / (2. * a)
                } else {
                    0.
                };
                let _ = dr;
                gradient_pixel(table, *spread, t)
            }
        }
    }
}

// ------------------------------------------------------------------ geometry → coverage

/// QCosmeticStroker::renderCubicSubdivision: points are stored end-first
/// (`pts[3]` = start, `pts[0]` = end); split until flat or 6 levels deep, emitting lines.
fn cosmetic_cubic(pts: &mut [(f64, f64)], level: u32, out: &mut Vec<(f64, f64)>) {
    if level > 0 {
        let (dx, dy) = (pts[3].0 - pts[0].0, pts[3].1 - pts[0].1);
        let len = 0.25 * (dx.abs() + dy.abs());
        if (dx * (pts[0].1 - pts[2].1) - dy * (pts[0].0 - pts[2].0)).abs() >= len
            || (dx * (pts[0].1 - pts[1].1) - dy * (pts[0].0 - pts[1].0)).abs() >= len
        {
            // splitCubic (de Casteljau, in place over 7 points), per axis.
            fn split(pts: &mut [(f64, f64)], axis: usize) {
                let g = |p: &(f64, f64)| if axis == 0 { p.0 } else { p.1 };
                let set = |p: &mut (f64, f64), v: f64| if axis == 0 { p.0 = v } else { p.1 = v };
                let (p0, c, d, p3) = (g(&pts[0]), g(&pts[1]), g(&pts[2]), g(&pts[3]));
                set(&mut pts[6], p3);
                let a = (p0 + c) * 0.5;
                set(&mut pts[1], a);
                let b = (p3 + d) * 0.5;
                set(&mut pts[5], b);
                let cd = (c + d) * 0.5;
                let a2 = (a + cd) * 0.5;
                set(&mut pts[2], a2);
                let b2 = (b + cd) * 0.5;
                set(&mut pts[4], b2);
                set(&mut pts[3], (a2 + b2) * 0.5);
            }
            split(pts, 0);
            split(pts, 1);
            cosmetic_cubic(&mut pts[3..], level - 1, out);
            cosmetic_cubic(&mut pts[..], level - 1, out);
            return;
        }
    }
    out.push(pts[0]);
}

fn device_path(data: &usvg::tiny_skia_path::Path, m: Affine) -> Path {
    build_path(data, m, false)
}

/// `cosmetic`: curves flattened as QCosmeticStroker does (for pens ≤ 1 device px).
fn build_path(data: &usvg::tiny_skia_path::Path, m: Affine, cosmetic: bool) -> Path {
    use usvg::tiny_skia_path::PathSegment as S;
    let mut path = Path::new();
    let cubic = |path: &mut Path, from: P, c1: P, c2: P, to: P| {
        if !cosmetic {
            path.cubic_to(c1, c2, to);
            return;
        }
        let mut pts = [(0., 0.); 3 * 6 + 4];
        pts[3] = (from.x, from.y);
        pts[2] = (c1.x, c1.y);
        pts[1] = (c2.x, c2.y);
        pts[0] = (to.x, to.y);
        let mut out = Vec::new();
        cosmetic_cubic(&mut pts, 6, &mut out);
        for (x, y) in out {
            path.line_to(P { x, y });
        }
    };
    let mut cur = P { x: 0., y: 0. };
    let pt = |x: f32, y: f32| {
        let (x, y) = m.map(x as f64, y as f64);
        P { x, y }
    };
    let mut last = (0f32, 0f32);
    let mut start = P { x: 0., y: 0. };
    for seg in data.segments() {
        match seg {
            S::MoveTo(p) => {
                cur = pt(p.x, p.y);
                start = cur;
                path.move_to(cur);
                last = (p.x, p.y);
            }
            S::LineTo(p) => {
                cur = pt(p.x, p.y);
                path.line_to(cur);
                last = (p.x, p.y);
            }
            S::QuadTo(q, p) => {
                // QPainterPath::quadTo → cubicTo
                let c1 = (
                    last.0 + 2. / 3. * (q.x - last.0),
                    last.1 + 2. / 3. * (q.y - last.1),
                );
                let c2 = (p.x + 2. / 3. * (q.x - p.x), p.y + 2. / 3. * (q.y - p.y));
                let to = pt(p.x, p.y);
                cubic(&mut path, cur, pt(c1.0, c1.1), pt(c2.0, c2.1), to);
                cur = to;
                last = (p.x, p.y);
            }
            S::CubicTo(c1, c2, p) => {
                let to = pt(p.x, p.y);
                cubic(&mut path, cur, pt(c1.x, c1.y), pt(c2.x, c2.y), to);
                cur = to;
                last = (p.x, p.y);
            }
            S::Close => {
                path.close_subpath();
                cur = start;
            }
        }
    }
    path
}

fn white() -> Rgba64 {
    Rgba64::from_color(Color {
        r: 1.,
        g: 1.,
        b: 1.,
        a: 1.,
    })
}

fn coverage(w: usize, h: usize, paint: impl FnOnce(&mut Canvas)) -> Vec<u8> {
    let mut canvas = Canvas::new(w, h, 1.0);
    paint(&mut canvas);
    canvas.px.iter().map(|p| (p >> 24) as u8).collect()
}

struct Ctx {
    w: usize,
    h: usize,
    root: Affine,
    px: Vec<u32>,
}

impl Ctx {
    fn blend(&mut self, cov: &[u8], src: &Source) {
        for y in 0..self.h {
            for x in 0..self.w {
                let i = y * self.w + x;
                let c = cov[i] as u32;
                if c == 0 {
                    continue;
                }
                src_over(&mut self.px[i], src.at(x, y), c);
            }
        }
    }

    fn group(&mut self, g: &usvg::Group, opacity: f64) -> Option<()> {
        if g.mask().is_some() || !g.filters().is_empty() {
            return None;
        }
        let opacity = opacity * g.opacity().get() as f64;
        // QtSvg (6.12) ignores clip-path: fcitx's clipped body matches Qt only unclipped
        // (max 13 vs 138 at 72 px).
        for node in g.children() {
            match node {
                usvg::Node::Group(child) => self.group(child, opacity)?,
                usvg::Node::Path(p) => self.path(p, opacity)?,
                _ => return None,
            }
        }
        Some(())
    }

    fn path(&mut self, p: &usvg::Path, opacity: f64) -> Option<()> {
        if !p.is_visible() {
            return Some(());
        }
        let m = self.root.then(Affine::from(p.abs_transform()));
        let device = device_path(p.data(), m);
        let fill = |ctx: &mut Ctx| -> Option<()> {
            if let Some(f) = p.fill() {
                if f.rule() != usvg::FillRule::NonZero {
                    return None;
                }
                let src = Source::new(f.paint(), 1.0, opacity * f.opacity().get() as f64, m)?;
                let cov = coverage(ctx.w, ctx.h, |c| {
                    c.fill_path(&device, Brush::Solid(white()))
                });
                ctx.blend(&cov, &src);
            }
            Some(())
        };
        let stroke = |ctx: &mut Ctx| -> Option<()> {
            if let Some(s) = p.stroke() {
                if s.dasharray().is_some() {
                    return None;
                }
                let k = m.uniform_scale()?;
                let src = Source::new(s.paint(), 1.0, opacity * s.opacity().get() as f64, m)?;
                let cap = match s.linecap() {
                    usvg::LineCap::Butt => Cap::Flat,
                    usvg::LineCap::Round => Cap::Round,
                    usvg::LineCap::Square => Cap::Square,
                };
                let join = match s.linejoin() {
                    usvg::LineJoin::Round => Join::Round,
                    usvg::LineJoin::Bevel => Join::Bevel,
                    _ => Join::Miter,
                };
                let width = s.width().get() as f64 * k;
                let miter = s.miterlimit().get() as f64;
                // QRasterPaintEngine fast_pen: antialiased pens ≤ 1 device px go through
                // QCosmeticStroker (curves flattened by its own subdivision).
                let path = if width <= 1. {
                    build_path(p.data(), m, true)
                } else {
                    device.clone()
                };
                let cov = coverage(ctx.w, ctx.h, |c| {
                    c.stroke_path(&path, width, cap, join, miter, white())
                });
                ctx.blend(&cov, &src);
            }
            Some(())
        };
        match p.paint_order() {
            usvg::PaintOrder::FillAndStroke => {
                fill(self)?;
                stroke(self)?;
            }
            usvg::PaintOrder::StrokeAndFill => {
                stroke(self)?;
                fill(self)?;
            }
        }
        Some(())
    }
}

/// QtSvg's colour parser understands `#rgb`, `#rrggbb`, `rgb(…)` and names, but not
/// CSS `rgba()`/`hsl()`/`hsla()`: those become an invalid QColor, painted opaque black
/// (measured: keepassxc's `fill:rgba(15,15,13,.35)` matches Qt only as `#000000`).
fn qtsvg_colors(svg: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(svg);
    let mut out = String::with_capacity(text.len());
    let mut rest: &str = &text;
    while let Some(i) = ["rgba(", "hsla(", "hsl("]
        .iter()
        .filter_map(|k| rest.find(k))
        .min()
    {
        out.push_str(&rest[..i]);
        match rest[i..].find(')') {
            Some(j) => {
                out.push_str("#000000");
                rest = &rest[i + j + 1..];
            }
            None => {
                rest = &rest[i..];
                break;
            }
        }
    }
    out.push_str(rest);
    out.into_bytes()
}

/// Render `svg` into a `w × h` pixmap like `QSvgRenderer::render(p, QRect(0, 0, w, h))`
/// (the document's viewport stretched to the rect). `None` if the document uses
/// something this renderer does not reproduce.
pub fn render(svg: &[u8], w: usize, h: usize) -> Option<Raster> {
    if w == 0 || h == 0 {
        return None;
    }
    let svg = qtsvg_colors(svg);
    let tree = usvg::Tree::from_data(&svg, &usvg::Options::default()).ok()?;
    let size = tree.size();
    let root = Affine::scale(
        w as f64 / size.width() as f64,
        h as f64 / size.height() as f64,
    );
    let mut ctx = Ctx {
        w,
        h,
        root,
        px: vec![0; w * h],
    };
    ctx.group(tree.root(), 1.0)?;
    let rgba = ctx
        .px
        .iter()
        .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, (p >> 24) as u8])
        .collect();
    Some(Raster { w, h, rgba })
}

/// The document's default size (`QSvgRenderer::defaultSize`): width/height, else the
/// viewBox size.
pub fn default_size(svg: &[u8]) -> Option<(f64, f64)> {
    let tree = usvg::Tree::from_data(&qtsvg_colors(svg), &usvg::Options::default()).ok()?;
    Some((tree.size().width() as f64, tree.size().height() as f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_square_is_opaque_and_edges_follow_qt_coverage() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect x="2.5" y="2" width="5" height="6" fill="#ff0000"/></svg>"##;
        let r = render(svg, 10, 10).unwrap();
        let at = |x: usize, y: usize| &r.rgba[(y * 10 + x) * 4..(y * 10 + x) * 4 + 4];
        assert_eq!(at(4, 4), [255, 0, 0, 255]);
        assert_eq!(at(0, 0), [0, 0, 0, 0]);
        // Half-covered column at x = 2 (2.5 → 50 % coverage).
        let half = at(2, 4);
        assert!(
            (126..=129).contains(&half[3]) && half[0] == half[3],
            "{half:?}"
        );
    }

    #[test]
    fn gradients_follow_qt_tables_and_spread() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="1"><defs>
            <linearGradient id="g" x1="0" y1="0" x2="50" y2="0" gradientUnits="userSpaceOnUse" spreadMethod="reflect">
            <stop offset="0" stop-color="#000000"/><stop offset="1" stop-color="#ffffff"/></linearGradient></defs>
            <rect width="100" height="1" fill="url(#g)"/></svg>"##;
        let r = render(svg, 100, 1).unwrap();
        let red = |x: usize| r.rgba[x * 4];
        assert!(red(0) < 4 && red(49) > 250, "{} {}", red(0), red(49));
        // Reflect: x = 75 mirrors x = 24.
        assert!(
            (red(75) as i32 - red(24) as i32).abs() <= 2,
            "{} {}",
            red(75),
            red(24)
        );
    }

    #[test]
    fn opacity_multiplies_like_qtsvg_instead_of_compositing_a_layer() {
        // Two overlapping opaque squares in a 50 % group: QtSvg draws each at 50 %, so
        // the overlap is darker than the single-coverage area (a layer would be equal).
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="1"><g opacity="0.5">
            <rect x="0" width="3" height="1" fill="#000000"/><rect x="1" width="3" height="1" fill="#000000"/></g></svg>"##;
        let r = render(svg, 4, 1).unwrap();
        let alpha = |x: usize| r.rgba[x * 4 + 3];
        assert!(alpha(1) > alpha(0), "{} {}", alpha(1), alpha(0));
    }

    /// Diagnostic: `HYPRV_QT_SVG_DUMP=<svg>:<size>:<out.rgba>,…` writes raw premultiplied
    /// RGBA8 (`size × size`, viewport stretched) for comparison with Qt's QIcon pixmaps.
    #[test]
    #[ignore]
    fn dump() {
        let Ok(spec) = std::env::var("HYPRV_QT_SVG_DUMP") else {
            return;
        };
        for one in spec.split(',') {
            let mut it = one.split(':');
            let (file, size, out) = (it.next().unwrap(), it.next().unwrap(), it.next().unwrap());
            let n: usize = size.parse().unwrap();
            let r = render(&std::fs::read(file).unwrap(), n, n).expect("supported SVG");
            std::fs::write(out, r.rgba).unwrap();
        }
    }
}
