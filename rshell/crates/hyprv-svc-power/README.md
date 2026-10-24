# Native power services

This crate owns `power`, `battery_history`, `backlight`, `sysstats`, `fan`, and `inhibit`. Each module exports serde `State`, `Action`, and `spawn(Context)`. Each handle exposes a core `Topic<State>`, a core error topic, bounded typed actions, and core subscription guards. `sysstats` additionally wraps those guards in metric/cadence leases. No executable or subprocess transport is used by the crate or helper.

## Integration

Keep one `Arc<hyprv_core::runtime::ServiceRuntime>` in the application and pass a clone to `Context::system(...).await`. Context holds a **weak** runtime reference so actor ownership does not keep its own reactor alive. Clone a single Context for all six module spawns. Its shared store, bus and sleep channel must not be duplicated per output. `power::spawn` owns the logind PrepareForSleep subscription for the other samplers. When integrating with another logind owner, forward sleep transitions into `Context.sleeping` instead of creating conflicting timers.

The handle's `subscribe()` guard is core's subscriber-counted activation mechanism. Battery details sample only with a guard; `sysstats::ServiceHandle::lease(metrics, period)` embeds the same core subscription, registers a bounded metric request, negotiates the union of metrics and the fastest requested cadence, and removes the request on drop. Multiple consumers share one timer. A direct `state.subscribe()` is supported as a default all-metrics 1-second consumer. Periods are bounded to 100 ms–3600 seconds; at most 128 explicit stats leases exist. Samplers implement core `Service` and run under core `run_sampled`. There is no second store, subscriber counter or sampling lifecycle. Core has no concrete `SamplingLease` type; its `Subscription` is the authoritative guard.

Actors subscribe before initial snapshots, suppress equal publications through core, watch provider owner changes, and invalidate unavailable state. Commands remain alive when a popup drops its state subscription. Action futures report provider acceptance/errors; observed state arrives independently. Commands have a 15-second deadline and state that a timed-out hardware operation may have completed. Errors are available both on the reply and error topic. Closed command channels do not cause a busy loop. Runtime shutdown cancels services; the owning application must call `request_shutdown` before dropping the runtime. Reconnect backoff exists only while a source is disconnected. Sampling stops on sleep and resumes with fresh rate baselines.

## Module behavior and source mapping

### power

UPower root/device PropertiesChanged, DeviceAdded/Removed, and logind PrepareForSleep feed `State`. Fields include OnBattery, presence, percent, battery mode, energy/rate/time estimates, available profiles, current profile and performance degradation. Profile strings preserve unknown values. Both `net.hadess.PowerProfiles` and `org.freedesktop.UPower.PowerProfiles` are capability-tested; subscriptions precede reads for each. Actions are refresh, explicit profile, profile cycle and suspend. Profile cycles performance → balanced → power-saver → performance and refuses modes the provider does not offer.

UPower uses native percentages (0–100), not Quickshell's normalized fraction. Present display device type 2 identifies a laptop battery. Modes match `BarServices.qml:79-85`. Profile set maps `ControlPanelPopup.qml:312`; the standalone cycle is `scripts/power-profile.sh:5-22`. Extra battery values are nullable rather than substituted measured zeroes.

### battery_history

The detailed state includes status/mode/capacity, signed current and average watts, sample count/window span/completeness, energy, estimate seconds and estimate basis. `legacy_json()` exposes the exact legacy camelCase schema consumed at `shell.qml:1865-1877`.

Sysfs supplies energy_now/full, or charge × voltage conversion with micro-unit rounding; power_now or absolute current × voltage supplies power. Charging is positive, discharging negative. History imports/persists `epoch_seconds<TAB>signed_power_uw` at `$XDG_RUNTIME_DIR/hyprv/battery-power-history.tsv` (fallback `/tmp/hyprv`). It prunes samples older than 1800 seconds, averages only samples of the current direction, rounds the micro-watt average, and computes energy/power estimates. Full and empty charge-target states estimate zero; missing power stays unavailable and contributes no invented zero sample. History is bounded to 100000 entries. Udev power_supply events update discovery; the one-second fallback is owned by core subscription demand. There is no always-running history timer.

