# Full-screen parity: hyprv-shell vs the original shell

Run 2026-10-23 by spec-panels (QA). Commit `a51ae43` (plus uncommitted working tree),
release build of `hyprv-shell`.

```sh
hyprv-shell --nested-test OUT --fixtures parity/fullscreen/fixtures \
    --references parity/fullscreen/reference                  # both themes, live DPR 2
python3 -I parity/fullscreen/tools/compare.py OUT             # gate + triage evidence
python3 parity/fullscreen/tools/residual.py OUT               # re-score with cause T1 masked
python3 parity/fullscreen/tools/matrix.py OUT                 # the table below
```

The harness ran with `TMPDIR=/tmp`, `LIBSEAT_BACKEND=seatd`,
`AQ_DRM_DEVICES=<nonexistent>`, and without `HYPRLAND_INSTANCE_SIGNATURE`/`WAYLAND_DISPLAY`.
These are extra guards. Its isolation was reviewed before the run (`nested.rs`):
- private KWin, Hyprland, XDG paths and runtime dir;
- a private D-Bus that is also the system bus;
- read-only fixture mode: service actions are refused;
- the only contact with the live compositor is the read-only `j/monitors` query.

References are `parity/fullscreen/reference`: the original quickshell config, rendered
under a nested Hyprland and captured whole-output (see `parity/fullscreen/README.md`).
The gate is ARCHITECTURE's, unchanged, on premultiplied RGBA. All 64 references have
`distance_field_text: true`, because the bar always shows QtRendering text, so the gate
per image is:
- painted footprint within 1 logical px;
- painted-mask IoU ≥ 0.98;
- max channel diff ≤ 8;
- painted MAE ≤ 0.5.

`compare.py`'s gate reproduces the engine's own `comparison` numbers exactly (e.g. idle
light: max 212, MAE 2.3968, IoU 0.99988).

## Run 4: fresh release build, regression in the tray strip (2026-10-23, spec-panels)

Fresh `cargo build --release -p hyprv-shell` at a51ae43 plus the working tree, pinned to a
copy for the run. 26/26 jobs complete. **0/64 pass the unchanged gate** (Run 3: 52/64).
**With the tray strip masked, the results are identical to Run 3:** 52/64 pass, and the
same 12 non-tray failures remain.

**R1 regressed** (every image, max 239 in the tray strip). On idle, light:
- wechat: max 239, 1,119 px over 8 (Run 3: ≤ 3). Rust draws a **system** hicolor
  wechat, not Qt's choice, the user-level `~/.local/share/icons/hicolor/256x256`;
- chrome 28, keepassxc 23, fcitx 39: back at Run-2-level errors;
- discord 2 and the placeholder 3: still fine.

Diagnosis so far:
- the nested child's environment is correct (`HYPRV_ICON_SEARCH_PATHS` includes the
  user root; captured from `/proc`);
- `icon_theme::lookup("wechat", 36, 2)` returns the user-level file (new `#[ignore]`
  `lookup_dump` test);
- `assets::fixture_tray(wechat, 2.0)` in-process returns the correct 72-px pixmap,
  mean diff 0.8 vs 74.8 for the system icon (new `#[ignore]` `dump_fixture_tray` test).

So the images the nested bar draws no longer come from `fixture_tray` at DPR 2. The
regression appeared with the changes made after Run 3, between 14:37 and 14:49, in
`assets/tray_loader.rs`, `assets.rs`, `fixture.rs`, `mapping/tray.rs`, `assembly.rs`
(`tray_assets`/`TrayLoader::get`) and `replay.rs`.

