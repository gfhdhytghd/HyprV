# Bar validation — 2026-10-23

## Current recaptured-reference acceptance

The current production GPU replay is **304/304 bit-exact**, up from288/304 at
this follow-up’s start. All structural checks pass; maximum difference and painted
mean error are zero. Four submenu crops needed the fixture’s remaining hover/move
steps replayed after page replacement. Twelve tray-overflow crops needed the exact
font-derived idle island width120.15625 instead of the rounded default120.16.
Both fixes are in the reference adapter; production geometry/input APIs are unchanged.

Sum of absolute integer-premultiplied RGBA channel error (spatial masks):

| Category | Before | After |
|---|---:|---:|
| Rect edge AA | 105,349 | 0 |
| Text | 2,484,277 | 0 |
| SVG/icons | 18,603 | 0 |
| Compositing | 2,271,360 | 0 |

Regression tests exercise the replacement submenu’s hovered row and exact Qt idle
width/status-pill position. The shared engine’s relative-transform, smooth-color
batch and QRectF precision changes also retain304/304. Current canonical replay:
[accepted-bar/gate/gate.json](../hyprv-ui-island/artifacts/accepted-bar/gate/gate.json),
with source/reference/binary fingerprints beside it. Immutable starting receipt:
[followup-bar-baseline.json](../hyprv-ui-island/artifacts/followup-bar-baseline.json).

Island control is380/380 exact; panels retains all255 pre-refinement passes
(255/300). All-target tests, strict Clippy, six GPU goldens and a6,000-frame
four-mapped-surface stress run pass; links and limitations are in
[the island acceptance report](../hyprv-ui-island/VALIDATION.md).
No bar reference, comparator, mask or threshold was edited in this follow-up.
The approved island fixture change is separately logged in
`docs/requests/parity-capture.md`. Runtime remains Qt-free.

The sections below record historical checkpoints against older inputs.

## Exact native crop closure — second 90-minute task, 2026-10-23

**All 64 targeted max-1 crops are exact.** Production GPU gate **239→303/304**;
exact images **47→191** (native 111/112 exact; DF 80/192 exact, all 192 pass).
Painted MAE **0.02561787134751602→0.02505368580906826**. No gate pass was lost,
and no image maximum or painted mean increased. The sole remaining gate failure
is the unchanged frozen `tray-menu-submenu--dark@2` input-state mismatch (max 23).
It is excluded from the native rendering attribution below, as in the prior table.
No reference image, fixture state, comparator or acceptance threshold was changed.

Sum of absolute integer-premultiplied RGBA channel error, native-only crops:

| Category | Before | After | Reduction | Nonexact crops before→after |
|---|---:|---:|---:|---:|
| Rect edge AA | 1,285 | 0 | 100% | 41→0 |
| Native text | 0 | 0 | — | 0→0 |
| SVG/image icons | 2,811 | 0 | 100% | 13→0 |
| Compositing | 1,283 | 0 | 100% | 62→0 |

Combined error **5,379→0**. Categories are spatial masks, not separate causal
contributions. Receipt: `artifacts/exact2-final/{gate-0/gate.json,progress.json}`;
immutable starting receipt: `artifacts/exact2-baseline/gate-0/gate.json`.

The fixes preserve Qt's actual GPU operations: top-left effect source targets;
canonical indexed rectangle strips and batch coordinate bases; explicit image
position/UV vertices; full-opacity texture shader selection independently of alpha
blending; original output-sized fixture projection before cropping; and QSG's
TL–BR effect-quad diagonal. The final two battery crops were five channel units
each: their source and all four blur levels were byte-identical to Qt, and matching
the final quad triangle order removed the remaining interpolation rounding error.
Runtime remains Qt-free. Actual Qt shader/VBO/index observations and raw stage
comparisons are retained under `artifacts/qt-battery-{stage,root}` and
`artifacts/rust-battery-stage`; temporary runtime stage-readback hooks were removed.

Regression/verification receipts:

- Engine+bar all-target tests: **94 passed, 0 failed, 18 ignored** (engine 59/17,
  bar 35/1); strict all-target Clippy passes. Initial broad link attempt hit a full
  filesystem; retry after removing only stale generated bar example build files
  passed. Logs: `.build/exact2-final-{tests-retry,clippy3}.log`.
- Independent Qt batch/image/layer goldens cover DPR 1/2, inherited image opacity,
  hard clip, fractional image vertices, and an off-center 324x190 effect in the
  original 3072x1728 viewport. Restoring the old quad diagonal fails both DPR2
  cells at max 1 (five channel units across four pixels each); corrected order makes all 4 exact. Final private GPU integration:
  **3 passed** (`artifacts/exact2-output-gpu-tests/run.log`). Portable source `tools/batch_material.qml`;
  engine goldens/provenance `../hyprv-ui/tests/fixtures/batch-materials/`.
