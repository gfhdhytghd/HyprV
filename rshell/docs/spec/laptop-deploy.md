# Laptop deployment — Phase A

2026-10-24. Design plus an **unapplied draft patch**; no profile implementation,
installation, service operation, compositor reload or Git commit is performed.
Source: `origin/laptop` at `24972f711735ce92e72ef1db5bdf27f85ae8cfc9`, read with
`git show`; the shared checkout remains on `main`. See [the task](../tasks/laptop.md),
[laptop services](laptop-services.md), [panels](laptop-panels.md),
[bar](laptop-bar.md), [island](laptop-island.md), and
[minimal resources](laptop-minimal-resources.md). No XPS is available for validation.

## Recorded user decisions (2026-10-24)

- Internal XPS panel: **2880x1800, scale 2**, logical **1440x900**, DPR2.
- **No eGPU on laptop**: no eGPU service, policy/action, API or activation is installed.
- Island **read-only**, resolving D1: no Allow/Deny/Reply/question response controls,
  response-file writes or legacy 24-hour blocking permission hooks. Retain the current
  read-only hook contract; legacy interactive references are documented divergences.
- After confirming the separate expanded-bar watts module, the user permits an
  **optional read-only CPU-power helper**. Cooling and eGPU remain disabled. The
  default user-only install still works with CPU watts unavailable.

## Runtime profile contract

Use one ELF, one dependency graph and runtime policy objects, without laptop `cfg`
forks. Resolve an immutable `ProfileSelection { profile, reason, evidence }` before
constructing services, output owners or renderer resources. Pass it explicitly to
both service policy and UI assembly; do not let individual actors independently
redetect the host. `HYPRV_PROFILE=desktop|laptop` overrides detection; reject any
other present value, including empty, with a useful startup error. Test injection
provides fake environment, DMI and power-supply roots; production uses real roots.

Without an override, read DMI product name once and enumerate physical system
batteries once. Select laptop when either a recognized **portable** XPS product
identity matches the reviewed table or a present internal system battery is found.
The initial table may include exact normalized `XPS 13 9350` / `XPS 9350` identities;
confirm the real machine's spelling before treating that table as hardware proof.
Never match the substring `XPS`: an XPS 8930 tower must remain desktop. A battery
must have type `Battery`, be present, and identify system power supply rather than
`scope=Device`; corroborate absent scope with the internal ACPI/platform identity
and UPower `Type=Battery, PowerSupply=true`, rather than accepting a USB peripheral
or UPS. Missing/denied/ambiguous evidence is diagnosed and defaults to desktop;
`HYPRV_PROFILE=laptop` makes installation and isolated tests deterministic. Bound
initial bus discovery; do not hold shell startup waiting forever for UPower.
Persist the reason in startup diagnostics, not a polling task. Battery hotplug,
AC changes and legacy `bt`/`v2` variants do not change a running process's profile.

The desktop policy supplies exactly today's defaults, timers, namespaces and
geometry. The laptop policy selects the branch deltas through shared controllers:
collapsed bar, per-output expansion, centered IPC control panel, 1% brightness
steps, persistent calendar week/model and laptop-only battery/power surfaces.
Hidden calendar data can stay subscribed without retaining its rendered surface.
No Windows VM/Viewflow/Deskflow jobs or fan UI are created for laptop. The existing
`bar toggle <output>` desktop no-op is **not** laptop support: implementing and
testing per-output collapse is a prerequisite to using the draft patch.

## Privilege and cooling boundary

Laptop fans remain under Dell firmware. Laptop user policy must never construct
nct6799 monitoring/control, call cooling methods, request fan subscriptions, or
activate the desktop cooling/eGPU helper. Package-power sampling may use only the
explicitly installed CPU-only capability described below; it must not activate the
existing combined desktop daemon. Unsupported privileged data is unavailable, not
a reason to install/start cooling. UI/profile overrides cannot authorize hardware writes.

