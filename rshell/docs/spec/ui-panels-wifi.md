# UI spec — Wi-Fi / network popup (`WifiFallback`)

Read `ui-panels.md` first. Sources: `WifiFallback.qml` (popup + logic), `WifiIndicator.qml`
(base type: the bar icon — geometry owned by the bar spec, summarised here),
`WifiIconWithFallback.qml`, `WifiActionChip.qml`, `WifiSecurityBadge.qml`,
`ExpandableSection.qml`, `WifiNative.qml` (= `WifiFallback {}`), and `shell.qml` icon helpers.

## 1. Host and entry points
- One instance per bar window, loaded into the bar's `wifiTraySlot` while
  `networkWidgetVisible` (`shell.qml:4170-4192`), with `shellRoot` and `parentWindow = barWindow`.
  The instance on the primary bar is `shellRoot.wifiPanelController`.
- The instance *is* the bar icon (`WifiIndicator`, 30×37 implicit, filled into a 38-high slot):
  hover background radius 10, `activeWorkspaceBackground` α0.18 D / 0.22 L; icon 24 px
  (Fluent SVG or Nerd glyph fallback, §5). Left click → `togglePopup()`; right click → nothing
  (`rightClicked` unconnected) (`WifiIndicator.qml:18-51`, `WifiFallback.qml:266`).
- `shellRoot.openWifiPanel()` (from the control panel Wi-Fi tile) → `openPopup()` on the
  primary instance (`shell.qml:2553-2557`).
- While the popup is visible the instance increments `shellRoot.openWifiPanelCount`
  (decrements on hide/destruction) — the Wi-Fi service uses it to scan more actively
  (`WifiFallback.qml:11-12, 245-249`).

## 2. Surface
`PanelWindow popup` (`:290-310`): namespace `shell:hyprv-wifi`, Overlay, anchors T L R B,
`exclusiveZone −1`, transparent, focusable + keyboard OnDemand while visible,
`visible = popupVisible || animatingClose`, screen = `parentWindow.screen`.
Children: full-surface MouseArea (left/right/middle → `closePopup()`), then a FocusScope
(focus while visible, `Esc` → `closePopup()`) which **contains** the card (so the password
TextInput can take focus inside it).

## 3. Card (`popupChrome: AnimatedGlassPanel`, `:376-428`)
- width `popupPanelWidth = 384`; `fullPanelHeight = panelColumn.implicitHeight + 20`
  (`panelVerticalPadding`); radius 19; fill `glassFill`, stroke `glassStroke`, surface 0.82,
  no shadow.
- Timing: openRevealPause 85, openRevealDuration 280, openContentDelay 60, openFadeDuration 180,
  openSlideDuration 220, openContentOffset −10, closeRevealPause 35, closeRevealDuration 210,
  closeFadeDuration 110, closeSlideDuration 170, closeContentOffset −8.
  (Content slide is a no-op, see `ui-panels.md` §2.1.)
