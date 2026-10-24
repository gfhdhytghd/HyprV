# Laptop profile: bar delta vs main (spec)

Phase A of `docs/tasks/laptop.md`, owned by spec-bar. This file covers only what the laptop branch's bar
has or renders differently from the main working tree that `ui-bar.md`, `ui-bar-layout.md` and
`ui-bar-popups.md` were written against. Anything not listed here is identical, and the desktop
spec applies unchanged.

Line references:
- **`L:n`** = `git show origin/laptop:quickshell/shell.qml` at `24972f7`, line n. Other laptop
  files are written `origin/laptop:<path>:n`.
- **`M:n`** = the main working tree's `quickshell/shell.qml` (mtime 2026-10-20), line n. These
  are the same line numbers the desktop spec uses.

## 0. Summary of the delta

| # | area | main (desktop) | laptop | § |
|---|---|---|---|---|
| 1 | bar mode | standard only (`barExpanded` → true, `toggleBar` no-op) | **adaptive**: per-output compact/full state, which **starts compact** | 1 |
| 2 | compact strip | absent | 15 px black strip drawn in the corner window while `reveal < 1` | 2 |
| 3 | corner-window input | empty mask | top `ceil(topCornerOffset)` px take input (the strip) | 2.1 |
| 4 | system-pill slot 1 | `temperatureTrigger` (temperature text, click → btop) | `cpuPowerTrigger` (CPU package W, **no click**, hover tooltip with temperature) | 3 |
| 5 | TextModule | no tooltip | optional delayed glass tooltip (`tooltipTitle` / `tooltipText`) | 4 |
| 6 | battery source | `batteryInfo` (self-gated poll → effectively never) | UPower `displayDevice` (event-driven) | 5 |
| 7 | left pill | launcher + **Viewflow Windows button** + workspaces | launcher + workspaces (no Viewflow button) | 6 |
| 8 | workspace filter | drops `special:*` names | no `special:` name filter | 6.1 |
| 9 | `fullRightWidth` | counts temperature always | counts CPU W only when available | 7 |
| 10 | bar surface height | 58, or 820 while calendar/island open | always `min(output.h, 820)` | 8 |
| 11 | clock (`now`) | `SystemClock` Minutes | `Timer`: 1 s while `mediaUiVisible`, else re-armed at each minute boundary | 9 |
| 12 | system stats | `desktop-state.py` stream | `PollCommand system-snapshot.sh`, 1 s `--full` / 5 s `--minimal` | 9 |
| 13 | AnimatedGlassPanel | shadow layer gated on `shadowColor.a>0 && surfaceOpacity>0` | shadow layer **always** on | 10 |
| 14 | IPC `bar dataStatus` | literals | live values | 11 |
| 15 | notification right-click | `swaync-client -d -sw` | same, plus a 350 ms follow-up `refreshControlPanelStatus()` | 12 |

Checked and **identical** in both trees, so the desktop spec applies as written:
- QML files: `BarMotion.qml`, `BarScreenState.qml`, `BarServices.qml`, `BatteryInfoLine.qml`,
  `ScreenCornerShade.qml`, `GroupPill.qml`.
- Scripts: `scripts/battery-info.sh`, `scripts/system-snapshot.sh`, `scripts/ui-state.sh`,
  `scripts/cpu-power-sampler.{sh,service}`.
- Battery definitions: `batteryText`, `batteryGlyph`, `cpuPackagePowerText`,
  `batteryOrCpuPowerText`, `batteryColor`, `criticalColor`, `usageSeverityColor` (L:266-279,
  L:1897-1910, L:1936-1940 = M:276-289, M:1802-1815, M:1841-1845), and the battery popup.
- Workspace swipe limit: `workspace_swipe_forever = false` is in **both** trees
  (`hypr/hyprland-main.lua:41`, commit `854242f` is already in main). It is a Hyprland gesture
  setting and not a bar delta, so nothing needs porting.

`origin/laptop:quickshell/LaptopBatteryCard.qml` (42 lines) is **dead code**: nothing on the
branch instantiates it (`git grep LaptopBatteryCard origin/laptop` matches only the file). Do not port it.

Implementation rule, from the brief: every row above is a **runtime** profile switch
(`Profile::Laptop`), never a `#[cfg]`. Each behaviour keeps a named `QUIRK_*` const. Desktop
selects the main column, and the desktop gates must stay byte-identical.

---

## 1. Adaptive bar state (L:18, L:34-50)

