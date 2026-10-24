# UI stats tick — 2026-10-23

The required 1 Hz sampling remains enabled. All runtime tests use a private
nested compositor; the desktop session is only read for monitor/service state.
Targets: changed-tick main-thread CPU <0.3 ms; controlled-quiet whole-shell CPU
≤0.1%. Final measurements are recorded below; these targets must not be inferred
from microbenchmarks or coarse `/proc` CPU tick counters.

## Implementation

- The service bridge suppresses identical formatted labels while preserving
  severity thresholds. Stable-width stats changes patch only changed Text nodes;
  ambiguous, width or style changes fall back to the retained component update.
  Direct patches invalidate cached component signatures, preventing stale values
  on subsequent full updates. A bar-owned stats patch API remains requested in
  `docs/requests/hyprv-ui-bar.md`.
- Damage retains up to 16 disjoint rectangles. Device-pixel outward rounding occurs
  before clipping. The retained RGBA8 target repaints each region; EGL buffer age
  repairs only damage accumulated since the current back buffer was last used.
  Unknown age/history/resize falls back to a complete copy. Optional
  `EGL_KHR_partial_update` hints and swap-with-damage accompany this repair.
- Measurement, extent and painting share shaped runs and bounded face/advance/
  bearing caches. Integer native A8 glyphs pack losslessly into four bounded
  nearest-sampled atlas pages; warmed glyphs do not upload again. Consecutive page
  runs batch vertices into a streaming append buffer, orphaned only on wrap.
- Skia and native GL share command ordering and submit at EGL swap, avoiding
  per-node submit calls. Rectangular damage clears directly on the RGBA8 target.
  Stable surfaces do not resend unchanged viewport requests or request redundant
  presentation feedback outside animation.

## Measurement method

Use the unchanged `docs/verify/measure-shell.py --idle-only --quiet-agents` with
release ELFs pinned under `target/tick-ui/{before,final-append}/hyprv-shell`.
Normal budget runs omit all diagnostics/profiling flags. Fine CPU percentages use
summed per-thread `/proc/PID/task/TID/schedstat` runtime deltas divided by measured
wall seconds, including service and helper threads; the report's coarse CPU
percentage uses 10 ms process ticks and is retained separately.

For stage profiling, `HYPRV_UI_PROFILE=1 HYPRV_SHELL_DIAGNOSTIC=1` records wall
and thread CPU scopes without per-frame file writes. A stats tick spans
`stats.tick.begin` to `frame.submitted`; no-frame updates end at
`stats.model.end`. `egl.swap` includes vendor submission and the implicit
`wl_surface.commit`, which are not separate userspace calls in this EGL path.
Scopes nest: do not sum inclusive durations. GPU execution time is not CPU time.

An earlier instrumented baseline measured 0.1863% whole-shell schedstat CPU
(0.1523% UI). Intermediate experiments ranged 0.1487–0.2200%, illustrating
scheduler/driver and tray-event variability. A direct-default-backbuffer
experiment did not improve the result and was removed. These are not substitutes
for the final uninstrumented paired measurement.

The isolated `paced_submit` control uses the same bar-sized EGL surface at the
real output DPR, changing one pixel at 1 Hz without services/text/layout.
Its initial 32-tick run measured 0.553 ms mean main-thread CPU, including 0.317 ms
inside EGL swap. This identifies backend submission overhead on this setup;
it does not establish a universal hardware lower bound.

## Final evidence

The final 90-second normal release pair (no instrumentation) measured:

| Metric | Before | After |
| --- | ---: | ---: |
| Whole-shell fine schedstat CPU | 0.17919% | 0.14165% |
| UI-thread fine CPU | 0.15722% | 0.11830% |
| Services + agent process threads | 0.02197% | 0.02335% |
| Coarse process-tick CPU | 0.17752% | 0.14423% |
| Ending RSS | 147.13 MiB | 174.41 MiB |

