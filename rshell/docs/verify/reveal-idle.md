# Reveal-frame and idle wake follow-up

All interaction tests use private nested KWin/Hyprland with mocked services.
Controlled-quiet CPU uses the production service graph, read-only, with private
agent inputs and PID namespace. No live desktop, provider or device was changed.
Cargo uses the shared target with CARGO_INCREMENTAL=0.

## Changes

- Production `Application::connect` creates its shared EGL context and executes
  the native material/MultiEffect path on a small private framebuffer before
  accepting input. A startup GPU barrier completes compilation/uploads there.
  It maps no warmup window and installs no timer. The real renderer retains the
  same native cache epoch/context for subsequent surfaces.
- Profiling observed a cold GL effect-program compilation of29.47ms inside75ms
  of startup preparation. That work now occurs before a window frame or input.
  This is evidence of a moved cold pipeline cost; the exact cause of the old
  isolated35.10ms sample cannot be reconstructed from its coarser trace alone.
- A settled frame acknowledgement skips the host update callback itself via
  `UiHandler::update_after_frame`; a fallback guard also avoids reconciliation unless a
  provider/input/output change, animation, missing native surface or due deadline
  requires it. Missing-surface recovery and output removal remain active.
- The host caches the armed absolute component deadline until an update consumes
  it. The Wi-Fi blink regression verifies that a due500ms phase cannot silently
  roll forward to1000ms when a static frame completes.
- Coalesced provider notifications retain stats/other bits. A sole stats update,
  with no popup or animation, snapshots stats once and updates the retained bars;
  island inputs, panel models and hidden calendar closures are not rebuilt.
  Mixed notifications retain the full update path.
- Tray icon cache identity follows pixmap/name/theme inputs, not title/tooltip
  metadata. The visibility-gated bridge ignores title/tooltip-only changes when
  tray priority/network-filter classification is unchanged. Menu data, click
  flags, identities, icon pixels and classification changes still notify.
  The concurrently delivered bounded async tray loader preserves this identity.
- Integration caught an unconditional native tray-cache cleanup in mocked
  refresh. It is now guarded to the non-fixture production graph, with a startup
  regression test that panics if a native-provider dereference returns.

## Battery frame gate

Timing runs enable HYPRV_UI_PROFILE=1 and HYPRV_FRAME_BUDGET_MS=16.7. They start
with real virtual-pointer input, hold the card open for600ms after the open-state
assertion, then close and wait400ms. This covers the completed reveal and fully
visible content. The ordinary input-only smoke case previously clicked outside
approximately130ms after release, interrupting its reveal; those earlier timing
samples are retained but are not the complete-reveal gate.

`battery-cycle-profile.csv` records all frame-begin/submitted pairs; the test
writes `battery-frame-budget.json` and fails if any measured interval exceeds
16.7ms. The boundary includes render/swap to first GPU submission, not compositor
presentation or physical display latency. Startup setup and the declared reveal
pause/duration are separate from rendering work. Test runs disable the driver's
shader disk cache to exercise fresh-process preparation.

Final whole-reveal results are recorded in `target/reveal-idle/` together with
per-run CSV/JSON and the pinned ELF receipt. Final whole-reveal totals: **291/291 frames passed**, zero over16.7ms.
Median0.646ms, p99 (sorted index floor((n-1)*0.99))1.691ms, maximum1.946ms. Across six fresh
processes, native release receipt to first buffer was8.23–11.16ms
(median9.19ms); controller/content preparation was1.90–2.40ms.
`target/reveal-idle/timing-summary.json` and `complete-{0..5}-profile.csv` retain
all per-run measurements. The pinned release SHA256 is
`b3ea54e817a5a33126a08160a7d62dcca87df7dba07903906aad67f82c27e457`.

## Idle attribution

Separate instrumented production runs (approximately60s after warmup):

