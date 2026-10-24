# Live native-shell idle investigation — 2026-10-24

Status: freeze lifted; installed release6719efa8 already contains the complete
static-frame patch. All six source/test/doc files match its saved hashes. No
patch reapplication, code mutation or new build is required. Fresh read-only
installed preflight is READY; the confirming180s live window measured0.5500%
whole-process CPU including exited workers. No new release is required. No install or live lifecycle action was performed by this task.
The user accepted approximately0.95% live idle with hook traffic;0.1% remains an
aspiration. Historical Quickshell2.7–2.9% is user-provided context, not re-measured.

## Live baseline (not a nested-shell substitute)

Read-only PID2709217, hyprv-shell.service, installed release
`eda5ca090166790a41ddcd171f79008f01f589c766816b0d3040b41c07be0f03`.
Actual `/proc/PID/exe` SHA256:
`2073ea266e34c5aee6a69038baeaaeb75a2a287da636ee55176469f7d54c89d0`.
Observed2026-10-24 04:19:32–04:22:32UTC,180.0017s, without perf/strace or builds
from this task during the baseline. CPU is summed thread schedstat runtime /
wall time; scheduling/context-switch counts are reported separately.

| Thread | CPU% | Voluntary switches/s | Scheduling slices/s |
|---|---:|---:|---:|
| Main UI2709217 |0.197359 |10.483232 |10.699897 |
| Services2709219 |0.123308 |23.049778 |23.199776 |
| PipeWire2709222 |0.000025 |0.011111 |0.011111 |
| module-rt2709226 |0 |0 |0 |
| Agent discovery2709241 |0.358488 |0.666660 |1.799983 |
| Blocking worker2759442 |0.264310 |1.638873 |2.266645 |
| **Total** |**0.943490** |**35.849654** |**37.977412** |

All six endpoint threads persisted across the one-second sampled snapshots.
This does not prove absence of sub-second transient threads. Scheduling slices
and voluntary switches are wakeup proxies, not a complete `sched_wakeup` ledger.
[Raw baseline](raw/live-idle/before.json), [sampler](live-idle.py).

The passive socket2 connection received91events: configreloaded5, windowtitle9,
windowtitlev2 9, activewindow11, activewindowv2 11, workspace10, workspacev2 10,
hymission-tiles10, activelayout16. The desktop was not event-free. No input,
workspace/focus command, reload, service change or provider write was performed
by this task. The socket observer records names/counts only, not titles/payloads.
The original baseline sampler had no hook-file watcher; later sampler runs add
an inotify counter. Do not invent a live baseline hook count from later windows.

## Permitted tracing and evidence limits

A separate180s `perf record -e cpu-clock:u -F997 --call-graph dwarf,8192
--user-callchains -p2709217` captured2187samples, no lost samples. User-space
samples identify actual agent discovery (`ProcTable::refresh/restat`) and full
normalization (`sync_process_sessions`, session_pid/public_row, JSON and map
allocation), plus UI text lookup, damage bounds, allocation and painting.
It excludes kernel CPU; sampled percentages are not shares of all runtime.
Some call-chain parents remain unresolved, so no invented complete inclusive
flame graph is used. [Flat report](raw/live-idle/perf-flat.txt),
[call stacks](raw/live-idle/perf-report.txt), raw `perf.data` beside them.

`strace` was absent; an Arch package was extracted locally, not installed.
`strace -f -c -p2709217` was **DENIED** by ptrace/Yama. Kernel tracepoint access
for `sched:sched_wakeup` was also **DENIED**. No sudo, sysctl/tracefs changes,
ptrace-policy changes or injected code were attempted.
[Denials](raw/live-idle/strace-summary.txt), [tracepoint denial](raw/live-idle/sched-attach.log).

Permitted `perf record -e dummy:u --switch-events -m32 -p2709217` records kernel
PERF_RECORD_SWITCH events without privileged tracefs. In its separate30s
instrumented interval: services608schedule-ins, UI288, agent discovery117,
blocking worker147, PipeWire6. Non-preempting switch-outs: services584, UI236,
discovery19, blocking43, PipeWire6. These identify block/preemption counts but
not the identity of every waker. [Records](raw/live-idle/switch.txt).
The initial context-switch sampling attempt produced no usable samples; it is
not silently substituted for the successful switch-event recording.

