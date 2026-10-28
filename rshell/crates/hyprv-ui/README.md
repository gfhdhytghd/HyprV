# hyprv-ui

Retained UI engine for HyprV. All scene, Wayland, EGL and Skia work stays on the UI
thread. Services send changes over a calloop channel. The engine installs **no
periodic timers** and requests a compositor frame only for damage or an active
animation. No engine code launches helper processes.

## Build and verify

From the rshell workspace:

```sh
cargo test -p hyprv-ui
cargo clippy -p hyprv-ui --all-targets -- -D warnings
cargo run -p hyprv-ui --example demo -- --offscreen demo.png
# Explicit opt-in: shows two tiny surfaces briefly, reserves no space, takes no focus.
WAYLAND_DISPLAY=wayland-1 cargo test -p hyprv-ui --test wayland -- --ignored
# A four-second demo; exits automatically, exclusive zone 0, keyboard None.
WAYLAND_DISPLAY=wayland-1 cargo run -p hyprv-ui --example demo -- --idle-test 4
```

Build dependencies: C++ toolchain, fontconfig/FreeType, EGL/GLES, libwayland-client,
libwayland-egl and libxkbcommon. Skia enables `gl`, `textlayout`, and `svg`. Its
build may fetch prebuilt Skia binaries; that happens at build time.

`scripts/check.sh` forwards to ordinary workspace Cargo with the shared root
`Cargo.lock` and `target/`. The former isolated-manifest workaround is retired.

## Scene and rendering API