| state | frame | light max / MAE / IoU | dark max / MAE / IoU | tray strip masked (light, dark) | causes |
|---|---|---|---|---|---|
| calendar-morph | open-t0000 | FAIL 239 / 0.370 / 0.9999 | FAIL 239 / 0.374 / 0.9999 | **pass** 4/0.005, **pass** 4/0.010 | R1 |
| calendar-morph | open-t0016 | FAIL 239 / 0.248 / 0.9999 | FAIL 239 / 0.250 / 0.9999 | **pass** 4/0.003, **pass** 4/0.006 | A2, R1 |
| calendar-morph | open-t0100 | FAIL 239 / 0.078 / 1.0000 | FAIL 239 / 0.076 / 1.0000 | **pass** 6/0.007, **pass** 8/0.005 | A2, R1 |
| calendar-morph | open-t0210 | FAIL 239 / 0.061 / 1.0000 | FAIL 239 / 0.060 / 1.0000 | **pass** 6/0.004, fail 9/0.004 | R1, S1 |
| calendar-morph | open-t0500 | FAIL 239 / 0.061 / 1.0000 | FAIL 239 / 0.060 / 1.0000 | **pass** 6/0.004, fail 9/0.004 | R1, S1 |
| calendar-morph | close-t0000 | FAIL 239 / 0.061 / 1.0000 | FAIL 239 / 0.060 / 1.0000 | **pass** 6/0.004, fail 9/0.004 | R1, S1 |
| calendar-morph | close-t0100 | FAIL 239 / 0.060 / 1.0000 | FAIL 239 / 0.060 / 1.0000 | **pass** 4/0.004, **pass** 4/0.003 | A2, R1 |
| calendar-morph | close-t0209 | FAIL 239 / 0.370 / 0.9999 | FAIL 239 / 0.374 / 0.9999 | **pass** 4/0.005, **pass** 4/0.010 | R1 |
| calendar-morph | close-t0211 | FAIL 239 / 0.370 / 0.9999 | FAIL 239 / 0.374 / 0.9999 | **pass** 4/0.005, **pass** 4/0.010 | R1 |
| calendar-morph | close-t0500 | FAIL 239 / 0.370 / 0.9999 | FAIL 239 / 0.374 / 0.9999 | **pass** 4/0.005, **pass** 4/0.010 | R1 |
| calendar | — | FAIL 239 / 0.061 / 1.0000 | FAIL 239 / 0.060 / 1.0000 | **pass** 6/0.004, fail 9/0.004 | R1, S1 |
| control-open-close | open-t0000 | FAIL 239 / 0.370 / 0.9999 | FAIL 239 / 0.374 / 0.9999 | **pass** 4/0.005, **pass** 4/0.010 | R1 |
| control-open-close | open-t0016 | FAIL 239 / 0.370 / 0.9999 | FAIL 239 / 0.374 / 0.9999 | **pass** 4/0.005, **pass** 4/0.010 | R1 |
| control-open-close | open-t0060 | FAIL 239 / 0.370 / 0.9999 | FAIL 239 / 0.374 / 0.9999 | **pass** 4/0.005, **pass** 4/0.010 | A1, R1 |
| control-open-close | open-t0120 | FAIL 239 / 0.416 / 0.9957 | FAIL 239 / 0.519 / 0.9957 | fail 82/0.286, fail 77/0.393 | A1, R1 |
| control-open-close | open-t0220 | FAIL 239 / 0.175 / 0.9992 | FAIL 239 / 0.174 / 0.9992 | fail 112/0.055, fail 105/0.055 | A1, R1 |
| control-open-close | open-t0400 | FAIL 239 / 0.124 / 1.0000 | FAIL 239 / 0.126 / 1.0000 | **pass** 4/0.003, **pass** 4/0.006 | R1 |
| control-open-close | close-t0000 | FAIL 239 / 0.124 / 1.0000 | FAIL 239 / 0.126 / 1.0000 | **pass** 4/0.003, **pass** 4/0.006 | R1 |
| control-open-close | close-t0060 | FAIL 239 / 0.123 / 1.0000 | FAIL 239 / 0.125 / 1.0000 | **pass** 4/0.002, **pass** 4/0.004 | A1, R1 |
| control-open-close | close-t0150 | FAIL 239 / 0.370 / 0.9999 | FAIL 239 / 0.374 / 0.9999 | **pass** 4/0.005, **pass** 4/0.010 | R1 |
| control-open-close | close-t0210 | FAIL 239 / 0.370 / 0.9999 | FAIL 239 / 0.374 / 0.9999 | **pass** 4/0.005, **pass** 4/0.010 | R1 |
| control-open-close | close-t0400 | FAIL 239 / 0.370 / 0.9999 | FAIL 239 / 0.374 / 0.9999 | **pass** 4/0.005, **pass** 4/0.010 | R1 |
| control | — | FAIL 239 / 0.125 / 1.0000 | FAIL 239 / 0.128 / 1.0000 | **pass** 4/0.004, **pass** 4/0.007 | R1 |
| idle | — | FAIL 239 / 0.370 / 0.9999 | FAIL 239 / 0.374 / 0.9999 | **pass** 4/0.005, **pass** 4/0.010 | R1 |
| island-expanded | — | FAIL 239 / 0.158 / 0.9999 | FAIL 239 / 0.162 / 0.9999 | **pass** 4/0.004, **pass** 4/0.008 | R1 |
| media-playing | — | FAIL 239 / 0.372 / 0.9999 | FAIL 239 / 0.377 / 0.9999 | **pass** 4/0.006, **pass** 4/0.012 | R1 |
| stats | — | FAIL 239 / 0.052 / 1.0000 | FAIL 239 / 0.056 / 1.0000 | **pass** 4/0.005, **pass** 4/0.009 | R1 |
| tray-menu | — | FAIL 239 / 0.151 / 1.0000 | FAIL 239 / 0.155 / 1.0000 | **pass** 4/0.003, **pass** 4/0.009 | R1 |
| tray-overflow | — | FAIL 255 / 6.165 / 1.0000 | FAIL 255 / 6.019 / 1.0000 | fail 255/6.165, fail 255/6.019 | T2 |
| tray-submenu | — | FAIL 239 / 0.185 / 0.9999 | FAIL 239 / 0.190 / 0.9999 | **pass** 4/0.003, **pass** 4/0.008 | R1 |
| volume | — | FAIL 239 / 0.272 / 0.9999 | FAIL 239 / 0.275 / 0.9999 | **pass** 4/0.004, **pass** 4/0.007 | R1 |
| wifi | — | FAIL 239 / 0.120 / 0.9999 | FAIL 239 / 0.171 / 0.9999 | fail 56/0.033, fail 46/0.071 | B1, R1 |


Remaining causes (handed to svc-power, `docs/requests/hyprv-shell.md`):

| cause | images | notes |
|---|---|---|
| R1 regression, tray image source in the nested run | 64 (alone it fails all) | see above; masked, these images pass |
| A1 control-panel open-t0120 / open-t0220 | 4 | slider row 2 logical px high |
| T2 tray overflow order | 2 | max 255 |
| B1 Wi-Fi trigger pill while popup open | 2 | max 56/46 |
| P1 dark calendar event-block corner | 4 | 7 px, max 9 |

Evidence: `docs/verify/raw/parity-fullscreen/run4/`.

## Run 3: R1 chased to the gate (2026-10-23, spec-panels)

**52 / 64 images pass the unchanged ARCHITECTURE gate** (run 2: 0). All 26 jobs complete.
The release binary was pinned for the run, because a concurrent rebuild of the shared
`target/release` had broken an earlier attempt.

**R1 root cause: two Rust-side bugs plus two QtSvg quirks.** The reference capture was
right. Established against real Qt 6.12 (a QIcon probe on a private headless weston,
plus parity-host probe scenes) and qtbase 6.12 `qiconloader.cpp`:
1. **Request size.** Quickshell's 18-px tray `IconImage` asks for 36 px and calls
   `QIcon::pixmap(36, 36)` at qApp DPR 2, so pixmaps are up to 72 px (chrome 64), and
   the tray's `mipmap: true` minifies them. Run 2 assumed 36 @ 1. parity-host's probe
   drawing the raw 32 px texture matched Rust (max 4) but not the live shell (max 30);
   the 64 px texture with mipmaps matched to max 6.
