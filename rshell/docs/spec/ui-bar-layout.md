# UI spec — Bar window item tree & layout (part 2 of 3)

Read `ui-bar.md` §0 (conventions, fonts, Qt rounding rules) and §5 (theme tokens) first.
Source: `quickshell/shell.qml:3548-4368` unless noted.

---

## 1. Qt layout rules the port must replicate

1. **Row**: children laid out left→right at `x = Σ(previous widths) + i*spacing`, skipping
   invisible children (an invisible child takes no space and no spacing). Row's implicit
   width = Σ visible widths + (n_visible−1)·spacing. Fractional positions are kept (no
   rounding) — `spacing` here is 9.5 or 5 and many widths are fractional.
2. **Centre anchors** (`anchors.horizontalCenter` / `verticalCenter` / `centerIn`): Qt uses
   `hcenter(w) = (int(w) is odd) ? (w+1)/2 : w/2` for **both** the parent line and the item
   (qquickanchors.cpp), so `x = hcenter(parent.w) − hcenter(item.w) + offset`. Same for
   vertical. This produces whole-pixel centring for odd integer sizes.
3. **Text implicit size**: width = Σ advances (all glyphs 0.6 em, ui-bar.md §0.1) **plus the
   ink overflow of the last glyph** (Nerd icons overflow their 0.6 em advance); height =
   `ceil(1.32·px)`: 16 px → **22**; 17 px → **23**; 14 px → **19**; 10 px → **14**; 18 px → **24**.
   A Text with `anchors.left/right` still reports its natural `implicitWidth`.
   <!-- audit: measured in Qt 6.12 (components.md §0.2). Previous text said 17→24, 14→20, 18→25 (per-side rounding) and ignored last-glyph overflow; both wrong. -->
4. **AlignHCenter** inside a Text whose width exceeds the natural width: text origin
   `x = (width − naturalWidth)/2`.
5. **Hover** = pointer inside the MouseArea's rect (`containsMouse`), updated on motion /
   enter / leave; no hover delay, no animation.
6. **Click** = Qt `MouseArea.clicked`: press and release of the same button inside the area
   (release outside cancels). Fires on release.
7. **Wheel**: every `wl_pointer.axis` frame that Qt turns into a `QWheelEvent` with
   `angleDelta.y ≠ 0` triggers one action (no accumulation; touchpad smooth scrolling fires
   many). Port: one action per wheel event / per discrete step — and match Qt for
   continuous scroll (one action per frame with nonzero vertical delta).
8. **Cursor shapes**: set per area (PointingHand vs Arrow) — use `wp_cursor_shape_v1`
   `pointer` / `default`.

---

## 2. Bar window (`barWindow`, `shell.qml:3551-3666`)

Surface properties: see ui-bar.md §2 (layer Top/Overlay, anchors top+left+right, height 58
or 820, exclusive zone 58, namespace `hyprv-quickshell`, no keyboard). `margins.bottom: 10`
(`:3642`) is set but has **no effect** in wlr-layer-shell (bottom edge not anchored); do not
apply it.

Hyprland blurs behind this surface wherever pixel alpha > 0.15 (`ignore_alpha 0.15`), so
the pills (alpha 0.8) get a blurred backdrop; fully transparent areas do not.

### 2.1 Input region (`:3647-3660`)
Union of:
- rect `(floor(contentRoot.x), floor(contentRoot.y), ceil(contentRoot.width), 58)` —
  `contentRoot.x = 0`, so this is the **whole 58 px top strip** moved with the slide. Clicks
  on gaps between pills are consumed by the bar (they would land in the exclusive zone
  anyway).
- rect of `centerSection` (island / calendar host) — important when the island or calendar
  is taller than 58.
- rect of `calendarDismissLayer` (full surface height while the calendar is visible, else
  height 0).

Quickshell recomputes `Region{item:…}` only when the item's *own* geometry changes, not when
an ancestor (`contentRoot`) moves (comment `:3644-3646`). Consequently during the fullscreen
slide the centre-section region probably stays at its un-slid position. The intended
behaviour (and what the port should do) is that all three rects move with `contentRoot.y`.

### 2.2 Root items

```
barWindow (W × H, H = 58 or 820)
└─ contentRoot  Item  x=0  y=-standardTravel*fullscreenHide  W×H   visible: reveal>0   (:3721-3727)
   ├─ topBarMask        Item  x=0 y=0  W×58   (geometry only, for the mask)            (:3729-3736)
   ├─ calendarDismissLayer MouseArea z=45  W × (calendarVisible ? H : 0)                (:3738-3756)
   ├─ leftSection       Row   x=10 y=10  spacing=sectionSpacing                        (:3758-3877)
   ├─ centerSection     Rectangle z=50 (island / calendar host)                        (:3879-3997)
   ├─ windowSection     Rectangle (active window title)                               (:3999-4030)
   └─ rightSection      Row   right edge at W-9.5, y=10, spacing=sectionSpacing        (:4032-4365)
```