## Source attribution and owner handoff

| Source | What is measured/proven | Required work and remaining boundary |
|---|---|---|
| Stats timer | Native host subscribes to1Hz display metrics; private real-provider diagnostic gives61stats notifications/60s | Changed labels must paint; metadata/sub-rounded-value suppression is already present |
| Agent scan timer |120discovery voluntary switches/180s exactly match1.5s ProbeWorker cadence;0.358488% CPU | Discovery freshness/PID-reuse/exec semantics must remain; range-scan work can be optimized |
| Agent hook/expiry/scan refresh | Blocking worker0.264310%; sampled full normalization; separate nested diagnostic129agent-state notifications with114hook-file events | Shared worker switches cannot all be assigned to this actor; incremental refresh bypass with a scan overlay is an optimization candidate |
| socket2 |91named live events/180s, including paired v1/v2 notifications | Event reads and genuine desktop state changes required; no exact per-event wake count without tracing |
| D-Bus | Separate60s metadata monitor:45FanStateChanged,2system PropertiesChanged; session2NewIcon,2NewToolTip,2launcher Update | Broadcast counts are inputs, not GetFans calls or exact reactor wakeups |
| Redraw/Wayland | Private diagnostic192paint submissions from129agent +61stats +2tray updates; no self-sustaining animation/paint loop | Static empty-commit/frame-callback round trip is removable; buffer delivery/release and animation callbacks remain necessary |
| PipeWire |2voluntary switches/180s, module-rt zero | No evidence supporting a PipeWire change |

System BecomeMonitor was denied; the monitor's permitted broadcast fallback
worked. Unicast fan RPCs were not observed. The source's unsampled fan actor
refreshes without subscriber demand and accepts broad helper signals; do not
label45broadcasts as45proven GetFans RPCs or as every service wake.
[System metadata](raw/live-idle/dbus-system.txt),
[session metadata](raw/live-idle/dbus-session.txt), [FD registrations](raw/live-idle/fd-sources.json).

The user explicitly kept svc-desktop and svc-power ownership with their active
agents. Exact findings, measured costs, proposed diffs and regression cases are
filed in [desktop request](../requests/hyprv-svc-desktop.md) and
[power request](../requests/hyprv-svc-power.md). Engine changed neither crate.
No sampling interval, hook behavior, completion lifetime or fan control was
removed as an idle shortcut.

## UI change under validation

`UiHandler::immediate_static_frames` defaults tofalse. The shell opts in only
for production scheduling; fixture clocks retain their existing behavior. A
configured, dirty surface with no pending callback/animation may paint directly
when the host needs no post-frame reconciliation. This removes the empty
commit/frame-callback round trip. Animation pacing and the existing callback
clock anchor remain intact; a static monotonic timestamp must not re-anchor a
later animation callback.

The isolated native test compares8updates on both paths: **8/8PNG byte-exact**,
**8→0frame callback requests**,8paints on each path, and a subsequent animation
retains monotonic/correct elapsed time. Rotation/scale tests opt into the same
immediate path across all8transforms and DPR1/1.5/2. Both tests pass.
[Native regression](raw/live-idle/static-and-rotation.log).

Same pinned candidate ELF
`1fa92e02152714b7e50fc493ae8115efd2e8359722abf97688dfee357bed2c05`,
separate instrumented real-hook diagnostic windows:

| Path | Hook-file inputs | Agent UI notifications | Paints | Main dispatches | Frame callbacks requested |
|---|---:|---:|---:|---:|---:|
| Callback control |66 |85 |150 |483 |150 |
| Immediate static |41 |53 |123 |248 |0 |

Dispatches/paint3.22→2.02 supports the independently proven callback removal.
Whole process CPU0.8150%→0.6152% is **NOT a causal improvement claim**: these
windows are instrumented, external inputs differ and concurrent builds existed.
[Control CSV](raw/live-idle/nested-control.csv),
[immediate CSV](raw/live-idle/nested-immediate.csv).

Uninstrumented90s ABBA measurements on that same ELF (all hook counters0):

