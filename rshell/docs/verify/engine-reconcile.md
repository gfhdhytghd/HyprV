# Engine consolidation — 2026-10-23

Engine owns hyprv-ui, hyprv-shell and widgets core. No live session, device,
provider write, install, commit or compositor reload was used. All runtime
checks below use their own private compositor, runtime and bus.

## Changes and preserved contracts

| Area | Result |
| --- | --- |
| ENG-PQ8 | One `hyprv_ui::offscreen` owns production roleless Wayland/EGL RGBA8 targets. `Renderer::{raster,image,png}` honors `HYPRV_PARITY_BACKEND=cpu\|gl` or an explicit Backend override. Invalid configuration and unavailable GL fail explicitly. Display changes and target recreation invalidate context-dependent caches. |
| Consumers | UI harnesses import the engine API; no source path includes of parity/gpu remain in crate Rust source. The former QA crate is a compatibility re-export. Custom F16 diagnostic and bar crop dimensions are preserved. |
| Qt geometry/materials | qt_rect remains the canonical rectangle mesh and CPU coverage implementation; qt_gl_rect transports it in a context-owned VBO/EBO. Native text, inherited material opt-in and transport switches remain independent. Existing layer texture padding, mip sampling, Canvas and image paths are preserved. |
| Cache/recovery | Preserve UI-R2 upload failure fallback, partial text batch recovery, complete mip accounting, Shared-DAG reachability, effect invalidation and coordinated native epochs. Renderer clear also releases bounds/reachability and UI-thread measurement caches. |
| Battery popup | Reuse bounded SkParagraph/font collection metrics; skip hidden battery measurement. Cache keys preserve line-breaking inputs. Native open/close takes approximately1.15s versus prior~55s, including reset and400ms close wait; this is not isolated paint timing. |
| Scheduled bar updates | A per-mount RetainedView skips construction and recursive reconciliation of unchanged modules. Changes to layout/theme/scale/assets/hover/notification dots and baked Wi-Fi icon keys invalidate correctly. Removing mounts drops updater state. Pure build/snapshot APIs remain available. |
| Widgets | Preserve Qt QIconLoader theme/dash/size lookup, Qt image downscaling, and controls/viz ownership. README text accuracy claims now match the engine fixtures. |
| Host integration | Preserve rotation viewport source, degraded system-bus retry/re-exec, exactly-one real-bus sleep topic, desktop CLI and standalone startup-confirm. Fix concurrent SVG test raw-string delimiter and fixture needless struct update. |

The new incremental regression builds zero modules/damages zero nodes on an
unchanged formatted stats callback. A real CPU-label change retains other modules
and damages less than half the bar. Nine transitions compare incremental and fresh
raster output: hover, theme, scale, width, calendar, network presence, connected
Wi-Fi, changed baked Wi-Fi signal icon and notification dot. Wrapped metrics match
independently constructed legacy paragraphs and have bounded/flushable cache tests.

The CPU audit found a newly introduced direct destination triangle blend dispatch
changed608/984 CPU images and made two panel assertions fail. Removing its
automatic dispatch and unused helper restores the established cached-raster CPU
composition and panel248/248. Native GL rendering is unaffected. The failed
intermediate receipts remain available; no gate, reference or mask was weakened.

## Validation

- Owned ordinary tests:160 passed,0 failed,21 explicitly ignored. Strict owned
  all-targets Clippy passes; bar library Clippy also passes.
- Production backend entry points: private EGL regression matches explicit
  GpuTarget at DPR1/1.5/2 and after target recreation. CPU override and invalid
  extents are covered independently.
- Private GPU goldens:84 Canvas stroke cells exact; two native-text integration
  tests pass, including64 independent color/opacity cells.
- Renderer/cache regressions:11 NVIDIA and11 Mesa checks pass, including private
  SharedEgl flush, upload failures, context changes and full mip budgeting.
- Rotation: private sandbox27 scale/transform steps pass.
- Compatibility facade builds with all targets; cross-GPU helper Python compiles.

