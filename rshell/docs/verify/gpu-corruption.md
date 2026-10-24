# GPU frame-corruption risk closure — 2026-10-23

The tracked risk is closed with two separate causal findings. The two historical
bar outliers are reproduced by the already-fixed fixture icon-directory race. The
new lifecycle search additionally exposed and fixed a direct `EglWindow::draw` resize
bug: a correct retained image could be copied into an old-size EGL back buffer.
No further lifecycle corruption occurred in the final private-session search.

## Reproduction and fix

The original `cpu-power-medium--light@1` and `fullscreen-slide--hide-t0000--light@2`
captures are not evidence of a missing GL fence. Absent and empty icon-directory
interventions match their entire tray strips at max0 and explain all6,209 pixels
that differed by more than8 from their controls. Remaining full-image differences
are at most1. [Fresh pixel/hash recheck](../../crates/hyprv-ui-bar/artifacts/gpu-risk-asset-recheck.json).
The existing report-local, immutable asset snapshots prevent this race.

The extra Mesa llvmpipe run exposed a separate real bug. The retained RGBA8 image
matched CPU exactly, but the actual default framebuffer was transparent or clipped.
Its queried EGL/default framebuffer format wasRGBA8, with no multisampling; this was
not PNG unpremultiplication or a colour-depth discrepancy. Both the native blit and
forced Skia final-copy paths reproduced the failure. Merely rebinding a pbuffer
fixed initial acquisition but failed after82 frames on a mixed-axis640x64→400x300
resize. [Old-size buffer proof](../../crates/hyprv-ui-bar/artifacts/gpu-risk-mesa-dump/400x300-614b86309c5ea4e5-source.png),
[actual transparent back buffer](../../crates/hyprv-ui-bar/artifacts/gpu-risk-mesa-dump/400x300-614b86309c5ea4e5-window.png),
[format probe](../../crates/hyprv-ui-bar/artifacts/gpu-risk-mesa-format/probe-full.log).

`EglWindow::draw` now unbinds/destroys/recreates its EGL drawable on physical-size
changes, then invalidates cached Skia state and allocates a fully damaged retained
target. Its optional handle makes failure teardown exception-safe. Shared GL
context, native glyphs and shader arithmetic are unchanged; no glFinish/fence was
added. The actual host already retires whole windows on configure/rotation, so it
already guarded this condition; the direct API now provides the same guarantee.
[Implementation](../../crates/hyprv-ui/src/egl.rs),
[seven-transition regression](../../crates/hyprv-ui/src/egl/frame_audit.rs).

## Final post-fix search

All eight rows below use the same pinned test ELF SHA256
`0f30933fc877f282c64394a593ac17d06a298f44d2f6a79af80d04602540f742`.
Every run has zero watched-source drift. Primary runs read each submitted retained
buffer, compare deterministic CPU pixels and require its pre-swap EGL back buffer
to match byte-for-byte. The rich rect/native-text/Skia-picture profile retains a
fixed max1 CPU raster difference; every stressed pixel must also equal a fresh GPU
control exactly. The unread-burst row intentionally avoids synchronous per-frame
inspection, then checks all outstanding targets periodically. It is auxiliary timing
evidence, not a claim that its unread frames were captured.

| Private run | Complete submissions | CPU comparisons | CPU max Δ | Lifecycle mismatches |
|---|---:|---:|---:|---:|
| NVIDIA Weston,60/144Hz | 4,116 | 4,116 | 0 | 0 |
| NVIDIA Hyprland,hotplug/scale/rotation | 4,116 | 4,116 | 0 | 0 |
| NVIDIA rich GL/Skia interleaving | 2,744 | 2,744 | 1 | 0 |
| NVIDIA bursts,delayed readback | 4,116 | 116 | 0 | 0 |
| NVIDIA compositor restart,fresh epoch | 1,372 | 1,372 | 0 | 0 |
| Mesa llvmpipe,60/144Hz | 4,116 | 4,116 | 0 | 0 |
| NVIDIA forced Skia window copy | 686 | 686 | 0 | 0 |
| Mesa forced Skia window copy | 686 | 686 | 0 | 0 |

