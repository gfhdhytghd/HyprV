# Spec coverage & audit report

Auditor: agent `spec-audit`, 2026-10-23. Source: `/home/wilf/.config/HyprV/quickshell`
(working tree; `shell.qml` 4411 lines, unchanged since 2026-10-20). Specs audited:
`ui-bar.md`, `ui-bar-layout.md`, `ui-bar-popups.md`, `ui-island.md`, `ui-panels*.md`,
`services.md`, `services-api.md`. Every in-place fix is marked `<!-- audit: … -->`.

Legend: **R** = loaded at runtime by the main shell; **P** = separate runtime process;
**D** = present but never instantiated (dead); **X** = out of scope (other config / tests / backups).

---

## 1. QML files → spec sections

| file | status | UI spec | services / data |
|---|---|---|---|
| `shell.qml` | R | see §2 (line map) | services.md "shell.qml" inventory |
| `AgentTaskModel.qml` | R | ui-island.md §5 | services.md (agent stack) |
| `AnimatedGlassPanel.qml` | R | ui-bar-popups.md §2 (canonical), ui-panels.md §2.1 (identical; both note the no-op content slide) | — |
| `AnimatedReveal.qml` | R (via ExpandableSection) | ui-bar-popups.md §6, ui-panels.md §2.2 | — |
| `AudioSpectrum.qml` | R | ui-island.md §7 | services.md (spectrum) |
| `BarMotion.qml` | R | ui-bar.md §7.1 | services.md line 38 |
| `BarScreenState.qml` | R | ui-bar.md §7.2 | — |
| `BarServices.qml` | D | ui-bar.md §12 | services.md (alternative design reference) |
| `BatteryInfoLine.qml` | R | ui-bar-popups.md §5.2 | — |
| `CalendarBridge.qml` | R (inside calendar Loader) | ui-panels-calendar.md §3 | services.md "Calendar" |
| `CalendarSocket.qml` | R | ui-panels-calendar.md §3 | services.md "Calendar" |
| `ControlPanelBluetoothDetails.qml` | R | ui-panels-control.md §3 | services.md "Network and Bluetooth" |
| `ControlPanelMediaCard.qml` | R | ui-panels-control.md §2.4 | — |
| `ControlPanelPopup.qml` | R | ui-panels-control.md §1 | services.md (actions) |
| `ControlPanelSlider.qml` | R | ui-panels-control.md §2.3 (+ ui-panels-stats.md §1.3) | — |
| `ControlPanelSplitTile.qml` | R | ui-panels-control.md §2.1 | — |
| `ControlPanelToggle.qml` | R | ui-panels-control.md §2.2 | — |
| `ControlPanelWifiDetails.qml` | D | ui-panels-control.md §8 | — |
| `DeskflowTargetControl.qml` | D | ui-panels-control.md §6 | services.md "VM, target switching" |
| `DynamicIsland.qml` | R | ui-island.md §2–§4, §8 | services.md (media, agents) |
| `ExpandableSection.qml` | R | ui-panels.md §2.3 | — |
| `GroupPill.qml` | R | ui-bar-layout.md §4 | — |
| `MihomoControlPanel.qml` | R-compiled, **unreachable** (no caller of `toggleMihomoExpanded`) | ui-panels-control.md §7 | services.md (Mihomo) |
| `MihomoControlTile.qml` | D | ui-panels-control.md §7.1 | services.md (Mihomo) |
| `PollCommand.qml` | R | ui-island.md §6.1 (canonical), ui-panels.md §2.6 | services.md line 17 |
| `QuickAdjustPopup.qml` | R | ui-panels-stats.md §1 | services.md (audio/brightness) |
| `ScreenCornerShade.qml` | R | ui-bar.md §9.3 | — |
| `StartupConfirm.qml` | P (login dialog, `restore-session-layout.sh`) | ui-panels-calendar.md §4 | services.md line 36 |
| `StreamCommand.qml` | R | ui-island.md §6.4.2 | services.md line 17 |
| `SystemStatsPopup.qml` | R | ui-panels-stats.md §2 | services.md (system stats) |
| `SystemTrendChart.qml` | R | ui-panels-stats.md §3 | — |
| `TrayButton.qml` | R | ui-bar-layout.md §8.5.1 | services.md (tray) |
| `ViewflowTargetControl.qml` | R | ui-panels-control.md §5 | services.md (Viewflow) |
| `WeekCalendarPanel.qml` | R | ui-panels-calendar.md §2 | services.md "Additional exact adapter contracts" |
| `WifiActionChip.qml` | R | ui-panels.md §2.4 | — |
| `WifiFallback.qml` | R | bar part: ui-bar-layout.md §8.4; popup: ui-panels-wifi.md | services.md (NM) |
| `WifiIconWithFallback.qml` | R | ui-bar-layout.md §8.4, ui-panels-wifi.md §5 | — |
| `WifiIndicator.qml` | R | ui-bar-layout.md §8.4 | — |
| `WifiNative.qml` | R (alias of WifiFallback) | ui-bar-layout.md §8.4 | — |
| `WifiSecurityBadge.qml` | R | ui-panels.md §2.5 | — |
| `WindowsVmController.qml` | R | ui-panels-control.md §4 | services.md (VM) |
| `WindowsVmControl.qml` | R | ui-panels-control.md §4 | — |
| `hypr-comp/*.qml` (7 files) | X (separate config, never loaded) | ui-bar.md §11 | services.md (hypr-comp sections) |
| `tests/tst_AgentTaskModel.qml`, `tests/windows-vm-lifecycle.qml` | X (tests) | — | services.md line 146 |
| `shell.qml.before-*` (2) | X (backups) | ui-bar.md §12 | — |

