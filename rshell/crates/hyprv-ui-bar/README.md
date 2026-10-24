# Bar UI host contract

This crate ports the bar, its per-output motion/calendar host, one passive corner
overlay per output, SNI menu, overflow grid, and battery detail popup. It does no filesystem,
process, D-Bus, socket, or device I/O. Service actions use the actual provider enums;
application launches and foreign panel toggles are typed `HostAction` intents.

The source contracts are `docs/spec/ui-bar*.md`. Geometry comes from real widget
font layout, including Qt anchor rounding and glyph ink overflow. The primary
layout matches `shell.qml:3758-4350`; fullscreen motion follows `BarScreenState.qml`
and `shell.qml:3629-3659`; calendar hosting follows `shell.qml:3668-3744,3879-3997`.
Screen corners follow `ScreenCornerShade.qml` and `shell.qml:3467-3546` using the
legacy single full-output surface with four 27×27 children. The topology table in
`docs/spec/ui-bar.md:139` agrees; its later four-surface optimization suggestion
is superseded by the explicit legacy topology requirement. Menus, overflow, and battery cards follow
`shell.qml:611-1618`; glass chrome uses the canonical widget `Reveal` and
`glass_panel` implementation of `AnimatedGlassPanel.qml:73-219`.

## Public API for the shell integrator

| Entry point | Contract |
|---|---|
| `Model`, `Theme`, `model::{Output, Assets, IslandGeometry, Slots}` | typed snapshots and immutable UI inputs; `Model::default()` is safe before services arrive |
| `build(&Model, &Theme) -> Node` / `update(&mut Scene, &Model) -> usize` | full per-output tree, including center host and slide; return count of changed nodes |
| `view::build_strip` / `view::update_strip` | **use for existing `hyprv-shell::host::Slot::Bar`**; omits center/dismiss layers and applies no slide, since the shell already owns them |
| `layout::compute` | real measured module/pill geometry, center/title, trigger hit boxes, input union and QuickAdjust anchor |
| `Input::pointer(&mut Model, PointerEvent) -> InputResult` | immediate typed actions, popup request, cursor and redraw flag; use an unshifted model for input already translated into Slot::Bar coordinates |
| `Input::{advance, next_deadline}` | 80 ms ShowVolume delay, no polling |
| `Controller::{new, sync, frame, pointer, next_deadline, animating}` | complete standalone per-output lifecycle when the host delegates bar motion to this crate |
| `Popups::{command, update, build, update_scene, pointer, keyboard, card_rect, requirements, next_deadline, animating}` | global singleton popup controllers; full-output trees, preserving individual opening outputs |
| `surfaces::{bar, corners}` and `Requirement::layer_config()` | logical surface data and engine adapter |
| `view::{svg_image, svg_picture, tray_pixmap}` | Qt-free memory-only dynamic SVG / SNI ARGB decoding; prefer `svg_image(bytes, width, height, scale)` at the current DPR; picture compatibility helper is scale 1 |
| `baked::image(icon, logical_size, scale, color)` | immutable offline QtSvg PNG cache; `source` selects embedded source colors, corner color is `000000ff` |
| `svg::{decode, decode_backend, Backend, Decoded}` | pure in-memory GrayOutline solid-path decoder and resvg complex fallback; returned backend makes parity provenance explicit |
| `ipc::{toggle, layout_status, input_status, data_status}` | source-compatible bar IPC helpers; requested output for layout, primary for input |

`Slot::Bar` should use `build_strip`, and the shell's existing Island/Calendar
slots should stay attached to its center host. Choose one lifecycle owner:
`Controller` + full `build`, or the shell's existing motion + `build_strip`.
Do not compose both motion/calendar hosts. Likewise popup `build` already includes
full-output glass chrome, input backdrop, and local positioning; mount that tree
as a complete popup when using this crate's `Popups`, rather than putting it
inside the shell's second glass/reveal wrapper. The host owns all actual surfaces,
service subscriptions, timers, and action dispatch in either arrangement.

