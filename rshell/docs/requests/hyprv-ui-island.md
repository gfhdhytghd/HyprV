# Owner requests: hyprv-ui-island (owner: svc-media)

## From parity QA — 2026-10-23 03:45 (verify pass 1)

Source: `python3 parity/verify/verify.py` → `docs/verify/parity.md` (island section). The crate's own
`tests/parity.rs` replays every `parity/fixtures/island` fixture (thanks — 380/380 rendered). Gate
(`docs/ARCHITECTURE.md`): island references contain no distance-field text, so every island image
must be **exact**. Today 0/380 pass; 0/380 pass even the old ≤8/≤0.5 gate. Ordered by impact.
Evidence images: `parity/verify/out/island/tri/<file>` (reference | ours | heat) and
`parity/verify/zoom.py island <file> x y w h out.png [zoom]` for close-ups.

- [ISL-PQ1] [P1] **Completed agent draws an empty progress ring.** QML `agentProgress()` returns 1
  for done statuses before reading `progress` (`DynamicIsland.qml:257-269`), so the compact,
  minimal (dual) and expanded-header rings are fully accent-coloured with the ✓ inside. Ours draws
  only the track. Fails: `agent-compact-completed` (max 198, mean 7.4), `dual-collapsed-done`
  (mean 8.2), `agent-expanded-done-row` (header ring). Evidence:
  `parity/verify/out/island/tri/agent-expanded-done-row--dark@2.png`.
- [ISL-PQ2] [P1] **Content does not follow the animated frame during morphs.** `compactMusic`,
  `expandedMusic`, `compactAgent`/`expandedAgent` are `anchors.fill: islandFrame` (or
  `musicExpandedFrame`) whose width/height animate (`DynamicIsland.qml:850-1227`); the compact row
  is vertically centred in the *current* frame height, so during expand/collapse the compact clock
  slides between the frame centre and y=19. Ours pins compact content at the final position
  from t=0, giving a double clock in every mid frame. 82 images classified `animation`, worst
  `music-expand-collapse--collapse-t0105--dark@2.png` mean 24.4/255; also `dual-music-expand`
  collapse t0060 (footprint 8 px off). Evidence: tri of `music-expand-collapse--collapse-t00{30,70}`.
- [ISL-PQ3] [P1] **Qt anchors round centred positions (`alignWhenCentered`, default true).**
  `anchors.centerIn` / `horizontalCenter` / `verticalCenter` place the item at
  `qRound(parentCentre − size/2)` in logical px. Two visible cases: (a) the island itself is
  centred in the bar host (`shell.qml:3879-3890`): x = (640 − 120.157)/2 = 259.92 → Qt 260; ours
  keeps 259.92, which at @1 shifts the lock/power glyphs by 1 px (`idle@1` mean 4.4, max 205;
  all `idle-*@1`); (b) `MediaGlyphButton`'s glyph (`DynamicIsland.qml:474-483`): Qt x = 16,
  ours 15.70 → pause/play glyph 1 device px left at @2 (`music-expanded-*`). Apply the rounding
  everywhere QML uses a centre anchor (it is not applied to plain `x:` bindings or Row/Column
  positions).
- [ISL-PQ4] [P2] **`attachedCenterOffset` jumps instead of animating** at the first frame of
  `agent-appear` (t=0: content already shifted; Qt still shows the unshifted idle pill), see
  `Behavior on attachedCenterOffset` 210 ms OutCubic (`DynamicIsland.qml:159-164`) and its
  use in the host (`shell.qml:3880-3884`). `agent-appear--appear-t0000--dark@1.png` max 205.
- [ISL-PQ5] [P2] **Image sampling** (album art with rounded `MultiEffect` mask, agent logos
  816 px → 34/46/100 device px): Qt samples with `smooth: true, mipmap: true` (trilinear on a
  box-filtered GL mip chain) → visibly softer; ours is sharper. 29 % of the island's failing
  pixels (static states). Needs engine support (requested in `hyprv-ui.md` ENG-PQ2); then select
  mipmap sampling for these images (`DynamicIsland.qml:415-432, 545-553`).
- [ISL-PQ6] [P2] **Canvas paths**: `AgentProgressRing` arcs (round caps, 3 px) and
  `AudioSpectrum` bars (quadratic-Bézier corners) are QPainter-rasterised into the Canvas image
  in Qt; 22 % of failing pixels. Needs the engine's Canvas/QPainter-compatible path (ENG-PQ4);
  in the meantime verify geometry (centre, radius = size/2 − 3, start −90°, arc sweep).
- [ISL-PQ7] [P3] **Rounded-rect / border edge AA** (pills, frames, row cards): 40 % of failing
  pixels, all on coverage edges. Engine work (ENG-PQ1, BAR-RECT-1); switch the island to the Qt
  rectangle mode once the engine exposes it.
- Data note: the reference set changed once today (harness fix, see `docs/verify/parity-findings.md`);
  island references were not affected.

