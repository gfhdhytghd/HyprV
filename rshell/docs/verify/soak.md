# Release shell activity soak

**Completed: 3720.00 seconds (62.00 minutes), one retained shell PID, clean exit 0.** No crash, failed replay, status-probe timeout, EGL error, fd/thread accumulation, or prolonged render/submission stall was observed. **Small private-dirty and GPU-memory retention is flagged as SOAK-1 for owner investigation; this is not a blanket leak-free verdict.**

Run: **2026-10-23T12:09:38.705525+00:00 → 2026-10-23T13:11:38.727166+00:00**. The pinned release executable is retained at [raw/soak/release-62m/hyprv-shell-release](raw/soak/release-62m/hyprv-shell-release), SHA256 `a06f202398919db3917914c85bb17a6a12cc3b838e81c097d218e14a929281b1`. [Release build log](raw/soak/build.log); [binary/source-at-launch/tool hashes](raw/soak/release-62m/provenance.json); [final status](raw/soak/release-62m/status.json). The shared workspace and release target changed during parallel work: this result certifies the copied executable, not later workspace builds or a causal commit. Source-at-launch hashes are a checkpoint, not a frozen build receipt.

## Isolation and workload

Actual uid 1000, no installation or root execution. The production assembly ran with `--read-only --no-exclusive` under **private virtual KWin + nested Hyprland**, at 1280×720, DPR 1, with secondary headless-output hotplug. Bubblewrap isolated user/PID/network/IPC/UTS namespaces and made the root filesystem read-only except the test output/runtime. HOME/config/cache/state were private. Both D-Bus addresses pointed at a private daemon with **no activation service directories**. PipeWire runtime was private; configured bus/runtime endpoints and IP networking were isolated from the host providers. Live Hyprland, Quickshell and their service/config state were untouched. Syntax validation and private compositor config-error/status receipts are retained.

The same shell ran through **534 scheduled steps**, with **0 rejected/failed steps**:

- Calendar open/close, host morph and week-scroll; control panel open/close; volume and brightness **previews**.
- Twelve synthetic Claude/Codex sessions: processing, waiting, completed, list hover/scroll and removal. No approval, focus or hook installation action was sent.
- Mock MPRIS owner acquire/release, playing/paused states and track metadata; synthetic logind `PrepareForSleep(true/false)` on the private bus. No real media transport or machine suspend action.
- **20 workspace changes and 20 output-add / 20 output-remove cycles**, all confined to the private compositor.
- Supplemental virtual-pointer input verified **37 island expansions** and **18 stats-popup opens** from layer geometry, with 0 supplemental errors. [Screenshots and input receipts](raw/soak/release-62m/extra-input.jsonl).

Visual inspection found that the initial center-clock hover intentionally keeps app UI compact. Correct side-hotspot/stats input was added at **427.9 seconds without restarting the shell**. Comparable growth analysis begins at **720 seconds**, after a complete supplemented cycle; every earlier raw sample remains in the plots/table. This workload change is not classified as a leak.

## Resources and timing

742 resource samples at approximately 5-second intervals; 62 per-process NVIDIA graphics-memory samples at approximately 60-second intervals. NVIDIA accounting identified the copied shell as a graphics client on the RTX PRO 6000. Memory figures below are MiB except the explicitly identified KiB retention.

| Metric | Observation |
|---|---|
| RSS | Cold 255.5; observed range 255.5–575.0; last sample 564.4. Comparable settled samples 575.0 → 564.4. |
| PSS | Observed 175.0–423.2; last 392.9. Shared-page attribution fluctuates; no monotonic PSS growth. |
| Private dirty | Comparable samples 358.570 → 358.785; **+220 KiB**, fitted 233.3 KiB/hour. Flagged, not assumed to be a leak. |
| Fds / threads | Fds 79–81, threads 8–8; matching settled phases remain 79 → 79 fds and 8 → 8 threads. |
| Kernel timers | Timerfd descriptor count 3–3; armed count 2–3, matching settled phases 2 → 2. POSIX timer receipts and arm/disarm calls retained. |
| NVIDIA graphics accounting | Cold 23; observed 23–69; matching settled samples **56 → 61**. Includes cached/reserved driver memory, not solely live GL objects. |
| Render/submission p50 / p99 | **0.530 / 3.278 ms**, after the initial 120 seconds; 17479 measured surface submissions. |
| Long submissions | 6 exceeded 16.7 ms; maximum 53.515 ms; 0 exceeded the 250 ms stall screen. Most early outliers coincide with first-use expanded layouts. [Exact timestamps](raw/soak/release-62m/long-submissions.json). |
| Status liveness | Resource-probe + status-RPC p50/p99 **4.232 / 14.282 ms**; 0 probe failures. The frozen driver's interval includes the preceding `/proc` snapshot: it is an upper bound on RPC latency, not a pure IPC benchmark. |

![RSS and PSS over the complete run](raw/soak/release-62m/memory.svg)

![Per-process GPU accounting; gold points are matching settled phases](raw/soak/release-62m/gpu.svg)

