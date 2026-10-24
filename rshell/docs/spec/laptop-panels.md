# Laptop spec — panels delta (`origin/laptop` vs main)

Scope: everything a **panel** renders or does differently on the laptop (Dell XPS 13 9350)
branch, relative to the main working tree that `ui-panels*.md` was written from. Anything not
listed here is **identical** and the desktop spec applies unchanged. Read `ui-panels.md`,
`ui-panels-control.md`, `ui-panels-stats.md` (QuickAdjust §1) and `ui-panels-calendar.md` first.

Sources: `git show origin/laptop:quickshell/<file>` (commit `24972f7`), compared against the
`../quickshell` working tree as of 2026-10-24. Laptop line numbers are written `L:file:line`.
Main line numbers are the ones cited in the desktop specs.

Profile switch: every difference below is chosen at runtime by `Profile::Laptop`, following
the rule in `docs/ARCHITECTURE.md`. Constants named `QUIRK_*` reproduce legacy behaviour on
purpose and are laptop-only unless the entry says otherwise. Desktop output must stay
byte-identical.

## 0. File-level delta

| file | laptop vs main | effect on panels |
|---|---|---|
| `ControlPanelPopup.qml` | different | §1 (tiles, sliders, positioning, brightness finish, page lifetime) |
| `ControlPanelSlider.qml` | different | §1.4 (no `adjustmentFinished`) |
| `WindowsVmControl.qml` | older self-polling version; **not instantiated** | §1.6 |
| `WindowsVmController.qml` | absent | §1.6 |
| `ViewflowTargetControl.qml`, `DeskflowTargetControl.qml` | identical files, **not instantiated** | §1.6 |
| `LaptopBatteryCard.qml` | laptop-only file, **not instantiated anywhere** | §5 |
| `AnimatedGlassPanel.qml` | shadow layer always enabled | none visible (§6) |
| `SystemStatsPopup.qml`, `SystemTrendChart.qml` | no `Loader`, charts always repaint | none visible (§6) |
| `QuickAdjustPopup.qml` | identical file | behaviour delta through shell bindings, §2 |
| `WeekCalendarPanel.qml`, `CalendarBridge.qml` | identical files | lifetime delta in `shell.qml`, §3 |
| `StartupConfirm.qml` | identical file | launcher delta only, §4 |
| `WifiFallback.qml`, `WifiNative.qml` (`= WifiFallback {}`) | identical | none. The Wi-Fi status source differs (`BarServices.nativeWifi`), which is svc-net's concern |
| `ControlPanelSplitTile.qml`, `ControlPanelToggle.qml`, `ControlPanelMediaCard.qml`, `ControlPanelBluetoothDetails.qml`, `Mihomo*.qml`, `BatteryInfoPopup` (component in `shell.qml`) | identical | none. Battery popup becomes reachable, §5 |
| `scripts/control-panel-status.sh` | `fan_available=false` always; recording detection uses `pgrep -x` on exact names | §1.5 |

Output geometry on the laptop: `eDP-1` is 2880×1800 at scale 2, so **logical 1440×900, DPR 2**
(`L:hypr/hyprland-monitors.lua:4-16`). External outputs use `preferred/auto/auto`.

---

## 1. Control panel (`ControlPanelPopup`)

### 1.1 Main page layout (`L:ControlPanelPopup.qml:506-668`)
The card is **278 × 350** (main is 278 × 422): one slider row instead of two. The constants
are unchanged (`moduleSize 62`, `cardSpacing 10`, `columnWidth 134`).

```
y=0   Row (spacing 10)
      ├ left Column x=0 w=134
      │   y=0    Wi-Fi SplitTile        134×62          (unchanged)
      │   y=72   Bluetooth SplitTile    134×62          (unchanged)
      │   y=144  Power SplitTile        134×62          ← replaces WindowsVm │ Viewflow
      │   y=216  Screen-record x=0 62×62 │ Prevent-sleep x=72 62×62   (unchanged)
      └ right Column x=144 w=134
          y=0    MediaCard              134×134         (unchanged)
          y=144  Dark-mode x=144 │ DND x=216            (unchanged)
          y=216  Display SplitTile      134×62          ← replaces Demo │ Power-profile toggles
y=288 Grid (2 columns, spacing 10)
      (0,288) Brightness slider 134×62   (144,288) Volume slider 134×62
height = 278 + 10 + 62 = 350
```

