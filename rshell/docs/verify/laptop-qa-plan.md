# Laptop (Dell XPS) profile — QA plan

2026-10-24, parity QA lead. Phase A of `docs/tasks/laptop.md`; Phase B progress in §8.

**User decisions (2026-10-24):**
- D1: read-only island on laptop, same as main.
- Panel: `eDP-1` 2880×1800 px at scale 2 (1440×900 logical).
- No eGPU on the XPS: keep the legacy eGPU UI unchanged, but install no root service for it
  on the laptop.
- Gestures: bar toggle via IPC is sufficient; no uinput.

**Companion specs.**
- `docs/spec/laptop-island.md` (mine).
- The bar/panels/services deltas in the other `docs/spec/laptop-*.md` files (owned by their
  agents). Where this plan names a laptop behaviour outside the island, it cites the laptop
  QML directly; the owning spec is authoritative once written.

**Reference source.**
- `origin/laptop` pinned at **`24972f7`**. Every manifest records the commit, plus the
  `source_sha256` it already writes.
- If the branch moves, re-pin explicitly; the harness never follows it silently.

Citation prefixes:
- `LSH:` = laptop `quickshell/shell.qml`
- `LBS:` = laptop `quickshell/BarServices.qml`

## 1. Principles

1. **One fake world, two consumers.**
   - Every laptop-only source is described once, as a scenario (§2.3).
   - That scenario drives the **QML reference** through the harness mocks
     (`parity/qml-mocks`, canned `processes`), and the **rshell nested run** through
     protocol-level fakes: private D-Bus, a fake sysfs tree, and a stub Hyprland Lua module.
   - The same scenario must yield the same pixels on both sides.
2. **The desktop stays byte-identical.**
   - Mock extensions are additive and default to today's values.
   - Before any laptop capture is accepted, re-capture the desktop `island`, `bar` and
     `panels` with the extended mocks. Every image must be **byte-identical** to the
     published `parity/reference/*`, except distance-field images within the documented ±2.
   - Then run `verify.py` and the desktop nested e2e (204 cases) with `HYPRV_PROFILE` unset
     on this desktop. Any change is a blocker.
3. **No live session use.**
   - Captures run in the private headless Weston (GL).
   - rshell runs as `--nested-child` / `--nested-functional-test` inside a private KWin plus
     nested Hyprland, with a private runtime dir, a private session bus and a private
     "system" bus.
   - No sudo, so no uinput and no real `/sys` writes.
4. **Disk and process hygiene.**
   - Reference source exports and fake trees live in the session scratch dir and are
     deleted after each run.
   - Use the shared `CARGO_TARGET_DIR`, with `CARGO_INCREMENTAL=0`.
   - Check `df -h /` before full captures (904 desktop + laptop images ≈ < 1 GB); abort if
     less than 30 GB is free.

## 2. Rendering the laptop Quickshell reference

### 2.1 Driver (Phase B: `parity/laptop/capture-laptop.sh`)

```
git -C ~/.config/HyprV archive 24972f7 quickshell | tar -x -C "$S/laptop-src"   # read-only export
QUICKSHELL_SRC="$S/laptop-src/quickshell" \
PARITY_FIXTURES=parity/fixtures/laptop-<component> \
PARITY_OUT=parity/reference/laptop-<component> \
PARITY_SCALES="2 1" parity/capture.sh                                          # existing harness
rm -rf -- "${S:?}/laptop-src"
```

- **Components:** `laptop-island`, `laptop-bar`, `laptop-panels`.
- **Outputs:** `parity/reference/laptop-*/` with the same output contract and manifest.
  Add the manifest fields `source_ref: "origin/laptop@24972f7"` and `profile: "laptop"`.
- **Screen:** `eDP-1`, **2880×1800 px at scale 2 = 1440×900 logical**
  (`hyprland-monitors.lua:5`, user-confirmed). Scale-1 renders use the same 1440×900
  logical canvas.
- **Theme tokens.** The laptop `shell.qml` token block matches main in the lines checked.
  Phase B re-verifies the island scene's `ShellRootMock.qml` copy against `LSH:` before
  the first capture.
