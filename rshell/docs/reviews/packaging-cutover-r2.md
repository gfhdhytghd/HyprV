# Packaging/cutover independent re-review

Reviewed 2026-10-23. **Six verified findings remain: one P1 and five P2.** CUT-1..7 have their intended normal-path changes, but CUT-4/5/6 still have failure-path gaps. Do not treat the first review's “all fixed” status as a complete recovery guarantee.

Scope: current `packaging/` and `cutover/`, the prior review and owner response, and read-only inspection of the legacy launcher. Implementation files were not changed. Every probe ran as uid 1000, with temporary destinations and fake process/service/compositor boundaries. No root process, installation, live session operation, real service command, or hardware access was performed. This is control-flow/file-transaction evidence, not live systemd, graphical, or cooling-device validation.

## Fresh evidence and original-fix verification

The independent [probe suite](scratch/packaging-cutover-r2/probe.py) imports a [frozen copy](scratch/packaging-cutover-r2/frozen/) of the reviewed Python files. Unexpected subprocesses fail closed. The only real external operation permitted by the harness is `git apply` against a disposable copy of the Lua fixtures. Root guards are simulated in-process; actual uid remains 1000. Root installer destinations and state are redirected to scratch directories; their ownership expectations are adapted to the real unprivileged uid.

Run from rshell:

```sh
python3 -B docs/reviews/scratch/packaging-cutover-r2/probe.py
python3 -B docs/reviews/scratch/packaging-cutover-r2/probe.py --expect-fixed
```

The first command passes **18/18**: twelve controls plus six assertions reproducing the bad outcomes below. The second substitutes the required safety assertions and intentionally fails exactly those **six** tests. See [normal log](scratch/packaging-cutover-r2/probe.log), [failing safety assertions](scratch/packaging-cutover-r2/expected-failures.log), and [source hashes](scratch/packaging-cutover-r2/source-hashes.json). These are newly written probes, not reruns of the original owner's tests.

| Original | Fresh verification | Assessment |
|---|---|---|
| CUT-1 | Native reinstall restarts the helper without masking; `ensure_helper()` requests unmask, enable/start, active check. | Original normal-path defect fixed. Concurrent handoff remains unsafe: R2-6. |
| CUT-2 | Valid temporary manifest verifies; changed content and user-owned root-stage paths are rejected; checkout control is rejected as a staged root tool. Wrappers and staging instructions inspected. | The original checkout execution path is removed. No actual root staging/execution was attempted. |
| CUT-3 | Missing runtime-linked unit symlink is recreated exactly and a previously active unit is started. | Original linked-unit loss fixed. Retry and I/O failures remain: R2-3/4. |
| CUT-4 | Normal fallback removes the marker, clears reversed patch ownership and saves `fallback`. | Basic fallback added; stop failure and readiness deadline remain: R2-2/5. |
| CUT-5 | Quiesce never consults patch drift; content reversal preserves an unrelated edit in temporary Lua files. | Original drift blocker removed. Failed rollback is prematurely finalized: R2-3/4. |
| CUT-6 | Failure on the second destination rename restores both original files and records `restored`. | Ordinary mid-commit failure fixed. Post-commit interruption remains: R2-1. |
| CUT-7 | Recorded namespaces exist in copied layer rules; corner-overlay is absent from the namespace list. | Inventory corrected. |

## CUT-R2-6 [P1] Root install and cooling handoff use different locks

**Locations:** `packaging/install.py:102`, `packaging/install.py:199`, `packaging/install.py:203`, `packaging/install.py:220`; `cutover/control.py:108`, `cutover/control.py:358`, `cutover/control.py:377`, `cutover/control.py:379`, `cutover/control.py:383`.

The installer locks `install.lock`; root handoff locks `lock`. They can change the same helper and cooling ownership concurrently. Concrete interleaving:

1. Installer holds its lock and reads phase `native`, helper enabled/active.
2. Root rollback obtains its different lock and disables/stops the helper.
3. Installer commits files and restarts the helper using its cached enabled state. A disabled unit can still be explicitly restarted.
4. Rollback masks the helper **without `--now`**, restores old PWM permissions and legacy units, and saves `legacy`.

The helper remains active while legacy controllers are restored. Masking only prevents future activation. This reintroduces competing cooling writers despite a successful rollback.

**Verification:** `test_r2_6_*` runs the actual installer and actual rollback control flow in two threads with barriers and both real file locks held. Systemd is modeled, not executed. Result: journal `legacy`, helper `masked` **and active**, legacy restoration performed. No fabricated cached probe values are injected; the installer really probes before the rollback disable.