All 41 runtime/dead QML files in the main config are covered. No runtime file is unspecified.

## 2. `shell.qml` line ranges → spec sections

| lines | content | spec |
|---|---|---|
| 1–17 | pragma, imports, `ShellRoot` | ui-bar.md §1 |
| 18–43 | dirs, `barScreenStates`, `BarMotion`, `barExpanded`/`toggleBar` | ui-bar.md §1.1, §7, §8 |
| 44–81 | `IpcHandler bar` | ui-bar.md §4.1, services.md IPC |
| 83–89 | `Connections { Hyprland }` | ui-bar.md §1.1 |
| 91–161 | root state properties | ui-bar.md §5, services.md |
| 162–165 | `onMediaPlayersChanged` | ui-island.md §6.2 |
| 166–211 | private state | services.md |
| 212–240 | theme tokens & bar constants | ui-bar.md §5.1 (canonical), ui-panels.md §1.3, ui-island.md §0.7 |
| 241–368 | workspaces, audio, battery derived strings | ui-bar-layout.md §5.2, ui-bar-popups.md §5.3 |
| 369–384 | `volumeIcon` | ui-bar.md §5.2 |
| 386–389 | bluetooth hooks, `Component.onCompleted` | services.md "Network and Bluetooth" |
| 390–481 | `resetMediaState`, `updateMediaState` | ui-island.md §6.2 |
| 482–503 | `networkTypeForInterface`, network/notification props | ui-bar.md §5.2, ui-bar-layout.md §8.4 |
| 503–521 | `sortedTrayItems` | ui-bar-layout.md §8.5 |
| 523–609 | `component TextModule` | ui-bar-layout.md §3 |
| 611–1140 | `component TrayMenuPopup` | ui-bar-popups.md §3 |
| 1142–1308 | `component TrayOverflowPopup` | ui-bar-popups.md §4 |
| 1310–1618 | `component BatteryInfoPopup` | ui-bar-popups.md §5 |
| 1620–1668 | `updateControlPanelState` | ui-panels-control.md §1.6, services.md line 30 |
| 1670–1673 | `withAlpha` | ui-bar.md §0, ui-panels.md §1.2 |
| 1675–1711 | `formatPower`, `formatDuration` | ui-bar-popups.md §5.3 |
| 1713–1786 | fluent/wifi/wired icon sources | ui-bar-layout.md §8.4, ui-panels-wifi.md §5 |
| 1788–1814 | `humanRate`, `usageSeverityColor` | ui-bar.md §5.2, ui-panels-stats.md §2.3 |
| 1841–1845 | `batteryGlyph` | ui-bar.md §5.2 |
| 1925–1963 | wifi glyph helpers | ui-bar.md §5.2, ui-panels-wifi.md §5 |
| 1965–1981 | `updateNotificationState` | ui-bar-layout.md §8.7, services.md |
| 2046–2093 | `applyWifiStatus` | services.md line 23 |
| 2095–2312 | bluetooth snapshot/sort/sync/actions | services.md, ui-panels-control.md §3.3 |
| 2314–2421 | `updateSystemStats` | services.md line 22, ui-panels-stats.md §2.4 |
| 2423–2510 | tray priority & budget | ui-bar-layout.md §8.5 |
| 2512–2538 | `trayIconSource` | ui-bar-layout.md §8.5.1 |
| 2540–2570 | tray menu / wifi manager / `openWifiPanel` | ui-bar-layout.md §8.5, ui-panels-wifi.md §1 |
| 2571–2720 | wifi & bluetooth actions | services.md, ui-panels-wifi.md §6, ui-panels-control.md §3.3 |
| 2722–2729 | `runDetached` | services.md (no-exec rule) |
| 2730–2762 | calendar open/close/child | ui-panels-calendar.md §1.1, ui-bar.md §10 |
| 2764–2870 | brightness, fan, audio refresh | ui-panels.md §4.1, ui-panels-control.md §1.5, services.md "Power, brightness, fans" |
| 2872–2922 | `seekMedia`, `focusMediaApp` | ui-island.md §1.4 |
| 2924–2931 | power profile refresh | services.md |
| 2935–2960 | `fluentIconLocator` | ui-bar-layout.md §8.4, services.md line 37 |
| 2962–3115 | processes & timers (brightness probe/set, fan apply/status, audio/media follow-ups) | services.md; UI-visible timings in ui-panels.md §4.1 |
| 3117–3190 | bluetooth `Connections` / `Instantiator` | services.md |
| 3191–3198 | `SystemClock` | ui-bar.md §5.3, ui-island.md §1.4 |
| 3200–3217 | `desktopState` stream | ui-bar.md §5.3, ui-island.md §6.4, services.md line 21 |
| 3219–3226 | `themeState` FileView | ui-bar.md §5.1, ui-panels.md §1.3 |
| 3228–3276 | cava spectrum | ui-island.md §6.3 |
| 3278–3283 | swaync stream | ui-bar-layout.md §8.7 |
| 3285–3312 | PipeWire | services.md "Audio" |
| 3314–3327 | media poll | ui-island.md §6.2 |
| 3329–3337 | PowerProfiles | services.md |
| 3339–3352 | battery poll | ui-bar-popups.md §5 |
| 3354–3370 | bar popup instances | ui-bar.md §1.1 |
| 3372–3401 | stats/control-panel/quick-adjust instances + status poll | ui-panels.md §4 |
| 3403–3454 | IPC controlPanel/calendar/quickAdjust | ui-bar.md §4, ui-panels.md §4.1 |
| 3458–3465 | `BarScreenState` Variants | ui-bar.md §7.2 |
| 3467–3546 | corner overlay | ui-bar.md §9 |
| 3548–3757 | bar window, mask, timers, dismiss layer | ui-bar-layout.md §2, ui-bar.md §10, ui-panels-calendar.md §1 |
| 3758–3877 | left section | ui-bar-layout.md §5 |
| 3879–3997 | centre host, island, calendar Loader | ui-bar-layout.md §6, ui-island.md §1, ui-panels-calendar.md §1.4 |
| 3999–4030 | window title | ui-bar-layout.md §7 |
| 4032–4365 | right section (status, media, tray, wifi, control-panel, notification) | ui-bar-layout.md §8 |
| 4370–4402 | calendar child poll / launch timeout | ui-panels-calendar.md §1.1, ui-bar.md §10 |
| 4404–4410 | `powerProfileRefresh` | services.md |

