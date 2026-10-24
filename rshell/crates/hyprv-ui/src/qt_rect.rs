// Copyright (C) 2016 The Qt Company Ltd.
// Copyright (C) 2026 HyprV contributors.
// SPDX-License-Identifier: LGPL-3.0-only
//! Qt Quick vertex-antialiased rectangles. Adapted from Qt 6
//! src/quick/scenegraph/qsgbasicinternalrectanglenode.cpp and
//! shaders_ng/smoothcolor.vert/.frag. This is geometry AA, not path coverage AA.
//!
//! Integration: call `draw` for Content::Rect in QtQuick mode. Unsupported fills
//! or asymmetric corners return false. No Qt dependency, I/O, clock or shader
//! compiler. CPU rasterization also provides a deterministic offscreen oracle;
//! `Mesh` exposes the same transformed vertices for a future GPU backend.
use std::{collections::HashMap, rc::Rc};
use crate::{
    animation::Color,
    geometry::Rect,
    scene::{Fill, Rectangle},
    skia as sk,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RectMode {
    #[default]
    Skia,
    QtQuick,
}
#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub position: [f32; 2],
    pub offset: [f32; 2],
    pub color: [u8; 4],
}
#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub triangles: Vec<[u16; 3]>,
    /// Canonical Qt triangle strip, including its degenerate connectors.
    pub indices: Vec<u16>,
}
pub struct Raster {
    pub image: sk::Image,
    pub bounds: sk::IRect,
}

fn byte(v: f32) -> u8 {
    (v.clamp(0., 1.) * 255.).round() as u8
}
fn premul(c: Color) -> [u8; 4] {
    // QColor stores 16-bit components before colorToColor4ub's float conversion.
    let q = |v: f32| (v.clamp(0., 1.) * 65535.).round() / 65535.;
    let a = q(c.a);
    [
        byte(q(c.r) * a),
        byte(q(c.g) * a),
        byte(q(c.b) * a),
        byte(a),
    ]
}
fn fast_trig(x: f32) -> (f32, f32) {
    // qFastSin/qFastCos, QtCore/qmath.h. Sine table entries are qreal.
    let x = x as f64;
    let i = (x * (128. / std::f64::consts::PI)) as i32;
    let d = x - i as f64 * (std::f64::consts::PI / 128.);
    let s = (i as f64 * std::f64::consts::PI / 128.).sin();
    let c = ((i + 64) as f64 * std::f64::consts::PI / 128.).sin();
    (
        (s + (c - 0.5 * s * d) * d) as f32,
        (c - (s + 0.5 * c * d) * d) as f32,
    )
}
impl Mesh {
    fn vertex(&mut self, p: [f32; 2], o: [f32; 2], c: [u8; 4]) -> u16 {
        let i = self.vertices.len() as u16;
        self.vertices.push(Vertex {
            position: p,
            offset: o,
            color: c,
        });
        i
    }
    fn strip(&mut self, indices: &[u16]) {
        self.indices.extend_from_slice(indices);
        for (i,t) in indices.windows(3).enumerate() {
            if t[0] != t[1] && t[0] != t[2] && t[1] != t[2] {
                self.triangles.push(if i%2==0 {[t[0], t[1], t[2]]}else{[t[1], t[0], t[2]]});
            }
        }
    }
}
/// Solid uniform-radius subset of Qt's rectangle node; pen alignment is Qt's
/// default `aligned=true` and the vertex AA is enabled, as in QML Rectangle.
pub fn tessellate(bounds: Rect, rect: &Rectangle) -> Option<Mesh> {
    tessellate_with_size(bounds, rect, None)
}
/// Same mesh, retaining QRectF's qreal edges until corner-center conversion.
pub fn tessellate_qreal(bounds: Rect, rect: &Rectangle, size: (f64,f64)) -> Option<Mesh> {
    if !size.0.is_finite() || !size.1.is_finite() || size.0 as f32 != bounds.width || size.1 as f32 != bounds.height { return None; }
    tessellate_with_size(bounds, rect, Some(size))
}
fn tessellate_with_size(bounds: Rect, rect: &Rectangle, size: Option<(f64,f64)>) -> Option<Mesh> {
    let Fill::Solid(fill) = rect.fill else {
        return None;
    };
    if !rect
        .radii
        .iter()
        .all(|v| v.is_finite() && (*v - rect.radii[0]).abs() < 1e-6)
    {
        return None;
    }
    let (w, h) = (bounds.width, bounds.height);
    if !w.is_finite() || !h.is_finite() || w < 0. || h < 0. {
        return None;
    }
    if w == 0. || h == 0. {
        return Some(Mesh::default());
    }
    let pen = rect
        .border
        .as_ref()
        .map_or(0., |b| b.width.max(0.).min(w.min(h) * 0.5))
        .round();
    let fc = premul(fill);
    let bc = rect.border.as_ref().map_or([0; 4], |b| premul(b.color));
    let radius = rect.radii[0].max(0.).min(w.min(h) * 0.4999);
    let rounded = radius > 0.5;
    let radius_path = rect.radii[0] > 0.;
    let r = if rounded { radius } else { 0. };
    let ir = (r - pen).max(0.01);
    let delta = w.min(h) * 0.5;
    let segments = if rounded {
        ((r * (std::f32::consts::PI / 6.)).ceil() as usize).clamp(3, 18)
    } else {
        0
    };
    let mut mesh = Mesh::default();
    let mut rows: Vec<[u16; 8]> = vec![];
    for part in 0..2 {
        let mut cos = if part == 0 { 1. } else { 0. };
        let mut sin = if part == 0 { 0. } else { 1. };
        let (sin_step, cos_step) = fast_trig(std::f32::consts::FRAC_PI_2 / segments.max(1) as f32);
        for index in 0..=if radius_path { segments + 1 } else { 0 } {
            let (y, xl, xr, oy, oxl, oxr) = if radius_path && !rounded {
                // Qt's rounded-node branch keeps minimum inner radius0.01 even
                // after clamping small outer radii to0. Its paired left/right
                // samples remain distinct; a thin separator is not radius=0.
                if part==0 {
                    (pen, pen+if index==0 {ir}else{0.}, w-pen-if index==0 {0.}else{ir},0.,0.,w)
                } else {
                    (h-ir-pen,pen,w-pen,h,0.,w)
                }
            } else if rounded {
                let cy = if let Some((_,height))=size { if part==0 {(ir as f64+pen as f64)as f32}else{(height-ir as f64-pen as f64)as f32} } else if part == 0 { ir + pen } else { h - ir - pen };
                let ocy = if let Some((_,height))=size {if part==0 {r}else{(height-r as f64)as f32}} else if part == 0 { r } else { h - r };
                let left=if size.is_some(){(ir as f64+pen as f64)as f32}else{ir+pen};
                let right=if let Some((width,_))=size{(width-ir as f64-pen as f64)as f32}else{w-ir-pen};
                let outer_right=if let Some((width,_))=size{(width-r as f64)as f32}else{w-r};
                (
                    cy - ir * cos,
                    left - ir * sin,
                    right + ir * sin,
                    ocy - r * cos,
                    r - r * sin,
                    outer_right + r * sin,
                )
            } else {
                let y = if part == 0 { pen } else { h - pen };
                let oy = if part == 0 { 0. } else { h };
                (y, pen, w - pen, oy, 0., w)
            };
            let dp = if part == 0 {
                (delta - y).max(0.)
            } else {
                (h - y - delta).min(0.)
            };
            let outside = if part == 0 { -delta } else { delta };
            let v = |x: f32, y: f32| [x + bounds.x, y + bounds.y];
            let mut row = [0; 8];
            row[0] = mesh.vertex(v(xr, y), [w - xr - delta, dp], fc);
            row[1] = mesh.vertex(v(xl, y), [delta - xl, dp], fc);
            let (ix, iy, ox, oyoff) = if rounded {
                (
                    0.49 * pen * sin,
                    -0.49 * pen * cos,
                    -0.49 * pen * sin,
                    0.49 * pen * cos,
                )
            } else {
                (
                    0.49 * pen,
                    0.49 * pen * if part == 0 { -1. } else { 1. },
                    -0.49 * pen,
                    0.49 * pen * if part == 0 { 1. } else { -1. },
                )
            };
            if pen > 0. {
                row[2] = mesh.vertex(v(xr, y), [ix, iy], bc);
                row[3] = mesh.vertex(v(xl, y), [-ix, iy], bc);
                row[4] = mesh.vertex(v(oxr, oy), [ox, oyoff], bc);
                row[5] = mesh.vertex(v(oxl, oy), [-ox, oyoff], bc);
                row[6] = mesh.vertex(v(oxr, oy), [delta, outside], [0; 4]);
                row[7] = mesh.vertex(v(oxl, oy), [-delta, outside], [0; 4]);
            } else {
                row[2] = mesh.vertex(v(xr, y), [delta, outside], [0; 4]);
                row[3] = mesh.vertex(v(xl, y), [-delta, outside], [0; 4]);
            }
            rows.push(row);
            if radius_path && index == 0 {
                continue;
            }
            let old = cos;
            cos = cos * cos_step - sin * sin_step;
            sin = ((sin * cos_step) as f64 + old as f64 * sin_step as f64) as f32;
        }
    }
    if fc[3] > 0 {
        mesh.strip(&rows.iter().flat_map(|r| [r[0], r[1]]).collect::<Vec<_>>());
    }
    for (inner, outer) in if pen > 0. {
        vec![(0, 2), (2, 4), (4, 6)]
    } else {
        vec![(0, 2)]
    } {
        let last=rows.last().unwrap();
        let mut indices = vec![last[inner+1],last[outer+1]];
        for r in rows.iter().rev() {
            indices.extend([r[inner], r[outer]]);
        }
        for r in &rows {
            indices.extend([r[inner + 1], r[outer + 1]]);
        }
        mesh.strip(&indices);
    }
    Some(mesh)
}
/// smoothcolor.vert for affine 2D transforms: each offset axis is independently
/// clamped to half a physical pixel, before the two displacements are added.
pub fn device_vertex(vertex: Vertex, m: &sk::Matrix) -> ([f32; 2], [u8; 4]) {
    let p = m.map_point((vertex.position[0], vertex.position[1]));
    let mut p = [p.x, p.y];
    for (offset, direction) in [
        (vertex.offset[0], [m.scale_x(), m.skew_y()]),
        (vertex.offset[1], [m.skew_x(), m.scale_y()]),
    ] {
        let d = [direction[0] * offset, direction[1] * offset];
        let length = d[0].hypot(d[1]);
        if length > 0. {
            let k = (0.5 / length).min(1.);
            p[0] += d[0] * k;
            p[1] += d[1] * k;
        }
    }
    (p, vertex.color)
}
/// GL rasteriser sub-pixel precision (8 bits on Mesa and every desktop GPU).
const SUBPIXEL: f32 = 256.;