- **Island scene** (`scenes/island.qml`):
  - It already loads `DynamicIsland.qml` from `QUICKSHELL_SRC` and deep-copies `props`.
  - Add `agentPending` / `agentPendingCount` to the copied props.
  - Record the four new signals as `"signals"` in the dump. This lets e2e-style fixtures
    assert which signal a click emits.
- **Desktop scene** (`scenes/desktop.qml`): it loads the laptop `shell.qml` unchanged.
  - Laptop starts with **every bar compact** (`expandedBarScreens = {}`).
  - Expanded fixtures use the step `{"set":{"ipc":["bar","toggle","eDP-1"]}}`, then advance
    by `barMotionDuration`.
  - Note the 500 ms toggle debounce. The virtual clock starts at the fixture's `now`, so a
    toggle at t < 500 ms after an earlier toggle is dropped, as live.

### 2.2 Mock gaps the laptop QML needs (Phase B, `parity/qml-mocks`, all additive)

| module | laptop reads (cite) | today's mock | change |
|---|---|---|---|
| `Services.UPower` | `UPower.displayDevice.{ready,isPresent,isLaptopBattery,percentage(0..1),state}`, `UPower.onBattery` (`LBS:79-85`) | `displayDevice` without `isLaptopBattery` | add `isLaptopBattery` (default false, so the desktop is unchanged); keep `state` ints = `UPowerDeviceState` |
| `Services.UPower` | `PowerProfiles.profile`, `PowerProfile.*` (`LBS:71-76`) | present | none; add a `hasPerformanceProfile` scenario switch |
| `Services.Mpris` | player objects: `isPlaying`, `playbackState`, `trackTitle/Artist/ArtUrl/Album`, `length`, `lengthSupported`, `position`, `positionSupported`, `dbusName`, `identity`, `positionChanged` (`LBS:87-149`) | count only (`values` is plain data) | QtObject players built from `services.mpris.players[]`; `position` advances with virtual time while `isPlaying` |
| `Networking` | `backend`, `devices.values[].{type,connected,name,networks.values[],scannerEnabled}`, `WifiSecurityType`, `DeviceType`, `NetworkBackendType` (`LBS:15-56`) | `devices` empty | device + network QtObjects from `services.wifi.devices`; `scannerEnabled` writable and recorded |
| `Services.Pipewire` | `defaultAudioSink.{ready,audio.{volume,muted}}`, `ready` (`LBS:58-69`) | present (desktop) | verify only |
| QtQuick.Controls | `import QtQuick.Controls as Controls` (`LSH:4`) | real Qt | none (Qt 6.12 has it) |

**Canned processes** (laptop command lines):

| match | used for | canned output |
|---|---|---|
| `scripts/battery-info.sh` | battery popup, polled 1 s while open (`LSH:3416-3429`) | JSON `{available, status, mode, capacity, powerW, averagePowerW, sampleCount, sampleWindowSeconds, windowComplete, estimateSeconds, estimateBasis, energyNowWh, energyFullWh}` per scenario |
| `scripts/system-snapshot.sh --minimal` / `--full` | bar stats: 5 s compact, 1 s detailed (`LSH:3276-3286`, reset `LSH:3304`) | per scenario: CPU, mem, net, temperature from x86_pkg_temp, hwmon or none |
| `hyprctl repl` `brightness_get` | brightness read (`LSH:3081`) | the integer level |
| `hyprctl eval` `brightness_set(` | brightness write | exit 0; the command line is recorded for the e2e cross-check |
| `hyprctl -j animations` | `BarMotion.qml:38` (slide duration/curve) | the laptop `hyprland-main.lua` animation values |
| `scripts/control-panel-status.sh` | CP status | laptop script values; `fan_available=false` (laptop script line 43) |
| `swaync-client -swb` | notifications | `stream: true` lines |
| `scripts/agent-island-watch.py` | agent island | `stream: true`, `lines` at virtual ms (snapshots incl. `pending`) |
| `scripts/agent-island-codex-appserver.py` | spawned on laptop | exit 0 (codex absent) |
| `sh -lc command -v cava` | spectrum gate | exit 0/1 per scenario |
| `scripts/audio-spectrum.sh` | spectrum | the existing desktop canned stream |
| `scripts/ui-state.sh print` | theme init | the existing desktop canned output |