- Final synthetic GPU stress: **6,000 frames, 4 surfaces, 776 checkpoints,
  1,974 resizes, 63 recreations, 2,662 partial-damage frames, 0 mismatches**;
  separate mapped-surface run repeats those totals with **0 mismatches**.
  Receipts `artifacts/exact2-final-{stress,mapped}/run.log`.
- Final all-three production-EGL parity: bar **303/304**; all **380 island and 300
  panels/calendar images byte-identical** to the disabled-native-material control
  on the same pinned binaries. Their strict published gate counts remain 0/380 and
  82/300; this proves no regression, not that those owners pass the strict gate.
  Receipt `artifacts/exact2-final-cross/published-comparison.json`, with canonical
  verifier before/after reports beside it. No other UI source was edited.
- Two full pinned corpus stability replays: **608 frames, 0 mismatches**, compared
  byte-for-byte against the independent 303/304 final oracle. Receipt
  `artifacts/exact2-final-repeated/run-0/stress.json`. Binary/asset hashes are retained.
  Together with both final synthetic stress runs, 12,608 checked frames are clean.

## GPU closure checkpoint — 2026-10-23, 90-minute task

**230→239/304 gate passes**, **38→47 exact**, interim v1 **303/304** and
structural **304/304**. Painted MAE **0.02787943475047115→0.02561787134751602**.
Remaining **65 failures: 64 native-only max-1 crops and the frozen submenu
input-state mismatch at max23**. All192 distance-field crops pass. No gate passes
were lost and no image maximum increased. One corner's painted mean increased
from0.0008974358974358974 to0.0009615384615384616; it remains max1.

Per-category progress below is the sum of absolute **integer-premultiplied RGBA
channel error**, over native-only crops excluding the already documented submenu
input mismatch. Categories are spatial masks, not independent causal attribution.
They must not be confused with the all-image ranking polluted by that mismatch.

| Category | Before | After | Reduction | Nonexact crops before→after |
|---|---:|---:|---:|---:|
| Rect edge AA | 10,204 | 1,285 | 87.4% | 71→41 |
| Native text | 73,102 | 0 | 100% | 44→0 |
| SVG/image icons | 6,415 | 2,811 | 56.2% | 26→13 |
| Compositing | 3,971 | 1,283 | 67.7% | 71→62 |

Combined error **93,692→5,379 (94.3% lower)**. Final residual order is icons,
rect edges, compositing, text. No frozen images, state fixtures, gate thresholds
or comparators were changed.

Changes and independent checks:

- Direct GL text now matches QQuickText's QRgb8 channel storage, then inherited
  opacity. All64 independent Qt alpha/opacity/DPR/background cells are exact.
- Canonical Qt rectangle mesh uses Qt's thin clamped-radius branch and ring
  ordering; a Qt-derived vertex/color/index fixture protects it. Native GL uses
  that same cached mesh and Qt vertex-AA material, without a geometry fork.
- MultiEffect GPU samplers explicitly use highp; native images, opacity,
  downsampling and final effect composition use Qt material semantics. A scoped
  `Node.native_materials` opt-in activates bar roots, including text-free corner
  surfaces, and resets at foreign Shared subtree boundaries. Unsupported paths
  retain the existing fallbacks. Runtime remains Qt-free.
- Thin-rect geometry and QRgb8 alpha fixes have regression tests. Engine all-target
  tests: **54 passed, 15 ignored** (final restored-shader run; other owners added
  five opt-in tests during this task); bar: **34 passed, 1 ignored**. Separate private
  GPU integration run: **2 passed**, including four established exact goldens and
  the64 new cells. Strict all-target Clippy passed for engine and bar.
- Final production-EGL cross-crate replay: **380/380 island +300/300 panels/calendar
  images pixel-identical** with native GL disabled/enabled on the same pinned
  binaries. Gate counts remain island0/380, panels82/300; these existing failures
  are not attributed to this change. Other owners' translators were copied,
  never edited.
- Pinned corpus stress: **3,040 frames /10 complete304-image replays**, compared
  against an independent earlier oracle, **zero mismatches**, with pinned binary
  and hashed private asset snapshot. Receipt:
  `artifacts/final-pinned-corpus-stress/run-0/stress.json`.
- Final synthetic GPU stress: **6,000 frames**, four mapped private surfaces,
  **776 comparison checkpoints**,26 oracle cases,1,974 resizes,63 recreations,
  2,662 partial-damage frames, **zero mismatches**. No synchronization workaround
  was introduced.

The historical apparent GPU corruptions were causally reproduced as a fixture
asset race: concurrent preparation removed shared icon directories, selecting
Fluent icons or fallback tray letters. Private absent/empty/partial snapshots
reproduce all four affected tray strips exactly. Preparation now uses report-local
asset directories; replay pins its binary and hashes a complete private asset
snapshot. See docs/STATUS.md and `artifacts/icon-race-reproduction/comparison.json`.
This explains those captures; it does not prove the absence of every GPU bug.