/// Which edges own pixel centres that lie exactly on them, as the sign of a vanishing
/// sample offset `(x, y)` in y-down device space: `(1, 1)` owns left and top edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TieRule {
    pub x: i8,
    pub y: i8,
}
/// The tie rule the CPU rasteriser uses: GL's top-left rule in its y-up window space,
/// i.e. left and bottom edges in y-down device space. Measured on the GPU (NVIDIA, Skia
/// GL drawing the same mesh): this rule matches it to ≤2/255 with no coverage
/// differences; the other three each disagree on hundreds of edge pixels. Parity
/// references do not distinguish the rules (ties only change non-AA edges at exact
/// half-pixel positions).
pub const TIE_RULE: TieRule = TieRule { x: 1, y: -1 };

/// One Qt rectangle mesh transformed to device space: fixed-point coverage positions
/// and floating-point positions for colour interpolation.
struct DeviceMesh {
    fixed: Vec<[i64; 2]>,
    float: Vec<[f32; 2]>,
    colors: Vec<[u8; 4]>,
}

fn device_mesh(mesh: &Mesh, matrix: &sk::Matrix, antialias: bool) -> DeviceMesh {
    let mut d = DeviceMesh {
        fixed: Vec::with_capacity(mesh.vertices.len()),
        float: Vec::with_capacity(mesh.vertices.len()),
        colors: Vec::with_capacity(mesh.vertices.len()),
    };
    for v in &mesh.vertices {
        let mut v = *v;
        if !antialias {
            v.offset = [0., 0.];
        }
        let (p, c) = device_vertex(v, matrix);
        d.fixed.push(p.map(|c| (c * SUBPIXEL).round_ties_even() as i64));
        d.float.push(p);
        d.colors.push(c);
    }
    d
}