- Click-absorbing MouseArea fills the card.
- Position (`updatePopupPosition`, `:319-342`), W×H = output size, `popupScreenMargin 8`,
  `popupRightMargin 10`:
  - anchor point = bar icon centre-bottom in screen coords: `relY = icon.mapToGlobal(w/2, h).y
    − screen.y` (≈ 48 with the bar shown).
  - `x = max(8, min(W − 384 − 8, W − 384 − 10)) = W − 394`.
  - `belowY = round(relY + 10)`, `aboveY = round(relY − cardH − 10)`; if it fits below
    (`belowY + cardH ≤ H − 8`) or doesn't fit above (`aboveY < 8`): `y = max(8, min(H − cardH
    − 8, belowY))`; else `y = max(8, aboveY)`. Normally `y ≈ 58`.
  - Recomputed on open, open-finished, height change while pending, output resize, and when
    the bar icon moves (`onXChanged/onYChanged`, `:268-278`).

### 3.1 Open / close logic (`:165-195`, `:344-360`, `:397-423`, `:976-994`)
- `openPopup()`: if the live network list is empty → ask the Wi-Fi service to refresh; set
  `popupVisible = true`. If currently closing: cancel closing and `playOpenAnimation()` directly
  (animates from the current partial state, no reset); else reposition.
- Window becomes visible → reposition, focus the FocusScope, (if not closing) pending = true,
  `prepareOpenAnimation()`, start 16 ms `popupOpenTimer` → when content height > 0: pending =
  false, reposition, `playOpenAnimation()`.
- `closePopup()`: if not visible or already closing → `popupVisible = false` only. Else
  `animatingClose = true`, `popupVisible = false`, `playCloseAnimation()`; on finish
  `animatingClose = false` → window unmaps (visible binding).
- Height changes while idle → snap `revealHeight = fullPanelHeight` (instant).
- `togglePopup()`: visible && !closing → close, else open.
- On `popupVisible` change: collapse any expanded password row (`expandedSsid = ""`,
  `passwordText = ""`) when hiding; resync the displayed list (`syncNetworkList(true)`) both ways.

## 4. Content (`panelColumn`, `:430-972`)
Column at (10, 10), width 364, spacing `panelSectionSpacing 10`. Constants: `panelPadding 10,
innerPadding 10, innerRadius 9, maxVisibleNetworks 5`. All cards use `panelColor()` (α×0.82).

1. **Header** Item 364×36 (`:444-472`): `"Network"` 17 px Bold baseFont primaryText, left,
   vcentred; Close chip right (x = 364 − w), vcentred: label "Close", minWidth 76, cornerRadius
   9, fill primaryText α0.08 D / 0.12 L, stroke cardStroke → `closePopup()`.
2. **Status card** (`:474-567`): width 364, height = body + 24 (top 10, bottom 14), radius 9,
   fill `panelColor(networkConnected ? accentFill : cardStrongFill)`, 1 px `networkConnected ?
   accentStroke : cardStroke`. Body (margins 10, height = max(info, icon 24, pill 0)):
   - left: `WifiIconWithFallback` 24 px, source `networkTrayIconSource()`, fallback glyph
     `networkTrayGlyph()` (§5).
   - right: disabled WifiActionChip pill, minWidth 72, r9, fill cardFill, label = wired →
     "Wired"; other → "Online"; no wifi device → "No Wi-Fi"; hardware blocked → "Blocked";
     radio on ? (connected ? "Online" : "Ready") : "Off". fg = networkConnected ? accent :
     primaryText (stroke override is hidden by disabled colours; opacity 0.56).
   - info Column (icon+10 … pill−10, vcentred, spacing 4):
     - summary 14 px Bold primaryText ElideRight (`connectionSummary`, `:48-71`): wired →
       "Ethernet connected"; other → "Connected via <iface or 'network'>"; no device → "No
       wireless device detected"; hw blocked → "Hardware blocked"; radio off → "Wi-Fi
       disabled"; wifi connected → `"<ssid or 'Wi-Fi connected'>  <round(strength·100)>%"`
       (two spaces); else "Not connected".
     - detail 12 px mutedText ElideRight: networkConnected → "Interface: <defaultInterface or
       'network'>"; wifi device present → "Wireless interface: <wifiInterface or 'wifi'>";
       else "No wireless device detected".
3. **Action row** Row spacing 10 (`:569-608`), chips r9 h34:
   - "Turn Off"/"Turn On" (by radio state) minWidth 96, disabled iff no wifi controls || hw
     blocked || busy; fill cardStrongFill, fg accent, stroke accent α0.18 →
     `wifiSetRadio(!enabled)`.
   - "Rescan" minWidth 88, disabled iff no controls || radio off || busy; cardFill/cardStroke →
     `wifiRescan()`.
   - "Advanced" minWidth 98 → `openWifiManager()` (spawns nm-connection-editor / iwgtk /
     `alacritty -e nmtui`).
4. **Message card** (visible iff `wifiActionMessage != ""`, `:610-635`): height text + 18,
   r9, `panelColor(cardFill)`, cardStroke; text 12 px primaryText α0.82 wrap, margins 10.
5. **Empty card** (visible iff no displayed networks, `:637-665`): height text + 26, r9,
   `panelColor(cardFill)`, cardStroke; centred text (width 336, 13 px mutedText, wrap):
   no controls → (net connected ? "Ethernet is active. No wireless device detected." : "No
   wireless device detected."); radio on → (net ? "Ethernet is active. No visible Wi-Fi
   networks right now." : "No visible networks right now."); radio off → (net ? "Ethernet is
   active. Turn Wi-Fi on to scan for wireless networks." : "Turn Wi-Fi on to scan for
   networks.").
6. **Network list** Flickable (visible iff networks, `:667-971`): width 364; `height =
   min(contentHeight, cappedContentHeight)` where capped = sum of the first ≤5 cards' current
   heights + (count−1)·10 (re-evaluates as cards expand); clip; vertical only; StopAtBounds;
   interactive iff overflow. Custom wheel (`:956-970`): if interactive, step = `pixelDelta.y`
   if non-zero else `angleDelta.y / 2` (= 60 px per notch), `contentY = clamp(contentY −
   step, 0, contentH − h)`, accepted. Column spacing 10.

   **Network card** (`:704-952`): width 364, height = body + 20, r9, fill
   `panelColor(active ? accentFill : cardFill)`, 1 px `active ? accentStroke : cardStroke`.
   Body Column margins 10, spacing 10:
   - Row Item height = max(info, icon 24) (= 20 + 4 + 15 = 39 typically → collapsed card 59) <!-- audit: measured metrics; was 40/60 -->:
     - signal icon `WifiIconWithFallback` 24 px: source `wifiListIconSource(signal, secure)`,
       fallback `wifiSignalGlyph(signal)` (§5).
     - connect chip right, icon-only 16 px, minWidth 50, r9. Glyph (`:709-715`): active →
       U+F08B (sign-out); enterprise && !known → U+F013 (cog); secure && !known && !expanded →
       U+F084 (key); else U+F090 (sign-in). Disabled iff busy || (radio off && !active) ||
       (expanded && secure && !known && password empty). Fill active ? cardStrongFill :
       cardFill; fg active ? critical : accent; stroke active ? critical α0.20 : accent α0.18.
       Click → `activateNetwork(network)`.
     - info Column (icon+10 … chip−10, vcentred, spacing 4):
       - title line (clip, height max(19, badge 20) = 20): Row spacing 6: SSID 14 px Bold primaryText,
         width `min(implicit, lineW − badgeW)` ElideRight, followed by `WifiSecurityBadge`
         (22×20, r9, lock U+F023) iff secure (badgeW = 22 + 6).
       - meta 11 px mutedText ElideRight: `[Connected|Saved] • [802.1X | security | Open] •
         <signal>%` joined by `"  •  "` (`:197-213`).
   - **Password section** `ExpandableSection` (width 364−20, `fullHeight = 82`, expanded iff
     this SSID is `expandedSsid` && secure && !known). Timing: openRevealDuration 180,
     openContentDelay 20, openFadeDuration 130, openSlideDuration 180, openContentOffset −6,
     closeRevealDuration 150, closeFadeDuration 90, closeSlideDuration 130, closeContentOffset
     −4 (open/close pauses 0). Expanded card height = 39 + 10 + 82 + 20 = 151 <!-- audit: was 152 -->. Content Column
     spacing 10:
     - field 344×38, r9, fill `panelColor(inputFill)`, 1 px `accentStroke` when the input has
       focus else cardStroke. TextInput (left/right margins 10, vcentred): 13 px baseFont,
       **letterSpacing 3**, primaryText, `echoMode Password` (Qt mask char U+25CF ●),
       selectByMouse, sensitive input hints, focus requested when expanded (and again 60 ms
       later via `passwordFocusTimer`). Text bound to `passwordText`. `Return` with non-empty
       text → `activateNetwork`. Placeholder "Password" 13 px softText at x 10 while empty.
     - Row spacing 10: "Join" chip minWidth 82, disabled iff empty password || busy, fill
       cardStrongFill, fg accent, stroke accent α0.18 → `activateNetwork`; "Cancel" chip
       minWidth 82, cardFill/cardStroke → collapse (`expandedSsid = ""`).

### 4.1 `activateNetwork(n)` (`:215-241`)
Ignored if busy. Active → collapse, `wifiDisconnect()`. Enterprise && !known → set
`wifiActionMessage = "802.1X networks need a saved profile. Open the editor for first-time
setup."` and open the manager. Secure && !known: if already expanded with empty password →
nothing; if not expanded → expand this SSID (clear password); else → `wifiConnect(ssid,
password, security)` and collapse. Open/known → `wifiConnect(ssid, "", security)`.