## Mounting

1. Build a `Model` from the service snapshots reexported in `model`, plus the
   output's logical geometry and theme. Create one `Controller` and `Scene` per
   output using `Controller::new(model)` and `build(&controller.model, &theme)`.
2. On a service delta call `controller.sync(next_model, now)`. Supply current
   monotonic event-loop time to `frame`, which samples motion, returns delayed
   actions, and reconciles only changed nodes. Reuse the scene: unchanged builds
   create no scene edits or damage. Native SVG slots should be cached Pictures
   or Images; opaque SVG DOMs conservatively count as changed.
3. Translate engine pointer coordinates/buttons/wheel events to
   `hyprv_widgets::controls::PointerEvent`, then call `controller.pointer`.
   Dispatch `InputResult.actions`, route its optional `popup` through the global
   `Popups::command`, and apply its cursor. Foreign island/calendar contents have
   their own input handlers; forward events within their host rectangle to them.
4. Mount `surfaces::bar`, `surfaces::corner_window`, and `popups.requirements`.
   `corner_window(width, height, scale, top, bottom)` builds the production scene.
   Mount it with `LayerConfig.full_redraw = true` in exactly one overlay per output: namespace
   `hyprv-screen-corner-overlay-<output>`, all four anchors, compositor-sized
   `(0, 0)`, zone `-1`, no keyboard/input. Its root covers the model's full output;
   rebuild on geometry/scale or reveal changes. Its A8 sprites share a screen-sized
   R8 atlas, preserving Qt window sampling without a BGRA atlas. `surfaces::corners`
   remains the offscreen component-fixture scene, whose Qt RenderControl resource
   context differs from an on-screen window. Top shades follow
   `BarScreenState.qml:22`; bottom margin is zero.
   Use the `Requirement` returned by `surfaces::corners` for this overlay, and
   replace its fixture tree with `corner_window`. `Requirement::layer_config()`
   provides the engine configuration; set `full_redraw = true`, resolve its
   output name to a `wl_output`, honor `mapped`, and apply `input_regions` through
   `UiContext::set_input_region`. Pass the actual output scale to both the corner
   builder and the EGL buffer configuration.
   Popups remember their own opening output independently and refresh its geometry
   when that output's model arrives. Pass globally shared service snapshots even
   when the current model describes a different output.
5. Keep one global `Popups`. Route each popup's pointer and Escape keysym (`0xff1b`)
   to `pointer` / `keyboard`. `update(model, now)` advances reveals and returns
   lifecycle actions; `update_scene(scene, model, kind)` edits only changed nodes.
   Route `PopupInput.command` back through `command` for menus opened in overflow.
6. Request presentation frames only while `animating()`, after input/state damage,
   or when a one-shot `next_deadline()` fires. Call `frame` / popup `update` when
   that timer fires and re-arm to the new minimum. There are no idle timers here.

`Slots` accepts pure builders for the island and calendar. Supply the island's
current width/height/attached offset and requested `expanded` state in every
external model. The bar holds its tall surface for 230 ms after collapse; calendar
close keeps its geometry for 210 ms, then snaps to the island. Calendar dismissal
arms after 260 ms and uses a 220 ms one-shot leave timer. The panels host supplies
child-active/opening changes from toplevel events, without polling.

Service state currently omits workspace urgency and normalizes fullscreen modes.
The host must supply `urgent_workspaces` and exact `fullscreen == 2`; maximized
windows must not hide the bar. `is_primary` is metadata for host IPC routing:
`layoutStatus` takes the requested output model or None; `inputStatus` takes
the primary output model or None. `monotonic_us` must use
the MPRIS position anchor's clock domain. Root sampling diagnostics are supplied
through `ipc::Diagnostics`. The bar never manufactures service subscriptions.

