# UI spec — Dynamic Island (centre of the bar)

Source of truth: `quickshell/DynamicIsland.qml` (1542 lines), `AgentTaskModel.qml`,
`AudioSpectrum.qml`, `PollCommand.qml`, `StreamCommand.qml`, the island host in
`shell.qml` (≈3555–3990), `scripts/audio-spectrum.sh`, `scripts/media-status.sh`,
`scripts/desktop-state.py`, `scripts/agent_island.py`, `scripts/agent-island-action.py`.
All file references are relative to `~/.config/HyprV/quickshell/`.

Abbreviations: `DI` = `DynamicIsland.qml`, `SH` = `shell.qml`, `AS` = `AudioSpectrum.qml`.

---

## 0. Global conventions

### 0.1 Units and scaling
- **The island does not use `Scaler.qml`.** `hypr-comp/Scaler.qml` is only used by
  `hypr-comp/applauncher`. Every number in this document is a *logical* px (Wayland
  surface-local coordinate). Fractional output scale is applied by the compositor scale
  (render at `logical × scale` buffer px, per ARCHITECTURE.md).
- `scripts/launch.sh:10` sets `QSG_RENDER_LOOP=basic`: Qt animations are driven by the
  wall-clock animation driver; durations below are wall-clock milliseconds.

### 0.2 Easing curves (Qt `QEasingCurve`, t ∈ [0,1])
| Name | Formula |
|---|---|
| `OutCubic` | `1 − (1−t)³` |
| `OutQuad` | `1 − (1−t)²` = `t(2−t)` |

Only these two curves are used anywhere in the island and its host.

### 0.3 `Behavior` semantics (must be reproduced exactly)
Every animation in the island is a QML `Behavior on <prop> { NumberAnimation {...} }`
unless stated otherwise. Semantics:
1. When the *bound target value* of the property changes, the animation (re)starts **from
   the property's current (animated) value** to the new target, with the **full duration**
   and the curve applied from t=0. There is no velocity continuity.
2. If the target changes every frame (e.g. `radius: height / 2` while `height` is itself
   animating), the Behavior restarts every frame → the value "chases" the target with a
   lag. Reproduce by literally restarting the tween on every target change.
3. Setting a property to its current target value does not restart anything.
4. `visible: opacity > 0.01` items stop rendering *and stop receiving input* once their
   opacity falls to ≤ 0.01.
5. Behaviors do not run on first creation (initial values are applied instantly).
6. A disabled Behavior (`enabled: false`) applies the new value instantly.

### 0.4 Rectangle radius clamp
Qt clamps `Rectangle.radius` to `min(width, height) / 2` at paint time. Many radii below
(e.g. 24 on a 38-px-tall box) are therefore effectively 19. The *animated* value is the
unclamped one; clamping happens per frame when painting.

### 0.5 Borders
`Rectangle { border.width: 1 }` draws the border **inside** the rectangle bounds (Qt
Quick inner stroke), same rounded shape, antialiased (`antialiasing: true` everywhere).

### 0.6 Fonts
- `shellRoot.baseFont` = `shellRoot.iconFont` = `"JetBrainsMono Nerd Font"`
  (`SH:232-233`). Files: `/usr/share/fonts/TTF/JetBrainsMonoNerdFont-*.ttf`.
