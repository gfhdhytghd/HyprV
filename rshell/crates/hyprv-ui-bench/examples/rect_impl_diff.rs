//! Proof that hyprv-ui `qt_rect::rasterize_mesh` is bit-identical to its reference copy
//! (`support/cpu_ref_qt_rect.rs`, the CPU rasteriser before the span/flat-fill
//! optimisation): same image bytes and bounds over a broad sweep; windowed rasters
//! (`rasterize_mesh_window`) equal the matching crop of the reference; plus timing.
#[path = "support/cpu_ref_qt_rect.rs"]
mod reference;
use hyprv_ui::{
    animation::Color,
    geometry::Rect,
    qt_rect::{rasterize_mesh, rasterize_mesh_window, tessellate, Raster, TieRule, TIE_RULE},
    scene::{Border, Fill, Rectangle},
    skia as sk,
};
use std::time::Instant;

fn bytes(r: &Raster) -> Vec<u8> {
    let i = &r.image;
    let info = sk::ImageInfo::new((i.width(), i.height()), sk::ColorType::RGBA8888, sk::AlphaType::Premul, None);
    let mut v = vec![0u8; (i.width() * i.height() * 4) as usize];
    assert!(i.read_pixels(&info, &mut v, i.width() as usize * 4, (0, 0), sk::image::CachingHint::Allow));
    v
}

fn main() {
    let colors = [
        Color { r: 0.96, g: 0.96, b: 0.97, a: 0.92 },
        Color { r: 0.1, g: 0.4, b: 0.9, a: 1. },
        Color { r: 1., g: 1., b: 1., a: 0.055 },
        Color { r: 0.3, g: 0.2, b: 0.1, a: 0. },
    ];
    let borders = [None, Some((1., Color { r: 0., g: 0., b: 0., a: 0.09 })), Some((2., Color { r: 0.9, g: 0.2, b: 0.1, a: 0.7 })), Some((5., Color { r: 0.2, g: 0.9, b: 0.4, a: 1. }))];
    let sizes = [(40., 38.), (120.16, 38.), (391.3, 200.7), (13., 13.), (3., 30.), (300., 1.), (1000., 720.), (0.4, 9.)];
    let matrices: Vec<sk::Matrix> = {
        let mut v = Vec::new();
        for (sx, sy) in [(1f32, 1f32), (2., 2.), (1.5, 1.5), (1.25, 2.), (0.5, 0.5)] {
            for (tx, ty) in [(10f32, 10f32), (20.6, 15.42), (201., 6.5), (3071.999, 1700.25), (6143.5, 0.125)] {
                let mut m = sk::Matrix::scale((sx, sy));
                m.post_translate((tx, ty));
                v.push(m);
            }
        }
        // Rotated (affine) placement.
        let mut m = sk::Matrix::rotate_deg(17.);
        m.post_scale((2., 2.), None);
        m.post_translate((300.3, 40.7));
        v.push(m);
        v
    };
    let rules = [TIE_RULE, TieRule { x: 1, y: 1 }, TieRule { x: -1, y: -1 }];
    let (mut cases, mut differ, mut windows) = (0usize, 0usize, 0usize);
    let (mut t_ref, mut t_new) = (0f64, 0f64);
    let mut examples = Vec::new();
    for (w, h) in sizes {
        for radius in [0f32, 0.4, 3., 10., 19., 30.] {
            for border in borders {
                for fill in colors {
                    let rect = Rectangle { fill: Fill::Solid(fill), radii: [radius; 4], border: border.map(|(width, color)| Border { width, color }) };
                    let Some(mesh) = tessellate(Rect::new(0., 0., w, h), &rect) else { continue };
                    for m in &matrices {
                        for opacity in [1f32, 0.37] {
                            for antialias in [radius > 0., radius == 0.] {
                                for rule in rules {
                                    if rule != TIE_RULE && (opacity != 1. || fill.a != 0.92) {
                                        continue;
                                    }
                                    cases += 1;
                                    let t = Instant::now();
                                    let a = reference::rasterize_mesh(&mesh, m, opacity, antialias, rule);
                                    t_ref += t.elapsed().as_secs_f64();
                                    let t = Instant::now();
                                    let b = rasterize_mesh(&mesh, m, opacity, antialias, rule);
                                    t_new += t.elapsed().as_secs_f64();
                                    let same = match (&a, &b) {
                                        (None, None) => true,
                                        (Some(a), Some(b)) => a.bounds == b.bounds && bytes(a) == bytes(b),
                                        _ => false,
                                    };
                                    // Windowed rasterisation = crop of the reference full raster.
                                    let mut same = same;
                                    if let Some(a) = &a {
                                        let ab = a.bounds;
                                        let full = bytes(a);
                                        let (aw, ah) = (ab.width(), ab.height());
                                        for (fx0, fy0, fx1, fy1) in [(0., 0., 1., 0.5), (0.3, 0.2, 0.71, 0.9), (0.5, 0.5, 2., 2.), (-1., -1., 0.1, 0.1), (2., 2., 3., 3.)] {
                                            let win = sk::IRect::new(ab.left + (aw as f32 * fx0) as i32, ab.top + (ah as f32 * fy0) as i32, ab.left + (aw as f32 * fx1) as i32, ab.top + (ah as f32 * fy1) as i32);
                                            windows += 1;
                                            let got = rasterize_mesh_window(&mesh, m, opacity, antialias, rule, Some(win));
                                            let ok = match got {
                                                None => false,
                                                Some(None) => {
                                                    let l = ab.left.max(win.left); let t = ab.top.max(win.top);
                                                    let r = ab.right.min(win.right); let b = ab.bottom.min(win.bottom);
                                                    r <= l || b <= t
                                                }
                                                Some(Some(w)) => {
                                                    let wb = w.bounds;
                                                    let exp_b = sk::IRect::new(ab.left.max(win.left), ab.top.max(win.top), ab.right.min(win.right), ab.bottom.min(win.bottom));
                                                    let px = bytes(&w);
                                                    wb == exp_b && (0..wb.height()).all(|y| {
                                                        let src = (((wb.top - ab.top + y) * aw + (wb.left - ab.left)) * 4) as usize;
                                                        let dst = (y * wb.width() * 4) as usize;
                                                        px[dst..dst + (wb.width() * 4) as usize] == full[src..src + (wb.width() * 4) as usize]
                                                    })
                                                }
                                            };
                                            if !ok {
                                                same = false;
                                            }
                                        }
                                    }
                                    if !same {
                                        differ += 1;
                                        if examples.len() < 10 {
                                            examples.push(format!("{w}x{h} r{radius} border {:?} a{} m {:?} op {opacity} aa {antialias} rule {rule:?}", border.map(|b| b.0), fill.a, m));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    println!("{cases} cases (+{windows} windowed rasters vs crops of the reference), {differ} differ (image bytes + bounds)");
    println!("rasterize_mesh total: reference {:.0} ms, optimised {:.0} ms ({:.1}×)", t_ref * 1e3, t_new * 1e3, t_ref / t_new.max(1e-9));
    for e in examples {
        println!("  {e}");
    }
    if differ > 0 {
        std::process::exit(1);
    }
}