Route `AcquireBatteryDetails(true/false)` to subscriber demand for battery history;
keep demand only while requested. Subscribe to normal system statistics while bars
are mounted. Route `ShowVolume` to QuickAdjust after the source's 80 ms delay.
Rofi, btop, audio mixer, viewflow and foreign panel intents belong to the shell/app
service host. No runtime shell commands are embedded here.

## Assets and quirks

The host resolves/decode assets once and places immutable images in
`model.assets.images`. Keys: tray item `key`, menu `menu:<item-key>:<entry-id>`, and
`format::wifi_asset_key(model, dark)` (Fluent SVG basename, including `-symbolic`
for dark icons). Prepare images at the current fractional buffer scale. Fixed shell-owned icons use
`baked::image` automatically; caller-provided assets take precedence. An absent
image intentionally draws the QML font fallback. `view::tray_pixmap` converts SNI
ARGB buffers without I/O. Tray icons retain QtRendering; other text is native.
Bundled control-panel and light wired SVGs are copied from the original
`quickshell/assets/bar` and `quickshell/assets/tray` files.

The named `QUIRK_*` constants in `model.rs` preserve: no-op bar toggle, inverted
volume wheel, horizontal-only tray wheel producing a vertical -1 event, tray width
excluded from the supplementary-info threshold, ineffective glass content slide,
the globally focused window title across outputs, and retention of the last CPU
power severity color while its label reads `-- W`. `QUIRK_BATTERY_POPUP_NEVER_OPENS` defaults to true, matching the live QML's
popup-only discovery loop. The label requires an available battery-detail
snapshot; UPower presence alone cannot open it. Setting this flag false opts
into the UPower discovery fix. Seeded detail fixtures still exercise the latent
popup UI. No shell edge-hover surfaces are needed: those
bindings belong to Hyprland's UltraEdge plugin (`ui-bar.md §8`).

## Validation

```sh
cargo test -p hyprv-ui-bar --all-targets
cargo clippy -p hyprv-ui-bar --all-targets --no-deps -- -D warnings
cargo run -p hyprv-ui-bar --example render
python3 crates/hyprv-ui-bar/scripts/parity.py
# Same strict comparison as an explicit opt-in integration test:
cargo test -p hyprv-ui-bar --test parity -- --ignored
```

The behavior suite checks adaptive metrics, tray budgets/filtering, continuity on
fullscreen retarget, stationary zero damage, input cancellation, typed actions,
calendar lifetime/dismissal, popup hydration/cleanup, and independent outputs.
The offscreen test exports every `ui-bar.md §14` state in both themes at 1, 1.5,
and 2 native scales, checks PNG dimensions, and writes
`artifacts/actual/bar/manifest.json`. Popup fixtures include indicator/icon,
disabled/separator, submenu, animation, overflow 1/6/7, and CJK battery variants.
The fixture host reads installed Fluent icons when available, with an explicit
missing-icon case. Its tray/menu demonstration images are synthetic; island and
calendar foreign slots are empty. These exports verify rendering, not full-shell
pixel parity. QML references include separately owned contents; parity results and
coverage limitations are recorded in `artifacts/parity` and `VALIDATION.md`.
No live layer surfaces or devices are opened by these tests.

During parallel construction, `python3 crates/hyprv-ui-bar/scripts/check.py test
--all-targets` (or `clippy --all-targets --no-deps -- -D warnings`) uses a manifest
under this crate's ignored `.build/check`. It compiles the identical source and
real canonical dependencies; it does not stub or copy provider/widget code.
This avoids unrelated workspace manifests temporarily lacking targets.

The reference comparison adapter requires Python Pillow/NumPy, Qt6Widgets/Qt6Gui
development files and a C++ compiler, matching the existing QML capture setup.
These Qt helpers are offline fixture tooling only. Calendar/island content crops
remain their owning crates' responsibility. The default offscreen suite has no
Qt or display-server runtime dependency.

## Qt-free raster parity