2. **Directory choice.** `hyprv-widgets/src/icon_theme.rs` now ports Qt 6.12 verbatim:
   - directories in sorted `QSettings::allKeys()` order, or GTK-cache order;
   - PNG entries inserted at the front;
   - the order-dependent `entryForSize`/`directorySizeDelta`;
   - the dash fallback skips Applications/MimeTypes;
   - parents plus the fallback theme.

   A test asserts all 108 choices real Qt made on a synthetic colour-coded theme.
   Run 2's "first content dir wins" rule was an artifact of the wrong size and is gone.
3. **SVG icons.** New `hyprv-widgets/src/qt_svg.rs` rasterizes them like QtSvg:
   - Qt-raster coverage through `qt_painter`, Qt gradient tables, focal radial
     gradients, spread modes, BYTE_MUL source-over;
   - multiplicative opacity;
   - `QCosmeticStroker` subdivision for pens ≤ 1 device px.

   Measured QtSvg quirks:
   - CSS `rgba()`/`hsl()` become opaque black;
   - `clip-path` is ignored.

   At 72 px vs Qt's own pixmaps: keepassxc max 2, fcitx max 13.

Tray strip now: every icon ≤ 4 in all images.

| state | frame | light max / MAE / IoU | dark max / MAE / IoU | tray strip masked (light, dark) | causes |
|---|---|---|---|---|---|
| calendar-morph | open-t0000 | PASS 4 / 0.007 / 0.9999 | PASS 4 / 0.011 / 0.9999 | **pass** 4/0.005, **pass** 4/0.010 |  |
| calendar-morph | open-t0016 | PASS 4 / 0.005 / 0.9999 | PASS 4 / 0.007 / 0.9999 | **pass** 4/0.003, **pass** 4/0.006 |  |
| calendar-morph | open-t0100 | PASS 6 / 0.008 / 1.0000 | PASS 8 / 0.006 / 1.0000 | **pass** 6/0.007, **pass** 8/0.006 |  |
| calendar-morph | open-t0210 | PASS 6 / 0.005 / 1.0000 | FAIL 9 / 0.004 / 1.0000 | **pass** 6/0.004, fail 9/0.004 | R1, S1 |
| calendar-morph | open-t0500 | PASS 6 / 0.005 / 1.0000 | FAIL 9 / 0.004 / 1.0000 | **pass** 6/0.004, fail 9/0.004 | R1, S1 |
| calendar-morph | close-t0000 | PASS 6 / 0.005 / 1.0000 | FAIL 9 / 0.004 / 1.0000 | **pass** 6/0.004, fail 9/0.004 | R1, S1 |
| calendar-morph | close-t0100 | PASS 4 / 0.004 / 1.0000 | PASS 4 / 0.004 / 1.0000 | **pass** 4/0.004, **pass** 4/0.003 |  |
| calendar-morph | close-t0209 | PASS 4 / 0.007 / 0.9999 | PASS 4 / 0.011 / 0.9999 | **pass** 4/0.005, **pass** 4/0.010 |  |
| calendar-morph | close-t0211 | PASS 4 / 0.007 / 0.9999 | PASS 4 / 0.011 / 0.9999 | **pass** 4/0.005, **pass** 4/0.010 |  |
| calendar-morph | close-t0500 | PASS 4 / 0.007 / 0.9999 | PASS 4 / 0.011 / 0.9999 | **pass** 4/0.005, **pass** 4/0.010 |  |
| calendar | — | PASS 6 / 0.005 / 1.0000 | FAIL 9 / 0.004 / 1.0000 | **pass** 6/0.004, fail 9/0.004 | R1, S1 |
| control-open-close | open-t0000 | PASS 4 / 0.007 / 0.9999 | PASS 4 / 0.011 / 0.9999 | **pass** 4/0.005, **pass** 4/0.010 |  |
| control-open-close | open-t0016 | PASS 4 / 0.007 / 0.9999 | PASS 4 / 0.011 / 0.9999 | **pass** 4/0.005, **pass** 4/0.010 |  |
| control-open-close | open-t0060 | PASS 4 / 0.007 / 0.9999 | PASS 4 / 0.011 / 0.9999 | **pass** 4/0.005, **pass** 4/0.010 |  |
| control-open-close | open-t0120 | FAIL 82 / 0.278 / 0.9957 | FAIL 77 / 0.381 / 0.9957 | fail 82/0.287, fail 77/0.394 | A1, R1 |
| control-open-close | open-t0220 | FAIL 112 / 0.055 / 0.9992 | FAIL 105 / 0.054 / 0.9992 | fail 112/0.056, fail 105/0.055 | A1, R1 |
| control-open-close | open-t0400 | PASS 4 / 0.004 / 1.0000 | PASS 4 / 0.007 / 1.0000 | **pass** 4/0.003, **pass** 4/0.006 |  |
| control-open-close | close-t0000 | PASS 4 / 0.004 / 1.0000 | PASS 4 / 0.007 / 1.0000 | **pass** 4/0.003, **pass** 4/0.006 |  |
| control-open-close | close-t0060 | PASS 4 / 0.005 / 1.0000 | PASS 4 / 0.009 / 1.0000 | **pass** 4/0.004, **pass** 4/0.008 |  |
| control-open-close | close-t0150 | PASS 4 / 0.007 / 0.9999 | PASS 4 / 0.011 / 0.9999 | **pass** 4/0.005, **pass** 4/0.010 |  |
| control-open-close | close-t0210 | PASS 4 / 0.007 / 0.9999 | PASS 4 / 0.011 / 0.9999 | **pass** 4/0.005, **pass** 4/0.010 |  |
| control-open-close | close-t0400 | PASS 4 / 0.007 / 0.9999 | PASS 4 / 0.011 / 0.9999 | **pass** 4/0.005, **pass** 4/0.010 |  |
| control | — | PASS 4 / 0.005 / 1.0000 | PASS 4 / 0.008 / 1.0000 | **pass** 4/0.005, **pass** 4/0.008 |  |
| idle | — | PASS 4 / 0.007 / 0.9999 | PASS 4 / 0.011 / 0.9999 | **pass** 4/0.005, **pass** 4/0.010 |  |
| island-expanded | — | PASS 4 / 0.005 / 0.9999 | PASS 4 / 0.009 / 0.9999 | **pass** 4/0.004, **pass** 4/0.008 |  |
| media-playing | — | PASS 4 / 0.008 / 0.9999 | PASS 4 / 0.013 / 0.9999 | **pass** 4/0.006, **pass** 4/0.012 |  |
| stats | — | PASS 4 / 0.005 / 1.0000 | PASS 4 / 0.009 / 1.0000 | **pass** 4/0.005, **pass** 4/0.009 |  |
| tray-menu | — | PASS 4 / 0.004 / 1.0000 | PASS 4 / 0.009 / 1.0000 | **pass** 4/0.003, **pass** 4/0.009 |  |
| tray-overflow | — | FAIL 255 / 3.960 / 1.0000 | FAIL 255 / 3.753 / 1.0000 | fail 255/3.960, fail 255/3.753 | T2 |
| tray-submenu | — | PASS 4 / 0.004 / 0.9999 | PASS 4 / 0.009 / 0.9999 | **pass** 4/0.003, **pass** 4/0.008 |  |
| volume | — | PASS 4 / 0.005 / 0.9999 | PASS 4 / 0.008 / 0.9999 | **pass** 4/0.004, **pass** 4/0.007 |  |
| wifi | — | FAIL 56 / 0.043 / 0.9999 | FAIL 46 / 0.095 / 0.9999 | fail 56/0.033, fail 46/0.073 | B1, R1 |


