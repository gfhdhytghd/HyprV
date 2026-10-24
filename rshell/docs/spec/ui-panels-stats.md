# UI spec — QuickAdjust, SystemStats, SystemTrendChart

Read `ui-panels.md` first. Sources: `QuickAdjustPopup.qml`, `SystemStatsPopup.qml`,
`SystemTrendChart.qml`, `ControlPanelSlider.qml` (reused), `shell.qml`.

---

## 1. `QuickAdjustPopup` (`QuickAdjustPopup.qml`) — brightness/volume OSD

### 1.1 Surface
`PanelWindow` (`:112-135`): namespace `shell:hyprv-quick-adjust`, Overlay, anchors T L R B,
`exclusiveZone −1`, transparent, non-opaque, **not focusable**, keyboard None, screen = primary
bar's screen. **Input region = the slider card only** (`mask: Region { item: popupBounds }`) —
the rest of the output stays click-through. Starts unmapped.

### 1.2 Geometry
Constants (`:12-17`): `popupScreenMargin 10`, `moduleSize 62`, `cardSpacing 10`,
`popupWidth = 62·4 + 10·3 = 278`, `popupHeight 62`, `hiddenScale 0.96`.
`popupBounds` Item 278×62 positioned by `updatePopupPosition()` (`:150-174`), W = output width:
- `barBottom = max(0, primaryBar.exclusiveZone || 58)` (58 with the bar shown) — **no extra
  margin**: `y = barBottom`.