Additional geometry sweep (`tools/check_rect_mesh.py`):15/16 shapes have
bit-identical Qt/Rust vertex components and all16 have exact triangle ordering,
covering popup/pill sizes, fractional widths, radii10/14/19 and borders. The
remaining artificial1px-high bordered case differs by9.3e-9 in a component.
`artifacts/rect-mesh-sweep/{metrics,provenance}.json` preserves this independent
check. A vec4 vertex shader-input experiment changed no pixels and was reverted.

Residual investigation: source PNG decode and Qt's actual decoded image pixels
are exact, canonical rounded mesh vertices match installed Qt, and Qt and the
engine both use NVIDIA GLES3.2. Remaining sampling/interpolation/blend rounding
is unresolved. Desktop-GL, atlas-offset, UV-reflection and alternate layer-basis
experiments were not retained. The submenu's max23 is still counted as a failure;
no theme-specific hover workaround was added.

Receipts: `artifacts/remaining-final-bar/{comparison.json,gate-0/gate.json}`,
`artifacts/remaining-cross-final-v2/published-comparison.json`,
`artifacts/final-native-material-{stress,tests}/run.log`,
`artifacts/alpha-qrgb/metrics.json`, `.build/final-{engine,bar}-{tests,clippy}.log`.
Final engine receipts: `.build/restored-final-engine-{tests,clippy}.log`.
Shared engine changes and handoff are logged in `docs/requests/hyprv-ui.md`.
A ranked per-image/category handoff is
`artifacts/remaining-s1-crop-regression/residual-inventory.json`: the largest
residual is the menu raster icon277 channel units in each settled DPR2 frame;
rect edge peaks53 and compositing55. These are all max1; native text has no
remaining channel error outside the retained input mismatch.

## Queued S1: actual-output SVG source DPR

The full-screen report's S1 is independently reproduced using the frozen idle
fixture and frozen whole-output references. The bell is native text in original
`shell.qml:4327-4334`, not SVG. Current direct-GL bell is max3 in both themes.
Control-panel icon before correction: dark max12/16 pixels above8; light max9/3
pixels above8. Exact original SVG source files match bundled SHA256 values.

Cause confirmed by intervention: `sourceSize.width/height:32` on a real DPR2
output loads a64×64 QtSvg raster before mip filtering into the16-logical-pixel
slot. The previous bake held32×32 at all scales, faithfully matching the crop
host, but under-resolving the real-output source. Offline bake now uses32*DPR
for control icons at all eight supported scales. The real-output probe clears
the crop adapter's explicit DPR1 asset override and follows the runtime path.
After correction: **control dark max3 (error1571→125), light max2 (1052→48),
zero pixels above8**; bell remains max3. All four regions meet the ≤8 S1 gate. The restored final shader repeats the
same region metrics (`artifacts/fullscreen-s1-final-restored/metrics.json`),
and `linked-libraries.txt` contains no Qt libraries.

The canonical crop host's actual QQuickImagePrivate image was independently
captured as32×32 at both capture scales (`artifacts/qt-image-trace-final`): it
scales windows while effective devicePixelRatio remains1. Its test asset provider
now explicitly supplies that32-pixel image through existing `Model.assets`;
production defaults use the DPR-correct embedded bake. No reference pixels,
fixture states or gate criteria changed. Regression checks all eight bake sizes
and emitted tree images. Control assets may be supplied under
`control-panel-{dark,light}` keys through the existing asset map.

`tools/check_fullscreen_s1.py` is a standalone private-Weston probe. Receipts:
`artifacts/fullscreen-s1-{current,64,fixed}/metrics.json`. This proves the isolated
idle bar regions, **not37 new full-screen passes**: T1/full-shell replay remains
with the other owners. Final bar all-target tests: **35 passed, 1 ignored**; strict all-target Clippy
passed. All **304 crop PNG arrays are identical** to the239/304 checkpoint;
receipt `artifacts/remaining-s1-crop-regression/png-comparison.json` and full
gate `gate-0/gate.json`. Logs `.build/s1-bar-{tests,clippy}.log`. A static
idle control-region patch also meets ≤8 against **64/64** frozen whole-output
references (`static-control-patch-sweep.json`), without claiming a full-image
or integrated runtime replay.

## Historical checkpoints


Earlier native-GL integration checkpoint:

- `hyprv-ui::scene::NativeTextMode::{Skia,QtGl}` is now available per text leaf.
  Default Skia preserves other UIs; bar view/wrapped text opts in and retained
  synchronization copies the field. Opaque A8 text on the registered EGL target
  uses the cached GL mask shader; translucent/fading paint, complex clips,
  affine transforms and save layers retain Skia. Runtime stays Qt-free.
- Bar integer-premultiplied gate **228→230/304**, exact **36→38**, v1 **303/304**,
  structural304/304, painted MAE **.0284993688→.0278794348**. No lost passes,
  increased maxima or increased per-image means. Remaining:73 max1 native crops
  and the original frozen submenu input-state mismatch at max23.