**Remaining 12 failures:**

| cause | images | evidence | owner |
|---|---|---|---|
| A1 control-panel open-t0120 / open-t0220 | 4 | slider row at logical y 392 / 476 sits 2 logical px high (a 4 px shift removes 96.7 %); open-t0060 and close-t0060 now pass | hyprv-ui-panels |
| T2 tray overflow grid order | 2 | Rust alternates chrome/placeholder; the original takes `sortedTrayItems.slice(visibleTrayCount)`, max 255 | hyprv-ui-bar |
| B1 Wi-Fi trigger hover pill while its popup is open | 2 | 4.1k px at logical (2828, 8), max 56/46 | hyprv-ui-bar |
| P1 dark calendar event-block corner | 4 (calendar, morph open-t0210/t0500, close-t0000, dark) | 7 px at logical (1593–1595, 267–274), max 9 (limit 8) | hyprv-ui-panels |

A2 (morph mid-frames) and I3 (expanded-island pause glyph) now pass, fixed by their
owners' concurrent work; S1 is gone. Evidence: `docs/verify/raw/parity-fullscreen/run3/`.

## Run 2: after the T1 / I1 / I2 / H1 fixes (2026-10-23, spec-panels)

Same command and references, release build with the fixes below plus the engine's
concurrent working-tree changes. **26 / 26 jobs run to completion** (H1 fixed) and every
image is captured.

**Under the unchanged gate, 0 / 64 images pass.** The tray strip still fails every image
(R1, max 44); **with it masked, 38 / 64 pass**. Settled images are at painted MAE
0.008–0.05 (run 1: 0.4–6.7).

Fixes (tests in brackets):
- **T1.1 fixture loader** (`hyprv-shell/src/fixture.rs`): tray items now carry
  `icon_name` (`@asset:` → absolute path) [`tray_items_carry_icon_name_for_theme_lookup`].
- **T1.2 QIconLoader port** (`hyprv-widgets/src/icon_theme.rs`, which `resolve_icon`
  now delegates to). Theme `hicolor`: Qt's generic theme fallback, because qt6ct
  fails. It ports the XDG search paths, the `Inherits` chain, the dash and `pixmaps`
  fallbacks, `entryForSize` (exact, then closest; ties go to the larger dir; PNG
  first), Qt's GTK `icon-theme.cache` reader with its validity rules, and Qt's
  `qSmoothScaleImage` downscale; pixmaps are never upscaled. Measured on the live
  shell:
  - discord: hicolor 256 → 36, max diff 4;
  - chrome: the hicolor 32x32 file, stretched by the scene graph;
  - wechat: **the first content dir containing the icon wins**, i.e.
    `~/.local/share/icons/hicolor/256x256` beats the system 32x32.

  (11 unit tests, incl. goldens and system-cache checks.)
- **T1.2b SVG icons** (`hyprv-shell/src/assets.rs`): sized like `QSvgIconEngine`, i.e.
  the document's default size fitted to the request. Skia drew absolutely sized
  documents unscaled, so fcitx came out blank. [`raster_icons_are_downscaled_qt_style_but_never_upscaled`]
- **T1.3 missing icon**: Quickshell's 36 px black/#d900d8 checker, measured on the live
  shell. The letter is kept only for an empty name.
  [`unresolvable_icon_names_draw_quickshells_missing_icon_not_a_letter`]
- **T1 harness**: the nested child gets the live icon roots
  (`HYPRV_ICON_SEARCH_PATHS`/`_FALLBACK_PATHS`), as the reference shell had.
- **I1** (`fixture.rs`): island inputs carry `art_url`; the island only draws art for a
  non-empty URL. [`island_gets_art_url_and_the_streamed_spectrum`]
- **I2** (`fixture/translate.rs`): the `audio-spectrum.sh` stream is replayed with
  shell.qml's parser and run condition. [same, plus `spectrum_needs_cava_like_audio_spectrum_process_running`]
- **H1** (`replay.rs`):
  - a 10 s *stall* bound that names the pending op and the unconfigured surfaces;
  - a total budget of 25 s + 40 ms/op;
  - `HYPRV_REPLAY_TRACE=1` op trace;
  - fixture asset images decoded once.

  The 300–900 ms/frame tray-menu slowdown seen in run 1 was gone after the engine's
  concurrent `hyprv-ui` change; tray-menu now replays in 0.64 s. I couldn't profile
  it (ptrace/perf are blocked), so the cause is not attributed.

