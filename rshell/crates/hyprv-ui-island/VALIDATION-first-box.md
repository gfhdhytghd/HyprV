# Island exactness checkpoint — 2026-10-23

The production GPU replay improves **0/380 → 368/380 exact**. All380 crops render;
none are excluded and the canonical premultiplied gate is unchanged. Total integer
premultiplied absolute channel error falls **31,556,797 → 119,801** (99.6204%).
The gate remains open. Source hashes did not change during the final replay.

## Per-class progress

Cohorts stay fixed to each crop's initial QA label, so the table does not move a
crop between categories as its largest remaining contributor changes. All initial
fill/color, text and image/icon crops are now exact. Residual pixels in other
cohorts are rectangle edges; “canvas” here is its initial cohort, not a claim that
Canvas rasterization is still wrong. The standalone six-cell Canvas probe is
exact in all four theme/DPR captures.

| Initial QA class | Exact before | Exact after | Absolute PM channel error before → after |
|---|---:|---:|---:|
| fill/color | 0/140 | 140/140 | 258,825 → 0 |
| canvas | 0/102 | 98/102 | 465,001 → 31 |
| text | 0/54 | 54/54 | 1,348,397 → 0 |
| animation | 0/44 | 42/44 | 26,143,621 → 7 |
| rect AA | 0/35 | 29/35 | 3,326,278 → 119,763 |
| svg/icon | 0/5 | 5/5 | 14,675 → 0 |

Current spatial classifier:5 fill/color and7 rect-AA failures. Eleven crops have
max1 with only **49 total channel-error units**. The twelfth crop is a stale
hover state in the frozen reference and contributes119,752 units/max161.

## What changed

- `Island::build` opts into the existing Qt GPU materials; NativeRendering leaves
  select QtGl, including shared controls. Reconciliation preserves those flags.
  Shared widget production sources are unchanged.
- `AudioSpectrum.qml:31-105` is reproduced with island-local QPainter Canvas
  quadratic paths and JS double math. Idle-wave phase uses f64 and the replay
  no longer advances an extra initial driver tick. Compact content follows the
  animated frame centre (`DynamicIsland.qml:850-1227`).
- Text preserves item origin/width and applies alignment within the line. Centre
  anchors use Qt6 rounding. Native scaled glyph masks raster at output DPR, then
  transform/snap their quads. Qt's QFixed truncation applies on the native GL path.
- Progress-fill Behavior restarts from its last presented value. The dependent
  radius job advances once before repeated height-binding retargets. Media-scale
  jobs created during polish start next turn, matching the observed transient
  hover trace. Origin/centre-transform arithmetic keeps qreal precision until
  conversion to the scene graph's float matrix.
- The omitted hard clips on islandFrame, musicExpandedFrame and AlbumArtFrame
  now match `DynamicIsland.qml:737,777,403`.
- Shared engine fixes are surgical: GPU rounded masks reuse the consolidated
  rectangle mesh; Canvas uses the existing native image material's float opacity;
  masks sample NEAREST while their source samples LINEAR; scissors use Qt's float
  projection and lower-left qRound; QPainter outline conversion uses Qt's SSE2
  signed-half addition/truncation. Elision includes terminal whitespace when
  measuring prefixes, fixing the extra space before the ellipsis.
- Capture transport matches QImage's SIMD unpremultiplication. The independent
  QImage oracle verifies all32,896 valid8-bit channel/alpha pairs. Runtime rendering,
  references and comparison thresholds are not adjusted by this transport fix.

No additional rectangle rendering mode or Qt runtime dependency was added. The
candidate separate mask shader and two rounding/diagonal experiments were removed
because they did not improve the independent probes. Extra qreal geometry helpers
were likewise reverted after producing identical PNGs.

## Stale-hover reference: causal proof

`music-expanded-playing--dark@2.png` is bit-identical between ordinary fresh Qt
and Rust. Both differ from the frozen capture by119,752 channel units. Moving
Qt's real pointer onto Previous in an owned fixture copy, then advancing200ms,
reproduces the frozen image **exactly** (max0/error0/pixels0). This intervention is
not applied to runtime or acceptance replay: the crop remains a gate failure.

Qt6.12 throttles frame-synchronous hover with a100ms real QElapsedTimer, while
this host virtualizes animation/wall-clock time but leaves monotonic time real.
The resulting capture can retain stale containsMouse/scale state. QA needs a
controlled hover-timing protocol and an explicit reference decision. Simply
changing the hover interval to0 does not replace the separate deferred-animation
startup rule; both were investigated independently.

Evidence: [intervention](artifacts/hover-intervention/result.json),
[tool](tools/prove_hover.py), original-QML property traces in `artifacts/observe-*`.

## Validation and other UI controls

- `CARGO_INCREMENTAL=0 cargo test -p hyprv-ui -p hyprv-ui-island --all-targets`:
  **96 ordinary tests pass**, with deliberately ignored GPU tests run separately.
