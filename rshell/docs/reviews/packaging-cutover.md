# Adversarial review: packaging/ and cutover/

Reviewed 2026-10-23 (round 6). **Review only, dry-runs only.** Nothing was installed, switched, rolled back, applied, reloaded, stopped or started. No sudo was used. The real hypr files, unit states, `/usr`, `/etc`, `/var/lib` and `/sys` were not written.

Evidence is in [scratch/round6](scratch/round6/):
- [results.json](scratch/round6/results.json)
- [probes_cutover.py](scratch/round6/probes_cutover.py), which runs against frozen copies in `frozen/`
- [dryruns.log](scratch/round6/dryruns.log)
- [source-hashes.json](scratch/round6/source-hashes.json)

**Reproduce:** `python3 docs/reviews/scratch/round6/verify.py`. It exits 0 only when all of these hold:
- every probe fails with its own assertion;
- every control passes;
- the audited sources (control.py, install.py, hypr.patch, hypr-inputs.json, legacy-units.json and the four hypr lua files) are unchanged.

The probes mock `subprocess.run` with a small stateful systemctl model. Journal, sysfs and `/proc` are mocked too. Patch probes run `git apply` on temporary copies of the lua files.

**Severity.** P1: can leave the live desktop without cooling control or without a working rollback. P2: correctness or safety defect with a realistic trigger. P3: defense in depth, rare trigger, or documentation.

## Fix status (2026-10-23, same day)

All seven findings are fixed in `packaging/` and `cutover/`. Regressions are in `cutover/tests/test_review_fixes.py`; the cutover suite is 38/38. Only dry-runs were executed; see [fix-dryruns.log](scratch/round6/fix-dryruns.log) (no mutation).

| Finding | Fix |
|---|---|
| CUT-1 | Install reads the root journal: after cutover it never masks, and restarts an enabled helper on the new files, restoring the old files if it does not come back. The root switch in `phase=native` re-runs unmask/enable/is-active. |
| CUT-2 | Root runs only a root-owned staged release (`/usr/local/lib/hyprv-cutover/releases/<id16>`, copied and `sha256sum`-checked with system tools), under `/usr/bin/python3 -I`. Tree ownership, every manifest digest, no unlisted files or symlinks, and the release id typed at install are all checked. Wrappers refuse root. Root preflight runs as the desktop user. |
| CUT-3 | The journal records every unit symlink (link, alias, wants/requires/upholds, runtime). Rollback recreates exactly those, starts what ran, and reports any unit-file state that differs (exit 2). Masked/static/disabled units are only stopped. |
| CUT-4 | Ready = IPC `bar dataStatus` plus a `hyprv-quickshell` layer of the unit's MainPID on every enabled monitor, within 20 s. Otherwise automatic fallback to legacy Quickshell (`phase=fallback`). |
| CUT-5 | Quiesce never reads hypr files. Full rollback: exact reverse, else line-by-line by content, else the lines and backup path are reported. Native is always stopped and Quickshell always restored. |
| CUT-6 | Journalled transaction: temps beside destinations, backups, renames, and restore on failure or after an interrupted run. |
| CUT-7 | `hypr-inputs.json` and the README no longer claim a corner-overlay rule. The naming mismatch stays with hyprv-ui-bar. |

The original probes in `scratch/round6` keep running against the frozen pre-fix copies; `verify.py` now records changed sources instead of failing on them.

## What root install changes (exact, before the fixes)

`sudo packaging/install-root.sh --apply`, as `install.py root`, does the following in order:

1. Validates `packaging/dist/build.json`. The receipt holds the release profile and SHA-256 hashes of the binaries and sources.
2. Refuses if `hyprv-privhelper.service` is active.
3. `install -d -m 0700 /var/lib/hyprv-cutover`.
4. `systemctl mask hyprv-privhelper.service`. This creates `/etc/systemd/system/hyprv-privhelper.service -> /dev/null`.
5. For each of the six files below:
   - copy any existing destination once to `/var/lib/hyprv-cutover/install-backup/<basename>` (never overwritten);
   - `install -m MODE` the file to `.<name>.cutover-new` in the same directory;
   - `mv -f` it into place.

   | Destination | Mode |
   |---|---|
   | `/usr/libexec/hyprv-privhelper` | 0755 |
   | `/usr/libexec/hyprv-cooling-failsafe` | 0755 |
   | `/usr/lib/systemd/system/hyprv-privhelper.service` | 0644 |
   | `/usr/share/dbus-1/system-services/org.hyprv.Shell.PrivHelper.service` | 0644 |
   | `/etc/dbus-1/system.d/org.hyprv.Shell.PrivHelper.conf` | 0644 |
   | `/usr/share/polkit-1/actions/org.hyprv.shell.fan-control.policy` | 0644 |
6. `systemctl daemon-reload`.

It does **not** enable or start anything, touch the legacy units or sysfs, or add polkit rules. dbus-daemon and polkitd pick up the new policy files through their own inotify watches. That is harmless while the helper is masked.

The live legacy state the rollback must restore was read only (no changes):
- System units `hyprv-fan-control` (running `/usr/local/libexec/hyprv-fanctl run-system`), `hyprv-fan-permissions` and `hyprv-cpu-power` are all **enabled**.
- User unit `hyprv-fan-control` is **disabled/inactive**.
- Quickshell is running.

## Verified good (controls and inspection)

- **Dry-run never mutates.** I ran every README dry-run: build-release, install-user, install-root, switch/rollback in user, system and quiesce scopes, `--ab-output` (exit 4) and preflight (NOT READY, exit 1). Before/after snapshots of the state dirs, `/var/lib`, `/usr/libexec`, the hypr lua hashes and `git status` were identical. `CONTROL_dry_runs_execute_nothing` also fails on any `subprocess.run` or `os.open` during the plans.
- **No destructive rm.** The only deletions are `unlink(missing_ok)` of the `native.enabled` marker and removal of the journal temp file. Install uses `mv -f` only onto its own destinations, after a first-copy backup.
- **Path and permission safety of the journals.** Directories must be 0700, owned by the caller and not symlinks. The state file and lock are opened `O_NOFOLLOW` and must have nlink 1. Writes are atomic with fsync. Install temp files sit in root-only directories.
- **Idempotency.** Re-running install keeps the first backup and replaces files again. An interrupted root or user switch resumes without recapturing its baseline. Completed rollback is a no-op. Root rollback restores the saved enabled/active states with the permissions unit first (`CONTROL_root_rollback_restores_enabled_units_permissions_first`).
- **hypr.patch** (`CONTROL_patch_*`):
  - It applies cleanly to the current `../hypr` working tree, including the uncommitted `controls.lua` edits.
  - Post-image hashes equal `hypr-inputs.json`, and `--reverse` restores the bytes exactly.
  - It touches only exec-once, bind and controls. No layer rule line changes.
  - Native namespaces (`hyprv-quickshell`, `shell:hyprv-*`) match the existing `layer-quickshell` / `layer-shell-blur` matchers.
  - `controls.lua` defines `local home` (line 3), so the patched `notify()` resolves.
  - `math.floor` gives Lua 5.4 integers, so `showBrightnessLevel 35` passes the native `parse::<i32>` validation.
  - All IPC calls the patch emits exist in `hyprv-core/src/ipc.rs`: `controlPanel toggle` and `quickAdjust show*`.

## CUT-1 [P1] Re-running root install after cutover masks the cooling helper, and switch cannot undo it

**Location:**
- `packaging/install.py:47-52`: the active check, then the unconditional `systemctl mask`
- `cutover/control.py:187-188`: `phase == 'native'`, then `is-active` and `return`

After a completed root switch, the helper is enabled and the legacy units are disabled. Install refuses only an *active* helper. If the helper was stopped for an upgrade, or hit its restart limit, install masks it. Then `sudo cutover/switch.sh --apply --scope system` sees journal `phase=native`, runs `systemctl is-active` (exit 3) and stops with "cutover blocked". It never unmasks.