| state | frame | light max / MAE / IoU | dark max / MAE / IoU | tray strip masked (light, dark) | causes |
|---|---|---|---|---|---|
| calendar-morph | open-t0000 | FAIL 44 / 0.042 / 0.9999 | FAIL 44 / 0.037 / 0.9999 | **pass** 4/0.005, **pass** 4/0.011 | R1 |
| calendar-morph | open-t0016 | FAIL 44 / 0.093 / 0.9999 | FAIL 44 / 0.090 / 0.9999 | fail 26/0.073, fail 27/0.077 | A2, R1 |
| calendar-morph | open-t0100 | FAIL 251 / 1.233 / 0.9944 | FAIL 251 / 0.533 / 0.9944 | fail 251/1.248, fail 251/0.537 | A2, R1 |
| calendar-morph | open-t0210 | FAIL 44 / 0.010 / 1.0000 | FAIL 44 / 0.008 / 1.0000 | **pass** 6/0.004, fail 9/0.004 | R1, S1 |
| calendar-morph | open-t0500 | FAIL 44 / 0.010 / 1.0000 | FAIL 44 / 0.008 / 1.0000 | **pass** 6/0.004, fail 9/0.004 | R1, S1 |
| calendar-morph | close-t0000 | FAIL 44 / 0.010 / 1.0000 | FAIL 44 / 0.008 / 1.0000 | **pass** 6/0.004, fail 9/0.004 | R1, S1 |
| calendar-morph | close-t0100 | FAIL 44 / 0.021 / 1.0000 | FAIL 44 / 0.019 / 1.0000 | fail 20/0.015, fail 20/0.015 | A2, R1 |
| calendar-morph | close-t0209 | FAIL 44 / 0.042 / 0.9999 | FAIL 44 / 0.037 / 0.9999 | **pass** 4/0.005, **pass** 4/0.011 | R1 |
| calendar-morph | close-t0211 | FAIL 44 / 0.042 / 0.9999 | FAIL 44 / 0.037 / 0.9999 | **pass** 4/0.005, **pass** 4/0.011 | R1 |
| calendar-morph | close-t0500 | FAIL 44 / 0.042 / 0.9999 | FAIL 44 / 0.037 / 0.9999 | **pass** 4/0.005, **pass** 4/0.011 | R1 |
| calendar | — | FAIL 44 / 0.010 / 1.0000 | FAIL 44 / 0.008 / 1.0000 | **pass** 6/0.004, fail 9/0.004 | R1, S1 |
| control-open-close | open-t0000 | FAIL 44 / 0.042 / 0.9999 | FAIL 44 / 0.037 / 0.9999 | **pass** 4/0.005, **pass** 4/0.011 | R1 |
| control-open-close | open-t0016 | FAIL 44 / 0.042 / 0.9999 | FAIL 44 / 0.037 / 0.9999 | **pass** 4/0.005, **pass** 4/0.011 | R1 |
| control-open-close | open-t0060 | FAIL 44 / 1.774 / 0.5817 | FAIL 44 / 1.147 / 0.5816 | fail 22/1.849, fail 20/1.194 | A1, R1 |
| control-open-close | open-t0120 | FAIL 82 / 0.292 / 0.9957 | FAIL 77 / 0.391 / 0.9957 | fail 82/0.287, fail 77/0.395 | A1, R1 |
| control-open-close | open-t0220 | FAIL 112 / 0.066 / 0.9992 | FAIL 105 / 0.063 / 0.9992 | fail 112/0.056, fail 105/0.056 | A1, R1 |
| control-open-close | open-t0400 | FAIL 44 / 0.016 / 1.0000 | FAIL 44 / 0.015 / 1.0000 | **pass** 4/0.003, **pass** 4/0.007 | R1 |
| control-open-close | close-t0000 | FAIL 44 / 0.016 / 1.0000 | FAIL 44 / 0.015 / 1.0000 | **pass** 4/0.003, **pass** 4/0.007 | R1 |
| control-open-close | close-t0060 | FAIL 82 / 6.162 / 0.9976 | FAIL 75 / 3.374 / 0.9975 | fail 82/6.339, fail 75/3.469 | A1, R1 |
| control-open-close | close-t0150 | FAIL 44 / 0.042 / 0.9999 | FAIL 44 / 0.037 / 0.9999 | **pass** 4/0.005, **pass** 4/0.011 | R1 |
| control-open-close | close-t0210 | FAIL 44 / 0.042 / 0.9999 | FAIL 44 / 0.037 / 0.9999 | **pass** 4/0.005, **pass** 4/0.011 | R1 |
| control-open-close | close-t0400 | FAIL 44 / 0.042 / 0.9999 | FAIL 44 / 0.037 / 0.9999 | **pass** 4/0.005, **pass** 4/0.011 | R1 |
| control | — | FAIL 44 / 0.017 / 1.0000 | FAIL 44 / 0.017 / 1.0000 | **pass** 5/0.005, fail 11/0.008 | R1, S1 |
| idle | — | FAIL 44 / 0.042 / 0.9999 | FAIL 44 / 0.037 / 0.9999 | **pass** 4/0.005, **pass** 4/0.011 | R1 |
| island-expanded | — | FAIL 154 / 0.039 / 0.9999 | FAIL 140 / 0.041 / 0.9999 | fail 154/0.024, fail 140/0.031 | I3, R1 |
| media-playing | — | FAIL 44 / 0.044 / 0.9999 | FAIL 44 / 0.040 / 0.9999 | **pass** 5/0.008, fail 11/0.015 | R1, S1 |
| stats | — | FAIL 44 / 0.010 / 1.0000 | FAIL 44 / 0.013 / 1.0000 | **pass** 4/0.005, **pass** 4/0.009 | R1 |
| tray-menu | — | FAIL 44 / 0.018 / 1.0000 | FAIL 44 / 0.019 / 1.0000 | **pass** 4/0.003, **pass** 4/0.009 | R1 |
| tray-overflow | — | FAIL 255 / 4.134 / 1.0000 | FAIL 255 / 3.876 / 1.0000 | fail 255/4.134, fail 255/3.876 | T2 |
| tray-submenu | — | FAIL 44 / 0.022 / 0.9999 | FAIL 44 / 0.021 / 0.9999 | **pass** 4/0.003, **pass** 4/0.008 | R1 |
| volume | — | FAIL 44 / 0.031 / 0.9999 | FAIL 44 / 0.027 / 0.9999 | **pass** 4/0.004, **pass** 4/0.008 | R1 |
| wifi | — | FAIL 56 / 0.050 / 0.9999 | FAIL 46 / 0.101 / 0.9999 | fail 56/0.033, fail 46/0.074 | B1, R1 |


