// Copyright (C) 2016 The Qt Company Ltd.
// Copyright (C) 2000-2016 David Turner, Robert Wilhelm, and Werner Lemberg.
// SPDX-License-Identifier: LGPL-3.0-only AND FTL
//! Qt raster paint engine subset for QML Canvas: non-cosmetic cubic/line strokes,
//! caps/joins, thin straight grid lines, non-zero fills and vertical gradients.
//!
//! Ported from qtbase / qtdeclarative v6.12.0:
//! - context2d path building (qquickcontext2d.cpp: moveTo/lineTo/closePath/arc) on top
//!   of QPainterPath semantics (qpainterpath.cpp, qpainterpath_p.h: fuzzy point
//!   equality, implicit moveTo, close),
//! - `QPainter::strokePath` → QRasterPaintEngine::stroke (qpaintengine_raster.cpp):
//!   2-element "lines" paths with flat/square caps take `QRasterizer::rasterizeLine`
//!   (qrasterizer.cpp), everything else goes through QPaintEngineEx::stroke →
//!   QStroker (qstroker.cpp: qt_stroke_side, joinPoints, qt_curves_for_arc) → fill,
//! - `fill` → QOutlineMapper (transform, QBezier::addToPolygon flattening, 26.6 rounding)
//!   → qgrayraster.c (cell/cover/area accumulation, non-zero coverage) → spans,
//! - span blending for ARGB32_Premultiplied (qdrawhelper.cpp: comp_func_solid_SourceOver
//!   with BYTE_MUL rounding; vertical linear gradients through
//!   blend_vertical_gradient_argb with QGradientCache's 1024-entry colour table).
//!
//! The Canvas paints into a QImage of round(size) × dpr device pixels; the scene graph
//! then draws that texture into the item rect (qquickcanvasitem.cpp updatePaintNode).
use crate::{animation::Color, skia as sk};
use std::collections::HashMap;

// ------------------------------------------------------------------ fuzzy compares