| Run | Path | Summed thread CPU% | Process-tick CPU% | Voluntary switches/s | UI voluntary switches/s |
|---|---|---:|---:|---:|---:|
|2 |Callback |0.167858 |0.177546 |20.451113 |3.650795 |
|3 |Immediate |0.140051 |0.133141 |17.918539 |2.152444 |
|4 |Immediate |0.164513 |0.155244 |18.973046 |2.162328 |
|5 |Callback |0.203433 |0.210790 |20.968038 |3.616709 |
|Mean |Callback |**0.185645** |0.194168 |**20.709575** |**3.633752** |
|Mean |Immediate |**0.152282** |0.144192 |**18.445792** |**2.157386** |

Main UI CPU mean0.140022%→0.108045%. Summed CPU includes service/discovery
threads; it does not hide offloaded work. These runs retain all actors and
read-only real providers, but use an empty hook fixture/private PID namespace.
Parallel builds/parity overlapped some intervals, and real stats changed, so
these are sequential observations, not a claim of an uncontended budget pass
or of exactly18% CPU improvement in the live shell. The callback-removal
regression independently isolates the removed protocol work. No<=0.1% claim.
[Raw summary and all four receipt paths](raw/live-idle/quiet-abba.json).

The first attempt failed before measurement because the helper identified the
pinned executable by its comm name. It now matches `/proc/PID/exe` inode identity
against the selected ELF, preserving renamed/pinned-binary measurements. A
separate setup failure was an unset asset root for the relocated ELF; reruns
explicitly selected the staged runtime assets. Both failures remain in logs and
are excluded from results.

The final combined staged binary differs from this earlier pinned diagnostic
ELF because service owners concurrently changed their crates. Its CPU is not
silently substituted for the same-ELF ABBA measurements. A live after result
requires a settled interval after the user's installation. Original live PID
2709217 remained active at the freeze checkpoint.

## Validation / release status at the freeze checkpoint

- Private native static/rotation tests:2PASS;8/8static captures byte-exact;
  opt-in scheduling tested across DPR1/1.5/2 and all8output transforms.
- Renderer regressions:PASS, including private EGL buffer audit/cache tests.
- Strict owned-crate Clippy:PASS (`clippy.log`, empty).
- Shell library tests:61PASS,2intentional ignored (`shell-unit.log`).
- Canonical GL renders: island380/380, panels300/300, bar300active PASS with
  the same12declared exclusions. Preflight subsequently rejects stale dependency
  hashes caused by parallel source changes; numerical pass is not current-source
  readiness. [Run log](raw/live-idle/parity-gl.log).
- Final fresh private E2E: **204/204PASS**, `suite_complete=true`, on the
  combined staged binary; it started before the freeze and completed afterward.
  [Summary](raw/live-idle/e2e-final-summary.json), raw manifest
  `target/live-idle/e2e-final/manifest.json`. No further shared build or new suite
  was started during the freeze.

`packaging/build-release.sh` completed successfully after parallel inputs settled:
[build receipt log](raw/live-idle/build-release-verified.log).

Release ID:
`eeafb5a678912bbaa802016d562b2b69fcea3ab0afc20f2f2a54ddba788a04c5`

Staged shell ELF SHA256:
`c364f556a1a5168a7bcac866616e1912a87b06951b7ac321b2a7a0f22d1d52df`

The existing final release rebuild **includes this task's six source/test/doc
changes**. User was told immediately at the freeze. No install, source revert,
waiver rebind, helper change, systemd restart or Hyprland action was performed.

Read-only preflight at this checkpoint: paths/binaries/runtime assets PASS;
automated receipt failed/stale, parity dependency hashes stale, performance
source evidence stale, installed helper release differs from this candidate.
[Exact preflight result](raw/live-idle/preflight-final.json). The cutover tests
have pre-cutover assumptions against the now-cut-over live Lua tree
(1failure/2errors); packaging owner request is in hyprv-shell.md. Root/orchestrator
is now gating/installing under its freeze; later authoritative receipts may
supersede this checkpoint. No gate was edited to make these results green.

Review patch (already applied before freeze):
[static-frame.patch](scratch/live-idle/static-frame.patch),
[per-file hashes](scratch/live-idle/static-frame-inputs.json).
It contains only the six files reported to the user, reconstructed by removing
this task's edits from current sources in memory. **Do not apply or reverse it
in the shared tree during the freeze without the user's decision.** The patch
was saved without writing any frozen file; all six hashes matched afterward.