Remaining causes:
- **R1, tray strip draw stage** (hyprv-widgets / hyprv-ui, engine). Discord, wechat,
  the placeholder, the control icon and the bell are within ≤ 3. The rest:
  - chrome (32 px PNG stretched to 36): max 28 over 61 px. A CPU simulation of GL
    bilinear sampling gives the same residual, so Qt's texture path differs in some
    detail;
  - keepassxc and fcitx (SVG): max 40 and 44, QtSvg vs Skia rasterization.

  `zoom/R1-tray-icons.png`.
- **T2, tray overflow grid order** (hyprv-ui-bar). The original groups the overflow
  items as `sortedTrayItems.slice(visibleTrayCount)` (shell.qml:4212), giving a block
  of chrome and then the telegram placeholders. Rust alternates them, so its overflow
  list doesn't use the bar's sorted order. 1,296 px (one whole 36×36 cell) per
  misplaced icon. `after-t1/zoom/T2-overflow-order.png`.
- **I3, expanded-island pause glyph** (hyprv-ui-island): 200 px at logical (1528, 144);
  the bars sit about 1–2 px lower and differ in size. `after-t1/zoom/I3-pause-glyph.png`.
- **B1, Wi-Fi trigger highlight** (hyprv-ui-bar): unchanged.
- **A1 / A2, popup and morph mid-frames** (panels, island): unchanged. Settled frames
  pass with the strip masked.
- **S1, dark theme** (hyprv-widgets): ≤ 11/255 on ≤ 7 px (control-panel/bell
  icon edges), on 6 dark images.

Evidence for run 2: `docs/verify/raw/parity-fullscreen/after-t1/`.

---

# Run 1 (before fixes)

## Result: 0 / 66 images pass; 20 / 26 jobs run to completion

**One cause (T1, tray icons) fails every image on its own.** The column "without T1"
re-scores each image with the tray-icon strip (logical x 2850–3012, y 16–42) masked out
in both images. tray-overflow is not masked, because its whole tray is T1. With T1
gone:
- 37 of the 60 captured images fail only on S1: 3–23 px at ≤ 12/255;
- with T1 and S1 fixed, those 37 pass;
- the rest carry the real component defects below.

