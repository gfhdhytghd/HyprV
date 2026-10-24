# Prepared cutover — NOT applied

Everything here is a plan until a human explicitly runs `--apply`. Install/switch/rollback scripts default to dry-run and never call sudo. The Lua patch has not been applied; no compositor reload, service stop/start, installation, root execution or hardware access was performed while preparing this package.

The final candidate is **READY for installation** under explicit `--candidate` scope; actual installed-artifact admission remains separate until the user installs it. See `docs/verify/install-gate-refresh.md` for release ID, SHA-256, full test/parity/performance evidence and the active-owner fan policy. A/B mode is **unsupported** in the current CLI: `switch.sh --ab-output DP-4` prints why and exits 4 without actions. Native assembly mounts all outputs; QML also lacks a corresponding output exclusion contract. `--no-exclusive` does not isolate outputs and is not used as a substitute. Both output filters and explicit IPC/output routing are needed before safe coexistence is possible.

Candidate-only admission: `cutover/preflight.sh --candidate --run-tests` validates the staged release, runtime assets, full tests, current GL/full-screen/corner evidence, accepted performance and the manifest-verified helper/D-Bus/polkit install set. `READY (candidate; installation not certified)` authorizes no live switch by itself. After installation, `cutover/preflight.sh --installed` still checks the actual installed artifacts and helper; default cutover preflight remains strict. These scopes are explicit and cannot be combined.

## Deployment path validation

Releases record the persistent deployment checkout, never the frozen build directory. Snapshot builds must set `HYPRV_DEPLOY_WORKSPACE=/home/wilf/.config/HyprV/rshell`; missing, temporary, snapshot, symlink-to-temporary or incomplete deployment roots are rejected. The builder, installer, staged root controller and preflight share `packaging/release_paths.py`. Installed metadata also binds the workspace and owner. Only the actual user preflight is run from that workspace; no checkout code is executed as root.

For changes confined to packaging/cutover, `packaging/build-release.sh --repackage` rebuilds the hash-manifested release while preserving each exact ELF. It rejects any changed non-packaging input (including runtime assets and the release profile), verifies all ELF hashes, records the prior build receipt's SHA-256, and retains that original receipt as historical evidence. The resulting manifest has a **new release ID**: restage and reinstall that release before the root handoff. Do not reuse the previously installed release or its root commands. Historical receipts can retain original build paths; active release metadata and commands cannot depend on them.

`python3 -B packaging/rehearse-handoff.py --out cutover/evidence/deployment-path-handoff.json` safely imports the actual staged controller, resolves its TREE, intercepts the switch's first action, and executes the real `--installed` preflight as the current desktop user. It simulates only the root/trust context and never runs runuser, systemctl, Hyprland dispatches or hardware operations. Any real preflight blocker remains visible. The assets rehearsal independently installs into scratch HOME with a stubbed systemctl.

## Files and ownership

