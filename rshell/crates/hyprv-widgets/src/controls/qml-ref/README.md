# Widget-level QML references (controls/ + viz/)

Reference PNGs rendered from the **original** quickshell QML components, one widget per
image, so `hyprv-widgets` controls and visualisations can be diffed individually instead of
only through whole-island captures.

- `fixtures/*.json` — one widget state each (component file or inline component, size,
  bound properties, props, pointer steps).
- `gen.py` — turns a fixture into a parity-host scene: `"kind": "file"` instantiates
  `quickshell/<file>` unmodified via `Qt.createComponent`; `"kind": "inline"` splices the
  inline `component X:` block **verbatim** from its source file. The scene root carries the
  `shell.qml` theme tokens copied verbatim at generation time (pillOpacity … statsHistoryLimit,
  `withAlpha`, `usageSeverityColor`), so token changes upstream are picked up on re-capture.
- `capture.sh` — runs the shared parity host (`rshell/parity/build/parity-host`, override with
  `PARITY_HOST=`) under a **private headless weston** (never the live session), themes
  light+dark × scales 1+2 → `reference/<fixture>--<theme>@<scale>.png` (straight-alpha RGBA).

```sh
crates/hyprv-widgets/src/controls/qml-ref/capture.sh                 # everything
crates/hyprv-widgets/src/controls/qml-ref/capture.sh chart-cpu-120   # one or more fixtures
```

Tests: `controls::tests` and `viz::tests` (`cargo test -p hyprv-widgets`) render each widget
offscreen in the same state and compare premultiplied pixels (`controls/testutil.rs`):
mean |Δ| per channel (0–255) and the fraction of pixels whose worst channel differs by > 48.
Outputs (`*.mine.png`, `*.ref.png`, `*.diff.png`, `parity.tsv`) go to
`$HYPRV_WIDGET_TEST_OUT` (default `$TMPDIR/hyprv-widgets-test`). Island crops use
`rshell/parity/reference/island` directly.

## Harness findings (keep in mind when adding fixtures)
- Fixture `props` arrays reach QML as QVariantList (`Array.isArray` is false). `SystemTrendChart`
  tests `Array.isArray`, so it drew nothing until `applyProps` converted with `Array.from`.
- The render-control window is never active → `activeFocus`/`visualFocus` can't be captured
  (DialogButton focus ring is tested against the spec only).
- Components importing `Quickshell.*` (TrayButton → `Quickshell.Widgets`, MihomoControlPanel →
  `Quickshell.Io`) can't load in the host; those widgets are spec-tested only.
- The live-host font guard rejects "HarmonyOS Sans SC" as a family name; fixtures list only
  JetBrainsMono, fontconfig still resolves the CJK fallback.

## Coverage
| widget | reference |
|---|---|
| W1 TextModule | `tm-*` (inline component from shell.qml) |
| W2 PillButton | `hb-*` (inline HeaderButton) + island `idle`, `idle-hover-lock` crops |
| W3 CircleGlyphButton | island `music-expanded-playing` crop |
| W4 GlyphButton | `media-*` (ControlPanelMediaCard.qml) crop of the controls row |
| W5 ActionChip, W6 Badge | `chip-*`, `badge-lock` |
| W7 ToggleTile | `toggle-*` |
| W8 SplitTile | `split-*` |
| W9 SliderTile | `slider-*` |
| W10 Switch | `bt-header-*` (ControlPanelBluetoothDetails.qml) crop |
| W12 DialogButton | `db-*` (inline StartupConfirm ActionButton) |
| W11 SegmentedRow, W13 MenuRow, W14 TrayButton | spec-only (not loadable / not a component) |
| V1 ProgressTrack, V2 ProgressRing | island `music-expanded-playing`, `agent-compact-running`, `agent-compact-progress-zero` crops |
| V3 SpectrumBars | `spectrum-*` (AudioSpectrum.qml) |
| V4 TrendChart | `chart-*` (SystemTrendChart.qml) |
| V5 InverseCorner | `corner-*` (ScreenCornerShade.qml) |
| V6 TimelineGrid | `calendar-week` (WeekCalendarPanel.qml with a bridge mock), body below the toolbar |

## Results (2026-10-23, all passing)
236 comparisons over 59 fixtures/crops (light/dark × scale 1/2); median mean |Δ|
0.23, worst 3.94 (0–255 scale). Thresholds: geometry/colour fixtures mean ≤ 3.0
& bad ≤ 3 %; text-dominated fixtures mean ≤ 4.5 & bad ≤ 4.5 % (residual = glyph rasterisation;
ink centroids agree within 0.05 px).

