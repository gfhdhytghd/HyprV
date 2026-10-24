//! `qt_rect::Cache` on a GPU canvas (vertex mesh through Skia's GL backend) vs the CPU
//! rasteriser, over a sweep of Qt rectangles; plus per-frame cost of a size morph.
#[path = "../src/gpu.rs"]
#[allow(dead_code)]
mod gpu;
use hyprv_ui::{animation::Color, geometry::Rect, qt_rect::Cache, scene::{Border, Fill, Rectangle}, skia as sk};
use std::time::Instant;

fn read(surface: &mut sk::Surface) -> Vec<u8> {
    let info = sk::ImageInfo::new(surface.image_info().dimensions(), sk::ColorType::RGBA8888, sk::AlphaType::Premul, None);
    let mut out = vec![0u8; (info.width() * info.height() * 4) as usize];
    assert!(surface.read_pixels(&info, &mut out, info.width() as usize * 4, (0, 0)));
    out
}

fn main() {
    let mut g = gpu::Gpu::new().expect("EGL");
    println!("{} ({})", g.renderer, g.platform);
    let colors = [
        Color { r: 0.96, g: 0.96, b: 0.97, a: 0.92 },
        Color { r: 0.1, g: 0.4, b: 0.9, a: 1. },
        Color { r: 1., g: 1., b: 1., a: 0.055 },
    ];
    let borders = [None, Some((1., Color { r: 0., g: 0., b: 0., a: 0.09 })), Some((2., Color { r: 0.9, g: 0.2, b: 0.1, a: 0.7 }))];
    let (mut cases, mut differ, mut px, mut worst) = (0, 0, 0usize, 0u8);
    let mut hist = [0usize; 256];
    let (mut gc, mut cc) = (Cache::default(), Cache::default());
    for dpr in [1f32, 2., 1.5] {
        for (w, h) in [(40., 38.), (120.16, 38.), (391.3, 200.7), (13., 13.), (3., 30.), (300., 1.)] {
            for radius in [0f32, 3., 10., 19.] {
                for border in borders {
                    for fill in colors {
                        for abs in [(10f32, 10f32), (10.3, 7.71), (100.5, 3.25)] {
                            for opacity in [1f32, 0.37] {
                                cases += 1;
                                let rect = Rectangle { fill: Fill::Solid(fill), radii: [radius; 4], border: border.map(|(width, color)| Border { width, color }) };
                                let size = (((abs.0 + w + 4.) * dpr).ceil() as i32 + 2, ((abs.1 + h + 4.) * dpr).ceil() as i32 + 2);
                                let mut gs = g.render_target(size.0, size.1).unwrap();
                                gs.canvas().clear(sk::Color::TRANSPARENT);
                                gs.canvas().scale((dpr, dpr));
                                gs.canvas().translate(abs);
                                gc.draw(gs.canvas(), Rect::new(0., 0., w, h), &rect, opacity, radius > 0.);
                                g.skia.flush_and_submit();
                                let mut cs = sk::surfaces::raster_n32_premul(size).unwrap();
                                cs.canvas().scale((dpr, dpr));
                                cs.canvas().translate(abs);
                                cc.draw(cs.canvas(), Rect::new(0., 0., w, h), &rect, opacity, radius > 0.);
                                let (a, b) = (read(&mut gs), read(&mut cs));
                                let mut n = 0;
                                for (pa, pb) in a.chunks(4).zip(b.chunks(4)) {
                                    let d = pa.iter().zip(pb).map(|(x, y)| x.abs_diff(*y)).max().unwrap();
                                    hist[d as usize] += 1;
                                    if d > 0 {
                                        n += 1;
                                        worst = worst.max(d);
                                    }
                                }
                                let big = a.iter().zip(&b).filter(|(x, y)| x.abs_diff(**y) > 2).count();
                                if big > 0 && std::env::var_os("BIG").is_some() {
                                    println!("BIG dpr {dpr} {w}x{h} r{radius} border {:?} a{} abs {abs:?} op {opacity}: {big} channels >2", border.map(|b| b.0), fill.a);
                                }
                                if n > 0 {
                                    differ += 1;
                                    px += n;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    println!("GPU vs CPU: {cases} cases, {differ} differ, {px} px differ, max delta {worst}");
    println!("  delta histogram (px): {}", (1..=worst as usize).map(|d| format!("{d}:{}", hist[d])).collect::<Vec<_>>().join(" "));

    // Cost: 1000×720 card at scale 2, 14-frame size morph, GPU path.
    for (label, gpu_canvas) in [("GPU", true), ("CPU", false)] {
        let mut cache = Cache::default();
        let mut surface = if gpu_canvas { g.render_target(2400, 1700).unwrap() } else { sk::surfaces::raster_n32_premul((2400, 1700)).unwrap() };
        let rect = |t: f32| Rectangle { fill: Fill::Solid(colors[0]), radii: [24. + 6. * t; 4], border: Some(Border { width: 1., color: Color { r: 0., g: 0., b: 0., a: 0.09 } }) };
        let mut record = 0.;
        let mut gpu_us = 0.;
        let n = 14;
        for k in 0..n {
            let t = k as f32 / n as f32 + 0.013;
            let (w, h) = (200. + 800. * t, 38. + 682. * t);
            let start = Instant::now();
            let ((), us) = g.timed(|ctx| {
                let c = surface.canvas();
                c.save();
                c.scale((2., 2.));
                cache.draw(c, Rect::new(10.3, 10., w, h), &rect(t), 1., true);
                c.restore();
                if gpu_canvas {
                    ctx.flush_and_submit();
                }
            });
            record += start.elapsed().as_secs_f64();
            gpu_us += us.unwrap_or(0.);
        }
        println!("{label} canvas, card 200×38→1000×720 morph: {:.0} µs/frame CPU (GPU {:.0} µs/frame)", record / n as f64 * 1e6, gpu_us / n as f64);
    }
}