Complete epochs total **21,952 submissions** and **17,952 CPU comparisons**.
The deliberately interrupted epoch preserves458 additional verified CPU-exact
buffers before private compositor termination. Across complete and interrupted
epochs,18,406 pre-swap audits match source/default-buffer checksums and bytes; this
includes112 fresh rich-profile controls. All18,410 recorded stress readbacks pass
their declared CPU/control conditions:15,666 are CPU-exact;2,744 rich samples have
the same fixed max1 CPU delta and are GPU-control exact.
[Validated aggregate](../../crates/hyprv-ui-bar/artifacts/gpu-risk-final-summary.json),
[receipt validator](../../crates/hyprv-ui-bar/tools/summarize_lifecycle.py).

| Requested condition | Mechanism and evidence |
|---|---|
| Output hotplug,scale,rotation during changing scenes | Private Hyprland native IPC; two real800x600 Wayland outputs; scales1/1.25/2 and transforms0/1/3 observed. Buffer cases additionally use1/1.25/1.5/2 and all four rotations. |
| Popup and surface/EGL lifetime storms | 2,752 popup create/destroy cycles, 1,752 explicit full surface lifetimes, plus drawable recreation on physical-size changes. |
| Context loss/robustness | 192 sticky typed EGL_CONTEXT_LOST injections, owner/window teardown and fresh context recovery. Real robust contexts report access1 and reset strategy0x8252. No device reset. |
| Compositor restart | Terminate only the Popen-owned private compositor mid-render; client exits1 without panic, then a fresh epoch reconnects and validates pixels. Actual host disconnect policy is clean exit/supervisor restart. |
| VT-like callback pause | 192 client dispatch pauses; private compositor SIGSTOP/SIGCONT intervals; an additional2,000ms pause on the final NVIDIA rate run resumes cleanly. |
| Concurrent outputs with different rates | Private Weston module drives/advertises60,000 and144,000mHz concurrently; both wl_output mode events are asserted. Four surfaces use different submission schedules. |
| Memory pressure and closed-target purge | 525 bounded32MiB live scratch allocations under a2MiB cache budget; 1,133 coordinated cache purges. Odd epochs also purge unused/native resources after paint and before copy. Window retirement executes engine scratch purges while other targets stay live. |
| Direct GL/Skia state and synchronization | 1,485 legal external-state poison/invalidate cycles; rich native rectangles/text alternate with Skia pictures; native blit and forced Skia final copies both pass on both drivers. |
| Partial damage and buffer reuse | 5,540 partial-damage frames; stable-size phases reuse EGL buffers rather than always allocating. Full CPU oracles verify retained pixels outside damage too. Protocol traces contain real wl_buffer releases; no reattachment before an observed release was found among surviving proxies. |

The transport receipt records client-proxy destruction while a server may still
hold the buffer separately; that destruction is legal and is not treated as an
ordering failure. Physical scanout, a real KMS VT switch, real hardware reset and
machine-wide OOM were not induced. Early unaligned synthetic scenes had static
CPU/GPU raster discrepancies and are retained as prototype diagnostics; they are
not counted as passing final CPU-exact coverage. Headless callback/drawable tests cannot prove
all driver/hardware paths universally safe. The earlier private Weston helper
prototype assertion and invalid fullscreen geometry were test-helper errors and
were corrected before final measurement; their unsuccessful receipts are retained.