**Suggested fix:** use one root orchestration lock across install, switch and rollback, acquired before reading phase/service state and held through publication/finalization. Define lock ordering if transaction locks remain separate. Re-check that the helper is inactive before granting legacy access and restoring legacy units.

## CUT-R2-1 [P2] A committed-but-unfinished install is skipped on recovery

**Locations:** `packaging/install.py:118`, `packaging/install.py:155`, `packaging/install.py:215`, `packaging/install.py:217`, `packaging/install.py:226`, `packaging/install.py:229`.

`Transaction.run()` durably records `committed` before daemon reload, helper restart, transaction completion and release publication. `recover()` ignores that phase. Kill the installer after that save but before daemon reload: all files contain the new release while systemd and the active helper still use the old generation. On retry, byte comparison finds zero changes; an enabled active helper skips both reload and restart. The installer publishes the new release and returns success while the old helper keeps running. This matters especially when the new release fixes cooling/security behavior.

**Verification:** `test_r2_1_*` injects a process-interruption equivalent (`BaseException`, bypassing the normal command-failure handler) immediately before daemon reload. The real journal is `committed`. Retry returns 0, publishes `new-id`, performs no reload/restart and leaves the modeled running generation old.

**Suggested fix:** recover every nonterminal transaction, including `committed`. Either restore its previous generation then perform a complete new install, or resume the recorded reload/restart/health-check/publication steps. Include publication in the durable state machine; `done` must mean the complete installation has succeeded, not merely that file renames finished.

## CUT-R2-2 [P2] Fallback launches Quickshell even when native could not stop

**Locations:** `cutover/control.py:249`, `cutover/control.py:252`, `cutover/control.py:257`.

After failed native readiness, `stop` is unchecked. The added `is-active` query only appends a warning if native remains active; execution still reloads Hyprland, starts Quickshell and records fallback. A failed stop can therefore leave both shells handling the same outputs/input while native is still alive and incompletely initialized. This is materially different from the full rollback path, which refuses to continue while native is active.

**Verification:** `test_r2_2_*` makes stop fail and the active query succeed. The legacy launcher is still called exactly once and fallback reports it up.

**Suggested fix:** require confirmed native inactivity before starting the legacy UI. Preserve a retryable failure journal when that cannot be established, and report the blocked stop clearly. Avoid reversing routing into a second live shell before exclusive ownership is established.

## CUT-R2-3 [P2] Incomplete rollback is finalized, making retry silently succeed

**Locations:** `cutover/control.py:303`, `cutover/control.py:310`, `cutover/control.py:316`, `cutover/control.py:318`; analogous root finalization at `cutover/control.py:373` and `cutover/control.py:383`.

With the helper stopped, user rollback writes `phase=legacy` even when a patched line remains unresolved, a unit failed to start, or Quickshell failed to launch. It initially returns 2, but the next rollback returns 0 at the early exit and never retries the outstanding work. A transient unit failure or a manually corrected ambiguous Lua line cannot be recovered by rerunning the advertised rollback command. Root rollback likewise records `legacy` even when `restore_units()` reports problems.

**Verification:** `test_r2_3_*` returns an unresolved patch line and a failed unit start. First rollback returns 2 and saves `legacy` with `patch_owned=True`. Second rollback returns 0 without invoking either restoration function. The analogous root branch is source-confirmed; the executable probe exercises the user branch.

**Suggested fix:** save a distinct incomplete rollback phase and pending operations; retry them until resolved. Only finalize `legacy` once required restoration succeeds, and retain a nonzero result while outstanding problems remain. Preserve the original baseline across retries.

## CUT-R2-4 [P2] Unit-link restoration errors prevent the promised UI recovery

**Locations:** `cutover/control.py:93`, `cutover/control.py:94`, `cutover/control.py:316`, `cutover/control.py:317`.

`restore_units()` returns problems for certain conflicts, but filesystem errors creating a link/directory and daemon-reload failures raise. Full rollback calls it before the legacy launcher. For example, an ENOSPC while recreating a removed runtime link aborts rollback after native has stopped and configuration has been reversed, before Quickshell is launched. The documented guarantee that restoration errors never block legacy UI recovery is therefore false.

**Verification:** `test_r2_4_*` injects an I/O exception at the unit-restoration boundary. Rollback propagates the exception and never calls the launcher. The concrete throwing link/reload operations were inspected at the locations above.

**Suggested fix:** collect per-operation restoration failures and independently attempt UI recovery after native inactivity is confirmed. Journal the remaining failures as retryable. Catch restoration failures without allowing a failed stop to bypass the single-shell requirement in R2-2.

## CUT-R2-5 [P2] Readiness bounds IPC pings but leaves surface queries unbounded