```rust
use hyprv_ui::{animation::Color, geometry::Rect,
    scene::{Content, Fill, Node, Rectangle, Scene}, render::Renderer};

let root = Node::new(Rect::new(0., 0., 160., 48.),
    Content::Rect(Rectangle {
        fill: Fill::Solid(Color::rgba(0.1, 0.12, 0.16, 0.85)),
        radii: [12.; 4], border: None,
    }));
let id = root.id();
let mut scene = Scene::new(root);
scene.edit(id, |node| node.opacity = 0.8);
Renderer::new().png(&scene, 160., 48., 1.25, "panel.png")?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

`Node` retains a logical-pixel frame, affine/perspective Skia transform, opacity,
visibility, optional per-corner subtree clip, effects, content and children.
`NodeId` is stable and globally unique. Construct children with `Node::add`, then
mutate exclusively through `Scene::edit`. Read through `Scene::root`/`Node::find`.
An edit damages the union of old/new subtree bounds, transformed through ancestors
and expanded for ancestor effects and AA. Unrelated sibling bounds are excluded.
`Scene::invalidate` forces full damage; removing children works through an edit
of their parent. `Scene::hit_test` walks reverse paint order, inverts transforms,
and respects visibility, opacity and rounded subtree clips. Set `interactive` on
nodes which should receive pointer targets.

Content variants:

| Content | Parameters |
| --- | --- |
| `Rect(Rectangle)` | Solid/linear-gradient fill, four radii (TL/TR/BR/BL), inset border |
| `Text(Text)` | Families/fallback, pixel size, numeric weight, color, letter spacing, align, trailing ellipsis, per-node render type |
| `Image(skia::Image)` | Decoded image, scaled into the frame; crop/fit via parent clip/transform |
| `Svg(skia::svg::Dom)` | Retained parsed SVG, rendered at frame size |
| `Path` | Skia path, solid/gradient fill, optional stroke |
| `Group` | Container with the same transforms, clip, opacity and effects |

Text uses Skia/HarfBuzz for glyph selection and fontconfig/FreeType for masks.
Select `Text.render_type`: `NativeRendering` implements the supplied Qt native
recipe; the QML-compatible default `QtRendering` uses the ported Qt distance-field glyph cache and shader.
`text::measure` and `TextEngine` expose the same cached layout/metrics to widgets.
Skia typefaces and cached FreeType named instances borrow one mapped buffer per
resolved font file; different weights retain independent FreeType face state,
without mapping the same CJK file again for each weight.
The supplied native reference samples match alpha exactly at DPR 1.5/2; distance
fields differ by at most 1/255 alpha in the 18 supplied comparisons (<0.2% ink error).
The cache uses Qt's fixed-point distance kernels and complete path simplifier,
including intersection splitting; no Qt library is linked at runtime. Independent
Qt cache-byte fixtures cover Latin and CJK glyphs. Rotation, shear, reflection and
nonuniform transforms apply to glyph quads; only positive uniform axis-aligned
matrices use Qt's device-space snapping optimization. These paths have regression
coverage, while affine glyph raster parity against Qt remains unmeasured.
Wrapping/multiline, color emoji and arbitrary custom font assets are not yet
covered by this pipeline. `max_lines`, `hinting` and `subpixel` remain compatibility
fields; this renderer follows the specified single-line Qt mask configuration.
The crate re-exports Skia as `hyprv_ui::skia` for constructing assets and paths.

`Effects` provides Gaussian blur in **logical sigma**, alpha-based drop shadow in
logical sigma/offset/straight RGBA, and an optional `MultiEffect` using **Qt's
normalized parameters**. `MultiEffect` implements blurEnabled/blur/blurMax/
blurMultiplier, auto padding, shadowEnabled/shadowBlur/shadowScale, offset, color
and opacity. Its SkSL shaders retain Qt's four downsample taps, 3–5 pyramid
levels, six normalized LOD weights and shadow composition formula. These shaders
run on both raster and GL Skia canvases. The result is retained until the node or
one of its descendants changes. `Scene::set_position` reuses a node's effect
texture while invalidating affected ancestor effects; `Scene::edit` conservatively
invalidates the edited subtree's caches.

For example, `AnimatedGlassPanel.qml:190-197` sets `shadowBlur: 10/64` while
leaving MultiEffect's actual `blurMax` at its default 32; `shadowLayer.blurMax` is
a separate QML property. Use `MultiEffect { shadow_enabled: true, shadow_blur:
10./64., ..Default::default() }` directly. Never convert this normalized value to
Gaussian sigma. Qt uses intermediate RGBA textures: Skia's image quantization and
fractional texture rounding still require QML reference captures before claiming
pixel parity. Non-blur/shadow MultiEffect features (color adjustment and arbitrary
alpha masks) are outside this engine task; rounded subtree masks use node clips.
Hyprland handles compositor background blur using namespace `hyprv-quickshell`.

For an explicit QML `layer.textureSize`, set `MultiEffect.source_texture_size`
to its physical pixel dimensions. Auto padding expands the captured logical
region while retaining that source texture size; the final effect still renders
at padded logical extent times output scale. `None` preserves the existing
capture sizing/transform. Zero, oversized or excessive-area textures are rejected
with `RenderError::InvalidSize`. Edit the node through `Scene::edit` when changing
the size so retained effect caches are invalidated.

`Renderer::{raster,image,png}` select their offscreen transport through
`HYPRV_PARITY_BACKEND=cpu|gl` (default CPU), or the per-renderer
`set_offscreen_backend(Backend)` override. GL uses `offscreen::GpuTarget`, the
production Wayland EGL/RGBA8 path on an unmapped roleless surface. Supply a private
headless compositor for tests; the engine starts no compositor or helper process.
Invalid backend values and failed GL setup/readback return errors. `raster`
returns a CPU premultiplied readback surface, `image` retains the established PNG
transport used by component parity tests, and `png` writes the retained target's
PNG directly. `offscreen::clear_target` releases the calling thread's target;
changing display endpoints recreates it and invalidates renderer context caches.
`parity/gpu` is a compatibility re-export, with no second renderer implementation.
Transport selection is independent of `RectMode`, per-leaf `NativeTextMode` and
inherited `native_materials`; it does not change their validated defaults.

`text::measure_wrapped(&Text, width)` caches SkParagraph line metrics and font
collections (512 entries / 1 MiB keys, 16 default families). It preserves existing
paragraph geometry while avoiding repeated Fontconfig initialization. Clear it
with `text::clear_measurement_caches` or `Renderer::clear_caches`.

`Renderer::png` writes PNG.
`Renderer::paint` accepts a damage rectangle and assumes the destination retains
undamaged pixels. It returns a `Result` for allocation/filter failures. Each EGL window keeps a persistent GPU render target, repaints
up to 16 coalesced dirty regions, and copies the result to the current EGL back
buffer. EGL buffer age repairs the union of current damage and damage since that
buffer was last used (eight frames retained); unknown age, resize or unavailable
history copies the full retained target. `EGL_KHR_partial_update` supplies a damage
hint when available. Swap uses KHR/EXT swap-with-damage when advertised and falls
back to a complete swap. Each region is outward-snapped to device pixels before
painting, preserving translucent edge coverage.
One EGL context and one Skia direct context serve every window. External users of
`Renderer` must use `clear_caches` while their GPU context is current before
destroying that context; `Engine` performs this teardown automatically. EGL/Skia objects
are destroyed while the context is current, before the native Wayland surface.

## Wayland and service integration

Implement `UiHandler`, then `Application::connect(handler)`. Add a scene with
`app.engine.add_surface(LayerConfig, scene)` and call `app.run()`. `LayerConfig`
controls output, namespace, layer, anchors, margins, size, exclusive zone,
keyboard interactivity and input passthrough. Defaults reserve **no space** and
take **no keyboard focus**. `LayerConfig::popup` selects an overlay layer with
on-demand keyboard focus; it is a popup-like layer surface, without an xdg_popup
grab or implicit outside-click dismissal. Advanced changes are available through
`Engine::layer_surface` and SCTK's `WaylandSurface::commit`.

`UiHandler::output_added` returns scenes for each discovered/hotplugged output;
those layers are automatically output-bound. Output removal closes associated
surfaces and calls `output_removed`. SCTK supplies xdg-output metadata. Explicit
layers can also select `LayerConfig::output` from `Engine::outputs`.

`UiHandler::input` receives pointer enter/leave/motion/button/axis, hover changes,
clicks, keyboard focus, keys (including XKB text), modifiers and resize/scale
changes. Axis events preserve smooth pixel delta, discrete steps, source and stop
flags. Click requires matching press/release targets; hit tests respect stacking.
Events preserve their seat through the SCTK pointer/keyboard objects or explicit
keyboard seat. Return `Response` to enable animation, close the layer or change
cursor shape. `Engine::set_cursor` supports the cursor-shape-v1 protocol; it is a
no-op on compositors without that optional protocol. Keyboard events represent
physical key presses/releases; automatic key repetition is left to consumers.

Fractional-scale-v1 preferred scale is divided by 120. Buffers allocate rounded-up
physical extents, then render geometry/text directly at that scale. Viewporter
uses an explicit source rectangle to prevent rounding slack from stretching the
contents. Integer wl_surface scale is the fallback when these optional protocols
are absent. Resize/scale changes invalidate the scene.

`UiHandler::frame` receives `FrameInfo` and returns true only while another frame
is needed. `time` unwraps compositor callback milliseconds into a monotonic app
timeline. Actual presentation feedback and refresh duration are supplied in
`presented` and `refresh`; `Engine::presentation_clock` identifies that clock.
Presentation timestamps are asynchronous and in a separate domain from app time;
consumers must not subtract them directly. The engine never requests a continuous
stream of callbacks for a stationary scene.

`Application::handle()` is Send; send a closure capturing a typed service diff:

```rust,ignore
handle.send(move |engine| {
    if let Some(scene) = engine.scene(surface_id) {
        scene.edit(label_id, |node| {
            if let Content::Text(text) = &mut node.content {
                text.text = new_label;
            }
        });
    }
})?;
```

This wakes the calloop channel and schedules damage automatically. Direct UI-thread
edits outside handlers can call `Engine::request_redraw`; animations started there
call `Engine::animate(surface_id)`. `Application::event_loop_handle` accepts the
core actor bridge or additional event sources without polling. `dispatch` is
available for embedding/testing; production `run` blocks without a timeout.

## Reactive properties and explicit layout

`Property<T>::set` compares values and synchronously notifies only on changes.
`get` records dependencies while an `Effect` or binding executes. Dependencies
are re-collected on every run, so conditional bindings unsubscribe correctly.
Keep the returned `Effect` alive; dropping it disconnects the binding. Binding
cycles panic rather than spin. No global timer, thread or polling scheduler exists.

```rust
use hyprv_ui::reactive::Property;
let width = Property::new(120.0_f64);
let double_width = Property::new(0.0_f64);
let _binding = double_width.bind({ let width = width.clone(); move || width.get()*2. });
width.set(160.);
assert_eq!(double_width.get(), 320.);
```

`linear_layout` positions measured sizes in a row/column with explicit spacing.
`Anchors::resolve` supports left/right/top/bottom and center offsets;
`Margins::inset` computes content bounds. These functions return geometry and do
not schedule layout passes or create implicit scene dependencies.

## Animations and Qt reference checks

`Easing::new(Family, Direction)` covers Quad/Cubic/Quart/Quint/Sine/Expo/Circ/
Back/Elastic/Bounce, including In/Out/InOut/OutIn. `Easing::Curve` additionally
exposes Qt's overshoot/amplitude/period. `BezierSpline` accepts chained cubic
segments from (0,0), ending at (1,1); segment x must increase monotonically.
The solver retains Qt's approximate Cardano/cube-root calculations, not an
iterative mathematical inverse. Invalid/empty final endpoints fall back to linear.

`Behavior<f64>` and `Behavior<Color>` sample an interrupted animation before
retargeting, preserve continuity, and do not restart for an unchanged target.
Call `advance(frame.time)` and apply a scene edit only if the sampled value changed.
`Color` interpolates straight RGBA, without gamma conversion or premultiplication.
`Spring` implements Qt Quick's fixed 16ms semi-implicit update, spring/damping/mass,
epsilon termination, velocity cap/velocity-only mode, modulus and velocity-preserving
retargets. Configure positive mass, nonnegative parameters, and positive modulus.
Animation structs contain no clocks or timer sources; the caller supplies timestamps.

Source behavior motivating these APIs:

- `BarScreenState.qml:26-37`: Bezier NumberAnimation Behaviors on reveal/hide.
- `BarMotion.qml:9`: overshooting control points `[0.1,0.9,0.1,1.05,1,1]`.
- `hypr-comp/moon.qml:1444`: OutBack with explicitly configured overshoot 2.
- `AnimatedGlassPanel.qml:190-197`: normalized MultiEffect shadow parameters.
- `DynamicIsland.qml:436-445`: Nerd Font, numeric pixel size/weight, native text.

Numerical sources: [Qt easing formulas](https://github.com/qt/qtbase/blob/v6.8.3/src/3rdparty/easing/easing.cpp),
[Qt BezierEase](https://github.com/qt/qtbase/blob/v6.12.0/src/corelib/tools/qeasingcurve.cpp),
[Qt SpringAnimation](https://github.com/qt/qtdeclarative/blob/v6.8.3/src/quick/util/qquickspringanimation.cpp).
`tests/qt-oracle.cpp` and `tests/qt-special-oracle.cpp` regenerate **450 fixtures**
using the installed Qt 6.12.0 QEasingCurve. Tests compare values at 1e-12 tolerance,
including custom overshoot, amplitude/period, the actual bar curve and a multisegment
spline. Spring tests cover the first integration steps, settling and modular
retargeting; these are source-algorithm checks, not independent Qt runtime fixtures.

## Validation evidence and limits

See [VALIDATION.md](VALIDATION.md) for commands and measurements. Engine primitives,
CPU PNG export, live EGL rendering, two-window context reuse, service-triggered
redraw and idle behavior are exercised. Application component pixel parity is
**PENDING** until live QML baselines and complete shell scenes exist. Qt
MultiEffect output pixel matching, fonts beyond the supplied text references, physical input, output hotplug,
and independently changing output scales need dedicated acceptance captures.

Licensing: engine code is MIT except the Qt-derived numerical Bezier solver
(`src/qt_bezier.rs` and `src/effects.rs`, LGPL-3.0-only) and Penner easing equations (BSD-3-Clause).
See `THIRD_PARTY_LICENSES` and `licenses/` for the retained notices/license texts.


## Shared engine primitives

`Content::Shared(Rc<Node>)` retains an immutable subtree across host builds.
Rendering, bounds, read-only lookup and hit testing recurse into it. `Scene::edit`
and `set_position` refuse its descendants; replace the Rc to update content and
move the wrapper for container animations. This preserves inner node identities
and effect caches without cloning strings or rebuilding glyph/layout nodes.

`Node.image_sampling` selects `Nearest`, `Linear` (default), or `Mipmap`;
`ImageSampling::qt(smooth, mipmap)` maps QML flags. Device-centre sampling and
box-filter mip levels preserve the full decoded source. Translation-normalized
sample caches and affine native mipmaps are bounded and released with the renderer.

`canvas::CanvasPath` exposes Qt Canvas arc/line/quadratic/cubic geometry and
`stroke_node` / `fill_node`. Solid paths use the Qt QStroker cubic offset,
26.6 outline mapper, analytic QRasterizer line AA and gray raster with Qt's
integer premultiplied blending. Arc angles are radians; equal endpoints draw
nothing. `CanvasPaint { path, fill, stroke }` describes one paint;
`CanvasBatch { operations: Vec<CanvasPaint> }.node(frame)` keeps ordered draws
in one backing image. Use a batch for a ring track plus progress: independently
blended GL textures can round differently from multiple QPainter draws.

84 independent Qt6.12 QPainter coverage goldens are byte-exact at DPR1/1.25/1.5/2
on CPU and production EGL RGBA8 (private Weston): round/square/flat caps, cubic
arcs/circles/curves, diagonal lines, round polyline joins, thin straight grid and
overlapping strokes. Tests and oracle provenance: `tests/data/canvas-strokes/README.md`.
This proves the solid arc/line subset used by the shell; arbitrary cosmetic
curves, dash styles and transformed pens are not covered. `CanvasPaint::gradient: Option<VerticalGradient>`
adds ordered Qt-style vertical gradient fills; the 84 stroke goldens prove the solid
stroke subset, not arbitrary gradient parity.
Raster caches are bounded to256 entries/16MiB including keys, and released by
`Renderer::clear_caches`. No Qt runtime linkage is introduced.

`geometry::{qt_center_half, centered_position, qt_centered}` and
`Anchors::resolve_qt` implement Qt's independent half-size rounding. Existing
legacy helpers are retained; migrate actual Qt anchor sites explicitly.

Native input negotiates pointer v9 where advertised while keeping SCTK keyboard
handling. `InputEvent::Scroll` / `scroll::ScrollEvent` carry Qt-style pixel and
angle deltas, phases, synthesis and inversion. Conversion follows Qt 6.12 Wayland:
finger starts with a zero-delta Begin, fractional pixels accumulate rounding error,
axis_stop sends End, value120 preserves high-resolution wheel increments. Direction
inversion is metadata; deltas are not inverted twice. Older compositor versions
fall back to the events they advertise. No synthetic momentum phase is inferred.

`LayerConfig::input_only` provides a transparent 1×1 backing buffer with full logical
input coordinates via viewporter. `Engine::take_scene` temporarily detaches the real
retained scene for host reconciliation; restore it or close the surface before
returning. `dump_surfaces` returns per-surface PNG metadata from retained GPU targets.

Set a native text leaf's `Node.native_text_mode = NativeTextMode::QtGl` to use
Qt's A8 mask fragment operation in `EglWindow::draw` directly
on the registered retained GL target. The default `NativeTextMode::Skia` preserves
existing rendering; the bar opts its text leaves in. Shaping, FreeType masks and device snapping
are unchanged. One EGL context owns the shader, buffers and an LRU mask texture
cache (512 entries / 8 MiB); teardown deletes them while that context is current.
Skia commands are submitted before the native batch and its state cache is reset
afterward. No CPU wait, `glFinish`, runtime Qt library or subprocess is involved.

Native text includes translucent color and inherited opacity: QQuickText's
QRgb8 material conversion is covered by 64 exact independent Qt golden cells.
Positive uniform device transforms and hard rectangular clips are supported;
antialiased/rounded clips, affine transforms and arbitrary caller-owned canvases
preserve Skia. Independently allocated MultiEffect source targets register their
own framebuffer; parent save-layer targets are never mistaken for the root.

`Node.native_materials: Option<bool>` independently opts a component into Qt GL
rectangle/image/effect materials. `None` inherits with base false. Shared subtree
boundaries reset the scope; their root must explicitly opt in. This mode uses the
same canonical `qt_rect` mesh cache, context-owned VBOs, RGBA texture/mipmap cache
and Qt MultiEffect shaders. The bar's view, popup and corner roots opt in. Text
mode alone does not change other materials. Unsupported cases keep Skia. Effect
cache identity includes the active material mode, so switching it cannot reuse a
source captured by another backend.
With CPU transport, these opt-ins retain the established Skia fallback.
`Renderer::clear_caches` clears renderer caches and advances a native epoch; the
next draw flushes stale context caches, including a scene with no native nodes.
`SharedEgl::clear_caches` / `Engine::clear_render_caches` make the owning context
current and release native textures/meshes immediately between frames, restoring
previous EGL bindings. Optional image/mask upload failures log once and fall back
to a context-aware CPU copy/Skia path; unrecoverable native failures return
`RenderError::NativeGl`. `HYPRV_NATIVE_GL_DISABLE=1` is a diagnostic A/B switch for the original Skia
native-text path. The ignored `tests/native_gl.rs` test must be explicitly run
with `HYPRV_PARITY_BACKEND=gl` on a private Wayland compositor; it checks independent
Qt goldens, fractional scales, clipping, opacity and layer fallback.

Native-material precision: `qt_rect::Mesh.indices` retains the canonical Qt triangle strip (including degenerate connectors); CPU triangles remain available. GPU rectangle/image vertices use the current scene/clip/source batch coordinate basis. Images choose the full-opacity shader independently of opaque-texture blending, and supply explicit position/UV vertices. Native effect sources preserve Qt top-left texture orientation; final effect quads preserve QSG triangle order and logical vertex extents. Independent full/faded, DPR1/2, clip/image/layer goldens live in `tests/fixtures/batch-materials`.

Font performance: the UI thread shares one system font manager and a bounded
resolved-typeface database between measurement and rendering engines. Variable
named weights share a base stream keyed by file/TTC index (16 entries /64MiB),
then apply their original design coordinates. Native metrics and glyph masks reuse
opened FreeType faces with explicit size/load flags; raster transforms reset on
exit so fractional masks cannot contaminate later metrics. `text::prepare_fonts`
prepares supplied native and paragraph styles before user input; shell startup
uses500/600/700 for its latent battery popup. The font database is application
state; renderer glyph/layout/effect caches still clear through the existing API.

Opt-in `HYPRV_UI_PROFILE=1` records bounded UI-thread wall-time scopes/events in
memory, including thread CPU time alongside wall time. No timer or per-frame file write is added. `diagnostics::{clear,csv}`
expose the collection; `HYPRV_UI_PROFILE_OUTPUT` writes it on clean Engine drop.
Default rendering keeps profiling disabled. These scopes are inclusive; do not
sum nested entries as exclusive CPU time. See docs/verify/perf-followup.md for
native input→GPU-buffer timing and controlled-quiet release measurements.

`Application::connect` prepares the shared production EGL material/effect
pipelines on a small private framebuffer and waits for startup GPU work before
accepting input. It creates no mapped warmup surface or timer; later popup
rendering reuses that context's programs. Profiling exposes `graphics.prepare`,
`effect.{paint,apply,sksl_compile,gl_compile}`. Cross-thread diagnostic counters
(`diagnostics::{count,count_by}`) are active only with HYPRV_UI_PROFILE, and
are included in `clear`/`csv`; normal operation creates no diagnostic wakeups.

`UiHandler::update_after_frame` defaults to true for compatibility. A host that
already reconciled a static frame can return false to skip its update callback
after submission; protocol scheduling/flushing and pending deadlines remain
active. hyprv-shell opts out only while settled, preserving animation, pending
input/provider/output updates and fixture clock behavior.

`UiHandler::immediate_static_frames` is an additional opt-in (default false).
When the handler needs no post-frame update and a damaged surface is settled,
configured and has no pending callback, the engine paints it directly. This
avoids an empty commit/frame-callback round trip for static data. Animated
surfaces retain compositor pacing. Immediate paints use monotonic time without
moving the saved callback clock anchor, so a subsequent animation resumes at
the correct time. The private nested rotation suite also compares eight native
EGL captures byte for byte against callback scheduling, checks callback counts
8→0, and resumes an animation after the static edits.

When opting out of post-frame updates, retain an armed absolute deadline until
`update` consumes it. Computing a fresh `now + period` from a deadline getter can
otherwise postpone due work. The shell caches its consumed/next deadline, and
the Wi-Fi500ms blink boundary regression covers this case.

The UI-thread shared font database is exposed as `text::font_manager()` for
component picture/text backends, avoiding a fresh system font scan per face.
Production GL3 final copies use a framebuffer blit. ES2 retains Skia's copy;
`HYPRV_SKIA_WINDOW_COPY=1` forces that control path. Explicit
`HYPRV_VERIFY_FRAME_COPY=1` reads both actual EGL back-buffer RGBA8 results and
requires byte equality; this diagnostic is intentionally unsuitable for timing.
Startup preparation includes tiny A8-picture and RGBA-image paths, but does not
promise that all component-specific first-use work has been warmed.

`capture::qt_png(&Image)` serializes a completed capture with Qt QImage's
RGBA8 premultiplied-to-straight conversion, including SSE reciprocal/Newton
half ties. Use it after the final crop when comparing Qt PNG references.
It does not alter scene rendering or compositor buffers. An independent Qt
oracle covers all32,896 valid channel/alpha pairs, and tests cover intermediate
Skia PNG decode/crop/re-encode transport. `EglWindow` keeps its original encoder;
reference adapters can opt into the shared converter at their final encode.

`Engine::prepare_scene` renders a built hidden component on the existing EGL
context during startup/output initialization. It creates no Wayland window or
sampling lease, completes GPU submission, and purges the temporary scratch target.
Call only for explicit initialization/invalidation, never on an idle timer.
Closing an EGL window also purges unlocked Skia scratch targets on the shared
current context. Live output targets, glyph atlases and shader programs remain
resident; this does not flush renderer/text caches or add idle cleanup polling.

For animated QtQuick rectangles, optional `Node.qt_rect_size: Option<(f64,f64)>`
retains the original QRectF size until corner centers are converted to GPU floats.
Its f32 projection must match `Node.frame`; default `None` preserves existing
callers. The existing bounded rectangle-mesh cache includes those source-size bits.
The renderer also preserves direct clip-relative matrices and selects merged versus
full-matrix smooth-color geometry per compatible batch, including clip/opacity/
overlap barriers. These arithmetic changes add no shader program or draw reordering.

For an explicit `MultiEffect::source_texture_size`, native merged rectangles use
the layer’s original padded logical orthographic projection. Physical texture size
still sets smoothcolor pixelSize. This preserves Qt’s float operation order instead
of refactoring projection through the rounded texture dimensions. Metadata is
scoped to the active offscreen target; no public API or cache budget changes.

`text::font_mapping(family, weight)` returns the engine's fontconfig-selected file,
full FreeType face index (including named-instance bits), and shared immutable
`&'static [u8]`. Call on the UI thread. Components can pass `mapping.bytes` and
`mapping.index` to FreeType `new_memory_face2` without copying or mapping a CJK
font once per weight. The mapping has process lifetime, matching direct Skia streams.