| Counter/scope | Before | After, detailed tray trace |
| --- | ---: | ---: |
| Stats notifications |61 |61 |
| Tray notifications |10 |12 |
| Full model refreshes |72 |13 |
| Stats-only updates |0 |61 |
| Skipped settled assembly passes |0 |75 |
| Panel-model constructions |290 |26 |
| Island-input constructions |72 |13 |
| Assembly inclusive wall time |39.44ms |20.29ms |
| Submitted frames |72 |74 |

All12 tray notifications in the detailed trace correspond to changed pixmap
bytes and tooltip data. The remaining full update is the minute clock. There is
no unexplained UI polling/reconciliation loop: new frames follow changed stats,
real tray image changes or the changing minute label. Wayland frame/presentation
acknowledgements still arrive for real rendering; they do not rebuild models.
The stats formatted-output regression confirms that generation/sub-label-only
changes construct zero modules and damage zero nodes; real changes retain other
modules and damage only the changed region.

Raw instrumentation: `target/reveal-idle/idle-before.csv`, `idle-after.csv` and
`idle-tray-trace.csv`. Scopes are inclusive wall time and overlap; they must not be
summed as exclusive CPU. Diagnostic counters/clocks are disabled in CPU-budget
runs. The private functional harness has a test clock; production does not use it.

## Controlled-quiet CPU

Each row below is a separate uninstrumented60s run using
`docs/verify/measure-shell.py --quiet-agents --idle-only`, verified DPR2
6144x3456/3072x1728 geometry and zero private agent hook events.

| Checkpoint | Process CPU | Summed thread CPU | Main/UI thread CPU |
| --- | ---: | ---: | ---: |
| Previous final quiet release |0.2163% |0.2261% |0.1435% |
| Engine changes |0.1995% |0.2137% |0.1214% |
| Merged async-loader release |0.1997% |0.1925% |0.1080% |

The merged row is7.69% below the previous process measurement; main-thread
scheduler runtime is24.7% lower. A process CPU tick over60s is about0.0167
percentage points, so the process change alone is only about one tick; the
higher-resolution thread runtime and direct work counters are the stronger
cross-checks. Real service/animated tray traffic varies between runs. These are
whole-binary observations, including concurrent service changes, not isolation of
every difference to one patch. The separate whole-shell0.1% budget remains unmet.

Raw receipts, geometry and ELF hashes are under
`docs/verify/raw/perf-shell/{reveal-idle-final,reveal-idle-merged-final}`. A final
same-binary quiet repeat after all interactive verification is appended below.

## Regression evidence

Owned ordinary tests, strict all-targets Clippy,11 explicit renderer/EGL resource
and cache tests, native GL Qt goldens and84 Canvas stroke golden cells are run.
The final pinned release passes **204/204** native input cases. Owned ordinary
tests: **179 passed,0 failed,24 ignored**. Strict owned all-targets Clippy: **PASS**.
Explicit renderer/EGL regressions: **11 passed**; native Qt GL goldens: **3 passed**;
Canvas stroke golden test: **84 cells pass**. Initial fixture-startup failures from the tray-loader handoff are
retained as failed evidence, not counted as successful input tests.

## Later consolidation checkpoint: tray correction and broader cold frames

Production theme icons now use the widgets `tray_raster` helper with a
scale-1 theme lookup and an already device-scaled request (36 px at DPR 2).
SVG/custom-path rendering follows the same request. See
[tray-provider-size.md](tray-provider-size.md) for focused coverage.

Additional startup preparation exercised A8 picture/image variants. These
synthetic variants did **not** remove every cold calendar/control frame and
must not be credited as a solution to those samples. The default EGL copy now
uses a GL3 framebuffer blit, with the previous Skia copy retained for ES2 and
`HYPRV_SKIA_WINDOW_COPY=1`. Explicit `HYPRV_VERIFY_FRAME_COPY=1` compares actual
EGL back-buffer RGBA8 from both paths byte for byte; the battery A/B run passed
41 comparisons. Readback diagnostics are excluded from frame timing claims.