These rules replace `scripts/battery-info.sh:8-276` and `scripts/lib/battery.sh`; detail visibility is `shell.qml:3341-3345`.

### backlight

`State.outputs` contains per-output backend, UI level, observed raw/max, reference luminance, request/confirmed sequence, drag state, retries and failure. Actions: refresh, begin, sequenced set, sequenced finish. `spawn` reads the current literal `hypr/controls-profile.lua`; `spawn_with_config` supports multiple typed OutputConfigs and monitor-field overrides. The simple Lua importer refuses expressions/nested tables; it does not execute arbitrary configuration Lua. Parent inotify watches profile/state changes and supports atomic replacement. Backlight/drm kernel events trigger hardware acquisition.

The exact three-region curve follows `hypr/controls.lua:27-48` and `hypr/brightness-hdr.lua:1-14`: 0–10 holds minimum hardware and ramps SDR white from 10 nits to desktop; 10–70 traverses the original piecewise/logarithmic hardware curve; 70–100 holds maximum hardware and ramps desktop-to-peak reference luminance. Sysfs minimum raw is 1; DDC minimum is 0. Tests traverse all 101 levels. UI set/finish behavior maps `shell.qml:2768-2815`.

Native sysfs and I²C DDC/CI VCP 0x10 transports are serialized. DDC replies validate address/opcode/result/checksum/range. Reads occur on acquisition/explicit refresh/change and bounded finish verification, never through an idle DDC timer. Finish verifies after 90 ms and retries at most three times; an unconfirmed/failed target stays distinct from observed raw. Old request sequences are rejected. Timed-out I²C work retains its serialization mutex even if the caller stops waiting, including across profile replacement. Kernel I²C calls cannot be forcibly cancelled safely: the result explicitly warns that hardware may still complete.

The only compositor mutation is a native Hyprland request-socket Lua monitor update. It retains current position/scale/transform, requested mode, bitdepth=10, cm=hdr, SDR minimum and reference_luminance, plus validated profile fields. User strings are byte-escaped Lua literals. It never invokes `hypr.controls` workers or `hyprctl`. Confirmed levels are atomically saved in `brightness-hdr.json`: the primary retains legacy `level`, while `levels` adds per-output storage. The active config chain was inspected read-only; live socket verification was unavailable from this process environment. UI cannot claim compositor readback from static tests.

### sysstats

CPU aggregate/per-core deltas, memory totals/availability/percent, temperature, RAPL package watts, default interface, counters/rates, elapsed time, generation, and bounded 120-point CPU/memory/network histories map `shell.qml:2314-2420` and `shell.qml:240`. CPU counter regressions and interface/counter changes yield no invented rate. Resume and lease reactivation discard baselines. Memory uses MemAvailable, with MemFree+Buffers+Cached fallback. Package watts come from the root helper’s demand-scoped RAPL API; the user process never opens energy_uj. The helper handles wrap/elapsed time and source precision. Missing/denied sensors stay unavailable and error transitions are logged.

Rtnetlink multicast tracks link/default-route changes; requested snapshots read proc counters. CPU temperature discovery prefers named thermal zones, then package/Tctl/Tdie hwmon labels. Udev hwmon/thermal/powercap changes refresh caches; there is no rediscovery glob every timer tick. Sensor/package-power metrics are omitted unless requested by active leases. Shared domain metric negotiation is local service behavior; core still owns activation and the timer lifecycle. A CpuPackagePower lease acquires SubscribeCpuPower; sleep, final release and teardown release it. CpuPowerChanged updates watts without polling helper RPCs. Owner replacement reacquires only active demand. Subscription acquisition is cancellation-safe; a late reply is balanced before release. The bar/panels already negotiate metrics through visibility-controlled leases, so no UI polling or formatter changes are required.

### fan