### 2.3 Scenario catalogue: one source, two consumers

| source | scenario knobs | QML reference consumer | rshell nested consumer (fake) |
|---|---|---|---|
| **UPower battery** | present, percent, state (Charging, Discharging, FullyCharged, PendingCharge, PendingDischarge, Unknown, Empty), `OnBattery`, `isLaptopBattery`, energy/power fields for the popup | `services.upower` + `battery-info.sh` JSON | **fake `org.freedesktop.UPower`** on a private system bus (`DBUS_SYSTEM_BUS_ADDRESS`): `/org/freedesktop/UPower` (`OnBattery`, `EnumerateDevices`, `GetDisplayDevice`) + `devices/DisplayDevice` + `battery_BAT0` + `line_power_AC`, with `PropertiesChanged` emitted on scenario steps. The popup's 30-min history comes from the **fake sysfs** `class/power_supply/BAT0/*` (§2.4) |
| **power-profiles** | profile, `hasPerformance`, `PerformanceDegraded`, set-calls recorded | `services.powerProfile` | **fake `org.freedesktop.UPower.PowerProfiles`** (+ `net.hadess.PowerProfiles` alias) on the private system bus; `ActiveProfile` writable and recorded |
| **backlight + HDR** | level 0..100, `max_brightness` 400 (`controls-profile.lua`: `initial_max=400`, backend `backlight`, `eDP-1`, desktop/client 153, peak 400) | canned `hyprctl repl brightness_get`, recorded `hyprctl eval brightness_set` | nested Hyprland loads a **stub `hypr.controls` Lua module**. `brightness_get` returns the scenario level; `brightness_set`/`_finish`/`step` append to `$S/controls.log`. HDR SDR-white mapping (`targets()`: white = 153 + 247·clamp((L−70)/30)) and hardware mapping (linear ≤ 10 %, log 10..70) are compositor-side. Verify them by evaluating the **real** laptop `controls.lua` `M.plan(L)` in the nested Hyprland with `cfg` pointed at the fake sysfs (§3.4); the shell only has to send the right levels. If rshell gains a native backlight path for laptop, it reads the fake `class/backlight/intel_backlight/{brightness,max_brightness,actual_brightness}` |
| **lid** | `LidClosed`, `LidIsPresent`, `PrepareForSleep(true/false)` | none: the laptop QML and hypr Lua have **no lid consumer** (grep of `24972f7`) | **fake `org.freedesktop.login1`** Manager on the private system bus (rshell `hyprv-svc-power/src/power.rs:116-137` subscribes to it). The assertion is **no UI change and no wakeup burst** on lid toggles; sleep/resume re-syncs within one frame |
| **thermals** | `x86_pkg_temp` zone, `TCPU`, hwmon `coretemp` only, none; values incl. ≥ the bar's warn/critical thresholds | canned `system-snapshot.sh` | fake sysfs `class/thermal/thermal_zone{0..n}/{type,temp}` + `class/hwmon/hwmon*/{name,temp*_input,temp*_label}`; same preference order as `system-snapshot.sh:14-41` |
| **DMI / profile detect** | `product_name` (`XPS 15 9530`, `XPS 13 9340`, a desktop name), battery present or absent | n/a | fake sysfs `class/dmi/id/product_name` + `power_supply`; plus the `HYPRV_PROFILE` override |
| **Wi-Fi (laptop native NM path)** | device, networks, signal, security, known | `services.wifi.devices` (new mock) | the existing private-bus NM fake used by desktop e2e |
| **MPRIS** | players, metadata, position | new Mpris mock | the existing private session-bus MPRIS fake |
| **Agent** | sessions, `pending` (approval/question, options), emissions over time | canned `agent-island-watch.py` stream | private `HYPRV_AGENT_ISLAND_DIR`: the fixture writes `state.json` exactly as `agent_island.py` would; responses land in `responses/` |
| **Touchpad / edge gestures** | bar toggle per monitor | IPC step `bar toggle eDP-1` | `hyprv-shell ipc call bar toggle eDP-1`. The gesture itself is the Hyprland edge plugin (`hyprland-plugins.lua:196-205`), and workspace swipe is compositor-only. Per the user, IPC coverage is sufficient: no touchpad injection |

