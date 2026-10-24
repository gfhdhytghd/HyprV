# Adversarial re-review: incremental agent process discovery

**Implementation follow-up:** all findings in this report are fixed locally; see [fixes and regression evidence](../verify/discovery-icons-r2-fixes.md). Original review evidence below describes the pre-fix checkpoint.

Reviewed 2026-10-23. Production code was not edited. [Evidence and reproduction](scratch/discovery-icons-reconcile/README.md), [expected-failing process probes](scratch/discovery-icons-reconcile/process-results.json), [source fingerprints](scratch/discovery-icons-reconcile/source-hashes.json).

Reviewed ProcTable's PID-window accounting, pidfd ownership, identity reads, ancestry, sweep scheduling, actor refresh/watch lifetimes and incremental file-state mapping. The findings below are deterministic discovery errors. No additional fd leak or kernel PID-reuse race is claimed as reproduced. The legacy reference reads every PID's stat on each scan (`../quickshell/scripts/agent_island.py:470`, especially lines 481–492); `docs/spec/services.md:144` preserves subscriber-scoped process discovery with the legacy scan cadence.

<a id="desktop-r2-1"></a>
## DESKTOP-R2-1 [P2] Reused or reparented non-agent ancestors produce false active Codex sessions

**Location:** `crates/hyprv-svc-desktop/src/agent_island/model.rs:1149` (recheck selection), `model.rs:1294` (codex_ancestor).

**Failure scenario:** the initial table contains Codex PID 10 and a shell PID 20 parented to it. PID 20 exits and is reused by a shell parented to init; that new shell starts a matching systemd-inhibit PID 30. The PID-window probe reads 30 but retains PID 20's old start time and ppid. Discovery follows the cached 30→20→10 chain and reports Codex 10 active even though the inhibitor is unrelated. Reparenting a surviving intermediate shell has the same stale-edge problem. The error can persist until the 60-second sweep. The range endpoint cannot validate cached ancestry, and agent-only pidfds do not track the shell's identity.

**Verification:** `probe_processes.py`, `reuse`: at time 101.5, incremental discovery is `[(10,5,"codex")]`; a fresh full scan of the identical filesystem is `[]`. The expected-equivalence assertion fails. The probe actually changes PID 20's start time from 6 to 99 as well as its ppid; no probabilistic real PID reuse is required.

**Suggested fix:** re-read and validate start time/ppid along every inhibitor ancestor chain before attributing it to Codex, rejecting missing/replaced links and retrying inconsistent snapshots. Remove stale entries as identities change. A full sweep remains useful housekeeping but must not be the correctness mechanism for an active ancestor chain. Add the fake-proc reuse and reparenting sequences as regressions.

<a id="desktop-r2-2"></a>
## DESKTOP-R2-2 [P2] Initial-snapshot processes miss even an immediate fork/exec transition

**Location:** `crates/hyprv-svc-desktop/src/agent_island/model.rs:1132` and `model.rs:1153`.

**Failure scenario:** the first scan catches a newly started shell just before it execs Claude. The initial sweep overwrites every first_seen with `f64::MIN`, excluding these PIDs from the next-probe young-process recheck. Exec preserves the PID, so `/proc/loadavg` need not advance. A Claude process that execs immediately after the initial snapshot stays absent until the full sweep, potentially nearly 60 seconds; a short session may never appear. Older non-agent PIDs execing agents later have the broader same limitation. The source explicitly documents the sweep tradeoff, but that does not preserve the legacy 1.5-second full-scan result.

**Verification:** `probe_processes.py`, `exec`: initial PID 10 is bash at time 100; the same PID/start time becomes Claude before time 101.5, with an unchanged allocation endpoint. Incremental discovery is `[]`, while a fresh scan immediately yields `[(10,5,"claude")]`. This is an expected-failing equivalence test of actual extracted production methods.

**Suggested fix:** retain a next-probe recheck for initial-snapshot PIDs as well as newly observed ones. To preserve the wider late-exec contract, use an exec event source where available or reconcile relevant process identities at the promised discovery cadence; explicitly revising the parity/latency contract would require a product decision. Keep scanning subscriber-scoped and avoid adding an idle heartbeat.