| Elapsed min | Median RSS / PSS | Fds | Threads | Armed timerfds | GPU range | Render p50 / p99 ms |
|---|---|---|---|---|---|---|
| 0–5.0 | 526.9 / 362.7 | 79–81 | 8–8 | 2–3 | 23–37 | 0.493 / 4.277 |
| 5–10.0 | 535.1 / 378.2 | 79–81 | 8–8 | 2–3 | 37–47 | 0.502 / 4.147 |
| 10–15.0 | 575.0 / 417.2 | 79–81 | 8–8 | 2–3 | 39–65 | 0.526 / 3.087 |
| 15–20.0 | 575.0 / 414.9 | 79–81 | 8–8 | 2–3 | 56–65 | 0.543 / 3.604 |
| 20–25.0 | 574.4 / 410.7 | 79–81 | 8–8 | 2–3 | 57–69 | 0.524 / 3.297 |
| 25–30.0 | 573.3 / 410.3 | 79–81 | 8–8 | 2–3 | 61–69 | 0.538 / 3.387 |
| 30–35.0 | 572.9 / 407.7 | 79–81 | 8–8 | 2–3 | 61–69 | 0.557 / 3.233 |
| 35–40.0 | 567.1 / 404.5 | 79–81 | 8–8 | 2–3 | 61–69 | 0.524 / 3.208 |
| 40–45.0 | 567.1 / 409.7 | 79–81 | 8–8 | 2–3 | 61–65 | 0.523 / 3.298 |
| 45–50.0 | 567.0 / 404.4 | 79–81 | 8–8 | 2–3 | 61–65 | 0.561 / 3.246 |
| 50–55.0 | 565.6 / 392.2 | 79–81 | 8–8 | 2–3 | 61–65 | 0.540 / 3.555 |
| 55–60.0 | 564.4 / 389.1 | 79–81 | 8–8 | 2–3 | 61–65 | 0.542 / 2.905 |
| 60–62.0 | 564.5 / 395.9 | 79–80 | 8–8 | 2–2 | 61–61 | 0.696 / 2.717 |

Render timing comes from a **test-only LD_PRELOAD observer**: successful `eglMakeCurrent` completion → swap completion, separately recording swap duration. It includes CPU-side drawing/submission and driver waiting, excludes preceding scene/model construction and GPU/presentation completion, and is per surface. The release executable is unchanged. This is **not whole-frame presentation latency**. The observer adds one trace fd and some measurement overhead. Logical Tokio timer cardinality is not exposed; reported timer counts are kernel timerfds, not all application deadlines. Missing mock providers perform retries, visible UI waves intentionally animate, and probes themselves request redraws: no production zero-wakeup or full-live CPU-budget pass is claimed.

## Findings and owner follow-up

**SOAK-1 — small monotonic retention; investigation, not a confirmed unbounded leak.** Across matched settled samples, private-dirty memory retained 220 KiB and NVIDIA process graphics accounting retained 5 MiB. RSS fell and fd/thread/timer counts stayed bounded. GPU accounting plateaued after the earlier step; cache/allocator high-water retention remains a plausible explanation. The analyzer flags these observations so they are not silently called leak-free. Ask the renderer owner to expose bounded cache/live-allocation bytes and repeat the retained-binary workload with a warmed, unchanged working set before assigning a cause or changing eviction. Primary owner: [hyprv-ui](../requests/hyprv-ui.md), host lifecycle follow-up: [hyprv-shell](../requests/hyprv-shell.md). No causal source line is assigned because the renderer changed during the run.

**Stall-screen false positives were checked.** The first-next-swap proxy sometimes reported long delays after control-panel close or output add. Unmapping need not cause a new swap. A separate 50 ms read-only observer saw the new output and its native bar layer in the same polling interval in 3 final hotplug cycles. Those proxy candidates are **not confirmed UI stalls**; layer registration is not pixel/presentation proof. [Rejected candidates](raw/soak/release-62m/rejected-next-swap-stall-candidates.json), [direct observer](raw/soak/release-62m/hotplug-watch.jsonl), [summary](raw/soak/release-62m/hotplug-analysis.json). No prolonged render/submission or status-liveness stall was demonstrated.

One fd-count candidate at the final matched phase was a transient socket: the following sample returned to 79. Count trends use the per-cycle quiet floor while preserving every raw maximum; this was not filed as accumulation.

The private-bus method observer began partway through the run; its recorded interval contains no provider mutation methods. Mock services also logged no forbidden Set calls. Namespace/read-only constraints covered the entire run. See [private-bus methods](raw/soak/release-62m/private-bus-methods.log), [mock events](raw/soak/release-62m/mock-events.jsonl), [measurement notes](raw/soak/release-62m/measurement-notes.json).

Cleanup completed successfully: shell/driver/helpers exited 0, the private runtime was removed, and the private-bus observer is gone. [Cleanup and mutation-audit receipt](raw/soak/release-62m/cleanup.json).

## Limits, evidence and reproduction

This covers the pinned release at DPR 1 with a small, known workload. Real PipeWire/audio, NM/BlueZ devices, artwork decoding/downloads, bus-daemon restart/reconnection, physical input, fractional/high DPR and hardware suspend are **not exercised**. It does not certify later builds, full-screen parity, production GPU memory budgets or presentation smoothness under arbitrary system load.

[Raw samples](raw/soak/release-62m/samples.jsonl), [scheduled actions](raw/soak/release-62m/events.jsonl), [full GL/timer trace](raw/soak/release-62m/trace.log), [machine-readable analysis](raw/soak/release-62m/analysis.json), [frozen launch tools](raw/soak/release-62m/tools-at-launch/), [reproduction instructions](soak-tools/README.md). To retain the exact executable, pass `--binary docs/verify/raw/soak/release-62m/hyprv-shell-release` to `run.py`; use a new output directory. `extra-input.py` supplies the actual app-side hotspots and stats click. `analyze.py` regenerates tables/plots, and `finalize.py` refuses to claim completion before a successful 60+ minute run.

Disk-policy cleanup (2026-10-23): the copied release executable was deleted after analysis. Its SHA/provenance, summarized metrics, plots and compact telemetry receipts remain.
