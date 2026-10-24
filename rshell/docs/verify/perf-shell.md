# Real assembled shell performance

Updated 2026-10-24T05:52:41+00:00. Selected run tag: `emergency-20261024t052816z`.

**Idle budget: NOT MET.** the controlled zero-agent-input release session uses **0.1828% process CPU**, **0.1909% summed thread CPU**, versus ≤0.1%. Host process discovery with external hooks frozen uses 0.4646%. The actual live-hook idle interval uses 0.9974%; its 41 hook-file notifications are reported separately, not blindly subtracted.

## Setup and scope

- This is the actual `hyprv-shell` crate, release opt-level 3, fat LTO, one codegen unit, panic abort. It runs the normal production assembly, retained EGL/Skia UI and service graph. It is not the service probe, a rendering benchmark, `--headless` compatibility host, or a short fixture scene.
- `--read-only --no-exclusive` exposes the existing assembly read-only parameter: no native control actions or network-agent registration. Session/system service buses stay live. Shell IPC, cache and state directories are private; agent hook files are observed without writing them. PipeWire uses the original runtime directory. The nested Hyprland service socket targets only the private instance.
- A private D-Bus / virtual KWin parent hosts nested Hyprland. Native measured output and layers are verified at **6144×3456 physical, scale 2, 3072×1728 logical, 60 Hz**, matching the live DP-4 geometry. Only private KWin geometry is changed. A sacrificial read-only client can establish the output before measured clients start. No host compositor configuration, Quickshell UI, service or device is changed.
- The shell uses its EGL path and appears as a graphics process on the NVIDIA RTX PRO 6000 Blackwell. NVIDIA per-process graphics allocations come from `nvidia-smi -q -x`; these are not total desktop GPU memory. The nested compositor may use software rendering and has no host blur/HDR rules; compositor processes are excluded from both application-family comparisons.
- Native initialization/provider hydration is excluded (12-second warmup). Each headline interval is ≥60 seconds. Activity sends 12 successful UI-only IPC calls, alternating calendar and control-panel open/close every five seconds. Legacy Quickshell receives no injected activity; its same-window background observation is not an equal-workload activity benchmark.
- The host is already native. Its running shell supplies environment only and is never driven or restarted. No same-window legacy comparison is available; historical legacy figures are not presented as current measurements.

## Same-window CPU, scheduling and memory

| Window / family | CPU % process (thread runtime) | Sched / s | Context switches / s | RSS MiB start → end / sampled max | GPU MiB start → end |
|---|---:|---:|---:|---|---|
| Controlled quiet (private PID namespace) / Rust | 0.1828 (0.1909) | 21.26 | 21.26 | 183.2 → 182.3 (sampled max 191.3) | 257.0 → 257.0 |
| Frozen hooks / host discovery / Rust | 0.4646 (0.4646) | 21.29 | 21.29 | 192.3 → 191.6 (sampled max 192.5) | 259.0 → 259.0 |
| Live-hook idle / Rust | 0.9974 (0.9352) | 31.29 | 31.29 | 211.9 → 173.6 (sampled max 212.1) | 259.0 → 259.0 |
| Typical native UI activity / Rust | 1.4637 (1.4700) | 29.34 | 29.34 | 205.0 → 234.0 (sampled max 259.8) | 259.0 → 259.0 |

Scheduling timeslices (`/proc/PID/task/TID/schedstat`) and voluntary/involuntary context switches are **wakeup proxies**, not exact `sched_wakeup` events. CPU process ticks include reaped child time; native thread runtime is the higher-resolution budget value when no threads exit. Endpoint task counters miss short-lived legacy children; exited/new task counts are retained in raw data. RSS is sampled about once per second; GPU memory is measured at endpoints only, not as a peak.

## Agent events and eligibility

| Native interval | Native hook-file notifications | Notifications with observed non-ProcessScan session changes | Host hook notifications seen by observer | Agent inputs |
|---|---:|---:|---:|---|
| Controlled quiet | 0 | 0 | 67 | quiescent agent fixture + private PID namespace; all actors retained |
| Host discovery without hooks | 0 | 0 | 74 | frozen hook fixture; host process discovery and agent animations retained |
| Live idle | 41 | 36 | 41 | live hooks + host process discovery |
| Activity | 32 | 32 | 32 | live hooks + host process discovery |