fn fuzzy_compare(a: f64, b: f64) -> bool {
    (a - b).abs() * 1e12 <= a.abs().min(b.abs())
}
fn fuzzy_is_null(d: f64) -> bool {
    d.abs() <= 1e-12
}
/// QtPrivate::fuzzyCompare (QPointF / QLineF equality).
fn fuzzy_eq(a: f64, b: f64) -> bool {
    if a == 0. || b == 0. {
        fuzzy_is_null(a - b)
    } else {
        fuzzy_compare(a, b)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct P {
    pub x: f64,
    pub y: f64,
}
const fn p(x: f64, y: f64) -> P {
    P { x, y }
}
impl P {
    fn feq(self, o: P) -> bool {
        fuzzy_eq(self.x, o.x) && fuzzy_eq(self.y, o.y)
    }
    fn add(self, o: P) -> P {
        p(self.x + o.x, self.y + o.y)
    }
    fn sub(self, o: P) -> P {
        p(self.x - o.x, self.y - o.y)
    }
    fn mul(self, k: f64) -> P {
        p(self.x * k, self.y * k)
    }
}

// ------------------------------------------------------------------ QLineF

#[derive(Clone, Copy, Debug, Default)]
struct Line {
    p1: P,
    p2: P,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Isect {
    None,
    Bounded,
    Unbounded,
}
impl Line {
    fn new(p1: P, p2: P) -> Self {
        Self { p1, p2 }
    }
    fn dx(&self) -> f64 {
        self.p2.x - self.p1.x
    }
    fn dy(&self) -> f64 {
        self.p2.y - self.p1.y
    }
    fn length(&self) -> f64 {
        self.dx().hypot(self.dy())
    }
    fn is_null(&self) -> bool {
        self.p1.feq(self.p2)
    }
    fn angle(&self) -> f64 {
        let theta = (-self.dy()).atan2(self.dx()) * (180. / std::f64::consts::PI);
        let n = if theta < 0. { theta + 360. } else { theta };
        if fuzzy_compare(n, 360.) {
            0.
        } else {
            n
        }
    }
    fn angle_to(&self, l: &Line) -> f64 {
        if self.is_null() || l.is_null() {
            return 0.;
        }
        let delta = l.angle() - self.angle();
        let n = if delta < 0. { delta + 360. } else { delta };
        if fuzzy_compare(delta, 360.) {
            0.
        } else {
            n
        }
    }
    fn intersects(&self, l: &Line) -> (Isect, P) {
        let a = self.p2.sub(self.p1);
        let b = l.p1.sub(l.p2);
        let c = self.p1.sub(l.p1);
        let den = a.y * b.x - a.x * b.y;
        if den == 0. || !den.is_finite() {
            return (Isect::None, P::default());
        }
        let r = 1. / den;
        let na = (b.y * c.x - b.x * c.y) * r;
        let pt = self.p1.add(a.mul(na));
        if !(0. ..=1.).contains(&na) {
            return (Isect::Unbounded, pt);
        }
        let nb = (a.x * c.y - a.y * c.x) * r;
        if !(0. ..=1.).contains(&nb) {
            return (Isect::Unbounded, pt);
        }
        (Isect::Bounded, pt)
    }
    fn normal_vector(&self) -> Line {
        Line::new(self.p1, self.p1.add(p(self.dy(), -self.dx())))
    }
    fn set_length(&mut self, len: f64) {
        let old = self.length();
        if old > 0. {
            self.p2 = p(
                self.p1.x + len * (self.dx() / old),
                self.p1.y + len * (self.dy() / old),
            );
        }
    }
    fn translate(&mut self, dx: f64, dy: f64) {
        self.p1 = self.p1.add(p(dx, dy));
        self.p2 = self.p2.add(p(dx, dy));
    }
}

// ------------------------------------------------------------------ QPainterPath

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Et {
    Move,
    Line,
    Curve,
    CurveData,
}

#[derive(Clone, Debug, Default)]
pub struct Path {
    pub el: Vec<(Et, P)>,
    c_start: usize,
    require_move: bool,
}

impl Path {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn is_empty(&self) -> bool {
        self.el.is_empty()
    }
    fn last(&self) -> P {
        self.el.last().map(|e| e.1).unwrap_or_default()
    }
    pub fn move_to(&mut self, pt: P) {
        self.require_move = false;
        match self.el.last_mut() {
            Some(e) if e.0 == Et::Move => e.1 = pt,
            _ => self.el.push((Et::Move, pt)),
        }
        self.c_start = self.el.len() - 1;
    }
    fn maybe_move_to(&mut self) {
        if self.require_move {
            let l = self.last();
            self.el.push((Et::Move, l));
            self.require_move = false;
        }
    }
    pub fn line_to(&mut self, pt: P) {
        if self.el.is_empty() {
            self.el.push((Et::Move, P::default()));
        }
        self.maybe_move_to();
        if pt.feq(self.last()) {
            return;
        }
        self.el.push((Et::Line, pt));
    }
    pub fn cubic_to(&mut self, c1: P, c2: P, e: P) {
        if self.el.is_empty() {
            self.el.push((Et::Move, P::default()));
        }
        if self.last().feq(c1) && c1.feq(c2) && c2.feq(e) {
            return;
        }
        self.maybe_move_to();
        self.el.push((Et::Curve, c1));
        self.el.push((Et::CurveData, c2));
        self.el.push((Et::CurveData, e));
    }
    pub fn close_subpath(&mut self) {
        if self.el.is_empty() {
            return;
        }
        self.require_move = true;
        let first = self.el[self.c_start].1;
        let last = self.last();
        if first.x != last.x || first.y != last.y {
            if first.feq(last) {
                self.el.last_mut().unwrap().1 = first;
            } else {
                self.el.push((Et::Line, first));
            }
        }
    }
    pub fn arc_move_to(&mut self, rect: (f64, f64, f64, f64), angle: f64) {
        let (s, _) = find_ellipse_coords(rect, angle, 0.);
        self.move_to(s);
    }
    pub fn arc_to(&mut self, rect: (f64, f64, f64, f64), start: f64, sweep: f64) {
        let (start_pt, curves) = curves_for_arc(rect, start, sweep);
        self.line_to(start_pt);
        for c in curves.chunks(3) {
            self.cubic_to(c[0], c[1], c[2]);
        }
    }
    /// QVectorPathData: alternating moveTo/lineTo → LinesHint.
    fn is_lines(&self) -> bool {
        self.el
            .iter()
            .enumerate()
            .all(|(i, e)| e.0 == if i % 2 == 0 { Et::Move } else { Et::Line })
    }
}

// ------------------------------------------------------------------ QBezier / arcs

const KAPPA: f64 = 0.5522847498;

#[derive(Clone, Copy, Debug, Default)]
struct Bezier {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    x3: f64,
    y3: f64,
    x4: f64,
    y4: f64,
}
impl Bezier {
    fn from(a: P, b: P, c: P, d: P) -> Self {
        Self {
            x1: a.x,
            y1: a.y,
            x2: b.x,
            y2: b.y,
            x3: c.x,
            y3: c.y,
            x4: d.x,
            y4: d.y,
        }
    }
    fn pt1(&self) -> P {
        p(self.x1, self.y1)
    }
    fn pt2(&self) -> P {
        p(self.x2, self.y2)
    }
    fn pt3(&self) -> P {
        p(self.x3, self.y3)
    }
    fn pt4(&self) -> P {
        p(self.x4, self.y4)
    }
    fn split(&self) -> (Bezier, Bezier) {
        let mid = |a: P, b: P| a.add(b).mul(0.5);
        let m12 = mid(self.pt1(), self.pt2());
        let m23 = mid(self.pt2(), self.pt3());
        let m34 = mid(self.pt3(), self.pt4());
        let m1223 = mid(m12, m23);
        let m2334 = mid(m23, m34);
        let m = mid(m1223, m2334);
        (
            Bezier::from(self.pt1(), m12, m1223, m),
            Bezier::from(m, m2334, m34, self.pt4()),
        )
    }
    fn parameter_split_left(&mut self, t: f64) -> Bezier {
        let mut l = Bezier {
            x1: self.x1,
            y1: self.y1,
            ..Default::default()
        };
        l.x2 = self.x1 + t * (self.x2 - self.x1);
        l.y2 = self.y1 + t * (self.y2 - self.y1);
        l.x3 = self.x2 + t * (self.x3 - self.x2);
        l.y3 = self.y2 + t * (self.y3 - self.y2);
        self.x3 += t * (self.x4 - self.x3);
        self.y3 += t * (self.y4 - self.y3);
        self.x2 = l.x3 + t * (self.x3 - l.x3);
        self.y2 = l.y3 + t * (self.y3 - l.y3);
        l.x3 = l.x2 + t * (l.x3 - l.x2);
        l.y3 = l.y2 + t * (l.y3 - l.y2);
        l.x4 = l.x3 + t * (self.x2 - l.x3);
        l.y4 = l.y3 + t * (self.y2 - l.y3);
        self.x1 = l.x4;
        self.y1 = l.y4;
        l
    }
    fn on_interval(&self, t0: f64, t1: f64) -> Bezier {
        if t0 == 0. && t1 == 1. {
            return *self;
        }
        let mut b = *self;
        let _ = b.parameter_split_left(t0);
        let true_t = (t1 - t0) / (1. - t0);
        b.parameter_split_left(true_t)
    }
    /// QBezier::addToPolygon (end points only; the start point is already present).
    fn add_to_polygon(&self, out: &mut Vec<P>, threshold: f64) {
        let mut bez = [Bezier::default(); 10];
        let mut levels = [0i32; 10];
        bez[0] = *self;
        levels[0] = 9;
        let mut top: isize = 0;
        while top >= 0 {
            let b = bez[top as usize];
            let y4y1 = b.y4 - b.y1;
            let x4x1 = b.x4 - b.x1;
            let mut l = x4x1.abs() + y4y1.abs();
            let d;
            if l > 1. {
                d = ((x4x1) * (b.y1 - b.y2) - (y4y1) * (b.x1 - b.x2)).abs()
                    + ((x4x1) * (b.y1 - b.y3) - (y4y1) * (b.x1 - b.x3)).abs();
            } else {
                d = (b.x1 - b.x2).abs()
                    + (b.y1 - b.y2).abs()
                    + (b.x1 - b.x3).abs()
                    + (b.y1 - b.y3).abs();
                l = 1.;
            }
            if d < threshold * l || levels[top as usize] == 0 {
                out.push(b.pt4());
                top -= 1;
            } else {
                let (first, second) = b.split();
                let t = top as usize;
                bez[t + 1] = first;
                bez[t] = second;
                levels[t] -= 1;
                levels[t + 1] = levels[t];
                top += 1;
            }
        }
    }
}

impl Bezier {
    fn point_at(&self, t: f64) -> P {
        let m = 1. - t;
        p(
            self.x1 * m * m * m
                + 3. * self.x2 * m * m * t
                + 3. * self.x3 * m * t * t
                + self.x4 * t * t * t,
            self.y1 * m * m * m
                + 3. * self.y2 * m * m * t
                + 3. * self.y3 * m * t * t
                + self.y4 * t * t * t,
        )
    }
    fn normal_at(&self, t: f64) -> P {
        let m = 1. - t;
        p(
            m * m * (self.y2 - self.y1) + m * t * (self.y3 - self.y2) + t * t * (self.y4 - self.y3),
            -(m * m * (self.x2 - self.x1)
                + m * t * (self.x3 - self.x2)
                + t * t * (self.x4 - self.x3)),
        )
    }
    fn start_tangent(&self) -> Line {
        let start = self.pt1();
        let end = if start.feq(self.pt2()) {
            if start.feq(self.pt3()) {
                self.pt4()
            } else {
                self.pt3()
            }
        } else {
            self.pt2()
        };
        Line::new(start, end)
    }
    fn offset(&self, offset: f64, threshold: f64) -> (Shift, Self) {
        let pts = [self.pt1(), self.pt2(), self.pt3(), self.pt4()];
        let mut points = vec![pts[0]];
        let mut map = [0usize; 4];
        for i in 1..4 {
            if !(fuzzy_compare(pts[i - 1].x, pts[i].x) && fuzzy_compare(pts[i - 1].y, pts[i].y)) {
                points.push(pts[i]);
            }
            map[i] = points.len() - 1;
        }
        if points.len() == 1 {
            return (Shift::Discard, Self::default());
        }
        let w = pts.iter().map(|v| v.x).fold(f64::NEG_INFINITY, f64::max)
            - pts.iter().map(|v| v.x).fold(f64::INFINITY, f64::min);
        let h = pts.iter().map(|v| v.y).fold(f64::NEG_INFINITY, f64::max)
            - pts.iter().map(|v| v.y).fold(f64::INFINITY, f64::min);
        if points.len() == 4 && w < 0.1 * offset && h < 0.1 * offset {
            let l = (self.x1 - self.x2).powi(2)
                + (self.y1 - self.y2).powi(2) * (self.x3 - self.x4).powi(2)
                + (self.y3 - self.y4).powi(2);
            let dot = (self.x1 - self.x2) * (self.x3 - self.x4)
                + (self.y1 - self.y2) * (self.y3 - self.y4);
            if dot < 0. && dot * dot < 0.8 * l {
                return (Shift::Circle, Self::default());
            }
        }
        let normal = |a: P, b: P| {
            let d = b.sub(a);
            let len = (d.x * d.x + d.y * d.y).sqrt();
            p(d.y / len, -d.x / len)
        };
        let mut prev = normal(points[0], points[1]);
        if !prev.x.is_finite() || !prev.y.is_finite() {
            return (Shift::Discard, Self::default());
        }
        let mut shifted = [P::default(); 4];
        shifted[0] = points[0].add(prev.mul(offset));
        for i in 1..points.len() - 1 {
            let next = normal(points[i], points[i + 1]);
            let r = 1. + prev.x * next.x + prev.y * next.y;
            shifted[i] = points[i].add(if fuzzy_is_null(r) {
                prev.mul(offset)
            } else {
                prev.add(next).mul(offset / r)
            });
            prev = next;
        }
        let n = points.len() - 1;
        shifted[n] = points[n].add(prev.mul(offset));
        let out = Self::from(
            shifted[map[0]],
            shifted[map[1]],
            shifted[map[2]],
            shifted[map[3]],
        );
        if points.len() > 2 {
            for t in [0.25, 0.5, 0.75] {
                let a = self.point_at(t);
                let b = out.point_at(t);
                let d = a.sub(b);
                if ((d.x * d.x + d.y * d.y) - offset * offset).abs() > threshold * offset * offset {
                    return (Shift::Split, out);
                }
                let normal = self.normal_at(t);
                let l = normal.x.abs() + normal.y.abs();
                if l != 0.
                    && (normal.x * (a.y - b.y) - normal.y * (a.x - b.x)).abs() / l
                        > threshold * offset
                {
                    return (Shift::Split, out);
                }
            }
        }
        (Shift::Ok, out)
    }
    fn circle(&self, offset: f64) -> Option<[Self; 2]> {
        let unit = |v: P| {
            let l = (v.x * v.x + v.y * v.y).sqrt();
            if fuzzy_is_null(l) {
                None
            } else {
                Some(v.mul(1. / l))
            }
        };
        let a = unit(p(self.y2 - self.y1, self.x1 - self.x2))?;
        let c = unit(p(self.y4 - self.y3, self.x3 - self.x4))?;
        let mut b = unit(p(
            self.x1 - self.x2 - self.x3 + self.x4,
            self.y1 - self.y2 - self.y3 + self.y4,
        ))?
        .mul(-1.);
        let angle =
            |a: P, b: P| ((a.x * b.x + a.y * b.y).clamp(-1., 1.)).acos() / std::f64::consts::PI;
        let mut angles = [angle(a, b), angle(b, c)];
        let mut sign = 1.;
        if angles[0] + angles[1] > 1. {
            b = b.mul(-1.);
            angles = angles.map(|x| 1. - x);
            sign = -1.;
        }
        let normals = [a, b, c];
        let circle = [
            self.pt1().add(a.mul(offset)),
            self.pt1().add(self.pt4()).mul(0.5).add(b.mul(offset)),
            self.pt4().add(c.mul(offset)),
        ];
        Some(std::array::from_fn(|i| {
            let k = 2. * KAPPA * sign * offset * angles[i];
            Self::from(
                circle[i],
                circle[i].add(p(-normals[i].y * k, normals[i].x * k)),
                circle[i + 1].add(p(normals[i + 1].y * k, -normals[i + 1].x * k)),
                circle[i + 1],
            )
        }))
    }
    fn shifted(&self, offset: f64, mut threshold: f32) -> Vec<Self> {
        if [self.pt2(), self.pt3(), self.pt4()]
            .iter()
            .all(|p| fuzzy_compare(self.x1, p.x) && fuzzy_compare(self.y1, p.y))
        {
            return Vec::new();
        }
        'redo: loop {
            let mut stack = vec![*self];
            let mut out = Vec::new();
            while let Some(b) = stack.last().copied() {
                if stack.len() == 10 || out.len() == 15usize.saturating_sub(stack.len()) {
                    threshold *= 1.5;
                    if threshold <= 2. {
                        continue 'redo;
                    }
                    while let Some(b) = stack.pop() {
                        let (r, s) = b.offset(offset, threshold as f64);
                        if matches!(r, Shift::Ok | Shift::Split) {
                            out.push(s);
                        }
                    }
                    return out;
                }
                let (result, shifted) = b.offset(offset, threshold as f64);
                match result {
                    Shift::Discard => {
                        stack.pop();
                    }
                    Shift::Ok => {
                        out.push(shifted);
                        stack.pop();
                    }
                    Shift::Circle if 15 - out.len() >= 2 => {
                        if let Some(c) = b.circle(offset) {
                            out.extend(c);
                        }
                        stack.pop();
                    }
                    _ => {
                        let (a, b) = b.split();
                        *stack.last_mut().unwrap() = b;
                        stack.push(a);
                    }
                }
            }
            return out;
        }
    }
}
#[derive(Clone, Copy)]
enum Shift {
    Ok,
    Discard,
    Split,
    Circle,
}

fn coefficients(t: f64) -> (f64, f64, f64, f64) {
    let m_t = 1. - t;
    let mut b = m_t * m_t;
    let mut c = t * t;
    let d = c * t;
    let a = b * m_t;
    b *= 3. * t;
    c *= 3. * m_t;
    (a, b, c, d)
}

fn t_for_arc_angle(angle: f64) -> f64 {
    if fuzzy_is_null(angle) {
        return 0.;
    }
    if fuzzy_compare(angle, 90.) {
        return 1.;
    }
    let rad = angle.to_radians();
    let (cos_a, sin_a) = (rad.cos(), rad.sin());
    let k = KAPPA;
    let mut tc = angle / 90.;
    tc -= ((((2. - 3. * k) * tc + 3. * (k - 1.)) * tc) * tc + 1. - cos_a)
        / (((6. - 9. * k) * tc + 6. * (k - 1.)) * tc);
    tc -= ((((2. - 3. * k) * tc + 3. * (k - 1.)) * tc) * tc + 1. - cos_a)
        / (((6. - 9. * k) * tc + 6. * (k - 1.)) * tc);
    let mut ts = tc;
    ts -= ((((3. * k - 2.) * ts - 6. * k + 3.) * ts + 3. * k) * ts - sin_a)
        / (((9. * k - 6.) * ts + 12. * k - 6.) * ts + 3. * k);
    ts -= ((((3. * k - 2.) * ts - 6. * k + 3.) * ts + 3. * k) * ts - sin_a)
        / (((9. * k - 6.) * ts + 12. * k - 6.) * ts + 3. * k);
    0.5 * (tc + ts)
}

fn find_ellipse_coords(r: (f64, f64, f64, f64), angle: f64, length: f64) -> (P, P) {
    let (x, y, w, h) = r;
    let (w2, h2) = (w / 2., h / 2.);
    let center = p(x + w / 2., y + h / 2.);
    let mut out = [P::default(); 2];
    for (i, a) in [angle, angle + length].into_iter().enumerate() {
        let theta = a - 360. * (a / 360.).floor();
        let mut t = theta / 90.;
        let quadrant = t as i32;
        t -= quadrant as f64;
        t = t_for_arc_angle(90. * t);
        if quadrant & 1 != 0 {
            t = 1. - t;
        }
        let (ca, cb, cc, cd) = coefficients(t);
        let mut pt = p(ca + cb + cc * KAPPA, cd + cc + cb * KAPPA);
        if quadrant == 1 || quadrant == 2 {
            pt.x = -pt.x;
        }
        if quadrant == 0 || quadrant == 1 {
            pt.y = -pt.y;
        }
        out[i] = center.add(p(w2 * pt.x, h2 * pt.y));
    }
    (out[0], out[1])
}

/// qt_curves_for_arc: (start point, control points in triples).
fn curves_for_arc(rect: (f64, f64, f64, f64), start_angle: f64, mut sweep: f64) -> (P, Vec<P>) {
    let (x, y, w, h) = rect;
    if w == 0. && h == 0. {
        return (P::default(), vec![]);
    }
    let (w2, h2) = (w / 2., h / 2.);
    let (w2k, h2k) = (w2 * KAPPA, h2 * KAPPA);
    let pts = [
        p(x + w, y + h2),
        p(x + w, y + h2 + h2k),
        p(x + w2 + w2k, y + h),
        p(x + w2, y + h),
        p(x + w2 - w2k, y + h),
        p(x, y + h2 + h2k),
        p(x, y + h2),
        p(x, y + h2 - h2k),
        p(x + w2 - w2k, y),
        p(x + w2, y),
        p(x + w2 + w2k, y),
        p(x + w, y + h2 - h2k),
        p(x + w, y + h2),
    ];
    sweep = sweep.clamp(-360., 360.);
    let mut curves = Vec::new();
    if start_angle == 0. {
        if sweep == 360. {
            for i in (0..=11).rev() {
                curves.push(pts[i]);
            }
            return (pts[12], curves);
        } else if sweep == -360. {
            for pt in pts.iter().skip(1) {
                curves.push(*pt);
            }
            return (pts[0], curves);
        }
    }
    let mut start_segment = (start_angle / 90.).floor() as i32;
    let mut end_segment = ((start_angle + sweep) / 90.).floor() as i32;
    let mut start_t = (start_angle - start_segment as f64 * 90.) / 90.;
    let mut end_t = (start_angle + sweep - end_segment as f64 * 90.) / 90.;
    let delta = if sweep > 0. { 1 } else { -1 };
    if delta < 0 {
        start_t = 1. - start_t;
        end_t = 1. - end_t;
    }
    if fuzzy_is_null(start_t - 1.) {
        start_t = 0.;
        start_segment += delta;
    }
    if fuzzy_is_null(end_t) {
        end_t = 1.;
        end_segment -= delta;
    }
    start_t = t_for_arc_angle(start_t * 90.);
    end_t = t_for_arc_angle(end_t * 90.);
    let split_start = !fuzzy_is_null(start_t);
    let split_end = !fuzzy_is_null(end_t - 1.);
    let end = end_segment + delta;
    if start_segment == end {
        let quadrant = 3 - ((start_segment % 4) + 4) % 4;
        let j = 3 * quadrant as usize;
        return (if delta > 0 { pts[j + 3] } else { pts[j] }, curves);
    }
    let (start_point, end_point) = find_ellipse_coords(rect, start_angle, sweep);
    let mut i = start_segment;
    while i != end {
        let quadrant = 3 - ((i % 4) + 4) % 4;
        let j = 3 * quadrant as usize;
        let mut b = if delta > 0 {
            Bezier::from(pts[j + 3], pts[j + 2], pts[j + 1], pts[j])
        } else {
            Bezier::from(pts[j], pts[j + 1], pts[j + 2], pts[j + 3])
        };
        if start_segment == end_segment && fuzzy_compare(start_t, end_t) {
            return (start_point, curves);
        }
        if i == start_segment {
            if i == end_segment && split_end {
                b = b.on_interval(start_t, end_t);
            } else if split_start {
                b = b.on_interval(start_t, 1.);
            }
        } else if i == end_segment && split_end {
            b = b.on_interval(0., end_t);
        }
        curves.push(b.pt2());
        curves.push(b.pt3());
        curves.push(b.pt4());
        i += delta;
    }
    if let Some(l) = curves.last_mut() {
        *l = end_point;
    }
    (start_point, curves)
}

// ------------------------------------------------------------------ QStroker

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cap {
    Flat,
    Square,
    Round,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Join {
    Miter,
    Bevel,
    Round,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JoinMode {
    Flat,
    Square,
    Miter,
    Round,
    RoundCap,
}

struct Stroker {
    width: f64,
    miter_limit: f64,
    cap: JoinMode,
    join: JoinMode,
    back1: P,
    back2: P,
    out: Vec<(Et, P)>,
}

impl Stroker {
    fn emit_move(&mut self, pt: P) {
        self.back2 = self.back1;
        self.back1 = pt;
        self.out.push((Et::Move, pt));
    }
    fn emit_line(&mut self, pt: P) {
        self.back2 = self.back1;
        self.back1 = pt;
        self.out.push((Et::Line, pt));
    }
    fn emit_cubic(&mut self, c1: P, c2: P, e: P) {
        if c2.x == e.x && c2.y == e.y {
            if c1.x == e.x && c1.y == e.y {
                self.back2 = self.back1;
            } else {
                self.back2 = c1;
            }
        } else {
            self.back2 = c2;
        }
        self.back1 = e;
        self.out.push((Et::Curve, c1));
        self.out.push((Et::CurveData, c2));
        self.out.push((Et::CurveData, e));
    }

    fn join_points(&mut self, focal: P, next: Line, join: JoinMode) {
        if fuzzy_compare(self.back1.x, next.p1.x) && fuzzy_compare(self.back1.y, next.p1.y) {
            return;
        }
        let prev = Line::new(self.back2, self.back1);
        let (ty, isect) = prev.intersects(&next);
        match join {
            JoinMode::Flat => {
                let short = Line::new(prev.p2, next.p1);
                let angle = short.angle_to(&prev);
                if ty == Isect::Bounded || (angle > 90. && !fuzzy_compare(angle, 90.)) {
                    self.emit_line(focal);
                    self.emit_line(next.p1);
                    return;
                }
                self.emit_line(next.p1);
            }
            JoinMode::Miter => {
                let limit = self.width * self.miter_limit;
                let short = Line::new(prev.p2, next.p1);
                let angle = short.angle_to(&prev);
                if ty == Isect::Bounded || (angle > 90. && !fuzzy_compare(angle, 90.)) {
                    self.emit_line(focal);
                    self.emit_line(next.p1);
                    return;
                }
                let miter = Line::new(self.back1, isect);
                if ty == Isect::None || miter.length() > limit {
                    let mut l1 = prev;
                    l1.set_length(limit);
                    l1.translate(prev.dx(), prev.dy());
                    let mut l2 = next;
                    l2.set_length(limit);
                    let (dx, dy) = (-l2.dx(), -l2.dy());
                    l2.translate(dx, dy);
                    self.emit_line(l1.p2);
                    self.emit_line(l2.p1);
                    self.emit_line(next.p1);
                } else {
                    self.emit_line(isect);
                    self.emit_line(next.p1);
                }
            }
            JoinMode::Square => {
                let offset = self.width / 2.;
                let mut l1 = prev;
                let dp = prev.dx() * next.dx() + prev.dy() * next.dy();
                if dp > 0. {
                    l1 = Line::new(prev.p2, prev.p1);
                } else {
                    l1.translate(l1.dx(), l1.dy());
                }
                l1.set_length(offset);
                let mut l2 = Line::new(next.p2, next.p1);
                l2.translate(l2.dx(), l2.dy());
                l2.set_length(offset);
                self.emit_line(l1.p2);
                self.emit_line(l2.p2);
                self.emit_line(l2.p1);
            }
            JoinMode::Round => {
                let offset = self.width / 2.;
                let short = Line::new(prev.p2, next.p1);
                let angle = short.angle_to(&prev);
                if (ty == Isect::Bounded || angle > 90.01) && next.length() > offset {
                    self.emit_line(focal);
                    self.emit_line(next.p1);
                    return;
                }
                let x_axis = Line::new(p(0., 0.), p(1., 0.));
                let l1_on_x = x_axis.angle_to(&prev);
                let l2_on_x = x_axis.angle_to(&next);
                let sweep = (l2_on_x - l1_on_x).abs();
                let (_, curves) = curves_for_arc(
                    (focal.x - offset, focal.y - offset, offset * 2., offset * 2.),
                    l1_on_x + 90.,
                    -sweep,
                );
                for c in curves.chunks(3) {
                    self.emit_cubic(c[0], c[1], c[2]);
                }
                self.emit_line(next.p1);
            }
            JoinMode::RoundCap => {
                let offset = self.width / 2.;
                let mut l1 = prev;
                let dp = prev.dx() * next.dx() + prev.dy() * next.dy();
                if dp > 0. {
                    l1 = Line::new(prev.p2, prev.p1);
                } else {
                    l1.translate(l1.dx(), l1.dy());
                }
                l1.set_length(KAPPA * offset);
                let mut l2 = Line::new(focal, prev.p2);
                let (tx, ty2) = (-l2.dy(), l2.dx());
                l2.translate(tx, ty2);
                l2.set_length(KAPPA * offset);
                self.emit_cubic(l1.p2, l2.p2, l2.p1);
                l2 = Line::new(l2.p1, p(l2.p1.x - l2.dx(), l2.p1.y - l2.dy()));
                l1.translate(next.p1.x - l1.p1.x, next.p1.y - l1.p1.y);
                self.emit_cubic(l2.p2, l1.p2, l1.p1);
            }
        }
    }

    /// qt_stroke_side for line and cubic subpaths. Returns (closed, start tangent).
    fn stroke_side(&mut self, el: &[(Et, P)], cap_first: bool) -> (bool, Line) {
        let start = el[0].1;
        let mut prev = start;
        let mut first = true;
        let offset = self.width / 2.;
        let mut start_tangent = Line::default();
        let mut index = 1;
        while index < el.len() {
            let e = &el[index];
            index += 1;
            if e.0 == Et::Curve {
                let cp2 = el[index].1;
                let end = el[index + 1].1;
                index += 2;
                let bezier = Bezier::from(prev, e.1, cp2, end);
                let shifted = bezier.shifted(offset, (1. / self.width).clamp(0.00025, 0.25) as f32);
                if !shifted.is_empty() {
                    let first_pt = shifted[0].pt1();
                    let mut tangent = bezier.start_tangent();
                    tangent.translate(first_pt.x - prev.x, first_pt.y - prev.y);
                    if first {
                        if cap_first {
                            self.join_points(prev, tangent, self.cap);
                        } else {
                            self.emit_move(first_pt);
                        }
                        start_tangent = tangent;
                        first = false;
                    } else {
                        self.join_points(prev, tangent, self.join);
                    }
                    for b in shifted {
                        self.emit_cubic(b.pt2(), b.pt3(), b.pt4());
                    }
                }
                prev = end;
                continue;
            }
            let mut line = Line::new(prev, e.1);
            if !line.p1.feq(line.p2) {
                let mut normal = line.normal_vector();
                normal.set_length(offset);
                line.translate(normal.dx(), normal.dy());
                if first {
                    if cap_first {
                        self.join_points(prev, line, self.cap);
                    } else {
                        self.emit_move(line.p1);
                    }
                    start_tangent = line;
                    first = false;
                } else {
                    self.join_points(prev, line, self.join);
                }
                self.emit_line(line.p2);
                prev = e.1;
            }
        }
        if fuzzy_compare(start.x, prev.x) && fuzzy_compare(start.y, prev.y) {
            if !first {
                self.join_points(prev, start_tangent, self.join);
            }
            (true, start_tangent)
        } else {
            (false, start_tangent)
        }
    }

    fn process_subpath(&mut self, el: &[(Et, P)]) {
        let (fw_closed, fw_tangent) = self.stroke_side(el, false);
        // QSubpathBackwardIterator: last element first (as MoveTo), the rest as LineTo.
        let mut bw = vec![(Et::Move, el.last().unwrap().1)];
        let mut i = el.len() - 1;
        while i > 0 {
            if el[i].0 == Et::CurveData && i >= 3 {
                bw.extend([
                    (Et::Curve, el[i - 1].1),
                    (Et::CurveData, el[i - 2].1),
                    (Et::CurveData, el[i - 3].1),
                ]);
                i -= 3;
            } else {
                bw.push((Et::Line, el[i - 1].1));
                i -= 1;
            }
        }
        let (bw_closed, _) = self.stroke_side(&bw, !fw_closed);
        if !bw_closed && !fw_tangent.is_null() {
            self.join_points(el[0].1, fw_tangent, self.cap);
        }
    }
}

/// QPaintEngineEx::stroke with a solid pen: the stroke outline (winding fill).
fn stroke_outline(path: &Path, width: f64, cap: Cap, join: Join, miter_limit: f64) -> Vec<(Et, P)> {
    let mut s = Stroker {
        width: if width == 0. { 1. } else { width },
        miter_limit,
        cap: match cap {
            Cap::Flat => JoinMode::Flat,
            Cap::Square => JoinMode::Square,
            Cap::Round => JoinMode::RoundCap,
        },
        join: match join {
            Join::Bevel => JoinMode::Flat,
            Join::Miter => JoinMode::Miter,
            Join::Round => JoinMode::Round,
        },
        back1: P::default(),
        back2: P::default(),
        out: Vec::new(),
    };
    let mut cur: Vec<(Et, P)> = Vec::new();
    for e in &path.el {
        match e.0 {
            Et::Move => {
                if cur.len() > 1 {
                    s.process_subpath(&cur);
                }
                cur.clear();
                cur.push(*e);
            }
            _ => cur.push(*e),
        }
    }
    if cur.len() > 1 {
        s.process_subpath(&cur);
    }
    s.out
}

// ------------------------------------------------------------------ QOutlineMapper

/// QOutlineMapper::convertPath: contours of 26.6 device points (curves flattened in
/// logical space with threshold 0.25 / scale, then transformed).
fn outline(el: &[(Et, P)], scale: f64) -> Vec<Vec<(i64, i64)>> {
    let threshold = if scale == 0. { 0.25 } else { 0.25 / scale };
    let mut elements: Vec<P> = Vec::new();
    let mut types: Vec<Et> = Vec::new();
    let mut subpath_start = 0usize;
    let close = |elements: &mut Vec<P>, types: &mut Vec<Et>, start: usize| {
        if let (Some(&last), Some(&first)) = (elements.last(), elements.get(start)) {
            if !last.feq(first) {
                elements.push(first);
                types.push(Et::Line);
            }
        }
    };
    let n = el.len();
    let mut i = 0;
    while i < n {
        let (t, pt) = el[i];
        match t {
            Et::Move => {
                if i != n - 1 {
                    close(&mut elements, &mut types, subpath_start);
                    subpath_start = elements.len();
                    elements.push(pt);
                    types.push(Et::Move);
                }
            }
            Et::Line => {
                elements.push(pt);
                types.push(Et::Line);
            }
            Et::Curve => {
                if !elements.is_empty() {
                    let b = Bezier::from(*elements.last().unwrap(), pt, el[i + 1].1, el[i + 2].1);
                    let before = elements.len();
                    b.add_to_polygon(&mut elements, threshold);
                    for _ in before..elements.len() {
                        types.push(Et::Line);
                    }
                }
                i += 2;
            }
            Et::CurveData => {}
        }
        i += 1;
    }
    close(&mut elements, &mut types, subpath_start);
    let mut contours: Vec<Vec<(i64, i64)>> = Vec::new();
    for (t, pt) in types.iter().zip(&elements) {
        // QtCore QRoundImpl on SSE2 adds the signed half before truncation.
        // Unlike round(), that addition also rounds near-half values to a tie.
        let qround = |v: f64| (v + 0.5f64.copysign(v)) as i64;
        let fx = qround((pt.x * scale) * 64.);
        let fy = qround((pt.y * scale) * 64.);
        if *t == Et::Move || contours.is_empty() {
            contours.push(Vec::new());
        }
        contours.last_mut().unwrap().push((fx, fy));
    }
    contours
}

// ------------------------------------------------------------------ qgrayraster

const PIXEL_BITS: i64 = 8;
const ONE_PIXEL: i64 = 1 << PIXEL_BITS;

struct Gray {
    cells: HashMap<(i64, i64), (i64, i64)>, // (ey, ex) → (area, cover)
    ex: i64,
    ey: i64,
    area: i64,
    cover: i64,
    invalid: bool,
    x: i64,
    y: i64,
    max_ex: i64,
    min_ey: i64,
    max_ey: i64,
    count_ex: i64,
    count_ey: i64,
}

fn div_mod(a: i64, b: i64) -> (i64, i64) {
    let mut q = a / b;
    let mut r = a % b;
    if r < 0 {
        q -= 1;
        r += b;
    }
    (q, r)
}

impl Gray {
    fn new(w: i64, h: i64) -> Self {
        Self {
            cells: HashMap::new(),
            ex: 0,
            ey: 0,
            area: 0,
            cover: 0,
            invalid: true,
            x: 0,
            y: 0,
            max_ex: w,
            min_ey: 0,
            max_ey: h,
            count_ex: w,
            count_ey: h,
        }
    }
    fn record_cell(&mut self) {
        if self.area | self.cover != 0 {
            let c = self.cells.entry((self.ey, self.ex)).or_insert((0, 0));
            c.0 += self.area;
            c.1 += self.cover;
        }
    }
    fn set_cell(&mut self, ex: i64, ey: i64) {
        let ey = ey - self.min_ey;
        let mut ex = ex.min(self.max_ex);
        if ex < 0 {
            ex = -1;
        }
        if ex != self.ex || ey != self.ey {
            if !self.invalid {
                self.record_cell();
            }
            self.area = 0;
            self.cover = 0;
            self.ex = ex;
            self.ey = ey;
        }
        self.invalid = (ey as u64) >= (self.count_ey as u64) || ex >= self.count_ex;
    }
    fn start_cell(&mut self, ex: i64, ey: i64) {
        let mut ex = ex.min(self.max_ex);
        if ex < 0 {
            ex = -1;
        }
        self.area = 0;
        self.cover = 0;
        self.ex = ex;
        self.ey = ey - self.min_ey;
        self.invalid = false;
        self.set_cell(ex, ey);
    }
    fn render_scanline(&mut self, ey: i64, x1: i64, y1: i64, x2: i64, y2: i64) {
        let mut ex1 = x1 >> PIXEL_BITS;
        let ex2 = x2 >> PIXEL_BITS;
        let mut y1 = y1;
        if y1 == y2 {
            self.set_cell(ex2, ey);
            return;
        }
        let mut fx1 = x1 & (ONE_PIXEL - 1);
        let fx2 = x2 & (ONE_PIXEL - 1);
        if ex1 != ex2 {
            let mut dx = x2 - x1;
            let dy = y2 - y1;
            let (pp, first, incr);
            if dx > 0 {
                pp = (ONE_PIXEL - fx1) * dy;
                first = ONE_PIXEL;
                incr = 1;
            } else {
                pp = fx1 * dy;
                first = 0;
                incr = -1;
                dx = -dx;
            }
            let (mut delta, mut m) = div_mod(pp, dx);
            self.area += (fx1 + first) * delta;
            self.cover += delta;
            y1 += delta;
            ex1 += incr;
            self.set_cell(ex1, ey);
            if ex1 != ex2 {
                let (lift, rem) = div_mod(ONE_PIXEL * dy, dx);
                loop {
                    delta = lift;
                    m += rem;
                    if m >= dx {
                        m -= dx;
                        delta += 1;
                    }
                    self.area += ONE_PIXEL * delta;
                    self.cover += delta;
                    y1 += delta;
                    ex1 += incr;
                    self.set_cell(ex1, ey);
                    if ex1 == ex2 {
                        break;
                    }
                }
            }
            fx1 = ONE_PIXEL - first;
        }
        let dy = y2 - y1;
        self.area += (fx1 + fx2) * dy;
        self.cover += dy;
    }
    fn render_line(&mut self, to_x: i64, to_y: i64) {
        let mut ey1 = self.y >> PIXEL_BITS;
        let ey2 = to_y >> PIXEL_BITS;
        'end: {
            if (ey1 >= self.max_ey && ey2 >= self.max_ey)
                || (ey1 < self.min_ey && ey2 < self.min_ey)
            {
                break 'end;
            }
            let fy1 = self.y & (ONE_PIXEL - 1);
            let fy2 = to_y & (ONE_PIXEL - 1);
            if ey1 == ey2 {
                self.render_scanline(ey1, self.x, fy1, to_x, fy2);
                break 'end;
            }
            let dx = to_x - self.x;
            let mut dy = to_y - self.y;
            if dx == 0 {
                let ex = self.x >> PIXEL_BITS;
                let two_fx = (self.x & (ONE_PIXEL - 1)) << 1;
                let first = if dy > 0 { ONE_PIXEL } else { 0 };
                let mut delta = first - fy1;
                self.area += two_fx * delta;
                self.cover += delta;
                delta = first + first - ONE_PIXEL;
                let area = two_fx * delta;
                let max_ey1 = self.count_ey + self.min_ey;
                if dy < 0 {
                    if ey1 > max_ey1 {
                        ey1 = if max_ey1 > ey2 { max_ey1 } else { ey2 };
                        self.set_cell(ex, ey1);
                    } else {
                        ey1 -= 1;
                        self.set_cell(ex, ey1);
                    }
                    while ey1 > ey2 && ey1 >= self.min_ey {
                        self.area += area;
                        self.cover += delta;
                        ey1 -= 1;
                        self.set_cell(ex, ey1);
                    }
                    if ey1 != ey2 {
                        ey1 = ey2;
                        self.set_cell(ex, ey1);
                    }
                } else {
                    if ey1 < self.min_ey {
                        ey1 = if self.min_ey < ey2 { self.min_ey } else { ey2 };
                        self.set_cell(ex, ey1);
                    } else {
                        ey1 += 1;
                        self.set_cell(ex, ey1);
                    }
                    while ey1 < ey2 && ey1 < max_ey1 {
                        self.area += area;
                        self.cover += delta;
                        ey1 += 1;
                        self.set_cell(ex, ey1);
                    }
                    if ey1 != ey2 {
                        ey1 = ey2;
                        self.set_cell(ex, ey1);
                    }
                }
                let delta = fy2 - ONE_PIXEL + first;
                self.area += two_fx * delta;
                self.cover += delta;
                break 'end;
            }
            let (pp, first, incr);
            if dy > 0 {
                pp = (ONE_PIXEL - fy1) * dx;
                first = ONE_PIXEL;
                incr = 1;
            } else {
                pp = fy1 * dx;
                first = 0;
                incr = -1;
                dy = -dy;
            }
            let (delta, mut m) = div_mod(pp, dy);
            let mut x = self.x + delta;
            self.render_scanline(ey1, self.x, fy1, x, first);
            ey1 += incr;
            self.set_cell(x >> PIXEL_BITS, ey1);
            if ey1 != ey2 {
                let (lift, rem) = div_mod(ONE_PIXEL * dx, dy);
                loop {
                    let mut delta = lift;
                    m += rem;
                    if m >= dy {
                        m -= dy;
                        delta += 1;
                    }
                    let x2 = x + delta;
                    self.render_scanline(ey1, x, ONE_PIXEL - first, x2, first);
                    x = x2;
                    ey1 += incr;
                    self.set_cell(x >> PIXEL_BITS, ey1);
                    if ey1 == ey2 {
                        break;
                    }
                }
            }
            self.render_scanline(ey1, x, ONE_PIXEL - first, to_x, fy2);
        }
        self.x = to_x;
        self.y = to_y;
    }
    fn move_to(&mut self, x26: i64, y26: i64) {
        if !self.invalid {
            self.record_cell();
        }
        let (x, y) = (x26 * (ONE_PIXEL >> 6), y26 * (ONE_PIXEL >> 6));
        self.start_cell(x >> PIXEL_BITS, y >> PIXEL_BITS);
        self.x = x;
        self.y = y;
    }
    fn line_to(&mut self, x26: i64, y26: i64) {
        self.render_line(x26 * (ONE_PIXEL >> 6), y26 * (ONE_PIXEL >> 6));
    }
}

/// Rasterise contours (non-zero winding) and call `span(x, y, len, coverage)`.
fn gray_fill(
    contours: &[Vec<(i64, i64)>],
    w: usize,
    h: usize,
    mut span: impl FnMut(i64, i64, i64, u32),
) {
    let mut g = Gray::new(w as i64, h as i64);
    for c in contours {
        if c.is_empty() {
            continue;
        }
        g.move_to(c[0].0, c[0].1);
        for pt in &c[1..] {
            g.line_to(pt.0, pt.1);
        }
        g.line_to(c[0].0, c[0].1);
    }
    if !g.invalid {
        g.record_cell();
    }
    let mut rows: HashMap<i64, Vec<(i64, i64, i64)>> = HashMap::new();
    for ((ey, ex), (area, cover)) in g.cells {
        rows.entry(ey).or_default().push((ex, area, cover));
    }
    let coverage = |area: i64| -> u32 {
        let mut c = area >> (PIXEL_BITS * 2 + 1 - 8);
        if c < 0 {
            c = -c;
        }
        if c >= 256 {
            c = 255;
        }
        c as u32
    };
    for (ey, mut cells) in rows {
        cells.sort_by_key(|c| c.0);
        let mut cover = 0i64;
        let mut x = 0i64;
        for (cx, carea, ccover) in cells {
            if cx > x && cover != 0 {
                let cv = coverage(cover * (ONE_PIXEL * 2));
                if cv != 0 {
                    span(x, ey, cx - x, cv);
                }
            }
            cover += ccover;
            let area = cover * (ONE_PIXEL * 2) - carea;
            if area != 0 && cx >= 0 {
                let cv = coverage(area);
                if cv != 0 {
                    span(cx, ey, 1, cv);
                }
            }
            x = cx + 1;
        }
        if g.count_ex > x && cover != 0 {
            let cv = coverage(cover * (ONE_PIXEL * 2));
            if cv != 0 {
                span(x, ey, g.count_ex - x, cv);
            }
        }
    }
}

// ------------------------------------------------------------------ colours / blending

/// BYTE_MUL (qdrawhelper_p.h, 64-bit variant): per channel (c·a + (c·a >> 8) + 0x80) >> 8.
fn byte_mul(x: u32, a: u32) -> u32 {
    let mut t = ((x as u64) | ((x as u64) << 24)) & 0x00ff00ff00ff00ff;
    t *= a as u64;
    t = (t + ((t >> 8) & 0x00ff00ff00ff00ff) + 0x0080008000800080) >> 8;
    t &= 0x00ff00ff00ff00ff;
    (t as u32) | ((t >> 24) as u32)
}

/// comp_func_solid_SourceOver for one span.
fn blend_solid(dst: &mut [u32], color: u32, const_alpha: u32) {
    if (const_alpha & (color >> 24)) == 255 {
        dst.fill(color);
        return;
    }
    let c = if const_alpha != 255 {
        byte_mul(color, const_alpha)
    } else {
        color
    };
    let ia = (!c) >> 24;
    for d in dst {
        *d = c.wrapping_add(byte_mul(*d, ia));
    }
}

/// QColor as stored by QML (`Qt.rgba`/hex): 16-bit components, qRound(f · 65535).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgba64 {
    r: u16,
    g: u16,
    b: u16,
    a: u16,
}
impl Rgba64 {
    pub fn from_color(c: Color) -> Self {
        let q = |v: f32| (v * 65535.).round().clamp(0., 65535.) as u16;
        Self {
            r: q(c.r),
            g: q(c.g),
            b: q(c.b),
            a: q(c.a),
        }
    }
    /// QRgba64::premultiplied (64-bit integer form).
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
        let m = |c: u16| {
            let x = c as u64 * a;
            ((x + (x >> 16) + 0x8000) >> 16) as u16
        };
        Self {
            r: m(self.r),
            g: m(self.g),
            b: m(self.b),
            a: self.a,
        }
    }
    /// QRgba64::toArgb32 (div_257 rounding).
    fn to_argb32(self) -> u32 {
        let d = |x: u16| {
            let x = x as u32 + 128;
            (x - (x >> 8)) >> 8
        };
        (d(self.a) << 24) | (d(self.r) << 16) | (d(self.g) << 8) | d(self.b)
    }
}

/// QGradientCache::generateGradientColorTable for two stops (ColorInterpolation).
fn gradient_table(c0: Rgba64, c1: Rgba64) -> Vec<u32> {
    const N: usize = 1024;
    let (first, second) = (c0.premultiplied(), c1.premultiplied());
    let first_index = 0usize;
    let second_index = N - 1;
    let mut table = vec![0u32; N];
    let (mut r, mut g, mut b, mut a) = (
        (first.r as u32) << 16,
        (first.g as u32) << 16,
        (first.b as u32) << 16,
        (first.a as u32) << 16,
    );
    let (r2, g2, b2, a2) = (
        (second.r as u32) << 16,
        (second.g as u32) << 16,
        (second.b as u32) << 16,
        (second.a as u32) << 16,
    );
    let mut i = 0usize;
    while i <= first_index.min(N) {
        table[i] = first.to_argb32();
        i += 1;
    }
    if i < second_index {
        let rec = 1. / (second_index - first_index) as f64;
        let dl = |s: u32, f: u32| ((s as f64 - f as f64) * rec).round() as i64;
        let (dr, dg, db, da) = (dl(r2, r), dl(g2, g), dl(b2, b), dl(a2, a));
        r += 1 << 15;
        g += 1 << 15;
        b += 1 << 15;
        a += 1 << 15;
        while i < second_index.min(N) {
            r = (r as i64 + dr) as u32;
            g = (g as i64 + dg) as u32;
            b = (b as i64 + db) as u32;
            a = (a as i64 + da) as u32;
            table[i] = Rgba64 {
                r: (r >> 16) as u16,
                g: (g >> 16) as u16,
                b: (b >> 16) as u16,
                a: (a >> 16) as u16,
            }
            .to_argb32();
            i += 1;
        }
    }
    while i < N {
        table[i] = second.to_argb32();
        i += 1;
    }
    table
}

// ------------------------------------------------------------------ QImage + painter

#[derive(Clone, Copy, Debug)]
pub enum Brush {
    Solid(Rgba64),
    /// Vertical QLinearGradient from (·, y0) to (·, y1) in logical coordinates.
    VerticalGradient {
        y0: f64,
        y1: f64,
        c0: Rgba64,
        c1: Rgba64,
    },
}

/// A QImage ARGB32_Premultiplied with a QPainter whose transform is scale(dpr).
pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub dpr: f64,
    pub px: Vec<u32>,
}

