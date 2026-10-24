# Widget API

Owner split: shell-host owns this manifest, lib.rs, `foundations`, `primitives`,
`containers`; spec-panels owns `src/controls/` and `src/viz/`. Both public modules
are implemented and included by the default features.

All widget trees use `hyprv_ui::scene::{Node, NodeId, Scene}` and logical-pixel
`hyprv_ui::geometry::Rect`. Colors are **straight RGBA** at the API boundary;
Skia renders premultiplied buffers.

## Stable foundations (available now)

`foundations::{Palette, BASE_FONT, rgb, parse_color, with_alpha, panel_color,
hcenter, centered, LayoutItem, row, column}`. `Palette::new(dark)` exposes
snake_case colors (`primary_text`, `module_background`, `accent`, `glass_fill`,
`glass_stroke`, `card_fill`, `card_strong_fill`, `card_stroke`, `muted_text`,
`soft_text`, `active_fill`, `active_stroke`, `brightness`, `battery`, `microphone`,
`critical`, `usage_low`, `usage_medium`, `chart_accent`, workspace tokens).
`rgb("#RRGGBB")` is for literals; `parse_color` returns Option and also accepts
Qt #AARRGGBB. `row/column` return optional positions, skipping zero-area/hidden
items. `centered(parent, (w,h))` uses Qt odd-integer anchor centering.

## Primitive API

`primitives::FontSpec { family: String, pixel_size: f32, weight: i32 }` (Default:
Nerd Font, 16px, Bold), `Label::new(text, font, color)`, `Label::measure()` returning
`TextMetrics { width, height, advance }`, `Label::node(frame)`.
`rounded_rect(frame, radius, fill, Option<Border>) -> Node`,
`glyph_icon(frame, glyph, px, color) -> Node`, `clip_box(frame, children) -> Node`.
All constructors return ordinary Nodes; caller may adjust opacity/interactive/
children before inserting, then use Scene::edit for updates. Additional image,
vector and animation/container APIs follow in this README.

Requests R1–R5 implemented: metric advances floor at 1/64, pixel sizes truncate,
last-glyph overflow ceils, fractional anchors follow Qt integer-parity rule,
CJK fallback family is explicit. `foundations::{lighter,darker}(Color,f64)` uses
Qt's 16-bit HSV arithmetic; Palette now includes all R4 tokens as fields and
`icon_well_fill(active)`. Detailed stroke support is provided by the engine
`Content::StrokedPath` and Qt Canvas APIs.

## Containers (available now)

`containers::{Reveal, RevealTiming, RevealSample, Transition, GlassStyle,
glass_panel, group_pill, PopupPlacement, popup_position}`.
`Reveal::new(full_height)` starts fully shown (creation never animates).
`prepare_open(); play_open(now)` / `play_close(now)` / `reset()` /
`collapse_immediately()`. `advance(now) -> Option<Transition>` reports finish;
`values()` exposes height/opacity/offset; `is_running()` requests frame callbacks.
`RevealTiming` fields are milliseconds with QML defaults. `set_full_height` snaps
when idle. `Reveal::node(frame, children)` applies the working content slide;
`glass_panel(frame, &reveal, &style, children)` intentionally does not slide its
anchored content, matching AnimatedGlassPanel.qml:211-217. Clips are rectangular.
`group_pill` clamps radius 24 at paint size. `popup_position((W,H),(w,h),anchor,
placement,gap)` supports centred anchor, right-aligned, fixed-left and anchor-right.

R6–R8 engine changes are available: `Content::StrokedPath { path, fill,
stroke: StrokeStyle { width, color, cap, join } }` and `Content::Picture(Picture)`
are supported variants. Rect borders
replace the fill under their ring. `Node.opacity_mode` defaults to `PerNode`;
select `OpacityMode::Group` explicitly for an isolated opacity layer. Effects
remain isolated. These remove the need for leaf-fading and path-border stopgaps.

## Text pipeline update