New tiles. Both use `ControlPanelSplitTile` (`ui-panels-control.md` §2.1) at 134×62, with the
defaults `expandIndicatorVisible=false`, `expanded=false` and `destructive=false`:

| tile | icon | title | subtitle | active | left / right click | src |
|---|---|---|---|---|---|---|
| Power | `powerProfileIcon(p)`: `power-saver` U+F0F86, `performance` U+F04C5, else U+F0F85 | `"Power"` | `powerProfileLabel(p)`: `power-saver` → `"Battery"`, `performance` → `"Performance"`, else `"Balanced"` | `p == "power-saver"` | both → `cyclePowerProfile()` | `L:…:547-557`, `:275-293` |
| Display | U+F0379 | `"Display"` | the tile's own `active` ? `"Demo mode"` : `"Normal"` | `demoModeEnabled` | both → `toggleDemoMode()` | `L:…:633-643` |

Here `p` is `shellRoot.powerProfile`, and `cyclePowerProfile`/`toggleDemoMode` are unchanged
(`ui-panels-control.md` §1.5). Using the split-tile metrics, the text block is 29 px tall with the
title at y=16 and the subtitle at y=33, and the text box is 62 px wide with `ElideRight`.
Parity check: `"Performance"` and `"Demo mode"` at 9 px Medium may elide in 62 px; the
reference render decides, so do not special-case it.

Notable visual consequences:
- The Power tile is **active (accent fill, white text) only in power-saver**. On main the
  Power toggle was never active.
- The Display tile shows the icon well, title and subtitle. On main the demo toggle was
  icon-only.

Removed on laptop (must **not** render or poll): the Windows VM toggle, the Viewflow target
toggle, and both **Chassis fan sliders** (grid row 2).

### 1.2 Brightness slider
Same tile as main (icon U+F00DF, value `brightnessPercent`), but **no `onAdjustmentFinished`
handler** (`L:…:648-657`). Every `valueChangeRequested(v)` calls `setBrightnessPercent(v)`,
which goes to the laptop brightness path in §2.2. Nothing extra is sent on release.

### 1.3 Positioning (`L:…:706-724`)
```
margin      = 10
panelHeight = popupCard.fullPanelHeight              // 350 on the main page
barBottom   = sourceItem ? round(sourceItem.mapToGlobal(0, sourceItem.height).y - screen.y)
                         : parentWindow.implicitHeight        // dead value, see below
card.x = sourceItem ? max(10, W - card.width - 10)
                    : max(10, round((W - card.width) / 2))
targetY = sourceItem ? barBottom + 10 : round((H - panelHeight) / 2)
card.y = max(10, min(targetY, H - panelHeight - 10))
```
- **IPC `controlPanel toggle`** (`toggleCentered`, Super+A) opens the panel **centred on the
  output** in both axes. On 1440×900 the main page is at (581, 275). The bluetooth page
  (width 384, content-dependent height) is re-centred whenever its height changes, through the
  existing `onFullPanelHeightChanged` → reposition path. Main instead right-aligns it at
  `exclusiveZone || 58` + 10.
- **Anchored open**: right-aligned as on main, but y is **clamped** so the card never extends
  below `H - 10`. If a page is taller than `H - 20`, the card pins at y = 10.
- The laptop bar starts **collapsed** (`expandedBarScreens = {}`, `L:shell.qml:19,41-50`).
  Anchors can therefore be compact-strip items on the corner window (`ui-bar` laptop delta:
  Wi-Fi glyph, control-panel icon). For those, `barBottom` is the strip's bottom,
  `topCornerOffset`, and the card sits at `topCornerOffset + 10`.
