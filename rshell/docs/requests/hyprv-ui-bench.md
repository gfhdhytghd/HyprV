

### Integration CI — 2026-10-23T08:38:20+00:00 — tests: `main` function not found in crate `old_qt_rect`

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-ui-bench/examples/old_qt_rect.rs:73:2`.

`crates/hyprv-ui-bench/examples/old_qt_rect.rs`: mtime 2026-10-23T08:30:26+00:00; SHA-256 `5dd0c9ede6a94bd90e35b689ed56ae3da651d8eb7429795463b99d4bac675845`; changed during command: False.

```text
error[E0601]: `main` function not found in crate `old_qt_rect`
  --> crates/hyprv-ui-bench/examples/old_qt_rect.rs:73:2
   |
73 | }
   |  ^ consider adding a `main` function to `crates/hyprv-ui-bench/examples/old_qt_rect.rs`


```

Full log: `docs/verify/raw/ci/round-03/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:48:22+00:00 — tests: unresolved import `hyprv_ui::qt_rect::Cache`

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-ui-bench/examples/qt_rect_gpu_diff.rs:5:50`.

`crates/hyprv-ui-bench/examples/qt_rect_gpu_diff.rs`: mtime 2026-10-23T08:33:35+00:00; SHA-256 `98345ad3a281ae8f36fe7234e36803f08e41e15cb1d73066ffeddf8e57ed1864`; changed during command: False.

```text
error[E0432]: unresolved import `hyprv_ui::qt_rect::Cache`
 --> crates/hyprv-ui-bench/examples/qt_rect_gpu_diff.rs:5:50
  |
5 | use hyprv_ui::{animation::Color, geometry::Rect, qt_rect::Cache, scene::{Border, Fill, Rectangle}, skia as sk};
  |                                                  ^^^^^^^^^^^^^^ no `Cache` in `qt_rect`


```

Full log: `docs/verify/raw/ci/round-04/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


Round 4 cross-crate API attribution: `examples/qt_rect_gpu_diff.rs:5` imports `hyprv_ui::qt_rect::Cache`, but the current producer exposes `Mesh`, `Raster`, `tessellate`, `rasterize_antialias`, and the free `draw` function, with no cache type. The example uses cache-specific six-argument `draw` calls for GPU/CPU comparison; replacing those with the four-argument CPU fallback would change the benchmark meaning, so CI has not made that substitution.

Producer `crates/hyprv-ui/src/qt_rect.rs`: mtime 2026-10-23T08:31:10.988113+00:00, SHA-256 `e4f6eae4bbb2411ba0804b184115a763ca2604f263bd99d4a592c0d81e25281d`. See `docs/verify/raw/ci/round-04/tests.log`.

### Mechanical strict-Clippy cleanup — 2026-10-23

The user extended the UI lint assignment to remaining workspace diagnostics. Re-read current files before each edit; preserved the benchmark owner's concurrent example/main/dead-code fixes. No benchmark was installed or run as part of this cleanup.

- `src/gpu.rs`: replaced inferred `_` source types in the existing EGL-to-GL function-pointer transmutes with the already inferred `extern "system" fn()`. All destination function signatures/ABIs and calls are unchanged.
- `src/scenarios.rs`, `bar_common`: removed only the redundant outer `dead_code` attribute because the imported common module already has `#![allow(dead_code)]`; kept the existing unused-import handling. No new lint allowance was added here.
- `src/main.rs`, `Bench::run`, plus all five calls in `scenarios.rs`: grouped unchanged width/height arguments into a size tuple and destructured it on entry. Measurement, frame count, target allocation, sample order, and output are unchanged.
- `src/scenarios.rs`, `panel_case`, plus its four calls: grouped unchanged panel kind/model arguments into a tuple and destructured it before the original body. No scenario logic changed.

Full command now passes: `cargo clippy --workspace --all-targets --locked -- -D warnings`. Raw receipt: `docs/reviews/scratch/clippy-mechanical/workspace-clippy-final-3.{jsonl,stderr}`. UI before/after parity evidence is in the same directory; it uses an isolated source/input snapshot to separate the mechanical patch from concurrent renderer work. No changes were made to the benchmark's performance acceptance criteria.

Mechanical-cleanup final verification: workspace all-targets strict Clippy passes; 67 focused existing tests pass. The isolated canonical verifier produced 1,236 before/after PNG pairs with identical PNG and RGBA bytes, and unchanged Qt-reference gate metrics. Existing reference failures remain; absent island/panels GPU hooks were not counted. [Receipt and scope](../reviews/scratch/clippy-mechanical/README.md).
