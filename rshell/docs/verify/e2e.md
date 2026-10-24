# Nested functional E2E — 2026-10-23

Command: `hyprv-shell --nested-functional-test target/functional-test`.

204 / 204 cases pass. Overall: **PASS**. Raw manifest: `/home/wilf/.config/HyprV/rshell/docs/verify/raw/laptop-phase-b/rss-complete-desktop-e2e/manifest.json`.

Input goes through zwlr_virtual_pointer_v1 / zwp_virtual_keyboard_v1 and native
seat dispatch. Service actors are absent; typed production service commands are
captured by a mock sink before execution. Exact ordered command arrays reject
extra, missing, duplicate or wrong-payload actions. Local actions assert UI state
and zero commands. Passwords below use only synthetic test credentials.

Only private KWin/Hyprland, private XDG paths and private D-Bus are used.
Output lifecycle mutations are sent to that instance's native IPC socket.
Mihomo is unreachable in the legacy shell (spec §7); its setup hook mounts the
page, then native input tests its controls. Reconnect policy is explicit: a lost
Wayland connection exits cleanly; a fresh supervised process reconnects after
compositor restart. Automatic in-process reconnect is not implemented.

| Action | Result | Spec / evidence |
| --- | --- | --- |
| `bar.launcher` | PASS | ui-bar-layout.md |
| `bar.workspace` | PASS | ui-bar-layout.md |
| `bar.media.previous` | PASS | ui-bar-layout.md |
| `bar.media.play` | PASS | ui-bar-layout.md |
| `bar.media.next` | PASS | ui-bar-layout.md |
| `bar.volume.mute` | PASS | ui-bar-layout.md |
| `bar.volume.mixer` | PASS | ui-bar-layout.md |
| `bar.notifications.show` | PASS | ui-bar-layout.md |
| `bar.notifications.dnd` | PASS | ui-bar-layout.md |
| `bar.hover.Launcher` | PASS | ui-bar-layout.md |
| `bar.scroll.Launcher` | PASS | ui-bar-layout.md |
| `bar.hover.Viewflow` | PASS | ui-bar-layout.md |
| `bar.scroll.Viewflow` | PASS | ui-bar-layout.md |
| `bar.hover.Workspace(1)` | PASS | ui-bar-layout.md |
| `bar.scroll.Workspace(1)` | PASS | ui-bar-layout.md |
| `bar.hover.Workspace(2)` | PASS | ui-bar-layout.md |
| `bar.scroll.Workspace(2)` | PASS | ui-bar-layout.md |
| `bar.hover.Workspace(3)` | PASS | ui-bar-layout.md |
| `bar.scroll.Workspace(3)` | PASS | ui-bar-layout.md |
| `bar.hover.Workspace(4)` | PASS | ui-bar-layout.md |
| `bar.scroll.Workspace(4)` | PASS | ui-bar-layout.md |
| `bar.hover.Workspace(5)` | PASS | ui-bar-layout.md |
| `bar.scroll.Workspace(5)` | PASS | ui-bar-layout.md |
| `bar.hover.Cpu` | PASS | ui-bar-layout.md |
| `bar.scroll.Cpu` | PASS | ui-bar-layout.md |
| `bar.hover.Memory` | PASS | ui-bar-layout.md |
| `bar.scroll.Memory` | PASS | ui-bar-layout.md |
| `bar.hover.Network` | PASS | ui-bar-layout.md |
| `bar.scroll.Network` | PASS | ui-bar-layout.md |
| `bar.hover.Temperature` | PASS | ui-bar-layout.md |
| `bar.scroll.Temperature` | PASS | ui-bar-layout.md |
| `bar.hover.Previous` | PASS | ui-bar-layout.md |
| `bar.scroll.Previous` | PASS | ui-bar-layout.md |
| `bar.hover.PlayPause` | PASS | ui-bar-layout.md |
| `bar.scroll.PlayPause` | PASS | ui-bar-layout.md |
| `bar.hover.Next` | PASS | ui-bar-layout.md |
| `bar.scroll.Next` | PASS | ui-bar-layout.md |
| `bar.hover.Volume` | PASS | ui-bar-layout.md |
| `bar.scroll.Volume` | PASS | ui-bar-layout.md |
| `bar.hover.Wifi` | PASS | ui-bar-layout.md |
| `bar.scroll.Wifi` | PASS | ui-bar-layout.md |
| `bar.hover.Tray("chrome_status_icon_1")` | PASS | ui-bar-layout.md |
| `bar.scroll.Tray("chrome_status_icon_1")` | PASS | ui-bar-layout.md |
| `bar.hover.Tray("discord")` | PASS | ui-bar-layout.md |
| `bar.scroll.Tray("discord")` | PASS | ui-bar-layout.md |
| `bar.hover.Tray("org.keepassxc.KeePassXC")` | PASS | ui-bar-layout.md |
| `bar.scroll.Tray("org.keepassxc.KeePassXC")` | PASS | ui-bar-layout.md |
| `bar.hover.Tray("Fcitx")` | PASS | ui-bar-layout.md |
| `bar.scroll.Tray("Fcitx")` | PASS | ui-bar-layout.md |
| `bar.hover.Tray("wechat")` | PASS | ui-bar-layout.md |
| `bar.scroll.Tray("wechat")` | PASS | ui-bar-layout.md |
| `bar.hover.Tray("telegram")` | PASS | ui-bar-layout.md |
| `bar.scroll.Tray("telegram")` | PASS | ui-bar-layout.md |
| `bar.hover.ControlPanel` | PASS | ui-bar-layout.md |
| `bar.scroll.ControlPanel` | PASS | ui-bar-layout.md |
| `bar.hover.Notifications` | PASS | ui-bar-layout.md |
| `bar.scroll.Notifications` | PASS | ui-bar-layout.md |
| `bar.click.Cpu.273` | PASS | ui-bar-layout.md |
| `bar.click.Memory.273` | PASS | ui-bar-layout.md |
| `bar.click.Temperature.272` | PASS | ui-bar-layout.md |
| `bar.click.Viewflow.273` | PASS | ui-bar-layout.md |
| `bar.viewflow.start` | PASS | ui-bar-layout.md |
| `bar.stats.Cpu` | PASS | ui-bar-layout.md |
| `bar.stats.Memory` | PASS | ui-bar-layout.md |
| `bar.stats.Network` | PASS | ui-bar-layout.md |
| `bar.control.open` | PASS | ui-bar-layout.md |
| `bar.volume.delayed-osd` | PASS | ui-bar-layout.md |
| `control.wifi.radio` | PASS | ui-panels-control.md §1.4–1.5 |
| `control.bluetooth.power` | PASS | ui-panels-control.md §1.4–1.5 |
| `control.vm` | PASS | ui-panels-control.md §1.4–1.5 |
| `control.viewflow` | PASS | ui-panels-control.md §1.4–1.5 |
| `control.record` | PASS | ui-panels-control.md §1.4–1.5 |
| `control.sleep` | PASS | ui-panels-control.md §1.4–1.5 |
| `control.theme` | PASS | ui-panels-control.md §1.4–1.5 |
| `control.dnd` | PASS | ui-panels-control.md §1.4–1.5 |
| `control.demo` | PASS | ui-panels-control.md §1.4–1.5 |
| `control.profile` | PASS | ui-panels-control.md §1.4–1.5 |
| `control.media.previous` | PASS | ui-panels-control.md §1.4–1.5 |
| `control.media.play` | PASS | ui-panels-control.md §1.4–1.5 |
| `control.media.next` | PASS | ui-panels-control.md §1.4–1.5 |
| `control.volume.mute` | PASS | ui-panels-control.md §1.4–1.5 |
| `control.volume.set` | PASS | ui-panels-control.md §1.4–1.5 |
| `control.fan.chassis` | PASS | ui-panels-control.md §1.4–1.5 |
| `control.fan.pump` | PASS | ui-panels-control.md §1.4–1.5 |
| `control.brightness.commit` | PASS | ui-panels-control.md §1.4 |
| `control.outside-close` | PASS | ui-panels-control.md §1.1 |
| `control.escape-close` | PASS | ui-panels-control.md §1.1 |
| `control.wifi-text` | PASS | ui-panels-control.md §1.5 |
| `ipc.quickAdjust.showBrightness` | PASS | ui-bar.md §4 / hypr/controls.lua:51 |
| `ipc.quickAdjust.showBrightnessIncrease` | PASS | ui-bar.md §4 / hypr/controls.lua:51 |
| `ipc.quickAdjust.showBrightnessDecrease` | PASS | ui-bar.md §4 / hypr/controls.lua:51 |
| `ipc.quickAdjust.showBrightnessLevel` | PASS | ui-bar.md §4 / hypr/controls.lua:51 |
| `ipc.quickAdjust.showVolume` | PASS | ui-bar.md §4 / hypr/controls.lua:51 |
| `ipc.bar.toggle` | PASS | ui-bar.md §4 |
| `ipc.bar.layoutStatus` | PASS | ui-bar.md §4 |
| `ipc.bar.inputStatus` | PASS | ui-bar.md §4 |
| `ipc.bar.dataStatus` | PASS | ui-bar.md §4 |
| `ipc.controlPanel.toggle` | PASS | ui-bar.md §4 / hyprland-bind.lua:54 |
| `ipc.calendar.toggle` | PASS | ui-bar.md §4 |
| `bluetooth.power-off` | PASS | ui-panels-control.md §3–5 |
| `bluetooth.power-on` | PASS | ui-panels-control.md §3–5 |
| `bluetooth.scan` | PASS | ui-panels-control.md §3–5 |
| `bluetooth.advanced` | PASS | ui-panels-control.md §3–5 |
| `bluetooth.disconnect` | PASS | ui-panels-control.md §3–5 |
| `bluetooth.connect-paired` | PASS | ui-panels-control.md §3–5 |
| `bluetooth.connect-unpaired` | PASS | ui-panels-control.md §3–5 |
| `bluetooth.unnamed-filter` | PASS | ui-panels-control.md §3 |
| `bluetooth.back` | PASS | ui-panels-control.md §3 |
| `bluetooth.list-scroll` | PASS | ui-panels-control.md §3 |
| `mihomo.stop` | PASS | ui-panels-control.md §7 (unreachable legacy page; setup-only mount) |
| `mihomo.mode-rule` | PASS | ui-panels-control.md §7 (unreachable legacy page; setup-only mount) |
| `mihomo.mode-global` | PASS | ui-panels-control.md §7 (unreachable legacy page; setup-only mount) |
| `mihomo.mode-direct` | PASS | ui-panels-control.md §7 (unreachable legacy page; setup-only mount) |
| `mihomo.refresh` | PASS | ui-panels-control.md §7 (unreachable legacy page; setup-only mount) |
| `mihomo.dashboard` | PASS | ui-panels-control.md §7 (unreachable legacy page; setup-only mount) |
| `wifi.radio` | PASS | ui-panels-wifi.md §2 |
| `wifi.rescan` | PASS | ui-panels-wifi.md §2 |
| `wifi.advanced` | PASS | ui-panels-wifi.md §2 |
| `wifi.open-populated-no-rescan` | PASS | ui-panels-wifi.md §1 |
| `wifi.close-button` | PASS | ui-panels-wifi.md §1 |
| `wifi.escape-close` | PASS | ui-panels-wifi.md §1 |
| `wifi.outside-close` | PASS | ui-panels-wifi.md §1 |
| `wifi.list-scroll` | PASS | ui-panels-wifi.md §2 |
| `wifi.disconnect` | PASS | ui-panels-wifi.md §3–4 |
| `wifi.known-connect` | PASS | ui-panels-wifi.md §3–4 |
| `wifi.open-connect` | PASS | ui-panels-wifi.md §3–4 |
| `wifi.secure-password-return` | PASS | ui-panels-wifi.md §3–4 |
| `wifi.password-cancel` | PASS | ui-panels-wifi.md §3 |
| `island.idle.lock` | PASS | ui-island.md §3.9 |
| `island.idle.power` | PASS | ui-island.md §3.9 |
| `island.clock-calendar` | PASS | ui-island.md §3.9 |
| `island.music-expand-collapse` | PASS | ui-island.md §4 |
| `island.music.previous` | PASS | ui-island.md §3.11 |
| `island.music.play` | PASS | ui-island.md §3.11 |
| `island.music.next` | PASS | ui-island.md §3.11 |
| `island.music.focus` | PASS | ui-island.md §3.11 |
| `island.music.seek` | PASS | ui-island.md §3.11 |
| `island.agent-expand-focus` | PASS | ui-island.md §3.7 / §6 |
| `calendar.previous` | PASS | ui-panels-calendar.md §1 |
| `calendar.next` | PASS | ui-panels-calendar.md §1 |
| `calendar.new-event` | PASS | ui-panels-calendar.md §1 |
| `calendar.expand-app` | PASS | ui-panels-calendar.md §1 |
| `calendar.range-close` | PASS | ui-panels-calendar.md §1 |
| `calendar.vertical-scroll` | PASS | ui-panels-calendar.md §1 |
| `calendar.outside-close` | PASS | ui-panels-calendar.md §1 |
| `tray.activate-272` | PASS | ui-bar-layout.md §tray |
| `tray.activate-273` | PASS | ui-bar-layout.md §tray |
| `tray.horizontal-scroll` | PASS | ui-bar-layout.md §tray |
| `tray.menu.activate` | PASS | ui-bar-popups.md §1 |
| `tray.menu.toggle` | PASS | ui-bar-popups.md §1 |
| `tray.menu.disabled` | PASS | ui-bar-popups.md §1 |
| `tray.menu.escape` | PASS | ui-bar-popups.md §1 |
| `tray.overflow.open-close` | PASS | ui-bar-popups.md §2 |
| `tray.overflow.activate` | PASS | ui-bar-popups.md §2 |
| `battery.open-close` | PASS | ui-bar-popups.md §3 |
| `quick-adjust.showVolume.slider` | PASS | QuickAdjustPopup.qml:216 (adjustmentFinished is unconnected) |
| `quick-adjust.showBrightness.slider` | PASS | QuickAdjustPopup.qml:216 (adjustmentFinished is unconnected) |
| `bar.workspace.1` | PASS | ui-bar-layout.md §workspaces |
| `bar.workspace.3` | PASS | ui-bar-layout.md §workspaces |
| `bar.workspace.4` | PASS | ui-bar-layout.md §workspaces |
| `bar.workspace.5` | PASS | ui-bar-layout.md §workspaces |
| `stats.escape-close` | PASS | ui-panels-stats.md |
| `quick-adjust.volume-icon` | PASS | ui-panels-control.md §quick-adjust |
| `quick-adjust.auto-hide` | PASS | ui-panels-control.md §quick-adjust |
| `tray.menu.open` | PASS | ui-bar-popups.md §1 |
| `tray.menu.scroll` | PASS | ui-bar-popups.md §1 |
| `tray.menu.submenu-back` | PASS | ui-bar-popups.md §1 |
| `bar.volume.touchpad-phases` | PASS | ui-bar-layout.md / Qt Wayland axis semantics |
| `control.slider-drag.volume` | PASS | ui-panels-control.md §2.3 |
| `control.slider-drag.chassis` | PASS | ui-panels-control.md §2.3 |
| `control.slider-drag.pump` | PASS | ui-panels-control.md §2.3 |
| `control.slider-drag.brightness` | PASS | ui-panels-control.md §2.3 |
| `calendar.horizontal-scroll-week` | PASS | ui-panels-calendar.md §2.4 |
| `calendar.open-event.all-day` | PASS | ui-panels-calendar.md §2 |
| `calendar.open-event.timed` | PASS | ui-panels-calendar.md §2 |
| `wifi.secure-password-join` | PASS | ui-panels-wifi.md §3 / WifiFallback.qml:921 |
| `wifi.empty-password-return` | PASS | ui-panels-wifi.md §3 |
| `quick-adjust.passthrough` | PASS | QuickAdjustPopup.qml:112 |
| `control.profile-cycle` | PASS | ControlPanelPopup.qml:297 |
| `mihomo.back` | PASS | MihomoControlPanel.qml:513 |
| `mihomo.start` | PASS | MihomoControlPanel.qml:513 |
| `bar.volume.scroll-decrease` | PASS | shell.qml:4138 |
| `bar.hover.Battery` | PASS | shell.qml:4152 / shell.qml:4260 |
| `bar.scroll.Battery` | PASS | shell.qml:4152 / shell.qml:4260 |
| `bar.hover.Overflow` | PASS | shell.qml:4152 / shell.qml:4260 |
| `bar.scroll.Overflow` | PASS | shell.qml:4152 / shell.qml:4260 |
| `ipc.desktop.setVariant.v1` | PASS | hypr/scripts/toggle-battery-mode.sh / native desktop IPC |
| `ipc.desktop.setVariant.v2` | PASS | hypr/scripts/toggle-battery-mode.sh / native desktop IPC |
| `ipc.desktop.setVariant.v3` | PASS | hypr/scripts/toggle-battery-mode.sh / native desktop IPC |
| `ipc.desktop.setVariant.bt` | PASS | hypr/scripts/toggle-battery-mode.sh / native desktop IPC |
| `ipc.desktop.deskflowReflow` | PASS | hypr/scripts/rotate-screen-ccw.sh / native desktop IPC |
| `navigation.bar-wifi` | PASS | shell.qml:4182 |
| `navigation.bluetooth-page` | PASS | ControlPanelPopup.qml:580 |
| `tray.overflow.secondary` | PASS | TrayButton.qml:46 / shell.qml:1204 |
| `tray.overflow.scroll` | PASS | TrayButton.qml:46 / shell.qml:1204 |
| `outputs.add` | PASS | ui-bar.md §1.1 |
| `outputs.input-second` | PASS | ui-bar.md §per-output instancing |
| `outputs.reorder` | PASS | ui-bar.md §1.1 |
| `outputs.remove` | PASS | ui-bar.md §1.1 |
| `outputs.scale-1.5` | PASS | ui-bar.md §0 / fractional-scale-v1 |
| `outputs.scale-2` | PASS | ui-bar.md §0 / fractional-scale-v1 |
| `ipc.shell.kill` | PASS | ui-bar.md §4 / hyprland-bind.lua:34 |
| `compositor.disconnect` | PASS | hyprv-ui Application connection-loss policy |
| `compositor.restart-fresh-reconnect` | PASS | hyprv-ui Application lifecycle |