```
property var expandedBarScreens: ({})                      // L:18  → every output starts COMPACT
property double lastBarToggle: 0                           // L:39
function barExpanded(name) { return expandedBarScreens[name] === true; }   // L:41
function toggleBar(name) {                                 // L:42-50
    const now = Date.now();
    if (now - lastBarToggle < 500) return;                 // one GLOBAL 500 ms debounce, not per output
    lastBarToggle = now;
    next = copy(expandedBarScreens); next[name] = !barExpanded(name);
    expandedBarScreens = next; barStateChanged();
}
anyBarExpanded       = screens.some(s => barExpanded(s.name))          // L:35
detailedStatsVisible = anyBarExpanded || systemStatsPopup.popupRequested // L:36
mediaUiVisible       = anyBarExpanded || controlPanelPopup.popupRequested// L:37
```

- `QUIRK_LAPTOP_BAR_STARTS_COMPACT`: the state is not persisted. Every start, and every new
  output (hotplug), comes up compact. An output name missing from the map counts as compact.
- `QUIRK_BAR_TOGGLE_GLOBAL_DEBOUNCE`: a toggle on output B within 500 ms of a toggle on
  output A is dropped. Wall-clock (`Date.now()`) based.
- `QUIRK_BAR_TOGGLE_IS_NOOP` (existing, `hyprv-ui-bar` `model.rs`) must be **false** on laptop.
  `hyprv-shell/src/host.rs:708` currently maps `("bar","toggle")` to `{}`. On laptop it must
  call the toggle with the monitor argument.
- **Trigger** (Hyprland side, already in the laptop hypr config; nothing for the shell to
  draw): `origin/laptop:hypr/hyprland-plugins.lua:196-208`. These are UltraEdge rules on
  target `top`, modes `push` and `move`, with `move_ms=200, movement_gap_ms=80, pause_ms=180,
  noise=0.5, band=2, cooldown_ms=500, exit_distance=20`. Their `on_enter` runs
  `quickshell ipc -p /home/wilf/.config/HyprV/quickshell call bar toggle '<monitor>'`.
  The cutover therefore needs the `quickshell ipc … call bar toggle` compatibility path to
  reach `hyprv-shell` on laptop (see §13).
- The per-output `BarScreenState` (identical file, `ui-bar.md` §7.2) now really animates:
  `reveal` targets `expanded ? 1 : 0`, over `barMotionDuration` (default 700 ms) with
  `barMotionCurve` (default `[0.1,0.9,0.1,1.05]`). The initial value is set without
  animation, so startup draws the compact frame immediately.
  - Overshoot on expand: `reveal` exceeds 1 (up to ≈1.05) and then settles to 1 from above.
  - Overshoot on collapse: it dips below 0 and settles back to 0.
  - All consumers use the unclamped value unless noted.

### 1.1 Bar window consumers of `reveal` (formulas unchanged, values now vary)

| item | formula | compact (`reveal=0`) | full (`reveal=1`) |
|---|---|---|---|
| `contentRoot.y` (L:3882) | `-T*(1-reveal) + S*reveal`, with `T = standardTravel = max(68, centerSection.height+10)` (L:3716) and `S = -T*fullscreenHide` (L:3717) | `-T` (−68) | `S` |
| `contentRoot.visible` (L:3883) | `reveal > 0` | **false**: the whole bar, island and calendar are hidden | true |
| `exclusiveZone` (L:3795) | `round(25 + 33*reveal)`; comment: "Compact: 15+10; expanded: 10+38+10" | **25** | 58 |
| layer (L:3786-3787) | `reveal < 1 ‖ fullscreen ‖ fullscreenHide ≠ 0` ? Overlay : Top | **Overlay** | Top |
| input mask (L:3804-3818) | outer rect `floor(contentRoot.x), floor(contentRoot.y), ceil(w), 58`, plus `centerSection` and `calendarDismissLayer` | rect at y = −68 → no input | as desktop |

- `QUIRK_EXCLUSIVE_ZONE_PER_FRAME`: the exclusive zone is recommitted on every animation frame
  where its rounded value changes. Hyprland re-tiles windows along with the bar. The overshoot
  can briefly commit 59–60.
- `QUIRK_COMPACT_HIDES_ISLAND`: `contentRoot.visible=false` also hides the Dynamic Island and
  the calendar panel, because both live under `contentRoot`. Compact mode never shows media,
  agent or calendar UI in the bar.

---

## 2. Compact strip (L:3578-3657, inside `cornerWindow`)

The strip is a child of the per-output corner overlay window (Overlay layer, anchored to all
four edges, `exclusiveZone -1`, namespace `hyprv-screen-corner-overlay-<name>`, as in
`ui-bar.md` §9.1).