- Other UI GPU proof uses pinned copies of original test translators with only
  capture transport changed to production EGL. **380 island +300 panel/calendar
  PNGs byte-identical** before/after. Original owner source was not edited.
  Existing strict-gate failures remain (island0/380, panels82/300 on GPU).
  CPU verifier also completed all three (island0/380, panels80/300, bar224/304).
- Corruption stress: **69,040 frames**,4,760 exact image comparisons,25,982
  resizes,677 recreations,9,304 partial updates, four surfaces/shared context,
  static and changing damage, mapped private-XDG compositor backpressure,
  texture churn and mixed renderers: **zero pixel mismatches** on NVIDIA RTX PRO
  6000 Blackwell. No speculative fence/glFinish/age workaround was added.
  The earlier two corruptions are not reproduced, not claimed causally fixed.
- Stress provenance qualification: the old3040-frame corpus child matched its
  independent oracle for all10 passes, but its launcher later reported that the
  executable pathname had been replaced by another build during replay or
  postprocessing. That warning is retained. Later stress runners pin executable
  copies; all detailed receipts remain inspectable.
- Independent Qt GPU goldens4/4 exact; private GPU integration test also checks
  fractional scale, clips, opacity, layer fallback and backend selection. Bar33
  established behavior tests and strict all-target Clippy pass. The established
  engine suite and native GL checks pass; a parallel `zz_mip` diagnostic currently
  blocks full engine all-target tests (missing MIP_DIR) and Clippy (two range-loop
  lints). Its source was left to its owner.

Evidence: `artifacts/native-stress-summary.json`,
`artifacts/native-cross-final/{png-comparison.json,before,after}`,
`artifacts/native-bar-opaque/{comparison.json,bar-gl-adapter/gate/gate.json}`,
`artifacts/native-final-source.json`, `.build/native-scoped-engine-*`,
`.build/native-opaque-bar-clippy.log`, and
`artifacts/native-gl-tests-opaque/run.log`. Shared changes are logged in
`docs/requests/hyprv-ui.md`.

Comparator qualification: the canonical float reconstruction from straight PNG
reports228 bar passes; integer RGBA8 premultiplication reports230. The two new
corner captures are integer-premultiplied exact. Both reports are retained;
no shared comparator/reference/fixture/threshold was altered.

Previous checkpoints (retained for audit):

Latest checkpoint: svc-media's consolidated QtQuick GPU rectangle mode is now
used automatically by bar `Content::Rect` leaves. Three complete GPU replays (including the final binary with provenance) are
**byte-identical304/304**, with **228/304 current gate passes**, **36 exact**,
**303/304 interim v1**,304 structural; painted MAE **0.0284993688/255**.
The remaining failures are **75 native-only max1 crops**, plus the frozen
submenu dark@2 input-state mismatch at max23. Gate is not complete.

| Frozen304, production GL | Before GPU meshes | After GPU meshes |
|---|---:|---:|
| Current gate |224|228|
| Exact |32|36|
| Interim v1 |303|303|
| Native-only max1 failures |79|75|
| Painted MAE /255 |0.0295340168|0.0284993688|

No lost gate passes or increased per-image maxima. Four zero-height menu reveal
frames become exact. One corner's mean rises .00278846→.00330128 (max1 remains);
all other means are unchanged or lower. Native residual spatial error ranking:
text79,011; rectangle edges10,204 (was34,388); icons6,415; compositing3,997
(was16,936). These are spatial categories, not causal attribution. Full data:
`artifacts/gpu-repeated/{gate-0,gate-1,comparison.json,repeat-comparison.json}`.

Submenu root cause: **the canonical unmodified Qt host** replayed the same frozen
fixture twice, with identical original-QML hashes. Dark@1/light@1/light@2 match
the frozen PNGs exactly; dark@2 differs max23, paintedMAE3.58903853. Adding only
an explicit hover move over DND in a private fixture copy reproduces frozen
dark@2 **byte-for-byte**, SHA256
`6ccc45699b3e9b412216b67e34a28d65d4bce1e7c3c1c78dee44d6146806da5d`.
This establishes hidden input-state drift. The frozen PNG remains a failure;
references, masks and thresholds were not changed. Evidence and reproduction:
`tools/check_submenu.py`, `artifacts/submenu-{input-check,explicit-hover}`.
Controller now clears hover when changing pages/back (destroyed Repeater delegate)
and the fixture adapter replays actual Move/Press/Release. Regression passes.

Native text isolation: same canonical FreeType mask drawn directly via a GL
shader (`color * texture(mask).a`, ONE/ONE_MINUS_SRC_ALPHA) matches Qt exactly
**4/4 cases**, transparent/bar background at DPR1/2. The ordinary Skia A8 path
is max1 in all four. Actual Qt scene-graph material, GL color uniform, nearest
RED_OR_ALPHA8 sampler and installed shader resource were traced; highp and
mediump direct-GL variants both match, so precision qualifier alone is not the
root cause. The Skia RGBA-mask variant reduces differences9→1 channels@1 and
13→3@2, but still max1. No production text/EGL changes were made. Direct GL is a
finite diagnostic, not a complete clipping/effects/opacity/affine implementation.
See `artifacts/glyph-raw-{highp,mediump}/metrics.json`,
`artifacts/glyph-rgba-{simple,float,shader}/metrics.json`, and archived prototype
`artifacts/native-gl-prototype/source`. Shared text implementation ownership was
requested because the architecture restricts edits to assigned paths.

