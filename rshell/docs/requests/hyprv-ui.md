# Owner requests: hyprv-ui

- [UI-1] [P2] Stroke extents are missing from damage bounds; verified failure and suggested fix: [review](../reviews/hyprv-ui.md#ui-1).
- [UI-2] [P2] Replacing a node through Scene::edit panics; verified failure and suggested fix: [review](../reviews/hyprv-ui.md#ui-2).
- [UI-3] [P2] Dropped reactive bindings leave allocations pinned; verified failure and suggested fix: [review](../reviews/hyprv-ui.md#ui-3).
- [UI-4] [P2] Text painter discards parent affine transforms; verified failure and suggested fix: [review](../reviews/hyprv-ui.md#ui-4).

## BAR-RECT-1 — Qt Quick rectangle parity implementation plan (2026-10-23)

Owner coordination: user explicitly assigned the bar agent the new
`crates/hyprv-ui/src/qt_rect.rs`; **engine retains ownership of scene.rs,
render.rs, lib.rs and all integration changes**. I will not edit those files.

Implement a self-contained Rust geometry/raster module adapted from
`qtdeclarative/src/quick/scenegraph/qsgbasicinternalrectanglenode.cpp` and
`shaders_ng/smoothcolor.vert/.frag`. Preserve Qt's premultiplied byte vertex
colors, radius tessellation, border strips and half-device-pixel vertex offset
shader semantics. Rasterize the resulting vertex-color triangles without Skia
edge AA (Qt geometry provides its own AA), using the canvas transform to resolve
pixel-space offsets. Support the bar's solid rounded rectangles and borders
first; gradients/unsupported transforms must return an explicit unsupported
result rather than silently approximate a claimed Qt rendering mode.

Proposed host interface in this new module: `RectMode::{Skia, QtQuick}` plus
`draw(canvas, bounds, rectangle, opacity) -> bool`, where false means unsupported
and lets the engine retain its existing Skia path. Engine can expose a Renderer
mode setter and call this before the old `Content::Rect` branch. Avoid adding a
required Rectangle field: that would break parallel widget/panel struct literals.
The bar parity harness will explicitly select QtQuick after engine wires it.

Validation: rank frozen before-image residuals by rectangle-edge AA, SVG/image,
text, and compositing/remaining regions; record per-image maximum channel error
and painted-pixel MAE. Compare every animation sample individually against the
new max<=8, painted-MAE<=0.5 gate. Preserve before inputs/outputs, exclude only
separately owned island contents, and retain strict tolerance-zero stretch data.
Use real QML captures to validate tessellation and shader interpolation, not a
test that copies the implementation. SVG changes stay inside the bar crate
unless separately coordinated. No source QML/reference mutation or live reload.

BAR-RECT-1 implementation is now present in `src/qt_rect.rs` (not yet wired).
Public `tessellate`, `device_vertex`, `rasterize`, `draw` and `RectMode` expose
uniform solid rounded/straight fills and borders, affine shader-offset semantics,
and premultiplied CPU raster output. Engine may wire the module/mode setter now;
bar will validate through direct inclusion until the engine integration lands.
The initial implementation deliberately rejects asymmetric corners and gradients.

BAR-RECT-1 measured first result: baseline painted MAE is down to 0.042/255
(before overall suite 0.5265). Geometry-AA module compiles through bar's direct
include; integration in engine files remains engine-owned. Separating fixed-point
triangle coverage from floating-point varying interpolation is under validation.
User clarified SVG runtime must stay Qt-free: fixed icons are baked offline under
bar/tools into immutable size/scale/color PNGs; unknown SVGs retain a Rust path.

BAR-RECT-1 follow-up evidence: after Qt geometry and offline fixed-SVG baking,
overall painted MAE is 0.04336/255 (before 0.52650). Native text has max<=8 in
all category masks; remaining gate failures are sparse low-alpha rectangle
fringe RGB, filtered image pixels and a reference-fixture hover mismatch. The
Qt raster path now separates GL coverage grid from varying interpolation.

Reference issue: the four `network-wifi-hover` PNGs are byte-identical in pixel
data to their `network-wifi-75-secure` counterparts; the recorded hover is not
visible. Production hover behavior is retained. Adapter comparison must replay
the observed non-hover state and explicitly label true-hover reference coverage
unavailable, rather than remove the hover from the actual UI implementation.

## From parity QA — 2026-10-23 04:10 (verify pass 1, all three UI crates)

Source: `python3 parity/verify/verify.py` → `docs/verify/parity.md`; tightened gate in
`docs/ARCHITECTURE.md` (non-DF images exact). Attribution of failing pixels (> 8/255) over all
static (non-animation) failures, from the reference regions sidecars
(`parity/reference/<c>/regions/*.json`): island — shapes 40 %, images 29 %, Canvas 22 %, text
9 % (all explained by anchor rounding, see `hyprv-ui-island.md` ISL-PQ3); bar — shapes 87 %,
canvas 6 %, images 5 %, text 0.3 %. **Native text already renders pixel-exact** wherever glyph
positions agree — the remaining work is geometry/raster paths, in this order:

- [ENG-PQ1] [P1] **Qt Quick rectangle/border vertex AA** (BAR-RECT-1 above): the largest class
  in island and bar (`rect AA` 214 + 106 images; max 38–72/255 at pill and card edges).
  `hyprv-ui-panels/src/qrect.rs` already shows the payoff (control-panel edge residue max 24–38,
  interiors exact) — consider promoting it into the engine so island and bar share one
  implementation.
- [ENG-PQ2] [P1] **Image sampling modes like Qt Quick**: `smooth: false` nearest,
  `smooth: true` bilinear, `mipmap: true` trilinear over a box-filtered mip chain
  (glGenerateMipmap), sampled from the full-resolution texture when `sourceSize` is unset.
  Evidence: island agent logos (816 px source → 34/46/100 device px) and album art — Qt is
  visibly softer; 29 % of island failing pixels; `agent-expanded-9-scrolled--top--dark@2.png`
  max 166.
- [ENG-PQ3] [P2] **SVG rasterisation of Fluent/quickshell icons** as QtSvg does it at
  `sourceSize × dpr` (no AA differences from resvg/skia paths, `PreserveAspectFit`), and
  QIcon theme lookup for tray icons by *device* size (see `hyprv-ui-bar.md` BAR-PQ2).
  Panels Wi-Fi list icons are the whole failure (max 30).
- [ENG-PQ4] [P2] **Canvas 2D (QPainter raster) parity** for `arc` with round caps and
  `quadraticCurveTo` rounded bars: island progress ring + spectrum (22 % of island failing
  pixels), bar corner shades (already within 1–2/255 — the arc path there is fine; the ring's
  round caps and 3 px stroke are where it diverges).
- [ENG-PQ5] [P3] **`alignWhenCentered`**: Qt rounds centre-anchored positions to whole logical
  px. If anchors/centering helpers live in hyprv-ui/hyprv-widgets, implement it there once
  (island ISL-PQ3 shows the effect: 1 px glyph shifts at @1).

### BAR-RECT-1: integration-ready update and measured dependencies

The bar's new `qt_rect.rs` compiles through direct inclusion. Qt private-node
oracle now proves exact vertex/color arrays AND triangle sets for pill, circle
32×32, narrow 18×38, and border cases. Fixed a real coverage crack: raster scan
bounds must use the snapped coverage vertices; using unsnapped bounds skipped a
whole pixel row when a circle edge snapped onto a sample center. Please expose
`qt_rect` and call `qt_rect::draw` before the existing rectangle renderer when
`RectMode::QtQuick` is selected; I still have not edited engine integration files.

Bar fixed SVGs are offline QtSvg PNGs. `hyprv-ui-bar::images::resolve` now provides
pure-memory device-center bilinear/trilinear sampling, including a box mip chain;
this closes ENG-PQ2 for the bar's image leaves without touching engine files.
Baseline @1/@2 in both themes now passes the current DF gate: max 3, painted mean
0.02219/255 overall (rectangle/text/compositing masks each max 2). API and cache
integration are in the bar crate; no runtime Qt linkage. A future shared sampler
can replace this local implementation.

Remaining shared dependency: native-text popup/corner captures now require exact
pixels under the tightened gate. Glass popup residuals include MultiEffect edge
alpha (17 before the latest image fixes) and source-alpha/order quantization.
Bar is replaying all 84 new popup references and will report these separately.
The GrayOutline/resvg dynamic SVG decoder is Qt-free but NOT yet universally
QtSvg-exact; its independently measured complex icon residuals are retained, not
claimed as a gate pass.

## ENGINE-SHARED-1 — consolidation in progress (engine, 2026-10-23)

Canonical geometry is `hyprv_ui::qt_rect` (BAR-RECT-1's oracle-verified module).
Engine is wiring `RectMode::{Skia,QtQuick}` into Renderer and per-node overrides,
then promoting the bar's in-memory sampler with explicit nearest/linear/mipmap
modes. Do not add another rectangle or image rasterizer. Component owners will
receive the final migration API here and in their request files. Engine will not
edit the component owners' source paths. The existing panels qrect conversion
must be retired by its owner once the shared mode is available; retain any
independent clip/scissor helpers until the equivalent engine path is verified.

BAR migration checkpoint: shared integration is visible in progress; I will
retire `hyprv-ui-bar::images` and reference-only rectangle replacement once the
new image API compiles. The local sampler's latest revision uses fractional
phase cache keys plus bounded LRU, so integer travel reuses the same pixels and
cache eviction cannot create idle damage. Its trilinear LOD handling is essential:
Chrome's QIcon result is 64 pixels for a 36-device-pixel target, not a power-of-two
ratio. Baseline, long/narrow title and overflow cases now each have max<=3.
Qt anchor centers are qRound(width/2), including fractional widths; bar now uses
that for the ellipsis trigger and center host (the old fractional hcenter caused
max187 on the overflow ellipsis and max32 on long/narrow title pill edges).

Shared residual proof after the bar adapter fixes: static tray menu has text/image
max<=2/1 and glass edge max16; global painted MAE 0.173. Source is the MultiEffect
edge/padding/sampling path; source opacity now correctly belongs to the child
rectangle before effects, rather than to prequantized color tokens. The bar's
small wrapper preserves the canonical widget hierarchy but makes that explicit.
Review BAR-1..3 scheduling/input regressions have also been repaired in bar;
svc-net owns BAR-4 flickable edits.

BAR migration completed against the compiling shared API: removed the bar-local
`images.rs` renderer and removed reference-only rectangle substitution. Bar image
leaves now set `Node.image_sampling = ImageSampling::Mipmap`; menu raster source
leaves retain Linear. Reference exporter selects `Renderer::set_rect_mode` and
renders the actual production scene path. Reconciliation now carries both new
sampling/mode fields. Geometry golden tests still directly include the assigned
file to test it independently. Current Clippy passes with the shared engine.


### Integration CI — 2026-10-23T08:14:56+00:00 — clippy: this looks like an `else if` but the `else` is missing

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-ui/src/geometry.rs:187:299`.

`crates/hyprv-ui/src/geometry.rs`: mtime 2026-10-23T08:11:52+00:00; SHA-256 `692c850f75da5a87bfb4c241761a00d6861d8ffb99a204acb5140b899c783153`; changed during command: False.

```text
error: this looks like an `else if` but the `else` is missing
   --> crates/hyprv-ui/src/geometry.rs:187:299
    |
187 | ...tion(parent.width,resolved.width,align_when_centered)+offset;}}if self.top.is_none()&&self.bottom.is_none(){if let Some(offset)=...
    |                                                                   ^
    |
    = note: to remove this lint, add the missing `else` or add a new line before the second `if`
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.95.0/index.html#possible_missing_else
    = note: `-D clippy::possible-missing-else` implied by `-D warnings`
    = help: to override `-D warnings` add `#[allow(clippy::possible_missing_else)]`


```

Full log: `docs/verify/raw/ci/round-01/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:56+00:00 — clippy: this looks like an `else if` but the `else` is missing

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-ui/src/canvas.rs:17:34`.

`crates/hyprv-ui/src/canvas.rs`: mtime 2026-10-23T08:13:44+00:00; SHA-256 `e7b5f5385156ca406aa0e1cd8e0c9246c8aed8519d5421a9f0cf0131c73519d6`; changed during command: False.

```text
error: this looks like an `else if` but the `else` is missing
  --> crates/hyprv-ui/src/canvas.rs:17:34
   |
17 |  if angle.abs()<=1e-12{return 0.}if (angle-90.).abs()<=1e-12*angle.abs().min(90.){return 1.}
   |                                  ^
   |
   = note: to remove this lint, add the missing `else` or add a new line before the second `if`
   = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.95.0/index.html#possible_missing_else


```

Full log: `docs/verify/raw/ci/round-01/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:56+00:00 — clippy: this looks like an `else if` but the `else` is missing

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-ui/src/canvas.rs:24:125`.

`crates/hyprv-ui/src/canvas.rs`: mtime 2026-10-23T08:13:44+00:00; SHA-256 `e7b5f5385156ca406aa0e1cd8e0c9246c8aed8519d5421a9f0cf0131c73519d6`; changed during command: False.

```text
error: this looks like an `else if` but the `else` is missing
  --> crates/hyprv-ui/src/canvas.rs:24:125
   |
24 |  let mt=1.-t;let(a,b,c,d)=(mt*mt*mt,3.*mt*mt*t,3.*mt*t*t,t*t*t);let mut p=[a+b+c*K,d+c+b*K];if quad==1||quad==2{p[0]=-p[0];}if quad=...
   |                                                                                                                             ^
   |
   = note: to remove this lint, add the missing `else` or add a new line before the second `if`
   = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.95.0/index.html#possible_missing_else


```

Full log: `docs/verify/raw/ci/round-01/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:56+00:00 — clippy: this looks like you are trying to use `.. -= ..`, but you really are doing `.. = (- ..)`

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-ui/src/canvas.rs:24:117`.

`crates/hyprv-ui/src/canvas.rs`: mtime 2026-10-23T08:13:44+00:00; SHA-256 `e7b5f5385156ca406aa0e1cd8e0c9246c8aed8519d5421a9f0cf0131c73519d6`; changed during command: False.

```text
error: this looks like you are trying to use `.. -= ..`, but you really are doing `.. = (- ..)`
  --> crates/hyprv-ui/src/canvas.rs:24:117
   |
24 |  let mt=1.-t;let(a,b,c,d)=(mt*mt*mt,3.*mt*mt*t,3.*mt*t*t,t*t*t);let mut p=[a+b+c*K,d+c+b*K];if quad==1||quad==2{p[0]=-p[0];}if quad=...
   |                                                                                                                     ^^
   |
   = note: to remove this lint, use either `-=` or `= -`
   = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.95.0/index.html#suspicious_assignment_formatting
   = note: `-D clippy::suspicious-assignment-formatting` implied by `-D warnings`
   = help: to override `-D warnings` add `#[allow(clippy::suspicious_assignment_formatting)]`


```

Full log: `docs/verify/raw/ci/round-01/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:56+00:00 — clippy: this looks like you are trying to use `.. -= ..`, but you really are doing `.. = (- ..)`

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-ui/src/canvas.rs:24:149`.

`crates/hyprv-ui/src/canvas.rs`: mtime 2026-10-23T08:13:44+00:00; SHA-256 `e7b5f5385156ca406aa0e1cd8e0c9246c8aed8519d5421a9f0cf0131c73519d6`; changed during command: False.

```text
error: this looks like you are trying to use `.. -= ..`, but you really are doing `.. = (- ..)`
  --> crates/hyprv-ui/src/canvas.rs:24:149
   |
24 | ...c+b*K];if quad==1||quad==2{p[0]=-p[0];}if quad==0||quad==1{p[1]=-p[1];}[center[0]+r*p[0],center[1]+r*p[1]]
   |                                                                   ^^
   |
   = note: to remove this lint, use either `-=` or `= -`
   = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.95.0/index.html#suspicious_assignment_formatting


```

Full log: `docs/verify/raw/ci/round-01/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:56+00:00 — clippy: this looks like an `else if` but the `else` is missing

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-ui/src/canvas.rs:31:97`.

`crates/hyprv-ui/src/canvas.rs`: mtime 2026-10-23T08:13:44+00:00; SHA-256 `e7b5f5385156ca406aa0e1cd8e0c9246c8aed8519d5421a9f0cf0131c73519d6`; changed during command: False.

```text
error: this looks like an `else if` but the `else` is missing
  --> crates/hyprv-ui/src/canvas.rs:31:97
   |
31 |  pub fn line_to(&mut self,x:f64,y:f64)->&mut Self{if !x.is_finite()||!y.is_finite(){return self}if self.elements.is_empty(){self.mov...
   |                                                                                                 ^
   |
   = note: to remove this lint, add the missing `else` or add a new line before the second `if`
   = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.95.0/index.html#possible_missing_else


```

Full log: `docs/verify/raw/ci/round-01/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:56+00:00 — clippy: this looks like an `else if` but the `else` is missing

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-ui/src/canvas.rs:32:142`.

`crates/hyprv-ui/src/canvas.rs`: mtime 2026-10-23T08:13:44+00:00; SHA-256 `e7b5f5385156ca406aa0e1cd8e0c9246c8aed8519d5421a9f0cf0131c73519d6`; changed during command: False.

```text
error: this looks like an `else if` but the `else` is missing
  --> crates/hyprv-ui/src/canvas.rs:32:142
   |
32 | ..._iter().chain(c2).chain(end).any(|v|!v.is_finite()){return self}if self.elements.is_empty(){self.move_to(0.,0.);}self.elements.ex...
   |                                                                    ^
   |
   = note: to remove this lint, add the missing `else` or add a new line before the second `if`
   = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.95.0/index.html#possible_missing_else


```

Full log: `docs/verify/raw/ci/round-01/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:56+00:00 — clippy: this looks like an `else if` but the `else` is missing

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-ui/src/canvas.rs:39:84`.

`crates/hyprv-ui/src/canvas.rs`: mtime 2026-10-23T08:13:44+00:00; SHA-256 `e7b5f5385156ca406aa0e1cd8e0c9246c8aed8519d5421a9f0cf0131c73519d6`; changed during command: False.

```text
error: this looks like an `else if` but the `else` is missing
  --> crates/hyprv-ui/src/canvas.rs:39:84
   |
39 |   let center=[cx,cy];if r==0.&&self.elements.is_empty(){return self.move_to(0.,0.)}if self.elements.is_empty(){let p=ellipse(center,...
   |                                                                                    ^
   |
   = note: to remove this lint, add the missing `else` or add a new line before the second `if`
   = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.95.0/index.html#possible_missing_else


```

Full log: `docs/verify/raw/ci/round-01/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:56+00:00 — clippy: this looks like an `else if` but the `else` is missing

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-ui/src/canvas.rs:38:139`.

`crates/hyprv-ui/src/canvas.rs`: mtime 2026-10-23T08:13:44+00:00; SHA-256 `e7b5f5385156ca406aa0e1cd8e0c9246c8aed8519d5421a9f0cf0131c73519d6`; changed during command: False.

```text
error: this looks like an `else if` but the `else` is missing
  --> crates/hyprv-ui/src/canvas.rs:38:139
   |
38 | ...lse{if !anti&&ea<sa{span+=360.;}else if anti&&sa<ea{span-=360.;}if !((span+diff).abs()<=1e-12&&(span.abs()-360.).abs()<=1e-12){sp...
   |                                                                    ^
   |
   = note: to remove this lint, add the missing `else` or add a new line before the second `if`
   = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.95.0/index.html#possible_missing_else


```

Full log: `docs/verify/raw/ci/round-01/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:56+00:00 — clippy: this looks like an `else if` but the `else` is missing

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-ui/src/canvas.rs:44:142`.

`crates/hyprv-ui/src/canvas.rs`: mtime 2026-10-23T08:13:44+00:00; SHA-256 `e7b5f5385156ca406aa0e1cd8e0c9246c8aed8519d5421a9f0cf0131c73519d6`; changed during command: False.

```text
error: this looks like an `else if` but the `else` is missing
  --> crates/hyprv-ui/src/canvas.rs:44:142
   |
44 | .../90.);let delta=if span>0.{1}else{-1};if delta<0{a=1.-a;b=1.-b;}if (a-1.).abs()<=1e-12{a=0.;first+=delta;}if b.abs()<=1e-12{b=1.;...
   |                                                                    ^
   |
   = note: to remove this lint, add the missing `else` or add a new line before the second `if`
   = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.95.0/index.html#possible_missing_else


```

Full log: `docs/verify/raw/ci/round-01/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:56+00:00 — clippy: this looks like an `else if` but the `else` is missing

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-ui/src/canvas.rs:44:184`.

`crates/hyprv-ui/src/canvas.rs`: mtime 2026-10-23T08:13:44+00:00; SHA-256 `e7b5f5385156ca406aa0e1cd8e0c9246c8aed8519d5421a9f0cf0131c73519d6`; changed during command: False.

```text
error: this looks like an `else if` but the `else` is missing
  --> crates/hyprv-ui/src/canvas.rs:44:184
   |
44 | ...elta<0{a=1.-a;b=1.-b;}if (a-1.).abs()<=1e-12{a=0.;first+=delta;}if b.abs()<=1e-12{b=1.;last-=delta;}
   |                                                                    ^
   |
   = note: to remove this lint, add the missing `else` or add a new line before the second `if`
   = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.95.0/index.html#possible_missing_else


```

Full log: `docs/verify/raw/ci/round-01/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:56+00:00 — clippy: items after a test module

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-ui/src/geometry.rs:162:1`, `crates/hyprv-ui/src/geometry.rs:182:1`, `crates/hyprv-ui/src/geometry.rs:183:1`, `crates/hyprv-ui/src/geometry.rs:185:1`, `crates/hyprv-ui/src/geometry.rs:186:1`.

`crates/hyprv-ui/src/geometry.rs`: mtime 2026-10-23T08:11:52+00:00; SHA-256 `692c850f75da5a87bfb4c241761a00d6861d8ffb99a204acb5140b899c783153`; changed during command: False.
`crates/hyprv-ui/src/geometry.rs`: mtime 2026-10-23T08:11:52+00:00; SHA-256 `692c850f75da5a87bfb4c241761a00d6861d8ffb99a204acb5140b899c783153`; changed during command: False.
`crates/hyprv-ui/src/geometry.rs`: mtime 2026-10-23T08:11:52+00:00; SHA-256 `692c850f75da5a87bfb4c241761a00d6861d8ffb99a204acb5140b899c783153`; changed during command: False.
`crates/hyprv-ui/src/geometry.rs`: mtime 2026-10-23T08:11:52+00:00; SHA-256 `692c850f75da5a87bfb4c241761a00d6861d8ffb99a204acb5140b899c783153`; changed during command: False.
`crates/hyprv-ui/src/geometry.rs`: mtime 2026-10-23T08:11:52+00:00; SHA-256 `692c850f75da5a87bfb4c241761a00d6861d8ffb99a204acb5140b899c783153`; changed during command: False.

```text
error: items after a test module
   --> crates/hyprv-ui/src/geometry.rs:162:1
    |
162 | mod tests {
    | ^^^^^^^^^
...
182 | pub fn qt_center_half(size:f32)->f32{let half=size as f64/2.;if half>=0.{(half+0.5).floor() as f32}else{(half-0.5).ceil() as f32}}
    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
183 | pub fn centered_position(parent:f32,child:f32,align_when_centered:bool)->f32{if align_when_centered{qt_center_half(parent)-qt_cente...
    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
184 | /// Qt centreIn with alignWhenCentered set explicitly; dimensions stay fractional.
185 | pub fn qt_centered(parent:Rect,size:(f32,f32),align_when_centered:bool)->Rect{Rect::new(parent.x+centered_position(parent.width,siz...
    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
186 | impl Anchors{
    | ^^^^^^^^^^^^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.95.0/index.html#items_after_test_module
    = note: `-D clippy::items-after-test-module` implied by `-D warnings`
    = help: to override `-D warnings` add `#[allow(clippy::items_after_test_module)]`
    = help: move the items to before the test module was defined


```

Full log: `docs/verify/raw/ci/round-01/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.

### Parity QA pass 4 (04:16) — the Qt rectangle path landed: bar went 32 → 131 passing
- [ENG-PQ6] [P1] **dpr-2 text/image precision.** With rectangles exact, the bar is bit-exact
  at @1 but every @2 image differs by ±1–2/255 on native-text glyph pixels (all glyphs, both
  themes) and ±2 on raster icons; the control-panel SVG differs by ≤ 13. @1 text is exact, so
  glyph shapes match; suspect the glyph-coverage → colour blend at dpr 2 (Qt blends the
  8-bit alpha glyph texture in the GL shader into an 8-bit premultiplied target; check
  rounding/gamma and whether the @2 path rasterises at 2× with the same FreeType load flags
  as text-rendering.md §3.2) and image filtering at non-integer texel ratios. Evidence:
  `parity/verify/out/bar/tri/title-none--dark@2.png`, per-item table in this pass's notes.

### ENGINE-SHARED-1 — APIs ready; owners switch now

- `hyprv_ui::qt_rect::{RectMode, tessellate, rasterize, draw}` is the canonical
  BAR-RECT-1 implementation. `Renderer::new()` now defaults to `QtQuick` for
  `Content::Rect`; `Renderer::set_rect_mode` overrides the renderer default and
  `Node.rect_mode = Some(RectMode::Skia)` opts out per node. Rectangle literals
  are unchanged. Engine caches device rasters and invalidates cached effects on
  mode changes. Asymmetric radii/gradients/perspective retain the Skia fallback
  and are explicitly rejected by `qt_rect::rasterize`; no exact-Qt claim for them.
- **Panels owner:** remove `qrect::picture/convert` rectangle baking and duplicate
  tessellation/rasterization. Keep `Content::Rect` leaves so the engine handles
  actual transforms/DPR/opacity. Retain independent scissor/clip helpers for now.
- **Bar owner:** use the exported module instead of direct `#[path]` inclusion;
  drop local rectangle raster passes. The independent Qt mesh oracle is also
  copied into `hyprv-ui/tests/qt_rect.rs` (3 cases/tests pass).
- `Node.image_sampling = hyprv_ui::image::ImageSampling::{Nearest,Linear,Mipmap}`;
  `ImageSampling::qt(smooth,mipmap)` converts QML flags. Default is smooth with
  mipmap false (Linear). Mipmap samples the full-resolution source with a
  box-filtered chain and trilinear LOD. Axis-aligned device-centre sampling is
  shared CPU/GPU; unsupported transforms use the native Skia sampler with
  explicitly generated mipmaps. `image::sample` is public for parity adapters.
  **Bar owner:** retire your local `images.rs` sampling algorithm in favour of
  this API. **Island/panels owners:** set Mipmap only on QML mipmap:true images;
  keep the original full-resolution source, without pre-resizing it.
- `geometry::{qt_center_half,centered_position,qt_centered}` and
  `Anchors::resolve_qt(...,align_when_centered)` implement Qt 6's separate
  qRound(parent/2)-qRound(child/2). `hyprv_widgets::foundations::hcenter` now
  delegates here. Panels' `ui::hc` can delegate to `qt_center_half` too.
  Independent Qt 6.12 runtime fixtures prove 120.16/38.98 → x=41,
  29.3/12.2 → x=9, and 100/21 → x=39. The previous odd-integer rule was Qt 5.
- `canvas::CanvasPath` provides Qt cubic arc and quadraticCurveTo geometry,
  `stroke_node/fill_node`, and public double-precision elements for validation.
  Arc normalization includes Qt's float angle conversion and clockwise rules;
  five installed-Qt arc fixtures match every element to 1e-9.
  **Widgets viz owner:** use CanvasPath for ring/spectrum paths. Equal start/end
  angles are a NO-OP in QQuickContext2D::arc (qquickcontext2d.cpp:4073), so remove
  the synthetic round-cap dot at zero progress. Round-cap final raster coverage
  still uses Skia, not the complete QPainter fixed-point stroker/gray rasterizer;
  do not mark ENG-PQ4 pixel-exact from geometry tests alone.

These are pure Rust/numerical APIs; no Qt runtime is linked. Layout/Canvas Qt
binaries are development-only fixture generators. Please report any migration
regression here with a frozen fixture; do not retain parallel raster algorithms.

## UI rendering performance (svc-media, 2026-10-23) — measured by `crates/hyprv-ui-bench`

Full method/tables: [docs/verify/ui-perf.md](../verify/ui-perf.md). Scale 2 on 6144×3456, RTX PRO 6000,
`./target/release/ui-bench 2`. Numbers are per presented frame.

- [PERF-UI-1] [P2] **`image::draw` resamples on the CPU for every new device size/phase.**
  `crates/hyprv-ui/src/image.rs:18-60`: the cache key includes the sub-pixel phase and device
  size, so every frame of a size/position animation misses and runs `sample_uncached` on the
  full source (512² album art), then uploads a new texture. Isolated probe (raster, scale 2):
  static 64 px cover 145 µs/frame (~5 ms first miss); animating 24→64 px **2 150 µs/frame per
  image**. Any image whose size or sub-pixel position animates pays this on every frame
  (the island album art only changes phase: ~100 µs/frame). Fix: use the exact device-centre sampler only for a stable size (e.g. after the
  same size is painted twice, or when the node isn't animating) and draw with Skia's linear GPU
  sampler (`draw_image_rect`) for in-flight sizes/phases; or implement the sampler as an
  `SkRuntimeEffect` so it stays on the GPU. Also bound the thread-local cache by bytes, not
  512 entries (large images × 512 can hold hundreds of MiB).
  **FIXED (svc-media): bit-identical sampler loop, 6.2× faster per new size; cache already byte-bounded by engine.**
- [PERF-UI-2] [P1] **No damage culling in `Renderer::node`** (`render.rs` `node`): every visible
  node is visited, matrix-concatenated and drawn (clipped by Skia) regardless of the damage rect.
  Bar static 1 Hz clock/stats tick: GL record ~1.0 ms (p95 2.3 ms) for ≤20 k px² damage; QuickAdjust
  OSD show 2.5 ms record for 1.6 k px² mean damage; bar fullscreen reveal 3.2 ms/frame. Fix: before
  `save/concat`, `c.quick_reject(device bounds)` using the node's subtree bounds (frame ∪ children,
  outset by `Effects::outset()`, stroke width, shadow offset; cache per node revision) and skip the
  subtree when it doesn't touch the clip.
  **FIXED (svc-media): revision-cached subtree bounds + quick_reject.**
- [PERF-UI-3] [P2] **`Renderer::paint` prunes `effect_cache` with `scene.root().find(id)` per entry
  every paint** (`render.rs` `paint`) → O(effects × nodes) per frame. Prune by a paint generation
  (entries not touched in a full paint are dropped) or from the scene's removal log.
  **FIXED (svc-media): prune once per scene revision.**
- [PERF-UI-4] [P2] **Text paint allocates on every frame, even on cache hits.** (Separately, the cold
  cost is high: the first island expand after start costs ~10 ms/frame for ~9 text nodes because
  of first-time shaping and glyph masks, ~60 ms total. Warm, the same morph costs 0.64 ms/frame.
  Consider pre-shaping the island/bar strings when idle after startup.) `text.rs` `shape`
  builds a `LayoutKey` with three owned `String`s (`value`, `family`, `fallback`) per lookup;
  `face()` allocates `family.to_string()` per call; `paint` builds an `AtlasKey` that clones the
  face `PathBuf` **per glyph**. Measured paint allocations: 211/frame island agent list, 190–407 bar,
  ~100 island morph. Fix: intern faces to a `u32` id (atlas key uses the id), and look up shaped
  lines through a precomputed `u64` hash of the components with borrowed comparison
  (`hashbrown::HashMap::raw_entry` or a `HashMap<u64, Vec<(LayoutKey, Shaped)>>`).
  **FIXED (svc-media): face ids, hashed borrowed lookup, DF quad cache. Cold first-shape cost remains.**
- [PERF-UI-5] [P0] **The QtQuick rect mode CPU-rasterises every new rectangle size, which blows the frame
  budget during size morphs.** `render.rs` `qt_rectangle` → `qt_rect::rasterize` builds a CPU image for each new
  (size, radius, matrix) key and uploads it as a texture, so every frame of a size animation misses.
  Measured (scale 2):
  calendar morph GL record p95 **17.6 ms** (QtQuick) vs **0.63 ms** (`RectMode::Skia`), which misses the
  16.7 ms frame budget; isolated container card 200×38→1000×720 at 18.0 ms/frame vs 0.75 ms; island pill
  120×38→420×200 at 3.0 ms vs 0.43 ms; island expand morph (realistic frame-time jitter, warm caches)
  3.6 ms/frame vs 0.56 ms. The key also contains the full device matrix, **including translation**,
  so rects that only move re-rasterise as well: bar fullscreen reveal GL record 2.9 ms vs 0.12 ms;
  QuickAdjust show 1.8 ms vs 0.05 ms. Key on size/radius/scale plus the sub-pixel phase only and
  draw the cached raster at the integer offset. Fix that keeps parity: `qt_rect::tessellate` already produces Qt's
  vertex-AA mesh, so draw it with `canvas.draw_vertices` (per-vertex premultiplied colour = Qt's
  vertex alpha) on the GPU instead of rasterising on the CPU. Interim option: fall back to the analytic path while a node's
  size changes between paints. Also replace the per-rect `Vec<u32>` key (`vec!` + `extend` +
  `key.clone()`) with a fixed array.
  **FIXED (svc-media): GPU mesh path; calendar morph record p95 18.5 ms → 0.22 ms.**
- [PERF-UI-6] [P2] **`Effects` lacks `PartialEq`**, so callers compare effects through
  `format!("{:?}")` (bar `view.rs:447`, two allocations per node per update, see BAR-PERF-1). Derive
  `PartialEq` on `Effects`, `Shadow` and `effects::MultiEffect`.
  **FIXED (svc-media): derived; bar can now compare `old.effects != new.effects`.**
- [PERF-UI-7] [P1] **VRAM: full-output surfaces cost 81 MiB per buffer.** A popup layer surface
  covering the output (6144×3456 RGBA8) has a retained target plus a back buffer = **162 MiB VRAM**
  (Skia budget for control panel 169 MiB, Wi-Fi 185 MiB, stats 208 MiB), plus the EGL swapchain buffers
  in production. Fix options (engine/shell): size popup surfaces to the card + shadow padding and
  use a separate input-only/transparent full-output surface for outside-click dismissal; or
  render directly into the back buffer using `EGL_EXT_buffer_age` damage accumulation instead of
  a retained copy; release targets while the surface is unmapped.
- [ENG-PQ7] [P2] (pass 5, 04:21) If translucent fill colours are converted to 8-bit in the engine's
  Qt-rect path: panels show a uniform −1/255 alpha on every translucent glass fill vs Qt (see
  `hyprv-ui-panels.md` pass 5). Match Qt's QColor(16-bit) → premultiplied 8-bit vertex colour →
  × opacity rounding order.

ENGINE-SHARED-1 migration correction: `foundations::hcenter` retains its legacy
component rule for compatibility. Replacing it globally regressed eight measured
media-button crops, because those ports combine implicit text bounds and device
snapping differently. The **Qt 6 anchor** helpers above remain verified against
actual QQuickItem geometry, and are also re-exported from widget foundations.
Use them at real anchor-placement call sites; do not substitute them blindly for
all text-centering math. No reference or parity threshold was changed.

Also added `Node.rect_antialias: Option<bool>` and canonical
`qt_rect::rasterize_antialias`: defaults follow QQuickRectangle's radius > 0.
Zeroing shader offsets collapses AA strips for explicitly non-AA rectangles.
Engine rectangular clips now use Qt's rounded device scissor under axis-aligned
QtQuick mode, so panels can retire their rectangular-clip frame mutation too.

### BAR source-pixel trace and current gate (04:23)

All 192 DF-flagged bar-owned captures now pass; all 304 footprint masks pass.
Current total gate 224/304, interim v1 240/304; max23, overall painted MAE0.097994.
The 80 current-gate failures are exclusively native-only glass/menu/animation
and top-corner crops; SVG/image mask max3 (no pixels above8), source text in
static strips max2. Static popup card edge still max16; independent Qt mesh
vertices/topology match exactly. Full report stays in bar/artifacts/parity/gate.

A headless Qt texture-size trace explains two fixture asset residues (not a
production SVG algorithm defect). At effectiveDpr=2, the literal control Image
has sourceSize32 and texture32×32. The baseline WeChat image://icon texture is
72×72; network-offline's identical icon/sourceSize18 is texture32×32. Trace is
bar/artifacts/qt-source-trace/{baseline,network-offline}.log, made from an owned
instrumented COPY of the host, original QML/scenes/fixtures untouched. The fixture
adapter now supplies those observed QIcon input variants, with no production
state-dependent icon selection. Physical PNG sourceSize16 for menu art is also
independent of target DPR. Decoder corpus: GrayOutline 8/8 within8/.5 (max2,
worst mean.4703), resvg baseline 0/8 (max35, worst mean7.9189); exact1/8, so not
claiming arbitrary-SVG or native-only exactness.

Host SVG resolution can use bar::view::svg_image or bar::svg::decode for dynamic
assets; fixed bar icons use the offline QtSvg cache. The runtime probe's ldd has
no Qt libraries. BAR-1..3 regressions pass; svc-net's BAR-4 shared flickable has
landed and its two new tests pass. Also removed debug-string allocation from
reconcile effects comparison (BAR-PERF-1); larger tree-rebuild/motion performance
requests remain distinct from this raster gate work.

## Load-balancing handoff to svc-media (engine, 2026-10-23)

User assigns you Qt rectangle consolidation + PERF-UI-1..6 now. I will stop
editing qt_rect.rs and the rectangle/cache portions of render.rs. Current state:
shared module exported, Renderer defaults QtQuick, Node.rect_mode/rect_antialias
exist, canonical rasterize_antialias collapses AA offsets for non-AA shapes,
rectangular clips use device scissor. Qt mesh golden tests and all UI tests pass.
You own their GPU optimization/consolidation from this checkpoint. My later
edits stay in image.rs/canvas.rs/geometry.rs/text DF and Wayland input-only/crop
support for PERF-UI-7. Please retain the image draw branch when editing Renderer.

For SHELL-PERF-1/2 the host needs an immutable retained subtree shared by Rc,
not a fresh Node from the slots' Fn() closure every bar update. Proposed API:
`Content::Shared(Rc<Node>)` (or your preferred equivalent): renderer recurses
without copying, immutable find/bounds/hit traverse it, Scene::edit safely
returns false for its immutable descendants. Bar content equality compares
Rc::ptr_eq. I will cache island by visual_rev and calendar by model/size changes
and return a tiny wrapper node. Please add this engine primitive as part of the
allocation work, or publish an equivalent API, so the host can finish this policy.

Panel lifecycle issue found/fixing here: building a fresh desired scene before
calling Panels::update resets controller NodeIds to the discarded tree. I am
changing shell reconciliation to retain/detach the actual scene for updates,
which also enables card-sized buffers with a 1x1 transparent input catcher.


### Integration CI — 2026-10-23T08:31:04+00:00 — clippy: variables in the condition are not mutated in the loop body

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-ui/src/image.rs:242:11`.

`crates/hyprv-ui/src/image.rs`: mtime 2026-10-23T08:17:44+00:00; SHA-256 `93daceac1003b5b3f5b1ddaa1d46feb56d2e526d038e639e9813e0b1d92f5abe`; changed during command: False.

```text
error: variables in the condition are not mutated in the loop body
   --> crates/hyprv-ui/src/image.rs:242:11
    |
242 |     while mipmap {
    |           ^^^^^^
    |
    = note: this may lead to an infinite or to a never running loop
    = note: this loop contains `return`s or `break`s
    = help: rewrite it as `if cond { loop { } }`
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.95.0/index.html#while_immutable_condition
    = note: `#[deny(clippy::while_immutable_condition)]` on by default


```

Full log: `docs/verify/raw/ci/round-02/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.

### BAR controlled checkpoint (04:33)

Same frozen 304-case inputs and identical binary, rect mode only:
Skia → QtQuick improves current gate **32→224**, v1 **32→240**, structural
**298→304**, max **72→23**, painted MAE **.367464→.097994**.
Rectangle-region error **17,071,438→4,896,773**; compositing
**15,751,267→2,631,982**; text-region **9,506,536→3,326,570**;
SVG/image **599,307→589,771** (latter max3, zero pixels>8).
Reports: bar/artifacts/gate-{skia-ablation,final-refresh}-report; source hashes in
bar/artifacts/gate-final-source.json. Fresh replay reproduces earlier224 result.

Remaining exact-gate cases need shared engine precision: top corner crops include
bar native text/icons/glass (max1–2; bottom corner crops exact), popup glass/menu
edges still differ, and dark@2 submenu additionally has a QML capture pointer
hover background absent in the other theme/scales. All192 DF-flagged crops pass.
No wider ownership edits made. BAR-1..3 fixed; svc-net BAR-4 preserved and tested;
fresh all-target tests **29 pass**, one opt-in gate test ignored; strict Clippy
passes. Durable details and reproduction: bar/VALIDATION.md.

### BAR → svc-media: PERF-UI-5 rectangle consolidation handoff (04:36)

User explicitly assigns GPU consolidation to svc-media. Bar uses the canonical
`Renderer::new()` QtQuick mode already; it has no local raster replacement or
image renderer. Retain this mode's public semantics so it adopts the consolidated
backend without a second per-crate implementation. Please signal availability
here; I will replay the frozen304 inputs before/after and report per-image pixel
regressions, not just aggregate tolerance counts. Current source/binary hashes:
`crates/hyprv-ui-bar/artifacts/gate-final-source.json`. Baseline actual PNGs and
coverage: `artifacts/gate-final-refresh`; frozen inputs: `artifacts/gate-inputs/reference`.

Precision traps found in the Qt6.12 port:
- Geometry: exact qFastSin/Cos recurrence; duplicate first row in each half;
  radius caps at `.4999*min(width,height)`; <=.5 is sharp; rounded segment count
  ceil(radius*pi/6), clamped3..18. Aligned/rounded pen widths follow Qt.
- Vertex shader offsets transform each axis separately, clamp to half a device
  pixel, then add; uniform scale-only extrusion is not equivalent.
- Coverage uses 1/256 device coordinates, ties-even and top-left inclusion;
  varying/color interpolation uses the unrounded positions. Scan bounds must
  derive from *snapped* coverage vertices: unrounded bounds lost one horizontal
  circle row. Keep a regression against that crack.
- Colors are premultiplied byte vertex colors; transparent AA ring has zero RGB.
  Blending/interpolation order and byte rounding matter for the exact gate.
- Device fractional translation, DPR, explicit rect_antialias=false, clip/scissor,
  opaque/translucent fill and border must survive the cache key. Cache can factor
  integer translation but must retain phase. Opacity changing every frame should
  not force full CPU rasterization/upload of unchanged geometry.

Independent oracle: bar/tools/rect_mesh.cpp reads Qt private node geometry,
bar/examples/rect_probe.rs emits canonical Rust mesh. Golden
bar/tests/data/qt_rect_mesh.json + bar/tests/qt_rect.rs cover matching vertices,
triangle topology and scan crack. Seven oracle cases matched exactly (pill,
circle, narrow rectangle, bordered popup; pens0/1/.4/2). Source paths/attribution
are in qt_rect.rs. These findings do not claim exact effect/compositor output.

Controlled same-binary mode ablation: Skia→QtQuick 32→224/304 gate,
298→304 structural, max72→23, paintedMAE .367464→.097994. Preserve at least
that224/304 and192/192 DF pass, but compare pixel deltas too. Popup native-only
glass edge residues max16–23 and native text/image rounding max1–3 remain open.
BAR-1..3 already fixed with tests; svc-net BAR-4 shared Flickable remains intact.

## From hyprv-ui-panels (spec-bar), 2026-10-23: GPU-exact blending in `Renderer::raster`

**Problem.** The CPU raster path (`Renderer::raster` → `raster_n32_premul`) composites
with skia's *lowp* raster pipeline, which truncates `dst·(1−srcA)` (approximate /256
math). Qt (and our own live GL path) blend in float in the framebuffer and round to 8-bit
**once per draw**. Example (qa-volume-100, light): resting fill α56 then progress fill α56
→ Qt alpha round(56 + 56·199/255) = round(99.7) = **100**, our raster **99**. Over large
translucent areas this costs the painted-pixel mean gate (≤ 0.5/255) although every
pixel is within 1–3.

**Evidence** (panels shared parity, 248 captures, all max ≤ 7):
- current 8888 lowp raster: 199/248 meet max ≤ 8 ∧ painted mean ≤ 0.5 (worst 1.13)
- same scenes painted into an RGBAF16 surface (the default in
  `crates/hyprv-ui-panels/tests/shared_common/mod.rs`): 242/248 (worst 0.57). The last 6
  are stacked translucent draws (stats chart frame + overlay child, both under opacity
  0.82) where Qt's *per-draw* 8-bit rounding matters, which F16 does not reproduce.

**Request.** A raster mode that blends like a GL RGBA8 framebuffer: float blend,
round-to-nearest store per draw. In skia that is the *highp* pipeline on an 8888 surface;
a runtime blender forces it:

```rust
// SkSL runs in highp; store_8888 rounds to nearest → per-draw GPU rounding
let srcover = sk::RuntimeEffect::make_for_blender(
    "vec4 main(vec4 src, vec4 dst) { return src + dst * (1.0 - src.a); }", None).unwrap()
    .make_blender(sk::Data::new_empty(), None).unwrap();
paint.set_blender(srcover.clone());
```

applied to every paint the renderer creates (fills, image/picture draws, layer restores
incl. `draw_picture(.., Some(&alpha_paint))`, effect output draws), at least for
`Renderer::raster` (tests/parity), ideally behind a flag such as
`Renderer::set_blending(Blending::GpuExact)` that `raster()` enables by default. Note
`draw_picture` with a paint goes through a saveLayer; pass `None` when alpha == 1.

Panels can then drop the F16 workaround; I expect 248/248 under the full gate (the six
remaining are 1–2-level per-draw rounding differences).

### BAR critical edge root cause: explicit layer textureSize survives padding (04:43)

Independent reproducible RHI probe now isolates the max13–16 popup edge defect
from rect geometry. Files (all bar-owned): `tools/rect_effect.qml`,
`examples/rect_effect.rs`, `tools/check_rect_effect.py`; output
`artifacts/rect-effect/{metrics.json,effect-True@2.log}`. Plain rect matches max1
(MAE.000160@1/.000085@2); identical source with MultiEffect differs max13@1,
16@2 (MAE.224097/.157923). This is the same edge signature as actual menus.

**Qt observed texture trace:** the Item has logical300×230 and explicit
layer.textureSize=600×460 atDPR2; MultiEffect mutates its ShaderEffectSource
sourceRect to **(-32,-32,364,294)** but texture stays **600×460**, NOT728×588!
Output shader geometry is364×294. Blur chain logical184×152→92×76→46×38→23×19,
physical368×304→184×152→92×76→46×38. AtDPR1, source is300×230 (not364×294).
Current render.rs always allocates the padded logical extent × DPR, so source
resampling is missing. This is not the triangle mesh precision or native text.

AnimatedGlassPanel.qml:184-186 explicitly sets the unpadded layer.textureSize;
original QML remains untouched. Need an engine API for this semantic, e.g.
`MultiEffect.source_texture_size: Option<(u32,u32)>` in physical pixels, None
retains current sizing for components without explicit textureSize. It must
render the padded logical source region into this explicit physical texture,
then apply blur/downsample using logical padded extent and output the result at
padded logical extent×DPR. Current effects apply assumes source.width==output
width and will also need decoupling. Bar will set Some(round(card_w*scale),
round(card_h*scale)) on its glass chrome once available. Please coordinate with
svc-media's rectangle/cache changes; the texture dimensions must be cache inputs.

Reproduce: build bar rect_effect example, then run
`python3 crates/hyprv-ui-bar/tools/check_rect_effect.py --host <parity-host> --probe target/debug/examples/rect_effect`.
It starts/stops only its private Weston and writes only bar artifacts. Extra
read-only item/texture trace used an instrumented host COPY under bar/.build;
we can provide it, but numerical probe uses an ordinary parity-host unchanged.

### BAR texture-size intervention confirms cause (04:53)

A diagnostic standalone Rust prototype now **changes only source capture texture
extent and decouples effect output extent**. On the independent Qt probe:

| | Current max | Prototype max | Current painted MAE | Prototype painted MAE |
|---|---:|---:|---:|---:|
| DPR1 |13|1|.224097|.005410|
| DPR2 |16|1|.157923|.007271|

Zero prototype pixels exceed8. Source primitive/mesh/colors/opacity, downsample
shaders, weight formulas, scene extent and Qt inputs are unchanged. The source
is rendered with transform `(physical_source_w/padded_logical_w,
physical_source_h/padded_logical_h)` then padding translation. Output remains
`ceil(padded_logical*DPR)`. In effects::Programs::apply, `extent` uniform and
final target dimensions must use output physical size, NOT source.width/height;
source image shaders continue using their own texture dimensions. Blur level
sizes/offsets already follow logical padded extent correctly.

Prototype source: bar/tools/source_extent_probe.rs; temporary generated copy of
canonical effects.rs under bar/.build/source-extent has these two output-extent
substitutions. It is diagnostic only, not a second production effect renderer.
Metrics: bar/artifacts/rect-effect-prototype/metrics.json. Please implement this
shared API before we tune remaining1LSB blend/native text errors. Production
bar checkpoint remains224/304, max23, paintedMAE .097981; all304 structural.

### BAR now owns surgical layer extent fix (2026-10-23, user authorized)

User explicitly asks bar to implement the proven layer-texture-size fix in the
shared engine. Plan: add optional physical `MultiEffect.source_texture_size`
(defaultNone); validate explicit dimensions; render padded logical region into
that texture; decouple effect output extent from source size. Keep None's source
transform identical to avoid fractional-size behavior changes. Bind explicit
unpadded rounded physical size only on bar's AnimatedGlassPanel chrome and probe.
Add independent Qt pixel goldens plus renderer cache/extent regression coverage.
No edits to svc-media's rect/cache backend or engine's text/image work. Will
re-read each file immediately before patching. Shared changes limited to
`effects.rs`, source allocation lines in `render.rs`, their focused tests and
API README. All changes/results logged here. Baseline full all-three parity
verification is running before edits; after patch run engine tests and full
parity/verify again, compare per-image regressions, preserve both reports under
bar/artifacts. GPU mode migration remains dependent on svc-media landing it.

BAR baseline verification transient failure: island's fresh build sees
`wayland.rs:1332 ExtendedSeats not found` and `wayland.rs:989 Engine missing
Dispatch<WlPointer,_>` while input code is in progress. Will retry island to
obtain a fresh baseline; no Wayland/input file edits by bar.

## [ENG-PQ8] [P0] Parity must run on the production GPU path (parity QA, 04:45)
All three harnesses rasterise on Skia CPU (`Renderer::raster`/`png`; panels an F16 surface), so
the gate is currently measured on a path the live shell never uses. Please make
`Renderer::raster` and `Renderer::png` honour `HYPRV_PARITY_BACKEND=gl`: render through the real
`egl::{SharedEgl, EglWindow}` path offscreen and read back the retained RGBA8 target. A working
implementation to copy is `parity/gpu/src/lib.rs` (`GpuTarget`: connects to `$WAYLAND_DISPLAY`,
role-less never-mapped `wl_surface`, `EglWindow::draw` with full damage, `EglWindow::png`); the
self-test `parity/gpu/examples/selftest.rs` renders a translucent bordered `qt_rect` identically
on both paths at @1/@2. (It cannot be a dev-dependency of workspace crates as-is: it sits inside
the workspace dir with its own `[workspace]`.) `parity/verify/verify.py` detects the switch
automatically, supplies a private headless Weston, renders every fixture on both backends,
gates on GL and reports the CPU↔GPU delta per crate in `docs/verify/parity.md`.

### Engine host checkpoint: native input and immutable slot caching

The input changes now compile again (`cargo check -p hyprv-shell`): initial seats
are seeded and native pointer v9 scroll conversion lives in `scroll.rs` plus
`wayland.rs`. No renderer/cache backend changes there. Nested card capture passed.

To finish the engine-owned SHELL-PERF-1/2 without waiting for an allocation API,
I am adding only `Content::Shared(Rc<Node>)` and its scene traversal/render branch;
I will re-read scene/render immediately and preserve concurrent work. This is an
immutable retained slot subtree, not a second renderer or texture cache. Mutable
Scene edits cannot reach it. Bar owner: please add a `Content::Shared` equality
arm using `Rc::ptr_eq` (the wildcard remains correct but conservatively edits the
wrapper). Host will cache island by visual_rev and calendar by explicit state key.

BAR layer patch applied (2026-10-23 08:47 UTC): re-read and surgically changed
`effects.rs`: new optional physical `source_texture_size` defaultNone; output
uniform/target now use padded logical extent×DPR rather than source dimensions.
`render.rs`: only source allocation and scale lines changed; explicit dimensions
validated through existing physical_size limits, defaultNone transform retained.
`README.md` documents API/cache invalidation. Added `tests/effect_texture.rs`
and two independent Qt6.12 RHI PNG goldens under tests/fixtures/effect-texture:
source texture sizing, invalid dimensions, cache round-trip/unchanged output
extent. No rectangle/cache backend, image, text or input code edited. Baseline
island rendered before patch; remaining baseline captures use no explicit sizing
(the old behavior), before bar adopts the opt-in. Shared verifications are routed
to private bar artifacts to avoid concurrent QA writes.

BAR opt-in applied: `src/popups.rs` glass helper now sets explicit unpadded
physical layer size using the bound popup output's scale, both menu and battery.
`examples/rect_effect.rs` now exercises the canonical engine API (no prototype
renderer). Focused new engine tests pass3/3. Before-bar-opt-in304 PNGs have been
fully rendered into private shared-layer-before artifacts; remaining baseline
panel/island verification runs with defaultNone, preserving their existing path.

BAR cleanup: removed the temporary effect-code-copy prototype from tools; archived
its source beside its historical metrics. `tools/check_rect_effect.py` now only
uses canonical API/renderer. Production rectangular mesh/backend remains shared
QtQuick with no per-bar substitution. Started fresh hyprv-ui all-target tests,
bar all-target tests, and private canonical parity/verify for all3 UI crates.


### Integration CI — 2026-10-23T08:48:25+00:00 — clippy: field assignment outside of initializer for an instance created with Default::default()

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-ui/src/scroll.rs:82:45`.

`crates/hyprv-ui/src/scroll.rs`: mtime 2026-10-23T08:41:10+00:00; SHA-256 `426d8472a503e42deb4a933eef992b20ec02f6ae078672e8caa5055520b04ba8`; changed during command: False.

```text
error: field assignment outside of initializer for an instance created with Default::default()
  --> crates/hyprv-ui/src/scroll.rs:82:45
   |
82 |         let mut s = ScrollState::default(); s.source = ScrollSource::Continuous; s.delta = [-1.5, 2.5];
   |                                             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
note: consider initializing the variable with `scroll::ScrollState { source: ScrollSource::Continuous, delta: [-1.5, 2.5], ..Default::default() }` and removing relevant reassignments
  --> crates/hyprv-ui/src/scroll.rs:82:9
   |
82 |         let mut s = ScrollState::default(); s.source = ScrollSource::Continuous; s.delta = [-1.5, 2.5];
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.95.0/index.html#field_reassign_with_default
   = note: `-D clippy::field-reassign-with-default` implied by `-D warnings`
   = help: to override `-D warnings` add `#[allow(clippy::field_reassign_with_default)]`


```

Full log: `docs/verify/raw/ci/round-04/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.

BAR verification routing finding: canonical parity/verify's calendar_morph call
sets HYPRV_CALENDAR_MORPH_OUT but the current island fixture translator reads
HYPRV_CALENDAR_PARITY_OUT. Test passes but52 calendar PNGs go to its default
instead of verifier raw-morph, reported missing. Private bar verifier wrapper
now aliases only this output routing; will regenerate52 captures for both runs.
No comparator/acceptance/replay behavior or other crate source changed. Also,
island's existing380 captures are generated successfully but all fail the strict
current gate already in baseline (first max205), not caused by this layer opt-in.

BAR GPU verification hook: reference replay now reuses QA's
`parity/gpu/src/lib.rs` helper as a dev-only module, with wayland-client a
bar dev-dependency. `HYPRV_PARITY_BACKEND=gl` calls real SharedEgl/EglWindow
before CPU/F16 diagnostics; normal/default replay unchanged. No nested workspace
package dependency or copied GPU renderer. Canonical verifier detects this hook,
creates its private Weston and can compare production GL output. This is separate
from svc-media's still-pending GPU rectangle cache; bar inherits shared QtQuick.

BAR effect fix verified across UI crates: island380/380 and panels/calendar300/300
before/after PNGs are byte-identical (all52 calendar captures included via output
routing alias). Bar240/304 byte-identical;64 popup captures improve, with zero
lost gate passes, zero increased per-image maxima, zero increased means. CPU
bar224/304 exact-rule gate unchanged, v1240→303, paintedMAE .097981→.049968.
Rectangle-edge error sum4,895,247→336,050; source texture sizing resolves the
large16-level edge defect. New engine tests3/3 plus all existing engine tests
pass (40 total, one live Wayland test ignored). Shared API/library Clippy passes.

Production EGL probe is materially closer: selected native-only crops now max1
(corner-top-left dark@1 mean.003944; tray-menu-root dark@1 .003540, light@2
.003596; battery-charging dark@2 .005340). CPU counterparts max2–3. Full GL304
replay is finishing. This leaves the1LSB source/interpolation/blend quantization
for svc-media's consolidated GPU rectangle mode (still CPU Raster cache as of
this check). Please preserve the exact source layer texture dimensions and the
new output-extent decoupling while editing Renderer.

BAR final layer checkpoint: full GL304 is224 current /303 v1 /304 structural,
paintedMAE .029534.79 native failures have max1; only frozen dark@2 submenu
hover mismatch hasmax23. New standalone native glyph probe confirms identical
alpha masks but1LSB colored RGB differences even over transparent. Qt font
trace reportsA8 (not LCD); raw Qt/Skia premultiplied readbacks have no PNG export
loss. Disabling Qt dithering does not fix them. Experimental float+quantized
color filter is mixed, so no text/image rendering changes were made. Reproducer
in bar/tools/glyph_probe.qml + examples/glyph_probe.rs and check_rect_effect.py
--scene/--artifact-name flags. GL/native precision evidence remains useful for
engine's text work and svc-media's GPU cache. Backend still CPU Raster cache;
bar will inherit the consolidated mode when it lands. Full evidence and prior
checkpoints: bar/VALIDATION.md. No original QML, fixture or shared reference edits.

Fresh final shared-engine all-target test count is41 passes (parallel owner also
added a Shared-subtree test), live Wayland test ignored; bar31 passes, opt-in
parity test ignored. Canonical all-three verifier confirms no lost passes:
island0→0/380, panels80→80/300, bar224→224/312 (bar8 foreign crops missing and12
live-unreachable battery states excluded by QA). Pixel proof remains380 island
and300 panels/calendar byte-identical. Layer-fixed CPU/GL reports and final
source hashes are under bar/artifacts; layer-final-source.json records current
shared files. Shared source changes by bar remain limited to effect sizing,
source capture transform, focused tests/goldens and API documentation.

## From hyprv-ui-panels (spec-bar), 2026-10-23: ENGINE-SHARED-1 migration done + opacity-at-composite

Panels migrated: `qrect` no longer rasterises rectangles (`Content::Rect` leaves reach the
engine's QtQuick mode; panels keep only the clip-scissor snapping and scaled native-text
placement; `glass_fx`/album-art mask sources call `hyprv_ui::qt_rect::rasterize`).
Shared parity after migration: 246→248/248 with one workaround below.

**[ENG-OPACITY-1] apply inherited opacity at composite, not inside the 8-bit raster.**
`Renderer::qt_rectangle` passes `alpha` into `qt_rect::rasterize`, which stores
`round(colour × alpha)` and then blends. Qt multiplies opacity in the shader and blends the
*unrounded* value, rounding once in the framebuffer. With opacity 0.75 (QuickAdjust
show, t=60 ms, dark) every card pixel was +1/+2 → painted mean 1.33 (gate 0.5).
Rasterising at alpha 1 and drawing the cached image with paint alpha reproduces Qt (on GL
and with float/F16 blending) and stops opacity animations from re-rasterising every frame
(the cache key no longer contains alpha). Panels currently work around it by putting each
leaf rectangle of the animating QuickAdjust card in `OpacityMode::Group` (single-leaf
layer ⇒ same semantics); please tell me when the engine path changes so I can drop it.

### Engine next task: Canvas + assembled full-screen replay
User assigned exact Canvas stroke coverage and owned strict workspace Clippy.
I promoted the generic panels QPainter/gray-raster port into ui::qt_painter,
adding QStroker cubic offset/reverse iteration needed by arcs (the chart's old
port strokes only lines). Canvas integration uses the same EGL/GL texture path;
Qt QImage captures will test actual coverage, caps/joins, scales. I am editing
canvas.rs/new qt_painter.rs and only the additive Content/renderer match arms.
For Clippy I saw Cache.rasters type_complexity in qt_rect.rs; alias edits may
race with svc-media (both RasterKey and RasterCacheKey appeared); I'll preserve
one canonical alias after rereading. Full-screen reference request is in
parity-capture.md; missing references remain a failure status.

### BAR follow-up, 2026-10-23 11:38 UTC — submenu + native precision

Bar is now exercising actual Move/Press/Release in the submenu replay instead
of pushing the stack. Controller clears hover on page replacement/back, matching
destroyed QML Repeater MouseAreas (`shell.qml:1041-1068`). New regression covers
both directions. Frozen dark@2 alone contains DND hover; all four independent
original-QML recaptures logged containsMouse=false (bar/artifacts/qt-submenu-trace).
No reference/mask/threshold edits; max23 remains under investigation.

Native precision diagnostic now inspects the actual scene-graph text material,
not only QRawFont: QSGTextMaskMaterial, color=(.537254930,.705882370,.980392158,1),
textureFormat=7 on installed Qt6.12. Trace under
bar/artifacts/glyph-material-gl. Shared renderer currently in-flight (RasterKey
missing at 11:40 build); bar will wait for svc-media to finish rather than edit
its files. Preserve source_texture_size sizing from the prior fix. Bar production
uses Content::Rect + canonical QtQuick; no private rectangle renderer to migrate.

BAR submenu root cause established (same original QML hashes): canonical
unmodified parity-host, env-i-equivalent qt6ct config, two independent replays
produce exact matches for dark@1/light@1/light@2; both dark@2 captures differ
from frozen by max23, paintedMAE3.589038528. A private fixture COPY adding one
explicit hover over DND produces the frozen dark@2 PNG byte-for-byte (SHA256
6ccc45699b3e9b412216b67e34a28d65d4bce1e7c3c1c78dee44d6146806da5d).
Evidence: bar/tools/check_submenu.py, artifacts/submenu-input-check/metrics.json
and artifacts/submenu-explicit-hover/metrics.json. This is hidden input state,
not a renderer residual. QA should stabilize capture pointer delivery and
recapture/review this state; bar keeps the frozen failure and original thresholds.
No shared reference, fixture or source edits. Renderer GPU Cache has now landed;
all-three canonical CPU+GL verifier started privately under bar/artifacts/submenu-gpu.

## svc-media engine-side rendering work — change log (2026-10-23, user-assigned)

User assigned svc-media: consolidated Qt rect mode + PERF-UI-5, then PERF-UI-1..4, 6.
I own `src/qt_rect.rs`. Every edit to other hyprv-ui files was made after re-reading
the current file and is listed here (engine: please keep these when you edit nearby).

- `qt_rect.rs` (owned): one rasteriser, `rasterize_mesh` (exact i64 edge functions,
  span iteration, flat-colour fast path, interpolation relative to the raster origin);
  bit-identical to the previous engine rasteriser on a 5 832-case sweep
  (`hyprv-ui-bench/examples/rect_impl_diff.rs`). New `qt_rect::Cache`: meshes cached by
  (size, radius, pen, premultiplied colours). **GPU canvases draw the mesh with
  `drawVertices`** (cached per device matrix). Skia interpolates vertex colours
  unpremultiplied, so vertices carry barycentric palette coordinates and a runtime
  shader outputs `u·fill + v·border` premultiplied, which is Qt's interpolation. Raster canvases
  cache the CPU raster per full device matrix: absolute f32 positions as in Qt's vertex
  shader (phase-split rasterisation was tried and is not bit-identical: 698/2160 cases). Type aliases
  `VertexKey`/`RasterKey` are canonical (engine's clippy note: done).
  `TIE_RULE` is now `(1, -1)`, GL's top-left in y-up window space. The GPU agrees with it to
  ≤2/255 (`examples/qt_rect_gpu_diff.rs`); the old rule disagreed on 288 cases. Parity
  references do not distinguish the rules.
- `render.rs`: `rect_cache` is now `qt_rect::Cache` and `qt_rectangle` delegates to it
  (PERF-UI-5). `paint` calls `prune` (PERF-UI-3): it runs once per scene revision with a single
  traversal instead of `find` per effect per paint. `node` culls subtrees whose cached
  bounds miss the clip (`subtree_bounds`: per node revision; text uses measured extents;
  `Canvas`/`Shared`/unknown content is never culled; clip limits children; effect
  outset added) (PERF-UI-2). New fields `bounds_cache`, `pruned`, `live`.
- `scene.rs`: `paint_bounds` → `pub(crate)`, new `pub(crate) effects_outset()`;
  `Shadow` and `Effects` derive `PartialEq` (PERF-UI-6).
- `text.rs` (PERF-UI-4): faces carry a `u32` id (atlas/field keys no longer clone a
  `PathBuf` per glyph); shaped lines are looked up by a hash of borrowed fields
  (`HashMap<u64, Vec<(LayoutKey, Rc<Shaped>)>>`, no allocation on a hit); DF glyph quads
  are cached by exact device placement (`df_masks`, previously re-rasterised every paint);
  new `TextEngine::extent` for culling. Output unchanged.
- `image.rs` (PERF-UI-1): `sample_uncached` hoists per-column/row coordinates out of the
  channel loop. Bit-identical on 180 samples (nearest/linear/mipmap); new size 2 294 → 369 µs.

### Mechanical strict-Clippy cleanup — 2026-10-23

The user assigned renderer/text lint cleanup while owners continue parity work. No new lint allowances or rendering changes were introduced. Current scoped command passes: `cargo clippy -p hyprv-ui -p hyprv-ui-panels --all-targets --locked -- -D warnings` (raw JSON/stderr in `docs/reviews/scratch/clippy-mechanical/focused-clippy-6.*`).

- `src/qt_rect.rs`, `Cache.vertices` / `Cache.rasters`: named the existing map types `VertexCache` and `RasterCache`. Preserved the concurrent owner's `VertexKey`/`RasterKey` aliases, vertex-plus-shader payload, palette, eviction, and drawing logic. The first observed raster tuple lint and the subsequent shader-cache tuple lint are both gone; key/renderer changes made by the renderer owner were not reverted.
- `src/text.rs`, `cache_regression` / `df_quad`: moved the existing `#[cfg(test)]` module to the file end. Both its body and `df_quad` were unchanged.
- `src/qt_painter.rs`, `Bezier::normal_at`: removed only redundant outer parentheses around the first argument to `p`; arithmetic and evaluation order are unchanged. This file was being added concurrently; its curve implementation was left intact.

Evidence is being written to [mechanical cleanup evidence](../reviews/scratch/clippy-mechanical/). Before/after parity uses an isolated frozen source/input copy because several agents are changing renderer/input code and running the canonical verifier concurrently. That distinguishes this lint-only delta from other owners' rendering changes. Full workspace all-targets Clippy also reaches unrelated benchmark/shell build failures; those paths are outside this edit assignment. Final measured results are in that evidence directory and the review summary.

BAR native text breakthrough: direct GL use of the canonical native mask with
Qt's `color * texture(mask).a`, ONE / ONE_MINUS_SRC_ALPHA, is **bit-exact4/4**:
glyph alone and over real shared QtQuick rectangle, DPR1/2. Both highp and
mediump variants match, so the broad hypothesis 'precision qualifier alone'
is insufficient; investigate Skia's A8 coverage optimization/color conversion.
Actual Qt GL uniforms are expected137/255,180/255,250/255,1; actual nearest
RED_OR_ALPHA8 texture. Source shader exported from the installed Qt resource.
Evidence: bar/artifacts/glyph-raw-{highp,mediump}/metrics.json; temporary standalone
probe source in bar/.build/raw-glyph (not production). I am trying equivalent
Skia RGBA-mask/color-filter routes before proposing direct-GL renderer hooks.
No shared text/render/EGL changes by bar. Engine: please coordinate native A8
path work here to avoid overlapping edits; finite4-case exactness is diagnostic,
not a full native-corpus pass.

BAR GPU corpus checkpoint: canonical QtQuick Cache replay gives226 current
passes/304 (was224), v1301 (was303), max245 (was23), paintedMAE .041881
(was .029534). Four zero-height menu frames become exact, but two DF frames
regress severely: `cpu-power-medium--light@1.png` max245 MAE2.028697 and
`fullscreen-slide--hide-t0000--light@2.png` max212 MAE1.800269. Output contains
incorrect/garbled textured pixels across bar, not just rectangle edges. Please
investigate shared GPU/text/image resource state; I am running isolated/repeated
replays to distinguish cache lifetime from geometry. Evidence:
bar/artifacts/submenu-gpu/{bar-gl-actual,bar-gl-adapter/gate/gate.json}.
CPU exact-gate224 and v1303 unchanged. Bar has no source-level private CPU rect
substitution to remove: canonical QtQuick now inherits Cache.draw. No claim of
no-regression until these two failures are resolved.

GPU regression follow-up: both failing states replayed together in a fresh
process, twice (52 images per run), now **52/52 pass**, max2, identical aggregate
MAE .0041652408647196845. Thus the two severe corruptions require preceding
corpus state/resource history; they are not deterministic geometry errors in
those states. Full304 replay repeated twice is running. Reproducer:
bar/tools/render_gl_probe.py gpu-isolated --filter cpu-power-medium,fullscreen-slide;
bar/artifacts/gpu-isolated/gate-{0,1}/gate.json. Please check shared cache/resource
lifetime/state interactions while consolidating. Existing full run is retained
as failed evidence; no images overwritten and no claimed no-regression yet.

## ENG-EFFECTS-1 — MultiEffect colorization + mask (spec-bar / panels, 2026-10-23)

Surgical additions to `src/effects.rs` (no other engine file touched; defaults leave
every existing effect bit-identical):
- `MultiEffect { colorization: f32 (0), colorization_color: Color (Qt default #ff0000),
  mask: Option<Mask> }`; `Mask { source: MaskSource, threshold_min (0), spread_at_min (0),
  threshold_max (1), spread_at_max (0), inverted }`, `Mask::uniform()` =
  `updateMaskThresholdSpread`; `MaskSource::Rect { radius }` (the QML's hidden
  `Rectangle { anchors.fill; radius; layer.enabled }`, rasterised by `qt_rect` at item
  size × scale) or `MaskSource::Image(sk::Image)`.
- COMPOSE shader extended in multieffect.frag order: colorization after blur, before the
  shadow; mask after the shadow (`smoothstep` pair, inverted); each branch guarded by a
  uniform so disabled paths do no extra math. One extra child (`maskSrc`, 1×1 transparent
  when off) and 10 floats of uniforms.
- QML users: ControlPanelToggle (colorization 1, colorizationColor), ControlPanelMediaCard
  and DynamicIsland album art (mask, thresholdMin 0.5, spreadAtMin 1). No ColorOverlay /
  OpacityMask in the QML. contrast/brightness/saturation are not ported (unused, Qt
  defaults 0).
- Test `tests/qt_effects.rs` vs a Qt 6.12 render (`tests/data/qt_effects/scene.qml`,
  dpr 2): colorization 1 white, 1 dark, 0.5 red and the masked album art — worst
  premultiplied diff 2/255 (mask edge). GPU path: same runtime shader (no CPU loops).
- Island owner: the island album art can use the same `Mask` (see DynamicIsland.qml:415-432).

GPU Cache migration stable repeats: **two full304 runs byte-identical**,
228/304 current-gate passes (224 before),36 exact (32 before),303 v1 unchanged,
304 structural, paintedMAE .0295340168→.0284993688. No lost gate passes or
increased per-image maxima against previous GL. Native-only max1 failures79→75;
rectangle-edge native error34,388→10,204; text79,011 unchanged. One corner crop
mean increases slightly (still max1); details in
bar/artifacts/gpu-repeated/comparison.json. First canonical GL run's two severe
corruptions are retained as a transient resource/state risk, not silently dropped.
Isolated52-frame runs twice and both full304 runs are clean apart from original
submenu max23. CPU all-three verifier loses no passes/v1 passes/increased maxima
(island0/380, panels80/300, bar224/304); per-image mean variation noted in
submenu-gpu/cross-crate-metric-comparison.json. Canonical verifier currently says
island/panels GPU hooks unwired, so their GPU proof remains unavailable.

Final bar binary verification: third clean full304 GPU replay matches both
prior clean full runs **byte-for-byte**, same228 current/303 v1/36 exact,
paintedMAE .028499368755237113, no new maximum increases. Final provenance
(binary SHA256, frozen manifest SHA256 and observed shared source hashes) and
repeat comparison: bar/artifacts/gpu-final/{provenance,repeat-comparison}.json.
Fresh engine all-target tests43 pass,1 live ignored; bar32 pass,1 opt-in ignored;
bar all-target strict Clippy passes. Direct GL native integration plan and finite
4-case proof are reviewable at bar/artifacts/native-gl-prototype/README.md.
Shared native text/EGL ownership decision is pending; no shared text/EGL edits
were made by bar. Original submenu remains a frozen failure pending QA input
stabilization (private explicit-hover copy matches it exactly).

Mechanical-cleanup final verification: workspace all-targets strict Clippy passes; 67 focused existing tests pass. The isolated canonical verifier produced 1,236 before/after PNG pairs with identical PNG and RGBA bytes, and unchanged Qt-reference gate metrics. Existing reference failures remain; absent island/panels GPU hooks were not counted. [Receipt and scope](../reviews/scratch/clippy-mechanical/README.md).

## BAR approved native GL text + corruption investigation (2026-10-23)

User explicitly authorizes shared text/EGL integration; prioritizes corruption
stress before it. No shared renderer edit yet. Production-path baseline stress
completed6,000 frames,4 surfaces on one SharedEgl,6,000 resizes,63 recreations,
776 delayed checkpoints versus24 isolated GPU oracles: **zero mismatches** on
NVIDIA RTX PRO6000 Blackwell. No glFinish or per-frame readback added. Evidence
bar/artifacts/gl-stress-before/results.json. This does not rule out the earlier
transient; next stress adds stable-size partial damage and longer asynchronous
bursts (257 frames). Retained readback corruption cannot be explained solely by
EGL buffer age/presentation damage: that image is painted before the EGL blit.

Approved integration plan: new private `src/native_gl.rs`; surgical text.rs
NativeRendering branch; EGL context-owned resource lifecycle in egl.rs; additive
private module/dependency. Keep canonical shaping/masks and CPU/unsupported-clip
fallback. Flush Skia before raw GL at scene traversal position, use active
surface target/origin/clip, reset Skia GL state afterward. Cache program/VBO/mask
textures per current context, clean up while that context is current. Test
multiple surfaces/resizes/context teardown, clips/opacity/layers + all-three
canonical parity. Re-read shared files immediately before each edit; other
owners retain qt_rect/effects/canvas ownership.

Shared edits now made after re-read: Cargo.toml adds glow0.16 (Cargo.lock resolves
it); lib.rs private native_gl module; new native_gl.rs owns context-scoped cached
program/VBO/VAO/RGBA mask textures with8MiB/512-entry eviction. egl.rs owns its
Rc beside Skia, deletes it while EGL current, registers only the retained
BottomLeft canvas/FBO during Renderer.paint (FlushWrite before raw access), and
resets Skia after native setup. text.rs keeps canonical native placement/masks,
collects eligible glyphs for one ordered GL batch per text node; unchanged Skia
fallback for CPU/affine/nonrect clips. render.rs adds NativeGl error and a scoped
block for saveLayer subtrees, preventing raw writes to the wrong internal target.
No qt_rect/effects/canvas edits. cargo check passes. No explicit wait/fence/
glFinish was added: all commands share the same ordered EGL GL stream.

Second baseline stress completed12,000 frames,4 surfaces,4,001 resizes,122
recreations,1,476 partial updates,188 checkpoints after257-frame bursts: zero
mismatches. Total baseline18,000 frames/964 checkpoints. Actual corpus stress
3,040 frames is running on the pinned pre-integration binary; first304 clean.

Added egl.rs diagnostic HYPRV_NATIVE_GL_DISABLE A/B switch to exercise the
unchanged Skia path on the same compiled binary. Necessary for before/after GL
proof in copied harnesses: island/panels currently lack production GPU hooks.
Their source paths remain untouched; bar will run private copies of their test
translators, changing only output transport to QA's canonical EGL helper. Same
fixtures, public UI implementations, assertions and canonical comparator.

Added tests/native_gl.rs + four independent Qt goldens under
 tests/fixtures/native-gl. Explicit private-Weston test passes:4 exact goldens,
rectangular scissor, fractional DPR, inherited opacity, and byte-identical
fallback for rounded clip/group opacity/blur layers. New module constructor now
cleans partial GL allocations on failure; egl.rs invalidates Skia state even
on constructor error. GLES2 without VAO support skips VAO calls (OES fallback
loader retained). No normal-draw arithmetic changes in this cleanup.

Post-integration12,000-frame burst stress also passes all188 checkpoints,
1,476 partial updates/4,001 resizes/122 recreations. Corpus pre-integration
2,736/3,040 frames clean so far. Added a second stress mode with XDG windows
mapped ONLY inside private headless Weston, to exercise compositor-held buffer
release/backpressure (unmapped surfaces alone cannot establish that). Running
12,000 before/after frames each on the same binary, native GL disabled/enabled.
No live desktop surface or compositor change. GPU golden/clip/layer tests pass;
shared engine all-target tests pass. Native cross-crate copies are linked once,
then executed before/after on the same binary; canonical comparator unchanged.

Shared renderer follow-up after re-read: body() now blocks direct native GL for
all AA clip_rrect branches, including rectangular AA clips (is_clip_rect alone
is insufficient). Added explicit fractional Skia-mode clip regression: byte-
identical fallback; private GPU test passes. README documents unchanged public
entry points, context-owned8MiB cache, fallback coverage and diagnostic switch.

Stress totals now57,040 frames:30,000 synthetic before +24,000 after +3,040
actual frozen bar corpus before (10 complete304 passes). All zero mismatches.
Includes four mapped private-XDG surfaces, compositor release/backpressure,
resizes/recreation, one/shared/per-surface renderers, texture eviction and partial
damage. No speculative fence/age workaround added; previous two corruptions have
not reproduced. Evidence under bar/artifacts/gl-{stress-*,mapped-*,corpus-before}.

Cross-GPU A/B global enablement loses no gate passes (island0/380, panels82/300),
but increases max by1–2 on three already-failing animation/calendar images.
Rather than force an unrequested change into those owners' current text maps,
I am making NativeTextMode explicit per Node (default Skia, QtGl opt-in), and
opting bar text leaves in. This preserves existing UI behavior byte-for-byte
while enabling the proven path in the assigned UI. New scene.rs enum/property
and renderer Text branch are surgical; bar view sync copies the property. Other
owners can migrate leaf text after their own parity checks. Original global A/B
receipts are retained at native-cross-gpu, not hidden. Final opt-in A/B uses
fresh pinned binaries/artifacts. Native GL goldens/stress explicitly opt in.
- **No-regression evidence:** the same MultiEffect scenes (shadow, blur 48, offset/scaled
  shadow with `source_texture_size`, default) render **byte-identical** with the old and new
  `effects.rs` at scales 1, 1.5, 2. `parity/verify` (3 runs: old engine, new engine, new
  engine + panels migration): island gate-v1 16/380 in all, bar 303/304 (CPU pass 224→228
  from concurrent bar work, GL 228 unchanged), panels 248/248, no panels capture worse.
- **Panels migrated** onto engine image nodes + effects: toggle icons (`Content::Image` of
  the QtSvg bake, `ImageSampling::Mipmap`, MultiEffect colorization), Wi-Fi IconImages
  (Mipmap), album art (Mipmap + `Mask::Rect`), shadowed glass (`source_texture_size`).
  Panels' private sampler/colouriser/mask/glass compositor are deleted.

Final opt-in API implemented after re-read: scene.rs NativeTextMode::{Skia,QtGl}
(default Skia) + Node.native_text_mode. Renderer Text branch blocks unselected
leaves; bar view/wrapped text selects QtGl and sync copies it. README public API
updated. Renderer caches whether the scene needs native GL by root/revision;
EglWindow skips GL setup/FBO access entirely when no leaf opts in. Thus existing
island/panels take exactly their original EGL path, not just a shader fallback.
No API/behavior edits in other UI crates. GPU stress/golden text explicitly opts
in, so tests cannot accidentally verify only the legacy default.

### Engine Canvas stroker checkpoint (2026-10-23)
`CanvasPath::stroke_node` now routes through the QStroker cubic offset algorithm,
QRasterizer analytic line coverage and gray raster with Qt source-over bytes.
84 independent Qt6.12 QPainter goldens (arcs, circles, diagonals, curves, polyline
joins, 1px grid, overlapping track/progress; all caps; DPR1/1.25/1.5/2) match with
max0 on CPU **and actual EGL RGBA8** on private headless Weston. `CanvasBatch`
keeps ordered paints within one texture (needed for track+progress rounding);
see engine README. Production has no Qt linkage. Solid paints only; arbitrary
cosmetic curves/dashes/transforms are not part of this port's proven subset.
Strict workspace Clippy now passes, including co-owner NativeTextMode changes.

Final bar A/B initially had no lost passes/increased maxima, but mean error grew
in12 translucent battery crops. To keep this integration inside independently
validated coverage, text.rs raw eligibility now requires opaque text color and
inherited opacity1. Translucent paint/fades retain the original path (no feature
loss, no fixture-specific condition). Added translucent-color fallback test;
README explicitly documents this limit. Opaque A8 path remains4/4 Qt-exact;
bar native-mode flag remains set so these leaves migrate when coverage expands.
This avoids silently accepting an alpha rounding regression based only on gate
pass counts. Re-rendering final bar corpus after this guard.
- **[ENG-IMG-1] trilinear edge residual (island fixture).** With `ImageSampling::Mipmap`
  the island agent logo (256 px PNG → 18 device px at @1, LOD ≈ 3.8, DynamicIsland.qml:545)
  matches Qt better overall but its right/bottom painted-rect edge differs up to 112/255
  (`parity/reference/island/agent-compact-running--dark@1.png`, x≈281 y≈32). Suspect LOD
  selection or clamp/half-texel handling at minified levels in `image::sample_uncached`.

Final current opt-in GPU proof: island380/380 and panels/calendar300/300 PNGs
are byte-identical disabled/enabled (native-cross-final/png-comparison.json).
Bar opaque-only path230/304 exact-rule passes (was228),38 exact (was36),303 v1,
304 structural; paintedMAE .0284993688→.0278794348, with **zero lost passes,
zero increased maxima and zero increased per-image means**.73 max1 native-only
failures + original submenu max23 remain. Final bar comparison receipt:
bar/artifacts/native-bar-opaque/comparison.json.

Stress aggregate69,040 frames /4,760 image comparisons,25,982 resizes,677 window
recreations,9,304 partial damage updates: zero pixel mismatches. Latest mapped
stress alternates changed rectangular patches under partial damage, rather than
only repainting static content. Driver NVIDIA RTX PRO6000 Blackwell. Source/
binary fingerprints in latest stress receipts. Qualification: old corpus launcher
flagged the reference *pathname* being rebuilt by the subsequent check during
its run/postprocessing; its child completed all10 passes and all3,040 images
matched the old independent oracle. That launcher warning is retained, not
reported as a clean exit. Synthetic/mapped stress uses its own executable and
latest runner pins it in a private directory.

New parallel `tests/zz_mip.rs` currently blocks full engine all-target test/Clippy:
requires MIP_DIR but unconditionally unwraps it; two needless_range_loop lints.
No native GL file/test lint failure. Please owner move/ignore the temporary probe
or provide its fixture env. Bar strict all-target Clippy and32 behavior tests
pass; native GL explicit private GPU test passes. Running established engine
suite separately while preserving zz_mip owner work; no unrelated test edits.

QA comparator note: canonical verifier reconstructs fractional float RGB from
straight PNG, so it reports228 bar passes versus230 under actual RGBA8 integer
premultiplication. The two new corner captures are RGBA8-exact. No comparator,
mask, reference or acceptance-rule edit was made; both reports are retained.

## ENG-IMG-1 fixed (spec-bar, 2026-10-23) — `src/image.rs` only

Isolated with a Qt 6.12 fixture (`tests/data/qt_mip/scene.qml`, dpr 1 and 2: 256 px and
816 px sources, 18/24/11/58 px painted rects, x/y phases 0, ¼, ½, ¾). Two causes:
1. **Edge tie rule.** Pixel centres exactly on a quad edge (an image at y + 0.5 device px)
   were included on the top and excluded on the bottom. Qt/GL use the top-left rule in
   y-up window space = left + bottom in y-down space, the same `qt_rect::TIE_RULE` the rect
   rasteriser uses. `sample_uncached` now takes rows with `r.y < c ≤ r.y + h`. Before: up to
   223/255 (whole first/last row); after ≤ 2.
2. **Odd mip levels.** `source_levels` averaged 2×2 blocks and dropped the last odd
   row/column (816 → … → 51 → 25). Now each texel averages its exact source footprint
   `[i·k, (i+1)·k)` area-weighted (identical to the 2×2 average for even sizes). Before:
   125–192/255 on the 816 px logo at 11–24 px; after ≤ 2. (A linear-filtered blit got
   13–23.)
Test `tests/qt_mip.rs` (≤ 2/255 at both scales). All engine tests pass.

Established engine suite (all named test targets except the parallel zz_mip
probe, library and examples), native GL GPU test, engine library/native-test
strict Clippy, and bar all-target strict Clippy now pass. Full all-target
attempt/failure remains recorded separately. No temporary probe or other
owner's test was edited. Native eligibility is opaque-only, QtGl per-node;
all final bar per-image max/mean deltas are non-increasing. Final validation
and stress qualification are summarized in bar/VALIDATION.md. No live surfaces,
reload, install, commit or runtime Qt dependency was introduced.

### Bar checkpoint: legacy corner topology and battery selector

Bar `surfaces::corners` now returns one all-edge anchored `(0,0)` overlay per
output, exact legacy namespace and empty input region; four children follow
`BarScreenState.qml:22-23` offsets. `assembly.rs:831` already mounts returned
requirements, so production mounting needs no shared-file edit. The older
`host.rs` model still describes four corner roles, but main.rs routes it only
through `--headless`; no live surfaces use that path. Engine owner: align that
headless model when convenient. No engine source changes in this checkpoint.
The contradictory four-tiny-surface optimization paragraph in ui-bar.md is
superseded by the user's legacy topology requirement and its own topology table.
Battery history now selects only the first BAT* directory, never a peripheral
fallback. Original equivalence failure remains preserved; new regression and
read-only selection evidence are recorded in the bar validation log.
GPU corruption remains explicitly OPEN in docs/STATUS.md with bounded stress
evidence and the corpus launcher provenance qualification.

### Remaining bar GPU crops: experiment plan (2026-10-23 12:30 UTC)

User requests continued rectangle/native-text/compositing parity work, max90min.
Re-read current qt_rect.rs, native_gl.rs, render.rs, egl.rs before any edit.
Qt smoothcolor.vert multiplies normalized premultiplied vertex colors by opacity
BEFORE interpolation; Skia's palette applies paint opacity afterwards. Test a
bounded GPU mesh cache/direct smoothcolor path using the already registered GL
context (new qt_gl_rect.rs, minimal hooks in native_gl/qt_rect). Initially only
canvases already eligible for native GL are affected. Preserve fallback for
complex clips/save layers; investigate independently owned offscreen surfaces
only after root-target proof. Rect geometry remains canonical qt_rect tessellation.
All shared edits and final retained/reverted experiments will be logged here;
run engine tests and all three UI parity after final shared changes. Do not
modify frozen references, comparator, fixture labels or acceptance thresholds.

Engine functional E2E checkpoint: currently building host native virtual-input
suite. Concurrent qt_gl_rect.rs:79/82 uses Matrix::trans_x/trans_y (Skia0.90 calls
translate_x/translate_y), blocking shell compilation. Please fix in your ongoing
backend edit; engine is leaving that file alone.

Direct rectangle experiment now hooks canonical Cache.draw only when an existing
native-GL target is eligible. qt_gl_rect.rs owns bounded device-mesh VBOs, retains
Rc meshes to prevent pointer-key reuse, normalizes premultiplied u8 vertex color
and applies opacity in vertex shader. No alternate CPU raster or geometry fork.
Next isolate Qt-owned layer-source surfaces: register their resolved FBO for the
body call with scoped target/block restoration, so chrome can use the same mesh
shader without drawing into the root framebuffer. No root saveLayer bypass.


### Integration CI — 2026-10-23T12:32:38+00:00 — clippy: mismatched types

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-ui/src/qt_gl_rect.rs:76:16`.

`crates/hyprv-ui/src/qt_gl_rect.rs`: mtime 2026-10-23T12:32:32+00:00; SHA-256 `3fd4b24d409d7746428345b7bb091823edc1b676cd8ffcdf2f1ac3bd12d8608d`; changed during command: True.

```text
error[E0308]: mismatched types
  --> crates/hyprv-ui/src/qt_gl_rect.rs:76:16
   |
75 |         match setup {
   |               ----- this expression has type `Result<(NativeUniformLocation, NativeUniformLocation), String>`
76 |             Ok((matrix, pixel_size, opacity)) => Ok(Self {
   |                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ expected a tuple with 2 elements, found one with 3 elements
   |
   = note: expected tuple `(glow::NativeUniformLocation, glow::NativeUniformLocation)`
              found tuple `(_, _, _)`
   = note: the full name for the type has been written to '/home/wilf/.config/HyprV/rshell/target/debug/deps/hyprv_ui-dc9b28004e744f13.long-type-3636877479654868614.txt'
   = note: consider using `--verbose` to print the full type name to the console


```

Full log: `docs/verify/raw/ci/round-13/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.

Independent native-alpha probe found QQuickText::setColor stores QRgb8, not
QColor16. New qrgb8 conversion before premultiplication makes all64 combinations
(16 colors/opacity cells ×2 DPR ×transparent/bar background) exact. Enabling
translucent path diagnostically via HYPRV_NATIVE_GL_ALPHA pending corpus proof.
Full smoothcolor vertex shader port in qt_gl_rect now matches bare Qt probe
exact@1 and differs in one channel@2; effect probe remains max1. Next isolate
MultiEffect shader/sample math in new qt_gl_effect.rs on registered GPU contexts;
keep CPU and non-opted-in UIs unchanged and preserve all prior receipts.

First full qrgb8-alpha GPU replay: native-only text category error73,102→0;
all44 text-bearing native crops now have zero text error. Gate remains230 due
other pixels. Direct Qt MultiEffect downsample/compose experiment improves
compositing3,971→2,220 and gate230→234, exact38→42 (rect edges still max1).
Next test direct premultiplied image blending (same cached sampled image,
texture*opacity before blend) for the fading menu icon residuals; no decoder or
SVG reference change. Native GL remains limited to registered eligible canvases.

Remaining effect edge errors are concentrated in shader fringe pixels. Qt draws
MultiEffect's final quad directly into the destination; current engine quantizes
it into an intermediate image first and then blits. Experiment: keep prepared
source/blur textures in the existing node revision cache and composite directly
on eligible registered rectangular-clip canvases, with Qt opacity in the fragment
shader. Preserve image/saveLayer fallback for unsupported matrices/filters and
non-opted-in contexts; no shared output/fixture/crop changes.

Coordination: image.rs changed concurrently at08:36:57; its original sampled-image
hook disappeared. Moved our blending hook to render.rs Content::Image and
native_gl::sampled_image, leaving current image.rs untouched. Image owner: keep
your sampler changes; please do not remove the native hook in render.rs. Programs
qt_gl_rect/qt_gl_effect now compile; earlier CI's mid-experiment tuple diagnostic
is resolved. Full parity/tests will follow stabilized experiments.

### svc-desktop → engine: P1 crash on output rotation, `wp_viewport` "Box doesn't fit" (2026-10-23)

Found in the isolated cutover sandbox (nested Hyprland 0.56.2 on headless KWin; see [docs/verify/cutover-rehearsal.md](../verify/cutover-rehearsal.md)). With the native shell running, changing an output's transform kills it:

```
hyprctl eval 'hl.monitor({ output = "WAYLAND-1", mode = "1280x720@60.00", position = "0x0", scale = "1", transform = 3 })'
→ wp_viewport#24: error 0: Box doesn't fit
→ hyprv-shell: Wayland: Protocol error 0 on object wp_viewport@24   (exit)
```

Before exiting it logged `Protocol error 0 on object wp_viewport@24` about 138 000 times, which is a busy loop on the dead connection. It looks like the viewport source rectangle is still set from the pre-rotation buffer size when the rotated configure arrives. SUPER+T (`rotate-screen-ccw.sh`) triggers this in real use; the unit's `Restart=on-failure` brings the shell back. The patched rotate script no longer depends on the shell process: `hyprv-shell desktop deskflowReflow` falls back in-process. Please (a) recompute or clear `set_source` before committing a buffer of the new size, and (b) stop the render/log loop on a fatal protocol error.

Found a canonical geometry gap: m_radius>0 enters Qt's rounded branch even when
radius is clamped to0 (<=0.5). Qt's minimum inner radius0.01 yields four rows and
bottom edge h-0.01; port treated it as the ordinary radius=0 rectangle (two rows,
bottom h). Independent installed-Qt geometry oracle confirms all16 vertices for
300×1 radius1. This explains the 600-pixel battery divider row's one-unit error.
Implementing the uniform clamped-radius branch in qt_rect tessellate, preserving
normal radii and radius=0. Keep a Qt geometry golden regression.


### Integration CI — 2026-10-23T12:44:27+00:00 — clippy: manual implementation of `.is_multiple_of()`

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-ui/src/qt_rect.rs:353:29`.

`crates/hyprv-ui/src/qt_rect.rs`: mtime 2026-10-23T12:43:56+00:00; SHA-256 `86f64e6c9c2870d1950c8b8fc5f0c49c9e2f879419da4b03d0d0a348b943845d`; changed during command: True.

```text
error: manual implementation of `.is_multiple_of()`
   --> crates/hyprv-ui/src/qt_rect.rs:353:29
    |
353 |         if ptr.is_null() || (ptr as usize) % std::mem::align_of::<u32>() != 0 {
    |                             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ help: replace with: `!(ptr as usize).is_multiple_of(std::mem::align_of::<u32>())`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.95.0/index.html#manual_is_multiple_of
    = note: `-D clippy::manual-is-multiple-of` implied by `-D warnings`
    = help: to override `-D warnings` add `#[allow(clippy::manual_is_multiple_of)]`


```

Full log: `docs/verify/raw/ci/round-14/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T12:44:27+00:00 — clippy: this function has too many arguments (8/7)

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-ui/src/native_gl.rs:320:5`.

`crates/hyprv-ui/src/native_gl.rs`: mtime 2026-10-23T12:44:03+00:00; SHA-256 `ee016c0f8d45bd091a8de9ce2bde747a3caeac3550d5e4623806831f5924fd00`; changed during command: True.

```text
error: this function has too many arguments (8/7)
   --> crates/hyprv-ui/src/native_gl.rs:320:5
    |
320 | /     fn draw(
321 | |         &mut self,
322 | |         target: &Target,
323 | |         glyphs: &[Glyph],
...   |
327 | |         image_size:Option<(f32,f32)>, linear:bool,
328 | |     ) -> Result<(), String> {
    | |___________________________^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.95.0/index.html#too_many_arguments
    = note: `-D clippy::too-many-arguments` implied by `-D warnings`
    = help: to override `-D warnings` add `#[allow(clippy::too_many_arguments)]`


```

Full log: `docs/verify/raw/ci/round-14/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T12:44:27+00:00 — clippy: this function has too many arguments (8/7)

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-ui/src/qt_gl_effect.rs:126:5`.

`crates/hyprv-ui/src/qt_gl_effect.rs`: mtime 2026-10-23T12:37:52+00:00; SHA-256 `f9131bc1452203bfbb4f685902ca4b0874276fddaddec6b90e4b3d55fcfce790`; changed during command: False.

```text
error: this function has too many arguments (8/7)
   --> crates/hyprv-ui/src/qt_gl_effect.rs:126:5
    |
126 | /     pub(crate) unsafe fn draw(
127 | |         &self,
128 | |         gl: &glow::Context,
129 | |         textures: &[(sk::gpu::gl::TextureInfo, sk::gpu::SurfaceOrigin)],
...   |
132 | |         size: (i32,i32), rect: [f32;4], opacity: f32,
133 | |     ) -> Result<(), String> {
    | |___________________________^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.95.0/index.html#too_many_arguments


```

Full log: `docs/verify/raw/ci/round-14/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.

### svc-media follow-up — CPU raster reveal regression (2026-10-23)

`qt_rect.rs` only (no other hyprv-ui file touched). CPU/raster path, output bit-identical:
- `coverage()` + `Coverage`: opacity-independent work (exact integer row spans, flat-run
  list, interpolated pre-opacity colours) cached per mesh + device matrix; a fade or
  reveal only replays blending. `render_window` replays just the pixels inside the clip
  (reveal clips), `raster_window`/`rasterize_mesh_window` expose it.
- Flat triangles: one blend per uniform destination run, word-sized fills; the first op
  writes into the known-zero buffer directly. `round_even` replaces the libm
  `round_ties_even` call (exact for |v| < 2²²).
- One-off rasters (each fade frame) render into a reused scratch buffer and are drawn as
  borrowed images: no fresh pages per frame. A repeated key (static content) earns a
  persistent cached image. Rasters are written straight into Skia-owned bitmaps (no copy).
- Proof: `hyprv-ui-bench/examples/rect_impl_diff.rs` compares against a verbatim copy of
  the previous rasteriser (`examples/support/cpu_ref_qt_rect.rs`): 99 840 cases + 499 200
  windowed rasters vs crops, 0 differing bytes/bounds. Unit test
  `clipped_cached_draw_matches_uncached`.

Qt mesh oracle shows ring triangle sets match, but triangle order starts on the
bottom-left pair in Qt versus bottom-right in the port. Preserve Qt's ring start
and GL triangle-strip odd-triangle winding; this matters for overlap/float
interpolation at corner joins. Thin-radius geometry regression uses installed Qt
oracle, not inferred expected vertices. Continuing independent probes.

Image refinement: direct texture sampling now uses GL mipmaps for minification,
not the CPU quantized mip oracle; texture cache retains original source and
bounded GPU allocations. Plain magnification uses actual GL linear filtering.
Native text qrgb8 alpha is enabled after64 independent exact cells. Render uses
prepared MultiEffect textures when eligible, preserving children and fallback.
Testing desktop GL vs GLES diagnostically (HYPRV_EGL_DESKTOP_GL): Qt RHI on Linux
may use desktop GL while engine binds GLES; do not change runtime default absent
proof. Egl API/config/context attributes are the only diagnostic edits.
- **ENG-PQ4 closed for the island ring** (spec-bar, 2026-10-23): `hyprv-widgets`
  ProgressRing now builds `CanvasPath::arc` paints in a `CanvasBatch` (engine qt_painter
  stroker), no engine edit needed. Island: 134 captures better, 0 worse; max ≤ 8 29 → 133.

Qt API hypothesis rejected: private instrumented canonical host reports
OpenGL ES3.2 NVIDIA615.78.08, same API as engine. DesktopGL experiment improves
some rounding but does not match legacy API; runtime default remains GLES and
this diagnostic will be removed. Texture trace proves Qt image atlas512×512:
control source32×32, cover-warm16×16 at both DPRs, with deterministic UV subrects.
The remaining image max1 is sampling/atlas precision, not SVG decoder data (offline
Qt raw premul versus Skia PNG decode is byte-identical for both control icons).
Independent native tests2/2 pass, including64 alpha/opacity cells; engine full
all-target tests49pass/3ignored. Starting all-three GPU cross-checkpoint now.

Engine strict-Clippy follow-up: exact workspace command currently stops at
qt_rect.rs966 log_once after cache_tests882 (clippy::items_after_test_module).
Please move your new helper above that test module; engine is leaving the shared
rectangle backend file untouched. Host's own Clippy findings were fixed.

### svc-desktop → engine: rotation crash FIXED in hyprv-ui (2026-10-23, at the user's request)

The edits are minimal; I re-read each region right before editing. Please keep them, or tell me where you want them moved.
- **Root cause.** `paint()` called `EglWindow::draw()`, which ran `make_current` and only then `wl_egl_window_resize`. A resize only reaches a later back buffer (NVIDIA egl-wayland already holds the current one), so the first frame after a rotated configure committed the old-size buffer together with the new `wp_viewport` source, and Hyprland rejected it with "Box doesn't fit".
- **`wayland.rs` `paint()`.** It computes the physical size first. If the window's size differs (new configure or new scale), it **recreates the EGL window**, so the first buffer has exactly the size the viewport source and destination describe for the latest configure, all in one commit. It also returns immediately once the engine has stopped (after a protocol error nothing is drawn or committed again).
- **`wayland.rs`.** New `output_logical_size(&MonitorInfo)` uses xdg-output's logical size, else the current mode divided by scale, with w/h swapped for 90/270 (flipped included). New helper `swap_for_transform()`.
- **`egl.rs`.** `EglWindow::size()`.
- **`qt_rect.rs`.** The native rect error print is deduplicated: each distinct message is printed once, at most 64 kinds. That was the ~138 000-line busy loop.
- **hyprv-shell.** `assembly.rs` and `host.rs` `output_added` use `output_logical_size` instead of the unrotated mode fallback. `nested.rs` appends `docs/verify/e2e-rotation.md` to the generated `docs/verify/e2e.md`, so the rotation matrix survives your regenerations.
- **Regression test.** `tests/nested_rotation.rs` (opt-in) plus `tests/run-nested-rotation.sh [report]`, which runs it in bwrap with a clean env, only the render node and NVIDIA control nodes, and private D-Bus/KWin/Hyprland. It covers 3 scales × the 8 transforms and back (27 steps), with a bar, an overlay and two popups mapped. It asserts no protocol error, engine and compositor sizes, and `output_logical_size`. With the fix disabled it fails at scale 1, transform 90 with the original error. The matrix is in `docs/verify/e2e-rotation.md`.
- `cargo test -p hyprv-ui`: all pass. No clippy warnings in the touched files.

Sampler precision found from installed Qt qsb: its MultiEffect GLSL100 declares
`uniform highp sampler2D`. GLES sampler precision is independent of default
float precision; our experimental effect shaders had unqualified (lowp) samplers.
Desktop GLSL ignored this and incidentally reduced the fringe errors. Correcting
GLES sampler qualifiers to Qt highp; no EGL API change retained. This is tested
against extracted installed Qt shader sources, not a pixel-specific correction.

Highp samplers remove the long battery border discrepancy; independent effect
probe mean falls ~10× further. Aligning textured quad order with QSGGeometry
(TL,BL,TR,BR). Next preserve Qt's precomposed projection/model-view matrix in
RGBA image vertex shading rather than translating device positions before
projection; this affects subtexel interpolation at exact rounding ties.

## ENG-CANVAS-GRAD — vertical gradient fill on `CanvasPaint` (spec-bar, 2026-10-23)

`src/canvas.rs`: `CanvasPaint.gradient: Option<VerticalGradient { y0, y1, top, bottom }>`
(Canvas `createLinearGradient(0, y0, 0, y1)` with stops 0/1; precedence over `fill`),
included in the raster cache fingerprint, replayed through the existing
`qt_painter::Brush::VerticalGradient`. Literal users updated with `gradient: None`
(canvas.rs `stroke_node`/`fill_node`, tests/canvas_strokes.rs, hyprv-widgets ProgressRing).
All engine tests pass (84-case canvas_strokes unchanged).
Panels' SystemTrendChart now uses `CanvasPath`/`CanvasBatch` + this gradient; its private
`qpainter.rs` (QPainter port) is deleted. The engine raster is byte-identical to it for
every chart operation at dpr 1 and 2.

## ENG-CANVAS-1 — `Content::Canvas` blit vs a skia picture blit (open question)

Drawing the same `CanvasBatch::raster` image via `Content::Canvas` (engine
`image::draw` exact copy) vs a skia `Picture` with `draw_image_rect` + Linear at the same
integer device position gives ±1 differences in ~350 px of a 300×108 chart over a
translucent background (F16 surface; more on 8888). Against Qt: on a transparent
background the engine node is marginally closer (Σ|d| 738 vs 747, worst 1 both); on the
panels' translucent glass (shared parity, F16 harness) the picture blit is closer on light
themes, the engine node on dark ones. Panels keep the picture blit
(`widgets::canvas_picture`) so stats parity is byte-identical; please look at the
compositing order of `Content::Canvas` (paint alpha / save_layer) when convenient.
Repro: render one chart both ways at (20, 20) on a 0.22-alpha white rect, scale 2.

### Soak → renderer owner: SOAK-1 [P3 investigation] (2026-10-23)

[62-minute report](../verify/soak.md), [raw analysis](../verify/raw/soak/release-62m/analysis.json). Pinned release SHA256 `a06f202398919db3917914c85bb17a6a12cc3b838e81c097d218e14a929281b1`, 534 successful replay steps, clean exit, no prolonged render/submission stall. Matching settled phases after the full workload warmed retained **220 KiB private dirty** (233 KiB/hour fitted) and **5 MiB NVIDIA process graphics accounting** (56→61 MiB, later plateau). RSS fell 575→564 MiB; fds/threads/kernel-timer counts were bounded. This is a verified retention observation, **not proof of an unbounded leak**. Please attribute cache/allocator/live-GPU bytes with bounded, read-only diagnostics and repeat the retained-binary workload after warming a fixed working set. No eviction change is justified solely by NVML reserved-memory growth. Exact causal source line is unassigned: renderer sources changed during the run. Full model/render/presentation timing and logical timer cardinality remain an observability gap; EGL draw/submission p50/p99 were 0.530/3.278 ms. All operations were private/unprivileged; no live session/provider mutation.

### ENG-CANVAS-1 — resolved as "no engine bug" (spec-bar, 2026-10-23)

- `Content::Canvas` composites the premultiplied Canvas image with the item opacity in
  float, which is what Qt's QSGImageNode does. At integer device positions it is an exact
  texel copy (checked against hand-computed src-over, opacity 1 and 0.5).
- The stats charts sit at integer positions/sizes, so Qt and both blits do an exact copy.
- On an **RGBAF32** capture surface the engine node and the old skia picture blit are
  **byte-identical** for all 16 stats captures. The ±1 seen before is the shared-parity
  harness's **F16** storage (the picture path's extra save_layer adds one more F16 store).
- Switching the panels harness to F32 is not an option: 144 captures better but 19 worse,
  and `qa-show-hide--show-t0060--dark` fails (painted mean 1.08) — F16 storage partly
  stands in for Qt's per-draw RGBA8 framebuffer rounding. The real fix remains the
  GPU-exact per-draw blending request above (highp blend + 8-bit store per draw); with it
  both blits would be identical and Qt-exact.
- Panels therefore keep `widgets::canvas_picture` (byte-identical under the current
  harness); switch to `CanvasBatch::node` once per-draw blending lands.

Layer AA precision experiment: Qt source layers construct an orthographic matrix
from the padded logical source rect. Engine currently composes physical pixel
scale then projection, introducing an extra f32 cancellation. Track the source
canvas basis in its scoped native target; when a rectangle uses that exact basis,
construct its matrix directly from the logical source rect (AA pixelSize remains
physical). Other transforms retain the established device matrix. Generic scene
semantics, no state/theme/fixture condition. Atlas-position and vertex-UV-flip
experiments were negative; do not retain those experimental runtime knobs.

Next image material correction: Qt removes alpha from fully opaque decoded images
(qquickpixmapcache.cpp maybeRemoveAlpha), then uses opaque texture material with
blending disabled at opacity1. Our direct RGBA path always enabled blending and
multiplied by vec4 opacity. Match Qt's opaque/no-multiply material and its scalar
opacity material for alpha/fades. Cached decoded RGBA determines opacity, no
fixture/asset-name condition. Root/mip source pixels are independently verified
identical to Qt's actual QQuickImagePrivate currentPix image at DPR1/2.

## Combined renderer re-review — 2026-10-23

- [UI-R2-1] [P2] GPU-backed Content::Image becomes a fatal native upload error when a QtGl text sibling enables the pipeline; [verified scenario and fix](../reviews/hyprv-ui-r2.md#ui-r2-1).
- [UI-R2-2] [P2] CPU blending-mode changes reuse old captured MultiEffect pixels; [1,568-byte regression and invalidation fix](../reviews/hyprv-ui-r2.md#ui-r2-2).
- [UI-R2-3] [P2] Renderer cache clearing leaves EGL-owned mesh/texture caches resident (300 VBOs / 2.34 MB reproduced); [lifetime boundary and suggested API](../reviews/hyprv-ui-r2.md#ui-r2-3).
- [UI-R2-4] [P2] Native mipmap storage is uncharged, allowing 10.67 MiB under the 8 MiB budget; [probe and accounting fix](../reviews/hyprv-ui-r2.md#ui-r2-4).
- [UI-R2-5] [P2] Shared-subtree effects are incorrectly pruned on unrelated edits and repeatedly rebuilt; [reachability probe and fix](../reviews/hyprv-ui-r2.md#ui-r2-5).
- [UI-R2-6] [P3] TextEngine::clear omits placement-specific distance-field raster masks; [five retained masks and fix](../reviews/hyprv-ui-r2.md#ui-r2-6).

Integration correction before final validation: native material activation must
not depend on whether a scene happens to contain QtGl text. Add inherited
Node.native_materials:Option<bool> (defaultNone, basefalse), explicit bar/view,
popup and corner-root opt-in. Shared subtree boundaries reset material scope,
so foreign island/calendar content does not inherit the bar's mode. Text mode
remains independent. Required for actual corner-only EGL surfaces (no text).
Update bar retained reconciliation and README; other UIs retain their defaults.

## UI-R2-1..6 implementation — 2026-10-23

User assigned these six fixes here while engine was busy; re-read/patch edits preserve the concurrent `native_materials` opt-in and layer-basis work.

- **UI-R2-1:** `native_gl.rs` catches optional image/mask upload failures, logs once per process, and uses context-aware CPU-copy/Skia fallback. Failed texture allocations are deleted before cache insertion. A partially uploaded text batch falls back only for remaining glyphs; normal batches keep their existing flush/submission behavior. Regression coverage includes a real GPU snapshot, injected failure, next-frame recovery and translucent partial-batch alpha.
- **UI-R2-2:** `render.rs` invalidates captured effects when CPU blending mode changes; the probe checks both directions against fresh output.
- **UI-R2-3:** `Renderer::clear_caches` now explicitly invalidates a native cache epoch; subsequent draws synchronize it even after a scene stops requesting native materials. `SharedEgl::clear_caches` and `Engine::clear_render_caches` provide immediate coordinated CPU/native/Skia cache release while the owning EGL context is current, restoring the caller's prior EGL bindings. `qt_gl_rect.rs` adds an 8 MiB mesh byte budget alongside the entry cap. Tests require populated caches before flushing and exercise the actual SharedEgl API on private Weston.
- **UI-R2-4:** `native_gl.rs` budgets complete mip chains for initial uploads and Linear→Mipmap upgrades, protects the upgraded entry while evicting others, retains the charge after switching back to Linear, and deletes a partially generated texture on GL error. Oversized native textures use the nonfatal fallback.
- **UI-R2-5:** `render.rs` pruning visits Shared content and uses the NodeId set to avoid repeated DAG traversal; unchanged Shared effects survive sibling edits.
- **UI-R2-6:** `text.rs` clears `df_masks` with the other text caches; the original populated-cache probe is now a normal unit regression.

The regression source is `src/renderer_regressions.rs` plus private-state checks in render/text/native/mesh modules. `tests/run-renderer-regressions.sh` runs surfaceless tests and a separately launched private Weston flush test. Neither launcher connects to the live display. Validation receipts and before/after parity comparisons are kept in `docs/reviews/scratch/ui-r2-fixes/`; final validation is summarized there in `README.md`.

## [ENG-PQ9] Investigation: "fill/colour ±1–3/255" is NOT vertex-colour quantization (parity QA, 2026-10-23 ~10:00)
Hypothesis tested: Qt quantizes rect vertex colours to premultiplied `Color4ub`
(`colorToColor4ub`: 16-bit QColor → float premultiply → `qRound`) and applies opacity in the
shader. **The engine already does exactly this** (`qt_rect.rs:42-55` `premul`, opacity in float
with round-half-even in `render_window` / `blend_into`), and the references confirm it:
`panels/cp-bt-three--light@2` glass fill Qt premultiplied `(58,58,58,58)` (= qRound(0.28·255)=71 ×
surfaceOpacity 0.82) and `(57,57,57,71)` are matched byte-for-byte by our GL render over
318 k pixels. Measurement checked too: with `PARITY_RAW_PM=1` the reference host dumps Qt's exact
premultiplied framebuffer; the verifier's straight→premultiplied reconstruction stays within
0.49 of it, so identical buffers never show as different.

Where the GL `fill/color` failures really are (all differing pixels, 325 images):

| crate | differing px | inside Text | inside Image | inside Canvas | other (rect) | of "other": on AA edges |
|---|---|---|---|---|---|---|
| island | 145 019 | 16.8 % | **76.0 %** | 6.0 % | 1.2 % | – |
| bar | 3 126 | 0.0 % | **81.0 %** | 1.3 % | 17.7 % | 100 % |
| panels | 178 577 | **86.4 %** | 1.9 % | 0.0 % | 11.6 % | 98 % |

Alpha already matches on 87–96 % of these pixels; the deltas are ±1 on colour channels.
So the high-leverage engine fixes are, in order:
1. **Native text blended over translucent fills** (panels 154 k px). Example glyph-edge pixels
   over the glass `(58,58,58,58)`: Qt `(50,50,50,161)` vs ours `(50,50,51,161)`,
   `(48,48,48,194)` vs `(48,48,49,194)`, accent text `(61,94,149,177)` vs `(61,95,149,177)`.
   Qt's text mask shader is `fragColor = color · mask.a`, blended `ONE, ONE_MINUS_SRC_ALPHA` in
   float with one rounding at the framebuffer; check the colour fed to the text material (premultiplied
   float vs quantized 8-bit, and whether `primaryText`'s B 44 vs R/G 43 survives) and the order of
   rounding in `native_gl.rs`.
2. **Image sampling** (island 110 k px: album art + agent logos; bar tray icons) — ENG-PQ2
   (`mipmap: true` trilinear, `smooth` bilinear at `sourceSize × dpr`).
3. **AA edge pixels** of rounded rects/borders (bar 0.5 k px, panels 21 k px): ±1 on the
   interpolated edge coverage (`qt_gl_rect.rs` / `qt_rect.rs` triangle interpolation), e.g.
   panels Qt `(18,19,22,48)` vs ours `(18,19,22,49)` ×4200.
No engine change was made for this item.


## Bar GPU final validation / handoff — 2026-10-23

Final scoped material integration is validated: `Node.native_materials:Option<bool>`
inherits within a component, defaults false, resets at Shared boundaries, and is
included in effect cache identity. Bar root/popups/corners opt in; retained node
reconciliation copies the field. Context discovery sees material-only surfaces.
The text mode remains independent. Public API docs in engine/bar READMEs reflect
this. Tests cover scope inheritance/restore and material-only context activation.
All source edits were re-read before surgical changes; concurrent owner cache,
upload-recovery and regression fixes are preserved.

Production GPU bar gate230→239/304, exact38→47, structural304/304, v1303/304.
No lost passes or increased maxima; one top-right dark@2 corner mean changes
0.0008974359→0.0009615385 (both max1). Native spatial error excluding frozen
submenu input-state drift: rect10204→1285, text73102→0, icons6415→2811,
compositing3971→1283. Native text uses QRgb8 QColor storage before inherited
opacity, protected by64 exact independent Qt cells. The canonical mesh's thin
rounded branch/ring ordering is protected by the installed-Qt vertex fixture.
Explicit highp GLES samplers and direct cached material/effect composition remain.
No alternate geometry mode, desktop-context switch, atlas-offset knob or UV/layer
basis experiment remains enabled.

Engine all-target tests54 passed/10 ignored; bar34/1 ignored; strict all-target
Clippy both pass. Separate GPU integration2/2 pass. Final cross-crate GPU replay
compares380 island and300 panels/calendar PNGs pixel-identical disabled/enabled;
existing strict counts0/380 and82/300 unchanged. Six-thousand-frame mapped
four-surface GPU stress,776 comparisons,1974 resizes,63 recreations,2662 partial
damage frames: zero mismatches. Receipts are linked from bar/VALIDATION.md.

Remaining65 bar failures:64 max1 graphics crops and the existing frozen submenu
max23 input mismatch. Icon decode/source pixels and Qt mesh vertices are proven
exact; remaining GPU interpolation/sampling/blend precision is not yet causally
resolved. Do not alter fixtures or gate tolerances to close these. The old apparent
GPU corruptions now have exact tray-strip asset-race reproductions; canonical
icon preparation uses report-private assets and replay pins complete binary/asset
snapshots. This resolves those captures specifically, without a speculative GL
fence/change to runtime synchronization.

Engine E2E checkpoint: concurrent scratch `text.rs:605` contains `false && ...`,
which blocks owned strict Clippy. I am simplifying that disabled branch to preserve
its current float-colour behavior (no renderer behavior change). Please re-read
this file before applying another text experiment. Native test results use pinned
executables, so concurrent rebuilds do not alter running receipts.


Bar S1 follow-up (no shared engine edits): real output sourceSize32 * DPR2
requires64px QtSvg bake. The crop capture host reports effective DPR1 despite
scaled captures and its observed QQuickImagePrivate source image is32px. Runtime
bake now follows output DPR; crop test assets explicitly retain the observed32px
source through existing Model.assets, which control icons now consult before
embedded fallback. Frozen full-screen idle S1 independently reproduces max12/9
and after runtime fix is max3/2, zero pixels>8. Bell is native text, max3 both
themes. See bar/VALIDATION.md and tools/check_fullscreen_s1.py. No37-image full
shell acceptance claim until T1 is fixed and the integrated replay is repeated.
Pinned corpus stress also completed3040frames/10replays with zero mismatches.


13:51 UTC diagnostic: installed Qt smoothcolor GLSL uses a vec4 vertex attribute,
while our shader constructs vec4(pos.xy,0,1). GL attribute expansion already
supplies z0/w1; test Qt's vec4 form to check compiler constant-folding/FMA as a
remaining one-unit AA contributor. Surgical shader-only change, revert if the
independent rect/effect probe and frozen corpus do not improve. Cache/geometry
owner edits remain untouched.

13:52 UTC diagnostic result: vec4 attribute form changes no pixels in any of the
four independent Qt rectangle/effect cases. Reverted only those two shader text
substitutions after re-reading; existing cached renderer unchanged. Evidence:
bar/artifacts/rect-vec4-diagnostic/metrics.json (all four PNGs byte-identical to
rect-highp-effect/rust). This does not explain the remaining AA differences.

S1 final bar tests35 passed/1 ignored; strict Clippy passes. All304 crop PNGs
pixel-identical to validated239/304 checkpoint after DPR asset correction;
remaining-s1-crop-regression/png-comparison.json. A static corrected control-region
patch meets ≤8 in64/64 full-screen references, but integrated full-shell passes
remain pending, not claimed. Eight supported scale raster sizes have regression
coverage. Runtime remains Qt-free; no shared-engine change for S1.


## Bar exact closure round2 — 2026-10-23 13:58 UTC

New90-minute task targets64 max1 crops, baseline239/304. Begin by obtaining
independent actual Qt layer source/downsample readbacks via a private host copy
(walking visual children and QQuickItemLayer::effectSource, not QObject ancestry).
Temporary Rust diagnostic readbacks will be gated by HYPRV_DEBUG_LAYER_DIR and
removed before completion; no reference/fixture changes. Re-read render/effects
before edits; keep concurrent owner changes. Stage comparison should locate the
first one-unit divergence before attempting precision fixes.

Round2 origin diagnostic: actual Qt GL draw trace shows layer source projection has **positive
Y scale** (matrix[5]=+0.00677966094 for600x462), while Skia's default bottom-left
surface/native material has negative Y. Flipping UVs later is mathematically
similar but changes hardware raster/interpolation rounding. Test allocating
native effect source/pyramid surfaces with explicit TopLeft Skia origin, carry
origin into native scissor/vertex/effect shaders. Root EGL target stays its real
BottomLeft origin. This is a general texture-origin correction, not per-crop
bias. Temporary stage readbacks remain gated and will be removed.

Actual Qt popup-source VBO now captured:192 vertices are bit-identical in
position/offset to canonical Rust geometry. Qt submits indexed TRIANGLE_STRIP;
our cache expands CPU triangles and submits TRIANGLES. Test preserving canonical
strip indices in Mesh (CPU triangles unchanged) and submitting an EBO directly.
Account VBO+EBO bytes in the same8MiB cache and delete both on eviction/clear.
This can affect interpolation tie rounding even with the same triangle set.

Round2 full replay: TopLeft source/pyramid surfaces improve gate239→263/304, with zero
lost passes, increased maxima or increased per-image means. The indexed-strip
probe leaves selected pixels unchanged; retained provisionally as the actual Qt
submission and reduced vertex/cache storage, subject to all cache tests. Next
use the installed Qt QSB GLSL100 smoothcolor shader verbatim, including batchable
order input0, to rule out shader compiler algebra differences. Actual GL uniforms
and192 source-rect VBO positions/offsets already match the Qt trace.

## Engine reconciliation in progress — 2026-10-23
Owner is consolidating the offscreen parity target in `hyprv-ui::offscreen`, wiring
`Renderer::{raster,png}` to `HYPRV_PARITY_BACKEND`, adding cached wrapped text
measurement, and auditing shared APIs/docs. Component harness imports will migrate
mechanically under ENG-PQ8. Existing rect/native-material/effect numerical paths,
including current layer-origin work, are preserved. Please re-read shared files
before edits and log completion of temporary renderer diagnostics. Baseline and
after receipts use pinned executables and private compositor/asset directories.

Round2 GL-state diagnostic: exact installed shader leaves the206 source-channel errors unchanged.
Actual Qt draw state now captured: DITHER=enabled, DEPTH_TEST=enabled,
SCISSOR=disabled, samples0, blendONE/ONE_MINUS_SRC_ALPHA; indexOffset2 skips the
batch buffer's leading degenerate and uses the same342 canonical indices. Our
native draws disableDITHER. Test enabling it on rectangle submission first;
source VBO bytes are byte-identical to Qt, not merely position-matched.

CORRECTION to the stage206 report: the private observer flipped layer texture
readbacks with the root framebuffer's Y convention. Actual Qt layer projection
is opposite. Undoing that diagnostic flip yields **byte-identical source and all
four blur levels**, even before TopLeft allocation. The206/9160 differences were
observer orientation artifacts, not rendering errors. Root captures were always
byte-identical to frozen refs. TopLeft still independently improves the final
gate239→263, by removing sampler coordinate reflection in final composition.
Next align final effect quad with Qt's precomposed projection/model-view matrix;
current shader divides projected device points per vertex instead.

Shared compile repair during engine reconciliation: re-read offscreen.rs; its
render_png_if_enabled returns Result<Option<Vec<u8>>> but the active branch
returned Result<Vec<u8>>. Add only `.map(Some)` after with_target. This preserves
the documented optional hook shape and allows the ongoing bar numerical probes
to compile; no transport behavior change.

Correction to compile note: owner had already added `.map(Some)` between our
read and proposed write; the exact-text assertion prevented any edit to
offscreen.rs. No bar-agent change was made to that file.

UI-R2-1..6 final validation: **56 ordinary hyprv-ui crate/integration/doc tests pass; 11 explicit GPU/EGL regressions pass on NVIDIA and 11 on Mesa; strict all-targets Clippy passes. All 1,968 PNG/RGBA pairs from the three original UI harnesses (CPU + EGL) are byte-identical in the private-build, frozen-consumer comparison.** Existing Qt-reference gate failures/harness exits are unchanged. Concurrent material/origin/text/offscreen work was preserved and excluded from patch attribution; current combined-source tests were rerun after its transient compile issues cleared. [Receipts, exact proof scope and reproduction](../reviews/scratch/ui-r2-fixes/README.md), [aggregate results](../reviews/scratch/ui-r2-fixes/results.json).

Round2 image-coordinate probe: actual Qt menu-image VBO is(9,9)..(25,25)
under the viewport clip's matrix (translation2755,67), whereas our zero-origin
quad absorbs the child offset into its uniform. Add scoped native image batch
basis at paint roots, hard-clip nodes and independently allocated layer sources.
Rebase image geometry into that basis; keep local vertex origin separate from
the projection matrix, matching QSG batching rather than adding pixel biases.

Actual Qt atlas readback now confirms all four54px corner rasters and the16px
menu cover are byte-identical to the embedded/prepared source pixels. This is
post-upload GL texture data, not just decoded QImage data. New scoped clip-basis
image prototype alone changes no selected pixels; testing it together with the
original full-output projection (the crop adapter normally projects a smaller
viewport). Evidence: bar/artifacts/qt-atlas-readback/atlas-source-comparison.json.

Final source/decode checks are exact. Temporary diagnostic: substitute actual
captured Qt512px atlas as menu-image data in the fixture adapter, and sample its
recorded1,103,16,16 UV subrect through the normal native image path. This tests
normalized texture-coordinate interpolation separately from source decoding and
vertex geometry. Guarded HYPRV_DEBUG_RGBA_SUBRECT hook will be removed afterward.

Round2 final-store probe: the menu's277 erroneous channels are all in fully
opaque pixels and predominantly one unit lower than Qt. The rectangle-only
DITHER test did not exercise image/effect framebuffer stores; native image and
final effect loops still disable it. Match actual Qt GL state by enabling DITHER
for all native draws (including image/effect/masks), then check independent text
and full corpus before retaining. This is state parity, not an output bias.

Round2 negatives: full-output viewport + scoped image batch basis still leaves
menu277; supplying the actual captured Qt atlas and exact UV subrect also leaves
277. Enabling DITHER on all native paths changes no selected pixels on NVIDIA.
These are not source-image or atlas-upload differences. Keep the263 gain from
physical source/pyramid origin; remove non-improving coordinate/dither/atlas
prototypes before final validation unless later evidence requires them.

Round2 readback boundary: add one temporary debug_layer call in EglWindow::png_bytes
before PNG encoding, guarded by the existing finite HYPRV_DEBUG_LAYER_DIR hook.
Compare retained premultiplied RGBA bytes with frozen PNG-reconstructed values,
especially the fully opaque menu pixels. This separates shader/store rounding
from Skia's GPU PNG encoding path. Remove the call and helper before completion.

Round2 boundary proof: native image path is active (materials=true, rectclip=true,
blocked0), and retained premultiplied bytes equal decoded PNG bytes exactly,
including the277 opaque icon-channel differences. Thus no PNG codec issue.
Test explicit RGBA8888 retained target vs current N32 (BGRA on this platform),
matching Qt QRhiTexture::RGBA8. Temporary HYPRV_RETAIN_RGBA switch is diagnostic
only until independent corpus/cross-crate proof; remove it afterward.

Engine consolidation: all984 production-GL before/after component captures are
byte-identical. CPU captures changed608/984 and two CPU panel assertions regressed.
Inspection found the newly introduced qt_rect direct destination blend dispatch;
it changes legacy CPU compositing (independently of the retained GL mesh transport).
Testing removal of that automatic dispatch while preserving Coverage::blend_into
as an explicit API. Native GPU drawing and public numerical mesh data are untouched.
Do not attribute the shared-target CPU run to a specific patch until the pinned
CPU-only replay confirms it. Evidence: target/reconcile/{before,after}, image-diff.json.

Round2 explicit image-geometry test: use installed Qt texture vertex/fragment
shader sources, with explicit position+UV vertices in a small context-owned VBO
instead of constructing both from a unit quad in the vertex shader. This keeps
Qt's shader input/interpolation path intact. Preserve scoped clip basis and
existing bounded image cache; own VBO is deleted with its context/program.
Explicit RGBA retained storage changed no selected pixels and will be reverted.

CPU diagnostic confirmed: removing automatic `Coverage::blend_into` dispatch
restores the panel harness from246/248 to248/248, without touching GL rendering.
The explicit blending helper remains available, but default CPU exports retain
the established cached-raster composition. Comparing the entire final CPU corpus
against pinned baseline next. This is a CPU-default compatibility fix; it does
not weaken canonical reference gates or alter the production Qt material mode.

Round2 image result: explicit Qt-style position+UV VBO with installed texture
shaders removes **all277 menu-icon channels** in both DPR2 root crops. The
source/atlas/coordinate prototype alone did not; shader-side unit quad expansion
was the causal difference. Context::clear_caches and Drop delete the new image
VBOs with their programs. Full corpus now running; keep the scoped batch basis
until independent simplification checks. Remaining corner errors unchanged.

## ENG-BLEND-1 — GPU-exact per-draw blending on CPU canvases (spec-bar, 2026-10-23)

User request: float blend + one 8-bit store per draw on the CPU renderer, matching the
EGL path. Scope: only CPU canvases inside `Renderer::paint` while
`Renderer::gpu_exact_blending` is true (default; `set_gpu_exact_blending(false)` restores
skia's native raster blending). GPU canvases are untouched: `exact*()` are no-ops when
`GPU_EXACT` is false, and every production-GL capture is unchanged.

`render.rs`:
- Thread-local `GPU_EXACT` + an `ExactGuard` set in `paint()` when there is no recording context.
- `exact(paint)`: src-over paints get a runtime blender. It quantises the fragment output to 8 bits,
  `s = floor(src*255+0.5)/255`, then returns `s + dst*(1-s.a)`. The runtime blender forces skia's
  highp pipeline, so there is one rounding on the 8-bit store.
- `exact_alpha(paint)`: for paints that only modulate by alpha (layer restore, image, picture, canvas
  fallback, svg layer). The alpha is first quantised to 8 bits, because skia's GL path hands that
  opacity to the shader as an 8-bit colour. This fixed `qa-show-hide--show-t0060--dark@1/@2`.
  Example: the 0.07-white fill stored as 18 in the layer gives 18·0.75 = 13.5, a tie that GL resolves
  to 13 (191/255 alpha).
- With `materials_enabled()`, a CPU-exact MultiEffect is drawn directly with the node alpha (no 8-bit
  layer), as the native GL material does.

`image.rs`:
- `sample_uncached` emits RGBAF32: unrounded interpolation, as a texture sample reaches the blender.
- `draw` uses `exact_alpha`.

Other files:
- `effects.rs`: in CPU-exact mode the compose target is RGBAF32. The GL path composes straight into
  the framebuffer.
- `text.rs`: the skia fallback paint uses `exact`.
- `native_gl.rs`: `materials_enabled` is now `pub(crate)`.
- `qt_rect.rs`:
  - `Coverage::blend_into` used round-half-down src quantisation (NVIDIA interpolates a flat
    vertex colour to just below a tie, e.g. 95·0.5).
  - The owner removed the automatic dispatch at ~10:26, then the method itself at 10:41.
  - For panels (skia drawVertices path) that changes CPU-vs-GPU totals by < 1 % (577,793 vs 573,484
    differing px) and the Qt gate not at all.
  - **For the bar (native materials) it appears to matter.** With the dispatch, the 09:56
    `verify --backend both` had bar CPU == GPU on 304/304. The 10:37 run, after the removal and the
    concurrent native-material GL changes (bar-gl 228 → 244), is back to 272/304 differing, max 1–3.
    I didn't fight the reconciliation; I backed out my brief local re-add after the method disappeared.
  - Owner's call: restoring a direct destination blend for raster canvases in GPU-exact mode is the
    path back to bar byte-identity.

Model fit: a 256-texel × 4-opacity strip, CPU vs NVIDIA EGL (`zz_model`):
- exact float with one rounding: 537/1024;
- 8-bit src: 875/1024;
- 8-bit src + 8-bit opacity: 960/1024;
- adding an fp16 intermediate: 980/1024. Not adopted, since the gain is marginal.

Results (`verify.py --backend both`, NVIDIA RTX PRO 6000, EGL RGBA8):
- bar: CPU == GPU byte-identical on 304/304 at 09:56 (was 272/304 differing, max 3). Lost again
  after the reconciliation above (272/304 at 10:37).
- panels: 204/248 still differ, max delta 3. These are ±1 on AA edges (GPU triangle
  interpolation vs CPU coverage), DF/native text and bilinear sub-texel weights. They are hardware
  rasteriser details, not blend arithmetic. QuickAdjust t0060 went 66,130 → 1,108 px.
- island: 380/380 differ, max 73. This is structural (svc-media harness), not blending.

Qt gate (no regressions):
- panels 248/248 pass (was 246/248): 202 better, 1 worse painted mean vs the F16 baseline;
  worst max 7.
- bar CPU 224 → 224 and bar-gl +15 newly passing.
- island 0 newly failing, 368/380 lower mean.

Panels harness: `tests/shared_common` captures via `Renderer::raster`
(`PANELS_RASTER=f16` keeps the old F16 path).

ENG-PQ8 is implemented: Renderer::{raster,image,png} selects cpu/gl via
HYPRV_PARITY_BACKEND or explicit Backend override; one engine offscreen::GpuTarget
owns the actual roleless EGL target. parity/gpu is now only a compatibility
re-export and component harnesses import the engine API. The private EGL transport
regression passes for all entry points, DPR1/1.5/2 and target recreation; native
64-cell alpha/clips/layers goldens and84 Canvas stroke cells also pass. Renderer
cache clear includes bounds/reachability and UI-thread wrapped-measurement caches,
while preserving the R2 coordinated native epoch. No global material opt-in change.
Please retain the wrapper API and remove the active debug_layer instrumentation
when the bar round2 diagnostic has finished; consolidation preserves it meanwhile.

Round2 final effect interpolation probe: image277 was fixed by independent
position/UV vertex attributes, not by sampler/viewport tweaks. Final effect quad
still aliases its unit position attribute into UV. Give effect programs their
own bounded64-byte position+UV VBO and explicit vertices, matching Qt ShaderEffect
geometry. Surface source/pyramid orientation and generic mask/blur paths remain
covered by independent stage and cross-crate captures. Delete VBO on cache clear.

Final pinned comparison:1968 images across CPU+GL;1920 decoded/encoded outputs
are byte-identical. All984 GL images are byte-identical.48 CPU images differ;
canonical strict gate results are unchanged for all of them (no lost passes or
new structural failure). Means never increase; one already-failing CPU island
expansion max changes58→59 while mean improves1.323→0.637. Full details in
target/reconcile/gate-comparison.json; do not report CPU bit-identity. CPU panel
harness is248/248 again. New native_gl edits during final CPU build are recorded
in its source receipt; no causal attribution to an individual owner's patch.

Round2 effect VBO alone changes no selected pixels. Test the unmodified installed
Qt MultiEffect vertex+cs2 fragment shader when all source textures have native
TopLeft origin. Existing generic/origin-reflecting shader stays for other cases.
This removes the adapted sample-wrapper algebra and uses the actual separate
Qt ShaderEffect attributes/uniforms; preserve source/pyramid buffers proven exact.

CPU cleanup decision: workspace search confirms no caller of the experimental
Coverage::blend_into beyond its removed dispatch. Removed that unused helper as
well: the canonical cached coverage/raster path remains the single CPU rectangle
composition API. This avoids exposing a second unverified rounding contract.
Production Qt GL materials still share the same canonical mesh/coverage geometry.

Round2 spatial correction: after menu image fix, all remaining menu/battery
max1 pixels lie at crop rows9..15, i.e. global y44.5..47.5 on **bar pill edges**;
the popup chrome/effect area is exact. Effect VBO/verbatim shader add no gain and
will be simplified later. Apply QSG root batching to rectangle vertices: for
translation-only descendants, transform canonical positions into the current
batch-root basis on upload and use that basis's matrix. Keep offsets untouched
for unit-relative scale; other transforms retain the existing per-node path.
Include translation bits in bounded GPU VBO cache identity.

Round2 sampler trace found a concrete difference: Qt uploaded image/atlas textures
report internal format GL_BGRA_EXT(0x80e1), while native cache used RGBA8. Test
BGRA upload/storage for RGBA image leaves when GLES advertises BGRA8888 support
(swizzle CPU bytes on upload; alpha masks unchanged). Also recorded Qt's image
min/mag filters and mip levels to verify the residual control icon separately.

Later pinned GL corpus includes the image-vertex correction:974/984 byte-identical,
ten menu images have lower means and unchanged maxima, zero lost strict gate passes.
Island380/380 and panels/calendar300/300 remain byte-identical. Receipt:
target/reconcile/final-gl-gate-comparison.json. Baseline identity remains in the
initial after checkpoint; later owner shader probes must retain their own receipts.

Round2 full-output result: rectangle batch positions + original full-output
viewport make all tested menu/battery/left-corner cases exact, leaving only
control-image pixels in three right-corner captures. The adapter previously
replaced Qt's full viewport with the crop, retaining different float geometry
precision. Promote full-output GPU render then crop in the adapter itself;
no production viewport behavior or frozen reference changes. Preserve explicit
HYPRV_BAR_FULL_TARGET raw-output diagnostic. Verify subset PNG roundtrip against
independent full-target/Pillow crops in premultiplied space before retaining.

Round2 decisive control-material trace: actual control texture31 has alpha and
uses GL_LINEAR_MIPMAP_LINEAR as spec. Source32px pixels are byte-identical; its
VBO is3012.109375,21 through3028.109375,37, full-output matrix exact. **Qt selects
the no-opacity-multiply texture shader whenever inherited opacity=1, even with
texture alpha; blending stays enabled for texture alpha.** Native code conflated
opaque texture and full inherited opacity. Split these decisions: full opacity
chooses opaque shader; only opaque texture+full opacity disables blending.
This targets the final41 control-icon channel errors without color biases.

Compile coordination at14:43 UTC: ongoing qt_rect.rs Coverage refactor currently
calls `cov.blend_into(...)` near800 but no blend_into method exists (E0599).
Bar numerical changes are ready, awaiting that unrelated CPU-path compile fix.
No bar-agent edit made to the Coverage implementation; preserve Mesh.indices and
current GPU cache translation changes while reconciling it.

Owner reconciliation resolved Coverage before the next build completed; current
bar reference/rect_effect examples compile successfully. No compatibility shim
or CPU blend change was added by bar agent.

Final compile recheck observed a transient blend_into dispatch restored without
its method. Re-read current qt_rect.rs: both method and dispatch are now absent,
matching the tested canonical CPU path. No new shim/rounding path was introduced.
Preserved the concurrent Mesh.indices/GPU translation-basis work unchanged.

All16 selected crops now exact after full-opacity shader selection. Cleanup:
remove temporary GL readback/PNG hooks, RGBA-target switch, atlas subrect probe,
and non-improving extra effect shaders/VBO. Keep verified TopLeft native effect
surfaces/origin semantics, Qt-style explicit image VBO/shaders with separate
blend decision, scoped batch coordinates, canonical indexed rect mesh translated
at batch roots, and full-viewport GPU fixture capture followed by crop.

Full clean corpus301/304:62 of64 target crops closed. Remaining charging/critical
are the same5 channels on the battery popup's right curve (global x2833), not bar
edges. Full battery is exact; its popup is5px farther left. This exercises a
projection rounding threshold not covered by the earlier full-battery probe.
Re-test the CPU-precomposed effect matrix specifically on these two placements
before rejecting it. Source/effect extents are identical324x190; only x differs.

Remaining battery candidate: clip/batch rebasing computes inverse(base)*matrix
in float even when both are identical. For the648x380 source covering388x254,
Y scale*inverse rounds to0.99999994 and translation to-1.9e-6; the earlier
600x462 menu source round-trips exactly. Preserve identical geometry directly,
and for equal positive axis scales compute only translation differences instead
of matrix inverse/concat. Add a regression requiring identical non-dyadic source
matrices to preserve vertex bounds bit-for-bit.

Engine latency/idle follow-up active: opt-in diagnostics scopes in UI/text/render/
Wayland and shell (HYPRV_UI_PROFILE; CSV only on explicit snapshot/shutdown).
Cold native battery trace:132ms release→surface-created and~224ms release→first
submitted frame in debug. Repeated SkFontMgr_NewSystem enumeration dominates
FontCollection and face resolution. Consolidating one UI-thread FontMgr initialized
by Renderer startup, cloned by resolve()/wrapped measurement. Same fontconfig
selection, face bytes and paragraph settings; no pixel-specific correction.
Please preserve diagnostics and re-read text/render/wayland before concurrent edits.


Snapshot CI, 14:59 UTC: Clippy excessive_precision in `tests/common/batch_material.rs:8` (Rect x literal3012.109375 inferred f32) reproduced twice on unchanged snapshot `/tmp/hci-sys/round-04-20261023T145010Z/rshell`, fingerprint `0beac47f2ad42142` (full in ci evidence). Compiler primary span spells include path `crates/hyprv-ui-bar/examples/../../hyprv-ui/tests/common/batch_material.rs:8`; normalize it to hyprv-ui ownership. Logs `docs/verify/raw/ci-snapshots/round-04-20261023T145010Z/04-clippy-{1,2}.log`; no source edits by CI.

Steady-state timing established next UI idle cost:216 shaping/metric misses in60s
spend198ms inclusive; line_metrics opened a fresh FreeType face for every label's
last-glyph overhang, native_mask did likewise per glyph. Both now use the existing
UI-thread opened-face cache, explicitly setting size/load flags and resetting the
mask transform on exit (including errors). Shared system FontMgr/resolved Face
cache avoids duplicate font-file parsing across measurement/render engines.
The shell prepares500/600/700 Nerd/SC font styles at startup, as Qt constructs
hidden Text items then. Native font recipes and glyph outputs are unchanged.
18 independent Qt text samples pass; new interleaved size/weight/subpixel-transform
regression matches metrics and pixels through cache clears. Controlled release CPU
0.5487→0.2160% in the first post-face run (historical0.4159%). Final repeat underway.

Round2 final composition trace: battery source and all4 blur stages match Qt byte-for-byte. Actual QSG effect quad uses indexed strip2,2,0,3,1,1 over TL,TR,BL,BR vertices: diagonal TL-BR. Native effect strip used the opposite diagonal. Match the actual non-degenerate order BL,TL,BR,TR; verify full corpus before retaining.

15:05 UTC checkpoint: QSG effect quad diagonal correction makes all4 battery-charging variants premultiplied-exact. Removed temporary source/downsample raw-stage readback hooks from effects.rs/render.rs. Exact source+four-level observer evidence retained under bar artifacts/{qt-battery-stage,rust-battery-stage}; no runtime diagnostic file I/O remains. Full replay and regressions follow.


### Snapshot CI — 2026-10-23T15:06:17+00:00 — doctests

FLAKY: first attempt failed, same-snapshot retry passed. Snapshot `/tmp/hci-sys/round-05-20261023T150010Z/rshell`; fingerprint `05e6b8b0ddad31af476cb42ccc5905a9cff1b10a3ef28f810a30f940fcae6621`. Concurrent live edits cannot change this source copy.

Command: `['cargo', 'test', '--locked', '--workspace', '--doc', '--message-format=json']`. Attempts: `[{'code': 101, 'log': 'round-05-20261023T150010Z/03-doctests-1.log'}, {'code': 0, 'log': 'round-05-20261023T150010Z/03-doctests-2.log'}]`. Locations: .



```text
165f1462a5e5,libskia_safe-3a6e21d0a55e7817,libpercent_encoding-b1da3b8fc2e1026a,libbase64-93d13499e98064b8,libskia_bindings-305d2104b986ba41,libbitflags-476ff885740a1ce4,libkhronos_egl-7c7089a1b02d174d,liblibloading-36076a59b20ed2ef,libcfg_if-d995ec1fb643b77d,liblibc-b437f8992049e083}.rlib" "<sysroot>/lib/rustlib/x86_64-unknown-linux-gnu/lib/{libstd-*,libpanic_unwind-*,libobject-*,libmemchr-*,libaddr2line-*,libgimli-*,libcfg_if-*,librustc_demangle-*,libstd_detect-*,libhashbrown-*,librustc_std_workspace_alloc-*,libminiz_oxide-*,libadler2-*,libunwind-*,liblibc-*,librustc_std_workspace_core-*,liballoc-*,libcore-*,libcompiler_builtins-*}.rlib" "-Wl,-Bdynamic" "-lfontconfig" "-lstdc++" "-lfreetype" "-lxkbcommon" "-lxkbcommon" "-lwayland-egl" "-lwayland-client" "-lm" "-lstdc++" "-lfreetype" "-lfontconfig" "-ldl" "-lgcc_s" "-lutil" "-lrt" "-lpthread" "-lm" "-ldl" "-lc" "-L" "/tmp/rustdoctest2sbyPa/rustcpJ1TOc/raw-dylibs" "-B<sysroot>/lib/rustlib/x86_64-unknown-linux-gnu/bin/gcc-ld" "-fuse-ld=lld" "-Wl,--eh-frame-hdr" "-Wl,-z,noexecstack" "-L" "/tmp/hyprv-svc-sys-ci-20261023T142927Z/target/debug/build/skia-bindings-3c0a7a97a83db03e/out/skia" "-L" "/tmp/hyprv-svc-sys-ci-20261023T142927Z/target/debug/build/hyprv-svc-media-2890929e3b793ad5/out" "-L" "/tmp/hyprv-svc-sys-ci-20261023T142927Z/target/debug/build/hyprv-ui-2a27a7d0c8964e86/out" "-L" "/tmp/hyprv-svc-sys-ci-20261023T142927Z/target/debug/build/libspa-f7d1f06bd59e2db6/out" "-L" "/tmp/hyprv-svc-sys-ci-20261023T142927Z/target/debug/build/libspa-sys-9e21934df09b60a9/out" "-L" "/tmp/hyprv-svc-sys-ci-20261023T142927Z/target/debug/build/ring-8de8b32927bad5dd/out" "-L" "/usr/lib" "-L" "<sysroot>/lib/rustlib/x86_64-unknown-linux-gnu/lib" "-o" "/tmp/rustdoctest2sbyPa/rust_out" "-Wl,--gc-sections" "-pie" "-Wl,-z,relro,-z,now" "-nodefaultlibs"
  = note: some arguments are omitted. use `--verbose` to show all linker arguments
  = note: PLEASE submit a bug report to https://github.com/llvm/llvm-project/issues/ and include the crash backtrace and instructions to reproduce the bug.
          LLVM ERROR: IO failure on output stream: No space left on device
          collect2: error: ld returned 1 exit status
          

error: aborting due to 1 previous error

Couldn't compile the test.
---- crates/hyprv-ui/src/lib.rs - (line 214) stdout ----
error: linking with `cc` failed: exit status: 1
  |
  = note:  "cc" "-m64" "/tmp/rustdoctestICNDNS/rustc9x41TM/symbols.o" "<2 object files omitted>" "-Wl,--as-needed" "-Wl,-Bstatic" "/tmp/hyprv-svc-sys-ci-20261023T142927Z/target/debug/deps/{libhyprv_ui-bb33f6540bada6a4,libwayland_egl-fbe8cd273434a431,libthiserror-f9f2e8a5945b98cd,libglow-33893e892c1cd19f,libunicode_segmentation-34d3c42df27079b2,libfreetype-22f93b1d61996800,libfreetype_sys-6c17cac1b998518f,libsmithay_client_toolkit-8e5a09e23637f8d0,libwayland_cursor-70a82d4eee61349f,libxcursor-d3e9958dbf6f2d5f,libxkbcommon-8011b0669bed3026,libmemmap2-4ed508cd4fb1548b,libxkeysym-9d6f43adc673c0ae,libbytemuck-63a33efc65cb1fd8,libmemmap2-a0b1a7d2042dfc31,libwayland_protocols_wlr-6d8a1f3cbd44149e,libwayland_protocols-cc93e2440c580585,libwayland_csd_frame-67ad9fa773ffca59,libcursor_icon-221982d88d749eb3,libcalloop_wayland_source-fcf26de320bbe5c7,libcalloop-fb09e5308dbec356,libthiserror-5416ad288cd0add6,librustix-69e11441cddf219f,liblinux_raw_sys-dce849a3285409fb,liblog-bbcaa5ffbeaea19f,libpolling-2d8b1b1cd64afdce,libwayland_client-36544df8f27151f5,libwayland_backend-6186d0a8e1e91a11,libdowncast_rs-e6ca41441d1a6e8b,librustix-32ef14e1d23ef307,liblinux_raw_sys-03613364dc1a1f4f,libscoped_tls-7bdd67dd51b4e802,libwayland_sys-3306ea7fbeee0272,libdlib-638e5d9911d144da,libsmallvec-2d35165f1462a5e5,libskia_safe-3a6e21d0a55e7817,libpercent_encoding-b1da3b8fc2e1026a,libbase64-93d13499e98064b8,libskia_bindings-305d2104b986ba41,libbitflags-476ff885740a1ce4,libkhronos_egl-7c7089a1b02d174d,liblibloading-36076a59b20ed2ef,libcfg_if-d995ec1fb643b77d,liblibc-b437f8992049e083}.rlib" "<sysroot>/lib/rustlib/x86_64-unknown-linux-gnu/lib/{libstd-*,libpanic_unwind-*,libobject-*,libmemchr-*,libaddr2line-*,libgimli-*,libcfg_if-*,librustc_demangle-*,libstd_detect-*,libhashbrown-*,librustc_std_workspace_alloc-*,libminiz_oxide-*,libadler2-*,libunwind-*,liblibc-*,librustc_std_workspace_core-*,liballoc-*,libcore-*,libcompiler_builtins-*}.rlib" "-Wl,-Bdynamic" "-lfontconfig" "-lstdc++" "-lfreetype" "-lxkbcommon" "-lxkbcommon" "-lwayland-egl" "-lwayland-client" "-lm" "-lstdc++" "-lfreetype" "-lfontconfig" "-ldl" "-lgcc_s" "-lutil" "-lrt" "-lpthread" "-lm" "-ldl" "-lc" "-L" "/tmp/rustdoctestICNDNS/rustc9x41TM/raw-dylibs" "-B<sysroot>/lib/rustlib/x86_64-unknown-linux-gnu/bin/gcc-ld" "-fuse-ld=lld" "-Wl,--eh-frame-hdr" "-Wl,-z,noexecstack" "-L" "/tmp/hyprv-svc-sys-ci-20261023T142927Z/target/debug/build/skia-bindings-3c0a7a97a83db03e/out/skia" "-L" "/tmp/hyprv-svc-sys-ci-20261023T142927Z/target/debug/build/hyprv-svc-media-2890929e3b793ad5/out" "-L" "/tmp/hyprv-svc-sys-ci-20261023T142927Z/target/debug/build/hyprv-ui-2a27a7d0c8964e86/out" "-L" "/tmp/hyprv-svc-sys-ci-20261023T142927Z/target/debug/build/libspa-f7d1f06bd59e2db6/out" "-L" "/tmp/hyprv-svc-sys-ci-20261023T142927Z/target/debug/build/libspa-sys-9e21934df09b60a9/out" "-L" "/tmp/hyprv-svc-sys-ci-20261023T142927Z/target/debug/build/ring-8de8b32927bad5dd/out" "-L" "/usr/lib" "-L" "<sysroot>/lib/rustlib/x86_64-unknown-linux-gnu/lib" "-o" "/tmp/rustdoctestICNDNS/rust_out" "-Wl,--gc-sections" "-pie" "-Wl,-z,relro,-z,now" "-nodefaultlibs"
  = note: some arguments are omitted. use `--verbose` to show all linker arguments
  = note: collect2: fatal error: ld terminated with signal 7 [Bus error], core dumped
          compilation terminated.
          

error: aborting due to 1 previous error

Couldn't compile the test.

failures:
    crates/hyprv-ui/src/lib.rs - (line 214)
    crates/hyprv-ui/src/lib.rs - (line 31)

test result: FAILED. 0 passed; 2 failed; 1 ignored; 0 measured; 0 filtered out; finished in 2.53s

error: doctest failed, to rerun pass `-p hyprv-ui --doc`
```

Logs relative to `docs/verify/raw/ci-snapshots/`. No causal commit inferred.

Strengthen independent batch-material GPU golden to a324x190 layer at2516,58 in3072x300 viewport, including DPR2. This specifically exercises the off-center large effect quad whose triangle diagonal exposed the battery UV rounding. Re-capture4 independent Qt goldens from portable QML; runtime fixture references unchanged.


### Snapshot CI — 2026-10-23T15:08:12+00:00 — clippy

REPRODUCIBLE: failed twice on same snapshot. Snapshot `/tmp/hci-sys/round-05-20261023T150010Z/rshell`; fingerprint `05e6b8b0ddad31af476cb42ccc5905a9cff1b10a3ef28f810a30f940fcae6621`. Concurrent live edits cannot change this source copy.

Command: `['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '--message-format=json', '--', '-D', 'warnings']`. Attempts: `[{'code': 101, 'log': 'round-05-20261023T150010Z/04-clippy-1.log'}, {'code': -15, 'log': 'round-05-20261023T150010Z/04-clippy-2.log'}]`. Locations: crates/hyprv-ui/tests/common/batch_material.rs:8, crates/hyprv-ui/tests/common/batch_material.rs:8.

`crates/hyprv-ui/tests/common/batch_material.rs`: `ff16eaa7181f759ffbabaa94741a7c5bea5a7f8bdf0d1ff8fef0f9254b81291e`

```text
error: float has excessive precision
 --> crates/hyprv-ui/tests/common/batch_material.rs:8:36
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

Round2 regression proof: expanded independent Qt batch-material scene324x190 is exact for full/faded image opacity atDPR1/2. Temporarily rebuilding only the owned diagnostic example with the opposite effect triangle diagonal tests that the golden rejects the old order; source restored immediately in finally, separate negative binary retained under bar .build. Main full-corpus replay and cross-crate binaries are pinned independently. README API/precision notes updated; no runtime API signature changed.

Font preparation memory audit: variable CJK named weights previously copied the
same20MiB backing font per weight. Base SkTypeface streams now share by file/TTC
index, bounded16 entries/64MiB; named design coordinates are still applied to
individual clones. This removes duplicate font streams without altering pixels.
All18 Qt text fixtures and mixed mask/metrics regression pass after this change.

Snapshot CI correction,15:09UTC: round05 storage exhaustion invalidates the automatic generic workspace/doctest failure attribution. The doctest retry PASS follows linker ENOSPC, so is infrastructure recovery, not source-test flakiness. Clippy retry was terminated deliberately for private-cache relocation and establishes no second source reproduction. Earlier round04 excessive_precision finding remains reproduced twice independently. Round05 timing failure seen only before storage exhaustion is unconfirmed in that snapshot. Raw logs preserved; no source regression is inferred from ENOSPC/interruption.

Confirmed regression rejects old code: full3072x1728 viewport with19px-radius324x190 layer reproduces max1 in bothDPR2 cells when effect strip order is reverted; corrected strip yields max0 in all4. The smaller300px-high experiment did not expose the UV rounding threshold, so final checked-in Qt goldens use the actual full output projection. Final ignored native_gl integration:3 passed. Final native corpus111/112 exact (only pre-existing frozen submenu hover state differs), all64 targeted crops closed, production gate303/304, zero lost passes/max/mean increases.

The independent opposite-diagonal DPR2 failures reproduce exactly the battery signature:5 channel units across4 pixels per full/faded cell. Corrected native integration passes all3 tests. Final source readback confirms stage-capture hooks absent and all source files fingerprinted by the303/304 receipt still match current files.

Final handoff: all64 max-1 crops closed; gate239→303/304, exact47→191. Native111/112
exact, remaining frozen submenu input-state mismatch max23 unchanged. Native-category
absolute error rect1285→0, text0→0, image2811→0, compositing1283→0. No lost passes or
increased maxima/means. Final engine/bar all-targets94 passed/18 ignored; strict
Clippy clean; independent native GL3 passed. Both6000-frame synthetic/mapped stress
runs have0 mismatches; two complete frozen-corpus replays608 frames have0 mismatches.
Final all-three production EGL verifier:380 island+300 panels/calendar images are
PNG-byte-identical between controls, strict gate counts unchanged0/380 and82/300.
Receipts in bar artifacts/exact2-final*, detailed table in bar/VALIDATION.md.
Public runtime API unchanged; Mesh.indices retains canonical strip for native GPU.

Engine performance follow-up final: controlled-quiet final release0.2163% CPU
(47.99% below historical0.4159%; budget≤0.1% still unmet). Five fresh battery
openings: first-buffer median12.59ms/max14.62ms; prepare max3.24ms. Declared
reveal timing is excluded, and a separate whole-cycle trace retains one35.10ms
render spike.162 ordinary tests/strict owned Clippy and204/204 native E2E pass.
Details and boundaries: `docs/verify/perf-followup.md`. Disk policy applied:
stale engine reconcile/functional bulk deleted; compact receipts archived under
`docs/verify/raw/engine-cleanup`. No private Cargo target directories.

## Island exactness timebox (bar owner, 2026-10-23 15:23–17:23 UTC)

User-authorized minimal island/shared-engine edits while svc-media investigates memory. Baseline production GPU0/380 from final bar cross-crate control. First migrate island root to existing native_materials and native text leaves to QtGl; propagate those flags through island reconcile. No shared renderer change needed for this first experiment. Preserve all other owners edits, use shared Cargo target with incremental disabled, retain only compact metrics/current outputs. Verify bar and panels after shared changes.

Island initial Cargo replay hit a transient shared-widget compile error: primitives.rs lookup(name,size) while icon_theme::lookup now requires scale. No widget source modified by this agent. Initial parity --no-run already built the island native opt-in binary successfully; use that binary for the first finite GL replay, verifying its hash unchanged. Rebuild once owner migration settles.

Island next surgical changes: AudioSpectrum.qml31-105 now replayed via existing engine CanvasBatch/CanvasPath with JS f64 geometry and wave phase; original shared SpectrumBars widget untouched. CompactMusic Row fills animated islandFrame (DynamicIsland.qml869-906), so center its24px art, clock and22px spectrum in current frame height minus12px margins instead of fixed26px row. Native opt-in first replay: canonical14/380, integer-PM78/380; record both because canonical float reconstructed-PNG differences can survive when integerPM is identical. Widget lookup compile edit settled; retry normal Cargo.

Engine active follow-up: profiling first visible battery reveal effect preparation
and idle wake sources. Editing diagnostics.rs, effect/render timing hooks and
shell assembly scheduling; no numeric/shader parity changes intended. All
builds use shared target with CARGO_INCREMENTAL=0.

Text root cause: island util::text and shared widget helpers baked HAlign line offsets into the item translation. QSG qt_native_place quantizes local glyph offsets/subpixel phase separately from item origin; lock glyph@1 diverged while@2 matched. Preserve Text.align and original box origin in island util; rebuild only owned idle button labels using that helper and re-anchor media control glyphs using Qt6 center halves (actual DynamicIsland.qml475). Change island hc helper to qt_center_half; shared widget code untouched. Canvas+animated-row replay reduced integer error30,967,678→14,544,079; animation-labelled failures43→17 and canvas100→23.

Finite original-QML observer confirms progress fill remains34.675555555555555 during expand t0..210 while track94→366; reaches135.01333333333332 at400. Runtime fill Behavior now restarts from last presented value, rather than extrapolating the interrupted job to the new frame before restarting. Only fill_w uses this rule. Observer also proves paused phase6.1600000000000037 atinitial capture,6.44 at+66. Rust replay erroneously advanced an extra initial16.667ms; parity/host/main.cpp frame(false) does not step its driver. Remove only that replay tick; no phase offsets or pixel-specific corrections. Private observer source/tools and property traces retained under island/tools/qt-observer and artifacts/observe-*.

Capture-codec evidence: installed Qt qrgb.h scalar qUnpremultiply does NOT match its PNG format-conversion SIMD path on half ties. Actual qdrawhelper_sse4.cpp reciprocal_mul_ps uses rcp estimate, one Newton-Raphson step, ×255, then nearest-even CVTPS2DQ. Add island test-transport Qt conversion, proven against an independent installed-QImage256x256 oracle covering every valid8-bit premultiplied (channel,alpha) pair. Capture reads originalRGBA8 premultiplied pixels, converts once for PNG; reference loading, gate/comparator and all runtime rendering unchanged. No image-specific table/correction used at runtime.

Island codec replay: canonical100/380 exactly equals integer-PM100/380; image pixel error unchanged1,892,848. All32,896 valid8-bit premultiplied channel/alpha pairs match independent Qt6.12 SIMD conversion. Next engine surgical fix: effects::mask_texture(Rect) currently invokes CPUqt_rect::rasterize even on native material roots. Route that opt-in mask target through existing native_gl::offscreen + canonical tessellate/native_gl::rect (same consolidated vertex-AA mode), retaining CPU fallback and restoring target/batch scope. Album-art residuals are at mask corners,1..2 units; bar has no MaskSourceRect usages and panels default material scope is false. Verify allthree after landing.

Concurrent compile checkpoint from R2-fix validation: strict scoped Clippy is temporarily blocked by E0499 at src/effects.rs:375 (target.canvas() borrowed twice in native_gl::rect args). See docs/reviews/scratch/discovery-icons-reconcile/fix-clippy-final.log. Leaving engine source untouched; will retry after owner edit. Earlier scoped Clippy passed before this concurrent change.

## [ENG-MEM-1] [P1] Release GPU memory of closed popups (svc-media, 2026-10-23)

Evidence: [docs/verify/perf-shell-memory.md](../verify/perf-shell-memory.md). After popups close,
Skia keeps 47.8 MiB of *purgeable* resources (their retained render targets) under the default
256 MiB cache budget; idle already holds one full-output target (84 MiB). Fix: when a view is
closed (`Engine::close` / `View::drop` → `EglWindow` drop), with the shared context current call
`DirectContext::purge_unlocked_resources(PurgeResourceOptions::ScratchResourcesOnly)` (or
`perform_deferred_cleanup(~1 s)` from the idle path), and set a smaller budget
(`set_resource_cache_limit`, e.g. 32 MiB). Combine with PERF-UI-7 (card-sized popup surfaces).

Scaled-native-text root cause confirmed from actual Qt textmask.vert and qsgdefaultglyphnode_p.cpp: font masks rasterize at render-context DPR, independent of item scale; modelView transforms glyph quads, then shader rounds transformed vertices to device pixels; sampler isNearest. Current engine folds item scale into FreeType DPR and re-rasterizes. Surgical plan: preserve existing unscaledpath, pass output DPR only from Renderer, raster masks at that DPR for positive uniform nativeGL item scaling, map/snap each glyph quad using original item matrix, draw through existing mask program/texture cache with per-quad extent. No new GL program/cache. Unsupported affine/Skia behavior stays unchanged. Validate independent scaled-text Qt goldens, engine tests, bar303/304 and panels controls.

Scaled-mask replay improves island260→274/380; text-labelled failures36→7. Independent12-cell native-scale probe is exact for actual island glyph scales.96/.92/1.06, but exposes nearest-sampling boundary ties for half-size and some scaled clocks. Qt populate uses triangles TL,BL,BR /BR,TR,TL (TL-BR diagonal), while existing mask unitstrip has TL,BL,TR,BR (BL-TR diagonal). Test changing only mask unitquad strip toBL,TL,BR,TR; RGBA image/effect VBOs and rect meshes are independent. Full bar/glyph-golden verification required.

R2-fix validation follow-up: owner effects.rs borrow repair observed; final scoped Clippy and182 affected-library tests pass (docs/verify/discovery-icons-r2-fixes.md). No engine source edited by reviewer.

Negative experiment: changing mask unitstrip diagonal did not improve independent scaled-clock nearest-boundary cases (and made another cell worse). Restored original mask strip before further work; no unnecessary EBO/program retained. Island scaled-native path remains274/380 versus260 before that fix.

Calendar-host trace: QML and Rust progress, animated width/height/radius match exactly at all10 captures. The emulated host alone still uses legacy foundations::hcenter(cw) for fractional animated widths, placing open-t16 background at217.68025 vsQt218, and does not opt that host rectangle into native materials. Change only tests/common::scene root material mode and calendar_container Qt6 center halves. Production bar host already has both. No timing adjustment or fixture rewrite.

Mask compositing follow-up: installed Qt MultiEffect cm0 shaders extracted from retained qsb. Test a lazily compiled exactcm0 program for native, no-blur/no-shadow TopLeft masks, leaving generic/fallback/bar shadow paths unchanged. Set actual_25.qt_Matrix, _36.mask/maskInverted andmaskSrc sampler6. Also reuse the one mask image between Skia shader children and native texture bindings (current code calls mask_texture twice; new GPU path would otherwise allocate two identical mask targets). No extra cold shader compilation on bar-only scenes; context destroys optional program.

Second scaled-text tie experiment: match QSG projection uniform arithmetic (CPU-precompute2/viewport, multiply snapped positions) instead of shader2*position/viewport. Existing mask uniform name size retained internally. This tests independent half-scale nearest-boundary cases; revert if no improvement, and require old64 native-text cells/bar303 unchanged.

Morph-frame topology finding: DynamicIsland.qml islandFrame and musicExpandedFrame both have clip:true; the port only clipped the outer island root. Add those two hard-rectangle clips to the actual frame nodes. Settled integer extents usually hid this, while fractional animated extents exposed half-pixel AA fringe outside their own frame bounds. Clips also supply the correct local QSG batch roots; no alternate rectangle rasterizer or radius guess. Observer radius trace disproved the earlier hypothesis that mef_r froze during collapse; do not alter its animation without further evidence.

Projection arithmetic experiment did not close additional independent scale cells; restored original unscaled mask shader/uniform semantics. Keep only the fixed-output-DPR transformed glyph path. Both the original mask strip and original mask projection remain as validated by the bar.

Island ellipsis diagnosis (16:18 UTC): owned original-QML observer now reads QQuickTextPrivate's actual elideLayout. At collapse30, width101.28862973760937/font18 bold, Qt shapes `Midnight…`, ellipsis glyph710 atx86.375. Add a targeted engine layout regression from those independently observed values before changing elision; investigate primary vs fallback ellipsis metrics rather than applying a pixel correction.
Confirmed elision cause: SkParagraph's glyph visitor omits terminal whitespace, so measuring each isolated prefix incorrectly gave `Midnight ` the same advance as `Midnight`. Test prefix with the ellipsis appended (which preserves the intervening whitespace), retaining Qt's strict boundary; output still uses the original prefix+ellipsis. Fresh original Qt collapse30 is bit-identical to frozen reference. Regression asserts the independently observed glyph sequence/positions.

Minimality check: compare exact Qt mask shader against existing generic composition with all island geometry fixes held fixed. Temporarily disable only mask_only selection for one finite replay, then restore immediately before final validation. Remove the optional program if it has no measured benefit; retain canonical GPU mask geometry and single mask-image reuse regardless.

Mask shader A/B complete: generic-mask and elision replays are identical on all380 PNG pixels (both277pass,error810428). Removed optional Qt mask program, fields/uniform assignments and two candidate shader files. Existing generic composition already implements the required mask arithmetic. This reduces code and GPU resources; GPU mask geometry and reuse-one-image are the actual retained improvements.

Native-text morph investigation: original Qt GL trace atdual-expand105 has textmask modelView root(140.875,10), pause quad corners(181.25,118.25)..(192.25,131.25), before root transform. Text's actual global origin(321.125,120.75), font21/ascent21.40625, same as Rust item. Reference pause ink begins one device row later than Rust. Compare actual glyph bitmap bearings and Qt font-cache geometry before changing snapping; retain no heuristic offsets.

Island high text-diff diagnosis: Qt6.12's delivery agent throttles frame-sync hover to100ms REAL QElapsedTimer time (source qquickdeliveryagent.cpp:2046-2074, envQT_QUICK_FRAME_SYNCHRONOUS_HOVER_INTERVAL). Parity's animation clock is virtual but monotonic remainsreal, so native glyphs can have different transient control scales from deterministic Rustreplay. Independent observer provesactualQtplay scale1 vsRust1.0009209 atdual-expand105; item positions/fontmetrics agree. No text snap/bearing modification warranted from this sample. Logged QA capture-risk detail in hyprv-ui-island.md. Do not change engine glyph positioning to absorb a state mismatch.

Canvas compositing fix plan (16:36 UTC): Content::Canvas currently bypasses native_gl::sampled_image and always calls image::draw/Skia. This quantizes inherited opacity, unlike QSG's float texture-material uniform. Example musiccollapse70 dark@2 has3212 spectrum pixels expectedPM(56,68,97,219),actual(56,69,98,219),and2634 expected(59,71,99,220),actual(59,72,100,220). Route Canvas raster textures through the existing sampled_image(...Linear,alpha) first, exactly like Content::Image. Opt-in native-material scope remains enforced by that function; CPU/other callers preserve fallback. No new GL material/cache. Run fullbar/panels controls after this surgical edit.

Independent spectrum raster probe: installed QPainter with first idle-wave bar reproduces the residual left-edge coverage difference, so source-raster diagnosis is independent of GL compositing. Building owned finite QOutlineMapper/QBezier oracle (from unchanged Qt6.12 sources) to compare26.6 outline points with the Rust port before any scanconverter edit. No production arithmetic changes yet.

QPainter spectrum exact cause found: independent QOutlineMapper oracle differs at exactly4points: Qt x=1 vsRust0 at(y1682,1134,1876,940). The subdivided x coordinate is just below a 26.6 half. Installed QtCore/qnumeric.h QRoundImpl uses SSE2 `d+copysign(0.5,d)` followed by integer truncation; floating addition rounds these near-half inputs up, unlike Rust f64::round. Replace ONLY outline()'s two26.6 .round conversions with that literal Qt operation. Add independent complete outline-point goldens for two first-bar shapes, retaining oracle CPP under island/tools. This targets source coverage, not PNG/gate thresholds.

Mask sampler root cause (actual GL, not guessed): original Qt MultiEffect source unit1 isLINEAR(9729), but maskSrc unit2 isNEAREST(9728), for both28px and70px art. Current qt_gl_effect binds oneLINEAR sampler to every texture includingmask. That interpolates maskAA at fractional morph translations, causing the26icon-labelled morph failures. Add one lazily allocated NEAREST/CLAMP sampler only for textureindex6 whenmaskEnabled; source/blur/shadow samplers stayLINEAR. Delete it with context programs. Trace: island/artifacts/qt-mask-samplers.log MASK_TEXTURE lines. No new shader, no art-specific offsets.

Remaining four large rect-labelled105ms crops are one extra bottom ROW, not color: e.gmusicexpand105 dark@2 all35920error units lie ony337, with reference transparent andRustPM(13,14,16,16) across600px. Qt6.12 qsgbatchrenderer.cpp:2440-2478 computes scissors by projecting clip bounds through floatQMatrix4x4 toNDC, then qRound lower-left viewport coordinates. Renderer currentlyrounds top-left device bounds directly; atbottom337.5 these differ byone because H-qRound(H-y) !=qRound(y). Implement native-material-only scissor helper with actual target dimensions/origin and Qt's projection/qreal rounding, retaining current fallback for non-native contexts. Add independent clipped-rect Qt GPU goldens atfractional38.75height; runallthree parity afterfinal sourcefreeze.

Last moving-time-label native diff: dual-expand150@1 differs only in29pixels of last '-2:22' digit. qt_native_place comment claims QFixedPoint::fromPointF but usesround(); installed qfixed_p.h:36,169 actuallyTRUNCATES r*64 toint. Ataligned localx173.48109, rounding selects.5-subpixel mask; literalQt truncation selects.25. Change the two QFixed point conversions (x andbaseline) totrunc, retaining existing device qRound/snap. Add direct independent QFixed regression and verify nativeGPUgoldens/barbeforekeeping. No glyph-specific correction.

Scope refinement for QFixed: preserve the existing Skia fallback's point quantization; use corrected QFixed truncation only when raw_native is active (explicit QtGl mode on a supported native GPU canvas). This avoids changing non-opted-in panel text while fixing the island's two last-digit crops. Native GPU goldens remain exact. Keep the actual QPainter outline conversion global, since the independent source outline/raster oracle proves that arithmetic itself.

### PQ-QA 12:55 EDT — bar GL ±1 rounding regression (parity QA)
After the missing-icon references (bar recapture; tray icon class now 0 failures) the bar GL run
shows 212 `fill/color` failures, all max 1, e.g. `baseline--dark@1` (28 px): rounded-corner
translucent pixels ref PM (25,25,51,10) vs Rust (26,26,51,10), and text/fill pixels ref
(122,127,151,228) vs Rust (122,128,151,228) — Rust is consistently +1, i.e. a round-half
direction change, not geometry. References are reproducible (single-fixture recapture sha
`239d5b91…` identical to the published one). In the 12:10 run these same states were exact
(bar GL 247/312), and only `hyprv-ui` (`native_gl.rs`, `qt_painter.rs`, `render.rs`, `egl.rs`)
changed in between, so the in-flight projection-uniform / native-material experiment is the
likely cause. Please run the full bar control (`python3 parity/verify/verify.py --component bar
--backend gl`) before keeping it; revert if bar < 247 exact.

## Single-rect opacity layer on native GL targets (spec-bar, 2026-10-23)

`render.rs` `node()`: a `OpacityMode::Group` node whose content is one childless `Rect`, with no
filter or MultiEffect, is drawn without a save-layer when `native_gl::material_target(c)`. For a
single leaf, group opacity equals per-node opacity, and the Qt rectangle material applies it per
vertex. Raster canvases keep the layer, so CPU output is unchanged.

Motivation: panels' QuickAdjust card puts leaf rectangles in single-leaf opacity layers for CPU
parity (ENG-OPACITY-1 workaround). The layer blocked native GL, and the panels GL
QuickAdjust error went 4,345 → 404 differing px with this.

Engine tests pass; bar/island scenes don't use childless Group-opacity rects.

Engine PQ-QA 12:55 active investigation: four native GL state blocks now enable
DITHER where they previously disabled it. This can change RGBA8 store rounding
across text/rect/image draws. Testing only that state bit first, retaining all
island geometry, clip, Canvas and text fixes. Diagnostic negative control
HYPRV_GL_DITHER_EXPERIMENT=1 restores the current QA behavior on the same binary;
default returns DITHER to disabled. Full three-crate GL controls follow. Please
coordinate before editing native_gl.rs while these paired runs are active.

Final control provenance warning (not an engine regression): concurrent QA changed bar/examples/reference.rs to missing-icon checker fallback and automatic full-output viewport while our saved gate-inputs references still contain the old letter fallback/crop protocol. Mixing the new adapter with that snapshot gives111/304; all193 losses are the changed18×18unknown trayicon (baseline old/newdifference bboxx2987..3004,y20..37), native111/111 remainexact. Running two matched controls: current adapter/current QA references; private owned frozen-protocol adapter/current engine against original savedbarcontrol. Live adapter is not reverted or edited. Owned scripts island/tools/check_bar_{current,frozen}.py record inputs/binary/protocol. Do not attribute mismatched-reference failures to text/shader changes.

PQ control result: DITHER on/off × old Skia/new blit copy yields byte-identical
four baseline images on the same binary (target/parity-rounding/{off,on}-{new,old}).
Restored original DITHER state; experiment branch removed. Reported examples
recover identical integerPM pixels and exhibit Qt-vs-Skia PNG half-tie conversion.
Adding shared capture transport validated by island's independent exhaustive Qt
oracle; requesting bar owner to use it for the final crop re-encode. No comparator,
reference, threshold or production blend arithmetic changes are proposed.

Final regression receipts: matched frozen bar control using the current engine, full output viewport and original letter-fallback inputs remains303/304, exact191, paintedMAE0.025054 unchanged. Cropping its render target instead caused39max1 regressions; full viewport is required. The current live QA adapter/references also need privately regenerated tray assets atdevice request18/36,DPR1 (shared cache stillhas64pxChrome@2). That separate matched-input check is recorded underisland/artifacts/bar-current. No live bar source was overwritten.

Current live-reference bar check completed: privately baked current hicolor inputs
(18/36 device request,DPR1, no shared-directory mutation) improves stale-asset
198→288/304 exact; gatev1=300,max24. Remaining12 max1–2 and4 menu/submenu crops
are routed inhyprv-ui-bar.md. Frozen matched renderer control is unchanged303/304,
exact191,MAE0.025054. Thus report both versioned controls; do not compare their
counts as if the references/DF flags were identical (oldmanifest196DF,new4DF).

Island final handoff:368/380 canonical exact from0, integerPMerror119801.
Eleven remaining max1 crops sum49units; largest frozen outlier119752 is proven
stalehover by actual Qt pointer intervention(max0reproduction), not rasterization.
Finalsourcefreeze hasno changes.93ordinarytests,scopedall-targetClippy,5nativeGL
oracle tests and11renderer/cacheGPUtests pass. Shared-context6000framestress:
23994checks,4mappedsurfaces,1974resizes,63recreates,zero mismatch. Details and
per-class/residualtable: crates/hyprv-ui-island/VALIDATION.md. Keep genericGPUriskOPEN.
All production experiments without measured improvement were removed; retain
consolidatedrectmode, existingimage/effectprograms, one lazilyownedNearestmasksampler.
Capture scope refinement: retain EglWindow's original capture encode (no runtime
change / no full-viewport CPU conversion). Apply Qt PNG serialization only at the
bar's final cropped encode, via the user-approved one-line call to the shared
helper. Island already has the independently validated Qt serializer; its GPU
render changes remain untouched. Exhaustive legacy-PNG→PM→Qt-PNG round-trip
regression added to prove that intermediate PNG transport recovers originalRGBA8.

Last precision experiment (17:12 UTC): independent Qt meshes match exactly for all
representative residual dynamic sizes and the44px control (precise-residual-mesh.json),
so no double-size node API is justified. Test desktop GLSL150 spelling of the same
canonical smoothcolor shader (explicitin/out/output rather than compatibility120),
only insideqt_gl_rect programcreation. EmbeddedGL unchanged. This isolates possible
GPU interpolator/compiler rounding; revert immediately unless it closes the tiny
residuals, then run all UI controls. No tessellation/color changes.

ENGINE PRIORITY SOURCE FREEZE (user PQ-QA request): full three-crate production
GL verification is running now, with user-approved bar final Qt PNG encode.
Please defer/revert unproven GLSL150/other new precision experiments until this
receipt completes; coowner edits during a replay invalidate causal attribution.
Retain the measured island368 win. Engine changes left in flight are the shared
capture serializer/tests and host-only startup calendar preparation. Original
DITHER state and original EGL PNG encoder are restored; old/new copy × dither
baseline controls are byte-identical. No new shader arithmetic from engine.

GLSL150 negative control: all380 PNGs are identical to the verified368/380 run
(error119801). Restored the original canonical GLSL100/desktop120 shader path;
no compiler-version experiment retained. The restored rect-shader hash matches the verified replay. Three independently
edited engine files (lib.rs, egl.rs, wayland.rs) changed meanwhile; rerunning all
three UI controls against that current tree before finishing. Representative residual Qt/Rust meshes also match every component.
The11 tiny edge crops therefore still need deeper transform/interpolator analysis;
do not add an unproven alternate geometry or color correction.

Concurrent-context refresh completed: current engine/island sources exactly match
fresh final replay (changed=[]), island368/380, matched frozen bar303/304 and
panels86/300. New engine context passed another6000frames with776sampled
readbacks/4surfaces/1974resizes/63recreates, zero mismatch; five nativeGL goldens
also pass. Original dense23994-readback stress is retained as a separate earlier
receipt. No version150 shader, duplicate rect mode or geometry-size API retained.

Final all-target tests on the refreshed combined tree: 96 passed, 0 failed,
19 intentionally ignored; strict all-target Clippy passed. Island final source
hashes checked again: changed=[]. Current receipts are linked in island VALIDATION.md.
- 13:20 update (parity QA): resolved — bar GL now 276/312 exact (remaining fill/color 12, text 4); the ±1 round-half regression is gone.

## Island follow-up — 17:24 UTC

Qt's `qsgbatchrenderer.cpp` mergeability requires all elements in a batch to be translation-only for RequiresFullMatrixExceptTranslate materials. Actual smoothcolor draw trace on no-length-next-hover: play and next form an unmerged batch because next is scaled; play's matrix therefore translates to(298,129), local vertices0..44. Rust instead bakes play translation into clip-root vertices. This explains its two max1 crops at the circle's bottom edge. First diagnostic: temporary environment-gated no-merge rectangle basis, compare380; remove gate after experiment. Plan retain only a faithful batch-merge compatibility decision with tests, not per-crop coordinate adjustments. Qt source downloaded offline under island artifacts/qt-source; original runtime untouched.

Unmerged diagnostic:338/380 overall (expected regressions from disabling all merges), but BOTH no-length-hover-next crops become exact. All other new errors are max1 edges where Qt still merges. Replace temporary switch with clip-scope material compatibility: collect inherited opacity values of smooth rectangles with non-translation transforms; compatible rectangles in that clip scope use their full matrices. Separate scopes reset at every batch root. This mirrors RequiresFullMatrixExceptTranslate; start with explicit clipping scopes, validate all380 before retaining. No public Node API change.

Clip-scope compatibility replay:370/380 (was368), no lost passes; error119795 (−6). Both next-hover controls exact. Add regressions for opacity/scope eligibility and an independent Qt sibling-control golden before retaining. Remaining9 max1 crops total43 units plus stale-hover outlier. Next diagnostic logs only: temporary env-gated rectangle matrix/vertex stderr in qt_gl_rect.rs for matched Qt/Rust upload comparison; will remove all temporary instrumentation before final checks.

Engine final valid GL checkpoint: bar276/312 canonical passes,288 exact rendered
images (12 non-live battery exclusions;8 foreign host captures not bar-adapted);
island368/380 exact; panels86/300 passes/50 exact. No prior QA pass lost in any
component; all2511 reference PNG/metadata hashes unchanged. User-approved single
bar final-crop Qt serializer call closes the reported half-tie transport issue;
original EGL serializer/DITHER state and renderer arithmetic remain intact.
Panel standard run had248 missing outputs after a concurrent raw-directory
removal; official-verifier retry with private OUT/REPORT paths restored all300.
Report docs/verify/pq-qa-rounding.md and engine-parity-rounding.md; compact receipts
target/parity-rounding/summary.json. The all-three gate remains incomplete.

Further actual-upload proof (dual-agent70@2): primary frame's272 vertices and uniforms match bit-for-bit. Detached38px music pill does not:178 x-components differ by one float step (Qt231.70750427246094 vsRust231.70748901367188). Root uniform and standalone mesh match. Current image_geometry recovers child translation by subtracting absolute world translations, losing precision; QSG composes relative-to-batch transforms directly. Add scoped tracking of exact Node-local matrix composition, reset at existing clip/effect batch roots, used only if its tracked absolute matrix equals the draw's matrix. Existing inverse/cancellation fallback remains for untracked canvas operations. No public API/geometry/color changes. Test all UI corpora and a numeric cancellation regression.
Calendar preparation memory check: it frontloads ~139MiB RSS (disabled260MiB,
prepared398MiB), but post-calendar/control interaction both reach ~444MiB and
GPU post-activity is360–361MiB in both. Thus it shifts existing cache/allocator
high-water residency to startup rather than increasing the measured active peak.
Testing a GNU-only startup malloc_trim after temporary target purge to return
free compiler/font-stream allocation pages without discarding live warm caches;
not on idle/frame callbacks. Will retain only if measured memory benefit holds.

17:39 source-size precision is now justified by actual Qt uploads, unlike the earlier negative cases: dual-music-expand60 width262.99125364431484,height125.70845481049561,radius25.991253644314867. Qt first inner corner x237.0; Rust236.99998474121094 (265 differing components). Qt uses QRectF double edges in float corner-center assignments (qsgbasicinternalrectanglenode.cpp:492-522), even though width/delta/radius are floats. Plan minimal optional Node.qt_rect_size:(f64,f64), defaultNone; island's two animated frame rectangles set it. Cache includes source-sizebits; one existing tessellator gets a precise-size entry point with Qt's exact staging. No extra rect mode/GPU program/cache, no global widening, all other callers retain default path. Island reconcile copies metadata. Re-run all380 plus bar/panels controls before retaining. Node metadata adds24bytes; GPU cache remains bounded at existing budgets.

Regression additions: engine qreal mesh fixture is independently generated by installed QSGDefaultInternalRectangleNode (all256 vertices/8components/indices asserted); direct-relative cancellation unit proves the previous world-subtraction loses one float step; scaled-sibling GPU golden atDPR1/2 passes; opacity/clip-scope policy unit added. Island reconcile regression detects qreal-size metadata changes even when float frame bounds are identical. Temporary RECT_TRACE logging still present only for finite diagnostics; removal and all-target checks next.

### PQ-QA 13:40 EDT — panels text-interior ±1 (parity QA, GL run 13:39)
GL matrix: island 370/380, bar 276/312, panels 128/300 (rect AA 100→14 since 13:20, thanks).
Panels' remaining 134 `fill/color` failures are 128× max 1, `text` share ≥ 0.99, mostly control
panel (cp-press/open/brightness/bt/hover/main/page), stats, wifi. They are SOLID glyph interiors over
the translucent card, not edges: `wifi-expanded-secure--dark@2` has 451 px ref PM (194,203,231,229)
vs Rust (194,203,230,229), and 37 px (136,141,162,150) vs (136,141,161,150) — always one channel,
Rust one LOWER. A solid-interior error means the text colour's per-channel value differs before
blending (coverage = 1), e.g. truncation where QSG keeps the float colour uniform
`(r·a·opacity, g·a·opacity, b·a·opacity, a·opacity)` and lets GL blending round. Check the native
text material's colour uniform for the inherited-opacity path (the Canvas fix at 16:36 UTC is the
same pattern).

Final island production result before QA hover decision:379/380, integer error119752, all rasterization residuals0. Temporary qt_gl_rect.rs RECT_TRACE branch removed. Existing shader/program sources unchanged. Qreal size, relative transforms and material-batch compatibility retained, pending final all-three UI verification. Row hierarchy is island-only. No extra render mode or GL program was introduced.

## Scaled native text on raster canvases + batch-geometry finding (spec-bar, 2026-10-23)

**`text.rs` (engine-wide):** NativeRendering text under a uniformly scaled item now follows
QSGTextMaskMaterial on raster canvases too, as it already did on GL (`native_gl::draw_transformed`):
- glyph masks are rasterised at the output DPR;
- each glyph quad goes through the item matrix and is snapped to `floor(x + 0.5)`;
- the mask is drawn with Nearest sampling.

Previously the raster path re-rasterised glyphs at the scaled size, so panels kept its own
coverage-picture fallback (now removed). The panels CPU gate stays 248/248, and the QuickAdjust
t0060 frames are max 3, unchanged. GL: the same frames now pass the strict gate.

**Batch geometry: no engine change needed.** Real Qt 6 RHI goldens (`textref`, private headless
weston, dpr 2) of the exact QuickAdjust slider tree pass pixel-exact through the existing
native rect + batch-basis path, on both GL and raster. The tree is: track, a fractional clip
x 0.2878 / w 96.947 / h 61.6016, the fill offset by −0.2878, and a border-only overlay, all at
radius 19. The panels corner residue came from the harness rendering a crop-sized GL target:
Qt's projection uses the whole output. The panels harness now renders 3072×1728 and crops, as
bar's adapter does. Other crates' harnesses should do the same.

**Reference PNG encoding.** parity-host saves `QImage::convertToFormat(RGBA8888)` of the
premultiplied grab. That's Qt's SSE4.1 `convertARGBFromARGB32PM_sse4`:
- per ≤1024-pixel chunk, groups of four use an `rcpps` reciprocal, one Newton–Raphson step and
  round-to-nearest-even;
- the remaining pixels go through `qUnpremultiply_sse4` (qt_inv_premul_factor, +0x8000).

Skia's unpremultiply differs by one unit on many translucent pixels, so a byte-exact
render still failed the strict (unpremultiplied-PNG) gate. Panels' harness now encodes like
parity-host (`tests/shared_common::qt_unpremultiply_rows`; `rcpps` matches this capture host).
Other harnesses may gain similarly. Fit: 68.6M equal-premultiplied samples.

Before final source freeze, harden the small smoothcolor compatibility pass: the opacity-only set could unnecessarily unmerge an unrelated rectangle when Qt splits a batch at a clip root or an overlapping intervening item. Replace it with per-NodeId decisions using Qt's forward compatible-element/overlap barrier rule (qsgbatchrenderer.cpp prepareAlphaBatches/uploadBatch). New private qt_batch.rs only, no public API; native draw gets a scoped merge decision from Renderer::qt_rectangle. Add overlap/clip/opacity unit regressions, then repeat all-three controls. Current pre-hardening controls: island379/380, recaptured bar304/304 exact, panels255/300; all128 passes in the independently retained QA panel control are preserved. This is precision-basis selection only; no draw reordering or extra GPU cache.
Allocator-trim trial concluded: startupRSS394.8MiB vs398.3–398.5MiB, with frame
max9.50ms, but this does not remove the139MiB warm-cache frontloading and is too
small/variant-confounded to justify a new libc coupling. Removed malloc_trim;
retain only temporary GPU-target purge and live cache preparation. Disabled and
prepared processes still have the same ~444MiB post-interaction peak. This memory
tradeoff is recorded explicitly in pq-qa-rounding.md for the memory owner.

Final validation window begins17:52UTC (roughly5min). The per-element overlap/clip batch decision passes all52 ordinary engine lib tests (11 ignored GPU regressions run separately). Rebuilding/rerunning island380, current recapturedbar304 and panels300, plus all GPU goldens/all-target tests/Clippy. Please avoid touching these precision paths during the receipts where feasible: native_gl.rs,qt_rect.rs,qt_batch.rs,render.rs,scene.rs. Existing independent owner context/memory changes are preserved. Tools now fingerprint reference PNGs as well as sources before/after each run.

Post-approval island380/380 exact; bar304/304 exact; panels255/300, all255 pre-hardening passes retained. Six nativeGL goldens pass. New6000frame/4mappedprivateSurface stress:776readbacks,1974resizes,63recreates,2662partialdamage,zero mismatches. Source fingerprints detected a concurrent capture.rs update during the380 replay (no reference drift). Rebuilding and rerunning all3 against current source; please hold all hyprv-ui/src precision/capture edits for the next5min where feasible. No owner change reverted. The initial GPU-golden invocation omitted HYPRV_PARITY_BACKEND=gl and returnedNone; corrected explicit-GL invocation passes all6 (island-followup-gpu-tests-gl/run.log).

Final refresh note: accepted-tests.log initially failed two newly added capture-tail tests (1x1 expected [25,51,76,10], got [26,51,77,10]). Source has already changed again to reciprocal SIMD tails and single-pixel oracle. I will preserve that owner update, wait for it to settle, then rebuild tests/replays. Please log when capture.rs is frozen; no precision paths were edited in this validation window.

Current combined-tree all-target tests:143 passed,0 failed,21 intentionally ignored. Unscoped strict Clippy reaches a concurrent panels DATA RefCell<HashMap<(usize,u64),Rc<Vec<u8>>>> at panels/src (type_complexity); no panels source touched by bar owner. Rerunning strict all-target --no-deps on the three changed crates. README now documents optional qt_rect_size plus clip-relative/batch coordinate semantics.

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

Engine closing receipt: latest official-verifier GL run (isolated outputs and bar
input/report path) is bar280/312 pass/292 exact, island379/380 exact, panels255/300
pass/219 exact; no previous QA pass lost and reference hashes unchanged. This
includes the later measured island precision package and panel-owner capture
adapter changes; not attributed solely to engine. Five shared-capture oracles
pass, including independent single-pixel and45 narrow/multiline images. Qt's SSE
EPILOGUE uses qUnpremultiply_sse4 too; no incorrect generic scalar tail retained.
Final calendar release29/29 frames under16.7ms, max12.62ms; model max1.58ms vs
84.68ms disabled. Scoped owned Clippy clean (--no-deps);102 UI/shell library
passes. Memory tradeoff and exact tested ELF hash in docs/verify/pq-qa-rounding.md.
No precision files in the coowner's17:52 validation window were edited by engine.

## Panels stats last-edge investigation — bar/island owner,18:12UTC

User assigns stats/control/Wi-Fi/Bluetooth to me, spec-bar keeps QA/calendar/startup. Shared Canvas port is exonerated: installed QPainter with Rust inputs matches exactly; stats history residue is fixture delivery-clock mismatch (fixed in panels test translator). Four dark@1 stats crops each retain the same3chrome-edge pixels/4integer-error units. Add TEMPORARY CORE_EFFECT_TRACE-only source/blur texture +placement readback in native_gl::effect_at, remove before finishing. Read current function immediately before this edit. No shared renderer behavior change planned unless actual Qt GL source/quad proof justifies it; bar304/island380 controls required afterward.

## Engine memory-budget follow-up (2026-10-23) — coordination with svc-media

User now requires RSS <=320MiB after all popups, with idle well below that. The
whole-calendar preparation is not acceptable merely because its ~139MiB startup
shift leaves the activity peak unchanged. I will profile a smaller first-frame
preparation and the host/EGL scratch lifetime; please post current heap attribution
and avoid concurrent egl.rs/assembly.rs edits while I test. Your mapped font-stream
changes in text.rs and panels qtext dedup are preserved. qtext still constructs
a fresh FontMgr per face; panels owner has the existing request to share
hyprv_ui::text::font_manager(). ENG-MEM-1 scratch purge on close is still absent
in current EglWindow::drop; I am taking that narrow lifecycle change.

QA lead verify.py is authoritative. Engine280/312 and bar-owner304/304 receipts
are different checkpoints/cohorts, not evidence warranting a bar change. No bar
rendering edits or regression conclusions without QA confirmation.

Stats edge causal proof18:15UTC: Qt/Rust source differs at exactly(44,542): alpha39 vs40; blur levels1/2 carry1unit each, levels3/4 exact. Qt GL layer projection y=0.00314465398 (=float2/636); Rust computes float(2/572)*float(572/636)=0.00314465421. All mesh, opacity, quad dimensions/indices/placement match. Plan minimal merged-rectangle layer projection metadata on the active native target: Renderer supplies original logical padded extent; merged root rects use QMatrix4x4::ortho coefficients directly, pixelSize stays physical2/548,2/572. Unmerged/transformed/other materials retain their existing path. Offscreen target guard restores metadata automatically; no new mode/program/cache/public API. Run stats then bar/island/full panel controls; retain only with no losses. Temporary texture readback removed before final validation.

Engine ENG-MEM-1 implemented: EglWindow::drop drops its retained surface then
purges unlocked Skia scratch with the shared pbuffer current. No timer, budget
cut, shader/glyph-cache flush or renderer precision edit. Private-Weston test
proves >=4MiB scratch release and live-target red pixel preserved; existing
10 surfaceless renderer tests +2 private EGL tests pass,102 UI/shell lib passes,
scoped Clippy passes. Current release measurement pending shared-target LTO.
Reduced warmup is150 logical px (header/day labels/first timeline row), provider
events cleared in a temporary model; actual service/model unchanged. Baseline
pinned ELF ad5ae63d initial356.3MiB, post-calendar/control386.1MiB,48/48 render
frames under16.7ms max6.44ms. Not a causal comparison against older497MiB.

Candidate controls: stats16/16 exact, island380/380 exact, bar304/304 exact. Seven independent nativeGL goldens now pass, including new explicit548x572 layer source/612x636 logical projection at both DPRs. Temporary CORE_EFFECT_TRACE block removed; no GL readback/I/O hook retained. Numeric oracle asserts actual Qt matrix float bits and demonstrates old factorization differs. Final all-target tests, strict Clippy and final all-three replay follows after source freeze. No owners' edits reverted.

Engine attribution18:22UTC: reduced150px warmup plus scratch purge still356MiB
initial/379MiB after BOTH calendar/control close (52/52 render frames, max8.77ms).
/proc of our private nested process shows ELEVEN HarmonyOS_Sans_SC.ttf mappings:
four fully resident20,136KiB each plus seven partially resident~6.7MiB each.
This is RSS counted repeatedly even though PSS stays low. Evidence copied to
target/parity-rounding/cjk-mappings.txt and memory-small-mappings.json.
I am changing engine FT cached faces to share the existing mapped backing bytes
with Skia via new_memory_face2; this avoids per-named-instance mmap copies.
Please preserve text.rs while I do that. Panels qtext face/typeface ownership
also creates independent mappings via FontMgr::new/to_font_data; please reuse
shared file backing there. I can expose a safe mapped-font buffer API if needed.

### PQ-QA 14:22 EDT — last panels rect-AA ±1 (parity QA, GL run 14:22)
GL matrix: island 380/380, bar 292/292 rendered (0 fail), panels 290/300 (gate v1 300/300).
The calendar bugs filed at 13:30/14:05 are fixed. All 10 remaining failures are PM max 1 rounded-rect
edge coverage:
- `calendar-morph--{open-t0000,close-t0209,close-t0211,close-t0500}--{dark,light}@2`: the idle
  capsule's (r19 pill, logical x 1476.5–1595.5, y 11–46.5) AA rim, 13 px dark / 37 px light, e.g.
  alpha 49 (ref) vs 48 (Rust). The capsule sits at a half-pixel logical origin at @2 → integer device
  px; check the r=19 pill's vertex-AA rim in this composited (island-over-panel) path.
- `qa-show-hide--show-t0120--{dark,light}@2`: 2/8 px on the left edge (logical x≈1405–1413,
  y 10–60) mid-slide; ref PM alpha 86 vs Rust 87.

Engine memory second finding: prepare_fonts unconditionally shaped a synthetic
wrapped paragraph at300px for each common weight, even though the host only
requests face/style preparation and never displays those paragraphs. That
retains separate system-font-manager Skia typefaces in WrappedCache. Removed
the synthetic measure_wrapped calls; actual wrapped content still uses the
unchanged lazy cache. Dedicated regression verifies no retained wrapping font
collections/entries at font preparation, followed by successful real wrapping.
Measuring separately from shared-FT-mapping candidate to establish the delta.

18:25UTC bar/panels share verification: final shared tree renders panels290/300 (no prior255 pass lost), myshare184/184 (176exact), bar304/304 exact, island380/380 exact. Seven GPU goldens and10surfaceless+2private EGL regressions pass; fresh6000frame/4surface stress776readbacks0mismatch. All10remainingpanel failures belong to spec-bar (calendar8/QA2,max1), same as QA14:22 report. The final source snapshots caught your concurrent text.rs FT-mapping/wrapped-cache memory changes; I preserve them. Please log completion and allow a~3minute final UI parity window once text.rs settles. My rendering edits are frozen, no additional pixel changes planned. Shared qtext edit is only a local no-behavior cache type alias to clear strictClippy, already logged.

Engine text.rs memory edits are settled18:25UTC for your3-minute parity window:
no further text/precision edits planned during your controls. New shared-FT-buffer
regression passes for4namedweights, byte-identical glyphs/advances; all UI/shell
libs105pass (57UI+48shell),12GPU/privateEGL checks as logged separately;
scopedClippy passed before the final synthetic-wrapped-preparation removal.
Shared-mapping release initial335.8→332.8MiB, settledaftercalendar/control363.8MiB;
51/51frames<=16.7ms,max8.15ms. Final candidate removes only unused synthetic
wrapped paragraphs from font preparation; actual wrapping/caches unchanged.
Release LTO in progress, memory/frame result follows. AuthoritativeQA bar policy
stands; I have not changed any bar rendering/precision path.

Acknowledged text.rs settled18:25. Rebuilding and starting all3 UI controls18:26UTC, source/header hashes included; expected render ends~18:28, metrics ends~18:30. Please extend hold through18:30 where feasible. Final ordinary all-target/strictClippy refresh also includes your shared-FT/wrapped-preparation tests.

Engine final measured candidate f73c3062459c6acbbfba3886ef05ba8c9fea07bae61b6cfb304a0557d608d933:
initial312.4→312.2MiB, after calendar/control BOTHclose339.4MiB. Still19.4MiB
over320, NOT marked solved.38/38renderframes<16.7ms,max11.565ms; calendar/bar
model max1.605ms. Only engine font mappings/preparation and EGL lifetime changed.
Initial CJK mapped RSS67,424KiB, postclose87,560KiB (the extra20,136KiB appears
with control content). qtext::shared_font_data also permanently retains one
~20MiB heap buffer after to_font_data copies; replacing it with shared mapped
backing is the next concrete owner task, potentially enough to cross320butnot
claimed before measurement. Evidence target/parity-rounding/memory-final-*.json
and docs/verify/calendar-memory-budget.md. Text remains frozen through18:30
for the requested UI parity window. Please take qtext stream/buffer ownership
with svc-media; I have not edited that owner file.

Same-final-ELF idle-only skip-preparation control:199.95MiB vs312.18MiB enabled,
so remaining calendar frontload112.23MiB (historical~139). Small geometry alone
is insufficient: font buffers/mappings dominate. No cold input/frame pass is
claimed for the disabled control. Budget gate remains FAIL and production
readiness is not claimed. Please consolidate qtext source/typeface backing with
svc-media before accepting the startup cache under the new memory policy.

## Accepted assigned share — 2026-10-23 18:31 UTC

My share177/184→184/184 gate,176exact: control main/pages124/124 exact;
Bluetooth16/16 exact; Wi-Fi16/16 exact; stats9/16→16/16 exact;
VM/Viewflow4/4 exact; Mihomo8/8 gate,0exact,max3,paintedmean<=0.02991.
No canonical fixture/reference/comparator/mask/threshold edits. Public API unchanged.

Retained changes: test translator uses actual QML Timer delivery clock (observed
first50ms→1000ms interval950ms); native merged rectangles in explicit-size layers
keep Qt's direct logical orthographic coefficients instead of physical-factor
refactoring. Added observed-clock/unit-matrix regressions and independent installed-Qt
GPU goldens atDPR1/2. qtext change is only local FontDataCache type alias, no storage/
font behavior change. Temporary engine readback hook removed; old raw captures/
observer build/binaries removed after numeric/hash receipts. Tools/docs are owned.

Final combined controls: bar304/304 and island380/380 bit-exact, no drift and hashes
matchcurrenttree. Panels298/300, no prior255pass lost: spec-bar's concurrent calendar
work closes its failures; only QA show-t0120 dark/light@2 remain,max1. The full-run
source snapshot caught ONLY unassigned island/tests/calendar_morph.rs; recompiled/
replayed those52 afterward:52/52pass, no source/reference drift. Aggregate composes
fresh52calendar with unchanged248non-calendar; assigned-share sources never drifted.

Current all-target UI/panels/bar/island tests177pass,0fail,24intentionallyignored;
strict all-target --no-deps Clippy passes. Seven GPU goldens;10surfaceless renderer
and2private EGL checks pass. Fresh6000frame/4mappedSurface stress:1974resizes,
63recreates,2662partialdamageframes,776readbacks,0mismatches. Generic unreproduced
GPU-corruption risk stays OPEN in STATUS.md. Engine font-memory edits preserved.

Report: crates/hyprv-ui-panels/VALIDATION-core.md.
Aggregate: crates/hyprv-ui-panels/artifacts/core-panels/accepted-summary.json.
No edits pending from this owner; the requested parity hold can be released.

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

## Memory (svc-media, 2026-10-23) — see docs/verify/perf-shell-memory.md

- text.rs: `mapped_typeface` (mmap-backed Skia typefaces, no heap copy) added by svc-media;
  engine's `mapped_font_bytes` now shares the mapping with FreeType. Please expose a `pub`
  accessor (font file match + mapped bytes) for PANELS-MEM-1.
- [ENG-MEM-1] still open (Skia purgeable render targets after popup close; GPU 306 → 358 MiB closed).
- [ENG-MEM-2] [P3] native-GL text context creation costs 24 MiB of driver heap (324 allocations
  during shader setup) plus libnvidia-gpucomp 36.6 MiB resident: compile only the programs a
  surface needs, lazily.

Engine ENG-MEM in progress: public `text::font_mapping(family,weight)` now returns
`FontMapping { file,index,bytes }`, shared immutable process-lifetime bytes, full
FreeType named-instance index. PANELS-MEM-1 API handoff logged in panels requests.
Editing egl/native_gl/qt_gl_effect for lifecycle and lazy allocation; please avoid
concurrent edits there until engine's measured handoff. No shader arithmetic changes.

19:06UTC full-screen owner checkpoint for engine/svc-media: final64-image matrix is pinned and running. Component GPU controls are bar304/304 exact, island380/380 exact, panels298/300 (unchanged QA max1 pair). Last source guard saw ONLY text.rs public font_mapping addition + its unit-test update; I reconstructed the pre-addition bytes in memory and matched old SHA97e22a4d88eeca6c58bdf4adcb78863861944414c351e8c49ed9f33507f5d235 exactly, proving existing text code unchanged. Please publish a settled memory-change source checkpoint here before calling current-tree parity verified; I will preserve all lifecycle changes and can revalidate after handoff. No renderer edits from me in this task.

Engine memory source checkpoint settled (hashes target/parity-rounding/mem-eng-source-checkpoint.json). Changes: zero shader compilation in native Context::new; independent lazy effect variants (shader arithmetic unchanged); scratch flush/purge at close, shrink, preparation; host crops canonical four corners to tiny surfaces on whole-device-pixel margins, ui-bar.md §9.1. CPU and private production-GL cropped-vs-full-output comparisons pass at1/1.25/1.5/2 for3reveal states. Native10surfaceless+2privateEGL regressions pass incl.lazy-program allocation/deletion. Canonical GL verify.py is running; full-screen QA owner may revalidate against this checkpoint. No further renderer behavior edits planned unless a gate fails.
Engine checkpoint refinement: corner top crops now reserve their full42px animation
travel, so reveal never changes native corner geometry. The regression additionally
covers actual6144×3456@2 and asserts stable configs across reveal. Renderer shaders
remain settled. Source-checkpoint JSON refreshed after this shell-only change and
Clippy's needless-borrow correction (no semantic shader change).
Engine matched memory run (ELF46687969): RSS312.4→202.2MiB idle,
341.3→205.7MiB after calendar/control bothclose; GPU307→40idle,305→39closed.
Includes svc-media's mapped_font/qtext sharing (preserved, credited), tiny corners,
and lazy shaders.47submitted frames max16.616ms; inclusive engine.paint max16.923.
One cold control masked-compose program costs12.060ms. Refining ONLY production
prepare_renderer to exercise that material before input; no shader/draw arithmetic
changes. All non-preparation renderer code remains settled. New pinned release
measurement will supersede this first candidate. Private-GL crop/full comparison
now also passes actual6144×3456@2, with stable target geometry across reveal.
Canonical GL verify.py COMPLETE for memory renderer: island380/380pass, all380exact;
bar ALL304exported images exact (owner gate304/304). Aggregate292pass+12excluded+
8missing calendar-host/island-expanded-host (not exported by component harness);
no pixel failures. Panels298/300pass, same twoQA show-t0120 dark/light@2 max1;
242exact, unchanged known pair. Do not mistake aggregate292/312 for a bar regression.
Raw canonical results parity/verify/out/results.json; log target/parity-rounding/
mem-eng-parity.log. Subsequent edit is startup prepare_renderer masked-material
warmup only; component offscreen path never calls that preparation.
Engine preparation regression now explicitly asserts native effects.compose exists
after production prepare_renderer. It caught the first masked warmup's transparent
fill being culled; offscreen-only fill now nontransparent.10surfaceless+3privateEGL
tests PASS, including the new assertion. No source/arithmetic changes to any
actual UI node or shader. Final release rebuild/measurement underway; previous
memory totals202.2idle/205.7closed RSS,40/39GPU remain the first-candidate receipt.
Engine extended40s candidate run: RSS201.9idle/205.9closed,40/39GPU; one second-cycle
calendar resize took51.7ms outside egl.draw (draw max12.9ms), so NOT claiming the
frame gate from that run. Added opt-in scopes for EGL window Drop / scratch purge /
surface destroy to distinguish resource retirement from external scheduling stalls.
Final production material preparation now has explicit GPU regression (3privateEGL
checks PASS). No shader or pixel arithmetic changes; final pinned profiling pending.
Engine frame-budget coordination: please hold concurrent GL captures during the
next final pinned memory/frame run if practical. The previous51.7ms resize was
outside draw, while other shared_parity GPU jobs were active; attribution remains
unproven. Retire/purge/destroy timing scopes now available to distinguish actual
UI work from driver contention. Production source settled incl.visible masked
preparation regression; no further behavior edits pending unless profiling fails.

### spec-bar: `text::mapped_font` accessor for PANELS-MEM-1 (2026-10-23)

`pub struct MappedFont { file, index, bytes: &'static [u8], typeface }` and
`pub fn mapped_font(family, weight)` are a cached wrapper over `resolve` (fontconfig match) plus
`mapped_font_bytes`. No change to the engine's own text paths. Panels' qtext now builds its
FreeType faces and shapers on it: one CJK mapping process-wide, panels parity byte-identical.

Hand-off for the merged-batch model: panels `qa-show-hide--show-t0120@2` keeps ±1 on 2–8
rounded-corner px of the QuickAdjust slider under the card's animated scale (~0.997, transform
origin TopRight) inside a fractional rect clip. Settled frames are exact. That makes it a good
regression case for the relative-transform rect geometry.

19:25UTC FINAL ACCEPTANCE HOLD REQUEST through19:32UTC: all64 native comparisons and independent scorer pass on both pinned pre-memory and first merged-memory releases. Bar304/island380 exact, panels298 unchanged across repeated controls. Every source guard has been invalidated solely by ongoing memory preparation/profiling edits; the latest release build now compiles those changes. Please queue further hyprv-ui/host/panels rendering-source diagnostics until this short acceptance run ends, and profile existing pinned ELFs meanwhile. I will run64 + all3 component controls, publish receipts, then explicitly release this hold here. Provider implementation work can continue; canonical inputs and presentation types stay stable. No renderer changes from me.
Engine ENG-MEM FINAL: ELF81ed2a60ac90,40s idle /40s activity (2complete popup cycles):
203.1MiB RSS/40MiB GPU idle;205.2MiB RSS/38MiB GPU bothclosed, sampled peak207.2.
85/85frames under16.7ms,max9.439ms; inclusive engine.paint9.952ms. ZERO effect
compile during activity. Drop3.343ms max, scratch purge2.219ms, destroy1.144ms.
Final visible masked material preparation GPU regression passes; zero-program
Context::new / lazy-variant/deletion checks pass. Avoid redundant retirement
submit (paint/preparation already submit); keep scratch-only purge. Source settled;
checkpoint JSON refreshed; QA GL captures can resume. Canonical controls unchanged
as above; later changes are preparation/retirement diagnostics only, shader/draw
arithmetic unchanged. Full report docs/verify/engine-memory.md, raw compact summary
at target/parity-rounding/mem-eng-complete-summary.json. Fonts sharing coowner
mapped_font/qtext changes preserved and credited, public font_mapping documented.
Engine user follow-up after19:32hold: investigating remaining two QuickAdjust
show-t0120@2 crops (2dark/8light premultiplied channel-level1 pixels). Root README
now records accepted four-tiny-corners topology deviation. Baseline QA replay
stored target/qa-precision/before. Temporary env-gated force-full-matrix probe in
native_gl.rs is diagnostic only and will be removed; actual batch/rect arithmetic
will change only after a causal repro. Preserve current files; no panel source edits.

Service-idle attribution follow-up (no UI code edits): new uninstrumented release 777b2a2954637e47b7d70fc4f0dbe1e5abdfbeaea42f464bf6a5529cd12a85fb, controlled-quiet/read-only, overlapping private-compositor 90s comparison with identical 15 NewIcon + 15 NewToolTip signals. Service thread fell 85.465→42.889 ms; UI thread 178.776→142.201 ms (0.158% alone), total precise CPU 0.2974→0.2097%, so whole-shell <=0.1% remains unmet. Whole-source ELF changes include concurrent UI work; this does not prove an engine regression. Standalone budget/profile check pending. Raw: docs/verify/raw/perf-shell/service-idle-normal-{before,after}/; report docs/verify/service-runtime-idle.md. Please keep real visible stats changes separate from metadata-only UI wakeups when interpreting the residual.

After-profile receipt: docs/verify/raw/service-idle/perf-after-dso.txt + profile-after.stacks, user-only perf 7999Hz, no lost samples. UI samples include __vdso_clock_gettime (17.44% of all samples) and unsymbolized libnvidia-eglcore.so.615.78.08 offsets (e.g. 0xa8bb86). This locates sampled user CPU, not blocked time or proof of a driver spin. Service-runtime after profile is 44.590ms/90s; remaining samples are dispersed among sampling/watch/reactor/D-Bus work. Standalone uninstrumented check running.

Final standalone (same 777b2a29 normal ELF,90.110s,controlled-quiet,one private compositor stack) confirms UI142.739ms=0.1584%, services42.503ms=0.0472%, blocking3.911ms=0.0043%, total0.2099% precise (coarse ticks0.2219%). No thread creation/exit. Overlapping-compositor load does not explain the residual. Global<=0.1% remains unmet. Complete report:docs/verify/service-runtime-idle.md; no UI fixes inferred or performed.

19:40UTC FULL-SCREEN HANDOFF — SOURCE HOLD RELEASED. Pinned combined release
4f83022262c0c4e6d7654d41595575aa838e9ff7a1df3d75cd9480756ce28d81:
26/26 jobs,64/64 unmasked native + independent gate passes; max6, worst painted
MAE0.0165, minimum IoU0.99987. Full frames are0/64 bit-exact. Explicit four-corner
coverage:256 additional128px crops, each corner max4 across all64 images; no masks.
Fresh stable controls:bar304/304 exact,island380/380 exact,panels298/300 pass
(unchanged two QuickAdjust show-t0120@2 max1;242 panel images exact).
Host all-target tests57pass/0fail/7ignored; scoped strict Clippy passes; explicit
Wi-Fi GPU regression passes both themes. Current mapped GPU stress6000frames,
4surfaces,1974resizes,63recreates,776readbacks,0mismatches. Earlier GPU-corruption
risk remains OPEN. Host/UI/reference hashes do not drift during accepted matrix;
provider implementation work continued and fixtures use mocked Hub.
Report:crates/hyprv-shell/VALIDATION-fullscreen.md; all receipts under
crates/hyprv-shell/artifacts/fullscreen-core/. QA/spec-panels:please fold this
accepted table into docs/verify/parity-fullscreen.md; I leave that QA-owned report
untouched. Engine/spec-bar can resume the two QuickAdjust diagnostics immediately.
Obsolete private ELF aliases/plugins/failed probes removed; cleanup.json records
hashes. No shared target or another agent's files removed. Runtime stays Qt-free.

GPU-RISK follow-up (svc-power, current task): extending private lifecycle stress
and adding an opt-in DEBUG-only pre-swap retained-vs-default-framebuffer checksum
self-check. Surgical egl.rs hook + new frame_audit.rs; native_gl.rs arithmetic,
shader paths and cache policy stay untouched. New private harness will read every
submitted buffer, compare deterministic CPU scenes, exercise surface destruction,
scale/transform, cache eviction, context recreation/reset simulation, callback
pauses and compositor restart. Historical two named PNGs already have exact asset
race interventions in icon-race-reproduction/comparison.json; will reconcile the
stale OPEN status only after fresh lifecycle evidence. Engine:please avoid egl.rs
for this task or log overlapping work here; large host changes will be queued.

Engine QuickAdjust precision follow-up: scoped full-matrix AA vertices to clips
with inherited scale (DPR alone keeps merging), avoiding double rounding of a
snapped clip origin and its opposite child translation. Both show-t0120@2 crops
are now max0/0 different pixels on GL (before dark2/light8 max1). Frozen original
Qt left-edge crops added to hyprv-ui/tests/quick-adjust; real NVIDIA EGL test
passes both exactly at6144×3456/DPR2. No panels source or canonical ref edits.
All-three-crate GL controls running; temporary FORCE_FULL diagnostic removed.

Engine validation: newly added examples/lifecycle_stress.rs tripped clippy possible_missing_else in Window::drop; inserted one newline between independent popup/toplevel destroys, no behavior change.

Engine validation update: svc-power imports are restored. Bar GL replay now
completed at19:46UTC:304/304 exact, owner gate JSON at
target/qa-precision/final-gl/bar-report/gate/gate.json. QA comparator code
passes panels300/300, island380/380. Of680 panel/calendar/island exports,
678 byte-identical vs pre-task QA and the requested2 QuickAdjust crops now
exact. Engine tests pass:6batch,59ordinary library,11surfacelessGPU+3private
Wayland EGL,2corner checker; all-target UI clippy clean. Strict legacy corner
max4 is separately flagged (pre-existing, crop-vs-full native unchanged);
see docs/verify/corner-precision.md.

GPU-RISK prototype note: extending output coverage with a private Weston module
(60/144Hz). First plugin version hit Weston's shell-private assertion when
re-enabling an existing output; corrected offline helper to set backend mode
refresh and broadcast it without destroying shell state. Fullscreen stress windows
also now declare32x32 content geometry so rotated/outset test buffers fit the
configured fullscreen bounds. Neither issue was renderer corruption. Shared
target lost glow rmeta during a coowner cleanup/build; rerunning Cargo normally
per disk policy. No private target dirs. Final source guard waits until harness
is settled, and all experimental failures will remain qualified receipts.

GPU-RISK surgical change inventory:egl.rs has DEBUG-only audit/reset state,
robust-context opt-in attributes, private mid-frame resource eviction and a
pre-swap audit call. New src/egl/frame_audit.rs owns actual retained/default-FBO
byte comparisons, reset queries, guarded negative controls and bounded32MiB
scratch pressure under a2MiB cache limit. examples/lifecycle_stress.rs exercises
production SharedEgl/EglWindow with CPU oracles and rich GL/Skia controls.
tests/run-renderer-regressions.sh now includes the real-buffer negative-control
regression (passes:one poisoned pixel detected; next clean frame recovers).
README documents every flag/API and synchronous timing limitation. Ordinary UI
library tests59pass/15ignored;11surfaceless+3cache-lifetime+1frame-audit GPU tests
pass; all-target scoped Clippy and release all-target checking pass. Native
shader/rect/text/blit arithmetic is unchanged. More than one private compositor
is active for long stress; all PIDs/socket paths are owned temporary objects.

GPU-RISK CAUSAL REPRO (Mesa llvmpipe): first default-framebuffer copy after a
wl_egl_window resize is clipped to the OLD drawable extent. Retained RGBA8 image
matches CPU exactly; actual EGL back buffer can be entirely transparent even
though its EGL config isRGBA8. Both blit and Skia copy reproduce it. EglWindow::draw
currently makes the window current BEFORE native.resize; Mesa refreshes drawable
geometry on binding/swap. Preparing a surgical resize-before-bind fix: bind the
pbuffer on resize, notify wl_egl_window, then bind the resized window; invalidate
Skia state and discard retained storage as before. No glFinish/fence workaround.
Will prove with same-window resize CPU/back-buffer regression and all-three UI
controls, plus rerun lifecycle suites on NVIDIA and Mesa. Engine:please preserve
the new debug audit hooks and queue egl.rs edits while I land this fix.

GPU-RISK resize refinement: rebinding alone fixed initial acquisition but still
failed after82 frames (old 640x64 buffer reused on400x300 resize, all transparent).
The production Engine already recreates native windows on size change precisely
to avoid stale EGL buffer extents (wayland.rs~910). EglWindow::draw now applies
that same guarantee for direct users by unbinding/destroying/recreating its
EGLSurface at the new native size. EGLSurface is Option internally so failed
recreation cannot double-destroy; no public API changes. A new seven-transition
same-window CPU+actual-back-buffer regression includes grow/shrink/mixed-axis and
fractional scales; its tiny initial16x16 catches the old first-frame bug. No fence.

GPU-RISK final candidate: EGLSurface recreation fixes both initial and later
mixed-axis resizes on Mesa; two240-frame CPU+back-buffer runs now pass. The earlier
pbuffer-only candidate is explicitly retained as FAILED (82/42 checks before
clipping). Negative-control and seven-transition resize regressions pass on
NVIDIA; Mesa test run and final multi-surface matrices pending. All renderer
shader math unchanged. Final source freeze begins now for the post-fix search;
please queue shared egl.rs/frame_audit.rs changes until this task's receipts land.
Temporary standalone format probe moved out of examples into a small .rs.txt
receipt under bar/artifacts/gpu-risk-mesa-format; no unused probe target source.

GPU-RISK source freeze RELEASED: all final lifecycle matrices complete. Eight
post-fix runs share ELF0f30933fc877f282c64394a593ac17d06a298f44d2f6a79af80d04602540f742:
21,952 complete submissions,18,410 CPU/control comparisons incl458 preserved
before intentional compositor loss;18,406 exact pre-swap source/back-buffer
audits (incl112 fresh controls),0 lifecycle mismatches.2,752popup cycles,
1,752explicit surface lifetimes,192simulated owner resets,525pressure cycles,
5,540partial-damage frames. Primary aligned scenes CPU-exact; rich text/rect
profile max1 CPU and byte-exact fresh-GPU controls. Both default native blit and
forced Skia copy pass on NVIDIA+Mesa. The new same-window resize regression and
negative-control/next-frame recovery tests pass on both drivers;3cache lifetime
tests also pass on both.59ordinary UI tests,strict all-target Clippy,release
all-target checking pass. All3UI controls zero source/reference drift:
bar304/304 exact,island380/380 exact,panels300/300pass(244exact,max3).
Actual host13/13output/navigation cases and192window audits pass. Pinned full-screen
matrix26/26jobs/native64/64 passes,924window audits exact; independent scorer
finishing. No more renderer edits from me. Full report docs/verify/gpu-corruption.md.
Engine can resume shared rendering work; preserve the EGLSurface resize fix.

20:19UTC GPU-RISK CLOSED final handoff: independent full-screen scorer also64/64
(max6),native26/26jobs+924window audits exact. All3component controls zero drift,
304bar/380island exact,300panels pass. docs/STATUS.md reconciles the stale OPEN
line and links docs/verify/gpu-corruption.md. All stress/test/report work complete;
source hold is released. No live reload/cutover. Final diagnostics use debug-only
audit/fault hooks; EGL drawable resize guarantee is the sole production fix.

- 2026-10-23 sampling result: Performance follow-up, not an attributed renderer bug: final 90 s controlled-quiet normal release pair measured UI thread 144.929 ms -> 203.355 ms (0.1608% -> 0.2253%), while service + discovery CPU fell 0.0521% -> 0.0314%. Initial optimized UI run was 0.1345%; working-tree UI hashes are unchanged across the two optimized runs. Cause of UI variance remains unattributed; no UI rendering edits made here. Details/receipts docs/verify/sampling-cost.md.

Engine bounded SHM experiment (start21:26 UTC, deadline22:56 UTC): user explicitly
requested last90min attempt to eliminate stats EGL swaps. I am temporarily editing
wayland.rs, egl.rs, assembly.rs and adding private shm_tick.rs plus shell probe
example. Default production path remains GL; experimental env
HYPRV_STATS_SHM_EXPERIMENT=1 only. Will keep only if <=0.1% normal whole-shell and
byte-identical GL+SHM composition; otherwise revert these experimental hunks.
Please re-read these files before edits. No new Cargo target directories.

### FR-NET-3 plan (bar owner, user-authorized exactness over corner topology)

Qt’s original full-output corner overlay reproduces the reference exactly in a
private nested Hyprland. Its QPainter/Canvas pixels are correct already. QSG packs
four corner bitmaps into a screen-sized8192×4096 atlas, at1,1 /1,57 /1,113 /1,169;
GL sampling adds39+1 alpha pixels in the bottom-left alone, followed by another
1–3 from Hyprland sampling the full-output window. Standalone54×54 native textures
and CPU assembled SourceOver omit these stages. All observations are private;
no tolerance/mask/reference edits. User permits restoring one full-output overlay.

Proposed narrow engine additions: explicit Qt Canvas atlas image sampling for the
corner batch (test alpha-only R8 storage to save3/4 atlas bytes), plus opt-in direct
full-redraw EGL-window presentation so this passive overlay does not allocate an
extra81MiB retained texture/copy. Default surfaces, effects and text stay on current
paths. Engine: these changes avoid the SHM/stats patch; I will re-read before edits
and run all three UI gates. Code has not been changed yet at this checkpoint.

First surgical edits: LayerConfig.full_redraw defaults false; EglWindow opt-in
wraps FBO0 directly, redraws the complete scene and skips retained-target blits.
Explicit Engine.dump_surfaces redraws this mode without swapping before readback,
so captures do not read a recycled post-swap back buffer. Ordinary retained/SHM
paths unchanged. No topology switch until the atlas renderer is verified.

Engine SHM checkpoint22:13UTC: byte-exact same-frame GPU→PBO→SHM path passed12/12
real full-output screenshots (DPR1/1.5/2), no fallback; native strided/padding and
forced-timeout→GL regressions pass. Normal90s same-ELF pair: EGL0.11531%, hybrid
0.09148%. Final default-on/native-capability version now frozen for final checks;
HYPRV_SHM_PRESENT=0 selects EGL. New private modules egl_fence/gl_readback/shm_present;
bar hint only, no namespace/input/surface topology change. Real HDR output is10bit,
so I am checking exact8bit EGL config capability before accepting SHM; retained
RGBA8/source bytes remain untouched. Please preserve these fields and methods.

FR-NET-3 owner: our final bar replay collided with your ImageSampling enum while
native_gl match was unfinished (panels/calendar/island passed, bar build failed).
Current match is now present; I will rerun bar. Please defer the corner topology
switch until my bounded stats-task receipts finish (~22:35UTC) so the CPU/parity
comparison does not change underneath it. Your existing full_redraw/atlas edits
will be preserved; no revert of your changes from engine.

Atlas edits: ImageSampling::QtCanvasAtlas explicitly selects four packed A8 sprites;
new qt_canvas.rs uploads a padded, Qt-sized R8 atlas with black-RGB/red-alpha swizzle.
Native cache keys include atlas dimensions; allocated atlas bytes are fully charged,
ordinary mask/image budget stays8MiB, stale atlases are bounded to four. Shader UVs
retain original texture dimensions/allocator slot positions (simply using a64px
atlas did NOT preserve floating sampler rounding). CPU fallback selects the proper
sprite. Added original pre-swap Qt bottom-left golden, packed source alpha and
provenance; GPU test verifies exact pixels and single32MiB allocation. Portrait
short-axis allocation and landscape UVs have a focused unit test.

Direct-window debug self-check now freezes expected bytes BEFORE fault injection;
otherwise source/window would alias FBO0 and hide a test corruption. Normal release
frames still have no readback or retained copy. Will rerun the negative control.

FR-NET-3 final implementation: QtCanvasAtlas R8 swizzled sampling, padded Qt UVs,
actual-byte bounded cache; default image/rect/text material arithmetic unchanged.
Full-redraw direct FBO0 mode avoids one81MiB retained target at6144×3456; capture
redraws before readback. Frame audit freezes direct expected bytes before fault
injection, with a private-Weston negative test proving detection/recovery.
Ordinary UI65pass; NVIDIA GPU15+2+3+3pass. Optional Mesa has14/15 surfaceless tests
(the new NVIDIA-specific oracle explicitly unavailable); existing QuickAdjust
frozen-golden assertion differs atone alpha unit (6vs5). All remaining Mesa
surfaceless and all8private-Weston lifecycle tests pass. No test tolerance changed.
Full component GL gates and actual-compositor256corner matrix underway.

Engine22:33UTC: default RGBA-PBO repeats0.10296/0.09757/0.10262% (mean0.10105%),
not accepted. Last bounded variation is native BGRA readback (only when core GL
or EXT_read_format_bgra guarantees it), avoiding CPU swizzle; raw-byte/padding/
timeout tests and24native screenshots will rerun. Current Source added exact8bit
EGL-config eligibility; HDR10bit EGL configs remain on EGL, no color-format
selection change. Final quiet measurements expected~22:39–22:48; please avoid
GPU parity/capture workloads then if practical. Native full_redraw and atlas
work is preserved. Engine hard cutoff22:56UTC; will revert its own optimization
if the final budget is not repeatably met. No new private Cargo targets.

Bar owner22:40UTC coordination: just saw the22:33 request to avoid GPU load in
22:39–22:48. The final actual-compositor matrix has been running since22:31,
currently8/26jobs and46/64images complete, all completed corners exact. Component
GPU gates have FINISHED (bar300+12excluded/0missing, island380, panels300; all source
receipts fresh). No more component GPU jobs planned. Matrix should finish around
22:44–22:46; please do not treat quiet CPU intervals overlapping this matrix as
uncontended. Actual pinned shell/Qt-window sampling and matte are required for
FR-NET-3, so I am finishing this run rather than discarding it. No shared production
source edits planned; only reports/receipts/cleanup remain. Normal memory measurement
finished at22:35; it is explicitly NOT a quiet CPU acceptance run.

Engine22:44UTC: acknowledged matrix overlap; BGRA CPU windows0.09400/0.08289%
will be marked CONTENDED, not quiet acceptance. Please append a GPU-jobs-finished
receipt as soon as the actual-compositor matrix is done. I have22:56 hard cutoff
and need two fresh90s default windows +control/profile after your matrix ends;
will not extend or count overlapping runs as uncontended.

FR-NET-3 follow-up22:49UTC: complete first matrix is64/64 broad PASS, but strict
corners expose a final max1 on3pixels/theme in tray-overflow's top-right crop.
Actual original Qt bar-window readback proves these are pill AA pixels (two below
the corner shade), not matte noise. Original QSG trace shows the large right Row
promoted to a batch root at(1603.65625,10), while native chrome remains world-based.
Qt qsgbatchrenderer.cpp promotes transformed subtrees above64renderables or1024
vertices. Planned narrow API: Node.native_batch_root(false by default), scoped GL
batch basis without a clip or layer; bar uses it for the large right subtree.
Will add independent original-window AA regression, rerun all three GPU gates and
actual assembled matrix. No masks/curve adjustments. Component freshness changed
only because cutover/verify_fullscreen.py was edited externally; will refresh.

Svc-sys FR-NET-2 note22:50UTC: corrected an overly broad dependency boundary after the22:49 note: edits to cutover/verify_fullscreen.py now invalidate only the assembled receipt, not component render provenance. Shared renderer/widget/harness/policy/reference dependencies remain hashed for all component slots. This policy-module update itself needs the next fresh control, which the upcoming batch-root change already requires. No more gate/producer edits planned before final RC; I will run fresh canonical GL and actual-compositor gates on that frozen RC, so no extra GPU replay is needed solely for my earlier producer edits. Waiting for your source-final reports; no GPU workload from svc-sys during the engine window.


Engine22:50UTC final decision: SHM/PBO stats optimization REVERTED. Native BGRA
contended repeats0.09400/0.08289/0.08088% looked promising, but fresh uncontended
90s window was0.106831% with zero other test GPU clients. User required<=0.1%
within90min, so no default/opt-in production SHM/fence/PBO path remains. All984
old PNGs were unchanged;24native GL↔SHM screens exact;204/204 E2E passed the
prototype. Your full_redraw/Canvas-atlas code is preserved, including the generic
render/present split used by capture. Three renderer-test fixture literals use
explicit f64→f32 spelling only, preserving bits for strict clippy. Full report:
docs/verify/shm-tick.md. Only rollback checks/cleanup remain; GPU hold released.

22:53UTC batch-root fix verified: actual tray-overflow dark now4/4corner crops
exact. New independent5×84original Qt bar-window golden passes with promoted
basis and FAILS its unscoped negative control. Added stable QML-subtree eligibility
(not cached placeholder contents) and sticky promotion for RetainedView lifetime;
regression checks unchanged cached updates stay clean and tray shrink preserves
promotion. No clipping/layer/extra allocation. Ordinary UI/shell/bar tests and
all-target clippy pass; NVIDIA GPU16+2+3+3pass. Engine SHM rollback noticed and
preserved. Starting full26job/64image pinned-ELF actual-compositor rerun now;
expected~23:09UTC. Component GPU rerun also active~22:53–23:00. Please exclude these
intervals from uncontended CPU-budget claims. No more rendering edits planned.

GPU count correction after engine rollback removed an experiment-only offscreen
test: final NVIDIA script is16surfaceless +1offscreen +3cache +3frame-audit =23pass.
The previously logged “+2offscreen” described the earlier SHM-prototype tree.
Final ordinary counts: UI64pass, shell59pass, bar39pass; optional ignored tests
remain explicit. All-target clippy is clean. Production-source edits are frozen.


Engine22:54UTC rollback verification COMPLETE: rebuilt release no SHM/PBO marker;
strict owned-crate clippy passes; renderer regressions pass; fresh rollback native
E2E204/204 passes. Experimental binaries retired with hashes. Baseline GL/text/
damage performance work retained. Report docs/verify/shm-tick.md. Bounded attempt
ended before22:56; final uncontended0.106831% was a failure of<=0.1 target.

## Frozen RC idle budget (2026-10-23, FR-NET-2 validation)

Candidate `31bebad0450c520def4e1448168d940f8203ddf53def4bbbd07c715d87731edc`, frozen input fingerprint `f17e92a957d28092e0d0f8197b85c9dc5dbc2ecda2252195c107ab5053eacf46`. Full non-installing preflight passes release/tests/parity and fails performance: 60.095757 s controlled quiet, zero native hook notifications, 0.199681% process CPU / **0.205699% precise thread CPU**. Per-thread CPU: UI `hyprv-shell` 0.183547%, `hyprv-services` 0.020228%, `hyprv-agentproc` 0.001925%; PipeWire/module threads zero. Scheduling proxy 12.5799/s, not exact wakeup events. Frozen-hook/host-discovery window is 0.402195% precise CPU with zero hook notifications. Live idle has 15 notifications with observed non-ProcessScan changes and is not subtracted from acceptance.

Same-ELF diagnostic, explicitly instrumented and excluded from acceptance: `HYPRV_UI_PROFILE=1 HYPRV_UI_PROFILE_OUTPUT=... HYPRV_SHELL_DIAGNOSTIC=1 measure-shell.py --quiet-agents --idle-only --seconds 30`. The existing `bar dataStatus` call clears profiling after hydration (`assembly.rs:825`). CSV records 31 stats ticks, 33 frames, `engine.paint` 83.562 ms CPU / `egl.draw` 83.387 ms, **`egl.swap` 76.524 ms CPU (~91.6% of paint CPU; 2.319 ms/frame)**, `shell.stats_only` 4.744 ms. Scope at `crates/hyprv-ui/src/egl.rs:666` wraps swap-with-damage/fallback swap plus history push. This localizes the dominant instrumented CPU to presentation, not proof of a specific driver defect; instrumentation changes absolute costs. No code change or new backend was used. The 10.1 s mock metadata-wakeup regression still passes with zero metadata UI wakes and zero paints; 204 native functional cases and 256 exact unmasked corners pass.

Please investigate the production EGL presentation cost on this nested output (6144x3456 @2, NVIDIA) before treating the owner's rounded 0.100% checkpoint as this candidate's pass. Preserve strict GL/full-screen/corner gates. Evidence: `docs/verify/raw/final-candidate/20261023T231008Z/ui-diagnostic.csv`, `docs/verify/raw/perf-shell/rc-20261023t231008z-quiet/`, and refreshed `docs/verify/perf-shell.md`. No host reload, legacy restart, install or device action.

## Packaging RC performance refresh (frozen source, no renderer changes)

New ELF `9a9a43e4e7e94f22fee6a70b6482f7a7cf26bf9c3507994b1462c57760361e5f` / snapshot `1f71e398dd97e417718440289b80895d40d51f5b7fb1907ae79dc87d5dad9d23` has only packaging/startup changes versus the prior selected RC. Fresh controlled quiet is 0.294618% precise CPU (0.299457% process ticks), zero hook notifications; UI thread 0.271657%, services 0.021029%, agent discovery 0.001932%. All 64 full-screen comparisons and 256 exact corners pass. Raw measurement uses the identical pinned ELF and bundle, no instrumentation, ≥60 s, after build/render jobs stopped. `docs/verify/perf-shell.md` and `docs/verify/raw/perf-shell/rc-20261023t234946z-quiet/` retain the counters. This does not establish a packaging regression or renew the prior ELF's diagnostic as current causal proof; presentation/hot-path investigation remains open.

### Engine LIVE idle checkpoint (2026-10-24)

Live read-only baseline PID2709217 (installed eda5ca09): 180s0.943490% CPU,
UI0.197359%, services0.123308%, discovery0.358488%, blocking0.264310%.
Service fixes filed with owners in hyprv-svc-desktop.md / hyprv-svc-power.md;
engine does not edit those active crates (user's explicit instruction).

Prototype in own wayland.rs + shell assembly: opt-in immediate_static_frames
(defaultfalse). Settled static data edits paint without the empty wl_surface
commit / frame-callback round trip. Requires update_after_frame=false,
!pending and !animate; animations retain compositor callbacks and fixtures keep
their existing external clock. Static painting must not move the animation
callback-time anchor. Regression in tests/nested_rotation.rs compares native
EGL PNGs + callback counts + an animation resumed after static updates.
No renderer/material/shader changes. Benchmark before keeping this prototype.
Builds use shared target/CARGO_INCREMENTAL=0; packaging build may refuse its
first receipt because other owners are still changing captured source inputs.

## Quiet cooling release coordination (svc-net, 2026-10-24 00:37 EDT)
My cooling source is frozen (`cooling.rs` SHA4c7b968069248c0a2f0125b6e0911b8eb4e36bae8cb8f15202cf8a99f09bcc6e;41tests+Clippy green). The00:34full release build finished but correctly rejected parallel source drift; another shared builder PID2944422 is now active. Please avoid overlapping staging or source edits during the final stable build, or note any changes here/privhelper requests. I will wait for the active builder, validate manifest/source/binary consistency after it exits, and record one shared release ID for the user/RAPL owner to reuse. No live helper/sysfs changes or test-gate exemptions.

svc-net independently verified shared release `eeafb5a678912bbaa802016d562b2b69fcea3ab0afc20f2f2a54ddba788a04c5`: all4855current sources match receipt (including final cooling4c7b...cc6e), full manifest PASS, all3staged/flatELF hashes and stable metadata match.41helper tests+strictClippy pass. Read-only candidate+installed preflight both NOT READY: tests failed/stale (native Hypr fixture failures), stale barGL/perf, installed artifacts differ. No live/root/sysfs actions or source edits after freeze. Reuse this shared candidate; report docs/verify/quiet-fan-curve.md.

Engine Phase B cache audit follow-up: image::sample cached RGBAF32 pixels but
charged4bytes/pixel, undercounting4x. Fixed charge16bytes and uncached oversize
results. All3image caches (sampledF32/source mip levels/affine mip images) now share
one2MiB family envelope, alongside2MiB each Canvas/QtRect coverage/QtRect raster.
This is a cache residency change, not sampling arithmetic. Rerendering canonical
GL controls after final input hashes; earlier mid-build receipts may be stale.
No more production UI edits intended after this audit unless validation fails.
