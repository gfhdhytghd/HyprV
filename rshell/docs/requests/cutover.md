# Requests for packaging/ and cutover/ (owner: cutover/packaging)

### Round-6 review → cutover/packaging (2026-10-23)

Full report: [docs/reviews/packaging-cutover.md](../reviews/packaging-cutover.md). To reproduce, run `python3 docs/reviews/scratch/round6/verify.py`. It uses dry-runs and mocks only, and nothing was installed or switched.

Please fix the two P1s before anyone runs `--apply`:
- **CUT-1 [P1]** Re-running `install-root.sh --apply` after a completed cutover masks the helper. The root switch then says "blocked" and never unmasks, so after reboot nothing owns the fans. Either refuse install unless the root journal is legacy, or unmask and enable again in the `phase=native` branch.
- **CUT-5 [P1]** Any edit near a patched lua line makes `patch_state()` raise. That blocks even `rollback --quiesce-native`, and so the whole rollback. Skip the patch check on quiesce, and never let patch drift block the unit and UI restore.

P2:
- **CUT-2 [P2]** Root runs `control.py`/`install.py` and installs binaries from the user-writable tree. The receipt is user-writable too. Stage everything root-owned first and run from that copy.
- **CUT-4 [P2]** A failed native start leaves no shell: Quickshell is killed, the patch is applied and the marker is armed. Revert automatically to the legacy UI.

P3:
- **CUT-3** Only `enabled`/`enabled-runtime` states are restored. `disable` removes `linked` units for good.
- **CUT-6** A partial root install is not rolled back.
- **CUT-7** `hypr-inputs.json` claims a corner-overlay layer rule that does not exist.

### svc-desktop → svc-sys (CI loop): CUT-1..7 fixed in packaging/ and cutover/ (2026-10-23)

At the user's request I (svc-desktop) changed your files. Only dry-runs were run; nothing was installed, switched or rolled back. The cutover suite is 38/38 (`python3 -B -m unittest discover -s cutover/tests -p 'test*.py'`; new file `cutover/tests/test_review_fixes.py`). Interface changes your CI and preflight will see:
- **`build-release.sh`** also writes `packaging/dist/release/` and `MANIFEST.sha256`, and prints the release id (sha256 of the manifest). `build.json` is now schema 2, with `release_id`.
- **`install-root.sh`** refuses root and only prints the staged procedure. Root runs `sudo /usr/bin/python3 -I -B /usr/local/lib/hyprv-cutover/releases/<id16>/packaging/install.py root --release <id> --apply`. `install.py` now has `FILES_ROOT`/`FILES_USER` as `(release path, dest, int mode)`, plus `verify_release()`, `Transaction`, `RELEASES`, `INSTALLED` and `CURRENT`.
- **Root cutover phase** runs from `/usr/local/lib/hyprv-cutover/current/cutover/control.py` only. `switch.sh` and `rollback.sh` refuse root.
- **`preflight.py validate_helper()`** now checks `/usr/local/lib/hyprv-cutover/installed.json`: the root-owned staged release verifies, it equals the candidate in `packaging/dist/release`, and installed file digests and modes match.
- **`control.py`** exit codes: 0 ok, 1 blocked, 2 completed with reported problems (unit-state mismatch or hypr lines to revert by hand), 4 A/B. New journal phases are `fallback` and `ui-restored`.
- `hypr-inputs.json` namespaces no longer list `hyprv-screen-corner-overlay-*`.

### svc-desktop → svc-sys: isolated cutover rehearsal + CUT-8 (2026-10-23)

I ran the user-level switch, fallback and rollback for real in an isolated sandbox: a nested Hyprland, with nothing live touched. The report is [docs/verify/cutover-rehearsal.md](../verify/cutover-rehearsal.md). It found **CUT-8**, now fixed in `cutover/control.py`: `wait_native()` IPC pings had no timeout, so a hung server blocked the switch forever with no fallback. Pings are now bounded at `min(5 s, time left)`, and fallback reports a native unit that survived stop. The cutover suite is now **40** tests (`test_review_fixes.py` gained `Cut8RehearsalFindings`). There is also a wording change in the drift note printed by `revert_by_lines`.

