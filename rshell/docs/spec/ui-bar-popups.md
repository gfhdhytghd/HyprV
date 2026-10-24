# UI spec — Bar-owned popups & animated panels (part 3 of 3)

Read `ui-bar.md` §0 and §5 first. Covers: the shared popup positioning rule,
AnimatedGlassPanel, the tray menu, the tray overflow grid, the battery info popup with
BatteryInfoLine, and AnimatedReveal. Panels owned by other specs (control panel, stats,
Wi-Fi) reuse AnimatedGlassPanel, but **not** this horizontal positioning rule: the control
panel and Wi-Fi popup are right-aligned (`W − w − 10`), the stats popup is pinned to x = 10,
QuickAdjust right-aligns to the tray pill at the bar's exclusive zone, and the calendar is not
a popup at all (ui-panels*.md). Only the below/above vertical rule is shared by Wi-Fi and stats.
<!-- audit: previous sentence claimed all panels use §1.1; contradicted ControlPanelPopup.qml:774, WifiFallback.qml:329-332, SystemStatsPopup.qml:165, QuickAdjustPopup.qml:150-174 -->

---

## 1. Popup surface pattern (tray menu, overflow, battery)

Each popup is one **full-output** layer surface (`shell.qml:842-859, 1204-1221,
1395-1412`): Overlay layer, anchored to all four edges, `exclusiveZone -1`, transparent,
`aboveWindows`, `focusable = visible`, keyboard `OnDemand` while visible else `None`,
namespace `shell:hyprv-tray-menu` / `shell:hyprv-tray-overflow` / `shell:hyprv-battery-info`
(Hyprland: blur behind alpha > 0.05, default layer open/close animation since `no_anim` is
not set for `shell:.*`). The surface is created hidden and mapped on open on
`parentWindow.screen` (the screen of the bar that opened it).

- A full-surface `MouseArea` (Left|Right|Middle) underneath the content: any click not
  consumed by content closes the popup. Content areas with no MouseArea do **not** consume
  clicks — so clicking the tray menu's padding or the overflow grid's empty padding closes it,
  while the battery card swallows clicks (`shell.qml:1538-1541`).
- A `FocusScope` with focus while visible; Escape handled as described per popup.
- The whole surface is the input region (port: you may restrict input to the card plus an
  "outside click" handling, but the visible behaviour must stay: a click anywhere else on
  that output closes the popup and is **not** delivered to the window underneath).

### 1.1 Positioning rule (identical in all three, e.g. `shell.qml:861-882`)

```
p  = sourceItem.mapToGlobal(round(sourceItem.width/2), sourceItem.height)   // bottom-centre
rx = p.x − screen.x;  ry = p.y − screen.y                                    // output-local
x  = clamp(round(rx − panelW/2), 8, max(8, W − panelW − 8))
belowY = round(ry + 10);  aboveY = round(ry − panelH − 10)
fitsBelow = belowY + panelH <= H − 8;  fitsAbove = aboveY >= 8
y  = (fitsBelow || !fitsAbove) ? max(8, min(H − panelH − 8, belowY)) : max(8, aboveY)
```
`panelH` = `fullPanelHeight` (animated panels) or the rectangle height (overflow). For any
bar module (`y ∈ [10,48]`, bar at the output top, no slide) `ry = 48` → **y = 58**.
Re-run on: open, surface resize, content height change (via a 0 ms `positionTimer`), and
open-animation completion.

---

## 2. AnimatedGlassPanel (`AnimatedGlassPanel.qml`)

Reusable translucent card with a reveal (height wipe) + content fade animation.

### 2.1 Properties

