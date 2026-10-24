//! `ui-bench [scale] [--json out.json]` — per-frame cost of every UI component through
//! hyprv-ui's two render paths:
//! - CPU: `Renderer::paint` into a raster surface (damage-clipped);
//! - GPU: the production EGL path minus the swap — `Renderer::paint` into a persistent
//!   retained Skia GL target (damage-clipped), then the whole retained image is copied to
//!   a back-buffer target (`EglWindow::draw` copies the full retained surface every frame).
//!
//! Per frame: component update (bindings, Behaviors, reconcile) CPU time and
//! allocations; damage rect; paint CPU time and allocations for both paths; GPU time of
//! paint and of the full copy (GL_EXT_disjoint_timer_query). Per scenario: RSS, Skia GPU
//! resource bytes, NVX video-memory delta. Frames run on a 60 Hz virtual clock.
mod gpu;
mod scenarios;

use hyprv_ui::{geometry::Rect, render::{physical_size, Renderer}, scene::Scene, skia as sk};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicU64, Ordering::Relaxed},
    time::{Duration, Instant},
};

// ------------------------------------------------------------------ allocation counter

struct Counting;
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(l.size() as u64, Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(n as u64, Relaxed);
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;
fn allocs() -> (u64, u64) {
    (ALLOCS.load(Relaxed), BYTES.load(Relaxed))
}

pub fn rss_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status").ok().and_then(|s| {
        s.lines().find(|l| l.starts_with("VmRSS:")).and_then(|l| l.split_whitespace().nth(1)?.parse().ok())
    }).unwrap_or(0)
}

// ------------------------------------------------------------------ measurement

#[derive(Clone, Copy, Default, Debug)]
pub struct Frame {
    pub update_us: f64,
    pub update_allocs: u64,
    pub damage_px: f64,
    pub cpu_paint_us: f64,
    pub cpu_paint_allocs: u64,
    pub gpu_record_us: f64,
    pub gpu_paint_us: Option<f64>,
    pub gpu_copy_us: Option<f64>,
}

pub struct Report {
    pub name: String,
    pub surface: (f32, f32),
    pub frames: Vec<Frame>,
    pub rss_kib: u64,
    pub skia_gpu_bytes: usize,
    pub vidmem_delta_kib: Option<i64>,
    pub target_bytes: u64,
    pub note: String,
}

pub struct Bench {
    pub gpu: Option<gpu::Gpu>,
    pub scale: f64,
    pub reports: Vec<Report>,
}

/// One surface's render targets and renderers (as a host keeps them per surface).
pub struct Target {
    pub w: f32,
    pub h: f32,
    cpu: sk::Surface,
    retained: Option<sk::Surface>,
    back: Option<sk::Surface>,
    r_cpu: Renderer,
    r_gpu: Renderer,
}

impl Bench {
    pub fn target(&mut self, w: f32, h: f32) -> Target {
        let (pw, ph) = physical_size(w, h, self.scale).unwrap();
        let cpu = sk::surfaces::raster_n32_premul((pw, ph)).expect("raster target");
        let (retained, back) = match self.gpu.as_mut() {
            Some(g) => (g.render_target(pw, ph), g.render_target(pw, ph)),
            None => (None, None),
        };
        let renderer = || {
            let mut r = Renderer::new();
            // UI_BENCH_RECT=skia compares against the analytic Skia rectangle path.
            if std::env::var("UI_BENCH_RECT").is_ok_and(|m| m == "skia") {
                r.set_rect_mode(hyprv_ui::qt_rect::RectMode::Skia);
            }
            r
        };
        Target { w, h, cpu, retained, back, r_cpu: renderer(), r_gpu: renderer() }
    }