Shared staging attempts that saw source changes correctly refused receipts.
An own duplicate staging job was stopped before it could race another writer.
No private Cargo target directory or frozen full workspace was created.

Own intermediate benchmark ELF and extracted strace tool retired after summaries
were saved (44.65MB nominal); hashes/sizes in
[retired artifact receipt](raw/live-idle/retired-artifacts.json). The staged
release and root/orchestrator gate artifacts were preserved.


## Freeze lifted: installed release6719efa8

The user installed
`6719efa8ffa9c3d3ccba8dc82545ee2bee9a8b25976cb99260bf39bd04c56dac`
and lifted the freeze. Live PID3156707 has ELF SHA256
`c364f556a1a5168a7bcac866616e1912a87b06951b7ac321b2a7a0f22d1d52df`:
**the same shell ELF** previously validated as part of eeafb5a6. Packaging-only
release identity changed; this task's patch did not need reapplication.

Before resuming, all six current source/test/doc hashes were compared against
`static-frame-inputs.json`; all match byte-for-byte. This is the reconciliation
onto current sources: preserve the installed implementation and apply nothing
twice. [Rebase/installed receipt](raw/live-idle/rebase-installed.json).

A new read-only `cutover/preflight.py --installed --json` returns **READY**:
release paths/binaries,44runtime assets, workspace all-targets/doctests/xtask/
cutover/probe tests, production GL components and assembled full-screen matrix
(including unmasked exact corners), accepted performance and installed helper
all pass. [Installed readiness](raw/live-idle/preflight-installed-6719efa8.json).
This supersedes the pre-freeze stale/failed readiness checkpoint above. The
accepted gate's separate measurements are0.154686%controlled quiet and
0.465925%live-hook idle,16hook-file events; they are the orchestrator's receipts,
not this task's new live observation or a causal UI-only A/B.

No code changed after the freeze was lifted, no shared-target build was run,
and no live lifecycle/input/provider action was performed. The180s after window
uses the same read-only sampler against the newly installed PID. Results follow;
the sampler was expanded to account for transient blocking workers.


### Completed live after measurement

Live PID3156707 remained unchanged. The confirming interval lasted
180.000880s, 2026-10-24 05:22:26
through05:25:26 UTC.

| Live observation | CPU% | Voluntary-switch proxy/s | socket2 inputs | Hook-file notifications |
|---|---:|---:|---:|---:|
| eda5ca09 before,180s |0.943490 |35.849654 |91 |Not measured in original window |
|6719efa8 after,180s |**0.549997** |**29.205413** |103 |65 |

The after CPU is **whole-process utime+stime**, so it includes exited workers;
its tick quantum is0.005556percentage points.
Summing last-observed runtime of every sampled thread independently gives
0.547766%, consistent within that quantum.
Do **not** use the raw JSON's matching-endpoint `cpu_percent`0.463126% as the
whole-process result: it omits transient workers. The first after window likewise
had0.455641%matching endpoints but0.540026%accounted sampled-thread CPU; this is
why the second confirmation was collected. Observed switch counts for transient
threads are lower bounds, not exact sched_wakeup counts.

| Confirmed after source/thread | Accounted CPU% | Observed voluntary switches/s |
|---|---:|---:|
| Main UI |0.166414 |7.427741 |
| Services |0.084752 |20.549900 |
| Agent discovery |0.211960 |0.661108 |
| Blocking workers, combined |0.084639 |0.566664 |
| PipeWire + module-rt |0 |0 |

These are **combined-release, sequential live observations** with different
external traffic. They do not assign the entire CPU reduction to this task's
UI change. The earlier same-ELF ABBA and8→0native callback regression remain
the evidence that isolates the UI optimization. No<=0.1%idle claim is made.
Remaining service/discovery findings stay with their owners; no cross-owner
source edits were made.

[Confirmed raw window](raw/live-idle/installed-6719efa8-confirmed.json),
[first window with transient accounting](raw/live-idle/installed-6719efa8-accounted.json),
[before/after summary](raw/live-idle/live-before-after.json).

Reconciliation is complete. The six original files are already installed,
source reapplication was a no-op, no production code changed after the freeze
lift, and the installed artifact is the same ELF with204/204E2E and the new
static-frame/animation-clock regression passing. No additional build/install
is requested by this task. Root/orchestrator controls all deployment actions.
