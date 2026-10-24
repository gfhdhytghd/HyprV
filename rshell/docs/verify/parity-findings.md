# Parity findings (QA log)

Owner: parity QA (`parity/`, `docs/verify/parity*.md`). Facts established from the reference
captures (`parity/reference`, Qt RHI/OpenGL renders of the original QML). Newest first.

## 2026-10-23 — reference fix: missing-icon placeholder instead of the letter fallback
The live shell never shows the QML letter fallback for an unresolvable tray icon: Quickshell's
`image://icon` provider returns its missing pixmap (spec-panels, `docs/requests/parity-capture.md`;
port: `hyprv_widgets::icon_theme::missing_icon`). The harness provider now returns the same
pixmap — side = request rounded up to even (18 at @1, 36 at @2), black top-left/bottom-right,
`#d900d8` top-right/bottom-left, opaque — so `TrayButton`'s Image is Ready and the letter is
hidden. Recaptured atomically: 194 bar and 49 panel references changed (every crop showing the
`telegram` / unknown-source tray items).

## 2026-10-23 (~11:30) — reference fix: tray icon pixmap size (supersedes the 04:xx "fix")
spec-panels measured the live shell on nested live-shell captures (`docs/requests/parity-capture.md`):
Qt already passes the `image://icon` provider a request in device pixels (sourceSize × DPR = 36
for an 18-px tray icon at @2), and Quickshell resolves `QIcon::pixmap(36×36)` at DPR 1 (chrome:
hicolor 32×32, never upscaled; discord: 256 → 36). The earlier harness change passed the DPR a
second time (64–72 px pixmaps), making @2 tray icons sharper than live. The provider now does
`QIcon::fromTheme(name).pixmap(requested, 1.0)`; verified requests 36×36 → chrome 32×32,
discord/keepassxc/fcitx 36×36. Bar (@2 variants) and panels references were recaptured
(235 images changed); recaptures are byte-identical on rerun.

## 2026-10-23 — reference fix: tray-menu-submenu pointer state
`bar/tray-menu-submenu` ended right after a click on "Status"; whether the submenu row that
appears under the pointer got a hover depended on delivery order, so one of four earlier
captures (`dark@2`) showed the "Do Not Disturb" hover and three did not (found by svc-power,
`crates/hyprv-ui-bar/VALIDATION.md`). The fixture now hovers "Do Not Disturb" explicitly before
capturing; all four variants show the hover and recapture byte-identically.

## 2026-10-23 — verify pass 1

### Reference-set changes
- **Tray icons at @2 were too soft (harness bug, fixed).** The mock `image://icon` provider
  rendered `QIcon::pixmap()` at the headless display's DPR 1, while the live quickshell's
  application DPR is 2 (scale-2 screen), so Qt delivered 36-px pixmaps live. The provider now
  renders at the capture DPR; bar and panel references were recaptured. Side effect worth
  knowing: QIcon then selects a different artwork variant for some icons (WeChat). The
  `IconImage` mock is now a verbatim copy of Quickshell's `Widgets/IconImage.qml` behaviour.
- New captures for `ui-bar.md` §14 items 22–24: `bar/tray-menu-{root,hover,submenu,anim}`,
  `bar/tray-overflow-{2,6,7}`, `bar/battery-{charging,critical,full}`; `panels/cp-page-switch`
  gains a settled t=800 frame. Each reference now has a regions sidecar
  (`parity/reference/<c>/regions/<file>.json`: Text/Image/Canvas item rects, text, render type,
  font) used for root-cause attribution.

### Answers to `docs/spec/COVERAGE.md` §3 open items
- **§3.3 popup positions** (`mapToGlobal` on layer surfaces; output DP-4 at origin 0,0): card
  rects measured from the references (logical px; glass cards include their soft shadow above
  α 60/255): control panel `x 2784, y 58, w 278` (opened from the bar icon; *y 58*, not 68);
  QuickAdjust `x 2784, y 58, w 278, h 62`; Wi-Fi ≈ `x 2688, y 69, w 364`; stats ≈ `x 26, y 76,
  w 516`; tray menu ≈ `x 2755, y 76, w 282`; battery ≈ `x 2528, y 72, w 300`; overflow grid
  (1000-px output) ≈ `x 834, y 59, w 157`.