    /// Run `frames` 60 Hz frames: `step(scene, now)` performs the component's per-frame
    /// work (input/time → bindings → scene edits), then damage is painted on both paths.
    pub fn run(&mut self, name: &str, size: (f32, f32), scene: &mut Scene, frames: usize, mut step: impl FnMut(&mut Scene, Duration, usize), note: &str) {
        let (w, h) = size;
        if std::env::var("UI_BENCH_FILTER").is_ok_and(|f| !name.contains(&f)) {
            return;
        }
        let vid0 = self.gpu.as_ref().and_then(|g| g.vidmem_kib());
        let mut t = self.target(w, h);
        // First full paint (surface creation) is excluded from the per-frame stats.
        let extent = Rect::new(0., 0., w, h);
        let _ = scene.take_damage(extent);
        scene.invalidate();
        self.paint(&mut t, scene, extent);
        let _ = scene.take_damage(extent);
        let mut out = Vec::with_capacity(frames);
        for i in 0..frames {
            let now = Duration::from_micros(((i as f64 + 1.) * 1_000_000. / 60.) as u64);
            let (a0, _) = allocs();
            let s = Instant::now();
            step(scene, now, i);
            let mut f = Frame { update_us: s.elapsed().as_secs_f64() * 1e6, update_allocs: allocs().0 - a0, ..Default::default() };
            if let Some(d) = scene.take_damage(extent) {
                f.damage_px = (d.width * d.height) as f64;
                let p = self.paint(&mut t, scene, d);
                f.cpu_paint_us = p.0;
                f.cpu_paint_allocs = p.1;
                f.gpu_record_us = p.2;
                f.gpu_paint_us = p.3;
                f.gpu_copy_us = p.4;
            }
            out.push(f);
        }
        let skia_gpu_bytes = self.gpu.as_ref().map_or(0, |g| g.skia.resource_cache_usage().resource_bytes);
        let vid1 = self.gpu.as_ref().and_then(|g| g.vidmem_kib());
        let (pw, ph) = physical_size(w, h, self.scale).unwrap();
        self.reports.push(Report {
            name: name.into(),
            surface: (w, h),
            frames: out,
            rss_kib: rss_kib(),
            skia_gpu_bytes,
            vidmem_delta_kib: vid0.zip(vid1).map(|(a, b)| a.1 - b.1),
            target_bytes: pw as u64 * ph as u64 * 4,
            note: note.into(),
        });
        drop(t);
        if let Some(g) = self.gpu.as_mut() {
            g.skia.free_gpu_resources();
        }
    }

    /// (cpu paint µs, cpu allocs, gpu record µs, gpu paint µs, gpu full-copy µs)
    fn paint(&mut self, t: &mut Target, scene: &Scene, damage: Rect) -> (f64, u64, f64, Option<f64>, Option<f64>) {
        let (a0, _) = allocs();
        let s = Instant::now();
        t.r_cpu.paint(t.cpu.canvas(), scene, self.scale, damage).expect("cpu paint");
        let cpu_us = s.elapsed().as_secs_f64() * 1e6;
        let cpu_allocs = allocs().0 - a0;
        let (mut rec, mut gp, mut gc) = (0., None, None);
        if let (Some(g), Some(retained), Some(back)) = (self.gpu.as_mut(), t.retained.as_mut(), t.back.as_mut()) {
            let s = Instant::now();
            t.r_gpu.paint(retained.canvas(), scene, self.scale, damage).expect("gpu paint");
            rec = s.elapsed().as_secs_f64() * 1e6;
            let (_, us) = g.timed(|ctx| { ctx.flush_and_submit(); });
            gp = us;
            // EglWindow::draw: snapshot the retained target and draw it over the whole back buffer.
            let (_, us) = g.timed(|ctx| {
                let image = retained.image_snapshot();
                back.canvas().draw_image(&image, (0, 0), None);
                drop(image);
                ctx.flush_and_submit();
            });
            gc = us;
        }
        (cpu_us, cpu_allocs, rec, gp, gc)
    }
}

// ------------------------------------------------------------------ statistics/report

fn pct(v: &mut [f64], p: f64) -> f64 {
    if v.is_empty() {
        return 0.;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() - 1) as f64 * p).round() as usize]
}

pub struct Summary {
    pub name: String,
    pub surface: String,
    pub frames: usize,
    pub damaged_frames: usize,
    pub update_p50: f64,
    pub update_p95: f64,
    pub cpu_p50: f64,
    pub cpu_p95: f64,
    pub frame_cpu_p95: f64,
    pub gpu_record_p50: f64,
    pub gpu_record_p95: f64,
    pub gpu_paint_p50: Option<f64>,
    pub gpu_copy_p50: Option<f64>,
    pub damage_mean: f64,
    pub damage_max: f64,
    pub allocs_update_p50: f64,
    pub allocs_paint_p50: f64,
    pub rss_mib: f64,
    pub skia_gpu_mib: f64,
    pub target_mib: f64,
    pub vidmem_delta_mib: Option<f64>,
    pub note: String,
}