| prop | default | notes |
|---|---|---|
| `revealHeight` | 0 | animated; height of the visible (clipped) part from the top |
| `contentOpacity` | 0 | animated |
| `contentOffset` | `openContentOffset` | animated; **no visible effect** (see 2.2) |
| `fullPanelHeight` | 0 | set by user; `height = implicitHeight = fullPanelHeight` |
| `lineHeight` | 2 | final reveal height of the close animation |
| `radius` | 19 | |
| `surfaceOpacity` | 0.82 | opacity of the fill rectangle (fill and border) |
| `fillColor` / `strokeColor` | `#202020` / `#404040` | users pass glassFill / glassStroke |
| `shadowColor` | transparent | users pass the popup shadow color |
| `devicePixelRatio` | 1 | users pass the window DPR (layer texture size) |
| `openRevealPause` | 0 | ms |
| `openRevealDuration` | 200 | |
| `openContentDelay` | 20 | |
| `openFadeDuration` | 140 | |
| `openSlideDuration` | 180 | |
| `openContentOffset` | −8 | |
| `closeRevealPause` | 0 | |
| `closeRevealDuration` | 180 | |
| `closeFadeDuration` | 90 | |
| `closeSlideDuration` | 150 | |
| `closeContentOffset` | −8 | |

Signals `openAnimationFinished`, `closeAnimationFinished`. Read-only
`openAnimationRunning`, `closeAnimationRunning`.

Functions: `resetAnimationState()` → reveal = full, opacity 1, offset 0.
`prepareOpenAnimation()` → stop; reveal 0, opacity 0, offset = openContentOffset.
`playOpenAnimation()` / `playCloseAnimation()` → stop both, restart the chosen one.
`stopAnimations()`.

### 2.2 Item tree & rendering (`:158-219`)

```
root Item (width = user, height = fullPanelHeight)
└─ clip Item  top-anchored, full width, height = revealHeight, clip: true
   └─ panelFrame Rectangle  (0,0) width × fullPanelHeight; radius; transparent; clip: true (rectangular)
      ├─ shadowLayer Item  fill; visible if surfaceOpacity>0;
      │    layer.enabled = shadowColor.a > 0 && surfaceOpacity > 0
      │    layer.textureSize = (round(w*dpr), round(h*dpr)); smooth; MirrorVertically
      │    layer.effect = MultiEffect { autoPaddingEnabled: true, shadowEnabled: true,
      │        blurEnabled: false, maskEnabled: false, shadowBlur: 10/64 = 0.15625,
      │        shadowScale: 1, shadowColor: root.shadowColor }
      │        (defaults: blurMax 32, shadowOpacity 1, shadowHorizontalOffset 0, shadowVerticalOffset 0)
      │    └─ Rectangle fill; radius; opacity = surfaceOpacity; color = fillColor;
      │         border 1 px strokeColor (inside); antialiased
      └─ panelContent Item  anchors.fill; opacity = contentOpacity; y: contentOffset
           (anchors.fill re-asserts y on every geometry change → the slide offset is
            overridden; content stays at y=0. Verify with open-animation reference frames.)
```

Rendering recipe for the port (per frame):
1. Offscreen: draw the rounded rect (`radius`, fill `fillColor`, 1 px inner border
   `strokeColor`), both multiplied by `surfaceOpacity`, at full size `w × fullPanelHeight`.
2. Shadow: `shadowColor × blur(alpha of step 1)` with MultiEffect blur ≈ `shadowBlur ×
   blurMax = 5 px` radius (MultiEffect uses a mip-level blur; a Gaussian with σ≈2.5 px
   logical is a close match), offset 0, then the step-1 image composited **over** it.
   Because the fill is translucent, the shadow shows *through the card*: interior =
   fill over `shadowColor·α_fill` (dark: black at 0.344·0.45 ≈ 0.155 under `#101214` at
   0.344; light: `#111111` at 0.2296·0.18 ≈ 0.041 under white at 0.2296). Near edges the
   blurred falloff is visible.
3. Clip the result to the **rectangle** `[0,w]×[0,fullPanelHeight]` (panelFrame clip) — the
   shadow halo outside the card is cut off except in the four corner notches outside the
   rounded corners — and then to `[0,w]×[0,revealHeight]`.
4. Draw content (children) at y=0 with `contentOpacity`, also clipped to
   `[0,w]×[0,revealHeight]`.

