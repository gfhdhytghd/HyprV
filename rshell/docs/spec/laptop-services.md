# Laptop services: legacy contract and native design

Phase A, 2026-10-24. **Design only; no service, configuration, unit, or runtime
change is part of this document.** Scope: battery, brightness/HDR, power profiles,
CPU thermals, lid, and visibility-driven system statistics. Desktop contracts in
[services.md](services.md) remain unchanged. UI layout belongs to the laptop UI
spec; this document specifies values, actions, lifetimes, and test inputs.

## Source and evidence boundary

All `origin/laptop:` citations below refer to commit
`24972f711735ce92e72ef1db5bdf27f85ae8cfc9`. Paths are relative to the **HyprV git
root**, not rshell. Reproduce a citation without checking out the branch:

```sh
git -C .. show 24972f711735ce92e72ef1db5bdf27f85ae8cfc9:hypr/controls.lua | nl -ba
```

Primary branch inputs:

| Source | Contract |
|---|---|
| `quickshell/BarServices.qml:71–85` | UPower display battery and power profile projection |
| `quickshell/shell.qml:260–357,1405–1450,1770–1805,1936–1972,3416–3428` | Battery labels, popup demand, formatting, detail polling |
| `quickshell/scripts/battery-info.sh:1–237`; `scripts/lib/battery.sh` | Physical battery selection, units, history, estimates |
| `hypr/controls-profile.lua`; `hypr/controls.lua`; `hypr/brightness-hdr.lua` | Laptop curve, actions, persistence, startup |
| `hypr/scripts/control-worker.lua:12–16`; `hardware/xps9350/90-xps-backlight.rules` | Hardware read/write and existing unprivileged access |
| `quickshell/scripts/system-snapshot.sh`; `quickshell/shell.qml:2408–2482,3276–3308` | Stats inputs, arithmetic, cadence |
| `quickshell/ControlPanelPopup.qml:275–312,409–436,547–556`; `quickshell/scripts/power-profile.sh` | Two distinct profile cycle orders |
| `hypr/scripts/battery-notify.sh`; `hypr/hypridle.conf`; `hypr/hyprland-exec-once.lua:33,51–52` | Notification thresholds, external idle policy, wluma startup |

This is source evidence, not an XPS measurement. No XPS hardware is available.
Host-specific lid defaults, installed wluma configuration, real backlight maximum,
sensor availability/permissions, compositor/plugin capability, and resource usage
remain **UNVERIFIED**. Fake-backed tests below are requirements, not claimed passes.

## Runtime profile and ownership

Use the same binary and actors with an immutable runtime `ServicePolicy` selected
before actor construction. Proposed selection, for the architecture owner to
incorporate into `docs/ARCHITECTURE.md`:

1. `HYPRV_PROFILE=desktop|laptop` wins. Reject an invalid nonempty value explicitly.
2. Otherwise select laptop for a recognized XPS product name read once from
   `/sys/class/dmi/id/product_name`, or a present physical system battery. A USB
   mouse battery or UPS is not sufficient. Match a battery's system-power-supply
   identity, not merely a directory name. Keep the recognized DMI names in a
   fixture-tested table; do not guess the target's exact DMI spelling from its
   marketing name.
3. Otherwise select desktop. Log the selected profile and evidence. Missing DMI
   plus unavailable battery enumeration is a diagnosed desktop fallback; override
   makes headless/fake runs deterministic. Latch the selection for this process;
   AC connection, battery removal, and the legacy `bt`/`v2` variant do not switch it.

This proposal is deliberately localized here: Phase A owns this spec, not the
shared architecture file. Detection must be injected in tests and never require
starting a privileged service. Laptop policy disables nct6799 discovery/control,
cooling subscriptions, and automatic RAPL subscriptions. Firmware retains fan
control. A retained `cpu-power-sampler.service` file in the branch does not prove
it is enabled; no laptop enabling reference was found. No new root unit is
justified by the laptop sources inspected.

`hypr/scripts/toggle-battery-mode.sh:9–24` changes the Hyprland config symlink,
shell variant, and reloads configuration. It is neither UPower OnBattery nor a
power-profiles-daemon profile; keep these three concepts separate.

## Battery: event-driven bar state