### 4.2 Displayed list snapshotting (`syncNetworkList`, `:124-163`)
The popup shows `displayedNetworks`, a snapshot of `shellRoot.wifiNetworks` (each cloned to
`{active, ssid, signal:Number, security, secure, known, enterprise}`):
- On live list change (non-forced): if a row is expanded and its SSID still exists live →
  keep the snapshot unchanged (no reflow while typing a password).
- If the new list is non-empty: when the popup is open, radio on, hardware enabled, the snapshot
  is < 30 s old, and the new list is *shorter* → merge (new entries first, then previous ones
  not in the new list, dedup by SSID); else replace. Timestamp updated.
- If the new list is empty: keep the old snapshot if (popup open & radio on & hw enabled &
  snapshot < 30 s), else clear.
- Afterwards, if the expanded SSID is no longer a secure-unknown entry → collapse.
- Forced sync on: shellRoot set, creation, popup show/hide, collapse.
Order is the service's order (the UI does not sort).

## 5. Network icons (`shell.qml:1717-1786`, `:1925-1960`, `WifiIconWithFallback.qml`)
`WifiIconWithFallback` (24 px box): shows an SVG icon (async `IconImage`) when it loads, else a
Nerd glyph (Bold, `fallbackPixelSize = max(13, effectiveIconSize − 2)` → **22 in light mode, 14 in dark mode** (effective size 16 for symbolic sources) <!-- audit: was '22' unconditionally; the bar's WifiIndicator instead passes an explicit 24, ui-bar-layout.md §8.4 -->, colour
primaryText). If the SVG path contains `/symbolic/` (dark-mode icons): drawn at
`round(24·16/24) = 16` px and both icon and glyph are offset **−2 px vertically**; else 24 px.
- Fluent theme dirs discovered at startup: first of `~/.local/share/icons`,
  `/usr/local/share/icons`, `/usr/share/icons` containing `Fluent-light`, `Fluent-dark`,
  `Fluent` (`shell.qml:2924-2960`).
