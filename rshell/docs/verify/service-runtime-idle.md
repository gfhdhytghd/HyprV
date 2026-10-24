# Service runtime idle attribution (2026-10-23)

**Result: service-thread CPU roughly halved; the <=0.1% whole-shell budget remains unmet (0.210% measured).**

## Method and evidence boundaries

Real release `hyprv-shell`, measured through `measure-shell.py --idle-only
--quiet-agents`, in its private nested compositor and PID namespace. Native
service actors remain enabled and read real providers with `--read-only`; agent
fixtures isolate uncontrolled process/hook activity. No live session actions,
root commands, provider writes, or installation. CPU is one-core percentage.
Pinned executable hashes and `/proc` task snapshots accompany each run under
`docs/verify/raw/perf-shell/service-idle-*`.

`service-runtime-idle.py` preserves the harness conditions and enables
unprivileged perf's `task-clock:u --user-callchains`: default kernel callchains
are denied on this host (`perf_event_paranoid=2`). Tracefs/eBPF are unavailable
without privileges. Perf sampling is for attribution, not the CPU budget.
A parallel `dbus-monitor --profile` observer records SNI signal headers only.

Temporary marked future wrappers log task type/parent, inner poll thread CPU,
polls and Rust waker notifications. These are diagnostic builds, not budget
measurements. A waker notification can coalesce; it is **not** a kernel wakeup.
`/proc/task/*/schedstat` gives exact per-thread runtime deltas; voluntary context
switches are reported separately and are also **not** exact kernel wakeups.
Unwrapped zbus executor/socket work and tracing overhead remain a shared runtime
residual, rather than being falsely assigned to one service. Raw traces record
no D-Bus bodies, hook contents or credentials.

## Findings and changes

* SNI: `NewIcon` and `NewToolTip` from the same item arrive roughly 20–40 us
  apart. Before, each can initiate a complete GetAll (or set its dirty retry),
  decode every icon byte into a generic zvariant Value, and clone unchanged
  pixmaps for publication while only scheduling the request. The first
  60-second trace contains 9 pairs and 15 GetAll tasks. Release perf samples
  corroborate generic array/value decoding and `tray::Actor::fetch`.
  `tray/wire.rs` now borrows typed byte arrays, copies at most eight validated
  pixmaps per property, discards tooltip pixmap bytes, and preserves the generic
  decoder as fallback for malformed optional property signatures. Batches of
  up to 64 already-ready signals schedule one read per item, without a timer;
  genuinely later updates retain the existing dirty retry. No-state-change
  signal handling does not publish/clone all icon pixels.
* systemd: VM/Mihomo listeners accepted unrelated unit traffic, including
  traffic admitted by a broad listener on the pooled connection. Unit streams
  now match the provider's unique owner, exact unit property path, unit lifecycle
  arguments, and JobRemoved's unit argument. The owner-change stream is installed
  before owner resolution. Generic job history remains enabled; lossless job
  queues, reconnect handling and overflow errors are retained. Mihomo requests
  the unit-specific stream. No polling or debounce timer was added.
* sysstats: the dominant remaining module is the legitimate 1 Hz sampler:
  timer -> sampler channel -> native `/proc`/sysfs snapshot -> state projection.
  Temperature/CPU/power sampling cadence is unchanged. No service spin loop was
  observed; low-rate provider signals and sampling are distinct from a spin.

## Validation

Final scoped test run: **80 passed, 5 intentionally ignored** (including the manual benchmark and live-provider tests). Strict media/sys all-target Clippy passes. The manual decode benchmark was also run separately and passes.
New tests cover paired signal deduplication, typed/legacy output equivalence,
malformed-property fallback, bounded retention after invalid pixmaps, and exact
unit filtering with a simultaneous broad listener. Existing property-flood,
unpolled-RPC and durable job-completion tests remain required gates.

A manual debug-build decode microbenchmark, 200 x 128px icon, measured generic
GetAll+model parsing 2.751 s versus typed parsing 2.530 ms. This isolates decoding,
not service/runtime CPU or a release-wide speedup; raw receipt is
`raw/service-idle/decode-benchmark-final.log` (final implementation).

