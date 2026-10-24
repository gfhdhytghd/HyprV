# FR-NET-3: compositor-backed corner exactness

2026-10-23. Acceptance uses unmasked premultiplied RGBA, unchanged references,
unchanged `verify_corners.py`, and the existing full-screen gate. Final strict corner gate: **256/256 exact, max 0, zero masks**. Independent
full-screen comparison: **64/64 PASS**, agreeing with the native comparisons.
No references, acceptance tolerances or crop masks were changed.

## Cause and repair

The source curve was already correct. `shell.qml:3467–3546` places four Canvas
shades in one full-output PanelWindow. The existing offline QPainter bake, private
Qt Canvas.save, and Qt item grab agree exactly. Reading the original shell's actual
EGL window before swap finds 39 additional +1 alpha pixels in the bottom-left
shade. Hyprland sampling and the black/white matte add another 1–3 at 16 of these
pixels, producing the observed maximum 4. A standalone 54×54 texture and four small
layer surfaces do not preserve these two sampling stages.

A private original-Qt GL trace identifies an 8192×4096 atlas at 6144×3456 physical
output size. The four 54×54 images have one-pixel duplicated padding, at inner
origins (1,1), (1,57), (1,113), (1,169). Screen-sized normalized UV arithmetic matters:
a 64×64 atlas with a shader UV remap still differs. Replacing the original Qt BGRA
atlas with R8 and (0,0,0,R) swizzle preserves its actual window pixels exactly.
The native implementation uses that R8 representation; no runtime Qt dependency
or pixel corrections are introduced.

Production now restores one full-output, all-edge anchored, passive Overlay per
output, namespace `hyprv-screen-corner-overlay-<output>`, compositor size (0,0),
exclusive zone -1. Both Assembly and fallback Host use the same builder. Existing
corner mounts reconcile reveal, geometry and scale changes. `full_redraw` renders
this sparse scene directly to EGL window FBO0: no retained offscreen color target
and no final blit. Ordinary surfaces keep their retained path.

The assembled harness previously used CPU SourceOver on raw EGL buffers while Qt
references were captured through Hyprland. It now uses the same compositor
screencopy, stable black/white/black matte, cursor/background settings and matte
validity checks. This is guarded by private fixture markers, isolated runtime,
private IPC signature and private D-Bus. `grim` is test-only; production gains no
helper, watcher, polling timer or readback. Raw EGL surfaces remain diagnostic
artifacts. Direct-window diagnostics redraw before reading a post-swap buffer.

## Regression evidence

- Frozen compact original-window bottom-left golden, source hashes and atlas UV
  provenance: `crates/hyprv-ui/tests/qt-canvas/`.
- `renderer_regressions::canvas_r8_atlas_matches_original_window_alpha_exactly`
  checks the actual full-output projection and 32 MiB R8 allocation on NVIDIA.
  Other drivers explicitly report this frozen NVIDIA oracle unavailable.
- `egl::frame_audit::tests::direct_window_has_no_retained_copy_and_fault_check_is_independent`
  asserts FBO0 wrapping, full redraw despite tiny damage, independent fault
  detection and recovery. Expected bytes are frozen before injected corruption.
- Host/Assembly tests assert one passive full-output direct overlay, four atlas
  sprites, correct scale/reveal geometry. Matte tests preserve0→4 fringe alpha
  and reject changing content. B3 tests render all eight formerly missing hosts.
- Initial real-compositor idle capture:4/4 strict corners exact, whole image PASS,
  max4, painted MAE0.000827, IoU1. Evidence:
  `crates/hyprv-ui-bar/artifacts/corner-idle2/`.

## Memory accounting

At6144×3456, one RGBA8 full-output image is81 MiB. Direct presentation removes that
additional retained target and its copy, while EGL presentation buffers remain
necessary. The atlas uses32 MiB R8 instead of128 MiB BGRA8, saving96 MiB. Four sprite
source images are packed into a small A8 CPU input; no full CPU atlas is allocated.
Atlas cache accounting charges actual R8 bytes and bounds live output/scale atlases
to four. The generic 8 MiB texture working set remains separate. RGBA8 is the portable
EGL window format with the required eight-bit alpha; reducing atlas dimensions or
changing the surface topology demonstrably breaks current exactness.

