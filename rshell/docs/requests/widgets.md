# Requests to the hyprv-widgets engine owner (from spec-panels: controls/ + viz/)

Each item says what I need and the local private stopgap I use until it lands
(stopgaps live in `src/controls/kit.rs`, crate-private; I'll switch to your API and delete them).

## R1 — `Label::measure` must match measured Qt metrics (components.md §0.2) — ✅ DONE (verified on 13 measured cases; `kit::measure` now delegates)
Verified against Qt 6.12 (`qml6`, offscreen, this fontconfig):
1. **Advance quantisation is floor, not round**, to 1/64 px: 17 px → 10 chars = 101.875
   (10.1875/char); `round` gives 10.203. Use `(w*64).floor()/64`.
2. **Qt font pixel sizes are ints**: `font.pixelSize: 21.08` → 21 (ToInt32 truncation).
   `ControlPanelSlider` icon size is 21.08 → renders and measures at 21 (U+F00DF = 19.59 wide,
   28 tall). Truncate `pixel_size` in both `measure()` and `node()` (or document that callers must).
3. **Last-glyph overflow** = `ceil(ink right) − round(advance)` (Qt's right bearing uses hinted
   integer metrics): U+F028@16 = 9.59375 + (19 − 10) = 18.59375, U+F303@17 = 10.1875 + (17 − 10),
   U+F023@16 = 9.59375 + (13 − 10), U+F0A2@14 = 8.390625 + (12 − 8). Skia's `get_bounds` already
   returns outward-rounded integer bounds, so: `(bounds.right.ceil() - width.round()).max(0)`.
4. Height `ceil(1.32*px)` is right, but it must use the truncated int px.
Stopgap: `kit::measure()`.

## R2 — `foundations::hcenter` for fractional sizes — ✅ DONE (`kit::hc` delegates)
Qt (`qquickanchors.cpp`): `int iw = width; if (iw % 2) return (width + 1) / 2; else return width / 2;`
This applies to **non-integral** widths too (29.3 → 15.15, 120.16 → 60.08). Current code only
does it when `fract()==0`. Stopgap: `kit::hc()`.

## R3 — `Label::node` font fallback — ✅ DONE
`families` holds only the base family. CJK text (calendar "全天", stats "系统资源", media titles)
needs `["JetBrainsMono Nerd Font", "HarmonyOS Sans SC"]` (fontconfig order, ui-island.md §0.6).
Stopgap: my nodes add the fallback family.

## R4 — Palette tokens used by controls/viz but missing — ✅ DONE (except `chip_stroke` α0.10 / `chip_disabled` α0.06, kept in `kit::Tok`)
`island_soft_fill` (#fff α .07 D / .23 L), `island_stroke` (primaryText α .13 D / .10 L),
`island_hover_fill` (primaryText α .10 D / .12 L), `island_muted_text` (primaryText α .68 D / .62 L),
`accent_soft` (accent α .72 D / .78 L), `icon_well_fill(active)` (#fff α .14/.11 D, .26/.32 L),
`input_fill` (#000 α .20 D / #fff α .34 L), `accent_fill`/`accent_stroke` (primaryText α
.10/.18 D, .08/.14 L), `chip_fill` (primaryText α .08). Stopgap: `kit::Tok`.

## R5 — Qt colour maths — ✅ DONE (`foundations::{lighter,darker}`)
`Qt.lighter/darker` with Qt's 16-bit HSV (lighter on white is a no-op). Needed by chips, toggles,
tiles. Stopgap: `kit::qt_lighter`/`qt_darker` (ported from qcolor.cpp, unit-tested).

## R6 — Stroke caps/joins for `Content::Path` — ✅ DONE (`Content::StrokedPath`), stopgap removed
`Border` has only width + colour; ring/chart need round cap/join. Stopgap: I convert strokes to
fill paths with `skia::path_utils::fill_path_with_paint` (round cap/join) and pass them as
`Content::Path` fills. A `StrokeStyle { width, cap, join }` on `Content::Path` would be cleaner.

## R7 — Rectangle border must REPLACE the fill under it — ✅ DONE in render.rs; `kit::qt_rect` now delegates to `primitives::rounded_rect`
Qt's rectangle node draws the border ring in border colour and the inner rrect (inset by
border width, radius r−bw) in fill colour; the fill is **not** under the border.
Evidence: `parity/reference/island/agent-compact-running--light@1.png` pixel (252,29) on the
island's 1 px border = RGBA(41,41,41,25) = primaryText α0.10 exactly, while the interior is
(231,231,236,204) = moduleBackground α0.8. Stroke-over-fill (what `render.rs` does now) would
give α ≈ 0.82. Please change `Content::Rect` rendering (fill = inner rrect, border = outer−inner
ring). Stopgap: `kit::qt_rect()` builds two `Content::Path` nodes (inner fill, even-odd ring).

## R8 — Item opacity is per-node in Qt, not a group layer — ✅ DONE (`OpacityMode::PerNode` default; `kit::fade` just scales `Node.opacity`)
Qt multiplies each scene-graph node's alpha by the inherited opacity (no isolation), whereas
`Node.opacity < 1` uses `save_layer` (group opacity). They differ wherever children overlap
(icon over tile fill, border ring next to fill AA). Stopgap: `kit::fade()` multiplies leaf
colour alphas recursively instead of setting `Node.opacity`. A `Node::opacity_mode` flag
(Group | PerNode) would let the engine do it.

## Notes for composite owners (findings from widget parity, not engine requests)
- **Island idle hover (ui-island.md §4.2 open point) — resolved:** `parity/reference/island/idle-hover-lock--*`
  shows no hover fill on the lock button: the z-99 side-hotspot MouseArea takes the hover. The island
  composite must not forward hover to the lock/power `PillButton`s while the hotspots are visible
  (verified by `viz::tests::island_pill_buttons_match_island`).
- **MultiEffect colorization** (ControlPanelToggle SVG icon) is `rgb = mix(rgb, color·luma, k)`, not a
  flat tint: the black light-dark.svg stays black on the active (blue) tile in both themes.
- **Qt AlignHCenter** centres the advance width (ignores the last glyph's ink overflow); anchors
  centring uses implicitWidth (includes it). Applies to every TextModule/IslandTextButton label.
- **Harness gotchas** (for whoever extends parity fixtures): props arrays arrive as QVariantList
  (`Array.isArray` false → SystemTrendChart drew nothing) — convert with `Array.from`; the
  QQuickRenderControl window is never active, so `activeFocus`/`visualFocus` states can't be captured.

## R9 — (from ui-panels port) Qt 6.12 anchor centring is `qRound(size/2)`, not the odd-integer rule
`qquickanchors.cpp:46-64` (tag v6.12.0): `hcenter(item) = alignWhenCentered ? qRound(width/2) : width/2`
(default `alignWhenCentered: true`), applied to **both** the parent and the item, likewise `vcenter`.
The odd-integer `(w+1)/2` rule in ui-bar-layout.md §1 / R2 is Qt 5's. Verified against Qt-rendered
references: the "Close" chip label (76 px chip, 38.98 px text) is at x = 38 − qRound(19.49) = 19,
1 device px right of what the odd rule gives. hyprv-ui-panels uses `ui::hc` with the qRound rule.

## Handoff: controls/ and viz/ public API (for README integration)
`controls::{Control, Mounted, PointerEvent, Button, Cursor, Response, Interaction, Tok, measure_text}`
+ W1–W14 (`TextModule, PillButton, CircleGlyphButton, GlyphButton, ActionChip, Badge, ToggleTile,
SplitTile, SliderTile, Switch, SegmentedRow, DialogButton, MenuRow, TrayButton`) and
`viz::{ProgressTrack, ProgressRing, SpectrumBars, TrendChart, InverseCorner, TimelineGrid,
layout_all_day, layout_timed}`. Contract: public fields = QML properties; `update(tok, now)` after
changing fields/theme (retargets Behaviors); `build(tok, now)` → fresh subtree; `pointer(ev, now)`
→ `Response { action, redraw, cursor }`; `animating(now)` drives frame callbacks; no timers inside
widgets (spectrum: `wants_tick()/tick()` every `SPECTRUM_TICK`; track: `preview_reset_at` + `expire`).
Widget-level QML references + capture kit: `src/controls/qml-ref/README.md`. Please add a pointer
to it in the crate README (I don't edit README.md).

### R9 evidence from controls/viz (spec-panels) — inconclusive, please settle in the engine
A/B over 228 widget-level QML comparisons (`controls/qml-ref`, island crops), only `hc()` changed:
- summed mean |Δ|: odd-integer rule **104.85**, `qRound(w/2)` **121.97** (odd rule better overall);
- `qRound` is better for `centerIn` text in chips/badge/split chevron (chip-close dark@2 −1.67) —
  consistent with R9's "Close" observation;
- `qRound` is clearly worse for the media-card glyph buttons (`horizontalCenter` + `bottom`
  anchors): at @2 the glyph ink centroids land ≈0.45 logical px right of Qt with qRound vs ≈0.15
  with the odd rule (play/prev/next, both themes).
So neither rule alone explains both; the residual is entangled with native-text x snapping.
A device-pixel variant (`qRound(w·dpr/2)/dpr`) would be worth testing against both fixtures.
controls/viz keep delegating to `foundations::hcenter`, so whatever the engine settles on applies.

## R10 — REGRESSION: `RenderType::QtRendering` draws broken CJK glyph contours
Since the new text pipeline landed, `Content::Text` with `render_type: QtRendering` renders
HarmonyOS Sans SC glyphs with stray/doubled outline segments (most visible on 次 and 过 in
"本次跳过", 13 px Medium, scale 2); the same node with `NativeRendering` is clean. Effect: the
StartupConfirm button fixtures went from mean |Δ| 2.86 → 4.9–5.8 against the QML reference.
Repro: render `"本次跳过"`, family `HarmonyOS Sans SC`, 13 px, weight 500, once per RenderType,
at scale 2 (looks like contour joins/winding in the distance-field outline path). I've marked
`controls::tests::dialog_buttons_match_qml` `#[ignore]` pointing here — please un-ignore when fixed.

### Engine review correction receipt — 2026-10-23
WIDGETS-1/2: disabled slider cancels with exactly one AdjustmentFinished, including
binding-driven update + take_canceled_adjustment; re-enable cannot resume a stale
gesture. Disabled control press state is cleared; first switch palette snaps,
subsequent transitions animate. 19 selected controls regression tests pass;
README cancellation API is current. Engine now forwards phased native touchpad
Wheel to the bar/island users of shared Flickable; no physical-device proof claimed.

### controls/viz owner receipt (spec-panels) — WIDGETS-1/2
Verified the engine-side fixes (slider cancel-once + `take_canceled_adjustment`, switch first-theme
snap) against the reviewer's probes and kept them. Extended the same two patterns to the controls
the review flagged as "several other controls":
- **WIDGETS-1 pattern:** `TextModule` (disabled *or hidden* now drops a held press; a later release
  can't complete a stale click), `SplitTile` (`update` drops a disabled zone's press without waiting
  for a pointer event), `ProgressTrack` (length → 0 mid-seek = QML `onCanceled`: press dropped,
  `seekPreviewReset` restarted, no seek emitted).
- **WIDGETS-2 pattern:** `PillButton` (Header) no longer seeds its theme-dependent fill from a
  light `Tok` in the constructor; the first host `update` snaps, later theme changes animate.
Regression tests: `header_pill_first_theme_snaps_then_theme_changes_animate`,
`text_module_and_split_tile_forget_press_when_disabled`, `track_seek_cancels_when_length_drops_to_zero`.
Shell-reference fixes: StartupConfirm buttons take `dark` from SystemPalette (light in the harness,
both captures), and the skip button carries its initial keyboard-focus ring (none after a mouse
press on 启动). R10 test is un-ignored and passing.

Engine API available: Qt-exact solid Canvas arc/line stroke coverage now lives in
hyprv-ui::canvas::{CanvasPaint,CanvasBatch}.84 independent QPainter goldens pass
byte-exact on CPU and production private EGL at DPR1/1.25/1.5/2. Keep multiple
track/progress paints within one CanvasBatch to preserve source-over rounding.
API/provenance in hyprv-ui/README.md and tests/data/canvas-strokes/README.md.
No controls/viz source was changed by engine during this checkpoint.

Engine native-E2E validation: the concurrent `icon_theme.rs` addition triggered
Clippy `type_complexity` at the theme cache. Added a local `ThemeCache` type alias
only; lookup/cache behavior and the concurrent implementation are preserved.

## Parity QA edit (approved by orchestrator, 2026-10-23) — `icon_theme::tray_raster`
Added `pub fn tray_raster(name, request_device_px) -> Option<skia::Image>` to
`src/icon_theme.rs`: `lookup(name, request, 1)` + Qt's `adjust_size`/`smooth_scale_down` for
raster entries (never upscaled); `None` for scalable entries or no match. Corrected the module
doc's "Request" bullet: the live shell resolves at **scale 1** with the request already in device
px (spec-panels' nested live-shell measurement, `docs/requests/parity-capture.md`), not at scale 2
with up to 72-px pixmaps. No existing function changed.

Engine current strict Clippy (target/reveal-idle/clippy.log) is blocked by
concurrent lookup/SVG edits: qt_svg.rs:223 unused Linear x1/y1 fields;
icon_theme.rs:540-549 identical branches; test line766 type_complexity.
Please fix these in the active parity changes without altering numeric behavior.
Engine is measuring reveal/idle and will re-run strict Clippy after consolidation.

Engine resolved those Clippy blockers surgically: removed unused Linear x1/y1
fields (kept their local gradient arithmetic), joined identical lookup-assignment
branches without changing predicates/order, and named the test tuple type. Also
the optional system GTK-cache test now follows its existing documented absence
rule when telegram is absent (None). Deterministic cache tests remain unchanged.

Engine current aggregate regression checkpoint: core/Shell/UI tests pass so far,
but controls::shell_ref_tests::tray_fallback_letter_matches_bar fails all four
light/dark @1/@2 crops (mean7.58–9.97, bad fraction0.069–0.107).
Test still assumes `zzz-unknown` produces letter Z; current shared bar references
were recaptured for corrected missing-icon/provider behavior. Please reconcile
this controls-owned test/probe with the intended reference state, preserving the
new exact tray sizing rule. Engine did not edit controls/. Log:
`target/reveal-idle/tests-final.log`; artifacts `/tmp/hyprv-widgets-test`.

Engine idle-priority CI handoff: user explicitly authorizes widget tray test and
panels cache lint repair. Tray comparison now supplies Quickshell checker image
to TrayButton; existing letter fallback behavior remains separately tested.
Panels qtext already contains spec-bar's minimal local FontDataCache alias, so
I preserve it and rerun strict Clippy rather than duplicate/revert that fix.
No production controls or panels text behavior changed in this CI repair.

Engine idle-priority final: production projected stats bridge ignores unchanged
bar labels/severity; visible graphs keep raw samples. Native CI gate now uses
a1Hz service-runtime sampler through that exact bridge, with native callbacks
and host deadlines active:10samples/10projections,0metadata UI wakes/0paints,
1dispatch/1voluntary switch (RPC boundary). Old raw bridge negative control
fails10metadata wakes/11switches while paints stay0. Non-ignored integration
test is included in workspace --all-targets CI; pinned debug ELFs auto-cleaned.
Exact workspaceClippy --all-targets -Dwarnings PASS;181affected-library tests
PASS. Tray test supplies actual18*DPR checker pixmap; panels FontDataCache alias
already fixed by owner and preserved. No canonical fixture/reference edits.
Uninstrumented overlapping old/new60s CPU0.17909/0.18313%: no significant CPU
win claimed; reported0.241% was not reproduced. <=0.1% remains unmet. Service
thread is the remaining attribution target, not an invented renderer regression.
Report docs/verify/idle-wakeup.md; raw/hash receipts target/idle-priority/.