Paint order: z 0 items in declaration order (leftSection, windowSection, rightSection), then
z 45 (dismiss layer, invisible), then z 50 (centre). Nothing in the bar is ever drawn
outside y ∈ [10, 48] except the centre host when the island/calendar is taller.

`calendarDismissLayer`: enabled only while `calendarVisible`; hover tracking on; left-click
→ `root.requestCalendarClose()`; on any pointer motion while visible, armed, and no
DankCalendar child opening/active → restart `calendarLeaveTimer` (220 ms → close if pointer
not inside the panel). See ui-bar.md §10.

### 2.3 Adaptive spacing: `supplementaryInfoVisible` (`:3570-3598`)

```
fullSectionSpacing    = 9.5
compactSectionSpacing = 5
fullLeftWidth  = 10 + leftPrimaryPill.implicitWidth + 9.5
               + cpuTrigger.natural + memoryTrigger.natural + networkTrigger.natural
fullRightWidth = 9.5 (rightSection.edgeMargin)
               + temperatureTrigger.natural
               + max(0, batteryModule.labelImplicitWidth + 5 + 12)
               + 9.5
               + max(0, previousMediaButton.labelImplicitWidth + 9 + 5)
               + playPauseMediaButton.natural
               + max(0, nextMediaButton.labelImplicitWidth + 5)      // right padding 0 in full mode
               + volumeButton.natural
               + 9.5
               + 4 (systemLeadingSpacer) + wifiTraySlot.implicitWidth
               + controlPanelTrigger.implicitWidth + notificationTrigger.implicitWidth
               + 6 (systemTrailingSpacer)
supplementaryInfoVisible = contentRoot.width >=
        (calendarPanelPresented ? 520 : 448) + 2*9.5 + 2*max(fullLeftWidth, fullRightWidth)
sectionSpacing = supplementaryInfoVisible ? 9.5 : 5
```

`natural` = `TextModule.naturalImplicitWidth` (§3) using the *full-mode* paddings — it is
computed even when the module is hidden, so the decision has no feedback loop. **The tray
width is not part of `fullRightWidth`.** The decision is re-evaluated on every content change
(e.g. the network rate string length), so it can flip at runtime on borderline-width outputs.

Effects of `supplementaryInfoVisible = false`: section spacing 5; temperature and volume
modules hidden (collapse to 0 width); battery left padding 12 (was 5); previous-button left
padding 12 (was 9); next-button right padding 17.5 (was 0).

Worked example (measured widths, components.md §0.2): 5 single-digit workspaces, CPU `7%`,
MEM `42%`, net `0 B/s`, temp `45°C`, power `12.3 W`, volume >50 %: fullLeftWidth = 10 + 223.38
+ 9.5 + 54.38 + 57.97 + 85.16 = **440.38**; fullRightWidth = **367.39** → threshold W ≥ 448 + 19
+ 880.75 = **1347.75**. DP-4 (3072) is always in full mode; HEADLESS-100 (1376 logical) is
borderline. <!-- audit: recomputed; was 430.5 / 344.5 / 1328 using advance-only widths -->

---

## 3. TextModule (inline component, `shell.qml:523-609`)

The building block of almost every bar module.

### 3.1 Properties and defaults

| prop | default | meaning |
|---|---|---|
| `label` | `""` | text |
| `textColor` | `root.primaryText` | |
| `fontFamily` | `root.baseFont` | |
| `fontPixelSize` | 16 | |
| `fontWeight` | `Font.Bold` (700) | |
| `paddingLeft` / `paddingRight` | 8 / 8 | clamped ≥0 |
| `minimumWidth` | 0 | |
| `moduleHeight` | 38 | |
| `interactive` | false | clickable |
| `wheelInteractive` | false | |
| `hoverable` | false | shows hover chip |
| `highlighted` | false | shows highlight chip |
| `collapseWhenHidden` | false | width 0 when `visible=false` |
| `highlightColor` | `root.activeWorkspaceBackground` | |
| `highlightedTextColor` | `root.activeWorkspaceText` | |
| `highlightInset` | 0 | chip inset |

Derived: `labelImplicitWidth = text natural width`; `naturalImplicitWidth = max(text +
padL + padR, minimumWidth)`; `implicitWidth = collapseWhenHidden && !visible ? 0 :
naturalImplicitWidth`; `implicitHeight = moduleHeight`. Signals `leftClicked`,
`rightClicked`, `wheelUp`, `wheelDown`.

### 3.2 Item tree

```
TextModule  (implicitWidth × moduleHeight)
├─ Rectangle chip   fill parent inset highlightInset; radius 19 (clamped to h/2);
│                   color = highlighted ? highlightColor : root.workspaceHoverBackground
│                   visible = highlighted || (hoverable && containsMouse)              (:556-562)
├─ Text label       left = padL, right = width-padR, verticalCenter;
│                   AlignHCenter; color = highlighted ? highlightedTextColor : textColor;
│                   family/pixelSize/weight from props; NativeRendering; no elide       (:564-579)
└─ MouseArea        fill; enabled = interactive||wheelInteractive||hoverable;
                    hoverEnabled = same; buttons Left|Right;
                    cursor = interactive||wheelInteractive ? PointingHand : Arrow       (:581-608)
```