impl Canvas {
    pub fn new(w: usize, h: usize, dpr: f64) -> Self {
        Self {
            w,
            h,
            dpr,
            px: vec![0; w * h],
        }
    }

    fn blend_span(&mut self, x: i64, y: i64, len: i64, coverage: u32, color: u32) {
        if y < 0 || y >= self.h as i64 {
            return;
        }
        let x0 = x.max(0) as usize;
        let x1 = ((x + len) as usize).min(self.w);
        if x0 >= x1 {
            return;
        }
        let row = y as usize * self.w;
        blend_solid(&mut self.px[row + x0..row + x1], color, coverage);
    }

    fn fill_contours(&mut self, contours: &[Vec<(i64, i64)>], brush: Brush) {
        let (w, h) = (self.w, self.h);
        let mut spans = Vec::new();
        gray_fill(contours, w, h, |x, y, len, c| spans.push((x, y, len, c)));
        match brush {
            Brush::Solid(c) => {
                let color = c.premultiplied().to_argb32();
                for (x, y, len, cv) in spans {
                    self.blend_span(x, y, len, cv, color);
                }
            }
            Brush::VerticalGradient { y0, y1, c0, c1 } => {
                let table = gradient_table(c0, c1);
                // getLinearGradientValues + calculate_fixed_gradient_factors; the span
                // matrix is inverse(translate(1/65536) · scale(dpr)).
                let l = (y1 - y0) * (y1 - y0);
                let (ldy, off) = if l != 0. {
                    ((y1 - y0) / l, -((y1 - y0) / l) * y0)
                } else {
                    (0., 0.)
                };
                let m22 = 1. / self.dpr;
                let mdy = -1. / 65536.;
                const GSS: f64 = 1023.;
                const FIXPT: f64 = 256.;
                let yinc = (ldy * m22 * GSS * FIXPT) as i64;
                let roff = ((ldy * (m22 * 0.5 + mdy) + off) * GSS * FIXPT) as i64;
                for (x, y, len, cv) in spans {
                    let fixed = yinc * y + roff;
                    let ipos = ((fixed + 128) >> 8).clamp(0, 1023) as usize;
                    self.blend_span(x, y, len, cv, table[ipos]);
                }
            }
        }
    }