GPU stability qualification: the first canonical full run produced two severe,
non-reproduced textured-pixel corruptions (max245/212). That failed run remains
at `artifacts/submenu-gpu/bar-gl-*`. Both affected states passed twice in isolated
52-image runs, and all three subsequent full304 replays are identical with no such
corruption. This remains an open shared-resource/state investigation; clean
repeats do not erase that observation. Findings were handed to engine/svc-media
in `docs/requests/hyprv-ui.md`.

Checks: bar32 tests pass,1 opt-in parity ignored; shared engine43 pass,1 live
Wayland ignored; strict bar all-target Clippy passes. Canonical all-three verifier
completed: CPU island0/380, panels80/300, bar224/304 with **no lost current/v1
passes or increased maxima** against the layer checkpoint. Some sub-LSB mean
variation remains. Island/panels GPU hooks are reported unwired by the canonical
verifier, so their GL proof is unavailable. Raw evidence:
`.build/submenu-gpu-{tests,verify}.log`,
`.build/submenu-final-{engine-tests,clippy}.log`,
`artifacts/submenu-gpu/{report.md,cross-crate-metric-comparison.json}`.

Previous layer-sizing checkpoint (retained for comparison):

| Same304 frozen bar captures | Before CPU | After CPU | After production GL |
|---|---:|---:|---:|
| Current gate |224|224|224|
| Interim v1 tolerance |240|303|303|
| Structural checks |304|304|304|
| Exact |32|32|32|
| Painted MAE /255 |0.097981|0.049968|0.029534|

All192 DF-flagged captures pass. For the64 changed popup PNGs, no individual
maximum/painted mean increases and no gate pass is lost. The other240 bar PNGs
are byte-identical. Rectangle-edge absolute error drops4,895,247→336,050 on CPU
and169,492 on GL. Current GL spatial rank is text→compositing→SVG/images→rect
edges; the single missing submenu hover dominates the larger residuals.

Cross-crate no-regression proof: **380/380 island and300/300 panel/calendar PNGs
are byte-identical** before/after. Existing strict failures remain: canonical
verifier counts island0/380, panels80/300, bar224/312 (bar includes8 foreign
missing crops and12 live-unreachable battery exclusions). These are not claimed
as complete acceptance passes. All52 calendar frames are present; the private
wrapper aliases the translator's actual output variable without changing replay
or comparison rules. DefaultNone effect sizing follows the existing path; this
baseline is before bar's explicit layer-size opt-in.

Evidence under `artifacts/`: `shared-layer-{before,after}/out/results.json`,
`shared-layer-gl/out/results.json`, `layer-png-comparison.json`,
`layer-bar-metric-comparison.json`. The bar's integer premultiplied gate details
are in each run's `bar-adapter/gate` (GL: `bar-gl-adapter/gate`). The canonical
verifier also reports its own continuous premultiplied metrics. Sources and
results are independent of the verifier's concurrently modified shared output.

Implementation: optional `MultiEffect.source_texture_size` specifies physical
layer pixels. The engine renders the padded logical source into those explicit
dimensions and separately sizes the output to padded logical extent×DPR. Zero,
oversized and excessive-area textures fail validation before allocation. `None`
retains the previous capture transform. Both new public API documentation and
every shared change are logged in `docs/requests/hyprv-ui.md`.

Validation: **41 hyprv-ui tests and31 bar tests pass**, one live/opt-in test ignored
in each suite; new engine tests include independent Qt RHI goldens atDPR1/2,
invalid dimensions and effect-cache source-size edits. Engine library and bar
all-target strict Clippy pass. Logs: `.build/engine-layer-after-tests.log`,
`.build/bar-layer-after-tests.log`, `.build/{engine,bar}-layer-clippy.log`.
Reproduce private full verification:

```
python3 tools/verify_all.py RUN_NAME --backend cpu
python3 tools/verify_all.py RUN_NAME_GL --component bar --backend gl
```

GL capture reuses parity QA's helper through a dev-only module; the private
Weston never maps a role-bearing surface on the user's desktop. Runtime remains
Qt-free. The bar inherits canonical QtQuick mode, ready for svc-media's GPU
cache; current `render.rs` still uses the CPU Raster cache. GPU-mode migration
and performance proof are pending its landing.

