# Full-screen parity acceptance — 2026-10-23

**64/64 images pass the unchanged, unmasked premultiplied-RGBA gate** (QA Run4:0/64).
26/26 jobs complete. Native comparison and QA's independent `compare.py` agree on every
image, including strict reference-clock and output identity checks. All references carry
`distance_field_text:true`; this is gate acceptance, not full-frame bit-exactness.
Worst channel delta is6/255, worst painted MAE0.0165/255, minimum painted IoU0.99987;
all footprints pass. Full-frame bit-exact count remains0/64.

Pinned combined release SHA256: `4f83022262c0c4e6d7654d41595575aa838e9ff7a1df3d75cd9480756ce28d81`.
[Aggregate receipt](artifacts/fullscreen-core/accepted-summary.json),
[independent measurements](artifacts/fullscreen-core/accepted/compare/compare.json),
[all32 frame/theme-pair rows](artifacts/fullscreen-core/accepted/frame-table.md).

The existing harness captures real retained EGL/GL RGBA8 surfaces and composes them in
layer/creation order. Original-shell references are compositor black/white/black matte
captures. Mode is DP-4,3072×1728 logical,6144×3456 physical,DPR2, both themes. The native
multi-output compositor itself is outside this single-output reference matrix; the new
unit regression proves composition excludes another output before DPR/file validation.
No live desktop, provider writes, references, fixture inputs, tolerances or masks changed.

## Full-screen table

| State | Passed | Light max / worst MAE | Dark max / worst MAE |
|---|---:|---:|---:|
| calendar | 2/2 | 4 / 0.0046 | 4 / 0.0040 |
| calendar-morph | 20/20 | 5 / 0.0071 | 4 / 0.0113 |
| control | 2/2 | 4 / 0.0048 | 4 / 0.0075 |
| control-open-close | 22/22 | 4 / 0.0068 | 4 / 0.0113 |
| idle | 2/2 | 4 / 0.0068 | 4 / 0.0113 |
| island-expanded | 2/2 | 4 / 0.0046 | 4 / 0.0088 |
| media-playing | 2/2 | 4 / 0.0079 | 4 / 0.0133 |
| stats | 2/2 | 4 / 0.0054 | 4 / 0.0091 |
| tray-menu | 2/2 | 4 / 0.0039 | 4 / 0.0090 |
| tray-overflow | 2/2 | 6 / 0.0158 | 6 / 0.0165 |
| tray-submenu | 2/2 | 4 / 0.0041 | 4 / 0.0086 |
| volume | 2/2 | 4 / 0.0051 | 4 / 0.0084 |
| wifi | 2/2 | 4 / 0.0020 | 4 / 0.0046 |

## Explicit four-corner coverage

All four corner regions are included without masks in all64 full-screen comparisons.
An additional256 crops (128×128 physical pixels, covering each corner surface) give
max4 for top-left, top-right, bottom-left and bottom-right across both themes and every
animation sample. None is bit-exact against the compositor matte captures; all are within
the unchanged full-screen gate. [Per-corner receipt](artifacts/fullscreen-core/accepted-corners.json).
This covers the accepted compact-surface topology visually, including all four corners.

## Closed causes

| Cause | Run4 | Accepted |
|---|---|---|
| R1 tray source/size | all64 fail, max239 | correct Qt request/selection, all64 pass |
| T2 overflow order |2 images,max255 |2/2 pass,max6 |
| B1 Wi-Fi retained hover |2 images,max56/46 |2/2 pass,max4 |
| A1 control opening mid-frames |4 images,max82/77 and112/105 |4/4 pass,max4 |
| P1 dark calendar time dot |4 images,max9 |4/4 pass,max4 |
| A2 calendar morph |passed with strip masked |20/20 pass,max5 |

- Tray: `assets.rs`, `assets/tray_loader.rs` resolve `QIcon::pixmap(round(18×DPR))`
  at ceil(DPR), then rasterize the chosen file at request×screen-scale. Chrome is64px,
  WeChat72px at DPR2. Supplied SNI pixmap selection, bounded workers, security limits,
  image identity and stale-reply rejection remain intact. The offline Qt6.12 probe is
  [qt_tray_probe.cpp](tools/qt_tray_probe.cpp), with [results](artifacts/fullscreen-core/qt-tray.log).