### 2.4 Fakes to build (Phase B)

**Location.** `parity/laptop/fakes/`: Python + `gi` (Gio D-Bus server and GLib main loop,
already required by the legacy watcher), so there are no new packages. `python-dbusmock`
and `umockdev` are **not installed** and cannot be without sudo.

**`fake-system-bus.py <scenario.json> <control-fifo>`.**
- Starts a private `dbus-daemon --config-file=<session-like config>` and prints its
  address.
- Exports UPower, PowerProfiles and login1.
- Applies timed scenario steps (`at_ms`, property changes, signals).
- Logs every method call (`SetActiveProfile`, `Inhibit` …) to NDJSON.

**`make-sysfs.py <scenario.json> <root>`.**
- Builds `class/power_supply/{BAT0,AC}`, `class/backlight/intel_backlight`,
  `class/thermal`, `class/hwmon` and `class/dmi/id`.
- It can rewrite values at scenario steps, using atomic rename so inotify consumers see
  them.

**Request to service owners (filed).** Each sysfs reader must honour a test-only root
override, proposed as **`HYPRV_SYSFS_ROOT`** (default `/sys`), plus
`DBUS_SYSTEM_BUS_ADDRESS` for every system-bus client.
- It is read once at startup.
- It has no effect when unset, so the desktop stays byte-identical.
- Without it, nothing laptop-specific can run here.

## 3. States to cover (references + rshell renders, both themes, scale 2 and 1)

Fixture names are `laptop-<component>/<state>`. Counts are per theme and scale.

### 3.1 Battery (bar module, battery popup, `LSH:260-345`)

1. **Absent.** No battery module. `batteryOrCpuPowerText` falls back to CPU power. A
   desktop-like laptop: dock, battery removed.
2. **Discharging, glyph buckets.** `round(p/10)` → 11 glyphs `󰂎󰁺…󰁹`. Percents
   **0, 4, 5, 14, 15, 50, 94, 95, 100**: the bucket edges, since JS `Math.round(0.5)=1`.
3. **Critical colour.** `batteryCritical = percent ≤ 20`, coloured only when not plugged:
   **20** (critical `#e92d4d`) vs **21** (normal), discharging. 20 % while charging is not
   red.
4. **Charging.** The plug glyph `` replaces the level glyph (`batteryPlugged`). Covers
   PendingCharge → "charging", PendingDischarge → "discharging", FullyCharged → "full",
   `onBattery && Unknown` → "idle", and else "plugged".
5. **Full.**
   - UPower FullyCharged.
   - The popup's `battery-info.sh` maps `Not charging` with capacity ≥ 99 to `full` (popup
     text 已充满). Capacity 98 with `Not charging` gives `plugged`
     (已接通电源，当前未充电).
6. **Battery popup**, for each mode:
   - status text, accent colour (critical only when discharging ≤ 20), power `formatPower`;
   - average and estimate, with `estimateBasis` current → 按当前功率 and average → 按均值;
   - `estimateSeconds` null → 计算中;
   - window incomplete (`sampleCount` 1);
   - popup open/close frames.
   - `LaptopBatteryCard.qml` is **dead code** (no QML references it); no fixtures.
7. **Live transitions.** Plug in while discharging (UPower `State` signal → glyph swap
   within one frame). Percent ticks 21 → 20 (colour flip) with no other repaint.

### 3.2 Minimal vs expanded bar (laptop adaptive bar; island facts in `laptop-island.md` §4)

1. **Boot.** Compact, all screens: `reveal 0`, exclusive zone 25, island hidden. The top
   corners show (`LSH:3585`).