Further native-glyph investigation (diagnostic only): `tools/glyph_probe.qml`
and `examples/glyph_probe.rs` isolate colored NativeRendering glyphs. Alpha masks
match exactly; a few colored RGB values differ1 even over transparent pixels.
Qt glyph format isA8 (not LCD). Raw premultiplied RHI/Skia readbacks reproduce
those differences, with no PNG round-trip loss. Disabling Qt dithering does not
remove them; an experimental float color filter helps some pixels and worsens
others, so it is not enabled in production. Artifacts: `glyph-probe-gl`,
`raw-glyph-gl`, `qt-premul-trace`, `glyph-filter-gl`, `glyph-no-dither`. No shared
text or image code was changed. Original QML/fixtures/reference PNGs remain intact.

The sections below retain earlier measured checkpoints.

The current parity gate is **not passed**. Frozen reference manifest: 312 images,
2026-10-23T03:41:36-04:00. The bar replays 304; eight calendar/island-expanded
content crops belong to other UI crates. The foreign island host rectangle is
excluded in both images. Measurements use premultiplied RGBA, with the exact
per-image `distance_field_text` rule in `docs/ARCHITECTURE.md`.

| Current shared-engine QtQuick replay | Result |
|---|---:|
| Current gate | 224/304 |
| Distance-field flagged captures | 192/192 pass |
| Structural mask/footprint checks | 304/304 pass |
| Interim v1 tolerance (not current acceptance) | 240/304 |
| Exact captures | 32/304 |
| Maximum channel error | 23/255 |
| Overall painted-pixel mean absolute channel error | 0.097994/255 |

Full per-image output: `artifacts/gate-final-report/gate.{json,md}`. The stable
renderer and relevant source hashes are recorded in `artifacts/gate-final-source.json`.
A fresh replay is retained separately in `artifacts/gate-final-refresh*` so that
concurrent integration work cannot overwrite the recorded run.

## Controlled rectangle before/after

Both runs use the **same compiled binary**, same 304 frozen reference images,
assets and fixture replay. Only `HYPRV_BAR_RECT_MODE=skia` differs from default
QtQuick; this isolates the rectangle rendering mode.

| Metric | Skia before | QtQuick after |
|---|---:|---:|
| Current gate | 32/304 | 224/304 |
| Interim v1 | 32/304 | 240/304 |
| Structural checks | 298/304 | 304/304 |
| Max channel error | 72 | 23 |
| Overall painted MAE | 0.367464 | 0.097994 |
| Rectangle edge absolute error | 17,071,438 | 4,896,773 |
| Compositing absolute error | 15,751,267 | 2,631,982 |
| Text-region absolute error | 9,506,536 | 3,326,570 |
| SVG/image-region absolute error | 599,307 | 589,771 |

Reports: `artifacts/gate-skia-ablation-report` and
`artifacts/gate-final-refresh-report`. Overall painted MAE drops 73.33%; the
rectangle edge-region error drops 71.32%. The other regions also improve because
rectangle backgrounds/effects contribute beneath text and images.

## Residual contributors

These disjoint semantic masks rank **where** differences occur; they are not
causal subsystem attribution. Glass effects overlap rectangle and text regions.

| Rank | Region | Absolute channel-error sum | Region painted MAE | Max | Pixels above 8 |
|---|---|---:|---:|---:|---:|
| 1 | Rectangle edge AA | 4,896,773 | 0.833910 | 23 | 99,483 |
| 2 | Text | 3,326,570 | 0.077699 | 23 | 18,774 |
| 3 | Compositing | 2,631,982 | 0.041623 | 23 | 45,287 |
| 4 | SVG/images | 589,771 | 0.120975 | 3 | 0 |

The 80 failures are 48 tray-menu animation frames, 12 menu root/hover/submenu,
12 battery popup and eight top-corner crops. Native-only popup glass edges have
residual errors around 16. The dark@2 submenu capture additionally contains a
pointer-hover background absent from the other three theme/scale submenu crops;
its painted MAE is 3.744828. These captures are retained as failures.
Top-corner crops include neighboring bar text/icons/glass: their maximum 1–2
error does not establish a defect in the baked corner itself. All eight bottom
corner crops are exact. Shared native text, image sampling and MultiEffect
precision need further engine work; the evidence and requests are recorded in
`docs/requests/hyprv-ui.md`. No acceptance threshold was relaxed.

The historical 220-image replay, evaluated against the same current frozen
reference and current semantic masks, improves from 0 to 212 current-gate passes,
0 to 220 v1 passes, max255 to3 and painted MAE0.730682 to0.022246. This is a
**historical replay comparison**, not a controlled renderer experiment: fixture
replay fixes and reference asset-input evolution contribute to the difference.
Its reports are `artifacts/gate-common-{before,after}-report`. All current
220-image tolerance failures have been closed; eight native-only crops still
fail the tightened exact requirement.

## Implemented precision work

The new `hyprv-ui/src/qt_rect.rs` ports Qt Quick rectangle vertices, border
geometry, vertex AA shader offsets and triangle rasterization. Engine has wired
it into the canonical renderer; the bar no longer substitutes rectangles or
uses a private image renderer. Seven independent Qt private-node oracle cases
match vertices and triangle sets exactly. Golden regression tests cover mesh
geometry, topology and the circle scan-bound crack. Unsupported geometry uses
the engine's explicit fallback. This does not imply exact MultiEffect pixels.