Native GL context construction allocates shared geometry handles without compiling
shaders. Mask, image, rectangle and each individual effect variant compile on first
use; production preparation exercises needed reveal paths before input. EGL window
close and target shrink release unused Skia scratch targets without clearing live
surfaces, glyph atlases or shader programs. Preparation also purges its temporary
targets. Explicit `SharedEgl::clear_caches` remains a separate full cache flush.
`text::mapped_font(family, weight)` additionally returns a Skia typeface with the
selected named instance's variations applied, backed by those same mapped bytes.
Panels use this accessor for both shaping and FreeType rasterization. The startup
preparation regression checks that the masked popup compose material was actually
executed; transparent preparation content would otherwise be culled.

Scaled clip rectangle precision: AA rectangles beneath a clip with an inherited
scale retain their full shader matrix. A layout's snapped clip origin and opposite
child displacement must not be folded separately into float AA vertices. Ordinary
DPR-only clips retain the merged path. The original-QML QuickAdjust edge goldens
in `tests/quick-adjust/` exercise this at the full production viewport on EGL/GL.


Debug frame audit (`debug_assertions` builds only): opt in with
`HYPRV_GPU_SELF_CHECK=1` to read every retained RGBA8 target and actual EGL back
buffer before swap, normalize row origin and require byte equality. A mismatch
returns `EglError::Init` with both checksums, size and first differing byte; the
application's existing graphics-error policy exits cleanly. The check also queries
advertised GL robustness reset status. `HYPRV_GPU_SELF_CHECK_LOG=1` emits per-frame
checksum receipts. These synchronous reads change timing and are off by default;
release builds contain none of this audit or fault-injection code.

