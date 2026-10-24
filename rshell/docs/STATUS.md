# Orchestration status — 2026-10-23 ~04:45

Claude agents (spec-bar, spec-panels, spec-island, svc-media, svc-desktop) hit the org usage limit; they auto-resume ~07:30 ET.
Codex agents (engine, svc-power, svc-net, svc-sys) keep running.

## Done
- Specs: docs/spec/* (UI, services, text rendering, coverage, components); parity reference harness (parity/, 904+ refs, GPU path)
- hyprv-core, hyprv-ui engine (0 idle wakeups), hyprv-widgets (+Qt-exact flickable)
- 5 service crates (net, media, power, sys, desktop); cavacore bit-exact spectrum; 6 review rounds, P1s fixed
- Service idle CPU: 0.35% live vs quickshell 2.66%; ~0.02% excluding genuine agent-hook events
- UI crates: island (passes old gate), panels 242/248 (F16 harness), bar CPU224/304 / production GPU303/304 (all64 remaining max-1 crops exact) (latest bar/VALIDATION.md)
- Cutover tooling prepared (not applied): packaging/, cutover/

## In flight
- engine: full assembly in hyprv-shell, distance-field text, nested e2e test, host fixes (SHELL-*, per-frame path, touchpad)
- svc-media: GPU-cached Qt rounded-rect mode (PERF-UI-5 P0) + engine perf items
- svc-power: bar GPU303/304; all64 max-1 crops closed, native111/112 exact; remaining frozen submenu input-state mismatch max23 (bar/VALIDATION.md)
- svc-net: flickable wiring into panels
- svc-sys: integration CI loop (docs/verify/ci.md)
- svc-desktop: cutover fixes CUT-1..7 (P1: CUT-1, CUT-5)
- spec-bar: panels PANEL-1 (password in text caches, P1), PANEL-2/3, last stats crops
- spec-island: QA lead loop incl. GPU-path parity gate (docs/verify/parity.md)

## Decisions taken (user may override)
- Quirks replicate LIVE behaviour behind QUIRK_* consts (battery popup never opens, inverted volume scroll, swipe-to-hide dead, no idle hover, light StartupConfirm)
- Legacy behaviour beats KDE semantics where they differ (e.g. new Wi‑Fi secrets stored in system profile)
- Runtime Qt-free; shell SVGs baked offline with QtSvg
- Parity gate on premultiplied RGBA; non-DF images must be exact

## Historical GPU capture outliers — fixture asset race reproduced

Two earlier retained-target captures were corrupted (`cpu-power-medium--light@1`,
max channel diff 245; `fullscreen-slide--hide-t0000--light@2`, max 212).
Evidence is preserved in `crates/hyprv-ui-bar/artifacts/submenu-gpu/`.
These two specific outliers are now explained by an input race, rather than
unreproduced GPU corruption: `hyprv-ui-bar/scripts/parity.py` deleted/rebuilt
shared theme-icon directories while the adapter resolved tray assets. A missing
directory selects different on-disk Fluent icons; an existing empty directory
selects fallback letters. Private absent/empty-directory interventions reproduce
the **entire affected tray strips of both original PNGs exactly** (max0), including
all pixels that differed by more than8. Evidence:
`crates/hyprv-ui-bar/artifacts/icon-race-reproduction/comparison.json` and
`icon-race-{absent,empty}/`. Two new outliers were also reproduced exactly with
empty/partially populated snapshots. Replay now pins binaries and hashed asset
snapshots; canonical preparation uses report-local asset directories. This closes
the diagnosis of these named captures, not a claim that every GPU failure is
universally ruled out. No synchronization/fence workaround was added.

Earlier negative stress evidence (retained):

Subsequent stress on NVIDIA RTX PRO 6000 Blackwell produced 0 pixel mismatches:
66,000 clean multi-surface frames with resizes, surface recreation, delayed
readback, mapped private surfaces and partial damage; plus 3,040 corpus frames
with 0 mismatches whose launcher's final provenance check failed because the
binary pathname was replaced during the run. Aggregate diagnostics record
69,040 frames, 4,760 exact comparisons, 25,982 resizes, 677 recreations and
9,304 partial updates (`crates/hyprv-ui-bar/artifacts/native-stress-summary.json`).
The 3,040 corpus frames are qualified evidence, not a clean launcher exit.
These negative results bound the reproduction attempt on this driver only.
The outlier PNGs precede EGL presentation. The asset interventions above now
provide the causal evidence that was missing from the earlier stress attempts.
Keep the historical captures and qualified stress receipts for audit.


Fresh pinned validation after the asset-race fix:6,000 synthetic frames over four
mapped private surfaces,776 checkpoints,1,974 resizes,63 recreations,2,662 partial
updates, plus3,040 corpus frames (10complete304-image replays against an independent
oracle): **zero mismatches**, clean receipts. See bar/artifacts/
`final-native-material-stress/run.log` and
`final-pinned-corpus-stress/run-0/stress.json`. This fresh corpus run has a pinned
binary and hashed private asset snapshot; the older qualified launcher result
above is retained independently.

Bar native exact closure checkpoint (2026-10-23): all64 residual max-1 crops now
exact; production GPU303/304, native111/112 exact. Sole remaining bar gate failure
is the already documented frozen submenu input-state mismatch max23. Native rect,
text, image and compositing attribution error all0 outside that mismatch. Final
stress:12,000 synthetic frames (half mapped) plus608 frozen corpus frames,0
mismatches;380 island+300 panels/calendar control images byte-identical. Full
metrics and regression receipts: `crates/hyprv-ui-bar/VALIDATION.md`.

# Update — 2026-10-23 ~11:40
Done since 04:45: assembly (hyprv-shell runs everything), e2e 204/204 in nested compositor, 62-min soak clean,
cutover rehearsed 5 rounds in isolation (switch/fallback/rollback byte-exact), rotation crash fixed,
system-bus degrade, Qt icon-theme lookup, workspace build/tests/clippy green (CI on snapshots).
Perf (real shell, controlled idle): 0.216% CPU vs Quickshell 2.74%; GPU mem 38 MiB vs 447 MiB. Target 0.1% not met yet.
Parity (GPU gate): bar 303/304 before tray-icon reference recapture (now 151, being re-matched by spec-island);
panels 82/300 strict (264 v1); island 0/380 strict (154 v1, svc-power working on it); full-screen 48/64 with tray masked.
Disk: root hit 100% at ~11:15; cleaned to ~88 GB free; disk policy in ARCHITECTURE.md.
Open: activity memory growth (svc-media), 35 ms reveal frame + idle (engine), svc-net R2 fixes, GPU corruption (historical status; CLOSED by the follow-up below).
Do NOT run cutover --apply until preflight is green.


## GPU-corruption risk CLOSED — 2026-10-23 20:19UTC

The two named historical PNG outliers are causally reproduced by the already-fixed
fixture icon-directory race:both entire tray strips exact,all6,209 historical
>8-delta pixels explained. Fresh pixel/hash check is retained.

The broader private lifecycle search also found and fixed a direct EglWindow resize
bug on Mesa:the retained image was CPU-correct but an old-size EGL back buffer could
clip/drop the first resized frame. Recreate the EGLSurface on size changes; rebinding
alone is insufficient. Host configure/rotation already retired whole windows.
A seven-transition CPU+actual-window regression and a poisoned-buffer detection/
next-frame recovery regression pass on NVIDIA and Mesa. No fence workaround.

Final search:21,952 complete submissions,18,410 recorded CPU/control comparisons
including458 preserved before intentional compositor loss,18,406 exact pre-swap
audits,0 lifecycle mismatches;2,752popup cycles,192simulated context resets,
525bounded pressure cycles,5,540partial-damage frames. Real60/144Hz private outputs,
hotplug/scale/rotation,compositor restart,2s callback pause,mid-frame cache purges,
GL/Skia interleaving and unread bursts covered. Debug-only opt-in
HYPRV_GPU_SELF_CHECK detects source/default-buffer mismatch and reset status;
release builds contain no audit/fault injection. Real hardware reset/KMS VT and
physical scanout were not induced; this closes the tracked risk, not every
possible upstream driver fault.

Regression controls:bar304/304 exact,island380/380 exact,panels300/300pass;
full-screen64/64unmasked under both scorers,max6,924window audits exact.
Actual host private output/navigation13/13;UI tests/Clippy/release checks pass.
Report and source/ELF/hash receipts:[GPU risk closure](verify/gpu-corruption.md).
No live session changes or cutover. Historical captures remain available.
