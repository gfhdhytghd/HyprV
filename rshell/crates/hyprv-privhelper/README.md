# Persistent privileged cooling controller

The helper implements persistent fan safety and privileged shell APIs. The native shell and helper are live on this host; changes in this checkout are built/staged for user installation without restarting either live service.

## Protocol and authorization

System bus `org.hyprv.Shell.PrivHelper`, object `/org/hyprv/Shell/PrivHelper`, interface `org.hyprv.Shell.PrivHelper1`, read-only ProtocolVersion=1. `GetFans()` returns serde FanState JSON. `SetFanLimit(channel: string, percent: uint32)` accepts exactly chassis/pump and 0–100. `FanStateChanged(state: string)` reports changed telemetry and errors. The client checks the version before writes. These use the actual core skeleton's namespace; the different names in services-api.md were illustrative proposals.

Fan mutations resolve the actual unique sender's Unix UID over the system bus. Only root or the configured desktop owner `wilf` may proceed; polkit action `org.hyprv.shell.fan-control` authorizes that unique system-bus-name subject. Missing identity, denied authorization or bus errors fail closed. No caller-specified path, command, arbitrary register or raw PWM method exists. Read-only GetFans does not write hardware. Root access is required for the process; it must not be started for a read-only probe.

## Cooling and persistence (PRIV-1–6 fixes)

Desktop control uses the quiet chassis curve: 10-second temperature EMA, start at72°C, stop below65°C, minimum start duty77 (or the lower saved cap), linear to the cap at88°C, rise10/fall2 PWM per second. Each control-center limit is a hard normal-operation cap; zero stays off below emergency. **User decision 2026-10-24:** emergency enters only at raw Package temperature **strictly above105°C** (>105000 mC), exits only below100°C, and immediately overrides caps/slew to255. Exactly105°C does not enter; exactly100°C does not clear a latch. Invalid state, unreadable temperature or sensor failures still force255 immediately. Both headers are chassis fans (pwm1 and legacy-named pump/pwm7). Boot/no graphical session retains the raw70→88°C ramp with80/80 caps, without changing saved desktop limits; its raw emergency check uses the same strict entry threshold.

Maximum-speed recovery writes manual `pwmN_enable=1` before `pwmN=255`, each integer in one sysfs store. Exit/crash failsafe remains unconditional255, independent of the emergency threshold. Each channel is attempted independently; a failed first channel never skips the second. Partial normal transitions recover to maximum speed before propagating the error. Held chip-directory FDs pin identity across fixed-name attribute writes. Unchanged manual duties do not cycle modes or rewrite registers.

Authoritative `fan-percent`, `fan-control.lock` and `fan-percent.lock` now live in **/var/lib/hyprv-privhelper**, created root-owned 0700 by systemd StateDirectory. Every ancestor is checked for root ownership and absence of group/other write permission; files must be root-owned regular single-link files with safe modes, opened without following symlinks. Persistence uses held-directory openat/renameat and fsync. User-home state is never imported or authoritative. Optional state is parsed atomically with a 4096-byte limit; invalid/unreadable state latches maximum cooling and reports an error until an authorized SetFanLimit repairs it. Missing state defaults to 80/80, never to the current temperature-dependent duty.

Only authenticated, polkit-authorized SetFanLimit changes saved fan limits. Root startup/internal safety recovery does not expose a D-Bus bypass or accept caller paths. Do not migrate legacy user state automatically; an administrator must choose and authorize the new limits.

## Exit, crash recovery and packaging

Controller Drop requests maximum-speed handoff. The separate **hyprv-cooling-failsafe** binary has a fixed hwmon root, accepts no arguments and requires root before any hardware access. It is called by ExecStartPre and ExecStopPost, so recovery does not rely on readable state or acquiring old locks. The unit uses Type=notify, a 5-second watchdog, bounded startup/stop timeouts and Restart=always. Heartbeats come only from the cooling loop after a completed tick, so a hung reactor/control path cannot be hidden by an unrelated heartbeat task. Startup establishes safe hardware policy before loading desktop identity or persistence.

The consolidated 1-Hz safety timer runs while hardware exists; with systemd watchdog enabled it also supplies necessary liveness heartbeats when no controller exists. Without hardware or watchdog, only hwmon events are awaited. Cooling runs independently of UI/D-Bus clients; a safety task failure terminates the helper for supervisor recovery. The maximum-speed handoff is deliberate: no unvalidated firmware thermal curve is assumed safe.

The canonical installation artifacts are in this crate's packaging/. Repository-root helper packaging files are symlinks to them, preventing stale units/policies. Install **both** built binaries under the unit's /usr/libexec paths. The state directory is created by systemd, not inside a user home. Packaging still grants only fixed root-side sysfs access; a host-specific deployment may narrow /sys/devices after device-path validation. Dereference the source symlinks when packaging files into system directories.