Measured normal release process (private KWin/Hyprland,6144×3456@2,
quiet agent inputs, read-only real providers). GPU figures use per-process
`nvidia-smi -q -x`, not a deduplicated shell-plus-compositor allocation total:

| Phase | RSS | GPU | Window |
|---|---:|---:|---:|
| idle endpoint |214.1 MiB|257 MiB|30.14s|
| two calendar/control cycles, both closed |247.5 MiB|255 MiB|40.13s|
| sampled activity RSS peak |247.46 MiB|not continuously sampled|eight successful toggles|

Binary SHA256 `4d3a41fb56fa775727fbc7b13f1d74ef280b5dff532bfc585ee109705e869efd`.
Receipts: `crates/hyprv-ui-bar/artifacts/corner-memory-final/{summary,corner-idle,corner-activity,build-receipt}.json`.
This final release includes the batch-root fix and engine’s SHM rollback. The
30/40-second windows overlap parity workloads and are memory evidence, not an
uncontended CPU acceptance claim. Earlier SHM-prototype 214.2/257.0 MiB RSS samples
are superseded; their original receipts remain under `artifacts/corner-memory/`.
The bounded workload stays 72.5 MiB below the 320 MiB RSS limit; this is not an all-popup
or long-duration soak. Capture-mode memory is deliberately not substituted here.

Historical four-small-surface baseline in `engine-memory.md`:203.1/205.2 MiB RSS,
40/38 MiB GPU idle/closed. Thus the current exact topology costs approximately
217 MiB additional GPU in either phase; this historical comparison is not a matched same-checkout
A/B. Compared with the older full-output retained implementation's 307/305 MiB,
the current 257/255 MiB is 50 MiB lower in either phase, also a historical comparison. The concrete
allocation savings established in the implementation are 81 MiB retained target and
96 MiB atlas representation; total driver-process memory includes swap buffers,
stencil, caches and other concurrent changes, so those arithmetic savings must
not be conflated with a causal measured A/B.

## Final one-unit tray-overflow cause

The first complete compositor-backed run passed 64/64 broad comparisons and 254/256
strict corners. The two failing top-right crops had three one-unit pixels each,
including two below the shade. A second original-Qt pre-swap bar readback reproduces
those values exactly, proving a separate bar AA cause, not matte noise.