Source index: bar hit areas/actions `shell.qml:3771`, `shell.qml:4032`, `TrayButton.qml:46`; tray backdrop/rows `shell.qml:914`, `shell.qml:1060`; main controls `ControlPanelPopup.qml:518`, `ControlPanelPopup.qml:683`; Bluetooth `ControlPanelBluetoothDetails.qml:194`; Wi-Fi form `WifiFallback.qml:776`, `WifiFallback.qml:921`; calendar `WeekCalendarPanel.qml:365`, `WeekCalendarPanel.qml:420`, `WeekCalendarPanel.qml:520`, `WeekCalendarPanel.qml:645`; island `DynamicIsland.qml:808`, `DynamicIsland.qml:932`, `DynamicIsland.qml:1228`; IPC `shell.qml:44`, `shell.qml:3403`, `shell.qml:3423`. Inactive legacy components (`hypr-comp`, DeskflowTargetControl, ControlPanelWifiDetails) are excluded as specified.

## Engine regression: output rotation (hyprv-ui)

Test: `crates/hyprv-ui/tests/nested_rotation.rs` (opt-in). Run it with `crates/hyprv-ui/tests/run-nested-rotation.sh [report]`, which uses a bwrap sandbox, a clean environment, private D-Bus, a virtual KWin and a nested Hyprland 0.56.2 with a 1920×1080 output.

