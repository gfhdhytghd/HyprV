# Shared Qt Flickable implementation evidence

Implemented 2026-10-23 in `crates/hyprv-widgets/src/flickable.rs`, with only `pub mod flickable;` added to the widgets library root. Minimal integrations in bar `src/popups.rs` and island `src/island.rs` fix BAR-4 and ISLAND-2; their new `tests/flickable.rs` files cover drag/flick, click cancellation, exactly-once ordinary actions and stopping after hide/unmap. Other agents' controller fixes and renderer changes were preserved.

## Qt baseline and scope

The authoritative source is **Qt v6.12.0**, matching the installed Qt 6.12.0 libraries used for capture. Source excerpts with their original license headers are in `qt-source/`; hashes and URLs are recorded in `sources.json`. The Rust algorithm port is LGPL-3.0-only, matching the widgets crate's existing combined licensing. Runtime code is Rust/std only; Qt/C++ is used solely for reference generation.

The actual QML uses vertical scrolling, with no margins, snapping or custom rebound transition:

- Tray menu: `quickshell/shell.qml:992-997`, contentWidth equals width, contentHeight is the rows' height, clip true; other physics properties retain defaults. The test viewport is 282×402, with 30 rows of height 34 and spacing 1, matching a capped 420 px menu with 9 px insets.
- Agent ListView: `quickshell/DynamicIsland.qml:1318-1330`, row height 42, spacing 8, viewport height up to 386, `StopAtBounds`, interactive only when overflowing. The test uses 30 rows to ensure a nontrivial scroll range.
- Captured defaults: drag threshold 10 logical px (`QStyleHints`), flick deceleration 1500 px/s², maximum velocity 2500 px/s, three wheel lines. The 15 px minimum flick distance and default 400 ms rebound come from Qt's platform/Flickable implementation. These are the measured Linux defaults, exposed through settings where applicable, not claimed universal platform values.
- Default wheel deceleration is 15000. This selects Qt's proportional 72 px/notch, 300 ms OutExpo path, rather than the earlier hand-coded 35/60 px jumps. Separate references set `QT_QUICK_FLICKABLE_WHEEL_DECELERATION=5000` to test the three-sample velocity-accumulation path, including reversal and velocity limiting.
- Elastic drag, velocity-sensitive touchpad overshoot, flick overshoot friction, integer-ms timeline truncation, pixel-aligned endpoints, the 1 ms unused-axis fixup split, and 100 ms InQuad + 300 ms OutExpo rebound are reproduced. BoundsMovement can clamp the painted position while preserving the internal overshoot trajectory.

This is the vertical/no-snap physics used by these components, not a port of Qt's item/model/delegate machinery or horizontal layouts. The reference generator does not need GPU rendering: it compares scalar contentY, not pixels or font rasterization.

## Reproduce

```sh
python3 docs/reviews/scratch/flickable-implementation/capture.py
python3 docs/reviews/scratch/flickable-implementation/gen_cases.py
cargo test -p hyprv-widgets --lib flickable -- --nocapture
cargo test -p hyprv-ui-bar -p hyprv-ui-island --test flickable --test behavior
cargo test -p hyprv-ui-bar --test review_regressions
cargo clippy -p hyprv-widgets -p hyprv-ui-bar -p hyprv-ui-island --lib --no-deps -- -D warnings
```

`scene.qml` is a tiny Qt Quick 6 scene with the same Flickable/ListView settings and real child MouseAreas. `host.cpp` follows the parity host's QQuickRenderControl/custom-QAnimationDriver event model, advancing virtual time and sending timestamped wheel/press/move/release events. An unrendered offscreen window is used; it never connects to the live compositor or creates a real desktop surface. Explicit device velocity and Qt's native timestamp-based estimator are both exercised. The host dumps contentY and diagnostic movement/velocity state every frame. Unit assertions compare contentY; no claim is made that all diagnostic velocity signals are bit-identical.

`cases.json` defines the scripted inputs, `cases.rs` is their generated Rust replay, and the per-mode CSVs/meta files are independent Qt output. References cover individual/burst/reversed wheels, wheel bounds, the alternative acceleration path, pixel begin/update/momentum/end (including top-edge rebound), threshold crossing, fast/max-velocity flick, paused release, elastic bounds, boundsMovement clamping, geometry/model shrink, an external contentY adjustment during drag, and 8/16/32 ms frames. ListView overrides not used by this QML (elastic ListView bounds) are excluded; the plain Flickable covers its elastic/drag-only/overshoot-only settings, and the ListView covers StopAtBounds.

Final result: **48 Qt traces, 10,952 frame samples; maximum contentY error 0.000002217421 logical px**, below the asserted 0.00001 px tolerance. A second independent Qt capture reproduced all `(time, contentY)` samples byte for byte; `reference-position-hashes.json` records those position hashes. The five widget tests, 29 bar/island behavior/integration tests and four existing bar review regressions pass in the actual workspace (38 selected tests total). Library Clippy passes with warnings denied. Logs are linked from `results.json`. An earlier temporary dependency-build conflict was resolved by the other owner before these final actual-workspace runs; no dependency crate was edited here.

## Runtime API and scheduling

`Flickable::press / drag_to / release` accept monotonic times; `drag_to` returns whether the child click grab must be canceled. `set_extents` updates viewport/content geometry; `set_content_y` preserves the pointer anchor when changed during a drag. `frame(now)` samples the timeline, and `animating()` alone controls further presentation frames. There are no periodic timers or background tasks. Hide/unmap calls `reset` to clear motion/grabs. At rest, the controller requests no work. The integration regressions verify canceled drags emit no menu/session action and ordinary clicks still dispatch exactly once.

For native wheels, pass `Wheel { angle_delta, pixel_delta, phase, synthesized }`. A physical/no-phase wheel takes the angle path even if an absolute distance is also present. The compatibility `Wheel::deltas` adapter prioritizes discrete angles; pixel-only unphased input is treated as a touchpad update. It cannot reconstruct missing native begin/end/source information.

Bar exposes `Popups::menu_wheel`; island exposes `Island::wheel(x,y,Wheel,now)`, preserving all phase/pixel data. Existing Pointer/PointerEvent APIs remain source compatible. **Host handoff:** the shell's existing angle-only island mapping still drops pixel/phase data. Its owner must route native touchpad input to `Island::wheel` (and retain native phases for the bar's `menu_wheel`) to obtain the full tested touchpad path. No shell/engine files were edited outside this task's assigned component paths. Drag, flick and ordinary wheel integration are complete in both components.
