# Cutover rehearsal in isolation

2026-10-23, svc-desktop. I ran the user-level cutover for real (install, `switch.sh --apply`, two injected start failures with automatic fallback, `rollback.sh --apply`, and a drift cycle) against a nested Hyprland. That instance ran inside a sandbox that cannot reach the live session. The root-level steps were run only as dry-runs or refusals; their outcome was simulated.

**Result.**
- **Native start works:** IPC answers, and the bar is mapped by the native process on both nested outputs.
- **Fallback works:** both injected faults fell back to legacy Quickshell, after one bug was fixed (CUT-8, below).
- **Restore is byte-identical:**
  - rollback restored the scratch configs byte for byte: all 64 hypr files, the systemd user dir with its `enabled` link and a `linked` out-of-tree unit link, and the runtime dir;
  - unit states, the Quickshell process and layers, binds and exec-once also came back.
  - The only intended difference is that the installed `hyprv-shell.service` stays in place (rollback does not uninstall).
- **The live session was untouched,** checked against a before/after snapshot.

Evidence is in [raw/cutover-rehearsal/](raw/cutover-rehearsal/):
- `steps/NN-*.log`: every command's output, in order;
- `snapshots/*.json`;
- `compare-*.txt`;
- `stub-calls.jsonl`: every systemctl call, 394 of them;
- `harness/`: everything needed to rerun it;
- `live-*.txt`: snapshots of the live session.

## Isolation

Everything runs in one `bwrap` sandbox (`harness/start-sandbox.sh`):

| Resource | In the sandbox |
|---|---|
| Filesystem | `/` read-only. Only the scratch dir is writable (HOME = `scratch/rehearsal/home`). Private `/tmp`. |
| `/run` | Private tmpfs. `XDG_RUNTIME_DIR=/run/user/1000` is bound to `scratch/rehearsal/run`. The live Wayland, session-bus, user-manager and system-bus sockets do not exist inside. |
| D-Bus | Private session bus and a private "system" bus at `/run/dbus/system_bus_socket`, with no services. NetworkManager, BlueZ, UPower, logind and the privhelper are unreachable, so no agents are registered and no system actions are possible. |
| Processes | Own PID namespace, so `/proc` scans (`qs_running`) and kills see only sandbox processes. `--die-with-parent`. |
| Hardware | `/sys` read-only, minimal `/dev`, only `renderD128` plus the NVIDIA control nodes (needed for EGL). No DRM primary node (so no modeset or master), no i2c (DDC), no hwmon or backlight writes. Legacy `fan-control-helper.sh restore` and Hyprland's control worker ran and failed harmlessly. |
| Compositor | `kwin_wayland --virtual` (headless) as the host, with a nested **Hyprland 0.56.2** (Wayland backend) on top, outputs `WAYLAND-1` and `WAYLAND-2`. Weston was rejected because it offers `wl_compositor` v5 and Hyprland needs v6. A headless Hyprland output could not allocate GBM buffers on NVIDIA in the sandbox, so the second output is a second Wayland-backend output. |
| Hyprland config | `harness/hyprland-rehearsal.lua` loads the real scratch `hyprland-exec-once.lua`, `hyprland-bind.lua` (which also pulls in `controls.lua`) and `hyprland-winrule.lua`. **exec-once commands are recorded, not run** (no real autostart apps). Bind commands are recorded at load and still registered. |
| systemd --user | **Stubbed.** A private user manager is not feasible: it needs a writable, delegated cgroupfs, and that would mean writing to the live session's cgroup tree. `harness/systemctl-stub.py` is bind-mounted over `/usr/bin/systemctl`. In user scope it emulates a manager from the real unit files and symlinks: enable/disable/link/mask create and remove the same symlinks systemd does; `start`/`stop` really spawn and stop processes with `ConditionPathExists`/`ConditionEnvironment`, `Environment=`, `ExecStop` and `TimeoutStopSec`; import-environment is supported. Not emulated: `Restart=` and cgroups. **System scope is read-only and every mutation is refused.** |

**Simulated or bypassed, on purpose:**
- **Root steps.** `install-root.sh` printed its staged procedure. Root switch/rollback ran as plans, and `--apply` as the user was refused ("system scope requires root"). Their outcome (helper active, legacy system units disabled, and back) was written into the stub's `system.json`.
- **Preflight gate.** It is bypassed in the scratch tree only (`cutover/preflight.py` replaced; the real one is kept beside it). Its parity, perf and root-helper gates are red by design and are not what this rehearsal tests.
- **Binaries were not rebuilt.** A scratch `build-release.sh` failed because the live tree was mid-edit by its owner (`hyprv-shell`: `#[path="replay.rs"]` with the file missing, plus E0004). The release was assembled with the real `build_release.assemble()` from the consistent `target/release` binaries (hyprv-shell 07:34). See `build.json`. Release id `96dde6cc…3991cc09`.
- **Legacy unit fixtures.** The scratch user units are `sleep` fixtures with the legacy names: `hyprv-fan-control.service` is **enabled + active** (wants link) and `fan-control.service` is **linked** (out-of-tree symlink) + inactive. That is a harder mix than live, where the user unit is plain disabled.

