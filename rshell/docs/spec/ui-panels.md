# UI spec — panels & popups (index + shared primitives)

Source of truth: `/home/wilf/.config/HyprV/quickshell` (QML, read-only). All citations are
`file.qml:line` relative to that directory. Written by agent `spec-panels`.

The spec is split into these files:

| file | content |
|---|---|
| `ui-panels.md` (this file) | conventions, theme tokens resolved to literal values, font metrics, the shared primitives (`AnimatedGlassPanel`, `AnimatedReveal`, `ExpandableSection`, `WifiActionChip`, `WifiSecurityBadge`, `PollCommand`), the popup layer-surface table, the shell.qml hosting/IPC, the screenshot list |
| `ui-panels-control.md` | `ControlPanelPopup` and every `ControlPanel*.qml` (`SplitTile`, `Toggle`, `Slider`, `MediaCard`, `BluetoothDetails`, `WifiDetails`), `WindowsVmControl/Controller`, `ViewflowTargetControl`, `DeskflowTargetControl`, `MihomoControlPanel/Tile` |
| `ui-panels-wifi.md` | `WifiFallback` (the network popup; `WifiNative.qml` is an alias of it) |
| `ui-panels-stats.md` | `QuickAdjustPopup`, `SystemStatsPopup`, `SystemTrendChart` |
| `ui-panels-calendar.md` | `WeekCalendarPanel`, the calendar hosting in the bar window, `CalendarBridge`, `CalendarSocket`, `StartupConfirm` |

---

## 1. Conventions

### 1.1 Units and scaling
- There is **no `Scaler.qml` in the active shell** (the only `Scaler.qml` is in the unrelated
  `hypr-comp/` tree). Every number in QML is a **logical pixel**. Qt maps logical → buffer
  pixels with the output scale (fractional-scale-v1). The Rust port must lay out in logical px
  and render at `logical × scale` (see ARCHITECTURE "render at buffer scale").
- Numbers in this spec are logical px unless stated. "px" font sizes are `font.pixelSize`.
- Where QML uses non-integer geometry (e.g. `dayWidth = 450/7`), Qt keeps the real value for
  layout and rasterises with antialiasing; do not round unless the QML calls `Math.round`.

### 1.2 Colour helpers (exact semantics required)
- `withAlpha(c, a)` (`shell.qml:1670`): take RGB of `c`, **replace** alpha with `a`.
- `panelColor(c)` (`WifiFallback.qml:83`, `ControlPanelBluetoothDetails.qml:122`,
  `SystemStatsPopup.qml:60`): RGB of `c`, alpha `= c.a × 0.82`.
- `Qt.lighter(c, f)`: QColor::lighter(int(f·100)): convert to HSV (Qt 16-bit precision),
  `v = v·f`; if `v > 255` then `s = max(0, s − (v − 255))`, `v = 255`; convert back; alpha kept.
  **Consequence:** `lighter()` of any pure-white-based colour (`#ffffff` with alpha) is a no-op —
  so most "hover lighter" effects on the white-based card fills are invisible.
- `Qt.darker(c, f)`: QColor::darker(int(f·100)): HSV, `v = v / f` (integer), back to RGB, alpha kept.
  E.g. `darker(#ffffff, 1.08)` = `#ececec`, `darker(#ffffff,1.06)` = `#f0f0f0`.
- Colour strings `#AARRGGBB` (8 hex digits) are **ARGB** in Qt (used only in StartupConfirm).
- `Rectangle.opacity` multiplies the whole subtree (fill, border, children) — it is *not* the same
  as changing fill alpha. `Rectangle` borders are drawn **inside** the rect bounds.
- `Rectangle { clip: true; radius: r }` clips children to the **bounding rectangle**, not the
  rounded shape.

### 1.3 Theme tokens (`shell.qml:212-240`) — literal values
<!-- audit: line citations corrected (were off by ~11 lines); darkMode source aligned with ui-bar.md §5.1 / ui-island.md §0.7 -->
`darkMode` is a bool on the shell root (`shell.qml:91`, default `false`), set by the
`FileView themeState` (`shell.qml:3219-3226`): true iff any trimmed line of
`${XDG_STATE_HOME:-~/.local/state}/hyprv/quickshell/ui-state.env` equals `theme=dark`
(inotify-watched). The control panel's theme toggle also flips it optimistically before
`toggle-theme.sh` rewrites that file (`ui-panels-control.md` §1.5). D = dark, L = light.

