# Laptop e2e — scaffold status

<!-- laptop-qa:consolidated -->
## Consolidated laptop QA

Generated 2026-10-24 12:52 EDT by `parity/laptop/run-laptop-qa.py`. Overall: **PASS**.

- shell binary: `target/release/hyprv-shell` built 2026-10-24 12:35, sha256 `3cd717d848388776…` (pinned copy)

| area | check | result | status |
| --- | --- | --- | --- |
| e2e | fakes self-test | 7/7 | PASS |
| e2e | laptop nested suite (`lap.*`, `ipc.shell.kill`) | 37/37 | PASS |
| desktop regression | desktop nested e2e (auto-detect) | desktop suite 204/204 (want 204/204) | PASS |
| laptop parity | `laptop-bar` (spec-bar replay, full frames) | 248/248 | PASS |
| laptop parity | `laptop-panels` control panel (card region) | 92/92 | PASS |
| laptop parity | `laptop-panels` QuickAdjust (card region) | 32/32 | PASS |
| laptop parity | `laptop-panels` battery popup (spec-bar, full frames) | 28/28 | PASS |
| laptop parity | island (= main island, D1; desktop island references, GL) | 380 pass | PASS |
| desktop regression | desktop parity `island-gl` (baseline 380 pass) | 380 pass | PASS |
| desktop regression | desktop parity `bar-gl` (baseline 300 pass, 12 excluded) | 12 excluded, 300 pass | PASS |
| desktop regression | desktop parity `panels-gl` (baseline 300 pass) | 300 pass | PASS |
<!-- /laptop-qa:consolidated -->


Generated 2026-10-24 12:43 EDT by `parity/laptop/e2e/run-laptop-e2e.py`. Plan:
`docs/verify/laptop-qa-plan.md` §4. Shell probed: /tmp/claude-1000/-home-wilf--config-HyprV-rshell/8a2a2d22-d87b-4cfc-a723-6962f94f890d/scratchpad/laptop-qa.8vs0u5xz/hyprv-shell (41 MiB).

- nested laptop: exit 0, suite_complete=True, 39 cases
- nested desktop: exit 0, suite_complete=True, 204 cases

## Fakes self-test (private dbus-daemon, temp sysfs tree)

| check | result | detail |
| --- | --- | --- |
| fake UPower: display device, OnBattery, enumerate | PASS |  |
| fake UPower: scenario step emits PropertiesChanged and updates state | PASS |  |
| fake PowerProfiles: both names, client write logged and mirrored | PASS |  |
| fake login1: LidClosed, PrepareForSleep signal, Inhibit returns an fd | PASS |  |
| org.hyprv.Test.Step: presets apply synchronously and emit their signals | PASS |  |
| fake sysfs: battery, AC, backlight, thermal, hwmon, DMI attributes | PASS |  |
| hypr.controls stub plan() = closed form | PASS | 101 levels match |

## Cases

PENDING names the missing shell hook. No case is passed by the scaffold itself, except
the table check of the brightness stub.

