# Corner overlay without an output-sized buffer — 2026-10-28

The per-output corner overlay (`hyprv-screen-corner-overlay-<OUTPUT>`) used a full-output
RGBA8 EGL swapchain: 81 MiB per buffer at 6144×3456, 19.8 MiB at 2880×1800. On the Intel
Lunar Lake laptop (xe, GTT = system RAM) this was nearly all of the shell's 303 MiB
`drm-resident-gtt`, with a 723 MiB peak when an output reconfigure kept old and new
full-output buffers alive together. The overlay now never allocates an output-sized
buffer, and the presented pixels are unchanged.

## Design

`hyprv_ui::wayland::LayerConfig::sparse` (requires `wp_viewporter` and `wl_subcompositor`;
otherwise the layer falls back to the previous path):

- **Layer surface unchanged.** One layer per output, same namespace, Overlay layer,
  all-edge anchors, size (0,0), exclusive zone -1, no keyboard, same input region
  (laptop: the strip mask; desktop: empty). Its own buffer is one transparent pixel,
  viewport-scaled to the output (the existing input-only mechanism). That pixel is
  redrawn only when the viewport changes: Hyprland resizes a layer on a buffer commit,
  not on a viewport-only commit (the rotation regression below caught this).
- **Scene unchanged.** The scene stays in full-output coordinates: `corner_window`
  (four shades sampling the shared R8 atlas with its screen-sized UVs) plus the laptop
  `compact_strip`. Assembly reconcile and the stats fast path still edit one scene.
- **Band subsurfaces.** `Scene::sparse_regions` takes the conservative damage bounds of
  each visible top-level child, snaps them outward to whole logical and device pixels
  (a 120/gcd(120·scale, 120) step under fractional scale), and merges overlapping bands.
  Each band is a synchronized `wl_subsurface` with its own wp_viewport and its own
  direct (full-redraw) EGL window. `Renderer::paint_at` draws the scene translated by
  whole device pixels, so every node keeps its full-output device phase. Bands are
  disjoint, so the compositor never blends two copies of a scene pixel.
  Subsurfaces have an empty input region: pointer input still hits the layer surface
  and its input region, in output coordinates, and hit testing is unchanged.
