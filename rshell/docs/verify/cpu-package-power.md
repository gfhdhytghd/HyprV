# P0 CPU package power repair — 2026-10-24

## Cause and implementation

Read-only UID 1000 audit confirms both package energy counters are root 0400:
`/sys/class/powercap/intel-rapl:0/energy_uj` and
`intel-rapl-mmio:0/energy_uj`. The old user sampler swallowed EACCES. Legacy
`scripts/cpu-power-sampler.sh:12-19` obtained turbostat's decimal `PkgWatt` as root
and exported `/run/hyprv/cpu-package-power-w`; `shell.qml:2377-2380` accepted a
finite nonnegative reading. Display remains the existing `.1` below 100 W / `.0`
at 100 W+ / `-- W` unavailable behavior (`shell.qml:286-289`).

`hyprv-privhelper::rapl` now owns the fixed-path counter read. Its independent
sampler exists only while a unique D-Bus client lease exists, at 1 Hz. Integer
energy subtraction handles one counter wrap against `max_energy_range_uj`, with
monotonic elapsed time, two-decimal source precision, baseline discard on final
release/reacquisition, read failure and pauses longer than 5 seconds. Package
names are de-duplicated across MSR/MMIO (both expose package-0 on this host), MSR
preferred; distinct packages sum, while core/uncore/DRAM children are excluded.
Missing sources/errors publish `{watts:null,error:...}` and log transitions.
This task did not edit cooling/controller/watchdog code or its independent safety timer. Concurrent svc-net work adds the separately authorized quiet chassis curve; the final shared release will include that change. The previous-live comparison is recorded in `cpu-package-power/cooling-unchanged.json`: cooling.rs differs due to svc-net, while its separate regression module, watchdog and unit match the live release.

The existing helper interface adds `SubscribeCpuPower`, `UnsubscribeCpuPower`
and `CpuPowerChanged`. Subscribe resolves the actual unique sender UID, restricts
it to configured desktop owner/root, and checks the new polkit action
`org.hyprv.shell.cpu-power` without interactive authorization. Policy allows an
active local session only. No paths/cadence/commands cross the bus. Unique-owner
loss, explicit release and bus teardown stop abandoned demand. A final liveness
query after lease insertion closes the authorization/disconnect race.

The sysstats actor requests this API only for active CpuPackagePower metric
leases while awake. Sleep/final power lease release unsubscribes even when a
CPU-only widget remains subscribed. Provider replacement reconciles demand.
Acquisition RPCs survive actor select cancellation; late acquisition finishes
before unsubscribe. Signals are broker-filtered by owner/path/interface/member,
so cooling signals do not wake this client. The fan client reciprocally filters only FanStateChanged, so wattage signals cannot cause extra GetFans RPCs. A power signal updates only watts,
without creating extra CPU/memory/network samples or changing their histories.
The user sampler no longer discovers or opens any RAPL energy file. No legacy
service, turbostat subprocess, permissive sysfs grant or runtime-file fallback
was restored.

Missing CPU/memory sample errors, absent temperature and absent default-route
counters now log on transitions instead of disappearing silently. Root power
and client subscription errors also log transitions and recovery.

## Audit

[Source audit](cpu-package-power/source-audit.json) records real paths, owner,
mode and read result as UID 1000. Of 87 inspected proc/sysfs inputs, 83 were readable;
the four denied files were RAPL energy counters (two package aliases and two
child domains). No other root-only source was found among the sysstats inputs:
proc CPU/memory/route/network, thermal temperatures, hwmon temperatures, and
network sysfs counters. This is evidence for this host, not a universal kernel
permission claim. Optional/missing sensors on other hosts remain unavailable
and logged.

## Validation and deployment boundary

- Targeted helper+power tests: 72 passed, 1 existing manual benchmark ignored.
  [Log](cpu-package-power/tests.log).
- Strict all-target helper+power Clippy:PASS.
  [Log](cpu-package-power/clippy.log).
- Packaging policy and D-Bus XML parse successfully; canonical root packaging
  symlinks include the new API allowlist/action in the staged release.
- Permission regression uses the real sysstats actor and a private D-Bus helper:
  an energy file returning actual EACCES still yields helper-supplied 12.35 W.
  It checks signal updates preserve local histories, sleep/resume subscriptions,
  final power-widget release while a CPU-only subscriber survives, and no
  re-acquisition without demand. Fake sysfs denies user access without sudo.
- Other regressions cover root timer inactivity without demand, first-sample
  baseline, wrap/invalid/stale counters, MSR/MMIO de-duplication, source rounding,
  read-error recovery, idempotent sender leases, late/cancelled subscription
  replies, unrelated fan-signal floods and explicit missing-data signals.

The targeted count includes svc-net’s eleven concurrent quiet-curve regressions in the merged tree. Final build/preflight results are recorded below. The renewed whole-workspace run completed 610 all-target tests plus two doctests and one xtask test, with 53 explicit ignores; four probe Python tests passed. The cutover Python suite retains three failures/errors out of 123 because its fixtures assume a pre-cutover live configuration. Its gate remains red, as does the source-stale overall test receipt after concurrent safety edits. Scoped tests above were refreshed against the final helper source.

No sudo, installation, live UI action, service restart/stop or compositor change
was performed. Read-only service snapshots identify the still-running shell
PID 2709217 and helper PID 2716595. Tests use private D-Bus and fake files; the
production root RAPL/polkit path and on-screen result require user installation
of the matching helper+shell+policy bundle. The old running helper does not
acquire new methods merely because its executable was staged/replaced.


## Final release and gates

Release ID: `eeafb5a678912bbaa802016d562b2b69fcea3ab0afc20f2f2a54ddba788a04c5`.
The coordinated official `packaging/build-release.sh` run succeeded after all
owners froze their inputs. This shared bundle includes the power fix, svc-net's
quiet cooling curve, and svc-sys's calendar discovery repair. Earlier drifting
builds correctly refused publication; their receipts were not reused.

Independent [verification](cpu-package-power/release-verification.json) confirms
all 4,855 current source hashes equal the build receipt, every one of the 60
manifest entries hashes correctly, all three staged/flat ELFs match, and the
helper/power inputs from the final 72-test/Clippy run match this candidate.
The new CPU polkit action and both D-Bus method allowlist entries are present.
[Build log](cpu-package-power/build-release.log),
[build receipt](cpu-package-power/build-receipt.json).

| Check | Result |
|---|---|
| Scoped helper/power tests | 72 PASS, 1 existing manual benchmark ignored |
| Strict helper/power all-target Clippy | PASS |
| Release source/ELF/manifest consistency | PASS |
| Release paths and runtime assets | PASS |
| Automated whole-tree admission | RED: three cutover preimage-fixture failures; receipt also spans concurrent edits |
| Component/full-screen parity admission | RED: dependency receipts require refresh for the new sources/ELF |
| Performance admission | RED: old source/artifact evidence cannot certify this ELF |
| Installed helper/candidate equality | RED: live eda5ca09 intentionally remains installed |

[Final read-only preflight](cpu-package-power/preflight-final.json) reports
`ready:false`; no tolerance, exclusion, acceptance binding or receipt was changed
to make it green. The successful Rust test runs and scoped tests are not a
substitute for fresh whole-tree readiness evidence.

[Final live readback](cpu-package-power/live-unchanged.json) proves the shell and
helper PIDs, active states and start timestamps equal the initial readbacks.
Neither service was restarted/stopped. Runtime wattage/polkit confirmation waits
for the user to install and restart the matching helper/shell bundle; no live
screen result is claimed from private-bus/fake-counter tests.
