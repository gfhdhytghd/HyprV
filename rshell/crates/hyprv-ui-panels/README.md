# hyprv-ui-panels

Owner: spec-bar. Rust port of the shell popups: control panel (main / Bluetooth /
Mihomo pages), Wi-Fi panel, QuickAdjust OSD, stats popup, week calendar content and the
startup confirmation dialog. The spec is `docs/spec/ui-panels-*.md`; every QML quirk we
reproduce is a named `QUIRK_*` const (listed below).

Build: `cargo build -p hyprv-ui-panels` (default feature `services` pulls in the
hyprv-svc-* crates for `services.rs`); `--no-default-features` builds the pure UI.

## Public API — mount contract

Everything takes a [`model::Model`] snapshot and returns `hyprv_ui::scene::Node` trees
plus `model::Action`s. The crate never talks to services, Wayland or IPC itself.

### Model (input)

`Model { theme, output, shell, now, wall, network, bluetooth, bluetooth_show_unnamed,
media, audio, controls, stats, calendar, assets }`

- `output`: name, logical width/height and **scale** (the device pixel ratio; text and
  rectangles are rasterised for it, so rebuild after a scale change).
- `now`: monotonic animation clock; `wall`: local wall time (calendar, clock text).
- `assets: Rc<Assets>`: SVG DOMs and images keyed by path relative to `quickshell/`
  (e.g. `assets/control-panel/light-dark.svg`) plus Fluent icon keys
  (`24/panel/nm-signal-75-secure.svg`, `symbolic/status/…-symbolic.svg`, see `icons.rs`).
  Shell SVGs are drawn from the **offline QtSvg bake** (`baked.rs`, below) when a raster
  exists for (key, size, scale); the DOM is only the fallback for unbaked scales. Load
  SVGs with `assets::load_svg` (resolves CSS `<style>` class colours, which skia's SVG
  parser ignores). `fixtures::load_assets(quickshell_dir, fluent_dir)` loads the set.
- `media.art`: the decoded cover image (`skia::Image`); the card crops and masks it.
- With the `services` feature, `services::{network, bluetooth, audio, media, profile,
  controls, stats, calendar}` convert hyprv-svc-* states into model parts.

### Output actions

`Action` variants are user intents (`Wifi*`, `Bluetooth*`, `SetVolume`, `ToggleMute`,
`Media*`, `BrightnessSet/Finish`, `FanLimit`, `ProfileSet`, `Toggle*`, `VmToggle`,
`ViewflowSet`, `Mihomo*`, `Calendar*`). `services::route(&action, backlight_output,
sequence)` maps them to `ServiceAction`. `Action::Host(HostAction)` is for the host:
`Mapped(PanelKind)` / `Unmapped(PanelKind)` (create / destroy the layer surface),
`OpenWifiPanel`, `OpenWifiManager`, `OpenBluetoothManager`, `SetBluetoothShowUnnamed`,
`WifiPanelVisible`, `RefreshPanelData`, `CloseCalendar { force }`, `ExpandCalendar`,
`CalendarPointerInside`, `StartupDecision(bool)`.

### Option 1: full popups (`Panels`)

`Panels` owns every popup's lifecycle and AnimatedGlassPanel reveal exactly as the QML
does. The host only creates the surfaces the façade asks for.

| Call | Use |
|---|---|
| `Panels::new()` | one per shell |
| `panels.control.toggle_for(anchor, output, now)`, `panels.wifi.set_anchor(..)` + `toggle(model)`, `panels.stats.toggle_for(..)` | bar triggers (anchor = trigger rect in output coordinates) |
| `ipc_control_panel_toggle(primary_output, model)`, `ipc_quick_adjust(function, model)` | IPC handlers |
| `surfaces(model) -> Vec<(PanelKind, SurfaceSpec)>` | layer surfaces that should exist now (namespace, layer, anchors, exclusive zone, keyboard mode, size, input region) |
| `build(kind, model) -> Node` | full-surface tree when a surface is mapped |
| `update(kind, &mut scene, model) -> bool` | per frame or after an event: edits only the changed nodes |
| `pointer(kind, Pointer, model)` / `key(kind, &KeyEvent, model)` / `escape(kind, model)` | input in surface-local logical coordinates; returns `Reply { actions, cursor, redraw }` |
| `tick(model) -> Vec<Action>` | timers (open/close timers, hold timers, QuickAdjust hide). Also measures content heights (`needs_measure()` / `ControlPanel::measure`) so a reveal never waits for a build, like QML's `fullPanelHeight` binding |
| `animating(model)`, `next_deadline(now)` | request a frame while animating; otherwise sleep until the deadline |

