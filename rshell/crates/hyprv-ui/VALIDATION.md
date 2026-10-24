# Engine validation — 2026-10-23

Scope: engine and its shell-host integration. Existing Quickshell/Hyprland
processes and configuration were not changed or restarted. Current end-to-end captures use only a private virtual KWin / nested Hyprland
and private D-Bus; no current test opened surfaces on the real desktop. Earlier
live observations below are historical and explicitly separated.

## Current checks

All commands use the shared root `Cargo.lock` and `target/`, without an isolated
manifest or unstable lockfile options.

| Check | Result |
| --- | --- |
| `cargo build --workspace` | PASS, all current workspace crates |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS, exact requested command; no target/dependency exclusions |
| `cargo test -p hyprv-ui -p hyprv-shell -p hyprv-widgets --all-targets` | PASS:123 tests;5 explicit compositor opt-ins skipped by default |
| UI coverage within that command |30 units;84-case Canvas coverage regression;3 effect texture;1 Qt effect;3 Qt rect;1 immutable subtree;3 review;3 shared raster;1 text reference |
| Shell coverage within that command |24 units and1 headless IPC/lifecycle test |
| Widgets coverage within that command |48 controls/viz unit/reference tests and4 core primitive tests |
| `cargo test -p hyprv-shell --test cli nested_ -- --ignored --nocapture` | PASS: both opt-in tests, six GPU scenes plus ten-frame assembled fixture replay |
| `HYPRV_PARITY_BACKEND=gl cargo test -p hyprv-ui --test canvas_strokes` on private headless Weston | PASS:84 Qt QPainter stroke cases, max channel diff0 through production EGL RGBA8 |
| Full-screen assembled fixture, DP-4 real DPR2, both themes | PASS runtime/capture:20 full6144×3456 NVIDIA GPU images; Qt comparison **PENDING**, missing references are pass=false |

Receipts: `/tmp/hv-final-build.log`, `/tmp/hv-final-clippy.log`,
`/tmp/hv-final-alltests.log`, `/tmp/hv-nested-final.log`,
`/tmp/hv-canvas-gpu-test.log`. Full-screen GPU manifests:
`target/fullscreen-assembled2/assembled-desktop--dark--DP-4@2/manifest.json`
and `target/fullscreen-final-light/assembled-desktop--light--DP-4@2/manifest.json`.
Their capture_manifest paths resolve the unique run-PID/fixture directory and
per-surface/composited PNGs. `target/nested-fixture-regression` exercises missing
reference failure semantics, actual animation changes and full-output dimensions
at a marked diagnostic DPR1. Six-scene smoke remains under
`target/nested-test-regression/manifest.json`.

Full-screen mode verifies actual native compositor dimensions and scale, requires
all expected native surfaces rendered, and rejects reference timestamp mismatch.
Earlier fallback640×360 captures were rejected and do not count as6K evidence.
The virtual replay clock alone drives fixture animation; production frame
callbacks and scheduling remain enabled. The harness uses private KWin/Hyprland,
XDG paths and D-Bus; no real session input or provider writes occur.
QA lead spec-island's full-screen reference request is in
`docs/requests/parity-capture.md`; `parity/reference/fullscreen` remains absent.
No full-screen parity gate pass is claimed.

The 450 Qt 6.12 easing oracle samples continue passing at 1e-12 tolerance.

### Review corrections UI-1 through UI-4

- Actual path geometry and Skia-expanded stroke outlines (caps/joins included)
  contribute to retained damage. Hide, move, remove and replacement are compared
  against fresh raster output for both path variants.
- Whole-value `Scene::edit` replacement preserves NodeId and advances its existing
  revision, including effect-cache invalidation. Both replacement and pre-existing
  cached-effect replacement regressions pass.
- Final Effect drop removes every dependency's weak subscriber immediately.
  A 1,000-effect unchanged-source loop leaves no listener storage; dynamic
  dependency recollection and cloned-handle lifetime tests pass.
- Both text modes preserve rotation, nonuniform scale, reflection and shear.
  Regression output is compared with an independently transformed local raster.
  Native snapping remains restricted to positive uniform axis-aligned transforms.

### Text reference evidence

`tests/text.rs` uses the supplied `experiments/text` snapshots and scene copied
into engine-owned fixtures. Across the three theme/DPR references (light at 1.5
and 2, dark at 2), all **39 native sample comparisons have zero differing alpha
pixels**. The 18 distance-field comparisons now have <0.2% ink error, worst
single-pixel alpha difference **1/255**. Independent Qt glyph-cache byte fixtures
cover 14 Latin/CJK glyphs with the same <=1 bound. The full Qt path-intersection
simplifier and fixed-point distance kernels are ported; runtime links no Qt. These are installed-font, single-line
reference results, not acceptance for arbitrary fonts or transformed Qt glyphs.
Wrapping, multiline and color emoji remain uncovered.