## Matched module attribution

The final diagnostic ELF (`b2d675e6…`) contains temporary old-behavior switches
for only these service changes. Both runs use that **same ELF**, overlapping
60-second windows and the same observed SNI headers: 6 NewIcon + 8 NewToolTip.
The old-behavior switch restores generic byte decoding, one-signal scheduling,
broad systemd listeners and calendar retry policy. Its direct GetAll transport
is shared with the fixed case; it does not reconstruct the old PropertiesProxy
setup overhead. These are instrumented attribution numbers, **not** budget CPU.

| Module/task | Trigger and measured frequency | Before CPU ms | After CPU ms | Before → after work |
| --- | --- | ---: | ---: | --- |
| media/tray actor + GetAll children | SNI D-Bus signals / property replies | 25.109 | 2.568 | 15 → 9 GetAll tasks; 22 → 16 actor polls |
| power/sysstats + sampler | 1 Hz timer, sampler channel, synchronous `/proc`/sysfs reads | 25.181 | 25.736 | 60 samples + 60 sampler polls in both; cadence preserved |
| desktop/agent-island | leased 1.5s discovery timer → 16ms coalescing timer → blocking completion | 5.513 | 6.036 | 40 probe jobs + 120 actor polls in both; legacy late-exec/PID-reuse checks preserved |
| sys/calendar, unconfigured | reconnect backoff timer, capped at 12–15s | 0.065 | 0 | 5 → 0 polls; no retry deadline until configuration |
| desktop/applications | inotify; filtered notifications, no index job during window | 0.035 | 0.035 | 5 → 5 polls; no redundant reindex observed |
| power provider actor + signal inboxes | external D-Bus signals | 0.335 | 0.286 | 18 actor polls + 3 inbox polls in both |
| sys/systemd + VM/Mihomo | D-Bus unit/job/owner streams | 0 | 0 | no traffic in these paired windows; targeted filter regression verifies unrelated traffic is rejected |
| net, MPRIS, notifications, other desktop actors | subscribed sockets/D-Bus/inotify | 0 | 0 | no task polls in the measured windows |

Values are inner task-poll CPU; blocking probe CPU is assigned by its registered
parent task and worker thread. Polls/waker notifications are coalesced executor
activity. The agent's three polls per discovery period are distinct deadline /
coalescing / join-completion wakes, not three scans. The typed decoder was
entered for every GetAll in the fixed trace, with **zero generic fallbacks**.
The earlier apparent candidate was excluded: its cached release media depfile
omitted `tray/wire.rs`. Package debug overrides forced a fresh diagnostic
library fingerprint; the pinned ELF marker verified actual decoder inclusion.

An earlier 60-second baseline had 107 systemd signal-drainer polls (35 + 24 +
24 + 24), including VM/Mihomo streams receiving unrelated unit invalidations.
Traffic disappeared in the matched windows, so **no measured CPU saving is
assigned to systemd**. The private-peer regression injects unrelated properties
while a broad receiver shares the connection; the narrow receiver remains
pending, then delivers its actual unit's change. Existing queued JobRemoved /
property-flood / unpolled-RPC regressions pass.

The initial trace missed `ctx.rt.spawn` actors and explicit `spawn_blocking`
closures. The matched capture adds both; earlier claims of no agent activity
were incomplete. The final source contains none of those wrappers or experiment
branches (`raw/service-idle/trace-cleanup-search.log` is empty).

Diagnostic service-thread CPU fell 92.503 → 69.643 ms/60s. Inner wrapped work plus
blocking jobs accounts for about 57 → 35 ms. Approximately 42 → 41 ms remains
unallocated among shared zbus/reactor work and tracing overhead; it cannot be
honestly charged to one module. Unprivileged perf corroborates the original
array-decoding hotspot; it does not provide an exact kernel wakeup census.

## Normal release comparison