- `mapping/tray.rs` reproduces Quickshell ObjectModel insertion order before
  `shell.qml:503-521`'s stable priority sort. Upstream's category-OR-ID predicate is
  intentionally preserved. [Quickshell comparator](https://raw.githubusercontent.com/quickshell-mirror/quickshell/master/src/services/status_notifier/qml.cpp),
  [ObjectModel insertion](https://raw.githubusercontent.com/quickshell-mirror/quickshell/master/src/core/model.hpp).
- Replay re-hit-tests surface focus after map/unmap, sending leave/enter only if the
  selected surface changes. It preserves same-surface submenu hover and explicit grabs.
- Fixture-only animation scheduling retains microsecond wall epochs and integer Qt
  animation epochs. It reproduces pause-only→continuous driver restart, rather than
  flooring every wall timestamp. The actual Qt unified-timer trace proves the missing
  fractional epoch caused the1ms A1 error. [Offline plugin](tools/qt_clock_trace.cpp).
  Production animation callbacks are unchanged.
- `minute_wall` matches `shell.qml:3189-3195`'s SystemClock.Minutes. The reported event
  corner was the current-time dot overlapping the event: passing seconds displaced it
  fractionally. Both production mapping and fixture mapping now zero seconds/subseconds,
  as [Quickshell clock.cpp:42-51](https://raw.githubusercontent.com/quickshell-mirror/quickshell/master/src/core/clock.cpp) does.
- Sequence reference times are relative to each frames block (`run.py:571`); ordinary
  captures remain absolute (`:564`). Receipts retain both clocks; validation remains strict.
  Per-output composition now filters native output identity and rejects unknown placement.

## Remaining exactness contributors

An independent offline plugin read the original bar's RGBA8 before composition. Its PNG
recovers the exact raw GL readback hash; the instrumented original's final full screenshot
is bit-identical to the canonical idle reference. Against that source, Rust bar rectangle
edges, text and fixed shell icons are exact. Only two dynamic tray SVGs differ: Keepass max2
on216 pixels, Fcitx max4 on202 pixels. The original Qt source itself differs by up to3 from
its own matte reference outside tray/corners (889 pixels). This locates that residual in
composition/capture transport; it is not evidence for changing correctly matched primitives.
These are diagnostic regions, never masks in the64-image gate.
[Source ranking](artifacts/fullscreen-core/native-source-ranking.json),
[source-frame measurements](artifacts/fullscreen-core/native-source-candidates.json),
[readback plugin](tools/qt_surface_trace.cpp).

## Regression evidence

- Bar304/304 exact; island380/380 exact; panels298/300 pass,242 exact. The same two
  `qa-show-hide--show-t0120--{dark,light}@2.png` remain max1 and belong to spec-bar.
  [Fresh component receipt](artifacts/fullscreen-core/components/receipt.json) has zero
  source/reference drift and preserves the established304 exported bar crops.
- Host all-targets:57 passed,0 failed,7 intentionally ignored;
  scoped all-target Clippy with `--no-deps -- -D warnings` passes. The explicit private
  Wi-Fi focus/tray GPU regression also passes both themes.
- Current renderer stress:6000 frames,4 mapped private surfaces,1974 resizes,63 recreates,
  2662 partial-damage frames,776 readbacks,0 mismatches. A separate6000-frame unmapped run
  also passes. [Mapped stress](../hyprv-ui-bar/artifacts/fullscreen-accepted-stress-mapped/results.json).
  The earlier unreproduced GPU-corruption risk stays OPEN in docs/STATUS.md.
- Engine memory changes are integrated, including spec9.1's existing permission for
  compact corner surfaces with the same namespace/pixel placement. This report claims
  visual composition acceptance; it does not claim identical native layer-surface counts.
- [Rendering source guard](artifacts/fullscreen-core/accepted-source-guard.json) records
  no host/UI/rendering source or full-screen input drift during the accepted run. Other
  agents continued provider implementation changes; these fixtures use the mocked Hub.
  Build input hashes, Cargo invocation ordering and pinned ELF identify the measured variant.

Runtime remains Qt-free. All Qt code here is offline diagnostics. No installation or
live-shell restart was performed.