The UI client uses the exported helper protocol constants, requires protocol version 1, reads typed JSON state via GetFans, watches FanStateChanged and provider ownership, and sends validated SetFanLimit. `available` maps fanControlAvailable; channel `Chassis` maps fanChassisPercent/Rpm and `Pump` maps fanPumpPercent/Rpm (`shell.qml:1644-1666`). Percentages are saved **limits**, never inferred from instantaneous PWM. No fan sampling timer lives in this client. See the helper README for independent cooling ownership, authorization, boot policy and installation boundaries.

### inhibit

Actions enable/disable/toggle a specific Manual or Demo owner. A logind `Inhibit("idle:sleep", "HyprV", "HyprV prevent sleep", "block")` FD is acquired once and retained until the last owner leaves. Removing Demo preserves Manual. No PID file or sleeper process exists; the compatibility `pid` field is nullable. Failed authorization never publishes enabled. Daemon owner replacement invalidates and reacquires only previously requested leases. This replaces `scripts/prevent-sleep.sh:1-118`; the panel toggles sleep inhibition at `ControlPanelPopup.qml:236-241`.

## Validation

```sh
cargo test -p hyprv-svc-power -p hyprv-privhelper
cargo clippy -p hyprv-svc-power -p hyprv-privhelper --all-targets -- -D warnings
cargo run -p hyprv-svc-power --example read_only_probe
```

Initial implementation validation result: 19 tests passed across this crate and the helper; Clippy with warnings denied passed. D-Bus and polkit packaging XML parsed successfully.

Tests use mocked brightness/inhibit transports, virtual time, text fixtures and isolated fake sysfs/state directories. They cover curve monotonicity/endpoints, drag/finish sequencing, stale requests, malformed DDC, failed readback/retry bounds, signed battery estimates/pruning, missing data, CPU/net/RAPL conversion, zero-demand/sleep timers, independent inhibitor owners/denial, fan boot versus saved desktop limits, missing telemetry, singleton exclusion, hwmon identity/hotplug, symlink rejection and write-denial propagation. They do not change real hardware or authenticate against polkit.

The read-only probe checks name ownership before probing existing providers and never activates the helper. Current evidence: UPower OnBattery=false; no display battery; both profile interfaces expose performance; the Rust power actor publishes matching state. Helper absent, deliberately not activated. No service, fan, brightness, sleep or profile mutation was performed. The helper packaging was not installed; Quickshell/Hyprland were not restarted.

## Gaps and manual verification

- Wire these handles into the shell/renderer and verify every UI presentation/action. This crate does not edit the shell or UI agent's paths.
- Under a separately authorized device test, verify DDC permissions and physical brightness, compositor HDR readback/fractional outputs, external hardware adjustments, sleep/resume and counter rebaseline. Native write failures remain visible. There is no implemented privileged SetBacklight fallback for hosts without seat/group access.
- Deploy the canonical helper packaging with an explicit old-controller/permissions handoff; top-level helper artifacts now reference that same set. Verify polkit allow/deny, cooling during early boot, logind/system-bus restart, suspend, hotplug and crash recovery. No such live test has been performed.
- Ordinary provider **name-owner** restarts are observed. A whole system-bus transport replacement requires the application to rebuild Context and actors; the core API does not yet provide a reconnecting shared bus owner.
- Literal Lua profile import supports the current source file, not arbitrary computed Lua tables. Typed profiles support explicit monitor overrides and multiple outputs. IPv4 default-route selection retains the source proc-route semantics; IPv6-only default-route selection is not yet implemented.

## Adversarial review fixes POWER-1–3 (2026-10-23)

Provider/owner signals are continuously drained into bounded 128-message inboxes; consumer RPCs never backpressure the shared zbus socket reader. Overflow explicitly invalidates state and requests reconnect/resnapshot, and PrepareForSleep's final state is reconciled from logind after recovery. Reader tasks are aborted with their inbox; no polling timer is added. Event failures and setup failures share exponential 250ms–30s backoff; successful setup alone does not reset it, and actions cannot shorten recovery deadlines. CPU totals include user through steal once, excluding guest/guest_nice columns already included in user/nice.

