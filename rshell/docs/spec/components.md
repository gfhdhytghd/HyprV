# hyprv-ui widget kit — deduplicated component list

Work list for the `hyprv-ui` engine (primitives + widgets) and the `hyprv-shell` composites.
Each entry lists its parameters and the spec sections that use it. Pixel and timing semantics are
**defined in the referenced spec sections**; this file only deduplicates them. Where two specs
describe the same thing, the canonical section is marked ★.

Notation: `Color` = premultiplied RGBA; `Anim<T>` = a property that can carry a Behavior;
lengths are logical px (f64).

---

## 0. Engine foundations (needed by every widget)

### 0.1 Core semantics
| facility | parameters / rules | spec |
|---|---|---|
| Theme tokens | `dark: bool` → all tokens; `withAlpha` replaces alpha; `panelColor` = α×0.82 | ★ui-bar.md §5.1, ui-panels.md §1.2–1.3, ui-island.md §0.7 |
| Qt colour maths | `lighter(c,f)`, `darker(c,f)` with Qt's 16-bit HSV algorithm (lighter on white is a no-op) | ui-panels.md §1.2, ui-bar.md §5.1 (`#8789c1`) |
| Opacity | per-node multiply, no group isolation unless an effect layer is used | ★ui-bar.md §0 |
| Rect clip | `clip: true` clips children to the bounding **rectangle** (not rounded) | ui-panels.md §1.2, ui-bar-popups.md §2.2 |
| Radius clamp | `radius = min(r, w/2, h/2)` at paint time, animated value unclamped | ★ui-island.md §0.4 |
| Inner border | 1 px stroke inside the bounds, same rounded shape, AA | ★ui-island.md §0.5 |
| Easing | QEasingCurve: Linear, InQuad, OutQuad, InOutQuad, InCubic, OutCubic, BezierSpline (BarMotion curve with overshoot) | ★ui-island.md §0.2, ui-bar.md §7.1 |
| `Behavior` animator | restart from current animated value to new target with full duration; gated by `enabled`; no run at creation | ★ui-island.md §0.3 |
| Timelines | `Sequential` / `Parallel` / `Pause` / `NumberAnimation{to}` / `ColorAnimation` / infinite loops | ui-bar-popups.md §2.3, ui-panels-control.md §7 (border pulse) |
| Positioners | Row / Column / Grid / Flow: skip invisible **and zero-width/height** children, fractional positions | ★ui-bar-layout.md §1.1, ui-panels.md §2.3 |
| Anchor centring | Qt `hcenter` rule (odd int sizes round up) | ★ui-bar-layout.md §1.2 |
| Text layout | family, pixelSize, weight (400/500/600/700/900→ExtraBold), colour, elide right, wrap / WordWrap, maximumLineCount, AlignH/V, `letterSpacing`, fixed line height (`lineHeightMode FixedHeight`), password echo; Native vs QtRendering; fontconfig fallback (HarmonyOS Sans SC for CJK) | ★§0.2 below, ui-bar.md §0.1, ui-island.md §0.6 |
| Pointer input | hover (`containsMouse`, HoverHandler with margin), click on release, press state, wheel (`angleDelta`/`pixelDelta`), drag threshold, TapHandler, DragHandler, cursor shapes (`wp_cursor_shape_v1` pointer/default) | ★ui-bar-layout.md §1.5–1.8, ui-island.md §4 |
| Keyboard | focus scope, Esc handling, Tab/arrow navigation, key swallowing before ready | ui-bar.md §3, ui-panels-calendar.md §4.1 |
| Effects | MultiEffect **shadow** (blur ≈ 5 px, clipped), **rounded mask** for images, **colorize** (solid tint keeping alpha) | ★ui-bar-popups.md §2.2, ui-island.md §3.2, ui-panels-control.md §2.2 |
| Vector paint (Canvas-like) | paths (lineTo, arc, quadraticCurveTo), fill, stroke (width, round cap/join), vertical linear gradient, antialiasing | ui-bar.md §9.3, ui-island.md §3.5/§7, ui-panels-stats.md §3 |
| Images | SVG raster at `sourceSize` then scale (smooth + mipmap), PreserveAspectFit/Crop, async load with a "Ready" state for fallback switching, freedesktop icon lookup, SNI pixmaps | ui-bar-layout.md §8.4–8.6, ui-panels-control.md §2.2 |
| Surfaces | layer-shell (layer, anchors, exclusive zone, keyboard mode, namespace), input-region union of item rects that follows animated geometry, transparent premultiplied buffers, resize on demand (58 ↔ 820) | ★ui-bar.md §2, ui-panels.md §3, ui-bar-layout.md §2.1 |