/// Rasterises a tessellated mesh into a premultiplied image: vertex positions snapped to
/// 1/256 px for coverage (exact integer edge functions, pixel centres, `rule` on ties),
/// colours interpolated from the unsnapped positions, src-over per triangle in draw
/// order with 8-bit rounding, the item opacity applied to the interpolated colour.
pub fn rasterize_mesh(mesh: &Mesh, matrix: &sk::Matrix, opacity: f32, antialias: bool, rule: TieRule) -> Option<Raster> {
    rasterize_mesh_window(mesh, matrix, opacity, antialias, rule, None).flatten()
}
/// [`rasterize_mesh`] restricted to the device pixels inside `window`: every pixel there is
/// bit-identical to the full raster (pixels are computed independently; interpolation
/// stays relative to the full raster's origin). `Some(None)` = nothing inside the window.
pub fn rasterize_mesh_window(mesh: &Mesh, matrix: &sk::Matrix, opacity: f32, antialias: bool, rule: TieRule, window: Option<sk::IRect>) -> Option<Option<Raster>> {
    if !opacity.is_finite() {
        return None;
    }
    let c = coverage(mesh, matrix, antialias, rule)?;
    let win = match window {
        Some(w) => match c.clip(w) {
            Some(w) => w,
            None => return Some(None),
        },
        None => c.bounds,
    };
    c.raster_window(win, opacity).map(Some)
}

/// Opacity-independent rasterisation work for one mesh + device matrix, in
/// draw order: the pixel runs of flat triangles and, for interpolated triangles, each
/// covered pixel with its interpolated colour before opacity. [`Coverage::raster`]
/// replays it for an opacity with exactly the operations [`rasterize_mesh`] performs, so a
/// fade only pays for blending.
pub struct Coverage {
    w: usize,
    h: usize,
    bounds: sk::IRect,
    ops: Vec<CoverageOp>,
    runs: Vec<(u32, u32)>,
    pixels: Vec<(u32, [f32; 4])>,
}
enum CoverageOp {
    /// Constant colour (before opacity) over `runs[range]` = (first pixel, length).
    Flat([f32; 4], std::ops::Range<usize>),
    /// `pixels[range]` = (pixel index, interpolated colour before opacity).
    Interpolated(std::ops::Range<usize>),
}
impl Coverage {
    fn bytes(&self) -> usize {
        self.runs.len() * 8 + self.pixels.len() * 20 + self.ops.len() * 32
    }
    /// The premultiplied raster at `opacity`.
    pub fn raster(&self, opacity: f32) -> Option<Raster> {
        self.raster_window(self.bounds, opacity)
    }
    /// `window` ∩ the raster bounds, or None when they do not overlap.
    pub fn clip(&self, window: sk::IRect) -> Option<sk::IRect> {
        let b = self.bounds;
        let r = sk::IRect::new(b.left.max(window.left), b.top.max(window.top), b.right.min(window.right), b.bottom.min(window.bottom));
        (r.right > r.left && r.bottom > r.top).then_some(r)
    }
    /// The raster restricted to `win` (a sub-rectangle of the bounds, see [`Self::clip`]).
    pub fn raster_window(&self, win: sk::IRect, opacity: f32) -> Option<Raster> {
        let (w, h) = (win.width() as usize, win.height() as usize);
        // Rasterise straight into Skia-owned pixels (no copy into the image afterwards).
        let info = sk::ImageInfo::new((w as i32, h as i32), sk::ColorType::RGBA8888, sk::AlphaType::Premul, None);
        let mut bitmap = sk::Bitmap::new();
        if !bitmap.try_alloc_pixels_info(&info, w * 4) {
            return None;
        }
        let ptr = bitmap.pixels() as *mut u32;
        if ptr.is_null() || !(ptr as usize).is_multiple_of(std::mem::align_of::<u32>()) {
            return None;
        }
        // SAFETY: the bitmap owns w·h 32-bit pixels (row bytes w·4, checked alignment) until
        // it is dropped; the slice is dead before `as_image` shares the immutable pixels.
        self.render_window(win, unsafe { std::slice::from_raw_parts_mut(ptr, w * h) }, opacity);
        bitmap.set_immutable();
        Some(Raster { image: bitmap.as_image(), bounds: win })
    }
    /// Size in pixels and device bounds of the raster.
    pub fn bounds(&self) -> sk::IRect {
        self.bounds
    }
    /// Writes the pixels of `win` (a sub-rectangle of the bounds) at `opacity` into `words`
    /// (premultiplied RGBA, row stride = window width), overwriting it. Every pixel equals
    /// the full raster's: pixels are independent, so runs and pixels outside are skipped.
    pub fn render_window(&self, win: sk::IRect, words: &mut [u32], opacity: f32) {
        let ww = win.width() as usize;
        assert!(win.left >= self.bounds.left && win.top >= self.bounds.top && win.right <= self.bounds.right && win.bottom <= self.bounds.bottom);
        assert_eq!(words.len(), ww * win.height() as usize);
        let (x0, y0) = ((win.left - self.bounds.left) as usize, (win.top - self.bounds.top) as usize);
        let (x1, y1) = (x0 + ww, (win.bottom - self.bounds.top) as usize);
        let full = x0 == 0 && y0 == 0 && ww == self.w && y1 == self.h;
        // Run (full-raster pixel index, length) → window-relative (index, length), clipped.
        let run = |start: u32, len: u32| -> Option<(usize, usize)> {
            if full {
                return Some((start as usize, len as usize));
            }
            let (y, x) = (start as usize / self.w, start as usize % self.w);
            if y < y0 || y >= y1 {
                return None;
            }
            let (a, b) = (x.max(x0), (x + len as usize).min(x1));
            (b > a).then(|| ((y - y0) * ww + (a - x0), b - a))
        };
        let pixel = |at: u32| -> Option<usize> {
            if full {
                return Some(at as usize);
            }
            let (y, x) = (at as usize / self.w, at as usize % self.w);
            (y >= y0 && y < y1 && x >= x0 && x < x1).then(|| (y - y0) * ww + (x - x0))
        };
        words.fill(0);
        let opacity = opacity.clamp(0., 1.);
        for (i, op) in self.ops.iter().enumerate() {
            match op {
                CoverageOp::Flat(c, range) => {
                    // The source colour is constant, so the blended result depends only on
                    // the destination pixel; reuse it while the destination repeats.
                    let flat_color = c.map(|v| v * opacity);
                    let inverse = 1. - flat_color[3];
                    let blend = |dst: u32| -> u32 {
                        let d = dst.to_ne_bytes();
                        u32::from_ne_bytes(std::array::from_fn(|ch| {
                            round_even((flat_color[ch] + d[ch] as f32 / 255. * inverse) * 255.).clamp(0., 255.) as u8
                        }))
                    };
                    let mut memo: Option<(u32, u32)> = None;
                    if i == 0 {
                        // First op: nothing written yet, and the runs of one triangle never
                        // overlap (one per row), so every destination is still 0.
                        let out = blend(0);
                        for &(start, len) in &self.runs[range.clone()] {
                            if let Some((a, n)) = run(start, len) {
                                words[a..a + n].fill(out);
                            }
                        }
                        continue;
                    }
                    for &(start, len) in &self.runs[range.clone()] {
                        let Some((a, n)) = run(start, len) else { continue };
                        let run = &mut words[a..a + n];
                        // Uniform destination (e.g. the fill strip over the empty buffer).
                        let first = run[0];
                        if run.iter().all(|w| *w == first) {
                            let out = match memo {
                                Some((d, o)) if d == first => o,
                                _ => blend(first),
                            };
                            memo = Some((first, out));
                            run.fill(out);
                            continue;
                        }
                        for px in run {
                            let out = match memo {
                                Some((d, o)) if d == *px => o,
                                _ => {
                                    let o = blend(*px);
                                    memo = Some((*px, o));
                                    o
                                }
                            };
                            *px = out;
                        }
                    }
                }
                CoverageOp::Interpolated(range) => {
                    for &(at, pre) in &self.pixels[range.clone()] {
                        let Some(at) = pixel(at) else { continue };
                        let color = pre.map(|v| v * opacity);
                        let inverse = 1. - color[3];
                        let px = &mut words[at];
                        let d = px.to_ne_bytes();
                        *px = u32::from_ne_bytes(std::array::from_fn(|ch| {
                            round_even((color[ch] + d[ch] as f32 / 255. * inverse) * 255.).clamp(0., 255.) as u8
                        }));
                    }
                }
            }
        }
    }
}