```
Rectangle compactStrip   z 100, x 0, width = output width, height 15        L:3578-3585
    y = (barState ? barState.topCornerOffset : 15) - 15
    color #000000 (opaque), visible: cornerWindow.reveal < 1
    reveal = barState ? barState.reveal : (expanded ? 1 : 0)                   L:3550
├─ Row (left)  x 10 (anchors.left + leftMargin 10), y 0, height 15, spacing 12    L:3587-3612
│   ├─ Row  spacing 10 — Repeater over root.hyprWorkspaces                         L:3592-3606
│   │    Text  String(name || id), 11 px, baseFont, height 15, VCenter
│   │          active (id == activeWorkspaceId): #ffffff, font.bold (700)
│   │          inactive: #858585, weight 400
│   │          MouseArea fill → modelData.activate()
│   └─ Text  " " + round(cpuUsage) + "%   " + round(memoryUsage) + "%"   L:3607-3612
│            (exactly 1 space after each glyph, 2 spaces before U+F0C9)
│            10 px, white, height 15, VCenter
│            MouseArea fill → systemStatsPopup.toggleFor(thisText, cornerWindow)
├─ Text (centre) anchors.centerIn strip                                            L:3614-3622
│     Qt.formatDateTime(now, "ddd  HH:mm")   (2 spaces), 11 px, white
│     MouseArea fill → { if (!cornerWindow.expanded) toggleBar(name); toggleCalendar(screen) }
└─ Row (right) anchors.right, rightMargin 10, height 15, spacing 12, order L→R:    L:3623-3656
    ├─ Text  batteryOrCpuPowerText, 10 px, white, h 15, VCenter
    │        MouseArea → batteryInfoPopup.toggleFor(thisText, cornerWindow)   (no battery guard)
    ├─ Text  wifiConnected ? U+F1EB : U+F05AA, 11 px, white, h 15, VCenter
    │        MouseArea → controlPanelPopup.toggleFor(thisText, cornerWindow)
    ├─ Item 13×15
    │    Image 11×11 centred, source assets/bar/control-panel-dark.svg (always the dark asset)
    │    MouseArea → controlPanelPopup.toggleFor(thisItem, cornerWindow)
    └─ Text  notificationIcon + (notificationHasDot ? "·" : ""), 11 px, white, h 15, VCenter
             MouseArea → exec swaync-client -t -sw
```

Exact values and quirks:
- `notificationIcon` = DND ? U+F009B : U+F0A2 (L:521). `notificationHasDot` = `alt` contains
  `"notification"` (L:520). The dot is the literal middle-dot character appended to the
  glyph, not the full bar's separate dot item.
- Font: `baseFont` = "JetBrainsMono Nerd Font". No `renderType` is set, so every strip text is
  **QtRendering (distance field)**: DF parity tolerance (`text-rendering.md` §4/§5.3).
  `QUIRK_COMPACT_STRIP_DF_TEXT`.
- Default font weight 400 except the active workspace (bold). All text is white `#ffffff`
  except inactive workspaces; theme (light/dark) **does not** change the strip.
- Vertical placement:
  - Row texts are 15 tall with `AlignVCenter` (the glyph box is centred in 15).
  - The centre text uses `anchors.centerIn`. `alignWhenCentered` is true, so the offset is
    pixel-snapped: `hyprv_ui::geometry::qt_center_half` on both axes against its implicit size.
- `QUIRK_STRIP_WIFI_IGNORES_ETHERNET`: the glyph tracks `wifiConnected` only, so a wired-only
  connection shows U+F05AA (disconnected).
- `QUIRK_STRIP_BATTERY_CLICK_UNGUARDED`: without a battery the text shows CPU W and a click
  still opens the battery popup, with title "电源" (`batteryPopupTitle`, L:295). This differs
  from the full bar, whose battery module is interactive only with a battery.
- `QUIRK_STRIP_ICON_FROM_200PX_SVG`: the `Image` has no `sourceSize`. The SVG's intrinsic size
  is `width="200" height="200"` (viewBox 1024), so Qt rasterises it at the intrinsic size and
  scales it down to 11×11 logical (22×22 device px at scale 2) with `smooth: true` and no
  mipmaps. The fill is `#cdd6f4` (the dark asset) in both themes. Verify the source raster
  size (200 vs 200×DPR) against a Qt reference capture before claiming parity.
- `QUIRK_STRIP_CALENDAR_DEBOUNCE_GAP`: if the expand click lands within 500 ms of another
  toggle, `toggleBar` is dropped but `toggleCalendar` still runs. The calendar is then open
  but invisible (it lives under the hidden `contentRoot`) until the bar is expanded.
