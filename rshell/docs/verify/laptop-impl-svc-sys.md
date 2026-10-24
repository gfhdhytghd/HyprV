# Laptop Phase B — svc-sys

2026-10-24. Assigned implementation complete. No real install, service operation,
Hyprland operation, branch checkout, Git staging/commit or push was performed.

Implemented:

- `hyprv-cpu-power`: separate read-only entry point and D-Bus endpoint
  `org.hyprv.Shell.CpuPower` / `/org/hyprv/Shell/CpuPower` /
  `org.hyprv.Shell.CpuPower1`, version1. Configured-owner + active-local polkit,
  no prompts, no caller paths, disconnect cleanup, no sampling without leases,
  5s idle exit. No controller, fan/eGPU methods or failsafe. Uses existing native
  RAPL arithmetic and engine-owned profile/sys_path API; no second selector.
- Desktop daemon and recovery entry points reject laptop cooling before controller
  construction. Native `ExecCondition` probes skip laptops; desktop late-module
  discovery and nct6799 recovery remain intact. Production root services reject
  caller-selected fixture profile/sysfs/bus environments.
- `build-release.sh --profile laptop [--with-cpu-power]`: hash-manifested laptop
  patch/unit selection, default user-only set; optional five-file CPU root set.
  Unit `hyprv-cpu-power-only.service` avoids the legacy turbostat unit name.
  CPU-only policies exclude fan/eGPU; mixed desktop policies/admin overrides fail
  closed. Root upgrades retain file rollback. User install validates profile,
  installs stable assets and the laptop unit, and does not start/enable the shell.
- Unapplied eight-file laptop patch maps brightness, Super+A, calendar/bar IPC,
  kill/reload, StartupConfirm and variant/rotation actions. Existing layer rules
  stay unchanged. User-only switch/rollback defaults to dry-run; verifies base and
  output hashes, journals files/legacy intent/marker, checks running ELF/profile,
  PID-owned layers and per-output IPC; restores on failure. No root-service handoff.
- Laptop preflight dispatch requires fresh tests, laptop/desktop functional proof,
  complete production-GL images and strict laptop resource evidence. Missing
  images, stale/binary-mismatched reports, numeric invalids and desktop waivers
  cannot pass. Receipt schema/commands: [laptop README](../../cutover/laptop/README.md).

| Validation | Result |
|---|---|
| Privhelper all targets | PASS:52 tests, plus the isolated bus child |
| Engine profile tests | PASS:5 |
| All cutover Python tests | PASS:150 (previous tests retained) |
| Helper Clippy all targets, `-D warnings` | PASS |
| Source-stable CPU-only release build + read-only fake-counter probe | PASS |
| Scratch-HOME unit/assets rehearsal | PASS; existing shell ELF, asset-location proof only |
| Patch forward/reverse, syntax | PASS:8 pinned inputs,5 Lua +3 shell files |

CPU component SHA-256: `80351fc449d18c2845d2c0ed0f1335f694872e65508f1a172625562068a5ecd5`.
This is **not an assembled shell release ID**. No new global candidate was staged.
[Logs and component receipt](raw/laptop-impl-svc-sys/),
[source checkpoint](raw/laptop-impl-svc-sys/sources.json).

Integration remains pending: no fresh desktop204/204 or desktop parity claim is
made here; UI/renderer/desktop patch were not edited. QA must publish the joint
laptop manifests/images/resource receipts; missing proof blocks admission. Fake
root layout must follow engine's API (`ROOT/sys/class/...`); QA/owners were notified.
No XPS hardware validation is claimed. Consumer wiring belongs to svc-power and
must use the distinct CPU endpoint under Laptop, never the combined helper.

Proposed branch/commit plan (not executed): keep shared dirty `main` intact, where
`rshell/` is untracked. In a separate worktree based on pinned laptop24972f7, review
and add an explicit common-rshell source/asset/fixture allowlist; then shared profile
and per-service changes; then CPU-only privilege, laptop packaging/cutover and
regression tests; finally integrated evidence and the laptop config patch after
joint gates. Exclude target/dist, scratch, raw captures and install journals; retain
all canonical manifest-listed references. Cherry-pick a reviewed common baseline
if it lands on main first; do not fork the shell or blanket-add the working tree.
The config patch remains unapplied here. Stopping this task as requested.