Mapped during every step: a top bar (anchored left+right, exclusive 32), a full-output overlay, and two popup overlays (300×200 centred; 360×480 top-right). After each transform the test forces a full repaint, then asserts:
- no Wayland protocol error;
- the engine's configured sizes equal the rotated logical output, with width and height swapped for 90/270;
- `output_logical_size()` equals the rotated logical output;
- the compositor's `hyprctl -j layers` sizes are identical;
- every surface carries the output's scale.

Regression for docs/requests/hyprv-ui.md, 2026-10-23 (`wp_viewport` "Box doesn't fit"). With the fix disabled, the test fails at scale 1, transform 90.

27 / 27 steps pass. Overall: **PASS**.

| Scale | Transform | Logical output | Result |
| --- | --- | --- | --- |
| 1 | 0 (normal) | 1920x1080 | PASS |
| 1 | 1 (90) | 1080x1920 | PASS |
| 1 | 2 (180) | 1920x1080 | PASS |
| 1 | 3 (270) | 1080x1920 | PASS |
| 1 | 4 (flipped) | 1920x1080 | PASS |
| 1 | 5 (flipped-90) | 1080x1920 | PASS |
| 1 | 6 (flipped-180) | 1920x1080 | PASS |
| 1 | 7 (flipped-270) | 1080x1920 | PASS |
| 1 | 0 (normal) | 1920x1080 | PASS |
| 1.5 | 0 (normal) | 1280x720 | PASS |
| 1.5 | 1 (90) | 720x1280 | PASS |
| 1.5 | 2 (180) | 1280x720 | PASS |
| 1.5 | 3 (270) | 720x1280 | PASS |
| 1.5 | 4 (flipped) | 1280x720 | PASS |
| 1.5 | 5 (flipped-90) | 720x1280 | PASS |
| 1.5 | 6 (flipped-180) | 1280x720 | PASS |
| 1.5 | 7 (flipped-270) | 720x1280 | PASS |
| 1.5 | 0 (normal) | 1280x720 | PASS |
| 2 | 0 (normal) | 960x540 | PASS |
| 2 | 1 (90) | 540x960 | PASS |
| 2 | 2 (180) | 960x540 | PASS |
| 2 | 3 (270) | 540x960 | PASS |
| 2 | 4 (flipped) | 960x540 | PASS |
| 2 | 5 (flipped-90) | 540x960 | PASS |
| 2 | 6 (flipped-180) | 960x540 | PASS |
| 2 | 7 (flipped-270) | 540x960 | PASS |
| 2 | 0 (normal) | 960x540 | PASS |