- `QUIRK_LAPTOP_CP_FALLBACK_BAR_BOTTOM`: the no-source `barBottom` reads
  `parentWindow.implicitHeight` (820 on the laptop bar window). This value is never used,
  because the no-source branch centres instead. Do not port it.

### 1.4 `ControlPanelSlider` (`L:ControlPanelSlider.qml`)
The `adjustmentFinished` signal does not exist. Release recomputes the preview, emits
`valueChangeRequested` and sets `adjusting = false`. Cancel only sets `adjusting = false`.
Rendering is unchanged. In rshell, a laptop-profile slider must not emit a finish event to the
brightness backend.

### 1.5 Status inputs while the panel is open
- `fan_available` is always `false` on the laptop (`L:scripts/control-panel-status.sh:42-43`,
  "Laptop cooling remains under Dell firmware control"). No fan UI exists, so
  `hyprv-privhelper` cooling stays inert.
- `screenRecording` is true iff a process is named **exactly** `wf-recorder`, `wl-screenrec`
  or `gpu-screen-recorder` (`pgrep -x`). Main matches a regex on the full command line.
  rshell should take this from its process/recorder service. Exact-name match is the laptop
  semantics.
- The legacy poll period is 2000 ms (main: 1500). rshell remains event-driven, so the period
  does not matter.
- `powerProfile` comes from `BarServices` through `PowerProfiles.profile`, with the same
  mapping as main (`L:BarServices.qml:71-76`). On the laptop, power-profiles-daemon is real and
  the Power tile depends on it. It needs a fake (§7).

### 1.6 Removed controls (Windows VM / Viewflow / Deskflow)
On the laptop none of the following is instantiated. They must not be built, polled or have
their helpers spawned:
- `WindowsVmController` / `WindowsVmControl`, including its 2000 ms
  `windows-vm-control.sh status` poll and the "unmap before pkexec" path.
- `ViewflowTargetControl` and `DeskflowTargetControl`.

The laptop branch still ships an older `WindowsVmControl.qml` (self-polling `PollCommand`, 2000 ms,
`active: root.visible`). It is dead code; do not port it. The bar's Viewflow Windows button
is also absent on the laptop (bar-side delta, see the bar laptop spec).

`ui-panels-control.md` §1.3 "Immediate unmap paths": on the laptop only the Wi-Fi tile path
remains.

### 1.7 Page lifetime
On the laptop, `pageLoader` has no `active: popupWindow.visible`, so the page stays
instantiated while hidden, and `SystemStatsPopup` has no `Loader` either. Neither is
visible. rshell should keep its desktop behaviour (destroy hidden surfaces and pages), as
the laptop minimal-state budget requires. Not a quirk.

### 1.8 Open/close timeline
Same as main (`ui-panels-control.md` §1.3) except that the reveal targets **350**. Main page:
map → 16 ms → 20 ms pause → reveal 0→350 over 200 ms OutCubic; opacity 0→1 from +40 ms over
140 ms OutQuad. Close: as on main.

---

## 2. QuickAdjust and brightness (including HDR)

### 2.1 Shell-side brightness state (`L:shell.qml`)
| item | laptop | main |
|---|---|---|
| `brightnessUiStepPercent` (`:156`) | **1** | 2 |
| `snapBrightnessPercent(v)` | `clamp(round(v))`, an integer 0..100 | multiple of 2 |
| `showBrightnessIncrease/Decrease` (IPC) | `previewBrightnessDelta(±1)` | ±2 |
| `showBrightnessLevel(level:int)` | identical body (signature `: void` only) | — |
| set path | `applyBrightnessPercent`, §2.2 | serialised set + finish |

QuickAdjust's own geometry, animation and auto-hide are unchanged (`ui-panels-stats.md` §1).
Its slider calls `applyBrightnessPercent` directly, on both branches. Laptop position
difference: `popupBounds.y = primaryBarWindow.exclusiveZone || 58` and
`exclusiveZone = round(25 + 33·reveal)`. With the **collapsed** laptop bar (reveal 0) the
card sits at **y = 25**; with the bar expanded it sits at y = 58, as on main. x is unchanged:
it right-aligns to `quickAdjustAnchorItem` (`rightTrayPill`), or to `W - width - 10` when
there is no anchor. While the bar animates, y follows the live `exclusiveZone`, but only when
`updatePopupPosition` runs (on map, size change or `show`). It does not track reveal frames.

