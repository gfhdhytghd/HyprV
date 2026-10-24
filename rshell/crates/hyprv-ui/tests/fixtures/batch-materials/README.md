Independent Qt 6.12 OpenGL RHI goldens. Generated from `crates/hyprv-ui-bar/tools/batch_material.qml` using `tools/check_rect_effect.py`, private headless Weston, basic render loop, DPR 1/2, full/faded image opacity. Assets are QtSvg control-panel-dark at32px and Qt-decoded cover at16px. Scene3072x1728 includes an off-center324x190 effect at2516,58, explicit source layer size and shadow padding; this exercises QSG quad diagonal/UV rounding as well as native batching, clipping and material opacity. Runtime remains Qt-free. Capture receipt: `crates/hyprv-ui-bar/artifacts/exact2-batch-output/metrics.json`. Expected PNG SHA256:

- `control.png`: `8b3d42ad847a04028d1ed3c546c8b1df81c4acfa06ac01dfbcf0a68a7fbbb01c`
- `cover.png`: `18d63b9403dad579b5787e98a489b2c78aeda0af9b2e6ec25d34faf7911efda4`
- `effect-false--dark@1.png`: `73cfa7abecea4f22cd5bff4118b9e2cd9b0536b7abf883a498641418a75decf6`
- `effect-false--dark@2.png`: `867d820bb4980eb8ab8baa01bc207a65e38afde61b9eedd56b95a4a32fdf0117`
- `effect-true--dark@1.png`: `c473383337658065893ce4dfd38e1100a873f4e9a9866f7c74a29a166b0a53a9`
- `effect-true--dark@2.png`: `d00fc004c2d8270545ade30da9a9064a39de7ff919296a61cfdbe8d3742c9f01`

Causal regression: temporary diagnostic binary with the previous opposite effect diagonal fails both DPR2 full/faded cells at max1; corrected order makes all4 cells exact. Negative receipt: `crates/hyprv-ui-bar/artifacts/exact2-batch-output-negative/metrics.json`. Source was restored before final GPU tests.