Effective glass colors with `surfaceOpacity 0.82`:

| | dark | light |
|---|---|---|
| fill | `#101214` α 0.42·0.82 = 0.3444 | `#ffffff` α 0.28·0.82 = 0.2296 |
| border | `#cdd6f4` α 0.14·0.82 = 0.1148 | `#2b2b2c` α 0.10·0.82 = 0.082 |
| shadow | `#000000` α 0.45 | `#111111` α 0.18 |

### 2.3 Animations (`:73-156`)

Open (SequentialAnimation):
```
Pause(openRevealPause)
Parallel {
  revealHeight → fullPanelHeight   duration openRevealDuration, OutCubic
  Sequential {
    Pause(openContentDelay)
    Parallel {
      contentOpacity → 1            duration openFadeDuration,  OutQuad
      contentOffset  → 0            duration openSlideDuration, OutCubic
    }
  }
}
onFinished: reveal = fullPanelHeight, opacity 1, offset 0; emit openAnimationFinished
```
Close (ParallelAnimation):
```
contentOpacity → 0                 duration closeFadeDuration,  InQuad
contentOffset  → closeContentOffset duration closeSlideDuration, InCubic
Sequential { Pause(closeRevealPause); revealHeight → lineHeight (2)  duration closeRevealDuration, InCubic }
onFinished: emit closeAnimationFinished
```
Animations have no `from`: they start from the current value (relevant if a close
interrupts an open). `to: fullPanelHeight` is evaluated when the animation starts.
Total open = `openRevealPause + max(openRevealDuration, openContentDelay +
max(openFadeDuration, openSlideDuration))`; close = `max(closeFade, closeSlide,
closeRevealPause + closeRevealDuration)`.

---

## 3. Tray menu (`TrayMenuPopup`, `shell.qml:611-1140`)

One instance (`trayMenuPopup`), registered as `root.trayMenuController`.

### 3.1 Constants

| | value |
|---|---|
| `textPixelSize` | `root.trayMenuTextPixelSize` = 14 |
| `animationDuration` | 200 |
| `rowHeight` | `max(34, textPixelSize + 18)` = **34** |
| `menuPadding` | 9 |
| `menuWidth` | 300 |
| `menuMaxHeight` | 420 |
| colors | glassFill / glassStroke / hoverFill (ui-bar.md §5.1) |

### 3.2 Panel

AnimatedGlassPanel `menuChrome` (`:934-990`): width 300; `fullPanelHeight = min(420,
menuContent.implicitHeight + 18)`; radius 19; fill glassFill; stroke glassStroke; shadow
dark `rgba(#000,0.45)` / light `rgba(#111111,0.18)`; dpr = window dpr. Animation params:
`openRevealDuration 200, openContentDelay 20, openFadeDuration 140, openSlideDuration 180,
openContentOffset −8, closeRevealDuration 200, closeFadeDuration 90, closeSlideDuration 150,
closeContentOffset −6`, pauses 0. → open total 200 ms, close total 200 ms.