- `../packaging/release.toml`: explicit release settings (opt-level 3, fat LTO, one codegen unit, abort panic, no debug/incremental, stripped debuginfo). Root Cargo.toml was already compatible and is unchanged.
- `../packaging/build-release.sh`: local build and staging only, with a source/binary SHA256 receipt under packaging/dist; never executes a built binary.
- `../packaging/install-user.sh`: installs `~/.local/bin/hyprv-shell`, `~/.config/systemd/user/hyprv-shell.service`, and the hash-verified catalog plus 44 runtime payloads under `~/.local/share/hyprv-shell/{assets,icons}`; no root, enable, start or marker creation. The static unit has graphical-session ordering/PartOf, environment conditions, bounded restart policy and graceful IPC stop. This session's graphical-session.target is inactive; the unit does not try to activate it. Hyprland imports the real session environment before explicit start. A cutover-owned marker prevents accidental startup merely from installation.
- `../packaging/install-root.sh`: run as the desktop user (it refuses root). It **only prints** the root procedure: stage a root-owned copy of `packaging/dist/release` with system tools, verify it with `sha256sum` against the release id printed by build-release, then run the staged `install.py` with `/usr/bin/python3 -I`. No user-writable file is ever executed as root. The root install is a journalled transaction: new files are written beside the destinations, old ones are backed up, then everything is renamed into place. Any failure, or an interruption found on the next run, restores every file. Re-running is idempotent. Before cutover it masks the helper. After cutover it never masks; it restarts an enabled helper on the new files and restores the old files if the helper does not come back.
- `../packaging/build-release.sh`: also assembles `packaging/dist/release/` (binaries, dereferenced unit/D-Bus/polkit files, `install.py`, `control.py`, unit inventory, patch, `release.json`) with `MANIFEST.sha256`. The SHA-256 of the manifest is the **release id**, which it prints.
- `hypr.patch`: git diff against the HyprV root, touching hyprland-exec-once.lua, hyprland-bind.lua and controls.lua, plus three helper scripts. Eight legacy call sites become native IPC, user-unit operations or `hyprv-shell startup-confirm`:
  - `scripts/toggle-battery-mode.sh`: `set-variant.sh` becomes `hyprv-shell desktop setVariant`;
  - `scripts/rotate-screen-ccw.sh`: `deskflow-target.sh reflow` becomes `hyprv-shell desktop deskflowReflow` (still rolls back the rotation on failure).

  Both use the running shell over IPC and run in-process when no shell is reachable.
  - `scripts/restore-session-layout.sh`: `qs --path StartupConfirm.qml` becomes `hyprv-shell startup-confirm`.

  File modes are kept, and rollback restores all six files byte for byte. `hypr-inputs.json` records exact pre/post hashes. The unrelated local-AI launch.sh is preserved. The native bar intentionally keeps `hyprv-quickshell`; popups keep `shell:.*`. There is no layer rule for `hyprv-screen-corner-overlay-*`, so none is preserved. Existing layer match rules are unchanged, including their historical display names.
- `legacy-units.json`: exact unit inventory and scope. Only the main Quickshell config is selected; DankCal and unrelated Quickshell instances remain running.
- `preflight.sh`: read-only by default, fails closed on absent, stale, partial or debug evidence and ineligible performance-input exclusions. Only the named unreachable battery parity states use the documented QUIRK exclusion. `--run-tests` explicitly runs the configured automated suite and records it; ignored opt-in live/device tests are not silently claimed as passed.

Root install destinations (all root-owned; binaries 0755, policy/unit files 0644):

| Source | Destination |
|---|---|
| staged hyprv-privhelper | /usr/libexec/hyprv-privhelper |
| staged hyprv-cooling-failsafe | /usr/libexec/hyprv-cooling-failsafe |
| canonical helper unit | /usr/lib/systemd/system/hyprv-privhelper.service |
| D-Bus activation | /usr/share/dbus-1/system-services/org.hyprv.Shell.PrivHelper.service |
| D-Bus policy | /etc/dbus-1/system.d/org.hyprv.Shell.PrivHelper.conf |
| polkit actions | /usr/share/polkit-1/actions/org.hyprv.shell.fan-control.policy |

The root install also writes:
- before cutover only: `/etc/systemd/system/hyprv-privhelper.service -> /dev/null` (`systemctl mask`);
- `/var/lib/hyprv-cutover/` (0700): `install.json` (transaction journal), `install-tx/<tx>/` (backups for restore, the last two generations), `install-backup/` (first-ever originals, never overwritten);
- `/usr/local/lib/hyprv-cutover/releases/<id16>/` (the staged release), `installed.json` (public record of the installed release and file digests, read by preflight) and `current -> releases/<id16>`.

It installs a scoped action policy: authorized active owners can change fan limits without a password prompt; eGPU removal requires administrator authentication. The canonical unit creates `/var/lib/hyprv-privhelper` only on later activation, runs the independent cooling failsafe before start and after stop, and has the owner's watchdog policy. Root installation does not import old user-writable fan state. Helper desktop identity comes from the release (`release.json` owner, currently `wilf`); this is not a generic multi-user installer.

**Trust model.** Root only executes files that are root-owned, under a root-owned path, not group/other-writable, not symlinks, and whose digests match a manifest whose own digest you compared with the id printed at build time. This closes the window between build and root execution. It does not protect against an account that was already compromised before the build: that account controls the sources too.

## Review now (safe plans)

Run from rshell:

```sh
packaging/build-release.sh --dry-run
packaging/install-user.sh --dry-run
packaging/install-root.sh --dry-run
cutover/switch.sh --dry-run --quiesce-legacy
cutover/switch.sh --dry-run --scope system
cutover/switch.sh --dry-run
cutover/rollback.sh --dry-run --quiesce-native
cutover/rollback.sh --dry-run --scope system
cutover/rollback.sh --dry-run
cutover/preflight.sh
```

Patch review/check only: `git -C .. apply --check "$PWD/cutover/hypr.patch"`. Neither dry-run nor preflight applies it. Do not apply it independently if using the journalled switch: that switch owns and reverses its own patch; unknown preexisting applications or drift are refused.

## Future manual deployment — do not run while gates are red

These commands are instructions for the user, not actions performed during preparation:

1. Build: `packaging/build-release.sh`. It stages shell/helper/failsafe and refuses a receipt if source changes during the build.
2. Run `HYPRV_ASSET_ROOT="$PWD/packaging/dist/release/runtime" cutover/preflight.sh --run-tests` before installation (after installation, the unit configures the stable asset root). It executes workspace all-targets + doctests, xtask tests and cutover/probe Python tests; non-test gates can still be red. Fix parity/performance separately and regenerate their reports against the same sources. The idle aspiration remains **<=0.1% CPU**. The explicit user acceptance in `perf-acceptance.json` permits this exact candidate up to **0.5% controlled quiet** and **2.0% live-hook idle**. Both intervals must be uninstrumented and >=60s, with scheduling evidence and reported hook counts; controlled quiet requires zero hooks. Live idle must use the host namespace and unfrozen inputs. Release provenance, runtime source fingerprints, the measured/staged (and, with `--installed`, installed) binary hashes and the updated report must match. Measurements expire after 24 hours. Invalid acceptance, a changed binary, missing evidence or an exceeded bound fails closed; no probe result can use the acceptance. Readiness-only policy edits are separately hash-recorded in `evidence/readiness-amendment.json`; the frozen build receipt and binary remain unchanged, and the full automated suite must pass against the current tree.
3. Review the plans, then run `packaging/install-user.sh --apply` as the user. For root, run `packaging/install-root.sh` as the user and follow the printed commands:
   ```sh
   sudo /usr/bin/install -d -o root -g root -m 0755 /usr/local/lib/hyprv-cutover/releases
   sudo /usr/bin/cp -R -P -T -- packaging/dist/release /usr/local/lib/hyprv-cutover/releases/<id16>
   /usr/bin/sha256sum /usr/local/lib/hyprv-cutover/releases/<id16>/MANIFEST.sha256   # must equal the release id
   /usr/bin/sh -c 'cd /usr/local/lib/hyprv-cutover/releases/<id16> && /usr/bin/sha256sum --strict --quiet -c MANIFEST.sha256'
   sudo /usr/bin/python3 -I -B /usr/local/lib/hyprv-cutover/releases/<id16>/packaging/install.py root --release <release-id> --apply
   ```
   The user installer never starts the shell. Before cutover, root installation keeps the helper masked/inactive. In an existing journalled `native` phase with the helper enabled, changed root files trigger daemon-reload, helper restart and an active-state check; restart/health failure restores the old files, reloads and restarts the previous helper. This upgrade path is covered by mocked failure regressions. Check `cutover/preflight.sh --installed`; all checks must be green before proceeding. Preflight also checks that the installed release equals the candidate in `packaging/dist/release`.
4. From a graphical-session terminal: `cutover/switch.sh --apply --quiesce-legacy`. It records original user-unit states (including their exact symlinks) and whether the main Quickshell was running, disables/stops installed legacy user controllers, and kills only that Quickshell config.
5. Root, from the staged release only: `sudo /usr/bin/python3 -I -B /usr/local/lib/hyprv-cutover/current/cutover/control.py switch --scope system --apply`. It verifies the staged tree and that it is the installed release, runs preflight as the desktop user, checks user quiescence, and records original system-unit states with their symlinks plus the four nct6799 PWM ownership/mode values. It then disables/stops the listed legacy system units (masked/static/disabled ones are only stopped), runs the maximum-cooling failsafe, revokes the old user sysfs grants (root:root 0600), unmasks/enables/starts the new helper and waits for notify-ready. Unmanaged legacy fan loops are rejected. Re-running it when already native repairs a masked or stopped helper. `cutover/switch.sh` refuses to run as root.
6. From the graphical-session terminal: `cutover/switch.sh --apply`. It verifies helper active / old system units inactive, backs up the six patched files to `~/.local/state/hyprv-cutover/hypr-backup/`, applies its exact Lua patch once, reloads Hyprland, arms native startup, imports session variables and starts the user unit. **Ready** means `hyprv-shell ipc call bar dataStatus` answers and `hyprctl -j layers` shows a `hyprv-quickshell` layer owned by the unit's MainPID on every enabled monitor, within 20 s. Otherwise it falls back automatically: stop native, remove the marker, reverse its hypr edit, `hyprctl reload`, launch legacy Quickshell. The journal is left at `phase=fallback` and cooling stays with the helper. Fix and rerun the switch, or roll back.