- `fluentWifiIconSource(name, dark)`: dark → `<Fluent-dark|Fluent>/symbolic/status/<name>-symbolic.svg`;
  light → `<Fluent-light|Fluent>/24/panel/<name>.svg`; "" if no theme.
- `wifiSignalBucket(p)`: <20 → "0", <40 → "25", <60 → "50", <80 → "75", else "100".
- `wifiListIconSource(sig, secure)` = fluent(`nm-signal-<bucket>[-secure]`, darkMode).
- `wifiTrayIconSource(enabled, hw, connected, strength, secure)`: dark: !hw →
  `network-wireless-hardware-disabled`, !enabled → `network-wireless-disabled`, !connected →
  `network-wireless-disconnected`, else `nm-signal-<bucket>[-secure]`; light: !hw →
  `network-wireless-offline`, !enabled → `network-wireless-off`, !connected →
  `network-wireless-disconnected`, else signal.
- `wiredTrayIconSource(true)`: dark → fluent `network-wired`; light →
  `quickshell/assets/tray/network-wired-light.svg`.
- `networkTrayIconSource()`: wired/other active → wired; wifi active → tray(true, hw, true,
  strength, secure); else tray(radioEnabled, hw, wifiConnected, strength, secure).
- Glyphs: `wifiSignalGlyph(p)`: <20 U+F092F, <40 U+F091F, <60 U+F0922, <80 U+F0925, else
  U+F0928. `wifiTrayGlyph`: !enabled U+F092D, !connected U+F092E, else signal glyph.
  `networkTrayGlyph()`: wired/other U+F0200; wifi active → signal glyph; else tray glyph.

## 6. Data contract
```rust
struct WifiNetwork { ssid: String, signal: u8 /*0..100*/, security: String, secure: bool,
                     known: bool, active: bool, enterprise: bool }
struct NetworkState {
    default_interface: String,               // "" = offline
    active_type: NetType,                    // Wifi | Wired | Other | Offline
    wifi_device_present: bool, wifi_capability_detected: bool,
    wifi_radio_enabled: bool, wifi_hardware_enabled: bool,
    wifi_connected: bool, wifi_ssid: String, wifi_signal_strength: f32 /*0..1*/,
    wifi_secure: bool, wifi_interface: String,
    networks: Vec<WifiNetwork>,
    action_busy: bool, action_message: String,
    network_widget_visible: bool,            // bar decides whether the icon/popup exist
}
// actions: set_radio(bool), rescan(), disconnect(), connect(ssid, password, security),
//          open_manager(), panel_open_count(+1/-1) for scan cadence
```
