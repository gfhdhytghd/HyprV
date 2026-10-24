# Final go/no-go review — svc-desktop (independent)

Reviewer: svc-desktop (Claude). 2026-10-23, 21:25–22:40 UTC. Done independently, without coordinating with the svc-net reviewer.

Method: everything below was **run**, not copied from reports, unless marked *(doc)*. Runs used either the live tree read-only or a fresh copy-on-write snapshot. The snapshot was taken at 21:27:08Z with the CI loop's own `clone()`: rshell plus sibling `hypr/` and `quickshell/`, consistent at 302 panel references. Builds used the shared `rshell/target` with `CARGO_INCREMENTAL=0` (disk policy). The snapshot release binary is SHA-256 `16a030479cee9966…`.

Nothing was installed, applied, restarted or written outside this report:
- cutover scripts ran as plans only;
- `preflight --run-tests` was **not** used, because it rewrites the shared `cutover/evidence/tests.json`; the same commands were run directly instead;
- the measurement and parity harnesses ran inside the snapshot, so their outputs landed there.

## Verdict: **NO-GO**

The product is close. Functionally and visually it looks ready: e2e 204/204, full-screen 64/64, island and panels gates pass, no open P1/P2 findings. It is not cleared to cut over:
- the project's own readiness gate is red on every row;
- the idle-CPU hard requirement is unmet;
- discovery now costs more than this afternoon (see B2);
- the bar parity gate is formally red.

## Blockers

**B1 — Preflight NOT READY: every gate is blocked.** Output of `cutover/preflight.sh`:

| Gate | Result | Assessment |
|---|---|---|
| release binaries | BLOCKED: candidate source changed since release build | Paperwork. Needs `packaging/build-release.sh` on a frozen tree. |
| automated tests | BLOCKED: receipt failed or stale | **Substance is green.** My direct runs: `cargo test --workspace --all-targets` 569 pass / 0 fail / 49 ignored (opt-in); `--doc` 2/0; xtask 1/0; `cutover/tests` 54/54. The receipt only needs regenerating. |
| parity gate | BLOCKED: island render evidence stale | Needs a regenerated report. Substance: see B3. |
| performance gate | BLOCKED: source evidence stale | Even fresh, it **fails**: see B2. |
| privhelper installed | BLOCKED: no `/usr/local/lib/hyprv-cutover/installed.json` | Expected before the root install (README step 4). |

**B2 — The idle CPU budget (≤ 0.1%, ARCHITECTURE hard requirement 3) is not met.**
- **Measurements.** Snapshot release binary, `measure-shell.py`, 60 s, 6144×3456@2:
  - **controlled quiet (gate case): 0.150%**, 12.2 sched/s, 146 MiB RSS;
  - **frozen hooks / real `/proc`: 0.813%**;
  - CI's own run at 20:25Z *(doc)*: quiet 0.233%, frozen 0.998%, live hooks 1.063%, activity 1.813%.
- **Cause of the host-discovery cost.** A dedicated `hyprv-agentproc` thread accounts for **416 ms per 60 s (~0.7% of a core)**. The agent-discovery probe in `hyprv-svc-desktop/src/agent_island/model.rs` (`ProcTable::probe`, after 15:50 local) now re-stats **every** known PID on every 1.5 s scan. On this host that is about 1,700–1,800 PIDs, roughly 10 ms per probe. This was added to fix DESKTOP-R2-1/R2-2. Its "1.7 µs/probe" figure (`sampling-cost.md`) came from an empty PID namespace and does not reflect the host.
- **Regression.** Earlier the same afternoon, with the range probe, frozen-hook idle was 0.23–0.45%. This is a regression in my own crate. It needs a revalidation strategy that stays correct without a full re-stat. Candidates: revalidate only agent-named PIDs and their ancestry chains, plus the 60 s full sweep.
- **Comparison with Quickshell is currently invalid.** Live Quickshell measures **0.18–0.27%** now, versus ~2.5–2.9% this morning. Its `desktop-state.py` (PID 3194225) wakes about 2.7/s but uses about 0.08% CPU. That rules out its agent scan, which costs about 24 ms in Python every 1.5 s. Most likely its agent observer died when `write_state` hit ENOSPC during the disk-full incident: a GLib callback that raises is never re-armed. This is unconfirmed, because its stderr is not logged. Native-vs-legacy CPU comparisons after about 15:00Z are therefore against a degraded legacy.