2. **Toggle to expanded.** Slide frames at the `barMotionDuration` sample points
   (t = 0, ¼, ½, ¾, end), from `hyprctl -j animations`.
3. **Toggle back to compact.** Same frames.
4. **Debounce.** Two toggles 300 ms apart leave one state change. Two toggles 600 ms apart
   leave two.
5. **Stats.**
   - Compact: `--minimal` every 5 s, no detailed stats, the network rate only if visible.
   - Expanded or stats popup open: `--full` every 1 s.
   - Render both, and the switch that resets the network-rate baseline
     (`onDetailedStatsVisibleChanged`).
6. **Fullscreen client in each mode** (`barState.fullscreen`/`fullscreenHide`).
7. **Compact + control panel open.** `mediaUiVisible` is true even though the bar is
   compact: the spectrum and position run, but the island stays hidden.

### 3.3 Power profiles

1. power-saver 🔋, balanced ⚖️, performance ⚡ in the bar and the CP tile.
2. No performance profile (`hasPerformanceProfile=false`).
3. A profile change via D-Bus signal while the CP is open.

### 3.4 Brightness and HDR

The UI step is **1 %** on laptop (`brightnessUiStepPercent: 1`), vs 2 on desktop.

1. CP slider and QuickAdjust OSD at levels **0, 1, 10, 11, 69, 70, 71, 85, 100**:
   - 1 is the backlight minimum;
   - 10/11 is the linear→log knee;
   - 70/71 is where the HDR SDR white starts to rise above 153.
2. One wheel or key step from 50 gives 51 (laptop) vs 52 (desktop).
3. OSD `quickAdjust showBrightnessLevel <L>` (IPC from `controls.lua:47`) at the same
   levels.
4. **Compositor-side mapping.** Evaluate `require("hypr.controls").plan(L)` for every L in
   0..100 in the nested Hyprland, using the laptop `controls.lua` + `controls-profile.lua`
   with the fake sysfs.
   - Compare against the closed form: raw = clamp(round(400·hw(L)/100), 1, 400), where
     hw(L) = L for L ≤ 10, else 10·10^((L−10)/60), capped at 70; white = 153 + 247·t.
   - This is a table check, not a pixel check, and it guards the shell → compositor
     contract.

### 3.5 Thermals and CPU power

1. Temperature from `x86_pkg_temp`, from hwmon `coretemp` only, and with no sensor
   (`--`). Values at and around the bar's colour thresholds (bar spec).
2. CPU power on laptop follows `batteryOrCpuPowerText`. Per `ARCHITECTURE.md` (profile
   contract), the laptop profile cannot activate privileged RAPL clients. So:
   - battery present → battery text (the usual laptop state);
   - battery absent → `-- W` (the legacy result too, since RAPL is root-only).
   Fixtures cover both.

### 3.6 Island (laptop deltas; full list in `laptop-island.md` §9)

D1 = read-only: items 1–8 are captured from the legacy laptop source as **expected
divergences** (component `laptop-island-legacy`, excluded from the gate). The gated island
set is item 9 plus item 10, rendered with **main's** read-only DynamicIsland semantics: the
laptop source differs from main's only in the withdrawn deltas, so item 10 is gated against
the existing desktop island references at the laptop geometry.

1. Approval pending, n = 1 and long detail.
2. Question pending with 0, 1, 3 and 4 options, and long CJK options.
3. Pending with n = 3 and n = 9.
4. Hover and press on Allow, Deny, Reply and an option.
5. Accent quirk (pending owned by a non-primary session).
6. Pending appear/disappear height morph frames.
7. `LIST_RESET`: 9 sessions scrolled to the middle, then an emission, then back at the top.
8. Ring cadence (5 s emissions).
9. Compact bar: island transparent; reveal frames.
10. Everything in `ui-island.md` §10 re-captured from the laptop source.
    - Expected identical to desktop, except where the deltas apply.
    - This proves no unlisted delta: any difference outside the deltas is a spec bug I
      must file.

### 3.7 Panels with laptop-specific content