At the next boot nothing owns the fans: the helper is masked and the legacy units are disabled, so it falls back to firmware defaults. The shell's fan UI is dead too.

The only way out is a full root and user rollback, then a fresh switch. Nothing tells the user that.

**Probe:** `PROBE_cut1_reinstall_after_native_handoff_leaves_helper_masked`. The helper stays `masked` after install plus root switch.

**Fix (either):**
- install-root refuses unless the root journal is absent or `phase=legacy`. Upgrades then go through rollback, install, switch.
- Or the `phase=native` branch of the root switch re-runs `unmask` and `enable --now`, then checks `is-active`.

Also say in the error and the README which one applies.

## CUT-2 [P2] Root runs and installs files the desktop user can rewrite

**Location:** `packaging/install.py:13-20` (`validate_build`); `cutover/switch.sh`, `rollback.sh`, `control.py`, `preflight.py`.

`sudo packaging/install-root.sh` and `sudo cutover/switch.sh --scope system` run Python from `~/.config/HyprV/rshell`, which is owned by uid 1000. The binaries copied to `/usr/libexec` come from user-owned `packaging/dist`. The receipt that "validates" them sits in the same writable directory. There is also a window between `validate_build()` and the later `install` copy.

Any process running as the user can therefore choose code that root executes, or that systemd later runs as root (`hyprv-privhelper`). It can rewrite a binary and its receipt, or edit control.py before the next sudo. This turns user compromise into root.

**Probe:** `PROBE_cut2_root_install_accepts_artifacts_rewritten_by_the_user`. A rewritten helper plus a regenerated receipt passes `validate_build`.

**Fix:**
1. Stage the binaries and scripts to a root-owned directory first, e.g. `install -o root -m 0755` into `/var/lib/hyprv-cutover/stage/`.
2. Show their SHA-256 hashes and ask for interactive confirmation.
3. Copy into place from that staged copy only.
4. Run the root phase from a root-owned copy of control.py.

At minimum, refuse when any input is not root-owned, or say in the README that the tree must be trusted.

## CUT-3 [P3] Rollback restores only `enabled` / `enabled-runtime` unit states

**Location:** `cutover/control.py:57-67`.

`stop_legacy` runs `disable --now` on every unit that is not masked. `restore_units` re-enables only `enabled` and `enabled-runtime`. For a `linked` or `linked-runtime` unit (`systemctl link` of an out-of-tree file), `disable` removes the link. The unit becomes not-found, and the rollback `start` then fails.

`alias`, `indirect` and `generated` states are not modelled either. `units()` raises on alias targets, but other states fall through silently.

The live units are plain `enabled`/`disabled`, so this does not trigger today.

**Probe:** `PROBE_cut3_rollback_does_not_restore_linked_unit`. The unit is not-found and inactive after rollback.

**Fix:** refuse to switch when a unit is in a state the rollback cannot restore (anything other than enabled, enabled-runtime, disabled, static or masked). Or journal the unit's fragment and link paths.

## CUT-4 [P2] A failed native start leaves the desktop without any shell

**Location:** `cutover/control.py:121-137`.

`user_switch` kills Quickshell, applies the Lua patch and reloads Hyprland before it starts the native unit. If `start`, `is-active` or the 10 s `wait_native` fails, the function raises with `phase=switching`. At that point:
- there is no bar;
- SUPER+A, the quickAdjust OSD and SUPER+SHIFT+R are bound to the absent native shell;
- `native.enabled` is armed, so the next login retries the broken shell.

Recovery takes the three-step rollback, including a sudo step, typed in a terminal the user may have to open blind. The README describes this ("leaves the journal for recovery"), but nothing restores the UI. Note the README's reason (no competing cooling loop) does not apply to the UI: legacy QML without the fan unit is safe while the helper owns cooling.

**Probe:** `PROBE_cut4_failed_native_start_leaves_no_shell`. Quickshell is killed, the patch applied, and there is no relaunch or revert.