| token | D | L | source |
|---|---|---|---|
| `primaryText` | `#cdd6f4` | `#2b2b2c` | `shell.qml:214` |
| `launchColor` (accent) | `#89b4fa` | `#407cdd` | `:220` |
| `criticalColor` | `#e92d4d` | `#e92d4d` | `:223` |
| `usageLowColor` | `#7ad48b` | `#2f9e44` | `:224` |
| `usageMediumColor` | `#f2d36b` | `#c99700` | `:225` |
| `batteryColor` | `#a6e3a1` | `#1d7715` | `:221` |
| `brightnessColor` | `#f3b35c` | `#d47b1f` | `:226` |
| `microphoneColor` | `#cba6f7` | `#ad6bfd` | `:222` |
| `systemChartAccent` | `#d7a26a` | `#b9782f` | `:229` |
| `moduleBackground` | `#1e1e2e` α0.80 | `#e7e7ec` α0.80 | `:213` (`pillOpacity 0.8`, `:212`) |
| `activeWorkspaceBackground` | `darker(#8e90cb,1.05)` = `#8789c1` | `#8eb6ec` | `:217` |
| `baseFont` / `iconFont` | `"JetBrainsMono Nerd Font"` | same | `:232-233` |

`usageSeverityColor(v)` (`shell.qml:1802`): non-finite → `primaryText`; `v ≥ 90` → critical;
`v ≥ 60` → usageMedium; else usageLow.

Derived panel tokens used across all panels (each component redefines them identically; values
below are pre-`panelColor`):

| name used in QML | D | L |
|---|---|---|
| `glassFill` | `#101214` α0.42 | `#ffffff` α0.28 |
| `glassStroke` / `panelStroke` | primaryText α0.14 | primaryText α0.10 |
| `cardFill` (= `baseFill`, `inactiveFill`, `restingFill`, `buttonFill`, `detailFill`) | `#ffffff` α0.07 | `#ffffff` α0.22 |
| `cardStrongFill` (= media card bg) | `#ffffff` α0.11 | `#ffffff` α0.30 |
| `cardStroke` (= `baseStroke`, `inactiveStroke`, `frameStroke`, `detailStroke`) | primaryText α0.12 | primaryText α0.08 |
| `accentFill` (connected rows) | primaryText α0.10 | primaryText α0.08 |
| `accentStroke` | primaryText α0.18 | primaryText α0.14 |
| `mutedText` | primaryText α0.68 | primaryText α0.68 |
| `softText` | primaryText α0.44 | primaryText α0.44 |
| `activeFill` (toggle/tile/slider progress) | accent α0.28 | accent α0.22 |
| `activeStroke` | accent α0.42 | accent α0.28 |
| `iconWellFill` active / inactive | `#fff` α0.14 / α0.11 | `#fff` α0.26 / α0.32 |
| `inputFill` | `#000000` α0.20 | `#ffffff` α0.34 |

### 1.4 Fonts and text metrics
- Family `JetBrainsMono Nerd Font` → `/usr/share/fonts/TTF/JetBrainsMonoNerdFont-{Regular,Medium,Bold,…}.ttf`.
  Metrics: unitsPerEm 1000, ascender 1020, descender −300, lineGap 0 (hhea = typo = win,
  USE_TYPO_METRICS set). Every glyph's *advance* is 600 units.
  <!-- audit: replaced estimated metrics with values measured in Qt 6.12 (same Qt as quickshell 0.3.2), offscreen QPA, this machine's fontconfig (hintslight, autohint, grayscale), QT_SCALE_FACTOR 1, 1.5 and 2 all identical. Canonical table: components.md §0.2 -->
