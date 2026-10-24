# Text rendering — reproducing Qt Quick text pixel-exactly

Status: research done, recipe verified. On 19 samples (bar clock, icon glyphs, a window title,
Chinese battery and calendar strings, letterSpacing, elide), at dpr 2 and 1.5, in both themes:
- **NativeRendering text: 0 differing pixels** with the FreeType-direct recipe (§5.2).
- **QtRendering (distance-field) text: 0.6–1.6 % ink error**, mean |Δ| ≤ 0.32 of 255, worst
  single pixel 26/255, with the SDF emulation (§5.3).
- The current hyprv-ui approach (skia `Paragraph` on a scaled canvas) is 17–207 % ink error.
  Do not use it for glyph rasterisation.

Tools and raw data: `rshell/experiments/text/` (`run.sh` reproduces every number here).
Qt sources: tag `v6.12.0`, the version installed here (`qt6-base 6.12.0`,
`qt6-declarative 6.12.0`, `quickshell 0.3.2`). They are cited as `file.cpp:line`.

---

## 1. What the live shell uses (environment facts)

### 1.1 Render types in the QML
- No `//@ pragma NativeTextRendering` (Quickshell supports it but `shell.qml:1` only has
  `UseQApplication`), and nothing calls `QQuickWindow::setTextRenderType`. So the default for
  any item without an explicit `renderType` is **`Text.QtRendering` = distance field**.
- **85 Text items set `renderType: Text.NativeRendering`.** That covers the whole bar, the
  island and most panels.
- **27 text items have no renderType and therefore use distance-field text:**
  - `WeekCalendarPanel.qml`: 440, 454, 501, 527, 620, 629, 696, 718, 751, 760, 767, 776
  - `MihomoControlPanel.qml`: 115, 128, 136, 149, 173, 176, 177, 200, 210, 215, 220, 231
    (`font.letterSpacing: 3` is used here)
  - `TrayButton.qml:28` (the fallback letter)
  - `ControlPanelWifiDetails.qml:255` (the password `TextInput`)
  - `StartupConfirm.qml` 75, 84, 135. This file is never instantiated.
- `Text.CurveRendering` is not used.

**The port must implement both pipelines** and choose per item from the spec's renderType.

### 1.2 Process environment of the running quickshell (`/proc/<pid>/environ`)
`QT_QPA_PLATFORM=wayland`, `QT_QPA_PLATFORMTHEME=qt6ct`, `QT_AUTO_SCREEN_SCALE_FACTOR=1`
(obsolete in Qt 6, no effect), `QT_WAYLAND_DISABLE_WINDOWDECORATION=1`,
`QT_IM_MODULE=fcitx`, `QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1`, and from `scripts/launch.sh:10`
`QSG_RENDER_LOOP=basic`.

There is no `QT_SCALE_FACTOR`, no `QSG_RHI_BACKEND`, and no `QT_DF_*` or
`QT_DISTANCEFIELD_*` variables, so all Qt defaults apply. The scene graph is RHI/OpenGL
(graphics API 3, measured).

### 1.3 Output scale and Qt's high-DPI mode
- DP-4 is scale 2 (`hypr/hyprland-monitors.lua`), so the window dpr is **2**. The headless
  outputs are also scale 2.
- On the Wayland QPA, Qt receives the dpr from the platform window, so
  **`QHighDpiScaling::isActive()` is false** (measured). This matters only at dpr 1 (§3.3).
  A harness that sets `QT_SCALE_FACTOR` makes it true.

### 1.4 fontconfig in effect
The source is `~/.config/fontconfig/fonts.conf`, which forces settings for every font
(`fc-match -v` on each family/weight used):

| key | value |
|---|---|
| antialias | true |
| hinting | true |
| hintstyle | 1 = `hintslight` |
| autohint | true |
| rgba | 5 = none (grayscale) |
| lcdfilter | 0 = `lcdnone` |
| embeddedbitmap | true (the TTFs used have no bitmaps) |
| dpi | 192 (also `Xft.dpi: 192`) |

Qt reads antialias, hintstyle, autohint, rgba and lcdfilter. **For text at dpr ≠ 1 none of the
hinting settings take effect** (§3.2). `LANG=zh_CN.UTF-8`.