- All `Text` items use `renderType: Text.NativeRendering` (FreeType hinting per
  fontconfig, Qt's native glyph rasteriser, not distance-field).
- Weight mapping (fontconfig closest match, verified with `fc-match`):
  `Font.DemiBold`(600) → SemiBold, `Font.Bold`(700) → Bold, `Font.Black`(900) →
  **ExtraBold** (no Black face installed).
- Font metrics (Bold file): unitsPerEm 1000, hhea ascender 1020, descender −300,
  lineGap 0 (OS/2 typo identical). **Every glyph used by the island has advance 600
  units** (digits, `:`, `!`, `?`, `✓` U+2713, Latin caps, and all Nerd glyphs
  U+F001, U+F011, U+F023, U+F03E4, U+F040A, U+F04AD, U+F04AE). At 16 px an advance is
  9.6 px; Qt's FreeType engine without design metrics rounds advances to integer px →
  **10 px** per character (verify against reference; see §9 "idle-width").
  <!-- audit: RESOLVED by measurement (Qt 6.12, offscreen, this fontconfig; scale-independent): advances are fractional, 9.59375 px at 16 px (hintslight ⇒ design metrics), NOT 10. Text.implicitWidth additionally includes the ink overflow of the last glyph: U+F023 alone = 12.59, U+F011 alone = 13.59, "12:34" = 47.97. Line height = ceil(1.32·px): 9→12, 10→14, 11→15, 12→16, 13→18, 16→22, 18→24. See components.md §0.2. -->
- Non-Latin fallback (titles, project names): fontconfig fallback chain for the family
  is `JetBrainsMono Nerd Font → Iosevka Fixed → Iosevka Nerd Font Mono → HarmonyOS Sans SC
  → Apple Color Emoji …`. CJK media titles render in HarmonyOS Sans SC.
- `elide: Text.ElideRight` → truncate with `…` (U+2026) to fit `width`.
- No `letterSpacing` is set anywhere (0).
- Text item height = font line height (ascent + descent); anchors `verticalCenter`
  centre that box, not the glyph ink.

### 0.7 Theme tokens (traced to literal values)
Root tokens (`SH:212-231`, `SH:1670-1673`): `withAlpha(c, a)` = `Qt.rgba(c.r, c.g, c.b, a)`
(replaces alpha). `darkMode` = file `$XDG_STATE_HOME/hyprv/quickshell/ui-state.env`
(default `~/.local/state/...`) contains a line exactly `theme=dark` (`SH:3219-3226`,
FileView watched, re-read on change). Currently the file says `theme=light`.

Island tokens (`DI:125-132`):

| token | definition | light | dark |
|---|---|---|---|
| `surfaceColor` | `moduleBackground` = `withAlpha(dark?#1e1e2e:#e7e7ec, 0.8)` | `rgba(231,231,236,0.80)` | `rgba(30,30,46,0.80)` |
| `textColor` | `primaryText` | `#2b2b2c` | `#cdd6f4` |
| `mutedTextColor` | `withAlpha(primaryText, dark?0.68:0.62)` | `rgba(43,43,44,0.62)` | `rgba(205,214,244,0.68)` |
| `strokeColor` | `withAlpha(primaryText, dark?0.13:0.10)` | `rgba(43,43,44,0.10)` | `rgba(205,214,244,0.13)` |
| `hoverFill` | `withAlpha(primaryText, dark?0.10:0.12)` | `rgba(43,43,44,0.12)` | `rgba(205,214,244,0.10)` |
| `softFill` | `withAlpha(#ffffff, dark?0.07:0.23)` | `rgba(255,255,255,0.23)` | `rgba(255,255,255,0.07)` |
| `accentColor` | `launchColor` | `#407cdd` | `#89b4fa` |
| `accentSoftColor` | `withAlpha(launchColor, dark?0.72:0.78)` | `rgba(64,124,221,0.78)` | `rgba(137,180,250,0.72)` |

Host-only token: calendar morph border `withAlpha(primaryText, dark?0.11:0.09)` (`SH:3893`).

Colours are output **premultiplied, translucent**; Hyprland blurs behind
(`hypr/hyprland-winrule.lua:118`: layerrule namespace `hyprv-quickshell`, `no_anim`,
`blur`, `blur_popups`, `ignore_alpha = 0.15` → only pixels with α > 0.15 get blur).

### 0.8 Shadows / effects
**No drop shadows anywhere in the island.** The only `MultiEffect` is the album-art
rounded mask (§3.2). No `layer` opacity groups except that one.

---

## 1. Host: surface and container (`shell.qml`)

### 1.1 Window (`barWindow`, one per screen) — `SH:3549-3660`
The island lives in the **bar's** layer surface (shared with spec-bar). Relevant facts:
- `PanelWindow` per screen (`Variants { model: Quickshell.screens }`).
- Layer: `Top`; `Overlay` while `reveal < 1` or the bar is in fullscreen-hide mode
  (bar spec). Namespace **`hyprv-quickshell`**.
- Anchors: top + left + right. `margins.bottom: 10` (no effect for a top-anchored surface
  of explicit height).
- **Height**: `min(screen.height, (calendarPanelPresented || islandExpanded) ? 820 : 58)`
  (`SH:3637`). Switches instantly (no animation).
- `exclusiveZone: round(25 + 33*reveal)` → 58 when fully revealed (desktop: always).
- Keyboard interactivity: **None**. `focusable: false`. Background transparent,
  non-opaque surface.
- `islandExpanded` (`SH:3564`): set `true` immediately when `DI.expanded` becomes true;
  when it becomes false, `islandCollapseTimer` (230 ms, single-shot, `SH:3668-3674`)
  sets it false — so the 820 px surface stays until the 210 ms collapse animation ends.
  Expanding cancels the timer (`SH:3933-3940`).
- **Input region** (`SH:3647-3660`): union of
  1. rect `(floor(contentRoot.x), floor(contentRoot.y), ceil(contentRoot.width), 58)`
     — the full-width 58-px top strip (`topBarMask`),
  2. `centerSection`'s bounds (= island bounds, follows its animated geometry every frame),
  3. `calendarDismissLayer` (full surface while calendar visible, else 0 high).
  Consequence: the island's 10-px hover margin (§4) only works inside the top 58-px strip.
- `contentRoot.y = −standardTravel·(1−reveal) + standardOffset·reveal` — bar slide;
  0 when revealed (bar spec owns this).

### 1.2 `centerSection` (morph container) — `SH:3879-3910`
`Rectangle`, child of `contentRoot`:
- `anchors.horizontalCenter: parent.horizontalCenter`,
  `anchors.horizontalCenterOffset: attachedCenterOffset` where
  `attachedCenterOffset = calendarPanelPresented ? 0 : dynamicIsland.attachedCenterOffset`.
- `anchors.top: parent.top`, `topMargin: 10` → island top edge at y = 10.
- `width  = calendarPanelPresented ? calendarPanel.width  : dynamicIsland.width`
- `height = calendarPanelPresented ? calendarPanel.height : dynamicIsland.height`
- `z: 50`, `clip: true`, antialiased.
- `radius: calendarVisible ? 30 : 24`
- `color: calendarProgress > 0.001 ? moduleBackground : transparent`
- `border.width: calendarProgress > 0.001 ? 1 : 0`, `border.color: withAlpha(primaryText, dark?0.11:0.09)`
- Behaviors on width, height, radius: 210 ms OutCubic, **`enabled: calendarPanelPresented`**
  (so normal island morphing is done by the island itself; this container just copies the
  island's animated size with no extra lag).
- Children: `DynamicIsland` (centred horizontally, top-anchored) and the calendar
  `Loader` (calendar panel content is out of scope — see spec-panels; here only its box:
  `width = min(520, contentRoot.width − 48)`, `height = min(500, barWindow.height − 78)`
  → 500 when the window is 820 tall, top-anchored, centred, `opacity: calendarProgress`).

### 1.3 Island ↔ calendar morph — `SH:3604-3628, 3676-3710, 3912-3920`
State: `calendarVisible = root.calendarOpen && root.calendarScreen === this screen`.
`calendarProgress` has `Behavior` 210 ms OutCubic.

Open (`calendarVisible` → true):
1. Stop leave/close timers; `calendarDismissArmed = false`; restart `calendarDismissArmTimer` (260 ms → `calendarDismissArmed = calendarVisible`).
2. `calendarPanelPresented = true` (synchronously): window height → 820; the
   container's Behaviors become enabled; width/height retarget to the calendar box and
   animate 210 ms OutCubic from the island's current size; `horizontalCenterOffset`
   jumps (no animation) from the island's offset to 0; radius retargets 24→30 (animated).
   Island gets `calendarOpen = true` → clears hover (§4), collapses if expanded.
3. `Qt.callLater(() => calendarProgress = 1)` → next event-loop turn: progress animates
   0→1 over 210 ms OutCubic. Container fill/border appear once progress > 0.001.
   Island `opacity = 1 − calendarProgress` (fades out; hidden at ≤ 0.01).
   Calendar panel opacity = `calendarProgress`.

Close (`calendarVisible` → false):
1. `calendarProgress = 0` (animated 1→0, 210 ms OutCubic); island fades back in,
   calendar fades out; container stays at the calendar size (its width/height targets are
   unchanged while presented).
2. `calendarPanelCloseTimer` (210 ms) fires → `calendarPanelPresented = false`:
   Behaviors disabled → container **snaps** to island size; offset snaps back to the
   island's `attachedCenterOffset`; window height → 58 (unless island expanded);
   island `calendarOpen = false`. Container fill disappears once progress ≤ 0.001.

### 1.4 Island instance wiring — `SH:3912-3958`
| DI property | bound to |
|---|---|
| `shellRoot` | root |
| `calendarOpen` | `barWindow.calendarPanelPresented` |
| `opacity` | `1 − barWindow.calendarProgress`; `visible: opacity > 0.01` |
| `now` | `root.now` (SystemClock, minute precision, `SH:3190-3195`) |
| `mediaAvailable/Playing/Title/Artist/PlayerName/ArtUrl/PositionSeconds/LengthSeconds` | root media state (§6.2) |
| `spectrumValues` | `root.audioSpectrumValues` (§6.3) |
| `agentSessions` | `root.agentIslandSessions` (§6.4) |
| `mediaManuallyHidden` | *not bound* — internal state only |

Signal → action:
| signal | action (`SH`) |
|---|---|
| `lockClicked` | `runDetached(["hyprlock"])` |
| `calendarClicked` | `toggleCalendar(screen)` (open on this screen, or close if already open here; close is refused while a calendar child window is opening/active unless forced) |
| `powerClicked` | `runDetached(["wlogout","--protocol","layer-shell","-b","5"])` |
| `previousClicked` | `runDetached(["playerctl","previous"])` |
| `playPauseClicked` | **optimistic** `root.mediaPlaying = !root.mediaPlaying`, then `playerctl play-pause` |
| `nextClicked` | `playerctl next` |
| `seekRequested(sec)` | `seekMedia`: clamp to [0, length] (or ≥0 if length unknown); `playerctl [-p <player>] position <sec.toFixed(3)>`; set `mediaPositionSeconds = target` immediately; restart `mediaFollowupRefresh` (180 ms → re-poll media) (`SH:2872-2888`) |
| `appFocusRequested` | `focusMediaApp` (`SH:2890-2922`): find MPRIS player PID via `busctl --user status org.mpris.MediaPlayer2.<player>` (or `instanceNNN` suffix), find Hyprland client with that pid, `hyprctl dispatch focuswindow address:<addr>`; fallback: player name contains cider/Cider → `class:^(Cider)$`, spotify → `class:^(Spotify)$`; nothing if player name empty. Rust: do natively via zbus `GetConnectionUnixProcessID` + Hyprland socket. |
| `agentFocusRequested(id)` | if id non-empty: `python3 scripts/agent-island-action.py focus <id>` (§6.4.4) |
| `expandedChanged` | window `islandExpanded` logic (§1.1) |
| `heightChanged` | `barWindow.islandCurrentHeight = height` (unused elsewhere) |

---

## 2. `DynamicIsland` root item — `DI:4-165`

### 2.1 Inputs (properties)
`shellRoot`, `now: date`, `mediaAvailable: bool`, `mediaPlaying: bool`,
`mediaTitle/mediaArtist/mediaPlayerName/mediaArtUrl: string`,
`mediaPositionSeconds/mediaLengthSeconds: real`, `spectrumValues: real[]`,
`mediaManuallyHidden: bool` (internal), `agentSessions: object[]`, `calendarOpen: bool`.

Internal state: `seeking=false`, `seekPreviewSeconds=0`, `hoverTarget: "" | "agent" | "music"`,
`appHoverActive=false`, `keepPrimaryCollapsedForMusic=false`.

### 2.2 Derived state (exact)
```
hasMusicContent   = mediaAvailable && (mediaPlaying || title≠"" || artist≠"" || artUrl≠"")
agentActive       = agentSessions is array && length > 0
primaryAgent      = agentActive ? agentSessions[0] : null
musicActive       = hasMusicContent && !mediaManuallyHidden
musicPrimaryActive= musicActive && !agentActive
dualIslandActive  = agentActive && musicActive
agentExpanded     = !calendarOpen && appHoverActive && agentActive && hoverTarget=="agent" && islandHover.hovered
musicExpanded     = !calendarOpen && appHoverActive && musicActive && hoverTarget=="music" && islandHover.hovered
expanded          = agentExpanded || musicExpanded
primaryFrameCollapsedForMusic = dualIslandActive && keepPrimaryCollapsedForMusic
displayedPositionSeconds = seeking ? seekPreviewSeconds : mediaPositionSeconds
progressRatio     = length>0 ? clamp(displayed/length, 0, 1) : 0
```

### 2.3 Geometry constants (`DI:58-112`)
| name | value |
|---|---|
| `collapsedHeight` | 38 |
| `expandedWidth` (music) | 392 |
| `expandedHeight` (music) | 176 |
| `agentExpandedWidth` | 448 |
| `swipeThreshold` | 34 |
| `hoverBridgeMargin` | 10 |
| agent **minimal** pill: leftPad 3, clockW 54, gap 0, statusSlotW 31, ringSize 24, rightPad 6 | `agentMinimalWidth` = **94** |
| `agentMinimalClockCenterX` = 3 + 31 + 0 + 54/2 | **61** |
| agent **compact** pill: clockW 58, logo 28, logoGap 3, ring 24, ringGap 3, sidePad 8 | leftExtent 39, rightExtent 35, `agentCompactPillWidth` = **132** |
| `agentCompactClockCenterX` = 39 + 29 | **68** |
| `musicDetachedWidth` | 38 |
| `agentDetachedGap` | 4 |
| `dualCollapsedWidth` = 94 + 4 + 38 | **136** |
| `dualMusicLeftX` | 98 |
| `compactWidth` (music compact) = 7 + 7 + 24 + 54 + 22 + 3·2 | **120** |
| agent session row height / spacing | 42 / 8 |
| `agentExpandedBudgetHeight` | 490 |
| `agentExpandedTopReserve` = 26 + 58 + 20 | 104 |

```
n = agentActive ? agentSessions.length : 0
agentSessionContentHeight = 42n + 8·max(0, n−1)
agentSessionListHeight    = max(42, min(contentHeight, 490 − 104 = 386))
agentExpandedHeight       = max(184, 104 + listHeight)
```
→ n=1: 184 · n=2: 196 · n=3: 246 · n=4: 296 · n=5: 346 · n=6: 396 · n=7: 446 · n≥8: 490 (list 386, scrolls).

### 2.4 Size targets (`DI:77-124`)
```
agentCompactWidth     = dual ? 136 : 132
primaryCollapsedWidth = dual ? 94 : 132
mainIslandWidth       = agentActive ? (agentExpanded ? 448 : primaryCollapsedWidth) : targetWidth
targetWidth  = agentActive
     ? (agentExpanded ? 448 : (musicExpanded ? 392 : agentCompactWidth))
     : (musicExpanded ? 392 : (musicPrimaryActive ? 120 : idleRow.implicitWidth))
targetHeight = agentExpanded ? agentExpandedHeight : (musicExpanded ? 176 : 38)
collapsedClockCenterX = agentActive ? (dual ? 61 : 68) : width/2      (width = animated)
attachedCenterOffset  = agentActive ? (expanded ? 0 : targetWidth/2 − (dual ? 61 : 68)) : 0
```
`attachedCenterOffset` values: single agent collapsed = 132/2 − 68 = **−2**; dual collapsed =
136/2 − 61 = **+7**; otherwise 0. Purpose: the host shifts the island so the **clock glyphs
stay exactly at the screen's horizontal centre**.

Root item: `width: targetWidth`, `height: targetHeight` (each with Behavior **210 ms
OutCubic**), `attachedCenterOffset` Behavior **210 ms OutCubic**, `clip: true`,
`z: expanded ? 40 : 1`, `transformOrigin: Top`, `scale: 1` (no effect).
Note `targetWidth`/`attachedCenterOffset` use *target* values while `collapsedClockCenterX`
(idle) uses the *animated* width.

### 2.5 Z-order of children (declaration order; z=0 unless stated)
1. `islandFrame` (§3.1)
2. `musicExpandedFrame` — z 12 if `musicExpanded || keepPrimaryCollapsedForMusic` else 0
3. `idleContent`
4. `compactMusic`
5. `expandedMusic` — z 13 if `musicExpanded || keepPrimaryCollapsedForMusic` else 0
6. `compactAgent`
7. `expandedAgent`
8. `islandHover` (HoverHandler on root)
9. `sideHotspots` z 99
10. `clockHotspot` z 100
11. `islandSwipe` (DragHandler on root)

---

## 3. Visual components

### 3.1 `islandFrame` — the primary background (`DI:721-759`)
`Rectangle`, top-anchored:
- `x = primaryFrameCollapsedForMusic ? island.width/2 − attachedCenterOffset − 61 : 0`
- `width = mainIslandWidth` — **own Behavior 210 ms OutCubic** (in addition to the root's)
- `height = primaryFrameCollapsedForMusic ? 38 : island.height` (no Behavior; follows the root's animated height)
- `radius = primaryFrameCollapsedForMusic ? height/2 : (expanded ? 30 : 24)`, Behavior 210 ms OutCubic (clamp §0.4 → 19 when 38 tall)
- `color: surfaceColor`
- `border.width = (musicPrimaryActive || agentActive) ? 1 : 0` (**idle state has no border**), `border.color: strokeColor`, instant
- `opacity: 1` (Behavior 120 ms OutQuad exists but never triggers)
- `visible: !primaryFrameCollapsedForMusic` (instant)
- `clip: true`, antialiased.

### 3.2 `AlbumArtFrame` (reusable) — `DI:395-444`
Rectangle of given size, `radius = artRadius`, `color: softFill`, no border, clip.
- `Image`: fills frame; `source = musicActive && artUrl≠"" ? artUrl : ""` (file:// or
  http(s) URLs, loaded asynchronously); `PreserveAspectCrop` (scale to cover, centre crop);
  smooth + mipmap (trilinear downscale); visible only when loaded (`status == Ready`).
  Rounded by `MultiEffect` mask (a hidden same-radius Rectangle), `maskThresholdMin 0.5`,
  `maskSpreadAtMin 1.0` → antialiased rounded-rect alpha mask of radius `artRadius`.
- Fallback (when image not visible): glyph **U+F001** (nf-fa-music), `accentColor`,
  JetBrainsMono NF Bold, `pixelSize = round(min(w,h) × 0.46)`, centred.
  (24→11 px, 28→13 px, 70→32 px.)
- While image loads/absent you see `softFill` + glyph.

Instances: compact 24×24 r8; minimal music pill 28×28 r14 (circle); expanded 70×70 r16.

### 3.3 `IslandTextButton` (idle row) — `DI:322-393`
- `width = labelImplicitWidth + paddingLeft + paddingRight`, `height = 38`.
- `scale = pressed ? 0.96 : 1`, Behavior 120 ms OutCubic, origin = item centre.
- Hover bg: Rectangle filling the button, `radius 19`, `color hoverFill`,
  `opacity = containsMouse ? 1 : 0`, Behavior 120 ms OutCubic.
- Label: centred horizontally between paddings, vertically centred; colour `textColor`;
  JetBrainsMono NF **16 px Bold**.
- MouseArea: hover enabled; enabled = `interactive`; cursor pointing-hand when enabled; click → `clicked`.

### 3.4 `MediaGlyphButton` — `DI:446-493`
- Size: primary 44×44, else 40×40.
- `scale = pressed ? 0.92 : (hover ? 1.06 : 1)`, Behavior 120 ms OutCubic, centre origin.
- Circle (radius h/2): primary → `accentSoftColor`, border 0; other → `softFill`,
  border 1 `strokeColor`. `opacity = hover || primary ? 1 : 0.82` (instant).
- Glyph: centred (primary: `verticalCenterOffset +1`), colour primary `#ffffff` else
  `textColor`; NF Bold, 21 px primary / 17 px others.
- Click → `clicked`. Cursor pointing-hand.

### 3.5 `AgentProgressRing` — `DI:495-535`
Canvas, default 25×25 (always overridden). Paint:
```
size = min(w,h); c = size/2; r = max(1, c − 3); p = clamp(value, 0, 1)
lineWidth 3, lineCap round
stroke full circle (c, c, r) in trackColor (= softFill)
stroke arc from −90° clockwise by 360°·p in ringColor (= accentColor)
```
Repaint on any value/colour/size change. No animation of the value (jumps).
Note: canvas is antialiased; with p = 0 Qt still emits a zero-length arc with round caps
(may render a 3-px dot at 12 o'clock — verify; real progress is always ≥ 0.16).

### 3.6 `AgentLogo` — `DI:537-565`
Item (default 24×24, overridden). `Image` with `anchors.margins 3`, PreserveAspectFit,
smooth + mipmap, source:
- `source == "codex"` → `$configDir/quickshell/assets/agent/codex.png` (816×816 RGBA)
- `source == "claude"` → `.../assets/agent/claude.png` (256×256 RGBA)
- else none.
Fallback text (when image not Ready): first char of `source` (default `"a"`) uppercased,
colour `accentColor`, NF **12 px Black(→ExtraBold)**, centred.

### 3.7 `AgentCompactPill` — `DI:567-692`
`width = minimal ? 94 : 132`, `height 38`, `radius 19`,
`color = minimal && primaryFrameCollapsedForMusic ? surfaceColor : transparent`,
`border.width = same condition ? 1 : 0`, `border.color strokeColor`.
(Normally transparent: the background is `islandFrame` underneath.)

**Minimal layout** (`visible: minimal`):
- Clock `Text` x=34, w=54, vertically centred, `Qt.formatTime(now,"hh:mm")` (24 h,
  zero-padded), `textColor`, 16 px Bold, h-centred.
- Status slot Item x=3, w=31, h=38: `AgentProgressRing` 24×24 centred (→ x 6.5, y 7;
  fractional, keep), `value = agentProgress(primaryAgent)`; overlaid glyph
  `agentMinimalGlyph`: `"!"` waitingApproval, `"?"` waitingQuestion, `"✓"` done, else none;
  `textColor`, 10 px Black(ExtraBold), centred.

**Compact layout** (`visible: !minimal`):
- `AgentLogo` 28×28 at x = 39 − 3 − 28 = **8**, vertically centred (y 5); image 22×22 at (11, 8).
- Clock `Text` x=39, w=58, centred, 16 px Bold, `textColor`.
- Ring 24×24 at x = 39+58+3 = **100**, y 7.
- `"✓"` centred on the ring, visible iff `isAgentDone`, `textColor`, 10 px Black.

MouseArea (whole pill): hover enabled, pointing-hand; click →
`agentFocusRequested(agent.id)`; `onEntered` → `hoverTarget = "agent"`.

### 3.8 `MusicMinimalPill` — `DI:694-719`
38×38, radius 19, `surfaceColor`, border 1 `strokeColor`. `AlbumArtFrame` 28×28 r14
centred (5, 5). MouseArea: pointing-hand; click → `appFocusRequested`; entered →
`hoverTarget = "music"`.

### 3.9 `idleContent` (no music, no agent) — `DI:807-848`
Fills `islandFrame`. `opacity = (musicPrimaryActive || agentActive) ? 0 : 1`, Behavior
**120 ms OutQuad**. Row centred, spacing 0:
| # | label | pad L/R | action |
|---|---|---|---|
| 1 | U+F023 (nf-fa-lock) | 11 / 12 | `lockClicked` |
| 2 | `hh:mm` | 0 / 0 | non-interactive (no hover, no cursor) |
| 3 | U+F011 (nf-fa-power_off) | 10 / 13 | `powerClicked` |

`idleRow.implicitWidth` = (11 + lockW + 12) + clockW + (10 + powerW + 13) with measured
`lockW = 12.59`, `clockW = 47.97`, `powerW = 13.59` → **120.16 px**.
Button heights 38. Island when idle: 120.16×38, radius 19 pill, `surfaceColor`, **no border**.
<!-- audit: was "116 (integer advances) / 113.2 (design advances)"; both wrong — the lock/power glyphs overflow their advance and Qt adds that overflow to implicitWidth -->

### 3.10 `compactMusic` (music, no agent, collapsed) — `DI:850-929`
Fills `islandFrame`. `opacity = musicPrimaryActive && !expanded ? 1 : 0`, Behavior
**140 ms OutQuad**. Row: margins L7 R7 T6 B6 (row box 106×26 at (7,6)), spacing 3:
- `AlbumArtFrame` 24×24 r8 at (7, 7).
- Clock `Text` x=34, w=54, 16 px Bold, `textColor`, h-centred, v-centred.
- `AudioSpectrum` 22×22 at (91, 8): barCount 6, active = mediaPlaying, barColor accent,
  peakColor textColor, quietColor mutedTextColor, minimumBarRatio 0.18, amplitudeGain
  2.05, heightRangeScale 0.65.
- `TapHandler` (left button, WithinBounds; enabled iff `musicPrimaryActive && !expanded &&
  !seeking`) → `appFocusRequested`. (The clock area is covered by `clockHotspot`, so a tap
  on the clock opens the calendar instead.)
- `HoverHandler` (enabled iff `musicPrimaryActive && !expanded`): hovered → `hoverTarget = "music"`.
Island: 120×38 pill, `surfaceColor`, border 1 stroke.

### 3.11 `expandedMusic` — `DI:931-1153`
Anchors fill `dual ? musicExpandedFrame : islandFrame` with **margins 13**.
`opacity = musicExpanded ? 1 : 0`, Behavior **150 ms OutQuad**. z per §2.5.
HoverHandler (enabled iff musicExpanded): hovered → `hoverTarget = "music"`.
Content (coordinates relative to the 13-px inset; at full size the content box is
366×150):
- **Top row** (y 0, h 72, full width, spacing 12):
  - `AlbumArtFrame` 70×70 r16 at (0, 0) (top-aligned, not centred).
  - Column x 82, width = W − 164 (202 at rest), v-centred in 72, spacing 4:
    title `displayTitle()` (= title or `"Unknown track"`) 18 px Bold `textColor` elide right;
    artist (hidden when empty) 13 px DemiBold `mutedTextColor` elide right.
  - `AudioSpectrum` 70×56 at x = W − 70 (296), y 8: barCount 8, minimumBarRatio 0.10,
    amplitudeGain 1.75, heightRangeScale 0.65, colours as compact.
  - TapHandler on the row (enabled iff `musicExpanded && !seeking`) → `appFocusRequested`.
- **Progress track** y = 72+14 = 86, h 5, full width, radius 2.5, `softFill`, clip.
  Fill: left-anchored, `width = max(5, W × progressRatio)`, radius 2.5, `accentColor`,
  visible iff `length > 0`. Width Behavior **180 ms OutCubic**, `enabled: !seeking`
  (instant while scrubbing).
- **Seek MouseArea**: track rect grown by 8 on every side (x −8…W+8, y 78…99), z 2,
  enabled iff length > 0, hover, pointing-hand cursor.
  - press: stop `seekPreviewReset`; `seeking = true`; preview = secondsForX(x).
  - move while pressed: update preview.
  - release: target = secondsForX; emit `seekRequested(target)`; restart `seekPreviewReset`
    (700 ms → `seeking = false`).
  - cancel: restart `seekPreviewReset`.
  - `secondsForX(x)` = `clamp(x/trackWidth, 0, 1) × length` clamped to [0, length]; 0 if
    length ≤ 0. x is in track coordinates (so pressing in the −8 margin clamps to 0).
- **Time labels** row y = 91+5 = **96**, two Texts each W/2 wide, 9 px Bold
  `mutedTextColor`: left = `trackTime(displayed)`; right (right-aligned) =
  `"-" + trackTime(max(0, length − displayed))`; both `"--:--"` when length ≤ 0.
  `trackTime(s)`: non-finite or < 0 → `"0:00"`; else `m:ss` with m unpadded
  (`floor(s)/60`), ss zero-padded (e.g. `63:05`).
- **Controls** row: bottom-anchored (y = H−44 = 106), height 44, spacing 18, centred
  horizontally (total 40+18+44+18+40 = 160 → x 103):
  previous U+F04AE (nf-md-skip_previous) → `previousClicked`;
  play/pause primary: `mediaPlaying ? U+F03E4 (pause) : U+F040A (play)` → `playPauseClicked`;
  next U+F04AD (nf-md-skip_next) → `nextClicked`. 40-px buttons v-centred (y +2).
Note: all children are anchored to the *animating* frame, so widths (and elision) reflow
every frame of the morph.

### 3.12 `musicExpandedFrame` (dual-mode music background) — `DI:761-805`
Rectangle top-anchored:
- `x = dual && musicExpanded ? 0 : 98` — Behavior 210 ms OutCubic
- `width = dual && musicExpanded ? 392 : 38` — Behavior 210 ms OutCubic
- `height = island.height` (follows animated height)
- `radius = musicExpanded ? 30 : height/2` — Behavior 210 ms OutCubic (chases while height animates, §0.3)
- `color surfaceColor`, border 1 `strokeColor`, clip
- `opacity = dual && musicExpanded ? 1 : 0` — Behavior **120 ms OutQuad**; `visible: opacity > 0.01`.
Effect: in dual mode the music panel grows out of the detached 38-px music pill.

### 3.13 `compactAgent` (agent present, collapsed) — `DI:1155-1225`
- `x = dual && expanded ? island.width/2 − attachedCenterOffset − 61 : 0` (no Behavior;
  follows animated width/offset), `y 0`, `height 38`, `width = agentCompactWidth` (132/136).
- `opacity = agentActive && !expanded ? 1 : 0`, Behavior **140 ms OutQuad**;
  `visible: opacity > 0.01`; `enabled` (input) = `agentActive && !expanded` (instant).
- HoverHandler (enabled iff `agentActive && !expanded && !dual`) → `hoverTarget = "agent"`.
- `AgentCompactPill` (left, v-centred), `agentData = primaryAgent`, `minimal = dual`.
- `MusicMinimalPill` at x = pill.width + 4 (= 98 in dual), v-centred, `visible: dual`.
- `dualHoverResolver` MouseArea (fills, z 10, enabled iff `dual && !expanded`, hover only,
  no buttons, pointing-hand cursor): on enter / move → `hoverTarget =
  dualCompactHoverTarget(mouseX)` = `x ≥ 94 ? "music" : "agent"`.

### 3.14 `expandedAgent` — `DI:1227-1427`
Fills `islandFrame` with margins 13. `opacity = agentExpanded ? 1 : 0`, Behavior
**150 ms OutQuad**. HoverHandler (enabled iff agentExpanded) → `hoverTarget = "agent"`.
Content box at full size: 422 × (H − 26).
- **Header** row y 0, h 58, spacing 12:
  - `AgentLogo` 50×50 at (0, 4); image 44×44 at (3, 7); fallback letter 12 px.
  - Column x 62, width = W − 132 (290), v-centred, spacing 4:
    title `agentDisplayTitle(primary)` 18 px Bold `textColor` elide;
    status `agentStatusText(primary)` 12 px DemiBold `mutedTextColor` elide.
  - Ring holder 50×50 at x 364, y 4: ring fills (r = 22, stroke 3); `"✓"` centred if done,
    `textColor`, **18 px Black**. (8 px slack remains on the right since 58 was subtracted.)
- **Session list** `ListView`: left/right full width, bottom-anchored, height
  `agentSessionListHeight`, clip, spacing 8, model = `AgentTaskModel` (§5),
  `boundsBehavior: StopAtBounds` (no overshoot), `interactive = contentHeight > height`
  (wheel/drag-flick scroll only when overflowing; Qt Flickable physics: wheel scrolls,
  touch/drag flicks with deceleration — use Qt defaults: flickDeceleration 1500 px/s²,
  maximumFlickVelocity 2500 px/s).
  List y = (H−26) − listHeight → 116 when n=1 (H=184), else 78.
  **Delegate** (each row): Rectangle W×42, radius 13, `softFill`, border 1 `strokeColor`.
  Row inside: margins L8 R8 (inner 406), spacing 7, children v-centred:
  - `AgentLogo` 23×23 (image 17×17).
  - Column width = 406 − 30 − (done ? 25 : 0), spacing 1: title 11 px Bold `textColor`
    elide; status 9 px DemiBold `mutedTextColor` elide.
  - Done slot 18 × 42 (only when done; Row skips invisible items): `"✓"` 13 px Black,
    colour `accentColor`, centred.
  - MouseArea fill: pointing-hand; click → `agentFocusRequested(row.id)`. (No hover visual.)
- **Scroll indicator**: Rectangle 3 px wide, radius 2, `mutedTextColor`,
  x = listRight − 4 − 3, `y = list.y + visibleArea.yPosition × list.height`,
  `height = max(24, list.height × visibleArea.heightRatio)`,
  `opacity = contentHeight > height ? 0.34 : 0`, Behavior 120 ms OutQuad.

### 3.15 Text helpers (`DI:166-295`)
```
agentField(a,k,fb)   = a && a[k] != null ? String(a[k]) : fb
agentDisplayTitle(a) = project if project≠"" && project≠"Session" else agentField(a,"source_label","Agent")
isAgentDone(a)       = lower(status) ∈ {completed, complete, done, success, succeeded}
agentStatusText(a):
  waitingApproval → "Waiting approval"
  waitingQuestion → "Waiting input"
  running         → current_tool or "Running"  (empty string → "Running")
  processing      → "Thinking"
  done            → "Done"
  else            → detail or "Working"   (missing field → "Working"; present-but-empty → "")
agentProgress(a) = null→0; done→1; Number(a.progress) non-finite (missing/"" coerces: missing→NaN→0.35, ""→0) → 0.35; else clamp 0..1
agentAccent(a)   = accentColor (always)
```
Note: `AgentTaskModel` rows do **not** carry `progress`, but delegates never draw a ring.

---

## 4. Hover, expansion and pointer logic (exact)

### 4.1 Elements
- `islandHover` (`DI:1445-1455`): HoverHandler on the island root with `margin: 10`
  (hover area = island rect grown by 10 px each side, clipped by the input region §1.1).
  On un-hover: `appHoverActive = false; hoverTarget = ""`.
- `sideHotspots` (`DI:1457-1491`, z 99, visible iff `!expanded && !calendarOpen`):
  - Left MouseArea: x 0 … `clockHotspot.x`, y 0 … 38, hover only (no buttons — clicks pass
    through). On **enter**: `appHoverActive = true`; `hoverTarget = dual ?
    dualCompactHoverTarget(mouseX) : (agentActive ? "agent" : musicActive ? "music" : "")`.
  - Right MouseArea: `clockHotspot.right` … island right, y 0 … 38. On **enter**:
    `appHoverActive = true`; `hoverTarget = dual ? "music" : agentActive ? "agent" : musicActive ? "music" : ""`.
- `clockHotspot` (`DI:1493-1515`, z 100, visible iff `!expanded && !calendarOpen`):
  x = `collapsedClockCenterX − 29`, width 58, height 38. Arrow cursor. On enter:
  `appHoverActive = false; hoverTarget = ""`. Click → `calendarClicked`.
  Positions: idle → centred on the island; single agent → 39…97; dual → 32…90; music → centred (31…89 of 120).

### 4.2 Resulting behaviour
- Expansion requires a pointer **enter** on a side hotspot (left/right of the clock) —
  merely moving onto the clock never expands, and entering the clock collapses
  (appHoverActive=false → expanded=false).
- While expanded, the hotspots are hidden, so the expanded view stays open while the
  pointer remains inside the island's hover rect (+10 px, within the input region).
  Content HoverHandlers keep `hoverTarget` consistent.
- Leaving the island hover rect → collapse.
- In **dual** collapsed state the left hotspot chooses agent vs music by x (≥ 94 → music),
  and `dualHoverResolver` keeps updating `hoverTarget` while moving (but only an enter on a
  side hotspot sets `appHoverActive`). Moving from the agent half to the music pill without
  leaving the left hotspot: resolver retargets → expanded view switches from agent to
  music immediately.
- Idle state: hotspots set `appHoverActive = true` with target `""` → no expansion; lock
  and power remain clickable (hotspots accept no buttons).
- **To verify live** (Qt 6.12 hover delivery): whether the idle lock/power buttons' hover
  fill appears while the z-99 side hotspot MouseArea is on top (Qt Quick delivers hover to
  the topmost accepting MouseArea; the button's `containsMouse` may stay false). Capture a
  reference screenshot "idle, pointer over lock" to settle it.
- Opening the calendar (`calendarOpen` → true) forces `appHoverActive = false`,
  `hoverTarget = ""` (`DI:303-306`).

### 4.3 Swipe to hide music (`DI:1517-1541`)
`DragHandler`, `target: null`, x axis enabled iff `!calendarOpen && musicActive && !seeking`,
y axis disabled. Default drag threshold (Qt `startDragDistance`, 10 px). On release
(`active` → false) with `hasMusicContent`: let dx = translation.x, dy = translation.y;
ignore if `|dx| < 34` or `|dx| < 1.4·|dy|`; `dx > 0` (swipe right) →
`mediaManuallyHidden = true`; `dx < 0` → `false`.
Quirk (preserve): once hidden, `musicActive` is false so the x axis is disabled → swipe-left
cannot restore; `mediaManuallyHidden` is reset only when `hasMusicContent` becomes false
(`DI:297-301`).

### 4.4 `keepPrimaryCollapsedForMusic` (`DI:308-320`, timer `DI:1429-1435`)
On every change of `musicExpanded`:
- not dual → `keep = false`, stop `musicFrameRelease`.
- dual → `keep = true`; if expanding stop the timer, else restart `musicFrameRelease`
  (**230 ms** single-shot → `keep = false`).

---

## 5. `AgentTaskModel` (`AgentTaskModel.qml:1-22`)
A ListModel synced from `agentSessions` via `Qt.callLater` on each change (and once at
creation). Purpose: keep delegates/scroll position stable. Algorithm (keyed by `id`):
```
fields = [id, source, source_label, title, project, status, current_tool, detail]  (all String(x ?? ""))
for i in 0..items.len:
    find first j ≥ i with row[j].id == items[i].id
    if none: insert at i
    else: if j ≠ i: move(j → i); set each changed field
remove rows ≥ items.len
```
Rust: diff by id → insert/move/update/remove with stable row identity; scroll offset is
preserved across updates (clamped to content).

---

## 6. Data sources and contracts

### 6.1 `PollCommand` semantics (`PollCommand.qml`)
Props: `command: string[]`, `interval` ms (default 1000), `active` (default true), `output`.
- A repeating timer runs while `active`, **fires immediately when started**
  (`triggeredOnStart`), then every `interval`.
- `refresh(queueIfRunning)`: no-op if inactive or empty command; if the process is still
  running: set a queued flag only when `queueIfRunning` is truthy (timer ticks pass nothing
  → tick is dropped); else start process.
- On exit: `text = trim(stdout)`; if exit 0 → `output = text`; always emit
  `updated(text, exitCode)`; if queued → clear flag and `refresh()`. stderr discarded.
Rust replacement: native services; keep the "coalesce while busy" semantics.

### 6.2 Media (`SH:3314-3327`, `SH:401-453`, `scripts/media-status.sh`)
Poll `sh scripts/media-status.sh` every 1000 ms while `mediaUiVisible && Mpris players > 0`
(`mediaUiVisible` = any bar expanded || control panel open; desktop: always true).
Also refreshed 180 ms after the MPRIS player list changes (`mediaFollowupRefresh`) and
after seeking; player list empty → `resetMediaState()`. Exit ≠ 0 → reset.
Output `key=value` lines: `cider`, `available`, `playing`, `title`, `artist` (multi-artist
joined with `, `), `player` (MPRIS bus suffix), `art_url`, `position` (s, float),
`length` (s, `mpris:length`/1e6, 3 decimals). Player selection: first Playing player, else
paused-with-metadata, else any-with-metadata, else paused, else first.
`updateMediaState` retention rule: if `available && !playing && no incoming
title/artist/art && same player && cached content` → keep cached title/artist/art and,
if incoming length/position ≤ 0, keep cached ones.
Position is only updated by the 1 s poll (no local extrapolation); the progress fill
smooths each jump with its 180 ms OutCubic Behavior.
Rust: MPRIS via zbus (same selection rule, PropertiesChanged + Seeked signals; position
read on the same 1 s cadence only while the expanded music view is visible, or
extrapolated — but the *displayed* value must step once per second to match).

### 6.3 Audio spectrum (`SH:3228-3276`, `scripts/audio-spectrum.sh`)
- Process runs while `mediaUiVisible && cavaAvailable && mediaAvailable && mediaPlaying`.
- `cava` config: framerate 24, bars 16, autosens 1, sensitivity 45, lower cutoff 50 Hz,
  higher 12000 Hz, raw ascii output, `ascii_max_range=100`, mono (average),
  noise_reduction 35, integral 88, gravity 92, ignore 2, monstercat 1.4.
- Line format: `v0;v1;…;v15;` — 16 integers 0..100, `;`-terminated, one line per frame
  (~24 Hz). Parsing: split on `;`, skip empty, `Number` → non-finite → 0, clamp ≥ 0;
  publish only if ≥ 1 value.
- Process stops (or exits non-zero) → values = `[]`.
- Rust: in-process PipeWire capture + cava-equivalent DSP (services spec); UI contract is
  `Vec<f32>` of 16 values in 0..100, ≈24 updates/s while playing, empty otherwise.

### 6.4 Agent sessions
#### 6.4.1 Transport (`SH:3200-3217`, `scripts/desktop-state.py`)
`StreamCommand` running `python3 scripts/desktop-state.py`; one JSON object per line
`{"type":"agent","data":{"active":bool,"count":int,"sessions":[…]}}` (other types
`system`, `wifi` belong to other specs). Lines only emitted when the encoded data differ
from the previous emission. On stream stop → reset (`active=false, count=0, sessions=[]`).
Parse failure → reset.

#### 6.4.2 `StreamCommand` semantics (`StreamCommand.qml`)
Process starts immediately; stdout split on `\n`, each line → `received(line)`; on exit →
`stopped()`; a 5000 ms repeating timer runs while not running → restart (first retry 5 s
after exit).

#### 6.4.3 Session snapshot (`agent_island.py:906-947`, `desktop-state.py:162-193`)
Agent observer ticks every **650 ms** while any session is active, else 1200 ms.
Per session the UI receives only:
| field | type | notes |
|---|---|---|
| `id` | string | session id (key) |
| `source` | `"codex"` \| `"claude"` | selects logo |
| `source_label` | `"Codex"` \| `"Claude Code"` | |
| `title` | string | unused by island UI |
| `project` | string | basename of cwd, `"Session"` if none |
| `status` | `idle`\|`processing`\|`running`\|`waitingApproval`\|`waitingQuestion`\|`completed` | idle sessions are excluded |
| `current_tool` | string | tool name when running |
| `detail` | string | free text |
| `progress` | float 0..1 | see below |
Ordering: by status priority desc (waitingApproval 5, waitingQuestion 4, running 3,
processing 2, completed 1, idle 0), then `last_activity` desc. `sessions[0]` is the
"primary" agent. Progress per status: processing 0.16 (prompt) / 0.55 (compact) /
0.66 (after tool) / ≥0.5 (notification), running 0.42 (codex process scan ≥0.38/0.46),
waiting 0.82, completed 1.0; **for running/processing the published value is
`min(0.94, max(base, 0.22 + 0.66 × ((now − last_activity) mod 12)/12))`** — a 12-s
sawtooth sampled at the 650-ms tick, so the ring advances in steps every 650 ms.
Completed/idle sessions disappear 20 s after last activity (900 s for other states;
immediately when their process dies without a pending request).

#### 6.4.4 Focus action (`agent-island-action.py`)
`focus <id>`: load the session from `~/.cache/hyprv/agent-island/state.json`
(`$HYPRV_AGENT_ISLAND_DIR`), then: (1) recorded Hyprland window address if it is still a
terminal/editor client (class keyword list `agent_island.py:23-37`) → `focuswindow
address:…` + kitty remote focus; (2) kitty `@ --to <listen_on> focus-window --match
id:<win>`; (3) score clients (terminal class +15, each session term ≥3 chars in title
+40 / class +10, base 5), take best ≥ 25. Rust: port natively (Hyprland socket).

---

## 7. `AudioSpectrum` component (`AudioSpectrum.qml`)
Canvas. Props: `values`, `barCount` (14), `active`, `barColor`, `peakColor`, `quietColor`,
`minimumBarRatio` (0.12), `amplitudeGain` (1.7), `heightRangeScale` (0.65), `phase` (0).
`hasEnergy` = any value > 0.

Sample for bar i:
```
if !active || !hasEnergy:                              // idle wave
    ratio = min + 0.34 · ((sin(phase + 0.72·i) + 1)/2)^1.45
else:
    src = min(len−1, floor(i·len / max(1,barCount)))
    raw = max(0, values[src])
    ratio = max(min, min(1, (min(1, raw/100))^0.62 · gain))
return min + (min(1, ratio) − min) · heightRangeScale
```
Paint (clear first):
```
count = max(1, barCount); gap = clamp(width/36, 2, 4)
barW = max(2, (width − gap·(count−1)) / count); cy = height/2; maxH = max(2, 0.98·height)
for i: r = sample(i); h = max(3, maxH·r); x = i·(barW+gap); y = cy − h/2
  peak = active && hasEnergy && r > 0.74
  fill = active ? (peak ? peakColor : barColor) : quietColor
  rounded rect (x, y, barW, h), corner radius barW/2 drawn with quadratic-Bézier corners
  (moveTo(x+r,y) … quadraticCurveTo(corner, …)) — not circular arcs
```
Compact (22×22, 6 bars): gap 2, barW 2, maxH 21.56. Expanded (70×56, 8 bars): gap 2,
barW 7, maxH 54.88. With heightRangeScale 0.65, the peak threshold 0.74 is reachable only
if `min + (1−min)·0.65 > 0.74` (compact: 0.713 → **never peak**; expanded: 0.685 → never
peak). So in practice bars are always `barColor` (playing) or `quietColor` (paused).
Repaint on any input change while visible.
**Idle-wave timer**: 66 ms repeating, running while `visible && (!active || !hasEnergy)`:
`phase += 0.28`, repaint. ⇒ a paused track shown in the compact island animates at
~15 Hz forever (quietColor wave); a playing track without cava data animates in
`barColor`. Parity requires the wave; performance rule allows it only because it is a
visible animation — stop it whenever the spectrum is not visible (opacity ≤ 0.01 or
calendar open, island hidden).

---

## 8. Transition timelines (frame-exact reference)
All size/position morphs: 210 ms OutCubic (Behavior, §0.3). Content cross-fades:
OutQuad with the per-layer durations below. All start on the same frame as the state
change unless noted.

| layer | in → out durations | curve |
|---|---|---|
| island width/height/attachedCenterOffset | 210 | OutCubic |
| islandFrame width / radius | 210 | OutCubic |
| musicExpandedFrame x/width/radius | 210 | OutCubic |
| musicExpandedFrame opacity | 120 | OutQuad |
| idleContent opacity | 120 | OutQuad |
| compactMusic opacity | 140 | OutQuad |
| compactAgent opacity | 140 | OutQuad |
| expandedMusic / expandedAgent opacity | 150 | OutQuad |
| progress fill width | 180 | OutCubic |
| buttons scale / hover fill | 120 | OutCubic |
| scroll indicator opacity | 120 | OutQuad |
| calendar progress / container size | 210 | OutCubic (+210 ms close timer) |

Key sequences:
1. **Idle → music compact** (music starts): width 120.16→120 <!-- audit: was 116→120 -->, idle fades out 120 ms while
   compactMusic fades in 140 ms; border 0→1 instant.
2. **Music compact → expanded** (hover a side hotspot): w 120→392, h 38→176, radius
   24→30 (clamped by h/2 until h ≥ 60), compactMusic fade out 140, expandedMusic fade in
   150; window height 58→820 instant.
3. **Expanded → collapsed**: reverse; window height back to 58 after 230 ms.
4. **Agent appears (no music)**: w → 132, offset 0 → −2 (animated), idle fades out 120,
   compactAgent fades in 140; frame border 1.
5. **Agent expand (single)**: frame w 132→448, h 38→agentExpandedHeight, radius 24→30,
   offset −2→0, compactAgent fade out 140, expandedAgent fade in 150.
6. **Dual collapsed** (agent + music): island 136 wide, offset +7; islandFrame 94 wide
   (agent background), detached music pill 38×38 at x 98 with its own background; 4-px gap
   shows the blurred desktop.
7. **Dual → music expanded**: `keep = true` instantly → islandFrame hidden instantly;
   agent minimal pill now draws its own background (surface + border) but fades out with
   compactAgent (140 ms); compactAgent.x follows `width/2 − offset − 61` so the agent
   clock stays centred while fading. musicExpandedFrame: opacity 0→1 (120), x 98→0,
   width 38→392, radius 19→30 (210); island w 136→392, h 38→176, offset 7→0.
8. **Dual music expanded → collapsed**: musicExpandedFrame fades out (120) while shrinking
   back toward x 98 / w 38 (210) above everything (z 12/13); after **230 ms** `keep = false`
   → islandFrame re-appears instantly (94 wide, at x 0) and z order returns to normal.
9. **Dual → agent expanded**: islandFrame 94→448 visible throughout; compactAgent
   (agent minimal + music pill) fades out 140; expandedAgent fades in 150; offset 7→0.
10. **Calendar**: §1.3.

---

## 9. Data contract (typed inputs for the Rust port)
```rust
struct IslandTheme { dark: bool }                // drives all §0.7 tokens
struct IslandInputs {
    now_hh_mm: String,                          // local time, minute ticks
    media: Option<MediaState>,
    spectrum: Vec<f32>,                         // 0..=100, 16 values @~24 Hz, empty when stopped
    agents: Vec<AgentSession>,                  // ordered, primary = [0]
    calendar_presented: bool, calendar_progress: f32, // host
}
struct MediaState {
    available: bool, playing: bool, title: String, artist: String,
    player: String, art_url: String,            // file:// or http(s)
    position_s: f64, length_s: f64,
}
struct AgentSession {
    id: String, source: String, source_label: String, title: String,
    project: String, status: AgentStatus, current_tool: String,
    detail: String, progress: Option<f64>,
}
enum IslandAction { Lock, Power, ToggleCalendar, Previous, PlayPause, Next,
    Seek(f64), FocusMediaApp, FocusAgent(String) }
```
Internal UI state to keep: `media_manually_hidden`, `seeking`, `seek_preview_s`,
`hover_target`, `app_hover_active`, `keep_primary_collapsed_for_music`, timers
`music_frame_release` 230 ms, `seek_preview_reset` 700 ms, host `island_collapse` 230 ms.

Open verification points (resolve with references):
- **idle-width**: resolved by measurement → 120.16 (see §3.9). <!-- audit -->
- Text line height at 16/18/13/12/11/10/9 px (hinted ascent/descent rounding) — affects
  vertical centring by ±0.5 px.
- Idle-button hover fill under the z-99 hotspot (§4.2).
- Zero-progress ring dot (§3.5).

---

## 10. Visual states worth a reference screenshot
Capture each in light **and** dark theme, island region (+ 12 px margin) of the bar
surface, at scale 1 and the user's real fractional scale.
1. Idle (no media, no agent), pointer away.
2. Idle, pointer over lock button; idle, pointer over power; idle, lock pressed (mid-press scale 0.96).
3. Music compact, playing with spectrum data (art loaded).
4. Music compact, paused (quiet wave, 2 frames 66 ms apart to check phase step).
5. Music compact, no art (fallback U+F001 glyph on softFill).
6. Music expanded, playing, mid-track (position e.g. 1:23 / 3:45), art loaded.
7. Music expanded, paused (play glyph), long CJK title (elision + fallback font), no artist.
8. Music expanded, length 0 (`--:--`, no fill), pointer hovering the next button (scale 1.06).
9. Music expanded while scrubbing (seeking preview).
10. Single agent compact: `running` (ring ~0.5), `waitingApproval`, `completed` (✓ in ring),
    codex logo and claude logo, unknown source (letter fallback).
11. Single agent expanded with n = 1 (184 tall), n = 3, n = 9 (scrolling, indicator visible,
    scrolled to middle); a done row (accent ✓ slot).
12. Dual collapsed (agent minimal with `!`, `?`, `✓`, and no glyph) + detached music pill.
13. Dual → music expanded (final state), and the mid-morph frames at t = 60, 105, 150 ms.
14. Dual → agent expanded.
15. Morph frames for single music expand/collapse at t = 0, 30, 70, 105, 140, 210 ms.
16. Calendar open (final) and calendar opening at t = 105 ms (island half-faded, container growing).
17. Music manually hidden by swipe (shows idle/agent layout while media plays).