The three original failing probes are now passing crate tests: private zbus signal flooding with a successful concurrent RPC, a bounded reconnect count on actual ServiceRuntime, and a guest-counter CPU fixture. Run `cargo test --locked -p hyprv-svc-power`; no real bus or device mutation is needed.

Repair validation: **37 tests passed across helper and power; Clippy all-targets with warnings denied passed**. Commands ran as UID1000, with private transports/fake sysfs only.

### Battery history device selection correction

`battery_history` follows `scripts/lib/battery.sh:22-29`: select the first
lexically sorted `BAT*` directory under `class/power_supply`, following directory
symlinks. Do not filter by `type`, `scope`, or `present`, and do not fall back to
peripheral batteries. This is independent of UPower's indicator selection.
Regression coverage includes peripheral-only hosts, multiple BAT names, non-directory
matches, symlinks, missing roots, and absent batteries. The original equivalence
report's peripheral-selection mismatch is historical evidence, not a rerun.


## Laptop public API (Phase B, 2026-10-24)

The host/core selects its profile once per `docs/spec/laptop-deploy.md`; this crate does
not independently inspect DMI/environment or change profiles on AC events. Pass
`hyprv_svc_power::Profile::{Desktop,Laptop}` to
`power::spawn_for_profile`, `battery_history::spawn_for_profile`,
`backlight::spawn_for_profile`, `sysstats::spawn_for_profile`, and
`fan::spawn_for_profile`. `spawn` constructors now dispatch through the shared latched `hyprv_core::profile::profile()`; `spawn_for_profile` remains available for explicit host/test construction.
`hyprv_svc_power::Profile` is a re-export of the core type. No installation or helper change is required for this API.

- Laptop power snapshots are local cached projections. Initial/owner-change
  snapshots and invalidated-property fetches are the only observation RPCs;
  UPower percentage/state/lid and profile changes come from signals. Optional lid
  fields are omitted on desktop serialization. Modern PPD wins when both owners
  exist; missing providers wait for owner events. `ProfileToggle` preserves the
  CLI order; `ProfileCyclePanel` uses power-saver→balanced→performance→power-saver.
- Battery detail `subscribe()` is an **open-popup/card guard**. Drop on close
  request, before the exit animation; never keep a passive detail subscription
  while hidden. One immediate sample then1Hz, shared by subscribers, no event-
  triggered extra rows. Hidden explicit Refresh does not sample. The laptop
  projection preserves numeric defaults, first BAT selection, real history means,
  and three-decimal JSON power/energy. Use `legacy_json_for_profile` for the
  no-BAT short object (missing estimate differs from a null estimate).
- Laptop brightness uses a literal `controls-profile.lua` with its monitor table,
  auto_brightness and separate `CurveKind::Laptop`. `parse_profile` supports an
  injected sysfs root and pinned literal input; `plan_for_config` is the curve
  oracle API. Startup adopts saved HDR anchors and observes ALS-owned raw without
  hardware writes. `Action::Restore { output }` performs explicit restore: auto
  mode resets/persists70 without a hardware write. Set-only QuickAdjust actions
  confirm/persist without needing Finish. Laptop refresh accepts hardware readback
  even while dragging or awaiting a sequence confirmation; desktop retains its
  existing guards. Native IPC updates plugin reference,
  monitor SDR max and refresh while preserving current geometry. Same raw/SDR
  writes are elided. Queued sequences supersede obsolete hardware completions
  before SDR/persistence. Denied access is an error, never helper activation.