Its QSG trace promotes the large right Row to a transform batch root at
(1603.65625,10); the three right pills are encoded relative to that origin. Native
chrome had used absolute output coordinates. Floating-point vertex arithmetic
therefore differs at those AA samples despite identical world geometry. Qt’s
[batch renderer](https://raw.githubusercontent.com/qt/qtdeclarative/dev/src/quick/scenegraph/coreapi/qsgbatchrenderer.cpp)
promotes transformed subtrees above 64 renderables or 1024 vertices. Native
`Node.native_batch_root` supplies the same scoped basis without clipping, layers,
textures or copies. The bar counts its QML subtree, including both image/fallback
text nodes in TrayButton.qml, and retains promotion for the transform lifetime.
Eligibility must not inspect empty retained-module placeholders: that would change
the decision between cached and uncached builds. The bar regression checks this.

A compact 5×84original-window RGBA golden proves the promoted basis exact and the
unscoped negative control non-exact. The real tray-overflow probe now passes 4/4
corner crops and its broad image comparison. Evidence:
`crates/hyprv-ui-bar/artifacts/corner-batch-probe2/`. The full final matrix was recaptured on one pinned ELF; its receipts and gate
summary follow below.

## Reproduce

Use the shared Cargo target with incremental compilation disabled. All compositor
sessions and matte background changes are private; reference files are read-only.

```sh
CARGO_INCREMENTAL=0 cargo build -p hyprv-shell
target/debug/hyprv-shell --nested-test crates/hyprv-ui-bar/artifacts/corner-rerun --fixtures parity/fullscreen/fixtures --references parity/fullscreen/reference
python3 -I parity/fullscreen/tools/compare.py crates/hyprv-ui-bar/artifacts/corner-rerun
python3 crates/hyprv-shell/tools/verify_corners.py crates/hyprv-ui-bar/artifacts/corner-rerun
CARGO_INCREMENTAL=0 python3 parity/verify/verify.py --backend gl
CARGO_INCREMENTAL=0 bash crates/hyprv-ui/tests/run-renderer-regressions.sh
```

The matrix covers the currently discovered DP-4@2 (6144×3456) references, not every
monitor/driver/fractional scale. Component fixtures cover their declared DPRs.
The optional llvmpipe run has an observed existing QuickAdjust frozen-golden
assertion difference at dark pixel(31,18): alpha6 vs5; final source reproduced it.
That scene uses neither the Canvas atlas nor an explicit promoted batch root.
No tolerance or test was changed. Other llvmpipe surfaceless checks and all private
Wayland lifecycle checks pass; the two newly frozen NVIDIA window-oracle tests
explicitly report UNAVAILABLE on llvmpipe. The target NVIDIA suite passes all 23.

## Final results

| Check | Result |
|---|---:|
| Assembled jobs |26/26 PASS|
| Full-screen GPU images, native + independent |64/64 PASS|
| Full-screen worst max / painted MAE / IoU |5 / 0.0112 / 1.0|
| Strict unmasked corner crops |256/256 bit-exact; max 0|
| Bar canonical GPU |300 PASS +12 declared exclusions; zero missing;312 rendered|
| Bar owner coverage metric |312/312 PASS;308 exact; max 2 in calendar-host DF text|
| Island canonical GPU |380/380 PASS|
| Panels canonical GPU |300/300 PASS|
| Ordinary UI/shell/bar tests |64+59+39 =162 PASS|
| Explicit NVIDIA GPU tests |23 PASS|
| All-target UI/bar/shell Clippy |PASS, warnings denied|

Before FR-NET-3 ([independent review](final-review-svc-net.md)): 0/256 exact, max 4. After atlas/topology/compositor repair:254/256
exact, max 1 in two tray-overflow crops. After promoted batch-root repair:256/256
exact, max 0. No painted-pixel mask was used in any of these corner checks.

Pinned validation ELF (debug, runtime GL code):
`f464a001c0b97fdd6799fa26e8f5a9995b6d3b09e098c197a0c1560eb4de8c36`.
It differs from the release ELF used for the bounded memory measurement above;
this report is renderer evidence, not a staged-release cutover receipt. Source
inputs were recorded before the first matrix job and are unchanged at completion.
All three canonical component receipts match the final dependency hashes.

Receipts under `crates/hyprv-ui-bar/artifacts/corner-exact/`:
`summary.json`, `fullscreen-results.json`, `corner-comparison.json`,
`compare/compare.json`, `binary.json`, `source-inputs.json`, `freshness.json`,
`component-controls.json`, `component-inputs.json`, per-job manifests and PNGs.
Large finished scratch ELF pins are retired after hashing under the disk policy;
compact receipts, source/golden provenance and referenced PNGs remain.

| State | Passed | Light max / worst MAE | Dark max / worst MAE |
|---|---:|---:|---:|
| calendar | 2/2 | 4 / 0.0012 | 4 / 0.0013 |
| calendar-morph | 20/20 | 4 / 0.0019 | 4 / 0.0020 |
| control | 2/2 | 4 / 0.0015 | 4 / 0.0018 |
| control-open-close | 22/22 | 4 / 0.0010 | 4 / 0.0012 |
| idle | 2/2 | 4 / 0.0010 | 4 / 0.0008 |
| island-expanded | 2/2 | 4 / 0.0015 | 4 / 0.0016 |
| media-playing | 2/2 | 4 / 0.0017 | 4 / 0.0018 |
| stats | 2/2 | 4 / 0.0026 | 4 / 0.0036 |
| tray-menu | 2/2 | 4 / 0.0008 | 4 / 0.0017 |
| tray-overflow | 2/2 | 5 / 0.0112 | 5 / 0.0097 |
| tray-submenu | 2/2 | 4 / 0.0007 | 4 / 0.0013 |
| volume | 2/2 | 4 / 0.0007 | 4 / 0.0007 |
| wifi | 2/2 | 4 / 0.0004 | 4 / 0.0009 |


Small validation logs are retained in `artifacts/corner-exact/checks/`;
`retired-binaries.json` records the hashes of the finished scratch executables
removed under the disk policy. No private Cargo targets were created.