/// Coverage of `mesh` under `matrix`: vertex positions snapped to 1/256 px (exact integer
/// edge functions, pixel centres, `rule` on ties), colours interpolated from the unsnapped
/// positions. None = unsupported (perspective, empty or oversized).
pub fn coverage(mesh: &Mesh, matrix: &sk::Matrix, antialias: bool, rule: TieRule) -> Option<Coverage> {
    if matrix.has_perspective() || mesh.vertices.is_empty() {
        return None;
    }
    let d = device_mesh(mesh, matrix, antialias);
    let s = SUBPIXEL as i64;
    let left = d.fixed.iter().map(|p| p[0]).min()?.div_euclid(s);
    let top = d.fixed.iter().map(|p| p[1]).min()?.div_euclid(s);
    let right = (d.fixed.iter().map(|p| p[0]).max()? + s - 1).div_euclid(s);
    let bottom = (d.fixed.iter().map(|p| p[1]).max()? + s - 1).div_euclid(s);
    let (w, h) = ((right - left) as usize, (bottom - top) as usize);
    if w == 0 || h == 0 || w > 16384 || h > 16384 || w.checked_mul(h)? > 64 * 1024 * 1024 {
        return None;
    }
    let (bl, bt, br, bb) = (left, top, right, bottom);
    let w = (br - bl) as usize;
    let mut cov = Coverage { w, h: (bb - bt) as usize, bounds: sk::IRect::new(bl as i32, bt as i32, br as i32, bb as i32), ops: Vec::new(), runs: Vec::new(), pixels: Vec::new() };
    // Interpolation in coordinates relative to the full raster origin keeps f32 precision
    // independent of where the item sits on the output (and of the window).
    let origin = [left as f32, top as f32];
    for triangle in &mesh.triangles {
        let mut idx = triangle.map(|i| i as usize);
        let f = idx.map(|i| d.fixed[i]);
        let area = edge_i(f[0], f[1], f[2]);
        if area == 0 {
            continue;
        }
        if area < 0 {
            idx.swap(1, 2);
        }
        let f = idx.map(|i| d.fixed[i]);
        let p = idx.map(|i| [d.float[i][0] - origin[0], d.float[i][1] - origin[1]]);
        let c = idx.map(|i| d.colors[i].map(|v| v as f32 / 255.));
        let flat = c[0] == c[1] && c[1] == c[2];
        let farea = edge_f(p[0], p[1], p[2]);
        if farea == 0. {
            continue;
        }
        let owns = [owns_edge(f[1], f[2], rule), owns_edge(f[2], f[0], rule), owns_edge(f[0], f[1], rule)];
        let half = s / 2;
        // Centres strictly below the minimum cannot be covered; include the boundary one.
        let x0 = (f.iter().map(|v| v[0]).min().unwrap() - half).div_euclid(s).max(bl);
        let x1 = (f.iter().map(|v| v[0]).max().unwrap() - half).div_euclid(s).min(br - 1);
        let y0 = (f.iter().map(|v| v[1]).min().unwrap() - half).div_euclid(s).max(bt);
        let y1 = (f.iter().map(|v| v[1]).max().unwrap() - half).div_euclid(s).min(bb - 1);
        let (runs0, pixels0) = (cov.runs.len(), cov.pixels.len());
        for y in y0..=y1 {
            let py = y * s + half;
            // Exact row span: the centres where every edge function is > 0, or == 0 on an
            // owned edge (the per-pixel inclusion test solved for x).
            let (mut sx0, mut sx1) = (x0, x1);
            for k in 0..3 {
                let (a, b) = (f[(k + 1) % 3], f[(k + 2) % 3]);
                // e(x) = K - D·x with px = x·s + half
                let dy = b[1] - a[1];
                let k0 = (b[0] - a[0]) * (py - a[1]) - dy * (half - a[0]);
                let d = dy * s;
                if d == 0 {
                    if k0 < 0 || (k0 == 0 && !owns[k]) {
                        sx0 = 1;
                        sx1 = 0;
                    }
                } else if d > 0 {
                    // decreasing: e >= 0 ⇔ x <= ⌊K/D⌋; e > 0 ⇔ x <= ⌈K/D⌉ − 1
                    let max = if owns[k] { k0.div_euclid(d) } else { -(-k0).div_euclid(d) - 1 };
                    sx1 = sx1.min(max);
                } else {
                    // increasing: e = K + |D|·x; e >= 0 ⇔ x >= ⌈−K/|D|⌉; e > 0 ⇔ x >= ⌊−K/|D|⌋ + 1
                    let dn = -d;
                    let min = if owns[k] { -(k0.div_euclid(dn)) } else { (-k0).div_euclid(dn) + 1 };
                    sx0 = sx0.max(min);
                }
            }
            if sx0 > sx1 {
                continue;
            }
            let row = (y - bt) as usize * w;
            if flat {
                cov.runs.push(((row + (sx0 - bl) as usize) as u32, (sx1 - sx0 + 1) as u32));
                continue;
            }
            for x in sx0..=sx1 {
                let q = [(x - left) as f32 + 0.5, (y - top) as f32 + 0.5];
                let wgt = [edge_f(p[1], p[2], q) / farea, edge_f(p[2], p[0], q) / farea, edge_f(p[0], p[1], q) / farea];
                let pre: [f32; 4] = std::array::from_fn(|ch| wgt[0] * c[0][ch] + wgt[1] * c[1][ch] + wgt[2] * c[2][ch]);
                cov.pixels.push(((row + (x - bl) as usize) as u32, pre));
            }
        }
        if flat {
            if cov.runs.len() > runs0 {
                cov.ops.push(CoverageOp::Flat(c[0], runs0..cov.runs.len()));
            }
        } else if cov.pixels.len() > pixels0 {
            cov.ops.push(CoverageOp::Interpolated(pixels0..cov.pixels.len()));
        }
    }
    Some(cov)
}
/// `v.round_ties_even()` without the libm call (the default x86-64 target has no
/// `roundss`): adding and subtracting 2²³ rounds half to even exactly for |v| < 2²².
#[inline]
fn round_even(v: f32) -> f32 {
    const MAGIC: f32 = 8_388_608.;
    if v.abs() < 4_194_304. {
        (v + MAGIC) - MAGIC
    } else {
        v.round_ties_even()
    }
}
fn edge_i(a: [i64; 2], b: [i64; 2], p: [i64; 2]) -> i64 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}
fn edge_f(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}
/// Whether centres exactly on edge a→b (of a positive-area triangle) are inside: the
/// sample nudged by the rule's vanishing offset must increase the edge function.
fn owns_edge(a: [i64; 2], b: [i64; 2], rule: TieRule) -> bool {
    let (gx, gy) = (-(b[1] - a[1]), b[0] - a[0]);
    let primary = gx * rule.x as i64;
    if primary != 0 {
        primary > 0
    } else {
        gy * rule.y as i64 > 0
    }
}
/// Rasterizes transformed vertex colors directly into a premultiplied image.
/// No supersampling or analytic circle coverage: sample positions are GL centers.
pub fn rasterize(bounds: Rect, rect: &Rectangle, matrix: &sk::Matrix, opacity: f32) -> Option<Raster> {
    rasterize_antialias(bounds, rect, matrix, opacity, rect.radii[0] > 0.)
}
/// Explicit QQuickRectangle.antialiasing. The default follows radius > 0.
/// Disabling the offset shader collapses AA strips into degenerate triangles.
pub fn rasterize_antialias(bounds: Rect, rect: &Rectangle, matrix: &sk::Matrix, opacity: f32, antialias: bool) -> Option<Raster> {
    let mesh = tessellate(bounds, rect)?;
    rasterize_mesh(&mesh, matrix, opacity, antialias, TIE_RULE)
}
/// Draw in current device coordinates, preserving the existing canvas clip (uncached).
pub fn draw(canvas: &sk::Canvas, bounds: Rect, rect: &Rectangle, opacity: f32) -> bool {
    if bounds.width == 0. || bounds.height == 0. {
        return true;
    }
    let Some(raster) = rasterize(bounds, rect, &canvas.local_to_device_as_3x3(), opacity) else {
        return false;
    };
    let _save = sk::AutoCanvasRestore::guard(canvas, true);
    canvas.reset_matrix();
    canvas.draw_image(raster.image, (raster.bounds.left as f32, raster.bounds.top as f32), Some(&crate::render::exact_default()));
    true
}