The helper itself needs an independent native, read-only hardware eligibility
check before any cooling/failsafe path: validated desktop board/controller identity
and expected nct6799 channels. No generic hwmon/PWM fallback. Current root units
run `ExecStartPre` **and** `ExecStopPost` failsafes and `Restart=always`; guarding
only helper `main` would still run a failsafe and could restart forever. Phase B
must gate activation with a native `ExecCondition` probe (ineligible is a clean
skip), and independently guard both failsafe invocations and D-Bus entry points.
A laptop override on an actual desktop must not stop its existing boot-time safety
controller; only the user shell's clients become inert. Root uses trusted hardware
facts, never a user-supplied environment override to arm cooling. Test all paths
against fake sysfs and assert zero writes and no retry timers on laptop fixtures.
Do not edit or enable the desktop privileged units as part of laptop deployment.

Branch audit found no fan-control invocation in the active laptop shell/config.
The old `cpu-power-sampler.service` file and VM/eGPU scripts still exist but do not
prove activation or a laptop need for root. Existing
`hardware/xps9350/90-xps-backlight.rules` grants the video group brightness access;
the active compositor worker uses `brightnessctl`, not pkexec. Retain existing
access and wluma ownership. A denied brightness write must show unavailable/error,
not prompt-install a helper. An optional future udev/group change requires its own
explicit reviewed root plan; it is not part of the default laptop install. The
unrelated `hypr/scripts/startup.sh` virtual-FIDO pkexec action is outside shell
cutover. Per the user decision, eGPU support and its root install files are excluded
entirely from the laptop profile; desktop eGPU behavior is unchanged.

## CPU watts visibility audit and optional privilege

Source truth remains `origin/laptop:quickshell/shell.qml` at the pinned commit:

| Surface/state | Source lines | What actually renders |
|---|---|---|
| Shared fallback text | 276–279 | `batteryOrCpuPowerText` chooses battery text when a battery is available |
| Compact strip | 3630–3634 | Uses that fallback, so no CPU W with a present battery |
| Expanded right status pill | 4193–4207 | Independent `cpuPowerTrigger`, label `cpuPackagePowerText`, visible iff `cpuPackagePowerAvailable`; **no battery condition** |
| Adjacent battery slot | 4218–4226 | Uses `batteryOrCpuPowerText`; with both sources available the expanded bar shows **[CPU W][battery]** |
| Provider parsing | 2471–2475 | Power available iff parsed `__CPU_POWER__` is finite and nonnegative |

Thus “battery present means CPU W never renders” is false. With no power source,
the separate W trigger collapses; with no battery, the fallback slot may also show
W (or `-- W` when unavailable). Preserve those branch rules rather than inventing
battery-based suppression. `scripts/system-snapshot.sh:65–66` reads the public
`/run/hyprv/cpu-package-power-w`; the legacy sampler service starts a turbostat
publisher as a system service, but its presence in Git does not prove it is installed
or enabled on the XPS. No device/root-readability claim is made here. A hashed
[source receipt](../verify/raw/laptop-deploy/cpu-w-source.json) records this audit.

The user permits optional read-only privilege for this real visible consumer.
Phase B should reuse the existing native RAPL sampler arithmetic as a library,
through a **separate CPU-only entry point/interface and install set**. The current
combined `hyprv-privhelper` initializes a cooling controller and is not a safe
laptop substitute merely because clients promise not to call fan methods. A new
mode/entry point must branch before any cooling discovery, watchdog or failsafe
construction. This is a design requirement, not an existing CLI flag claim.

Default user install performs no root operation. A separately requested manual
root plan may install just a hash-listed CPU-power executable/entry point, a
CPU-only system-bus activation file/unit, restrictive D-Bus policy and the
`org.hyprv.shell.cpu-power` authorization action in its own policy file (do not
copy the existing multi-action fan/eGPU policy). Use a distinct CPU-only bus
endpoint so desktop helper activation cannot occur by name/interface reuse. No fan/eGPU policy, writable
hwmon permission, cooling state directory, boot target dependency,
`ExecStartPre`/`ExecStopPost` failsafe, cooling watchdog or fan API is in that set.
Do not reuse the combined root unit or ship an activatable desktop D-Bus service
that could start it indirectly. Do not overwrite a desktop installation in place:
profile/install-set conflicts must be diagnosed before mutation.