Both normal ELFs have no tracing wrappers. The fixed ELF is
`777b2a2954637e47b7d70fc4f0dbe1e5abdfbeaea42f464bf6a5529cd12a85fb`;
the old normal checkpoint is `c255265a…`. Overlapping 90s controlled-quiet windows
observe the same 15 NewIcon + 15 NewToolTip signals. No tasks started/exited during
either measured window; precise summed task runtime agrees with the sampled
`/proc` task counters. The coarse CPU-tick estimates differ slightly.

| Thread | Before CPU ms / 90.149s | After CPU ms / 90.148s | Before → after one-core CPU |
| --- | ---: | ---: | ---: |
| hyprv-services | 85.465 | 42.889 | 0.0948% → 0.0476% |
| tokio-rt-worker (five blocking workers combined) | 3.850 | 3.964 | 0.0043% → 0.0044% |
| hyprv-shell (UI) | 178.776 | 142.201 | 0.1983% → 0.1577% |
| hyprv-pw-audio / module-rt | 0 / 0 | 0 / 0 | 0 → 0 |
| Total | 268.091 | 189.053 | **0.2974% → 0.2097%** |

Service-thread CPU is down 49.8%; the total budget **does not pass** in this
comparison. UI CPU alone exceeds 0.1%. Whole-source checkpoints include concurrent
owner changes; only the same-ELF traced service comparison isolates this patch.
The UI difference is not assigned to these service changes or described as a
proven renderer regression. A standalone normal-ELF budget run below checks
the overlapping-compositor effect.

A separate unprivileged 7999 Hz user-only after profile (not a budget run) has no
lost samples. The service samples are spread across allocations, watch/notify,
sysstats, reactor work and D-Bus signatures; the original generic icon-array
hotspot no longer dominates. Some records have no usable callchain / unknown
symbols, so those are left unallocated. UI samples include `__vdso_clock_gettime`
(17.44% of all samples) and unsymbolized `libnvidia-eglcore.so.615.78.08` locations;
these are UI-thread samples, not evidence of a service timer spin. No kernel
stack, blocked-time or causal driver claim is possible from this user-only data.


The final standalone, uninstrumented 90.110 s run of the **same normal ELF** gives:
UI 142.739 ms (0.1584%), services 42.503 ms (0.0472%), blocking workers 3.911 ms
(0.0043%), PipeWire/spectrum threads 0. Total 189.153 ms = **0.2099%**.
No thread creation/exit occurred during the window. The coarse process-tick
estimate is 0.2219%; the precise per-task runtime is used for the conclusion.
The overlapping-compositor effect therefore does not explain away the residual.

The measured service fixes are complete, but the requested global target is
**not achieved**. Remaining required sampling/discovery is preserved, and no
additional service spin was demonstrated. Engine receives the UI runtime/profile
receipt in `docs/requests/hyprv-ui.md`; syscall/kernel wakeup attribution remains
unavailable without tracefs privileges. Neither a driver diagnosis nor a claim
that every remaining CPU microsecond is allocated is made.

Final validation receipts: `raw/service-idle/final-tests.log`,
`final-clippy.log`, `decode-benchmark-final.log`, `trace-after-build.json`,
`normal-after-build.json`, and `service-idle-attributed-{before,after}-attribution.json`.
The normal build verifies actual decoder symbols and absence of diagnostic
symbols; a source search confirms all 90 wrappers and experiment branches removed.
Only the current normal ELF is retained in `target/service-idle/control-after/`.
Retired diagnostic/comparator ELFs freed 229,058,600 bytes (~218 MiB); raw perf
recordings were reduced to header/hash receipts and compact symbol/stack reports.
No new Cargo target directory was created.


Permanent implementation edits are confined to media `tray/mod.rs` + new
`tray/wire.rs`, and sys `systemd.rs`, `mihomo.rs`, `support.rs`, `calendar.rs`.
Owner requests document every change. Core, net, power and desktop code have
only had the temporary instrumentation removed; their existing behavior is
preserved. The diagnostic helper/install/remove scripts remain outside the
crate graph in `docs/verify/scratch/service-idle/` for audit; the original capture
also used the temporary before-mode patches described above, which are no longer
present in source.