Behaviour: left click → `leftClicked`; right click → `rightClicked` (both fire even when
only `hoverable`, but no handler is connected then). Wheel (only when `wheelInteractive`):
`angleDelta.y > 0` → `wheelUp`, `< 0` → `wheelDown`.

Notes: hover chip color does **not** change the text color (dark mode: muted text on an
opaque black chip). No animations on chip or color — instant switch. `minimumWidth` larger
than the natural width → the text is centred in the padded area (§1.4).

Vertical text position for 16 px in a 38 px module: `y = hcenter(38) − vcenter(22) = 19 −
11 = 8` (baseline ≈ 8+17 = 25).

---

## 4. GroupPill (`GroupPill.qml`)

```
Rectangle  radius 24 (→ clamped 19 at h=38); color = shellRoot.moduleBackground (fallback #303030)
           implicitWidth = contentRow.implicitWidth; height 38; no border
└─ Row contentRow  anchors.left, verticalCenter; spacing 0;  children = pill's content
```

Width is always exactly the sum of its visible children (children are never clipped).

---

## 5. Left section (`shell.qml:3758-3877`)

`Row` at x=10, y=10, spacing `sectionSpacing`. Two pills.

### 5.1 `leftPrimaryPill` (GroupPill) children, in order

| # | item | label / glyph | font | color | padL / padR | other | actions |
|---|---|---|---|---|---|---|---|
| 1 | launcher TextModule (`:3771-3781`) | U+F303 (Arch logo) | 17 px Bold | `launchColor` | 7 / 4 | interactive, **not** hoverable, h 38 | left → exec `rofi-wayland -show drun` |
| 2 | `viewflowWindowsButton` TextModule (`:3784-3801`) | U+F05B3 (md-microsoft-windows) | 17 px Bold | `launchColor` | 7 / 7 | interactive + hoverable (hover chip `workspaceHoverBackground`, 38 tall, radius 19) | left → `$HOME/.local/bin/viewflow-windows start --anchor <X> <Y>` where `(X, Y) = round(mapToGlobal(0, height))` with `Y += 8` (global logical coords of the button's bottom-left, +8 px); right → `$HOME/.local/bin/viewflow-windows menu` |
| 3 | workspace container Item (`:3803-3836`) | — | — | — | — | `implicitWidth = workspaceRow.implicitWidth + 8`, h 38 | — |

Widths (measured, incl. last-glyph overflow): launcher 17.19+11 = **28.19**; viewflow
13.19+14 = **27.19**. Text y for 17 px (height 23): hcenter(38)=19 − hcenter(23)=12 = 7.
<!-- audit: was 21.2 / 24.2 (advance only) -->

### 5.2 Workspaces (inside item #3)

`Row workspaceRow` at x=4, vertically centred (row height 32 → y=3), spacing 0, Repeater
over `root.hyprWorkspaces` (`shell.qml:242-256`):

- Source: `Hyprland.workspaces` (all monitors). Keep only `id > -1`, name not starting with
  `"special:"`, name ≠ `"viewflow-underlay"`; sort by id ascending. If none remain, use one
  fake workspace `{id:1, name:"1", urgent:false}` whose activate does nothing.
- Delegate TextModule (`:3817-3833`):
  - `label = String(name || id)`
  - `textColor = mutedWorkspaceText`
  - `interactive`, `hoverable`
  - `moduleHeight 32`, `paddingLeft 5`, `paddingRight 5`, `minimumWidth 32`,
    `highlightInset 0`, font 16 Bold.
  - `isActive = root.activeWorkspaceId === id` where `activeWorkspaceId =
    Hyprland.focusedWorkspace?.id || 1` (globally focused workspace, `:257`).
  - `highlighted = isActive || (urgent && !isActive)`
  - `highlightColor = urgent && !isActive ? urgentWorkspaceBackground (#a6e3a1) :
    activeWorkspaceBackground`
  - `highlightedTextColor = urgent && !isActive ? urgentWorkspaceText (#11111b) :
    activeWorkspaceText`
  - left click → `workspace.activate()` (Quickshell → Hyprland `dispatch workspace <id>`).
  - no right-click / wheel action.
- Geometry: each chip `max(9.6·len + 10, 32)` × 32, radius 16 (19 clamped). Single-digit →
  32×32 circle; text centred at x = 5 + (22 − 9.6)/2 = 11.2.
- Hover on the active/urgent chip: chip stays highlight-colored (highlight wins).

### 5.3 `performancePill` (GroupPill, always visible)

| item | label | color | padL / padR | actions |
|---|---|---|---|---|
| `cpuTrigger` (`:3844-3853`) | U+F2DB + `" "` + `round(cpuUsage)` + `"%"` | primaryText | 12 / 4 | left → `systemStatsPopup.toggleFor(cpuTrigger, barWindow)`; right → exec `alacritty -t btop -o window.startup_mode=Fullscreen -e btop` |
| `memoryTrigger` (`:3855-3864`) | U+F0C9 + `" "` + `round(memoryUsage)` + `"%"` | primaryText | 6 / 4 | left → `systemStatsPopup.toggleFor(memoryTrigger, barWindow)`; right → same btop command |
| `networkTrigger` (`:3866-3874`) | `networkIcon + " " + networkText` (ui-bar.md §5.2) | primaryText | 6 / 12 | left → `systemStatsPopup.toggleFor(networkTrigger, barWindow)`; no right action |

All 16 px Bold, interactive (pointer cursor), not hoverable (no chip). `round` = JS
`Math.round` (half up). The stats popup positions itself under the given trigger (panels
spec).

---

## 6. Centre section — island / calendar host (`shell.qml:3879-3997`)

```
centerSection  Rectangle  z=50
  width  = calendarPanelPresented ? calendarPanel.width  : dynamicIsland.width
  height = calendarPanelPresented ? calendarPanel.height : dynamicIsland.height
  x      = hcenter(W) − hcenter(width) + attachedCenterOffset
           attachedCenterOffset = calendarPanelPresented ? 0 : dynamicIsland.attachedCenterOffset
  y      = 10
  radius = calendarVisible ? 30 : 24                    (clamped to h/2)
  color  = calendarProgress > 0.001 ? moduleBackground : transparent
  border = calendarProgress > 0.001 ? 1 px rgba(primaryText, dark?0.11:0.09) : none
  clip: true (rectangular clip of children to the host rect)
  Behavior width/height/radius: enabled only while calendarPanelPresented,
           NumberAnimation 210 ms Easing.OutCubic
├─ DynamicIsland  hcenter in host, top 0; opacity = 1 − calendarProgress; visible = opacity > 0.01
│     inputs: now, media*, spectrumValues, agentSessions, calendarOpen = calendarPanelPresented
└─ Loader calendarPanel  (active = calendarPanelPresented) top 0, hcenter;
      width  = min(520, contentRoot.width − 48)
      height = min(500, barWindow.height − 78)      (barWindow.height is 820 then → 500)
      opacity = calendarProgress; transformOrigin Top
      sourceComponent = WeekCalendarPanel{ embeddedSurface: true, bridge: CalendarBridge{weekStart: new Date()} }
```

With the calendar closed the host is fully transparent and simply the island's size: the
island draws its own pill (see ui-island.md). **The clock** is part of the island idle row:
`Qt.formatTime(now, "hh:mm")` 16 px Bold between a lock glyph button and a power glyph button
(`DynamicIsland.qml:822-850`) — fully specified in ui-island.md.

Island signal wiring (`:3933-3957`):

| island signal | action |
|---|---|
| `expandedChanged` | expanded → stop `islandCollapseTimer`, `islandExpanded = true` (surface → 820 tall); collapsed → restart `islandCollapseTimer` (230 ms → `islandExpanded = false`) |
| `heightChanged` | `islandCurrentHeight = height` |
| `lockClicked` | exec `hyprlock` |
| `calendarClicked` | `root.toggleCalendar(barWindow.modelData)` |
| `powerClicked` | exec `wlogout --protocol layer-shell -b 5` |
| `seekRequested(sec)` | `root.seekMedia(sec)` (`playerctl [-p player] position <sec.toFixed(3)>`) |
| `appFocusRequested` | `root.focusMediaApp()` (shell script that focuses the player's window via busctl PID → hyprctl clients) |
| `agentFocusRequested(id)` | exec `python3 scripts/agent-island-action.py focus <id>` |
| `previousClicked` / `nextClicked` | exec `playerctl previous` / `playerctl next` |
| `playPauseClicked` | optimistic `root.mediaPlaying = !mediaPlaying`, exec `playerctl play-pause` |

Calendar panel wiring (`:3984-3994`): `pointerInsideChanged` → inside: stop leave timer;
outside (and no child opening/active): restart leave timer. `closeRequested` →
`requestCalendarClose()`. `expandRequested` → `requestCalendarClose(true)` then
`calendarBridge.showFullApp()`. `childWindowRequested` → `root.beginCalendarChild()`.

Transition summary (calendar open): host morphs from island size to 520×500 (210 ms
OutCubic, Behaviors enabled because `calendarPanelPresented` became true first), radius 24→30,
background/border appear as soon as progress > 0.001, island fades out and calendar fades in
over 210 ms OutCubic (`calendarProgress` Behavior). Close: progress → 0 (210 ms), host keeps
calendar size until `calendarPanelCloseTimer` (210 ms) clears `calendarPanelPresented`; at
that instant Behaviors are disabled, so the host **snaps** back to the island size.

---

## 7. Window title section (`shell.qml:3999-4030`)

```
windowSection Rectangle
  availableWidth = max(0, centerSection.x − (leftSection.x + leftSection.width) − 2*sectionSpacing)
  x = leftSection.x + leftSection.width + sectionSpacing
  y = 10;  height 38;  radius 19;  color moduleBackground;  no border
  width = min(availableWidth, max(38, windowLabel.implicitWidth + 24))
  visible = activeWindowTitle.length > 0 && width > 0
└─ Text windowLabel  left 12, right 12, verticalCenter; text = activeWindowTitle;
   primaryText; 16 px Bold; NativeRendering; elide Right; left-aligned (default)
```

`activeWindowTitle = Hyprland.activeToplevel?.title || ""` (global, same on every bar). No
mouse interaction. Elision uses "…" (U+2026) at the right when the text exceeds `width −
24`. CJK titles fall back to another font via fontconfig.

---

## 8. Right section (`shell.qml:4032-4365`)

`Row` anchored right with `rightMargin = edgeMargin = 9.5`, y=10, spacing `sectionSpacing`.
`centerGap = 9.5` is used only in formulas. Three pills: status, media, tray.

### 8.1 `rightStatusPill`

| item | label | color | padL / padR | visibility | actions |
|---|---|---|---|---|---|
| `temperatureTrigger` (`:4048-4059`) | `(temperatureC >= 70 ? U+F2C7 : U+F2C9) + " " + round(temperatureC) + "°C"` | `temperatureC >= 70 ? criticalColor : primaryText` | 10 / 5 | `supplementaryInfoVisible`, collapseWhenHidden | left → exec btop command (§5.3); no right action |
| `batteryTrigger` Item wrapping `batteryModule` TextModule (`:4061-4084`) | `batteryOrCpuPowerText` | battery available → (`batteryCritical && !batteryPlugged` ? critical : batteryColor); else `usageSeverityColor(cpuPackagePowerW)` | (supp ? 5 : 12) / 12 | always | interactive only if `batteryAvailable`; left → `batteryInfoPopup.toggleFor(batteryTrigger, barWindow)` (anchored to the wrapper Item) |

`batteryCritical = batteryAvailable && percent <= 20`. `usageSeverityColor` on watts: ≥90
critical, ≥60 medium, else low (yes, it reuses the percent thresholds on watts). As noted in
ui-bar.md §5.3, on this machine the slot always shows CPU package power and is not
clickable (arrow cursor).

### 8.2 `rightMediaPill`

| item | label | padL / padR | visibility | actions |
|---|---|---|---|---|
| `previousMediaButton` (`:4091-4099`) | U+F137 (circle-chevron-left) | (supp ? 9 : 12) / 5 | always | left → exec `playerctl previous` |
| `playPauseMediaButton` (`:4101-4112`) | `mediaPlaying ? U+F04C (pause) : U+F04B (play)` | 5 / 0 | always | left → `root.mediaPlaying = !root.mediaPlaying` (optimistic) and exec `playerctl play-pause` |
| `nextMediaButton` (`:4114-4122`) | U+F138 (circle-chevron-right) | 5 / (supp ? 0 : 17.5) | always | left → exec `playerctl next` |
| `volumeButton` (`:4124-4149`) | `volumeIcon` (ui-bar.md §5.2) | 8 / 12 | `supplementaryInfoVisible`, collapseWhenHidden | see below |

All 16 px Bold primaryText except `volumeButton` color `launchColor`, family `iconFont`
(same font). All interactive. Media buttons are always shown even with no player (they act
on whatever `playerctl` picks).

Volume actions (`:4136-4148`) — note the **inverted wheel**:
- left → exec `hyprctl eval 'require("hypr.controls").volume("toggle")'`, then restart
  `audioFollowupRefresh` (150 ms → `syncAudioStatus()`).
- right → exec `pavucontrol`.
- wheel up → `…volume("dec")`; wheel down → `…volume("inc")`; each followed by the 150 ms
  refresh. (Hyprland's controls.lua then calls `quickAdjust showVolume` 80 ms later, which
  shows the QuickAdjust popup.)
- Port: these `hyprctl eval` calls go to Hyprland's Lua API; keep them as Hyprland IPC
  requests (`/eval` over the socket) — no fork needed.

### 8.3 `rightTrayPill` (`:4152-4364`)

On creation, if this bar is the primary bar, `root.quickAdjustAnchorItem = rightTrayPill`
(QuickAdjust popup anchors to it); cleared on destruction.

Children, in order:

| # | item | size | notes |
|---|---|---|---|
| 1 | `systemLeadingSpacer` | 4 × 38 | empty |
| 2 | `wifiTraySlot` | (`item.available ? item.implicitWidth : 0`) × 38; visible if width>0 | Loader, §8.4 |
| 3 | `trayContainer` | (`total>0 ? requestedTrayWidth + 2 : 0`) × 38; visible if total>0 | §8.5 |
| 4 | `controlPanelTrigger` | (16 + 12 = **28**) × 38 | §8.6 |
| 5 | `notificationTrigger` | (glyph width + 10 = 12.39 + 10 = **22.39**; DND glyph 13.39 → 23.39) × 38 <!-- audit: was 8.4+10 --> | §8.7 |
| 6 | `systemTrailingSpacer` | 6 × 38 | empty |

### 8.4 Wi-Fi indicator (WifiNative → WifiFallback → WifiIndicator)

Loader (`:4177-4192`): `active = root.networkWidgetVisible`, source `WifiNative.qml`
(= `WifiFallback {}`), fills the slot. On load: sets `shellRoot`, `parentWindow = barWindow`,
and registers as `root.wifiPanelController` if none yet or this is the primary bar.
WifiFallback extends WifiIndicator and owns the Wi-Fi popup panel (panels spec); bar-facing
parts only:

`WifiFallback.qml:73-75, 266`: `available = root.networkWidgetVisible`; `iconSource =
root.networkTrayIconSource()`; `fallbackLabel = root.networkTrayGlyph()`; left click →
`togglePopup()`. **No right-click action** (signal unconnected).

WifiIndicator (`WifiIndicator.qml`):
```
Item  implicitWidth = available ? 30 : 0; implicitHeight 37 (but filled to 30×38 by the Loader)
      visible = available
├─ Rectangle hover  fill; radius 10;
│     color = containsMouse ? rgba(activeWorkspaceBackground, dark?0.18:0.22) : transparent
├─ WifiIconWithFallback  centred (24×24 box at x=3, y=7); iconSize 24; fallbackPixelSize 24
└─ MouseArea fill; enabled=available; hover; Left|Right; PointingHand
```
Hover tint: dark `rgba(#8789c1, 0.18)`, light `rgba(#8eb6ec, 0.22)`.

WifiIconWithFallback (`WifiIconWithFallback.qml`):
- `usesSymbolicSource = iconSource contains "/symbolic/"` (true for every dark-mode Fluent
  icon).
- `effectiveIconSize = max(16, round(24 * (symbolic ? 16/24 : 1)))` → 16 (dark) / 24 (light).
- `effectiveVerticalOffset = symbolic ? −2 : 0`.
- `IconImage` centred with that vertical offset, size `effectiveIconSize²`, async, smooth,
  mipmap; visible only when the image status is Ready.
- Fallback `Text` (when image not Ready — missing file or still loading): `fallbackLabel`
  glyph, 24 px Bold (explicit `fallbackPixelSize: 24`), `primaryText`, centred with the same
  vertical offset, NativeRendering.

Icon source resolution (`shell.qml:1733-1786`). Fluent theme dirs are found once at startup
by `fluentIconLocator` (`:2935-2960`): first existing of `$HOME/.local/share/icons`,
`/usr/local/share/icons`, `/usr/share/icons` containing `Fluent-light`, `Fluent-dark`,
`Fluent` respectively (on this machine all three are in `~/.local/share/icons`). Port: do the
directory probing in-process.

```
fluent(name, dark):
  dir = dark ? (FluentDark || FluentBase) : (FluentLight || FluentBase); if !dir → ""
  path = dark ? dir/symbolic/status/<name>-symbolic.svg : dir/24/panel/<name>.svg
bucket(p%) = p<20 "0" | p<40 "25" | p<60 "50" | p<80 "75" | "100"
wifiIcon(enabled, hw, connected, strength, secure):
  dark:  !hw → network-wireless-hardware-disabled; !enabled → network-wireless-disabled;
         !connected → network-wireless-disconnected; else nm-signal-<bucket>[-secure]
  light: !hw → network-wireless-offline; !enabled → network-wireless-off;
         !connected → network-wireless-disconnected; else nm-signal-<bucket>[-secure]
wiredIcon(connected):
  dark:  fluent(connected ? network-wired : network-wired-disconnected, dark)
  light: quickshell/assets/tray/network-wired-light.svg | network-wired-offline-light.svg
networkTrayIconSource():
  wired or other active → wiredIcon(true)
  wifi active           → wifiIcon(true, wifiHardwareEnabled, true, wifiSignalStrength, wifiSecure)
  else                  → wifiIcon(wifiRadioEnabled, wifiHardwareEnabled, wifiConnected, strength, secure)
```
`strength` is 0..1, percent = round(strength·100). Active network type from
`defaultInterface` name: `wl*`/`wlan*`/`wifi*` → wifi; `en*`/`eth*` → wired; other non-empty →
other; empty → offline (`:474-490`). SVG colors are baked in (dark symbolic fill `#dedede`,
light panel fill `#363636`, bundled wired light icons `#000000`); no recoloring.

### 8.5 Tray (`trayContainer`, `:4195-4280`)

Items: `root.sortedTrayItems` (`shell.qml:503-521`):
1. All `SystemTray.items` (SNI).
2. If `networkWidgetVisible`, drop items whose key (id, title, tooltipTitle,
   tooltipDescription, icon joined by spaces, lower-cased) contains `nm-applet`,
   `networkmanager` or `network-manager`.
3. Stable sort by `trayItemPriority` (`:2423-2456`), key as above:
   0 = id `chrome_status_icon_1` with empty tooltip title and description; 1 = contains
   `discord`; 2 = `keepass` or `password.kdbx`; 3 = `local-ai-service`, `lais_gui` or
   `preferences-system`; 4 = `懒猫` or `微服`; 5 = `nm-applet`, `wi-fi` or `网络`; 6 = `fcitx`
   or `输入法`; 7 = `wechat` or `微信`; else 100. Ties keep SNI order.

Layout math (`:4198-4224`, helpers `:2458-2510`), with `B=18` (button), `O=22` (overflow
button), `S=7` (spacing), `Smin=3`:
```
total            = sortedTrayItems.length
availableRight   = max(0, W − 9.5 − (centerSection.x + centerSection.width) − 9.5)
fixedRight       = rightStatusPill.w + rightMediaPill.w + 2*sectionSpacing
                   + 4 + wifiSlot.w + controlPanelTrigger.w + notificationTrigger.w + 6
budget           = max(0, availableRight − fixedRight)
slots(v)         = v + (total > v ? 1 : 0)
fixed(v)         = v*B + (total > v ? O : 0)
minW(v)          = fixed(v) + max(0, slots(v)−1)*Smin
prefW(v)         = fixed(v) + max(0, slots(v)−1)*S
visible          = largest v in [total..0] with minW(v) <= budget  (0 if none / total==0)
overflow         = total − visible
requested        = total==0 ? 0
                 : (overflow>0 || prefW(visible) > budget) ? max(minW(visible), budget)
                 : prefW(visible)
spacing          = slots−1 > 0 ? max(0, (requested − fixed(visible)) / (slots−1)) : 0
trayContainer.w  = total>0 ? requested + 2 : 0
```
So with room, icons are 7 px apart; when squeezed (or overflowing), the spacing stretches or
shrinks between 3 px and whatever fills the budget exactly. Edge case: `visible = 0,
total > 0` → only the overflow button, container width `max(22, budget) + 2` with the button
left-aligned. Because `availableRight` depends on the island width, the tray re-flows when
the island resizes. When `overflow` drops to 0 the overflow popup is closed (`:4230-4232`).

```
trayContainer (w × 38)
└─ Row trayRow  x=1, verticalCenter, spacing = spacing (fractional)
   ├─ TrayButton × visible      18 × 38 each (§8.5.1)
   └─ trayOverflowTrigger Item  22 × 38, visible = overflow > 0
        ├─ Text "⋯" (U+22EF) centred; 18 px Bold; primaryText; NativeRendering
        └─ MouseArea fill; Left only; PointingHand;
           click → trayOverflowPopup.toggleFor(trayOverflowTrigger, barWindow, overflowItems)
```

#### 8.5.1 TrayButton (`TrayButton.qml`)
```
Item 18 × 38
├─ IconImage 18×18 at x=0, vertically centred (y=10); source = root.trayIconSource(item);
│     async, smooth, mipmap; visible when status Ready
├─ Text fallback  centred; visible when icon not Ready;
│     text = first char of item.id upper-cased, or "?"; primaryText; baseFont; 10 px;
│     weight 400; default renderType (QtRendering)
└─ MouseArea fill; hover; Left|Right; PointingHand
```
Actions (`TrayButton.qml:46-71`):
- Left: if `!item.onlyMenu` → `item.activate()` (SNI `Activate(x,y)`); else if `hasMenu`
  → open tray menu at this button.
- Right: if `hasMenu` → open tray menu (`root.openTrayMenu(item, button, parentWindow)` →
  `trayMenuPopup.openFor`); else `item.secondaryActivate()` (SNI `SecondaryActivate`).
- Wheel: `delta = angleDelta.y > 0 ? +1 : −1` → `item.scroll(delta, false)` (vertical).
  Note a horizontal-only wheel event (y = 0) sends −1.

Icon source (`shell.qml:2512-2538`):
1. id `local-ai-service` → bundled `quickshell/assets/lais/lais.svg`.
2. empty icon → none (fallback letter).
3. contains `?path=` (Qt "icon name + theme path" form): `file://<path>/<basename>` where
   basename = part after the last `/` of the part before `?path=`; if basename starts with
   `dropboxstatus` it becomes `hicolor/16x16/status/<basename>`.
4. absolute path → `file://` + path.
5. already a URL (`image://`, `qrc:/`, `file://`, `http(s)://`) → as is (`image://` covers
   Quickshell's pixmap provider for SNI IconPixmap data).
6. else icon-theme name → `image://icon/<name>`: Qt icon theme lookup, theme from qt6ct
   (`icon_theme=Fluent-light`), fallback hicolor. Port: freedesktop icon lookup with theme
   Fluent-light (regardless of dark mode — Qt's theme does not follow `darkMode`), size 18
   × scale; SNI `IconPixmap` → pick the best size and scale.

### 8.6 Control-panel trigger (`:4282-4309`)
```
Item 28 × 38
├─ Image 16×16 centred (x=6, y=11); source = darkMode ? assets/bar/control-panel-dark.svg
│     (fill #cdd6f4) : assets/bar/control-panel-light.svg (fill #2b2b2c);
│     PreserveAspectFit, smooth, mipmap, sourceSize 32×32
└─ MouseArea fill; PointingHand; click (left only, default) →
   controlPanelPopup.toggleFor(controlPanelTrigger, barWindow)
```
The SVG (viewBox 1024², three horizontal "slider" bars with knobs) is rasterised at 32×32
and drawn into 16×16 logical (= 1:1 at scale 2). Port: rasterise at `16·scale`.

### 8.7 Notification trigger (`:4311-4356`)
```
Item (12.39 + 10) × 38   (DND: 13.39 + 10)
├─ Text notificationGlyph  centred; notificationIcon (U+F0A2 bell / U+F009B bell-off);
│     primaryText; 14 px Bold; NativeRendering
├─ Text dot  anchored top-right: topMargin 4, rightMargin 2 (x = w − 2 − 8.0, y = 4; U+F444 @10 px measures 8.0 wide, 14 tall) <!-- audit: was 6.0 -->;
│     U+F444 (oct-dot-fill); color #ff0000; 10 px Bold; visible = notificationHasDot
└─ MouseArea fill; Left|Right; PointingHand
     left  → exec `swaync-client -t -sw` (toggle control centre)
     right → exec `swaync-client -d -sw` (toggle DND)
```
State source: `swaync-client -swb` stream (`alt` contains `"dnd"` → DND; contains
`"notification"` → dot). Port: subscribe to swaync over D-Bus (`org.erikreider.swaync.cc`)
instead of the subprocess if possible; otherwise keep one long-lived subscriber.

---

## 9. Worked example — DP-4 (3072 px), light mode, fractional advances

Assume 5 workspaces (1 focused), CPU 7 %, MEM 42 %, net `0 B/s`, temp 45 °C, CPU power
`12.3 W`, playing, volume 60 %, Wi-Fi connected, 4 tray items, no notification dot, island
idle (lock + `hh:mm` + power; width from ui-island.md, call it `Iw`, offset 0).

<!-- audit: recomputed with measured Qt widths (components.md §0.2); previous numbers used 0.6 em per glyph and ignored the last glyph's ink overflow -->
- Left: x=10. leftPrimaryPill: 28.19 + 27.19 + (5·32 + 8) = 223.38 wide → [10, 233.38].
  performancePill at 242.88: cpu 38.38+16 = 54.38; mem 47.97+10 = 57.97; net 67.16+18 =
  85.16; pill 197.5 → [242.88, 440.38].
- Window title: x = 440.38 + 9.5 = 449.88; width ≤ centre.x − 440.38 − 19.
- Centre: x = 1536 − Iw/2 (Iw = 120.16 idle, ui-island.md §3.9).
- Right (from the right edge 3062.5 going left): tray pill = 4 + 30 + (4·18 + 3·7 + 2 = 95)
  + 28 + 22.39 + 6 = 185.39 → [2877.11, 3062.5]; media pill = 28.59 + 14.59 + 19.59 + 38.59
  = 101.38 → [2766.23, 2867.61]; status pill = 72.56 + 74.56 = 147.13 → [2609.61, 2756.73].

All pills y ∈ [10, 48], radius 19, color `rgba(#e7e7ec,0.8)`.

---

## 10. Data contract — bar window

```rust
struct BarInputs {
    output: OutputInfo,
    theme: Theme,
    screen_state: BarScreenState,          // reveal, fullscreen_hide
    hypr: HyprState,                       // workspaces, focused id, active title
    stats: SystemStats,                    // cpu, mem, temp, cpu power, iface, rx/tx
    audio: AudioState,
    media_playing: bool,                   // play/pause glyph (optimistically toggled on click)
    battery: BatteryInfo,                  // available==false on desktop
    wifi: WifiState,                       // icon/glyph selection, network_widget_visible
    fluent_dirs: (Option<PathBuf>, Option<PathBuf>, Option<PathBuf>), // light, dark, base
    notification: NotificationState,
    tray: Vec<TrayItem>,
    island: IslandGeometry,                // width, height, attached_center_offset, expanded
    calendar_open_here: bool,              // calendarOpen && calendarScreen == this output
    is_primary: bool,
}
enum BarAction {
    ExecDetached(Vec<String>),             // rofi, viewflow-windows, btop, playerctl, pavucontrol, swaync-client, hyprlock, wlogout
    HyprEval(String),                      // hypr.controls volume(...)
    ActivateWorkspace(i64),
    ToggleStatsPopup { anchor: Rect },     // cpu / mem / net trigger rects
    ToggleBatteryPopup { anchor: Rect },
    ToggleControlPanel { anchor: Rect },
    ToggleWifiPanel,
    ToggleTrayOverflow { anchor: Rect },
    TrayActivate(TrayId), TraySecondary(TrayId), TrayScroll(TrayId, i32), OpenTrayMenu { item: TrayId, anchor: Rect },
    ToggleCalendar, CloseCalendar,
}
```