    /// QPainter::fillPath (QRasterPaintEngine::fill, non-rectangle shapes).
    pub fn fill_path(&mut self, path: &Path, brush: Brush) {
        if path.is_empty() {
            return;
        }
        let contours = outline(&path.el, self.dpr);
        self.fill_contours(&contours, brush);
    }

    /// QPainter::strokePath with a non-cosmetic solid pen.
    pub fn stroke_path(
        &mut self,
        path: &Path,
        width: f64,
        cap: Cap,
        join: Join,
        miter_limit: f64,
        color: Rgba64,
    ) {
        if path.is_empty() {
            return;
        }
        let device_w = width * self.dpr;
        if device_w <= 1. && path.el.iter().all(|e| matches!(e.0, Et::Move | Et::Line)) {
            // fast_pen: QCosmeticStroker (FastDraw, antialiased)
            let opacity = ((256. * width * self.dpr) as i64).clamp(0, 256) as u32;
            let pm = color.premultiplied();
            let m = |c: u16| ((c as u32 * opacity) >> 8) as u16;
            let c = Rgba64 {
                r: m(pm.r),
                g: m(pm.g),
                b: m(pm.b),
                a: m(pm.a),
            }
            .to_argb32();
            let caps = if cap != Cap::Flat { 3 } else { 0 };
            let mut start = P::default();
            let mut prev = P::default();
            for (i, e) in path.el.iter().enumerate() {
                let pt = e.1.mul(self.dpr);
                match e.0 {
                    Et::Move => {
                        start = pt;
                        prev = pt;
                    }
                    Et::Line => {
                        let last = path.el.get(i + 1).is_none_or(|n| n.0 == Et::Move);
                        let first = path.el[i - 1].0 == Et::Move;
                        let closed = start.feq(pt) && last;
                        let mut k = 0;
                        if !closed && caps != 0 {
                            if first {
                                k |= 1;
                            }
                            if last {
                                k |= 2;
                            }
                        }
                        self.cosmetic_line_aa(prev, pt, k, c);
                        prev = pt;
                    }
                    _ => debug_assert!(false, "curves are not cosmetic-stroked by the chart"),
                }
            }
            return;
        }
        if cap != Cap::Round && path.is_lines() {
            // QRasterPaintEngine::stroke: non_complex_pen && LinesHint → rasterizeLine
            let c = color.premultiplied().to_argb32();
            for pair in path.el.chunks(2) {
                if pair.len() == 2 {
                    let a = pair[0].1.mul(self.dpr);
                    let b = pair[1].1.mul(self.dpr);
                    if a.feq(b) {
                        continue;
                    }
                    let len = Line::new(a, b).length();
                    self.rasterize_line(a, b, device_w / len, cap == Cap::Square, c);
                }
            }
            return;
        }
        let out = stroke_outline(path, width, cap, join, miter_limit);
        if out.is_empty() {
            return;
        }
        let contours = outline(&out, self.dpr);
        self.fill_contours(&contours, Brush::Solid(color));
    }