Broader production activity still has an unresolved cold calendar path:
`activity-copy-trace.csv` records scene rendering30.02ms / frame38.30ms,
while GL blit max0.544ms; an earlier repeat attributed32.74ms to the copy scope.
Thus the broader all-surfaces16.7ms goal is **not yet met**. Calendar model
construction also reaches89.4ms outside the paint scope; the panels owner has
a shared-FontMgr integration request. These are separate from the completed
battery reveal samples above. No claim of a globally frame-bounded renderer
is supported by the current evidence.

A tray-corrected60s quiet sample measured0.2494% process /0.2404% summed thread /
0.1203% main-thread CPU. This window overlapped a shared release build; it is
retained, not silently discarded, and is not a clean improvement result.
The next30s instrumented quiet sample measured0.1328% process, but has coarser
CPU tick resolution and profiling overhead; it is not a substitute for the
final60s uninstrumented comparison.

Stale engine ELFs were retired under the disk policy. Compact SHA256/size
receipts remain in `target/reveal-idle/retired-elf-receipts.json`; historical
CSVs and JSON gate results are retained.

Current tray-corrected release validation (`current/hyprv-shell`, SHA256
`f31e62f911ca9dff88cb68638c0df3427910870a29bbf2bd345d534ad3a801a6`):
204/204 native functional cases pass. Six fresh-process, shader-disk-cache-disabled
full battery cycles pass285/285 frame intervals under16.7ms: median0.639ms,
p992.059ms, maximum6.346ms. Release-to-first-buffer values are8.53,10.05,10.97,
11.36,12.19,19.63ms (median11.16ms); the19.63ms sample is retained and exceeds
one nominal60Hz interval despite its render frame remaining under budget.
Thus this does not establish a universal input-to-buffer16.7ms guarantee.
Raw evidence: `tray-budget-summary.json`, `tray-budget-{0..5}/run-*/fixture/`
and `functional-current/manifest.json` under `target/reveal-idle/`.
Renderer regressions10/10 and coordinated-cache regression1/1 pass; current
scoped all-targets Clippy passes with `-D warnings`. UI library42 pass/11 ignored,
shell48 pass/1 diagnostic ignored. The aggregate widgets run has a controls-owned
tray reference probe failure, recorded separately; no aggregate pass is claimed.

Final60s instrumented attribution (`idle-final-profile.csv`):61 stats-only
updates,1 tray notification with changed pixmap bytes, and2 full refreshes
(the tray change and minute clock). Island inputs are constructed2 times,
panel models4 times. There are63 submitted frames and127 engine schedules;
only64 host sync calls remain (61 narrow stats,2 changed-data refreshes and
1 skipped explicit diagnostic request). Settled static frame acknowledgements
therefore perform no host model update. Assembly inclusive wall time16.08ms
for the entire minute, of which the stats-only path accounts for15.29ms.
Stats updates built179 changed modules and edited227 nodes in this real-data
window; the unchanged-formatted-output regression still constructs/damages zero.
Protocol acknowledgement dispatch remains necessary for real submissions.

Final uninstrumented60s controlled-quiet run, after builds and interaction tests
ended: **0.149591% process**, **0.158555% summed threads**, **0.097720% main/UI**.
This is30.84% below the previous0.2163% process measurement and64.03% below the
historical0.4159%. Private hook events0; exact geometry/DPR checks passed.
The same pinned ELF hash appears in the input suite, frame gate and CPU receipt.
Raw report:
`docs/verify/raw/perf-shell/reveal-final-quiet/reveal-final-quiet-idle.json`.
The earlier0.2494% overlapping-build sample is retained above. Tick resolution
and varying real tray/provider activity still limit causal precision; the
higher-resolution main-thread runtime and direct work counters cross-check the
removed UI reconciliation. Whole-shell0.1% remains unmet; no zero-process-CPU
claim is made. The relevant zero-work property is absence of host rebuilds for
unchanged static frame acknowledgements; changed stats, minute labels and tray
pixels continue to update.