No background migration touches VM, network, media, Mihomo, Viewflow, DankCal, portals or other unrelated services. User legacy inventory: **hyprv-fan-control.service**, plus source-name alias **fan-control.service** only if installed (canonical aliases are deduplicated). System legacy inventory: **hyprv-fan-control.service**, **hyprv-fan-permissions.service**, **hyprv-cpu-power.service**. The last is the root turbostat publisher replaced by native statistics. Original enabled/runtime-enabled/disabled and active states are journalled, not guessed on rollback.

## Future manual rollback

Rollback remains available even if tests/parity/performance have turned red:

1. User: `cutover/rollback.sh --apply --quiesce-native` removes the startup marker and stops the native unit. It never looks at the hypr files, so config edits cannot block it.
2. Root, from the staged release: `sudo /usr/bin/python3 -I -B /usr/local/lib/hyprv-cutover/current/cutover/control.py rollback --scope system --apply`. It requires native stopped and validates cooling hardware identity. It disables/stops the helper, runs the failsafe, masks the helper, and restores exact prior PWM owner/group/modes. It then recreates exactly the journalled unit symlinks (enabled, enabled-runtime, linked, alias; masked units were never touched), permissions unit first, starts what was active, and compares unit-file states. Any mismatch is printed and exits 2. Masking prevents later D-Bus autoactivation alongside legacy control. Installed files and protected helper state remain for a future reviewed cutover.
3. User: `cutover/rollback.sh --apply` **always** stops native and restores the legacy UI. The hypr edit is reversed exactly when possible. Otherwise each patched line that still appears exactly once is put back by content, keeping your other edits. Any line it cannot place is printed with the path of the pre-cutover backup and exits 2, but never blocks the rest. Then it reloads Hyprland, restores the journalled user-unit links (if the root helper is still active it leaves them disabled, saves `phase=ui-restored`, and asks you to rerun after step 2), and launches legacy Quickshell unless it already runs. Repeated completed rollback is a no-op. New switch transactions recapture baseline state; interrupted ones resume without overwriting their original snapshots.

**Failure phases.**
- `fallback`: the legacy UI is back after a failed native start.
- `fallback-blocked`: native could not be stopped, so no legacy UI was started. Stop it, then rerun switch or rollback.
- `rollback-incomplete`: rollback reported problems; rerun the same rollback command until it exits 0.
- `ui-restored`: user rollback ran while the root helper was still active.

Only exit 0 with `phase=legacy` means done. `/var/lib/hyprv-cutover/lock` is the single root orchestration lock: root install, root switch and root rollback never run concurrently.

Journals: user `~/.local/state/hyprv-cutover/state.json`, root `/var/lib/hyprv-cutover/state.json`, with 0700 directories, 0600 files, no-follow opens, atomic replacements and file locks. Cooling restore uses fixed attribute names on the same physical nct6799 identity; changed/missing hardware requires explicit recovery, not guessed writes. A kernel/uninterruptible-device failure can exceed software handoff guarantees; no live or physical recovery proof is claimed here.

Performance preflight now prefers `docs/verify/raw/perf-shell-latest.json` when available: it validates the archived actual release `hyprv-shell` ELF and all UI/service source fingerprints, then checks the real idle/activity windows. Debug/instrumented, short, frozen-input or private-PID measurements cannot establish a full-host idle pass; a ≥60s zero-hook controlled quiet failure also blocks the budget. The older service-probe report is retained as a fallback/historical diagnostic. See `docs/verify/perf-shell.md` for the separate agent-event conditions and raw evidence. CI can refresh a coherent staged candidate without installing it.