// ---------------------------------------------------------------- renderer cache

/// Identity of a tessellated mesh: local size, radius, pen and premultiplied colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct MeshKey([u32; 6], [u64; 2]);
fn pack(c: [u8; 4]) -> u32 {
    u32::from_le_bytes(c)
}
fn mesh_key(w: f32, h: f32, rect: &Rectangle) -> Option<MeshKey> {
    let Fill::Solid(fill) = rect.fill else { return None };
    let border = rect.border.as_ref().map_or((0., [0; 4]), |b| (b.width, premul(b.color)));
    Some(MeshKey([w.to_bits(), h.to_bits(), rect.radii[0].to_bits(), border.0.to_bits(), pack(premul(fill)), pack(border.1)], [0;2]))
}
fn matrix_bits(m: &sk::Matrix) -> [u32; 6] {
    [m.scale_x(), m.skew_x(), m.translate_x(), m.skew_y(), m.scale_y(), m.translate_y()].map(f32::to_bits)
}

type VertexKey = (MeshKey, [u32; 6], bool);
type RasterKey = (MeshKey, [u32; 6], u32, bool, [i32; 4]);
type CoverageKey = (MeshKey, [u32; 6], bool);
const COVERAGE_BYTES: usize = crate::render::RASTER_CACHE_SHARE;
const SEEN: usize = 4096;
const MESHES: usize = 1024;
const VERTICES: usize = 1024;
const RASTER_BYTES: usize = crate::render::RASTER_CACHE_SHARE;
type VertexCache = HashMap<VertexKey, ((sk::Vertices, sk::Shader), u64)>;
type RasterCache = HashMap<RasterKey, (Rc<Raster>, u64)>;