Only where the laptop source differs. The panels owners' specs list the deltas; the QA
matrix follows them.

1. **Control panel:**
   - Fan section absent (`fan_available=false`).
   - eGPU card: kept exactly as legacy (the XPS has no eGPU, so it shows the legacy
     no-device state); **no root service** is installed for it on the laptop.
   - Battery and power rows.
2. **Stats popup:** the laptop `SystemStatsPopup.qml` and `SystemTrendChart.qml` differ
   heavily from main (193/185 and 18/11 lines added/removed vs the main tree). Cover all
   tabs in compact and detailed sampling.
3. **Wi-Fi panel:** native NM device path (`LBS:15-56`), including scanner enable while the
   page is visible.

## 4. E2E list (nested, laptop profile, fakes from §2.4)

The harness is the existing `hyprv-shell --nested-functional-test` with
`HYPRV_PROFILE=laptop`.
- Input goes through virtual pointer/keyboard.
- Typed service commands are captured by the mock sink, with exact ordered command arrays.
- An ID is pass only if the UI assertion **and** the command/signal record match.

| ID | action | expected |
|---|---|---|
| `lap.profile.detect.dmi` | start with no override; fake DMI `XPS …` + BAT0 | laptop profile; logged reason |
| `lap.profile.detect.desktop` | fake desktop DMI, no battery | desktop profile; desktop e2e subset passes unchanged |
| `lap.profile.override` | `HYPRV_PROFILE=desktop` on XPS fakes, and vice versa | the override wins |
| `lap.bar.boot.compact` | start | all bars compact, exclusive zone 25, no island surface content |
| `lap.bar.toggle` | `ipc call bar toggle eDP-1` | expanded after the slide; `layoutStatus` JSON matches the legacy keys |
| `lap.bar.toggle.debounce` | two toggles 300 ms apart | one transition |
| `lap.bar.toggle.synced` | two outputs, toggle one | both expand together (user request; legacy was per output) |
| `lap.battery.discharge.tick` | UPower percent 21 → 20 | glyph and colour flip; no other surface damage |
| `lap.battery.plug` | `State` Discharging → Charging, `OnBattery` false | plug glyph |
| `lap.battery.full` | FullyCharged | full rendering |
| `lap.battery.popup.open` | click the battery module | popup opens; the fake sysfs history is read; estimate text per mode |
| `lap.battery.popup.close` | click outside / Esc | closes; the 1 s sampling stops (no further sysfs reads) |
| `lap.power.profile.set` | CP profile tile → power-saver | fake PPD `ActiveProfile` set once; UI follows the signal |
| `lap.power.profile.external` | the fake changes the profile | bar/CP update, no command emitted |
| `lap.brightness.slider` | drag the CP slider 50 → 71 | ordered `BrightnessSet(n)` (`brightness_set(n)`), one per distinct value, the last being 71; **never** `BrightnessFinish` / `brightness_finish` on laptop (`laptop-quickadjust.md` §2 rows 5–6, `laptop-panels.md` §1.4); one read-back 90 ms after the last set (`QUIRK_LAPTOP_BRIGHTNESS_PROBE_AFTER_SET`), and no read-back overwrites the value mid-drag. (Corrected 2026-10-24: the earlier "final `brightness_finish(71)`" was main's semantics.) |
| `lap.brightness.wheel` | wheel one notch on the CP slider | ignored: the laptop `ControlPanelSlider.qml` has no wheel handler. The ±1 step applies to the IPC and keyboard path (§3.3) (corrected 2026-10-24) |
| `lap.brightness.osd` | `ipc call quickAdjust showBrightnessLevel 70` | OSD at 70 |
| `lap.brightness.plan` | Lua `plan(L)` for L = 0..100 | matches the closed form (§3.4) |
| `lap.lid.close_open` | login1 `LidClosed` true → false | no UI change; no timer or wakeup burst (perf delta ≤ noise) |
| `lap.sleep.resume` | `PrepareForSleep(true)`, then `(false)` | state re-synced (battery, profile, clock) on resume within one frame |
| `lap.thermal.source` | fake zones: pkg → hwmon only → none | bar temperature source and `--` |
| `lap.media.compact.idle` | MPRIS playing while the bar is compact | no cava, no position reads (attributed wakeups 0) |
| `lap.media.expand` | toggle the bar while playing | island shows current track and position on the first frame |
| `lap.island.hidden.compact` | agent active, bar compact | no island draw; agent model still updates (state-file change → state observed via `dataStatus`) |
| `lap.island.agent.focus` | click a session row | the focus action (existing desktop case) |
| `lap.island.readonly` | pending approval in `state.json`, expand the island | no pending card or action buttons; the expanded agent view equals main's (`ui-island.md` §3.14) |
| `lap.island.hook.nonblocking` | run `hyprv-agent-hook` with a PermissionRequest payload | exits at once (main semantics); installer writes `PermissionRequest`/`Notification` timeout 5 on the laptop profile too |
| `lap.island.list_inplace` | 9 sessions, scroll to the middle, write `state.json` | scroll position and rows kept (`AgentTaskModel` semantics) |
| `lap.cp.nofans` | open the CP | no fan section; privhelper not contacted for fans (D-Bus log) |
| `lap.privhelper.inert` | laptop profile start | no `GetFans`/`SubscribeCpuPower`/`RemoveExternalGpu` calls; the install plan for the laptop contains no cooling, RAPL or eGPU root service |
| `lap.egpu.ui` | open the CP with no eGPU device | no eGPU card: laptop `confirmEgpuAction()` exists (`ControlPanelPopup.qml:396`) but no item calls it, so legacy shows nothing. No root service; the inactive channel rejects actions (corrected 2026-10-24) |
| `lap.wifi.scan.visible` | open the Wi-Fi page | NM `RequestScan` / scanner enabled only while visible |
| `lap.desktop.regression` | the full desktop nested e2e (204) + parity on this desktop, profile auto | unchanged: 204/204, the same parity counts as `docs/verify/parity.md` |

## 5. Minimal-state budget measurement (laptop)

**Setup.**
- Nested run, laptop profile, fakes: `OnBattery=true`, Discharging 63 %, power-saver, no
  MPRIS player, no agent sessions.
- All bars compact, nothing open. Settle 60 s, then measure 600 s.

**Wakeups.**
- `perf trace -s` / `perf sched` on the shell PID and threads, and timerfd/epoll inventory
  from `/proc/<pid>/fdinfo`. Reuse `docs/verify/live-idle.py` and `measure-shell.py`.
- Attribution table: one row per timer or fd source.
- Expected:
  - the clock minute tick (1/min);
  - UPower signals only when the fake changes;
  - **zero** sysstats sampling of hidden detailed stats;
  - no network-rate sampling unless that module is visible in compact;
  - no agent tick;
  - no frame callbacks.

**GPU.** No commits or frame callbacks while static. Count with a `WAYLAND_DEBUG=1`
subset on the nested client, or the engine's frame counter.

**Surfaces.** Panels and popups are destroyed when hidden. Count `wl_surface` objects
before and after the open/close cycles. Reopen latency must be within the animation budget
(first frame ≤ 1 vsync after the request).

**Targets.**
- RSS ≤ 120 MiB steady.
- Idle CPU ≤ 0.1 %.
- Report real numbers in `docs/verify/laptop-budget.md`, plus the same run with
  `HYPRV_PROFILE=desktop` as the regression baseline.

## 6. Gates and order

1. **B1.** Mock extensions → desktop re-capture byte-identical (blocker gate).
2. **B2.** Laptop references captured; `laptop-*` fixture counts and the manifest committed
   to the tree (no git commit by me; the files only).
3. **B3.** Fakes + `HYPRV_SYSFS_ROOT` / system-bus overrides available, from the owners.
4. **B4.** `verify.py --component laptop-{island,bar,panels}` with the same strict GL gate
   as desktop.
5. **B5.** E2E §4 all pass. Budget §5 within targets.
6. **B6.** Desktop regression (parity + 204 e2e) unchanged.

## 7. Decisions (resolved 2026-10-24)

- **D1:** read-only island on laptop (main semantics). Legacy interactive states are
  recorded as expected divergences only.
- **D-Q1:** XPS `eDP-1` 2880×1800 at scale 2.
- **D-Q2:** bar toggle via IPC is sufficient.
- **D-Q3:** no eGPU; legacy UI kept, no root service on laptop.
- **Laptop CPU power:** no privileged RAPL on laptop (`ARCHITECTURE.md`).

## 8. Phase B progress (2026-10-24, PQ-QA)

| gate | status | evidence |
|---|---|---|
| B1 desktop identity | **PASS** (re-run 2026-10-24 after the host change) | full desktop recapture with the current host and mocks: 992/992 unchanged (942 byte-identical, 50 distance-field images within ±2), the same as the first B1. The in-window construction is laptop-only (`PARITY_SCENE_IN_WINDOW`); unconditionally it would move 112 desktop images. `parity/laptop/b1-desktop-identity.py` |
| B2 laptop references | **done (recaptured 2026-10-24 with the in-window host)** | `parity/laptop/reference/`: `laptop-bar` 248, `laptop-panels` 152, `laptop-island-legacy` 40 (not gated) = 440 images. Source `origin/laptop@24972f7`, 1440×900 logical, scales 2 and 1, light and dark. 73 fixtures from `parity/laptop/gen-laptop-fixtures.py` |
| B3 fakes + shell hooks | fakes **done**, hooks pending | fakes self-test 6/6 (`docs/verify/laptop-e2e.md`). `HYPRV_PROFILE`, `HYPRV_SYSFS_ROOT` and the laptop suite were requested from hyprv-shell, svc-power and svc-sys |
| B4 Rust vs laptop references | **PASS** (final run 2026-10-24 12:52) | `laptop-bar` 248/248 (spec-bar replay, full frames); `laptop-panels` CP 92/92 and QuickAdjust 32/32 (card region), battery popup 28/28 (full frames); island = main island (D1) 380/380 GL. `parity/laptop/run-laptop-qa.py` → `docs/verify/parity-laptop.md` |
| B5 e2e + budget | e2e **PASS**; budget owned by engine | final run (release built 12:35, sha256 `3cd717d8…`): fakes 7/7, laptop nested suite 37/37 (the 32 spec cases, `lap.strip.*` ×4, `ipc.shell.kill`). Nested run: `HYPRV_PROFILE=laptop`, fake bus as system bus, fake sysfs root, controls stub, staged `HYPRV_ASSET_ROOT`, 1440×900 @2. Coverage gaps filed (`hyprv-shell.md`): island-hidden with an agent, slider read-back, nested `plan()` eval. The minimal-state budget is measured by engine, not here |
| B6 desktop regression | **PASS** (final run 2026-10-24 12:52) | desktop nested e2e 204/204 (auto-detect); desktop parity GL island 380, bar 300 + 12 excluded, panels 300, all equal to the baseline; B1 covers the references |

**Reference artifact fixed (2026-10-24).**
- **Cause:** parity-host built the scene window-less, so items created at startup loaded at qApp DPR 1. This
  affected the laptop CP page (always-active Loader, so `light-dark.svg` was rasterised at 44 px), the strip/tray
  icons and the tooltip layers.
- **Fix:** laptop captures now build the scene inside the scale-N window (`PARITY_SCENE_IN_WINDOW=1`, see
  `parity/laptop/README.md`).
- **Recapture:** `laptop-bar` 182/248 images changed (120 @2, plus 62 @1 with max-1 animation-timing noise),
  `laptop-panels` 106/152, `laptop-island-legacy` 0. The old set is kept in scratch until sign-off.
- **Result:** the laptop CP replay is now **92/92** (it was 56/92).
- **Desktop:** the same artifact exists in the desktop references (98 bar and 6 island images at @2, when
  re-rendered in-window). Desktop is deliberately unchanged, because it is signed off.

Harness changes are listed in `parity/laptop/README.md`. All are additive.