### 0.2 Measured Qt text metrics ★
Measured with Qt 6.12 (the Qt that quickshell 0.3.2 links), `qml6`, QPA offscreen, this
machine's fontconfig (hintslight, autohint, grayscale). `QT_SCALE_FACTOR` 1, 1.5 and 2 gave
identical logical values. NativeRendering and QtRendering gave identical sizes. Family
`JetBrainsMono Nerd Font`, weight Bold.

- **Advance** = `0.6·px` quantised to 1/64 px (fractional, not pixel-rounded): 16 px → 9.59375;
  10 chars at 8/9/10/11/12/13/14/15/16/17/18/24 px = 47.97/53.91/60/65.94/71.88/77.97/83.91/
  90/95.94/101.88/107.97/143.91.
- **`implicitWidth`** = Σ advances + ink overflow of the **last** glyph (glyphs in the middle add
  only their advance).
- **`implicitHeight`** = `ceil(1.32·px)`:

| px | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 15 | 16 | 17 | 18 | 20 | 21.08 | 24 | 26 | 31 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| h | 11 | 12 | 14 | 15 | 16 | 18 | 19 | 20 | 22 | 23 | 24 | 27 | 28 | 32 | 35 | 41 |

Measured single-label widths (Bold):

| text @ px | w | text @ px | w |
|---|---|---|---|
| U+F028 @16 | 18.59 | U+F303 @17 | 17.19 |
| U+F027 @16 | 12.59 | U+F05B3 @17 | 13.19 |
| U+F026 @16 | 9.59 | U+F0A2 @14 | 12.39 |
| U+F466 @16 | 16.59 | U+F009B @14 | 13.39 |
| U+F137 / U+F138 @16 | 14.59 | U+F444 @10 | 8.00 |
| U+F04C @16 | 9.59 | U+22EF @18 | 10.80 |
| U+F04B @16 | 11.59 | U+F023 @16 / @11 | 12.59 / 8.59 |
| U+F011 @16 | 13.59 | U+F001 @18 | 16.80 |
| U+F0928 @16 / @20 | 15.59 / 19.98 | U+F00AF @20 | 11.98 |
| U+F04AE @16 / @26 | 9.59 / 15.59 | U+F040A @31 | 18.59 |
| U+F03E4 @16 | 9.59 | U+F00DF @21.08 | 19.59 |
| U+F0EC3 @15 | 13.00 | "12:34" @16 | 47.97 |
| U+F2DB" 7%" @16 | 38.38 | U+F0C9" 42%" @16 | 47.97 |
| U+F0200" 0 B/s" @16 | 67.16 | U+F2C9" 45°C" @16 | 57.56 |
| "12.3 W" @16 | 57.56 | "WIFI" @11 | 26.38 |
| "Close" @13 | 38.98 | "Turn Off" @13 | 62.38 |
| "C1 12%" @13 | 46.78 | "Bluetooth" / "Network" @17 | 91.69 / 71.31 |
| "系统资源" @15 | 60 (CJK = 1 em) | "＋ 创建日程" @16 | 89.59 |

The engine's text shaper must reproduce these numbers. Use them as unit tests.

---

## 1. Primitives