No line range is unassigned.

## 3. Gaps and open items

Not covered by QML reading alone; each needs a reference capture or a decision:

1. **Glyph rasterisation parity.** Implicit sizes are now measured (components.md §0.2), but
   glyph pixels (hintslight + autohint, grayscale, at scale 2 and fractional scales) are not
   specified beyond "match Qt FreeType". The engine agent needs reference crops.
2. **Hyprland layer animation** for `shell:*` namespaces (no `no_anim`) is compositor-side and
   unspecified; parity captures of popups must be taken after it settles.
3. **`mapToGlobal` on layer surfaces.** Several positions (control panel y ≈ 58 vs 68, Wi-Fi,
   stats, QuickAdjust x) assume Quickshell returns output-local + output origin. Verify with one
   capture per popup.
4. **Calendar close snap** (container returns to island size in one frame) — specified
   identically in ui-bar-layout.md §6, ui-island.md §1.3 and ui-panels-calendar.md §1.4, but
   only derived from the Behavior/binding order. Needs a capture.
5. **Island idle hover under the z-99 hotspot** (ui-island.md §4.2) — unresolved.
6. **Input-region tracking during the fullscreen slide** (ui-bar-layout.md §2.1) — current QML
   probably lags; the specs ask the port to do the intended behaviour, which is a deliberate
   deviation.