Fixed shell icons use 288 embedded PNG cache entries baked offline with QtSvg
at scales 1/1.25/1.5/1.75/2/2.5/3/4. Cache manifest keys and source/tool hashes
are in `assets/baked/`; Cargo and the runtime neither invoke nor link Qt.
The four corner orientations apply the QPainter Canvas transform before rasterizing.

Dynamic SVG decoding remains Rust-only. Against eight independent QtSvg cases:

| Decoder | Within max8 / painted MAE0.5 | Max channel | Worst painted MAE | Exact |
|---|---:|---:|---:|---:|
| resvg baseline | 0/8 | 35 | 7.918863 | 0/8 |
| Qt-style flattening + FreeType gray outlines | 8/8 | 2 | 0.470320 | 1/8 |

`artifacts/svg-decoder/metrics.json` records input and binary hashes. The probe
was freshly built, not supplied as a prebuilt binary. Its `runtime-libraries.txt`
contains no Qt libraries. Solid paths use the improved decoder; complex SVG
features fall back to resvg. Arbitrary dynamic SVG parity and native-only
bit-exactness remain unproven.

Independent headless Qt texture diagnostics explain fixture asset preparation:
control sourceSize32 resolves to a 32×32 texture even at effective DPR2; WeChat
baseline resolves to72×72 and network-offline to32×32. Logs/provenance are in
`artifacts/qt-source-trace`. Diagnostics instrumented an owned copy of the host;
original QML, fixture states and shared reference images were unchanged. This
asset-load timing difference is supplied as fixture input, not a production
state-dependent icon choice. Menu raster sourceSize16 remains16 physical pixels.

## Review and automated checks

BAR-1 now waits for subscribed menu changes after one staging turn, with no
repeated idle deadline. Vanished items cancel work. BAR-2 hit regions intersect
the current reveal clip and reject unmapped rows. BAR-3 consumes releases and
clears hover/grabs at every overflow close and fresh mapping. Four regression
tests include valid-click controls and the legacy battery-availability quirk.
BAR-4 landed from svc-net through shared `hyprv_widgets::flickable`; three tests
verify ordinary clicks, wheel easing, drag/flick, click cancellation and eventual idle. Reconcile
also compares effect fields without allocating Debug strings (BAR-PERF-1).

| Check | Result |
|---|---|
| Fresh Cargo all-target tests, canonical real dependencies | 29 passed, 1 opt-in parity test ignored |
| Behavior / flickable / offscreen / rectangle / review / SVG | 16 / 3 / 1 / 3 / 4 / 2 passed |
| Offscreen coverage | 456 unique PNGs; both themes and scales1/1.5/2 |
| Fresh strict Clippy all targets, no-deps, warnings denied | PASS |
| Owned formatting and Python syntax | PASS |
| Opt-in QML acceptance | FAIL as quantified above; adapter exits nonzero |

`python3 scripts/check.py test --all-targets` uses an isolated manifest with
identical source files and canonical service/widget/engine dependencies. It
avoids unrelated workspace members under active parallel development; no stubs.
Logs: `.build/tests-final.log`, `.build/clippy-final.log`. Reproduce comparisons
with `python3 scripts/parity.py` after generating matching QML references. The
adapter snapshots inputs and reports current gate, interim v1 and exact counts.
Offline SVG probes: `python3 tools/check_svg.py`.

No live shell restart, install, physical input, full composed desktop, device or
output-hotplug proof is claimed. No source QML/reference edits, sudo or commits
were made. Public integration APIs remain documented in `README.md`.


## Follow-up checkpoint — review and consolidated mode handoff

BAR-1..3 fixes remain in production. Six review regressions now pass, including
late hydration after cancellation, release during closing before unmap, and
empty overflow followed by remap. svc-net's three Flickable tests pass unchanged.
The complete all-target suite now has31 passing tests (plus the ignored parity adapter).
Latest logs are `.build/tests-checkpoint.log` and `.build/clippy-checkpoint.log`.

The rectangle oracle, shader/coverage pitfalls, cache-phase requirements and
frozen304-case baseline were handed to svc-media in `docs/requests/hyprv-ui.md`.
As inspected at this checkpoint, the canonical renderer still has the CPU
`rect_cache: HashMap<..., qt_rect::Raster>`; the consolidated GPU backend has
**not landed**. The bar already uses the shared QtQuick mode and no private
rectangle renderer. A GPU migration parity result is therefore **PENDING**.

An independent Qt RHI experiment isolates the largest edge defect. Reproduce:

```
python3 scripts/check.py build --example rect_effect
python3 tools/check_rect_effect.py --host /path/to/parity-host --probe ../../target/debug/examples/rect_effect
```

| Dark300×230 card probe | Max @1 | Painted MAE @1 | Max @2 | Painted MAE @2 |
|---|---:|---:|---:|---:|
| Plain rect | 1 | 0.000160 | 1 | 0.000085 |
| Same rect + MultiEffect | 13 | 0.224097 | 16 | 0.157923 |

