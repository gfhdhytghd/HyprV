# Corner topology and scaled-clip precision — 2026-10-23

The host retains four tiny corner surfaces to avoid full-output render/swap targets.
The accepted memory result was approximately 267 MiB GPU saved at 6144×3456@2.
The root README documents the shared namespace, empty input region, overlay layer,
exclusive zone and compositor-rule implications. Removed its obsolete claim that
corner namespaces have a direction suffix.

Native cropped-versus-full-output composition tests are exact, including fractional
scales and reveal travel (see [memory verification](engine-memory.md)). That proves
surface cropping adds no pixel differences; it does not prove legacy Qt equality.

The new `crates/hyprv-shell/tools/verify_corners.py RUN` compares all four rectangular
corner crops, without masks or text tolerance, for every assembled capture/output/
scale. Top crops include the entire 15 logical pixel reveal travel. The full-screen
summary tool writes `corner-comparison.json` and includes these independent results
under `summary.corners`. Two checker tests verify that a one-level alpha difference
at any corner fails and missing captures cannot pass.

Current accepted full-screen captures: **64 captures / 256 corners, max 4 alpha
levels, 0 exact corners** against legacy Qt. This pre-existing curve rasterization
difference is present in the full-output native scene too. Routed to svc-power in
[bar requests](../requests/hyprv-ui-bar.md), with concrete pixels and the suspected
Canvas-arc versus QPainterPath-arc bake distinction. Strict legacy corner equality
remains open; the full-image distance-field tolerance must not hide it.

QuickAdjust `qa-show-hide--show-t0120--{dark,light}@2.png`: fixed AA coordinate
precision beneath scaled clips. A device-snapped clip origin and its opposite
child displacement previously rounded twice when folded into float mesh vertices.
Keeping the full transform in the shader preserves the original curve coordinates;
ordinary DPR-only clip batches keep their merged path. No panels source or canonical
references changed.

| Crop | Before | After |
|---|---:|---:|
| dark @2 | max 1 / 2 differing pixels | max 0 / 0 pixels |
| light @2 | max 1 / 8 differing pixels | max 0 / 0 pixels |

Frozen 32×130 Qt left-edge crops live in `hyprv-ui/tests/quick-adjust/`, with source
reference SHA-256 values. The GPU regression retains the full 6144×3456 projection.
Disabling this fix reproduces the dark corner failure (actual `[10,11,14,15]`, Qt
`[10,11,13,15]`); restoring it passes both themes exactly.

Validation: six batch unit tests, 59 UI library tests (14 explicit GPU/oracle tests
ignored by the ordinary run), 11 surfaceless NVIDIA GPU regressions, three private
Weston EGL regressions, two corner checker tests, and UI all-target clippy with
`-D warnings` pass. Clippy also caught a newly added lifecycle example's adjacent
independent `if` formatting; inserted a newline, with no behavior change.

Fresh GL controls: 248 panels, 52 calendar, 380 island exports. Against the pre-task
QA exports, all **678 other images are byte-identical**, and only the two requested
QuickAdjust crops change. Existing Mihomo/startup small deltas are byte-identical
to QA's prior exports; no acceptance thresholds were changed. The first bar attempts hit svc-power's ongoing edits (missing `async_trait` imports); after its owner restored the imports, the GL replay passed **304/304 exact** (bar-owned gate receipt). The QA comparator also passes **300/300 panels** and **380/380 island**. The panels' max-3 Mihomo/startup deltas already existed in QA's prior exports and remain within their declared distance-field tolerance.

Receipts: `target/qa-precision/controls-vs-qa.json`, `component-results.json`,
`fullscreen-corners.json`, `regression-before.log`, `regression.log`,
`all-gpu-tests.log`, `clippy-final.log`, and `final-gl/qa-comparator-results.json`
plus the bar-owned `final-gl/bar-report/gate/gate.json`. Temporary Qt observer builds, source copies
and raw GL buffer/texture dumps were removed after retaining the useful observation
and hashes. The live session was not touched.
