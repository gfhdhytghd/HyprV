# QuickAdjust scaled-clip regression

Frozen crops from the original-QML references `parity/reference/panels/qa-show-hide--show-t0120--{dark,light}@2.png`, at crop coordinates `(800, 0, 32, 130)`. These contain only the left curved card/fill/border edges; icon text is outside them. References were not regenerated or changed.

The engine GPU test retains the original 6144×3456 viewport and DPR 2, and reconstructs the rounded slider with the snapped clip origin and opposite child displacement. A small render target changes the projection rounding and does not reproduce this defect. Both crops must match premultiplied RGBA exactly.

Run with `CARGO_INCREMENTAL=0 bash crates/hyprv-ui/tests/run-renderer-regressions.sh`.

Original reference SHA-256:

- `qa-show-hide--show-t0120--dark@2.png`: `d03d611a971adc47f66dffa1680a6182485ebcc236bc79b3cf6ad05483f6aed7`
- `qa-show-hide--show-t0120--light@2.png`: `0efa3547084bec457f4d276a72f50555c04ae7df74d184c9fadd29572dc251f0`