| state | frame | light: max / MAE / IoU | dark: max / MAE / IoU | without T1 (light, dark) | causes |
|---|---|---|---|---|---|
| calendar-morph | open-t0000 | FAIL 212 / 2.40 / 0.9999 | FAIL 244 / 2.78 / 0.9999 | 9/0.007 (3 px >8), 12/0.013 (16 px >8) | T1, S1 |
| calendar-morph | open-t0016 | FAIL 212 / 1.67 / 0.9999 | FAIL 244 / 1.93 / 0.9999 | 26/0.074 (1028 px >8), 27/0.078 (1042 px >8) | T1, A2 |
| calendar-morph | open-t0100 | FAIL 251 / 1.69 / 0.9944 | FAIL 251 / 1.06 / 0.9944 | 251/1.248 (13241 px >8), 251/0.538 (13306 px >8) | T1, A2 |
| calendar-morph | open-t0210 | FAIL 212 / 0.38 / 1.0000 | FAIL 244 / 0.43 / 1.0000 | 9/0.005 (3 px >8), 12/0.004 (23 px >8) | T1, S1 |
| calendar-morph | open-t0500 | FAIL 212 / 0.38 / 1.0000 | FAIL 244 / 0.43 / 1.0000 | 9/0.005 (3 px >8), 12/0.004 (23 px >8) | T1, S1 |
| calendar-morph | close-t0000 | FAIL 212 / 0.38 / 1.0000 | FAIL 244 / 0.43 / 1.0000 | 9/0.005 (3 px >8), 12/0.004 (23 px >8) | T1, S1 |
| calendar-morph | close-t0100 | FAIL 212 / 0.39 / 1.0000 | FAIL 244 / 0.44 / 1.0000 | 20/0.016 (959 px >8), 20/0.016 (979 px >8) | T1, A2 |
| calendar-morph | close-t0209 | FAIL 212 / 2.40 / 0.9999 | FAIL 244 / 2.78 / 0.9999 | 9/0.007 (3 px >8), 12/0.013 (16 px >8) | T1, S1 |
| calendar-morph | close-t0211 | FAIL 212 / 2.40 / 0.9999 | FAIL 244 / 2.78 / 0.9999 | 9/0.007 (3 px >8), 12/0.013 (16 px >8) | T1, S1 |
| calendar-morph | close-t0500 | FAIL 212 / 2.40 / 0.9999 | FAIL 244 / 2.78 / 0.9999 | 9/0.007 (3 px >8), 12/0.013 (16 px >8) | T1, S1 |
| calendar | — | FAIL 212 / 0.38 / 1.0000 | FAIL 244 / 0.43 / 1.0000 | 9/0.005 (3 px >8), 12/0.004 (23 px >8) | T1, S1 |
| control-open-close | open-t0000 | FAIL 212 / 2.40 / 0.9999 | FAIL 244 / 2.78 / 0.9999 | 9/0.007 (3 px >8), 12/0.013 (16 px >8) | T1, S1 |
| control-open-close | open-t0016 | FAIL 212 / 2.40 / 0.9999 | FAIL 244 / 2.78 / 0.9999 | 9/0.007 (3 px >8), 12/0.013 (16 px >8) | T1, S1 |
| control-open-close | open-t0060 | FAIL 212 / 3.14 / 0.5817 | FAIL 244 / 2.74 / 0.5816 | 22/1.850 (11572 px >8), 20/1.195 (4606 px >8) | T1, A1 |
| control-open-close | open-t0120 | FAIL 212 / 1.19 / 0.9957 | FAIL 244 / 1.44 / 0.9957 | 82/0.288 (2105 px >8), 77/0.395 (2118 px >8) | T1, A1 |
| control-open-close | open-t0220 | FAIL 212 / 0.85 / 0.9992 | FAIL 244 / 0.97 / 0.9992 | 112/0.056 (419 px >8), 105/0.057 (432 px >8) | T1, A1 |
| control-open-close | open-t0400 | FAIL 212 / 0.80 / 1.0000 | FAIL 244 / 0.93 / 1.0000 | 9/0.004 (3 px >8), 12/0.007 (16 px >8) | T1, S1 |
| control-open-close | close-t0000 | FAIL 212 / 0.80 / 1.0000 | FAIL 244 / 0.93 / 1.0000 | 9/0.004 (3 px >8), 12/0.007 (16 px >8) | T1, S1 |
| control-open-close | close-t0060 | FAIL 212 / 6.94 / 0.9976 | FAIL 244 / 4.29 / 0.9975 | 82/6.340 (152421 px >8), 75/3.469 (111934 px >8) | T1, A1 |
| control-open-close | close-t0150 | FAIL 212 / 2.40 / 0.9999 | FAIL 244 / 2.78 / 0.9999 | 9/0.007 (3 px >8), 12/0.013 (16 px >8) | T1, S1 |
| control-open-close | close-t0210 | FAIL 212 / 2.40 / 0.9999 | FAIL 244 / 2.78 / 0.9999 | 9/0.007 (3 px >8), 12/0.013 (16 px >8) | T1, S1 |
| control-open-close | close-t0400 | FAIL 212 / 2.40 / 0.9999 | FAIL 244 / 2.78 / 0.9999 | 9/0.007 (3 px >8), 12/0.013 (16 px >8) | T1, S1 |
| control | — | FAIL 212 / 1.05 / 1.0000 | FAIL 244 / 1.36 / 1.0000 | 183/0.269 (2561 px >8), 213/0.455 (2616 px >8) | T1, I1, I2 |
| idle | — | FAIL 212 / 2.40 / 0.9999 | FAIL 244 / 2.78 / 0.9999 | 9/0.007 (3 px >8), 12/0.013 (16 px >8) | T1, S1 |
| island-expanded | — | FAIL 212 / 4.24 / 0.9999 | FAIL 244 / 6.70 / 0.9999 | 183/3.361 (24550 px >8), 213/5.743 (24611 px >8) | T1, I1, I2 |
| media-playing | — | FAIL 212 / 3.17 / 0.9999 | FAIL 244 / 4.09 / 0.9999 | 183/0.857 (2561 px >8), 213/1.452 (2616 px >8) | T1, I1, I2 |
| stats | — | FAIL 212 / 0.31 / 1.0000 | FAIL 244 / 0.37 / 1.0000 | 9/0.005 (3 px >8), 12/0.010 (16 px >8) | T1, S1 |
| tray-menu | — | FAIL 212 / 0.97 / 1.0000 | **runtime:** fixture replay exceeded 25-second bound | 9/0.004 (3 px >8), — | T1, S1, H1 |
| tray-overflow | — | **runtime:** fixture replay exceeded 25-second bound | FAIL 248 / 17.82 / 1.0000 | —, 248/17.818 (92817 px >8) | T1, H1 |
| tray-submenu | — | **runtime:** fixture replay exceeded 25-second bound | **runtime:** fixture replay exceeded 25-second bound | —, — | H1 |
| volume | — | FAIL 212 / 1.76 / 0.9999 | FAIL 244 / 2.04 / 0.9999 | 9/0.005 (3 px >8), 12/0.009 (16 px >8) | T1, S1 |
| wifi | — | FAIL 212 / 0.55 / 0.9999 | FAIL 244 / 0.69 / 0.9999 | 56/0.034 (2875 px >8), 46/0.074 (2980 px >8) | T1, B1 |

Evidence:
- `docs/verify/raw/parity-fullscreen/`: `fullscreen-results.json` (engine), `compare.json`
  (gate, regions, layer attribution, shift and timing analysis per failing image) and
  `residual.json`;
- `heat/` for the representative failures: red = actual stronger, blue = weaker;
- `zoom/`: reference on top, hyprv-shell below, over grey.

## Causes

### T1 · tray icons (layout ok, content wrong) → hyprv-shell, hyprv-widgets, hyprv-ui-bar

Every tray item renders as a letter (C D O F W T). The reference shows the theme icons
plus Quickshell's magenta/black missing-icon placeholder for `telegram` (see
`zoom/T1-tray-icons.png`). Positions, pill geometry and the overflow grid layout match.
Three independent defects:
1. **Fixture loader (`hyprv-shell/src/fixture.rs:204`).** It writes the fixture's
   `icon` into `TrayItem.icon`, but `assets::themed_icon` resolves only `icon_name`, so
   every replayed item is iconless. This doesn't affect production (the provider fills
   `icon_name`), but it blocks the gate.
2. **`hyprv_widgets::primitives::resolve_icon`.** It only probes `hicolor`/`Adwaita` at
   an exact `{size}x{size}` (36x36 at DPR 2) or `scalable`. Qt's `QIcon::fromTheme`
   picks the closest available size and follows the theme's `Inherits` chain. The
   fixture icons exist only as hicolor 32/48/64/128/256 PNGs (google-chrome, wechat,
   fcitx) or in Papirus (discord), so even with (1) fixed they would not resolve.
   The original resolves all of them.
3. **Missing icon.** The original shell draws Quickshell's magenta/black placeholder
   for an unresolvable `IconName` (telegram: no installed theme the shell actually
   uses has it, because qt6ct fails to load, see parity/README). Rust draws the letter
   fallback. **User decision (2026-10-23):** "references must match what the user actually sees"
   (the live shell shows the icon missing). Rust should draw the placeholder, not the
   letter. The letter fallback stays only for an empty icon name / failed source.