### 1.5 Font files actually used
- `"JetBrainsMono Nerd Font"` → `/usr/share/fonts/TTF/JetBrainsMonoNerdFont-{Regular,Medium,SemiBold,Bold}.ttf`.
  Qt weight → fontconfig weight: 400 → 80, 500 → 100, 600 → 180, 700 → 200. This is a
  separate static file per weight, with `FT_FACE_FLAG_FIXED_WIDTH` set.
- **CJK fallback = `HarmonyOS Sans SC` ("鸿蒙黑体")**, the *variable* font
  `/usr/share/fonts/harmonyos-sans/HarmonyOS_Sans_SC.ttf`. fontconfig returns named-instance
  indices: Regular = 0, SemiBold = `393216` (instance 6 << 16), Bold = `458752` (instance 7).
  Qt reported these fonts in its glyph runs (measured with QTextLayout glyph runs).
- The `"sans-serif"` / `"monospace"` / `""` fallbacks in the QML only apply when `shellRoot`
  is null, i.e. never.

---

## 2. Layout: how Qt measures and positions a line

All values below are in logical px and were verified against Qt's own glyph positions
(max |Δx| 0.0005 px, which is print rounding).

1. **Shaping.** HarfBuzz with default features (calt/liga/kern on). JetBrains Mono's
   contextual alternates (e.g. `:` between digits) therefore apply. Skia's textlayout shaper
   produces identical glyph ids.
2. **Font itemisation** (QFontEngineMulti). For each character, use the primary family
   (JetBrains Mono) if it has a glyph, otherwise the fallback (HarmonyOS Sans SC). Digits and
   punctuation inside Chinese strings stay in JetBrains Mono, e.g. `3小时5分钟`. The `…`
   used for elision always comes from the primary font (§2.7).
3. **Advances: design metrics, truncated to 1/64 px.**
   `advance = (FT linearHoriAdvance(16.16) >> 10) / 64`, from a face sized with
   `FT_Set_Char_Size(px·64)`.
   - JetBrains Mono at 16 px gives 9.59375 (not 9.6). At 14 px: 8.390625. At 17 px: 10.1875.
   - HarmonyOS at 12/13 px gives exactly 12/13.
   - Why design metrics: `QFontEngineFT::shouldUseDesignMetrics` is true for HintNone and
     HintLight (`qfontengine_ft.cpp:2527-2533`), and `hintslight` maps to HintLight.
     **If someone switches fontconfig to `hintfull`/`hintmedium`, NativeRendering layout would
     switch to hinted integer advances.** QtRendering always uses design metrics
     (`qquicktext.cpp:514`).
4. **Ascent and descent.** `ascent = floor(hhea.ascender · px / upem · 64) / 64`; descent
   likewise.
   - JB 16 px: 16.3125 / 4.796875. 17 px: 17.328125. 13 px: 13.25 / 3.890625.
   - **Line ascent = max over the primary font and every fallback font used on the line.**
     The primary font always counts, so an all-Chinese line has JetBrains Mono's ascent
     (12 px: 12.234375). Getting this wrong shifted CJK text by one device row in the
     experiment.
5. **Baseline** of a single-line Text = item top + line ascent. It is not rounded at layout
   time; native text rounds it at raster time (§3.4).
6. **Implicit size** (single line):
   - `implicitHeight = ceil(ascent + descent)` (`qquicktext.cpp:464`, "QTextLine rounds up").
     16 px → 22, 17 → 23, 14 → 19, 13 → 18, 12 → 16, 10 → 14, 9 → 12, 24 → 32.
   - `implicitWidth = naturalTextWidth = Σ advances + max(0, −rightBearing(last glyph))`.
     The right bearing comes from the font engine's (hinting-rounded) glyph metrics:
     `overhang = ceil(ink right edge px) − round(advance px)`.
   - This matters for Nerd icons, which overflow their 0.6 em advance. Examples:
     - U+F028 at 16 px: 9.594 + 9 = **18.594**
     - U+F303 at 17 px: 10.188 + 7 = **17.188**
     - U+F0A2 at 14 px: 8.391 + 4 = **12.391**
   - The full measured table for every bar label is in
     `experiments/text/out/bar-labels.png` + `run.sh` output.
     `ui-bar-layout.md` has already been corrected to these values.
7. **Elide (ElideRight, single line)** — `QTextEngine::elidedText`, `qtextengine.cpp:3216-3336`.
   1. Only when `naturalTextWidth > width` and there is more than one character.
   2. `ellipsis` = U+2026 from the **primary** font (else `"..."`, 3 × '.').
   3. `available = width − ellipsisAdvance`; if negative, the text becomes empty.
   4. Walk graphemes, adding their advances while `width < available`. The grapheme that
      reaches the limit is **dropped**.
   5. The result is `prefix + "…"`, laid out again from scratch.
   - Verified exact for a Latin title and a CJK string (where the `…` is JetBrains Mono).