fn summarize(r: &Report) -> Summary {
    let steady = &r.frames[r.frames.len() / 10..];
    let damaged: Vec<&Frame> = steady.iter().filter(|f| f.damage_px > 0.).collect();
    let col = |f: &dyn Fn(&Frame) -> f64, only_damaged: bool| -> Vec<f64> {
        if only_damaged { damaged.iter().map(|x| f(x)).collect() } else { steady.iter().map(f).collect() }
    };
    let opt = |v: Vec<Option<f64>>| -> Option<f64> {
        let mut v: Vec<f64> = v.into_iter().flatten().collect();
        (!v.is_empty()).then(|| pct(&mut v, 0.5))
    };
    Summary {
        name: r.name.clone(),
        surface: format!("{}×{}", r.surface.0, r.surface.1),
        frames: steady.len(),
        damaged_frames: damaged.len(),
        update_p50: pct(&mut col(&|f| f.update_us, false), 0.5),
        update_p95: pct(&mut col(&|f| f.update_us, false), 0.95),
        cpu_p50: pct(&mut col(&|f| f.cpu_paint_us, true), 0.5),
        cpu_p95: pct(&mut col(&|f| f.cpu_paint_us, true), 0.95),
        frame_cpu_p95: pct(&mut col(&|f| f.update_us + f.gpu_record_us, false), 0.95),
        gpu_record_p50: pct(&mut col(&|f| f.gpu_record_us, true), 0.5),
        gpu_record_p95: pct(&mut col(&|f| f.gpu_record_us, true), 0.95),
        gpu_paint_p50: opt(damaged.iter().map(|f| f.gpu_paint_us).collect()),
        gpu_copy_p50: opt(damaged.iter().map(|f| f.gpu_copy_us).collect()),
        damage_mean: steady.iter().map(|f| f.damage_px).sum::<f64>() / steady.len().max(1) as f64,
        damage_max: steady.iter().map(|f| f.damage_px).fold(0., f64::max),
        allocs_update_p50: pct(&mut col(&|f| f.update_allocs as f64, false), 0.5),
        allocs_paint_p50: pct(&mut col(&|f| f.cpu_paint_allocs as f64, true), 0.5),
        rss_mib: r.rss_kib as f64 / 1024.,
        skia_gpu_mib: r.skia_gpu_bytes as f64 / 1048576.,
        target_mib: r.target_bytes as f64 / 1048576.,
        vidmem_delta_mib: r.vidmem_delta_kib.map(|k| k as f64 / 1024.),
        note: r.note.clone(),
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let scale: f64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(2.);
    let json_out = args.skip_while(|a| a != "--json").nth(1);
    let gpu = match gpu::Gpu::new() {
        Ok(g) => {
            eprintln!("GPU: EGL {} platform, {}; timer queries: {}", g.platform, g.renderer, g.has_timer());
            Some(g)
        }
        Err(e) => {
            eprintln!("GPU path unavailable: {e}");
            None
        }
    };
    let mut bench = Bench { gpu, scale, reports: Vec::new() };
    let rss0 = rss_kib();
    scenarios::all(&mut bench);
    let summaries: Vec<Summary> = bench.reports.iter().map(summarize).collect();
    println!("scale {scale}; baseline RSS {:.1} MiB", rss0 as f64 / 1024.);
    println!("| scenario | surface | frames (damaged) | update µs p50/p95 | CPU raster paint µs p50/p95 | GL record (CPU) µs p50/p95 | GPU paint µs p50 | GPU full copy µs p50 | damage px² mean/max | allocs/frame update / paint p50 | RSS MiB | Skia GPU MiB | targets MiB |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    for s in &summaries {
        let o = |v: Option<f64>| v.map_or("–".into(), |v| format!("{v:.0}"));
        println!(
            "| {} | {} | {} ({}) | {:.0} / {:.0} | {:.0} / {:.0} | {:.0} / {:.0} | {} | {} | {:.0} / {:.0} | {:.0} / {:.0} | {:.1} | {:.1} | {:.1} |",
            s.name, s.surface, s.frames, s.damaged_frames, s.update_p50, s.update_p95, s.cpu_p50, s.cpu_p95, s.gpu_record_p50, s.gpu_record_p95,
            o(s.gpu_paint_p50), o(s.gpu_copy_p50), s.damage_mean, s.damage_max, s.allocs_update_p50, s.allocs_paint_p50,
            s.rss_mib, s.skia_gpu_mib, s.target_mib * 2.,
        );
    }
    if let Some(path) = json_out {
        let v: Vec<serde_json::Value> = bench.reports.iter().map(|r| serde_json::json!({
            "name": r.name, "surface": [r.surface.0, r.surface.1], "note": r.note,
            "rss_kib": r.rss_kib, "skia_gpu_bytes": r.skia_gpu_bytes, "vidmem_delta_kib": r.vidmem_delta_kib,
            "frames": r.frames.iter().map(|f| serde_json::json!([f.update_us, f.update_allocs, f.damage_px, f.cpu_paint_us, f.cpu_paint_allocs, f.gpu_record_us, f.gpu_paint_us, f.gpu_copy_us])).collect::<Vec<_>>(),
        })).collect();
        std::fs::write(path, serde_json::to_string(&serde_json::json!({"scale": scale, "frame_fields": ["update_us","update_allocs","damage_px","cpu_paint_us","cpu_paint_allocs","gpu_record_us","gpu_paint_us","gpu_copy_us"], "scenarios": v})).unwrap()).unwrap();
    }
}