| # | widget | parameters | used by |
|---|---|---|---|
| P1 | **RoundedRect** | size, `radius: Anim<f64>`, `fill: Anim<Color>`, `border_width`, `border_color`, `opacity`, `clip_children: bool` | everywhere |
| P2 | **Label** | text, font {family, px, weight}, colour, elide, wrap, max_lines, h/v align, letter_spacing, fixed_line_height, render_type; reports implicit size per §0.2 | everywhere |
| P3 | **GlyphIcon** | = Label with icon font; colour; optional vertical offset | all icon glyphs |
| P4 | **ImageIcon** | source (file/SVG/theme name/SNI pixmap), size, raster `source_size`, fit (Fit/Crop), smooth+mipmap, async, `ready` signal | tray, control-panel trigger, Wi-Fi, agent logo, menu icons |
| P5 | **IconWithFallback** | `ImageIcon` + `GlyphIcon` shown until/unless the image is ready; symbolic-source size scale 16/24 and −2 offset | ★ui-bar-layout.md §8.4 (WifiIconWithFallback), ui-bar-layout.md §8.5.1 (TrayButton letter), ui-island.md §3.6 (AgentLogo) |
| P6 | **TintedIcon** | ImageIcon + colorize(colour) | ui-panels-control.md §2.2 (light-dark.svg) |
| P7 | **RoundedImage** | source, size, radius, placeholder fill, fallback glyph {codepoint, px = round(min·k), colour} | ★ui-island.md §3.2 (AlbumArtFrame), ui-panels-control.md §2.4 (media card art) |
| P8 | **ClipBox** | rect clip region, used for two-tone and reveal effects | ui-panels-control.md §2.3, ui-bar-popups.md §2.2 |
| P9 | **CanvasItem** | paint callback with the vector API, repaint on input change | ScreenCornerShade, ring, spectrum, chart |
| P10 | **Flickable / ScrollView** | content size, clip, StopAtBounds, interactive iff overflow, drag + flick physics (deceleration param), wheel policy (default / custom step / custom handler), `content_x/y` bindable, optional pressDelay | ui-panels-control.md §3.2 (BT list), ui-panels-wifi.md §4 (60 px/notch), ui-panels-calendar.md §2.4 (2-axis, infinite paging), ui-bar-popups.md §3.2 (menu), ui-island.md §3.14 (agent list) |
| P11 | **ScrollIndicator** | track rect, w 3, radius 2, colour, height `max(min, ratio·h)`, opacity by state (+Behavior) | ui-island.md §3.14; ui-panels-calendar.md §2.3 item 5 (buggy content-attached variant) |
| P12 | **TextInput** | text, font, colour, letter_spacing, echo Password (mask U+25CF), placeholder Label, focus-dependent border, Return handler, selectByMouse | ui-panels-wifi.md §4 item 6 |
| P13 | **HoverRegion / Tap / Drag recognisers** | hover margin, enter/leave callbacks, tap within bounds, horizontal drag with threshold and release callback | ui-island.md §4.1–4.3, ui-panels-calendar.md §1.3 |

---

## 2. Containers & animation wrappers

