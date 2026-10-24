# Island exactness tools

All tools run finite captures on a private headless compositor. They never
connect to the user's compositor, install anything, or change canonical references.
Use the workspace `target` with `CARGO_INCREMENTAL=0`; no private Cargo targets.

- `python3 crates/hyprv-ui-island/tools/exactness.py final --binary target/debug/deps/parity-<hash>` replays
  all380 frozen island crops with the canonical verifier, plus integer-PM error.
  It pins the executable inode, records its SHA and source changes, and keeps
  eight diagnostic triptychs. Run from the workspace root with the actual binary path.
- `progress.py artifacts/final/metrics.json` reports fixed initial QA cohorts;
  a crop does not move cohorts when its largest residual changes category.
- `qt-observer` is an owned copy of the Qt parity host with property/GL diagnostics.
  Build with `cmake -S tools/qt-observer -B artifacts/qt-observer-build` and
  `cmake --build artifacts/qt-observer-build -j2` from this crate. Qt6.12 private
  development headers are needed only for these offline tools.
- `observe.py <fixture> [scale]` renders original QML without editing it.
  `HYPRV_RADIUS_TRACE=1` adds finite animation/hover-timer traces;
  `QT_QUICK_FRAME_SYNCHRONOUS_HOVER_INTERVAL=0` is an explicit diagnostic intervention.
- `prove_hover.py` moves Qt's actual pointer onto Previous in an owned fixture
  copy; it proves the frozen dark@2 playing capture's stale hover state. It does
  not change Rust behavior or waive that reference's failure.
- `capture_mask.py` / `QT_PROBE=clip capture_mask.py` generate independent engine
  GPU regression goldens from the adjacent QML probes.
- `spectrum_outline.cpp` uses installed QPainter and Qt6.12 QOutlineMapper/QBezier
  sources to print the complete26.6 outline. `qt-spectrum-outline.json` in
  hyprv-ui/tests/fixtures is its captured independent numeric oracle.
- `check_canvas.py` compares six fixed-phase Canvas configurations at both DPRs
  and themes. `check_scale.py` is a broader transform diagnostic; its half-scale
  and some scaled-clock cells still fail and are not accepted goldens.
- `check_panels.py` runs all300 current GPU panel/calendar crops and compares
  their gates to the retained pre-work control. Source changes are recorded.
- `check_bar_current.py` compares QA's current adapter/references. `check_bar_frozen.py`
  builds a small private adapter against the shared target to retain the old
  letter-fallback protocol and the production full viewport for the frozen bar control. It never
  rewrites QA's live adapter; these controls must not be mixed.

Capture PNG transport uses Qt's SIMD unpremultiplication semantics in tests only.
The exhaustive32896-pair QImage oracle is documented in tests/fixtures/README.md.
The Rust runtime remains Qt-free.

Accepted follow-up replay commands (shared target; no private Cargo target):

```sh
CARGO_INCREMENTAL=0 cargo test -p hyprv-ui-island --test parity --no-run
python3 crates/hyprv-ui-island/tools/exactness.py accepted-final --binary target/debug/deps/parity-3611aa98e55fa3a0
CARGO_INCREMENTAL=0 cargo build -p hyprv-ui-bar --example reference
python3 crates/hyprv-ui-island/tools/check_bar_current.py --name accepted-bar
python3 crates/hyprv-ui-island/tools/check_panels.py
HYPRV_PARITY_BACKEND=gl python3 crates/hyprv-ui-bar/tools/stress_gl.py accepted-ui-goldens --tests
```

The parity binary suffix is Cargo-dependent; take the actual path from `--no-run`.
`qt-observer/rect_mesh_precise.cpp` supplies the independent fractional QRectF
mesh oracle. `batch_probe.qml` supplies the scaled-sibling GPU goldens. The
approved parked-hover fixture patch is retained beside these probes; canonical
publication was performed only after user authorization using `parity/capture.sh`.
Obsolete raw GL blobs, observer CMake build and intermediate PNG corpora were
removed after summarizing them; `artifacts/followup-cleanup.json` retains hashes.