A file notification is not automatically a genuine tool/hook event: the legacy process scanner also rewrites state. The observer compares hashed session fields and classifies ProcessScan-only changes; coalesced file notifications can conceal intermediate states, so semantic counts are observations/lower bounds, not an exact event ledger. No session contents or tool arguments are copied into the event classification data.

The controlled quiet session uses an empty agent-state fixture, absent app-server endpoint and a private PID namespace. Every service actor remains compiled and started, but process-dependent observations/discovery differ. The host-discovery session freezes only external hook inputs and retains real `/proc`; native process discovery and agent animation remain active. Neither isolated interval is silently accepted as a full-host idle pass. The controlled case alone cannot certify a full-host pass; current numeric status is shown above.

## Historical investigation (not current-candidate causal proof)

1. **Fixed Shared-slot dirty feedback (`hyprv-ui-bar`).** `content_equal` lacked `Content::Shared`, so the same cached island `Rc<Node>` was unequal every reconciliation. Each paint scheduled another dirty frame. The comparator now uses `Rc::ptr_eq`; a changed tree still damages the slot. The regression fails before (one edit instead of zero), then all 17 bar behavior tests pass. [Before](raw/perf-shell/shared-slot-before.log), [after](raw/perf-shell/shared-slot-after.log).
2. **Fixed clip canonicalization feedback (`hyprv-shell`).** The host compared a new unsnapped tree against the retained snapped tree and then finalized clips again. A 120.15625-wide center Rectangle alternated with a 120-wide Group plus a Rectangle child. A frozen diagnostic copy records the repeated mismatches. The host now canonicalizes before initial mounting and reconciliation. The regression fails before (four edits on an equal model), verifies unchanged updates stay clean and genuine stats still damage, and all 24 shell library tests pass. [Before](raw/perf-shell/clip-before.log), [after](raw/perf-shell/clip-after.log), [diagnostic trace](raw/perf-shell/damage-trace/shell-damage-trace-idle.log).
3. **Historical periodic discovery (`hyprv-svc-desktop`).** Earlier candidates scanned host `/proc` every 1.5 seconds and renewed synthetic activity. The owner subsequently landed B2 process-discovery optimizations. This report measures the selected candidate afresh; it does not attribute its residual CPU to the former scan loop. Sequential frozen-hook and controlled windows are observations, not a causal subtraction. [Request](../requests/hyprv-svc-desktop.md).
4. **Historical UI hot paths (`hyprv-shell` / `hyprv-ui-bar`).** Earlier controlled candidates spent most CPU in main-thread update/paint, including full bar reconstruction on callbacks and 1 Hz stats. Subsequent owners changed these paths. Current per-thread counters can localize CPU by thread, but this uninstrumented run does not separately measure tree construction, layout, painting or prove the previous diagnosis still applies. [Shell request](../requests/hyprv-shell.md), [bar request](../requests/hyprv-ui-bar.md).

## Before / after integration observations

| Release case | CPU % process | Sched / s | Native hook events |
|---|---:|---:|---:|
| Original controlled quiet | 5.8278 | 113.77 | 0 |
| After Shared fix, before clip fix | 4.7394 | 90.20 | 0 |
| Selected final controlled quiet | 0.1828 | 21.26 | 0 |

These whole-binary runs are sequential integration comparisons, not a one-line-only causal experiment: other agents changed renderer/shell code concurrently, and early runs lacked the PipeWire runtime override. Original preliminary runs hashed a mutable target path; final selected binaries are archived and immutable, and later refreshes hash the actual procfs executable as well. The final candidate is archived with frozen/source receipt fingerprints. The two failing-before/passing-after regressions isolate the actual dirty-feedback defects. Earlier 1280×720 setup trials and instrumented/debug/profile attempts are retained as diagnostics and excluded from headline results. Perf counting worked, but sampling-record permission was denied; no sysctls or privileges were changed.

## Reproduction and provenance

