# Installed d64 gate refresh and visual acceptance

2026-10-24. **NOT READY against the current development checkout.** No build, install,
live shell/helper restart, Hyprland reload, sudo or Git commit was performed in this refresh.
The user independently installed d64 before the post-install baseline was taken.

Release: `d64aecf167b9b86e3744ae8d5a9ddaac37247c5f15a2a64d7fc259bb8035435a`.
Staged, archived measured, and installed shell ELF: `c364f556a1a5168a7bcac866616e1912a87b06951b7ac321b2a7a0f22d1d52df`.

## Gate results

| Installed preflight row | Status | Detail |
|---|---|---|
| release paths | PASS | stable deployment workspace and preflight exist: /home/wilf/.config/HyprV/rshell |
| release binaries | BLOCKED | candidate source changed since release build |
| runtime assets | PASS | 44 runtime payloads and catalog hash-verified; unit asset root configured (installed) |
| automated tests | BLOCKED | automated test receipt failed or stale; run preflight --run-tests |
| parity gate | PASS | accepted: user 2026-10-24; user accepted visual parity; no re-render required; last fully passing evidence 6719efa8ffa9c3d3; exact shell SHA-256 verified |
| performance gate | FAIL | real-shell performance source evidence stale; rerun perf-shell measurement |
| privhelper installed | PASS | True |

Exact command output: [preflight-installed.txt](raw/emergency-gate-refresh/preflight-installed.txt);
[JSON](raw/emergency-gate-refresh/preflight-installed.json). Both commands exited 1.

## What changed

`cutover/parity-acceptance.json` records the user decision dated 2026-10-24, the
exact accepted ELF, and SHA-pinned last fully passing preflight/release receipts
from 6719efa8 (the same shell ELF). `preflight.py` accepts that record instead of
requiring visual re-renders. Wrong/missing/tampered evidence, ambiguous/red prior
parity rows and a changed shell hash fail closed. All eight new acceptance tests
pass. Removing the acceptance restores the strict gate; parity tools remain runnable
on demand with their missing-image and non-text exactness checks intact.

Stopped only the two owned parity producers and descendants; retained canonical
references and historical acceptance receipts, removed unfinished generated outputs
and private compositor scratch. [Stop audit](raw/emergency-gate-refresh/parity-stopped.json).
A cancelled fullscreen receipt is explicitly cancelled, not reported as a fresh PASS.

The original d64 build receipt and release manifest are unchanged. The narrowly
SHA-pinned readiness amendment covers the three acceptance implementation/fixture
files only. New runtime edits and laptop draft inputs are not exempted.

## Automated tests and concurrent source drift

All five configured commands returned zero: 620 workspace tests, 2 documentation
tests, 1 xtask test, 135 cutover tests and 4 probe tests. Existing opt-in ignored
tests remain ignored (53 all-targets, 1 doc); no tests were relaxed or skipped.
The full receipt correctly has status fail because inputs changed during the run.
An earlier idle regression failure (zero paints, one dispatch but 19 voluntary
context switches) and its passing same-source, correctly asset-configured retry
are retained and filed with the engine owner. The final full run passed that test.

The initial d64-versus-6719 audit found only the intended two cooling Rust files
changed among compiled inputs; 1458 engine/shared inputs were identical then.
During the later acceptance run, concurrent engine/UI, svc-desktop and svc-power
edits appeared. This refresh does not claim those unbuilt edits are present in d64
or certified by its older evidence. Exact paths/hashes are in
[release-source-drift.json](raw/emergency-gate-refresh/release-source-drift.json);
the earlier run race and per-command results are in
[post-test-source-drift.json](raw/emergency-gate-refresh/post-test-source-drift.json).
Names/ownership are inferred from crate paths; mtimes are not proof of an editor identity.
The unapplied laptop patch/manifest were also added after the automated run.

## Fresh exact-ELF measurements

Actual archived release binary, production assembly, private nested compositors,
60s per idle/activity interval; no live session input or lifecycle operations.
Existing helper reads are observational. Separate hook events are reported, never
subtracted from CPU. Live legacy Quickshell is no longer running, so no new legacy
comparison is claimed. [Full report](perf-shell.md).

| Window | Thread-accounted CPU | Hook file events |
|---|---:|---:|
| Controlled quiet | 0.190864% | 0 |
| Host discovery / frozen hooks | 0.464611% | 0 |
| Live-hook idle | 0.997420% | 41 |
| Activity | 1.470025% | 32 |

accepted: user 2026-10-23; controlled_idle 0.190864% <= 0.5%, hook events 0; live_idle 0.997420% <= 2%, hook events 41; User accepts current release performance for deployment; legacy Quickshell was reported at 2.7–2.9% under the same hooks. Hook events are allowed and must be reported separately; 0.1% remains the idle aspiration.

The CPU bounds validator passes for the exact installed ELF; this is separate
from the full performance row, which still rejects the current runtime source
fingerprint. The 0.1% aspiration remains unmet and documented. Neither the waiver
nor source-freshness checks were broadened. A frozen integration candidate and
matching full test/performance provenance are needed to gate the concurrent code;
the user visual acceptance does not require another parity re-render.

The live shell PID3156707 and helper PID3243233 retained their post-user-install
start ticks throughout these operations.
[Baseline comparison](raw/emergency-gate-refresh/live-baseline-check.json).

## Laptop Phase A

[Design](../spec/laptop-deploy.md), [unapplied patch](../../cutover/laptop/hypr.patch),
[pinned inputs](../../cutover/laptop/hypr-inputs.json). Runtime profile selection,
independent cooling inertness, user-only packaging/cutover, testing requirements
and a proposed uncommitted branch series are specified. Eight source files pass
forward/reverse application plus five Lua/three shell syntax checks in a disposable
tree. No laptop implementation, hardware validation or deployment is claimed.
[Validation log](raw/emergency-gate-refresh/laptop-patch-check.log).