- **Measured Qt text metrics** (identical for NativeRendering and QtRendering, and independent
  of output scale):
  - advance per glyph = `0.6·px` quantised to 1/64 px (16 px → 9.59375, 13 px → 7.796875,
    11 px → 6.59375). Advances are **fractional** (design metrics under hintslight), not rounded
    to whole pixels.
  - `implicitWidth` = Σ advances **plus the ink overflow of the last glyph** past its advance.
    Most Nerd icons overflow: a lone U+F028 at 16 px measures 18.59 (not 9.6), U+F023 12.59,
    U+F011 13.59, U+F0928 15.59; but `md` media glyphs (U+F04AE, U+F03E4) don't overflow (9.59).
    A glyph in the middle of a string contributes only its advance.
  - CJK fallback glyphs (HarmonyOS Sans SC) are 1 em wide ("系统资源" at 15 px = 60).
  - `implicitHeight` (one line) = `ceil(1.32·px)`: 8→11, 9→12, 10→14, 11→15, 12→16, 13→18,
    14→19, 15→20, 16→22, 17→23, 18→24, 20→27, 24→32, 26→35, 31→41.
- CJK fallback for this family resolves (fontconfig sort order) to `鸿蒙黑体` = HarmonyOS Sans SC.
  Line height stays that of the primary font.
- `font.weight`: `Font.Medium` = 500, `Font.DemiBold` = 600, `Font.Bold` = 700, unset = 400.
  `font.bold: true` = 700.
- `renderType: Text.NativeRendering` is used on almost every panel text (FreeType hinted glyphs,
  no distance-field). Texts **without** a `renderType` (all of `WeekCalendarPanel`, some
  `MihomoControlPanel` texts, `StartupConfirm`) use the Qt default `Text.QtRendering`
  (distance-field glyphs) — slightly softer/heavier; reproduce with unhinted, grayscale AA.
- Texts with no `font.family` (Mihomo "ON/OFF", mode buttons, refresh/dashboard, "‹ Back") use
  the application default font (QGuiApplication default = fontconfig `sans-serif` → `Inter` on
  this machine) at the given pixel size.
- `elide: Text.ElideRight` appends `…` (U+2026) when the text exceeds `width`.
- Icon glyphs are Nerd Font private-use codepoints; this spec always gives them as `U+XXXXX`.

### 1.5 Animation vocabulary
Qt easing names map to QEasingCurve (port exactly in `hyprv-ui`): `OutCubic`, `InCubic`,
`OutQuad`, `InQuad`, `InOutQuad`, `Linear` (default when no easing is given; `ColorAnimation`
default too). A `Behavior on p` animates **from the current animated value** to the new target
whenever the bound value changes (retargets mid-flight). `NumberAnimation { to: X }` inside a
`SequentialAnimation` animates from the property's value at the moment that step starts.
`PauseAnimation` = wait. `Timer { interval: 0 }` ≈ "next event-loop turn"; `interval: 16` ≈
"next frame".

### 1.6 Pointer conventions
- `cursorShape: Qt.PointingHandCursor` → `wp_cursor_shape` "pointer"; `ArrowCursor` → "default".
- `MouseArea { enabled: false }` neither reacts nor changes the cursor nor blocks events
  (events fall through to items below).
- A `MouseArea.onClicked` fires on release inside the area after a press inside it.
- `hoverEnabled: true` → `containsMouse` true while the pointer is over it.

---

## 2. Shared primitives

### 2.1 `AnimatedGlassPanel` (`AnimatedGlassPanel.qml`)
A clipped, height-revealing card used as the chrome of ControlPanel, Wi-Fi and SystemStats popups.

Properties (defaults): `revealHeight 0`, `contentOpacity 0`, `contentOffset = openContentOffset`,
`fullPanelHeight 0`, `lineHeight 2`, `radius 19`, `surfaceOpacity 0.82`, `fillColor #202020`,
`strokeColor #404040`, `shadowColor transparent`, `devicePixelRatio 1`; open timing
`openRevealPause 0, openRevealDuration 200, openContentDelay 20, openFadeDuration 140,
openSlideDuration 180, openContentOffset −8`; close timing `closeRevealPause 0,
closeRevealDuration 180, closeFadeDuration 90, closeSlideDuration 150, closeContentOffset −8`
(`:13-37`). `height = implicitHeight = fullPanelHeight` (`:42-43`).