**B3 — The bar parity gate is red (GL, the gated path).** `parity/verify/verify.py --backend gl` on the snapshot:

| Component | Result |
|---|---|
| island | 380/380 **PASS** |
| panels | 300/300 **PASS** (better than the live report's 298/300) |
| bar | 292 pass, **0 fail**, 8 missing, 12 excluded → **FAIL** |

- **The 8 missing** are references with no render: `calendar-host` ×4 and `island-expanded-host` ×4. The bar crate's harness doesn't render those host states, so this is a coverage gap, not a pixel defect.
- **The same states pass full-screen.** Assembled shell, `--nested-test` plus `compare.py`: **64/64 pass** under the unchanged gate, against 0/64 in `parity-fullscreen.md` at 14:52 EDT; `residual.py` also passes 64/64.
- **To clear:** render the 8 states in the bar harness, or have the parity owner formally accept the full-screen evidence for them.
- **The 12 excluded** are deliberate (`QUIRK_BATTERY_POPUP_NEVER_OPENS`, matching live behaviour).

## Checks performed

1. **Feature parity.**
   - **E2E:** `--nested-functional-test` on the snapshot gives **204/204 PASS** (`manifest.json`, `suite_complete`). Input is native (virtual pointer and keyboard).
   - **It verifies UI → typed command only.** Service actors are absent and commands go to a mock sink.
   - **Equivalence** *(doc, 07:20Z)*: 3648 PASS, 1 MISMATCH, 28 UNAVAILABLE, 17 TEMPORAL. It is read-side only.
   - **The MISMATCH** (battery_history picking a peripheral battery) is fixed in code: `hyprv-svc-power/src/battery_history.rs:218-223`, `BAT*`-only, with a test. Equivalence has not been re-run, and the README still lists it as open.
   - **IPC.** The patched hypr config, with `cutover/hypr.patch` applied to the snapshot, calls `ipc call controlPanel toggle`, `ipc call quickAdjust <kind>`, `desktop setVariant`, `desktop deskflowReflow` and `startup-confirm`. All are covered by the e2e matrix and the `hyprv-core` IPC contract tests.
   - **`hypr.patch` applies cleanly** to the live, locally modified `hypr/` tree (`git apply --check`, 6 files).
   - **Only mocked or never executed against real side effects:**
     - fan helper writes, and backlight/DDC writes;
     - audio stream and device actions; tray menu activation; notification dismiss and DND toggle;
     - Cider lookups; spectrum capture (gated off);
     - VM, Mihomo, IR, eGPU, theme apply, app activation and startup approvals.
   - Each of these is reached by the e2e only up to the mock sink.
2. **Visual parity.** See B3. The production GL path differs from the CPU harness by at most 3/255 (bar, panels) and 50/255 (island, CPU harness only); the gate is decided on GL.
3. **Safety.**
   - **Review findings.** All 75 P1/P2 findings in `docs/reviews/*` are fixed in the current code; none are OPEN or DEFERRED. 73 have passing regression tests; P1 tests were re-run.
   - **Two P1 fixes have regression tests that never compile:** WIDGETS-R2-1 (cyclic GTK icon-cache hang) and SHELL-R2-1 (unbounded tray-icon reads). `icon_theme_regressions.rs` and `assets/regressions.rs` are not included by any `mod`. Wired in on the snapshot only, they pass 4/4 and 3/3, so the fixes hold, but CI won't catch a regression. A one-line `#[cfg(test)] #[path] mod` each would fix it.
   - **Privileged helper / fan failsafe:** fixes PRIV-1..6 are verified in software with mocks. Physical fan response, watchdog recovery and early-boot behaviour are unvalidated, and the root phases were only simulated in the rehearsal.
   - **Network secrets:** NET-1..8 and PANEL-1 are fixed and tested (zeroized buffers; the password never reaches caches). In mock or capture mode only, `hyprv-shell/src/services.rs:651` writes the synthetic Wi-Fi password to the mock-actions JSON.
   - **User configs:** neither the shell nor the cutover edits `~/.claude` or `~/.codex`. Hooks are installed only by an explicit `hyprv-agent-hook install`, with DESKTOP-1..7 fixed and tested.
   - **Cutover scripts:** dry-run plans for `install-user`, `install-root`, `switch --quiesce-legacy`, `switch`, `rollback --quiesce-native` and `rollback` all match `CUTOVER.zh-CN.md`, with exit 0.
4. **Performance, memory, stability.**
   - CPU: see B2.
   - **Memory: within budget.** 146 MiB quiet and 187 MiB frozen-hook (this run); 241 MiB after activity *(doc, 20:25Z)*; budget ≤ ~320 MiB. `engine-memory.md` *(doc)* gives 205 MiB after the popup cycle.
   - **Stability:** a 62-minute soak *(doc)* had a clean exit with no crash or stall. SOAK-1 (+220 KiB private dirty, +5 MiB GPU, plateauing) is still open for investigation.
   - The rotation crash is fixed (27/27 nested). The GPU-corruption risk is closed *(doc)*.
5. **Preflight:** see B1.
6. **Docs.**
   - **Procedures are accurate.** Every path and flag in `README.md` and `CUTOVER.zh-CN.md` exists, and the plans match.
   - **The README's status section is stale.** It still cites:
     - parity "bar 228/304, island 0 exact";
     - CPU "0.35% vs 2.66%";
     - the battery mismatch as open.
   - **One plan line in my own cutover script is inaccurate:** `switch.sh` says "Back up the three hypr files", but the code backs up all six patch files (`control.py:460`).

## Non-blocking risks

1. **Service actions are verified only up to a mock sink** (see check 1). The first real use of fan, backlight, tray-menu, VM and Mihomo actions will happen in the live session.
2. **Two P1 regression suites aren't compiled** (WIDGETS-R2-1, SHELL-R2-1). The MEDIA-1 spectrum tests rely on 20–60 ms sleeps and flaked once under load (passed 7/7 on retry).
3. **Live legacy state is degraded.** Quickshell's `desktop-state.py` agent observer appears stalled since the disk-full incident. An **orphaned `desktop-state.py` (PID 2558754, reparented to init, running for about 3 days)** also exists. `quickshell kill` at cutover won't stop it, and it rewrites `state.json`. I suggest stopping it by hand; I didn't touch it.
4. **The privileged helper has never run as root on this machine**, and the cooling failsafe has never driven the real fans. Watch fan behaviour during step 2b.
5. **No automatic in-process Wayland reconnect.** After a compositor restart, systemd starts a fresh shell process.
6. **The equivalence report predates the battery fix and the agent-island parity work.** It should be re-run before the final receipt.

## Path to GO

1. Fix B2. Make agent discovery's revalidation proportional to agent-related PIDs (owner: svc-desktop), then re-measure. The controlled-quiet case also needs about 0.05% more trimmed from the UI and main thread (`perf-shell.md` cause 4), or an explicit user waiver of the 0.1% budget.
2. Close B3: render the 8 bar host states, or formally accept the full-screen evidence.
3. Wire in the two uncompiled regression suites.
4. Freeze the tree, run `packaging/build-release.sh` and `cutover/preflight.sh --run-tests`, and regenerate the parity, perf and equivalence reports.
5. Update the README status section.
6. Restore a healthy legacy baseline (stalled observer and orphan) before quoting native-vs-legacy CPU.
7. Then the root install (step 4), with `preflight --installed` READY.

Evidence (session scratch, not committed): test logs, the GL parity output, the e2e and full-screen manifests and comparisons, and the perf JSONs. They sit under the scratchpad's `review/` directory, along with the binary hash.