Content (`:992-1137`):
```
Flickable  inset 9 on all sides (282 wide); clip; contentHeight = column height
           (vertical flick/scroll when the menu exceeds 420 total)
└─ Column menuContent  width 282, spacing 1
   ├─ Back row (only when a submenu is open): 282 × 34, radius 11,
   │    color = hover ? hoverFill : transparent;
   │    Text "< Back" left 9, vcenter; primaryText; 14 px weight 400; NativeRendering
   │    MouseArea hover, PointingHand, click → goBack()
   └─ Repeater over the current level's entries → delegate Rectangle
        separator: 282 × 1, radius 0, color glassStroke
        entry:     282 × 34, radius 11,
                   color = (hover && enabled) ? hoverFill : transparent
        ├─ MouseArea fill; hover; enabled = !separator && enabled; Left only;
        │    cursor PointingHand if enabled else Arrow; click → triggerEntry(entry)
        └─ Item inset left/right 9 (visible for non-separators)
           ├─ indicator Text  left, vcenter; "[x]"/"[ ]" (checkbox, buttonType 1) or
           │    "(o)"/"( )" (radio, buttonType 2) by checkState; hidden if none;
           │    primaryText; max(11, 14−1) = 13 px; weight 400
           ├─ icon Image 16×16 vcenter; left = indicator.right + 8 if indicator else 0;
           │    visible if entry.icon non-empty; PreserveAspectFit, smooth, sourceSize 16
           ├─ submenu arrow Text ">" right, vcenter; visible if hasChildren; 14 px; primaryText
           └─ label Text  left = after icon (+8) / after indicator (+8) / 0;
                right = arrow.left − 8 if arrow else edge; vcenter; entry.text;
                color enabled ? primaryText : rgba(primaryText, 0.55); 14 px weight 400;
                elide Right; NoWrap
```
Entry fields come from DBusMenu (via Quickshell `QsMenuOpener.children`): `text`,
`icon` (URL), `enabled`, `isSeparator`, `buttonType` (0 none / 1 checkbox / 2 radio),
`checkState`, `hasChildren`, `menu`.

Column height = Σ(row heights) + 1·(n−1), rows 34 or 1 (separators), + Back row (34 + 1)
when in a submenu.

### 3.3 Behaviour

- `openFor(item, source, window)` (`:683-708`): requires `item.hasMenu`. Set item/source/
  window, `menuHandle = item.menu`, clear the submenu stack, `animatingClose=false`,
  `menuVisible=true`, schedule reposition. If the surface is already visible (re-open for
  another item): `updateLayout()` + `sendOpened()` on the root menu, hydrate, reposition,
  `scheduleOpenAnimation()`. Else map the surface; `onVisibleChanged(true)` does the same
  (+ focus).
- `scheduleOpenAnimation()`: `prepareOpenAnimation()` (reveal 0, opacity 0) then a 16 ms
  timer; when it fires, if content height is still 0 it re-arms (every 16 ms) until the
  DBusMenu layout arrives; then reposition and `playOpenAnimation()`.
- **Hydration**: DBusMenu children are only populated after the menu is "opened"; QML briefly
  opens/closes a hidden `QsMenuAnchor` on the handle (next event-loop turn) to force
  `AboutToShow` + layout fetch (`:638-662`). Port: call `com.canonical.dbusmenu.AboutToShow(id)`
  + `GetLayout(id, …)` and send `Event(id,"opened")`/`"closed"`.
- `triggerEntry(entry)` (`:748-762`): ignore separators/disabled; `hasChildren` →
  `showSubMenu` (push on the stack, updateLayout, hydrate, reposition — **no** animation, the
  content swaps instantly and the panel height snaps, see height-change rule); else
  `entry.activate()` (DBusMenu `Event(id,"clicked")`) then close after 80 ms (`closeTimer`).
- `goBack()`: pop one level, reposition.
- Escape: submenu open → `goBack()`, else `closeMenu()`.
- Click outside the rows (anywhere else on the output, including the card padding) →
  `closeMenu()`.
- `closeMenu()` (`:710-723`): if not visible → just reset flags. Else `menuVisible=false`,
  `animatingClose=true`, play close animation; on `closeAnimationFinished` (if still
  closing) unmap the surface. On unmap: `sendClosed()`, reset flags, stop timers,
  `resetAnimationState()`, and 120 ms later (`clearTimer`) clear item/source/handle if not
  reopened.
- **Height change while visible** (`:954-976`): if an open is pending → reposition and
  restart the 16 ms open timer; if visible, not closing, and no animation running → snap
  `revealHeight = fullPanelHeight`, opacity 1, offset 0 (instant resize); if an animation is
  running → just reposition. Always reposition afterwards.
- The menu's screen = the opening bar's screen; position from §1.1 with `panelH =
  fullPanelHeight` (target height, not the animated reveal height).