The immutable before/after executables, source fingerprints, output PNGs and
comparison JSON are under `target/reconcile`. The CPU+GL comparison covers1968
images:1920 are byte-identical. All984 GL images are byte-identical;48 CPU images
differ. Canonical strict gate outcomes are unchanged (zero lost passes/new
structural failures), and all per-image means stay equal or improve. One already
failing CPU island expansion max rises58→59 while mean improves1.323→0.637.
This is preservation of existing parity outcomes, not a claim all reference gates
pass. The source receipt records a concurrent native_gl edit during the final CPU
build; no individual patch is assigned causal credit for the combined pixel delta.

Receipts: `target/reconcile/{before,after,final-cpu}/source.json`,
`target/reconcile/gate-comparison.json`. Initial and repaired CPU panel reports
remain in `after/cpu/panels` and `cpu-legacy-panels`. The checker uses unchanged
canonical comparison/masking and distance-field flags; identical images need no
second reference comparison.

## Remaining evidence boundaries

The bar owner continues independent native material/shader parity experiments;
source fingerprints identify exactly which combined builds are covered here.
Finite debug_layer hooks are preserved while that owner's diagnostic is active.
Their later experiments need their own final receipts. The measured release idle
budget in perf-shell.md is not retroactively changed by the incremental regression.
Expected activity-memory findings have not arrived in the host request log;
existing SOAK-1 records cache/driver retention without proving a host leak.
No memory-growth fix is claimed.

A later pinned GL checkpoint includes the bar owner's image-vertex correction:
974/984 images are byte-identical to baseline, and ten menu images improve their
means with unchanged maxima and unchanged strict gate outcomes. Island380/380
and panels/calendar300/300 remain byte-identical. Receipt:
`target/reconcile/final-gl-gate-comparison.json`; build fingerprints and immutable
executables: `target/reconcile/final-gl`. This later checkpoint supersedes the
GL identity count above for that specific combined source.

Native functional testing initially exposed a test setup dependency: a newly
created nested Wayland output did not acquire a mode until the subsequent monitor
rule. The full failed receipts are preserved under functional-reconciled and
functional-final. The repaired test registers its mode with private native IPC
before hotplug, pins both initial monitor positions, verifies host/native origins,
and waits for observed host output registration; actual virtual
pointer/keyboard input and exact action assertions are unchanged. Isolated output,
quit and compositor restart checks pass9/9. The complete final matrix is published
in [e2e.md](e2e.md).

Core reproduction commands (workspace root):

```sh
cargo test -p hyprv-ui -p hyprv-shell -p hyprv-widgets
cargo clippy -p hyprv-ui -p hyprv-shell -p hyprv-widgets --all-targets -- -D warnings
cargo test -p hyprv-shell scheduled_bar_updates
crates/hyprv-ui/tests/run-renderer-regressions.sh
__EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json LIBGL_ALWAYS_SOFTWARE=1 crates/hyprv-ui/tests/run-renderer-regressions.sh
crates/hyprv-ui/tests/run-nested-rotation.sh target/reconcile/rotation-repeat.txt
cargo run -p hyprv-shell -- --nested-functional-test target/functional-repeat
```

Pinned rendering/comparison runners are retained under `target/reconcile/tools`;
check logs and aggregate counts are under `target/reconcile/checks`. The raw
runners are diagnostic tools, not an additional runtime rendering implementation.

Final single-run native matrix: **204/204 PASS**, including the calibrated two-output
hotplug/input/reorder/remove,1→1.5→2 scaling, IPC quit and compositor-loss/fresh-reconnect
checks. Raw immutable-binary receipt:
`target/functional-consolidated-verified/manifest.json`.
The final battery.open-close duration is1,186ms. The matrix and per-action evidence
are regenerated in [e2e.md](e2e.md). Earlier failed setup runs are retained.

## Disk-policy retention update

After verification, engine removed stale `target/reconcile` and44 total stale
reconcile/functional directories (12.52GiB logical files). Large raw captures,
frozen binaries and diagnostic tools referenced above are retired; small
JSON/CSV/log/Markdown receipts were archived under
`docs/verify/raw/engine-cleanup/`, preserving their relative paths.
`cleanup.json` records the removed paths and retained receipts. Current-task
verification is in [perf-followup.md](perf-followup.md).
