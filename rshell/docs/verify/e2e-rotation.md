## Engine regression: output rotation (hyprv-ui)

Test: `crates/hyprv-ui/tests/nested_rotation.rs` (opt-in). Run it with `crates/hyprv-ui/tests/run-nested-rotation.sh [report]`, which uses a bwrap sandbox, a clean environment, private D-Bus, a virtual KWin and a nested Hyprland 0.56.2 with a 1920×1080 output.

Mapped during every step: a top bar (anchored left+right, exclusive 32), a full-output overlay, and two popup overlays (300×200 centred; 360×480 top-right). After each transform the test forces a full repaint, then asserts:
- no Wayland protocol error;
- the engine's configured sizes equal the rotated logical output, with width and height swapped for 90/270;
- `output_logical_size()` equals the rotated logical output;
- the compositor's `hyprctl -j layers` sizes are identical;
- every surface carries the output's scale.

Regression for docs/requests/hyprv-ui.md, 2026-10-23 (`wp_viewport` "Box doesn't fit"). With the fix disabled, the test fails at scale 1, transform 90.

27 / 27 steps pass. Overall: **PASS**.

| Scale | Transform | Logical output | Result |
| --- | --- | --- | --- |
| 1 | 0 (normal) | 1920x1080 | PASS |
| 1 | 1 (90) | 1080x1920 | PASS |
| 1 | 2 (180) | 1920x1080 | PASS |
| 1 | 3 (270) | 1080x1920 | PASS |
| 1 | 4 (flipped) | 1920x1080 | PASS |
| 1 | 5 (flipped-90) | 1080x1920 | PASS |
| 1 | 6 (flipped-180) | 1920x1080 | PASS |
| 1 | 7 (flipped-270) | 1080x1920 | PASS |
| 1 | 0 (normal) | 1920x1080 | PASS |
| 1.5 | 0 (normal) | 1280x720 | PASS |
| 1.5 | 1 (90) | 720x1280 | PASS |
| 1.5 | 2 (180) | 1280x720 | PASS |
| 1.5 | 3 (270) | 720x1280 | PASS |
| 1.5 | 4 (flipped) | 1280x720 | PASS |
| 1.5 | 5 (flipped-90) | 720x1280 | PASS |
| 1.5 | 6 (flipped-180) | 1280x720 | PASS |
| 1.5 | 7 (flipped-270) | 720x1280 | PASS |
| 1.5 | 0 (normal) | 1280x720 | PASS |
| 2 | 0 (normal) | 960x540 | PASS |
| 2 | 1 (90) | 540x960 | PASS |
| 2 | 2 (180) | 960x540 | PASS |
| 2 | 3 (270) | 540x960 | PASS |
| 2 | 4 (flipped) | 960x540 | PASS |
| 2 | 5 (flipped-90) | 540x960 | PASS |
| 2 | 6 (flipped-180) | 960x540 | PASS |
| 2 | 7 (flipped-270) | 540x960 | PASS |
| 2 | 0 (normal) | 960x540 | PASS |