`tools/bake.py` is an **offline build/install tool** requiring Qt6Svg/Qt6Gui and
a C++ compiler. Run it when installing a different fixed icon set or adding an
output scale. Cargo/runtime consume embedded PNGs and do not invoke or link Qt.
The cache covers 1, 1.25, 1.5, 1.75, 2, 2.5, 3 and 4; `--scales` selects a new
set. The identity is icon + logical size + preferred scale + color; manifest
source SHA-256 and tool/Qt provenance detect stale assets. Multicolor assets use
`source` as their color key. Control icons bake the explicit sourceSize32 at output DPR (64 physical pixels
at DPR2), before mip filtering into the16-pixel slot (`shell.qml:4299-4301`).
The crop-test host has effective DPR1 even for scaled captures; its adapter
supplies the independently observed32-pixel texture through `Model.assets`. Four corner variants
bake Canvas's transform before painting (`ScreenCornerShade.qml:29-63`).

Unknown tray/app SVGs use `svg::decode`; the host supplies bytes and caches the
returned image. External href resolution is disabled. Solid paths use Qt's
cubic flattening semantics and the FreeType gray rasterizer; gradients, masks,
filters and other complex content fall back to resvg. Independent QtSvg golden
probes and numerical limitations are recorded in `VALIDATION.md`; a passing
finite corpus does not establish exactness for arbitrary SVGs.

The current recaptured-reference GPU acceptance is **304/304 bit-exact**.
The adapter replays submenu hover after page replacement and derives idle island
width from the actual font advances. Public/runtime APIs are unchanged.
[Current metrics and regression receipts](VALIDATION.md).

`python3 scripts/parity.py` freezes complete QML inputs, prepares fixture-only
QIcon/QImageReader assets, and reports **premultiplied RGBA**, the current
per-image distance-field acceptance flag, painted-mask IoU and footprint edges.
The interim v1 tolerance and exact count are distinct metrics. A failure exits
nonzero. Reference source files and shared reference PNGs are never changed.
The shared rectangle mode lives in `hyprv_ui::qt_rect`: Renderer defaults to
QtQuick, with cached GPU vertex meshes on GL and cached CPU rasterization for
offscreen tests. The bar emits `Content::Rect` directly and has no private rect
renderer. `set_rect_mode(RectMode::Skia)` remains an explicit fallback. Bar image
leaves select `Node.image_sampling = ImageSampling::Mipmap`; menu raster icons
retain Linear. The local rectangle substitution and image renderer were retired
after engine integration. See `VALIDATION.md` for measured results.

Review BAR-1..3: empty menus wait for subscribed service changes after one staged
open turn; disappearance cancels work. Hit regions follow the reveal viewport.
Every overflow close, outside release, escape and fresh mapping cancels its grab.
`tests/review_regressions.rs` checks these paths, including a valid click control.
Changing menu pages clears the old delegate hover; a new move selects the new
row, matching QML Repeater replacement. The submenu fixture replay now sends
move/press/release through this same controller.
BAR-4 uses svc-net's shared `hyprv_widgets::flickable` for drag, finite flick and
wheel motion; `tests/flickable.rs` verifies scrolling and cancellation at rest.

`tools/check_rect_effect.py` isolates Qt RHI source rectangles from MultiEffect
using an existing parity host and the `rect_effect` example. It owns a private
headless compositor and writes `artifacts/rect-effect`; it never updates shared
reference files. The canonical `MultiEffect.source_texture_size` API now preserves QML's explicit
unpadded layer size while padding its logical source region. Bar menu/battery
chrome uses it at the bound output scale. The temporary prototype was retired;
its evidence remains in `artifacts/rect-effect-prototype`.