The CPU-only service reads validated package energy/range counters, never writes
sysfs/MSRs, invokes turbostat, changes permissions or accepts caller-controlled
paths. Authorize the configured owner and active local session, export telemetry
methods only, and manage leases per unique bus sender with disconnect cleanup.
Subscribe only while a watts consumer is visible: expanded bar, or compact strip
without a battery and with the CPU-power capability selected. A present battery
and collapsed bar has no watts lease. On last unsubscribe/client exit, stop the
sampler, drop baselines and arrange event-driven service idle exit; no keepalive
poll, cooling loop or restart spin. Multi-output consumers share one sampler.
Missing/denied counters, missing optional install, suspend or provider restart
publish availability honestly and must not activate a different helper.

Gate the optional install separately: fresh energy/wrap/delta fixtures and sender
cleanup tests; zero sysfs writes and zero fan/eGPU/cooling entry points under all
activation/failure paths; private-bus integration including polkit denial; repeated
manual install/rollback plans; and no idle sampling after the bar collapses. Both
CPU-available and CPU-unavailable UI fixtures are required. The optional install
is never a prerequisite for a valid user-only/unavailable-watts laptop deployment.

## Test-only hardware and bus isolation contract

Every sysfs reader across crates must honor **`HYPRV_SYSFS_ROOT`**, read once at
startup via engine's shared API: `sys_root()` defaults to `/`, and
`sys_path("sys/class/...")` joins below that filesystem root. A fixture therefore
contains `ROOT/sys/class/...` (not `ROOT/class/...`). Resolve `class/dmi/id/product_name`, `class/power_supply`,
`class/backlight`, `class/leds`, `class/net`, `class/thermal`, `class/hwmon`, `class/drm`,
`class/powercap` and discovered `devices/...` paths from that one injected root.
This covers detection, feature probes, availability checks, enumeration, reads,
and watchers/reconnects, not just the main sampling function. No hard-coded `/sys`
read may bypass an override. Rebase absolute fixture symlinks or reject escapes;
missing fixture files, denied reads and broken links never fall back to host sysfs.
Use explicit mock backends for udev/netlink events that a fake directory cannot emit.
Any test writer must use a fixture backend confined to its root; it must not gain
hardware write authority from this read override.

Every system-bus client must honor **`DBUS_SYSTEM_BUS_ADDRESS`**, including profile
UPower discovery, UPower/battery, both PowerProfiles names, login1/lid/inhibition,
NM, BlueZ, polkit/optional power telemetry and reconnection/activation paths. Do
not hard-code `/run/dbus/system_bus_socket` or silently retry the host bus after a
private-bus error. Session-bus clients keep their distinct session-bus selection.
Default zbus system connection behavior is not sufficient proof: test each client
construction path and manual address builder. With both variables unset desktop
behavior remains unchanged. Installed production units do not set fixture roots or
forward user-provided addresses/paths into privileged actions; helper fixture
launches are unprivileged isolated tests. Real root-service eligibility uses trusted
hardware and production paths independently of caller-controlled test inputs.

Phase B acceptance uses the existing `parity/laptop/fakes/` plan: contradictory
host/fixture DMI and sensor values; missing fixture entries; symlink escape; private
UPower/PowerProfiles/login1/NM/BlueZ names and owner restart; fake CPU-energy values;
and an audit asserting **zero host sysfs/bus access** throughout startup, errors
and reconnect. Reference QML consumes the same fixture data through its harness.
These are required owner integration tasks, not a claim all readers are wired yet.

## Laptop config patch and coverage

[cutover/laptop/hypr.patch](../../cutover/laptop/hypr.patch) is generated against the
pinned **laptop** files, not by copying desktop hunks. Its
[input manifest](../../cutover/laptop/hypr-inputs.json) pins all eight original file
hashes. Compare `git diff origin/main origin/laptop -- hypr` as branch context;
the patch itself transforms laptop legacy invocations only. Laptop monitor/HDR
curves, gestures, unrelated exec-once entries and startup-app selection survive.
Do not apply the desktop `cutover/hypr.patch` to the laptop branch.

