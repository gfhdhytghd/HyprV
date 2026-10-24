# Owner requests: hyprv-ui-bar (owner: svc-power)

## From parity QA — 2026-10-23 03:45 (verify pass 1)

Source: `python3 parity/verify/verify.py` → `docs/verify/parity.md` (bar section), which runs your
`scripts/parity.py` / `examples/reference.rs` unchanged and diffs `artifacts/reference-actual`
against `parity/reference/bar` with the foreign island rectangle cleared in both images (same
scope as your "bar-owned" report). Gate: images with `distance_field_text: false` must be exact,
DF images ≤ 8 max / ≤ 0.5 mean (the bar tray's fallback letters use QtRendering, so most bar
crops are DF images). Today: 0/312 pass; 32/230 replayed static images pass the old ≤8/≤0.5
gate (median max 43, median painted mean 0.47). Evidence: `parity/verify/out/bar/tri/<file>`.

- [BAR-PQ1] [P1] **Replay the new §14 items 22–24 fixtures** (84 references, currently compared
  against a popup-less render → all `layout`): `tray-menu-{root,hover,submenu,anim}`,
  `tray-overflow-{2,6,7}`, `battery-{charging,critical,full}` (`parity/fixtures/bar/*.json`).
  Inputs: tray menu entries are `services.tray[].menu.entries` (text, icon, isSeparator, enabled,
  buttonType 1 = checkbox / 2 = radio, checkState 0/2, nested `entries` = submenu); menus open by
  right-click on the tray icon; `tray-menu-anim` frames are ms after the right-click and after a
  click outside. Overflow uses a 1000 px output with 9/13/14 items (2/6/7 overflowed; a 1-item
  grid is unreachable, see `docs/verify/parity-findings.md`). Battery states seed
  `root.batteryInfo` (the popup is unreachable in the live shell) and click the battery label.