7. **Bugs specs ask to reproduce literally** (decide port-or-fix once, globally): QuickAdjust
   never calls `brightness_finish`; QuickAdjust unmaps at 140 ms before its 160 ms scale ends;
   calendar scroll indicator scrolls with the content; slider labels/RPM never shown; black play
   glyph in dark mode; battery display unreachable on desktop (ui-bar.md §5.3); inverted volume
   wheel.
8. **Unreachable features** (Mihomo page, eGPU confirm, power-expanded, Deskflow, Wi-Fi
   details): specified, but whether to port them is a product decision.
9. **Font fallback for default-font texts** (Mihomo "ON/OFF" etc. use the app default font,
   `sans-serif` → Inter) — only matters if the Mihomo page is ported.

## 4. Cross-spec consistency audit

| topic | finding | resolution |
|---|---|---|
| Text advances | ui-island.md said advances round to 10 px; ui-bar.md / ui-bar-layout.md assumed 9.6 fractional and "every glyph = 0.6 em"; ui-panels.md estimated per-side-rounded heights | Measured with Qt 6.12 (same Qt as quickshell 0.3.2), offscreen, this fontconfig, scales 1/1.5/2: advances fractional (9.59375 @16), **implicitWidth adds the last glyph's ink overflow**, height = `ceil(1.32·px)`. Fixed in ui-bar.md §0.1, ui-bar-layout.md §1.3/§2.3/§5.1/§8.3/§8.7/§9, ui-island.md §0.6/§3.9/§8/§9, ui-panels.md §1.4, ui-panels-control/-wifi/-stats heights. Canonical table: components.md §0.2. |
| Island idle width | 116 (island) vs 113.2 (implied by bar) | 120.16 measured-derived; ui-island.md §3.9, ui-bar-layout.md §9 |
| Bar worked example | widths ignored icon overflow | recomputed (threshold 1328 → 1347.75) |
| StartupConfirm | ui-bar.md §2/§12 "dead code" vs ui-panels-calendar.md §4 / services.md "launched at login" | ui-bar.md fixed: separate process, must be ported |
| Popup positioning | ui-bar-popups.md said all panels reuse the tray-menu centring rule | fixed: control panel/Wi-Fi right-aligned, stats x = 10, QuickAdjust tray-pill-aligned |
| `darkMode` source | ui-panels.md implied control-panel-only | fixed: `themeState` FileView (+ optimistic toggle), same as ui-bar.md §5.1 / ui-island.md §0.7 |
| Wi-Fi fallback glyph size | ui-panels-wifi.md said 22 always | fixed: popup 22 light / 14 dark; bar 24 (explicit) |
| Mihomo cadence | services.md 2000 ms vs QML 1500 ms (panel) | services.md fixed |
| eGPU confirm | services.md presented it as a live panel flow | services.md fixed: no UI caller |
| AnimatedGlassPanel | ui-bar-popups.md §2 vs ui-panels.md §2.1 | consistent (defaults, no-op slide, shadow recipe, timings). Canonical: ui-bar-popups.md §2 |
| Theme token values | ui-bar.md §5.1, ui-panels.md §1.3, ui-island.md §0.7 | values consistent; ui-panels.md citations were off by ~11 lines → fixed |
| Behavior semantics / easing | ui-island.md §0.2–0.3, ui-bar.md §0, ui-panels.md §1.5 | consistent (retarget from current value, full duration, Linear default) |
| Surface namespaces / layers | ui-bar.md §2, ui-panels.md §3, ui-island.md §1.1 | consistent |
| Island ↔ calendar morph | ui-bar-layout.md §6, ui-island.md §1.3, ui-panels-calendar.md §1.4, ui-bar.md §10 | consistent (210 ms OutCubic, Behaviors gated on `calendarPanelPresented`, 210 ms close timer, snap) |
| Calendar border colour | all three | consistent (`primaryText` α0.11 D / 0.09 L) |
| PollCommand | ui-island.md §6.1 vs ui-panels.md §2.6 vs services.md | consistent |

## 5. Numeric spot check (values transcribed by other agents)

Values picked across ui-bar, ui-bar-layout, ui-bar-popups, ui-island and services, then
re-read in QML.