- The workspace click on the fallback entry (`{id:1,name:"1"}`, which has no `activate`)
  throws in QML and does nothing. The port does nothing either.
- Popups anchored from the strip use the shared bar popup positioning algorithm
  (`ui-bar-popups.md`), with the corner window as parent window and the strip item as anchor
  (anchor bottom = strip bottom = `topCornerOffset`).

### 2.1 Corner window mask and corner placement (L:3559-3566, L:3659-3701)

- `QUIRK_CORNER_MASK_FOLLOWS_STRIP`: the input mask is
  `{0, 0, width, barState ? ceil(topCornerOffset) : 15}`. Compact: the whole top 15 px of the
  output takes input, and clicks on empty strip areas are swallowed rather than passed
  through. Full: 0 (click-through, as desktop). During fullscreen it is 0.
- Corner shades are the same 27×27 opaque black (`ui-bar.md` §9). The top pair sits at
  `topMargin = topCornerOffset`:
  - compact: 15, directly under the strip;
  - full or fullscreen: 0.
  - Strip and corners move together during the animation.
- The bottom pair is unchanged (`bottomCornerOffset = 0`).
- `topCornerOffset = clamp(15*(1-reveal)*(1-fullscreenHide), 0, 15)`. Entering fullscreen
  while compact slides the strip up off-screen (y → −15) with the bar curve.
- Performance note: the desktop spec's advice to use four tiny 27×27 surfaces no longer
  holds on laptop. The strip needs a full-width × 15 px surface at the top. Laptop needs:
  - one top surface, full width × 42 px (15 strip + 27 corners), with the strip and the top
    corners moving inside it;
  - two bottom corner surfaces.
  
  Any split is fine as long as the output pixels and input region match.

---

## 3. CPU-power trigger replaces the temperature trigger (L:4198-4209 vs M:4048-4059)

First module of `rightStatusPill` (L:4194):

| property | main `temperatureTrigger` | laptop `cpuPowerTrigger` |
|---|---|---|
| label | `(t>=70 ? U+F2C7 : U+F2C9) + " " + round(t) + "°C"` | `cpuPackagePowerText` = W ≥ 100 ? `toFixed(0)` : `toFixed(1)`, + `" W"` (`"-- W"` if unavailable, but then hidden) |
| color | `t>=70 ? criticalColor : primaryText` | `usageSeverityColor(cpuPackagePowerW)`: ≥90 critical `#e92d4d`, ≥60 `usageMediumColor` (dark `#f2d36b` / light `#c99700`), else `usageLowColor` (dark `#7ad48b` / light `#2f9e44`) |
| visible | `supplementaryInfoVisible` | `cpuPackagePowerAvailable` (**not** gated on width) |
| collapseWhenHidden | true | true |
| padding L / R | 10 / 5 | **12 / 8** |
| interactive | true (left → btop in alacritty) | **false**: no click handler, `PointingHand` cursor never set (Arrow) |
| hover | none | `tooltipTitle "CPU 温度"`, `tooltipText round(temperatureC) + " °C"` (space before °), which enables hover (§4) |

Then `batteryTrigger`/`batteryModule` (L:4211-4233, code identical to M:4061-4084) follow.
With a battery present on laptop, the right status pill therefore shows **two** modules:
`[CPU W][battery]`. Without the sampler (`/run/hyprv/cpu-package-power-w` absent), it shows only
`[battery]`, and the temperature is then shown nowhere on the bar.

- `cpuPackagePowerAvailable` = the parsed `__CPU_POWER__` section is finite and ≥ 0, refreshed
  on every snapshot (L:2471-2475).
- Data source: on desktop, rshell replaces the turbostat publisher with
  `hyprv-privhelper` RAPL (`crates/hyprv-privhelper/src/rapl.rs`, two-decimal PkgWatt
  semantics). **On laptop this is a privileged action the laptop branch needs.** The cooling
  (nct6799) parts stay inert, but RAPL must be active for this pill (XPS 9350 = Intel Lunar
  Lake: `intel-rapl:0` package domain).
- `QUIRK_UNAVAILABLE_POWER_RETAINS_SEVERITY` (existing) still applies to the battery slot.

---

## 4. TextModule tooltip (L:542-735; added parts at L:546-547, L:576-649, L:680-681)

New properties `tooltipText: ""` and `tooltipTitle: ""`. Only `cpuPowerTrigger` sets them.
With `tooltipText` non-empty:
- the MouseArea becomes `enabled` and `hoverEnabled` (L:680-681);
- the cursor stays Arrow unless the module is also interactive.