Item tree (`:158-219`):
```
root (w = given, h = fullPanelHeight)
└─ revealClip Item: x0 y0 w=root.w h=revealHeight, clip:true           (:158-163)
   └─ panelFrame Rectangle: w=root.w h=fullPanelHeight radius 19, transparent, clip:true (:165-176)
      ├─ shadowLayer Item fill, visible iff surfaceOpacity>0             (:178-182)
      │   layer.enabled iff shadowColor.a>0 && surfaceOpacity>0, MultiEffect:
      │   shadowEnabled, shadowBlur = 10/64 = 0.15625 (MultiEffect.blurMax default 32 →
      │   ≈5 px blur radius), shadowScale 1, offsets 0,0, shadowOpacity 1, shadowColor,
      │   autoPadding (padding is then clipped away by panelFrame)       (:183-198)
      │   └─ Rectangle fill, radius 19, opacity = surfaceOpacity, color fillColor,
      │      border 1 px strokeColor                                      (:200-208)
      └─ panelContent Item: anchors.fill, opacity = contentOpacity, y: contentOffset (:211-217)
```
**Important quirk:** `panelContent` has `anchors.fill: parent` *and* `y: root.contentOffset`.
Anchors win, so **the content-offset slide is a no-op in this component**; only reveal-height
clipping and the opacity fade are visible. (It *does* work in `AnimatedReveal`, §2.2.)
Shadow rendering when enabled (only SystemStatsPopup): a blurred (≈5 px) copy of the
fill/border alpha mask, tinted `shadowColor`, drawn **under** the translucent fill and clipped
to the panel's bounding rectangle — it darkens the panel interior slightly and shows as a faint
halo only in the four corner cut-outs. Effective fill on screen = `fillColor` with alpha
`fillColor.a × surfaceOpacity`; border likewise × surfaceOpacity.

Functions (`:45-71`):
- `resetAnimationState()`: reveal = full, opacity 1, offset 0.
- `prepareOpenAnimation()`: stop all; reveal 0, opacity 0, offset = openContentOffset.
- `playOpenAnimation()` / `playCloseAnimation()`: stop all, (re)start the given animation.

Open animation (`:73-120`), `SequentialAnimation`:
1. Pause `openRevealPause`.
2. Parallel:
   - `revealHeight → fullPanelHeight`, `openRevealDuration`, **OutCubic**;
   - Sequential: pause `openContentDelay`; then parallel `contentOpacity → 1`
     (`openFadeDuration`, **OutQuad**) and `contentOffset → 0` (`openSlideDuration`, OutCubic).
3. `onFinished`: snap reveal = full, opacity 1, offset 0; emit `openAnimationFinished`.

Close animation (`:122-156`), `ParallelAnimation`:
- `contentOpacity → 0`, `closeFadeDuration`, **InQuad**;
- `contentOffset → closeContentOffset`, `closeSlideDuration`, InCubic;
- Sequential: pause `closeRevealPause`; `revealHeight → lineHeight (2 px)`,
  `closeRevealDuration`, **InCubic**.
- `onFinished` → emit `closeAnimationFinished` (the panel stays at a 2 px sliver until the
  owner unmaps the window).

### 2.2 `AnimatedReveal` (`AnimatedReveal.qml`)
Same timing model as 2.1 but without chrome. Defaults identical to AnimatedGlassPanel
(`:12-32`) plus `fullHeight 0`, `layoutHeightFollowsReveal false`.
`implicitHeight = layoutHeightFollowsReveal ? revealHeight : fullHeight; height = implicitHeight`
(`:37-38`). Tree: `viewport` Item (x0, top, w = root.w, h = revealHeight, clip) → `contentItem`
(x 0, **y = contentOffset** (works here), w = viewport.w, h = fullHeight, opacity = contentOpacity)
(`:75-93`). Extra function `collapseImmediately()`: stop; reveal 0; opacity 0; offset =
openContentOffset (`:46-51`). Animations identical to §2.1 (`:95-178`).

### 2.3 `ExpandableSection` (`ExpandableSection.qml`)
`AnimatedReveal` with `fullHeight: contentItem.childrenRect.height`, `layoutHeightFollowsReveal:
true`, `lineHeight: 0` (`:3-10`), plus `expanded` bool:
- on creation: expanded → `resetAnimationState()`, else `collapseImmediately()` (`:12-18`).
- `expanded` → true: if `fullHeight ≤ 0` wait (16 ms timer retry) else `prepareOpenAnimation();
  playOpenAnimation()`. → false: `playCloseAnimation()` (to 0 height) (`:20-35`, `:47-65`).
