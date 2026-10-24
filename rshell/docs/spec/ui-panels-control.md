# UI spec — Control panel

Read `ui-panels.md` first (tokens §1.3, helpers §1.2, fonts §1.4, `AnimatedGlassPanel` §2.1,
`WifiActionChip` §2.4). Sources: `ControlPanelPopup.qml`, `ControlPanelSplitTile.qml`,
`ControlPanelToggle.qml`, `ControlPanelSlider.qml`, `ControlPanelMediaCard.qml`,
`ControlPanelBluetoothDetails.qml`, `ControlPanelWifiDetails.qml`, `WindowsVmControl.qml`,
`WindowsVmController.qml`, `ViewflowTargetControl.qml`, `DeskflowTargetControl.qml`,
`MihomoControlPanel.qml`, `MihomoControlTile.qml`.

Reachability today: **main page** and **bluetooth page** are reachable. The **mihomo page**
exists but nothing calls `toggleMihomoExpanded()` (and `MihomoControlTile` is not instantiated);
`ControlPanelWifiDetails` and `DeskflowTargetControl` are not instantiated anywhere;
`powerExpanded`/`togglePowerExpanded`/`confirmEgpuAction` (`ControlPanelPopup.qml:20,327,408`)
have no UI caller. They are specified for completeness (lower priority).

---

## 1. `ControlPanelPopup` — window, card, state machine

### 1.1 Surface
`PanelWindow` (`ControlPanelPopup.qml:739-756`): namespace `shell:hyprv-control-panel`, layer
Overlay, anchored top+left+right+bottom (covers the whole output, including the bar),
`exclusiveZone −1`, transparent, `focusable` while visible, keyboard OnDemand while visible,
screen = `parentWindow.screen` (the bar window that opened it, or the primary bar for IPC).
Initially unmapped (`visible: false`).

Children (z-order bottom→top):
1. Full-surface `MouseArea` (left/right/middle) → `closePopup()` (`:806-810`) — click-outside
   dismiss.
2. Full-surface `FocusScope` focused while visible, `Esc` → `closePopup()` (`:812-819`).
3. `popupCard: AnimatedGlassPanel` (`:821-920`) containing a click-absorbing `MouseArea`
   (all buttons, no action) and the page.

### 1.2 Card geometry and chrome
Constants (`:22-31`): `popupScreenMargin 10`, `popupPadding 0`, `columnSpacing 10`,
`cardSpacing 10`, `moduleSize 62`, `smallModuleSize 48` (unused), `columnWidth = 62·2+10 = 134`,
`popupWidth = currentPage == "main" ? 0·2 + 134·2 + 10 = 278 : 384`.

`popupCard` (`:821-841`):
- `width = popupWidth` (278 main / 384 other pages), `fullPanelHeight = page.implicitHeight`.
- `fillColor` = main ? transparent : `glassFill`; `strokeColor` = main ? transparent :
  `glassStroke`; `surfaceOpacity` = main ? 0 : 0.82; `shadowColor` transparent; radius 19
  (default). **The main page has no panel background at all** — only its tiles are drawn.
- Timing (non-bluetooth / bluetooth): openRevealPause 20/85, openRevealDuration 200/280,
  openContentDelay 20/60, openFadeDuration 140/180, openSlideDuration 180/220,
  openContentOffset −8/−10, closeRevealPause 30/35, closeRevealDuration 180/210,
  closeFadeDuration 90/110, closeSlideDuration 150/170, closeContentOffset −8 (both).
  These bind to `currentPage` at the moment each animation runs.

Position (`updatePopupPosition`, `:765-776`), surface-local coordinates, W = output width:
- `barBottom` = if `sourceItem` (opened from the bar icon): `round(sourceItem.mapToGlobal(0,
  sourceItem.height).y − screen.y)` — the bottom edge of the 38 px-high bar trigger, ≈ 48 with
  the bar fully revealed (trigger row at top margin 10). If opened by IPC (no source):
  `max(0, parentWindow.exclusiveZone || 58)` = 58 when the bar is fully shown.
- `card.x = max(10, W − card.width − 10)` (right-aligned, 10 px margin).
- `card.y = barBottom + 10` (≈ 58 from bar icon, 68 from IPC).
- Recomputed on: open, window width/height change, `positionTimer` (0 ms) after content height
  changes, open/close transitions and page switches.

Card height: main page = 422 (see §1.4); bluetooth page = content dependent (§6).

### 1.3 State machine
State (`:8-20`): `sourceItem`, `parentWindow`, `popupRequested`, `animatingClose`,
`openAnimationPending`, `currentPage ∈ {main, bluetooth, mihomo}` (`wifi` exists as a name but
is never set), `pendingPage`.

- **openFor(source, window)** (`:58-76`, bar icon): set source/window, requested = true,
  closing = false, `resetExpandedState()` (page = main, pendingPage = "", …), schedule
  reposition. If already mapped (e.g. mid-close): `prepareOpenAnimation()`, pending = true,
  start 16 ms open timer, reposition. Else map the window.
- **toggleFor(source, window)** (`:119-125`): mapped and same source+window → `closePopup()`,
  else `openFor`.
- **toggleCentered(window)** (`:127-141`, IPC): mapped → `closePopup()`; else source = null,
  window, requested = true, closing = false, page main, map.
- **On map** (`onVisibleChanged` true, `:778-793`): reposition; focus the FocusScope; request
  fresh data: brightness probe, control-panel status poll, media status, Wi-Fi status (no-op),
  bluetooth sync; if not closing: pending = true, `prepareOpenAnimation()` (reveal 0,
  opacity 0), start `popupOpenTimer`.