| # | widget | parameters | used by |
|---|---|---|---|
| C1 | **GlassPanel** (AnimatedGlassPanel) | width, `full_height`, radius 19, fill, stroke, surface_opacity 0.82, shadow_color (+ blur ≈ 5 px, rect-clipped), reveal timeline {open: pause, reveal_ms, content_delay, fade_ms, slide_ms (no-op), offset; close: pause, reveal_ms, fade_ms, slide_ms, offset}, line_height 2; API `prepare_open / play_open / play_close / reset`, signals open/close finished, "snap to new height when idle" | ★ui-bar-popups.md §2, ui-panels.md §2.1; users: tray menu, battery, control panel, Wi-Fi, stats |
| C2 | **Reveal / ExpandableSection** | full_height, `layout_height_follows_reveal`, line_height (0 for sections), same timeline as C1 but the content slide **is** applied, `expanded` driver with 16 ms retry while height is 0 | ★ui-panels.md §2.2–2.3, ui-bar-popups.md §6 |
| C3 | **GroupPill** | content row, radius 24 (clamped 19), fill moduleBackground, h 38, width = Σ children | ★ui-bar-layout.md §4 |
| C4 | **MorphBox** | rect whose w/h/radius have Behaviors gated by a flag, fill/border toggled by a progress threshold, clip; children cross-faded by `progress` | ★ui-bar-layout.md §6, ui-island.md §1.2–1.3, ui-panels-calendar.md §1.4 (centerSection) |
| C5 | **PopupSurface** | full-output Overlay layer, namespace, OnDemand keyboard while mapped, click-outside → close (all buttons), Esc → close, card swallows clicks (configurable: tray menu doesn't), map/unmap tied to the close-animation end, open timer that waits for content height; positioning strategy enum: `AnchorCentred` (tray menu, battery, overflow), `RightAligned{margin 10}` (control panel, Wi-Fi), `FixedLeft{x 10}` (stats), `AnchorRight{to tray pill, y = bar zone}` with input region = card only (QuickAdjust); vertical: below (+10) with above fallback, 8 px screen margin | ★ui-bar-popups.md §1, ui-panels-control.md §1, ui-panels-wifi.md §2–3, ui-panels-stats.md §1–2 |
| C6 | **CenteredDialogSurface** | unanchored layer surface (centred), exclusive keyboard, fixed width, height from content | ui-panels-calendar.md §4 (StartupConfirm) |
| C7 | **Tooltip card** | width, anchor item rect, below/above placement with 7 px gap and 8 px margin, radius 12, fill, border, column of labels | ui-panels-calendar.md §2.3 item 8 |
| C8 | **Grid / Flow helpers** | columns, row/column spacing; Flow wrapping | ui-bar-popups.md §4, ui-panels-control.md §3.2, ui-panels-stats.md §2.3 |
| C9 | **Repeater with keyed diff** | stable identity insert/move/update/remove, scroll position preserved | ★ui-island.md §5 (AgentTaskModel) |

---

## 3. Controls

| # | widget | parameters | used by |
|---|---|---|---|
| W1 | **TextModule** (bar text button) | label, colour, font, padL/R, min width, module height, interactive, wheel, hoverable (chip `workspaceHoverBackground`), highlighted (chip + text colour), collapseWhenHidden, inset; signals left/right/wheel | ★ui-bar-layout.md §3 |
| W2 | **PillButton** (animated) | label, font, padL/R, h, hover-fill rect (opacity Behavior 120 ms OutCubic) **or** colour Behavior (120 ms linear), press scale (0.96; 120 ms OutCubic / 90 ms linear), emphasized fill, border | ★ui-island.md §3.3 (IslandTextButton), ui-panels-calendar.md §2.2 (HeaderButton) |
| W3 | **CircleGlyphButton** | size 40/44, primary fill vs soft fill + border, glyph px 17/21 (+1 offset), scale press 0.92 / hover 1.06 (120 ms OutCubic), opacity 0.82 when idle | ★ui-island.md §3.4 |
| W4 | **GlyphButton** (no chrome) | glyph, px, colour, bottom-anchored margin, width, opacity when disabled | ui-panels-control.md §2.4 (MediaButton), ui-bar-layout.md §8.5 (overflow ⋯), §8.7 (notification + dot) |
| W5 | **ActionChip** | label or icon, min width, h 34, corner radius, fill / fg / stroke, disabled colours + opacity 0.56, hover lighter 1.06 + stroke fg α0.22, press darker 1.08; width = max(min, contentW + 24) | ★ui-panels.md §2.4 (WifiActionChip); variant InlineChip ui-panels-control.md §8 |
| W6 | **Badge** | 22×20, radius, fill, border, glyph | ui-panels.md §2.5 |
| W7 | **ToggleTile** | icon glyph or tinted SVG, label (shown iff !icon_only), active, destructive, enabled; derived sizes from (w,h); fill/stroke state machine (disabled α0.48 + opacity 0.56, pressed darker 1.06, hover lighter 1.04) | ★ui-panels-control.md §2.2; WindowsVmControl/Viewflow/Deskflow §4–6 |
| W8 | **SplitTile** | icon, title, subtitle, active, chevron (expanded), two click zones (`contentStartX`), icon well with press/hover | ★ui-panels-control.md §2.1; MihomoControlTile §7.1 |
| W9 | **SliderTile** | value 0–100, preview while dragging, icon two-tone split at the fill edge (white over fill, accent α over empty), icon x (centre rule or fixed inset), icon hit box, signals change/finish/iconClick | ★ui-panels-control.md §2.3; QuickAdjust ui-panels-stats.md §1.3 |
| W10 | **Switch** | 48×28 track, 22 px knob, on/off fill + stroke ColorAnimation 140 ms, knob x Behavior 160 ms InOutQuad, disabled opacity 0.58 | ui-panels-control.md §3.2 |
| W11 | **SegmentedRow** (mode buttons) | n options, equal widths, selected fill/border/text | ui-panels-control.md §7 (Mihomo) |
| W12 | **DialogButton** (Qt Basic Button look) | 88×32, radius 8, normal/hover/pressed fills, keyboard focus ring (2 px accent at −3 px, radius 11), Return/Enter/Space activation | ui-panels-calendar.md §4.3 |
| W13 | **MenuRow** | 34 h, radius 11, hover fill, check/radio indicator text, 16 px icon, label elide, submenu arrow, disabled α0.55; separator variant 1 px | ui-bar-popups.md §3.2 |
| W14 | **TrayButton** | 18×38, icon 18 px or fallback letter, left/right/wheel → SNI actions | ui-bar-layout.md §8.5.1 |

---

## 4. Data visualisation & decoration

| # | widget | parameters | used by |
|---|---|---|---|
| V1 | **ProgressTrack** (seekable) | track h 5, radius h/2, fill min width, width Behavior 180 ms OutCubic disabled while seeking, enlarged hit area (+8), press/move/release preview, 700 ms preview hold | ui-island.md §3.11 |
| V2 | **ProgressRing** | size, stroke 3, track colour, ring colour, start −90°, clockwise, round cap, centre glyph overlay | ui-island.md §3.5 |
| V3 | **SpectrumBars** | values, bar_count, min ratio, gain, height range scale, colours (bar/peak/quiet), gap rule, quadratic-corner bars, idle sine wave timer 66 ms (+0.28 phase) only while visible | ui-island.md §7 |
| V4 | **TrendChart** | samples, max/auto-scale ×1.15, frame (radius 10, opacity 0.82), 4 grid lines, gradient area, 2.25 px round polyline, end dot r 3, 10 px plot padding, 2 px inset | ui-panels-stats.md §3 |
| V5 | **InverseCorner** (ScreenCornerShade) | size 27, colour, corner orientation (mirroring) | ui-bar.md §9.3 |
| V6 | **TimelineGrid** (calendar) | hour height 44, day width, 21-day buffer, line colour, lane-packed event blocks, all-day lane chips, now line + dot, time-column labels synced to scroll | ui-panels-calendar.md §2.3–2.5 |

---

## 5. Composites (built in `hyprv-shell`, not the kit)

| composite | built from | spec |
|---|---|---|
| Bar window (left/centre/title/right sections, tray layout budget) | C3, W1, W4, W14, P5, C4 | ui-bar-layout.md |
| Screen corners | V5 × 4 tiny surfaces | ui-bar.md §9 |
| Dynamic Island (idle / compact / expanded music / agent / dual) | C4, W2, W3, P7, V1, V2, V3, C9, P10, P11, P13 | ui-island.md |
| Tray menu / overflow / battery popups | C5 + C1, W13, W14, C8, P2 | ui-bar-popups.md |
| Control panel (main page, Bluetooth page, Mihomo page) | C5 + C1, W7, W8, W9, media card (P7 + W4), W5, W10, P10, W11 | ui-panels-control.md |
| Wi-Fi popup | C5 + C1, W5, W6, P5, C2, P12, P10 | ui-panels-wifi.md |
| QuickAdjust OSD | C5 (AnchorRight, input = card), W9 + opacity/scale Behaviors | ui-panels-stats.md §1 |
| System stats popup | C5 + C1 (with shadow), V4, C8 | ui-panels-stats.md §2 |
| Week calendar | C4 host, W2, V6, P10, C7 | ui-panels-calendar.md §1–2 |
| Startup confirm | C6, W12, P2 | ui-panels-calendar.md §4 |

## 6. Suggested build order
1. §0 foundations, with the §0.2 metrics table as text-layout unit tests.
2. P1–P4, P8, P9.
3. C1, C2, C5, W1, C3.
4. Bar composites (fastest visible parity).
5. W2–W9 plus V1–V3 (island and control panel).
6. P10–P13, V4, V6, W10–W14 (lists, inputs, charts, calendar, menu).