### 3.4 Data contract
```rust
struct MenuEntry { id: i32, text: String, icon: Option<ImageRef>, enabled: bool,
                   is_separator: bool, button_type: u8 /*0,1,2*/, checked: bool,
                   has_children: bool }
struct TrayMenuInputs { level_stack: Vec<i32>, entries: Vec<MenuEntry>, anchor: Rect,
                        output: OutputInfo, theme: Theme }
```

---

## 4. Tray overflow grid (`TrayOverflowPopup`, `shell.qml:1142-1308`)

Constants: `popupPadding 8`, `maxColumns 6`. With `n = trayItems.length`:
`columns = clamp(n, 1, 6)`, `rows = max(1, ceil(n / columns))`,
`popupWidth = 16 + columns·18 + (columns−1)·7`, `popupHeight = 16 + rows·38`.
E.g. n=1 → 34×54; n=6 → 159×54; n=7 → 159×92.

```
overflowChrome Rectangle  popupWidth × popupHeight; radius 18; color glassFill (NOT ×0.82);
                          border 1 px glassStroke; no shadow; no animation
└─ Grid  inset 8; columns; columnSpacing 7; rowSpacing 0
   └─ TrayButton × n  (18 × 38 each; parentWindow = overflowWindow, so their menus anchor here)
```
Behaviour: `toggleFor(source, window, items)` closes if visible for the same source+window,
else `openFor` (sets items, `popupRequested=true`, maps or repositions). Appears and
disappears **instantly** (only Hyprland's layer animation for `shell:` namespaces applies).
Escape or outside click → close. If the bound item list becomes empty while open → close;
otherwise reposition. On unmap: clear source/window/flag. The bar also closes it when the
overflow count drops to 0 (`shell.qml:4230-4232`). Position: §1.1 with `panelH =
popupHeight`, source = the `⋯` trigger (22×38).

---

## 5. Battery info popup (`BatteryInfoPopup`, `shell.qml:1310-1618`) + BatteryInfoLine

Reachable only when `batteryAvailable` (never on this desktop — ui-bar.md §5.3). Opening
calls `batteryInfoPoll.refresh()`; the poll (`scripts/battery-info.sh`, JSON) then runs
every 1000 ms while `popupRequested` (`shell.qml:3339-3352`).

### 5.1 Card

AnimatedGlassPanel `popupCard` (`:1479-1536`): width **324**; `fullPanelHeight =
content.implicitHeight + 24`; radius 19 (default); glass colors + shadow as §2.2.
Animation params: `openRevealPause 20, openRevealDuration 200, openContentDelay 20,
openFadeDuration 140, openSlideDuration 180, openContentOffset −8, closeRevealPause 30,
closeRevealDuration 180, closeFadeDuration 90, closeSlideDuration 150, closeContentOffset −8`
→ open 220 ms, close 210 ms. A full-card MouseArea swallows clicks.

```
Column popupContent  inset 12 (300 wide); spacing 10
├─ Text title    batteryPopupTitle (= batteryText, or "电源" when empty);
│                color batteryDetailAccentColor; 15 px Bold
├─ Text status   batteryStatusText; mutedTextColor; 12 px DemiBold; WordWrap
│                (no explicit width → effectively unwrapped)
├─ Rectangle     300 × 1; radius 1; glassStroke
├─ BatteryInfoLine "当前净功率"     value batteryPowerDetailText, valueColor = accent
├─ BatteryInfoLine "半小时平均功率" value batteryAveragePowerDetailText
├─ BatteryInfoLine batteryEstimateTitle   value batteryEstimateText
└─ Text sample   batterySampleWindowText; visible if non-empty; width 300; mutedTextColor;
                 11 px Medium; AlignRight; WordWrap
```
All NativeRendering, baseFont. `mutedTextColor = rgba(primaryText, dark 0.72 / light 0.68)`.

### 5.2 BatteryInfoLine (`BatteryInfoLine.qml`)
```
Item  width = parent width (300); implicitHeight = max(title.h, value.h)
├─ Text title  (0,0) width 118; titleColor = rgba(primaryText, dark 0.72 / light 0.68);
│              13 px DemiBold; WordWrap
└─ Text value  x = 118 + 12 = 130 → right edge (width 170); valueColor (default primaryText);
               13 px Bold; AlignRight; WordWrap
```

### 5.3 Strings (`shell.qml:276-368, 1675-1711`)

| prop | rule |
|---|---|
| `batteryStatusText` | mode charging → "正在充电"; discharging → "电池供电中"; full → "已充满"; plugged → "已接通电源，当前未充电"; else `status` or "电池状态未知" |
| `batteryDetailAccentColor` | charging/full/plugged → batteryColor; discharging && critical → criticalColor; else primaryText |
| `batteryPowerDetailText` | available ? `formatPower(powerW, true)` : "--" |
| `batteryAveragePowerDetailText` | available ? `formatPower(averagePowerW, true)` : "--" |
| `batteryEstimateTitle` | charging/full/plugged → "预计剩余充电时间" else "预计剩余使用时间" |
| `batteryEstimateText` | full → "已充满"; plugged → "已接通电源，当前未充电"; estimateSeconds not finite or <0 → "计算中"; else `formatDuration(s) + " (" + (basis=="current" ? "按当前功率" : "按均值") + ")"` |
| `batterySampleWindowText` | !available → ""; ≥1800 s → "最近 30 分钟采样"; ≥60 → "已采样 " + formatDuration(s); else "开始采样中" |
| `formatPower(v, signed)` | non-finite → "--"; decimals = |v| ≥ 10 ? 1 : 2; prefix "+" if v > 0.004, "-" if v < −0.004 (signed only); `|v|.toFixed(d) + " W"` |
| `formatDuration(s)` | non-finite/<0 → "计算中"; m = round(s/60); m ≤ 0 → "0分钟"; h = ⌊m/60⌋, r = m mod 60; h>0&&r>0 → "{h}小时{r}分钟"; h>0 → "{h}小时"; else "{m}分钟" |

CJK glyphs are not in JetBrains Mono → fontconfig fallback (the same fallback font Qt picks;
check `fc-match "JetBrainsMono Nerd Font:charset=7535"` at port time).

### 5.4 Behaviour
`openFor` maps the surface (or, if already visible, re-prepares and replays the open
animation and repositions). On map: position, focus, `prepareOpenAnimation()`, 16 ms timer
(re-arming while content height is 0) → `playOpenAnimation()`. `closePopup()` plays the close
animation and unmaps on finish. `toggleFor` closes when visible for the same source+window.
Height-change rule identical to the tray menu (§3.3). Escape / outside click → close. On
unmap: reset animation state, clear source/window.

### 5.5 Data contract
```rust
struct BatteryPopupInputs { info: BatteryInfo, theme: Theme, anchor: Rect, output: OutputInfo }
```

---

## 6. AnimatedReveal (`AnimatedReveal.qml`)

Not used by the bar (only by `ExpandableSection.qml`, panels spec); same animation model as
AnimatedGlassPanel but without chrome, and here the content offset **is** visible:

```
root Item  implicitHeight = layoutHeightFollowsReveal ? revealHeight : fullHeight
└─ viewport Item  top-anchored, full width, height revealHeight, clip
   └─ contentItem Item  x 0, y = contentOffset, width = viewport.width, height = fullHeight,
                        opacity = contentOpacity
```
Properties/defaults: `fullHeight 0, lineHeight 2, layoutHeightFollowsReveal false,
openContentOffset −8, closeContentOffset −8, openRevealPause 0, openRevealDuration 200,
openContentDelay 20, openFadeDuration 140, openSlideDuration 180, closeRevealPause 0,
closeRevealDuration 180, closeFadeDuration 90, closeSlideDuration 150`; initial
`revealHeight 0, contentOpacity 0, contentOffset = openContentOffset`. Functions as in
AnimatedGlassPanel plus `collapseImmediately()` (stop; reveal 0, opacity 0, offset =
openContentOffset). Open/close animations are identical to §2.3 with `fullHeight` in place
of `fullPanelHeight`.
