# UI rendering performance (bar, island, panels, calendar morph)

Measured 2026-10-23 by svc-media with `crates/hyprv-ui-bench`. Offscreen only: no live surfaces,
no compositor or service interaction.

## Setup and method

- Real output geometry: DP-4 6144×3456 physical, **scale 2** (3072×1728 logical), 60 Hz. Intel Core Ultra 7
  265K; NVIDIA RTX PRO 6000 Blackwell (driver 615.78.08), EGL surfaceless platform, GLES2 context
  configured like `hyprv_ui::egl::SharedEgl`.
- `./target/release/ui-bench 2 --json <file>` (`UI_BENCH_FILTER=<substr>`, `UI_BENCH_RECT=skia`).
  Per frame (16 ms steps): component update → `Scene::take_damage` → **CPU raster** `Renderer::paint`
  (damage-clipped, retained raster target) → **GPU** `Renderer::paint` into a retained Skia GL target
  (CPU recording time + `GL_EXT_disjoint_timer_query` GPU time) → full-target copy into a back buffer,
  mirroring `EglWindow::draw`. A counting global allocator gives allocations per update and per paint.
  RSS comes from `/proc/self/statm`, Skia GPU memory from `DirectContext::resource_cache_usage`, and target sizes from the surface
  dimensions. The first full paint (surface creation) is excluded.
- Host policy modelled for the island: `frame()` only while animating, at a due deadline or after new input;
  rebuild/reconcile only when `Island::visual_rev()` changed. Bar and panels are updated every frame
  as their current APIs require.
- `perf` is unavailable (`perf_event_paranoid=2`, no sudo) and ptrace is restricted, so hotspots were
  isolated by bisection. The bisection probes are `crates/hyprv-ui-bench/examples/{rect_morph,island_morph_bisect,image_sampler}.rs`.
- Raw per-frame data: [raw/ui-bench-qtquick.json](raw/ui-bench-qtquick.json) (engine default
  `RectMode::QtQuick`) and [raw/ui-bench-skia-rects.json](raw/ui-bench-skia-rects.json).

## Results (engine default, QtQuick rect mode)

| scenario | surface | frames (damaged) | update µs p50/p95 | CPU raster paint µs p50/p95 | GL record (CPU) µs p50/p95 | GPU paint µs p50 | GPU full copy µs p50 | damage px² mean/max | allocs/frame update / paint p50 | RSS MiB | Skia GPU MiB | targets MiB |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| island idle (clock only, no change) | 3072×58 | 108 (0) | 0 / 0 | 0 / 0 | 0 / 0 | – | – | 0 / 0 | 0 / 0 | 192.5 | 5.6 | 5.4 |
| island compact, spectrum 24 Hz | 3072×58 | 162 (65) | 1 / 8 | 13 / 26 | 8 / 17 | 7 | 3 | 411 / 1024 | 7 / 16 | 204.5 | 5.6 | 5.4 |
| island compact, paused (66 ms idle wave) | 3072×58 | 108 (28) | 0 / 8 | 19 / 25 | 14 / 18 | 7 | 3 | 265 / 1024 | 0 / 16 | 212.2 | 5.6 | 5.4 |
| island music expand morph (0→600 ms) | 3072×820 | 33 (33) | 20 / 57 | 316 / 6218 | 114 / 5305 | 11 | 21 | 34046 / 74772 | 199 / 100 | 339.6 | 90.2 | 76.9 |
| island expanded, spectrum 24 Hz + 1 Hz position | 3072×820 | 162 (89) | 13 / 26 | 62 / 350 | 28 / 135 | 8 | 21 | 9812 / 41580 | 206 / 77 | 359.1 | 79.1 | 76.9 |
| island agent expanded n=9, ring step 650 ms | 3072×820 | 162 (4) | 1 / 1 | 84 / 89 | 50 / 53 | 38 | 21 | 89 / 3600 | 56 / 211 | 387.5 | 82.8 | 76.9 |
| bar static, stats sample 1 Hz | 3072×58 | 162 (3) | 38 / 45 | 1187 / 2329 | 1144 / 2253 | 10 | 4 | 146 / 20270 | 1062 / 190 | 377.8 | 6.6 | 5.4 |
| bar fullscreen hide → reveal | 3072×58 | 108 (73) | 57 / 86 | 3698 / 4484 | 2942 / 3519 | 14 | 4 | 141706 / 250927 | 988 / 394 | 442.8 | 25.9 | 5.4 |
| control panel open (reveal) + steady | 3072×1728 | 81 (5) | 4 / 27 | 1020 / 1108 | 165 / 168 | 358 | 42 | 7958 / 128915 | 28 / 0 | 568.8 | 168.9 | 162.0 |
| Wi-Fi popup open + steady | 3072×1728 | 81 (13) | 5 / 43 | 2158 / 2613 | 192 / 279 | 515 | 42 | 50909 / 317203 | 10 / 0 | 604.8 | 184.7 | 162.0 |
| QuickAdjust OSD show | 3072×1728 | 54 (4) | 1 / 7 | 2281 / 2372 | 1797 / 2609 | 21 | 43 | 1589 / 21460 | 6 / 109 | 604.8 | 171.2 | 162.0 |
| stats popup, sample 1 Hz | 3072×1728 | 162 (3) | 19 / 20 | 10999 / 11542 | 917 / 960 | 130 | 42 | 99315 / 5363000 | 11 / 6 | 628.1 | 196.6 | 162.0 |
| calendar morph open (0–450 ms) → close | 3072×820 | 54 (54) | 344 / 465 | 2577 / 20714 | 256 / 18495 | 294 | 21 | 952784 / 1676115 | 1165 / 16 | 803.6 | 147.4 | 76.9 |