### 2.2 Set and probe path (`L:shell.qml:2877-2882, 3077-3108`)
```
applyBrightnessPercent(v):
    n = snap(v); brightnessPercent = n
    hypr eval  require("hypr.controls").brightness_set(n)     // one arg, fire-and-forget
    brightnessProbeDebounce.restart()                          // 90 ms
probe (on debounce, panel open, showBrightness):
    hypr repl  return require("hypr.controls").brightness_get()
    if exit == 0 and Number(stdout) is finite: brightnessPercent = snap(parsed)   // unguarded
    then show QuickAdjust if requested, and re-run if a probe was queued meanwhile
```
Removed compared with main: `finishBrightnessPercent`, the serialised `brightnessSetProcess`,
request sequencing, `_brightnessInteractionActive` and the probe guard.
- **Rate**: each slider move sends a set request. The compositor coalesces them (latest
  `wanted` wins on its 25 ms tick), so rshell may keep at most one request in flight and
  replace the queued value. Order must be preserved, and the **final value is always sent**.
  Use Hyprland's IPC socket (`/eval`) natively. Do not spawn `hyprctl`.
- `QUIRK_LAPTOP_BRIGHTNESS_PROBE_UNGUARDED`: a probe result is applied even during a drag.
  `brightness_get` returns `wanted`, then `busy.level`, before reading hardware, so it echoes
  the in-flight level and normally causes no visible jump. Replicate it: accept every finite
  probe result.
- `brightness_get` may print `nil` (DDC not yet known). `Number("nil")` is NaN, so the result
  is ignored.

### 2.3 Compositor-side HDR mapping (context for fakes; not shell code)
Laptop `hypr/controls.lua` with `controls-profile.lua` =
`{backend="backlight", output="eDP-1", desktop=153, peak=400, auto_brightness=true,
initial_max=400, sdr_min=0, …}`. One UI level L ∈ 0..100 drives two stages:
```
hardwarePercent(L) = L                       for L ≤ 10            (L clamped to 0..70)
                   = 10 · 10^((L-10)/60)     for 10 < L ≤ 70       (100 % at 70)
raw(L)   = clamp(round(max · hardwarePercent(L) / 100), 1, max)    // intel_backlight
white(L) = round(153 + 247 · clamp((L-70)/30, 0, 1))               // nits, SDR white
           → plugin hypr_sdr_white.reference_luminance = white
           → monitor eDP-1 sdr_max_luminance = white (only when it changes), cm=hdr, 10-bit
brightness_get: wanted ?? busy.level ?? (raw < max ? rawLevel(raw)
                : round(70 + 30·clamp((white-153)/247, 0, 1)))
rawLevel(r): p = 100r/max; p ≤ 10 → round(p); else round(10 + 60·log10(p/10))
restore (startup): auto_brightness → apply SDR for level 70 and persist 70; hardware is left
                   to the ambient-light sensor
```
Main's curve differs (UI 0..10 dims SDR white, and there is a verify/retry state machine with
`brightness_finish`). The shell **does not** reimplement either curve. It only sends levels
and reads them back. The UI has **no marker at 70**: the slider is linear 0..100 and the HDR
stage is invisible.

### 2.4 Who shows QuickAdjust on the laptop
- Super+Shift+B → `brightness_set(35, true)` → the compositor runs
  `quickshell -p ~/.config/HyprV/quickshell ipc call quickAdjust showBrightnessLevel 35`
  (laptop `controls.lua:notify`; main runs `~/.local/bin/hyprv-shell ipc call …`).
- Brightness keys → `brightness_step(±d)` → same IPC.