Private debug diagnostics require `HYPRV_TEST_PRIVATE_WAYLAND=1`:
`SharedEgl::{debug_simulate_context_loss,debug_poison_gl_state,debug_memory_pressure,
debug_gl_capabilities}` are available only in debug builds. Reset injection is a
sticky simulated `EGL_CONTEXT_LOST`, requiring owner/window destruction and a fresh
context; it never resets the device. `HYPRV_GPU_ROBUST_CONTEXT=1` requests actual
robust access and reset notifications when the EGL extension is present.
`HYPRV_GPU_STRESS_PURGE_AFTER_PAINT=1` evicts unused resources between queued paint
and presentation. `HYPRV_GPU_SELF_CHECK_INJECT=1` changes one back-buffer pixel as
a negative control; these two injections also require the private-session marker.
Run `tests/run-renderer-regressions.sh` for the real back-buffer fault detection
and clean-frame recovery regression. The `lifecycle_stress` example and
`../hyprv-ui-bar/tools/stress_lifecycle.py` own private compositors and perform
CPU-oracle, surface/context lifetime and output mutation checks; no daemon/timer
is introduced into normal runtime.


`EglWindow::draw` recreates its EGL drawable on physical-size changes, so the
first buffer after resize already has the requested extent. Resizing only the
`wl_egl_window` can leave a previously acquired back buffer at the old size;
re-binding alone does not guarantee freshness. The host already retires whole
windows on configure/rotation. This direct-user safeguard retains the shared GL
context/caches and forces full damage on the new retained target. It adds no
GL finish/fence. Failed drawable recreation leaves a nullable handle so teardown
cannot destroy the old EGL surface twice. The frame-audit seven-transition test
covers mixed-axis grow/shrink and fractional scales on NVIDIA and Mesa.
Private failures can optionally dump the two premultiplied RGBA byte arrays with
`HYPRV_GPU_SELF_CHECK_DUMP=/private/path` (also requires the private-session marker).