- Strict all-target Clippy for both changed crates passes.
- Five native GPU golden tests pass, including independent fractional mask and
  fractional clipped-rectangle images atDPR1/2. Ten surfaceless renderer regressions
  and the private-Wayland coordinated-cache-flush regression also pass.
- A6,000-frame production EGL stress run covers4 mapped private surfaces,
  1,974 resizes,63 window recreations and2,662 partial-damage frames. All23,994
  readback checks match26 GPU oracle scenes; **zero mismatches**. This adds evidence
  while the existing generic GPU-corruption risk remains OPEN in docs/STATUS.md.
- The matched frozen bar control remains **303/304**, exact191 and painted
  MAE0.025054/255, unchanged. A private small adapter retains its old letter icon
  input semantics while using the production full viewport; the live QA adapter
  is not reverted. Rendering that control with a crop-sized viewport instead
  exposes39 max1 failures, so viewport protocol must also remain matched.
- Current panels/calendar: **82/300 → 86/300**, no prior gate pass lost. The panel
  owner independently enabled native materials/text during this work; the result
  proves the current combined tree and is not attributed solely to these changes.
  Source hashes remain unchanged during the final300-crop replay.
- The current QA bar references changed missing-icon semantics and live tray-icon
  source sizes. The latest live-reference check is **288/304 exact** (gatev1=300,max24):
  twelve max1–2 overflow crops and four submenu crops, recorded in
  [the current-input receipt](artifacts/bar-current/gate/gate.md);
  mixing its adapter/assets with the frozen references is invalid control evidence.

Receipts: [island metrics](artifacts/final/metrics.json),
[cohorts](artifacts/final/progress.json), [source freeze](artifacts/final/source-hashes.json),
[tests](artifacts/current-acceptance-tests.log), [Clippy](artifacts/current-acceptance-clippy.log),
[GPU goldens](../hyprv-ui-bar/artifacts/island-qfixed-native-tests/run.log),
[renderer/cache tests](artifacts/renderer-regressions.log),
[stress](../hyprv-ui-bar/artifacts/island-final-stress/results.json),
[bar control](artifacts/bar-frozen/gate/gate.md),
[panels control](artifacts/panels-cross/results.json).

The broader12-cell scaled-text diagnostic still exposes nonexact half-scale and
some scaled-clock cases. Those are not passing goldens; actual island lock scales
and this corpus are verified separately. Real fractional output DPRs and other
GPUs remain outside the frozenDPR1/2 corpus.

## Residual frozen crops

| Crop | Max PM channel diff | Absolute channel error |
|---|---:|---:|
| dual-agent-expand--expand-t0070--dark@2.png | 1 | 2 |
| dual-agent-expand--expand-t0070--light@2.png | 1 | 2 |
| dual-music-expand--collapse-t0060--dark@2.png | 1 | 1 |
| dual-music-expand--expand-t0030--dark@2.png | 1 | 13 |
| dual-music-expand--expand-t0030--light@2.png | 1 | 12 |
| dual-music-expand--expand-t0060--dark@1.png | 1 | 1 |
| dual-music-expand--expand-t0060--dark@2.png | 1 | 2 |
| dual-music-expand--expand-t0060--light@1.png | 1 | 4 |
| music-expand-collapse--collapse-t0030--light@2.png | 1 | 6 |
| music-expanded-nolength-hover-next--dark@1.png | 1 | 2 |
| music-expanded-nolength-hover-next--light@1.png | 1 | 4 |
| music-expanded-playing--dark@2.png | 161 | 119,752 |


## Storage and ownership

All Cargo builds reuse the workspace target with incremental compilation disabled.
After summarizing intermediate runs,184,484,508 bytes of owned duplicate captures,
GL texture/VBO dumps and CMake build output were removed; compact receipts and
final images remain (about57MiB). The shell/compositor was never installed,
reloaded or restarted. No panel production sources, shared widget production
sources, canonical fixture/reference data or live bar adapter were edited.
Every shared engine change is recorded in `docs/requests/hyprv-ui.md`.


Final concurrent-context refresh: after the engine owner changed lib.rs, egl.rs
and wayland.rs, all three UI GPU controls were rerun against the new tree.
Island remains368/380, frozen bar303/304 and panels86/300; no sources changed
during island/panel replay and the current engine/island source hashes match.
The updated context also passed another6,000-frame/four-surface stress run with
776 sampled readbacks, zero mismatches, and all five native GPU goldens.
[Current-context stress](../hyprv-ui-bar/artifacts/island-current-context-stress/results.json),
[current GPU tests](../hyprv-ui-bar/artifacts/island-current-gpu-tests/run.log).
The GLSL150 negative control produced380 identical PNGs and was removed. Exact
Qt meshes match for representative dynamic sizes and the44px control; neither
an extra geometry API nor a shader-version fork is retained.