| ID | action | expected | result | note |
| --- | --- | --- | --- | --- |
| `lap.profile.detect.dmi` | no override; fake DMI XPS 13 9350 + BAT0 | laptop profile; reason logged | PASS | {"profile": "laptop", "sys_root": "/run/user/1000/hvl.tkwnexu1/run/nested-laptop/run-909387/fixture/lap.profile.detect.dmi"} |
| `lap.profile.detect.desktop` | fake desktop DMI, no battery | desktop profile; desktop subset unchanged | PASS | {"profile": "desktop", "sys_root": "/run/user/1000/hvl.tkwnexu1/run/nested-laptop/run-909387/fixture/lap.profile.detect.desktop"} |
| `lap.profile.override` | HYPRV_PROFILE=desktop on XPS fakes and vice versa | override wins | PASS | {"profile": "desktop", "sys_root": "/run/user/1000/hvl.tkwnexu1/run/nested-laptop/run-909387/fixture/lap.profile.override"} |
| `lap.bar.boot.compact` | start | all bars compact, exclusive zone 25, no island content | PASS | {"actions": [], "bars": [{"battery_available": false, "battery_on_battery": true, "battery_percent": 0.0, "calendar": false, "expanded": false, "hovered": "None", "island": [660.0, -58.0, 120.0, 38.0], "modules": [{"interactive": true, "rect": [10.0, -58.0, 28.1875, 38.0], "target": "Launcher"}, {"i |
| `lap.bar.toggle` | ipc call bar toggle eDP-1 | expanded after the slide; layoutStatus keys = legacy | PASS | {"bottomCornerOffset": 0, "compactOffset": -0.0, "curve": [0.1, 0.9, 0.1, 1.05, 1.0, 1.0], "duration": 700, "expanded": true, "fullscreen": false, "fullscreenHide": 0.0, "presented": true, "reveal": 1.0, "topCornerOffset": 0.0} |
| `lap.bar.toggle.debounce` | two toggles 300 ms apart | one transition | PASS | {"bottomCornerOffset": 0, "compactOffset": -0.0, "curve": [0.1, 0.9, 0.1, 1.05, 1.0, 1.0], "duration": 700, "expanded": true, "fullscreen": false, "fullscreenHide": 0.0, "presented": true, "reveal": 1.0, "topCornerOffset": 0.0} |
| `lap.bar.toggle.permonitor` | two outputs, toggle one | other stays compact | PASS | {"actions": [], "bars": [{"battery_available": false, "battery_on_battery": true, "battery_percent": 0.0, "calendar": false, "expanded": true, "hovered": "None", "island": [660.0, 10.0, 120.0, 38.0], "modules": [{"interactive": true, "rect": [10.0, 10.0, 28.1875, 38.0], "target": "Launcher"}, {"inte |
| `lap.battery.discharge.tick` | UPower Percentage 21 -> 20 | glyph/colour flip, no other damage | PASS | {"actions": [], "bars": [{"battery_available": true, "battery_on_battery": true, "battery_percent": 20.0, "calendar": false, "expanded": false, "hovered": "None", "island": [660.0, -58.0, 120.0, 38.0], "modules": [{"interactive": true, "rect": [10.0, -58.0, 28.1875, 38.0], "target": "Launcher"}, {"i |
| `lap.battery.plug` | State Discharging -> Charging, OnBattery false | plug glyph | PASS | {"actions": [], "bars": [{"battery_available": true, "battery_on_battery": false, "battery_percent": 57.0, "calendar": false, "expanded": false, "hovered": "None", "island": [660.0, -58.0, 120.0, 38.0], "modules": [{"interactive": true, "rect": [10.0, -58.0, 28.1875, 38.0], "target": "Launcher"}, {" |
| `lap.battery.full` | FullyCharged | full rendering | PASS | {"actions": [], "bars": [{"battery_available": true, "battery_on_battery": false, "battery_percent": 100.0, "calendar": false, "expanded": false, "hovered": "None", "island": [660.0, -58.0, 120.0, 38.0], "modules": [{"interactive": true, "rect": [10.0, -58.0, 28.1875, 38.0], "target": "Launcher"}, { |
| `lap.battery.popup.open` | click the battery module | popup; fake sysfs history read; estimate text per mode | PASS | {"actions": [], "bars": [{"battery_available": true, "battery_on_battery": true, "battery_percent": 57.0, "calendar": false, "expanded": true, "hovered": "None", "island": [660.0, 10.0, 120.0, 38.0], "modules": [{"interactive": true, "rect": [10.0, 10.0, 28.1875, 38.0], "target": "Launcher"}, {"inte |
| `lap.battery.popup.close` | click outside / Esc | closes; 1 s sampling stops | PASS | {"actions": [], "bars": [{"battery_available": true, "battery_on_battery": true, "battery_percent": 57.0, "calendar": false, "expanded": true, "hovered": "None", "island": [660.0, 10.0, 120.0, 38.0], "modules": [{"interactive": true, "rect": [10.0, 10.0, 28.1875, 38.0], "target": "Launcher"}, {"inte |
| `lap.power.profile.set` | CP profile tile -> power-saver | fake PPD ActiveProfile set once; UI follows signal | PASS | {"state": {"available": true, "battery_mode": "discharging", "battery_percent": 57.0, "battery_present": true, "energy_full_wh": 54.0, "energy_now_wh": 30.8, "energy_rate_w": 8.4, "lid_closed": false, "lid_present": true, "on_battery": true, "performance_degraded": null, "profile": "PowerSaver", "pr |
| `lap.power.profile.external` | fake changes profile | bar/CP update, no command | PASS | {"native": {"available": true, "battery_mode": "discharging", "battery_percent": 57.0, "battery_present": true, "energy_full_wh": 54.0, "energy_now_wh": 30.8, "energy_rate_w": 8.4, "lid_closed": false, "lid_present": true, "on_battery": true, "performance_degraded": null, "profile": "Performance", " |
| `lap.brightness.slider` | drag CP slider 50 -> 71 | ordered BrightnessSet per distinct value, last 71; never BrightnessFinish; one read-back 90 ms after the last set, no overwrite mid-drag | PASS | {"actual": [{"action": "Set { output: \"WAYLAND-1\", percent: 50.0, sequence: 1 }", "service": "backlight"}, {"action": "Set { output: \"WAYLAND-1\", percent: 60.0, sequence: 2 }", "service": "backlight"}, {"action": "Set { output: \"WAYLAND-1\", percent: 65.0, sequence: 3 }", "service": "backlight" |
| `lap.brightness.wheel` | wheel one notch on the CP slider; IPC step | wheel ignored (laptop ControlPanelSlider.qml has no wheel handler); IPC/key step 1 | PASS | {"legacy": "ControlPanelSlider.qml:112-166 has no onWheel/handler; wheel is ignored, IPC preview changes by1", "native_wheel": {"actual": [], "expected": [], "native_inputs": ["Pointer(Enter { serial: 79 })", "Pointer(Motion { time: 20657 })", "Scroll(ScrollEvent { pixel_delta: (-0.0, 15.0), angle_d |
| `lap.brightness.osd` | ipc quickAdjust showBrightnessLevel 70 | OSD at 70 | PASS | {"actions": [], "bars": [{"battery_available": false, "battery_on_battery": true, "battery_percent": 0.0, "calendar": false, "expanded": false, "hovered": "None", "island": [660.0, -58.0, 120.0, 38.0], "modules": [{"interactive": true, "rect": [10.0, -58.0, 28.1875, 38.0], "target": "Launcher"}, {"i |
| `lap.brightness.plan` | Lua plan(L), L = 0..100 | matches the closed form | PASS | {"levels": 101, "oracle": "svc-power pinned origin/laptop controls.lua; M.plan only; no timer/action/hardware call"} |
| `lap.lid.close_open` | login1 LidClosed true -> false | no UI change, no wakeup burst | PASS | {"closed": {"available": true, "battery_mode": "discharging", "battery_percent": 57.0, "battery_present": true, "energy_full_wh": 54.0, "energy_now_wh": 30.8, "energy_rate_w": 8.4, "lid_closed": true, "lid_present": true, "on_battery": true, "performance_degraded": null, "profile": "Balanced", "prof |
| `lap.sleep.resume` | PrepareForSleep(true) then (false) | state re-synced within one frame | PASS | {"sleep": {"available": true, "battery_mode": "discharging", "battery_percent": 57.0, "battery_present": true, "energy_full_wh": 54.0, "energy_now_wh": 30.8, "energy_rate_w": 8.4, "lid_closed": false, "lid_present": true, "on_battery": true, "performance_degraded": null, "profile": "Balanced", "prof |
| `lap.thermal.source` | zones: pkg -> hwmon only -> none | temperature source and -- | PASS | {"hwmon": {"core_percent": [], "cpu_history": [], "cpu_percent": null, "elapsed": {"nanos": 0, "secs": 0}, "generation": 0, "iface": null, "memory_available_bytes": 0, "memory_history": [], "memory_percent": null, "memory_total_bytes": 0, "network_history": [], "package_power_w": null, "rx_bytes": 0 |
| `lap.media.compact.idle` | MPRIS playing, bar compact | no cava, no position reads | PASS | {"actions": [], "bars": [{"battery_available": false, "battery_on_battery": true, "battery_percent": 0.0, "calendar": false, "expanded": false, "hovered": "None", "island": [660.0, -58.0, 120.0, 38.0], "modules": [{"interactive": true, "rect": [10.0, -58.0, 28.1875, 38.0], "target": "Launcher"}, {"i |
| `lap.media.expand` | toggle bar while playing | track + position on first frame | PASS | {"actions": [], "bars": [{"battery_available": false, "battery_on_battery": true, "battery_percent": 0.0, "calendar": false, "expanded": true, "hovered": "None", "island": [660.0, 10.0, 120.0, 38.0], "modules": [{"interactive": true, "rect": [10.0, 10.0, 28.1875, 38.0], "target": "Launcher"}, {"inte |
| `lap.island.hidden.compact` | agent active, bar compact | no island draw; model still updates | PASS | {"before": {"actions": [], "bars": [{"battery_available": false, "battery_on_battery": true, "battery_percent": 0.0, "calendar": false, "expanded": false, "hovered": "None", "island": [652.0, -58.0, 132.0, 38.0], "modules": [{"interactive": true, "rect": [10.0, -58.0, 28.1875, 38.0], "target": "Laun |
| `lap.island.agent.focus` | click a session row | focus action | PASS | {"actual": [{"action": "FocusSession(\"codex-session-0\")", "service": "agent"}], "expected": [{"action": "FocusSession(\"codex-session-0\")", "service": "agent"}], "native_inputs": ["Pointer(Motion { time: 17463 })", "Pointer(Press { time: 17463, button: 272, serial: 61 })", "Pointer(Release { time |
| `lap.island.readonly` | pending approval in state.json, expand | no pending card or buttons (main semantics) | PASS | {"actions": [], "bars": [{"battery_available": false, "battery_on_battery": true, "battery_percent": 0.0, "calendar": false, "expanded": true, "hovered": "None", "island": [652.0, 10.0, 132.0, 38.0], "modules": [{"interactive": true, "rect": [10.0, 10.0, 28.1875, 38.0], "target": "Launcher"}, {"inte |
| `lap.island.hook.nonblocking` | hyprv-agent-hook PermissionRequest | exits at once; installer timeouts 5 | PASS | {"binary": "/home/wilf/.config/HyprV/rshell/crates/hyprv-shell/../../target/debug/hyprv-agent-hook", "elapsed_ms": 5, "state": {"pending_requests": {}, "sessions": {"laptop-permission-e2e": {"current_tool": "Bash", "cwd": "/tmp", "detail": "true", "event": "PermissionRequest", "had_activity": true,  |
| `lap.island.list_inplace` | 9 sessions, scroll, write state.json | scroll and rows kept | PASS | {"after": {"hash": "50299cb9e8fe4917", "pixels": 878080, "source": "retained EGL RGBA8"}, "before": {"hash": "93de74ff9e50ce41", "pixels": 878080, "source": "retained EGL RGBA8"}, "scrolled": {"hash": "50299cb9e8fe4917", "pixels": 878080, "source": "retained EGL RGBA8"}} |
| `lap.cp.nofans` | open the CP | no fan section; no fan D-Bus calls | PASS | {"actions": [], "bars": [{"battery_available": false, "battery_on_battery": true, "battery_percent": 0.0, "calendar": false, "expanded": false, "hovered": "None", "island": [660.0, -58.0, 120.0, 38.0], "modules": [{"interactive": true, "rect": [10.0, -58.0, 28.1875, 38.0], "target": "Launcher"}, {"i |
| `lap.privhelper.inert` | laptop profile start | no GetFans/SubscribeCpuPower/RemoveExternalGpu; no root units planned | PASS | {"bus_names": ["org.freedesktop.DBus", "org.freedesktop.login1", "net.hadess.PowerProfiles", "org.freedesktop.UPower", "org.freedesktop.UPower.PowerProfiles", ":1.0", ":1.1", "org.hyprv.Test"], "fan": {"available": false, "channels": [], "chip": "", "desktop_policy": false, "last_error": null, "temp |
| `lap.egpu.ui` | open the CP, no eGPU device | no eGPU card (laptop confirmEgpuAction() is never called); no root service, inactive channel rejects actions | PASS | {"egpu": {"busy": false, "devices": [], "last_error": null, "last_results": []}, "inactive_channel_rejected_action": true, "ui": "active laptop CP has no eGPU card; unavailable state only"} |
| `lap.strip.control` | click the compact-strip control-panel icon | CP opens anchored to the strip | PASS | {"actions": [], "bars": [{"battery_available": false, "battery_on_battery": true, "battery_percent": 0.0, "calendar": false, "expanded": false, "hovered": "None", "island": [660.0, -58.0, 120.0, 38.0], "modules": [{"interactive": true, "rect": [10.0, -58.0, 28.1875, 38.0], "target": "Launcher"}, {"i |
| `lap.strip.stats` | compact strip stats | CPU % / MEM % / power text from minimal sampling | PASS | {"actions": [], "bars": [{"battery_available": false, "battery_on_battery": true, "battery_percent": 0.0, "calendar": false, "expanded": false, "hovered": "None", "island": [660.0, -58.0, 120.0, 38.0], "modules": [{"interactive": true, "rect": [10.0, -58.0, 28.1875, 38.0], "target": "Launcher"}, {"i |
| `lap.strip.clock` | compact strip clock | minute-aligned hh:mm | PASS | {"actions": [], "bars": [{"battery_available": false, "battery_on_battery": true, "battery_percent": 0.0, "calendar": true, "expanded": true, "hovered": "None", "island": [460.0, 10.0, 520.0, 500.0], "modules": [{"interactive": true, "rect": [10.0, 10.0, 28.1875, 38.0], "target": "Launcher"}, {"inte |
| `lap.strip.workspace` | click a strip workspace | workspace switch action | PASS | {"actual": [{"action": "WorkspaceActivate(3)", "service": "hyprland"}], "expected": [{"action": "WorkspaceActivate(3)", "service": "hyprland"}], "native_inputs": ["Pointer(Enter { serial: 147 })", "Pointer(Motion { time: 32975 })", "Pointer(Press { time: 32975, button: 272, serial: 148 })", "Pointer |
| `ipc.shell.kill` | hyprv-shell kill at suite end | clean shutdown acknowledged, IPC socket removed | PASS | {"acknowledgement": {"acknowledged": true, "result": ""}, "exit_code": 0, "ipc_removed": true} |
| `lap.wifi.scan.visible` | open the Wi-Fi page | scan enabled only while visible | PASS | {"actions": [], "bars": [{"battery_available": false, "battery_on_battery": true, "battery_percent": 0.0, "calendar": false, "expanded": true, "hovered": "None", "island": [660.0, 10.0, 120.15625, 38.0], "modules": [{"interactive": true, "rect": [10.0, 10.0, 28.1875, 38.0], "target": "Launcher"}, {" |
| `lap.desktop.regression` | desktop nested e2e + parity, profile auto on this desktop | 204/204, parity counts unchanged | PASS | desktop suite 204/204 (want 204/204); parity counts: see parity gates |