8. **letterSpacing** (`font.letterSpacing`) adds absolute logical px after **every** glyph,
   including the last one (`qtextengine.cpp:1567-1585`; the dpi factor is 1 here). It counts
   in the implicit width. Measured: 3 px at 16 px gives an advance of 12.594.

---

## 3. NativeRendering pipeline (Qt 6.12, RHI)

### 3.1 Glyph cache transform
`QSGTextMaskMaterial` builds a glyph cache with transform `scale(dpr, dpr)`, so glyphs are
rasterised at device size. When the font engine loads a glyph under such a transform:
`if (t.type() >= QTransform::TxScale) default_hint_style = HintNone; // disable hinting if
the glyphs are transformed` (`qfontengine_ft.cpp:2829-2830`).

### 3.2 Consequence at dpr ≠ 1: unhinted, grayscale FreeType
1. `FT_Set_Char_Size(face, px·64)` at the *logical* size.
2. `FT_Set_Transform(matrix = dpr·I, delta = (subpixel·64, 0))`.
3. `FT_Load_Glyph(FT_LOAD_DEFAULT | FT_LOAD_NO_HINTING | FT_LOAD_NO_BITMAP [| FORCE_AUTOHINT,
   ignored])` (`:1139-1170`, `:1832-1835`).
4. Synthetic bold if needed (§3.5).
5. `FT_Render_Glyph(FT_RENDER_MODE_NORMAL)` produces an 8-bit coverage (A8) mask
   (`:1960-1975`).

So fontconfig's `hintslight` and `autohint` have **no effect** on the shell's glyph shapes
at scale 2 or 1.5.

### 3.3 dpr = 1 (untested here, from source)
There is no transform, so HintLight applies: `FT_LOAD_TARGET_LIGHT | FT_LOAD_FORCE_AUTOHINT`
plus `FT_RENDER_MODE_LIGHT` (vertical-only autohinting).

Under `QT_SCALE_FACTOR` (`isActive()` true) Qt would instead use HintNone
(`qfontconfigdatabase.cpp` `defaultHintStyleFromMatch`). Reference captures at dpr 1
therefore must **not** use `QT_SCALE_FACTOR`, or the port must mirror whichever mode the
real output uses. The user has no scale-1 output today.

### 3.4 Horizontal subpixel positioning and placement
FT engines report `supportsHorizontalSubPixelPositions()` for HintLight/HintNone, with
`m_subPixelPositionCount = 4` (`qfontengine_ft.cpp:943`).

Per glyph (`qsgdefaultglyphnode_p.cpp:400-470`, `qfontengine.cpp:905-920`,
`shaders_ng/textmask.vert`):
```
lx      = round64(local glyph x)                 // node-local, QFixedPoint::fromPointF
dx      = lx · dpr
sub     = dx == 0 ? 0 : floor((frac(dx) + 1/64) · 4) / 4     // 0, .25, .5, .75 (1.0 possible)
by      = round(round64(local baseline y) · dpr)
glyph   = FreeType bitmap rendered with delta = sub (above)
device x = floor(itemX·dpr + 0.5) + floor(dx) + bitmap_left
device y = floor(itemY·dpr + 0.5) + by − bitmap_top
```
- The item's own (possibly fractional) scene position is snapped **separately** by the
  vertex shader: `floor(xformed·dpr + 0.5)/dpr`. Here `itemX/itemY` = the Text's scene
  position.
- The quad samples the A8 texture 1:1, so there is no filtering.

### 3.5 Synthetic bold (`qfontengine_ft.cpp:1016-1023`, applied at `:1882`)
Qt emboldens a glyph when all of these hold:
- requested weight ≥ 700
- the face lacks `FT_STYLE_FLAG_BOLD`
- the face is not fixed-width
- `calculateActualWeight() < 700`
- `px < 64`

For HarmonyOS Sans SC (variable font), Qt gets no named-instance index here and falls back
to the OS/2 `usWeightClass` of the default instance (400). So **bold Chinese text is the Bold
named instance plus `FT_GlyphSlot_Embolden`**. This was verified: without the embolden step,
CJK bold differed by 15 % ink; with it, 0. JetBrains Mono is never emboldened (it is
fixed-width and its Bold file has the bold flag).