- With an anchor (`shellRoot.quickAdjustAnchorItem` = the primary bar's `rightTrayPill`):
  `anchorRight = round(anchor.mapToGlobal(anchor.width, anchor.height).x − screen.x)`;
  `x = max(10, min(W − 278 − 10, anchorRight − 278))` (right edge aligned with the pill's right
  edge). Without anchor: `x = max(10, W − 288)`.
- Recomputed on map, output size change, screen change.

### 1.3 Content
`ControlPanelSlider` filling `popupBounds` (`:182-219`) — see `ui-panels-control.md` §2.3 for
drawing; here with 278×62: `iconLeftInset 24` (glyph x = 24), `iconPixelSize` 21.08, icon hit
box centre x = 24 + 10.54, half-size 18.6.
- mode `brightness`: icon U+F00DF, value `brightnessPercent`, not icon-clickable.
- mode `volume`: icon = shell `volumeIcon` (U+F026 unavailable / U+F466 muted / U+F026 ≤20 /
  U+F027 ≤50 / U+F028), value `audioVolumePercent`, icon clickable → `toggleAudioMute()`.
- `opacity = cardOpacity`, `scale = cardScale`, **transformOrigin TopRight**.
  `Behavior on opacity`: 120 ms **OutQuad**; `Behavior on scale`: 160 ms **OutCubic**.
- `valueChangeRequested(v)` → `applyValue(v)`; **`adjustmentFinished` is not connected** (so
  brightness drags here never call `brightness_finish`; replicate or fix deliberately).

### 1.4 Behaviour (`:22-110`)
- `show(kind)`: mode = `volume` if kind == "volume" else `brightness`; cancel closing; restart
  the 3000 ms auto-hide timer; if unmapped: set opacity 0, scale 0.96, map, position; then
  opacity → 1, scale → 1 (animated by the Behaviors: fade-in 120 ms, grow from top-right 160 ms).
  Calling `show` while visible just switches mode (instant content change) and restarts the
  timer (and reverses a running hide).
- `hide()` (auto after 3000 ms idle): closing = true, stop timer, opacity → 0, scale → 0.96,
  unmap after **140 ms** (`closeFinalizeTimer`) — i.e. the 160 ms scale animation is cut at
  140 ms.
- `applyValue(v)` (`:58-78`): v = clamp(round(v)); restart auto-hide; brightness →
  `applyBrightnessPercent(v)`; volume → `audioAvailable = true`, `audioVolumePercent = v`,
  unmute if muted and v > 0, `hyprctl eval 'require("hypr.controls").volume("set",v)'`, schedule
  audio refresh.
- `toggleAudioMute()` (volume mode only): restart auto-hide; flip `audioMuted`;
  `hyprctl eval 'require("hypr.controls").volume("toggle")'`.
- Entry points: IPC `quickAdjust.*` (`ui-panels.md` §4.1) and the brightness probe completion
  when `_showQuickAdjustAfterBrightnessProbe` (`shell.qml:3013-3016`).

### 1.5 Data contract
```rust
struct QuickAdjustInputs { mode: Mode /*Brightness|Volume*/, brightness_percent: u8,
    audio_available: bool, audio_muted: bool, audio_volume_percent: u8,
    anchor_right_x: Option<f64>, bar_bottom: i32 }
// actions: set_brightness(u8), set_volume(u8), toggle_mute()
```

---

## 2. `SystemStatsPopup` (`SystemStatsPopup.qml`)

### 2.1 Surface and lifecycle
`PanelWindow` (`:133-150`): namespace `shell:hyprv-system-stats`, Overlay, anchors T L R B,
`exclusiveZone −1`, focusable + keyboard OnDemand while visible, screen = `parentWindow.screen`.
Full-surface MouseArea (any button) → `closePopup()`; FocusScope `Esc` → `closePopup()`.
State machine = ControlPanelPopup's without pages (`:64-131`, `:179-197`, `:235-271`):
`openFor(source, window)`, `toggleFor(source, window)` (same source+window while mapped →
close), `closePopup()`; 16 ms open timer waits for content height; close-finished → unmap;
unmap → reset, clear source/window. Opened from bar CPU / memory / network triggers
(`shell.qml:3851, 3862, 3873`); clicking another trigger while open re-runs the open animation
at the new anchor.

### 2.2 Card (`popupCard: AnimatedGlassPanel`, `:214-233`)
- width **548**, `fullPanelHeight = content + 2·16` (`popupPadding 16`), radius 19.
- fill `glassFill`, stroke `glassStroke` (D α0.14 / L α0.10), surface 0.82,
  **shadowColor = `#000000` α0.45 (D) / `#111111` α0.18 (L)** — the only popup with the
  MultiEffect shadow (see `ui-panels.md` §2.1 for its exact, mostly-clipped appearance).
- Timing: 20 / 200 / 20 / 140 / 180 / −8 open; 30 / 180 / 90 / 150 / −8 close (the defaults
  plus the two pauses).
- Position (`:159-177`): **left-aligned**: `x = max(10, min(W − 548 − 10, 10)) = 10`.
  Vertical: anchor = source centre-bottom `relY`; `belowY = round(relY + 10)`, `aboveY =
  round(relY − cardH − 10)`; fits below (`belowY + cardH ≤ H − 8`) or not above (`aboveY < 8`)
  → `y = max(8, min(H − cardH − 8, belowY))`; else `y = max(8, aboveY)`. Normally y ≈ 58.

### 2.3 Content (`Loader` active while mapped, margins 16 → width 516; Column spacing 18)
Colours (`:18-27`): `mutedTextColor` = primaryText α0.72 D / 0.68 L;
`metricTextColor` = primaryText (D) / **`#000000` (L)**; `cpuChartColor =
usageSeverityColor(cpuUsage)`, `memoryChartColor = usageSeverityColor(memoryUsage)`,
`networkChartColor = launchColor`.

1. **Header** (`:294-323`), height = max(20, 15) = 20 <!-- audit: measured metrics, was 21 -->: left `"系统资源"` 15 px Bold primaryText;
   right `"过去 120s · 1s"` 11 px Medium mutedTextColor; both vcentred. (CJK via fallback font.)
2. **CPU section** Column spacing 8 (`:325-414`):
   - title row height 22: `"CPU:"` 16 px Bold primaryText; 8 px gap; value
     `round(cpuUsage) + "%"` 16 px Bold metricTextColor.
   - chart row (height = max(chart implicit 116, grid 152) = 152):
     - `SystemTrendChart` at (0,0): samples `cpuHistory`, accent cpuChartColor, `width =
       max(240, 516 − gridW − 16)` = 516 − 158 − 16 = **342**, `height = max(112, gridH)` =
       **152**, `maxValue 100`, no autoscale, `active = popupRequested || animatingClose`.
     - core Grid right-aligned, vcentred: 2 columns, rowSpacing 8, columnSpacing 10, always
       **8 tiles** C1…C8 in row-major order (C1 C2 / C3 C4 / …); grid 158 × 152. Tile 74×32,
       radius 10, fill `panelColor(primaryText α0.06 D / 0.04 L)`, 1 px glassStroke; centred
       text `"C<n> <round(v)>%"` or `"C<n> --"` when the core is missing, 13 px Bold
       metricTextColor (`coreUsageColor` always returns metricTextColor).
3. **RAM section** Column spacing 8 (`:416-462`): title row `"RAM:"` + `round(memoryUsage)%`
   (same styles); chart 516×112, samples `memoryHistory`, memoryChartColor, max 100.
4. **Network section** Column spacing 8 (`:464-522`): title row: `"Network:"` + value
   (`humanRate(rx+tx)` if a default interface else `"未连接"`), and right-aligned detail
   `"↓ " + humanRate(rx) + "   ↑ " + humanRate(tx)` (three spaces) 11 px Medium
   metricTextColor; chart 516×112, samples `networkHistory` (bytes/s), accent launchColor,
   **autoScale**.
   `humanRate(b)` (`shell.qml:1788-1800`): <1024 → `round(b) + " B/s"`; <1 MiB →
   `(b/1024).toFixed(1) + "kB/s"`; <1 GiB → `toFixed(1) + "MB/s"`; else `"GB/s"` (no space
   before the unit except for B/s).

Approximate total card height: 16 + 20 + 18 + (22+8+152) + 18 + (22+8+112) + 18 + (22+8+112)
+ 16 = **572** <!-- audit: was 573 -->.

### 2.4 Data contract
```rust
struct SystemStats {
    cpu_usage: f32, memory_usage: f32,                 // percent
    cpu_core_usages: Vec<f32>,                         // first 8 shown
    default_interface: String, rx_rate: f64, tx_rate: f64,   // bytes/s
    cpu_history: VecDeque<f32>, memory_history: VecDeque<f32>,
    network_history: VecDeque<f64>,                    // rx+tx, each capped at 120 samples
}
```
Sampling: 1 s cadence (`"过去 120s · 1s"`), history limit `statsHistoryLimit = 120`
(`shell.qml:240`); network history only appended when net counters exist (`:2420`) <!-- audit: citations corrected -->. The
sampler should run only while a consumer needs it (subscriber-counted, ARCHITECTURE).

---

## 3. `SystemTrendChart` (`SystemTrendChart.qml`)
Props (`:6-28`): `active true`, `samples []`, `maxValue 100`, `autoScale false`,
`lineWidth 2.25`, `gridLineCount 4`, `surfaceOpacity 0.82`, `accentColor`,
`frameColor` = primaryText α0.12 D / 0.10 L, `gridColor` = primaryText α0.10 D / 0.08 L,
`panelFill` = `#fff` α0.055 D / α0.032 L, `overlayFill` = `#fff` α0.018 D / transparent L,
`areaTopColor` = accent α0.18 D / 0.14 L, `areaBottomColor` = accent α0.02.
`implicitWidth 280`, `implicitHeight 116`.
`resolvedMaxValue` (`:30-40`): start = autoScale ? 0 : max(1, maxValue); take max with every
sample (negatives → 0); if autoScale ×1.15; result max(1, ·).

Drawing:
1. Frame Rectangle fill, radius 10, **opacity 0.82** (whole rect incl. border), fill panelFill,
   1 px frameColor; inside it (D only) an overlay rect same radius with overlayFill (`:49-66`).
2. Canvas inset 2 px on all sides (`:68-154`), canvas size (cw, ch); plot padding 10 each side:
   `left = top = 10`, `chartW = max(1, cw − 20)`, `chartH = max(1, ch − 20)`.
   - Grid: for i in 0..3, `y = 10 + chartH·i/3`, 1 px line gridColor from x 10 to 10+chartW
     (not pixel-snapped; antialiased).
   - No samples → stop.
   - Points: `stepX = n > 1 ? chartW/(n−1) : 0`; `x_i = 10 + stepX·i`; `y_i = 10 + chartH −
     clamp(v_i, 0, max)/max · chartH`. With 1 sample, a second point is added at
     (10 + chartW, y_0). **The series always spans the full width** regardless of n.
   - Area: path (x_0, bottom) → all points → (x_last, bottom) → close; fill vertical linear
     gradient from areaTopColor at y = 10 to areaBottomColor at y = 10 + chartH.
   - Line: polyline through points, width 2.25, round join, round cap, accentColor.
   - End dot: filled circle radius 3 at the last point, accentColor.
3. Repaint triggers: active/visible/samples/max/size/colour changes (only when active &&
   visible). No animation between samples (each new sample redraws instantly).