**Timing** (L:576-580, L:584):
- `Timer tooltipDelay { interval 350; running: mouseArea.containsMouse }`, single-shot.
- `visible = tooltipText.length>0 && containsMouse && !tooltipDelay.running`.
- Result: the tooltip appears 350 ms after pointer enter and stays while hovered. It hides on
  the leave frame, and re-entering restarts the 350 ms. There is no fade or slide.

**Surface:** a `PopupWindow` (xdg_popup child of the bar surface) (L:582-591):

| field | value |
|---|---|
| anchor.item | the module |
| anchor.rect.x | `round((module.width - popupW) / 2)` (JS `Math.round`, half up) |
| anchor.rect.y | `module.height + 9` = **47** (module height 38) → bar-surface y = 10 + 47 = 57 |
| size | `popupW = row.implicitWidth + 20`, `popupH = row.implicitHeight + 24` |
| color | transparent |
| input | `mask: Region {}`: no input, `grabFocus: false` |

**Glass background:** the same glass as the tray dropdown (`TrayMenuPopup` tokens, L:731-732),
drawn with `AnimatedGlassPanel` at full size and no animation (`resetAnimationState()` on
completion and on height change, L:593-603):

| param | dark | light |
|---|---|---|
| fillColor | `withAlpha(#101214, 0.42)` | `withAlpha(#ffffff, 0.28)` |
| strokeColor | `withAlpha(primaryText, 0.14)` (`#cdd6f4`) | `withAlpha(primaryText, 0.10)` (`#2b2b2c`) |
| shadowColor | `withAlpha(#000000, 0.45)` | `withAlpha(#111111, 0.18)` |
| radius | 19 | 19 |
| surfaceOpacity | default 0.82 | 0.82 |
| devicePixelRatio | `module.Window.window.devicePixelRatio` | same |

**Content** (L:605-646):

```
Row  x 8, y 12, spacing 5                       (so the popup has 8 px left and 12 px right/top/bottom insets)
├─ Rectangle 36×36, radius 12, transparent, anchors.verticalCenter: Row centre
│   └─ Text "" (thermometer-half), centred, baseFont 21 px, weight default 400,
│        color: t < 70 → #2eaa45 ; t ≤ 95 → #e5b629 ; else #e5484d        (t = temperatureC, unrounded)
│        no renderType → QtRendering (distance field)
└─ Column spacing 0  (top-aligned in the Row)
    ├─ Text tooltipTitle "CPU 温度", withAlpha(primaryText, 0.65), 11 px, NativeRendering,
    │        visible only if non-empty
    └─ Text tooltipText "<n> °C", primaryText, 18 px, Font.DemiBold (600), NativeRendering
```

- `row.implicitHeight = max(36, columnHeight)`. `columnHeight` = the 11 px line height plus
  the 18 px DemiBold line height. Use qtext's line metrics, never hardcode. With JetBrainsMono
  the column (~38) is taller than 36, so the icon box is centred against it.
- The CJK glyphs in "CPU 温度" come from fontconfig fallback (the Noto CJK used elsewhere).
- `QUIRK_TOOLTIP_ICON_DF_TEXT`: the 21 px icon is distance-field and the two labels are native.
- `QUIRK_TEMP_STALE_IN_MINIMAL`: in `--minimal` snapshots `__TEMP__` is empty, so
  `temperatureC` keeps its last full-mode value (0 after a cold start in compact). The
  tooltip is reachable only in full mode, and the expand transition forces an immediate full
  sample (§9). The first hover after expanding can still show the stale value if it lands
  before that sample returns.

---

## 5. Battery data source (L:260-262, `origin/laptop:quickshell/BarServices.qml:79-85`)

```
battery        = UPower.displayDevice
batteryPresent = battery && battery.ready && battery.isPresent && battery.isLaptopBattery
batteryPercent = present ? battery.percentage * 100 : 0       // Quickshell exposes 0..1
batteryMode    = !present ? "unknown"
               : state ∈ {Charging, PendingCharge}       ? "charging"
               : state ∈ {Discharging, PendingDischarge} ? "discharging"
               : state == FullyCharged                    ? "full"
               : UPower.onBattery ? "idle" : "plugged"
root.batteryAvailable = batteryPresent
root.batteryPercent   = clamp(batteryPercent, 0, 100)
root.batteryMode      = batteryMode
batteryPlugged  = mode ∈ {charging, plugged, full}       (unchanged definition, L:265)
batteryCritical = available && percent <= 20             (unrounded percent, L:266)
```

