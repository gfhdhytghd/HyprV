# UI spec — Bar, shell root, surfaces (part 1 of 3)

Source of truth: `/home/wilf/.config/HyprV/quickshell` at commit `a51ae43` plus the working tree
on 2026-10-23. All citations are `file.qml:line`. Unless noted otherwise, `shell.qml` means
`quickshell/shell.qml`.

This spec has three files:

| file | content |
|---|---|
| `ui-bar.md` (this file) | process/root structure, every layer surface, per-screen instancing, IPC, global state and theme tokens, scaling, bar motion (BarMotion/BarScreenState), adaptive vs standard mode, edge actions, screen corners, hypr-comp, dead code, data contracts, parity screenshot list |
| `ui-bar-layout.md` | the bar window item tree: TextModule, GroupPill, left/centre/window-title/right sections, workspaces, clock host, system stats triggers, battery/CPU power, media buttons, volume, Wi-Fi indicator, tray layout, control-panel and notification triggers, layout math with worked example |
| `ui-bar-popups.md` | popups owned by the bar: tray menu, tray overflow grid, battery info popup (+BatteryInfoLine), AnimatedGlassPanel, AnimatedReveal, and the shared popup positioning algorithm |

Scope boundaries (other spec files):
- `ui-island.md` (spec-island): DynamicIsland internals including the **clock** (the clock is
  drawn by the island's idle row, `DynamicIsland.qml:822-850`), media/agent modes.
- `ui-panels.md` (spec-panels): ControlPanelPopup, QuickAdjustPopup, SystemStatsPopup,
  WifiFallback's popup panel, WeekCalendarPanel/CalendarBridge, and their open/close logic.
  This file documents only how the bar *triggers* them and where they are anchored.
- `services.md` (spec-sys): scripts, desktop-state.py, media-status.sh, etc.

---

## 0. Conventions used in this spec

- **px** = logical (surface-local) pixels, Qt "device independent pixels". The main output
  `DP-4` is 6144×3456 at scale 2 → **3072×1728 logical** (`hypr/hyprland-monitors.lua:21-30`).
  The renderer must render at buffer scale (2.0 here, or the fractional preferred scale) and
  never resample.
- **No Scaler.** The main shell does **not** use `Scaler.qml` (that file lives only in
  `hypr-comp/`, see §11). Every geometry value in this spec is a fixed logical-px constant;
  the only scaling is the output scale. There is no per-resolution or user UI scale.
- Colors: `#RRGGBB` opaque; `rgba(#RRGGBB, a)` = that color with alpha `a` (0..1). Qt stores
  alpha as 16-bit and composites in 8-bit → effective 8-bit alpha = `round(a*255)`.
  `withAlpha(c, a)` (`shell.qml:1670-1673`) **replaces** the alpha of `c` with `a` (does not
  multiply).
- Item `opacity` in Qt multiplies down the tree; each scene-graph node is alpha-multiplied
  individually (no group isolation) unless `layer.enabled`. E.g. a Rectangle with fill+border
  and `opacity: 0.82` draws the fill at `a_fill*0.82` and the border on top at
  `a_border*0.82`.
- Qt `Rectangle.radius` is clamped to `min(width,height)/2`. Borders are drawn **inside**
  the rectangle bounds. All Rectangles here are antialiased.
- Text: every bar Text uses `renderType: Text.NativeRendering` unless noted (FreeType glyph
  rasterisation with the system fontconfig settings: `hintstyle=hintslight`, `autohint=true`,
  grayscale AA (`rgba=none`), `lcdfilter=none` — measured with `fc-match -v`). Text with no
  explicit renderType (only TrayButton's fallback letter) uses Qt's default
  `QtRendering` (distance-field).
- Font weights: `Font.Medium`=500, `Font.DemiBold`=600, `Font.Bold`=700, default 400.
- Easing names are Qt `QEasingCurve` types; `BezierSpline [x1,y1,x2,y2,1,1]` is a single
  cubic-bezier segment from (0,0) to (1,1).
- "Behavior" = QML `Behavior on prop` (implicit animation, retargets from the current
  animated value whenever the bound target changes).

### 0.1 Font facts (measured from the installed font files)

`baseFont` = `iconFont` = **"JetBrainsMono Nerd Font"** (`shell.qml:232-233`), resolved by
fontconfig to `/usr/share/fonts/TTF/JetBrainsMonoNerdFont-{Regular,Medium,SemiBold,Bold}.ttf`
(non-"Mono", non-"Propo" variant).

- unitsPerEm 1000, hhea ascent 1020, descent −300, lineGap 0 (`USE_TYPO_METRICS` set; typo
  metrics identical). Natural line height = 1.32 em (21.12 px at 16 px).
- **Every glyph used on the bar has advance 600 units (0.6 em = 9.6 px at 16 px)**, including
  all Nerd Font icons. Icons overflow their advance to the right (e.g. U+F028 volume-up bbox
  x 0..1154 units, U+F303 Arch 0..1000). So text implicit width = `n_codepoints × 0.6 ×
  pixelSize` before hinting.
- **Verify against references:** Qt NativeRendering with hintslight on Qt 6 may round
  advances to whole device pixels. At scale 2 a 16 px glyph is 32 device px; 0.6×32 = 19.2
  device px → either 19.2 (fractional) or 19 (rounded) per glyph. Widths below assume
  fractional 9.6 px; the parity harness must confirm (see §14 "metrics" screenshots).
  <!-- audit: resolved by measurement (Qt 6.12 offscreen, this fontconfig, QT_SCALE_FACTOR 1/1.5/2 identical): advances ARE fractional, 0.6·px quantised to 1/64 (16 px → 9.59375). BUT Text.implicitWidth also adds the ink overflow of the LAST glyph, so icon-terminated labels are wider than n×0.6em: at 16 px U+F028 = 18.59, U+F466 = 16.59, U+F027 = 12.59, U+F137/U+F138 = 14.59, U+F04B = 11.59, U+F04C = 9.59; U+F303 @17 = 17.19, U+F05B3 @17 = 13.19, U+F0A2 @14 = 12.39, U+F009B @14 = 13.39. Line height = ceil(1.32·px). Full table: components.md §0.2. -->
- U+2696 (⚖) is **not** in the font (fallback font). It only appears in `powerProfileText`,
  which nothing on the bar renders.

---

## 1. Process / root structure

`quickshell -p ~/.config/HyprV/quickshell -n -d` (`scripts/launch.sh:17`), started from
`hypr/hyprland-exec-once.lua:14`. Environment: `QSG_RENDER_LOOP=basic` (`launch.sh:10`).
Before exec it runs `ui-state.sh ensure` and `fan-control-helper.sh restore`.

`shell.qml:1` `//@ pragma UseQApplication` (needed for tray menus). Root is `ShellRoot`
(`shell.qml:15`) with id `root`; all global state lives on it (§5).

### 1.1 Object tree (instantiated at startup)

| object | where | notes |
|---|---|---|
| `BarMotion` `barMotion` | `shell.qml:24` | reads Hyprland animation curve (§7) |
| `IpcHandler bar` | `shell.qml:44-81` | §4 |
| `Connections { target: Hyprland }` | `shell.qml:83-89` | raw event `fullscreen` → `Hyprland.refreshToplevels()`; `configreloaded` → `barMotion.refresh()` |
| `Component.onCompleted` | `shell.qml:389` | `syncBluetoothStatusFromModel()` |
| Processes/timers/streams | `shell.qml:2935-3352` | services; see services.md. Bar-relevant ones listed in §5.3 |
| `TrayMenuPopup trayMenuPopup` | `shell.qml:3354-3362` | registers itself as `root.trayMenuController` |
| `TrayOverflowPopup trayOverflowPopup` | `shell.qml:3364-3366` | |
| `BatteryInfoPopup batteryInfoPopup` | `shell.qml:3368-3370` | |
| `SystemStatsPopup systemStatsPopup` | `shell.qml:3372-3376` | panels spec |
| `PollCommand controlPanelStatusPoll` | `shell.qml:3378-3389` | active only while control panel open |
| `ControlPanelPopup controlPanelPopup` | `shell.qml:3391-3395` | panels spec |
| `QuickAdjustPopup quickAdjustPopup` | `shell.qml:3397-3401` | panels spec |
| `IpcHandler controlPanel/calendar/quickAdjust` | `shell.qml:3403-3454` | §4 |
| `Variants{Quickshell.screens} → BarScreenState` | `shell.qml:3458-3465` | one per output (§7.2) |
| `Variants{Quickshell.screens} → PanelWindow cornerWindow` | `shell.qml:3467-3546` | one per output (§9) |
| `Variants{Quickshell.screens} → PanelWindow barWindow` | `shell.qml:3548-4368` | one per output (ui-bar-layout.md) |
| `Timer calendarChildPoll / calendarChildLaunchTimeout / powerProfileRefresh` | `shell.qml:4370-4410` | |

**Per-screen instancing:** `Quickshell.screens` = every wl_output Quickshell sees, including
headless outputs (`HEADLESS-100` Moonlight, `HEADLESS-13` Viewflow, see
`hyprland-monitors.lua`). Every output gets its own BarScreenState, corner window and bar
window. Outputs appearing/disappearing create/destroy these instances. The **primary bar**
(`root.primaryBarWindow`) is the first bar window whose `Component.onCompleted` runs
(`shell.qml:3662-3666`); it is never reassigned if that output disappears (the property goes
to `null` only when the object is destroyed — QML var props referencing destroyed objects
become null). The Rust port should pick the first output in creation order and re-pick on
removal.

Things that are **global, not per-monitor** (identical on every bar): workspace list (all
workspaces of all monitors), focused workspace highlight, active window title, all system
stats, tray items, media, notification state.

---

## 2. Layer surfaces inventory (whole shell)

Hyprland layer rules that matter (`hypr/hyprland-winrule.lua:109-134`):
- `namespace = "hyprv-quickshell"` → `no_anim`, `blur`, `blur_popups`, `ignore_alpha 0.15`.
- `namespace = "shell:.*"` → `blur`, `blur_popups`, `ignore_alpha 0.05` (animations not
  disabled → Hyprland's default layer fade/slide applies to these popups).
- `namespace = ".*"` → `xray = false` (all layers).
- `namespace = "corner.*"` → blur. Hyprland matches rule regexes as a full match (RE2
  `FullMatch`, to be confirmed against the running Hyprland build), so
  `hyprv-screen-corner-overlay-*` most likely does **not** match; the corners are opaque
  black anyway, so blur would not change their pixels.

| surface | file:line | instances | layer | anchors | size | exclusive zone | kbd | namespace | input region |
|---|---|---|---|---|---|---|---|---|---|
| bar | `shell.qml:3551-3660` | 1 / output | `Top`, or `Overlay` while `reveal<1` or fullscreen or `fullscreenHide≠0` (`:3629-3630`) | top+left+right | width = output width; height `min(output.h, calendarPanelPresented‖islandExpanded ? 820 : 58)` (`:3637`) | `round(25 + 33*reveal)` = **58** in standard mode (`:3638`) | None (`:3643`) | `hyprv-quickshell` | see ui-bar-layout.md §2 |
| corner overlay | `shell.qml:3470-3500` | 1 / output | `Overlay` | all four edges | full output | `-1` (ignore other zones, covers whole output) | None | `hyprv-screen-corner-overlay-<output name>` | **empty** (width × 0 region) |
| tray menu | `shell.qml:842-859` | 1 (moves to the parent bar's screen) | `Overlay` | all four | full output | −1 | `OnDemand` while visible, else None | `shell:hyprv-tray-menu` | full surface (click outside = close) |
| tray overflow | `shell.qml:1204-1221` | 1 | `Overlay` | all four | full output | −1 | OnDemand/None | `shell:hyprv-tray-overflow` | full surface |
| battery info | `shell.qml:1395-1412` | 1 | `Overlay` | all four | full output | −1 | OnDemand/None | `shell:hyprv-battery-info` | full surface |
| system stats | `SystemStatsPopup.qml:133-145` | 1 | Overlay | — | — | — | — | `shell:hyprv-system-stats` | panels spec |
| control panel | `ControlPanelPopup.qml:739-751` | 1 | Overlay | — | — | — | — | `shell:hyprv-control-panel` | panels spec |
| quick adjust | `QuickAdjustPopup.qml:112-130` | 1 | Overlay | — | — | — | — | `shell:hyprv-quick-adjust` | panels spec |
| Wi-Fi panel | `WifiFallback.qml:290-305` | 1 per loaded indicator (i.e. per bar) | Overlay | — | — | — | — | `shell:hyprv-wifi` | panels spec |

All surfaces: `color: "transparent"`, non-opaque surface format (premultiplied ARGB). Popup
surfaces are created hidden (`visible: false`) and mapped on demand; their `screen` follows
`parentWindow.screen` of the bar that opened them.

Surface **not** instantiated by the shell: `StartupConfirm.qml` (namespace
`hyprv-startup-confirm`) is never referenced from shell.qml, but it is **not dead**: it is a
separate one-shot process launched at login by `hypr/scripts/restore-session-layout.sh:163-171`
(`qs --path …/StartupConfirm.qml`, env `HYPRV_STARTUP_APPROVAL`). Spec: ui-panels-calendar.md §4.
<!-- audit: was "dead code"; contradicted ui-panels-calendar.md §4 and services.md line 36 -->

---

## 3. Keyboard

No bar surface ever takes keyboard focus (`WlrKeyboardFocus.None`). The three bar-owned
popups take `OnDemand` focus while visible and handle **Escape** only (tray menu: Escape goes
back one submenu level, else closes — `shell.qml:925-931`; overflow and battery: close).

---

## 4. IPC targets (`quickshell ipc call <target> <fn> [args]`)

Callers: `hypr/hyprland-bind.lua:54` (SUPER+A → `controlPanel toggle`), `hypr/controls.lua:51`
(`quickAdjust showBrightnessLevel <n>` at `controls.lua:139`, `quickAdjust showVolume` 80 ms
after a volume change at `controls.lua:166`). `bar toggle <monitor>` was called by an
UltraEdge rule that has since been removed (§8). Also `hyprland-bind.lua:34` SUPER+Y runs
`quickshell kill -p …` and `:50` SUPER+SHIFT+R runs `scripts/reload.sh` — the port needs
`hyprv-shell kill` and a reload equivalent.

### 4.1 `bar` (`shell.qml:44-81`)

| fn | signature | behaviour |
|---|---|---|
| `toggle` | `(monitor: string): void` | calls `root.toggleBar(monitor)` which is a **no-op** (`:43`). Keep for compatibility. |
| `layoutStatus` | `(monitor: string): string` | JSON of the BarScreenState for that output name, or `null` if none: `{"expanded":bool,"fullscreen":bool,"reveal":real,"fullscreenHide":real,"compactOffset":real,"topCornerOffset":real,"bottomCornerOffset":real,"presented":bool,"duration":int,"curve":[x1,y1,x2,y2,1,1]}` (`:47-54`) |
| `inputStatus` | `(): string` | JSON for the **primary** bar or `null`: `{"monitor":name,"reveal":real,"contentY":-standardTravel*(1-reveal)+standardOffset*reveal,"inputX","inputY","inputWidth","inputHeight"}` — the last four are the bar mask's *outer* rect (`:55-63`) |
| `dataStatus` | `(): string` | JSON diagnostic snapshot (`:64-80`): `expanded` (=anyBarExpanded, always true), `systemInterval` (literal 1000), `detailedStats`, `nativeAudio` (=audioAvailable), `volume`, `nativeWifi` (literal true), `wifiConnected`, `wifiScanning` (literal false), `wifiNetworkCount`, `networkInterface` (=defaultInterface), `dnd`, `cpu`, `memory`, `networkRxRate`, `networkTxRate`, `mediaPlayer`, `mediaPosition`, `mediaAvailable`, `mediaPlaying`, `spectrum` (spectrum process running), `notifications` (true), `agentWatcher` (true), `powerProfile`, `batteryAvailable`, `batteryPercent`, `darkMode`, `panelPolling` (control-panel poll active), `batteryDetailPolling` (battery poll active) |

JSON serialisation is JS `JSON.stringify` (numbers as JS doubles, no spaces).

### 4.2 `controlPanel` (`shell.qml:3403-3410`)
`toggle()` → `controlPanelPopup.toggleCentered(root.primaryBarWindow)` (panels spec).

### 4.3 `calendar` (`shell.qml:3412-3421`)
`toggle()` → if a primary bar exists, `root.toggleCalendar(primaryBarWindow.modelData)`
(the screen object). `toggleCalendar(screen)` (`:2741-2748`): if open on the same screen →
`requestCalendarClose()`; else set `calendarScreen=screen; calendarOpen=true`.

### 4.4 `quickAdjust` (`shell.qml:3423-3454`)

| fn | behaviour |
|---|---|
| `showBrightness()` | `quickAdjustPopup.show("brightness")`; `refreshBrightnessStatus(false)` |
| `showBrightnessLevel(level: int)` | if finite: `updateBrightnessPercentLocally(level)` (snap to multiple of 2, clamp 0..100); `show("brightness")`; `refreshControlPanelStatus()`; restart `brightnessProbeDebounce` (90 ms) |
| `showBrightnessIncrease()` | `previewBrightnessDelta(+2)` |
| `showBrightnessDecrease()` | `previewBrightnessDelta(-2)` |
| `showVolume()` | `show("volume")`; `syncAudioStatus()` |

QuickAdjustPopup anchors to `root.quickAdjustAnchorItem` = the **primary bar's right tray
pill** (`shell.qml:4156-4161`) and to the primary bar's exclusive zone
(`QuickAdjustPopup.qml:154-171`).

---

## 5. Global state on `root`

### 5.1 Theme tokens (`shell.qml:212-240`)

There is **no matugen / MatugenColors** in the main shell. The palette is hard-coded
Catppuccin-Mocha-ish (dark) and a custom light palette, switched by one boolean `darkMode`.

`darkMode` source: `FileView themeState` (`shell.qml:3219-3226`) reads
`${XDG_STATE_HOME:-$HOME/.local/state}/hyprv/quickshell/ui-state.env`, watched with inotify
(`watchChanges: true`, reload on change). `darkMode = any line, trimmed, == "theme=dark"`.
Current file content: `theme=light`, `variant=v2`. Written by `scripts/ui-state.sh` /
`toggle-theme.sh` (services spec). Theme change is instant — no color animations anywhere on
the bar.

| token | dark | light | used by |
|---|---|---|---|
| `pillOpacity` | 0.8 | 0.8 | |
| `moduleBackground` | `rgba(#1e1e2e, 0.8)` | `rgba(#e7e7ec, 0.8)` | GroupPill, window title pill, calendar host |
| `primaryText` | `#cdd6f4` | `#2b2b2c` | default text |
| `mutedWorkspaceText` | `#575b6a` | `#859abf` | inactive workspace numbers |
| `activeWorkspaceText` | `#0c0d14` | `#1b1b1b` | active workspace number |
| `activeWorkspaceBackground` | `Qt.darker("#8e90cb", 1.05)` = **`#8789c1`** (computed with Qt's 16-bit HSV algorithm: rgb16 34756,35246,49686) | `#8eb6ec` | active workspace chip, Wi-Fi hover tint |
| `urgentWorkspaceText` | `#11111b` | `#11111b` | |
| `urgentWorkspaceBackground` | `#a6e3a1` | `#a6e3a1` | |
| `launchColor` | `#89b4fa` | `#407cdd` | launcher, viewflow, volume glyph |
| `batteryColor` | `#a6e3a1` | `#1d7715` | battery text |
| `microphoneColor` | `#cba6f7` | `#ad6bfd` | (unused on bar) |
| `criticalColor` | `#e92d4d` | `#e92d4d` | temp ≥ 70, battery critical, CPU power ≥ 90 W |
| `usageLowColor` | `#7ad48b` | `#2f9e44` | CPU power < 60 W |
| `usageMediumColor` | `#f2d36b` | `#c99700` | CPU power 60–89.99 W |
| `brightnessColor` | `#f3b35c` | `#d47b1f` | (panels) |
| `mediaInactiveColor` | `#6c7086` | `#808080` | (unused on bar) |
| `workspaceHoverBackground` | **`#000000`** (opaque black) | `= activeWorkspaceBackground` `#8eb6ec` | hover chip of hoverable TextModules |
| `systemChartAccent` | `#d7a26a` | `#b9782f` | (stats popup) |
| `screenCornerShadeSize` | 27 | 27 | |
| `screenCornerShadeColor` | `#000000` | `#000000` | |
| `baseFont`, `iconFont` | "JetBrainsMono Nerd Font" | same | |
| `trayMenuTextPixelSize` | 14 | | |
| `trayButtonWidth` / `Height` | 18 / 38 | | |
| `trayButtonSpacing` / `trayMinButtonSpacing` | 7 / 3 | | |
| `trayOverflowButtonWidth` | 22 | | |
| `statsHistoryLimit` | 120 | | |

Popup glass tokens (defined locally, identical in tray menu, overflow, battery popup and
Wi-Fi panel — `shell.qml:624-626, 1155-1156, 1318-1320`):

| token | dark | light |
|---|---|---|
| `glassFill` | `rgba(#101214, 0.42)` | `rgba(#ffffff, 0.28)` |
| `glassStroke` | `rgba(primaryText, 0.14)` = `rgba(#cdd6f4,0.14)` | `rgba(#2b2b2c, 0.10)` |
| `hoverFill` (tray menu) | `rgba(primaryText, 0.10)` | `rgba(primaryText, 0.12)` |
| `mutedTextColor` (battery) | `rgba(primaryText, 0.72)` | `rgba(primaryText, 0.68)` |
| popup shadow color | `rgba(#000000, 0.45)` | `rgba(#111111, 0.18)` |

### 5.2 Derived display strings/glyphs used by the bar

All glyphs are given as code points (they are Nerd Font private-use characters).

| property | definition | file:line |
|---|---|---|
| `networkIcon` | no `defaultInterface` → U+F092E (md-wifi-strength-off-outline); iface name starts `wl`/`wlan`/`wifi` → U+F05A9 (md-wifi); else U+F0200 (md-ethernet) | `:491-496` |
| `networkText` | iface set → `humanRate(rx+tx)` else `"nocon"` | `:497` |
| `humanRate(B/s)` | `<1024` → `round(v)+" B/s"` (note the space); `<1 MiB` → `(v/1024).toFixed(1)+"kB/s"`; `<1 GiB` → `(v/1048576).toFixed(1)+"MB/s"`; else `GB/s` (no space in the k/M/G forms) | `:1788-1800` |
| `volumeIcon` | `!audioAvailable` → U+F026; muted → U+F466; ≤20 → U+F026; ≤50 → U+F027; else U+F028 | `:369-384` |
| `batteryGlyph(p)` | `["U+F008E","U+F007A","U+F007B","U+F007C","U+F007D","U+F007E","U+F007F","U+F0080","U+F0081","U+F0082","U+F0079"][clamp(round(p/10),0,10)]` | `:1841-1845` |
| `batteryText` | no battery → `""`; plugged → U+F1E6 + `" "` + `round(p)` + `"%"`; else `batteryGlyph(round(p))` + `" "` + `round(p)` + `"%"` | `:276-285` |
| `cpuPackagePowerText` | available → `(W>=100 ? W.toFixed(0) : W.toFixed(1)) + " W"`; else `"-- W"` | `:286-288` |
| `batteryOrCpuPowerText` | `batteryAvailable ? batteryText : cpuPackagePowerText` | `:289` |
| `notificationIcon` | DND → U+F009B (md-bell-off) else U+F0A2 (fa-bell-o) | `:502` |
| `notificationDoNotDisturb` | `dndEnabled || notificationAlt contains "dnd"` | `:500` |
| `notificationHasDot` | `notificationAlt contains "notification"` | `:501` |
| `usageSeverityColor(v)` | non-finite → primaryText; ≥90 → critical; ≥60 → medium; else low | `:1802-1814` |
| `networkTrayGlyph()` | wired/other active → U+F0200; wifi active → `wifiSignalGlyph(strength*100)`; else `wifiTrayGlyph(radioEnabled, wifiConnected, strength)` | `:1955-1963` |
| `wifiTrayGlyph` | radio off → U+F092D; not connected → U+F092E; else `wifiSignalGlyph` | `:1945-1953` |
| `wifiSignalGlyph(%)` | <20 U+F092F, <40 U+F091F, <60 U+F0922, <80 U+F0925, else U+F0928 | `:1929-1943` |
| `networkTrayIconSource()` / `wifiTrayIconSource` / `wiredTrayIconSource` | see ui-bar-layout.md §8.2 | `:1733-1786` |

### 5.3 Bar-relevant data sources (what feeds the bar; details in services.md)

| state | source in QML | update cadence today |
|---|---|---|
| `cpuUsage`, `memoryUsage`, `temperatureC`, `cpuPackagePowerW/Available`, `defaultInterface`, `networkRx/TxRate` | `StreamCommand desktopState` running `python3 scripts/desktop-state.py`, JSON lines `{type:"system", data:<snapshot text>}` → `updateSystemStats()` (`shell.qml:3200-3217, 2314-2421`) | pushed by helper (≈1 s) |
| Wi-Fi state | same stream, `type:"wifi"` → `applyWifiStatus` | event-driven (NM) |
| agent sessions | same stream, `type:"agent"` | island spec |
| workspaces, focused workspace, active title, fullscreen | `Quickshell.Hyprland` (socket2 events) | event |
| `audioAvailable/Muted/VolumePercent` | `Pipewire.defaultAudioSink` + `PwObjectTracker` (`:3285-3312`) | event |
| media | `PollCommand mediaStatusPoll` 1000 ms while `mediaUiVisible && players>0` (`:3314-3327`) — `mediaUiVisible` is always true in standard mode, so it polls whenever an MPRIS player exists | 1 s poll (port: MPRIS D-Bus events) |
| notifications | `StreamCommand notificationStream` = `swaync-client -swb` JSON lines `{alt, tooltip, …}` (`:3278-3283, 1965-1981`) | event |
| tray items | `Quickshell.Services.SystemTray` (StatusNotifierItem watcher/host) | event |
| `darkMode` | inotify on ui-state.env | event |
| `now` | `SystemClock` precision Minutes (`:3191-3196`) | minute-aligned |
| battery | `batteryInfo` only updated by `PollCommand batteryInfoPoll` (`battery-info.sh`, 1 s) **while the battery popup is open** (`:3339-3352`) | see quirk below |
| `powerProfile` | `PowerProfiles.profile` (`:3329-3337`) | event |

**Quirk — battery on this machine:** `batteryAvailable` derives from `batteryInfo.available`
(`:262`), whose default is `false` and which is only refreshed while the battery popup is
open; the popup can only be opened when `batteryAvailable` is already true (`:4077, 4080`).
So in the current QML the battery display is **never** shown — the slot always shows CPU
package power. (`BarServices.qml` contains a UPower-based battery source but is not
instantiated, §12.) For parity on a desktop: show CPU power. For a laptop port the intended
behaviour is clearly "battery present → show battery"; implement battery from UPower and
gate it behind presence, but keep desktop output identical.

### 5.4 Global visibility flags

- `anyBarExpanded` = `screens.some(barExpanded)` = **true** (barExpanded always true) (`:36`).
- `detailedStatsVisible = anyBarExpanded || systemStatsPopup.popupRequested` → true (`:37`).
- `mediaUiVisible = anyBarExpanded || controlPanelPopup.popupRequested` → true (`:38`).
  Drives media polling and the audio-spectrum process.
- `networkWidgetVisible = networkConnected || wifiWidgetVisible` (`:498-499`), where
  `wifiWidgetVisible = wifiCapabilityDetected || wifiDevicePresent || wifiNetworks.length>0
  || isWifiInterfaceName(defaultInterface)`.

---

## 6. Scaling

None beyond the output scale (see §0). `hypr-comp/Scaler.qml` (`LayoutMath.getScale(w,h,
uiScale)` from `hypr-comp/WindowRegistry.js`, `uiScale` from
`hypr-comp/settings.json`, which does not exist) is only used by hypr-comp's launcher/moon
and is never loaded by the main shell (§11).

---

## 7. Bar motion

### 7.1 BarMotion (`BarMotion.qml`)

Purpose: make the bar's slide animation use the same duration/curve as Hyprland window moves.

- Defaults: `duration = 700` ms, `bezierCurve = [0.1, 0.9, 0.1, 1.05, 1, 1]`, `curveName =
  "myBezier"` (`BarMotion.qml:8-10`).
- At startup and on Hyprland `configreloaded` raw event (`shell.qml:87`) run `hyprctl -j
  animations` (`:35-48`; if already running, queue one re-run).
- Parse `[animations[], curves[]]` (`:17-34`): take the first of `windowsMove`, `windows`,
  `global` (in that order) whose entry has `overridden == true`. Find the curve whose `name
  == config.bezier`. Then `duration = config.enabled ? max(0, round(config.speed*100)) : 0`,
  `bezierCurve = [X0, Y0, X1, Y1, 1, 1]` from the curve's fields. Any failure → keep previous
  values.
- Current config (`hypr/hyprland-visual-daily.lua:41-42`): curve `myBezier` points
  (0.10,0.9),(0.1,1.05); `windows` speed 7 → **700 ms, BezierSpline [0.1,0.9,0.1,1.05,1,1]**
  (overshoots to ~1.05 then settles). In the battery profile
  (`hyprland-visual-battery.lua`) `animations.enabled=false`, but the per-leaf tree is still
  read; whatever `hyprctl` reports is used.
- Port: query via Hyprland IPC socket (`j/animations`) instead of exec; re-query on
  `configreloaded` socket2 event.

### 7.2 BarScreenState (`BarScreenState.qml`) — one per output

"One animation clock per output, shared by the bar and its corner window." Registered into
`root.barScreenStates[screen.name]` (`:41-42`, `shell.qml:25-35`).

| property | definition |
|---|---|
| `monitor` | `Hyprland.monitorFor(screen)` |
| `expanded` | `root.barExpanded(name)` → always true |
| `fullscreen` | monitor's active workspace `hasFullscreen` **and** some toplevel on it has `lastIpcObject.fullscreen == 2` (real fullscreen; 1 = maximised keeps the bar) (`:11-16`). Hyprland toplevel data are refreshed on every `fullscreen` raw event (`shell.qml:86`). |
| `hideForFullscreen` | = fullscreen |
| `reveal` | target `expanded ? 1 : 0` → 1. Behavior: NumberAnimation duration `barMotionDuration`, `Easing.BezierSpline` with `barMotionCurve` (`:26-32`) |
| `fullscreenHide` | target `fullscreen ? 1 : 0`, same Behavior (`:33-39`) |
| `compactOffset` | `-15 * fullscreenHide` (unused by any renderer now) |
| `topCornerOffset` | `clamp(15*(1-reveal)*(1-fullscreenHide), 0, 15)` → 0 in standard mode |
| `bottomCornerOffset` | 0 |
| `presented` | `fullscreenHide < 1` |

Note the curve overshoots (y up to ≈1.05 at mid-curve) — `fullscreenHide` momentarily
exceeds 1 / goes below 0 during the animation; values are used unclamped except where noted.
A curve/duration change while an animation runs only affects the next animation (QML reads
the Behavior's params at start).

### 7.3 Bar slide (fullscreen hide) — consumer side

In the bar window (`shell.qml:3557-3560, 3726-3727`):
- `reveal = barState ? barState.reveal : (expanded ? 1 : 0)` → 1.
- `standardTravel = max(68, centerSection.height + 10)` (68 normally; larger while the island
  or calendar is tall).
- `standardOffset = -standardTravel * fullscreenHide`.
- `contentRoot.y = -standardTravel*(1-reveal) + standardOffset*reveal` =
  **`-standardTravel * fullscreenHide`** in standard mode. `contentRoot.visible = reveal > 0`.
- So entering real fullscreen slides the entire bar content up by ≥68 px over 700 ms with
  the Hyprland curve; leaving fullscreen slides it back down. The surface stays mapped, the
  exclusive zone stays 58 (comment `:3561-3562`: remapping would retarget the window
  animation mid-flight), and the layer is raised to `Overlay` while `fullscreenHide ≠ 0` or
  `fullscreen` (so the slide is visible above the fullscreen window), returning to `Top`
  only when `fullscreenHide` is exactly 0 again.
- The input mask moves with `contentRoot` every frame (ui-bar-layout.md §2), so while
  hidden the bar accepts no input except where the slid-up rects still intersect the
  surface (none once `y ≤ -58`).

---

## 8. Adaptive vs standard bar mode, and edge actions

**Current: standard mode only.** Commit `75f91fa` ("Keep desktop bar in standard mode")
made `barExpanded(name)` return `true` and `toggleBar` a no-op (`shell.qml:41-43`) and
removed the compact strip and the UltraEdge top-edge rule. Everything the port must render
is the standard (full) bar. The reveal machinery (`reveal`, `topCornerOffset`, the
Overlay-while-`reveal<1` layer switch, `exclusiveZone = 25 + 33*reveal`) is still present
and is what the adaptive mode used; it must be kept so `bar layoutStatus` output matches, but
`reveal` is permanently 1.

**Historical adaptive mode** (commit `ffedead`, removed in `75f91fa`; documented for
completeness — not required for parity):
- `expandedBarScreens[name]` toggled per monitor by `bar toggle <monitor>`, debounced to
  once per 500 ms.
- Collapsed (`reveal=0`): bar content slid up by `standardTravel`, exclusive zone 25, and the
  corner window drew a 15 px **compact strip** across the top: black `#000000`, at
  `y = topCornerOffset - 15`; left: workspace names (11 px, active `#ffffff` bold, others
  `#858585`, spacing 10, click = activate) then `" " cpu%   " " mem%` 10 px white (click
  → stats popup); centre: `Qt.formatDateTime(now, "ddd  HH:mm")` 11 px white (click → expand
  bar + toggle calendar); right (spacing 12): battery/CPU-power text, Wi-Fi glyph, 11×11
  control-panel icon, notification glyph + "·" if dot. The corner window's input mask was the
  top `topCornerOffset` px.
- **Edge actions:** UltraEdge (Hyprland plugin) rules on target `top`, modes `push` and
  `move`, `move_ms=200, movement_gap_ms=80, pause_ms=180, noise=0.5, band=2,
  cooldown_ms=500, exit_distance=20`, `on_enter` → `quickshell ipc call bar toggle
  <monitor>`.

**Current edge actions** (`hypr/hyprland-plugins.lua:180-223`) are implemented entirely in
Hyprland (UltraEdge plugin), **not** in the shell: left edge → `hymission.stage_toggle()`,
right edge → `hypr/scripts/sidechat.sh show/hide`. The shell has no edge-hover surfaces and
needs none.

---

## 9. Screen corners — corner overlay window + ScreenCornerShade

### 9.1 Window (`shell.qml:3467-3546`)
One per output. Overlay layer, anchored to all four edges, `exclusiveZone: -1` (covers the
whole output including the bar's zone), transparent, `focusable: false`, input mask = empty
region (`x=0,y=0,width=W,height=0`) so it never receives input. Namespace
`hyprv-screen-corner-overlay-<screen.name>`. Always visible.

The comment at `:3456-3457` claims "four tiny surfaces" but the code uses **one full-output
surface** with four 27×27 children. The port should use four tiny 27×27 layer surfaces (or
subsurfaces) for performance — pixel output is identical as long as each corner is placed
at the same output coordinates and the surfaces are on the Overlay layer.

### 9.2 Placement

| child | anchors | size |
|---|---|---|
| topLeft | left=0, top = `barState ? topCornerOffset : 15` → **0** | 27×27 |
| topRight | right=0, top = same | 27×27 |
| bottomLeft | left=0, bottom = `barState ? bottomCornerOffset : 0` → 0 | 27×27 |
| bottomRight | right=0, bottom = same | 27×27 |

`shadeColor = #000000` (opaque). Corners are drawn on top of everything (Overlay), including
fullscreen windows and the bar.

### 9.3 ScreenCornerShade drawing (`ScreenCornerShade.qml`)
A `Canvas` (antialiased, 2D context, `renderStrategy: Cooperative`) repainted on
corner/color/size change. With `w=h=27`, `r = min(w,h) = 27`, for corner `topLeft` (others
are mirrored: `topRight` = translate(w,0) scale(−1,1); `bottomLeft` = translate(0,h)
scale(1,−1); `bottomRight` = translate(w,h) scale(−1,−1)):

```
moveTo(0,0); lineTo(w,0);
arc(cx=w, cy=h, r, start=1.5π, end=π, anticlockwise=true);   // from (w,0) bulging toward (0,0) to (0,h)
lineTo(0,0); closePath(); fill(shadeColor)
```

i.e. the region between the square's outer corner and a quarter circle of radius 27 centred
at the inner corner — an "inverse rounded corner" that makes the screen look like it has
27 px rounded corners. Canvas AA: Qt's QPainter raster antialiasing at the window's device
pixel ratio. Implicit size 24 is overridden by the 27×27 size.

---

## 10. Calendar & island hosting (bar window state; details in ui-bar-layout.md §5)

Per-bar properties (`shell.qml:3564-3569, 3600-3623, 3668-3719`):
- `islandExpanded` (true immediately when the island reports `expanded`; false 230 ms after
  it collapses — `islandCollapseTimer`), `islandCurrentHeight` (mirrors island height).
- `calendarVisible = root.calendarOpen && root.calendarScreen === this screen`.
- `calendarPanelPresented`, `calendarProgress` (Behavior 210 ms OutCubic),
  `calendarDismissArmed`.
- On `calendarVisible` → true: stop leave/close timers, disarm, restart `calendarDismissArmTimer`
  (260 ms → `armed = calendarVisible`), `calendarPanelPresented = true`, next event-loop tick
  `calendarProgress = 1`.
- On → false: stop timers, disarm, `calendarProgress = 0`, and if presented restart
  `calendarPanelCloseTimer` (210 ms → if visible again set progress 1, else progress 0 and
  `calendarPanelPresented = false`).
- `calendarLeaveTimer` 220 ms: closes the calendar if still visible, pointer not inside the
  panel, and no DankCalendar child window opening/active.
- `onCalendarChildReleased` → restart leave timer if pointer outside.
- Root timers: `calendarChildPoll` 180 ms repeating while calendar open or child
  opening/active, checks for a toplevel with appId `com.danklinux.dankcalendar`
  (`shell.qml:2730-2739, 4370-4389`); `calendarChildLaunchTimeout` 2600 ms
  (`:4391-4402`). Port: use foreign-toplevel events instead of polling.
- Surface height grows to 820 while `calendarPanelPresented || islandExpanded`.

---

## 11. hypr-comp/ — is it loaded?

**No.** `quickshell/hypr-comp/` is an isolated port of `ilyamiro/hypr-comp` with its own
`shell.qml` (IPC target `hyprComp`, functions `openLauncher/closeLauncher/toggleLauncher`;
namespace `hypr-comp-launcher`; loads `Stars.qml`, `moon.qml`, `applauncher/applauncher.qml`),
`Scaler.qml`, `MatugenColors.qml` (reads `/tmp/qs_colors.json`, Catppuccin Mocha defaults),
`Caching.qml`, scripts. Evidence:
- The main config is started with `quickshell -p ~/.config/HyprV/quickshell`
  (`scripts/launch.sh:17`), which loads only `quickshell/shell.qml`; QML does not import
  subdirectories implicitly, and nothing in the main shell imports `"hypr-comp"` or references
  `Scaler`/`MatugenColors` (grep).
- hypr-comp is started only by its own `hypr-comp/scripts/launch.sh`; nothing in `hypr/*.lua`
  references it (grep for `hypr-comp`/`qs_manager` is empty).
- Running processes on 2026-10-23: only `quickshell -p …/quickshell -n -d`.
- `hypr-comp/README-HyprV.md`: "It does not modify Hyprland exec-once or replace the existing
  HyprV Quickshell config."

→ Out of scope for the Rust port. Theme tokens come only from §5.1.

---

## 12. Dead / unused code (do not port, but be aware)

| item | status |
|---|---|
| `BarServices.qml` | Not instantiated anywhere. Contains native Networking (NM) Wi-Fi snapshot, Pipewire audio, PowerProfiles, UPower battery (`batteryPresent`, `batteryMode`: charging/discharging/full/idle/plugged), MPRIS player selection (playing > paused-with-content > with-content > paused > first) with Cider metadata repair, and a second theme FileView. Useful as the *intended* event-driven design for the services port, but today the equivalents in shell.qml are what run. |
| `StartupConfirm.qml` | not instantiated by shell.qml, but launched as its own process at login (ui-panels-calendar.md §4) — must be ported <!-- audit --> |
| `AnimatedReveal.qml` | only used by `ExpandableSection.qml` (panels); documented in ui-bar-popups.md §6 |
| `WifiNative.qml` | 4-line alias: `WifiFallback {}` — this *is* used (loaded by the bar) |
| `root.barStateChanged` signal, `openWifiPanelCount`, `powerProfileText`, `compactOffset`, `bottomCornerOffset` | no bar renderer reads them |
| `shell.qml.before-*` | backups, not loaded |

---

## 13. Data contracts (typed inputs per component)

```rust
// Global theme
struct Theme { dark: bool }               // from ui-state.env "theme=dark"

// Per output
struct OutputInfo { name: String, logical_w: f64, logical_h: f64, scale: f64 }

struct BarScreenState {                    // BarScreenState.qml
    expanded: bool,                        // always true
    fullscreen: bool,                      // active ws has a toplevel with fullscreen==2
    reveal: AnimatedF64,                   // target 1.0
    full_screen_hide: AnimatedF64,         // target fullscreen as f64, bezier from BarMotion
}
struct BarMotion { duration_ms: u32, curve: [f64; 4] }   // default 700, [0.1,0.9,0.1,1.05]

struct Workspace { id: i64, name: String, urgent: bool }  // filtered + sorted, fallback [{1,"1"}]
struct HyprState {
    workspaces: Vec<Workspace>, focused_workspace_id: i64 /*default 1*/,
    active_window_title: String,
}
struct SystemStats {                       // desktop-state.py "system"
    cpu_percent: f64, mem_percent: f64, temperature_c: f64,
    cpu_package_power_w: Option<f64>,
    default_interface: String,             // "" = offline
    rx_bps: f64, tx_bps: f64,
}
struct AudioState { available: bool, muted: bool, volume_percent: i32 }
struct MediaState { available: bool, playing: bool, player: String, /* + island fields */ }
struct NotificationState { alt: String, tooltip: String, dnd: bool }  // swaync -swb
struct WifiState {
    device_present: bool, radio_enabled: bool, hardware_enabled: bool, connected: bool,
    signal: f64 /*0..1*/, ssid: String, secure: bool, networks_len: usize,
    capability_detected: bool,
}
struct TrayItem { id: String, title: String, tooltip_title: String, tooltip_description: String,
                  icon: IconRef, has_menu: bool, only_menu: bool }
struct BatteryInfo { available: bool, mode: String /*charging|discharging|full|plugged|unknown*/,
                     capacity: f64, status: String, power_w: f64, average_power_w: f64,
                     sample_window_seconds: f64, estimate_seconds: Option<f64>,
                     estimate_basis: String /*current|average|none*/ }
struct CalendarHostState { open: bool, screen: Option<String> }
struct IslandGeometry { width: f64, height: f64, attached_center_offset: f64, expanded: bool }
```

Per-component inputs are listed again at the end of each section of `ui-bar-layout.md` and
`ui-bar-popups.md`.

---

## 14. Visual states worth a reference screenshot (parity harness)

Capture with `grim -o DP-4 -g "<x>,<y> <w>x<h>"` at scale 2 (buffer pixels). Bar strip =
top 58 px (116 buffer px) of the output; popups as noted. Each in **light and dark** unless
stated.

Bar strip:
1. **baseline**: idle, 5 workspaces with ws 1 focused, a window title, no media player,
   tray with ~6 items, Wi-Fi/ethernet connected, no notifications.
2. **metrics**: same as 1 but force known strings (e.g. CPU 7%, MEM 42%, net `0 B/s`,
   temp 45°C, `-- W`) to verify advance rounding (0.6 em fractional vs hinted).
3. workspace **hover** on an inactive workspace (dark mode shows opaque black chip).
4. **urgent** workspace (green chip `#a6e3a1`) while another is focused.
5. workspace names longer than 1 char (e.g. a named workspace) → chip wider than 32.
6. long window title → title pill elided, filling available width up to the island.
7. no active window → title pill hidden.
8. temperature ≥ 70 °C (critical colour + U+F2C7 glyph).
9. CPU power in each band: < 60 W (low), 60–89 W (medium), ≥ 90 W (critical), ≥100 W
   (integer format), unavailable (`-- W`).
10. media playing (pause glyph U+F04C) vs paused (play glyph U+F04B).
11. volume glyphs: muted U+F466, ≤20 U+F026, ≤50 U+F027, >50 U+F028.
12. notification states: bell, bell + red dot, DND bell-off.
13. network: ethernet (wired icon), Wi-Fi at each signal bucket (0/25/50/75/100,
    secure/insecure), Wi-Fi disconnected, radio off, offline (`nocon`, U+F092E).
    Light mode uses 24 px Fluent panel SVGs, dark mode 16 px symbolic SVGs (offset −2).
14. Wi-Fi indicator hover (tinted 30×38 chip, radius 10).
15. tray: icon fallback letter (item with no loadable icon), tray overflow (`⋯` shown) on a
    narrow output, tray squeezed spacing (3–7 px).
16. narrow output where `supplementaryInfoVisible` is false (temperature and volume hidden,
    paddings change) — e.g. a 1280-px-wide headless output.
17. viewflow button hover.
18. **fullscreen slide**: frames at t = 0, 100, 200, 350, 700 ms after a window goes
    fullscreen (bar sliding up; layer = Overlay), and the reverse.
19. calendar presented (centre host becomes a 520×500 panel with border) — host frame
    only; panel content belongs to ui-panels.
20. island expanded (bar surface 820 px tall) — host behaviour only.

Corners:
21. each of the four 27×27 corners at buffer scale (AA quality of the arc).

Popups:
22. tray menu: root level, with hover row, with checkbox/radio indicators, with icons,
    disabled row, separator, submenu with "< Back" row; open animation frames at 0, 20, 60,
    120, 200 ms; close at 0, 45, 90, 150, 180 ms.
23. tray overflow grid with 1, 6, 7 items.
24. battery popup (only reachable on a battery machine): charging / discharging critical /
    full.