- `fullHeight` change while expanded and idle → `resetAnimationState()` (`:37-45`).
Because the layout height follows `revealHeight`, parents (Columns) grow/shrink every frame.
Qt positioners **skip items whose width or height is 0** — so a collapsed section consumes no
spacing; the parent Column's spacing appears the first frame the reveal height becomes > 0.

### 2.4 `WifiActionChip` (`WifiActionChip.qml`)
Rounded button/pill used by Wi-Fi and Bluetooth pages.
- Props: `label ""`, `iconLabel ""`, `disabled false`, `fillColor = primaryText α0.08`,
  `foregroundColor = primaryText`, `strokeColor = primaryText α0.10`, `minimumWidth 0`,
  `iconPixelSize 16`, `cornerRadius 10` (`:6-15`). Signal `clicked()`.
- Geometry: `height 34` (explicit; **implicitHeight is 0**, which matters in `Math.max(...)`
  layouts), `implicitWidth = max(minimumWidth, max(label.contentWidth, icon.contentWidth) + 24)`
  (`:20-21`), `radius = cornerRadius`, border 1 px, antialiased.
- Fill: disabled → primaryText α0.06; pressed → `darker(fill, 1.08)`; hovered →
  `lighter(fill, 1.06)`; else `fillColor` (`:22-26`).
- Border colour: disabled → primaryText α0.06; hovered → `foregroundColor` α0.22; else
  `strokeColor` (`:29-33`). `opacity` = disabled ? 0.56 : 1 (`:27`).
- Content: if `iconLabel` empty → label Text centred: 13 px Bold, baseFont, colour
  `foregroundColor`, NativeRendering (`:36-47`); else icon Text centred: `iconPixelSize` (16) px
  Bold iconFont, `foregroundColor` (`:49-60`).
- MouseArea fill, enabled iff !disabled, hover, pointer cursor iff enabled, click → `clicked()`.
- No animations.

### 2.5 `WifiSecurityBadge` (`WifiSecurityBadge.qml`)
Visible iff `active`. `implicitWidth 22`, `implicitHeight 20`, radius `cornerRadius` (callers
pass 9), fill primaryText α0.08 (D) / α0.06 (L), border 1 px primaryText α0.08. Centred glyph
U+F023 (lock), 11 px Bold iconFont, colour primaryText α0.82.

### 2.6 `PollCommand` (`PollCommand.qml`)
Runs a command on a repeating timer while `active` (`triggeredOnStart: true`, so immediately on
activation, then every `interval`); `refresh()` runs now unless already running. Emits
`updated(stdoutTrimmed, exitCode)`. **The Rust port must replace every PollCommand with event
sources** (ARCHITECTURE "zero wakeups when idle"); the intervals below are listed only to define
the freshness the UI expects *while the panel is open*.

---

## 3. Popup surfaces (layer-shell)

All popups are separate `PanelWindow`s that cover the whole output and draw a card inside it.

| component | namespace | layer | anchors | exclusiveZone | keyboard | focusable | input region | screen |
|---|---|---|---|---|---|---|---|---|
| ControlPanelPopup (`ControlPanelPopup.qml:739-756`) | `shell:hyprv-control-panel` | Overlay | T L R B | −1 | OnDemand while visible | yes | whole surface | `parentWindow.screen` |
| WifiFallback popup (`WifiFallback.qml:290-310`) | `shell:hyprv-wifi` | Overlay | T L R B | −1 | OnDemand while visible | yes | whole surface | bar window screen |
| SystemStatsPopup (`SystemStatsPopup.qml:133-150`) | `shell:hyprv-system-stats` | Overlay | T L R B | −1 | OnDemand while visible | yes | whole surface | `parentWindow.screen` |
| QuickAdjustPopup (`QuickAdjustPopup.qml:112-135`) | `shell:hyprv-quick-adjust` | Overlay | T L R B | −1 | None | **no** | **only the slider card** (`mask: Region{item: popupBounds}`) | primary bar screen |
| WeekCalendarPanel | lives inside the **bar window** (`hyprv-quickshell`, see calendar spec) | — | — | — | — | — | — | — |
| StartupConfirm (`StartupConfirm.qml:45-53`) | `hyprv-startup-confirm` | Overlay | none (centred) | ignore (`ExclusionMode.Ignore`) | **Exclusive** | — | whole surface 360×h | compositor default |