- On laptop, `QUIRK_BATTERY_POPUP_NEVER_OPENS` (existing, `hyprv-ui-bar/src/model.rs:289`)
  must be **false** at runtime. That path already reads `power.battery_present` /
  `battery_percent`. It additionally needs the 5-way mode above: `"idle"` is new and counts
  as not plugged, so it shows the level glyph and can turn critical.
- Battery text, from the identical `batteryText` (L:266-275):
  - plugged → `" " + round(p) + "%"`;
  - otherwise → `batteryGlyph(round(p)) + " " + round(p) + "%"`;
  - glyph index = `clamp(round(round(p)/10), 0, 10)`, double rounding with JS half-up, from
    U+F008E, U+F007A … U+F0082, U+F0079.
- Battery color: critical (`#e92d4d`) when `batteryCritical && !batteryPlugged`, else
  `batteryColor` (dark `#a6e3a1` / light `#1d7715`).
- `QUIRK_BATTERY_CRITICAL_UNROUNDED`: 20.4 % displays "20%" without the critical color;
  20.0 % is critical.
- `batteryModule.interactive = batteryAvailable` (unchanged), so on laptop the battery popup
  is reachable for the first time. Popup content is as specced (`ui-bar-popups.md`, from
  `battery-info.sh` polled at 1 s **only while open**, unchanged).
- Events: UPower `PropertiesChanged` on the DisplayDevice and the `OnBattery` property. No
  polling. Fake backend for tests: a system-bus UPower mock exposing
  `/org/freedesktop/UPower/devices/DisplayDevice` with `Type=2` (Battery), `PowerSupply=true`,
  `IsPresent`, `Percentage`, `State`.

---

## 6. Left pill without the Viewflow button (L:3925-3955 vs M:3768-3801)

Laptop `leftPrimaryPill` = launcher TextModule (unchanged) + workspace strip. The
`viewflowWindowsButton` (U+F05B3, `ui-bar-layout.md` §5.1 row 2, M:3784-3801) **does not exist**
on laptop. Its 7+glyph+7 width is gone from `leftPrimaryPill.implicitWidth`, and through it
from `fullLeftWidth`. `QUIRK_LAPTOP_NO_VIEWFLOW_BUTTON`.

### 6.1 Workspace filter (L:242-251 vs M:242-252)