| fixture | variants | worst mean | worst bad-px | median mean |
|---|---|---|---|---|
| `badge-lock` | 4 | 2.47 | 1.74 % | 1.11 |
| `bt-header-off-switch` | 4 | 1.01 | 0.33 % | 0.93 |
| `bt-header-on-switch` | 4 | 2.02 | 0.42 % | 1.48 |
| `calendar-body` | 4 | 0.31 | 0.19 % | 0.25 |
| `chart-cpu-120` | 4 | 0.39 | 0.03 % | 0.32 |
| `chart-empty` | 4 | 0.09 | 0.00 % | 0.06 |
| `chart-memory-120` | 4 | 0.49 | 0.01 % | 0.38 |
| `chart-network-auto` | 4 | 0.42 | 0.02 % | 0.39 |
| `chart-one-sample` | 4 | 0.50 | 0.00 % | 0.39 |
| `chip-close` | 4 | 1.79 | 1.27 % | 0.69 |
| `chip-close-hover` | 4 | 1.73 | 1.27 % | 0.85 |
| `chip-close-pressed` | 4 | 1.74 | 1.27 % | 0.70 |
| `chip-icon-connect` | 4 | 0.66 | 0.50 % | 0.40 |
| `chip-scan-disabled` | 4 | 0.35 | 0.00 % | 0.34 |
| `chip-turnoff-accent` | 4 | 0.09 | 0.00 % | 0.08 |
| `corner-bottomLeft` | 4 | 0.23 | 0.00 % | 0.23 |
| `corner-bottomRight` | 4 | 0.25 | 0.00 % | 0.25 |
| `corner-topLeft` | 4 | 0.22 | 0.00 % | 0.22 |
| `corner-topRight` | 4 | 0.25 | 0.00 % | 0.25 |
| `db-launch-hover` | 4 | 1.78 | 2.24 % | 1.73 |
| `db-launch-pressed` | 4 | 1.69 | 2.19 % | 1.64 |
| `db-skip` | 4 | 3.00 | 3.50 % | 2.86 |
| `hb-create` | 4 | 0.17 | 0.03 % | 0.14 |
| `hb-next-pressed` | 4 | 0.14 | 0.09 % | 0.11 |
| `hb-prev-hover` | 4 | 0.14 | 0.10 % | 0.07 |
| `hb-range` | 4 | 0.06 | 0.00 % | 0.03 |
| `island:agent-compact-progress-zero[352,17,24,24]` | 4 | 1.72 | 2.08 % | 1.67 |
| `island:agent-compact-running[352,17,24,24]` | 4 | 1.42 | 0.61 % | 0.95 |
| `media-idle-buttons` | 4 | 0.13 | 0.00 % | 0.12 |
| `media-paused-buttons` | 4 | 0.08 | 0.00 % | 0.07 |
| `media-playing-buttons` | 4 | 0.09 | 0.00 % | 0.08 |
| `slider-brightness-0` | 4 | 0.12 | 0.00 % | 0.11 |
| `slider-brightness-100` | 4 | 0.85 | 0.00 % | 0.84 |
| `slider-brightness-37` | 4 | 0.43 | 0.00 % | 0.36 |
| `slider-fan-disabled` | 4 | 0.51 | 0.00 % | 0.42 |
| `slider-quick-volume` | 4 | 0.46 | 0.00 % | 0.45 |
| `spectrum-compact-paused-phase0` | 4 | 0.47 | 0.00 % | 0.42 |
| `spectrum-compact-playing` | 4 | 0.67 | 0.98 % | 0.54 |
| `spectrum-expanded-playing` | 4 | 0.49 | 0.41 % | 0.40 |
| `split-bt-off` | 4 | 0.63 | 0.45 % | 0.36 |
| `split-icon-pressed` | 4 | 0.56 | 0.50 % | 0.31 |
| `split-long-elide` | 4 | 0.33 | 0.24 % | 0.21 |
| `split-proxy-chevron` | 4 | 0.93 | 0.68 % | 0.83 |
| `split-wifi-active` | 4 | 0.44 | 0.23 % | 0.36 |
| `tm-cpu` | 4 | 0.00 | 0.00 % | 0.00 |
| `tm-launcher` | 4 | 0.83 | 0.89 % | 0.67 |
| `tm-volume` | 4 | 1.50 | 1.91 % | 1.21 |
| `tm-workspace-active` | 4 | 0.36 | 0.12 % | 0.31 |
| `tm-workspace-hover` | 4 | 0.36 | 0.11 % | 0.19 |
| `tm-workspace-urgent` | 4 | 0.35 | 0.17 % | 0.35 |
| `toggle-active-hover` | 4 | 0.53 | 0.00 % | 0.33 |
| `toggle-dnd-active` | 4 | 0.40 | 0.00 % | 0.21 |
| `toggle-dnd-hover` | 4 | 0.15 | 0.00 % | 0.11 |
| `toggle-dnd-inactive` | 4 | 0.14 | 0.00 % | 0.12 |
| `toggle-dnd-pressed` | 4 | 0.15 | 0.00 % | 0.11 |
| `toggle-labelled` | 4 | 3.94 | 2.83 % | 3.75 |
| `toggle-svg-active` | 4 | 0.49 | 0.00 % | 0.29 |
| `toggle-svg-inactive` | 4 | 0.22 | 0.04 % | 0.22 |
| `toggle-vm-disabled` | 4 | 0.78 | 0.00 % | 0.78 |