No installation or root execution was performed. Deployment must separately disable the legacy user controller and revoke its old writable sysfs grants; the new protected lock namespace cannot exclude an old controller using user-home locks. This needs an explicit deployment handoff, not a live change during development. A kernel/hardware failure or uninterruptible task can exceed software supervisor guarantees; actual watchdog recovery and physical fan response remain device-validation work.

## Regression evidence and remaining verification

Run `cargo test --locked -p hyprv-privhelper -p hyprv-svc-power` and `cargo clippy --locked -p hyprv-privhelper -p hyprv-svc-power --all-targets -- -D warnings`.

Current validation: **37 tests passed across helper and power**, with Clippy warnings denied. The original PRIV probes are now production regression tests in src/cooling_regression.rs, adapted for the deliberate emergency-mode policy and protected root storage. Tests cover teardown, boot with malformed state, root/user ownership rejection, unsafe modes/hardlinks, singleton exclusion, independent channels, missing/over-range sensors, emergency override of zero caps, and bounded/atomic state parsing. Tests use fake sysfs/state as an ordinary user; a cfg(test)-only constructor injects that user's test directory as trusted. Static packaging tests check the canonical unit/policy and polkit-before-persistence order. Watchdog notification tests use a private Unix datagram socket. No test installs units, authenticates on a real bus or writes real hardware.

Real polkit allow/deny prompts, early boot, systemd sandbox execution, watchdog kill/recovery, suspend/hotplug and physical fan/temperature response remain **pending**. Tests/builds are not evidence of physical thermal safety.

## RemoveExternalGpu (added by svc-desktop)

`RemoveExternalGpu(bdf: string) -> a(ss)` is the privileged half of `quickshell/scripts/egpu-disconnect.sh`.

**Input validation**
- Only a PCI address token crosses the bus. It must be exactly `DDDD:BB:DD.F` (hex digits, function 0–7) and is lowercased.
- No caller-supplied paths are accepted. The helper re-derives everything from `/sys/bus/pci/devices`.

**Device checks**
- The device must exist and be a display controller (class `0x030000` or `0x030200`).
- It must be **proven external**: a Thunderbolt ancestor (`domain0`, a `thunderbolt` parent, or a `label` containing Thunderbolt), or the kernel's `removable` attribute set to `removable`.
- The script's bus>=3 heuristic is *not* accepted by root, so heuristic-only candidates are refused with `InvalidArgs`.

**Authorization**
- The caller UID must be root or the desktop owner.
- Polkit action `org.hyprv.shell.egpu-remove` (`auth_admin`) is checked after a validation pass.
- The topology is revalidated after the polkit prompt.

**Removal**
- The chain is removed in the script's order: slot functions first (descending), then the non-root parent bridges.
- Each removal writes `1` to `<device>/remove`.
- The reply has one `(bdf, "removed"|"error")` pair per device. Devices already gone are skipped.
- The client confirms removal when the kernel `remove` uevent arrives.

**Packaging and tests**
- The polkit policy and the D-Bus bus policy in `packaging/` include the new action and method.
- The unit already allows `/sys/devices` writes.
- Tests use a fake sysfs only. No real device was removed.

## CPU package power telemetry (2026-10-24)

Additive protocol-v1 methods `SubscribeCpuPower() -> string` and
`UnsubscribeCpuPower()` manage one idempotent lease per unique bus sender.
`CpuPowerChanged(state: string)` carries JSON `{ "watts": number|null,
"error": string|null }`. Initial/renewed demand returns unavailable until two
energy observations exist. Subscribe authenticates the sender UID as root or
configured desktop owner, then checks `org.hyprv.shell.cpu-power` with polkit
interaction **disabled**; the policy grants only an active local session. There
are no caller-supplied paths or cadence. Unsubscribe can release only its caller's
lease; unique-name disappearance and bus teardown release abandoned leases.
Read-only signal visibility follows the existing fan telemetry API.

The independent RAPL sampler reads only while at least one lease exists, at
1 Hz. It uses integer `energy_uj`, `max_energy_range_uj`, monotonic elapsed time
and single-wrap subtraction; pauses over five seconds discard the baseline.
Only package domains contribute, with duplicate MSR/MMIO package names counted
once (MSR preferred). Distinct packages are summed; child core/uncore/DRAM domains
are excluded. Watts retain turbostat's two-decimal source precision before the
existing UI's one/zero-decimal display. Missing/denied/invalid reads publish
unavailability with a descriptive error, log error transitions, and rebaseline
on recovery. Final lease release removes the timer and baseline, including rapid
release/reacquisition. No turbostat process, world-readable energy counter,
legacy sampler service or runtime file is needed. Cooling/controller/watchdog
policy and its independent safety cadence remain unchanged.

Tests use fake counters and private buses as the ordinary user. Production root
RAPL reads and installed polkit activation remain device/deployment validation;
no live helper was restarted by this change.