`ui::Pointer` = `Motion | Leave | Cancel | Press | Release | Wheel { angle_*, pixel_* }`
(Qt angleDelta 120/notch and pixelDelta). `Leave` or `Cancel` while a button is held ends
every grab like QtWayland's ungrab: sliders run MouseArea `onCanceled` (control-panel
brightness emits `BrightnessFinish(preview)`), Flickables `cancelInteraction()`, no click
is synthesised. Hosts should map `PointerEvent::Cancel` to `Pointer::Cancel`; `ui::Key` covers Escape/Return/Enter/Space/Tab/
BackTab/arrows/Backspace/Delete/Home/End and `Text(String)` from xkb.

### Option 2: host-owned chrome (hyprv-shell `Component`)

hyprv-shell owns the full-output layers, glass reveal, outside-click and Escape. Wrap
the content-level calls in a `host::Component` per slot:

| Slot | Content call | Notes |
|---|---|---|
| `ControlPanel` | `ControlPanel::content(model, card_abs) -> (Node, w, h)` | `card_abs` = card origin on the surface (used for pixel snapping and input zones). Send `HostAction::PopupSize { w, h }`. After input call `take_page_request()`; if `Some(page)`: close with `ControlPanel::timing(current)`, `set_page(page)`, reopen with `timing(page)` |
| `Wifi` | `WifiPanel::content_node(model, card_abs)` | per output; `PopupSizeOn` |
| `Stats` | `StatsPopup::content(model, card_abs) -> (Vec<Node>, h)` | width `stats::WIDTH` (548) |
| `QuickAdjust` | `QuickAdjust::build/update` | its own opacity/scale animation (both are baked into the tree; `update` rebuilds every frame while they run); input region = card |
| `Calendar` | `CalendarPanel::build(model, abs)` / `update(scene, slot, model, abs)` / `pointer(ev, model, abs)` | embedded in the bar centre morph; `CalendarPanel::host_size` gives the morph target |