    /// QCosmeticStroker drawLineAA<drawPixelARGB32> (caps: 1 = begin, 2 = end).
    fn cosmetic_line_aa(&mut self, a: P, b: P, caps: i32, color: u32) {
        let (xmin, xmax) = (-1., self.w as f64 + 1.);
        let (ymin, ymax) = (-1., self.h as f64 + 1.);
        if a.x < xmin && b.x <= xmin
            || a.x > xmax && b.x >= xmax
            || a.y < ymin && b.y <= ymin
            || a.y > ymax && b.y >= ymax
        {
            return;
        }
        // unported qClipLine: pixels outside the image are dropped by `pix`
        debug_assert!(
            a.x >= xmin
                && a.x <= xmax
                && b.x >= xmin
                && b.x <= xmax
                && a.y >= ymin
                && a.y <= ymax
                && b.y >= ymin
                && b.y <= ymax,
            "clipped cosmetic lines are not ported"
        );
        let (w, h) = (self.w as i64, self.h as i64);
        let mut pix = |x: i64, y: i64, cov: i64| {
            if x < 0 || x >= w || y < 0 || y >= h {
                return;
            }
            let i = (y * w + x) as usize;
            let c = byte_mul(color, cov as u32);
            self.px[i] = c.wrapping_add(byte_mul(self.px[i], (!c) >> 24));
        };
        let f26 = |v: f64| (v * 64.) as i64;
        let (mut x1, mut y1, mut x2, mut y2) = (f26(a.x), f26(a.y), f26(b.x), f26(b.y));
        let (dx, dy) = (x2 - x1, y2 - y1);
        let swap_caps = |c: i32| ((c & 1) << 1) | ((c & 2) >> 1);
        let mut caps = caps;
        if dx.abs() < dy.abs() {
            let xinc = dx * (1 << 16) / dy;
            if y1 > y2 {
                std::mem::swap(&mut y1, &mut y2);
                std::mem::swap(&mut x1, &mut x2);
                caps = swap_caps(caps);
            }
            let mut x = (x1 - 32) * (1 << 10);
            x -= (((y1 & 63) - 32) * xinc) >> 6;
            if caps & 1 != 0 {
                y1 -= 32;
                x -= xinc >> 1;
            }
            if caps & 2 != 0 {
                y2 += 32;
            }
            let mut y = y1 >> 6;
            let ys = y2 >> 6;
            let (a_start, a_end) = if y == ys {
                (y2 - y1, 0)
            } else {
                (64 - (y1 & 63), y2 & 63)
            };
            let al = |x: i64| (x >> 8) & 0xff;
            pix(x >> 16, y, ((255 - al(x)) * a_start) >> 6);
            pix((x >> 16) + 1, y, (al(x) * a_start) >> 6);
            x += xinc;
            y += 1;
            while y < ys {
                pix(x >> 16, y, 255 - al(x));
                pix((x >> 16) + 1, y, al(x));
                x += xinc;
                y += 1;
            }
            if a_end != 0 {
                pix(x >> 16, y, ((255 - al(x)) * a_end) >> 6);
                pix((x >> 16) + 1, y, (al(x) * a_end) >> 6);
            }
        } else {
            if dx == 0 {
                return;
            }
            let yinc = dy * (1 << 16) / dx;
            if x1 > x2 {
                std::mem::swap(&mut x1, &mut x2);
                std::mem::swap(&mut y1, &mut y2);
                caps = swap_caps(caps);
            }
            let mut y = (y1 - 32) * (1 << 10);
            y -= (((x1 & 63) - 32) * yinc) >> 6;
            if caps & 1 != 0 {
                x1 -= 32;
                y -= yinc >> 1;
            }
            if caps & 2 != 0 {
                x2 += 32;
            }
            let mut x = x1 >> 6;
            let xs = x2 >> 6;
            let (a_start, a_end) = if x == xs {
                (x2 - x1, 0)
            } else {
                (64 - (x1 & 63), x2 & 63)
            };
            let al = |y: i64| (y >> 8) & 0xff;
            pix(x, y >> 16, ((255 - al(y)) * a_start) >> 6);
            pix(x, (y >> 16) + 1, (al(y) * a_start) >> 6);
            y += yinc;
            x += 1;
            while x < xs {
                pix(x, y >> 16, 255 - al(y));
                pix(x, (y >> 16) + 1, al(y));
                y += yinc;
                x += 1;
            }
            if a_end != 0 {
                pix(x, y >> 16, ((255 - al(y)) * a_end) >> 6);
                pix(x, (y >> 16) + 1, (al(y) * a_end) >> 6);
            }
        }
    }