Legacy uses **UPower.displayDevice**, not the first physical BAT directory
(`BarServices.qml:79–85`). The battery is visible only when the object is ready,
present, and `isLaptopBattery`. Quickshell defines the last predicate as
`Type == Battery && PowerSupply`; its percentage adapter divides the D-Bus value
by 100. The QML multiplies that normalized value back by 100. Native state should
store percentage in 0–100 units once, avoiding a second multiplication.
See [Quickshell's device implementation](https://github.com/quickshell-mirror/quickshell/blob/master/src/services/upower/device.cpp).

| UPower state | Legacy bar mode |
|---|---|
| Absent/not ready/not a laptop battery | `unknown`, absent, percent 0 |
| Charging / PendingCharge | `charging` |
| Discharging / PendingDischarge | `discharging` |
| FullyCharged | `full` |
| Other state | `idle` on battery; otherwise `plugged` |

Clamp the displayed percentage to 0–100. Critical is **raw percent ≤20**, before
rounding, and the bar only paints the critical color when not plugged. Charging,
plugged and full count as plugged. Label: JS-rounded integer `N%`, prefixed by
` ` when plugged; otherwise use index `clamp(round(N/10),0,10)` in
`["󰂎","󰁺","󰁻","󰁼","󰁽","󰁾","󰁿","󰂀","󰂁","󰂂","󰁹"]`.
Absent battery has an empty battery label. An available CPU-power fallback uses
one decimal below 100 W, no decimals at/above 100 W; unavailable is `-- W`
(`shell.qml:260–279,1936–1939`). Availability must not become a sampling request.

Native actor contract:

- Watch the UPower bus owner, root properties, device additions/removals, and the
  display device's `PropertiesChanged`. Establish matches before the initial
  snapshot; serialize initialization/signals and discard replies from an old
  owner/generation. Obtain `GetDisplayDevice` and initial properties once, then
  apply changed properties. Resolve invalidated properties only when necessary.
- Rebind after owner replacement/device topology changes; never keep displaying
  an old battery as current. Represent unavailable/stale separately from zero.
  No periodic `Refresh`, `GetAll`, `upower` subprocess, battery timer, or fallback
  sysfs polling for the bar. A missing provider parks on owner-change monitoring.
- Keep the small signal subscription while a visible bar or low-battery notifier
  needs it; sleeping must not arm a replacement polling loop. Emit UI changes
  only when projected state changes. Ignore unrelated bus traffic rather than
  re-reading all power/profile providers.

UPower exposes a composite display device, device events, OnBattery and lid
properties; a display device may also represent a UPS, hence the laptop filter.
See the [UPower root API](https://upower.freedesktop.org/docs/UPower.html) and
[device API](https://upower.freedesktop.org/docs/Device.html).

## Battery detail: separate physical source and explicit demand

`BatteryInfoPopup.openVisible` is `popupRequested`. Opening requests an immediate
refresh; while requested, `batteryInfoPoll` runs every **1000 ms**. Closing clears
requested before the exit animation finishes, so polling stops then
(`shell.qml:1405–1450,3416–3428`). `PollCommand.qml:15–53` skips overlapping ticks
and coalesces explicit refreshes into at most one pending rerun. Native detail
leases must use this boundary, not surface existence or generic panel visibility.
One immediate logical sample plus 1 Hz samples, serialized, is the native design;
do not depend on the race between legacy timer-start and explicit refresh.

The compact battery click opens this popup even when absent; the expanded click
requires battery availability (`shell.qml:3631–3634,4222–4230`). The branch's
`LaptopBatteryCard.qml` is an unmounted passive component: no other branch file
references it. It is not evidence that opening any control panel starts details.
If mounted later, it must hold its own detail lease only while its card is open.

The detail script picks the **first glob-ordered `/sys/class/power_supply/BAT*`
directory**, without the display-device filter. The bar can therefore describe a
composite battery while detail describes BAT0. Preserve this difference, including
hot-removal/reselection; do not route detail through UPower's estimates.

Exact script arithmetic (`battery-info.sh:20–85,160–237`):

- Missing present defaults to 1; missing status to `Unknown`; missing numeric
  fields to 0. No BAT directory returns `{available:false}` with no history row.
  Present ≤0 makes detail unavailable even if a BAT directory exists.
- Prefer nonempty `energy_now`/`energy_full` even when zero. Otherwise use positive
  charge × positive voltage / 1,000,000, rounded to integer µWh. Prefer nonempty
  `power_now` even when zero; take absolute value. Otherwise positive absolute
  current × positive voltage / 1,000,000 gives rounded integer µW.
- `Charging`/`Pending charge` → charging; `Discharging`/`Pending discharge` →
  discharging; `Full` → full; `Not charging` → plugged, upgraded to full at
  capacity ≥99; all other strings → idle. This differs from UPower bar mapping.
- Signed power is positive charging, negative discharging, zero otherwise.
  Wh/W JSON numbers are formatted to three decimal places by the script.
- Intended history calculation selects positive rows when charging, negative
  when discharging, all rows otherwise. Mean is rounded to integer µW. Estimate
  prefers absolute mean, then instantaneous power; charging uses
  `max(energy_full-energy_now,0)`, discharging uses energy_now, and converts
  energy/power ×3600 to rounded seconds. Zero remaining charging energy/full
  yields 0; unavailable estimates are null. The full-script oracle below verifies this algorithm.

History file: first writable `$XDG_RUNTIME_DIR/hyprv`, else `/tmp/hyprv`, named
`battery-power-history.tsv` (`scripts/lib/battery.sh:3–29`). This is not the HDR
XDG state file. On each completed detail sample prune rows older than
`now_epoch_seconds-1800`, keeping the boundary. Retain valid integer timestamps
and signed integer µW rows in original order, including duplicate/future
timestamps; append the current row and atomically replace. No hidden samples,
gap fill, or deduplication. Native storage must validate directory ownership and
use private permissions; do not reproduce an unsafe shared `/tmp` directory.
Closed intervals do not keep this sampler alive. Multiple cards share one timer,
one snapshot and one row per tick, not one per output/subscriber.

### Actual history and formatting quirks

Correction verified during Phase B (2026-10-24): `battery-info.sh:145` contains
an actual TAB byte (`49 46 53 3d 27 09 27` for the assignment), not a literal
backslash+t. The earlier Phase A split probe used the wrong bytes. The complete
pinned script, extracted into an isolated tree with fake sysfs/clock, confirms
normal history averages/counts/window and `estimateBasis=average`. There is **no**
`QUIRK_LAPTOP_BATTERY_HISTORY_LITERAL_TAB_IFS`; preserve the averaging algorithm
above. The native regression compares all JSON fields and the entire TSV with
that script. This corrects source interpretation, not the legacy behavior.

`shell.qml:295–357,1770–1805` detail strings/rounding:

- Status: charging `正在充电`, discharging `电池供电中`, full `已充满`, plugged
  `已接通电源，当前未充电`; otherwise raw status or `电池状态未知`.
- Available current/average power uses `+` above 0.004 W, `-` below −0.004 W;
  absolute magnitude ≥10 uses one decimal, otherwise two, then ` W`. Unavailable
  uses `--`. Accent uses battery color for charging/full/plugged, critical color
  for detail discharging plus the **bar's** critical predicate, otherwise primary.
- Duration rounds seconds/60 with JS semantics: ≤0 → `0分钟`, then hours/minutes.
  Estimate title is `预计剩余充电时间` for charging/full/plugged, otherwise
  `预计剩余使用时间`.
  Full/plugged use their status strings. Other modes: invalid/negative number →
  `计算中`; otherwise duration plus ` (按当前功率)` for basis current or
  ` (按均值)` for every other basis.
- `Number(null)==0`: a null estimate displays `0分钟 (按均值)`, not `计算中`.
  Preserve as `QUIRK_LAPTOP_NULL_ESTIMATE_AS_ZERO`. Window text: unavailable empty;
  ≥1800 `最近 30 分钟采样`; ≥60 `已采样 ` plus duration; otherwise `开始采样中`.
- Invalid/empty/non-object detail JSON resets to unavailable defaults
  (`shell.qml:1942–1972`). Native errors retain diagnostics but project that
  unavailable state; stale power must not be painted as a current measurement.

Use integer base units and a tested compatibility formatter. Freeze the oracle's
locale and shell/awk versions; JS round, Lua floor(x+.5), and awk `printf` ties
must not be replaced indiscriminately by Rust floating-point `.round()`.

### Low-battery notification

`hypr/scripts/battery-notify.sh:12–14` defaults to threshold 10, reset 15, and a
60-second legacy check. It pins the first BAT at startup. Notify only for exact
sysfs status `Discharging` and capacity **<10**, once per latch. Reset at capacity
≥15 or exact `Charging`/`Full`; other states retain the latch. Pending discharge
does not qualify. The runtime `battery-notify.state` stores this latch. Title
`电量过低 (${pct}%)`, body `请尽快连接电源以避免关机。`, icon `battery-caution`,
critical urgency, synchronous tag `battery-low`. Preserve supported environment
threshold overrides (`THRESHOLD`, `RESET_THRESHOLD`) and single-notification
replacement semantics. Legacy `CHECK_INTERVAL` only controls polling latency;
the event-driven replacement does not carry a periodic interval override.

Native notification subscribes to corresponding physical-battery changes via
UPower/udev; it must not confuse the composite display percentage with physical
capacity. Read that battery's capacity/status on relevant events if needed.
The new event-driven requirement intentionally replaces the 60-second loop;
do not introduce a notification poll timer. Coordinate eventual retirement of
the external legacy loop to avoid duplicate notifications. No automatic shutdown
threshold or suspend action is present in this script.

## Backlight and HDR brightness

### Pinned laptop configuration and exact curve

`hypr/controls-profile.lua` specifies `auto_brightness=true`, backend `backlight`,
output `eDP-1`, mode `preferred`, desktop/client SDR white **153 nits**, peak
**400 nits**, initial raw 1, fallback maximum 400, SDR minimum 0. Monitor overrides:
supports_hdr=1, supports_wide_color=1, min_luminance=0.0005,
max_luminance=530, max_avg_luminance=400. The live sysfs maximum overrides the
fallback; **400 raw units is not a device-independent assumption**.

Let `L=clamp(floor(request+0.5),0,100)`, `M=max_brightness>0`, `R(x)=floor(x+0.5)`:

```text
u = clamp(L, 0, 70)
p = u                         if u <= 10
    10 * 10^((u-10)/60)        otherwise
raw = clamp(R(M*p/100), 1, M)
white = R(153 + 247 * clamp((L-70)/30, 0, 1))
desktop_white = client_reference_luminance = white
```

For M=400: L=0 → raw1/153 nits; 10 →40/153; 40 →126/153;
70 →400/153; 85 →400/277; 100 →400/400. Low UI values do **not** lower SDR white
below 153. Inverse: if raw<M, compute percent=100*raw/M, then round(percent) if
percent≤10, else round(10+60*log10(percent/10)); if raw≥M, use
`round(70+30*clamp((applied_white-153)/247,0,1))`.
Identical rounded raw targets stay identical; never force adjacent hardware steps.
These are not the desktop 10–70 compressed curve or desktop luminance anchors
(`controls.lua:21–41,77–100`).

### Startup, ALS, actions and persistence

- State file `${XDG_STATE_HOME:-$HOME/.local/state}/hyprv/brightness-hdr.json`,
  integer `level`, default70; legacy loads via its integer regex. Write temporary
  file then rename only after the current action succeeds. Keep the compatible
  primary `level` if extending to per-output state.
- Actual startup/reload: `brightness-hdr.lua:1–18` loads saved level and applies
  its 153→400 SDR anchor to plugin configuration/monitor setup. Controls starts
  with those applied anchors. **No automatic call to `controls.restore` was
  found.** Do not invent a startup reset to 70 or restore saved hardware raw.
- Explicit `brightness --restore` invokes `controls.restore`: with auto_brightness
  true, force SDR70/153 nits and persist70, **without writing hardware brightness**;
  with false, request saved level. Separate `StartupAdopt` and `ExplicitRestore`.
- `hyprland-exec-once.lua:51–52` starts wluma.service because the graphical-session
  target is not activated. wluma's configuration/calibration is not in this
  branch. Treat ALS as an external writer: observe it, do not add an ALS sampling
  loop or assume calibration, and do not disable/restart it on manual adjustment.
- `controls.lua:49–65`: if anchors unchanged, skip HDR mutation. Otherwise set
  `hypr_sdr_white.reference_luminance`; when desktop white changes update the
  monitor using its **current position/scale/transform**, configured output/mode,
  bitdepth10, cm=hdr, SDR min0/maxwhite, and all profile monitor overrides. Call
  plugin refresh; only then confirm applied anchors. Missing plugin/output is an
  action error, not successful hardware-only brightness.
- One hardware operation in flight, latest requested level wins. Read confirmed
  raw/max after writing. Superseded results may update actual hardware knowledge,
  but cannot apply obsolete SDR/persist obsolete level. Same-raw requests still
  apply changed SDR and persist. The legacy worker timeout is elapsed integer
  seconds >12 and the warning lasts4000ms (`controls.lua:69–75,131–159`). Native
  uses bounded in-flight deadlines, not the legacy 25ms repeat worker-check loop.
- Pending wanted/busy level wins UI reads; otherwise read hardware and apply the
  inverse curve. Keep optimistic requested, confirmed hardware and applied HDR
  separate. Reconcile external ALS changes without writing them back.
- Keyboard display-brightness actions step ±2 (`hypr/media-binds.lua:22–27`);
  QuickAdjust UI steps ±1. Its 90ms debounce is action-scoped, not a permanent
  monitor timer (`shell.qml:156,2863–2881,3077–3108,3504–3524`). Opening the control
  panel obtains brightness once; its status script deliberately omits periodic
  brightness reads (`ControlPanelPopup.qml:726–735`, `control-panel-status.sh:9–11`).
- Super+brightness controls keyboard backlight separately with a 33% step using
  `*::kbd_backlight`. Do not send it through the display/HDR curve. Give the LED
  device an independent fake/availability and use native LED access where supported.

### Native transport and API changes proposed

Extend `hyprv-svc-power::backlight` with explicit runtime `CurveKind` and startup
policy; keep the current desktop planner default and unchanged. Merely changing
153/400 anchors in the existing desktop curve is incorrect. Preserve all laptop
monitor fields and auto_brightness when loading the literal profile. Reuse the
sequenced brightness engine, with fakeable hardware, persistence and compositor
transports; support udev-originated external changes and owner/output generations.

Read/write `/sys/class/backlight/intel_backlight/{brightness,max_brightness}`
natively. The branch's worker uses requested `brightness`, not actual_brightness,
for inverse/UI feedback. Kernel `actual_brightness` can differ; use backlight
uevents for invalidation and keep the two readings distinct. See the
[kernel backlight contract](https://docs.kernel.org/gpu/backlight.html).
Subscribe before the initial snapshot; coalesce bursts into one read, rescan on
device removal/addition. No periodic brightnessctl, no sysfs inotify assumption,
and no idle brightness timer.

`hardware/xps9350/90-xps-backlight.rules:1–2` grants video-group write access on
intel_backlight add/change. Prefer this existing access; permission failures must
publish unavailable/action failure with diagnostics. A native logind session
brightness method is only a fallback after target API/authorization verification;
it is not evidence to introduce or auto-activate a laptop root helper. Never chmod
devices or change groups from the shell.

Use the existing native Hyprland Lua/IPC transport for equivalent monitor/plugin
actions, with response validation and no compositor reload. On output recreation,
invalidate applied-state caches and reconcile actual state; do not write to a
replacement output from a stale request. All fake/e2e operations use a private
socket and a fake plugin; no live session probes count as XPS proof.

## Power profiles

Signal-driven provider snapshot and changes, not `powerprofilesctl` polling.
Legacy bar maps Performance→`performance`, PowerSaver→`power-saver`, all else→
`balanced`, with ⚡/🔋/⚖️ (`BarServices.qml:71–76`). Control panel labels/icons:
power-saver `Battery`/󰾆; balanced `Balanced`/󰾅; performance `Performance`/󰓅.

**Two distinct cycle actions must remain distinct:**

| Entry point | Cycle | Unknown input |
|---|---|---|
| Control panel, either mouse button | power-saver → balanced → performance → power-saver | balanced |
| `quickshell/scripts/power-profile.sh` CLI | performance → balanced → power-saver → performance | balanced |

Panel writes optimistically, then runs a350ms profile refresh and400ms status
refresh (`ControlPanelPopup.qml:409–436`). Native keeps pending/confirmed state,
sets ActiveProfile over D-Bus, and reconciles replies/signals; replace those
post-action polling delays with acknowledgements. On rejection restore confirmed
state and expose the error. Do not confuse a balanced UI default with an actually
available daemon. Preserve CLI cycle order if its compatibility target is exposed.

Support old `net.hadess.PowerProfiles` (path `/net/hadess/PowerProfiles`) and modern
`org.freedesktop.UPower.PowerProfiles` (path `/org/freedesktop/UPower/PowerProfiles`)
providers, with matching interfaces. Prefer the modern owner if both exist; watch
owner changes and bind only one. Subscribe before reading ActiveProfile/Profiles;
refresh invalidated data selectively. Keep supported profiles, degraded/inhibited
information and action failures typed. No periodic retry when no owner exists,
and no automatic profile change on AC/lid/battery-mode transitions: the branch
defines none. The current Rust CLI-style toggle must not be reused for panel cycle.
Provider properties, names and paths are documented by the
[modern API](https://upower.pages.freedesktop.org/power-profiles-daemon/gdbus-org.freedesktop.UPower.PowerProfiles.html)
and [older API](https://hadess.fedorapeople.org/power-profiles-daemon-docs/gdbus-net.hadess.PowerProfiles.html).

## Laptop CPU thermals and lid

### CPU temperature and cooling

`system-snapshot.sh:5–50` accepts integer milli°C **1…125000 inclusive**. Search
thermal-zone types in this priority order: `x86_pkg_temp`, `TCPU`, `cpu_thermal`,
`cpu-thermal`, `soc_thermal`; within each use the first valid glob-ordered zone.
Fallback: glob-ordered hwmon drivers coretemp/k10temp/zenpower and temp inputs,
accepting labels `Package id 0`, `Tctl`, `CPU Package`, `CPU`. First matching valid
sensor wins, not hottest core, GPU, or Dell fan sensor. Minimal legacy snapshots
omit thermal reads entirely. The QML projection divides by1000 **only when the
raw value is >1000**, so raw1…1000 passes through as Celsius. Missing/invalid
temperature retains the last numeric value, initially0 (`shell.qml:102,2466–2469`).
Preserve these edges as `QUIRK_LAPTOP_TEMPERATURE_GT1000_DIVISOR` and
`QUIRK_LAPTOP_TEMPERATURE_RETAINS_LAST`; typed availability remains unknown/stale,
never a fabricated valid0°C. Compatibility text may still show the retained value.

Native discovers metadata once/on udev changes and samples a selected input at
1 Hz **only while a temperature value or tooltip is actually needed**. No thermal
poll when hidden. Revalidate selection after removal/read error and on next
visible request. Coalesce this with the stats tick. Publish missing/permission
errors once per state transition, with source/error context, then recovery; no
silent failures or log line each tick. Firmware owns cooling; laptop control-panel
status explicitly reports fan_available=false (`control-panel-status.sh:42–43`).
No nct6799 scan, fan lease, PWM write, cooling helper call, or thermal-control unit.

### Lid state and existing external power policy

No explicit lid switch rule, logind HandleLid override, or custom lid handler was
found in the pinned laptop tree. This is **absence of branch policy**, not proof
of the installed host's suspend behavior. Do not invent lid-close→suspend, disable
internal output, or keep-awake behavior.

Expose `lid_present: Option<bool>`, `lid_closed: Option<bool>` through UPower root
properties; a feature-detected login1 `LidClosed` property can be a fallback. Use
initial snapshots plus property changes/owner changes. Unknown must not mean
open. If providers disagree, retain primary UPower data and diagnose the conflict.
Do not assume login1 Docked or OnExternalPower emits property changes. For sleep,
use shared login1 PrepareForSleep events, not a timer watching PreparingForSleep.
See [login1's D-Bus contract](https://www.freedesktop.org/software/systemd/man/latest/org.freedesktop.login1.html).

No `handle-lid-switch` inhibitor. Lid closed alone does not release visible stats
on an external monitor; demand follows actual visible output/session/sleep state.
Suspend cancels sampling and invalidates delta baselines; resume resnapshots active
providers once and restarts only visible leases.

The external `hypr/hypridle.conf:13–26` locks after180s and DPMS-offs after240s
only when any physical BAT status is exactly Discharging; resume DPMS-on is
configured. The600s suspend block is commented out. Preserve external ownership;
do not duplicate these timers inside the shell. Existing manual idle inhibition
is a separate service, not an implicit lid-policy override.

## Minimal-state sysstats: metric demand, not panel existence

Legacy `detailedStats` is any expanded bar OR stats popup requested. Its global
snapshot runs1000ms when detailed,5000ms otherwise, even when all its content is
hidden (`shell.qml:35–37,3276–3308`). `--minimal` reads total CPU, memory, the
legacy package-power file and route; no per-core/network-counter/thermal sample.
The new brief intentionally removes **hidden** reads/timers from this behavior.

The collapsed compact strip **does paint CPU and RAM** (`shell.qml:3609–3613`),
beside minute clock and battery. Therefore visible compact CPU/RAM retain their
5000ms cadence; "minimal" does not mean suppressing visible values. Fullscreen
hide/reveal can move this strip outside its painted region even though its parent
surface remains visible (`BarScreenState.qml:20–22`, `shell.qml:3545–3585`). Demand
must use actual UI visibility, not the surface/container `visible` flag alone.

| Actually painted/requested consumer | Required metric demand | Timer while active |
|---|---|---|
| Compact strip CPU + RAM | CpuTotal, Memory only | shared5000ms |
| Visible expanded CPU/RAM modules | CpuTotal, Memory | shared1000ms |
| Visible network-rate module | selected interface Rx/Tx + route | shared1000ms |
| Stats popup/core tiles | only displayed total/core/memory/network metrics | shared1000ms |
| Visible temperature text/tooltip | Temperature | shared1000ms |
| Battery label / power-profile label / lid | provider signals | none |
| Battery detail popup/card requested | physical battery detail | shared1000ms, independent lease |
| Brightness popup/card | initial/action/udev/output events | no periodic timer |
| Fully hidden stats or suspended session | no stats demand | none |

Take the shortest period requested **per metric**, with one shared scheduler;
two outputs must not double samples. Do not widen a temperature request into
network/core/RAPL requests. Opening a generic control panel is not an expanded
bar or stats-popup request. Hold the final sample as explicitly stale while hidden;
no history append, hidden baseline maintenance, route lookup, cache warming, or
root-helper acquisition. Prime CPU/network deltas on reshow before producing a
new rate; do not spread hidden work over a displayed interval.

For laptop, a typed metric lease is the only sampling authority. State observers,
UI bridges and generic `Topic::subscribe` must not invoke the sysstats fallback
"all metrics" plan. Existing `hyprv-shell/src/services.rs` broad bar/panel leases
include network/temperature/package-power; they are **not** the laptop plan.
Separate state observation from sampler ownership before wiring this profile.
Do not read root-only RAPL directly or activate the privileged package-power
sampler just because a fallback field exists. A future visible CPU-power consumer
needs an explicit capability plus a subscriber-scoped helper lease; absent battery
alone is insufficient. Laptop cooling stays inert regardless of visibility.

Preserve legacy arithmetic at the compatibility boundary when relevant:

- CPU totals sum all `/proc/stat` numeric columns, including guest columns; idle
  includes idle+iowait. Initial/no-positive delta keeps default/last display.
  `QUIRK_LAPTOP_CPU_TOTAL_INCLUDES_GUEST` must not change desktop's kernel-correct
  first-eight-column sampler. Core histories are120 points; CPU/memory append on
  their actual samples, not during hidden intervals.
- Memory is `(MemTotal-available)/MemTotal`; QML's `MemAvailable || fallback`
  treats0 as absent and uses MemFree+Buffers+Cached.
  `QUIRK_LAPTOP_ZERO_MEMAVAILABLE_FALLBACK` preserves this edge case.
- Default route is the first parsed row with destination AND mask `00000000`,
  not lowest metric or first UP route (`shell.qml:1987–1995`). Pin as
  `QUIRK_LAPTOP_FIRST_ZERO_MASK_ROUTE` rather than changing desktop routing.
- Network rate is max(counter_delta,0)/max(elapsed_wall_seconds,0.001). Interface
  change/invalid counters prime/reset. Detailed-visibility changes reset network
  baselines before immediate refresh. A snapshot with no NET section clears its
  baselines but retains displayed rates; preserve this projection when replaying
  legacy fixtures, while marking hidden native state stale. CPU counter regressions
  retain last display. Fixture wall time separately from monotonic scheduling.

These quirks are laptop policy/projection constants, never global changes to
shared samplers. Unchanged desktop behavior remains the existing spec. The only
intentional cadence differences here are eliminating hidden work, replacing
notification/post-action polling with events, and rebasing after a fully hidden
interval. They require explicit resource/lifecycle tests as well as visible parity.

## Test architecture and acceptance required for Phase B

Inject dependencies at actor construction. Tests must not silently fall back to
the machine's real system bus, sysfs, Hyprland socket, helper or wluma service.

| Fake | Required controls and observations |
|---|---|
| Private D-Bus UPower | Root/device properties, display composite vs BAT0/BAT1, UPS/mouse, pending states, invalidation, owner loss/replacement, delayed snapshot replies, device add/remove, percentage noise |
| Physical battery filesystem | Injectable class root + glob order, energy vs charge families, missing/zero/invalid fields, permissions, hot-removal, signed current/power; count every open/read/write |
| History/clock | Separate epoch and monotonic clocks, private runtime dir, seeded TSV, atomic-write failures, time jumps,1800s boundary, duplicate/future rows; compare pinned shell-script oracle |
| Backlight + udev | Injectable raw/max/actual paths, minimum1, real-max variants, video access/denial, device replacement, uevents/ALS writes, delayed write/readback; operation trace |
| Hyprland/plugin transport | Output position/scale/transform/HDR fields, reference luminance/refresh acknowledgements, missing plugin, output removal, command failure and out-of-order replies |
| Power-profile daemon | Both service names, advertised profiles/ActiveProfile, denied set, owner races, both owners simultaneously, unsupported/degraded profile |
| Thermal/proc filesystem | Multiple competing sensors, labels, invalid temperatures, denied/disappearing paths, CPU/guest/wrap cases, zero MemAvailable, routes/counter resets; per-path access counters |
| Lid/login1 | Present/absent/unknown, closed/open, contradictory providers, no-emission properties, sleep/resume and external-output visibility |
| Scheduler/notification sink | Advance virtual time without real sleep; enumerate armed deadlines, leases, history rows, bus calls, helper calls and emitted/replaced notifications |

Mandatory deterministic cases:

1. **Signals-only battery:** advance an idle hour with a visible battery label:
   zero repeated UPower gets/Refresh and zero detail/sysfs samples. Apply a signal
   and verify the correct projected diff. Repeat provider replacement during
   initial snapshot; stale replies cannot resurrect old state. Test percent20 vs
   20.1, every icon boundary, UPS/peripheral rejection and bar/detail disagreement.
2. **Detail lifetime:** open → immediate sample then1Hz; close requested before
   animation ends → no further tick; two cards share one sample; delayed work does
   not overlap; close/reopen invalidates stale completion. Test no BAT/present0,
   energy/current fallbacks, explicit zero preference, modes/99% upgrade, rounding,
   history cutoff and real-TAB splitting against the pinned script. Verify
   null-estimate rendering and all Chinese status/duration/window strings.
3. **Brightness exactness:** compare all101 UI levels against pinned Lua for M=400
   and alternate maxima. Test inverse quantization, raw1, same-target low buckets,
   ±2 keys vs ±1 popup, SDR-only stage, no-op anchor elision, saved startup vs
   explicit restore, and no hardware write on auto restore. Rapid requests + ALS
   writes + delayed completions must never persist/apply stale SDR. Test access
   denial, missing plugin, output replacement, failed persistence and bounded
   timeout. No25ms or periodic read task survives completion.
4. **Profiles/lid/thermals:** assert both cycle orders; denial rolls back optimism;
   no AC-triggered profile change. Assert sensor priority and1/125000 boundaries,
   raw1000/1001 conversion and missing-read retention, no fan/helper access,
   unknown lid remains unknown, no suspend/inhibitor call,
   and closed lid with a visible external output retains only its visible demand.
5. **Minimal wakeup audit:** compact visible CPU/RAM → only5000ms CPU+memory reads
   plus visible minute-clock deadlines; no route/network/temperature/per-core,
   battery-detail, RAPL or fan reads/calls. Hide compact strip and advance an hour:
   zero stats reads/ticks/history appends. Expand network/temperature/core views
   independently and prove only requested sources acquire1Hz leases. Last lease
   drop and suspend cancel deadlines; reshow/resume prime delta baselines.
6. **Notification latch:**9/10 and14/15 boundaries, exact-vs-pending statuses,
   custom thresholds, persisted latch, removal, reconnect and duplicate events;
   never two independent notifier loops.

For script/Lua differential tests, extract the pinned sources into a **small**
fixture tree and inject fake paths/clock/commands at a documented oracle seam.
Record source hash and any seam-only substitutions. Do not patch the live scripts
or run their hardcoded sysfs/state paths. A source-only split probe is insufficient
as the final history regression test. Qt may run offline as the reference oracle;
the native runtime remains Qt-free.

Nested-compositor acceptance must feed identical scripted fakes and virtual clock
to laptop QML and Rust: compact/expanded bar, battery detail transitions, all modes,
critical thresholds, profile cycles, HDR slider/QuickAdjust and thermal tooltip.
Use laptop-specific fixture/reference directories; do not overwrite desktop
references to hide regressions. Run existing desktop bar/island/panels gates and
service tests unchanged. Add runtime tests for visibility-demand release across
multiple outputs, sleep and hotplug. Use the shared CARGO_TARGET_DIR with
CARGO_INCREMENTAL=0; no private target trees.

Finally measure, in an isolated idle-on-battery fake session after exercising and
closing popups: steady RSS target≤120MiB, idle CPU≤0.1%, per-source wakeup table,
zero unchanged-frame GPU submissions, and released hidden surfaces/caches.
Attribute minute-clock/visible5000ms CPU+RAM wakes separately from unsolicited
work. These are **PENDING acceptance measurements**, not results of this Phase A.
XPS physical brightness/ALS/HDR/lid/permission validation remains separately
pending even after all fake and nested tests pass.