“frames (damaged)”: frames measured / frames with non-empty damage. Paint columns count damaged frames only.
“targets MiB”: retained target + back buffer for the surface (production EGL also has swapchain buffers).

### Same run with `UI_BENCH_RECT=skia` (analytic Skia rectangles)

| scenario | frames (damaged) | update µs p50/p95 | CPU raster paint µs p50/p95 | GL record (CPU) µs p50/p95 | GPU paint µs p50 | GPU full copy µs p50 | damage px² mean/max | allocs/frame update / paint p50 |
|---|---|---|---|---|---|---|---|---|
| island idle (clock only, no change) | 108 (0) | 0 / 0 | 0 / 0 | 0 / 0 | – | – | 0 / 0 | 0 / 0 |
| island compact, spectrum 24 Hz | 162 (65) | 1 / 11 | 26 / 40 | 16 / 24 | 36 | 3 | 411 / 1024 | 7 / 8 |
| island compact, paused (66 ms idle wave) | 108 (28) | 0 / 8 | 26 / 33 | 16 / 19 | 36 | 3 | 265 / 1024 | 0 / 8 |
| island music expand morph (0→600 ms) | 33 (33) | 19 / 53 | 177 / 1206 | 106 / 1448 | 9 | 21 | 34046 / 74772 | 199 / 49 |
| island expanded, spectrum 24 Hz + 1 Hz position | 162 (89) | 15 / 28 | 49 / 222 | 39 / 130 | 8 | 21 | 9812 / 41580 | 206 / 49 |
| island agent expanded n=9, ring step 650 ms | 162 (4) | 1 / 1 | 69 / 73 | 54 / 56 | 31 | 21 | 89 / 3600 | 56 / 171 |
| bar static, stats sample 1 Hz | 162 (3) | 34 / 43 | 1038 / 1087 | 909 / 1161 | 10 | 4 | 146 / 20270 | 1062 / 158 |
| bar fullscreen hide → reveal | 108 (73) | 47 / 67 | 838 / 1091 | 120 / 200 | 13 | 4 | 141706 / 250927 | 988 / 140 |
| control panel open (reveal) + steady | 81 (5) | 4 / 25 | 955 / 1070 | 161 / 173 | 293 | 42 | 7958 / 128915 | 28 / 0 |
| Wi-Fi popup open + steady | 81 (13) | 4 / 49 | 2186 / 2491 | 203 / 277 | 572 | 42 | 50909 / 317203 | 10 / 0 |
| QuickAdjust OSD show | 54 (4) | 1 / 3 | 60 / 203 | 46 / 120 | 11 | 42 | 1589 / 21460 | 6 / 0 |
| stats popup, sample 1 Hz | 162 (3) | 19 / 22 | 10799 / 10910 | 918 / 926 | 119 | 42 | 99315 / 5363000 | 11 / 6 |
| calendar morph open (0–450 ms) → close | 54 (54) | 338 / 397 | 1617 / 2156 | 353 / 589 | 419 | 21 | 952784 / 1676115 | 1165 / 8 |

## Budget verdict (16.7 ms at 60 Hz)

- **Steady states are fine.** The compact island with 24 Hz spectrum costs 1 µs update + 8 µs GL record + 7 µs GPU per
  damaged frame, 7 allocations per update. The idle island costs nothing (0 rebuilds, 0 allocations, 0 damage). The agent list
  ring tick (n=9) costs 50 µs record. The control panel, Wi-Fi and stats popups at rest cost ≤1 ms record and ≤0.5 ms GPU.
- **Calendar morph misses frames:** GL record p95 **18.5 ms** (QtQuick rects) vs 0.59 ms (Skia rects).
  Isolated cause: the animating container rectangle is CPU-rasterised every frame (18 ms/frame for a
  1000×720 card at scale 2). → engine **PERF-UI-5 [P0]**.