/// Renderer-owned caches for [`RectMode::QtQuick`]. A mesh is tessellated once per
/// (size, radius, border, colours). GPU canvases draw it as vertex-coloured triangles
/// (`drawVertices` + palette shader, cached per device matrix); raster canvases use
/// [`rasterize_mesh`], cached per device matrix (static rectangles reuse their image).
/// Entries are evicted least-recently-used.
#[derive(Default)]
pub struct Cache {
    clock: u64,
    meshes: HashMap<MeshKey, (Rc<Mesh>, u64)>,
    vertices: VertexCache,
    palette: Option<sk::RuntimeEffect>,
    rasters: RasterCache,
    raster_bytes: usize,
    /// Opacity-independent coverage per mesh + matrix + window: a fade re-blends only.
    coverage: HashMap<CoverageKey, (Rc<Coverage>, u64)>,
    coverage_bytes: usize,
    /// Raster keys painted once; only a repeat (static content) earns a persistent image.
    seen: HashMap<RasterKey, ((), u64)>,
    /// Reused pixels for one-off rasters (fades): no fresh pages per frame.
    scratch: Vec<u32>,
}
fn evict<K: Copy + Eq + std::hash::Hash, V>(map: &mut HashMap<K, (V, u64)>, limit: usize) {
    while map.len() >= limit {
        let Some(oldest) = map.iter().min_by_key(|(_, v)| v.1).map(|(k, _)| *k) else { break };
        map.remove(&oldest);
    }
}
impl Cache {
    pub fn clear(&mut self) {
        self.meshes.clear();
        self.vertices.clear();
        self.rasters.clear();
        self.coverage.clear();
        self.coverage_bytes = 0;
        self.seen.clear();
        self.scratch = Vec::new();
        self.raster_bytes = 0;
    }
    fn mesh(&mut self, key: MeshKey, w: f32, h: f32, rect: &Rectangle, size: Option<(f64,f64)>) -> Option<Rc<Mesh>> {
        if let Some((mesh, used)) = self.meshes.get_mut(&key) {
            *used = self.clock;
            return Some(mesh.clone());
        }
        let bounds=Rect::new(0.,0.,w,h);
        let mesh = Rc::new(match size { Some(size)=>tessellate_qreal(bounds,rect,size)?,None=>tessellate(bounds,rect)? });
        evict(&mut self.meshes, MESHES);
        self.meshes.insert(key, (mesh.clone(), self.clock));
        Some(mesh)
    }
    /// Draws `rect` at `bounds` (local coordinates of `canvas`) with the item opacity.
    /// Returns false for content Qt's rectangle node does not cover (gradients,
    /// per-corner radii, perspective); the caller then uses its generic path.
    pub fn draw(&mut self, canvas: &sk::Canvas, bounds: Rect, rect: &Rectangle, opacity: f32, antialias: bool) -> bool {
        self.draw_with_size(canvas,bounds,rect,opacity,antialias,None)
    }
    /// Qt QRectF-size metadata shares the same bounded mesh/GPU caches.
    pub fn draw_with_size(&mut self, canvas: &sk::Canvas, bounds: Rect, rect: &Rectangle, opacity: f32, antialias: bool, size: Option<(f64,f64)>) -> bool {
        let Some(mut key) = mesh_key(bounds.width, bounds.height, rect) else { return false };
        if let Some((w,h))=size {key.1=[w.to_bits(),h.to_bits()];}
        if !rect.radii.iter().all(|v| v.is_finite() && (*v - rect.radii[0]).abs() < 1e-6) {
            return false;
        }
        let device = canvas.local_to_device_as_3x3();
        if device.has_perspective() || !opacity.is_finite() {
            return false;
        }
        if bounds.width == 0. || bounds.height == 0. || opacity <= 0. {
            return true;
        }
        self.clock += 1;
        let Some(mesh) = self.mesh(key, bounds.width, bounds.height, rect, size) else { return false };
        if mesh.vertices.is_empty() {
            return true;
        }
        let matrix = sk::Matrix::concat(&device, &sk::Matrix::translate((bounds.x, bounds.y)));
        let _restore = sk::AutoCanvasRestore::guard(canvas, true);
        canvas.reset_matrix();
        if canvas.recording_context().is_some() {
            // Same canonical mesh, Qt's vertex-stage opacity/color interpolation.
            match crate::native_gl::rect(canvas, mesh.clone(), &matrix, opacity, antialias) {
                Ok(true) => return true,
                Ok(false) => (),
                Err(error) => { log_once(&error.to_string()); return false; }
            }
            let vkey = (key, matrix_bits(&matrix), antialias);
            let clock = self.clock;
            let (vertices, shader) = match self.vertices.get_mut(&vkey) {
                Some((v, used)) => {
                    *used = clock;
                    v.clone()
                }
                None => {
                    let effect = match &self.palette {
                        Some(e) => e.clone(),
                        None => {
                            let Ok(e) = sk::RuntimeEffect::make_for_shader(PALETTE_SKSL, None) else { return false };
                            self.palette = Some(e.clone());
                            e
                        }
                    };
                    let (fill, border) = (key.0[4].to_le_bytes(), key.0[5].to_le_bytes());
                    let Some(shader) = palette_shader(&effect, fill, border) else { return false };
                    let v = (gpu_vertices(&mesh, &matrix, antialias, fill, border), shader);
                    evict(&mut self.vertices, VERTICES);
                    self.vertices.insert(vkey, (v.clone(), clock));
                    v
                }
            };
            let mut paint = sk::Paint::default();
            paint.set_anti_alias(false);
            paint.set_alpha_f(opacity.clamp(0., 1.));
            paint.set_shader(shader);
            canvas.draw_vertices(&vertices, sk::BlendMode::Modulate, &paint);
            return true;
        }
        // Raster: device positions in absolute f32 coordinates, exactly as Qt's vertex
        // shader computes them (rasterising at the sub-pixel phase and offsetting is not
        // bit-identical: 698/2160 sweep cases differ), so the cache key is the full matrix.
        let (ix, iy, local) = (0f32, 0f32, matrix);
        let clock = self.clock;
        // Coverage is per full rectangle (cached across opacities and clip windows).
        let ckey = (key, matrix_bits(&local), antialias);
        let cov = match self.coverage.get_mut(&ckey) {
            Some((c, used)) => {
                *used = clock;
                c.clone()
            }
            None => {
                let Some(c) = coverage(&mesh, &local, antialias, TIE_RULE) else { return false };
                let c = Rc::new(c);
                let bytes = c.bytes();
                while !self.coverage.is_empty() && self.coverage_bytes + bytes > COVERAGE_BYTES {
                    let Some(oldest) = self.coverage.iter().min_by_key(|(_, v)| v.1).map(|(k, _)| *k) else { break };
                    if let Some((c, _)) = self.coverage.remove(&oldest) {
                        self.coverage_bytes -= c.bytes();
                    }
                }
                if bytes <= COVERAGE_BYTES {
                    self.coverage_bytes += bytes;
                    self.coverage.insert(ckey, (c.clone(), clock));
                }
                c
            }
        };
        // Rasterise only the visible part when the clip cuts the rectangle (reveal
        // animations); the full raster otherwise, so static rectangles stay cached.
        let clip = canvas.device_clip_bounds().unwrap_or_default();
        let full = cov.bounds();
        let Some(win) = cov.clip(clip) else { return true };
        let window = (win != full).then_some(win);
        let rkey = (key, matrix_bits(&local), opacity.to_bits(), antialias, window.map_or([0; 4], |w| [w.left, w.top, w.right, w.bottom]));
        let raster = match self.rasters.get_mut(&rkey) {
            Some((r, used)) => {
                *used = clock;
                r.clone()
            }
            None => {
                if self.seen.insert(rkey, ((), clock)).is_none() {
                    // First use (e.g. one frame of a fade): rasterise into the scratch
                    // buffer and draw it without keeping an image.
                    evict(&mut self.seen, SEEN);
                    let b = win;
                    let n = (b.width() * b.height()) as usize;
                    if self.scratch.len() < n {
                        self.scratch.resize(n, 0);
                    }
                    let words = &mut self.scratch[..n];
                    cov.render_window(win, words, opacity);
                    let info = sk::ImageInfo::new((b.width(), b.height()), sk::ColorType::RGBA8888, sk::AlphaType::Premul, None);
                    // SAFETY: `words` outlives the borrowed image, which is drawn and dropped
                    // before the scratch buffer can change (raster canvases draw immediately).
                    let bytes = unsafe { std::slice::from_raw_parts(words.as_ptr() as *const u8, n * 4) };
                    let Some(image) = sk::images::raster_from_data(&info, unsafe { sk::Data::new_bytes(bytes) }, b.width() as usize * 4) else { return false };
                    canvas.draw_image(&image, (b.left as f32, b.top as f32), Some(&crate::render::exact_default()));
                    drop(image);
                    if self.scratch.capacity() * 4 > RASTER_BYTES { self.scratch = Vec::new(); }
                    return true;
                }
                let Some(r) = cov.raster_window(win, opacity) else { return false };
                let r = Rc::new(r);
                let bytes = raster_bytes(&r);
                while !self.rasters.is_empty() && self.raster_bytes + bytes > RASTER_BYTES {
                    let Some(oldest) = self.rasters.iter().min_by_key(|(_, v)| v.1).map(|(k, _)| *k) else { break };
                    if let Some((r, _)) = self.rasters.remove(&oldest) {
                        self.raster_bytes -= raster_bytes(&r);
                    }
                }
                if bytes <= RASTER_BYTES {
                    self.raster_bytes += bytes;
                    self.rasters.insert(rkey, (r.clone(), clock));
                }
                r
            }
        };
        canvas.draw_image(&raster.image, (ix + raster.bounds.left as f32, iy + raster.bounds.top as f32), Some(&crate::render::exact_default()));
        true
    }
}
fn raster_bytes(r: &Raster) -> usize {
    r.image.width() as usize * r.image.height() as usize * 4
}
/// Device-space triangles for Skia's GPU backend, which rasterises them as GL does for
/// Qt's geometry node (pixel centres, no edge AA). Skia interpolates `SkVertices` colours
/// unpremultiplied, Qt premultiplied; a Qt rectangle mesh only uses the fill, border and
/// transparent colours, so each vertex instead carries barycentric "texture" coordinates
/// (fill = (1,0), border = (0,1), transparent = (0,0)) and [`palette_shader`] outputs
/// `u·fill + v·border` in premultiplied space: exactly Qt's interpolation.
fn gpu_vertices(mesh: &Mesh, matrix: &sk::Matrix, antialias: bool, fill: [u8; 4], border: [u8; 4]) -> sk::Vertices {
    let mut b = sk::vertices::Builder::new(sk::vertices::VertexMode::Triangles, mesh.vertices.len(), mesh.triangles.len() * 3, sk::vertices::BuilderFlags::HAS_TEX_COORDS);
    for (i, v) in mesh.vertices.iter().enumerate() {
        let mut v = *v;
        if !antialias {
            v.offset = [0., 0.];
        }
        let (p, c) = device_vertex(v, matrix);
        b.positions()[i] = sk::Point::new(p[0], p[1]);
        let uv = if c == [0; 4] {
            (0., 0.)
        } else if c == fill {
            (1., 0.)
        } else {
            debug_assert_eq!(c, border);
            (0., 1.)
        };
        b.tex_coords().unwrap()[i] = sk::Point::new(uv.0, uv.1);
    }
    let indices = b.indices().unwrap();
    for (i, t) in mesh.triangles.iter().enumerate() {
        indices[i * 3..i * 3 + 3].copy_from_slice(t);
    }
    b.detach()
}
const PALETTE_SKSL: &str = "uniform float4 fill; uniform float4 border; half4 main(float2 uv) { return half4(fill * uv.x + border * uv.y); }";
/// Premultiplied two-colour palette indexed by interpolated barycentric coordinates.
fn palette_shader(effect: &sk::RuntimeEffect, fill: [u8; 4], border: [u8; 4]) -> Option<sk::Shader> {
    let mut u = [0f32; 8];
    for i in 0..4 {
        u[i] = fill[i] as f32 / 255.;
        u[4 + i] = border[i] as f32 / 255.;
    }
    let bytes: Vec<u8> = u.iter().flat_map(|v| v.to_ne_bytes()).collect();
    effect.make_shader(sk::Data::new_copy(&bytes), &[], None)
}