| # | spec claim | QML | ok |
|---|---|---|---|
| 1 | BarMotion default duration 700 | `BarMotion.qml:8` | ✓ |
| 2 | default curve [0.1,0.9,0.1,1.05,1,1] | `BarMotion.qml:9` | ✓ |
| 3 | `compactOffset = −15·fullscreenHide` | `BarScreenState.qml:20` | ✓ |
| 4 | `topCornerOffset` clamp 0..15 | `BarScreenState.qml:22` | ✓ |
| 5 | corner shade 27 | `shell.qml:230` | ✓ |
| 6 | tray overflow button 22 | `shell.qml:239` | ✓ |
| 7 | mediaInactiveColor `#6c7086`/`#808080` | `shell.qml:227` | ✓ |
| 8 | brightnessProbeDebounce 90 ms | `shell.qml:3047` | ✓ |
| 9 | launcher pad 7/4, h 38 | `shell.qml:3776-3780` | ✓ |
| 10 | viewflow pad 7/7 | `shell.qml:3790-3794` | ✓ |
| 11 | workspace h 32, pad 5/5, min 32 | `shell.qml:3824-3827` | ✓ |
| 12 | cpu pad 12/4 | `shell.qml:3849-3850` | ✓ |
| 13 | mem pad 6/4 | `shell.qml:3860-3861` | ✓ |
| 14 | net pad 6/12 | `shell.qml:3871-3872` | ✓ |
| 15 | temperature pad 10/5, critical ≥ 70 | `shell.qml:4051-4057` | ✓ |
| 16 | GroupPill radius 24 | `GroupPill.qml:9` | ✓ |
| 17 | TrayButton fallback 10 px | `TrayButton.qml:37` | ✓ |
| 18 | title min 38, +24, margins 12 | `shell.qml:4003-4021` | ✓ |
| 19 | tray menu rowHeight `max(34, px+18)` | `shell.qml:620` | ✓ |
| 20 | menu width 300 / max 420 / padding 9 | `shell.qml:621-623` | ✓ |
| 21 | tray menu closeContentOffset −6 | `shell.qml:952` | ✓ |
| 22 | overflow radius 18 | `shell.qml:1280` | ✓ |
| 23 | overflow padding 8, maxColumns 6 | `shell.qml:1149-1150` | ✓ |
| 24 | battery popup width 324, padding 12 | `shell.qml:1316-1317` | ✓ |
| 25 | BatteryInfoLine title 118, gap 12, 13 px | `BatteryInfoLine.qml:11,26,37,42` | ✓ |
| 26 | island music 392×176, agent 448 | `DynamicIsland.qml:94-97` | ✓ |
| 27 | MediaGlyphButton 44/40, scale 0.92/1.06 | `DynamicIsland.qml:454-456` | ✓ |
| 28 | compactMusic fade 140 OutQuad | `DynamicIsland.qml:859-860` | ✓ |
| 29 | expanded margins 13, track h 5, r 2.5 | `DynamicIsland.qml:936, 1029-1030` | ✓ |
| 30 | spectrum idle timer 66 ms, phase +0.28 | `AudioSpectrum.qml:62,66` | ✓ |
| 31 | musicFrameRelease 230, seekPreviewReset 700 | `DynamicIsland.qml:1432,1440` | ✓ |
| 32 | album fallback glyph `round(min·0.46)` | `DynamicIsland.qml:440` | ✓ |
| 33 | island mutedText α 0.68 D / 0.62 L | `DynamicIsland.qml:127` | ✓ |
| 34 | fan apply debounce 35 ms | `shell.qml:3055` | ✓ |
| 35 | calendar discovery retry 1800, debounce 350 | `CalendarBridge.qml:192,199` | ✓ |
| 36 | Mihomo "2000-ms visible polling" | `MihomoControlPanel.qml:75` = 1500 | ✗ (fixed) |
| 37 | control-panel status poll 1500 ms | `shell.qml:3382` | ✓ |
| 38 | `exclusiveZone = round(25 + 33·reveal)` | `shell.qml:3638` | ✓ |

**Literal values: 1 error in 38 (2.6 %).**

Derived values are a different story. Every UI agent, this author included, computed text widths
or heights from font-file metrics instead of Qt's real layout, and all of them were wrong somewhere:

| spec | wrong derived values found |
|---|---|
| ui-island.md | advance 10 px; idle width 116 (actual 120.16) |
| ui-bar-layout.md | heights 17→24, 14→20, 18→25 (actual 23, 19, 24); 5 icon-terminated widths (launcher, viewflow, notification, prev/next, volume); worked-example threshold 1328 (actual 1347.75); notification dot x |
| ui-bar.md | "every glyph 0.6 em" used as text width (true for advance, false for implicitWidth) |
| ui-panels*.md (own) | 8 of 15 tabulated line heights; 6 derived card heights; Wi-Fi dark fallback size; 7 wrong `shell.qml` line citations |

Takeaway for implementers: take literal constants from the specs, but compute every text-derived
size with the engine's real text layout and check it against components.md §0.2.
