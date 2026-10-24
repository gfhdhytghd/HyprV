# Bounded stats presentation experiment — 2026-10-23

Started21:26UTC; hard deadline22:56UTC (90 minutes). No live session input,
compositor restart or provider writes. All clients/captures run in private nested
compositors; the real session supplies read-only environment and monitor data.
The real output is6144×3456@2, HDR/XBGR2101010. Hybrid presentation is restricted
to an exactly8/8/8/8 EGL configuration; output HDR mode alone does not select it.

## Approach

The bar keeps the same Wayland layer surface, namespace, viewport and input
region. Stats-only updates render through the original retained GL RGBA8 target.
Damaged rectangles queue into one bounded PBO; an exported native-fence FD blocks
in poll rather than a driver busy wait. Only completed bytes are copied into a
reusable wl_shm ARGB8888 buffer and committed. GPU glyphs and materials stay on
GL; there is no CPU text/rectangle approximation, subsurface or additional timer.

At most8MiB PBO storage is retained globally (4MiB for this bar), plus one SHM
surface buffer per eligible bar (2.72MiB at6144×116). A5ms fence timeout returns to
EGL before mapping an unfinished PBO. Missing capabilities, animation/feedback
requests, busy buffers and allocation/readback failures retain EGL. EGL age
history is cleared on return from SHM. Full UI reconciliation clears the hint.
`HYPRV_SHM_PRESENT=0` is the diagnostic EGL control.

CPU painting was rejected: all120 tested stats module crops differed from GL by
one channel level. Synchronous GL→SHM reads were also rejected:0.1721% normal
CPU despite removing swap. The first asynchronous PBO prototype reached0.09148%
(normal90s), compared with0.11531% for the same ELF with EGL. Its GPU+SHM
composition was exact, but repeated default-enabled RGBA results were too close
to the budget:0.10296%,0.09757%,0.10262% (mean0.10105%). The final bounded variation
requests native BGRA readback where GL advertises it, eliminating CPU swizzling;
other implementations retain the exact tested RGBA conversion.

## Final decision and measurements

**REVERTED at22:50UTC**, within the90-minute limit. The first uncontended final
90-second window measured **0.106831%** whole-shell CPU, above0.1%. No other test
GPU clients were detected during that window (`uncontended-witness.json`). The
second window was stopped cleanly rather than extending a rejected experiment.
No SHM binding, fence/PBO allocation, stats presentation hint, test RPC or native
BGRA presentation path remains in production. The earlier incremental bar,
damage, text/glyph-cache and EGL buffer-age improvements remain.

The preceding BGRA repeats were0.093998%,0.082888%,0.080877%; they overlapped the
bar owner's actual-compositor corner matrix and are **CONTENDED**, not quiet
acceptance evidence. The corresponding EGL control was0.125576% and is also
contended. The initially promising0.09148% prototype was therefore not sufficient
to retain the change. Normal measurements omit all profiling flags.

The BGRA instrumented run (also contended) measured31 stats frames through SHM,
31 completed native fences,31 native BGRA transfers, no stats swap, and no
fallback. Mean tick CPU0.686839ms; maximum tick wall6.185387ms. Mean stage CPU:
model/update0.208356ms, retained GL render0.200154ms/frame, PBO transfer/wait/copy
0.260643ms (including0.047649ms mapping/copy and0.033345ms fence CPU). The fence
wait's mean wall time was0.595071ms. Inclusive scopes must not be summed blindly.
This is a measured reduction of busy waiting, not achievement of the idle budget.

Raw authoritative receipts are under
`docs/verify/raw/perf-shell/{shm-uncontended-1,shm-bgra-default-1,shm-bgra-default-2,shm-bgra-default-3,shm-bgra-control,shm-bgra-profile}/`.
`target/shm-tick/numbers.json` summarizes scheduler CPU; `uncontended-witness.json`
records the interference check. The failed final ELF SHA256 was
`8a028532ece3ca96fca2b3b75a1536e9a821d92b3031080774cf68b008dd70d8`.
Executable/source hashes identify measurements despite parallel workspace edits.

The generic render-versus-present split in EglWindow remains because the parallel
FR-NET-3 full-redraw/capture implementation now uses it. It creates no SHM/PBO
path and ordinary retained surfaces continue to swap through EGL. The parallel
Canvas-atlas/full-redraw changes were preserved. Three exact fixture coordinates
in the new parallel renderer test now use explicit f64→f32 casts to satisfy
clippy without changing their f32 values.

## Pixel and functional evidence

- All984 original GL PNGs remain byte-identical:248 panels +52 calendar +380
  island +304 bar. Eight additional bar host states also pass (312/312 total).
  `target/shm-tick/parity-final/byte-comparison.json` and
  `bar-report/gate/gate.json`. The transient bar build failure came from a
  parallel, unfinished ImageSampling enum edit; the completed rerun passed.
- Default RGBA8-gated candidate:24/24 real full-screen GL↔SHM comparisons exact,
  light/dark, DPR1/1.5/2, including consecutive partial updates, changing widths
  and severity. Every measured case used SHM; no fallback masks a comparison.
  `target/shm-tick/final-composition/results.json`.
- Fresh native-input E2E:204/204 passing, complete manifest at
  `target/shm-tick/e2e-final/manifest.json` (pre-format optimization candidate).
- Renderer regression script:14 GPU regressions,2 private partial/PBO tests,
  3 cache-lifetime tests and2 back-buffer audits pass. New PBO test compares raw
  bytes with GL snapshots, including BGRA ordering, vertical orientation,
  strided padding and forced timeout→GL recovery. Hardware without native fences
  explicitly verifies GL fallback instead.

The now-retired GPU+SHM screen comparison was invoked as:

```sh
python3 crates/hyprv-shell/tools/verify_shm.py \
  --binary target/release/hyprv-shell --output target/nested-shm
```

The prototype command is no longer installed in production sources. Its small
read-only source receipts are archived as `.txt` in
`docs/verify/scratch/shm-tick/`; source hashes are in `source-receipt.json`.
The experimental GPU lane required the same private-compositor tools/environment as
`docs/verify/measure-shell.py`, plus grim, Pillow and numpy. It pins the executable,
uses mocked services, drives fixture state through the production stats path,
and compares actual compositor screencopies. The compositor's animated startup
warning is dismissed only on its private socket. Unsupported SHM transport is
reported as a failed transport check, never a passing GL-only substitute.


Final candidate validation before rejection: **204/204 native-input E2E PASS**
(`target/shm-tick/e2e-bgra/manifest.json`), **24/24 GL+SHM whole-screen images exact**
(`target/shm-tick/bgra-composition/results.json`), and strict all-target clippy PASS
(`target/shm-tick/clippy-bgra.log`). These validate pixels/functionality, not idle
acceptance. Production rollback validation is recorded in
`target/shm-tick/{clippy-reverted-final.log,renderer-reverted.log,e2e-reverted/}`.

Production rollback completed22:50UTC; release rebuild completed22:53:56UTC.
Strict all-target clippy and renderer regressions pass after removal. The rebuilt
release has no `shm.pbo_frames` / experimental SHM markers; its hash is recorded
in `target/shm-tick/reverted-release.json`. Twelve experimental executable copies
were retired (561,399,056 nominal bytes; hard links may share storage); their
hashes remain in `target/shm-tick/retired-artifacts.json`.

Final rollback E2E completed before22:54:25UTC: **204/204 PASS**, complete manifest
`target/shm-tick/e2e-reverted/manifest.json`; regenerated matrix in
`docs/verify/e2e.md`. No requested production optimization is retained. End of
bounded task before22:56UTC.