- **Atomic commits.** Band buffers, positions and viewports are cached (sync
  subsurfaces) and published by the layer commit (or by the layer pixel's swap), together
  with frame callbacks and presentation feedback. Surplus bands unmap with a null
  attach in the same commit and are kept for reuse. Only damaged bands are redrawn.
  Idle band swapchains are retired like the old direct window.
- **Resizes.** A band whose extent changes gets a new EGL window whose first buffer
  has the new size; the old one is dropped first. Output resize/scale/rotation
  therefore only ever holds band-sized buffers (a full-width band is at most
  output width × ~45 logical px).

### Why full-width bands, not corner-sized tiles

The first implementation used one tight tile per child (28×28 logical per corner).
Client pixels were byte-identical (see the GPU regression below), but the composited
6144×3456@2 capture differed from the old full-output buffer at **50 corner-fringe
pixels by ±1 alpha** (left shades −1, right shades +1): Hyprland 0.56/NVIDIA samples a
56-pixel-wide texture differently from a 6144-wide one. This is the compositor stage
noted in `assembled-corners.md`. Bands that keep the layer's full width remove it
completely: 0 differing pixels, including bottom bands at y = 3400 px, so the band
height and vertical position do not matter. Cost: 6144×56×4 B = 1.31 MiB per 6K band
buffer instead of 4 × 12 KiB tiles; still ~60× less than one full-output buffer.

| Output | Old buffer (each) | New buffers (each) |
| --- | ---: | ---: |
| 6144×3456@2, desktop | 81.0 MiB | 1 × 1 px + 2 bands of 6144×56 px = 2 × 1.31 MiB |
| 2880×1800@2, laptop compact (strip + top shades merged) | 19.8 MiB | 1 × 1 px + 2880×86 px (0.94 MiB) + 2880×56 px (0.62 MiB) |
| 2880×1800@2, laptop expanded | 19.8 MiB | 1 × 1 px + 2 × 0.62 MiB |

The shared atlas texture (8192×4096 R8 = 32 MiB at 6K) is unchanged. It is part of the
documented pixel contract (screen-sized UVs, `canvas_r8_atlas_matches_original_window_alpha_exactly`).

## Pixel identity

All on this machine: NVIDIA RTX PRO 6000 (proprietary 615.78), nested Hyprland 0.56.2.

| Check | Result |
| --- | --- |
| `renderer_regressions::sparse_regions_match_the_full_output_target_exactly` (new): full-output target vs every band, 6144×3456@2 corners and 2880×1800@2 corners + strip at top 15, 7.3125, 2.71828 (fractional reveal), QtRendering and native QtGl text | byte-identical; no paint outside the bands |
| `--nested-test --fixture parity/fullscreen/fixtures/idle.json`, DP-4 3072×1728@2 (6144×3456), dark, grim black/white matte; before vs after | byte-identical (0 / 21,233,664 pixels); vs reference unchanged (417 px, max 4, MAE 0.000827, PASS) |
| Strict corner gate `crates/hyprv-shell/tools/verify_corners.py` on that capture | PASS, 4/4 corners, 0 masked, max channel diff 0 |
| Laptop profile 2880×1800@2, compact strip, private probe (below) with pinned `/proc/stat`/`/proc/meminfo`, before and after run concurrently (same clock minute), grim on black/white/black/black | idle: 4/4 grabs byte-identical; after scale/transform/mode reconfigure and back: 4/4 byte-identical |
| Laptop expanded (strip hidden, shades at top 0) and desktop 6K idle/reconfigured, same probe | corner shades, strip and everything below the bar identical; the only differing pixels are the bar's live temperature/network text (real sensors, y 42–72 px, never within 60 px of a corner) |

The corner-sized tile negative result above (50 px, ±1) is what this table guards against.
Not verified here: Intel xe (no Intel GPU on this machine). Client pixels are
translation-exact by construction (`paint_at`), and full-width bands keep the
compositor's horizontal texture extent, but Hyprland/Mesa on xe was not captured.

## Memory

`docs/verify/corner-overlay-memory.py BIN OUT.json --size WxH --scale S --profile P
[--reconfigure] [--toggle] [--fake-proc] [--grab DIR]`: private runtime, private D-Bus,
`kwin_wayland --virtual` + nested Hyprland, `hyprv-shell --read-only`, live-session
variables removed. Samples every ~2–25 ms: NVML per-process graphics memory (NVIDIA) and
`drm-*` keys of `/proc/PID/fdinfo` where the driver publishes them (xe/i915/amdgpu).
**This NVIDIA machine publishes no drm-* fdinfo keys**, so the figures are NVML
`usedGpuMemory` of the shell process; xe `drm-resident-gtt`/`drm-total` could not be
measured here. `--reconfigure`: scale 2→1.5→2, transform 90→0, mode 3840×2160→back,
4 s each. `--toggle` (laptop): bar expand then compact.

| Run (MiB, NVML) | start peak | idle | reconfigure peak | toggle peak | final | overall peak |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 6144×3456@2 desktop, before | 173.9 | 73.9 | 310.1 | – | 85.9 | 310.1 |
| 6144×3456@2 desktop, after | 75.2 | 73.9 | 164.2 | – | 85.9 | 164.2 |
| 2880×1800@2 laptop, before | 57.4 | 29.4 | 93.4 | 125.8 | 49.4 | 125.8 |
| 2880×1800@2 laptop, after | 33.2 | 29.4 | 57.0 | 59.4 | 33.4 | 59.4 |

Raw receipts: `target/corner-mem/mem/{before,final}-{desk,lap}.json` (not committed). The "after"
rows used the build before the final one, which differs only in building the transparent
pixel's empty scene lazily; the final build's laptop rerun (`ab2-final-lap.json`) agrees.

Reading the numbers:

- On NVIDIA the settled idle value was already equal: the engine retires idle
  direct-window swapchains and the proprietary driver releases them. What remains of the
  old design is every redraw and reconfigure, where a full-output swapchain is
  (re)created: start 174→75 MiB, 6K reconfigure 310→164 MiB, laptop bar toggle 126→59 MiB.
  The remaining reconfigure peak is the bar and other surfaces, not the overlay.
- The xe report (303 MiB resident steady, 723 MiB peak) shows full-output buffers staying
  resident on that driver. Whatever the retention mechanism, the overlay can now only
  hold the buffers in the table above: at most ~3 MiB per 6K output per swapchain
  buffer, against 81 MiB, and nothing output-sized exists during a reconfigure.

## Regression evidence

```sh
export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0
HYPRV_ASSET_ROOT=$HOME/.local/share/hyprv-shell cargo test -p hyprv-ui -p hyprv-ui-bar -p hyprv-shell
crates/hyprv-ui/tests/run-renderer-regressions.sh
crates/hyprv-ui/tests/run-nested-rotation.sh REPORT
python3 -I parity/laptop/e2e/run-laptop-e2e.py --shell BIN --desktop --report OUT.md
```

- `scene::sparse_tests` (4): band geometry, merge of strip and top shades, device-pixel
  alignment at 1.25/1.5/1.75, self-painting roots fall back to one region.
- `assembly::corner_overlay_keeps_legacy_viewport_and_avoids_retained_copy`: sparse,
  passthrough, two short full-width bands at every scale and reveal.
- `nested_rotation`: now also maps a sparse layer; all 27 scale/transform steps keep
  compositor layer sizes, viewports and positions valid (no protocol error). Without the
  layer-pixel redraw it failed at scale 1, transform 90 (layer stayed 1920×1080).

## Results (final release build, sha256 `f11be7fde2f041ec…`)

| Suite | Result |
| --- | --- |
| `cargo test -p hyprv-ui -p hyprv-ui-bar -p hyprv-shell` (nested idle test included) | 205 passed, 0 failed, 46 ignored |
| `run-renderer-regressions.sh` (surfaceless EGL + private Weston) | 26/26, incl. the frozen NVIDIA Qt-window atlas oracle and the new sparse identity test |
| `run-nested-rotation.sh` (bwrap, nested Hyprland, scales 1/1.5/2, 8 transforms) | 27/27 steps, plus static-frame test; 2/2 tests |
| `run-laptop-e2e.py --desktop` | laptop suite 39/39 (38/38 laptop cases PASS); desktop functional **204/204** |
| `layer-commits.py` (protocol trace) | 0 violations in every run of both binaries. The 5-transition probe is a wall-clock flake: the baseline also failed it 2 of 3 runs (desktop probe 4/5), the final build failed 2 of 3 runs and passed one fully (5, 5, 25) |

The laptop e2e harness's own fake-bus self-test (`org.hyprv.Test.Step … no LidClosed signal`)
failed in every run, before any shell is started. An earlier e2e run, on a build without
the layer-pixel redraw fix, had one desktop failure (`outputs.scale-1.5`, a bar-surface
scale check with an 800 ms settle); it passed in the final run.
`--nested-test --fixture` replay does not settle in the laptop profile on this setup
(this probe saw Hyprland's start-up warning toast animating over the strip), so the laptop A/B uses the probe,
which sets `misc.disable_watchdog_warning`.
