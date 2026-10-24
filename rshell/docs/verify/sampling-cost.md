# Sysstats and agent-discovery sampling cost (2026-10-23)

**Zero-allocation steady sampling is implemented. Paced sysstats is ~99 us
(previously ~420 us); paced quiet agent refresh is ~24.5 us. The 50 us
sysstats target and 0.1% whole-shell target remain unmet. Service-thread CPU
fell in both comparisons; total CPU did not improve consistently.**

## Scope and behavior

Optimized the native read-only sampling paths. No installation, root command,
provider action, or live shell/compositor restart. All builds share `target/`
with incremental compilation disabled. Owner coordination is appended to the
core, power, desktop, shell and UI adapter request files.

* `hyprv-core::sample_file` retains bounded buffers and descriptors, using
  positioned reads from offset zero. Failed reads rearm once; synthetic files
  can detect replacement. A retained directory FD/getdents buffer supports the
  agent's periodic reconciliation without allocating a directory iterator.
* Sysstats parses aggregate CPU and only the eight core tiles QML displays,
  required memory fields, and requested sensor metrics. Inline ordered rings
  and shared interface strings make snapshot clones allocation-free. Wire
  histories remain ordered JSON arrays; interface remains a string/null.
  `elapsed` alone no longer publishes; actual values, ordered histories and
  counter-generation changes still do. Backend futures are statically dispatched
  without a per-call async-trait box.
* Shell visibility now acquires typed metric leases. The visible bar and control
  panel keep temperature/package-power, which their QML actually displays;
  expanded stats additionally requests per-core samples. Hidden consumers drop
  demand; hardware discovery remains event-driven.
* As explicitly authorized, the selected interface's cached sysfs RX/TX byte
  counters replace the expensive all-interface proc read, with `/proc/net/dev`
  fallback for missing/malformed/unavailable sysfs counters. Selection remains
  **the first UP IPv4 default entry in `/proc/net/route`, in file order**;
  there is no new metric preference or IPv6 fallback. The existing rtnetlink
  link/route subscription is established before the initial snapshot. Its event
  branch invalidates both selection and counter handles; regular ticks do not
  poll route selection. Suspend/reset and reconnect recovery also invalidate it.
  Unsafe interface path components cannot be used for sysfs traversal.
* Agent discovery retains bounded proc readers and reuses argv, PID, poll and
  discovery-key storage. It still checks cached identities/parents and named
  process argv each leased probe, retains pidfd/start-time guards and the 60 s
  sweep, and follows late exec/reparent/PID reuse. A single sleeping
  `hyprv-agentproc` worker exchanges the table through a reusable slot, replacing
  per-probe boxed blocking jobs/path clones. Thread creation failure preserves
  the old blocking-pool fallback. Discovery changes may allocate; steady state
  does not. Cached stat/cmdline descriptors have an RLIMIT-derived budget.

## Evidence and limits

`raw/service-idle/sampling-tests.log` contains core/power/desktop regression
results. Tests count alloc/alloc_zeroed/realloc on the sampling thread only;
no claim is made about kernel allocation or unrelated runtime tasks. Positive
controls verify that the counter detects the old allocating CPU parser. Warm
sampling includes snapshot cloning and Topic publication; history rotation and
elapsed-only publication are covered. Agent tests include named agents and
periodic sweeps, process-identity scenarios, replaced fixture directories, and
worker completion/drop. A stable non-UTF-8 argv case exposed 162 allocations
before the final fix; cached lossy comparisons now avoid allocating replacement
strings (failure receipt: `sampling-nonutf8-before.log`).

`sysfs_equals_legacy_counters_and_rates_and_route_selection_is_event_driven`
compares the proc and sysfs paths at identical timestamps, including counter
rollback, histories, rates and generation. It tests first-UP/file-order route
selection and calls the same invalidation hook as Native's rtnetlink branch.
A separate fallback/traversal test covers malformed sysfs and unsafe names.
`sampling-network-equivalence.json` records 100 additional real read-only
comparisons for the current default interface: sysfs-before <= legacy proc <=
sysfs-after, accounting for traffic between reads. No privileged route changes
were performed.

