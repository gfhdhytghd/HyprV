# Assigned-panel diagnostics and replay

Cargo runs share rshell/target with CARGO_INCREMENTAL=0. These tools do not publish
canonical parity data. Private Weston instances never use the live desktop.

- core_gate.py NAME [--filter stats]: compile/pin the shared replay binary and run
  the canonical strict verifier for the184 assigned panel images. Source/reference
  hashes accompany each run. --no-render compares retained images only.
- core_observe.py STATE DPR: observe unmodified original QML with the owned island
  observer host. Requires a regenerable CMake build at
  artifacts/core-panels/qt-observer-build, built from island/tools/qt-observer.
- core_painter.cpp: independent installed-QPainter oracle consuming path JSON from
  the ignored core_stats_probe::stats_sources diagnostic. It established that the
  chart mismatch came from replay inputs, not the Rust painter port.
- core_gl_trace.cpp: offline Qt GL observation only. CORE_GL_VIEWPORT/CORE_GL_UNIT
  restrict observations. Never linked into the Rust shell.
- core_capture_layer.py: regenerate engine layer-projection GPU goldens at DPR1/2
  from core_layer_probe.qml with the canonical host privately. Oracle hashes are
  retained in artifacts/core-panels/layer-oracle/oracle.json.

The ordinary core_stats_probe test asserts the observed process-delivery clock.
Its separate, explicitly ignored finite diagnostic requires CORE_STATS_OUT;
CORE_STATS_STATE defaults to stats-120. CORE_STATS_RENDER additionally renders the
full3072x1728 scene on the selected offscreen backend. No runtime readback hook is
retained. Old raw GL captures and Qt build binaries were removed after summarizing
and hashing in artifacts/core-panels/causal-receipt.json.