**Cutover blocker** (for svc-shell and the cutover owner): the laptop compositor config and
`hyprland-bind.lua` call **`quickshell … ipc call`** (also for `controlPanel toggle`, Super+A).
rshell must either be the binary those configs invoke, or the laptop hypr config must be
switched to `hyprv-shell ipc call` at cutover. We must not edit `../hypr` here, so this is a
request, not a spec action.

---

## 3. Week calendar (`WeekCalendarPanel` + `CalendarBridge`)

The panel and bridge files are identical. The **lifetime** differs (`L:shell.qml:4049-4053,
4117-4146`):

| | main | laptop |
|---|---|---|
| `CalendarBridge` | created inside the panel `Loader`, once per open | one per bar window, created at **shell start**, never destroyed |
| `WeekCalendarPanel` | `Loader { active: calendarPanelPresented }` | always instantiated, `visible: calendarPanelPresented` |
| socket / subscription | connect on every open | connected and subscribed for the whole session |

Visible consequences, which parity references must reproduce:
- **First open frame**: the laptop bridge is already connected and has events loaded, so the
  "正在连接日历…" status pill (`ui-panels-calendar.md` §2.3 item 7) is **not** shown on open.
  It still shows if the dcal socket is down or `lastError` is set. Main shows the pill until
  the per-open connection completes.
- `QUIRK_LAPTOP_CALENDAR_WEEK_PERSISTS`: the week a user navigated to (prev/next) **stays**
  across close and reopen. Nothing calls `today()` on open. Main always opens on the current
  week, because a new bridge is created with `weekStart: new Date()`. On reopen the laptop
  still runs `positionNearNow()` (scroll position only).
- `QUIRK_LAPTOP_CALENDAR_STALE_WEEK`: `weekStart` is bound once to `new Date()` at shell
  start, and normalised to Monday by the first `loadWeek` after connect. After a week boundary
  passes (for example a laptop suspended over a weekend), the panel opens on the **old week**
  until the user navigates.
- Pushed `events`/`calendars` updates arrive while the panel is hidden. rshell must not render
  hidden surfaces: keep the data model current and render on open. The resulting frame is
  identical.

Size is unchanged: `min(520, contentRoot.width - 48)` × `min(500, barWindow.height - 78)` gives
520 × 500 on 1440×900. On the laptop the bar window `implicitHeight` is always
`min(H, 820) = 820` (main only uses 820 while the calendar or island is open), so the height
term is constant at 742 and the panel is 500 tall. The laptop can also open the calendar by
clicking the compact-strip clock. That handler first expands the bar if it is collapsed, then
calls `toggleCalendar`, so the calendar always opens over the expanded bar (bar-side delta).

---

## 4. StartupConfirm

`StartupConfirm.qml` is identical, so the visuals and the approval-file contract follow
`ui-panels-calendar.md` §4. Differences:
- The laptop `restore-session-layout.sh:164` launches it with
  `qs --path ~/.config/HyprV/quickshell/StartupConfirm.qml` (main:
  `~/.local/bin/hyprv-shell startup-confirm`). This is the same cutover note as §2.4: the laptop
  hypr script must point at `hyprv-shell startup-confirm`.
- The dialog renders at **DPR 2** on 1440×900 logical. The layout is the same, centred on that
  output.
- The laptop session restores fewer apps (no Discord/QQ/Telegram/WeChat). That affects the
  script, not the dialog.

---

## 5. Battery surfaces

- `batteryAvailable`/`batteryPercent`/`batteryMode` come from **UPower `DisplayDevice`**
  (`L:BarServices.qml:79-85`, event-driven):
  - present iff the device is `ready && isPresent && isLaptopBattery`;
  - percent = `percentage·100`;
  - mode: Charging/PendingCharge → `charging`; Discharging/PendingDischarge → `discharging`;
    FullyCharged → `full`; otherwise `UPower.onBattery ? "idle" : "plugged"`.
  On main they come from the `battery-info.sh` JSON.