[Long-pause receipt](../../crates/hyprv-ui-bar/artifacts/gpu-risk-final-long-pause.json).
[EGL robust-context specification](https://registry.khronos.org/EGL/extensions/EXT/EGL_EXT_create_context_robustness.txt),
[GL reset-notification semantics](https://registry.khronos.org/OpenGL/extensions/KHR/KHR_robustness.txt).

## Runtime detection and regression controls

Debug builds opt in with `HYPRV_GPU_SELF_CHECK=1`. Every frame reads retained
premultiplied RGBA8 and the actual EGL back buffer before swap, normalizes origin,
checks hashes **and bytes**, and returns a graphics error with hashes/first byte if
they differ. Advertised robustness reset status is queried. The application already
exits cleanly on graphics errors. `HYPRV_GPU_SELF_CHECK_LOG=1` emits checksum receipts.
Normal operation has no added readback/timer/file write; release builds contain
none of the audit or private fault-injection code. Readback changes timing, hence
the separate unfenced burst run.
[Public diagnostic contract](../../crates/hyprv-ui/README.md).

A real one-pixel back-buffer mutation is detected; the next clean frame recovers.
The same-window regression checks the first buffer at seven grow/shrink/fractional
transitions against CPU and retained pixels. Both regressions and the three closed-
cache/lifetime tests pass on **NVIDIA and Mesa**.
[Cross-driver tests](../../crates/hyprv-ui-bar/artifacts/gpu-risk-final-tests/receipt.json).

Other checks:59 ordinary UI library tests pass;11 surfaceless GPU regressions pass;
all-target scoped Clippy with-D warnings and release all-target checking pass.
Actual-shell private output/navigation E2E:13/13,192 pre-swap audits exact.
[Host E2E](../../crates/hyprv-shell/artifacts/gpu-risk-functional/e2e.md).

Fresh component GPU controls have zero watched-source/reference drift:

| Component | Gate | Exact | Worst Δ |
|---|---:|---:|---:|
| Bar |304/304|304|0|
| Island |380/380|380|0|
| Panels |300/300|244|3|

Panel residuals are existing declared distance-field images; the engine-owned two
QuickAdjust precision crops are now exact.
[Component receipt](../../crates/hyprv-shell/artifacts/gpu-risk-final-controls/receipt.json).

Full-screen post-fix control: **64/64 pass** under native and independent QA
comparators, max6;26/26 jobs and924 additional exact actual-back-buffer audits.
[Independent full-screen rows](../../crates/hyprv-shell/artifacts/gpu-risk-fullscreen/compare/compare.json),
[rendering/input guard](../../crates/hyprv-shell/artifacts/gpu-risk-fullscreen-source-guard.json).
Only the coowned service agent-island implementation changed during this pinned
run; host/UI/canonical inputs stayed unchanged. Fixtures use the mocked Hub.
The first unpinned parent matrix stopped after three successful jobs with ENOENT
before its fourth spawn; this was a launcher pathname issue, not a render failure.

## Reproduce

From rshell root, using the shared Cargo target and a fresh artifact name:

```sh
CARGO_INCREMENTAL=0 cargo build -p hyprv-ui --example lifecycle_stress
python3 crates/hyprv-ui-bar/tools/stress_lifecycle.py recheck-nvidia --backend hypr --output-kind wayland --epochs 2 --frames 1800
__EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json LIBGL_ALWAYS_SOFTWARE=1 python3 crates/hyprv-ui-bar/tools/stress_lifecycle.py recheck-mesa --dual-rates --epochs 2 --frames 1800
CARGO_INCREMENTAL=0 bash crates/hyprv-ui/tests/run-renderer-regressions.sh
```

`--rich` adds native text/rounded rectangles/Skia pictures; `--no-audit --no-readback`
selects the auxiliary burst mode; `--restart-live` deliberately interrupts epoch0.
`HYPRV_SKIA_WINDOW_COPY=1` selects the final-copy control. Robust/mid-frame-purge
mode is enabled on odd epochs. The launcher owns private HOME/XDG directories, bus,
compositor PIDs/sockets and pinned ELF; it removes temporary binaries/plugins at exit.
Only checksums/receipts and useful mismatch PNGs are retained. No live shell, output,
device, compositor or user service was changed.
