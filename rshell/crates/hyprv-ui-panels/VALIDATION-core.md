# Assigned panel share — strict GPU acceptance, 2026-10-23

The bar/island owner’s assigned share improves **177/184 →184/184 gate passes**.
**176/184 are bit-exact**. Eight DF-flagged Mihomo captures remain nonexact, max3;
these meet the canonical distance-field tolerance. No crop, mask, comparator,
fixture or reference changed. The combined-tree control is **298/300** with no
lost baseline passes; the remaining2 are spec-bar’s QuickAdjust crops.
Concurrent owner fixes account for improvements outside this assigned share. Runtime remains Qt-free.

The user’s split leaves QuickAdjust, calendar and startup with spec-bar. Counts
below are fixed cohorts, compared with the255/300 combined-tree baseline captured
before this task. Viewflow’s main/off appearances are also covered by the main-page
cohort; the separate VM fixture contributes four captures.

| Assigned cohort | Before gate | After gate | Exact after | Worst max after |
|---|---:|---:|---:|---:|
| Control panel main/pages | 124/124 | 124/124 | 124/124 | 0 |
| Bluetooth | 16/16 | 16/16 | 16/16 | 0 |
| Wi-Fi | 16/16 | 16/16 | 16/16 | 0 |
| Stats | 9/16 | 16/16 | 16/16 | 0 |
| Mihomo | 8/8 | 8/8 | 0/8 | 3 |
| VM/Viewflow | 4/4 | 4/4 | 4/4 | 0 |
| **Total** | **177/184** | **184/184** | **176/184** | **3** |

## What closed the seven failures

**Process delivery clock, not Canvas rasterization.** The original QML network
history begins `[0,2155.7894736842104,512000,...]`; the replay translator supplied
`[0,2048,512000,...]`. `parity/qml-mocks/Quickshell/Io/Process.qml:78–88` schedules
at0 through a Timer whose interval is at least1ms. With the fixture’s50ms ticks,
the initial line arrives at50ms; elapsed time to1000ms is950ms. The translator
now defers delivery until the positive clock tick and uses the actual floored
delivery clock for `Date.now()` (`shell.qml:2386–2414`). An ordinary regression
asserts the independently observed samples. Runtime service sampling is unchanged.

The raw original-QML CPU/RAM Canvas textures were already exact. Installed
QPainter given the old Rust points matched Rust exactly, exonerating the painter
port. With corrected inputs, the network texture is exact at both DPRs, closing
the history-curve residue (max3@1/max2@2).

**Explicit layer projection.** Four dark@1 stats images retained the same three
chrome-edge pixels, four total integer channel-error units per image. Only one
source alpha byte differed, propagating into two blur levels. Actual Qt GL uploads
showed `2/636 =0.00314465398`; Rust factored through texture height572, computing
`float(2/572)*float(572/636)=0.00314465421`. Qt projects the original padded logical
extent directly, independently of `layer.textureSize` (`AnimatedGlassPanel.qml:180–214`).

The active native target now holds that root orthographic basis for merged
rectangles. The offscreen target guard restores it; physical pixel size, meshes,
shaders, programs, caches, unmerged transforms and other materials retain their
existing paths. Renderer supplies it only for explicit source texture sizes.
The independent installed-Qt golden preserves the3072×1728 viewport and checks
both DPRs. A numeric regression asserts Qt’s matrix bits and proves the old
factorization differs. All temporary runtime source-readback hooks were removed.

A local type alias in `qtext::shared_font_data` clears its preexisting Clippy
complexity warning; font storage and rendering behavior are unchanged.

## Evidence and controls

[Aggregate acceptance receipt](artifacts/core-panels/accepted-summary.json).
The combined full run saw only an unassigned calendar-test edit; its52 captures
were independently rebuilt/replayed afterward, all52 pass with stable source and
reference hashes. The final300-image receipt composes those52 with the unchanged
248 non-calendar captures. Relevant assigned-share sources did not drift; bar and
island source/reference fingerprints are stable and match the final tree.

Ordinary all-target tests: **177 passed,0 failed,24 intentionally ignored**;
strict all-target Clippy passes. Seven GPU goldens and12 renderer/private EGL
checks pass separately. Final stress:6,000 frames,4 mapped private surfaces,
1,974 resizes,63 recreates,2,662 partial-damage frames,776 readbacks,0 mismatches.

- [Immutable starting result](artifacts/core-panels/baseline.json).
- [Final assigned-share metrics](artifacts/core-panels/final-share.json),
  [full replay source fingerprints](artifacts/core-panels/final-panel-source-hashes.json).
- [Numeric causal receipt and removed raw-capture hashes](artifacts/core-panels/causal-receipt.json).
- [Clock-only checkpoint](artifacts/core-panels/sample-clock/metrics.json):12/16 stats exact.
- [Projection checkpoint](artifacts/core-panels/layer-projection/metrics.json):16/16 exact.
- [Independent layer oracle hashes](artifacts/core-panels/layer-oracle/oracle.json).
- [Settled calendar control](artifacts/core-panels/accepted-calendar/results.json):52/52 pass.
- [Ten surfaceless renderer and two private EGL regressions](artifacts/core-panels/renderer-regressions.log).
- [All-target tests](artifacts/core-panels/final-tests.log),
  [strict Clippy](artifacts/core-panels/final-clippy.log).
- [Seven native GPU goldens](../hyprv-ui-bar/artifacts/panels-final-goldens/run.log).
- [Mapped multi-surface stress](../hyprv-ui-bar/artifacts/panels-final-stress/results.json).
- Final bar and island controls remain **304/304** and **380/380 bit-exact**:
  [bar gate](../hyprv-ui-island/artifacts/panels-final-bar/gate/gate.json),
  [island gate](../hyprv-ui-island/artifacts/panels-final-island/metrics.json).

The eight Mihomo images are not described as exact or entirely reference noise.
A fresh installed-Qt online-page capture differs from its frozen reference by max1,
while Rust differs by max3@1/max2@2. The existing DF approximation remains visible
numerically, within the accepted tolerance:
[Qt control](artifacts/core-panels/mihomo-qt-control.json). No tolerance was widened.
The earlier unreproduced GPU-corruption risk remains OPEN in `docs/STATUS.md`.