- The detail fields `batteryStatusText`, `batteryDetailAccentColor`, `batteryPowerDetailText`,
  `batteryEstimate*` and `batterySampleWindowText` still come from `batteryInfo`, which is
  filled by `battery-info.sh` (sysfs `BAT*/{present,status,capacity,voltage_now,charge_now,
  charge_full,…}` plus the script's own power-sample window). That poll runs **only while the
  battery popup is open**, at 1000 ms (`L:shell.qml:3416-3428`). rshell can derive the same
  values from UPower plus sysfs on open, but must not sample while the popup is closed.
- The **battery popup** (`BatteryInfoPopup`, `ui-bar-popups.md` §5) is unchanged but
  becomes **reachable** on the laptop: from the bar battery module and from the compact strip's
  battery/CPU-power text. It was unreachable in desktop parity, so it needs new laptop
  references (owned by ui-bar).
- `LaptopBatteryCard.qml` is **not instantiated** anywhere on the laptop branch. Do not build
  it unless a later laptop commit wires it in. Spec for completeness: a Rectangle with radius
  12, fill `detailFill` (white α0.07 dark / 0.22 light) and a 1 px border of primaryText α0.12.
  It holds a centred Column (width − 18, spacing 3) of three centred texts:
  - `batteryText`, or `"电池不可用"` when unavailable: 13 px Bold baseFont, colour
    `batteryDetailAccentColor` when available, else primaryText;
  - `batteryStatusText`: 10 px default font, primaryText, ElideRight;
  - `batteryPowerDetailText`: 10 px default font, primaryText.
  The last two are empty when unavailable.

## 6. No visible delta (do not port the laptop variant)
- `AnimatedGlassPanel`: the laptop always enables the shadow layer. Its source rectangle has
  `opacity: surfaceOpacity`, so at `surfaceOpacity 0` (control-panel main page) the shadow is
  empty, and with a transparent `shadowColor` it is invisible. The pixels are identical, so keep
  main's cheaper behaviour.
- `SystemStatsPopup`/`SystemTrendChart`: the layout, colours and text are identical. The
  laptop drops the `Loader` and the `active` repaint gate. Keep main's lazy behaviour, which
  the minimal-state budget requires. The stats **data** cadence is svc-power's concern:
  laptop `system-snapshot.sh --minimal` every 5000 ms when not detailed, `--full` every
  1000 ms when detailed, plus CPU package power and the temperature tooltip, all bar-side.

## 7. Fakes needed for laptop panel parity and e2e
| source | consumer | fake |
|---|---|---|
| power-profiles-daemon (`net.hadess.PowerProfiles` / `org.freedesktop.UPower.PowerProfiles`) | Power tile active/subtitle, cycle | system-bus mock with settable `ActiveProfile` |
| UPower `DisplayDevice` + `OnBattery` | battery popup reachability, bar battery | system-bus mock (Percentage, State, IsPresent, Type=Battery, PowerSupply=true) |
| `battery-info.sh` sysfs | battery popup details | fake `/sys/class/power_supply/BAT0` tree via the script's dir override |
| `hypr.controls` `brightness_set`/`brightness_get` | CP brightness slider, QuickAdjust | Lua module in the nested Hyprland config implementing §2.3 over a fake backlight file (`max=400`) |
| dcal socket | calendar connected/subscribed at start | existing calendar socket mock, started **before** the shell |
| recorder process names | Screen-record tile | spawn a dummy process named exactly `wf-recorder` |

Parity references to add, at 1440×900 and DPR 2, light and dark:
- control panel main page via IPC (centred), with the Power tile in balanced and power-saver
  states;
- control panel via the compact-strip anchor;
- QuickAdjust over the collapsed bar (y = 25) and the expanded bar (y = 58);
- calendar on first open (connected, no pill) and after reopen with a persisted week;
- battery popup;
- StartupConfirm.

## 8. Ownership pointers
- Control panel tiles, sliders, the Power tile and the battery binding: svc-power (per the
  panels ownership split).
- QuickAdjust, calendar and StartupConfirm: spec-panels.
- Battery popup and compact strip: ui-bar.
- Cutover of the `quickshell … ipc call` and `qs --path` invocations in the laptop hypr config:
  cutover owner (§2.4, §4).