Stats text rendering shares bounded shaped runs between measurement, extent and
paint (2048 entries / 4 MiB estimated keys and glyphs); non-elided width constraints
share one run. FreeType design advances and hinted right edges each retain up to
8192 entries. Native integer-position A8 glyphs use a lossless nearest-sampled GPU
atlas: at most four 512×512 R8 pages (1 MiB total on GL3+, with a lossless alpha
swizzle; GL2 retains RGBA8/4 MiB), plus a streaming vertex buffer
(normally 256 KiB; at most 1 MiB). New masks upload once; warmed masks retain their
texture coordinates. Unsupported transforms, oversized masks and atlas exhaustion
use the original mask path. These caches clear with renderer teardown/cache clear.
`HYPRV_GLYPH_ATLAS_DISABLE=1` is a diagnostic comparison switch.

`examples/paced_submit.rs` is a private-Wayland-only 1 Hz one-pixel submission
control; it requires `HYPRV_TEST_PRIVATE_WAYLAND=1`. It measures backend overhead
without services or text/layout. See `docs/verify/ui-tick.md` for timing evidence.


`Node.native_batch_root` preserves a promoted QSG transform coordinate basis
without a clip, layer, texture or copy. It defaults to false. The scoped batch
guard restores its parent basis on exit; explicit rectangular clips already
establish their own basis. Large native bar tray rows use this to match original
window AA rounding. A compact original-window golden includes a failing unscoped
negative control; the scope does not change world geometry.