`artifacts/rect-effect/metrics.json` records the binary/host hashes. Diagnostic
QML is owned tooling, not a replacement reference or production QML change.
A read-only host trace shows `layer.textureSize` remains300×230 (@1) or600×460
(@2), even after MultiEffect expands sourceRect to(-32,-32,364,294). The output
geometry remains364×294. Our engine allocates364×294×DPR instead, losing the
source downsample/resample. A shared explicit source-texture-size API and its
output-size decoupling are requested from engine; bar will bind the original
QML unpadded layer size when available. This isolates a causal candidate, not a
claimed gate improvement: production gate remains224/304 with80 failures.

Fresh independent original-QML submenu captures in all four theme/scale
combinations have no MouseArea containsMouse=true after the fixture click.
The frozen dark@2 capture's hover rectangle remains a capture-state discrepancy;
it was not patched into the renderer or hidden from the gate. Diagnostic logs
and host-copy hashes are retained in `artifacts/qt-submenu-trace`.


Latest canonical production replay: **224/304**, structural304/304, exact32,
v1240, max23, paintedMAE**0.0979810286**. This uses the current QColor16
rectangle quantization in the shared engine. Compared per-image with the previous
224-case checkpoint: **zero lost gate passes, zero increased maxima, zero
increased painted means**; five captures improve slightly. Artifacts:
`gate-checkpoint-report`, `gate-checkpoint-comparison.json`,
`gate-checkpoint-source.json`. The adapter hash in that source file predates
its added opt-in F16 diagnostic branch; production rendering is unchanged.

A controlled source-sizing intervention confirms the MultiEffect cause:

| Probe | Current max | Prototype max | Current MAE | Prototype MAE |
|---|---:|---:|---:|---:|
| DPR1 | 13 | 1 | 0.224097 | 0.005410 |
| DPR2 | 16 | 1 | 0.157923 | 0.007271 |

`tools/source_extent_probe.rs` is a diagnostic standalone binary. The optional
`--source-extent-prototype` flag on `tools/check_rect_effect.py` copies canonical
engine effect code into an isolated build, changes only source/output extent
handling, and compares against the same Qt images. No production engine file is
edited. Both cases have zero pixels above8. The shared explicit source-texture
API is still pending; these prototype numbers are **not production gate passes**.

The opt-in F16 corner probe did not close any exact failures and worsens light
crop painted means; it remains diagnostic only, never default. Canonical replay
still uses `Renderer::png`. No GPU migration result is claimed before svc-media's
mode becomes available.


## Legacy selection and surface topology checkpoint

- Battery history now selects the first sorted BAT* directory exactly as
  `scripts/lib/battery.sh:22-29`, following directory symlinks and ignoring
  type/scope/present. A peripheral alone produces unavailable. The live read-only
  shell check found no BAT directory and one HID type=Battery device; no service
  mutation or full equivalence replay was performed. Historical mismatch retained
  with a resolution note in docs/verify/equivalence.md.
- Corner surfaces: exactly one overlay/output, legacy namespace, all four edges,
  compositor full-output size (0,0), zone -1, keyboard None and empty input region.
  Four 27px children track BarScreenState.qml:22-23, including clamped top offsets.
  Production assembly consumes the single returned requirement unchanged. Tests
  cover topology, child placement, output geometry/scale and reveal/fullscreen.
- `cargo test -p hyprv-svc-power --all-targets`: 18 passed. Bar all targets:
  34 passed, 1 ignored. Strict all-target Clippy for both crates passed. Logs:
  `.build/battery-selection-{tests,clippy}.log` and
  `.build/corner-topology-{tests,clippy,build,gpu}.log`.
- Private GPU replay: 304/304 PNG pixel arrays identical to prior opaque native
  checkpoint. Gate unchanged: 230/304, 38 exact, interim v1 303/304, structural
  304/304; painted MAE 0.02787943475047115. Receipt
  `artifacts/corner-topology/comparison.json`; full gate `gate-0/gate.json`.
- Earlier GPU corruption remains OPEN in docs/STATUS.md, with its failed captures,
  negative stress evidence and the qualified corpus launcher exit explicitly
  distinguished. No speculative runtime synchronization change was added.

## Disk-policy cleanup

After the final closure receipts were recorded, obsolete bar-owned scratch builds,
frozen binaries, copied translators and duplicate PNG corpora were removed. Raw
RGBA captures were hashed before removal (`artifacts/raw-capture-hashes.json`).
Historical receipts remain; their unneeded raw/image paths may no longer exist.
Final exact corpus PNGs, final other-UI published outputs, diagnosed outlier PNGs,
independent regression outputs and checked-in engine goldens are retained. Cleanup
summary: `artifacts/disk-cleanup.json`. Bar Cargo helpers now force the shared
workspace target and `CARGO_INCREMENTAL=0`, and abort build/test jobs below30 GiB
free. No rendering source or acceptance result changed during cleanup.
