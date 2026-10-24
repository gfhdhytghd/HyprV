//! Verbatim copy of hyprv-ui `qt_rect::rasterize_mesh` as of 2026-10-23 (before the
//! span/flat-fill optimisation): the reference the optimised rasteriser must equal.
#![allow(dead_code)]
use hyprv_ui::{qt_rect::{device_vertex, Mesh, Raster, TieRule}, skia as sk};
const SUBPIXEL: f32 = 256.;
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
    if matrix.has_perspective() || !opacity.is_finite() || mesh.vertices.is_empty() {
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
    let mut data = vec![0u8; w * h * 4];
    let opacity = opacity.clamp(0., 1.);
    // Interpolation in coordinates relative to the raster origin keeps f32 precision
    // independent of where the item sits on the output.
    let origin = [left as f32, top as f32];
    for triangle in &mesh.triangles {
        let mut idx = triangle.map(|i| i as usize);
        let f = idx.map(|i| d.fixed[i]);
        let mut area = edge_i(f[0], f[1], f[2]);
        if area == 0 {
            continue;
        }
        if area < 0 {
            idx.swap(1, 2);
            area = -area;
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
        let x0 = (f.iter().map(|v| v[0]).min().unwrap() - half).div_euclid(s).max(left) + 1;
        let x1 = (f.iter().map(|v| v[0]).max().unwrap() - half).div_euclid(s).min(right - 1);
        let y0 = (f.iter().map(|v| v[1]).min().unwrap() - half).div_euclid(s).max(top) + 1;
        let y1 = (f.iter().map(|v| v[1]).max().unwrap() - half).div_euclid(s).min(bottom - 1);
        // Centres strictly below the minimum cannot be covered; include the boundary one.
        let (x0, y0) = (x0 - 1, y0 - 1);
        for y in y0..=y1 {
            let py = y * s + half;
            // Row span: centres whose three edge functions can be non-negative.
            let (mut sx0, mut sx1) = (x0, x1);
            for k in 0..3 {
                let (a, b) = (f[(k + 1) % 3], f[(k + 2) % 3]);
                // e(x) = (b.x-a.x)(py-a.y) - (b.y-a.y)(px-a.x) >= 0
                let dy = b[1] - a[1];
                let c0 = (b[0] - a[0]) * (py - a[1]);
                if dy == 0 {
                    if c0 < 0 {
                        sx0 = 1;
                        sx1 = 0;
                    }
                    continue;
                }
                // c0 - dy*(px - a.x) >= 0 with px = x*s + half
                let bound = a[0] as f64 + c0 as f64 / dy as f64;
                let xb = (bound - half as f64) / s as f64;
                if dy > 0 {
                    sx1 = sx1.min(xb.floor() as i64 + 1);
                } else {
                    sx0 = sx0.max(xb.ceil() as i64 - 1);
                }
            }
            if sx0 > sx1 {
                continue;
            }
            let row = (y - top) as usize * w;
            for x in sx0..=sx1 {
                let px = x * s + half;
                let e = [edge_i(f[1], f[2], [px, py]), edge_i(f[2], f[0], [px, py]), edge_i(f[0], f[1], [px, py])];
                if (0..3).any(|k| e[k] < 0 || (e[k] == 0 && !owns[k])) {
                    continue;
                }
                let color: [f32; 4] = if flat {
                    c[0].map(|v| v * opacity)
                } else {
                    let q = [(x - left) as f32 + 0.5, (y - top) as f32 + 0.5];
                    let wgt = [edge_f(p[1], p[2], q) / farea, edge_f(p[2], p[0], q) / farea, edge_f(p[0], p[1], q) / farea];
                    std::array::from_fn(|ch| (wgt[0] * c[0][ch] + wgt[1] * c[1][ch] + wgt[2] * c[2][ch]) * opacity)
                };
                let at = (row + (x - left) as usize) * 4;
                let inverse = 1. - color[3];
                for ch in 0..4 {
                    data[at + ch] = ((color[ch] + data[at + ch] as f32 / 255. * inverse) * 255.).round_ties_even().clamp(0., 255.) as u8;
                }
            }
        }
    }
    let info = sk::ImageInfo::new((w as i32, h as i32), sk::ColorType::RGBA8888, sk::AlphaType::Premul, None);
    let image = sk::images::raster_from_data(&info, sk::Data::new_copy(&data), w * 4)?;
    Some(Raster { image, bounds: sk::IRect::new(left as i32, top as i32, right as i32, bottom as i32) })
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