    /// QRasterizer::rasterizeLine, antialiased, for horizontal lines (the only kind the
    /// chart draws through this path).
    fn rasterize_line(&mut self, a: P, b: P, width: f64, square: bool, color: u32) {
        unsafe extern "C" {
            fn hyprv_qt_raster_line(
                ax: f64,
                ay: f64,
                bx: f64,
                by: f64,
                width: f64,
                square: bool,
                w: i32,
                h: i32,
                callback: unsafe extern "C" fn(*mut std::ffi::c_void, i32, i32, i32, u32),
                cookie: *mut std::ffi::c_void,
            );
        }
        unsafe extern "C" fn span(cookie: *mut std::ffi::c_void, x: i32, y: i32, len: i32, c: u32) {
            // SAFETY: synchronous C++ callback, cookie is this local Vec for its entire call.
            let spans = unsafe { &mut *cookie.cast::<Vec<(i32, i32, i32, u32)>>() };
            spans.push((x, y, len, c));
        }
        let mut spans = Vec::<(i32, i32, i32, u32)>::new();
        // SAFETY: arguments are values; callback/cookie live until this synchronous call returns.
        unsafe {
            hyprv_qt_raster_line(
                a.x,
                a.y,
                b.x,
                b.y,
                width,
                square,
                self.w as i32,
                self.h as i32,
                span,
                (&mut spans as *mut Vec<_>).cast(),
            );
        }
        for (x, y, len, c) in spans {
            self.blend_span(x as i64, y as i64, len as i64, c, color);
        }
    }