- **popupOpenTimer** (16 ms, `:467-484`): abort if unmapped/not requested/closing; if page
  implicitHeight ≤ 0 retry in 16 ms; else reposition and `playOpenAnimation()`.
  Open animation finished → pending = false, reposition (`:869-875`).
- **closePopup()** (`:78-92`): if (not requested and not closing) or unmapped → clear flags,
  return. If closing → return. Else requested = false, closing = true, pendingPage = "",
  `playCloseAnimation()`.
- **closeAnimationFinished** (`:877-889`): if `pendingPage` non-empty and still requested and
  mapped and not closing → page switch (below). Else if closing and not requested → closing =
  false, **unmap**.
- **On unmap** (`:794-803`): flags cleared, page = main, timers/animations stopped,
  `resetAnimationState()`, source/window = null.
- **Page switch** `switchPageWithAnimation(target)` (`:104-117`): no-op if same; if unmapped or
  closing → set page directly. Else pendingPage = target, stop open timer & animations,
  `playCloseAnimation()` (with the *old* page's close timing). On finish: `currentPage = target`
  (width/fill/stroke/surfaceOpacity switch instantly; card x re-right-aligns via 0 ms reposition),
  `animateCurrentPageOpen()` (`:94-102`): pending = true, `prepareOpenAnimation()`, reposition,
  16 ms timer → open animation with the *new* page's timing.
- **Height change while open** (`onFullPanelHeightChanged`, `:843-867`): if open pending →
  reposition, restart open timer if not running. If mapped, not closing, and no animation
  running → snap `revealHeight = fullPanelHeight`, opacity 1, offset 0 (instant resize, no
  animation). If an animation is running just reposition.
- **Immediate unmap paths** (no close animation): Wi-Fi tile content click (§1.5) and the
  Windows VM toggle (`windowsVmController.onActionStarting`, `:40-49`).

Open timeline, main page: t0 map → t≈16 ms start → 20 ms pause → reveal 0→422 over 200 ms
OutCubic (top-down wipe of the tiles); opacity 0→1 from t+40 ms over 140 ms OutQuad. Ends at
≈ 236 ms after map. Close: opacity 1→0 90 ms InQuad; reveal holds 30 ms then →2 px over
180 ms InCubic; unmap at 210 ms (a 2-px sliver of the top tile row is the last frame).

### 1.4 Main page layout (`mainPageComponent`, `:518-737`)
Card-local coordinates (card is 278 × 422). Column spacing 10.

```
y=0   Row (spacing 10)
      ├ left Column x=0 w=134 (spacing 10)
      │   y=0    Wi-Fi SplitTile        134×62
      │   y=72   Bluetooth SplitTile    134×62
      │   y=144  WindowsVmControl  x=0  62×62 │ ViewflowTargetControl x=72 62×62
      │   y=216  Screen-record toggle x=0 62×62 │ Prevent-sleep toggle x=72 62×62
      └ right Column x=144 w=134 (spacing 10)
          y=0    MediaCard              134×134   (moduleSize·2 + spacing)
          y=144  Dark-mode toggle x=144 │ DND toggle x=216          (62×62 each)
          y=216  Demo-mode toggle x=144 │ Power-profile toggle x=216
y=288 Grid (2 columns, spacing 10)
      (0,288)   Brightness slider 134×62   (144,288) Volume slider 134×62
      (0,360)   Chassis 1 fan slider       (144,360) Chassis 2 fan slider
height = 278 + 10 + 134 = 422
```
Half-width items: `(134 − 10)/2 = 62`.

Per-item configuration (all with `shellRoot`):

| item | props | src |
|---|---|---|
| Wi-Fi SplitTile | icon = active ? U+F0928 : U+F092D; title `"WIFI"`; subtitle: radio off → `"Turned off"`, connected → SSID or `"Connected"`, else `"Not connected"`; `active = wifiRadioEnabled`; `expanded = (page=="wifi")` (never); left → `toggleWifiEnabled()`; right → `toggleWifiExpanded()` | `:533-544`, `:342-357` |
| Bluetooth SplitTile | icon = active ? U+F00AF : U+F00B2; title = name of first connected device with a name, else `"Bluetooth"`; subtitle: no adapter → `"No adapter"`, off → `"Turned off"`, connected>1 → `"N connected"`, 1 → `"Connected"`, discovering → `"Scanning"`, paired>0 → `"N paired"`, else `"Ready"`; `active = bluetoothEnabled`; left → `bluetoothSetPower(!enabled)`; right → `toggleBluetoothExpanded()` | `:546-557`, `:359-406` |
| WindowsVmControl | §4 | `:563-568` |
| ViewflowTargetControl | §5 | `:570-574` |
| Screen record | icon U+F0EC3, iconOnly, label "Screen\nrecord" (not shown), active = `screenRecording`, click → `toggleRecording()` | `:581-590` |
| Prevent sleep | icon = active ? U+F0F4 : U+F236, iconOnly, active = `preventSleepEnabled`, click → `togglePreventSleep()` | `:592-601` |
| MediaCard | available/playing/title/subtitle(artist)/playerName/artUrl from shell media state; prev/play-pause/next → `handleMediaAction(["playerctl", …])` | `:609-622` |
| Dark mode | `iconSource = assets/control-panel/light-dark.svg`, iconOnly, active = `darkMode`, click → `toggleTheme()` | `:628-637` |
| DND | icon U+F009B, iconOnly, active = `dndEnabled`, click → `toggleDnd()` | `:639-648` |
| Demo mode | icon U+F0379, iconOnly, active = `demoModeEnabled`, click → `toggleDemoMode()` | `:655-663` |
| Power profile | icon by profile: `power-saver` U+F0F86, `performance` U+F04C5, else U+F0F85; iconOnly; **active always false**; click → `cyclePowerProfile()` | `:665-673`, `:297-305` |
| Brightness slider | icon U+F00DF, value = `brightnessPercent`; change → `applyBrightnessPercent(clamp)`; finished → `finishBrightnessPercent(v)` | `:683-695` |
| Volume slider | icon = shell `volumeIcon` (unavailable U+F026, muted U+F466, ≤20 U+F026, ≤50 U+F027, else U+F028 — `shell.qml:369-384`), `iconClickable`, value = `audioVolumePercent`; change → `setAudioVolumePercent`; icon click → `toggleAudioMute()` | `:697-708` |
| Chassis 1 slider | `enabled = fanControlAvailable`, `opacity = enabled ? 1 : 0.48`, icon U+F0210, value = `chassisFanPercent`; change → `applyFanPercent("chassis", v)` | `:710-721` |
| Chassis 2 slider | same with `pumpFanPercent`, channel `"pump"` | `:723-734` |

Slider `label`s ("Brightness", "Chassis 1 Limit · N RPM", …) and `accentColor`s are set but
**never rendered** by `ControlPanelSlider` (see §2.3) — the RPM values are not visible anywhere.

### 1.5 Actions (`:143-340`)
All "optimistic" flips update shell state immediately, then the backend refreshes.
`controlPanelRefreshTimer` = 400 ms one-shot → `refreshControlPanelStatus()` (`:421-429`).

| action | effect |
|---|---|
| `toggleWifiEnabled()` | `wifiRadioEnabled = wifiEnabled = !wifiRadioEnabled`; `wifiSetRadio(new)` (Wi-Fi service; sets `wifiActionBusy`, message "Turning Wi-Fi on/off..."); refresh 400 ms |
| `toggleWifiExpanded()` | requested = false, closing = false, **unmap immediately**; `shellRoot.openWifiPanel()` → opens the Wi-Fi popup anchored at the primary bar's Wi-Fi icon (`ui-panels-wifi.md`) |
| `toggleBluetoothExpanded()` | on bluetooth page → `showMainPage()`; else bluetooth sync then `switchPageWithAnimation("bluetooth")` |
| `toggleBluetoothEnabled()` | `bluetoothSetPower(!bluetoothEnabled)` |
| `toggleDnd()` | `dndEnabled = !dndEnabled`; run `swaync-client -d -sw`; refresh |
| `toggleTheme()` | `darkMode = !darkMode`; run `scripts/toggle-theme.sh` |
| `toggleRecording()` | `screenRecording = !screenRecording`; on → run `~/.config/hypr/scripts/record-script.sh --fullscreen-sound`; off → `sh -c "pkill -INT wf-recorder \|\| pkill -INT wl-screenrec \|\| pkill -INT gpu-screen-recorder"`; refresh |
| `togglePreventSleep()` | flip; run `scripts/prevent-sleep.sh toggle`; refresh |
| `toggleDemoMode()` | flip; run `scripts/demo-mode.sh toggle`; refresh |
| `cyclePowerProfile()` | order `[power-saver, balanced, performance]`, next index (unknown current → `balanced`); `powerProfile = p`; run `powerprofilesctl set p`; refresh 400 ms; power-profile resync 350 ms |
| `handleMediaAction(cmd)` | ignored if `!mediaAvailable`; play-pause flips `mediaPlaying` optimistically; run `playerctl previous\|play-pause\|next`; media refresh 250 ms |
| `setAudioVolumePercent(v)` | `audioVolumePercent = clamp(v)`; run `hyprctl eval 'require("hypr.controls").volume("set",v)'`; audio refresh scheduled |
| `toggleAudioMute()` | `audioAvailable = true; audioMuted = !audioMuted`; `hyprctl eval 'require("hypr.controls").volume("toggle")'` |
| `setBrightnessPercent(v)` | `applyBrightnessPercent(clamp(v))` — local snap to 2 %, serialised `hyprctl eval 'require("hypr.controls").brightness_set(v, false, true)'` (`shell.qml:2780-2808`) |
| finish brightness | `finishBrightnessPercent(v)` → `brightness_finish(v)` |
| `setFanPercent(ch, v)` | `applyFanPercent(ch, clamp(v))` (no-op if fans unavailable; debounced write, `shell.qml:2814-2834`) |
| `confirmEgpuAction()` (no UI) | two-step: first call arms for 3000 ms, second runs `scripts/egpu-disconnect.sh` |

Commands that spawn processes are the *current* implementation; the Rust port should call the
equivalent service APIs (spec-sys) but keep the optimistic UI semantics.

### 1.6 Data contract (popup)
```rust
struct ControlPanelInputs {
    dark_mode: bool,
    wifi_radio_enabled: bool, wifi_connected: bool, wifi_ssid: String,
    bluetooth_present: bool, bluetooth_enabled: bool, bluetooth_discovering: bool,
    bluetooth_devices: Vec<BtDevice>,               // see §6
    screen_recording: bool, prevent_sleep: bool, demo_mode: bool, dnd: bool,
    power_profile: String,                          // "power-saver" | "balanced" | "performance"
    media: MediaState { available, playing, title, artist, player_name, art_url },
    brightness_percent: u8,                         // 0..100, multiples of 2 after local edits
    audio_available: bool, audio_muted: bool, audio_volume_percent: u8,
    fan_available: bool, chassis_fan_percent: u8, pump_fan_percent: u8,
    chassis_fan_rpm: u32, pump_fan_rpm: u32,        // not rendered
    windows_vm: VmState { running: bool, busy: bool },
    viewflow_target: Target /* Windows|MacOs */, viewflow_busy: bool,
}
```
Freshness while open (current impl): status poll 1500 ms (`shell.qml:3378`), VM 2000 ms,
viewflow 2000 ms; replace with events.

---

## 2. Tile primitives

### 2.1 `ControlPanelSplitTile` (`ControlPanelSplitTile.qml`)
Two-zone tile: icon well (left click zone) + text (right click zone).
Props: `icon, title, subtitle, active, destructive=false, leftEnabled=true, rightEnabled=true,
expandIndicatorVisible=false, expanded=false, iconAreaWidth = height` (`:6-16`);
`implicitHeight 72` (callers set 62). Signals `leftClicked`, `rightClicked`.

Derived (`:23-42`), values for 134×62 in brackets:
- `accentColor` = destructive ? critical : launchColor.
- `inset = max(6, round(h·0.14))` [9]; `iconWellWidth = max(28, iconAreaWidth − 2·inset)` [44];
  `iconWellHeight = max(28, h − 2·inset)` [44]; `iconWellRadius 11`;
  `iconPixelSize = clamp(min(wellW, wellH)·0.48, 16, 20)` [20]; `iconTextGap 9`;
  `contentStartX = inset + wellW + 9` [62]; `frameRadius 19`.

Tree:
1. `frame` Rectangle fill, radius 19, colour active ? `activeFill` : `cardFill`, border 1 px
   active ? `activeStroke` : `cardStroke` (`:44-53`). No hover effect on the frame.
2. `iconWell` at (inset, inset) size well, radius 11, colour: icon-zone pressed →
   `darker(iconWellFill, 1.08)`; hovered → `lighter(…, 1.04)` (no-op on white, §1.2);
   else `iconWellFill` (active/inactive variant) (`:55-67`).
3. Icon Text centred on the well: `icon`, colour active ? `#fff7ef` : primaryText, iconFont
   `iconPixelSize` Bold, Native (`:69-77`).
4. Text block Item: x = contentStartX, right margin 10, vertically centred, height =
   column implicit height (`:79-133`):
   - Column (right edge = chevron.left − 6 if chevron visible else parent right), spacing 2 if
     subtitle non-empty:
     - title: width = column width, 11 px Bold baseFont, colour active ? `#ffffff` : primaryText,
       ElideRight.
     - subtitle (visible iff non-empty): 9 px Medium, colour active ? white α0.82 :
       `mutedText`, ElideRight.
   - chevron (visible iff `expandIndicatorVisible`): right-aligned, vcentred, `expanded ?
     U+F077 : U+F078`, 11 px Bold iconFont, colour active ? white : mutedText.
   For 134×62 with subtitle: block = 15 + 2 + 12 = 29 px → title top y = 16, subtitle
   y = 33, text width = 134 − 62 − 10 = 62 px. <!-- audit: heights per measured metrics (11 px→15, 9 px→12), was 16/13 -->
   (Qt anchor centring, ui-bar-layout.md §1 rule 2: hcenter(62)=31 − hcenter(29)=15 → 16.)
5. `iconArea` MouseArea x 0, w = contentStartX, full height, enabled = leftEnabled, hover,
   pointer iff enabled, click → `leftClicked` (`:135-146`).
6. `contentArea` MouseArea x = contentStartX to right edge, enabled = rightEnabled, hover,
   pointer iff enabled, click → `rightClicked` (`:148-159`).
No animations.

### 2.2 `ControlPanelToggle` (`ControlPanelToggle.qml`)
Square button. Props: `icon, iconSource (url), label, iconOnly=false, active=false,
destructive=false, toggleEnabled=true` (`:6-13`). Signal `clicked`.
Root is a Rectangle: radius 19, default height 72 (callers 62), border 1 px, antialiased.
- `activeColor` = destructive ? critical : launchColor; `activeFill` = activeColor α0.28 D /
  0.22 L; `activeStroke` = activeColor α0.42 D / 0.28 L; inactive = cardFill / cardStroke.
- **Fill** (`:37-49`): base = active ? activeFill : inactiveFill; `!toggleEnabled` → base RGB with
  alpha **0.48** (replaced); pressed → `darker(base, 1.06)`; hovered → `lighter(base, 1.04)`;
  else base.
- **Border** (`:51-59`): disabled → inactiveStroke; hovered → active ? `lighter(activeStroke,
  1.08)` : `lighter(inactiveStroke, 1.04)`; else active ? activeStroke : inactiveStroke.
- `opacity` = toggleEnabled ? 1 : 0.56 (`:60`).
- Sizes (`:30-33`), [values for 62×62 iconOnly]:
  `iconBoxSize = iconOnly ? clamp(min(w,h) − 12, 22, 30) : clamp(h − 16, 28, 34)` [30];
  `iconRadius = round(iconBoxSize·0.34)` [10];
  `iconPixelSize = iconOnly ? clamp(box·0.5, 15, 18) : clamp(box·0.46, 15, 18)` [15];
  `iconImageSize = iconOnly ? clamp(box·0.74, 18, 22) : clamp(box·0.64, 18, 20)` [22].
- Content Column centred, spacing iconOnly ? 0 : 6 (`:63-123`):
  - icon box `iconBoxSize²`, radius iconRadius, colour iconOnly ? transparent : (pressed →
    `darker(iconWellFill,1.05)`, hover → `lighter(…,1.03)`, else iconWellFill).
    - If `iconSource` set: Image `iconImageSize²` centred, PreserveAspectFit, smooth+mipmap,
      rasterised at `sourceSize = max(32, round(size·2))` (44 px) then scaled; colourised by
      MultiEffect `colorization 1, colorizationColor = iconColor` (= solid tint keeping the SVG
      alpha).
    - else glyph Text centred: `icon`, colour `iconColor`, iconFont `iconPixelSize` Bold.
    - `iconColor` = active ? `#ffffff` : primaryText.
  - label Text (visible iff `!iconOnly && label != ""`): width `max(40, w − 14)`, 10 px Bold,
    colour active ? white : primaryText, centred, WordWrap. **All control-panel toggles are
    iconOnly, so labels are never drawn.**
- MouseArea fill, enabled = toggleEnabled, hover, pointer iff enabled → `clicked`.
No animations (all colour changes are instant).

### 2.3 `ControlPanelSlider` (`ControlPanelSlider.qml`)
Full-tile horizontal slider; the fill *is* the tile.
Props: `icon, label (unused), value=50, accentColor (unused), iconClickable=false,
iconLeftInset=−1, adjusting=false, previewValue=0` (`:6-14`). Signals
`valueChangeRequested(real)`, `adjustmentFinished(real)`, `iconClicked()`. Height 62.

Derived (`:23-38`): `displayValue = adjusting ? previewValue : value`; `clampedValue =
clamp(displayValue, 0, 100)`; `fillWidth = w·clampedValue/100`; `frameRadius 19`;
`iconCenterX = min(w − h/2, max(h/2, w·0.24))` [134 px: 32.16]; `iconPixelSize =
clamp(h·0.34, 18, 22)` [21.08]; `restingFill = cardFill`; `progressFill = launchColor α0.28 D /
0.22 L` (always launchColor, ignores `accentColor`); `iconSolidColor = progressFill`;
`frameStroke = cardStroke`.

Tree (all inside `frame`, `:40-167`):
1. `frame` Rectangle fill, radius 19, colour restingFill (no border).
2. Progress: clip box [0, fillWidth] × h containing a full-width rounded rect (radius 19,
   colour progressFill) → left corners rounded, right edge cut straight at `fillWidth`.
3. Icon over the filled part: clip box [0, fillWidth]; glyph `icon`, colour `#ffffff`, iconFont
   `iconPixelSize` Bold, x = `iconLeftInset ≥ 0 ? iconLeftInset : iconCenterX − glyphW/2`,
   vertically centred.
4. Icon over the empty part: clip box [fillWidth, w]; same glyph at the same absolute x, colour
   `iconSolidColor` (translucent accent). → the glyph is two-tone, split exactly at the fill edge.
5. Border overlay: transparent rect radius 19, 1 px `frameStroke`.
6. `trackArea` MouseArea fill, hover, pointer cursor:
   - `computeValue(x) = round(clamp(x/w, 0, 1)·100)`.
   - press: if `iconClickable` and inside icon hit box (centre = `iconLeftInset ≥ 0 ?
     iconLeftInset + iconPixelSize/2 : iconCenterX`, half-size `max(18, h·0.3)` in x and y
     around (centre, h/2)) → `iconClicked()` and stop. Else `adjusting = true`, preview =
     computeValue, emit `valueChangeRequested(preview)`.
   - move while pressed & adjusting: update preview, emit.
   - release (if adjusting): preview = computeValue, emit, `adjusting = false`, emit
     `adjustmentFinished(preview)`. Cancel: same finish without recompute.
   - No wheel handling, no keyboard, no animations; the fill follows the pointer instantly.
`enabled: false` on the item (fans unavailable) disables the MouseArea; callers also set
`opacity 0.48`.

### 2.4 `ControlPanelMediaCard` (`ControlPanelMediaCard.qml`)
Props: `available, playing, title, subtitle (unused in drawing), playerName (unused), artUrl`.
Signals `previousClicked, playPauseClicked, nextClicked`.
Root Rectangle (`:19-27`): radius 19, colour `cardStrongFill`, border 1 px `cardStroke`, clip,
`implicitHeight 160` (caller sets 134×134).
Constants (`:34-52`), values for 134×134: `outerPadding 9`, `artSize = clamp(h·0.42, 42, 44)`
[44], `artRadius = min(11, art/2)` [11], `titlePixelSize 12`, `controlScale 1.7`,
`secondaryGlyphPixelSize = round(15·1.7)` [26], `primaryGlyphPixelSize = round(18·1.7)` [31],
`primaryGlyphBottomMargin 3`, `controlRowHeight = 31 + 3` [34],
`secondaryGlyphBottomMargin = max(0, round(3 + (31−26)/2))` [6], `secondaryButtonSize = 26+8`
[34], `primaryButtonSize = 31+10` [41], `controlSpacing 8`.

Tree:
1. `topContent` Column at (9, 9), width 116, spacing 13 (`:95-170`):
   - `artFrame` 44×44, radius 11, colour `artFill` (`#fff` α0.08 D / α0.34 L), clip:
     - Image fill (`available && artUrl` else none), PreserveAspectCrop, async, smooth+mipmap,
       masked to a radius-11 rounded rect (MultiEffect mask, threshold 0.5, spread 1), visible
       when loaded.
     - Placeholder when no image: glyph U+F001, colour `mutedText`, iconFont
       `round(44·0.42)=18` px Bold, centred.
   - title Text at y = 9+44+13 = 66, x = 9 + 2 (leftMargin 2), width 116 (so it overhangs the
     column by 2 px; card clips), text = available ? (title or `"Unknown track"`) : `"Idle"`,
     12 px Bold baseFont primaryText, ElideRight.
2. Controls: Item centred horizontally, bottom = card bottom, size = row width × 34 (`:172-204`).
   Row centred, spacing 8: total 34+8+41+8+34 = 125 → x = 4.5 in the card, y = 100.
   `MediaButton` (`:53-93`): transparent, no border, width primary ? 41 : 34, height 34,
   opacity `buttonEnabled ? 1 : 0.6`; glyph Text horizontally centred, bottom-anchored with
   bottomMargin 3 (primary) / 6 (secondary); colour **`#000000` for the primary button in both
   themes**, primaryText for secondary; size 31 / 26 px Bold iconFont. MouseArea fill, enabled
   iff buttonEnabled, pointer → `tapped`. No hover visuals.
   - previous: U+F04AE; play-pause: playing ? U+F03E4 (pause) : U+F040A (play); next: U+F04AD.
   - all `buttonEnabled = available`.

Data: `MediaState { available, playing, title, art_url }` (artist/player not drawn).

---

## 3. Bluetooth page — `ControlPanelBluetoothDetails` (`ControlPanelBluetoothDetails.qml`)
Hosted as `Column { width = 384; spacing 10; ControlPanelBluetoothDetails { width: parent.width;
useExternalPanelBackground: true; onCloseRequested: showMainPage() } }`
(`ControlPanelPopup.qml:486-500`). Card chrome = glassFill/glassStroke at 0.82 (§1.2).

### 3.1 Root
`useExternalPanelBackground` → root transparent, no border (otherwise `panelFill`/`panelStroke`,
radius 19) (`:115-119`). `implicitHeight = contentColumn.implicitHeight + 20`.
Constants: `pageSpacing 10, panelPadding 10, innerPadding 10, innerRadius 9,
maxDeviceListHeight 420` (`:37-42`). Cards use `panelColor()` (α × 0.82).

Extra tokens: power toggle on fill `#7f99bd` α0.94 D / `#8faad1` α0.90 L; on stroke `#98b2d8`
α0.84 D / `#7f9ec8` α0.62 L; off fill primaryText α0.16 D / 0.14 L; off stroke primaryText
α0.18 D / 0.12 L; knob `#f7f7fa` D / `#ffffff` L; knob stroke `#000` α0.18 D / 0.10 L
(`:31-36`).

### 3.2 Layout (`contentColumn` at (10,10), width 364, spacing 10, `:194-585`)
1. **Header** Item 364×40 (`:203-291`):
   - Row at left, vcentred, spacing 12: title `"Bluetooth"` 17 px Bold baseFont primaryText;
     power switch 48×28, radius 14, colour on/off fill, 1 px on/off stroke, opacity
     `(!present || busy) ? 0.58 : 1`, `Behavior on color` and `on border.color`:
     ColorAnimation 140 ms (linear). Knob 22×22, radius 11, x = enabled ? 48−22−3 = 23 : 3,
     vcentred, knob colour + 1 px knob stroke, `Behavior on x`: 160 ms **InOutQuad**.
     MouseArea fill, enabled iff present && !busy, pointer → `bluetoothSetPower(!enabled)`.
   - Close chip right-aligned (x = 364 − w), vcentred: WifiActionChip label `"Close"`,
     minWidth 76, cornerRadius 9, fill primaryText α0.08 D / 0.12 L, stroke cardStroke →
     `closeRequested()` → `showMainPage()` (animated page switch).
2. **Status card** (`:293-374`): width 364, height = body + 24 (body at top 10 → 14 px bottom
   padding), radius 9, colour `panelColor(connected>0 ? accentFill : cardStrongFill)`, 1 px
   `connected>0 ? accentStroke : cardStroke`. Body (margins 10, height = max(info, icon, 0)):
   - left icon: `enabled ? U+F00AF : U+F00B2`, 24 px Bold iconFont primaryText, vcentred.
   - right pill (WifiActionChip, disabled ⇒ opacity 0.56 and disabled colours): label
     `statusLabel` (`"No BT" | "Off" | "Online" | "Scan" | "Ready"`, `:43-57`), minWidth 72,
     cornerRadius 9, fill cardFill, fg = connected>0 ? accent : primaryText, stroke =
     connected>0 ? accent α0.18 : cardStroke (stroke/fill are overridden by the disabled
     colours; only text colour shows).
   - info Column between icon+10 and pill−10, vcentred, spacing 4: summary 14 px Bold primary
     ElideRight (`connectionSummary`, `:58-78`); detail 12 px (weight 400) mutedText ElideRight
     (`detailText`, `:79-98`). Typical height 19+4+16 = 39 → card 63. <!-- audit: was 20+4+17=41 → 65 -->
3. **Action chips** Flow, spacing 10 (`:376-417`), all cornerRadius 9, height 34:
   - `"Scan"`/`"Scanning"` minWidth 88, disabled iff !present || !enabled || busy, fill cardFill,
     stroke cardStroke → `bluetoothScan()`.
   - `"Hide Unnamed"` / `"Show Unnamed"` (shown when showUnnamedDevices / not) minWidth 132,
     fill showUnnamed ? cardStrongFill : cardFill, fg showUnnamed ? accent : primaryText, stroke
     showUnnamed ? accent α0.18 : cardStroke → toggle `bluetoothShowUnnamedDevices` (UI-only
     preference, default true).
   - `"Advanced"` minWidth 98 → `openBluetoothManager()` (spawns blueman-manager / blueberry /
     `alacritty -e bluetoothctl`).
   Widths 88 + 132 + 98 + 2·10 = 338 ≤ 364 → one row.
4. **Action message** card (visible iff `bluetoothActionMessage != ""`, `:419-442`): width 364,
   height = text + 18 (top 10 / bottom 8), radius 9, `panelColor(cardFill)`, 1 px cardStroke;
   text 12 px primaryText α0.82, wrap, margins 10.
5. **Empty state** card (visible iff no visible devices, `:444-466`): height = text + 26, radius
   9, `panelColor(cardFill)`, cardStroke; text centred, width 364−28, 13 px mutedText, centred,
   wrap, `emptyStateText` (`:99-113`).
6. **Device list** Flickable (visible iff devices, `:468-584`): width 364, height =
   `min(contentHeight, 420)`, clip, interactive iff overflow, StopAtBounds, default Qt wheel
   and drag scrolling. Column spacing 10 of device cards:
   - card: width 364, height = row + 20, radius 9, `panelColor(connected ? accentFill :
     cardFill)`, 1 px `connected ? accentStroke : cardStroke`; inner margins 10.
   - row Item height = max(info, icon) (typically 19+4+15 = 38 → card 58; the 24 px icon is
     32 tall but `max` uses it: max(38, 32) = 38) <!-- audit: was 40 → 60 -->:
     - icon glyph by `device.icon` substring (`:126-150`): audio-headset/headphones/card/
       speakers → U+F02CB; input-mouse → U+F037D; input-keyboard → U+F030C; phone → U+F03F2;
       camera → U+F0100; printer → U+F042A; computer → U+F0322; else enabled ? U+F00AF :
       U+F00B2. 24 px Bold iconFont primaryText.
     - action chip right: icon-only (`iconLabel` = connected ? U+F08B : U+F090, 16 px),
       minWidth 50, cornerRadius 9, disabled iff busy || (!enabled && !connected), fill
       connected ? cardStrongFill : cardFill, fg connected ? critical : accent, stroke connected
       ? critical α0.20 : accent α0.18 → `activateDevice(d)`: connected →
       `bluetoothDisconnect(address, name)`, else `bluetoothConnect(address, paired, name)`.
     - info Column (icon+10 … chip−10), spacing 4: name = `displayName || address` 14 px Bold
       ElideRight; meta 11 px mutedText ElideRight = parts joined by `"  •  "`: Connected |
       Paired | Available, + "Unnamed" if !hasName, + "Trusted", + "Blocked", + "<rssi> dBm"
       if rssi non-null (`:152-174`).
   - device order comes from the service: connected, then paired, then trusted, then
     case-insensitive name (`shell.qml:2137-2160`).

Page height = 10 + 40 + 10 + 63 + 10 + 34 + [10 + msg] + 10 + list/empty + 10.

### 3.3 Data contract
```rust
struct BtDevice { address: String, name: String, display_name: String, has_name: bool,
                  icon: String, paired: bool, trusted: bool, connected: bool, blocked: bool,
                  rssi: Option<i16> }
struct BluetoothPanel { present: bool, enabled: bool, discovering: bool, pairable: bool,
                        busy: bool, action_message: String, show_unnamed: bool,
                        devices: Vec<BtDevice> }
// actions: set_power(bool), scan(), connect(addr, paired, label), disconnect(addr, label),
//          open_manager(), set_show_unnamed(bool)
```

---

## 4. `WindowsVmControl` + `WindowsVmController`
`WindowsVmController` (`WindowsVmController.qml`) lives at popup level so its action survives
page changes (`ControlPanelPopup.qml:39-49`): `pollingActive = popupRequested`; poll every
2000 ms `bash scripts/windows-vm-control.sh status` → `running = (stdout == "running")`.
`toggleVm()`: ignored if busy; busy = true; emit `actionStarting` (→ popup unmapped instantly so
pkexec's auth dialog gets input); run `windows-vm-control.sh hibernate` (running) or `start`;
on exit busy = false, re-poll.

`WindowsVmControl` (`WindowsVmControl.qml`) = ControlPanelToggle: icon U+F05B3, iconOnly,
label (hidden) busy ? (running ? "Sleeping" : "Starting") : (running ? "Windows on" :
"Windows off"), `active = running`, `toggleEnabled = controller && !busy` (busy → disabled look:
fill α0.48, opacity 0.56), click → `toggleVm()`.

Data: `VmState { running: bool, busy: bool }`; actions `vm_start()`, `vm_hibernate()`.

## 5. `ViewflowTargetControl` (`ViewflowTargetControl.qml`)
ControlPanelToggle: `target ∈ {windows, macos}` (default windows), `busy`.
icon = macos ? U+F179 : U+F05B3; iconOnly; label (hidden) busy ? "Switching HDMI / Viewflow" :
"HDMI → macOS"/"HDMI → Windows"; **active always false**; `toggleEnabled = !busy`.
Poll while visible every 2000 ms: `bash scripts/viewflow-target.sh status` → target if output
is `macos`/`windows`. Click → busy, `viewflow-target.sh set <other>`; on exit busy = false,
re-poll; on non-zero exit `notify-send "Viewflow Apple/Windows" <stderr or "Switch failed">`.

## 6. `DeskflowTargetControl` (`DeskflowTargetControl.qml`) — not instantiated
Identical to §5 with helper `scripts/deskflow-target.sh`, labels "Switching"/"macOS"/"Windows",
notification title "Win/macOS switch".

---

## 7. Mihomo page — `MihomoControlPanel` (`MihomoControlPanel.qml`) — unreachable today
Hosted like the bluetooth page (`ControlPanelPopup.qml:502-516`), width 384, external background,
`closeRequested → showMainPage()`. Opening would use the default (non-bluetooth) timings.

State (`:12-21`): `online, service ∈ {user, system, verge, none}, version, mode, connections,
upload, download (B/s), dashboard (url), busy, message`. Poll while visible every 1500 ms
`bash scripts/mihomo-control.sh status` → JSON `{online, service, version, mode,
connections, upload, download, dashboard}`; parse error → message "Could not read Mihomo
status". Actions `runAction(args)`: `mihomo-control.sh start|stop|mode <m>`; exit 0 → apply JSON
stdout + re-poll; exit 3 → "Service is still managed by Clash Verge"; else stderr text or
"Mihomo action failed".
`formatRate(b)`: ≥1 MiB → MB/s (0 decimals if ≥10 MiB else 1); ≥1 KiB → KB/s (0 decimals if
≥10 KiB else 1); else `round(b) B/s`.
Colours: cardFill/cardStrongFill/cardStroke/mutedText as §1.3; `accent = launchColor`;
`downColor = usageLowColor`; `upColor = microphoneColor`.

Layout: root radius 19 (transparent when external), `implicitHeight = content + 20`; content
Column at (10,10), width 364, spacing 10:
1. Row h 40, spacing 10: icon box 40×40 r12 (online: accent α0.24, else cardStrongFill), glyph
   online U+F0582 / offline U+F05AA, 20 px Bold iconFont, colour accent / mutedText (default
   QtRendering). Text Column width 364−100 = 264, vcentred, spacing 2: title `online ? "Mihomo ·
   " + (mode || "Connected") : "Mihomo"` 13 px Bold ElideRight; subtitle `online ? N + " active
   connections" : "Core is not reachable"` 9 px mutedText. ON/OFF pill 40×24 r12, vcentred,
   fill online ? downColor α0.22 : cardStrongFill, text "ON"/"OFF" 9 px bold **default font**,
   colour downColor / mutedText; click (enabled iff service user|system) → start/stop.
2. Rates Row spacing 8: two cards (364−8)/2 = 178 × 54, r14, cardFill, 1 px cardStroke; inner
   Row margins 9 spacing 8: glyph (Download U+F01DA, Upload U+F0552) 16 px tone colour; Column
   spacing 1: value 11 px bold primaryText, label ("Download"/"Upload") 8 px mutedText.
   Border animation: while online && that rate > 1024 B/s, infinite loop ColorAnimation
   border → tone (650 ms) → cardStroke (650 ms), linear.
3. Mode Row spacing 7: `rule`, `global`, `direct`, each (364−14)/3 ≈ 116.67 × 34, r11;
   selected (mode == m): fill accent α0.25, 1 px accent, text accent bold; else cardFill,
   cardStroke, mutedText normal. Text = capitalised name, 10 px default font. Click (enabled iff
   online && !busy) → `mode <m>`.
4. Row spacing 8: two 178×34 r11 cardFill/cardStroke buttons: "Working…"/"Refresh" (mutedText
   10 px bold; click iff !busy → re-poll); "Dashboard  ↗" (accent if online else muted; click
   iff online && dashboard → `xdg-open <dashboard>`).
5. Message Text (visible iff message or service == "verge"): width 364, 8 px wrap, critical
   colour if message else mutedText; text = message or "Managed by Clash Verge · controls stay
   read-only until migration".
6. Divider 364×1 cardStroke.
7. "‹  Back" text 10 px bold mutedText, click → `closeRequested`.

### 7.1 `MihomoControlTile` (`MihomoControlTile.qml`) — not instantiated
ControlPanelSplitTile filling a 62-high item: icon online ? U+F0002 : U+F001D, title "Proxy",
subtitle busy ? "Switching…" : online ? (mode or "Rule") + " mode" : "Off", active = online,
expandIndicatorVisible, left (enabled iff !busy && service user|system) → start/stop,
right → `detailsClicked`. Poll 2000 ms while visible.

---

## 8. `ControlPanelWifiDetails` (`ControlPanelWifiDetails.qml`) — not instantiated
Legacy inline Wi-Fi list with a warm palette. Root radius 19, colour `#362c24` D / `#f0dbc8` L,
1 px `#564435` / `#dcbda2`, `implicitHeight = content + 26`. Palette: title `#f8eadb`/`#5a4030`,
muted `#d8bea7`/`#8b6a53`, chip fill `#43362a`/`#f7ebdf`, chip stroke `#64513f`/`#d9baa0`,
field `#2b231d`/`#fdf5ee` (`:10-25`).
Content Column margins 13 spacing 12: header Row (title 12 px Bold wrap: "No wireless adapter
detected" / "Visible networks" / "Wi-Fi is turned off"; chips "Scan" (→ rescan, enabled iff
enabled && !busy) and "Manage" (→ open manager) — InlineChip h30 r15, width text+22, 11 px Bold,
disabled fill α0.45 + opacity 0.56, hover lighter 1.04, press darker 1.05); action message 11 px
Medium muted wrap; empty text 11 px; network cards (r19, chip fill with hover/press, padding 10,
spacing 8): SSID or "Hidden network" 12 px Bold elide + "Disconnect"/"Connect" 10 px Bold muted;
meta 10 px Medium (`Connected|Saved • security|Open • N%`); inline password field (36 h, r14,
TextInput 11 px Password echo, placeholder "Network password") with Connect/Cancel chips when a
secure unknown network is expanded. Same activate semantics as the Wi-Fi popup.
