# Equivalence coverage and limits

The probe includes all five service crates and all currently exported modules. Topics are subscribed and retained; fan is the sole startup skip because its privileged helper has no owner. Every comparison is recorded with exact legacy and native values in raw/comparisons.json. Counts include each application, session, window and calendar event separately; they are not a percentage of product parity.

| Domain | Original source or QML provider read | Observed mapping / remaining limit |
|---|---|---|
| Network / Bluetooth | wifi-status.sh / bluetooth-status.sh | Adapter and per-device/AP fields; RSSI/strength can change between reads. Unavailable optional source fields are explicit. The original Bluetooth script exceeded 12 seconds; a dedicated aligned net-only follow-up with a 45-second deadline succeeded and supplied its field comparisons. |
| Audio | audio_fetch.sh and native PipeWire topic | Mute and percent; original auxiliary script truncates while main QML rounds (one-point tolerance). Stream/device actions untested. |
| MPRIS / Cider / spectrum | media-status.sh / MPRIS provider | Empty-player branch observed. Position is asynchronous. No Cider lookup actions or spectrum capture; populated metadata/spectrum unverified. |
| Notifications | first swaync-client -swb record | Text, tooltip, alt, DND and dot; no dismiss/toggle actions. |
| Tray | existing SNI watcher + per-item GetAll | Registration count, IDs, title/status/category, icon names/theme/derived source, ARGB pixmap dimensions+SHA256, tooltip strings, menu presence/path. Menu hierarchy preparation/actions unverified. |
| Power | QML UPower display device and power-profiles properties | Battery gating/mode/energy/time and profiles. This host has no laptop battery; battery-history's peripheral mismatch is reported. |
| Battery history / sysstats | battery-info.sh / system-snapshot.sh | Battery script unavailable branch, absolute memory/temperature. CPU/network/history rates require aligned sampling and remain unverified. RAPL differs from legacy helper power file. |
| Fan / backlight | fan-control-helper.sh status / QML brightness provider | Fan saved status retained as reference; absent helper not activated. Native backlight snapshot recorded; legacy Lua brightness_get was not invoked, so no physical brightness parity claim. |
| Inhibitor / demo / recording | prevent-sleep.sh status / demo-mode.sh status / control-panel-status.sh | Current boolean/source/headless/PID fields; native FD lease ownership differs from legacy sleeper PID. No action handoff or recording capture. |
| VM / Mihomo | windows-vm-control.sh status / mihomo-control.sh status | Running and proxy service/online/version/mode/rates/dashboard fields; rates are asynchronous. No auth, units, HTTP config writes or portal actions. |
| Hyprland | original QML provider JSON via hyprctl read commands | Clients, monitor transform/scale/geometry, active window/workspaces, keymap and motion. This compositor omits numeric IDs for special workspaces; identifier mapping is flagged as unavailable, not certified. No compositor dispatch. |
| Calendar | CalendarBridge.qml calendars.list / events.list | Existing native socket, same local week converted to UTC -7/+14 days, per-calendar flags/color/name and per-event UID/start/end/all-day/title/location. Correlated replies ignore greeting; no ui.* actions. |
| Weather | original weather.sh --json with isolated cache | String-to-typed normalization, five dates/day fields/hourly slots/current fields. Missing-credential dummy branch observed; real forecast credentials/network branch unverified here. |
| Agent island | agent-island-status.py --json on shadow copy | Per-session labels/status/details/base-to-display progress; live hooks/process scans make timestamps and progress temporal. No real state cleanup. |
| Applications / theme | original app_fetcher.py / ui-state.sh print on shadow copy | All indexed names/exec/icon and theme/variant. No application activation/theme apply. |
| IR / Deskflow / Viewflow | hdmi-switch-ir.py list + existing database / target status scripts | Database carrier/duty/button records and observed target. No serial messages, host switch, service control or IR replay. |
| eGPU / startup / generic systemd | No eligible original status branch for these action-only/internal APIs | Native inventory/default readiness/unit topics are streamed; action results are unavailable. No eGPU removal or startup approval. |

Reference scripts run sequentially while Rust actors remain live, so readings are not atomic. TEMPORAL differences are retained and never counted as passes. Provider errors are recorded in raw/legacy.json; default/empty states only prove that branch. Visual rendering, hardware controls, suspend/resume, polkit and integrated UI behavior are outside this read-only task. Build inputs are frozen snapshots; later concurrent owner edits are not implicitly verified.