- [ISLAND-1] [P2] An agent reorder retargets an in-flight click to another session — [verified review](../reviews/hyprv-ui-island.md#island-1).
  **FIXED (svc-media, 2026-10-23):** press stores the session ID; `set_inputs` remaps the grab to
  that ID's new row or cancels it when the session disappears; release focuses the stored ID only
  when released inside that same row. Tests: `island::review_tests::island1_*` (mutation-checked).

- [ISLAND-2] [P2] Overflowing agent lists cannot be dragged or flicked — [verified review](../reviews/hyprv-ui-island.md#island-2).

- [ISLAND-3] [P2] Hiding the island during a seek leaves a permanent grab and preview — [verified review](../reviews/hyprv-ui-island.md#island-3).
  **FIXED (svc-media, 2026-10-23):** new `Pointer::Cancel` and `Island::cancel_pointer(now)`
  (grab loss, like MouseArea `onCanceled`: clears grab/drag/buttons_down without clicking, and
  restarts the 700 ms seek preview reset). `set_inputs` cancels automatically when the
  host hides the island (`calendar_progress >= 0.99`). Plain `Leave` still keeps an outside drag.
  Shell follow-up filed in `hyprv-shell.md` (map native `PointerEvent::Cancel` to `Pointer::Cancel`).
  Tests: `island::review_tests::island3_*` (mutation-checked).

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

## ENGINE-SHARED-1 — owner migration requested (engine, 2026-10-23)

The shared Qt rectangle, image filter, Canvas arc and centre-alignment APIs are
ready. Please switch the implementations you own as directed in
[the engine handoff](hyprv-ui.md#engine-shared-1--apis-ready-owners-switch-now).
Renderer now uses QtQuick rectangles by default; remove rectangle-to-Picture
baking so transforms and item opacity pass through the engine once. Keep the
original image pixels and set Node.image_sampling from QML smooth/mipmap flags.
No component source outside engine ownership was edited for this migration.


### Integration CI — 2026-10-23T08:16 UTC — component-specific parity evidence

Latest strict parity result: 0/380 entries pass; 380 fail/missing/excluded. First nonpass: `agent-appear--appear-t0000--dark@1.png` (fail, max=205, mean=4.129251956939697). Render crate_mtime=1792743020.8642466; latest crate file `crates/hyprv-ui-island/src/island/visual_rev_tests.rs` mtime=2026-10-23T08:14:27.434409+00:00, SHA-256 `f5367b7632a6ab72e203acbbd3ac9d87f12a9f7db593a057bc73cafb891f3213`. Evidence: `parity/verify/out/results.json`, `docs/verify/parity.md`. Shared renderer changes in `hyprv-ui` can invalidate component results; this identifies the gate owner, not a causal commit.

- 2026-10-23 ISLAND-2 implemented: `src/island.rs` now owns a shared StopAtBounds Flickable for the agent list, cancels row/music grabs on vertical scroll takeover, advances via frame time, includes inertia in animating/visual revision, and resets on effective hide. Added `tests/flickable.rs` (drag never focuses a row, ordinary click exactly once, hide/end idle cleanup). [Qt traces and validation](../reviews/scratch/flickable-implementation/README.md): 48 traces / 10,952 frames, ≤0.000002218 px error; actual-workspace tests and library Clippy pass. New `Island::wheel(x,y,Wheel,now)` preserves native pixel/phase data. Host handoff: the current angle-only shell mapping still needs to forward touchpad pixels/phases through this entry point; no shell/engine files were changed here.


### Integration CI — 2026-10-23T08:36:30+00:00 — Preflight parity gate

Non-installing `cutover/preflight.sh --run-tests --json`: bar: render evidence stale. See `docs/verify/raw/ci/round-02/preflight.log`. Please regenerate valid current evidence after the underlying owner changes pass; do not relax the gate. This is gate routing, not proof that every routed crate caused the failure.

### Parity QA status (04:36) — island 0/380, unchanged since pass 1; bar 224/312, panels 64/300
The engine's Qt rect path reduced island `rect AA` from 214 to 40 images without island
changes. What blocks the island now, in order (all still open): ISL-PQ1 completed ring
(`agent-compact-completed` mean 8.4), ISL-PQ2 morph content (`music-expand-collapse--collapse-t0105`
mean 23.9), ISL-PQ3 centre rounding (`idle@1` max 205), ISL-PQ4 offset animation
(`agent-appear` t0), then images (ISL-PQ5, 219 images now classified `svg/icon`: album art
mask + agent logos; the engine's mipmap sampling may already be available — check
`hyprv-ui` for an image sampling option) and the Canvas ring/spectrum (ISL-PQ6).


### Integration CI — 2026-10-23T08:40:09+00:00 — Preflight parity gate

Non-installing `cutover/preflight.sh --run-tests --json`: panels: render evidence stale. See `docs/verify/raw/ci/round-03/preflight.log`. Please regenerate valid current evidence after the underlying owner changes pass; do not relax the gate. This is gate routing, not proof that every routed crate caused the failure.

### Parity QA (07:37) — regression: dual-island captures no longer produced
53 references have no render: `dual-music-expand` (32), `dual-agent-expand` (5),
`dual-collapsed-{approval,done,question,running}` (4 each). `tests/parity.rs` used to render
all 380. Also wire the GPU path (ENG-PQ8 / `HYPRV_PARITY_BACKEND=gl`); the bar has already done so.

### Mechanical strict-Clippy cleanup — 2026-10-23

After the user extended the UI lint assignment to remaining workspace diagnostics, changed only `src/island/visual_rev_tests.rs`, `agent_list_wheel_and_reorder`: the equivalent integer OR pattern `60 | 61 | 62` is now `60..=62`. No production island code or test events/assertions changed. Re-read the file immediately before editing.

`cargo clippy --workspace --all-targets --locked -- -D warnings` now passes (`docs/reviews/scratch/clippy-mechanical/workspace-clippy-final-3.{jsonl,stderr}`). The focused existing regression is recorded as `island-test.log` in that directory. Before/after island/bar/panels parity evidence uses an isolated source/input snapshot to separate the mechanical UI changes from concurrent renderer work.

Mechanical-cleanup final verification: workspace all-targets strict Clippy passes; 67 focused existing tests pass. The isolated canonical verifier produced 1,236 before/after PNG pairs with identical PNG and RGBA bytes, and unchanged Qt-reference gate metrics. Existing reference failures remain; absent island/panels GPU hooks were not counted. [Receipt and scope](../reviews/scratch/clippy-mechanical/README.md).

## Qt rect mode consolidated in hyprv-ui (svc-media, 2026-10-23) — informational

One rectangle implementation now serves every crate: `hyprv_ui::qt_rect` through the
Renderer's default `RectMode::QtQuick`. GPU surfaces draw Qt's vertex-AA mesh with
`drawVertices` (exact premultiplied interpolation via a palette shader; ≤2/255 from the
CPU rasteriser, which parity uses). PERF-UI-5: calendar-morph rect cost ~18 ms → <0.1 ms
per frame. No edits were needed in this crate: it already draws `Content::Rect` (panels:
`qrect.rs` now only adds scissor/text snapping and wraps `qt_rect::rasterize` in
`rect_into`). Please don't add private rectangle rasterisers; file engine requests instead.

## From spec-bar (2026-10-23): album art + agent icon moved onto engine effects (minimal edit)

`src/view.rs` only (3 surgical replacements; nothing else touched):
- `album_art`: the rounded `clip` (skia AA rrect clip) is replaced by the QML's actual
  pipeline (DynamicIsland.qml:415-432): an item-sized group with
  `MultiEffect { mask: Mask { source: MaskSource::Rect { radius }, threshold_min: 0.5,
  spread_at_min: 1 } }` (engine ENG-EFFECTS-1, Qt-exact ≤ 2/255).
- `cover()` and `add_fit()` (agent logo, DynamicIsland.qml:545-552) set
  `image_sampling = ImageSampling::qt(true, true)` (QML `smooth: true; mipmap: true`).
Your harness (`tests/parity.rs`, 380 captures) before → after: mean 0.1738 → 0.1550,
worst max 231 → 219; 302 captures better, 0 worse in both mean and max. 20 agent dark@1
captures have a lower mean but a higher single-pixel max on the icon's right/bottom
edge (e.g. agent-compact-running--dark@1 73 → 112): trilinear at LOD ≈ 3.8 of the 256 px
logo in the engine sampler; filed for engine. music-compact-wideart--light@1 82 → 10.
All island tests pass. Revert = restore the three blocks if you prefer.

## From spec-bar (2026-10-23): island origin uses Qt 6 anchor centring (one-line fix)

`src/island.rs` `Island::origin()`: `hc()` (legacy `foundations::hcenter`, Qt 5's
odd-integer rule) → `hyprv_ui::geometry::qt_center_half` for both halves, i.e. Qt 6's
`qRound(host/2) − qRound(width/2) + attachedCenterOffset` (shell.qml:3886 centerSection;
identical to hyprv-ui-bar layout.rs). While the width animates (fractional width) the
legacy rule moved the whole island up to 0.5 px; that was the "icon" residual after
ENG-IMG-1. Your harness (380): mean 0.1505 → 0.1051, captures with max ≤ 8 16 → 29; the
six mid-animation frames (agent-appear t0060, dual-agent-expand t0070/t0140) e.g.
0.252/127 → 0.025/28, 0.523/213 → 0.174/81. agent-appear t0120 (light/dark @2): mean better,
single-pixel max 39 → 44 / 67 → 76 on the progress ring, now at the exact position but
still rasterised by Skia (round caps; engine ENG-PQ4). All island tests pass.
`hc()` elsewhere in the crate is untouched (text-centering call sites, see the engine's
ENGINE-SHARED-1 correction).


### Integration CI — 2026-10-23T12:55:37+00:00 — clippy: field `elements` of struct `hyprv_ui::canvas::CanvasPath` is private

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-ui-island/src/island/visual_rev_tests.rs:21:28`.

`crates/hyprv-ui-island/src/island/visual_rev_tests.rs`: mtime 2026-10-23T12:55:31+00:00; SHA-256 `ec07afe8c5ab62f4650c4fbdface8b4c4d3802c54a97dddc726b49b02baef85a`; changed during command: True.

```text
error[E0616]: field `elements` of struct `hyprv_ui::canvas::CanvasPath` is private
  --> crates/hyprv-ui-island/src/island/visual_rev_tests.rs:21:28
   |
21 |                     x.path.elements == y.path.elements
   |                            ^^^^^^^^ private field
   |
help: a method `elements` also exists, call it with parentheses
   |
21 |                     x.path.elements() == y.path.elements
   |                                    ++


```

Full log: `docs/verify/raw/ci/round-15/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T12:55:37+00:00 — clippy: field `elements` of struct `hyprv_ui::canvas::CanvasPath` is private

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-ui-island/src/island/visual_rev_tests.rs:21:47`.

`crates/hyprv-ui-island/src/island/visual_rev_tests.rs`: mtime 2026-10-23T12:55:31+00:00; SHA-256 `ec07afe8c5ab62f4650c4fbdface8b4c4d3802c54a97dddc726b49b02baef85a`; changed during command: True.

```text
error[E0616]: field `elements` of struct `hyprv_ui::canvas::CanvasPath` is private
  --> crates/hyprv-ui-island/src/island/visual_rev_tests.rs:21:47
   |
21 |                     x.path.elements == y.path.elements
   |                                               ^^^^^^^^ private field
   |
help: a method `elements` also exists, call it with parentheses
   |
21 |                     x.path.elements == y.path.elements()
   |                                                       ++


```

Full log: `docs/verify/raw/ci/round-15/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.
- (spec-bar, later) The agent progress ring now comes from the engine Canvas path
  (hyprv-widgets ProgressRing, see that crate's request file). Only island change:
  `visual_rev_tests::same_content` also compares `Content::Canvas` (paths + strokes) —
  without it the test saw two identical rings as different. Production `reconcile` is
  unchanged (rebuilt vector content still counts as changed, as for the old picture).

### Parity QA (09:40) — GPU path wired: 0/380 exact, but 154 within the old gate (was 16)
Gated on the production GL path now. GL is closer to Qt than your CPU harness (median painted
mean 0.064 vs 0.149). Remaining by class (GL): fill/color 140 — 105 of them max ≤ 3/255
(colour rounding on small fills; `dual-*`, `agent-appear`, `calendar-host`), canvas 102 —
`AudioSpectrum` bars incl. the 66 ms idle wave (`music-compact-paused`, `music-swipe-*`) and the
calendar-host morph, text 54, animation 44, rect AA 35. The 53 dual captures render again.

## From spec-panels (full-screen parity QA, 2026-10-23)

Evidence: `docs/verify/parity-fullscreen.md`, `docs/verify/raw/parity-fullscreen/` (compare.json/residual.json, heat/, zoom/). References: `parity/fullscreen/reference` (original shell, nested Hyprland, whole output).

1. **I2 spectrum decays without new frames.** `audio-spectrum.sh` streams one line at t=0 (`12;38;74;92;66;41;58;80;…`). The original holds those levels (expanded: 8 bars, 30–75 px tall). Rust shows low bars/dots at the same virtual time (compact 452 px at 1564, 20; expanded 5.6k px at 1648, 40). Hold the last frame like the QML does.
2. **I1 album art** (with hyprv-shell): the ♪ placeholder shows instead of the cover in compact and expanded. If the art reaches the island and still isn't drawn, the decode/draw path is yours.
3. **A2 morph mid-frames.** calendar-morph open-t0016/close-t0100: about 1k px in the clock area (1476–1576, y 32–48), no shift. open-t0100: 13.2k px over the morphing card, Rust mean alpha +1.6. The settled frames match.

## From spec-panels (full-screen parity QA, run 2, 2026-10-23)

Evidence: `docs/verify/parity-fullscreen.md` (Run 2), `docs/verify/raw/parity-fullscreen/after-t1/`.
1. **I1 and I2 were replay bugs, now fixed in hyprv-shell.** The fixture never set `art_url` (view.rs:90 needs it) and never replayed `audio-spectrum.sh`. media-playing and island-expanded now match apart from item 2; no island change was needed.
2. **I3 (new) expanded transport glyphs.** 200 px at logical (1528, 144): the pause bars in the expanded island sit about 1–2 px lower and differ slightly in size from the original (max 154 light / 140 dark). `after-t1/zoom/I3-pause-glyph.png`.
3. **A2 unchanged** (morph mid-frames open-t0016/t0100, close-t0100).

## Island exactness timebox (bar owner, 2026-10-23 15:23–17:23 UTC)

User-authorized minimal island/shared-engine edits while svc-media investigates memory. Baseline production GPU0/380 from final bar cross-crate control. First migrate island root to existing native_materials and native text leaves to QtGl; propagate those flags through island reconcile. No shared renderer change needed for this first experiment. Preserve all other owners edits, use shared Cargo target with incremental disabled, retain only compact metrics/current outputs. Verify bar and panels after shared changes.

Island next surgical changes: AudioSpectrum.qml31-105 now replayed via existing engine CanvasBatch/CanvasPath with JS f64 geometry and wave phase; original shared SpectrumBars widget untouched. CompactMusic Row fills animated islandFrame (DynamicIsland.qml869-906), so center its24px art, clock and22px spectrum in current frame height minus12px margins instead of fixed26px row. Native opt-in first replay: canonical14/380, integer-PM78/380; record both because canonical float reconstructed-PNG differences can survive when integerPM is identical. Widget lookup compile edit settled; retry normal Cargo.

Text root cause: island util::text and shared widget helpers baked HAlign line offsets into the item translation. QSG qt_native_place quantizes local glyph offsets/subpixel phase separately from item origin; lock glyph@1 diverged while@2 matched. Preserve Text.align and original box origin in island util; rebuild only owned idle button labels using that helper and re-anchor media control glyphs using Qt6 center halves (actual DynamicIsland.qml475). Change island hc helper to qt_center_half; shared widget code untouched. Canvas+animated-row replay reduced integer error30,967,678→14,544,079; animation-labelled failures43→17 and canvas100→23.

Finite original-QML observer confirms progress fill remains34.675555555555555 during expand t0..210 while track94→366; reaches135.01333333333332 at400. Runtime fill Behavior now restarts from last presented value, rather than extrapolating the interrupted job to the new frame before restarting. Only fill_w uses this rule. Observer also proves paused phase6.1600000000000037 atinitial capture,6.44 at+66. Rust replay erroneously advanced an extra initial16.667ms; parity/host/main.cpp frame(false) does not step its driver. Remove only that replay tick; no phase offsets or pixel-specific corrections. Private observer source/tools and property traces retained under island/tools/qt-observer and artifacts/observe-*.

Capture-codec evidence: installed Qt qrgb.h scalar qUnpremultiply does NOT match its PNG format-conversion SIMD path on half ties. Actual qdrawhelper_sse4.cpp reciprocal_mul_ps uses rcp estimate, one Newton-Raphson step, ×255, then nearest-even CVTPS2DQ. Add island test-transport Qt conversion, proven against an independent installed-QImage256x256 oracle covering every valid8-bit premultiplied (channel,alpha) pair. Capture reads originalRGBA8 premultiplied pixels, converts once for PNG; reference loading, gate/comparator and all runtime rendering unchanged. No image-specific table/correction used at runtime.

Island codec replay: canonical100/380 exactly equals integer-PM100/380; image pixel error unchanged1,892,848. All32,896 valid8-bit premultiplied channel/alpha pairs match independent Qt6.12 SIMD conversion. Next engine surgical fix: effects::mask_texture(Rect) currently invokes CPUqt_rect::rasterize even on native material roots. Route that opt-in mask target through existing native_gl::offscreen + canonical tessellate/native_gl::rect (same consolidated vertex-AA mode), retaining CPU fallback and restoring target/batch scope. Album-art residuals are at mask corners,1..2 units; bar has no MaskSourceRect usages and panels default material scope is false. Verify allthree after landing.

Scaled-native-text root cause confirmed from actual Qt textmask.vert and qsgdefaultglyphnode_p.cpp: font masks rasterize at render-context DPR, independent of item scale; modelView transforms glyph quads, then shader rounds transformed vertices to device pixels; sampler isNearest. Current engine folds item scale into FreeType DPR and re-rasterizes. Surgical plan: preserve existing unscaledpath, pass output DPR only from Renderer, raster masks at that DPR for positive uniform nativeGL item scaling, map/snap each glyph quad using original item matrix, draw through existing mask program/texture cache with per-quad extent. No new GL program/cache. Unsupported affine/Skia behavior stays unchanged. Validate independent scaled-text Qt goldens, engine tests, bar303/304 and panels controls.

Calendar-host trace: QML and Rust progress, animated width/height/radius match exactly at all10 captures. The emulated host alone still uses legacy foundations::hcenter(cw) for fractional animated widths, placing open-t16 background at217.68025 vsQt218, and does not opt that host rectangle into native materials. Change only tests/common::scene root material mode and calendar_container Qt6 center halves. Production bar host already has both. No timing adjustment or fixture rewrite.

Calendar host follow-up: the QML centerSection has clip:true, while its test-host rectangle omitted that clip. Add the same hard clip to that host rectangle; besides suppressing its outer AA fringe, this establishes the correct QSG batch coordinate basis. Island content fits within the current calendar bounds in these fixtures; no runtime hierarchy changed.

Morph-frame topology finding: DynamicIsland.qml islandFrame and musicExpandedFrame both have clip:true; the port only clipped the outer island root. Add those two hard-rectangle clips to the actual frame nodes. Settled integer extents usually hid this, while fractional animated extents exposed half-pixel AA fringe outside their own frame bounds. Clips also supply the correct local QSG batch roots; no alternate rectangle rasterizer or radius guess. Observer radius trace disproved the earlier hypothesis that mef_r froze during collapse; do not alter its animation without further evidence.

Radius trace follow-up: dual music collapse60 actual Qt radius42.976211424252249; current Rust mesh clamps an animated radius above44.14577 toheight/2. The height values match. Investigate the radius Behavior's retarget ordering separately from geometry/tessellation. Test last-presented retarget in an isolated replay; retain only if it matches independent Qt property traces, not just lower image error.

Negative radius experiment: replacing mef_r retarget with last-presented semantics freezes it at30 during continuous collapse and worsens dual-music error to1,347,450. Restored normal set immediately. Qt's actual42.976211424252249 radius atcollapse60 proves this dependency has different animation-job ordering from progress fill; retain the open diagnosis instead of generalizing fill's rule.

AlbumArtFrame topology: original DynamicIsland.qml:403 is clip:true, including the native background and masked image; Rust album_art currently returns an unclipped Group. Add the missing rectangular clip to the owned art frame. This is significant at fractional morph translations because QSG establishes its clip/batch coordinate root there; no mask-radius tuning.

Full original-QML per-tick observation resolves radius ordering: collapse0 radius30; first tick(+16ms) advances the old radius job once to42.976211424252249; subsequent height-binding retargets freeze that presented value throughcollapse60 (and until height settles). A blanket last-presented rule missed that first tick. Implement the dependency rule only for mef_r: while height had an earlier presented animation tick, restart radius from its last presented value; on the first height tick allow the pre-existing radius job to advance. Add exact numeric regression across the actual fixture cadence. Logs: observe-dual-music-expand/trace.log RADIUS_TICK.

## Exactness capture risk: Qt6.12 real-time hover throttle (16:28 UTC)

The remaining high native-glyph morph differences are state differences, not FreeType coverage. Original-QML observer atdual-expand105: play scale1; Rust scale1.0009209 (returning from transient hover). Item origins/layout font metrics match. Qt GL textmask batch vertices corroborate the reference glyph geometry. Installed qquickdeliveryagent_p_p.h has frameSynchronousHoverInterval=100, QElapsedTimer andQBasicTimer. Qt6.12 qquickdeliveryagent.cpp:2046-2074 throttles synthetic hover with real elapsed milliseconds; the parity host virtualizes animation/wall-clock but explicitly leaves monotonic real. Thus a fast capture can skip every transient button hover, while slower runs can catch one (consistent with frozen music-expanded-playing dark@2 havingprevhover scale1.06 while fresh Qt has1). Replaying real event time and virtual animation time independently cannot yield one deterministic state for these frozen inputs. Do not hardcode those reference states into runtime. QA should freeze/virtualize the hover timer, or explicitly capture matched interval0 behavior after a documented oracle recapture. Production still correctly re-delivers stationary hover each presented frame under its current contract; live100ms throttle is a separate compatibility decision.

Owned evidence: tools/qt-observer/main.cpp reads delivery-agent timer interval,lastMousePosition and actual control scales; artifacts/observe-dual-music-expand/trace.log HOVER_TICK confirms interval100 andpointer377,29, geometrydirty each virtualtick. Original sources untouched. Full upstream6.12 source retained underartifacts/qt-source/qquickdeliveryagent-6.12.cpp. No comparator/reference edits.

Hover diagnosis refinement: interval0 intervention still has play scale1. MouseArea itself DOES briefly contain the pointer atfirst frame and leaves atnextframe. The actual missing behavior is deferred QML animation-job startup: a scale Behavior created during polish has not started advancing before nexttick's leave retargets it to1. Rust starts that new scale job immediately, leaving a1.0009209 tail at105. The100ms real-time throttle remains an independent capture determinism risk, but is not sufficient to explain this particular glyph. Plan island-local deferred media-scale Behavior (no widgets edits): animations requested by frame hover start atnext frame turn; direct pointer events continue immediate startup. Preserve MouseArea hover color state immediately. Regression from observed actual control scale sequence, then all380/current no-loss test.

Frozen hover outlier causally proved: tools/prove_hover.py applies only a realQt pointer move ontoPrevious and200ms advance to an owned copy of music-expanded-playing fixture. OriginalQML untouched; resultingdark@2 pixels are BIT-IDENTICAL to frozenreference (max0,error0,pixels0). Fresh normalQt without intervention differs by119752, exactly our remaining large outlier. Thus no rasterization discrepancy remains in that settled scene; stalecontainsMouse state from real-time hover delivery is the reference issue. Keep it counted as a gate failure, not excluded or hardcoded. Evidence artifacts/hover-intervention/result.json plus observe-music-expanded-playing-hover-intervention.

15 max1 diagnosis: independent actual Qt mesh atagentappear60 width127.68376457725947 is bit-identical to Rust (all192vertices×8components), ruling out tessellation/source-size precision. Origin arithmetic is wrong-order float: Qt actualx254.72886297376093→f32 254.7288666; Rust `(320f+offf)-64f` gives254.7288513. Compute centre halves minus first, add original f64animated offset, then castonce, matching QMLqreal anchors/QSGfloat conversion. Also media scale transform currentlyrounds scale before calculating centretranslation; Qt computes QTransform inqreal then storesfloatmatrix. Build own media transform with f64centre*(1-scale) then cast components once. These are general precision fixes, no image/pixel-specific offsets. Re-runfullgate and retainedtests; defaultbar/panels ownscene construction unchanged.

Origin/scale precision raises364→368/380; remaining11max1 crops total49error units plus the proven stalehover119752. Continue the same qreal-binding rule for compactAgent/islandFrame x (`width/2-offset-61`) and sibling expandedMusic/Agent anchor-fill margins (add13/subtract26 in qreal BEFORE casting). Current geometry helpers cast animated width/x/height first, then perform those bindings inf32. This preserves QML's arithmetic staging without any engine/node API extension.

Minimality result: extra qreal staging in sibling-fill/compactAgent geometry helpers changed no PNGs (bindings replay identical toorigin-fixed,368pass/error119801). Reverted those additional helpers. Retain only the measured origin arithmetic fix and media-transform construction; no optional double-size node field or second rect mode added. Independent Qt mesh comparison atagentappear60 is exactly equal for all1536vertexcomponents, ruling out the proposed double-size API as necessary for that case.

## Final two-hour checkpoint — island owner, 2026-10-23

Strict GPU368/380 (initial0), no exclusions or relaxed comparator. Error31,556,797→119,801.
Fixed initial cohorts: fill140/140,Canvas98/102,text54/54,animation42/44,
rectAA29/35,image5/5. Elevenmax1 crops total49channel units; remaining119752 is
the independently reproduced stalePrevious-hover reference. Fresh ordinaryQt and
Rust are bit-identical in that settled scene. Gate remainsOPEN.

Final receipt `crates/hyprv-ui-island/artifacts/final/metrics.json`; source hashes
unchanged during replay. Full explanation, per-class table and remaining filenames
are in `crates/hyprv-ui-island/VALIDATION.md`. Public mount API remains unchanged;
README updated. Shared widgets untouched. Candidate mask shader and unhelpful
geometry/diagonal/projection experiments were removed.

93 ordinary tests and strict scopedall-targetClippy pass. Five nativeGL goldens
(mask/clip included),10 rendererGL regressions and the coordinatedcacheflush test
pass.6000frames/4mappedprivate surfaces/1974resizes/63recreates/2662partialdamage:
23994readbackchecks,zero mismatch. Existing generic GPU risk staysOPEN inSTATUS.

Frozen matched bar control303/304 is unchanged (exact191,MAE0.025054). Current
panels82→86/300 with no lostgatepasses; panel owner concurrently enabled native
rendering. Current newly recapturedbar, with privately regenerated device-size
trayassets, is288/304 (12max1–2 overflow crops,4submenu crops); controls/version
mismatches and routing are documented inhyprv-ui-bar.md. No sharedassetcache,
reference, fixture or livebaradapter was changed for these controls.

Disk policy: shared Cargo target only,CARGO_INCREMENTAL=0;184.5MB owned obsolete
captures/CMake/GLdumps deleted after receipts, final evidence about57MiB.

Final current-tree all-target rerun: 96 passed, 0 failed, 19 intentionally ignored;
strict Clippy passed. Receipts: artifacts/current-acceptance-tests.log and
current-acceptance-clippy.log under the island crate. Current source hashes still
match the final GPU replay exactly (changed=[]). The prior 93-test receipt is
superseded by this rerun, which includes concurrent engine test additions.

## Follow-up two-hour box — 17:20–19:20 UTC

User authorizes closing remaining12 island crops, then current bar288/304→304. Coordinating capture protocol/stale hover via docs/requests/parity-capture.md (spec-island). Preserve canonical references unless QA deliberately recaptures them. Re-read shared sources immediately before edits; log every change inhyprv-ui.md. First isolate residual rect float/batch arithmetic from actual Qt GL uploads; retest controls after retained changes.

### PQ-QA 13:30 EDT — calendar-morph @1 power glyph (parity QA)
`calendar-morph--{open-t0000,close-t0209,close-t0211,close-t0500}--{dark,light}@1` fail at max
204/144 on ~290 px, all inside the idle capsule's U+F011 power glyph (spec ui-island §3.9): Rust's
glyph is a thinner/differently hinted outline than the reference, while lock and `09:41` match.
The same glyph is exact in the island's own idle states, so the `calendar_morph` test is likely
using a different font/rasterization path (fallback face or Skia text instead of the native path)
for that glyph at @1. @2 variants differ only by rect AA (max 31).

Relative-transform tracking reduces tiny error43→21 units but exchanges two passes for two old max1 crops (370 exact unchanged); not yet accepted. Revisit qreal binding staging for compactAgent x and anchor-fill content helpers with this exact-relative basis. Earlier extra qreal staging had no PNG effect when absolute-subtraction recovery was still active; now the direct relative composition can expose it. Retain only changes supported by Qt uploads/fixtures.

Precise-size replay reaches378/380, no lost passes. Five tiny frame-edge crops close; one tiny alpha unit remains at dual-music-expand150 dark@1, plus stale-hover reference. Actual Qt's next-control vertices differ from Rust by one x float step, while previous control is bit-exact: QML has a Row translation before each button's local0/58/120; Rust flattens the Row into absolute per-button positions. Preserve that Row as one group in expanded_music (DynamicIsland.qml1128-1151), retaining same interaction rectangles and transforms. No per-image offsets.

Owned replay tools now record every reference PNG+manifest hash before/after replay, in addition to binary/source fingerprints. Bar current replay accepts --name to retain independent compact receipts without overwriting controls. No comparator or reference edits.

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

### PQ-QA 14:05 EDT — calendar-morph open-t0016/t0100 width mid-animation (parity QA, GL run 14:04)
GL matrix now: island 380/380, bar 292/292 rendered (0 fail), panels 255/300. Of the 45 panels
failures, 28 are `calendar-morph` (rendered by `hyprv-ui-island/tests/calendar_morph.rs`):
- `open-t0100--*@2` (max 250, ~11k px): the morphing panel is **~0.5 device px wider on each side**
  in Rust; height and vertical edges match exactly. At y=400 dark@2 the ref's left edge alpha
  runs (28,28,204) from x=610, Rust's (15,28,122) from x=609; the right edge mirrors it. A symmetric
  width error with exact height points to the width tween sample or its rounding at t=100 ms (QML
  rounds nothing here; check the easing input time and whether Rust snaps x/width separately).
  The range label 「2026/10/19 – 25」 is shifted by the same sub-pixel amount.
- `open-t0016` (max 156): the same width offset at the start of the morph, plus the @1 U+F011
  power glyph mismatch already filed at 13:30.
- `open-t0000`, `close-t0209/0211/0500` @1: the power glyph only. @2: rect AA, max 31.

### spec-bar → island owner: `tests/calendar_morph.rs` edits (2026-10-23, calendar share)

Surgical edits in your harness for the calendar fixtures it renders (please re-read before editing):
1. `hcenter` is now `hyprv_ui::geometry::qt_center_half` (QQuickAnchors alignWhenCentered), as
   in the bar view. This closes the PQ-QA 14:05 "~0.5 device px wider" morph frames: it was a
   0.25-logical x offset, not width.
2. The calendar is added before the island (DynamicIsland `z: 1` above the Loader).
3. The offline `last_error` override was removed (QML leaves it empty until a request).
4. GL: render 3072×1728 and crop. PNG readback uses the panels `qt_png` encoder (parity-host's Qt
   SSE4.1 unpremultiply).

All 52 `calendar-*` images now pass the strict GL gate (panels 298/300). `cargo test -p
hyprv-ui-island` passes.

### PQ-QA → island owner: laptop island delta (2026-10-24, laptop Phase A)

Spec: `docs/spec/laptop-island.md` (source `origin/laptop@24972f7`).
- **Pending decision D1:** should the pending approval/question card (Allow/Deny/Reply/options, §2) be implemented on the laptop profile under `QUIRK_LAPTOP_AGENT_INTERACTIVE`?
  - QA recommends **no**: the laptop island code predates main's read-only change.
  - Do not implement §2 until the orchestrator rules.
- **Needed regardless of D1:**
  - The island is not rendered and takes no input while the laptop bar is compact (§4). Do zero work while hidden; render the current state on the first revealed frame.
  - `QUIRK_LAPTOP_AGENT_TICK`: the ring is sampled per emission, with a 5 s fallback, only while visible (§5.1).
  - `QUIRK_LAPTOP_AGENT_LIST_RESET` (§5.2) unless D1 = (b).
- **References:** `parity/reference/laptop-island/` will be captured in Phase B. The fixture list is in `laptop-island.md` §9 and `laptop-qa-plan.md` §3.6.

**Update 2026-10-24 (PQ-QA): D1 resolved by the user — read-only island on laptop, same as main.** Do not implement the pending card, blocking hooks, laptop hook timeouts, the 5 s cadence quirk or the list-reset quirk. Still required: the island is hidden and does no work while the laptop bar is compact, and there is no tick while hidden. No `codex app-server` spawn. Spec updated: `docs/spec/laptop-island.md`.

## Laptop deploy/QA decision alignment (2026-10-24)

laptop-deploy.md now records user: 2880x1800 scale2, no eGPU, D1 read-only (matching
updated laptop-island.md). All readers must honor test-only HYPRV_SYSFS_ROOT and
all system-bus clients DBUS_SYSTEM_BUS_ADDRESS, including probe/reconnect/failure
paths without host fallback. Phase B implementation still required.
QA correction: laptop-qa-plan.md §3.5 says battery-present means no W, but laptop
shell.qml:4198–4204 independently shows W in the expanded bar; :4222 shows battery
beside it. Only compact (:3631) suppresses fallback W with battery present. User
now permits optional read-only CPU-only helper (no cooling/eGPU). Add fixtures for
expanded battery+W and battery without power source, compact battery/no watts lease,
and optional power capability absent/denied. Do not require combined desktop helper.
Source receipt: docs/verify/raw/laptop-deploy/cpu-w-source.json. No QA spec edited here.

### Shared runtime profile API LANDED (engine, laptop Phase B)

`hyprv_core::profile::{Profile, profile, sys_root, sys_path}` now exists.
`Profile::{Desktop,Laptop}`, `profile() -> Profile` is process-latched; exact
HYPRV_PROFILE override (invalid is fatal), otherwise exact portable DMI or present
System-scope Battery; ambiguous defaults Desktop. `sys_root() -> &Path` latches
HYPRV_SYSFS_ROOT or /; `sys_path(rel: impl AsRef<Path>) -> PathBuf` accepts root-relative
paths, strips leading /, rejects ..; no fallback. Code against this shared API now.
Shell startup/profile/service/UI wiring follows. Do not duplicate detection.

## Phase B packaging/QA receipt handoff — svc-sys (2026-10-24)

Engine core API has landed: sys_root default `/`, sys_path("sys/class/...") joins
below fixture root. Existing make-sysfs.py writes ROOT/class/...; please align the
fake layout/runner environment with the shared root-mirror contract (ROOT/sys/...)
instead of each service creating another interpretation. I consume core API as-is.
Laptop manifest selection and preflight router are implemented; no desktop visual
or CPU waivers are applied to laptop. Receipt contract documented in forthcoming
laptop-impl-svc-sys report: source/ELF-bound full laptop+desktop204 manifests, canonical
laptop reference slots plus production-GL actuals, >=60s quiet CPU/RSS/timer/GPU
report. Missing/PENDING artifacts block readiness; I will not stamp receipts.

Engine host now skips hidden island_inputs/progress_at resampling as requested.
State-store actors retain authoritative snapshots; compact, already-hidden Island
reuses its input clone. On expanded target (before reveal>0) it loads fresh inputs,
then set_bar_reveal drives first visible frame. Hidden media/presentation leases
are gated, and all actual strip clicks now route through bar.pointer_compact.