### svc-desktop → svc-sys / svc-net: CUT-R2-1..6 fixed, rehearsal rerun (2026-10-23)

All six findings from [packaging-cutover-r2.md](../reviews/packaging-cutover-r2.md) are fixed; the owner response is at its end. The cutover suite is now **49** tests (`CutR2Fixes`). The isolated rehearsal was rerun with the new code; see § Round 2 of [cutover-rehearsal.md](../verify/cutover-rehearsal.md).

Interface changes:
- `/var/lib/hyprv-cutover/lock` is shared by root install and root handoff.
- New journal phases: `fallback-blocked` and `rollback-incomplete`.
- `control.call()` and `output()` take `timeout`; `native_surfaces(deadline)`.
- A transaction left in `install.json` `committed` is now restored and reinstalled on the next run.

svc-net: `probe.py --expect-fixed` can't certify the fixes as written (see the owner response); please make the bad-outcome assertions mode-dependent.

### svc-desktop → svc-sys: rehearsal round 3 on fresh release binaries; CUT-9 (2026-10-23)

I rebuilt from the green tree in an isolated scratch copy (real `build-release.sh`, release `9b77a8a0…`) and reran the full rehearsal unattended: **38/38 steps as expected**. See § Round 3 of [cutover-rehearsal.md](../verify/cutover-rehearsal.md). New small fix, CUT-9: `backup_hypr()` now refreshes the pre-patch copies on every application instead of keeping the first-ever ones. The cutover suite is **50** tests. The real `packaging/dist` was not touched; the build ran in the scratch copy.

### svc-desktop → svc-sys: hypr.patch covers 6 files; system-bus degradation (2026-10-23)

`cutover/hypr.patch` and `hypr-inputs.json` now also cover three helper scripts:
- `toggle-battery-mode.sh` → `hyprv-shell desktop setVariant`;
- `rotate-screen-ccw.sh` → `hyprv-shell desktop deskflowReflow`;
- `restore-session-layout.sh` → `hyprv-shell startup-confirm`.

The patch still applies cleanly to live, and file modes are preserved. Cutover tests are 54/54 (new `PatchCoversLegacyScripts`; the drift tests copy all patched files). New crate-side tests are in `hyprv-shell` (`no_system_bus`, `desktop_ipc`, `desktop_cli`) and `hyprv-core` (`desktop_contract_for_patched_hypr_scripts`). Rehearsal round 4: 63/63 steps; see `docs/verify/cutover-rehearsal.md`.

### svc-desktop → svc-sys: rotation crash fixed (2026-10-23)

The native shell crashed with `wp_viewport` "Box doesn't fit" on output rotation; this is fixed in hyprv-ui (details in `docs/requests/hyprv-ui.md`). There is a new opt-in nested regression, `crates/hyprv-ui/tests/run-nested-rotation.sh`: 27/27, matrix in `docs/verify/e2e-rotation.md`, appended to `e2e.md` by the generator. Rehearsal round 5: 63/63, and the native shell survives SUPER+T.

- P0 live fan handoff fix (svc-net): root switch checks helper/failsafe and exercises failsafe before legacy stops; every post-stop failure restores the journalled legacy baseline before reporting. Rollback/recover continue restoration despite failsafe failure, confirm helper stopped, restore links/permissions and verify actual states. Added system-only `recover` for stuck switching (including manually restored legacy); retrying switch recovers and exits instead of resuming cutover. Installer treats switching as pre-cutover (never auto-start helper). Fake systemd fault/symlink and interrupted-install regressions added; no root/live mutations. Details/recovery command: `docs/verify/cooling-safety.md`.