- Selected ELF: `/home/wilf/.config/HyprV/rshell/cutover/evidence/emergency-gate-refresh/hyprv-shell`; SHA-256 `c364f556a1a5168a7bcac866616e1912a87b06951b7ac321b2a7a0f22d1d52df`.
- Inputs / equivalent release command: [source snapshot](raw/perf-shell/emergency-20261024t052816z-live/emergency-20261024t052816z-live-sources.json). Initial pinned build: [log](raw/perf-shell/frozen-release-build.log). Subsequent CI refreshes use a hash-verified staged production release.
- Harness: `python3 -B docs/verify/measure-shell.py --label NAME` (live 60s + activity 60s); `--quiet-agents --idle-only` (controlled session); `--quiet-hooks --idle-only` (host discovery without external hook writes). `HYPRV_SHELL_BINARY` selects the archived real binary; source snapshots/receipts accompany it. The harness creates and stops only its own private processes.
- Raw selected runs: [quiet](raw/perf-shell/emergency-20261024t052816z-quiet/emergency-20261024t052816z-quiet-idle.json), [host discovery](raw/perf-shell/emergency-20261024t052816z-no-hooks/emergency-20261024t052816z-no-hooks-idle.json), [live idle](raw/perf-shell/emergency-20261024t052816z-live/emergency-20261024t052816z-live-idle.json), [activity](raw/perf-shell/emergency-20261024t052816z-live/emergency-20261024t052816z-live-activity.json).
- Machine-readable real-shell gate input: [perf-shell-latest.json](raw/perf-shell-latest.json). Preflight now prefers this evidence over service-probe serialization measurements, validates archived ELF and UI/service input hashes, and rejects debug/instrumented/short or isolated evidence as a full-host pass. A controlled quiet failure also blocks the budget. [Gate regressions](raw/perf-shell/preflight-tests.log).

No install, cutover, host reload, legacy restart, privileged helper activation or device-control action was performed.

## Hourly candidate history

| Candidate tag / source snapshot | Quiet thread CPU % | Host discovery without hooks CPU % | Live-hook thread CPU % | Live hook-file notifications |
|---|---:|---:|---:|---:|
| [ci-20261023t165719z](raw/perf-shell/ci-20261023t165719z-quiet/ci-20261023t165719z-quiet-idle.json) / `36e621a706c0dce8` | 0.1686 | 1.1207 | 1.4347 | 44 |
| [ci-20261023t174038z](raw/perf-shell/ci-20261023t174038z-quiet/ci-20261023t174038z-quiet-idle.json) / `472960c669016923` | 0.2060 | 1.1744 | 1.3719 | 21 |
| [ci-20261023t182400z](raw/perf-shell/ci-20261023t182400z-quiet/ci-20261023t182400z-quiet-idle.json) / `2b7f008d4baeab9f` | 0.2411 | 1.2701 | 1.4448 | 21 |
| [ci-20261023t184907z](raw/perf-shell/ci-20261023t184907z-quiet/ci-20261023t184907z-quiet-idle.json) / `ebf8e52e0c81319a` | 0.1396 | 1.2701 | 1.6502 | 59 |
| [ci-20261023t190637z](raw/perf-shell/ci-20261023t190637z-quiet/ci-20261023t190637z-quiet-idle.json) / `e096e02b4d2860b0` | 0.2103 | 1.3379 | 1.5026 | 30 |
| [ci-20261023t193744z](raw/perf-shell/ci-20261023t193744z-quiet/ci-20261023t193744z-quiet-idle.json) / `8bda9bfd2aa2388e` | 0.1998 | 1.1503 | 2.1389 | 126 |
| [ci-20261023t200936z](raw/perf-shell/ci-20261023t200936z-quiet/ci-20261023t200936z-quiet-idle.json) / `fd0d1ba122713757` | 0.1320 | 0.9861 | 1.1974 | 29 |
| [ci-20261023t202040z](raw/perf-shell/ci-20261023t202040z-quiet/ci-20261023t202040z-quiet-idle.json) / `2fb4b4a85ae0e9e3` | 0.2201 | 0.9953 | 1.0246 | 11 |

Only complete >=60s uninstrumented release windows from one ELF per candidate enter this history. Hook traffic is separate; differences between sequential windows are not exact event costs. Source snapshots are deleted after evidence is summarized under the disk policy; source hashes and command receipts remain. Only the selected measured ELF must remain available for the current performance gate.


## 2026-10-24 installed d64 acceptance boundary

The exact installed c364f556 ELF meets the recorded user bounds: controlled quiet
0.190864% (0 hooks) and live-hook idle 0.997420% (41 hook-file events). The 0.1%
aspiration above remains unmet. This fresh binary measurement does not certify
concurrent runtime source edits; installed preflight still rejects source freshness.
See [the gate report](emergency-gate-refresh.md) and
[standalone accepted-bounds validation](raw/emergency-gate-refresh/accepted-perf-bounds.json).