### S1 · small SVG icon antialiasing → hyprv-widgets (svg raster)

**3–23 px at Δ ≤ 12/255** on the control-panel trigger (`control-panel-*.svg`, logical
3012–3028, y 20–40) and the bell. Light: 3 px, Δ 9, alpha 234 vs 237. Dark: up to 23 px,
Δ 12. It isn't visible at 1× (`zoom/S1-control-icon-dark.png`), but it exceeds the
≤ 8 gate.

### I1 · island album art missing → hyprv-shell (replay asset load) / hyprv-ui-island

`media-playing`, `island-expanded` and `control` (Spotify playing): the island shows the
generic ♪ placeholder where the reference shows the fixture cover
(`@ASSETS@/cover-*.png` from `media-status.sh`). See `zoom/I1-I2-island-*.png`. Both
images:
- compact: 2.1k px at logical (1480, 16);
- expanded: 18.8k px at (1352, 20).

Art arrives through `art_receive` (async decode). The fixture replay doesn't wait for it
before capturing (parity-host awaits async images), or the `@ASSETS@` path isn't
resolved in replay. Owner: hyprv-shell first, island if the decode path is at fault.

### I2 · audio spectrum bars nearly flat → hyprv-ui-island (svc-media)

`audio-spectrum.sh` streams one frame at t=0: `12;38;74;92;66;41;58;80;…`. The
reference keeps those levels: tall bars, and the expanded island shows 8 bars of 30–75
px. Rust shows small dots or low bars at the same virtual time: compact 452 px at
(1564, 20), expanded 5.6k px at (1648, 40). Rust apparently decays or smooths toward
zero without new frames, while the QML holds the last frame.

### B1 · Wi-Fi trigger keeps a hover/pressed highlight while its popup is open → hyprv-ui-bar

`wifi` light/dark: about 2.9k px at logical (2828, 8). After the click the pointer is
still over the slot. The original shows no highlight on the Wi-Fi slot while the
popup is open; Rust paints the hover pill (`zoom/B1-wifi-trigger-highlight.png`).

### A1 · control-panel open/close animation timing → hyprv-ui-panels + hyprv-shell

- **open-t0060.** Rust still matches the *closed* frames (timing best match:
  close-t0150, mean 0.048). The reference already shows the opening panel and backdrop.
  IoU 0.558; the error concentrates on the panel's first rows, actual alpha +8…+12.
- **open-t0120 / open-t0220.** The slider row at logical y 392 (open-t0220: y 476) is
  2 logical px too high: a 4 px shift removes 96.7 % of the error.
- **close-t0060.** Rust fades out faster: the panel and the backdrop over the bar are
  ~10–12 alpha below the reference across 152k px.

The original's animation starts one frame after the input, under the real compositor
(see `parity/fullscreen/README.md` findings). The Rust timeline starts later on open
and ends sooner on close. Settled frames (t0400, close-t0150+) match apart from T1/S1.

### A2 · island → calendar morph mid-frames → hyprv-ui-island / hyprv-ui-panels (calendar host)

- **open-t0016 and close-t0100.** About 1k px in the island clock area (logical
  1476–1576, y 32–48), with no shift and no alpha bias. The content cross-fade
  differs slightly.
- **open-t0100.** 13.2k px over the morphing card (1304–1768 × 8–444), mean alpha +1.6.
  Rust is marginally more opaque mid-morph.

Settled morph frames (t0210, t0500, close-t0209+) match apart from T1/S1.

### H1 · replay exceeds its 25-second real-time bound → hyprv-shell (harness)

All three tray fixtures in both themes end with `fixture replay exceeded 25-second
bound`. tray-menu light and tray-overflow dark captured their frame before the bound
hit; the other four captured nothing. The Rust tray menu itself renders correctly
(`zoom/tray-menu.png`: only T1 differs). tray-overflow, with 96 SNI items, and the
menu/submenu steps (rightclick, 600 ms, click "Status") need more real time than 25 s,
or the loop stalls after a popup opens. `replay.rs` waits whenever any surface has zero
size. Raise or remove the bound, or make it virtual-time based, and report which op it
stalled on.

### Not attributable to Rust

- **Matte uncertainty.** At most 8 % of the differing (>2) pixels per image are in the
  reference's `matte.uncertain_px_coords` (alpha ±1–2 from Hyprland's blend). They
  never exceed the ≤ 8 tolerance, so none of them decide a failure.
- **Out of scope.** The battery popup and the Mihomo page are unreachable in the
  original (manifest `unreachable`).

## Fix status A1/A2 (spec-bar, 2026-10-23, nested re-run of all 26 jobs, tray strip T1 masked)

| issue | frames | before | after | change |
|---|---|---|---|---|
| A1 | open-t0060 | 11.6k px > 8 | 0 (max 4) | hyprv-shell replay emulates Qt's timer-mode animation driver (docs/requests/hyprv-shell.md) |
| A1 | close-t0060 | 152k px > 8 | 0 (max 4) | same |
| A1 residual | open-t0120 / open-t0220 | 2.1k / 0.4k px | unchanged | One reveal-clip edge row. The real shell lags about 1 ms / 7–8 ms there; not modelled. |
| A2 | calendar-morph open-t0016 / open-t0100 / close-t0100 | ~1k / 13.2k / ~1k px | 0 (max ≤ 6) | Driver above, plus `QUIRK_ISLAND_ABOVE_CALENDAR` in hyprv-ui-bar (DynamicIsland `z: 1` over the calendar Loader) |

Note on the triage text: the reference open-t0060 is blank (the real shell still shows the last
driver tick, t0050). It was Rust that showed the panel early, not the other way round.

Overall: 48/64 images now max ≤ 8. The rest are I1/I2 island-expanded, T1 tray-overflow,
B1 Wi-Fi, the A1 residual, and 4–7 px of S1 icon AA.