`tools/verify_all.py RUN_NAME [canonical verifier options]` runs the existing
`parity/verify/verify.py` evaluator with private output directories under this
crate, preserving all reference and acceptance logic. It routes the calendar
translator's actual `HYPRV_CALENDAR_PARITY_OUT` variable so all frames are kept.
`--backend gl` uses parity QA's shared EGL helper (dev-only) in the reference
adapter, on the verifier's private headless Weston; default runtime APIs are
unchanged. The bar inherits the engine's canonical QtQuick rectangle mode.

`tools/check_submenu.py --host parity/build/parity-host` independently replays the
frozen submenu fixture with the canonical Qt capture environment; `--explicit-hover`
is a separate input diagnostic. It checks original QML hashes and never replaces
shared fixtures/references. `tools/render_gl_probe.py NAME [--filter STATES]`
repeats the frozen corpus in private headless Weston contexts and retains each
run separately. Neither tool changes acceptance rules. Native glyph diagnostics
are in `examples/glyph_probe.rs` and `tools/glyph_probe.qml`; the direct-GL
prototype is archived under `artifacts/native-gl-prototype`, outside production.

Bar native text leaves select `Node.native_text_mode = NativeTextMode::QtGl`.
The host uses its normal `EglWindow::draw`; no host configuration is needed.
Scene synchronization preserves the mode. Native color and inherited opacity
follow QQuickText's QRgb8 conversion (64 independent Qt cases exact).

Bar component roots also set `Node.native_materials = Some(true)`: canonical
QtQuick rectangle meshes, image sampling/mipmaps and MultiEffect composition use
context-owned GL caches. The image-only corner overlay opts in independently of
text. `None` inherits within a component; `Content::Shared` starts a fresh scope
so foreign island/calendar trees keep their own mode. Other UIs retain defaults.
Unsupported clips/layers/transforms and CPU replay preserve their fallback paths.

`tools/render_gl_probe.py` pins its executable and hashed raster asset snapshot.
`scripts/parity.py` prepares assets under its report's `inputs/assets`, preventing
parallel preparation from changing an in-flight replay's tray icons. References,
fixtures and acceptance thresholds are never rewritten by these tools.

Host incremental API: retain one `view::RetainedView` per mounted Scene and call
`update(scene, model, normalize)` on presentation/data changes. `normalize` applies
host coordinate/clip canonicalization before diffing. `UpdateStats` reports built
and retained modules and edited nodes. Equal module signatures skip construction
and subtree reconciliation; scaffold/hosted-slot changes remain independent.
Module membership changes rebuild the affected structural arrangement safely;
retained Scene IDs are preserved by slot reconciliation. Replacing a Scene with
new IDs safely rebuilds modules. `build` / `snapshot` remain pure fixture APIs.

GPU fixture precision: `examples/reference.rs` renders the original output-sized viewport, then crops the PNG to the frozen capture bounds, matching the Qt capture order. CPU fixture transport is unchanged. `HYPRV_BAR_FULL_TARGET` retains the raw-output diagnostic option. This affects test transport only; the public runtime API above is unchanged. `tools/batch_material.qml` and its Rust companion supply independent native image/rectangle/layer goldens at DPR1/2.

Host fixture coverage: `examples/reference/host.rs` mounts the production
`hyprv-ui-island::Island` and `hyprv-ui-panels::calendar::CalendarPanel` through
`Model.slots` for `island-expanded-host` and `calendar-host`. Those crops compare
complete contents without an island mask. Component dependencies are dev-only;
the public runtime API is unchanged. The owner gate counts every reference in its
manifest, reports missing renders explicitly, and fails incomplete coverage.
Regression checks: `python3 scripts/test_gate_coverage.py` and
`python3 scripts/check.py test --example reference` (from this crate).

Large right-side tray subtrees preserve Qt’s promoted transform batch origin
(`Node.native_batch_root`) without clipping or offscreen allocation. Eligibility
uses the QML subtree (including the tray image/fallback-text nodes), not retained
placeholder contents. `RetainedView` keeps promotion for the transform lifetime,
matching QSG even after tray items disappear.