### 3.6 Compositing
Fragment shader: `color · coverage` (`8bittextmask.frag`), premultiplied source-over, 8-bit
render target. There is **no gamma or contrast correction**: the alpha planes of the light
(#2b2b2c) and dark (#cdd6f4) renders differ in 2 pixels by 1 level. Compare images
premultiplied, because straight-alpha RGB at low alpha is noisy (up to 43 levels).

---

## 4. QtRendering (distance-field) pipeline

1. **Base size.** 54 px (`QT_DISTANCEFIELD_DEFAULT_BASEFONTSIZE`), or 108 when the font has
   "narrow outlines" (1-px strokes at 54 px) and fewer than 2000 glyphs (`qsgadaptationlayer.cpp:42`).
   Both fonts here: not narrow, so base **54** (measured via `qt_fontHasNarrowOutlines`).
   `fontScale = px / 54`.
2. **Field.** The glyph outline comes from `QRawFont::pathForGlyph` at 54 × 16 = 864 px,
   scaled by 1/16 (unhinted, not emboldened). The path is unioned (`qSimplifyPath`), so
   overlapping CJK strokes have no inner edges. Then `makeDistanceField`
   (`qdistancefield.cpp:458-640`) builds the field:
   - Texture = path bbox + 5 base px margin on each side
     (`QT_DISTANCEFIELD_RADIUS 80 / SCALE 16`); texel 0 = bbox top-left − 5.
   - Value: `byte = (0x7f80 − v) >> 8`, with `v` an 8.8 fixed signed distance clamped to
     ±127.5 over ±5 base px. So 255 deep inside, 0 outside, ~127.5 on the outline, and a
     linear ramp. Corners are approximated with 15° fan steps.
3. **Quad.** The glyph quad is mapped *linearly* from base-size coordinates
   (`qsgdistancefieldglyphnode.cpp:221-301`): logical = glyph origin + base coord ·
   fontScale, plus a 2 px margin (capped at 5 base px). There is **no pixel snapping** of
   positions or baseline (`distancefieldtext.vert`).
4. **Shading** (`distancefieldtext.frag`, `qsgdistancefieldglyphnode_p.cpp:22-35,113-118`):
   ```
   a = smoothstep(alphaMin, alphaMax, texture(sdf, uv).r)   // GL_LINEAR on the 8-bit field
   fragColor = color · a
   s = fontScale · matrixScale(= dpr)
   base  = 0.5 − ((clamp(s, .15, .3) − .15)/.15 · −0.065 + 0.065)
   range = 0.06 / s
   alphaMin = max(0, base − range); alphaMax = min(1, base + range)
   ```
   | px | dpr 1 | dpr 1.5 | dpr 2 |
   |---|---|---|---|
   | 8 | 0.030–0.840 | 0.196–0.736 | 0.296–0.701 |
   | 9 | 0.082–0.802 | 0.238–0.718 | 0.320–0.680 |
   | 10 | 0.126–0.774 | 0.274–0.706 | 0.338–0.662 |
   | 11 | 0.164–0.753 | 0.304–0.696 | 0.353–0.647 |
   | 12 | 0.196–0.736 | 0.320–0.680 | 0.365–0.635 |
   | 13 | 0.225–0.724 | 0.334–0.666 | 0.375–0.625 |
   | 14 | 0.251–0.714 | 0.346–0.654 | 0.384–0.616 |
   | 16 | 0.296–0.701 | 0.365–0.635 | 0.399–0.601 |
   | 20 | 0.338–0.662 | 0.392–0.608 | 0.419–0.581 |
5. **Look.** Softer, slightly lighter edges than FreeType gray AA. Each glyph sits at its
   exact fractional position (no snapping). There is no synthetic bold in this path.
6. **Settings that would change it:** the env vars `QT_DF_BASE`, `QT_DF_BASEDEVIATION`,
   `QT_DF_SCALEFORMAXDEV`, `QT_DF_SCALEFORNODEV`, `QT_DF_RANGE` and
   `QT_DISTANCEFIELD_DEFAULT_{BASEFONTSIZE,SCALE,RADIUS,HIGHGLYPHCOUNT}`, and
   `Text.renderTypeQuality`. None are set or used.

---

## 5. Recommended implementation for hyprv-ui

### 5.1 Why skia's own text stack can't match
- **`Paragraph.paint` on a canvas scaled by dpr** (today's hyprv-ui): 17–207 % ink error.
  Causes:
  - advances of 9.6 instead of 9.59375
  - CJK ascent from the fallback font
  - no synthetic bold
  - skia's hinting/subpixel rounding
  - no item-offset snapping
- **skia `Font` with Qt placement** (`hinting None`, `Edging::AntiAlias`, `subpixel true`,
  `linear_metrics true`, glyphs drawn at Qt's exact device positions): 5–10 % ink error, max
  ~51–58 levels. The remaining error is skia's own scan converter versus FreeType's
  `FT_Render_Glyph`, and skia can't do FreeType embolden (CJK bold reached 22 %). Use this
  only as a fallback.

### 5.2 NativeRendering (exact)
1. **Font resolution** like Qt/fontconfig: `FcFontMatch` with family + fc weight. Keep the
   returned file *and index*, including named-instance bits; open with
   `FT_New_Face(file, index)`. Fallback chain: the primary family, then HarmonyOS Sans SC
   (from `FcFontSort`, i.e. `fc-match -s "JetBrainsMono Nerd Font:lang=zh-cn"`).
2. **Shaping**: HarfBuzz for glyph ids (skia textlayout `Paragraph::visit` or
   `skia_safe::Shaper` is fine). Itemise per character as in §2.2.
3. **Metrics**: advances, ascent, descent, overhang, implicit size, elide and letterSpacing
   exactly as in §2, using FreeType values.
4. **Raster**: a FreeType A8 glyph atlas keyed by
   `(face, glyph, px, dpr, subpixel bucket, embolden)`. Render per §3.2–3.5 and place per
   §3.4.
5. **Draw**: blit the A8 masks at integer device coordinates as alpha masks modulated by the
   colour, with premultiplied src-over. In skia: A8 `Image`s / `draw_atlas`, or
   `draw_image_rect` with nearest sampling and an identity or integer-translate matrix.
   Never draw through a fractional transform.
6. **Opacity** from ancestors multiplies the colour alpha (Qt passes `state.opacity()` into
   the colour).
7. **Performance**: glyphs are rasterised once per atlas key; redraw is a cheap textured-quad
   blit, which preserves the zero-idle-wakeup goal.

Reference implementation: `experiments/text/skia-text/src/main.rs` — `resolve`, `layout`,
`layout_elided`, `ft_linear_advance`, `qt_native_place`, `qt_embolden`, `render_ft`.

### 5.3 QtRendering (≈1 % residual)
1. Build each glyph's field as in §4.2. The experiment uses skia: `Font::get_path` at 864 px
   scaled by 1/16, `Path::simplify()` (pathops union), then exact Euclidean distance to the
   densely flattened outline, encoded with Qt's formula.
   - Latin fields match Qt's `QDistanceField` bytes to ≤ 4 levels; CJK to ≤ 3.
   - Exact parity would need porting `makeDistanceField` (polygon ramps + 15° fans). Note
     Qt's LGPL/GPL licence before copying code.
2. Store the fields in an A8 atlas and draw quads per §4.3 with a skia `RuntimeEffect`:
   bilinear sample + `smoothstep(alphaMin, alphaMax, d) · color`. Use GPU linear filtering
   to mirror Qt's GL sampling.
3. Residual sources: field bytes (≤ 4 levels), a sub-1/100 px bbox difference, and
   interpolation precision.

### 5.4 Things to keep in the engine API
- Text nodes need: `render_type`, `family`, `pixel_size`, `weight`, `letter_spacing`,
  `elide` + `width`, and colour.
- Text nodes must report `implicit_width/height` per §2.6, because layouts depend on them.
- The scene position used for native snapping is the item's absolute device position, not
  a parent-relative one.

---

## 6. Residual diff numbers

Columns are alpha-plane differences in the item rect + 3 px:
- mean = mean |Δ| in 8-bit levels
- max = max |Δ|
- >8 = % of pixels with |Δ| > 8
- ink = Σ|Δ| / Σ Qt alpha

**dpr 2, light** (dark is identical to within print rounding):

| sample | mode | para (today) | skia+Qt placement | **FreeType (§5.2)** | **SDF (§5.3)** |
|---|---|---|---|---|---|
| clock `12:34` 16 Bold | native | 8.98 / 255 / 29.1 % | 1.99 / 51 / 6.4 % | **0 / 0 / 0 %** | — |
| cpu ` 7%` | native | 33.2 % | 6.8 % | **0** | — |
| arch U+F303 17 px | native | 20.8 % | 5.3 % | **0** | — |
| volume U+F028 | native | 21.7 % | 5.3 % | **0** | — |
| window title | native | 36.9 % | 6.7 % | **0** | — |
| clock at x=200.9, y=121.3 | native | 31.8 % | 6.4 % | **0** | — |
| 正在充电 12 SemiBold | native | 86.6 % | 9.1 % | **0** | — |
| 当前净功率 13 SemiBold | native | 101.7 % | 9.3 % | **0** | — |
| 预计剩余使用时间 3小时5分钟 13 Bold | native | 33.6 % | 21.8 % | **0** | — |
| menu `Open Settings…` 14 Regular | native | 62.0 % | 10.0 % | **0** | — |
| letterSpacing 3, `12:34` | native | 120.4 % | 6.4 % | **0** | — |
| elided title (width 150.5) | native | 128.9 % | 8.2 % | **0** | — |
| elided 已接通电源，当前未充电 (width 100) | native | 207.1 % | 9.5 % | **0** | — |
| calendar `12:34` 16 Bold | qt (DF) | 17.5 % | 14.9 % | 13.0 % | **0.23 / 20 / 0.7 %** |
| 正在连接日历… 12 | qt (DF) | 31.5 % | 32.9 % | 30.9 % | **0.32 / 26 / 0.9 %** |
| 一 二 三 四 五 11 | qt (DF) | 40.0 % | 41.9 % | 41.1 % | **0.09 / 13 / 0.6 %** |
| `23` 14 | qt (DF) | 47.6 % | 29.1 % | 27.9 % | **0.32 / 18 / 1.4 %** |
| tray fallback `D` 10 | qt (DF) | 17.0 % | 20.0 % | 18.4 % | **0.26 / 16 / 1.6 %** |
| `MIHOMO ON` 9 Bold, letterSpacing 3 | qt (DF) | 130.8 % | 18.3 % | 15.7 % | **0.17 / 12 / 0.8 %** |

**dpr 1.5** (Qt at `QT_SCALE_FACTOR=0.75` on the scale-2 screen): FreeType is **0 on every
native sample**; SDF is 0.6–1.4 % ink with max 19.

Field check against Qt's own `QDistanceField` bytes: glyph '1' max 2, '2' max 4, '正' max 3
(after the pathops union; 55 before it).

---

## 7. Notes for the parity harness (`rshell/parity`, owned by spec-island)

- **Use a hardware RHI backend.** With `QT_QPA_PLATFORM=offscreen` Qt falls back to the
  software scene graph (graphics API 1). That draws QtRendering text with QPainter instead of
  distance fields (18–31 % ink error against the live look) and places fractional-position
  native text differently (up to 38 %).
- `parity-host` already has `PARITY_REQUIRE_RHI`; keep it on.
- A read-only way to get RHI with the true output scale: `QT_QPA_PLATFORM=wayland` with the
  window never shown, and `QQuickWindow::grabWindow()` renders offscreen. This is what
  `experiments/text/qt-ref` does (`TEXTREF_NO_SHOW=1`); nothing appears on screen.
- `QT_SCALE_FACTOR` is harmless for text at dpr ≠ 1 (hinting is off either way). At dpr 1 it
  flips native text from hintslight to unhinted (§3.3).
- Compare premultiplied pixels, or alpha only for single-colour text.

---

## 8. Open items
- dpr 1 hinted path (§3.3) is not verified (no scale-1 output available). The FreeType
  recipe would add `FT_LOAD_TARGET_LIGHT | FT_LOAD_FORCE_AUTOHINT` + `FT_RENDER_MODE_LIGHT`
  and drop the transform.
- Colour glyphs (Apple Color Emoji, via fontconfig fallback) are not on any current surface.
  Qt would use `Format_ARGB` + `FT_LOAD_COLOR`. Not covered.
- Multi-line wrapped text (calendar description, Mihomo message, battery popup `WordWrap`):
  same per-line rules. Line pitch = `ceil(ascent+descent)` × `lineHeight` (or `lineHeight` px
  in `FixedHeight` mode, `qquicktext.cpp:718,826`). Wrapping breaks were not exercised in the
  experiment.
- `TextInput` (password field) uses the same distance-field path; the cursor and selection
  are not covered.