**Fix:** on failure after the kill:
- stop the native unit;
- remove `native.enabled`;
- reverse the journal-owned patch;
- `hyprctl reload`;
- run legacy `launch.sh` with `fan-control-helper.sh restore` skipped, or with legacy fan writes disabled;
- record `phase=quiesced` so the root rollback is still required for cooling.

## CUT-5 [P1] Any edit near the patched lines blocks the whole rollback, including stopping native

**Location:** `cutover/control.py:83-88` (`patch_state`) and `:141` (called first in `user_rollback`, also for `--quiesce-native`).

`user_rollback` calls `patch_state()` before anything else. If the three lua files match neither the exact pre-image nor the exact post-image, it raises "drifted from both patch states". Adding one autostart line next to the patched `exec_once` is enough. The user edits these files actively (`controls.lua` is modified in the working tree now).

Then step 1 of the documented rollback, `--quiesce-native` (stop native, remove the marker), refuses. Step 2, root rollback, refuses because native is still running. The safety net is blocked end to end, while the cooling helper and native shell keep running.

**Probe:** `PROBE_cut5_unrelated_hypr_edit_blocks_even_native_quiesce`. One added line after the patched `exec_once` gives `RuntimeError` on `rollback --quiesce-native`.

**Fix:**
- Do not check patch state in the quiesce-native path.
- In the full rollback, restore units and launch the UI even when the patch cannot be reversed. Try a 3-way reverse first (`git apply --reverse --3way` or per-hunk `--reject`). Failing that, print the exact lines to revert by hand.

The rollback must never depend on config files staying pristine.

## CUT-6 [P3] Partial root install leaves a mixed set of files with no restore

**Location:** `packaging/install.py:52` and `copy()`.

Files are replaced one at a time and an error stops the loop. In the probe, an I/O error on the 4th destination leaves:
- new `hyprv-privhelper`, `hyprv-cooling-failsafe` and unit file;
- old D-Bus activation file, policy and polkit files.

Nothing puts the backups back. The helper is still masked, so the half-installed set is inert, and re-running install heals it. But the following root switch would start a new binary against old D-Bus and polkit policy if the user does not notice the error. A first install (no backups) after a partial failure also leaves files that rollback never removes. The README documents that rollback leaves installed files in place.

**Probe:** `PROBE_cut6_partial_root_install_leaves_mixed_generation`.

**Fix:** on error, restore the destinations already replaced from the backups (or remove them on first install), then re-raise. Or record an "install incomplete" marker that the switch refuses.

## CUT-7 [P3] `hypr-inputs.json` lists a preserved corner layer rule that does not exist

`hypr-inputs.json` lists `hyprv-screen-corner-overlay-*` among the namespaces whose layer rules are kept. The README says the same. No rule in `hyprland-winrule.lua` matches it, so there is nothing to preserve. It is harmless, but the evidence file asserts something untrue.

The native namespaces also disagree with each other:
- `hyprv-ui-bar/src/surfaces.rs:84` uses `hyprv-screen-corner-overlay-{output}-{corner:?}`;
- `hyprv-shell/src/host.rs:992` and QML use `…-{output}`.

This is routed to hyprv-ui-bar.

**Probe:** `PROBE_cut7_inputs_claims_corner_layer_rule_that_does_not_exist`.

## Notes (not findings)

- **Legacy entry points the patch leaves unpatched** (they keep working because legacy scripts stay installed, but they bypass the native shell):
  - `hypr/scripts/toggle-battery-mode.sh` → quickshell `set-variant.sh`
  - `rotate-screen-ccw.sh` → `deskflow-target.sh`
  - `restore-session-layout.sh` → `qs --path …/StartupConfirm.qml`
- **Rollback launch environment.** Rollback relaunches Quickshell from the terminal's environment, not Hyprland's exec-once environment, so `QSG_*` and session variables come from the terminal. `launch.sh` uses `-d`, so the call does not block.
- **Hard-coded owner.** `OWNER='wilf'` is hard-coded, as the README states.