Content nodes are rasterised for their absolute device position (`card_abs`, the
calendar's `abs`) and `output.scale`: place them exactly there, at an integer device
position, and rebuild if either changes. After editing a scene yourself, call
`qrect::finalize(&mut scene, dpr)` so newly inserted rectangles/clips get the Qt
raster (the controllers' own `update` already does).

Input for option 2: convert `InputEvent::Pointer` to `ui::Pointer` in surface
coordinates and call the controller's `pointer`; map `Reply.actions` to service actions
(`services::route`) or host actions. Components with running animations (switch
thumbs, Mihomo border pulse, Wi-Fi caret blink, calendar flick) report
`animating(..)` / `next_deadline(..)`; the host must rebuild while those are true.
**Host gap:** `Component::build` is currently called only on data/theme revisions;
it needs a per-frame path while a component is animating (or an `update` hook).

### Startup dialog

`StartupConfirm::new(platform_dark, now)` (`platform_dark` is ignored:
`QUIRK_STARTUP_CONFIRM_ALWAYS_LIGHT`), `surface()`, `build(dpr)` / `update`,
`key`, `pointer`, `tick`, `next_deadline`. On `StartupDecision(true)` write `launch\n`
to `$HYPRV_STARTUP_APPROVAL`, then quit on either decision.

### Offscreen previews

`ControlPanel::page_preview(model, page)`, `WifiPanel::preview`, `StatsPopup::preview`,
`QuickAdjust::preview(model, mode)` return the fully revealed card at the origin
(tests, parity scenes, docs).

## Rendering notes (Qt-exact, runtime Qt-free)

| Module | Reproduces |
|---|---|
| `qtext` | Qt native text (unhinted FreeType at device scale, ¼-px subpixel buckets, design advances + GPOS kerning, fontconfig hintslight/autohint at dpr 1, 26.6 line alignment). Native text under a scaled item: device-snapped glyph quads, Nearest sampling (textmask.vert). |
| `qdf` | `Text.QtRendering`: Qt's distance-field glyph cache bit-exact (outline as `QRawFont::pathForGlyph` at 54·16 px incl. synthetic embolden, `qSimplifyPath`, `makeDistanceField` 24.8 fixed point) plus the DF shader. |
| engine `qt_rect` | Rectangles stay `Content::Rect`; the engine's QtQuick rect mode draws them (ENGINE-SHARED-1). |
| `qrect` | What the engine does not cover yet: `clip: true` as a device-pixel scissor (qRound edges, the item's own painting stays put) and native text under uniform-scale subtrees (QuickAdjust). |
| engine `MultiEffect` | Shadowed glass chrome (`source_texture_size` = the QML's unpadded `layer.textureSize`), toggle-icon colorization, album-art rounded mask. |
| `baked` | The textures QQuickImage loads for shell SVGs: offline QtSvg rasters at `sourceSize × dpr` (sampling and effects are the engine's: `Content::Image` + `image_sampling`, `MultiEffect`). |
| engine `canvas` | SystemTrendChart's Canvas: `CanvasPath`/`CanvasBatch` (QQuickContext2D paths, vertical gradient fill) rasterised by `hyprv_ui::qt_painter` (QStroker, gray raster, QPainter blending); the engine Canvas node (`CanvasBatch::node`) draws its round(size) × dpr image into the item rect. |

Builders bake text, rectangles and images for their absolute device position; moving a
subtree by a fractional device pixel needs a rebuild. Centring uses Qt 6
`qRound(size / 2)` (`ui::hc`).

**Icon bake** (`tools/bake.py`): compiles the bar's `crates/hyprv-ui-bar/tools/svg_bake.cpp`
unchanged (QSvgRenderer → QPainter), bakes every panel SVG at the size QQuickImage
requests (toggle `sourceSize` 36–44, IconImage 24 / 16) for scales 1–4 into
`assets/baked/`, and generates `src/baked_data.rs` (`include_bytes!`, keyed by icon,
size, scale, colour "source"; colourisation is applied at runtime). Re-run after an SVG
changes; QtSvg is a build-time dependency only.

## Secrets

The typed Wi-Fi PSK exists in plaintext only in `WifiPanel`'s `SecretBuf`
(`Zeroizing<String>`, 512 bytes reserved so typing never reallocates, popped bytes and
the whole buffer wiped on `clear`/drop). The field renders the U+25CF mask string, so
text layout/glyph caches and the scene diff key only ever see bullets and a character
count (`qtext::TextBox::key` also masks password boxes). Control characters are dropped
(Qt `isAcceptableInput`); a `"\r"` text event submits like Return.
`Action::WifiConnect.password` is one exact-capacity copy made at submit; its `Debug` is
redacted. **Host contract:** move it out (`std::mem::take`) into hyprv-svc-net's
`Connect` (zeroized on drop) or zeroize it; never log or clone the action.
Tests: `wifi::secret_tests` (cache/key scan, buffer wiping, control chars),
`control_panel::grab_tests` (lost grab).

## Quirks reproduced (QML behaviour kept on purpose)

`QUIRK_GLASS_CONTENT_SLIDE_IS_NOOP`, `QUIRK_PAGE_SWITCH_TIMER_RESTARTS_NEXT_FRAME` (popup),
`QUIRK_PROFILE_TOGGLE_NEVER_ACTIVE`, `QUIRK_VIEWFLOW_NEVER_ACTIVE`,
`QUIRK_SLIDER_LABELS_HIDDEN` (control panel),
`QUIRK_NO_BRIGHTNESS_FINISH` (QuickAdjust), `QUIRK_MISSING_CORE_SHOWS_ZERO`,
`QUIRK_X_ALWAYS_10` (stats), `QUIRK_SCROLL_INDICATOR_IN_CONTENT`,
`QUIRK_WEEK_CHANGE_RESETS_VERTICAL` (calendar), `QUIRK_STARTUP_CONFIRM_ALWAYS_LIGHT`
(startup), `QUIRK_CLIP_IS_DEVICE_SCISSOR`
(qrect), `QUIRK_FONTCONFIG_HINTSLIGHT_AT_DPR1`, `QUIRK_KERNING` (qtext). Timing constant
`TICK_START_DELAY_S` (control panel): animations started inside an animation tick begin
on the next tick.

## Tests

- `tests/visual_states.rs`: PNG for every spec'd visual state → `target-png/states/`.
- `tests/text_parity.rs`, `tests/df_fields.rs`, `tests/df_parity.rs`: native text and
  distance-field fields/raster vs Qt (≤ 1).
- `tests/trend_canvas.rs`: the chart's engine Canvas vs Qt renders in `qml-ref/canvas` (≤ 2; worst 1).
- `tests/qml_parity.rs`: each `fixtures.rs` scene vs the original QML rendered by
  `qml-ref/render.sh` (dpr 2); gate max ≤ 8, a missing reference fails. All ≤ 3 today.
- `tests/shared_parity.rs` + `tests/shared_common/` (svc-media): replays
  `parity/fixtures/panels/*.json` through `Panels` and diffs against
  `parity/reference/panels`. PASS requires the ARCHITECTURE.md gate: max ≤ 8/255 and
  mean ≤ 0.5/255 over painted pixels (column `painted_mean`). Captures use
  `Renderer::raster`, whose GPU-exact blending matches the GL framebuffer's per-draw
  8-bit stores (hyprv-ui ENG-BLEND-1); `PANELS_RASTER=f16` paints into the older RGBAF16
  approximation. With `HYPRV_PARITY_BACKEND=gl` the same captures come from the EGL path,
  rendered at the full 3072×1728 output and cropped, as Qt's projection uses the whole
  output. Captures are encoded like parity-host's `QImage::convertToFormat(RGBA8888)`
  (Qt SSE4.1 unpremultiply), so strict comparisons see rendering differences only.

### Parity status (2026-10-23, `parity/reference/panels`, 248 non-calendar captures)

| Group | captures | worst max | worst painted mean | exact (max 0) | gate |
|---|---|---|---|---|---|
| control panel main, toggles, sliders, press/hover | 60 | 3 | 0.09 | 0 | 60 ✔ |
| control panel open/close + page-switch frames | 68 | 3 | 0.24 | 28 | 68 ✔ |
| Bluetooth pages | 16 | 2 | 0.02 | 0 | 16 ✔ |
| Mihomo | 8 | 3 | 0.04 | 0 | 8 ✔ |
| Wi-Fi | 16 | 2 | 0.02 | 0 | 16 ✔ |
| QuickAdjust (incl. show/hide frames) | 52 | 7 | 0.04 | 16 | 52 ✔ |
| startup dialog | 12 | 1 | 0.02 | 0 | 12 ✔ |
| stats | 16 | 3 | 0.01 | 0 | 16 ✔ |
| **total** | **248** | **7** | | **44** | **248 / 248** |

Rectangles go through the engine's QtQuick rect mode. Panel roots opt into the engine's
Qt GL materials (`native_materials`), and single-line NativeRendering labels are engine
`Content::Text` nodes in `NativeTextMode::QtGl` (`qtext::ENGINE_NATIVE_TEXT`). Wrapped labels
use qtext's Qt line breaking, one engine line item each. Scaled items are drawn by the
engine's QSGTextMaskMaterial path (raster and GL). Fixed-height and password texts keep the coverage
pictures, whose colour is QRgb8-quantised (`QUIRK_TEXT_COLOR_QRGB8`). Workaround pending engine
ENG-OPACITY-1 (docs/requests/hyprv-ui.md): QuickAdjust's animating card puts leaf
rectangles in single-leaf opacity layers. The 52 calendar captures are validated by
piece B (hyprv-ui-island calendar morph); `CalendarPanel` matches the local QML ≤ 3.

### Strict GL acceptance — assigned control/Wi-Fi/stats share

The bar/island owner’s user-assigned share now passes **184/184**, with176bit-exact.
Control panel main/pages, Bluetooth, Wi-Fi, stats and VM/Viewflow crops are exact;
eight Mihomo DF crops pass at max3. This is the current strict premultiplied gate,
separate from the historical raster/v1 table above. QuickAdjust/calendar/startup
remain spec-bar’s share. [Current table, fixes and controls](VALIDATION-core.md).
No public mount API changed. The shared test translator uses actual process-line
delivery time; the engine preserves explicit layer logical projection for merged
rectangles. Runtime stays Qt-free.
