// Copyright (C) 2016 The Qt Company Ltd.
// SPDX-License-Identifier: LGPL-3.0-only
//! Canvas 2D path geometry, from Qt 6.12 QQuickContext2D and qstroker.cpp.
//! Arc angles are radians, clockwise in screen coordinates. Qt uses cubic
//! Béziers, not Skia's conic arcs, and quantizes arc degrees to float first.
//! Stroke coverage uses the Qt QStroker/QRasterPaintEngine port in qt_painter.
use crate::{
    geometry::Rect,
    scene::{Content, Fill, Node, StrokeStyle},
    skia as sk,
};
const K: f64 = 0.5522847498;
type Point = [f64; 2];
#[derive(Clone, Debug, Default)]
pub struct CanvasPath {
    elements: Vec<(i32, Point)>,
    current: Point,
}
fn mix(a: Point, b: Point, t: f64) -> Point {
    [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]
}
fn split(p: [Point; 4], t: f64) -> ([Point; 4], [Point; 4]) {
    let a = mix(p[0], p[1], t);
    let b = mix(p[1], p[2], t);
    let c = mix(p[2], p[3], t);
    let d = mix(a, b, t);
    let e = mix(b, c, t);
    let f = mix(d, e, t);
    ([p[0], a, d, f], [f, e, c, p[3]])
}
fn interval(p: [Point; 4], a: f64, b: f64) -> [Point; 4] {
    let left = if b == 1. { p } else { split(p, b).0 };
    if a == 0. {
        left
    } else {
        split(left, a / b).1
    }
}
fn arc_t(angle: f64) -> f64 {
    if angle.abs() <= 1e-12 {
        return 0.;
    }
    if (angle - 90.).abs() <= 1e-12 * angle.abs().min(90.) {
        return 1.;
    }
    let (sin, cos) = angle.to_radians().sin_cos();
    let mut tc = angle / 90.;
    for _ in 0..2 {
        tc -= ((((2. - 3. * K) * tc + 3. * (K - 1.)) * tc) * tc + 1. - cos)
            / (((6. - 9. * K) * tc + 6. * (K - 1.)) * tc);
    }
    let mut ts = tc;
    for _ in 0..2 {
        ts -= ((((3. * K - 2.) * ts - 6. * K + 3.) * ts + 3. * K) * ts - sin)
            / (((9. * K - 6.) * ts + 12. * K - 6.) * ts + 3. * K);
    }
    (tc + ts) * 0.5
}
fn ellipse(center: Point, r: f64, angle: f64) -> Point {
    let theta = angle - 360. * (angle / 360.).floor();
    let quad = (theta / 90.) as i32;
    let mut t = arc_t((theta / 90. - quad as f64) * 90.);
    if quad & 1 != 0 {
        t = 1. - t;
    }
    let mt = 1. - t;
    let (a, b, c, d) = (mt * mt * mt, 3. * mt * mt * t, 3. * mt * t * t, t * t * t);
    let mut p = [a + b + c * K, d + c + b * K];
    if quad == 1 || quad == 2 {
        p[0] = -p[0];
    }
    if quad == 0 || quad == 1 {
        p[1] = -p[1];
    }
    [center[0] + r * p[0], center[1] + r * p[1]]
}
impl CanvasPath {
    pub fn new() -> Self {
        Self::default()
    }
    /// QPainterPath-compatible element kinds: move=0, line=1, cubic=2/data=3.
    pub fn elements(&self) -> &[(i32, Point)] {
        &self.elements
    }
    pub fn move_to(&mut self, x: f64, y: f64) -> &mut Self {
        if x.is_finite() && y.is_finite() {
            self.elements.push((0, [x, y]));
            self.current = [x, y];
        }
        self
    }
    pub fn line_to(&mut self, x: f64, y: f64) -> &mut Self {
        if !x.is_finite() || !y.is_finite() {
            return self;
        }
        if self.elements.is_empty() {
            self.move_to(x, y)
        } else {
            if self.current != [x, y] {
                self.elements.push((1, [x, y]));
                self.current = [x, y];
            }
            self
        }
    }
    pub fn cubic_to(&mut self, c1: Point, c2: Point, end: Point) -> &mut Self {
        if c1.into_iter().chain(c2).chain(end).any(|v| !v.is_finite()) {
            return self;
        }
        if self.elements.is_empty() {
            self.move_to(0., 0.);
        }
        self.elements.extend([(2, c1), (3, c2), (3, end)]);
        self.current = end;
        self
    }
    pub fn quadratic_curve_to(&mut self, cx: f64, cy: f64, x: f64, y: f64) -> &mut Self {
        let p = self.current;
        self.cubic_to(
            [(p[0] + 2. * cx) / 3., (p[1] + 2. * cy) / 3.],
            [(x + 2. * cx) / 3., (y + 2. * cy) / 3.],
            [x, y],
        )
    }
    pub fn close(&mut self) -> &mut Self {
        if let Some(start) = self.elements.iter().rev().find(|e| e.0 == 0).map(|e| e.1) {
            self.line_to(start[0], start[1]);
        }
        self
    }
    pub fn arc(
        &mut self,
        cx: f64,
        cy: f64,
        r: f64,
        start: f64,
        end: f64,
        counterclockwise: bool,
    ) -> &mut Self {
        if [cx, cy, r, start, end].iter().any(|v| !v.is_finite()) || r < 0. || start == end {
            return self;
        }
        let sa = (-start.to_degrees()) as f32 as f64;
        let ea = (-end.to_degrees()) as f32 as f64;
        let anti = !counterclockwise;
        let diff = ((ea as f32) - (sa as f32)) as f64;
        let mut span = 0.;
        if (!anti && diff >= 360.) || (anti && -diff >= 360.) {
            span = 360.;
        } else {
            if !anti && ea < sa {
                span += 360.;
            } else if anti && sa < ea {
                span -= 360.;
            }
            if !((span + diff).abs() <= 1e-12 && (span.abs() - 360.).abs() <= 1e-12) {
                span += diff;
            }
        }
        let center = [cx, cy];
        if r == 0. && self.elements.is_empty() {
            return self.move_to(0., 0.);
        }
        if self.elements.is_empty() {
            let p = ellipse(center, r, sa);
            self.move_to(p[0], p[1]);
        } else if r == 0. {
            return self.line_to(cx, cy);
        }
        self.arc_degrees(center, r, sa, span);
        self
    }
    fn arc_degrees(&mut self, c: Point, r: f64, start: f64, span: f64) {
        let span = span.clamp(-360., 360.);
        let mut first = (start / 90.).floor() as i32;
        let mut last = ((start + span) / 90.).floor() as i32;
        let (mut a, mut b) = (
            (start - first as f64 * 90.) / 90.,
            (start + span - last as f64 * 90.) / 90.,
        );
        let delta = if span > 0. { 1 } else { -1 };
        if delta < 0 {
            a = 1. - a;
            b = 1. - b;
        }
        if (a - 1.).abs() <= 1e-12 {
            a = 0.;
            first += delta;
        }
        if b.abs() <= 1e-12 {
            b = 1.;
            last -= delta;
        }
        a = arc_t(a * 90.);
        b = arc_t(b * 90.);
        if first == last + delta {
            return;
        }
        let start_point = ellipse(c, r, start);
        self.line_to(start_point[0], start_point[1]);
        let base = [
            [1., 0.],
            [1., K],
            [K, 1.],
            [0., 1.],
            [-K, 1.],
            [-1., K],
            [-1., 0.],
            [-1., -K],
            [-K, -1.],
            [0., -1.],
            [K, -1.],
            [1., -K],
            [1., 0.],
        ]
        .map(|p| [c[0] + r * p[0], c[1] + r * p[1]]);
        let mut i = first;
        while i != last + delta {
            let j = (3 - i.rem_euclid(4)) as usize * 3;
            let p = if delta > 0 {
                [base[j + 3], base[j + 2], base[j + 1], base[j]]
            } else {
                [base[j], base[j + 1], base[j + 2], base[j + 3]]
            };
            if first == last && (a - b).abs() <= 1e-12 * a.abs().min(b.abs()) {
                return;
            }
            let p = interval(
                p,
                if i == first { a } else { 0. },
                if i == last { b } else { 1. },
            );
            let end = if i == last {
                ellipse(c, r, start + span)
            } else {
                p[3]
            };
            self.cubic_to(p[1], p[2], end);
            i += delta;
        }
    }
    pub fn path(&self) -> sk::Path {
        let mut path = sk::Path::new();
        let mut i = 0;
        while i < self.elements.len() {
            let (k, p) = self.elements[i];
            match k {
                0 => {
                    path.move_to((p[0] as f32, p[1] as f32));
                }
                1 => {
                    path.line_to((p[0] as f32, p[1] as f32));
                }
                2 => {
                    let p2 = self.elements[i + 1].1;
                    let e = self.elements[i + 2].1;
                    path.cubic_to(
                        (p[0] as f32, p[1] as f32),
                        (p2[0] as f32, p2[1] as f32),
                        (e[0] as f32, e[1] as f32),
                    );
                    i += 2;
                }
                _ => {}
            }
            i += 1;
        }
        path
    }
    pub fn stroke_node(&self, frame: Rect, stroke: StrokeStyle) -> Node {
        Node::new(
            frame,
            Content::Canvas(std::rc::Rc::new(CanvasBatch {
                operations: vec![CanvasPaint {
                    path: self.clone(),
                    fill: None,
                    gradient: None,
                    stroke: Some(stroke),
                }],
            })),
        )
    }
    pub fn fill_node(&self, frame: Rect, fill: Fill) -> Node {
        if let Fill::Solid(color) = fill {
            Node::new(
                frame,
                Content::Canvas(std::rc::Rc::new(CanvasBatch {
                    operations: vec![CanvasPaint {
                        path: self.clone(),
                        fill: Some(color),
                        gradient: None,
                        stroke: None,
                    }],
                })),
            )
        } else {
            Node::new(
                frame,
                Content::Path {
                    path: self.path(),
                    fill,
                    stroke: None,
                },
            )
        }
    }
}
/// One Canvas texture paint, kept at double precision until Qt's 26.6 outline map.
#[derive(Clone, Debug)]
pub struct CanvasPaint {
    pub path: CanvasPath,
    pub fill: Option<crate::animation::Color>,
    /// `fillStyle = ctx.createLinearGradient(0, y0, 0, y1)` with stops 0/1; takes
    /// precedence over `fill` (QPainter's QGradientCache colour table).
    pub gradient: Option<VerticalGradient>,
    pub stroke: Option<StrokeStyle>,
}
/// A two-stop vertical Canvas gradient in item coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VerticalGradient {
    pub y0: f64,
    pub y1: f64,
    pub top: crate::animation::Color,
    pub bottom: crate::animation::Color,
}
/// Ordered paints sharing one Qt Canvas backing image. Use one batch for a ring's
/// track and progress so each draw retains QPainter's integer source-over rounding.
#[derive(Clone, Debug, Default)]
pub struct CanvasBatch {
    pub operations: Vec<CanvasPaint>,
}
impl CanvasBatch {
    pub fn node(self, frame: Rect) -> Node {
        Node::new(frame, Content::Canvas(std::rc::Rc::new(self)))
    }
    pub fn raster(&self, width: u32, height: u32, dpr: f64) -> Option<sk::Image> {
        raster_operations(&self.operations, width, height, dpr)
    }
}
impl CanvasPaint {
    pub fn raster(&self, width: u32, height: u32, dpr: f64) -> Option<sk::Image> {
        raster_operations(std::slice::from_ref(self), width, height, dpr)
    }
    fn fingerprint(&self, key: &mut Vec<u64>) -> Option<()> {
        key.push(u64::from(self.fill.is_some()));
        if let Some(fill) = self.fill {
            key.extend([fill.r, fill.g, fill.b, fill.a].map(|x| x.to_bits() as u64));
        }
        key.push(u64::from(self.gradient.is_some()));
        if let Some(g) = self.gradient {
            key.extend([g.y0.to_bits(), g.y1.to_bits()]);
            key.extend([g.top.r, g.top.g, g.top.b, g.top.a, g.bottom.r, g.bottom.g, g.bottom.b, g.bottom.a].map(|x| x.to_bits() as u64));
        }
        key.push(u64::from(self.stroke.is_some()));
        if let Some(s) = &self.stroke {
            if !s.width.is_finite() || s.width < 0. {
                return None;
            }
            key.extend(
                [s.width, s.color.r, s.color.g, s.color.b, s.color.a].map(|x| x.to_bits() as u64),
            );
            key.extend([s.cap as u64, s.join as u64]);
        }
        key.push(self.path.elements.len() as u64);
        for &(kind, point) in &self.path.elements {
            key.extend([kind as u64, point[0].to_bits(), point[1].to_bits()]);
        }
        Some(())
    }
}
fn raster_operations(
    operations: &[CanvasPaint],
    width: u32,
    height: u32,
    dpr: f64,
) -> Option<sk::Image> {
    if !dpr.is_finite()
        || dpr <= 0.
        || width == 0
        || height == 0
        || (width as u64) * (height as u64) > 64 * 1024 * 1024
    {
        return None;
    }
    let mut key = vec![
        width as u64,
        height as u64,
        dpr.to_bits(),
        operations.len() as u64,
    ];
    for paint in operations {
        paint.fingerprint(&mut key)?;
    }
    if let Some(image) = CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.clock = cache.clock.wrapping_add(1);
        let now = cache.clock;
        cache.entries.iter_mut().find(|e| e.key == key).map(|e| {
            e.used = now;
            e.image.clone()
        })
    }) {
        return Some(image);
    }
    let mut canvas = crate::qt_painter::Canvas::new(width as usize, height as usize, dpr);
    for paint in operations {
        paint.replay(&mut canvas);
    }
    let image = canvas.to_image()?;
    let bytes = width as usize * height as usize * 4 + key.capacity() * 8;
    if bytes <= crate::render::RASTER_CACHE_SHARE {
        CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            while cache.bytes + bytes > crate::render::RASTER_CACHE_SHARE || cache.entries.len() >= 256 {
                let Some((i, _)) = cache.entries.iter().enumerate().min_by_key(|(_, e)| e.used)
                else {
                    break;
                };
                cache.bytes -= cache.entries.remove(i).bytes;
            }
            cache.bytes += bytes;
            let used = cache.clock;
            cache.entries.push(CachedCanvas {
                key,
                image: image.clone(),
                used,
                bytes,
            });
        });
    }
    Some(image)
}
impl CanvasPaint {
    fn replay(&self, canvas: &mut crate::qt_painter::Canvas) {
        use crate::qt_painter::{Brush, Cap, Et, Join, Path, Rgba64, P};
        let mut path = Path::new();
        path.el = self
            .path
            .elements
            .iter()
            .map(|&(kind, p)| {
                (
                    match kind {
                        0 => Et::Move,
                        1 => Et::Line,
                        2 => Et::Curve,
                        _ => Et::CurveData,
                    },
                    P { x: p[0], y: p[1] },
                )
            })
            .collect();
        if let Some(g) = self.gradient {
            canvas.fill_path(&path, Brush::VerticalGradient { y0: g.y0, y1: g.y1, c0: Rgba64::from_color(g.top), c1: Rgba64::from_color(g.bottom) });
        } else if let Some(fill) = self.fill {
            canvas.fill_path(&path, Brush::Solid(Rgba64::from_color(fill)));
        }
        if let Some(stroke) = &self.stroke {
            let cap = match stroke.cap {
                sk::paint::Cap::Butt => Cap::Flat,
                sk::paint::Cap::Square => Cap::Square,
                _ => Cap::Round,
            };
            let join = match stroke.join {
                sk::paint::Join::Miter => Join::Miter,
                sk::paint::Join::Bevel => Join::Bevel,
                _ => Join::Round,
            };
            canvas.stroke_path(
                &path,
                stroke.width as f64,
                cap,
                join,
                10., // QQuickContext2D default miterLimit.
                Rgba64::from_color(stroke.color),
            );
        }
    }
}

struct CachedCanvas {
    key: Vec<u64>,
    image: sk::Image,
    used: u64,
    bytes: usize,
}
#[derive(Default)]
struct CanvasCache {
    entries: Vec<CachedCanvas>,
    bytes: usize,
    clock: u64,
}
thread_local! {static CACHE:std::cell::RefCell<CanvasCache>=std::cell::RefCell::new(CanvasCache::default());}
/// Drop bounded CPU Canvas textures at the renderer resource boundary.
pub fn clear_cache() {
    CACHE.with(|c| *c.borrow_mut() = CanvasCache::default());
}