`Label` now delegates exact layout/metrics to `hyprv_ui::text::measure` and has
`render_type: hyprv_ui::scene::RenderType` (Label default **NativeRendering**).
The raw engine `Text` default is **QtRendering**, matching QML; composite/kit
constructors must select NativeRendering explicitly for the 85 native Text items.
The engine now uses fontconfig/FreeType directly, never `fc-match` subprocesses.
Qt distance-field rendering is selectable per node. Reference samples from
`experiments/text` are now passing engine regression fixtures: native alpha matches exactly; distance-field alpha differs by at most 1/255 in the 18 supplied comparisons
(<0.2% ink error); affine raster parity remains unmeasured.

## Image/vector API

`ImageIcon` has `request() -> ImageTicket`, `complete(ticket, bytes, is_svg)`,
`ready: Property<bool>`, optional `asset`, `intrinsic`, `fit: ImageFit` and `tint`.
Late completion from a superseded ticket is ignored. `node(frame)`,
`with_fallback(frame, fallback)` and `rounded(frame, radius, placeholder, fallback)`
compose ordinary nodes. `ImageLoader` is a blocking-idle IO worker; `read(path,
callback)` returns bytes, which the caller delivers through its UI bridge before
`complete`. No decoding helper processes or polling. `resolve_icon(name, size)`
(device px) resolves like the live shell's Qt QIconLoader. See `icon_theme` below. `canvas_item(frame, |canvas|…)`
records a retained Skia Picture; rebuild it only when its vector inputs change.

## Theme icons (`icon_theme`)

A port of Qt 6's `QIconLoader`, as the original shell reaches it through Quickshell's
`image://icon/` provider.

- **Theme:** `hicolor`. qt6ct-kde fails to load under Qt 6.12, so Qt's generic theme
  falls back to it. Override with `HYPRV_ICON_THEME`.
- **Search paths:** XDG icon search paths, then a depth-first `Inherits` chain that
  always ends in hicolor, then the dash fallback (`a-b-c` → `a-b` → `a`), then the
  unthemed `pixmaps` fallback.
- **Size:** `entry_for_size` (exact directory match, else the closest; ties go to the
  larger directory; PNG before SVG).
- **Pixmap:** `adjust_size` downscales with Qt's `smooth_scale_down`
  (`qSmoothScaleImage` area averaging on premultiplied pixels) and never upscales.
- **Missing icons:** `missing_icon(size)` is Quickshell's black/#d900d8 checker.

Measured against the live shell (parity/fullscreen):
- discord: hicolor 256 PNG → 36 px, max channel diff 4;
- google-chrome: picks hicolor 32x32 at 36 px, drawn unscaled-then-stretched by the
  scene graph.

## Other containers and validation

`ExpandableSection::new(height, expanded)` exposes `reveal`, `set_expanded` and
`set_full_height(height, now)`. A zero-height open waits for a measured-height
event instead of polling. `MorphBox` owns width/height/radius Behaviors with
caller-selected duration/easing; `retarget` and `advance` take monotonic time.
`KeyedRepeater::attach(&mut parent)` creates a stable container;
`reconcile(&mut scene, keys, build, update)` preserves child IDs through reorder,
retains the container transform, and rejects duplicate keys.

`cargo test -p hyprv-widgets --no-default-features --test primitives` passes four
owned-API tests. It writes `target/widget-samples/glass-{dark,light}@1.25.png`;
both are 200×125 and exercise a partially revealed 160×100 logical glass panel.
These samples were inspected and are not final composite QML parity captures.
The same feature selection excludes co-owned controls/viz during focused checks;
normal workspace builds include both modules.


## Review and host integration notes

WIDGETS-1: disabling a grabbed slider cancels it and produces exactly one
`AdjustmentFinished`. After binding-driven `Control::update`, callers may drain
`take_canceled_adjustment()` immediately; otherwise the next pointer event drains
it. Re-enable starts a new gesture. Disabled switch/menu/segmented/pill/glyph/split
controls clear stale press state. WIDGETS-2: a switch's first bound palette snaps
to its initial state; subsequent toggle/theme changes use the declared Behavior.

The Qt-exact `flickable` module is shared with bar/island. Forward its full `Wheel`
source/phase/pixel data; see docs/reviews/scratch/flickable-implementation/README.md.
`foundations` re-exports the engine's Qt 6 half-size/centre helpers. Existing legacy
`hcenter` preserves its captured text placement; use `qt_centered` at actual Qt
anchor sites instead of replacing all centring arithmetic indiscriminately.