Main also drops names starting with `"special:"` (M:245-246, comment: "Hyprland git reports
special workspaces without a negative id"). Laptop keeps only `id > -1 && name !=
"viewflow-underlay"`.

**Decided (user, 2026-10-24): keep main's `special:` filter on both profiles.**
`QUIRK_LAPTOP_SPECIAL_WS_UNFILTERED = false`. Laptop QML on the same Hyprland build would show
`special:*` workspaces (for example `special:sidechat_hidden`) in the full bar and the compact
strip. That is pre-fix laptop code, not intended behaviour. This is a recorded, deliberate
deviation from laptop QML. Parity fixtures must not contain special workspaces.

---

## 7. `fullRightWidth` / supplementary threshold (L:3735-3748 vs M:3578-3591)

```
laptop fullRightWidth = 9.5
    + (cpuPackagePowerAvailable ? cpuPowerTrigger.natural : 0)          // natural = labelW + 12 + 8
    + batteryModule.natural - batteryModule.paddingLeft + 5             // = labelW + 12 + 5
    + 9.5
    + previousMediaButton.natural - previousMediaButton.paddingLeft + 9 // = labelW + 5 + 9
    + playPauseMediaButton.natural
    + nextMediaButton.natural - nextMediaButton.paddingRight            // = labelW + 5
    + volumeButton.natural + 9.5
    + systemLeadingSpacer + wifiTraySlot + controlPanelTrigger + notificationTrigger
    + systemTrailingSpacer
```

- The battery and media terms are algebraically equal to main's `max(minimumWidth, labelW + k)`
  forms, because their `minimumWidth` is 0 and the current-padding subtraction cancels. Only
  the first term differs: CPU W (when available) instead of the temperature module.
- The threshold formula is unchanged (L:3749-3752):
  `contentRoot.width >= (calendarPanelPresented ? 520 : 448) + 19 + 2*max(fullLeftWidth, fullRightWidth)`.
- XPS 9350 `eDP-1` is 2880×1800 at scale 2, so 1440 logical
  (`origin/laptop:hypr/hyprland-monitors.lua:4-5`). The desktop worked example gives ≈1348
  with temperature, so laptop sits **near the threshold**:
  - full mode with the calendar closed is likely;
  - opening the calendar (+72) can flip it to compact spacing (5), which also hides volume and
    changes paddings.
  - Fixtures must cover both sides at 1440 logical.
- The hidden-temperature effect of `supplementaryInfoVisible=false` no longer exists. The CPU
  W module stays visible regardless of width.

---

## 8. Bar surface height (L:3794 vs M:3637)

Laptop: `implicitHeight = min(output.h, 820)` always, a transparent 820-tall surface whose
input is restricted by the mask. Main switched to 58 unless the calendar or island is open.

**Decided (user, 2026-10-24): use main's dynamic sizing on both profiles.** Pixels and input are
identical, because outside the mask the surface is transparent and click-through, and the
minimal-state budget favours a 58 px buffer. Only `bar inputStatus`/`layoutStatus` expose
geometry, and neither reports the surface height. This is a deliberate non-parity of an
invisible property. `QUIRK_LAPTOP_TALL_BAR_SURFACE = false`, recorded.

---

## 9. Clock and stats cadence (what the visible bar needs)

**Clock** (L:3268-3274):

```
Timer { repeat: true; running: true; triggeredOnStart: true
        interval: mediaUiVisible ? 1000 : max(1, 60000 - (now.sec*1000 + now.ms))
        onTriggered: now = new Date() }
```

- Compact (`mediaUiVisible` false): this re-arms to the next minute boundary on every tick,
  giving one wake per minute.
- Full: 1 Hz, phase-locked to when the mode changed, not to second boundaries. A displayed
  `HH:mm` can lag the real minute change by up to 1 s. The island also receives `now`
  (L:4064).
- An interval change restarts the timer (Qt `QQmlTimer::update`). The first tick double-fires
  (a queued `triggeredOnStart` plus the interval-change re-arm), which is harmless and need
  not be emulated.
- Port: one minute-boundary timerfd in compact. In full mode the 1 Hz phase matters only for
  ≤1 s lag. Parity renders use the fixture clock, so a minute-aligned timer in both modes is
  acceptable, unless the island spec needs the 1 Hz `now`.
  `QUIRK_LAPTOP_CLOCK_1HZ_WHEN_MEDIA_VISIBLE` documents the difference.

**System snapshot** (L:3277-3286, L:3304-3309; script identical in both trees):

| mode | interval | sections read | feeds |
|---|---|---|---|
| `detailedStatsVisible` (full bar or stats popup) | 1000 ms `--full` | full `/proc/stat`, meminfo, CPU temperature (thermal zone `x86_pkg_temp`/`TCPU`/… then hwmon coretemp/k10temp/zenpower), `/run/hyprv/cpu-package-power-w`, route, `/proc/net/dev` | CPU %, per-core, MEM %, temp, CPU W, rates |
| minimal (all compact, no stats popup) | 5000 ms `--minimal` | first `/proc/stat` line, meminfo, power file, route; **no temp, no net** | strip CPU % / MEM %, CPU W (strip text when there is no battery), `defaultInterface` |

- On toggle (`onDetailedStatsVisibleChanged`): reset the network counters, run
  `refresh(true)` immediately, then continue on the new interval.
- The first CPU % after a cold start needs two samples, so the strip shows `0%` for the first
  5 s.
- In minimal mode the network rates are reset (no `__NET__`).

**Minimal-state budget consequence:** the compact strip **visibly shows CPU % and MEM %**, so
a 5 s `/proc/stat` + `/proc/meminfo` read is required for parity (native reads, no exec). That
makes **0.2 Hz** the floor in minimal state, on top of the minute clock tick. Two more reads
can be skipped because their results are invisible:
- the CPU-power file when a battery is present (the strip shows the battery instead);
- the route table (`defaultInterface` is not drawn in compact).

Everything else (battery, Wi-Fi, notifications, power profile, theme) is event-driven.
**Decided (user, 2026-10-24):** the 5 s CPU/MEM read (native, no exec) is the accepted
minimal-state floor, on top of the minute clock tick. It is required by the visible compact
strip.

---

## 10. AnimatedGlassPanel shadow layer (`origin/laptop:quickshell/AnimatedGlassPanel.qml:178-183`)

Main gates the shadow `layer.enabled` on `shadowColor.a > 0 && surfaceOpacity > 0` and sets
`visible: surfaceOpacity > 0`. Laptop has `layer.enabled: true` with no `visible` gate.

- Every **bar-owned** glass instance (tray menu L:1029, overflow, battery popup L:1574,
  tooltip L:594) has a non-transparent shadow and `surfaceOpacity 0.82`, so there is **no
  pixel delta for the bar**.
- It does matter for `ControlPanelPopup.qml:769` (`shadowColor transparent`, `surfaceOpacity`
  0 on the main page) and `WifiFallback.qml:376`: laptop routes them through a MultiEffect
  layer anyway (an 8-bit layer round-trip, plus an invisible-but-composited pass at
  opacity 0). → svc-power (`QUIRK_GLASS_SHADOW_LAYER_ALWAYS`).

---

## 11. IPC `bar dataStatus` (L:71-86 vs M:64-80)

| field | main | laptop |
|---|---|---|
| `expanded` | true | `anyBarExpanded` (false at start) |
| `systemInterval` | 1000 | `systemSnapshot.interval` (1000 / 5000) |
| `nativeAudio` | `audioAvailable` | `barServices.audioAvailable` (same value) |
| `nativeWifi` | true | `Networking.backend == NetworkManager` |
| `wifiScanning` | false | `wifiDevice.scannerEnabled` (true while the Wi-Fi page is open) |
| `notifications` | true | swaync watcher process running |
| `agentWatcher` | true | agent watcher process running |

Report the laptop values as live state (`notifications`/`agentWatcher` = the respective
subscription is alive). `toggle`, `layoutStatus` and `inputStatus` signatures are unchanged.
`layoutStatus.expanded`/`reveal` now vary.

---

## 12. Small behavioural deltas

- Notification module right-click (L:4499-4502): after `swaync-client -d -sw`, restart
  `notificationRefresh` (350 ms, L:4562-4567) → `refreshControlPanelStatus()`. With native
  swaync events this is redundant. Port it as a no-op, but keep the DND exec.
- The temperature trigger's btop launcher (`alacritty -t btop … -e btop`) is unreachable on
  laptop, because the module does not exist.

---

## 13. Coordination / requests generated by this spec

| owner | item |
|---|---|
| hyprv-shell (host) | `bar toggle <monitor>` must be live on laptop (`host.rs:708`), with the global 500 ms debounce from §1. The `quickshell ipc -p … call bar toggle` compat entry point must reach the shell on laptop (UltraEdge exec). |
| hyprv-privhelper / svc-power | RAPL CPU-package power is needed on laptop for the CPU W pill (§3). The fan/cooling units stay inert. |
| svc-power | `AnimatedGlassPanel` always-on shadow layer affects the control panel / Wi-Fi fallback glass on laptop (§10). Laptop `brightnessUiStepPercent = 1` (L:156, main 2) and the single `brightness_set(n)` path (L:2877-2882) concern QuickAdjust: **QuickAdjust is spec-bar's**, so it goes into a separate laptop QuickAdjust spec, not this one. |
| svc-sys | Minimal snapshot cadence and section list (§9), and the UPower DisplayDevice 5-way mode (§5). |
| island (spec-island) | Compact mode hides the island entirely (§1.1). `now` ticks at 1 Hz in full mode (§9). The laptop island has agent-approval additions (`agentPending*`, L:4075-4108), which are out of scope here. |
| parity / verify | Laptop fixture set (§14). The laptop QML must render with UPower/NM/sysfs fakes. |

## 14. Parity fixtures to add (laptop profile, light + dark, scale 2, 1440×900 logical)

1. `lap-compact-idle`: compact strip with 5 workspaces (ws 2 active), CPU 7 %, MEM 42 %,
   battery 57 % discharging, Wi-Fi connected, no notifications, corners at y = 15.
2. `lap-compact-charging`, `lap-compact-critical` (18 %, discharging), `lap-compact-idle-mode`
   (UPower state Unknown with `OnBattery=true`), `lap-compact-nobattery` (strip shows
   `"-- W"` / CPU W), `lap-compact-wired` (wifi disconnected glyph), `lap-compact-dnd-dot`.
3. `lap-expand--t0000…t0700`: reveal animation frames every 100 ms, covering the strip sliding up (it hides once `reveal` ≥ 1),
   `contentRoot` slide and the corner move, with exclusive-zone values logged.
4. `lap-collapse--t…`: the reverse, including the overshoot frame below 0.
5. `lap-full-battery+cpuw`: full bar with `[CPU W][battery]`, CPU W at 12.3 / 64.0 / 104 W
   (three severity colors).
6. `lap-full-nocpuw`: sampler absent, only `[battery]`.
7. `lap-tooltip--t0349` (not shown) and `--t0350` (shown) for temps 45 / 82 / 99 °C (three
   icon colors).
8. `lap-full-calendar-threshold`: at 1440 logical with the calendar open, which flips
   `supplementaryInfoVisible`.
9. `lap-fullscreen-compact`: strip slid off (`topCornerOffset` → 0).