- Surface colour is fully transparent; only the card is painted. "Full-output" popups therefore
  catch every click outside the card (used to dismiss).
- Hyprland rules (`hypr/hyprland-winrule.lua:117`): namespaces matching `shell:.*` get
  `blur = true, blur_popups = true, ignore_alpha = 0.05` → the compositor blurs behind every
  pixel with alpha > 0.05 (i.e. behind the cards). These surfaces do **not** have `no_anim`,
  so Hyprland's default layer open/close animation also applies (compositor side, out of scope,
  but the namespace must be kept). `hyprv-startup-confirm` matches no rule (no blur).
- Keep the exact namespaces: they are the contract with the compositor rules.

---

## 4. Hosting in `shell.qml`

| what | where | notes |
|---|---|---|
| `SystemStatsPopup { shellRoot: root }` | `shell.qml:3372-3376` | opened by bar CPU/RAM/Net triggers: `systemStatsPopup.toggleFor(cpuTrigger \| memoryTrigger \| networkTrigger, barWindow)` (`:3851, :3862, :3873`) |
| `controlPanelStatusPoll` PollCommand | `:3378-3389` | active iff `controlPanelPopup.popupRequested`, every 1500 ms, `sh scripts/control-panel-status.sh`, parsed by `updateControlPanelState` (`:1620-1668`) |
| `ControlPanelPopup { shellRoot: root }` | `:3391-3395` | opened by bar icon `controlPanelTrigger` (`:4283-4309`): `toggleFor(controlPanelTrigger, barWindow)`; and by IPC |
| `QuickAdjustPopup { shellRoot: root }` | `:3397-3401` | opened only by IPC / brightness probe |
| Wi-Fi popup | bar `wifiTraySlot` Loader loads `WifiNative.qml` (= `WifiFallback {}`) (`:4170-4192`); sets `shellRoot`, `parentWindow = barWindow`; the instance on the primary bar becomes `root.wifiPanelController` | opened by left-click on the bar Wi-Fi icon or by the control panel Wi-Fi tile (`openWifiPanel()`, `:2553-2557`) |
| Calendar panel | inside each bar window's `centerSection` (`:3962-3997`) | see `ui-panels-calendar.md` |
| `quickAdjustAnchorItem` | `rightTrayPill` of the primary bar (`:4152-4161`) | used to right-align QuickAdjust |
| `primaryBarWindow` | first bar window created (`:3662-3666`) <!-- audit: citation corrected --> | IPC-opened panels use it |

Derived shell state consumed by panels: `mediaUiVisible = anyBarExpanded ||
controlPanelPopup.popupRequested` (`:38`), `detailedStatsVisible = anyBarExpanded ||
systemStatsPopup.popupRequested` (`:37`) — on the desktop `anyBarExpanded` is always true
(`barExpanded()` returns true, `:42`), so these are effectively always true.

### 4.1 IPC targets (must be reproduced verbatim)
Callers: `hypr/hyprland-bind.lua:54` (`SUPER+A` → `controlPanel toggle`),
`hypr/controls.lua:50-51,139,166` (`quickAdjust showBrightnessLevel <n>` and `showVolume`).