    pub fn to_image(&self) -> Option<sk::Image> {
        let bytes: Vec<u8> = self
            .px
            .iter()
            .flat_map(|&c| [(c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as u8])
            .collect();
        let info = sk::ImageInfo::new(
            (self.w as i32, self.h as i32),
            sk::ColorType::RGBA8888,
            sk::AlphaType::Premul,
            None,
        );
        sk::images::raster_from_data(&info, sk::Data::new_copy(&bytes), self.w * 4)
    }
}

#[cfg(test)]
mod spectrum_outline_probe {
    use super::*;
    #[test]
    fn first_bar_outline_matches_independent_qt_mapper() {
        let expected:Vec<Vec<(i64,i64)>>=serde_json::from_str(include_str!("../tests/fixtures/qt-spectrum-outline.json")).unwrap();
        for (case,active) in [false,true].into_iter().enumerate() {
            let mut ratio=if active {0.18f64.max((0.12f64.powf(0.62)*2.05).min(1.))} else {0.18+0.34*((6.44f64.sin()+1.)/2.).powf(1.45)};
            ratio=0.18+(ratio.min(1.)-0.18)*0.65;
            let h=(22.*0.98*ratio).max(3.);let y=11.-h/2.;
            let mut path=Path::new();path.move_to(p(1.,y));
            let quad=|path:&mut Path,c:P,e:P| {let prev=path.last();path.cubic_to(p((prev.x+2.*c.x)/3.,(prev.y+2.*c.y)/3.),p((e.x+2.*c.x)/3.,(e.y+2.*c.y)/3.),e);};
            quad(&mut path,p(2.,y),p(2.,y+1.));path.line_to(p(2.,y+h-1.));quad(&mut path,p(2.,y+h),p(1.,y+h));
            quad(&mut path,p(0.,y+h),p(0.,y+h-1.));path.line_to(p(0.,y+1.));quad(&mut path,p(0.,y),p(1.,y));path.close_subpath();
            assert_eq!(outline(&path.el,2.),vec![expected[case].clone()]);
        }
    }
}