- Laptop stats only sample through **explicit metric leases**. A raw
  `state.subscribe()` is passive; `laptop_strip(painted)` returns an optional
  `{CpuTotal,Memory}`5s lease. Expanded/tooltip consumers use `lease(metrics,1s)`
  only for their painted data. Per-metric readers/cadences/baselines are independent;
  dropping a metric releases its files. Explicit laptop CpuPackagePower leases use
  the optional `hyprv_privhelper::cpu_power::{BUS,PATH,INTERFACE}` endpoint only;
  missing/denied telemetry is logged as unavailable, with no combined-helper or
  user-side RAPL fallback. Hidden/compact battery display acquires no watts lease.
  Hidden Network/Temperature/CpuCores are neither sampled nor added to history.
  `temperature_available` distinguishes a retained legacy display from stale or
  unavailable data. Laptop CPU guest/memory-zero/route/temperature quirks stay
  isolated from the desktop sampler. `subscribe()` is an explicit default compact
  lease on laptop; do not use it for a passive forwarding bridge.
- Fan observation is demand-gated on desktop: no inbox/owner watch/GetFans while
  hidden; first open resolves the current owner and hydrates. Only FanStateChanged
  refreshes readings. Explicit hidden actions remain available, and observation
  never activates an absent helper. Laptop fan is inert even if subscribed.

Fakes use injected `Context.paths`, private D-Bus providers, virtual sampling time,
backlight `Transport`, and pinned offline Lua/shell oracles under `tests/fixtures`.
Tests require local `dbus-daemon`, `lua`, `sh`, `awk`, and `jq`; production uses no
subprocesses. Host wiring, nested laptop visual/resource gates and physical XPS
validation remain separate from these crate-level tests. The external legacy
battery notifier/wluma/hypridle are not taken over by these constructors.

Phase B final crate validation: power **50 passed**, helper **39 unit +4 integration
passed** (93 total; no failures). Two manual power benchmarks are ignored in the
normal run; the new private-bus fan receipt was separately run and passed. Both
crates pass all-target strict Clippy (`-D warnings`). The isolated hidden-fan
10.000141s window recorded0ns service-thread CPU,0 voluntary switches and0 GetFans;
this is a quiescent fake-provider case, not a full-shell idle measurement.


### Shared root integration (2026-10-24)

`Paths::default().sys` is `hyprv_core::profile::sys_path("sys")`: with the announced
filesystem-root contract a fake tree is `$HYPRV_SYSFS_ROOT/sys/class/...` and the
normal path is `/sys/class/...`. All production readers, constructors, availability
checks and rediscovery use that path or explicit injected `Context.paths`; no
independent profile enum or detection exists here. Zbus system construction uses
`DBUS_SYSTEM_BUS_ADDRESS` without a fallback builder.

Fixture symlinks may resolve to devices inside the selected sysfs tree, but escape,
missing and denied paths never use host sysfs. DDC hardware is disabled with an
injected tree. Laptop construction uses the pinned XPS backlight profile when HOME
has no controls-profile or contains the workstation DDC profile, so forced laptop
fixtures cannot open the desktop I2C device. An actual laptop backlight profile is
still parsed with all monitor overrides. Fake backlight changes use file events;
fake thermal/BAT updates are read only on their visible leased cadence, with atomic
replacement support. Fixture contexts do not subscribe to host udev/netlink.

`tests/shared_profile.rs` runs fresh child processes for forced desktop/laptop,
XPS autodetection and a broken private-bus address. It exercises the default
constructors (not explicit-profile shortcuts), fake raw brightness/thermals/BAT,
real popup guard lifetime, shared enum identity and zero laptop fan calls.

### Laptop CP follow-up (2026-10-24)

`sysstats::ServiceHandle::lease([Metric::CpuPackagePower], period)` is now allowed
on laptop and only acquires the separate optional CPU-only service. `laptop_strip`
still means CPU+memory/5s, never watts. Helpers retain their fixed1Hz sampling
protocol; no cadence argument is sent. Closing all demand releases even a pending
acquisition; a fast reopen waits for that release before subscribing again. Signal
setup is cancellation-safe. Endpoint absence does not cause desktop-helper fallback.
Panel `power::State.profile` continues to be signal-cached from modern/old PPD;
existing cycle/denial/reconnect tests cover it. Recorder process matching and host
controls projection are tracked in their owners' request files, not implemented
by this crate.