## Production parity evidence (FR-NET-2)

On a coherent candidate, stage first, then generate the component GL and assembled receipts:

```sh
packaging/build-release.sh
python3 -B parity/verify/verify.py --backend gl
python3 -B cutover/verify_fullscreen.py --binary packaging/dist/hyprv-shell --run cutover/evidence/fullscreen-run
cutover/preflight.sh --run-tests --json
```

Use an empty `--run` directory. These commands build/test and start only private test compositors; none installs or switches the live desktop. The final preflight can still report a missing helper and unmet performance evidence. It never installs a helper to make that check pass.

Only `*-gl` component slots qualify. Every source/native/asset/replay/reference dependency and every actual PNG has a SHA-256 receipt. Changed dependencies, incomplete renders, duplicate/missing images and CPU-only evidence block readiness. `--no-render` cannot renew a stale render receipt. The verifier and preflight share only the documented unreachable battery-state exclusions; their PNG coverage must still be complete. No other state can declare itself excluded, and the canonical reference decides whether text tolerance is allowed.

The eight delegated calendar/island component crops must pass at both component DPRs and both themes. A separate same-release assembled matrix checks all canonical full-output references, including those two hosts, with exact unmasked corner crops. Current full-screen references cover DP-4 @2 only; this does not claim DPR-1 full-screen coverage. Non-text remains exact; no masks or older broad tolerance substitute for the exact corner gate.

## Runtime asset contract

The release bundles the original image assets and the Fluent SVG subset used by the production loader. `runtime-assets.json` is embedded in the ELF and copied with the payload. `install-user.sh --apply` verifies the complete release manifest before writing, installs assets/binary/unit in the same journalled transaction, then verifies destination hashes before the user daemon reload. No start/enable or root action is added.

The unit sets `HYPRV_ASSET_ROOT=%h/.local/share/hyprv-shell`. This root contains `runtime-assets.json`, `assets/` and `icons/`; it is the parent of the image directory. Without an override the binary uses the same stable HOME-relative root. `hyprv-shell --check-assets` validates every file against the ELF's catalog without starting providers or connecting to Wayland. Normal assembly performs the same validation before creating services/IPC/windows; missing, corrupt, mismatched or non-regular payloads fail with a named error and reinstall instruction. There is no checkout fallback. IPC and help remain usable without assets.

Preflight's **runtime assets** gate verifies the staged bundle and unit environment, then executes the staged ELF's asset-only check. `--installed` additionally verifies the actual HOME assets. `python3 -B packaging/rehearse-assets.py --out REPORT.json` rehearses the real user installer in a temporary HOME with only systemctl stubbed, reads the installed unit's environment/ExecStart, and runs the installed ELF from `/`. It checks idempotence, default/explicit roots, missing/corrupt assets and missing catalog failures. Scratch HOME is removed afterward; no host unit is reloaded or started.

Fan authorization matches the user-approved legacy behavior: `org.hyprv.shell.fan-control` has `allow_active=yes`, with `allow_any=no` and `allow_inactive=no`. The helper still checks the bus caller UID against the desktop owner/root and invokes polkit for the caller's active local session before changing a limit. This removes the password prompt for the authorized active owner. `org.hyprv.shell.egpu-remove` retains `allow_active=auth_admin`; CPU-power telemetry remains read-only with active-owner authorization. The policy is a manifest-verified root install input, not a user-session override.

## Visual acceptance — user decision 2026-10-24

`parity-acceptance.json` records user sign-off for the exact shell SHA-256, the date/note, and hash-pinned historical release/preflight receipts from the last fully passing visual gate. Preflight reports `PASS (accepted: ...)` without requiring new captures or dependency freshness. Missing/tampered proof, an invalid acceptance, or a different shell SHA fails closed. This records human acceptance rather than claiming a new render passed. Removing the acceptance restores the existing strict render gate; the component/full-screen/corner tooling remains available on demand with unchanged comparators. Automated tests remain mandatory and source-fresh. Readiness-only edits are audited separately against the unchanged installed build receipt; runtime source differences cannot use that amendment.
