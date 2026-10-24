# Task: engine (crate hyprv-ui)
Read ../ARCHITECTURE.md first. You own /home/wilf/.config/HyprV/rshell/crates/hyprv-ui only (plus examples inside it). Workspace Cargo.toml already globs crates/*.
Build a high-performance retained-mode UI engine for Wayland layer-shell:
1. sctk layer-shell surfaces (multi-output, anchors/margins/exclusive zone/keyboard interactivity/namespace, popup-like layer surfaces), fractional-scale-v1 + viewporter, frame callbacks, input (pointer enter/leave/motion/button/axis incl. smooth+discrete scroll, keyboard), cursor shape.
2. skia-safe (gl, textlayout) on EGL (wayland-egl), shared context, premultiplied alpha; damage tracking (only repaint dirty regions, set buffer damage), idle = zero wakeups, no redraw when nothing changes.
3. Retained scene graph nodes: Rect (color/gradient, per-corner radius, border, clip), Text (skia paragraph, elide, letter spacing, weight), Image/SVG, Path/Shape, drop shadow & blur effect (equivalent to Qt MultiEffect shadow/blur params), opacity/transform layers, clip.
4. Reactive properties (signals/bindings like QML) with cheap change propagation; layout helpers (row/column/anchors/margins) — keep it explicit and fast, no heavy flexbox needed unless useful.
5. Animation system that is **Qt-exact**: port QEasingCurve formulas (all types used by QML: Linear, In/Out/InOut Quad/Cubic/Quart/Quint/Sine/Expo/Circ/Back(overshoot)/Elastic/Bounce, BezierSpline), NumberAnimation/ColorAnimation (Qt interpolates colors in RGBA space) with Behavior semantics (retarget from current value), SpringAnimation (port Qt's spring/damping/mass/epsilon algorithm), driven by frame callbacks with presentation timestamps.
6. Offscreen render of any node tree to PNG at a given scale (CPU raster ok) for parity tests.
7. An example `examples/demo.rs` showing a translucent layer surface with an animated rounded rect + text, namespace `hyprv-quickshell`.
Write unit tests for easing curves against known Qt values. Document the API in crates/hyprv-ui/README.md. Prefer correctness + performance; measure idle CPU (should be 0) and frame time.
Do NOT run the demo in a way that grabs exclusive zone on the user's real outputs for long; a brief test run is fine, then exit.
When done with a milestone, reply with a short summary of the API and what is left.