The production shell regression opens calendar, control, Wi-Fi, stats and OSD
on its isolated compositor. It also checks output removal/deadline ownership,
native mount recreation, brightness preview without a service write, crop device
phase and outside input masks, actual retained-scene updates, and unchanged island/
calendar subtree reuse. Native scroll tests cover fractional pixel accumulation,
Begin/Update/End, value120, relative-direction metadata and source semantics.
Native device/seat capability loss emits cancellation; ordinary pointer Leave keeps
an implicit drag alive. Actual physical touchpad input remains unmeasured.

Qt Canvas arc/quadratic path elements and Qt centred rounding use an independently
captured runtime oracle. Image filter tests distinguish nearest/linear/mipmap and
exercise the renderer, not only a sampler helper. These engine checks do not claim
full-screen component pixel acceptance. In particular Canvas uses Skia stroke
coverage; a complete Qt QPainter stroker/gray raster port remains outside this
geometry proof. Other owners are migrating rectangles/cache/GPU parity paths.

## Earlier engine measurements

The measurements below were taken at the earlier engine checkpoint. They remain
historical observations; current automated/live checks above exercise the updated
engine independently.

## Live Wayland/EGL smoke

Command:

```sh
WAYLAND_DISPLAY=wayland-1 scripts/check.sh test --test wayland -- --ignored --nocapture
```

Result: **PASS**, approximately 0.27 seconds. Observed one output global; resize
notifications reported scale `[2.0, 2.0, 2.0, 2.0]`. The test created two independent
100×30 logical layer surfaces on that output, both using normalized MultiEffect
shadows and the same EGL/Skia context. Both configured and rendered. An external
thread sent a scene change through the calloop service handle, producing one
additional render. Idle dispatch produced no additional renders. Both surfaces
closed cleanly, followed by dispatching pending compositor feedback.

This proves two-surface context reuse, not simultaneous display across distinct
physical outputs. Physical input was not injected.

## Idle and frame-time measurement

Command:

```sh
WAYLAND_DISPLAY=wayland-1 scripts/check.sh run --example demo -- --idle-test 4
```

Final observed run, debug build, animated rounded rectangle with normalized Qt
shadow and paragraph text on a translucent 400×100 logical layer:

```text
idle: CPU ticks delta=0, frame delta=0, dispatches=1 (includes final test timeout)
total frames=45, render+submit=120.361879ms
first frame=98.601926ms
warm render+submit mean=494.544µs
last render+submit=396.738µs
```

The first 1.2 seconds let the 800ms animation and presentation feedback settle.
The remaining approximately 2.8 seconds recorded zero process CPU ticks, zero
frames and only the final test-timeout dispatch. CPU accounting has tick
resolution; this is evidence of idle behavior, not an assertion of infinitely
precise 0.000% CPU. Production `Application::run` installs no timeout. Compositor
input, pings and service changes can still wake the process.

Frame times measure CPU rendering/submission, including driver calls, and exclude
waiting for compositor presentation. First-frame EGL/Skia/font/shader setup is
reported separately from warm frames. These are host observations, not latency
bounds or GPU execution measurements.

## Offscreen output

```sh
scripts/check.sh run --example demo -- --offscreen .research/demo.png
```

Result: **PASS**. PNG inspected visually: translucent rounded backdrop, blue
rounded animated item at its initial state, readable text, transparent corners.
Physical size is 500×125 for 400×100 logical at scale 1.25. Final observed run:
37.874229ms cold (includes PNG encoding), then 2.940739ms for 100 CPU raster frames
with text/effects cached, approximately 0.0294ms per frame. No Wayland connection
is made in this mode. The ignored `.research/demo.png` is reproducible with the
command above and is not a QML reference capture.

## Remaining acceptance evidence

- **PENDING:** component-by-component QML reference PNG comparison, including
  MultiEffect intermediate texture rounding and text outside the supplied samples.
- **PENDING:** physical pointer/keyboard/cursor behavior and autorepeat integration
  for consumers that require repeated key events.
- **PENDING:** distinct-output simultaneous rendering, output hotplug/removal, and
  live fractional-scale changes. The APIs are implemented; this session exercised
  one output at scale 2 and CPU rendering at scale 1.25.
- **PENDING:** independent Qt runtime oracle for SpringAnimation. Its numerical
  tests verify the documented source algorithm rather than a live Qt animation.
- **PENDING:** presentation-to-display latency/GPU timing. Actual presentation
  feedback is exposed separately from the measured CPU render/submission time.

Background blur remains the compositor's responsibility under namespace
`hyprv-quickshell`; no client-side background capture/blur is performed.