Whole-shell CPU fell 20.95%; UI CPU fell 24.75%. This paired baseline was lower
than the user's earlier 0.21–0.26% observations. Do not substitute those older
observations for this measured baseline. This is one serial pair, not a confidence
interval or an isolated per-change bisect. The new bounded text/atlas caches trade
memory for repeated text work; ending RSS remains below the 320 MiB budget.

Receipts (including ELF hashes, timing endpoints, all thread counters and private
output configuration):

- `docs/verify/raw/perf-shell/tick-ui-before-normal/tick-ui-before-normal-idle.json`
  (90.132 s, instrumented=false).
- `docs/verify/raw/perf-shell/tick-ui-after-normal/tick-ui-after-normal-idle.json`
  (90.136 s, instrumented=false).
- `docs/verify/raw/perf-shell/tick-ui-append-profile/tick-ui-append-profile-idle.json`
  (30.121 s, instrumented=true).
- `target/tick-ui/final-numbers.json`, `append-profile.csv`, and
  `final-append.sha256`. Final ELF SHA256:
  `3d7ec2c1898ff268fd4b847654aa9d950f9f9eed7afa1a3762ea63f59d02629c`.

The final profiled run had 31 changed stats ticks and 33 submitted frames:

| Scope | Mean thread CPU | Interpretation |
| --- | ---: | --- |
| Tick begin → submitted | 1116.30 µs | Median 936.56; max 3311.20 µs |
| Stats model/update | 211.34 µs | Includes patch/layout fallback |
| Text patch validation | 66.41 µs | Included in stats update |
| Renderer paint | 105.33 µs / region | 68 regions; 217.05 µs / submitted frame |
| Native text draw | 10.88 µs / label | Included in paint |
| EGL buffer-age query | 9.59 µs | Per submitted frame |
| Retained → back buffer copy | 31.12 µs | Includes 28.53 µs GL blit |
| EGL swap / implicit commit | 628.16 µs | Vendor submit + Wayland commit, inclusive |

Ten ticks used the stable-width text patch; width/style changes used the safe
component fallback (176 module builds across the run). Only nine new native glyph
variants uploaded across the run; warmed-glyph regression tests forbid uploads
entirely. One tray update was also observed. Final instrumented whole-shell CPU
was 0.14975%, which is not the normal-release budget number above.

**Both requested targets remain unmet.** The 0.3 ms tick target cannot be claimed
from the 0.211 ms model path: painting and submission are part of the tick. EGL
swap/implicit commit is the largest remaining measured stage, exceeding the tick
budget by itself in this run. The standalone one-pixel control above confirms
submission overhead without text/layout, but does not prove that further driver
or architecture improvements are impossible.

## Regression gates

- Production GL: bar 304/304 exact; panels/calendar 300/300 pass (pre-existing
  distance-field max 3); island 380/380 exact. All **984 output PNGs byte-identical**
  to the pre-task GL snapshot. Receipts:
  `target/tick-ui/parity-final/{byte-comparison.json,qa-comparator-results.json}`
  and `bar-report/gate/gate.json`.
- Fresh private-compositor native-input E2E: **204/204 PASS**, complete manifest at
  `target/tick-ui/e2e-final/manifest.json`; matrix in `docs/verify/e2e.md`.
- `crates/hyprv-ui/tests/run-renderer-regressions.sh`: 14 surfaceless GPU tests,
  one partial EGL sequence, three private cache tests and two back-buffer audit
  tests pass. Forced atlas VBO wraps match the original mask shader; disjoint
  damage matches full redraw; warmed numeric labels reuse existing glyph uploads.
- Stats patch tests compare fresh pixels at DPR 1/1.5/2 and cover unchanged rounded
  labels and width/severity fallbacks. Damage/history/metric cache tests pass.
- Strict all-target clippy for hyprv-ui, hyprv-shell and hyprv-widgets passes
  (`target/tick-ui/clippy-append.log`).

Intermediate experimental ELFs were retired after preserving hashes in
`target/tick-ui/retired-binaries.json`; all builds reused the shared target.