## Steps

The ten steps below are a selection of 43 logs; the full set is in `steps/`.

| # | Step | Result |
|---|---|---|
| 00 | Baseline S0 | Legacy Quickshell maps `hyprv-quickshell` and corner layers on WAYLAND-1 and WAYLAND-2. Binds and exec-once are the legacy commands. |
| 02, 03 | `install-user.sh --apply`, then again | 2 files replaced, journalled; the re-run replaces 0 files. |
| 04, 05 | `install-root.sh`; `install.py root --apply` as user | Plan printed; refused with "root install requires root". |
| 06 | `switch.sh --apply --quiesce-legacy` | Links journalled, then `disable --now` on both units (the linked unit's link removed). Quickshell killed. |
| 08–10 | Root switch plan; `--apply` as user; stub system mutation | Plan printed; both refused. Root handoff simulated. |
| 11, 15 | **Fault 1: hang.** Installed a release whose `hyprv-shell` never answers, then switched. | Exit 1 after **31 s** (20 s deadline plus 10 s hung ExecStop). Patch reversed exactly, reload, Quickshell relaunched, `phase=fallback`. hypr files, layers, binds and exec-once match S0. |
| 17, 18 | **Fault 2: crash.** Installed a release whose `hyprv-shell` exits 1, then switched. | Exit 1 after **21 s**, the same fallback, matches S0. |
| 20, 24 | Good release reinstalled; `switch.sh --apply` (resumes from `fallback`) | Native ready in under 1 s. "bar mapped on WAYLAND-1, WAYLAND-2". `phase=native`, marker set. |
| 25–29 | Verify native | <ul><li>IPC `bar dataStatus` returns status JSON.</li><li>The native PID owns `hyprv-quickshell` plus 4 corner layers per output.</li><li>Binds and exec-once are the patched native commands.</li><li>`controls.lua brightness_set(40,true)` maps `shell:hyprv-quick-adjust` through the patched `notify()`.</li><li>The SUPER+A command string maps `shell:hyprv-control-panel`.</li><li>Re-running the switch is a no-op.</li></ul> |
| 30–36 | `rollback.sh --apply --quiesce-native`; root rollback plan, refusal and simulation; `rollback.sh --apply`; repeat | Native stopped. Patch reversed exactly, reload. **Exact links recreated** (`ln -s` of the wants link and the linked unit). Previously active unit started, Quickshell relaunched, `phase=legacy`. The repeat prints "No pending user cutover". **S0 vs end: identical** except `hyprv-shell.service` now being installed. |
| 37–42 | **Drift cycle:** cutover again, add a user line right after the patched `exec_once` while native runs, then roll back | Exact reverse impossible, so 5 lines (1 + 3 + 1) were reverted by content. The user's line was kept (`snapshots/exec-once.after-drift.lua`). With that line removed, everything matches S0 except `hyprv-shell.service` (`compare-S0-vs-drift-end.txt`). |

## Round 2: rerun after the CUT-R2 fixes (same sandbox)

After fixing CUT-R2-1..6 ([packaging-cutover-r2.md](../reviews/packaging-cutover-r2.md)), I restarted the sandbox and reran the whole cycle with the new `cutover/control.py`. Logs are in `steps-r2/`, snapshots in `snapshots/R2-*.json`, and the comparison in `compare-R2-S0-vs-end.txt`. The round-2 baseline `R2-S0` is a legacy "login" (fixture unit started, Quickshell launched the way exec-once does). It equals S0 except for the already-installed `hyprv-shell.service`.

| # | Step | Result |
|---|---|---|
| 02–03 | Re-install (no-op), quiesce-legacy, root switch plan plus simulation | 0 files replaced; units journalled and quiesced. |
| 04–06 | Hang fault | Exit 1 after 31 s, fallback. hypr files, layers, binds, exec-once and processes match R2-S0. |
| 07–08b | Crash fault | Exit 1 after 21 s, the same result. |
| 09–11 | Good release, switch | Native ready in about 1 s on WAYLAND-1 and WAYLAND-2. IPC answers. |
| 12–14 | Quiesce-native, root rollback plan plus simulation, then **full rollback with a unit fault** (the active legacy unit's ExecStart points to a missing file) | Exit 2 with `phase=rollback-incomplete` and the problem `hyprv-fan-control.service: start failed`. Patch reversed, links recreated, **legacy Quickshell still restored** on both outputs (CUT-R2-3/4). |
| 15–17 | Fixture repaired, then rollback rerun twice | The retry completes the outstanding start, giving `phase=legacy` and exit 0. The next run prints "No pending user cutover". **R2-S0 vs end: every section identical**, unit states included. |

Not reproducible in the sandbox, so covered by unit tests only:
- the "native cannot be stopped" path (`fallback-blocked`, CUT-R2-2), since the stub's SIGKILL always succeeds;
- the root lock race (CUT-R2-6), which needs root;
- a crash after commit (CUT-R2-1), which is a root-install path.

The live session was unchanged again (`live-after-rehearsal-r2.txt` is identical to the pre-rehearsal snapshot).

## Round 3: fresh release build, full rerun (unattended driver)

The workspace release build is green (CI round 12 release and stage-release PASS, and the current tree compiles again). I rebuilt everything from scratch:
- a **fresh scratch HOME**, copied from the live `hypr/`, `quickshell/` and rshell sources at 08:10;
- new fixtures and a reset stub manager;
- a **real `packaging/build-release.sh`** in the scratch tree. It took 1 m 43 s and the sources did not change during the build. Release **`9b77a8a0…c2fe8169`**, fresh binaries `hyprv-shell` 40 MB at 08:12; see `round3/build.json`.

The fault releases were derived from that release with regenerated manifests. `harness/run-rehearsal.sh` then ran all 38 steps unattended, each with an expected exit code. `round3/summary.txt` has the table: **38/38 as expected, 0 mismatches**.

- **Install:** applied, then re-applied as a no-op. Root install is plan only, and applying it as a user is refused.
- **Switch and fallback:** quiesce; root switch plan, refusal and simulated outcome. The hang fault fell back after 31.0 s and the crash fault after 21.0 s. Both fallback states match S0 in hypr files, marker, processes, layers, binds and exec-once.
- **Native on the fresh binaries:** ready in 0.6 s ("bar mapped on WAYLAND-1, WAYLAND-2"). IPC ping works. `controls.lua` maps `shell:hyprv-quick-adjust`, and SUPER+A maps `shell:hyprv-control-panel`. Re-running the switch is a no-op.
- **Rollback:**
  - quiesce-native; root rollback plan, refusal and simulation;
  - a rollback with a broken legacy unit exits 2 with `rollback-incomplete`, and its UI state still matches S0;
  - after the repair the retry exits 0, and the rerun reports "No pending".
- **End state vs S0:** the only difference is `hyprv-shell.service`, which is installed and stays by design (`cmp-S0-E.txt`).
- **Drift cycle:** with the user edit next to the patched line, 5 lines were reverted by content and the edit kept. With the edit removed, everything matches S0 except that same unit (`cmp-S0-D2.txt`).
- **Fresh hypr backups (CUT-9):** `~/.local/state/hyprv-cutover/hypr-backup/*` equal the legacy files of the *latest* application. That fix comes from this round: `backup_hypr()` used to keep the first-ever copies forever. It now rewrites them atomically before each new patch application; there is a regression test, and the cutover suite is 50/50.

The live session was unchanged (`round3/live-after.txt` is identical to the pre-rehearsal snapshot), and live Quickshell is still PID 3193809.

## Round 4: six-file patch, no system bus, native script entry points (fresh release `0b9fe10f…`)

This round verifies two changes:
- the shell now degrades without a system bus;
- the three legacy hypr scripts are ported, so `hypr.patch` now covers six files.

The scratch HOME was rebuilt from live, and the release was built with `build-release.sh`. Earlier ad-hoc sessions in this sandbox had left the scratch hypr tree half-patched, so I reset it to pristine before this run. The main driver (38 steps) and `run-extra.sh` (25 steps) ran unattended: **63/63 as expected, 0 mismatches**. The only end-state difference from S0 is `hyprv-shell.service`, which is installed and stays by design. See `raw/cutover-rehearsal/round4/`.

| Area | Result |
|---|---|
| Six-file patch | Switch, both fault fallbacks, rollback and the drift cycle all restore all six files byte for byte, modes included. |
| `toggle-battery-mode.sh` (native) | `variant=bt` and the battery Lua link, then back to `v2`; both exit 0. |
| `rotate-screen-ccw.sh` | <ul><li>No Deskflow config: the service error comes back through `hyprv-shell desktop deskflowReflow`, and the script restores transform 0 (exit 1).</li><li>With a config: rotation to transform 3 succeeds, and so does rotating back.</li></ul> |
| IPC `desktop deskflowReflow` | OK. `setVariant daily` is rejected. |
| `hyprv-shell startup-confirm` next to the running shell | The `hyprv-startup-confirm` layer maps. Right+Return gives exit 0 with `launch` written; Escape gives exit 0 with nothing written. The main shell is unaffected. |
| No system bus at start | Native starts with bars on both outputs, logs `system bus unavailable …`, and IPC and panels work. |
| Bus appears | Detected after 4 backoff attempts; re-exec keeps the **same PID**, the bars come back and IPC works. |
| Bus lost while running | `connection lost` is logged and the UI stays alive. When the bus returns, the shell re-execs again. |
| `hyprv-shell desktop …` with the shell stopped | Runs in-process ("not running; applying … directly") and writes `variant=bt`. |

**New finding (P1, routed to engine in `docs/requests/hyprv-ui.md`).** Rotating an output kills the native shell: `wp_viewport#24: error 0: Box doesn't fit`, after a busy loop of about 138 000 log lines. It reproduces with a bare `hyprctl eval` transform change. In step x05b, the reflow IPC was still answered before the shell died, so the rotation completed (transform 3, native MainPID 0 afterwards). The script entry point falls back in-process whenever no shell is reachable (x16b, `tests/desktop_cli.rs`), so SUPER+T never depends on the shell process.

## Round 5: rotation crash fixed in hyprv-ui (release `d2101105…`)

The release was rebuilt with the engine fix (docs/requests/hyprv-ui.md: EGL window recreated on any buffer-size change, painting stops after a fatal error, deduplicated error log, rotation-aware output size), and the scratch HOME was reset to pristine. Both drivers: **63/63 as expected, 0 mismatches**. The only difference from S0 is again the installed `hyprv-shell.service`.
- x05b and x05d now require the native shell to **survive** the real `rotate-screen-ccw.sh` rotations: same MainPID, bars on both outputs after rotating to 270° and back.
- The native log contains **no** viewport or protocol error.
- The live session is unchanged (`round5/live-after.txt`).
- The engine-level matrix (27/27) is in [e2e-rotation.md](e2e-rotation.md).

## Findings

- **CUT-8 [P1] (fixed): a hung IPC ping blocked the switch forever.** `wait_native()` ran the ping with no timeout. A server that accepts the connection but never replies (here the injected binary) blocked the switch indefinitely, so neither the deadline nor the fallback ran, and the user would be left with no shell. During the rehearsal I unblocked it by killing the hung sandbox ping, which let me observe the rest of the fallback.
  - Fix in `cutover/control.py`: every ping gets `timeout = min(5 s, time left)`.
  - Fallback now also verifies that native really stopped, and reports it if not.
  - Re-run (step 15): automatic fallback after 31 s.
  - Regressions in `cutover/tests/test_review_fixes.py`; the cutover suite is 40/40.
- **[P2 → hyprv-shell] Native shell exits without a system bus.** It quits with `Failed to connect to address unix:path=/var/run/dbus/system_bus_socket` instead of degrading. Legacy Quickshell only warns. In production, a dbus-broker restart or a missing bus during early login would take the bar down (the unit's `Restart=` only retries 3 times in 30 s). The cutover fallback handled it correctly (step 21). For the rest of the run the sandbox's private system bus was moved to the standard path. Routed in `docs/requests/hyprv-shell.md`.
- **[P3, cutover] A crash still waits out the full 20 s deadline.** Readiness polling could stop early once the unit is inactive and not restarting. Noted, not changed.
- **[Confirmed for hyprv-ui-bar] Corner surfaces differ from legacy.** Native creates 4 corner layers per output (`…-{output}-{Corner}`); QML creates one (`…-{output}`). This is the CUT-7 naming note. No layer rule matches either one.
- **Harness only:** the stub first crashed on an `ExecStop` timeout (fixed in the stub, matching systemd's kill-and-continue).

## Live session untouched

`live-fix-after.txt` (taken before the rehearsal) is identical to `live-live-after-rehearsal.txt`. Both snapshot:
- `~/.local/state/hyprv-cutover`, `/var/lib/hyprv-cutover` and `/usr/local/lib/hyprv-cutover` (all absent);
- `/usr/libexec/hyprv-*`, `~/.local/bin/hyprv-shell` and the user `hyprv-shell.service` (all absent);
- the helper mask link and the system and user unit states;
- the hashes of the live `../hypr/*.lua`, and `git status`.

The live Quickshell is still PID 3193809, and the live `hyprv-fan-control.service` is still active. The only host-side process I signalled was the hung ping inside the sandbox; another agent's perf `hyprv-shell --read-only` was not touched.

## Reproduce

1. Make a scratch HOME (reflinked copies of `hypr/`, `quickshell/` and the rshell sources) and an assembled release.
2. Create the two fixture units.
3. Run `harness/start-sandbox.sh` (which runs `sandbox-init.sh`).
4. Drive it with `harness/sbx '<command>'` in the order of `steps/`.
5. Snapshot with `harness/snapshot.py` and compare with `harness/compare.py`.

The harness paths are those of this session's scratchpad. Rebuild with `packaging/build-release.sh` once `hyprv-shell` compiles, and rerun to cover freshly built binaries.
