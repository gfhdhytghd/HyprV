# Laptop release and cutover

Implemented packaging path; no installation is implied. Shared runtime profile
comes from `hyprv_core::profile`. Default is desktop. Laptop release options:

```
packaging/build-release.sh --profile laptop --dry-run
packaging/build-release.sh --profile laptop --with-cpu-power --dry-run
```

Omit `--dry-run` only for an explicitly coordinated build/stage. Both select the
same shell codebase and the base-pinned laptop patch. The first ships no root
payload. The second additionally ships only `hyprv-cpu-power`, its unit, distinct
D-Bus activation/policy and the CPU-only polkit action. The unit is
`hyprv-cpu-power-only.service`, distinct from the legacy turbostat publisher. It excludes the combined
helper, cooling failsafe, fan action and eGPU action. There is no CPU boot enablement.
Metadata schema2 records `deployment_profile=laptop` and the boolean CPU option;
all files, including the selection and patch, are hash-manifested.

`packaging/install-user.sh --profile laptop --dry-run` validates that the candidate
is a laptop release. Future explicit `--apply` uses the existing file transaction
and installs the laptop unit with `HYPRV_PROFILE=laptop` plus stable verified assets
and `release.json` under `~/.local/share/hyprv-shell`. Installation does not start or
enable the shell or write the cutover marker. With CPU power selected, the separate
root installer prints five destination files, journal/record locations and the
reload/restart effects; it refuses a mixed desktop multi-action policy. Root apply
requires the same root-owned, isolated-interpreter, manifest-checked staging as the
desktop installer. Do not install the combined helper for laptop telemetry.

The CPU-only endpoint is `org.hyprv.Shell.CpuPower`, path
`/org/hyprv/Shell/CpuPower`, interface `org.hyprv.Shell.CpuPower1`, ProtocolVersion1.
Methods: `SubscribeCpuPower()` -> JSON `rapl::State`, `UnsubscribeCpuPower()`;
signal: `CpuPowerChanged(JSON)`. The configured owner and active-local polkit grant
are required; no interaction prompts are requested. One lease per unique sender,
removed on disconnect. The existing native RAPL sampler runs at1s only while any
lease exists; last unsubscribe destroys the baseline/timer and the service exits
after an idle grace of5s. There are no fan/eGPU methods, controller or safety loop.
Default user-only installs publish unavailable W; available expanded bars may show
[CPU W][battery]. A compact strip with a battery must hold no CPU-power lease.

`cutover/laptop/switch.sh` and `rollback.sh` print plans by default. `--dry-run`
never invokes systemctl, probes the session or writes a journal. Future `--apply`
is user-only: full installed laptop preflight, exact before/output config hashes,
private patch generation, journalled atomic files, marker and user unit/IPC checks.
Failure restores files/marker and the prior matching legacy config. Rollback uses
its own saved workspace/backup/hash journal even if the current candidate changed;
it refuses to overwrite unrelated post-switch config edits. It does not stop any
system unit, cooling service, dcal family or unrelated Quickshell configuration.
The actual laptop branch files are never patched by a build or install operation.

Desktop cooling has a native `ExecCondition=... --eligible-cooling` and guards in
both daemon/failsafe entry points. Laptop profile skips before any controller,
watchdog or hardware recovery. Desktop retains late-module discovery and the
original nct6799 validation/failsafe behavior. Root services unset/refuse caller
profile, sysfs and bus overrides; non-root fixture tests use the shared API.
`HYPRV_SYSFS_ROOT` is a filesystem mirror root (contains `sys/class/...`), default
`/`; `sys_path("sys/class/powercap")` selects counters. System connectors honor
`DBUS_SYSTEM_BUS_ADDRESS` in isolated, unprivileged test processes.

## Laptop admission evidence

`cutover/preflight.sh --candidate` / `--installed` dispatches from the candidate's
manifest to the laptop gate. Desktop visual acceptance and CPU waivers do not
satisfy it. Ordinary full automated test receipts must still be fresh. Candidate
mode verifies the optional CPU install set; installed mode verifies the separate
CPU install record/files only if selected. Without CPU power its row is N/A.

QA must produce these evidence files; packaging never generates a PASS receipt:

- `cutover/evidence/laptop-e2e.json`: schema1, profile `laptop`, `epoch`, exact
  `binary_sha256`, `sources_sha256`, `reports` with `laptop` and `desktop` references.
  Each reference is `{path: root-relative JSON, sha256: digest}`. Reports must be
  actual complete functional manifests, all rows PASS, unique IDs, laptop cases
  from `parity/laptop/e2e/run-laptop-e2e.py`, desktop exactly204 cases, and `binary`
  pointing to the exact measured ELF. Report mtime must follow the candidate build.
- `cutover/evidence/laptop-parity.json`: same binding fields, `backend=production-gl`,
  `rows` containing `id`, `reference` and `actual` image references (path/digest).
  `parity/laptop/reference/manifest.json` is the canonical oracle: nonempty unique
  `slots` (`id`, `component`, `reference`); components bar/island/panels/fullscreen/
  corners all required. Every declared image must exist. Gate currently requires
  complete decoded RGBA exactness, including text: no ignored/masked pixels. Any
  future text-tolerance policy requires explicit parity-owner integration; the
  gate does not silently invent one. Actual/reference must be separate artifacts.
- `cutover/evidence/laptop-perf.json`: same binding fields plus `report` JSON
  reference. The actual report contains profile/binary, `nested=true`,
  `instrumented=false`, `seconds>=60`, `hook_events=0`, `cpu_percent<=0.1`,
  `rss_mib<=120`, integer zero `hidden_timer_wakeups` / `idle_gpu_submissions`.
  Missing attribution is not a zero. This does not reuse the desktop CPU waiver.

Binding is `sha256(json.dumps(build_release.sources(), sort_keys=True).encode())`;
receipts require age<=24h. Laptop fake/e2e/reference inputs are included in source
freshness. Missing, stale or PENDING receipts block admission. This implementation
has not run/accepted the joint laptop visual/resource suite or installed anything.