- **§3.4 calendar close snap — no visible snap.** `panels/calendar-morph` close frames: the
  container keeps the 520×500 box while the calendar fades; its fill/border are drawn only
  while `calendarProgress > 0.001`, which with OutCubic over 210 ms ends at ≈ 189 ms
  ((1 − t/210)³ < 0.001). At t = 209 only the island (120×38) is painted, so the size flip at
  210 ms happens on an invisible container. Open: t = 16 already 205×135 (container animating),
  t = 100 463×434, t = 210 520×500.
- **§3.5 island idle hover under the z-99 hotspot — no fill.** `island/idle-hover-lock` and
  `idle-hover-power` are byte-identical to `island/idle`; only the press scale is visible
  (`idle-press-lock`).

### Unreachable or broken behaviour in the live QML (spec/product decisions)
- **Swipe-to-hide music never fires** under Qt 6.12: `DragHandler.translation` is reset before
  `activeChanged(false)`, so `DynamicIsland.qml:1524-1540` always reads dx = 0 (handler trace:
  70 px during the drag, 0 at release). Reference `island/music-swipe-hide` shows no change.
- **Battery popup is unreachable on any machine**, not only desktops: `batteryAvailable`
  derives from `batteryInfo`, which only `batteryInfoPoll` fills, and that poll runs only while
  the popup is open, which requires `batteryAvailable` (`shell.qml:3340-3349, 4078-4081`).
  References seed `root.batteryInfo` to capture it.
- **A 1-item tray overflow grid is unreachable**: hiding one item costs the 22-px `⋯` button
  instead of an 18-px icon, so when all items do not fit at least 2 overflow
  (`trayVisibleCountForBudget`, `shell.qml:2470-2484`). Captured 2/6/7 instead of 1/6/7.
- **StartupConfirm dark variant cannot occur**: the installed `qt6ct-kde` platform theme fails
  to load under Qt 6.12 (Qt falls back to `generic`) in the live quickshell too, so
  `SystemPalette` is Qt's light default and the dialog is always light (`StartupConfirm.qml:14`).
  Same root cause: theme icons resolve via the `hicolor` fallback, not Fluent.
- **Mihomo control-panel page** has no UI entry point (as `ui-panels-control.md` says);
  references reach it through `ControlPanelPopup.toggleMihomoExpanded()`.

### Verifier conventions (for owners reading `docs/verify/parity.md`)
- Premultiplied comparison; images with `distance_field_text: false` must be exact.
- The bar's own export leaves the island slot empty; that rect is cleared in both images
  (island contents are verified in the island section).
- Root-cause classes are automatic (regions attribution + edge test + sequence comparison);
  the requests files carry the human triage.

## 2026-10-23 QA loop 12:55–14:55 EDT (GPU path, `verify.py --backend gl`)
| run | island | bar | panels |
|---|---|---|---|
| 12:49 (placeholder refs, bar still letter) | 364/380 | 55/312 | 86/300 |
| 13:20 | 368/380 | 276/312 | 86/300 |
| 13:39 | 370/380 | 276/312 | 128/300 |
| 14:04 | 380/380 | 292/312 | 255/300 |
| 14:22 | 380/380 | 292/312 | 290/300 |
| 14:40, 14:56 (final) | **380/380** | **292/312** (0 fail; 12 excluded battery, 8 foreign-content host views not exported) | **298/300** (gate v1 300/300) |
Remaining: `qa-show-hide--show-t0120--{dark,light}@2`, PM max 1 on 2/8 left-edge px (hyprv-ui.md PQ-QA 14:22).
Harness changes in the loop: Quickshell missing-icon checker in `ThemeIconProvider` + bar replay;
`verify.py` serialised with `out/.verify.lock` (concurrent runs had clobbered out/ and the bar's
artifact dir); `zoom.py` accepts `-gl` slots.