- **Island expand morph:** 5.3 ms p95 record. Cold caches dominate (first-time shaping and glyph masks ~60 ms
  total for the first expand). Warm, with realistic frame-time jitter, the cost is 3.7 ms/frame (QtQuick) vs 0.62 ms
  (Skia rects). Within budget, but it costs 20–30 % of a core while morphing.
- **Bar fullscreen hide→reveal:** 2.9 ms/frame record (QtQuick) vs 0.12 ms (Skia): moving rects
  re-rasterise because the cache key contains the translation (PERF-UI-5).
- **Bar static 1 Hz tick:** ~1.1 ms record for ≤20 k px² damage, with no damage culling (PERF-UI-2), and
  1 062 allocations per update (BAR-PERF-1/2).
- **CPU raster fallback only:** the stats popup costs 11 ms and Wi-Fi 2.2 ms per damaged frame (MultiEffect shadow on CPU).
  This is irrelevant on the EGL path but would matter for software rendering.
- **GPU time** is ≤0.5 ms everywhere (worst: Wi-Fi 515 µs, calendar morph 294 µs p50). Full-target copy
  costs 21 µs (820-px strip) / 42 µs (full output).

## Memory

- RSS grew from 61 MiB at baseline to ~800 MiB across all scenarios in one process. That growth is allocator retention, Skia caches and
  separate targets per scenario, so it is not a leak measurement. Per-surface **VRAM** is the real issue: every full-output
  popup surface holds a 6144×3456 retained target plus a back buffer = **162 MiB** (Skia budget 169–197 MiB with
  the popups open), before EGL swapchain buffers. → PERF-UI-7 / SHELL-PERF-3.
- Engine caches with an entry bound but no byte bound: the image sampler (512 images), QtQuick rect rasters (512) and panels' `qrect` (4096).

## Fixed in my crates (svc-media)

- **hyprv-ui-island**: `Island::visual_rev()` changes only when `build()` output can change (input
  inequality, pointer events, timers, hover/fixpoint state). It is `None` while animating. `update()` skips the rebuild
  on an unchanged rev. `evaluate` no longer clones vectors for its fixpoint check (hash), the clock label,
  play glyph or theme tokens. New `Island::set_calendar()` saves the host a full `Inputs` clone per frame.
  Effect (bench, before → after): idle island 428 → **0** allocations/frame and no rebuild; compact spectrum 49 → **7**;
  agent list n=9 611 → **56**. Soundness test
  `island::visual_rev_tests` asserts that `build()` is identical whenever the rev reports "unchanged", across hover, seek,
  wave timer, spectrum, identical snapshots, calendar hide, wheel and reorder. It is mutation-checked: removing any
  rev bump fails it.
- **hyprv-core**: no UI-path hotspot found.

## Requests filed

| Request | Owner | Priority | Summary |
|---|---|---|---|
| PERF-UI-5 | engine (`hyprv-ui.md`) | P0 | QtQuick rects CPU-rasterised per new size *and translation* → draw the tessellated mesh with `draw_vertices`, key without translation |
| PERF-UI-2 | engine | P1 | No damage culling in `Renderer::node` |
| PERF-UI-7 | engine/shell | P1 | 162 MiB VRAM per full-output popup surface |
| PERF-UI-1 | engine | P2 | `image::draw` CPU-resamples per new size/phase (1.5–2.2 ms per animating 512² image) |
| PERF-UI-3 | engine | P2 | `effect_cache` pruning O(effects×nodes) every paint |
| PERF-UI-4 | engine | P2 | Text allocates `LayoutKey` strings per lookup and clones a `PathBuf` per glyph; cold first-morph cost |
| PERF-UI-6 | engine | P2 | `Effects` lacks `PartialEq` |
| BAR-PERF-1/2/3 | bar (`hyprv-ui-bar.md`) | P2/P3 | `format!("{:?}")` effect comparison; full rebuild per update; gate the island slot with `visual_rev` |
| PANELS-PERF-2/3/4 | panels (`hyprv-ui-panels.md`) | P2/P3 | stats `key_of` formats the model; whole-card rebuild per sample; calendar panel rebuilt per morph frame |
| SHELL-PERF-1/2/3 | shell (`hyprv-shell.md`) | P1–P3 | island host gating + `set_calendar`; calendar subtree reuse; popup surface size |

## Corrections during the investigation

- An earlier bench revision pushed a stats sample every frame (a broken 1 Hz guard). That produced 121/162 damaged
  frames, which I first attributed to `qrect::finalize`. The guard is fixed and the panels request withdrawn; the stats popup
  now damages 3/162 frames.
- An earlier run (run 3, before the QtQuick rect mode and the new image sampler landed in the engine) had an
  island-morph p95 of 1.2 ms. The regression is attributed above.