| target | fn(args) | effect | source |
|---|---|---|---|
| `controlPanel` | `toggle()` | `controlPanelPopup.toggleCentered(primaryBarWindow)`: if the popup window is visible → `closePopup()`; else open with **no source item** (y uses the bar's exclusive zone, see control spec) | `shell.qml:3403-3410`, `ControlPanelPopup.qml:127-141` |
| `calendar` | `toggle()` | `toggleCalendar(primaryBarWindow.modelData)` | `:3412-3421`, `:2742-2749` |
| `quickAdjust` | `showBrightness()` | `show("brightness")` + brightness probe (no re-show) | `:3427-3430` |
| `quickAdjust` | `showBrightnessLevel(level:int)` | if finite: `updateBrightnessPercentLocally(level)` (snap to 2 %); `show("brightness")`; refresh control-panel status; restart brightness probe debounce | `:3432-3440`, `:2768-2779` |
| `quickAdjust` | `showBrightnessIncrease()` / `showBrightnessDecrease()` | `previewBrightnessDelta(±2)`: local value ±2 (snapped), `show("brightness")`, probe debounce | `:3442-3448`, `:2840-2845` |
| `quickAdjust` | `showVolume()` | `show("volume")`; `syncAudioStatus()` | `:3450-3453` |

`brightnessUiStepPercent = 2` (`shell.qml:149`) <!-- audit: citation corrected -->; `snapBrightnessPercent(v) =
clamp(round(clamp(v)/2)*2)` (`:2772-2774`).

---

## 5. Visual states worth a reference screenshot (parity harness)

Capture each in **dark and light** mode unless noted, at scale 1 and at the user's fractional
scale. Card-only crops are enough (popups are transparent elsewhere).

Control panel (`ui-panels-control.md`):
1. Main page, idle: Wi-Fi on + connected (SSID), BT on + 1 device connected, media playing with
   album art, brightness 50, volume 35, fans available (chassis 40 %, 1200 RPM; pump 60 %),
   DND off, dark-mode toggle reflecting mode, power profile balanced.
2. Main page, everything off: Wi-Fi radio off ("Turned off"), BT off, media idle ("Idle",
   placeholder note glyph, disabled controls 0.6 opacity), fans unavailable (sliders 0.48),
   volume muted (mute glyph), screen recording on (active toggle), prevent-sleep on.
3. Main page hover states: pointer over a toggle (active and inactive), pressed toggle, pointer
   over Split-tile icon well (pressed).
4. Slider mid-drag (brightness 0, 37, 100 — check icon two-tone split exactly at the fill edge).
5. Windows VM toggle busy (disabled look) and running (active).
6. Open animation frames at t = 16, 60, 120, 220 ms; close at t = 60, 150, 210 ms.
7. Bluetooth page: adapter off; on + scanning + empty list; on with 3 devices (1 connected,
   1 paired, 1 unnamed), action message visible; list > 420 px (scroll clipped).
8. Page switch main → bluetooth mid-transition (both halves).
9. Mihomo page online (rule mode, rates > 1 KB/s border pulse) and offline (only reachable by
   test harness; not reachable from current UI).

Wi-Fi popup (`ui-panels-wifi.md`):
10. Connected Wi-Fi, 4 networks (active, saved, open, secure-unknown), no message.
11. Secure network expanded with password field focused, empty (Join disabled) and with text.
12. Ethernet connected, no wireless device; Wi-Fi off; hardware blocked; action message
    ("Connecting to …").
13. > 5 networks (list capped to 5 cards, scrolled to middle).

Quick adjust / stats (`ui-panels-stats.md`):
14. QuickAdjust brightness 0/50/100; volume 0 (muted glyph), 30, 100; mid-show (t = 60 ms) and
    mid-hide.
15. SystemStats with 120 samples (CPU ~45 % green, RAM ~70 % yellow, network bursty
    autoscaled); with 1 sample; with 0 samples (grid only); network disconnected ("未连接").
16. SystemStats with shadow visible (dark mode, over a bright wallpaper to see the corner halo).

Calendar (`ui-panels-calendar.md`):
17. Calendar panel open, current week, events: 2 all-day (one spanning 3 days), overlapping timed
    events (2 lanes), now-line visible, scrolled to now−2 h.
18. Hover tooltip below and above an event; header button hover and press.
19. Connecting / error status pill ("正在连接日历…").
20. Island → calendar morph mid-animation (t = 100 ms) and close mid-animation.
21. StartupConfirm in dark and light palette; with keyboard focus ring on "本次跳过"; hovered and
    pressed "启动".

---

## 6. Data contract overview
Every component file has its own "Data contract" section. Common inputs all panels need:

```rust
struct Theme { dark_mode: bool }                 // drives every token in §1.3
struct ShellGeometry {
    primary_bar_output: OutputId,
    bar_exclusive_zone: i32,                     // round(25 + 33*reveal); 58 when fully shown
    quick_adjust_anchor_right_x: Option<f64>,    // right edge of primary bar's rightTrayPill
}
```