Static `LayerConfig::full_redraw` windows retire their EGL drawable and native
window after the final submitted frame. Their layer-shell role, viewport, input
region and compositor-owned last buffer stay mapped; there is no null attach,
extra commit, readback, timer or changed corner atlas. Real damage recreates the
native drawable in the same shared EGL/Skia context. GPU diagnostics capture a
full-redraw scene into a temporary offscreen RGBA8 target, so capture never recreates
an EGL stream merely to read a mapped front buffer. Ordinary damage-retained views
keep their targets and buffer-age repair history.

Each native image/glyph records the windows that used it. Window retirement drops
only that owner's references and deletes orphan textures and empty trailing glyph
pages; surviving outputs and warmed material programs remain usable without upload.
The Skia unlocked resource budget is 8 MiB (locked live targets are additional).
CPU mip sources, Canvas rasters, Qt-rect coverage and Qt-rect rasters each have a
2 MiB share (8 MiB combined); oversized results render uncached. Surface close also
prunes effect/bounds entries against all surviving scenes, including shared trees.
No eviction/purge maintenance timer is installed. See
`docs/spec/laptop-minimal-resources.md` for measurement and parity boundaries.

Mapped layer sizes are published with the next buffer, including stretched
(zero) axes. Use `Engine::configure_surface` rather than raw layer `set_size` and
`commit`; input-region, focus and frame-callback commits can remain independent.
`python3 crates/hyprv-ui/tests/layer-commits.py --binary target/debug/hyprv-shell
--output target/layer-commits-new-run` runs private compositor wire regressions
(set `HYPRV_ASSET_ROOT` to staged assets). With no arguments it runs parser tests.
Audit and measured traces: [layer-size-commits.md](../../docs/verify/layer-size-commits.md).