## After the engine work (svc-media, 2026-10-23, same machine and scale)

Changes: one Qt rect mode with GPU mesh drawing (PERF-UI-5), damage culling (PERF-UI-2),
prune per scene revision (PERF-UI-3), allocation-free text hits + cached DF quads (PERF-UI-4),
bit-identical faster image sampler (PERF-UI-1), `Effects: PartialEq` (PERF-UI-6). Raw data:
[raw/ui-bench-after-engine.json](raw/ui-bench-after-engine.json). Before = the QtQuick table above.

| scenario | GL record p50/p95 µs before → after | GPU paint p50 µs | paint allocs p50 |
|---|---|---|---|
| calendar morph open → close | 256 / **18 495** → 117 / **223** | 294 → 124 | 16 → 0 |
| island music expand morph | 114 / 5 305 → 43 / 1 494 (rest: first-time text shaping) | 11 → 8 | 100 → 26 |
| island expanded, spectrum 24 Hz | 28 / 135 → 14 / 54 | 8 → 7 | 77 → 0 |
| island agent list n=9 | 50 / 53 → 14 / 15 | 38 → 31 | 211 → 0 |
| island compact spectrum | 8 / 17 → 13 / 14 | 7 → 7 | 16 → 0 |
| bar static 1 Hz | 1 144 / 2 253 → 716 / 775 | 10 → 8 | 190 → 22 |
| bar fullscreen hide → reveal | 2 942 / 3 519 → 129 / 215 | 14 → 30 | 394 → 87 |
| control panel reveal | 165 / 168 → 92 / 144 | 358 → 68 | 0 → 170 |
| Wi-Fi popup | 192 / 279 → 210 / 239 | 515 → 418 | 0 → 100 |
| QuickAdjust show | 1 797 / 2 609 → 141 / 163 | 21 → 24 | 109 → 15 |
| stats popup 1 Hz | 917 / 960 → 641 / 644 | 130 → 97 | 6 → 0 |

Isolated: Qt rect card morph 200×38→1000×720 at scale 2, GPU canvas, **18 ms → 58 µs CPU +
6 µs GPU per frame** (`examples/qt_rect_gpu_diff.rs`). GPU mesh vs CPU rasteriser: ≤2/255 on
3 888 cases. Image sampler for a new size: 2 294 → 369 µs (bit-identical).

**Parity** (`parity/verify/verify.py`, full run after the changes): bar 224/312 (unchanged;
the GPU harness `bar-gl`, which uses the new mesh path, 228/312); panels and island: no image
lost quality. Every pass/v1 change in that run is a *missing* render from the concurrently
edited harnesses, not a pixel regression. Rect rewrite is bit-identical to the previous engine
rasteriser (5 832 cases); text and image changes are output-identical by construction/test.

**Known follow-up (CPU raster fallback only, not production GPU):** panels now draw rects via
the engine instead of private pictures, so their CPU-raster reveal frames re-rasterise the card
at each opacity (control panel CPU paint 1.0 → 5.3 ms, Wi-Fi 2.2 → 7.0 ms, QuickAdjust 2.3 → 8.0
ms per damaged frame). Bit-exact Qt opacity forbids applying it after rasterisation. A faster
CPU rasteriser for large flat interiors (span fill) is the fix if software rendering matters.

## CPU raster reveal regression fixed (svc-media, 2026-10-23)

`qt_rect.rs`: cached opacity-independent coverage (exact spans, flat runs, interpolated
pre-opacity colours), clip-window replay, scratch-buffer one-off rasters, word fills,
exact libm-free rounding. Bit-identical to the previous CPU rasteriser:
`examples/rect_impl_diff.rs`, 99 840 cases + 499 200 windowed rasters vs crops, 0 differing
bytes or bounds; rasteriser 8.7× faster over that sweep.

| CPU raster paint p50, damaged frames | before switch (panels' private rects) | after switch | now | Skia rects (floor) |
|---|---|---|---|---|
| control panel reveal | 1.0 ms | 5.3 ms | **1.9 ms** | 0.7 ms |
| Wi-Fi popup | 2.2 ms | 7.0 ms | **2.2 ms** | 1.2 ms |
| QuickAdjust show | 2.3 ms | 8.0 ms | 6.6 ms | 6.4 ms (not rects: see note) |

The remaining gap to the Skia floor is the blit of the Qt raster (exact opacity is part of
the raster, so a fade cannot reuse images). QuickAdjust costs the same with Skia rects, and its damage
area grew ~250× (1.6 k → 394 k px² mean) between bench runs: panels-side, not rectangles.
Parity on a frozen source snapshot: [parity-snapshot-2026-10-23T0858.md](parity-snapshot-2026-10-23T0858.md).