- [BAR-PQ2] [P1] **Tray icons must use the variant QIcon picks for the *device* size.** The
  references changed in this pass (harness fix: `image://icon` now renders at the capture DPR,
  as `QIcon::pixmap()` does at the live shell's application DPR 2). At @2, Qt resolves
  `QIcon::fromTheme(name).pixmap(18×18 logical, dpr 2)` from the theme directory best matching
  36 device px, which for WeChat is different artwork (no green square) than the one you draw;
  the other icons now differ only by resampling. Emulate QIconLoader's directory choice
  (`directoryMatchesSize` / `directorySizeDistance` with `Scale=`) at `18 × scale`, then scale the
  chosen pixmap with Qt's smooth transform. Evidence: `baseline--light@2.png` (max 239).
  Also refresh `artifacts/theme-icons` from the same rule (your exporter currently prepares
  `18*scale` hicolor images; check WeChat/Steam/Telegram against `parity/reference/bar/baseline--*@2.png`).
- [BAR-PQ3] [P2] **Rounded-rect / border edge AA** is 87 % of the bar's remaining failing pixels
  (pills, hover chips, glass cards), all on coverage edges. Already planned as BAR-RECT-1 in
  `hyprv-ui.md`; once wired, select the Qt rectangle mode in the exporter and re-run
  `parity/verify/verify.py --component bar`.
- [BAR-PQ4] [P3] `fullscreen-slide` show frames t100–t350: footprint matches exactly; residual
  is sub-pixel position during the bezier overshoot (`[0.1, 0.9, 0.1, 1.05]`) — verify the bezier
  solver against `QEasingCurve::BezierSpline` (t=350 mean 1.35 vs 0.41 settled).
- Positive: corner shades (Canvas arcs) are within 1–2/255 (`corner-bottom-*`); text accounts
  for 0.3 % of failing pixels (≤ 44 px in any image, all where glyph boxes touch a chip edge);
  workspace/metrics/network/media/notification states differ only by rect AA and icon
  resampling.

- [BAR-1] [P2] An empty or vanished tray menu polls invisibly every 16 ms — [verified review](../reviews/hyprv-ui-bar.md#bar-1).

- [BAR-2] [P2] Tray menu hit testing activates rows before they are revealed — [verified review](../reviews/hyprv-ui-bar.md#bar-2).

- [BAR-3] [P2] Closing and reopening overflow retains an old item press — [verified review](../reviews/hyprv-ui-bar.md#bar-3).

- [BAR-4] [P2] Long tray menus omit drag and flick scrolling — [verified review](../reviews/hyprv-ui-bar.md#bar-4).

## From parity QA — 2026-10-23 04:30 (verify pass 2, after your popup replay landed)

Now **32/312 pass** (corner-bottom-* exact at both scales, 24 fullscreen-slide frames).
BAR-PQ1 (popup replay) and BAR-PQ2 (tray icon variants) are resolved — thanks. Every static
strip state now has the same single residue: rect AA, max 70 / painted mean 0.42 (BAR-PQ3).
New items from the popup fixtures:

- [BAR-PQ5] [P1] **Tray menu hover hits the row above.** `tray-menu-hover`: pointer at the
  centre of "Deafen" (Qt highlights Deafen); ours highlights "Mute" (mean 7.2). Row hit-testing
  is offset by one row (check menu padding 9 + row pitch `max(34, textPixelSize + 18)` and the
  icon row, `shell.qml:620-622, 1040-1050`). Probably related to BAR-2.
- [BAR-PQ6] [P1] **Tray menu open animation geometry/timing.** `tray-menu-anim` open frames
  t=20/60/120 ms: painted footprint off by 53/33/11 px (IoU 0.28 at t=20, mean 40.7); close
  frames and settled frames match (rect AA only). Ours runs ahead: at t=20 Qt has painted
  nothing below the bar yet while ours is 50 px tall; bottom edge at t=60 Qt 136 vs ours 169,
  at t=120 Qt 222 vs ours 233 (menu rows y). Qt starts the reveal only after `positionTimer`
  (interval 0) → `scheduleOpenAnimation()` → `openAnimationTimer`, then the glass reveal
  (`AnimatedGlassPanel.qml`, `openRevealDuration` 200, `openContentDelay` 20,
  `shell.qml:683-712, 940-946`) — replicate that start delay and curve.
- [BAR-PQ7] [P1] **Tray sort ignores the icon name.** `trayItemPriority()` matches keywords on
  `[id, title, tooltipTitle, tooltipDescription, icon].join(" ")` (`shell.qml:2423-2449`), so
  items whose *icon* is discord/keepassxc/fcitx/wechat sort before Chrome. In
  `tray-overflow-{2,6,7}` (ids `app0..`, icons varying) ours keeps fixture order → 5 swapped
  icons (max 250).
- [BAR-PQ8] [P2] **Island host position on narrow outputs.** On the 1000-px output
  (`tray-overflow-*`) the reference island (`09:41 ⏻`) sits right of the exported
  `foreign_island_local_rect`, so part of it remains after clearing. Either the centre host is
  placed differently from `shell.qml:3879-3890` when the bar is crowded, or the exported rect is
  stale; please check `layout::compute` on a 1000×720 output.

### Pass 3 (04:40, your 03:59 build)
- BAR-PQ5 (menu hover row) and BAR-PQ7 (tray sort) are **resolved**: `tray-menu-hover` and
  `tray-overflow-*` now differ only by rect AA.
- Still open: BAR-PQ6 (menu open start delay), BAR-PQ8 (narrow-output island host), BAR-PQ3
  (rect AA, the residue in every static state). Small new item: `network-offline--*@2` keeps
  ~650 px > 100/255 in the tray slot at x 2962–2980 (likely the slot left by the missing
  network icon) — compare slot order/spacing with `shell.qml:4200-4250` when no network
  widget is shown.

## DECISION (orchestrator default, 2026-10-23 04:05; relayed by parity QA) — match LIVE behaviour

Until the user overrides: the port reproduces what the live QML shell actually does, each
deviation-from-intent behind a named `QUIRK_*` constant (so a later "fix forward" is one flag):
- `QUIRK_STARTUP_CONFIRM_ALWAYS_LIGHT` — StartupConfirm renders the light palette in both
  themes (qt6ct fails to load under Qt 6.12 → SystemPalette is Qt's light default).
- `QUIRK_BATTERY_POPUP_NEVER_OPENS` — the bar battery label never opens the battery popup
  (`batteryAvailable` needs data only polled while the popup is open, `shell.qml:3340-3349, 4078-4081`).
- `QUIRK_SWIPE_HIDE_NOOP` — the island swipe gesture never hides music (Qt 6.12 resets
  `DragHandler.translation` before `activeChanged(false)`, `DynamicIsland.qml:1524-1540`).
- `QUIRK_IDLE_HOVER_NO_HIGHLIGHT` — idle island lock/power buttons show no hover fill
  (the z-99 side hotspot takes the hover; references `island/idle-hover-*` = `island/idle`).
The parity references already show the live behaviour (`panels/startup-confirm*--dark` is
light; `bar/battery-*` are harness-only captures, kept for the "fix forward" option; island
`music-swipe-hide` shows no change; `idle-hover-*` identical to `idle`).
Owners: StartupConfirm → panels; battery → bar; swipe + idle hover → island.


### Integration CI — 2026-10-23T08:15:04+00:00 — Preflight parity gate

Non-installing `cutover/preflight.sh --run-tests --json`: island: render evidence stale. See `docs/verify/raw/ci/round-01/preflight.log`. Please regenerate valid current evidence after the underlying owner changes pass; do not relax the gate. This is gate routing, not proof that every routed crate caused the failure.

### Pass 4 (04:16, after the engine's Qt rectangle work)
**131/312 pass.** Every static strip state is now **bit-exact at @1 in both themes** (56/56).
All @2 strip failures share one residue, which is engine-side (see `hyprv-ui.md` ENG-PQ6):
native text glyph pixels ±1–2/255, tray icons ±2, control-panel SVG ≤ 13 (`title-none--dark@2`:
5996 px differ, all inside Text/Image items, none on shapes). Bar-owned items still open:
BAR-PQ6 (tray-menu open start delay; `tray-menu-anim` mean 1.78 now, mostly rect AA ≤ 72 on the
menu card), BAR-PQ8 (narrow-output island host), `tray-menu-submenu` (mean 4.4: compare the
"< Back" row and radio indicators), `network-offline@2` tray slot (max 239).

## ENGINE-SHARED-1 — owner migration requested (engine, 2026-10-23)

The shared Qt rectangle, image filter, Canvas arc and centre-alignment APIs are
ready. Please switch the implementations you own as directed in
[the engine handoff](hyprv-ui.md#engine-shared-1--apis-ready-owners-switch-now).
Renderer now uses QtQuick rectangles by default; remove rectangle-to-Picture
baking so transforms and item opacity pass through the engine once. Keep the
original image pixels and set Node.image_sampling from QML smooth/mipmap flags.
No component source outside engine ownership was edited for this migration.

## UI rendering performance (svc-media, 2026-10-23) — see [docs/verify/ui-perf.md](../verify/ui-perf.md)

- [BAR-PERF-1] [P2] **`view::reconcile` compares effects through `format!("{:?}")`** (`view.rs:447`)
  for every node on every update: the main source of the bar's **1 772 allocations per update**
  (56 µs) even when nothing changed. Compare fields directly (engine PERF-UI-6 adds `PartialEq`).
- [BAR-PERF-2] [P2] **The whole bar tree is rebuilt for every update** (`view::update` →
  `build(m)`), including the island slot closure (`m.slots.island`, which calls `Island::build`).
  Skip `build` when the model inputs are unchanged and nothing animates, and for the island slot
  use `hyprv_ui_island::Island::visual_rev()` (new): reuse the existing island subtree while it
  returns the same `Some(rev)`; rebuild while it is `None` (animating).
- [BAR-PERF-3] [P3] Fullscreen hide → reveal damages the whole strip each frame
  (141 k px² mean, GL record 3.2 ms/frame). Mostly engine PERF-UI-2/4; if the strip motion is a
  pure translation, apply it as the root transform so text/rect caches stay valid.


### Integration CI — 2026-10-23T08:16 UTC — component-specific parity evidence

Latest strict parity result: 131/312 entries pass; 181 fail/missing/excluded. First nonpass: `baseline--dark@2.png` (fail, max=14, mean=0.0541674979031086). Render crate_mtime=1792743132.7482965; latest crate file `crates/hyprv-ui-bar/src/popups.rs` mtime=2026-10-23T08:15:28.968022+00:00, SHA-256 `109aabd9e8c98218dde59f96e712fedca24ae39f10f461c2fb898d3f6db0b120`. Evidence: `parity/verify/out/results.json`, `docs/verify/parity.md`. Shared renderer changes in `hyprv-ui` can invalidate component results; this identifies the gate owner, not a causal commit.

### Pass 5 (04:21) — **222/312 exact** (excluding 12 live-unreachable battery captures)
@2 strip states are exact now too. Remaining: `tray-menu-anim` (all 48 frames, rect AA ≤ 72 on
the menu card + the open delay BAR-PQ6), `tray-menu-{root,hover,submenu}` (card edge AA ≤ 16–23;
submenu mean 4.4), `corner-top-*` (max 3, fill), `network-offline@2` (one tray slot, max 239),
BAR-PQ8 (narrow-output island host).

- 2026-10-23 BAR-4 implemented: menu drag/flick and wheel physics now use shared `hyprv_widgets::flickable::Flickable` in `src/popups.rs`; child row clicks cancel on scroll grab, frame animation ends at rest/unmap, ordinary clicks still dispatch once. Added `tests/flickable.rs`. [Qt traces, validation and API](../reviews/scratch/flickable-implementation/README.md): 48 traces / 10,952 frames, ≤0.000002218 px error; actual-workspace tests and library Clippy pass. Native `Popups::menu_wheel` preserves pixel deltas/phases; the compatibility PointerEvent adapter prioritizes discrete angles. Preserve this wiring when touching popup lifecycle code.


### Integration CI — 2026-10-23T08:36:30+00:00 — Preflight parity gate

Non-installing `cutover/preflight.sh --run-tests --json`: bar: render evidence stale. See `docs/verify/raw/ci/round-02/preflight.log`. Please regenerate valid current evidence after the underlying owner changes pass; do not relax the gate. This is gate routing, not proof that every routed crate caused the failure.

## From hyprv-ui-panels (spec-bar), 2026-10-23 — FYI, no change needed

Panels reuse `tools/svg_bake.cpp` unchanged: `crates/hyprv-ui-panels/tools/bake.py`
compiles it from your path and bakes the panel SVGs (cache key icon/size/scale/colour,
same as yours). Verified against Qt Quick: QSvgRenderer at `sourceSize × dpr` +
glGenerateMipmap level + MultiEffect colourisation = Qt within 1/255. Please keep its CLI
(`source out w h scale`) stable, or tell me here if you change it.


### Integration CI — 2026-10-23T08:40:09+00:00 — Preflight parity gate

Non-installing `cutover/preflight.sh --run-tests --json`: panels: render evidence stale. See `docs/verify/raw/ci/round-03/preflight.log`. Please regenerate valid current evidence after the underlying owner changes pass; do not relax the gate. This is gate routing, not proof that every routed crate caused the failure.

### Round-6 review → hyprv-ui-bar (2026-10-23)

`surfaces.rs:84` names the corner overlay `hyprv-screen-corner-overlay-{output}-{corner:?}`. `hyprv-shell/src/host.rs:992` and QML use `hyprv-screen-corner-overlay-{output}`. Pick one; QML is the parity reference. No hypr layer rule matches either today. See CUT-7 in [docs/reviews/packaging-cutover.md](../reviews/packaging-cutover.md).

### Engine host retained slots and native wheel checkpoint
Content::Shared(Rc<Node>) is now available for immutable host subtrees. Please
add its content_equal arm with Rc::ptr_eq to avoid conservative wrapper edits;
engine already handles rendering/bounds/find/hit and rejects descendant edits.
Assembly caches island by visual_rev, calendar by model/geometry/input invalidation,
and forwards native phased Wheel to Popups::menu_wheel. Native input conversion
compiles again and the stronger nested GPU capture test passed.


Integration CI freshness clarification at 2026-10-23T09:46:02+00:00:

- bar: `crates/hyprv-ui-bar/tools/__pycache__/check_rect_effect.cpython-314.pyc` mtime 2026-10-23T09:07:43.785660+00:00, SHA-256 `f22576d6107adf3a36ef4b6376ac2ab07bb0baf66a954634e8e46a296fe090c4` exceeds render crate_mtime 1792744878.0077024. Pixel-gate result counts remain {'excluded': 12, 'fail': 68, 'missing': 8, 'pass': 224}.
The generated-file timestamp is not proof that this component source changed. The strict pixel failures remain independent blockers.

### Parity QA (07:37) — GPU path wired, thank you
CPU and production-GPU renders give the same pass set (224/312). 259 of 304 images differ
between the two backends, by at most 3/255 (worst `workspace-urgent--*`), so the CPU harness is a
close proxy for the bar. Remaining failures (GL, gated): fill/color 34, rect AA 33, text 1.

## Qt rect mode consolidated in hyprv-ui (svc-media, 2026-10-23) — informational

One rectangle implementation now serves every crate: `hyprv_ui::qt_rect` through the
Renderer's default `RectMode::QtQuick`. GPU surfaces draw Qt's vertex-AA mesh with
`drawVertices` (exact premultiplied interpolation via a palette shader; ≤2/255 from the
CPU rasteriser, which parity uses). PERF-UI-5: calendar-morph rect cost ~18 ms → <0.1 ms
per frame. No edits were needed in this crate: it already draws `Content::Rect` (panels:
`qrect.rs` now only adds scissor/text snapping and wraps `qt_rect::rasterize` in
`rect_into`). Please don't add private rectangle rasterisers; file engine requests instead.

### Real-shell idle CPU: Shared slot reconciliation feedback loop (fixed)

The real release shell (not the JSON probe) used 5.8278% CPU over 60.4003 s in a 6144x3456 scale-2 nested session with zero native agent-hook events and a private PID namespace. UI thread runtime was 3.4665 s; services were about 0.08% of a core. Raw: `docs/verify/raw/perf-shell/release-quiet/release-quiet-idle.json`.

`hyprv-shell::assembly::IslandTree` reuses `Rc<Node>` via `Content::Shared`, but `hyprv-ui-bar::view::content_equal` lacked that variant and returned false on every update. The paint callback schedules reconciliation again, so an unchanged slot perpetually marks the bar dirty and requests another frame. Fixed with `Rc::ptr_eq` for the Shared variant; new trees still invalidate. Added `shared_slot_identity_does_not_sustain_idle_frame_callbacks` to `tests/behavior.rs`. It fails on the original comparator (one edit instead of zero), then all 17 behavior tests pass with the fix. Logs: `docs/verify/raw/perf-shell/shared-slot-{before,after}.log`. No pixel content, animation durations, subscriptions, or hook processing was changed. Release remeasurement follows in `docs/verify/perf-shell.md`.

Rehearsal evidence (2026-10-23, `docs/verify/raw/cutover-rehearsal/snapshots/S4-native.json`): native maps 4 corner layers per output (`hyprv-screen-corner-overlay-WAYLAND-1-TopLeft`, …). Legacy QML maps one (`hyprv-screen-corner-overlay-WAYLAND-1`).

### Engine strict workspace Clippy checkpoint — 2026-10-23
Exact `cargo clippy --workspace --all-targets -- -D warnings` currently reaches
only three errors in new bar-owned examples/gl_stress.rs: unused rc::Rc import
(line10), i%2==0 (line61), i%3==0 (line80). Owned engine/shell/widgets targets pass.
Please fix in your path; I will rerun the exact workspace command afterward.
CanvasPath::stroke_node/fill_node now use Content::Canvas with Qt QStroker,
QRasterPaintEngine's analytic line AA and gray raster; 72 independent Qt 6.12
coverage goldens pass exactly at DPR1/1.25/1.5/2. This is available for remaining
bar Canvas migration; no component source outside owned paths changed here.


### Real-shell residual idle UI cost [P1]

After fixing Shared identity and clip canonicalization, the real release binary still uses **0.4159% process CPU** (0.4359% summed thread runtime) over 60.1166 s with zero agent-hook inputs, no media, no spectrum, and panels closed. Main UI thread consumes **0.221571 CPU s**; service thread **0.036702 s**; audio RT threads consume zero. Sampling/procfs counts fall to **12.71 scheduling timeslices/s**, so the perpetual frame feedback is substantially reduced, but <=0.1% is not met. See `docs/verify/raw/perf-shell/final-quiet/final-quiet-idle.json` and its `-data-status.json`.

Remaining path to optimize: `Assembly::sync` still rebuilds the full desired tree and calls `update_bar_scene` on ordinary schedule callbacks, including the follow-up callback after a genuine 1 Hz stats change. `bar::build` rebuilds all modules/layout even for a single changed stat; scene reconciliation then repaints the damaged bar. Preserve clock, stats, tray and animation changes while avoiding redundant rebuilds and retaining unchanged module trees. This localizes residual CPU to the UI update/paint path; build-versus-paint sub-costs are not claimed as separately sampled. Do not mark the 0.1% budget green from the event-excluded run.

Final Canvas coverage proof: now84 cases including overlapping ring track/progress,
all max0 through CPU and private EGL RGBA8. `canvas::CanvasBatch { operations }`
keeps sequential paints in one texture and `.node(frame)` mounts it. If migrating
bar paints, please handle `Content::Canvas(Rc<CanvasBatch>)` in reconciliation;
retained batch identity can use Rc::ptr_eq like Shared slots. Do not accidentally
reintroduce perpetual idle redraws by treating this content as always different.
Exact workspace strict Clippy passed after your example/native text updates.

## From spec-bar (2026-10-23): engine-effects audit — no bar change needed

Bar already uses engine image sampling (`ImageSampling::Mipmap`) and `MultiEffect`
(glass, `source_texture_size`); it has no private mask/colourise code. `src/svg.rs` is an
SVG rasteriser (QtSvg-style gray outline), not an effect, and the engine has no
equivalent, so it stays. No bar files were edited; verify bar 303/304 under the
ARCHITECTURE gate is unchanged across the effects work.

Native functional E2E checkpoint: bar clicks/hover/wheel, calendar and island pass
through virtual-pointer native seat dispatch. Tray menu first popup currently
stalls UI (91% CPU; no pointer input reaches the menu; subsequent private RPCs time
out). Engine is profiling the private shell under gdb; no live session touched.
Native menu layer/cursor geometry is correct. Actual menu Action is MenuEvent,
not MenuActivate; test expectation corrected. Large-tray seed lives under
hyprv-shell/tests/fixtures/functional/tray.json. Please avoid editing host files.

Private gdb found the stall's hot path: libfontconfig FcPatternGetString →
SkFontMgr::onMatchFamily → ParagraphImpl::computeEmptyMetrics → wrapped_height
(popups.rs1273) → battery_line_height → battery_height → Popups::update421,
including while only a tray menu is mapped. Every wrapped_height creates a new
FontCollection and font matches; CPU91%, native input/RPC starves. Please memoize
wrapped heights / retain FontCollection (and avoid measuring hidden battery every
menu frame) in your path. Stack: target/functional-menu-gdb2/run-*/fixture/shell.log.
Engine is separately correcting host popup scene retention and observer redraws.

## From spec-panels (full-screen parity QA, 2026-10-23)

Evidence: `docs/verify/parity-fullscreen.md`, `docs/verify/raw/parity-fullscreen/` (compare.json/residual.json, heat/, zoom/). References: `parity/fullscreen/reference` (original shell, nested Hyprland, whole output).

1. **T1 missing tray icon.** For an SNI `IconName` no theme resolves (fixture `telegram`), the original draws Quickshell's magenta/black missing-icon placeholder (2×2 checker, magenta top-right and bottom-left), not the letter fallback. **User decision 2026-10-23:** references must match what the user actually sees. Keep the letter only for an empty name or a failed image source. See `zoom/T1-tray-icons.png`.
2. **B1 Wi-Fi trigger highlight while its popup is open.** After clicking the Wi-Fi slot (pointer still over it), Rust paints the hover/pressed pill (≈2.9k px at logical 2828, 8); the original shows none (`zoom/B1-wifi-trigger-highlight.png`). Match the QML hover/press state for the trigger while the popup holds the grab.

Native E2E follow-up: tray menu activate/toggle/disabled/Escape/scroll/submenu-back
now pass through the real virtual-pointer/keyboard seat path; ordered MenuEvent +
MenuClose is observed. The hidden-battery text-measurement perf issue remains:
`battery.open-close` takes ~55 seconds in the debug assembled host (receipt
`target/functional-corrections/manifest.json`). The functional observer timeout is
now 60 seconds; no input or component-handler replay was substituted. Source
behavior retained: disabled tray rows fall through to `shell.qml:914` backdrop.

### Parity QA (10:05) — `tray-menu-submenu` recaptured with an explicit hover
Thanks for the root cause (VALIDATION.md: hidden input state). The fixture now ends with
`{"hover": {"text": "Do Not Disturb"}}` + 300 ms before the capture, so the pointer state is
explicit. All four references (light/dark × @1/@2) now show the DND row hover; the new
`dark@2` is byte-identical to your explicit-hover reproduction (SHA-256 `6ccc4569…6da5d`), and a
second capture is byte-identical. Your adapter already replays moves, so replaying the new step
should make this state comparable; the old frozen mismatch (max 23) is retired.

Engine consolidation is active: wrapped popup text now delegates to the engine's
bounded `text::measure_wrapped` cache, and Popups::update skips hidden battery
measurement. Only that measurement/visibility path was edited in production bar.
ENG-PQ8 test/example imports now use the engine offscreen backend. Pinned CPU/GL
before/after corpus validation is running.

Queued user request: perf-shell.md cause 4, incremental bar trees on schedule/1Hz
stats callbacks. Please coordinate an API that exposes only changed module nodes
and stable NodeIds to the host; the host should damage those nodes rather than
rebuild a full bar. Engine will inspect current reconcile APIs after consolidation.
Potential popup lifecycle memory findings are expected from svc-media; preserve
scene/cache ownership evidence until that review arrives.
- (10:23) After the recapture all four `tray-menu-submenu` variants fail at max 23–24, mean ≈ 4.2:
  the references now show the DND hover, your replay does not. Please replay the new final step
  (`hover {"text": "Do Not Disturb"}`, then 300 ms) through your adapter's text-target mapping.
  Bar GL overall: 244/312.

Engine implemented the queued cause4 API in `view::{RetainedView,UpdateStats}`
with the existing numerical module builders. `layout::Module` now derives
PartialEq. Host owns one view per mount and drops it with the surface. Unchanged
module subtrees are neither built nor traversed; geometry/scale/theme/hover/icon
IDs/notification-dot and membership inputs invalidate appropriately. Small chrome
and hosted slots remain independent. Scheduled callback/generation/sub-label
regression constructs zero modules, damages zero nodes; a real stats change
retains the other modules and damages less than half the bar. Nine transitions
compare incremental/fresh CPU rasters byte-exact. Please preserve the generic
`build_with` builder seam when editing the public pure `build` implementation.

## From spec-panels (full-screen parity QA, run 2, 2026-10-23)

Evidence: `docs/verify/parity-fullscreen.md` (Run 2), `docs/verify/raw/parity-fullscreen/after-t1/`.
1. **T1 resolved in hyprv-shell/hyprv-widgets.** The bar now receives Qt-identical tray pixmaps and Quickshell's placeholder. No bar change was needed: `image_node`'s Mipmap sampling is plain bilinear for these magnifications.
2. **T2 (new) overflow grid order.** The original fills the overflow popup from `sortedTrayItems.slice(visibleTrayCount)` (shell.qml:4211–4212), the same sorted list as the bar row. With 96 items that's a block of chrome icons, then the telegram placeholders. The Rust overflow grid alternates chrome/placeholder, so it isn't taking the sorted list's tail. Every misplaced icon is a full 1,296 px (36×36) cell. `after-t1/zoom/T2-overflow-order.png`.
3. **B1 unchanged:** the Wi-Fi trigger shows a hover/pressed pill while its popup is open (≈2.9k px at logical 2828, 8).

Battery latency follow-up:1.19s earlier figure was the entire reset/open/assert/
close/wait transaction. Actual cold trace is still slow:~132ms before creating
surface, chiefly repeated Skia system font-manager enumeration. Engine is fixing
shared manager reuse in text/render. No popups.rs or module visual changes needed
so far. First-frame timing now uses native-release/configure/submitted markers,
separately from declared20ms animation pause/16ms staging and close duration.


### Snapshot CI — 2026-10-23T14:58:10+00:00 — clippy

REPRODUCIBLE: failed twice on same snapshot. Snapshot `/tmp/hci-sys/round-04-20261023T145010Z/rshell`; fingerprint `0beac47f2ad42142b33ac88b6a3c54c564a81704cbd3d69d6ed31b4e39a046db`. Concurrent live edits cannot change this source copy.

Command: `['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '--message-format=json', '--', '-D', 'warnings']`. Attempts: `[{'code': 101, 'log': 'round-04-20261023T145010Z/04-clippy-1.log'}, {'code': 101, 'log': 'round-04-20261023T145010Z/04-clippy-2.log'}]`. Locations: crates/hyprv-ui-bar/examples/../../hyprv-ui/tests/common/batch_material.rs:8, crates/hyprv-ui-bar/examples/../../hyprv-ui/tests/common/batch_material.rs:8.

`crates/hyprv-ui-bar/examples/../../hyprv-ui/tests/common/batch_material.rs`: `unavailable`

```text
error: float has excessive precision
 --> crates/hyprv-ui-bar/examples/../../hyprv-ui/tests/common/batch_material.rs:8:36
  |
8 |  let mut image=Node::new(Rect::new(3012.109375,21.,16.,16.),Content::Image(control));image.image_sampling=hyprv_ui::image::ImageSampl...
  |                                    ^^^^^^^^^^^
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.95.0/index.html#excessive_precision
  = note: `-D clippy::excessive-precision` implied by `-D warnings`
  = help: to override `-D warnings` add `#[allow(clippy::excessive_precision)]`
help: consider changing the type or truncating it to
  |
8 -  let mut image=Node::new(Rect::new(3012.109375,21.,16.,16.),Content::Image(control));image.image_sampling=hyprv_ui::image::ImageSampling::Mipmap;image.opacity=if faded {0.72}else{1.};root.add(image);
8 +  let mut image=Node::new(Rect::new(3_012.109_4,21.,16.,16.),Content::Image(control));image.image_sampling=hyprv_ui::image::ImageSampling::Mipmap;image.opacity=if faded {0.72}else{1.};root.add(image);
  |


```

Logs relative to `docs/verify/raw/ci-snapshots/`. No causal commit inferred.

## A2 (parity-fullscreen calendar-morph) — island paints above the calendar (spec-bar, 2026-10-23)

`src/view.rs`: `QUIRK_ISLAND_ABOVE_CALENDAR` adds the calendar group before the island group in
the centre section. DynamicIsland's root has `z: expanded ? 40 : 1` (DynamicIsland.qml:140) while
the calendar Loader keeps z 0. So during the morph cross-fade the fading island paints over the
fading-in calendar, although it is declared first (shell.qml:3912/3958).

Effect:
- full-screen calendar-morph open-t0016 / open-t0100 / close-t0100: ~1k px > 8 each (date header
  under the island pill) → 0, max ≤ 6 (with the hyprv-shell driver change).
- `cargo test -p hyprv-ui-bar` passes. It's a surgical edit; please re-read before editing view.rs.

### Parity QA (~11:30) — tray-icon references recaptured at the live size (36 device px)
spec-panels' measurement showed the harness's `image://icon` provider over-resolved @2 tray
icons (64–72 px); it now resolves `QIcon::pixmap(36×36)` at DPR 1 exactly like the live shell
(chrome stays hicolor 32×32, discord 256→36). All @2 bar references were recaptured; please
make `artifacts/theme-icons` follow the same rule (request in device px, no extra DPR, never
upscale). `tray-menu-submenu` already carries the explicit DND hover (10:05).

Engine performance follow-up final: controlled-quiet final release0.2163% CPU
(47.99% below historical0.4159%; budget≤0.1% still unmet). Five fresh battery
openings: first-buffer median12.59ms/max14.62ms; prepare max3.24ms. Declared
reveal timing is excluded, and a separate whole-cycle trace retains one35.10ms
render spike.162 ordinary tests/strict owned Clippy and204/204 native E2E pass.
Details and boundaries: `docs/verify/perf-followup.md`. Disk policy applied:
stale engine reconcile/functional bulk deleted; compact receipts archived under
`docs/verify/raw/engine-cleanup`. No private Cargo target directories.

### Parity QA edit (approved by orchestrator) — tray icons follow the live lookup
`examples/reference.rs` now resolves fixture tray icons with
`hyprv_widgets::icon_theme::tray_raster(name, request)` (request = round(18 × scale) device px,
resolved at scale 1; `network-offline@2` keeps its documented 18-px first request) instead of the
Qt-helper PNGs from `scripts/theme-icons.cpp` (that helper passed the DPR a second time:
`pixmap(QSize(18·scale), scale)` → 64–72 px, the same over-resolution parity-host had). Removed
the now-unused `icon_path`. `scripts/parity.py` still builds the helper output but nothing reads
it any more — you may drop that block. Unresolvable names (telegram) still get no image → letter
fallback, as in the current references.
- (11:50) **Reverted** the `tray_raster` switch in `examples/reference.rs`: it regressed bar GL
  151 → 55 (all @1 tray icons too), so `hyprv_widgets::icon_theme::tray_raster` does not yet
  reproduce QIcon's pick/scaling at request 18. Back to the prepared-PNG path (pre-change logic;
  `icon_path` fallback dropped since prepared dirs always exist). Still open: regenerate the
  prepared PNGs with `pixmap(QSize(18·scale), 1.0)` (parity.py passes `str(scale)` as the DPR).
- (resumed) **Fixed** in `scripts/parity.py` (approved minimal edit): the theme-icon helper is
  now called with DPR `'1'` (was `str(scale)`; the network-offline `-initial` set `'2'` → `'1'`),
  i.e. `QIcon::fromTheme(name).pixmap(QSize(18·scale), 1.0)` as the live shell resolves it.
  Result on the production GL path: **bar 151 → 247/312**, `svg/icon` failures 96 → 0
  (@1 133/133 static pass, @2 114). Remaining 65: rect AA 20 (@2), fill/color 21, text 4.
  `examples/reference.rs` is back on the prepared-PNG path; `icon_theme::tray_raster` stays
  unused until its @1 mismatch with real QIcon is understood (it regressed to 55/312).

### Parity QA — references now show Quickshell's missing-icon placeholder
Unresolvable tray icons (`telegram`, `tray-fallback-letter`'s unknown icon, overflow fixtures)
now draw the live shell's 18/36-px black/magenta checker instead of the letter fallback in all
references (194 bar images recaptured). Your replay should insert
`hyprv_widgets::icon_theme::missing_icon(request)` (request = 18·scale; 18 for the
`network-offline@2` initial request) when the lookup finds nothing, as the shell host does
(`hyprv-shell/src/assets.rs` `themed_icon`).

## From spec-panels (full-screen run 3, 2026-10-23)

Evidence: `docs/verify/parity-fullscreen.md` (Run 3), `docs/verify/raw/parity-fullscreen/run3/`. 52/64 pass. Yours:
- **T2 overflow order:** 2 images, max 255.
- **B1 Wi-Fi trigger pill:** 2 images, max 56.

Both are unchanged from run 2; details above.

Engine PQ-QA 12:55 paired baseline diagnosis: same binary, all four combinations
of DITHER on/off × old Skia/new blit window copy produce byte-identical four
baseline PNGs. These two GPU-state/copy experiments do NOT cause the reported
[25,25,51,10]→[26,26,51,10] / [122,127,151,228]→[122,128,151,228]
examples: both pairs recover exactly the same integer RGBA8 premultiplied pixel.
They match the independent Qt PNG reciprocal/Newton half-tie conversion already
proven by island's oracle, versus Skia's PNG conversion. Full bar integer gate
288/304 versus canonical verifier55/312 illustrates the transport distinction.
I am adding shared `hyprv_ui::capture::qt_png(&sk::Image) -> Result<Vec<u8>,RenderError>`
with the independent exhaustive Qt oracle; production draw arithmetic is unchanged.
Please switch ONLY the final cropped image PNG encode at examples/reference.rs:571
to that shared helper, preserving GPU full-output projection/crop geometry and
all reference/comparison logic. Without that final change, the crop's second Skia
PNG encode destroys the corrected unpremultiply transport. Engine cannot edit
bar-owned source. Please coordinate / log when switched so I can rerun all3 GL.
Update: the engine's direct PNG regression proves that default lazy PNG decode +
make_subset + Skia encode PRESERVES an already Qt-serialized input. Therefore
NO bar-owned edit is required: I wired the shared converter into EglWindow's
capture serialization. The earlier requested final-crop change is withdrawn;
please preserve the replay while the GL control runs. Both independent full
channel/alpha oracle and actual encode/decode/crop/encode regression pass.
Correction to previous update: the crop probe initially used an inferred i32
pixel buffer, producing a misleading printed integer. With an explicit u8
buffer, the new regression establishes that the second Skia encode DOES restore
26; encoding the cropped image with capture::qt_png returns25 exactly. Therefore
please DO apply the originally requested final-crop helper switch. Engine tests
now assert both behaviors explicitly. No bar-owned source edited by engine.

## Island-owner controls — 2026-10-23 17:10 UTC

Shared renderer exactness fixes are logged inhyprv-ui.md. A matched frozen bar
control preserves303/304 (exact191, paintedMAE0.025054), with the old letter
fallback and full output viewport. No production bar source was changed by the
island owner. Receipt: island/artifacts/bar-frozen/gate/gate.json; private adapter
source/protocol hash is retained beside it. A crop-sized viewport was a negative
control (39 max1 regressions), not retained.

Current QA references/adapter were concurrently updated for missing-icon
checkerboards and device-pixel tray requests. Their shared artifact cache was
still old (Chrome@2 source64px). Baking hicolor assets PRIVATELY at18/36,DPR1,
including18px network-offline-initial, gives current bar288/304 exact, v1=300,
max24:12 max1–2 crops plus4 submenu/menu hover crops. No reference or shared asset
directory was edited. Current matched-input receipt and source asset hashes:
`crates/hyprv-ui-island/artifacts/bar-current/{gate/gate.json,provenance.json}`.
The incompatible mixed-protocol111/304 and stale-asset198/304 runs are explicitly
negative controls, not renderer regressions. Keep QA's live adapter changes;
`check_bar_current.py` shows how to prepare a private exact input set safely.
User explicitly approved the single replay-line edit. Applied ONLY the final
cropped PNG encoder replacement at reference.rs:571 with
`hyprv_ui::capture::qt_png(&cropped)?`; scene/render/crop arithmetic unchanged.
EGL capture retains its original encoder; the new shared converter runs only
after the final bar crop. No production blend or image/glyph geometry changed.
Full GL re-verification follows.

## Bar exactness follow-up — user-authorized 17:20–19:20 UTC

Island rendering residuals now all closed (379/380; remaining hidden stale hover sent to spec-island). Current bar submenu adapter still stops immediately after the Status click; it never reads QA's newly added hover("Do Not Disturb") +advance300. Minimal owned example change: replay remaining fixture hover/advance steps after that click through Popups::pointer, finding the new page's current text node each time. Add an example regression asserting the resulting DND row fill. Production popup handlers unchanged. All4 hover images should close; overflow12 remains separately diagnosed.

Overflow root cause proven from Qt's actual GL VBO: narrow-output right status pill startsx567.65625; Rustx567.6600341796875. The fixture adapter leaves IslandGeometry's approximate default width120.16, whereas real idleRow font geometry is120.15625. This feeds availableRightWidth→tray budget→entire right section x. Wide-output fixtures don't hit the budget clamp, hiding the approximation. Compute the adapter's idle island width from real16px bold lock/clock/power metrics and original paddings, using the fixture clock. Runtime host already supplies actual island geometry; no production layout/color adjustment. New regression asserts the independent Qt status-pill x567.65625 and exact idle width. Foreign-slot mask floor/ceil pixels remain identical; thresholds/mask logic unchanged.

Current recaptured bar now304/304 BIT-EXACT, worst0/paintedMAE0, all categories error0. Four DND hover crops closed by replaying actual remaining fixture steps; twelve overflow crops closed by measured idle-island width120.15625. Both adapter regressions pass. No production bar change, reference change, threshold change or foreign mask logic change. Receipt island/artifacts/bar-current/gate/gate.json; pinned inputs provenance.json. Full renderer/panel validation is completing.

## Accepted final combined-tree replay — 2026-10-23 18:04 UTC

Island380/380 and bar304/304 are bit-exact (max0, all category errors0).
Panels255/300 retains every one of the255 pre-batch-refinement passes; no regression.
All three final replay receipts have no source drift; island/bar also fingerprint
all reference PNGs/JSONs with no drift. Current source hashes match those receipts.
Aggregate proof: `crates/hyprv-ui-island/artifacts/accepted-summary.json`.

Engine/island/bar all-target tests143 passed,0failed,21intentionally ignored;
strict all-target Clippy with --no-deps passes. Six independent nativeGL goldens
pass. Stress:6000frames,4mappedprivate surfaces,1974resizes,63recreates,
2662partial-damageframes,776sampledreadbacks,0mismatches. Earlier unreproduced
GPU-corruption risk remains OPEN in docs/STATUS.md. The concurrent codec-tail
assertions are resolved by the owner's single-pixel Qt oracle; final tests and
all-three replays were rebuilt/repeated afterward. Unscoped Clippy's concurrent
panels type_complexity warning is separately logged above; no panels source edited.

Retained rendering changes: direct clip-relative matrices; per-element QSG
smooth-color batch coordinate basis with opacity/clip/overlap barriers; optional
QRectF source-size metadata on the existing cached GPU rectangle mode; island
controls Row hierarchy and qreal binding staging. Shared README documents the
optional metadata API; no new shader/program/cache or runtime Qt dependency.
Bar-only reference adapter replays submenu replacement hover and uses measured
idle width120.15625. Reports: island/VALIDATION.md and bar/VALIDATION.md; first-box
history preserved separately. Removed owned obsolete GL blobs/CMake observer
build/intermediate PNG corpora; hashed cleanup receipt in artifacts/followup-cleanup.json.

Engine idle priority: receipts show UI59.8→68.9→64.7ms/60s, service39.1→52.5→77.7ms;
latest increase is mainly service-side, unchanged provider source hashes. Same
ELF quiet runs vary0.226→0.306% while actual tray pixmap updates13→22. No nativeGL
rollback justified by those windows. Host now projects stats into bar displayed
rounding/rate/power labels plus temperature/power severity and iface. Raw graph
updates still forwarded only for visible Stats/Panel consumers. No bar source
edit; please flag any future layout input beyond layout.rs209–225/441 and
format::{network_label,power_label}. Boundary regression covers same-text/different
icon/color. Native quiet CI test already10ticks/11dispatches/0paints in10.1s.

Engine ENG-MEM idle GPU attribution: production `surfaces::corners` returns a
full-output 6144×3456 overlay (81MiB per RGBA buffer; retained + EGL swap chain).
Spec ui-bar.md §9.1 explicitly permits four tiny surfaces. Host now crops its
canonical four child nodes to four tiny passthrough overlay surfaces, preserving
fractional animation offsets within integer layer-shell margins; no edits in bar
owned paths or reference pixels. Engine validating composited equivalence and
fresh RSS/GPU/frame measurements. Please preserve component corner geometry.

Engine/user follow-up: user accepts four tiny corner surfaces for~267MiB GPU
saving ONLY with identical on-screen pixels. Root README now documents the
intentional topology deviation, same namespace on four layers, empty input,
no focus, exclusive−1, and compositor-rule/bounds implications. Please ensure
your full-screen parity explicitly scores all four corner regions (no masks),
including default/reveal/scale states; publish per-corner equality in the receipt.
Engine CPU/private-GL crop-vs-full test already compares all pixels atDPR1/1.25/
1.5/2 incl6144×3456@2; native top geometries stay fixed across reveal.
New user priority: close panels' two show-t0120@2 crops in engine rect batching;
reading now, will preserve your acceptance source hold through19:32UTC.

Engine strict-corner follow-up: added host tools/verify_corners.py (unmasked,
premultiplied exact per corner). Current artifacts/fullscreen-core/accepted has
64 captures /256 corners, max4 alpha difference vs legacy Qt. This is already
in native full-output geometry (crop-vs-full native is exact), NOT caused by
tiny surface topology. Example idle-dark@2 TL (25,8) ours52/ref56 alpha,
(22,10)56/60. Suspect baked @corner path uses QPainterPath::arcTo whereas
ScreenCornerShade.qml uses Canvas context arc cubic construction. Please own
corner asset geometry correction and run the strict checker alongside full-screen
gate. User accepts topology deviation only when actual corner pixels are equal.
Receipt target/qa-precision/fullscreen-corners.json.

Engine coverage hook: tools/summarize_fullscreen.py now writes
corner-comparison.json and includes summary.corners (captures/count/max/exact
pass_ flag) from verify_corners.py; existing whole-frame gate remains separately
reported. Added host checker unit tests proving one-level alpha at any corner
fails and absent captures cannot pass. Please use it on your next full-screen
run and close the pre-existing baked curve delta before claiming Qt corner exact.

Engine follow-up validation at19:46UTC: panels+calendar300 / island380 renders
complete; comparing against pre-task QA exports gives exactly2 changed images
(the requested show-t0120@2 pair), all other678 byte-identical. Both pair now
max0 in premultiplied RGBA. Fresh bar render attempted twice but blocked by
current svc-power backlight.rs:127/359 and inhibit.rs:23 #[async_trait] without
attribute import. Please finish the ongoing refactor/import update; I did not
edit service code. Bar log target/qa-precision/final-gl/bar.log.
No conclusions about bar parity regressions drawn from this failed build.

Sampling storage adapter only: examples/reference.rs converts shared sysstats iface to owned network iface; production rendering unchanged. This supports allocation-free sample snapshots.

Engine validation update: svc-power imports are restored. Bar GL replay now
completed at19:46UTC:304/304 exact, owner gate JSON at
target/qa-precision/final-gl/bar-report/gate/gate.json. QA comparator code
passes panels300/300, island380/380. Of680 panel/calendar/island exports,
678 byte-identical vs pre-task QA and the requested2 QuickAdjust crops now
exact. Engine tests pass:6batch,59ordinary library,11surfacelessGPU+3private
Wayland EGL,2corner checker; all-target UI clippy clean. Strict legacy corner
max4 is separately flagged (pre-existing, crop-vs-full native unchanged);
see docs/verify/corner-precision.md.

Corner checker JSON API finalized: summary.corners.pass (same key as standalone corner-comparison.json); no pass_ spelling.

- 2026-10-23 sampling validation: CPU fixture replay completed for the shared-interface reference adapter; production rendering code unchanged. CPU-backend gate retains its known differences, separate from owner's passing production GL gate. See docs/verify/sampling-cost.md.

Engine priority stats-tick follow-up: user wants <0.3ms main-thread changed tick
and <=0.1% whole-shell. Current RetainedView::update still layout::compute +
build scaffold + reconcile after the bridge already filters rendered keys.
Please expose a fast `RetainedView::update_stats(scene,m,normalize)` for the
stats-only host branch: no layout/scaffold for stable measured label widths;
edit only changed text/threshold nodes, fallback full update on width/geometry
changes. Preserve same rounded text no-op, severity thresholds and existing
parity/e2e; owner implementation in your paths requested. I will handle EGL
buffer-age/partial blit, renderer damage traversal and profiling. Host currently
calls full update at assembly.rs::sync_stats. Please log public API when ready.

Engine interim host fast path implemented while waiting for your public API:
assembly::patch_stats_text matches unique existing text against the five existing
stat label formats, validates old/new measured sizes using your public
layout::measure and preserves severity boundaries; edits only those Text nodes.
Any ambiguity, dimension or threshold change uses full RetainedView::update.
Private retained signatures may lag until next component update, which safely
reconciles the already-updated nodes; source tree/layout stay equivalent. Please
still expose your own API to remove this small host presentation adapter when
ready. I will add fresh-tree/no-op/threshold/width regression tests in host.

Engine correction to the interim adapter: after any direct text edit it drops
Mount.bar_view, so a future full update cannot incorrectly retain a cached old
signature if a value cycles back. Host tests compare fresh pixels atDPR1/1.5/2
and test no-op/width/severity fallbacks; all pass. An owner update_stats API
would avoid this conservative signature invalidation on the next full update.
Current profile: partial-only did not establish scheduler-CPU improvement;
working on shared non-elided shaping and eliminating per-node GL submissions.


Engine final stats-tick checkpoint: host patch + disjoint damage + EGL age repair
+ shared shaping/metrics + native glyph atlas pass fresh 204/204 E2E and all984
GL PNGs unchanged. Normal90s paired CPU: whole0.17919%→0.14165%, UI0.15722%→0.11830%.
31tick profile mean1.116ms (stats model0.211ms, EGL swap/commit0.628ms); targets
still unmet. Stable-width patch used10/31ticks; fallbacks built176modules.
Your public update_stats API remains useful to avoid host signature reset and
improve width-changing updates. Evidence: docs/verify/ui-tick.md.

### Final review B3: render both host states (2026-10-23)

The reference harness explicitly skipped calendar-host and island-expanded-host.
Add a harness-only adapter using the production island and CalendarPanel builders,
mounted through bar Model.slots and the production centre geometry. Dev dependencies
only; runtime APIs unchanged. These eight complete compositions have no island mask.
No reference, fixture, or foreign crate edits. Validate all manifest entries on GL;
report canonical exclusions separately from the owner export count.

B3 closure: canonical `verify.py --component bar --backend gl` now reports
300 PASS, 0 FAIL, 0 MISSING, 12 intentional battery exclusions (312 references).
Owner gate covers all 312/312, 308 exact, maximum2/255 (calendar text), zero missing.
This is the previous304 exports plus eight host crops; 304 is no longer the full
manifest denominator. All four expanded-island host crops are exact; calendar
maximum1 dark /2 light. Receipt: crates/hyprv-ui-bar/artifacts/host-coverage/receipt.json.
Bar ordinary suite35 pass/1 ignored; example replay3 pass; Python coverage regression
1 pass. Owner metrics now count the whole manifest, so skipped exports cannot yield
a false green. Existing example tests also resolve the crate root in the isolated
check manifest. Canonical generated report updated; its old explanatory sentence
claiming host views are unexported belongs to QA's report template and is obsolete.


### FR-NET-2 readiness contract — svc-sys, 2026-10-23

Preflight now requires the production `bar-gl`, `island-gl`, `panels-gl` slots; content-hashed transitive Rust/native/assets/replay/reference inputs are recorded by `parity/verify/verify.py`. CPU slots/mtime-only old evidence cannot pass. All canonical actual PNGs must exist and match their recorded hashes, including the 12 documented live-unreachable battery fixtures; only their pixel results use the shared `QUIRK_BATTERY_POPUP_NEVER_OPENS` exclusion policy. All eight calendar-host/island-expanded-host component crops must render and pass too.

Assembly remains separately gated: `python3 -B cutover/verify_fullscreen.py --binary packaging/dist/hyprv-shell --run cutover/evidence/fullscreen-run` captures the complete canonical real-output matrix in private compositors and writes `cutover/evidence/fullscreen.json`. The gate checks every image, same staged ELF/source hashes and exact unmasked corners. Full-screen references currently cover DP-4 @2; component @1 crops do not manufacture @1 full-output evidence. No tolerance or missing-image exemption added. Final frozen candidate awaits desktop/power/engine final-fix reports.

FR-NET-3 production renderer: new pure surfaces::corner_window builds the four
Canvas sprites in a shared A8 image and selects QSG screen-sized atlas sampling.
Existing component RenderControl replay uses its original capture contract;
production-window sampling is explicitly tested separately against real Qt windows.
R8 atlas experimentally reproduces original BGRA atlas/window/matte pixels exactly
while reducing8192×4096 atlas GPU storage128→32MiB. No curve/baked pixel edits.

23:11UTC final B3 +FR-NET-3 verification: all312bar images rendered,0missing;
canonical GPU300PASS+12previously declared battery exclusions; owner metric312/312
PASS,308exact, max2 only calendar-host DF text. Island380/380, panels300/300 GPU
controls pass and source-current. Full assembled64/64PASS, strict256/256unmasked
corner crops exact(max0). Restored one legacy full-output corner overlay using
R8 Qt atlas/direct window rendering; large tray Row uses Qt promoted batch basis.
Report + memory tradeoff: docs/verify/assembled-corners.md. No further source edits.

## Engine laptop Phase B resource/demand coordination (2026-10-24)
I am changing only engine/shell lifetimes and caches. Please expose your laptop
collapsed/expanded visibility model contract when ready: compact CPU+Memory5s,
detailed desktop/laptop-expanded stats1s; media/tray consumers only when rendered.
Current desktop bar.toggle is a no-op; I will not guess the laptop compact layout
or modify your surfaces. Corner sampling/viewport/namespace stay unchanged: testing
release of the **idle EGL drawable** after swap, not cropping or deleting visible
corner artwork. Exact full-screen GL and204/204 E2E remain mandatory. Service owners
own discovery/helper demand fixes. Please log public profile/model API in this file.

Engine host demand API ready in `services.rs`: `Visibility::set_bar_stats` accepts
`BarStatsDemand::{Hidden,Compact,Detailed}`; mode replacement cancels the old future
and drops its SamplingLease before acquiring a new one. Compact requests exactly
CpuTotal+Memory/5s; Detailed preserves desktop's five metrics/1s. `Consumer::Clock`
is now separate from broad native provider observation. Current desktop host derives
stats/clock demand from actual translated CPU/memory layout rectangles intersecting
the surface (so fully fullscreen-hidden labels hold no stats lease). Laptop compact
layout integration should aggregate Compact vs Detailed over outputs and keep Clock
true for its visible15px strip. Resource savings default on, no laptop flag required.

## Laptop deploy user decisions / CPU watts audit (2026-10-24)

Confirmed panel 2880x1800 scale2 (logical1440x900), no eGPU/root eGPU service,
island read-only. CPU W is NOT hidden by battery in expanded bar: independent
cpuPowerTrigger at laptop shell.qml:4198–4204, followed by battery fallback slot.
Compact battery strip alone uses batteryOrCpuPowerText. User permits OPTIONAL
read-only CPU-power helper; default user-only install can show legacy unavailable
state. Please qualify laptop-bar.md §3/ownership wording that RAPL "must" always
be installed/active: only an explicitly selected CPU-only capability may activate,
never the current desktop cooling helper. Visibility leases, CPU-available and
unavailable references remain required. Deploy spec/source receipt updated.

### PQ-QA → UI owner: laptop parity references are ready (2026-10-24, laptop Phase B)

The laptop references are in `parity/laptop/reference/{laptop-bar,laptop-panels}/`, with fixtures in `parity/laptop/fixtures/`. They use the same output contract as the desktop sets: manifest, regions sidecars, and `source_ref: origin/laptop@24972f7`. The screen is `eDP-1` 1440×900 logical at scales 2 and 1.

Please add laptop fixture replay to your parity renderer, so that `verify.py` can diff `laptop-*` the way it diffs desktop. Notes:
- The input data is canned process output, just as on desktop:
  - `system-snapshot.sh --minimal/--full` uses a per-call `sequence`;
  - `agent-island-watch.py` streams raw snapshots;
  - `battery-info.sh` returns JSON.
- `services.upower`, `services.wifi.devices` and the `mpris` players with `dbusName` are new.
- `laptop-island-legacy` is **not** for rendering (D1 = read-only); ignore it.

I will add `--component laptop-bar|laptop-panels` to `verify.py` once a renderer exists.

### Shared runtime profile API LANDED (engine, laptop Phase B)

`hyprv_core::profile::{Profile, profile, sys_root, sys_path}` now exists.
`Profile::{Desktop,Laptop}`, `profile() -> Profile` is process-latched; exact
HYPRV_PROFILE override (invalid is fatal), otherwise exact portable DMI or present
System-scope Battery; ambiguous defaults Desktop. `sys_root() -> &Path` latches
HYPRV_SYSFS_ROOT or /; `sys_path(rel: impl AsRef<Path>) -> PathBuf` accepts root-relative
paths, strips leading /, rejects ..; no fallback. Code against this shared API now.
Shell startup/profile/service/UI wiring follows. Do not duplicate detection.

Engine host integration needs your laptop Model/Controller contract as soon as possible:
profile field or constructor, per-output expanded target/reveal field, toggle/debounce
ownership, compact strip build/input mask, clock/stats visible queries. I will keep
500ms global debounce in Assembly/Host (one process-level controller) unless your
API supplies that. Need compact corners to replace the full-output viewport default
only if exact pixels survive. Please log API here; do not edit shell files.

### PQ-QA → UI owner: laptop replay output contract (2026-10-24, laptop Phase B)

The laptop diff is ready: `parity/laptop/verify-laptop.py`. It applies the same gate as the desktop `verify.py` and writes its report to `docs/verify/parity-laptop.md`. All it needs from your replay is a directory of PNGs:
- **Input:** replay `parity/laptop/fixtures/laptop-bar/*.json` with `HYPRV_PROFILE=laptop`, at scales 2 and 1, in both themes, on the 1440×900 output.
- **Output:** one PNG per `parity/laptop/reference/laptop-bar/manifest.json` entry, named exactly as its `file`.
- **Optional:** a `{file: [x,y,w,h]}` JSON of foreign-island rects to mask, the same as the desktop bar's `foreign_island_local_rect`.

Please make the reference and fixture roots selectable by env var (for example `HYPRV_PARITY_ROOT=parity/laptop`), so the desktop invocation stays byte-identical. Then run `python3 parity/laptop/verify-laptop.py --component laptop-bar --actual <dir> [--masks <json>]`, or tell me the command and I will run it.

Engine integration found compact strip input gap: Assembly now preserves corner
input region, but Role::Corner needs a public Input/Controller method to hit-test
surfaces::compact_strip(model).items using window coordinates and produce same
InputResult/actions. Input::pointer currently uses full layout only. Please expose
pointer_compact (or add automatic laptop compact branch to pointer). I will route
corner events to that method; log signature ASAP. Bar model/profile/reveal wired.

Engine fixed assembled corner updates (previously rebuilt only corner_window and
lost the compact strip); native atlas conversion now preserves children after the
four corner nodes, including the strip. Settled compact bar uses input_only1px
backing preserving role+25px exclusive zone, destroys hidden full-size GL target.
Please do not recapture reference to hide these host fixes; component parity uses
your paths unchanged. Need pointer_compact before final E2E, as requested above.

Engine saw Input::strip_pointer land and wired Role::Corner native events to it,
including popup commands and new HostAction::ToggleBar (global host gate). Native
compact workspace/control/stats/clock E2E cases added, using Strip.items hit rects.
No fallback synthetic input mapper or duplicated strip click policy was introduced.

### spec-bar: laptop bar API landed (2026-10-24, laptop Phase B)

Public contract in `hyprv-ui-bar` (all runtime, no cfg; desktop defaults unchanged):
- `Model.profile: Profile` (re-export of `hyprv_core::profile::Profile`, default Desktop),
  `Model.expanded: bool` (laptop per-output target), `Model.clock: WallClock {weekday
  0=Mon, hour, minute}` (strip `ddd  HH:mm`, English C-locale names), `Model.tooltip_visible`.
  Queries: `laptop()`, `bar_toggle_is_noop()`, `battery_popup_never_opens()`,
  `cpu_power_available()`, `top_corner_offset()`, `compact_strip_visible()`.
- Toggle: `ipc::toggle(&mut Model)` flips `expanded` on laptop (no-op desktop).
  `ipc::ToggleGate::allow(wall_ms)` = the one global 500 ms debounce; the host owns one.
  `HostAction::ToggleBar` is emitted by the strip clock (followed by `ToggleCalendar`).
- Motion: `reveal` animates with the Hyprland move duration/curve on laptop; initial value
  unanimated (starts compact). `Controller::sync` carries `reveal`, `tooltip_visible`.
  `Motion::reveal(now)` reads without advancing.
- Surfaces: `surfaces::compact_strip(&Model) -> Strip {frame, items:[(Target, Rect)], node}`
  (corner-window coords); `surfaces::corner_input(&Model)` = strip mask (laptop, empty on
  desktop); `surfaces::corners()` includes the strip and that mask on laptop.
- Input: `Controller::pointer_compact(PointerEvent) -> InputResult` (= `Input::strip_pointer`)
  for corner-window events. Strip clicks: workspace activate, `ToggleStats{anchor}`,
  clock → `ToggleBar`+`ToggleCalendar`, battery → `PopupCommand::Battery` (unguarded),
  Wi-Fi/icon → `ToggleControlPanel{anchor}`, bell → `ToggleVisibility`.
- CPU-power module `Target::CpuPower` (hover-only, Arrow cursor). Tooltip:
  `tooltip::rect/build(&Model)` (full-output root, input-less popup), shown after
  `tooltip::DELAY` (350 ms); `Controller::next_deadline` includes it, `frame` flips it.
- `ipc::Diagnostics` gained `any_bar_expanded`, `detailed_stats`, `wifi_scanning`,
  `notifications_alive`, `agent_watcher_alive` (laptop live values; desktop literals).
- Battery popup reachable on laptop (`power.battery_*` from UPower, 5-way mode incl. idle).

Parity (component replay, `examples/reference.rs`, production GL in private headless
Weston): laptop-bar **248/248** (94 exact, max 4), laptop battery popups **28/28**; desktop
bar **312/312** (308 exact, max 2, unchanged). Laptop selection is by the manifest's
`fixture` path (`laptop/…`) + `source_ref`, so the desktop invocation is untouched:
`HYPRV_BAR_REFERENCE_SNAPSHOT=parity/laptop/reference/laptop-bar HYPRV_BAR_ACTUAL_DIR=<dir>
HYPRV_BAR_FIXTURE_ASSET_ROOT=crates/hyprv-ui-bar/artifacts/parity/inputs/assets
target/release/examples/reference`. Island is composed from the island crate (no masks).

### PQ-QA → UI bar owner: laptop-bar references recaptured with the in-window host (2026-10-24)

`parity/laptop/reference/laptop-bar` was recaptured; 182/248 images changed. The cause: parity-host used to build the
scene window-less, so items created at startup loaded at qApp DPR 1. Laptop captures now build it inside the
scale-N window, matching Quickshell on the XPS, where qApp DPR is 2. What changed at @2:
- the strip `control-panel-*.svg` icon and the tray icons (chrome/discord/keepassxc) are rasterised at the output DPR;
- the CPU-temperature tooltip layer is crisp, where it used to be a soft 1× upscale.

The @1 changes are max-1 animation-timing noise.

**Laptop vs desktop.** For the laptop replay, render these at the true DPR. The **desktop** references are unchanged
and still carry the DPR-1 artifact for the same eagerly loaded images (98 bar images at @2), so keep the desktop path
as it is.

### PQ-QA → spec-bar: your replay against the recaptured laptop refs (2026-10-24)

Dry run of `parity/laptop/run-laptop-qa.py` with your current `examples/reference` (GL, private Weston):
- **laptop-bar 128/248.** All 120 failures are **@2**, exactly the images the in-window recapture changed:
  - the strip/tray icons are now rasterised at DPR 2;
  - the CPU-temperature tooltip layer is crisp.
- **laptop-panels battery popups 14/28.** All 14 failures are @2.
- Your earlier 248/248 (11:28) was measured against the pre-recapture set (11:44).

For the laptop profile, please render startup-loaded images and the tooltip layer at the output DPR. Keep desktop as is:
its references are unchanged.

Re-check with `python3 parity/laptop/run-laptop-qa.py --skip-e2e --skip-desktop-parity`, and post here when done. The full
laptop QA (e2e, laptop parity, desktop regression) runs after that.

Engine RSS follow-up: my first release builder correctly refused source drift
while examples/reference/laptop.rs changed. Runtime allocator-only patch is ready;
rerunning on current sources. Please leave a completion note if that reference
file is still being edited; all shared-source guard checks stay enabled.

Engine: second packaging attempt also correctly rejected the latest laptop.rs
reference edit. Production allocator build finished; retrying the now-cached
release once more against current files. My own production sources are stable.

## spec-bar: re-verified against the recaptured laptop refs (2026-10-24 12:39)

laptop-bar 248/248 (`verify-laptop.py --component laptop-bar`), battery popups 28/28,
QuickAdjust 32/32, desktop bar 312/312 unchanged. Runtime changes: `surfaces::strip_icon`
now rasterises at 200×DPR, and `tooltip::QUIRK_TOOLTIP_GLASS_DPR1` is removed (the glass uses
the output DPR). Both were artifacts of the old capture host. Details:
`docs/verify/laptop-impl-spec-bar.md`.
