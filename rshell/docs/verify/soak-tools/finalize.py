#!/usr/bin/env python3
"""Write the report only after the actual 60+ minute run has completed."""
import collections,hashlib,json,statistics,sys
from pathlib import Path
from analyze import analyze,chart,gpu_chart
run=Path(sys.argv[1]).resolve();root=Path(__file__).resolve().parents[3]
status=json.loads((run/'status.json').read_text())
assert status['status']=='complete' and status['elapsed']>=3600 and status['shell_exit_code']==0,status
a,samples=analyze(run);chart(run/'memory.svg',samples);gpu_chart(run/'gpu.svg',a)
frozen=run/'hyprv-shell-release';assert hashlib.sha256(frozen.read_bytes()).hexdigest()==status['binary_sha256']
values=lambda key:[s['shell'][key] for s in samples]
span=lambda key:f"{min(values(key))}–{max(values(key))}"
fmt=lambda value:f'{value:.3f}' if value is not None else 'unavailable'
counts=a['event_counts'];extra=[json.loads(x) for x in (run/'extra-input.jsonl').read_text().splitlines()]
watch=[json.loads(x) for x in (run/'hotplug-watch.jsonl').read_text().splitlines()]
registrations=[]
for cycle in sorted({x['cycle'] for x in watch}):
    rr=[x for x in watch if x['cycle']==cycle]
    present=next((x for x in rr if 'SOAK-HOTPLUG' in x.get('outputs',[])),None)
    bar=next((x for x in rr if x.get('bars',{}).get('SOAK-HOTPLUG')),None)
    if present and bar:registrations.append(dict(cycle=cycle,registration_delay_ms=(bar['elapsed']-present['elapsed'])*1000))
(run/'hotplug-analysis.json').write_text(json.dumps(registrations,indent=2)+'\n')
long_frames=[]
for line in (run/'trace.log').read_text().splitlines():
    p=line.split()
    if len(p)==6 and p[0]=='F' and int(p[4])/1e6>16.7 and int(p[1])/1e9-status['start_monotonic']>=120:
        long_frames.append(dict(elapsed=int(p[1])/1e9-status['start_monotonic'],milliseconds=int(p[4])/1e6))
(run/'long-submissions.json').write_text(json.dumps(long_frames,indent=2)+'\n')
tr=a['trends'];dirty=tr['private_dirty_kib'];gpu=tr['gpu_mib']
gpu_values=[x['memory_mib'] for x in a['gpu_samples']]
rel='raw/soak/release-62m'
table=[]
for b in a['bins']:
    gg=[x['memory_mib'] for x in a['gpu_samples'] if b['start_s']<=x['elapsed']<b['end_s']]
    g=f'{min(gg):g}–{max(gg):g}' if gg else '—'
    table.append(f"| {b['start_s']/60:g}–{b['end_s']/60:.1f} | {b['rss_mib']:.1f} / {b['pss_mib']:.1f} | {b['fds_min']}–{b['fds_max']} | {b['threads_min']}–{b['threads_max']} | {b['armed_timers_min']}–{b['armed_timers_max']} | {g} | {fmt(b['render_p50_ms'])} / {fmt(b['render_p99_ms'])} |")