The cached full `/proc/net/dev` floor was 99.4 us on this host: the non-default
`eno1` driver's statistics query cost about 92 us, while selected `br0` RX/TX
sysfs reads together cost about 0.6 us. Receipts: `sampling-pread-floor.log` and
`sampling-net-sysfs-floor.log`; reproduction source under
`scratch/sampling/`. Reading every interface cannot meet 50 us here.
The fallback preserves correctness rather than promising that budget for every
possible driver. Existing unavailable RAPL permissions remain unavailable;
they were not changed. Temperature/power requests and failure semantics are
preserved.

Release microbenchmarks use `CLOCK_THREAD_CPUTIME_ID` and production sampling
code, with compile-time-test-only allocation instrumentation. They exclude
startup/discovery, and the agent quiet benchmark uses the same private PID
namespace as controlled-quiet shell measurement. A busy host with changing or
many cached processes is not claimed to cost the same as this quiet namespace.
The earlier ~0.42 ms sysstats and ~56 us agent figures came from the previous
production attribution run; the microbenchmark is a more focused measurement,
not an identical instrumentation setup.

## Results

The target is **not met at real cadence on this host**. Removing allocations and
all-interface driver queries reduces work substantially, but the required sensor
and proc kernel reads retain a cold-read floor. Tight-loop results must not be
substituted for the paced result.

| Sampling CPU per iteration | Before | After | Allocations after |
|---|---:|---:|---:|
| Sysstats, previous production attribution / new 1 Hz paced mean | ~420 us | 98.866 us | 0 |
| Sysstats, optimized tight-loop mean | n/a | 15.447 us | 0 |
| Agent discovery, previous blocking job / new 1.5 s paced refresh | ~56 us | 24.477 us | 0 |
| Agent discovery, optimized quiet tight-loop refresh | n/a | 1.682 us | 0 |

The paced sysstats run uses 60 one-second sleeps **outside** each measured
sample; mean 98.866 us, p50 99.922 us, p99 119.853 us. It includes parsing,
state cloning and Topic publication. Histories remain live; no sensor or timed
presentation was disabled to meet the target. Receipts:
`sampling-paced-benchmark.log`, `sampling-paced-artifact.json`.

A separate 60-sample paced C probe with retained FDs, no Rust parser or allocator,
measured these syscall CPU costs (means; host load/core placement can vary):

| Source | CPU per read |
|---|---:|
| `/proc/stat` | 38.409 us |
| `/proc/meminfo` | 8.450 us |
| selected `br0` RX sysfs | 6.159 us |
| selected `br0` TX sysfs | 2.672 us |
| required preferred thermal-zone sensor | 74.923 us |

The thermal read alone exceeds 50 us at this cadence; its p50 is 79.200 us,
p99 98.874 us. These separate-probe means are not additive substitutes for the
Rust measurement. Source/receipt: `scratch/sampling/pread-paced.c` and
`raw/service-idle/sampling-paced-pread.log`. Changing the sensor source, its
freshness, or the displayed bar values was not authorized and was not done.

The agent paced run uses 40 sleeps of 1.5 s outside each measured refresh, in
its private quiet PID namespace: mean 24.477 us, p50 24.286 us, p99 27.319 us,
zero allocations. Reported benchmark percentile order statistics use index
`n/2` for p50 and `floor((n-1)*0.99)` for p99. It includes the periodic full sweep and keeps the legacy
identity/argv checks. This is the refresh body; scheduling/handoff CPU is also
included in the separate whole-shell worker totals below. Receipts:
`sampling-agent-paced-benchmark.log`, `sampling-agent-paced-artifact.json`.

## Normal-release controlled-quiet measurement

Both executables were pinned,
non-instrumented release builds, run serially through `service-runtime-idle.py`
(the unchanged `measure-shell.py` nested compositor and private-PID-namespace
conditions, plus a read-only SNI signal-header observer), with 90 s measured
windows following startup warmup. No perf profiler, tracing, build, or other
sampling probe runs during the final pair. Percent is summed per-thread
`/proc/*/schedstat` runtime divided by wall time, expressed as one-core CPU;
coarse utime/stime tick results are retained separately. The new
`hyprv-agentproc` thread is counted, not hidden as offloaded work.

Initial corroborating pair (before the final non-UTF-8 argv fix): 0.1817% ->
0.1576% summed CPU, service + blocking/discovery workers 0.0484% -> 0.0231%.
The before window overlapped the tiny read-only C cold-read probe, so the final
independent pair below is the primary budget evidence.