| Laptop source | Legacy entry | Draft native mapping |
|---|---|---|
| `controls.lua:47` | brightness popup through `quickshell -p ... ipc call quickAdjust` | quoted `~/.local/bin/hyprv-shell ipc call quickAdjust <method> [level]`; retains exact method/level and HDR curve |
| `hyprland-bind.lua:54` | Super+A control panel | `hyprv-shell ipc call controlPanel toggle` |
| `hyprland-bind.lua:34,50` | shell kill / reload binds | stop / restart `hyprv-shell.service`; preserve fcitx5 suffix and unrelated Super+Y actions |
| `hyprland-exec-once.lua:14` | `quickshell/scripts/launch.sh` | import the graphical environment, then start the static user shell unit |
| `hyprland-dropitem.lua:31` | Super+C calendar IPC | `hyprv-shell ipc call calendar toggle` |
| `hyprland-plugins.lua:199` | top-edge bar IPC with output argument | `hyprv-shell ipc call bar toggle <quoted output>`; binary and output are quoted separately |
| `scripts/restore-session-layout.sh:164` | `qs --path .../StartupConfirm.qml` | `hyprv-shell startup-confirm`; preserves `HYPRV_STARTUP_APPROVAL`, decision readback and cleanup |
| `scripts/toggle-battery-mode.sh:7,21` | Quickshell variant script | `hyprv-shell desktop setVariant <bt|v2>`; profile remains laptop |
| `scripts/rotate-screen-ccw.sh:8,169` | legacy Deskflow reflow script | shared native reflow command; laptop capability must return successful no-op when that topology is absent, preserving monitor rollback on real errors |

`hyprland-daily.lua` and `hyprland-battery.lua` require the relevant split modules.
Backup `.bak-*` / `.pre-*` files are not loaded and are deliberately unpatched.
The battery entry point currently does not explicitly require `hypr.controls` like
the daily entry point does: test brightness from a fresh battery-mode compositor;
if a load-chain fix is necessary it belongs in Phase B, not an invented cutover hunk.

Keep `hyprland-winrule.lua` byte-identical: native layers retain
`hyprv-quickshell` (no compositor animation, blur, alpha threshold 0.15) and the
existing `shell:.*` rule (blur/popups, alpha threshold 0.05) for roles using it.
Preserve existing `corner.*` rules. Verify role namespaces, exclusive zones and
input regions in an isolated laptop compositor; renaming namespaces would bypass
these existing rules. No new Hyprland API/plugin calls are introduced by this patch.
The StartupConfirm and standalone desktop-action commands explicitly set
`HYPRV_PROFILE=laptop`; those processes do not inherit the systemd service drop-in.
Stateless IPC commands address the already-profiled service.

Static validation: extracted only these eight original files to a temporary tree;
`git apply --check` and apply succeeded there; five Lua files passed the skill's
syntax checker and three scripts passed `bash -n`. Temporary tree removed. A
read-only `hyprctl version/status` lacked a session signature, so live API validation
was not claimed or attempted. Real laptop output is 2880x1800 at scale2 (1440x900
logical); reference/e2e must also cover an external output and per-output bar toggle.

## User installation and transactional cutover design

Extend the installer with an explicit `--profile laptop` selection and record it
in user-owned installed metadata. Keep desktop's existing install/root workflow
unchanged. Do **not** run today's desktop two-phase controller on the laptop: it
requires a root helper handoff and uses the desktop patch and legacy-unit list.
The Phase B laptop release selects its own input manifest, patch and controller
policy, pins their hashes in the release manifest, and does not replace the desktop
payload by mutating a release after its ID was calculated.

User install stages the common shell ELF at `~/.local/bin/hyprv-shell`, hash-verified
runtime catalog/assets/icons under `~/.local/share/hyprv-shell/`, static user unit at
`~/.config/systemd/user/hyprv-shell.service`, and a managed drop-in containing:

```ini
[Service]
Environment=HYPRV_PROFILE=laptop
Environment=HYPRV_ASSET_ROOT=%h/.local/share/hyprv-shell
```

The unit keeps graphical-session ordering, `PartOf`, restart-on-failure/backoff,
startup rate limit and `native.enabled` guard. Laptop's legacy session does not
activate `graphical-session.target`; exec-once imports Wayland/runtime/Hyprland
instance variables and starts the unit explicitly after compositor startup. Do
not add a WantedBy/After target cycle or enable the unit as an installation side
effect. Journal and restore any previous managed drop-in when switching profile.
Missing assets are fatal; no checkout-path fallback. Metadata records a real stable
checkout/install location, never a build snapshot or `/tmp`; validate all recorded
paths and hashes at staging, install and preflight. Scratch HOME is for test roots,
not a persisted deployment workspace. No root install set, D-Bus service/policy,
polkit action or cooling failsafe is installed/enabled on laptop by default. The
user-approved optional CPU-only root plan is separate and excludes cooling/eGPU.

Future `cutover/laptop/switch.sh` / rollback should be idempotent, printed-plan by
default, and user-only: verify exact base hashes and profile-specific gates; lock
and journal the user's state; stop only the matching legacy Quickshell config PID
(exclude the separate dcal family); apply the verified patch atomically with file
backup; arm marker and start the user unit; verify first mapped bar/IPC readiness.
On failure stop the new unit, restore patch/drop-in/marker/config variant state and
restart only the legacy family previously running. Repeat/rollback must tolerate
partial transactions and missing/exited processes. Do not kill by process name.

Default laptop legacy **service stop list is empty**: no evidence from this branch
justifies touching desktop `fan-control.service`, `hyprv-fan-control.service`,
`hyprv-fan-permissions.service` or `hyprv-cpu-power.service`. If installed laptop
inventory finds one active, abort with a precise prerequisite; do not take over
unknown cooling as a side effect. Keep wluma, hypridle, battery-notify (avoid adding
a duplicate native notification owner until its handoff is specified), NetworkManager,
BlueZ, PipeWire, SwayNC, portals and unrelated apps/services intact. No root phase.
A/B can place native shell on a chosen external output only after both families
support disjoint output filters and input/exclusive zones are verified; it is not
safe to promise isolation by merely starting both today. Use nested compositors
until that capability is implemented.

## Gates and minimal-state policy

Laptop preflight must require the explicit laptop profile, matching ELF/install
manifest, stable assets, complete fresh automated test receipt, laptop functional
fixtures, and laptop parity/performance receipts. Root helper installation is
`N/A (laptop, firmware cooling)` only with the inertness tests and non-activation
proof. If CPU-only telemetry was explicitly selected, validate its separate minimal
install set, authorization and demand-scoped tests; never require/install the desktop
combined helper. Without that option, its CPU-power install row is N/A; do not
reinterpret a failed desktop helper gate as laptop readiness. The desktop user's visual acceptance and 0.5%/2.0% CPU waiver do not certify a new laptop
profile. Keep fresh laptop reference generation available; exact non-text pixels
and missing-image rejection stay strict.

Resolve the visible-data constraint explicitly: the collapsed legacy strip shows
CPU/memory labels with a 5s minimal cadence. Those two **visible** values may hold
a `{CpuTotal,Memory}` lease; detailed/per-core/temperature/RAPL/network-rate leases
are zero while hidden. If the labels are not visible, no stats timer exists. Clock
uses a minute-boundary tick, UPower uses signals; no hidden history sampling or
polling for unsupported features. This is not a claim of a minute-only timer while
CPU percentages remain visible. Engine owns measured cache/surface trimming;
release GPU/hidden pages after close animation, retain semantic calendar state.
Report <=120MiB RSS and <=0.1% controlled idle as targets, with timer/event attribution,
not as achieved measurements. Zero repaint/GPU submissions when settled.