/// Each distinct error message is printed once per process (at most 64 kinds). A failing
/// draw call on every rectangle of every frame, e.g. after the connection was lost, used
/// to print one line per call.
fn log_once(message: &str) {
    use std::{collections::HashSet, sync::Mutex};
    static SEEN: Mutex<Option<HashSet<String>>> = Mutex::new(None);
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    let seen = seen.get_or_insert_with(HashSet::new);
    if seen.len() < 64 && seen.insert(message.to_string()) {
        eprintln!("{message} (further identical errors suppressed)");
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    use crate::scene::Border;

    fn rect(radius: f32, border: Option<f32>) -> Rectangle {
        Rectangle {
            fill: Fill::Solid(Color { r: 0.9, g: 0.9, b: 0.95, a: 0.92 }),
            radii: [radius; 4],
            border: border.map(|width| Border { width, color: Color { r: 0.2, g: 0.1, b: 0., a: 0.4 } }),
        }
    }

    /// The GPU palette shader relies on every vertex being fill, border or transparent.
    #[test]
    fn mesh_colours_are_a_two_colour_palette() {
        for r in [0., 3., 19.] {
            for b in [None, Some(1.), Some(2.)] {
                let q = rect(r, b);
                let m = tessellate(Rect::new(0., 0., 120.5, 38.), &q).unwrap();
                let Fill::Solid(f) = q.fill else { unreachable!() };
                let border = q.border.as_ref().map_or([0; 4], |b| premul(b.color));
                assert!(m.vertices.iter().all(|v| v.color == [0; 4] || v.color == premul(f) || v.color == border));
            }
        }
    }

    fn pixels(s: &mut sk::Surface) -> Vec<u8> {
        let info = sk::ImageInfo::new(s.image_info().dimensions(), sk::ColorType::RGBA8888, sk::AlphaType::Premul, None);
        let mut v = vec![0u8; (info.width() * info.height() * 4) as usize];
        s.read_pixels(&info, &mut v, info.width() as usize * 4, (0, 0));
        v
    }

    /// A clip that cuts the rectangle rasterises only the visible window; the composited
    /// result equals the full uncached raster under the same clip.
    #[test]
    fn clipped_cached_draw_matches_uncached() {
        let mut cache = Cache::default();
        let q = rect(19., Some(1.));
        let bounds = Rect::new(5.3, 2.7, 100.25, 50.);
        for clip in [Rect::new(0., 0., 120., 20.), Rect::new(30.5, 10.25, 40., 30.), Rect::new(200., 0., 10., 10.)] {
            let draw_with = |f: &mut dyn FnMut(&sk::Canvas)| {
                let mut s = sk::surfaces::raster_n32_premul((260, 130)).unwrap();
                s.canvas().scale((2., 2.));
                s.canvas().clip_rect(sk::Rect::from_xywh(clip.x, clip.y, clip.width, clip.height), None, false);
                f(s.canvas());
                pixels(&mut s)
            };
            let expected = draw_with(&mut |c| {
                draw(c, bounds, &q, 0.6);
            });
            for _ in 0..2 {
                let got = draw_with(&mut |c| {
                    assert!(cache.draw(c, bounds, &q, 0.6, true));
                });
                assert!(got == expected, "clip {clip:?}");
            }
        }
    }

    /// Installed Qt's QRectF edge arithmetic precedes the float corner centers.
    #[test]
    fn qreal_rectangle_matches_independent_qt_mesh() {
        let oracle: serde_json::Value=serde_json::from_str(include_str!("../tests/fixtures/qt-rect-qreal.json")).unwrap();
        let size=(262.99125364431484,125.70845481049561);
        let rectangle=Rectangle {
            fill:Fill::Solid(Color::rgba(30./255.,30./255.,46./255.,1.)),
            radii:[25.991253644314867f64 as f32;4],
            border:Some(crate::scene::Border {width:1.,color:Color::rgba(205./255.,214./255.,244./255.,29./255.)})
        };
        let bounds=Rect::new(0.,0.,size.0 as f32,size.1 as f32);
        let mesh=tessellate_qreal(bounds,&rectangle,size).unwrap();
        let vertices=oracle["vertices"].as_array().unwrap();
        assert_eq!(vertices.len(),mesh.vertices.len());
        for (i,(expected,actual)) in vertices.iter().zip(&mesh.vertices).enumerate() {
            for (j,value) in actual.position.into_iter().chain(actual.offset).chain(actual.color.map(|v|v as f32)).enumerate() {
                assert_eq!(value,(expected[j].as_f64().unwrap())as f32,"vertex{i} component{j}");
            }
        }
        let indices:Vec<u16>=oracle["indices"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap()as u16).collect();
        assert_eq!(mesh.indices,indices);
        assert_ne!(tessellate(bounds,&rectangle).unwrap().vertices[0].position[0],mesh.vertices[0].position[0]);
    }

    /// Cached raster draws (miss and hit) equal the uncached rasteriser.
    #[test]
    fn cached_raster_draw_matches_uncached() {
        let mut cache = Cache::default();
        for (k, (r, b)) in [(0., None), (10., Some(1.)), (19., Some(2.))].into_iter().enumerate() {
            let q = rect(r, b);
            let bounds = Rect::new(3.3 + k as f32, 4.7, 90.25, 40.);
            let mut expected = sk::surfaces::raster_n32_premul((260, 120)).unwrap();
            expected.canvas().scale((2., 2.));
            assert!(draw(expected.canvas(), bounds, &q, 0.6));
            let expected = pixels(&mut expected);
            for _ in 0..2 {
                let mut s = sk::surfaces::raster_n32_premul((260, 120)).unwrap();
                s.canvas().scale((2., 2.));
                assert!(cache.draw(s.canvas(), bounds, &q, 0.6, r > 0.));
                assert!(pixels(&mut s) == expected);
            }
        }
    }
}