| CPU (one-core percentage) | Initial before | Initial after | Final before | Final after |
|---|---:|---:|---:|---:|
| UI thread | 0.1333% | 0.1345% | 0.1608% | 0.2253% |
| Tokio service thread | 0.0440% | 0.0208% | 0.0477% | 0.0290% |
| Blocking/discovery workers | 0.0044% | 0.0023% | 0.0044% | 0.0024% |
| Services + blocking/discovery | 0.0484% | 0.0231% | 0.0521% | 0.0314% |
| Whole shell | 0.1817% | 0.1576% | 0.2130% | 0.2567% |

Final windows: 90.112 s before, 90.256 s after. Service + discovery CPU fell
about 40% in the final pair (about 52% in the initial pair). The final whole
shell grew because UI runtime grew from 144.929 ms to 203.355 ms; the cause of
that UI variation is **unattributed**, not claimed as a service regression or
improvement. No UI rendering code was changed by this task; the working-tree
UI hashes are identical between the two optimized runs. UI runtime alone
exceeds 0.1% in these windows. Neither a stable whole-shell reduction nor a
<=0.1% budget pass is established. The owner's UI request file has the numbers
for follow-up; this task does not guess the cause.

The final old/new ELF hashes are `777b2a2954637e47b7d70fc4f0dbe1e5abdfbeaea42f464bf6a5529cd12a85fb`
and `3bfe7ca65676a46b74dd97e5fbdcef3c8d46bcba822e8f96f85014721b85e0dd`.
Both harness receipts have `instrumented: false`, quiet fixtures and zero hook
events. Full inputs/results are in `raw/perf-shell/service-sampling-{final-before,final-after}/`;
`raw/service-idle/sampling-summary.json` collects per-thread runtime and coarse
tick CPU. The source manifests describe the working tree at launch, not a
reconstruction of the older pinned ELF's source; its earlier attribution/build
receipt is in `service-runtime-idle.md`. Thus these are actual pinned-binary
observations, not an isolated source-level causal proof for every changed
component in the shared checkout.

Reproduce normal measurement (substitute a retained or rebuilt release ELF):

```sh
HYPRV_SHELL_BINARY=/absolute/path/to/hyprv-shell DEBUGINFOD_URLS='' \
  python3 docs/verify/service-runtime-idle.py --label service-sampling-repeat \
  --seconds 90 --idle-only --quiet-agents
```

Paced benchmark switches are `HYPRV_SAMPLE_PACED=1` for the ignored power
`real_sample_cpu` release test and `HYPRV_PROBE_PACED=1` plus
`HYPRV_PROBE_QUIET_NAMESPACE=1` for the desktop `real_probe_cpu` test under
`unshare --user --map-current-user --pid --fork --mount-proc`. Exact executable
hashes/commands accompany their receipts. The old scratch comparison ELF and
duplicate current ELF were retired after the measurements; small receipts,
probe sources and logs remain. The shared `target/release/hyprv-shell` was
already replaced by another build by cleanup (hash
`8e4ee19d0912b914574a8443500bca81d2449c586bcf1bdd7c6bbc3a0f506f62`);
it is **not** the executable measured above. The harness verifies and records
the executed ELF hashes, independently of this mutable shared output path.

## Validation

* Core 17, desktop 74, power 26 tests pass (117 total; two manual benchmarks
  ignored in the normal suite, both run separately). Stable named agents,
  non-UTF-8 argv, history rotation, route/counter resets, source fallback,
  directory replacement, leases and unavailable sensors have regression tests.
* Strict all-target Clippy passes for core, power, desktop, shell, bar and panels.
* The bar and panel CPU fixture replays complete; panel corpus stress reports
  zero mismatches. The CPU-backend Qt comparison
  reports bar 32/312, panels 100/300 pass (bar includes eight unreachable states
  excluded from gating). These CPU replay differences are separate from the
  renderer owner's passing production GL receipts (19:46 UTC update in the UI
  requests files; `docs/verify/corner-precision.md`). This task reran CPU fixtures,
  not the GL gate.
  Receipt: `sampling-parity.log` and `parity/verify/out/{bar,panels}/render.log`.
* Release artifacts, expected sampler/worker symbols, source hashes and command
  receipts: `sampling-release-artifacts.json`, `sampling-*-artifact*.json`.
  Test allocation instrumentation is compile-time-test-only, not shipped in
  either measured shell executable.