**Locations:** `cutover/control.py:29`, `cutover/control.py:222`, `cutover/control.py:224`, `cutover/control.py:225`, `cutover/control.py:242`, `cutover/control.py:244`.

IPC pings now have a timeout, but all three surface queries (`systemctl show MainPID`, `hyprctl monitors`, `hyprctl layers`) use `output()`, whose subprocess has no timeout. A hung manager/compositor query stalls readiness indefinitely after Quickshell has been killed. Even finite slow replies can cause success after the advertised 20-second deadline: readiness returns before re-checking time when the surfaces finally look valid.

**Verification:** `test_r2_5_*` runs actual `wait_native()` → `native_surfaces()` → `output()` with a fake clock/runner. IPC is correctly bounded. Each actual surface-query call lacks a timeout and advances 30 seconds; readiness returns success at 90 seconds with a 20-second budget.

**Suggested fix:** propagate one remaining deadline through every readiness subprocess, handle timeouts as readiness failures, and re-check the deadline before accepting success. Bound stop/start and legacy-launch waits separately so recovery itself cannot hang indefinitely.

## Additional negative controls and limits

Two repeated no-op installs issue no reload, restart, start or mask commands. Failed atomic journal replacement preserves the complete previous JSON and removes its temporary file. The native unit has bounded restart rate configuration. These checks did not reproduce a no-op reload storm or torn JSON; they do not certify power-loss durability, every possible retry sequence, or live compositor behavior. The concurrency and terminal-phase findings above concern logically inconsistent state even when each JSON write is atomic.

The legacy launcher was inspected read-only: it performs the old fan `restore` before launching Quickshell. In the intended helper-owned state the revoked user PWM permissions prevent those writes; this review does not claim a separate confirmed cooling bypass from that call. The verified dual-owner problem is the root install/rollback race in R2-6.

## Owner response (svc-desktop, 2026-10-23)

All six findings are fixed in `packaging/install.py` and `cutover/control.py`. The regressions are in `cutover/tests/test_review_fixes.py::CutR2Fixes`; the cutover suite is 49/49. I reran the isolated rehearsal ([docs/verify/cutover-rehearsal.md](../verify/cutover-rehearsal.md) § Round 2) with the new code.

| Finding | Fix |
|---|---|
| R2-6 | **One root orchestration lock.** `install.py root` takes `/var/lib/hyprv-cutover/lock`, the same file the root cutover journal locks, before `install.lock`, and holds it for the whole run, publication included. Lock order: `lock`, then `install.lock`. Root rollback now uses `mask --now` and **re-checks that the helper is inactive** before restoring PWM permissions or legacy units. If the helper is still active it saves `rollback-incomplete` and refuses. A root switch refuses while a rollback is incomplete. |
| R2-1 | Every nonterminal transaction is recovered (only `done` and `restored` are terminal), `committed` included: restore the previous generation, then run a complete install. Publication now comes before `done`. |
| R2-2 | `fallback()` starts the legacy UI and reverses routing only after native is confirmed inactive (stop, then SIGKILL, then re-check; an unknown state counts as running). Otherwise it saves `phase=fallback-blocked` (resumable), keeps the patch, and launches nothing. |
| R2-3 | User and root rollback finalize `legacy` only with zero problems. Otherwise they save `rollback-incomplete` (or `ui-restored` while the helper runs), exit 2, keep the baseline, and retry everything on the next run. Quiesce does not overwrite an incomplete phase. |
| R2-4 | `restore_units()` never raises: link, mkdir, daemon-reload, start and state-query failures become problems. User rollback also catches restoration failures and always tries the legacy UI (native is confirmed stopped first). |
| R2-5 | One deadline covers every readiness query (`output(..., timeout=remaining)`). A query timeout counts as not ready. Success is accepted only inside the deadline. Switch/fallback/rollback commands are bounded too (unit 30 s, reload 10 s, launch 30 s); `TimeoutExpired` is handled everywhere. |

**About `probe.py --expect-fixed` on the new code.** I ran a copy of the probe with the current `cutover/` and `packaging/` swapped into `frozen/`. The 12 controls pass. The 6 R2 tests fail, but in their bad-outcome assertions, which come before the `EXPECT_FIXED` checks:
- R2-1: a restart *is* issued on retry;
- R2-2: fallback returns not-up without launching;
- R2-3: phase `rollback-incomplete`;
- R2-4: no `OSError` raised;
- R2-5: the surface query carries `timeout`;
- R2-6: rollback cannot reach disable while install holds the lock.

These are the fixed behaviours. The probe's fake runners would need to make those assertions mode-dependent before it can certify fixes directly.