report=f'''# Release shell activity soak

**Completed: {status['elapsed']:.2f} seconds ({status['elapsed']/60:.2f} minutes), one retained shell PID, clean exit 0.** No crash, failed replay, status-probe timeout, EGL error, fd/thread accumulation, or prolonged render/submission stall was observed. **Small private-dirty and GPU-memory retention is flagged as SOAK-1 for owner investigation; this is not a blanket leak-free verdict.**

Run: **{status['start_utc']} → {status['end_utc']}**. The pinned release executable is retained at [{rel}/hyprv-shell-release]({rel}/hyprv-shell-release), SHA256 `{status['binary_sha256']}`. [Release build log](raw/soak/build.log); [binary/source-at-launch/tool hashes]({rel}/provenance.json); [final status]({rel}/status.json). The shared workspace and release target changed during parallel work: this result certifies the copied executable, not later workspace builds or a causal commit. Source-at-launch hashes are a checkpoint, not a frozen build receipt.

## Isolation and workload

Actual uid 1000, no installation or root execution. The production assembly ran with `--read-only --no-exclusive` under **private virtual KWin + nested Hyprland**, at 1280×720, DPR 1, with secondary headless-output hotplug. Bubblewrap isolated user/PID/network/IPC/UTS namespaces and made the root filesystem read-only except the test output/runtime. HOME/config/cache/state were private. Both D-Bus addresses pointed at a private daemon with **no activation service directories**. PipeWire runtime was private; configured bus/runtime endpoints and IP networking were isolated from the host providers. Live Hyprland, Quickshell and their service/config state were untouched. Syntax validation and private compositor config-error/status receipts are retained.

The same shell ran through **{a['events']} scheduled steps**, with **{len(a['failed_events'])} rejected/failed steps**:

- Calendar open/close, host morph and week-scroll; control panel open/close; volume and brightness **previews**.
- Twelve synthetic Claude/Codex sessions: processing, waiting, completed, list hover/scroll and removal. No approval, focus or hook installation action was sent.
- Mock MPRIS owner acquire/release, playing/paused states and track metadata; synthetic logind `PrepareForSleep(true/false)` on the private bus. No real media transport or machine suspend action.
- **{counts.get('workspace',0)} workspace changes and {counts.get('hotplug-add',0)} output-add / {counts.get('hotplug-remove',0)} output-remove cycles**, all confined to the private compositor.
- Supplemental virtual-pointer input verified **{a['supplement_expansions_verified']} island expansions** and **{a['supplement_stats_verified']} stats-popup opens** from layer geometry, with {sum(x.get('ok') is False for x in extra)} supplemental errors. [Screenshots and input receipts]({rel}/extra-input.jsonl).

Visual inspection found that the initial center-clock hover intentionally keeps app UI compact. Correct side-hotspot/stats input was added at **427.9 seconds without restarting the shell**. Comparable growth analysis begins at **720 seconds**, after a complete supplemented cycle; every earlier raw sample remains in the plots/table. This workload change is not classified as a leak.

## Resources and timing

{a['samples']} resource samples at approximately 5-second intervals; {len(a['gpu_samples'])} per-process NVIDIA graphics-memory samples at approximately 60-second intervals. NVIDIA accounting identified the copied shell as a graphics client on the RTX PRO 6000. Memory figures below are MiB except the explicitly identified KiB retention.

| Metric | Observation |
|---|---|
| RSS | Cold {a['first']['rss_kib']/1024:.1f}; observed range {min(values('rss_kib'))/1024:.1f}–{max(values('rss_kib'))/1024:.1f}; last sample {a['last']['rss_kib']/1024:.1f}. Comparable settled samples {tr['rss_kib']['first']/1024:.1f} → {tr['rss_kib']['last']/1024:.1f}. |
| PSS | Observed {min(values('pss_kib'))/1024:.1f}–{max(values('pss_kib'))/1024:.1f}; last {a['last']['pss_kib']/1024:.1f}. Shared-page attribution fluctuates; no monotonic PSS growth. |
| Private dirty | Comparable samples {dirty['first']/1024:.3f} → {dirty['last']/1024:.3f}; **+{dirty['last']-dirty['first']} KiB**, fitted {dirty['slope_per_hour']:.1f} KiB/hour. Flagged, not assumed to be a leak. |
| Fds / threads | Fds {span('fds')}, threads {span('threads')}; matching settled phases remain {tr['fds']['first']} → {tr['fds']['last']} fds and {tr['threads']['first']} → {tr['threads']['last']} threads. |
| Kernel timers | Timerfd descriptor count {min(len(s['shell']['timerfds']) for s in samples)}–{max(len(s['shell']['timerfds']) for s in samples)}; armed count {span('armed_timerfds')}, matching settled phases {tr['armed_timerfds']['first']} → {tr['armed_timerfds']['last']}. POSIX timer receipts and arm/disarm calls retained. |
| NVIDIA graphics accounting | Cold {gpu_values[0]:g}; observed {min(gpu_values):g}–{max(gpu_values):g}; matching settled samples **{gpu['first']:g} → {gpu['last']:g}**. Includes cached/reserved driver memory, not solely live GL objects. |
| Render/submission p50 / p99 | **{fmt(a['render_p50_ms'])} / {fmt(a['render_p99_ms'])} ms**, after the initial 120 seconds; {a['post_warmup_frame_count']} measured surface submissions. |
| Long submissions | {len(long_frames)} exceeded 16.7 ms; maximum {fmt(a['render_max_ms'])} ms; {len(a['render_over_250ms'])} exceeded the 250 ms stall screen. Most early outliers coincide with first-use expanded layouts. [Exact timestamps]({rel}/long-submissions.json). |
| Status liveness | Resource-probe + status-RPC p50/p99 **{fmt(a['ipc_p50_ms'])} / {fmt(a['ipc_p99_ms'])} ms**; {len(a['probe_errors'])} probe failures. The frozen driver's interval includes the preceding `/proc` snapshot: it is an upper bound on RPC latency, not a pure IPC benchmark. |

![RSS and PSS over the complete run]({rel}/memory.svg)

![Per-process GPU accounting; gold points are matching settled phases]({rel}/gpu.svg)

| Elapsed min | Median RSS / PSS | Fds | Threads | Armed timerfds | GPU range | Render p50 / p99 ms |
|---|---|---|---|---|---|---|
{chr(10).join(table)}

Render timing comes from a **test-only LD_PRELOAD observer**: successful `eglMakeCurrent` completion → swap completion, separately recording swap duration. It includes CPU-side drawing/submission and driver waiting, excludes preceding scene/model construction and GPU/presentation completion, and is per surface. The release executable is unchanged. This is **not whole-frame presentation latency**. The observer adds one trace fd and some measurement overhead. Logical Tokio timer cardinality is not exposed; reported timer counts are kernel timerfds, not all application deadlines. Missing mock providers perform retries, visible UI waves intentionally animate, and probes themselves request redraws: no production zero-wakeup or full-live CPU-budget pass is claimed.

## Findings and owner follow-up

**SOAK-1 — small monotonic retention; investigation, not a confirmed unbounded leak.** Across matched settled samples, private-dirty memory retained {dirty['last']-dirty['first']} KiB and NVIDIA process graphics accounting retained {gpu['last']-gpu['first']:g} MiB. RSS fell and fd/thread/timer counts stayed bounded. GPU accounting plateaued after the earlier step; cache/allocator high-water retention remains a plausible explanation. The analyzer flags these observations so they are not silently called leak-free. Ask the renderer owner to expose bounded cache/live-allocation bytes and repeat the retained-binary workload with a warmed, unchanged working set before assigning a cause or changing eviction. Primary owner: [hyprv-ui](../requests/hyprv-ui.md), host lifecycle follow-up: [hyprv-shell](../requests/hyprv-shell.md). No causal source line is assigned because the renderer changed during the run.

**Stall-screen false positives were checked.** The first-next-swap proxy sometimes reported long delays after control-panel close or output add. Unmapping need not cause a new swap. A separate 50 ms read-only observer saw the new output and its native bar layer in the same polling interval in {len(registrations)} final hotplug cycles. Those proxy candidates are **not confirmed UI stalls**; layer registration is not pixel/presentation proof. [Rejected candidates]({rel}/rejected-next-swap-stall-candidates.json), [direct observer]({rel}/hotplug-watch.jsonl), [summary]({rel}/hotplug-analysis.json). No prolonged render/submission or status-liveness stall was demonstrated.

One fd-count candidate at the final matched phase was a transient socket: the following sample returned to 79. Count trends use the per-cycle quiet floor while preserving every raw maximum; this was not filed as accumulation.

The private-bus method observer began partway through the run; its recorded interval contains no provider mutation methods. Mock services also logged no forbidden Set calls. Namespace/read-only constraints covered the entire run. See [private-bus methods]({rel}/private-bus-methods.log), [mock events]({rel}/mock-events.jsonl), [measurement notes]({rel}/measurement-notes.json).

Cleanup completed successfully: shell/driver/helpers exited 0, the private runtime was removed, and the private-bus observer is gone. [Cleanup and mutation-audit receipt]({rel}/cleanup.json).

## Limits, evidence and reproduction

This covers the pinned release at DPR 1 with a small, known workload. Real PipeWire/audio, NM/BlueZ devices, artwork decoding/downloads, bus-daemon restart/reconnection, physical input, fractional/high DPR and hardware suspend are **not exercised**. It does not certify later builds, full-screen parity, production GPU memory budgets or presentation smoothness under arbitrary system load.

[Raw samples]({rel}/samples.jsonl), [scheduled actions]({rel}/events.jsonl), [full GL/timer trace]({rel}/trace.log), [machine-readable analysis]({rel}/analysis.json), [frozen launch tools]({rel}/tools-at-launch/), [reproduction instructions](soak-tools/README.md). To retain the exact executable, pass `--binary docs/verify/raw/soak/release-62m/hyprv-shell-release` to `run.py`; use a new output directory. `extra-input.py` supplies the actual app-side hotspots and stats click. `analyze.py` regenerates tables/plots, and `finalize.py` refuses to claim completion before a successful 60+ minute run.
'''
(root/'docs/verify/soak.md').write_text(report)
print(json.dumps(dict(elapsed=status['elapsed'],samples=a['samples'],events=a['events'],render_p50=a['render_p50_ms'],render_p99=a['render_p99_ms'],private_dirty_retained_kib=dirty['last']-dirty['first'],gpu_retained_mib=gpu['last']-gpu['first'],hotplug_registrations=registrations),indent=2))