| Phase B check | Required proof before laptop deployment |
|---|---|
| Selector | override/invalid input, exact portable DMI, XPS tower, internal battery, UPS/mouse, missing/denied roots; latched across AC/hotplug |
| Inert cooling/eGPU | fake laptop sysfs plus private bus; zero desktop-helper activation, fan/eGPU methods, sysfs writes, failsafe execution or cooling retry tasks, including D-Bus autoactivation and crash paths |
| Optional CPU power | separate read-only install set; expanded bar renders W alongside battery, compact battery strip holds no watts lease; sender cleanup/idle exit and denied/missing source covered |
| Test isolation | all readers honor HYPRV_SYSFS_ROOT and all system-bus clients honor DBUS_SYSTEM_BUS_ADDRESS at startup/reconnect; no host fallback or fixture-root escape |
| Installation | scratch HOME install/rollback, catalog corruption/missing assets fatal, path rejection, profile drop-in restored, no root commands; repeated plans do not mutate state |
| Config | base/hash drift rejection, patch forward/reverse, IPC capture for every row above, StartupConfirm launch/cancel/error cleanup, brightness from both daily/battery entry points |
| UI/backend | UPower/backlight/HDR/lid/power profiles/thermals/gesture fakes; laptop QML references with identical inputs at DPR2, compact/expanded/hidden lifecycle; desktop full regression unchanged |
| Resources | exact release ELF in private compositor: on-battery collapsed idle, hook counts separate, no hidden leases, CPU/RSS/GPU after open-close, per-thread wakeups and live desktop regression comparison |
| Device | real XPS DMI, permissions, lid, ALS/wluma ownership, HDR, suspend/resume and external output remain PENDING until hardware available |

## Branch and commit proposal — no commits performed

`main` tracks `origin/main` and `rshell/` is untracked there; shared desktop files
are already dirty from other agents. Do not checkout laptop in this tree, stash or
revert those changes, or bulk `git add rshell/`. Inventory untracked files first and
prepare a reviewed allowlist excluding `target/`, `packaging/dist/`, raw captures,
private snapshots, sockets, secrets and machine install journals. Keep runtime assets,
Cargo.lock and source fixtures required to reproduce tests; reference manifests must
not point to excluded/missing images. Review licensing/size before adding assets.

Proposed series in an isolated worktree/branch based on the pinned `origin/laptop`
(after the user explicitly authorizes commits): (1) reviewed common rshell baseline
and reproducible packaging/tests; (2) runtime selector and service capability policy
with desktop defaults; (3) laptop UI/service deltas and isolated backend fixtures;
(4) laptop user installer/controller plus this base-pinned patch; (5) regenerated
laptop/desktop evidence and deployment documentation. If common code lands on main
first, cherry-pick that reviewed baseline rather than copying a second divergent
rshell tree; port laptop behavior through runtime policy only. Record the actual
baseline tree SHA and dependency fingerprint, not assumed equivalence between
branches. Apply the config patch only in the laptop commit/worktree after gates;
resolve branch-specific context explicitly, never overwrite with main's hypr files.
No commit, push, branch switch or live application is authorized by this proposal.

Resolved integration decisions: D1 is read-only (user, 2026-10-24); the CPU watts
visibility audit above justifies the user-approved optional CPU-only helper. No
consumer may acquire the combined desktop helper as a fallback. Source-selection,
profile/backend wiring and optional install implementation remain Phase B work.

## Phase B packaging/helper implementation checkpoint

The svc-sys implementation is documented in
[cutover/laptop/README.md](../../cutover/laptop/README.md) and
[laptop-impl-svc-sys.md](../verify/laptop-impl-svc-sys.md). CPU-only entry point and
install set, profile-selected manifest/patch, guarded desktop cooling entry points,
user-only journalled laptop cutover and profile-specific admission are implemented.
Shared profile/root APIs are engine-owned; no second selector was added. This is
not laptop hardware or joint UI/visual/resource certification. The patch remains
unapplied; no build artifact has been installed by svc-sys.
