# System interactions and migration contract

Audit date: 2026-10-23. Sources are the current read-only `../quickshell` tree and its direct dependencies. References below are relative to `quickshell/` unless prefixed `HyprV/` or absolute. This is a source audit and proposed implementation, not evidence of live hardware, authentication, compositor, or remote-peer validation. `services-api.md` defines the proposed internal API. The evidence inventory at the end gives exact source lines for commands, schemas, paths, environment variables, and scheduling in **every script and QML I/O owner**, including the separate hypr-comp shell. Backup scripts are historical dependencies only, not active backends.

## Runtime and ownership

One actor per domain, shared by all screens. Acquire bus subscriptions before initial snapshots; merge changes received during enumeration; invalidate cached object paths when a bus owner disappears. Publish only changed typed values. All actions carry request IDs and return independently of their initiating popup. Keep observed state separate from requested state and errors; never infer hardware success from a successful write alone.

Use `zbus`/`zvariant` for system/session buses, `tokio` for sockets and deadlines, `udev` for hotplug, `rtnetlink` for routes/link changes, `inotify` or `notify` for parent-directory watches, `pipewire` for audio, `reqwest` for HTTP, `tokio-tungstenite` for WebSockets, `tokio-serial`/`nix` for serial and process handles, `serde`/`serde_json`, `chrono`/`chrono-tz`, and `rustfft` for spectrum. These are recommendations, not pinned dependency/version compatibility claims.

No runtime helper execution. Existing external-app actions must use desktop activation (`org.freedesktop.Application.Activate`/`Open`, `org.freedesktop.portal.OpenURI.OpenURI`, or systemd user activation of preinstalled app units). Services without activation need an explicitly designed protocol adapter; an implicit shell-command fallback would violate architecture. Maintenance/install tools may remain outside the runtime. Preserve app launch and lock/logout UI behavior; inability to activate an arbitrary application is an integration gap, not permission to silently drop an action.

KDE models: NetworkManagerQt/plasma-nm secrets and connection lifecycle; BluezQt pairing prompts; plasma-pa default-node tracking; MPRIS engine local position clock; PowerDevil UPower/profile/backlight ownership; ksystemstats demand-driven sampling; KAuth-style narrow privileged operations. Do not require a running KDE desktop or assume its private APIs exist.

## Current scheduling and wire contracts

`PollCommand.qml:8` defines default 1000 ms, triggered-on-start refresh when its active timer starts, and an explicitly requested queued refresh while a process runs; its timer only runs while active. `StreamCommand.qml:12` runs one process with line parsing; restarts a stopped process on a 5000-ms timer; this is reconnection, not delivery acknowledgement. Consumers below refer to actual instantiations; `BarServices.qml` is an additional component, not evidence that its media path replaces the inline shell path. The main shell currently uses the inline 1-second media poll (`shell.qml:3314`).

| Producer / consumer | Current cadence and exact payload |
|---|---|
| `desktop-state.py` → `shell.qml:3200` | NDJSON `{type:string,data:...}`; types `system` (string), `wifi` (object), `agent` (object). Suppresses identical serialized data. System every 1000 ms, agent 650 ms active /1200 ms inactive; process scan threshold 1.5 s. NM events coalesce 60 ms. |
| System → `shell.qml:2314` | String sections `__STAT__`, `__MEM__`, `__TEMP__`, `__CPU_POWER__`, `__ROUTE__`, `__NET__`, each followed by original proc/sys text. CPU jiffies per total/core; memory KiB from `/proc/meminfo`; temperature integer millidegrees C; package power decimal W; IPv4 route table; RX/TX cumulative bytes from `/proc/net/dev`. CPU and rates use successive samples and elapsed time, not raw counters. |
| Wi-Fi → `shell.qml:2046` | JSON `{present:bool,iface:string,enabled:bool,hardwareEnabled:bool,connected:bool,ssid:string,signal:number(0..100),security:string,networks:[{ssid:string,signal:number,security:string,secure:bool,enterprise:bool,active:bool,known:bool,bars?:string}]}`. Strongest/active AP per SSID; active first, descending strength. QML signal is normalized to 0..1. `wifi-status.sh` fallback schema and escaping appear in inventory. |
| Bluetooth native → `shell.qml:2119` | Device display snapshot comes from native Quickshell.Bluetooth objects; paired/connected/trusted/blocked/bonded/pairing/name/icon/state signals drive updates. Fallback script emits JSON; its literal field declaration is in inventory. Four-second user scan, 12-second action timeout; not periodic scans. |
| Native PipeWire → `shell.qml:3285` | Default audio sink ready/audio availability, mute boolean, volume scalar converted with `round(volume*100)` to nonnegative integer percent (may exceed 100). |
| Native UPower / PowerProfiles | Display-device present+laptop check; percentage normalized from 0..1 to 0..100; states charging/discharging/full/idle/plugged/unknown. Profile performance/balanced/power-saver. Native subscriptions, no shell timer. `BarServices.qml:75`. |
| `media-status.sh` → `shell.qml:3314` | Newline `key=value`, strings sanitized to one line: `cider`, `available`, `playing` boolean literals; `title`,`artist`,`player`,`art_url` strings; `position`,`length` decimal seconds (`mpris:length` microseconds divided by 1e6). No selected player: available=false and zero times, exit 1. Poll every 1000 ms while media UI and players exist. |
| `BarServices.qml:110` media alternative | Native metadata on change; visible+playing local position tick 1000 ms; Cider repair debounce 200 ms, retry 30000 ms only for confirmed Cider while visible+playing; one queued repair and stale-metadata suppression. |
| `battery-info.sh` → `shell.qml:3339` | JSON `{available:bool,status:string,mode:string,capacity:number(%),powerW:number,averagePowerW:number,sampleCount:integer,sampleWindowSeconds:integer,windowComplete:bool,estimateSeconds:number,estimateBasis:string,energyNowWh:number,energyFullWh:number}`; absent battery only `{available:false}`. Positive signed power charging, negative discharging, zero otherwise. Charge×voltage fallback from µAh/µV to µWh, current×voltage to µW. 1800-second rolling TSV window; 1000-ms poll only while detail popup visible. |
| `control-panel-status.sh` → `shell.qml:3378` | Newline booleans `recording`,`prevent_sleep`,`demo_mode`, plus `fan_available`; when available integers `fan_chassis_percent`,`fan_pump_percent` 0..100 and `fan_chassis_rpm`,`fan_pump_rpm` RPM. Brightness deliberately absent. 1500 ms only while panel requested. |
| VM, Deskflow, Viewflow | Trimmed scalar strings `running\n`/`stopped\n`, or `windows\n`/`macos\n`. Visible panel controls poll 2000 ms; VM controller is panel-owned so actions survive Loader replacement. |
| Mihomo controls | 1500-ms visible polling in `MihomoControlPanel.qml:72-80`; 2000 ms only in the uninstantiated `MihomoControlTile.qml:203-211` (the page itself is unreachable from the current UI, ui-panels-control.md §7) <!-- audit: was "2000-ms visible polling" -->. JSON `{online:bool,service:string(none/user/system/verge),version:string,mode:string,connections:integer,upload:number,download:number,dashboard:string}`. Upload/download bytes/s from first `/traffic` stream frame; connection count from array length. |
| Spectrum → `shell.qml:3238` | Cava ASCII `n;n;...;\n`, 16 mono bars 0..100, 24 fps, 50–12000 Hz; nonfinite values replaced by zero. Runs only media-visible, playing, available, and cava installed. |
| Notifications → `shell.qml:3278` | `swaync-client -swb` NDJSON; consumed `alt:string` (contains `dnd`), `tooltip:string`. Malformed/empty resets icon/tooltip; state-reset details must follow consumer. |
| Theme FileViews | Parent state path `$XDG_STATE_HOME/hyprv/quickshell/ui-state.env` (fallback `~/.local/state`), watched/reloaded; `theme=dark` selects dark. `BarServices.qml:206`, `shell.qml:3219`. |
| Startup approval FileView | `StartupConfirm.qml:29`: explicit affirmative interaction writes exactly `launch\n` to `$HYPRV_STARTUP_APPROVAL`; cancellation/crash/write failure never approves. No recurring poll. |
| Fluent icon discovery | `shell.qml:2935`: one startup scan of `~/.local/share/icons`, `/usr/local/share/icons`, `/usr/share/icons`; lines `light=path`, `dark=path`, `base=path`, first Fluent-light/dark/Fluent directory. |
| Motion query | `BarMotion.qml:35`: `hyprctl -j animations` on startup/configreloaded, not polling. JSON `[animations,curves]`; inherited windowsMove/windows/global first overridden config. Duration round(speed×100) ms or 0 disabled; curve uses X0/Y0/X1/Y1 then 1,1. |

## Network and Bluetooth

`wifi-common.sh` selects an interface and wraps nmcli; `wifi-status.sh` uses nmcli discovery/known profiles and preserves escaped SSIDs; `wifi-action.sh:25` implements `toggle on|off` → `nmcli radio wifi STATE`, `rescan` → `nmcli dev wifi rescan [ifname IFACE]`, `disconnect` → `nmcli device disconnect IFACE`, `connect SSID [PASSWORD] [SECURITY]` → `nmcli dev wifi connect SSID [password PASSWORD] ifname IFACE`. Human stdout on success and stderr/nonzero on failure are action feedback, not JSON. Enterprise first-time setup can require connection editor. Caller is `shell.qml:2571`. Do not log supplied passwords.

Replacement: system bus `org.freedesktop.NetworkManager`, `/org/freedesktop/NetworkManager`, root `DeviceAdded`/`DeviceRemoved`, properties `WirelessEnabled`,`WirelessHardwareEnabled`,`ActiveConnections`; per-device `org.freedesktop.NetworkManager.Device.Wireless`, `AccessPointAdded`/`AccessPointRemoved`, `RequestScan(a{sv})`, `LastScan`, `ActiveAccessPoint`; AP `Strength`,`Ssid`,`Flags`,`WpaFlags`,`RsnFlags`; settings `NewConnection`, connection `Updated`/`Removed`. All property changes use `org.freedesktop.DBus.Properties.PropertiesChanged`. Activation uses `ActivateConnection`/`AddAndActivateConnection2`; device `Disconnect`; profile settings use correct 802-11-wireless-security/802-1x structures. Register `org.freedesktop.NetworkManager.SecretAgent` through `AgentManager.RegisterWithCapabilities`; implement GetSecrets/CancelGetSecrets/SaveSecrets/DeleteSecrets, cancellation and keyring-backed storage. Polkit authorizes radio/profile operations. Scan only on visible Wi-Fi subscription/open or explicit rescan; await LastScan rather than a fixed refresh loop. [Official scan API](https://networkmanager.pages.freedesktop.org/NetworkManager/NetworkManager/gdbus-org.freedesktop.NetworkManager.Device.Wireless.html), [SecretAgent XML](https://cgit.freedesktop.org/NetworkManager/NetworkManager/tree/introspection/org.freedesktop.NetworkManager.SecretAgent.xml).

`bluetooth-common.sh`, `bluetooth-status.sh`, `bluetooth-action.sh` retain CLI fallback status and `toggle on|off`, `scan`, `connect ADDRESS [PAIRED]`, `disconnect ADDRESS`, `remove ADDRESS` behavior (pairing is part of connect for unpaired devices) (exact case arms/arguments in inventory). Main shell already manipulates native adapter/device properties directly (`shell.qml:2599`): enable, discovering, pair/connect/trust, disconnect and forget; action success is state-observed, with bounded timeout. Replacement system bus `org.bluez`, `/` `ObjectManager.GetManagedObjects` + `InterfacesAdded`/`InterfacesRemoved`; `Adapter1.Powered`, `StartDiscovery`/`StopDiscovery`, `RemoveDevice`; `Device1.Pair`/`Connect`/`Disconnect`, Trusted/Blocked and PropertiesChanged. Register `Agent1` at an owned object through `/org/bluez` `AgentManager1.RegisterAgent`/`RequestDefaultAgent`; support PIN/passkey/display/confirmation/authorization/cancel/release as appropriate. Pairing prompts are typed requests, never an uncontrolled external bluetoothctl session. rfkill requires separate hardware-block state (`udev` rfkill or `/dev/rfkill` readiness). Privileges follow BlueZ/polkit policy, no root shell. [BlueZ ObjectManager model](https://www.bluez.org/bluez-5-api-introduction-and-porting-guide/), [agent registration](https://github.com/bluez/bluez/blob/master/doc/org.bluez.AgentManager.rst).

## Audio, media, tray, notifications

Native `Pipewire` and `PwObjectTracker` usages in `shell.qml:3285` and `BarServices.qml:49` become one PipeWire registry/core connection, metadata tracking for `default.audio.sink`, node parameter subscription for channel volumes/mute. Set SPA Props per default sink; preserve cubic-volume conventions used by the existing controls, channel balance, and >100% values. Current UI commands are `hyprctl eval 'require("hypr.controls").volume("set",N)'`, toggle/inc/dec, and `pavucontrol` on context click (`QuickAdjustPopup.qml:76`, `shell.qml:4137`). Native PipeWire replaces both control and polling tools; no privilege escalation.

Media selects first playing, then paused-with-content, content, paused, first available (`BarServices.qml:88`, `media-status.sh:14`). Session bus `NameOwnerChanged` for `org.mpris.MediaPlayer2.*`, path `/org/mpris/MediaPlayer2`, Player `PropertiesChanged` for Metadata/PlaybackStatus/Rate and `Seeked(int64 µs)`. Actions Previous/Next/PlayPause, Seek relative µs, SetPosition(track object path, absolute µs); clamp against track length and verify capability flags. `Raise()` on base interface for media-app focus; retain Hyprland client matching fallback via native IPC if Raise unsupported. Anchor position+monotonic instant+rate; extrapolate only while playing and request visible frames, with no periodic D-Bus Position reads. Re-anchor on Seeked/track/playback/rate changes, resume, and initial owner discovery. [MPRIS Player API](https://specifications.freedesktop.org/mpris/latest/Player_Interface.html).

Cider is a real compatibility branch, not a generic player: `media-status.sh:65` identifies Chromium bus owner Comm=Cider, calls `cider-metadata-fallback.py --artist ... --album ... --length ... --current-title ... --shell`. That helper queries local Cider HTTP, token/config caches, and music lookup/cache fallbacks; consumed lines title/artist/album/art_url/source. API result may replace metadata; non-API only repairs absent/generic text. Exact endpoints, cache paths, environment and CLI are recorded below. Port via reqwest/serde with cancellable requests keyed by owner+track; no stale response may overwrite another song. No verified push contract in this source: metadata changes trigger repair, with existing 30-second visible+playing fallback maximum. HTTP lookup failure is degraded metadata, not player disappearance.

Spectrum replaces cava with passive PipeWire monitor capture plus `rustfft`; reproduce 16-bar smoothing/autosens/cutoffs from `audio-spectrum.sh:10`. Subscribe only when presented and playing, output at <=24 fps; drop buffered samples when hidden. Audio readiness is event-driven, but continuous spectrum is sampled computation and consumes wakeups while visible.

System tray: session bus `org.kde.StatusNotifierWatcher`, path `/StatusNotifierWatcher`, interface same; enumerate RegisteredStatusNotifierItems, subscribe StatusNotifierItemRegistered/Unregistered and host registration. Register host without stealing an existing watcher's name; own watcher only when absent. Each item `org.kde.StatusNotifierItem` properties and NewIcon/NewAttentionIcon/NewOverlayIcon/NewStatus/NewToolTip; Activate/SecondaryActivate/ContextMenu/Scroll; `com.canonical.dbusmenu` GetLayout/AboutToShow/Event and LayoutUpdated/ItemsPropertiesUpdated. Preserve icon pixmaps, themed icons, tooltip, ordering and menus (`TrayButton.qml:1`, `shell.qml:2423`). `hypr-comp/moon.qml:10` is an additional SystemTray consumer.

SwayNC stream/actions are replaced by session bus `org.erikreider.swaync`, path `/org/erikreider/swaync`, interface `org.erikreider.swaync.cc`: deployed introspection must verify subscribe/notification state and ToggleVisibility/ToggleDnd names/signatures before implementation. Do not invent guaranteed signal names from a CLI invocation. Source proves only `swaync-client -swb`, `-t -sw`, `-d -sw`, `-rs` (theme CSS reload). Consume provider events and request snapshot on owner change; no polling. If deployed API lacks those operations, implement a protocol adapter or report unavailable, not a resident CLI. This API detail remains an integration verification item.

## Power, brightness, fans, CPU telemetry

UPower system bus `/org/freedesktop/UPower`, root `DeviceAdded`/`DeviceRemoved`, `GetDisplayDevice`, device interface `org.freedesktop.UPower.Device` PropertiesChanged for Percentage/State/IsPresent/Energy/EnergyFull/EnergyRate/TimeToEmpty/TimeToFull; system `OnBattery`. Profile bus `net.hadess.PowerProfiles`, `/net/hadess/PowerProfiles`, ActiveProfile/Profiles/PerformanceDegraded changes and ActiveProfile Set; feature-detect newer providers via introspection rather than assuming desktop-specific daemon names. Current `power-profile.sh` prints glyph for current mode and `toggle` cycles performance→balanced→power-saver→performance; panel calls `powerprofilesctl set PROFILE` directly (`ControlPanelPopup.qml:312`). Polkit mediates sets.

Battery detailed history must retain the source's 1800-second sample window, signed watt values and estimate basis; UPower's own time estimates do not automatically reproduce it. Read sysfs `power_supply` extra fields on udev change and acquire a subscriber-counted 1-second detail sampling lease where telemetry does not emit changes. Preserve TSV format `epoch_seconds<TAB>signed_power_uw` and pruning/average semantics if importing old history. Missing values are unavailable, not physically measured zero. `battery-info.sh:8` sources `HyprV/scripts/lib/battery.sh`; this external dependency belongs in migration.

Brightness current ownership is `HyprV/hypr/controls.lua:1` and controls-profile/control-worker, not a linear display-backlight slider. Shell probes with `hyprctl repl 'return require("hypr.controls").brightness_get()'` on open/after interaction; sets via Lua brightness setters, previews and 90-ms finish verification (`shell.qml:2782`). Read `brightness-hdr.json` level; preserve 0..10 minimum hardware + SDR white, 10..70 logarithmic hardware, 70..100 HDR reference-luminance increase, per-output backend/ranges and 10-bit/HDR monitor fields. UI request sequences prevent stale probe overwrites. Replacement PowerDevil-style per-output actor, sysfs backlight brightness/max_brightness with udev `backlight` changes; DDC/CI VCP 0x10 via native I²C (`i2cdev`, ioctl) or linked libddcutil FFI, not command. Hyprland native socket commands retain compositor reference_luminance state. DDC often has no unsolicited brightness event: one read on acquisition and bounded end-of-interaction verification, never idle DDC reads. Native helper mediates device access where seat/group permission is insufficient; serial writes, timeout cancellation, and hardware readback errors remain visible.

**Legacy behavior (superseded for desktop control below):** Fan hardware `nct6799`, pwm1/fan1 chassis and pwm7/fan7 legacy-named pump (both chassis fans); status reports **saved percentage limits**, not instantaneous PWM (`fan-control-helper.sh:46`). For each channel cap=floor(limit*255/100); target 0 through 70000 m°C, floor(cap*(T−70000)/18000) through 88000 m°C, cap above; missing telemetry uses cap. Manual pwmN_enable=1; write only changed values. Persist `/home/wilf/.local/state/hyprv/fan-percent`, atomic rename, flock fan-percent.lock; service singleton fan-control.lock. Boot/no graphical wilf session uses 80/80 without replacing saved desktop limits; graphical session restores saved values. Sampling loop currently one second even when shell absent; UI writes debounce 35 ms and status refresh 250 ms (`shell.qml:3052`).

**Approved deliberate departure from legacy (2026-10-24, quiet chassis policy):** User decision 2026-10-24 raises emergency entry to strictly above 105000 mC (exactly 105C does not enter), with exit strictly below 100000 mC; faults still force255 immediately regardless of temperature or zero cap. Both nct6799 headers are chassis fans. Desktop control uses the independently saved control-center percentage as a hard normal-operation upper limit: cap=floor(limit*255/100). Named constants in `hyprv-privhelper/src/cooling.rs` define `MIN_START=77` (30% reliable start duty), `FAN_START_MC=72000`, `FAN_STOP_MC=65000`, `FAN_FULL_MC=88000`, `TEMPERATURE_EMA_SECONDS=10`, `PWM_RISE_PER_SECOND=10`, `PWM_FALL_PER_SECOND=2`, `EMERGENCY_MC=105000`, `EMERGENCY_EXIT_MC=100000`. Use elapsed monotonic time and EMA alpha=1-exp(-dt/10), seeded with the first valid reading. Start only at smoothed T>=72C; once running, stop only at smoothed T<65C. While running, duty is linear from min(77,cap) at72C to cap at88C, clamped to that floor/cap, including the hysteresis band below72C. A zero cap keeps its channel off except during emergency/fault recovery and clears that channel's normal running latch; re-enabling it below72C waits for the start threshold again. Short raw Package spikes are filtered for the normal curve, but **raw** T>105C or invalid state/missing telemetry immediately bypass all limits and slew to255, latched until the fault clears and raw T<100C. Channel write faults retain an independent high-duty latch until raw T<100C; one broken header never skips control/recovery of the other. Within normal running, limit rises to10 and falls to2 PWM/s, carrying fractional slew credit between samples. Starting jumps immediately to the floor (never traverses the stall range); stopping jumps to0; a lowered hard cap, including0 or emergency exit to a low cap, clamps immediately before downward slew. These explicit precedence rules preserve the hard cap and avoid stalled intermediate duties. Additional D-Bus limit requests do not count as extra seconds. Boot/non-desktop remains the legacy raw-temperature 80% policy (70->88C ramp, no desktop EMA/slew/hysteresis), without overwriting saved desktop limits. All paths retain one sysfs store per integer and manual enable=1 before duty; exit/crash failsafe remains255. Persistence is root-owned helper state; no UI-writable file bypasses polkit. Rediscovery keeps EMA/output history for unchanged topology; changed topology resets history but never clears a safety latch before a healthy raw<100C reading. No added timer beyond the existing 1Hz safety tick.

Replace fan CLI by a polkit/systemd-activated helper with narrowly typed GetFans/SetFanLimit, read-only chip discovery and channel allowlist. Root must never accept caller sysfs paths or arbitrary commands. State-change signal FanStateChanged (new HyprV interface, not existing API); observe udev `hwmon`, logind SessionNew/Removed and session properties, PrepareForSleep. Keep control sampling independent of UI subscription: most hwmon temperature/RPM files have no reliable change notification. One consolidated 1-Hz safety timer only while controller exists; no re-discovery glob every tick, rediscover on hotplug. Helper startup and crash policy must retain cooling independently of compositor lifecycle. This is an explicit necessary exception to global zero-idle-wakeup wording.

Units/configs: `fan-control.modules.conf:1` loads nct6775; `fan-control-permissions.service:1` root oneshot `/usr/local/libexec/hyprv-fanctl grant`, RemainAfterExit, grants wilf ownership/0600 only pwm1/7 enable/value; early drop-in removes default deps and orders before sysinit/shutdown. `fan-control-boot.service:1` requires permissions, before display-manager/sysinit, User=wilf, boot then run-system, Restart=always/1s; `fan-control.service:1` user run, restart 2s. `install-fan-control-boot.sh:1` is root deployment copying helper/unit/module/drop-in and enabling boot ownership; do not run it for specification. Preserve boot control ownership and remove competing controller only in a separately authorized deployment.

`cpu-power-sampler.service:1` root simple service starts `/usr/local/libexec/hyprv-cpu-power-sampler`, runtime dir hyprv, restart failure/2s; script invokes turbostat `--quiet --interval 1 --num_iterations 2 --show PkgWatt`, extracts last decimal and atomically publishes `/run/hyprv/cpu-package-power-w` mode0644. Not exactly a guaranteed 1-Hz publisher: each invocation spans measurement intervals and failure sleeps 1s. Replace with privileged/native RAPL powercap energy_uj/max_energy_range_uj (wrap-aware delta joules/time) or permitted perf/MSR access; read only during power subscribers, watch parent rename if retaining old publisher temporarily. `/proc/stat`, meminfo, network byte counters, thermal telemetry and fan RPM cannot generally be pushed through inotify; do not pretend udev emits every new sample.

System stats actor mirrors ksystemstats: subscriber leases carry requested metric set and period; use one earliest deadline for all current leases, zero sampling timers with none. Visible bar requests aggregate CPU/memory/net 1s; expanded charts add per-core/temp/package power; bounded ring history, discard baseline on counter reset/interface change/suspend. Routes/link membership use rtnetlink multicast, counters use requested snapshots; temperature discovery via udev thermal/hwmon and label preference. Kernel fan safety timer remains separate. A visible updating clock or metric necessarily creates wakeups: zero idle means no *unneeded* activity with no timed presentation/subscriber.

## VM, target switching, serial IR and eGPU

VM `windows-vm-control.sh:4`: exact system unit `windows-autoshop.service`; status `systemctl is-active --quiet`; start `pkexec /usr/bin/systemctl start UNIT`, hibernate `pkexec /usr/bin/systemctl stop UNIT`. Thus “hibernate” means service stop, not a generic libvirt suspend/save assumption. Replacement system `org.freedesktop.systemd1` manager `/org/freedesktop/systemd1` Subscribe, GetUnit/LoadUnit, UnitNew/Removed, JobRemoved; per-unit PropertiesChanged ActiveState/SubState/Job. StartUnit/StopUnit with mode replace, systemd polkit interactive auth. Unmap overlay/release exclusive keyboard **before** auth, retain action actor until JobRemoved and unit observation. Cancellation does not report running; external unit changes update immediately. `WindowsVmController.qml:19`, `ControlPanelPopup.qml:39` and tests enforce lifetime/input boundaries.

Deskflow `deskflow-target.sh:1`: status inferred from right-edge target in `$XDG_CONFIG_HOME/Deskflow/deskflow-server.conf`; set windows|macos rewrites mode0600 config for WindowsVM/linhaikuodeMac-mini.local/SuperPower, portal 38..71 when DP-4 transform1/3 else28..87. Finds core through service cgroup then unique verified deskflow-core server process; sends SIGHUP and waits at most20×100ms for appended “reloaded configuration” log, rollback on failed acknowledgement; absent service-managed core permits config-next-start. `reflow` regenerates ranges without switching IR. `display-switch TARGET` sends input1/windows or input2/macos; set queues separate transient user unit `hyprv-display-switch-TIMESTAMP` so slow IR does not block button. Notification on IR failure. Replacement inotify config+transform+log parent dirs, systemd unit/cgroup state, pidfd verification+SIGHUP, readiness-driven appended-log acknowledgement with one 2-second timeout. Serial IR worker remains independently scheduled from successful input routing. Same-user signals/config writes only; no root required beyond serial device access.

Viewflow wrapper `scripts/viewflow-target.sh:1` delegates **outside tree** to `/home/wilf/data/viewflow/tools/viewflow-target.sh:1`. Read that file for actual contract: status prefers `~/.local/state/hyprv/hdmi-target`, then active macos/windows/desktop units, then viewflow-target/default windows. Set locks `$XDG_CONFIG_HOME/viewflow/target-switch.lock`, sends IR first, atomically stores hdmi-target; active `viewflow-display-topology.service` owns subsequent reconciliation. Without supervisor discovers peer using desktop_peer_discovery.py with windows-input-source.json/macos-input-source.json (windows adds --start-windows); only then stops previous route and restarts requested viewflow-*-input.service. Wait up to100×200ms for vf-cursor-peer PID/exe, `$XDG_RUNTIME_DIR/viewflow/hyprland.sock`, cursor-ready PID, desktop-drag/config remote/pointer.devices/server_name match; failed readiness restores old input unit but **does not undo already switched HDMI**. Enable new/disable old unit and atomically store viewflow-target after ready. Replacement observes these exact files/socket and user systemd states; direct peer-discovery protocol must be ported/verified, not executed. Current delegated discovery's network transport is not defined in the wrapper; keep an explicit integration boundary. Use event-triggered readiness checks and one20s deadline. Publish HDMI selector, input route, peer readiness and error separately.

IR `hdmi-switch-ir.py:19`: MicroPython/Pico `/dev/serial/by-id` patterns, exactly one or explicit --device, raw115200 baud, newline UTF-8 commands / JSON reply records. Database `~/.local/share/hyprv-ir/hdmi-switch.json`: schema1, carrier_hz38000, duty_cycle0.33, buttons map; each button captured_at ISO time, source_pin integer, polarity, durations_us integer array, decoded NEC object or null. Commands `status`(2s), `scan MILLISECONDS`(seconds+3), `pin N`(2s), `capture 10000`(13s, capture/capture_timeout), `transmit HZ DUTY_X10000 CSV_DURATIONS`(5s). CLI list/show no device access; learn NAME [--force], scan [--seconds5], set-pin PIN, send NAME, --data override. Device errors terminate; command reply is event-specific JSON (not fabricated fixed schema). Port tokio-serial readiness+one deadline, udev tty hotplug, exclusive queue per device; never poll serial select every250ms. Serial group/udev ACL needed, no global root.

eGPU `egpu-disconnect.sh:1`: one-shot reads PCI class0x030000/0x030200, selects first candidate with Thunderbolt ancestry/label **or heuristic bus>=3**, removes slot functions descending then non-root parent PCI bridges, `pkexec sh -c "echo 1 > '/sys/bus/pci/devices/BDF/remove'"`; output `device=BDF`, `status=removed|error`, or `status=not_found`. Panel has a two-step eGPU confirmation function (`ControlPanelPopup.qml:408-419`, 3000-ms arm window), but **no UI element calls it** in the current QML <!-- audit: was presented as an active panel flow -->. Replacement udev pci/thunderbolt inventory + authenticated helper RemoveExternalGpu(validated identity). Revalidate external topology and present affected chain; do not perpetuate bus-number heuristic as proof of external hardware. Root sysfs writes, action results must distinguish each removed/failed device; disappearance from udev confirms operation. No idle polling.

## Demo, recording, sleep inhibition, theme and launch

`prevent-sleep.sh:6`: status/enable/disable/toggle, newline `{enabled=true|false,pid=digits|empty}`; validates stored PID command identity and clears stale PID. Enable launches systemd-inhibit --what=idle:sleep --mode=block --why="HyprV prevent sleep" sh -lc 'exec sleep infinity'; disable signals it. Replacement logind system bus Manager.Inhibit("idle:sleep","HyprV","HyprV prevent sleep","block") retains returned Unix FD; close releases. Subscribe `PrepareForSleep(bool)`; rebaseline samples/resync providers on resume. Never equate a stale PID file with a held inhibitor. Polkit policy may authorize block locks. [logind API](https://freedesktop.org/software/systemd/man/latest/org.freedesktop.login1.html).

`demo-mode.sh:5`: status/enable/disable/toggle scoped by HYPRLAND_INSTANCE_SIGNATURE; state directory active/session/source/headless/created-headless/prevent-sleep-was-enabled, status keys enabled bool literal/source/headless strings. Enable selects external monitor, chooses portrait1080x1920@60 or1920x1080@60 transform1, creates/reuses headless mirror, resets touchscreen transform, places external beside internal and acquires prevent-sleep if absent. Disable/rollback reloads Hyprland config, removes only created headless and releases only acquired inhibitor, clears state; preserve failure cleanup ordering. No recurring own timer; panel status1500ms. Port to transactional Hyprland native request IPC (monitor eval/output create/remove/reload), socket2 monitoradded[v2]/monitorremoved/configreloaded, explicit inhibitor ownership and rollback journal; no external hyprctl. Record compositor-specific command support as negotiated capability.

Recording status uses `pgrep -f '(^|/)(wf-recorder|wl-screenrec|gpu-screen-recorder)( |$)'`, no recorder start/stop in control-panel helper. Main panel recording tile action is in source inventory; maintain its delegated recording command separately from detection. Prefer shell-managed recorder activation through user systemd and Job/Unit signals, portal ScreenCast session Closed for portal-managed sessions. Existing externally started recorders offer no universal lifecycle bus signal: optional privileged proc-connector is not guaranteed; pidfd tracks known processes only. Preserve foreign-process detection via a subscriber-only bounded /proc scan (1.5s panel-visible); no idle scanner, label this unavoidable fallback. UI action to start/stop recorder must use a defined native or service protocol.

`ui-state.sh:5`: persistent theme light|dark and variant v1|v2|v3|bt; ensure initializes from legacy waybar symlink suffix/config, default light/v2. Commands ensure/print/get-theme/get-variant/set-theme/set-variant/toggle-theme; output print is two lines, toggle prints new theme. Existing write is direct truncate; Rust should atomic replace while maintaining parser compatibility. `toggle-theme.sh`, `set-variant.sh` delegate apply-ui-state; launch/reload deliberately **do not apply desktop theme** to preserve manual application settings.

`apply-ui-state.sh:484` is a desktop-wide action, not just bar color. Repoints wofi/rofi/alacritty/ghostty/kitty/swaync config/theme symlinks, notifies/reloads apps; sets GTK2/3/4 settings, XFConf, GSettings, Qt5/6ct, Kvantum, kdeglobals, konsole profiles, XSettings, cursor and activation environment, awww wallpaper (wipe2s/180fps). GTK theme Orchis-Light/Dark-Compact; icons Fluent-light/dark, cursor Adwaita24; Qt simple/darker.conf, KDE Orchis/OrchisDark. Sends Ghostty Ctrl+Shift+comma per client, Kitty socket set-colors + SIGUSR1 fallback, SwayNC CSS reload, Konsole Session.setProfile/Window.setDefaultProfile. WeChat compatibility automates settings/popups via X11 tools, temporarily moves/focuses windows and restores them; bounded waits/sleeps are action-time only. Exact file/command lines in inventory are migration requirements, not authorization to run these now.

Replacement theme actor: parent-dir inotify; atomic symlinks/INI/assignment edits; `gio` GSettings/XSettings native library or dconf interface, XFConf D-Bus, org.freedesktop.DBus.UpdateActivationEnvironment and systemd Manager.SetEnvironment, Hyprland cursor/socket actions, direct Kitty remote-control socket, native Konsole bus, notifications Notify; wallpaper daemon socket protocol must be ported. WeChat UI automation needs native X11 (`x11rb`) and compositor events; no verified appearance D-Bus exists in source. External plasma-apply-colorscheme/xsettingsd lifecycle and app shortcuts must be native protocol or activated units; document unsupported combinations visibly. Theme operation is user-owned and can fail per app; keep shell state while reporting partial application. Never periodically retry theme automation.

Launch/reload: `launch.sh:6` exports QSG_RENDER_LOOP=${QSG_RENDER_LOOP:-basic}, ensures state, restores fan, execs quickshell -p CONFIG -n -d; reload kills same config, waits0.2s and starts -d. Rust startup imports state but does not reapply theme, retains helper cooling ownership. These are process-lifecycle maintenance actions, no timer port needed. `active-window-right-edge.sh:7` one-shot activewindow/monitors JSON gives round(active.at.x−monitor.x+active.size.x), no output/nonzero if invalid. Replace Hyprland cached geometry refreshed from socket2 activewindowv2/movewindowv2/monitor changes and targeted request as necessary.

## Calendar and separate hypr-comp shell

Calendar `CalendarBridge.qml:8`: dcal default `~/.local/bin/dcal`; discovers with `--json socket` expecting `{path:string}`, 1800-ms discovery retry. Two Unix sockets carry newline JSON `{id:number,method:string,params:object}`; response `{id,result}` or `{id,error}`; separate subscribe method topics accounts/calendars/events/sync emits `{event:string,...}`. `calendars.list` result array; `events.list` params ISO UTC from weekStart−7d through+14d, limit2000; result `{events:array}`. UI invokes `ui.openEvent {uid,start,editorOnly:true}`, `ui.newEvent {editorOnly:true}`, `ui.show {view:"week"}`. Selected week Monday/local midnight; stale request serial cannot overwrite current week. `WeekCalendarPanel.qml` consumes event/calendar fields enumerated below. Requests and subscription both reconnect with base min(400×2^attempt,12000)ms+jitter<base/4; refresh debounce350ms. Disconnect clears pending callbacks; replacement must resolve their futures with errors and loading=false, not leak them.

Replacement `tokio::net::UnixStream` with serde_json lines and bounded frames/IDs/timeouts; retain two channels and subscription contract. dcal's socket path is discovered by CLI in current source; **a static path is not established here**. Need native dcal discovery/activation contract (configuration/runtime path file or owned service socket) before replacing --json socket; use configured path+parent inotify, not guessed location or a spawned binary. Calendar UI requests can remain native service RPC; no shell app execution.

hypr-comp is a separate config (`hypr-comp/shell.qml:1`) with its own `hyprComp` IPC; audit includes it because scripts live below source tree. Weather is here, not in main shell. `hypr-comp/scripts/weather.sh` delegates calendar/weather.sh. Fetcher reads sibling `.env` OPENWEATHER_KEY/OPENWEATHER_CITY_ID/OPENWEATHER_UNIT, stores weather JSON plus today/tomorrow/view caches; --json stale-while-revalidate900s, missing/invalid key3600s, retains good cache on errors. `moon.qml:116` polls current-icon/temp/hex and --json every150000ms, date/time1000ms. Exact payload `{current_temp:string,current_icon:string,current_hex:string,forecast:[{id:string,day:string,day_full:string,date:string,max:string,min:string,feels_like:string,wind:string,humidity:string,pop:string,icon:string,hex:string,desc:string,hourly:[{time:string,temp:string,icon:string,hex:string}]}]}`. Values are **strings**, temperatures one decimal in selected units, humidity/pop percentages, wind selected OpenWeather units. No key generates 5 dummy forecast days; --nav next|prev bounds0..4 and writes view, --view-listener tail-F; --icon/temp/hex and --current-* scalar modes.

Weather cannot be fully event-driven without a provider push API (none in source). reqwest HTTPS port to OpenWeather `/data/2.5/forecast` and `/weather`, `.env` inotify, one refresh deadline only while weather subscribed and data expired; 900s successful cache /3600s pending-key, offline backoff, preserve old cache. Never create a 150s UI read timer; presentation reads reactive state. Suspend pauses timers; resume checks freshness once. Preserve local/UTC grouping deliberately rather than silently fixing forecast-day behavior during parity work.

hypr-comp watchers: audio_fetch emits string-valued JSON volume/icon/is_muted, --toggle wpctl/pamixer; audio_wait pactl subscribe exits on sink/server. battery_fetch string percent/status/icon; battery_wait udev power_supply timeout **10 seconds** despite comment30. bt_fetch string status/icon/connected; --toggle bluetoothctl, bt_wait two PropertiesChanged monitors. kb_fetch two-uppercase-letter active keymap, kb_wait socket2 activelayout +50ms; network_fetch string status/ssid/icon/eth_status, --toggle nmcli; network_wait nmcli monitor exits on selected transitions. sys_fetcher reads two CPU/network snapshots0.5s apart and emits pipe-delimited `CPU|RAM_PCT|RAM_GB|TEMP|RX_RATE|TX_RATE` (integer CPU/RAM percent, used GiB decimal, Celsius, bytes/s). music_info emits JSON and moon's music waiter exits on MPRIS PropertiesChanged or Seeked. Replace each using shared actors above, no FIFO or monitor-child spawning. Exact script code/field declarations are in inventory.

hypr-comp workspaces.sh starts singleton producer, listens Hyprland socket2, atomically updates workspaces.json consumed by moon file read +inotifywait. Replace with hyprland actor. `Scaler.qml:21` reads settings.json then uses inotifywait or sleep86400 if missing; `MatugenColors.qml:34` cats `/tmp/qs_colors.json` every1000ms; replace both by parent-directory inotify and native parsing. `applauncher/app_fetcher.py` indexes XDG .desktop directories; QML Process loads it, uses desktop Exec activation. Port `freedesktop-desktop-entry`/`gio`, native app activation and inotify desktop directories; exact app schema in inventory. caching.sh creates cache/state/runtime module directories and exports QS_* variables; launch/stop/qs_manager operate only separate shell, numeric qs_manager args switch/move workspace and close launcher. Native IPC replaces process control and workspace dispatch.

## IPC entry points and callers

CLI migration: `hyprv-shell ipc call TARGET FUNCTION [ARGS...]` over `$XDG_RUNTIME_DIR/hyprv/shell.sock`; use typed argument validation, return values and nonzero error status. Existing quickshell `-p` selects config identity; if maintaining wrappers, normalize both observed option orders.

| Target | Functions / semantics | Source and observed callers |
|---|---|---|
| bar | toggle(monitor:string) no-op in current standard desktop bar; layoutStatus(monitor) JSON or null; inputStatus() JSON or null; dataStatus() JSON diagnostics | `shell.qml:44`; no production bind caller found for these diagnostic functions in hypr Lua; tests/manual compatibility API. Exact JSON fields in inventory. |
| controlPanel | toggle() | `shell.qml:3403`; `HyprV/hypr/hyprland-bind.lua:54` SUPER+A uses quickshell ipc call -p ... controlPanel toggle. |
| calendar | toggle() on primary screen | `shell.qml:3412`; no current hypr/*.lua calendar caller found; retain public IPC. |
| quickAdjust | showBrightness(), showBrightnessLevel(level:int), showBrightnessIncrease(), showBrightnessDecrease(), showVolume() | `shell.qml:3423`; `HyprV/hypr/controls.lua:50` builds commands dynamically (exact invoked names in inventory); carries level after native brightness changes. |
| hyprComp (separate config) | openLauncher(), closeLauncher(), toggleLauncher() | `hypr-comp/shell.qml:13`; `hypr-comp/scripts/qs_manager.sh:10` invokes these; numeric arg workspace [move] first closes. |

`HyprV/hypr/hyprland-exec-once.lua:14` launches main; bind.lua:34 kills main on SUPER+Y, :50 reloads then fcitx5. These lifecycle calls are not IPC methods. No undocumented target should be added based only on an old binding name.

## Files, environment, and proof boundaries

The inventory below records literal paths/env references from every source; interpolate HOME/XDG values at runtime. Main config path is hardcoded HOME/.config/HyprV, not universally XDG_CONFIG_HOME. Cover `/proc/{stat,meminfo,net/{dev,route,wireless},PID/{exe,cmdline,environ,stat}}`, `/sys/class/{hwmon,thermal,power_supply,backlight}`, PCI/thunderbolt sysfs, cgroup process lists, RAPL files; runtime sockets and state/config/cache/theme directories described above. Watch parent directories and rearm after moves/deletion; inotify on an old inode misses atomic replacements. Runtime files mode0600 except public CPU watts0644; reject unsafe ownership/symlinks for privileged persistence and IPC. Do not emit stored bearer tokens/secrets in diagnostics.

Environment families: HOME, PATH/locale/TZ, XDG_CONFIG_HOME/STATE_HOME/CACHE_HOME/DATA_HOME/DATA_DIRS/RUNTIME_DIR, HYPRLAND_INSTANCE_SIGNATURE, WAYLAND_DISPLAY/DISPLAY/session bus address; QSG_RENDER_LOOP and Qt/toolkit vars remain legacy only; HYPRV_STARTUP_APPROVAL, HYPRV_AGENT_ISLAND_DIR, HYPRV_CONFIG_DIR, CODEX_HOME and agent host window/session environment, HYPRV_CIDER_API_BASE and token vars, DESKFLOW_LOG_FILE, VIEWFLOW_HDMI_HELPER, QS_CACHE_DIR/STATE_DIR/RUN_DIR and derived module vars, TEMP. Exact source spellings (including less common overrides) appear below. UI fonts/icons/assets are file dependencies as well, although pixel specs own their selection.

Agent stack: agent_island.py owns locked state.json, atomic replace, responses/, sessions/pending_requests, process/transcript discovery, cleanup20s idle/completed or900s others and pending86400s; public_snapshot hides idle, sorts priority/activity, computes running synthetic progress capped0.94 on12s phase. desktop-state sends only id/source/source_label/title/project/status/current_tool/detail/progress; full snapshot includes version/active/count/sessions/pending/pending_count/updated_at. Hooks accept --source and --event plus stdin JSON, always observation-only; hook installer edits Claude/Codex configurations and produces backups, never a runtime action. agent-island-status supports --json/--watch, sleeps0.65/1.2s; watch.py directory GIO watch150ms debounce plus5s fallback/stdin trigger; codex-appserver.py spawns app-server --listen stdio://, initializes JSON-RPC then handles thread/started, thread/status/changed, thread/closed, reconnect4s. Source agent action **only focus ID**; no approvals, questions, keyboard input or response submission (`tests/test_agent_readonly.py`). Keep that boundary even though persisted legacy pending data exists.

Replacement agent actor consumes hook socket messages/native file watch, transcript append notifications and verified process pidfds; focus via native Hyprland request IPC plus terminal/window association. An existing external app-server must expose an attachable socket or a separately activated adapter to avoid launching codex inside shell; source does not prove an attachable endpoint. Process discovery for unhooked agents is optional subscriber-scoped scan with1.5s lower bound; pidfd only observes exits for already known PIDs. Remove artificial progress service ticks: UI samples phase only during visible animation. Expiry uses one next-expiry deadline, no inactive heartbeat. Hook installation remains explicit maintenance, not startup side effect.

Static regression sources: `tests/test_fan_control.py` isolates sysfs/state/control law; `test_windows_vm.py` and windows-vm-lifecycle.qml cover action lifecycle and input release; `test_desktop_state.py` covers stream shape and ownership; `test_agent_readonly.py`, tst_AgentTaskModel.qml cover read-only display/focus. Fixture windows-vm-control.sh is synthetic and must never be treated as live VM behavior. This documentation change does not execute any service or device tests. Integration verification still required for SwayNC introspection, dcal discovery, Viewflow peer discovery, Cider push absence/adapter, wallpaper/XSettings/WeChat activation, hardware DDC/RAPL and actual polkit flows.

## Additional exact adapter contracts

Calendar wire fields actually accessed by WeekCalendarPanel are event `uid`, `calendarId`, `summary`, `start`, `end`, `allDay`; calendar `id`, `summary`/`name`, `color`, `hidden`, `holdsTasks` (`WeekCalendarPanel.qml:99`, :111, :131, :505). Hidden/task calendars are filtered; calendar color wins else accent. The internal title/calendar_id model maps from summary/calendarId, not invented wire names. Date-only all-day end is exclusive and layout clamps spans to the viewed week.

Bluetooth fallback exact JSON (`scripts/bluetooth-status.sh:74`) is `{present:bool,enabled:bool,discovering:bool,pairable:bool,devices:[{address:string,name:string,icon:string,paired:bool,trusted:bool,connected:bool,blocked:bool,rssi:integer|null}]}`; RSSI is dBm. Sort connected, paired, trusted first then strongest RSSI then name. `bluetooth-action.sh:65` standalone commands are toggle/scan/connect/disconnect/remove; unpaired connect registers KeyboardDisplay agent, default-agent, pair/trust/connect with35s bluetoothctl timeout (+2s outer bound), while native shell action timeout is12s. These are distinct code paths, not one shared duration.

Cider uses HYPRV_CIDER_APP_TOKEN/CIDER_APP_TOKEN or ~/.config/hyprv/cider-app-token and ~/.config/HyprV/.cider-app-token, `apptoken` header, `/api/v2/playback/now-playing` then `/api/v1/playback/now-playing` on HYPRV_CIDER_API_BASE default http://127.0.0.1:10767 (`scripts/cider-metadata-fallback.py:227`). Keep token resolution private. OpenWeather .env is hypr-comp/scripts/quickshell/calendar/.env; OPENWEATHER_UNIT defaults metric, supports imperial/standard.

Recording start is exact `~/.config/hypr/scripts/record-script.sh --fullscreen-sound`; stop is `sh -c 'pkill -INT wf-recorder || pkill -INT wl-screenrec || pkill -INT gpu-screen-recorder'` (`ControlPanelPopup.qml:223`). Direct dependency `/home/wilf/.config/hypr/scripts/record-script.sh:1` toggles if wf-recorder exists; otherwise uses focused monitor from hyprctl, Pulse monitor audio source from pactl, XDG VIDEOS directory, filename recording_YYYY-MM-DD_HH.MM.SS.mp4, wf-recorder -o MONITOR --pixel-format yuv420p -f FILE -t --audio=SOURCE. Other script modes --sound/--fullscreen/default use region slurp and/or omit audio. Port native Wayland screen capture plus PipeWire audio and encoding (`ffmpeg-next` linked library/GStreamer Rust bindings) or activated recorder service with equivalent args; SIGINT known recorder pidfds for finalization. Same-user signals/device session access; no root. A portal alternative needs user consent and may not preserve silent fullscreen parity, so do not assume it is interchangeable.

hypr-comp music JSON uses **strings** for title/artist/status/length/position/lengthStr/positionStr/timeStr/percent/source/playerName/blur/grad/textColor/deviceIcon/deviceName/artUrl; stopped output omits numeric-text length/position and uses cached formatted timing (`hypr-comp/scripts/quickshell/music/music_info.sh:174`). Cover generation downloads/copies artwork, makes blur0x20/brightness−30/contrast−10, quantizes3 colors at50×50 for gradient and complementary text color, retains20 newest cache entries. Replace ImageMagick/curl by native image decoding/filter/quantization and HTTP, publish completion as one track-keyed change; retain placeholders and paused saved-position behavior. workspaces.json array entries are `{id:integer,state:"active"|"occupied"|"empty",tooltip:string}` for1..workspaceCount(default8),50ms event debounce and1s disconnected reconnect (`hypr-comp/scripts/workspaces.sh:46`). Existing producer also stops Bluetooth scan and kills older self processes during startup; Rust actor ownership removes these cross-domain cleanup side effects.

Validation: all69 current script/config text files (including historical backups) have inventory entries;20 QML I/O/action owners are indexed. Captured SHA-256 hashes matched source on recheck; Markdown fences balanced. The combined illustrative Rust type blocks in services-api.md compiled successfully with rustc edition2021 as a library. Existing application/device tests were inspected, not executed. Only services.md and services-api.md were authored for this task.

App launcher wire output is an alphabetically sorted JSON array of `{name:string,exec:string,icon:string}`, deduplicated by name, skipping NoDisplay entries; Exec text is truncated before ` %` or ` @@` (`hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:24`). It scans system/local/Flatpak/Nix .desktop trees listed in inventory. Native activation must retain launcher selection while parsing desktop field codes correctly; this audit does not establish an external arbitrary-command execution exemption.

## Source evidence inventory

The following inventory is intentionally line-addressable: exact source declarations supplement the normalized contracts above. Each subsection includes all non-comment action/schema/path/scheduling evidence selected from that file, and a per-file migration assignment. It is not permission to run the commands. Line ranges are anchored at each displayed source line, so future edits can be detected with the SHA-256. Script libraries with no direct QML output are identified as shared helpers rather than assigned fictitious wire formats.

### `hypr-comp/scripts/caching.sh`

Migration owner: `config/paths`. Source `hypr-comp/scripts/caching.sh:1`, 34 lines, SHA-256 `ad7381f5df423f878284e6e679515a86edc0046321c899246dd127121459068f`.

```text
hypr-comp/scripts/caching.sh:1: #!/usr/bin/env bash
hypr-comp/scripts/caching.sh:2: export QS_CACHE_DIR="${QS_CACHE_DIR:-$HOME/.cache/hyprv/hypr-comp}"
hypr-comp/scripts/caching.sh:3: export QS_STATE_DIR="${QS_STATE_DIR:-$HOME/.local/state/hyprv/hypr-comp}"
hypr-comp/scripts/caching.sh:4: export QS_RUN_DIR="${QS_RUN_DIR:-${XDG_RUNTIME_DIR:-/tmp}/hyprv-hypr-comp}"
hypr-comp/scripts/caching.sh:5: export QS_LOG_DIR="$QS_RUN_DIR/logs"
hypr-comp/scripts/caching.sh:6: 
hypr-comp/scripts/caching.sh:7: mkdir -p "$QS_CACHE_DIR" "$QS_STATE_DIR" "$QS_RUN_DIR" "$QS_LOG_DIR"
hypr-comp/scripts/caching.sh:8: SCRIPT_DIR="$(dirname "$(realpath "${BASH_SOURCE[0]}")")"
hypr-comp/scripts/caching.sh:9: QS_DIR="$SCRIPT_DIR/quickshell"
hypr-comp/scripts/caching.sh:10: 
hypr-comp/scripts/caching.sh:11: # Function to dynamically create and export cache directories for ANY module by request
hypr-comp/scripts/caching.sh:12: qs_ensure_cache() {
hypr-comp/scripts/caching.sh:13:     local WIDGET_NAME="$1"
hypr-comp/scripts/caching.sh:14:     local WIDGET_UPPER=$(echo "$WIDGET_NAME" | tr '[:lower:]' '[:upper:]')
hypr-comp/scripts/caching.sh:15:     
hypr-comp/scripts/caching.sh:16:     local WIDGET_CACHE="$QS_CACHE_DIR/$WIDGET_NAME"
hypr-comp/scripts/caching.sh:17:     local WIDGET_STATE="$QS_STATE_DIR/$WIDGET_NAME"
hypr-comp/scripts/caching.sh:18:     local WIDGET_RUN="$QS_RUN_DIR/$WIDGET_NAME"
hypr-comp/scripts/caching.sh:19:     
hypr-comp/scripts/caching.sh:20:     mkdir -p "$WIDGET_CACHE" "$WIDGET_STATE" "$WIDGET_RUN"
hypr-comp/scripts/caching.sh:21:     
hypr-comp/scripts/caching.sh:22:     export "QS_CACHE_${WIDGET_UPPER}=$WIDGET_CACHE"
hypr-comp/scripts/caching.sh:23:     export "QS_STATE_${WIDGET_UPPER}=$WIDGET_STATE"
hypr-comp/scripts/caching.sh:24:     export "QS_RUN_${WIDGET_UPPER}=$WIDGET_RUN"
hypr-comp/scripts/caching.sh:25: }
hypr-comp/scripts/caching.sh:26: 
hypr-comp/scripts/caching.sh:27: # Pre-initialize for all existing QML widget folders in the main directory
hypr-comp/scripts/caching.sh:28: if [ -d "$QS_DIR" ]; then
hypr-comp/scripts/caching.sh:29:     for dir in "$QS_DIR"/*/; do
hypr-comp/scripts/caching.sh:30:         [ -d "$dir" ] || continue
hypr-comp/scripts/caching.sh:31:         WIDGET_NAME=$(basename "$dir")
hypr-comp/scripts/caching.sh:32:         qs_ensure_cache "$WIDGET_NAME"
hypr-comp/scripts/caching.sh:33:     done
hypr-comp/scripts/caching.sh:34: fi
```

### `hypr-comp/scripts/launch.sh`

Migration owner: `applications/lifecycle`. Source `hypr-comp/scripts/launch.sh:1`, 5 lines, SHA-256 `03706d13cac2c1fcff2b471843daddac36c3642ae4321ffd7e292d38772aac1c`.

```text
hypr-comp/scripts/launch.sh:1: #!/usr/bin/env bash
hypr-comp/scripts/launch.sh:2: set -euo pipefail
hypr-comp/scripts/launch.sh:3: 
hypr-comp/scripts/launch.sh:4: config_dir="$HOME/.config/HyprV/quickshell/hypr-comp"
hypr-comp/scripts/launch.sh:5: exec quickshell -p "$config_dir" -n -d
```

### `hypr-comp/scripts/qs_manager.sh`

Migration owner: `applications/hyprland`. Source `hypr-comp/scripts/qs_manager.sh:1`, 40 lines, SHA-256 `f89a6f6f47a65bf26eccada7a0a43bc0b5fed773590248491bcc035a37063100`.

```text
hypr-comp/scripts/qs_manager.sh:1: #!/usr/bin/env bash
hypr-comp/scripts/qs_manager.sh:2: set -euo pipefail
hypr-comp/scripts/qs_manager.sh:3: 
hypr-comp/scripts/qs_manager.sh:4: config_dir="$HOME/.config/HyprV/quickshell/hypr-comp"
hypr-comp/scripts/qs_manager.sh:5: action="${1:-toggle}"
hypr-comp/scripts/qs_manager.sh:6: target="${2:-applauncher}"
hypr-comp/scripts/qs_manager.sh:7: 
hypr-comp/scripts/qs_manager.sh:8: if [[ "$action" =~ ^[0-9]+$ ]]; then
hypr-comp/scripts/qs_manager.sh:9:     quickshell -p "$config_dir" ipc call hyprComp closeLauncher >/dev/null 2>&1 || true
hypr-comp/scripts/qs_manager.sh:10:     if [[ "$target" == "move" ]]; then
hypr-comp/scripts/qs_manager.sh:11:         hyprctl dispatch movetoworkspace "$action" >/dev/null 2>&1 || true
hypr-comp/scripts/qs_manager.sh:12:     else
hypr-comp/scripts/qs_manager.sh:13:         hyprctl dispatch workspace "$action" >/dev/null 2>&1 || true
hypr-comp/scripts/qs_manager.sh:14:     fi
hypr-comp/scripts/qs_manager.sh:15:     exit 0
hypr-comp/scripts/qs_manager.sh:16: fi
hypr-comp/scripts/qs_manager.sh:17: 
hypr-comp/scripts/qs_manager.sh:18: start_shell() {
hypr-comp/scripts/qs_manager.sh:19:     if ! quickshell list --all 2>/dev/null | grep -Fq "Config path: $config_dir/shell.qml"; then
hypr-comp/scripts/qs_manager.sh:20:         quickshell -p "$config_dir" -n -d >/dev/null 2>&1 || true
hypr-comp/scripts/qs_manager.sh:21:         sleep 0.2
hypr-comp/scripts/qs_manager.sh:22:     fi
hypr-comp/scripts/qs_manager.sh:23: }
hypr-comp/scripts/qs_manager.sh:24: 
hypr-comp/scripts/qs_manager.sh:25: case "$action:$target" in
hypr-comp/scripts/qs_manager.sh:26:     close:*|close:)
hypr-comp/scripts/qs_manager.sh:27:         quickshell -p "$config_dir" ipc call hyprComp closeLauncher >/dev/null 2>&1 || true
hypr-comp/scripts/qs_manager.sh:28:         ;;
hypr-comp/scripts/qs_manager.sh:29:     open:applauncher|open:)
hypr-comp/scripts/qs_manager.sh:30:         start_shell
hypr-comp/scripts/qs_manager.sh:31:         quickshell -p "$config_dir" ipc call hyprComp openLauncher >/dev/null 2>&1 || true
hypr-comp/scripts/qs_manager.sh:32:         ;;
hypr-comp/scripts/qs_manager.sh:33:     toggle:applauncher|toggle:)
hypr-comp/scripts/qs_manager.sh:34:         start_shell
hypr-comp/scripts/qs_manager.sh:35:         quickshell -p "$config_dir" ipc call hyprComp toggleLauncher >/dev/null 2>&1 || true
hypr-comp/scripts/qs_manager.sh:36:         ;;
hypr-comp/scripts/qs_manager.sh:37:     *)
hypr-comp/scripts/qs_manager.sh:38:         :
hypr-comp/scripts/qs_manager.sh:39:         ;;
hypr-comp/scripts/qs_manager.sh:40: esac
```

### `hypr-comp/scripts/quickshell/applauncher/app_fetcher.py`

Migration owner: `applications`. Source `hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:1`, 63 lines, SHA-256 `f92ee8ebdaffe59b1c50c2d369a1ace169f88df231ad6b8f480e8f9bc5723260`.

```text
…
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:5: 
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:6: def fetch_apps():
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:7:     apps = {}
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:8:     home = os.path.expanduser('~')
…
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:13:         '/usr/local/share/applications',
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:14:         f'{home}/.local/share/applications',
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:15:         '/var/lib/flatpak/exports/share/applications',
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:16:         f'{home}/.local/share/flatpak/exports/share/applications',
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:17:         f'{home}/.nix-profile/share/applications',
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:18:         '/run/current-system/sw/share/applications'
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:19:     ]
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:20:     
…
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:24:             
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:25:         for f in glob.glob(os.path.join(d, '**/*.desktop'), recursive=True):
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:26:             try:
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:27:                 with open(f, 'r', encoding='utf-8') as file:
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:28:                     app = {'name': '', 'exec': '', 'icon': ''}
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:29:                     is_desktop = False
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:30:                     no_display = False
…
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:35:                             is_desktop = True
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:36:                         elif line.startswith('['):
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:37:                             is_desktop = False
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:38:                             
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:39:                         if is_desktop:
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:40:                             if line.startswith('Name=') and not app['name']:
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:41:                                 app['name'] = line[5:]
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:42:                             elif line.startswith('Exec=') and not app['exec']:
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:43:                                 # Strip %u, %f, and @@ placeholders
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:44:                                 app['exec'] = line[5:].split(' %')[0].split(' @@')[0]
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:45:                             elif line.startswith('Icon=') and not app['icon']:
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:46:                                 app['icon'] = line[5:]
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:47:                             elif line.startswith('NoDisplay=true') or line.startswith('NoDisplay=1'):
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:48:                                 no_display = True
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:49:                                 
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:50:                     if app['name'] and app['exec'] and not no_display:
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:51:                         apps[app['name']] = app
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:52:             except Exception:
…
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:57:     res.sort(key=lambda x: x['name'].lower())
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:58:     print(json.dumps(res))
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:59: 
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:60: if __name__ == "__main__":
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:61:     fetch_apps()
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:62: 
hypr-comp/scripts/quickshell/applauncher/app_fetcher.py:63: 
```

### `hypr-comp/scripts/quickshell/calendar/weather.sh`

Migration owner: `weather`. Source `hypr-comp/scripts/quickshell/calendar/weather.sh:1`, 333 lines, SHA-256 `e23549d087d998b85485a9c9db196abc0eeb99796011fb6c18deee326d027ac0`.

```text
…
hypr-comp/scripts/quickshell/calendar/weather.sh:12: # Paths
hypr-comp/scripts/quickshell/calendar/weather.sh:13: cache_dir="$QS_CACHE_WEATHER"
hypr-comp/scripts/quickshell/calendar/weather.sh:14: json_file="${cache_dir}/weather.json"
hypr-comp/scripts/quickshell/calendar/weather.sh:15: view_file="${cache_dir}/view_id"
…
hypr-comp/scripts/quickshell/calendar/weather.sh:21: # Load environment variables silently
hypr-comp/scripts/quickshell/calendar/weather.sh:22: if [ -f "$ENV_FILE" ]; then
hypr-comp/scripts/quickshell/calendar/weather.sh:23:     export $(grep -v '^#' "$ENV_FILE" | xargs)
hypr-comp/scripts/quickshell/calendar/weather.sh:24: fi
hypr-comp/scripts/quickshell/calendar/weather.sh:25: 
hypr-comp/scripts/quickshell/calendar/weather.sh:26: # API Settings from .env
hypr-comp/scripts/quickshell/calendar/weather.sh:27: KEY="$OPENWEATHER_KEY"
hypr-comp/scripts/quickshell/calendar/weather.sh:28: ID="$OPENWEATHER_CITY_ID"
hypr-comp/scripts/quickshell/calendar/weather.sh:29: UNIT="${OPENWEATHER_UNIT:-metric}" # Default to metric if not set
hypr-comp/scripts/quickshell/calendar/weather.sh:30: 
hypr-comp/scripts/quickshell/calendar/weather.sh:31: # Determine temperature symbol based on unit
hypr-comp/scripts/quickshell/calendar/weather.sh:32: case "$UNIT" in
hypr-comp/scripts/quickshell/calendar/weather.sh:33:     "imperial") UNIT_SYM="°F" ;;
hypr-comp/scripts/quickshell/calendar/weather.sh:34:     "standard") UNIT_SYM="K" ;;
…
hypr-comp/scripts/quickshell/calendar/weather.sh:39: 
hypr-comp/scripts/quickshell/calendar/weather.sh:40: get_icon() {
hypr-comp/scripts/quickshell/calendar/weather.sh:41:     case $1 in
hypr-comp/scripts/quickshell/calendar/weather.sh:42:         "50d"|"50n") icon="󰖑"; quote="Mist" ;;
hypr-comp/scripts/quickshell/calendar/weather.sh:43:         "01d") icon=""; quote="Sunny" ;;
…
hypr-comp/scripts/quickshell/calendar/weather.sh:50:     esac
hypr-comp/scripts/quickshell/calendar/weather.sh:51:     echo "$icon|$quote"
hypr-comp/scripts/quickshell/calendar/weather.sh:52: }
hypr-comp/scripts/quickshell/calendar/weather.sh:53: 
hypr-comp/scripts/quickshell/calendar/weather.sh:54: get_hex() {
hypr-comp/scripts/quickshell/calendar/weather.sh:55:     case $1 in
hypr-comp/scripts/quickshell/calendar/weather.sh:56:         "50d"|"50n") echo "#84afdb" ;;
hypr-comp/scripts/quickshell/calendar/weather.sh:57:         "01d") echo "#f9e2af" ;;
hypr-comp/scripts/quickshell/calendar/weather.sh:58:         "01n") echo "#cba6f7" ;;
hypr-comp/scripts/quickshell/calendar/weather.sh:59:         "02d"|"02n"|"03d"|"03n"|"04d"|"04n") echo "#bac2de" ;;
hypr-comp/scripts/quickshell/calendar/weather.sh:60:         "09d"|"09n"|"10d"|"10n") echo "#74c7ec" ;;
hypr-comp/scripts/quickshell/calendar/weather.sh:61:         "11d"|"11n") echo "#f9e2af" ;;
hypr-comp/scripts/quickshell/calendar/weather.sh:62:         "13d"|"13n") echo "#cdd6f4" ;;
hypr-comp/scripts/quickshell/calendar/weather.sh:63:         *) echo "#cdd6f4" ;;
hypr-comp/scripts/quickshell/calendar/weather.sh:64:     esac
hypr-comp/scripts/quickshell/calendar/weather.sh:65: }
hypr-comp/scripts/quickshell/calendar/weather.sh:66: 
hypr-comp/scripts/quickshell/calendar/weather.sh:67: write_dummy_data() {
hypr-comp/scripts/quickshell/calendar/weather.sh:68:     final_json="["
hypr-comp/scripts/quickshell/calendar/weather.sh:69:     for i in {0..4}; do
hypr-comp/scripts/quickshell/calendar/weather.sh:70:         future_date=$(date -d "+$i days")
hypr-comp/scripts/quickshell/calendar/weather.sh:71:         f_day=$(date -d "$future_date" "+%a")
hypr-comp/scripts/quickshell/calendar/weather.sh:72:         f_full_day=$(date -d "$future_date" "+%A")
hypr-comp/scripts/quickshell/calendar/weather.sh:73:         f_date_num=$(date -d "$future_date" "+%d %b")
hypr-comp/scripts/quickshell/calendar/weather.sh:74:         
hypr-comp/scripts/quickshell/calendar/weather.sh:75:         final_json="${final_json} {
…
hypr-comp/scripts/quickshell/calendar/weather.sh:92:     final_json="${final_json%,}]"
hypr-comp/scripts/quickshell/calendar/weather.sh:93:     echo "{ \"current_temp\": \"0.0\", \"current_icon\": \"\", \"current_hex\": \"#cdd6f4\", \"forecast\": ${final_json} }" > "${json_file}"
hypr-comp/scripts/quickshell/calendar/weather.sh:94: }
hypr-comp/scripts/quickshell/calendar/weather.sh:95: 
hypr-comp/scripts/quickshell/calendar/weather.sh:96: get_data() {
hypr-comp/scripts/quickshell/calendar/weather.sh:97:     # ---------------------------------------------------------
hypr-comp/scripts/quickshell/calendar/weather.sh:98:     # DUMMY DATA FALLBACK (If API key is missing or skipped)
hypr-comp/scripts/quickshell/calendar/weather.sh:99:     # ---------------------------------------------------------
hypr-comp/scripts/quickshell/calendar/weather.sh:100:     if [[ -z "$KEY" || "$KEY" == "Skipped" || "$KEY" == "OPENWEATHER_KEY" ]]; then
hypr-comp/scripts/quickshell/calendar/weather.sh:101:         write_dummy_data
hypr-comp/scripts/quickshell/calendar/weather.sh:102:         return
…
hypr-comp/scripts/quickshell/calendar/weather.sh:107:     # ---------------------------------------------------------
hypr-comp/scripts/quickshell/calendar/weather.sh:108:     forecast_url="http://api.openweathermap.org/data/2.5/forecast?APPID=${KEY}&id=${ID}&units=${UNIT}"
hypr-comp/scripts/quickshell/calendar/weather.sh:109:     raw_api=$(curl -sf "$forecast_url")
hypr-comp/scripts/quickshell/calendar/weather.sh:110:     
hypr-comp/scripts/quickshell/calendar/weather.sh:111:     weather_url="http://api.openweathermap.org/data/2.5/weather?APPID=${KEY}&id=${ID}&units=${UNIT}"
hypr-comp/scripts/quickshell/calendar/weather.sh:112:     raw_weather=$(curl -sf "$weather_url")
hypr-comp/scripts/quickshell/calendar/weather.sh:113:     
hypr-comp/scripts/quickshell/calendar/weather.sh:114:     # Check if curl failed OR if OpenWeather returned an error
hypr-comp/scripts/quickshell/calendar/weather.sh:115:     api_cod=$(echo "$raw_api" | jq -r '.cod' 2>/dev/null)
hypr-comp/scripts/quickshell/calendar/weather.sh:116:     
hypr-comp/scripts/quickshell/calendar/weather.sh:117:     if [ -z "$raw_api" ] || [ -z "$raw_weather" ] || [[ "$api_cod" != "200" ]]; then
hypr-comp/scripts/quickshell/calendar/weather.sh:118:         # If curl failed (network glitch, rate limit, API downtime), don't destroy
hypr-comp/scripts/quickshell/calendar/weather.sh:119:         # the existing working cache. Just abort the update.
hypr-comp/scripts/quickshell/calendar/weather.sh:120:         # If there is NO cache at all, then fall back to dummy data.
hypr-comp/scripts/quickshell/calendar/weather.sh:121:         if [ ! -f "$json_file" ]; then
hypr-comp/scripts/quickshell/calendar/weather.sh:122:             write_dummy_data
hypr-comp/scripts/quickshell/calendar/weather.sh:123:         fi
…
hypr-comp/scripts/quickshell/calendar/weather.sh:127:     # Parse LIVE current weather conditions to bypass UTC boundary issues
hypr-comp/scripts/quickshell/calendar/weather.sh:128:     c_temp=$(echo "$raw_weather" | jq -r '.main.temp')
hypr-comp/scripts/quickshell/calendar/weather.sh:129:     c_temp=$(printf "%.1f" "$c_temp")
hypr-comp/scripts/quickshell/calendar/weather.sh:130:     c_code=$(echo "$raw_weather" | jq -r '.weather[0].icon')
hypr-comp/scripts/quickshell/calendar/weather.sh:131:     c_icon=$(get_icon "$c_code" | cut -d'|' -f1)
hypr-comp/scripts/quickshell/calendar/weather.sh:132:     c_hex=$(get_hex "$c_code")
hypr-comp/scripts/quickshell/calendar/weather.sh:133: 
hypr-comp/scripts/quickshell/calendar/weather.sh:134:     current_date=$(date +%Y-%m-%d)
…
hypr-comp/scripts/quickshell/calendar/weather.sh:137:     # 1. ROLLOVER CHECK
hypr-comp/scripts/quickshell/calendar/weather.sh:138:     if [ -f "$next_day_cache_file" ]; then
hypr-comp/scripts/quickshell/calendar/weather.sh:139:         precache_date=$(cat "$next_day_cache_file" | jq -r '.[0].dt_txt' | cut -d' ' -f1)
hypr-comp/scripts/quickshell/calendar/weather.sh:140:         if [ "$precache_date" == "$current_date" ]; then
hypr-comp/scripts/quickshell/calendar/weather.sh:141:             mv "$next_day_cache_file" "$daily_cache_file"
hypr-comp/scripts/quickshell/calendar/weather.sh:142:         fi
hypr-comp/scripts/quickshell/calendar/weather.sh:143:     fi
…
hypr-comp/scripts/quickshell/calendar/weather.sh:145:     # 2. PROCESS TODAY
hypr-comp/scripts/quickshell/calendar/weather.sh:146:     api_today_items=$(echo "$raw_api" | jq -c ".list[] | select(.dt_txt | startswith(\"$current_date\"))" | jq -s '.')
hypr-comp/scripts/quickshell/calendar/weather.sh:147: 
hypr-comp/scripts/quickshell/calendar/weather.sh:148:     if [ -f "$daily_cache_file" ]; then
hypr-comp/scripts/quickshell/calendar/weather.sh:149:         cached_date=$(cat "$daily_cache_file" | jq -r '.[0].dt_txt' | cut -d' ' -f1)
hypr-comp/scripts/quickshell/calendar/weather.sh:150:         if [ "$cached_date" == "$current_date" ]; then
hypr-comp/scripts/quickshell/calendar/weather.sh:151:             merged_today=$(echo "$api_today_items" | jq --slurpfile cache "$daily_cache_file" \
hypr-comp/scripts/quickshell/calendar/weather.sh:152:                 '($cache[0] + .) | unique_by(.dt) | sort_by(.dt)')
hypr-comp/scripts/quickshell/calendar/weather.sh:153:         else
hypr-comp/scripts/quickshell/calendar/weather.sh:154:             merged_today="$api_today_items"
hypr-comp/scripts/quickshell/calendar/weather.sh:155:         fi
hypr-comp/scripts/quickshell/calendar/weather.sh:156:     else
hypr-comp/scripts/quickshell/calendar/weather.sh:157:         merged_today="$api_today_items"
hypr-comp/scripts/quickshell/calendar/weather.sh:158:     fi
hypr-comp/scripts/quickshell/calendar/weather.sh:159: 
hypr-comp/scripts/quickshell/calendar/weather.sh:160:     echo "$merged_today" > "$daily_cache_file"
hypr-comp/scripts/quickshell/calendar/weather.sh:161: 
hypr-comp/scripts/quickshell/calendar/weather.sh:162:     # 3. PRE-CACHE TOMORROW
hypr-comp/scripts/quickshell/calendar/weather.sh:163:     api_tomorrow_items=$(echo "$raw_api" | jq -c ".list[] | select(.dt_txt | startswith(\"$tomorrow_date\"))" | jq -s '.')
hypr-comp/scripts/quickshell/calendar/weather.sh:164:     echo "$api_tomorrow_items" > "$next_day_cache_file"
hypr-comp/scripts/quickshell/calendar/weather.sh:165: 
hypr-comp/scripts/quickshell/calendar/weather.sh:166:     # 4. BUILD FINAL JSON
hypr-comp/scripts/quickshell/calendar/weather.sh:167:     processed_forecast=$(echo "$raw_api" | jq --argjson today "$merged_today" --arg date "$current_date" \
hypr-comp/scripts/quickshell/calendar/weather.sh:168:         '.list = ($today + [.list[] | select(.dt_txt | startswith($date) | not)])')
hypr-comp/scripts/quickshell/calendar/weather.sh:169: 
hypr-comp/scripts/quickshell/calendar/weather.sh:170:     if [ ! -z "$processed_forecast" ]; then
hypr-comp/scripts/quickshell/calendar/weather.sh:171:         dates=$(echo "$processed_forecast" | jq -r '.list[].dt_txt | split(" ")[0]' | uniq | head -n 5)
hypr-comp/scripts/quickshell/calendar/weather.sh:172:         
hypr-comp/scripts/quickshell/calendar/weather.sh:173:         final_json="["
…
hypr-comp/scripts/quickshell/calendar/weather.sh:175:         
hypr-comp/scripts/quickshell/calendar/weather.sh:176:         for d in $dates; do
hypr-comp/scripts/quickshell/calendar/weather.sh:177:             day_data=$(echo "$processed_forecast" | jq "[.list[] | select(.dt_txt | startswith(\"$d\"))]")
hypr-comp/scripts/quickshell/calendar/weather.sh:178: 
hypr-comp/scripts/quickshell/calendar/weather.sh:179:             raw_max=$(echo "$day_data" | jq '[.[].main.temp_max] | max')
hypr-comp/scripts/quickshell/calendar/weather.sh:180:             f_max_temp=$(printf "%.1f" "$raw_max")
hypr-comp/scripts/quickshell/calendar/weather.sh:181: 
hypr-comp/scripts/quickshell/calendar/weather.sh:182:             raw_min=$(echo "$day_data" | jq '[.[].main.temp_min] | min')
hypr-comp/scripts/quickshell/calendar/weather.sh:183:             f_min_temp=$(printf "%.1f" "$raw_min")
hypr-comp/scripts/quickshell/calendar/weather.sh:184: 
hypr-comp/scripts/quickshell/calendar/weather.sh:185:             raw_feels=$(echo "$day_data" | jq '[.[].main.feels_like] | max')
hypr-comp/scripts/quickshell/calendar/weather.sh:186:             f_feels_like=$(printf "%.1f" "$raw_feels")
hypr-comp/scripts/quickshell/calendar/weather.sh:187: 
hypr-comp/scripts/quickshell/calendar/weather.sh:188:             f_pop=$(echo "$day_data" | jq '[.[].pop] | max')
hypr-comp/scripts/quickshell/calendar/weather.sh:189:             f_pop_pct=$(echo "$f_pop * 100" | bc | cut -d. -f1)
hypr-comp/scripts/quickshell/calendar/weather.sh:190:             f_wind=$(echo "$day_data" | jq '[.[].wind.speed] | max | round')
hypr-comp/scripts/quickshell/calendar/weather.sh:191:             f_hum=$(echo "$day_data" | jq '[.[].main.humidity] | add / length | round')
hypr-comp/scripts/quickshell/calendar/weather.sh:192:             
hypr-comp/scripts/quickshell/calendar/weather.sh:193:             f_code=$(echo "$day_data" | jq -r '.[length/2 | floor].weather[0].icon')
hypr-comp/scripts/quickshell/calendar/weather.sh:194:             f_desc=$(echo "$day_data" | jq -r '.[length/2 | floor].weather[0].description' | sed -e "s/\b\(.\)/\u\1/g")
hypr-comp/scripts/quickshell/calendar/weather.sh:195:             f_icon_data=$(get_icon "$f_code")
hypr-comp/scripts/quickshell/calendar/weather.sh:196:             f_icon=$(echo "$f_icon_data" | cut -d'|' -f1)
hypr-comp/scripts/quickshell/calendar/weather.sh:197:             f_hex=$(get_hex "$f_code")
hypr-comp/scripts/quickshell/calendar/weather.sh:198:             
hypr-comp/scripts/quickshell/calendar/weather.sh:199:             f_day=$(date -d "$d" "+%a")
hypr-comp/scripts/quickshell/calendar/weather.sh:200:             f_full_day=$(date -d "$d" "+%A")
hypr-comp/scripts/quickshell/calendar/weather.sh:201:             f_date_num=$(date -d "$d" "+%d %b")
hypr-comp/scripts/quickshell/calendar/weather.sh:202: 
hypr-comp/scripts/quickshell/calendar/weather.sh:203:             hourly_json="["
hypr-comp/scripts/quickshell/calendar/weather.sh:204:             count_slots=$(echo "$day_data" | jq '. | length')
hypr-comp/scripts/quickshell/calendar/weather.sh:205:             count_slots=$((count_slots-1))
hypr-comp/scripts/quickshell/calendar/weather.sh:206:             
hypr-comp/scripts/quickshell/calendar/weather.sh:207:             for i in $(seq 0 1 $count_slots); do
hypr-comp/scripts/quickshell/calendar/weather.sh:208:                 slot_item=$(echo "$day_data" | jq ".[$i]")
hypr-comp/scripts/quickshell/calendar/weather.sh:209:                 
hypr-comp/scripts/quickshell/calendar/weather.sh:210:                 raw_s_temp=$(echo "$slot_item" | jq ".main.temp")
hypr-comp/scripts/quickshell/calendar/weather.sh:211:                 s_temp=$(printf "%.1f" "$raw_s_temp")
hypr-comp/scripts/quickshell/calendar/weather.sh:212:                 
hypr-comp/scripts/quickshell/calendar/weather.sh:213:                 s_dt=$(echo "$slot_item" | jq ".dt")
hypr-comp/scripts/quickshell/calendar/weather.sh:214:                 s_time=$(date -d @$s_dt "+%H:%M")
hypr-comp/scripts/quickshell/calendar/weather.sh:215:                 s_code=$(echo "$slot_item" | jq -r ".weather[0].icon")
hypr-comp/scripts/quickshell/calendar/weather.sh:216:                 s_hex=$(get_hex "$s_code")
hypr-comp/scripts/quickshell/calendar/weather.sh:217:                 s_icon=$(get_icon "$s_code" | cut -d'|' -f1)
hypr-comp/scripts/quickshell/calendar/weather.sh:218:                 
hypr-comp/scripts/quickshell/calendar/weather.sh:219:                 hourly_json="${hourly_json} {\"time\": \"${s_time}\", \"temp\": \"${s_temp}\", \"icon\": \"${s_icon}\", \"hex\": \"${s_hex}\"},"
…
hypr-comp/scripts/quickshell/calendar/weather.sh:242: 
hypr-comp/scripts/quickshell/calendar/weather.sh:243:         echo "{ \"current_temp\": \"${c_temp}\", \"current_icon\": \"${c_icon}\", \"current_hex\": \"${c_hex}\", \"forecast\": ${final_json} }" > "${json_file}"
hypr-comp/scripts/quickshell/calendar/weather.sh:244:     fi
hypr-comp/scripts/quickshell/calendar/weather.sh:245: }
…
hypr-comp/scripts/quickshell/calendar/weather.sh:254: 
hypr-comp/scripts/quickshell/calendar/weather.sh:255:     if [ -f "$json_file" ]; then
hypr-comp/scripts/quickshell/calendar/weather.sh:256:         file_time=$(stat -c %Y "$json_file")
hypr-comp/scripts/quickshell/calendar/weather.sh:257:         current_time=$(date +%s)
hypr-comp/scripts/quickshell/calendar/weather.sh:258:         diff=$((current_time - file_time))
hypr-comp/scripts/quickshell/calendar/weather.sh:259:         
hypr-comp/scripts/quickshell/calendar/weather.sh:260:         if grep -q '"desc": "No API Key"' "$json_file"; then
hypr-comp/scripts/quickshell/calendar/weather.sh:261:             # Key is pending/invalid. Check once an hour.
hypr-comp/scripts/quickshell/calendar/weather.sh:262:             if [ $diff -gt $PENDING_RETRY_LIMIT ]; then
hypr-comp/scripts/quickshell/calendar/weather.sh:263:                 touch "$json_file" # Bump file timestamp slightly to avoid spamming processes
hypr-comp/scripts/quickshell/calendar/weather.sh:264:                 get_data &
hypr-comp/scripts/quickshell/calendar/weather.sh:265:             fi
…
hypr-comp/scripts/quickshell/calendar/weather.sh:267:             # Normal working API key. Check every 15 mins.
hypr-comp/scripts/quickshell/calendar/weather.sh:268:             if [ $diff -gt $CACHE_LIMIT ]; then
hypr-comp/scripts/quickshell/calendar/weather.sh:269:                 touch "$json_file"
hypr-comp/scripts/quickshell/calendar/weather.sh:270:                 get_data &
hypr-comp/scripts/quickshell/calendar/weather.sh:271:             fi
hypr-comp/scripts/quickshell/calendar/weather.sh:272:         fi
hypr-comp/scripts/quickshell/calendar/weather.sh:273:         cat "$json_file"
hypr-comp/scripts/quickshell/calendar/weather.sh:274:     else
hypr-comp/scripts/quickshell/calendar/weather.sh:275:         get_data
hypr-comp/scripts/quickshell/calendar/weather.sh:276:         cat "$json_file"
hypr-comp/scripts/quickshell/calendar/weather.sh:277:     fi
hypr-comp/scripts/quickshell/calendar/weather.sh:278: 
hypr-comp/scripts/quickshell/calendar/weather.sh:279: elif [[ "$1" == "--view-listener" ]]; then
hypr-comp/scripts/quickshell/calendar/weather.sh:280:     if [ ! -f "$view_file" ]; then echo "0" > "$view_file"; fi
hypr-comp/scripts/quickshell/calendar/weather.sh:281:     tail -F "$view_file"
hypr-comp/scripts/quickshell/calendar/weather.sh:282: 
hypr-comp/scripts/quickshell/calendar/weather.sh:283: elif [[ "$1" == "--nav" ]]; then
hypr-comp/scripts/quickshell/calendar/weather.sh:284:     if [ ! -f "$view_file" ]; then echo "0" > "$view_file"; fi
hypr-comp/scripts/quickshell/calendar/weather.sh:285:     current=$(cat "$view_file")
hypr-comp/scripts/quickshell/calendar/weather.sh:286:     direction=$2
hypr-comp/scripts/quickshell/calendar/weather.sh:287:     max_idx=4
hypr-comp/scripts/quickshell/calendar/weather.sh:288:     if [[ "$direction" == "next" ]]; then
hypr-comp/scripts/quickshell/calendar/weather.sh:289:         if [ "$current" -lt "$max_idx" ]; then
hypr-comp/scripts/quickshell/calendar/weather.sh:290:             new=$((current + 1))
hypr-comp/scripts/quickshell/calendar/weather.sh:291:             echo "$new" > "$view_file"
hypr-comp/scripts/quickshell/calendar/weather.sh:292:         fi
hypr-comp/scripts/quickshell/calendar/weather.sh:293:     elif [[ "$direction" == "prev" ]]; then
hypr-comp/scripts/quickshell/calendar/weather.sh:294:         if [ "$current" -gt 0 ]; then
hypr-comp/scripts/quickshell/calendar/weather.sh:295:             new=$((current - 1))
hypr-comp/scripts/quickshell/calendar/weather.sh:296:             echo "$new" > "$view_file"
hypr-comp/scripts/quickshell/calendar/weather.sh:297:         fi
hypr-comp/scripts/quickshell/calendar/weather.sh:298:     fi
…
hypr-comp/scripts/quickshell/calendar/weather.sh:300: elif [[ "$1" == "--icon" ]]; then
hypr-comp/scripts/quickshell/calendar/weather.sh:301:     cat "$json_file" | jq -r '.forecast[0].icon'
hypr-comp/scripts/quickshell/calendar/weather.sh:302: 
hypr-comp/scripts/quickshell/calendar/weather.sh:303: elif [[ "$1" == "--temp" ]]; then 
hypr-comp/scripts/quickshell/calendar/weather.sh:304:     t=$(cat "$json_file" | jq -r '.forecast[0].max')
hypr-comp/scripts/quickshell/calendar/weather.sh:305:     echo "${t}${UNIT_SYM}"
hypr-comp/scripts/quickshell/calendar/weather.sh:306: 
hypr-comp/scripts/quickshell/calendar/weather.sh:307: elif [[ "$1" == "--hex" ]]; then 
hypr-comp/scripts/quickshell/calendar/weather.sh:308:     cat "$json_file" | jq -r '.forecast[0].hex'
hypr-comp/scripts/quickshell/calendar/weather.sh:309: 
hypr-comp/scripts/quickshell/calendar/weather.sh:310: elif [[ "$1" == "--current-icon" ]]; then
hypr-comp/scripts/quickshell/calendar/weather.sh:311:     icon=$(cat "$json_file" | jq -r '.current_icon // empty')
hypr-comp/scripts/quickshell/calendar/weather.sh:312:     if [[ -z "$icon" || "$icon" == "null" ]]; then 
hypr-comp/scripts/quickshell/calendar/weather.sh:313:         get_data
hypr-comp/scripts/quickshell/calendar/weather.sh:314:         icon=$(cat "$json_file" | jq -r '.current_icon')
hypr-comp/scripts/quickshell/calendar/weather.sh:315:     fi
hypr-comp/scripts/quickshell/calendar/weather.sh:316:     echo "$icon"
hypr-comp/scripts/quickshell/calendar/weather.sh:317: 
hypr-comp/scripts/quickshell/calendar/weather.sh:318: elif [[ "$1" == "--current-temp" ]]; then 
hypr-comp/scripts/quickshell/calendar/weather.sh:319:     t=$(cat "$json_file" | jq -r '.current_temp // empty')
hypr-comp/scripts/quickshell/calendar/weather.sh:320:     if [[ -z "$t" || "$t" == "null" ]]; then 
hypr-comp/scripts/quickshell/calendar/weather.sh:321:         get_data
hypr-comp/scripts/quickshell/calendar/weather.sh:322:         t=$(cat "$json_file" | jq -r '.current_temp')
hypr-comp/scripts/quickshell/calendar/weather.sh:323:     fi
hypr-comp/scripts/quickshell/calendar/weather.sh:324:     echo "${t}${UNIT_SYM}"
hypr-comp/scripts/quickshell/calendar/weather.sh:325: 
hypr-comp/scripts/quickshell/calendar/weather.sh:326: elif [[ "$1" == "--current-hex" ]]; then
hypr-comp/scripts/quickshell/calendar/weather.sh:327:     hex=$(cat "$json_file" | jq -r '.current_hex // empty')
hypr-comp/scripts/quickshell/calendar/weather.sh:328:     if [[ -z "$hex" || "$hex" == "null" ]]; then 
hypr-comp/scripts/quickshell/calendar/weather.sh:329:         get_data
hypr-comp/scripts/quickshell/calendar/weather.sh:330:         hex=$(cat "$json_file" | jq -r '.current_hex')
hypr-comp/scripts/quickshell/calendar/weather.sh:331:     fi
hypr-comp/scripts/quickshell/calendar/weather.sh:332:     echo "$hex"
hypr-comp/scripts/quickshell/calendar/weather.sh:333: fi
```

### `hypr-comp/scripts/quickshell/music/music_info.sh`

Migration owner: `mpris`. Source `hypr-comp/scripts/quickshell/music/music_info.sh:1`, 245 lines, SHA-256 `d182e59320fb8bf2316a57fe4059f08b3ecc22bc0fb6428e56cfc38e6d8467b4`.

```text
…
hypr-comp/scripts/quickshell/music/music_info.sh:8: 
hypr-comp/scripts/quickshell/music/music_info.sh:9: TMP_DIR="$QS_RUN_MUSIC/covers"
hypr-comp/scripts/quickshell/music/music_info.sh:10: STATE_FILE="$QS_STATE_MUSIC/last_state.json"
hypr-comp/scripts/quickshell/music/music_info.sh:11: 
hypr-comp/scripts/quickshell/music/music_info.sh:12: mkdir -p "$TMP_DIR"
hypr-comp/scripts/quickshell/music/music_info.sh:13: PLACEHOLDER="$TMP_DIR/placeholder_blank.png"
hypr-comp/scripts/quickshell/music/music_info.sh:14: 
hypr-comp/scripts/quickshell/music/music_info.sh:15: # Prevent cold-boot D-Bus hangs from keeping the script alive
hypr-comp/scripts/quickshell/music/music_info.sh:16: PT="timeout 1.5 playerctl"
hypr-comp/scripts/quickshell/music/music_info.sh:17: 
hypr-comp/scripts/quickshell/music/music_info.sh:18: # --- 1. ENSURE PLACEHOLDER EXISTS ---
hypr-comp/scripts/quickshell/music/music_info.sh:19: if [ ! -f "$PLACEHOLDER" ]; then
hypr-comp/scripts/quickshell/music/music_info.sh:20:     convert -size 500x500 xc:"#313244" "$PLACEHOLDER"
hypr-comp/scripts/quickshell/music/music_info.sh:21: fi
hypr-comp/scripts/quickshell/music/music_info.sh:22: 
hypr-comp/scripts/quickshell/music/music_info.sh:23: # --- 2. CHECK STATUS ---
hypr-comp/scripts/quickshell/music/music_info.sh:24: STATUS=$($PT status 2>/dev/null)
hypr-comp/scripts/quickshell/music/music_info.sh:25: 
hypr-comp/scripts/quickshell/music/music_info.sh:26: if [ "$STATUS" = "Playing" ] || [ "$STATUS" = "Paused" ]; then
hypr-comp/scripts/quickshell/music/music_info.sh:27: 
hypr-comp/scripts/quickshell/music/music_info.sh:28:     # --- 3. GET INFO ---
hypr-comp/scripts/quickshell/music/music_info.sh:29:     rawUrl=$($PT metadata mpris:artUrl 2>/dev/null)
hypr-comp/scripts/quickshell/music/music_info.sh:30:     title=$($PT metadata xesam:title 2>/dev/null)
hypr-comp/scripts/quickshell/music/music_info.sh:31:     artist=$($PT metadata xesam:artist 2>/dev/null)
hypr-comp/scripts/quickshell/music/music_info.sh:32:     
hypr-comp/scripts/quickshell/music/music_info.sh:33:     if [ -n "$rawUrl" ]; then
hypr-comp/scripts/quickshell/music/music_info.sh:34:         trackHash=$(echo "$rawUrl" | md5sum | cut -d" " -f1)
hypr-comp/scripts/quickshell/music/music_info.sh:35:     else
hypr-comp/scripts/quickshell/music/music_info.sh:36:         idStr="${title:-unknown}-${artist:-unknown}"
hypr-comp/scripts/quickshell/music/music_info.sh:37:         trackHash=$(echo "$idStr" | md5sum | cut -d" " -f1)
hypr-comp/scripts/quickshell/music/music_info.sh:38:     fi
hypr-comp/scripts/quickshell/music/music_info.sh:39:     
hypr-comp/scripts/quickshell/music/music_info.sh:40:     finalArt="$TMP_DIR/${trackHash}_art.jpg"
hypr-comp/scripts/quickshell/music/music_info.sh:41:     blurPath="$TMP_DIR/${trackHash}_blur.png"
hypr-comp/scripts/quickshell/music/music_info.sh:42:     colorPath="$TMP_DIR/${trackHash}_grad.txt"
hypr-comp/scripts/quickshell/music/music_info.sh:43:     textPath="$TMP_DIR/${trackHash}_text.txt"
hypr-comp/scripts/quickshell/music/music_info.sh:44:     lockFile="$TMP_DIR/${trackHash}.lock"
hypr-comp/scripts/quickshell/music/music_info.sh:45: 
hypr-comp/scripts/quickshell/music/music_info.sh:46:     displayArt="$PLACEHOLDER"
hypr-comp/scripts/quickshell/music/music_info.sh:47:     displayBlur="$PLACEHOLDER"
hypr-comp/scripts/quickshell/music/music_info.sh:48:     displayGrad="linear-gradient(45deg, #cba6f7, #89b4fa, #f38ba8, #cba6f7)"
hypr-comp/scripts/quickshell/music/music_info.sh:49:     displayText="#cdd6f4"
…
hypr-comp/scripts/quickshell/music/music_info.sh:51:     # --- 4. ASYNC BACKGROUND LOGIC ---
hypr-comp/scripts/quickshell/music/music_info.sh:52:     if [ -f "$finalArt" ] && [ -s "$finalArt" ]; then
hypr-comp/scripts/quickshell/music/music_info.sh:53:         displayArt="$finalArt"
hypr-comp/scripts/quickshell/music/music_info.sh:54:         if [ -f "$blurPath" ]; then displayBlur="$blurPath"; fi
hypr-comp/scripts/quickshell/music/music_info.sh:55:         if [ -f "$colorPath" ]; then displayGrad=$(cat "$colorPath"); fi
hypr-comp/scripts/quickshell/music/music_info.sh:56:         if [ -f "$textPath" ]; then displayText=$(cat "$textPath"); fi
hypr-comp/scripts/quickshell/music/music_info.sh:57:     else
hypr-comp/scripts/quickshell/music/music_info.sh:58:         if [ ! -f "$lockFile" ] && [ -n "$rawUrl" ]; then
hypr-comp/scripts/quickshell/music/music_info.sh:59:             touch "$lockFile"
hypr-comp/scripts/quickshell/music/music_info.sh:60:             # THE FIX: We redirect standard output/error to /dev/null
hypr-comp/scripts/quickshell/music/music_info.sh:61:             # This severs the pipe to Quickshell, preventing the 30s Qt destructor lockup.
hypr-comp/scripts/quickshell/music/music_info.sh:62:             (
hypr-comp/scripts/quickshell/music/music_info.sh:63:                 tempArt="$TMP_DIR/${trackHash}_temp_art.jpg"
hypr-comp/scripts/quickshell/music/music_info.sh:64:                 tempBlur="$TMP_DIR/${trackHash}_temp_blur.png"
hypr-comp/scripts/quickshell/music/music_info.sh:65: 
hypr-comp/scripts/quickshell/music/music_info.sh:66:                 if [[ "$rawUrl" == http* ]]; then
hypr-comp/scripts/quickshell/music/music_info.sh:67:                     curl -s -L --max-time 10 -o "$tempArt" "$rawUrl"
hypr-comp/scripts/quickshell/music/music_info.sh:68:                 else
hypr-comp/scripts/quickshell/music/music_info.sh:69:                     cleanPath=$(echo "$rawUrl" | sed 's/file:\/\///g')
hypr-comp/scripts/quickshell/music/music_info.sh:70:                     if [ -f "$cleanPath" ]; then
hypr-comp/scripts/quickshell/music/music_info.sh:71:                         cp "$cleanPath" "$tempArt"
hypr-comp/scripts/quickshell/music/music_info.sh:72:                     else
hypr-comp/scripts/quickshell/music/music_info.sh:73:                         cp "$PLACEHOLDER" "$tempArt"
hypr-comp/scripts/quickshell/music/music_info.sh:74:                     fi
hypr-comp/scripts/quickshell/music/music_info.sh:75:                 fi
hypr-comp/scripts/quickshell/music/music_info.sh:76: 
hypr-comp/scripts/quickshell/music/music_info.sh:77:                 if [ ! -s "$tempArt" ]; then
hypr-comp/scripts/quickshell/music/music_info.sh:78:                     cp "$PLACEHOLDER" "$tempArt"
hypr-comp/scripts/quickshell/music/music_info.sh:79:                 fi
hypr-comp/scripts/quickshell/music/music_info.sh:80: 
hypr-comp/scripts/quickshell/music/music_info.sh:81:                 isPlaceholder=$(convert "$tempArt" -format "%[hex:u.p{0,0}]" info: 2>/dev/null | cut -c1-6)
hypr-comp/scripts/quickshell/music/music_info.sh:82:                 
hypr-comp/scripts/quickshell/music/music_info.sh:83:                 if [[ "$isPlaceholder" == "313244" ]] || [[ -z "$isPlaceholder" ]]; then
hypr-comp/scripts/quickshell/music/music_info.sh:84:                     cp "$tempArt" "$tempBlur"
hypr-comp/scripts/quickshell/music/music_info.sh:85:                 else
hypr-comp/scripts/quickshell/music/music_info.sh:86:                     convert "$tempArt" -blur 0x20 -brightness-contrast -30x-10 "$tempBlur" 2>/dev/null
hypr-comp/scripts/quickshell/music/music_info.sh:87:                     
hypr-comp/scripts/quickshell/music/music_info.sh:88:                     colors=$(convert "$tempArt" -resize 50x50 -alpha off +dither -quantize RGB -colors 3 -depth 8 -format "%c" histogram:info: 2>/dev/null | grep -E -o '#[0-9A-Fa-f]{6}' | head -n 3 | tr '\n' ' ')
hypr-comp/scripts/quickshell/music/music_info.sh:89:                     read -r -a color_array <<< "$colors"
hypr-comp/scripts/quickshell/music/music_info.sh:90:                     
hypr-comp/scripts/quickshell/music/music_info.sh:91:                     c1=${color_array[0]:-#cba6f7}
hypr-comp/scripts/quickshell/music/music_info.sh:92:                     c2=${color_array[1]:-$c1}
hypr-comp/scripts/quickshell/music/music_info.sh:93:                     c3=${color_array[2]:-$c1}
hypr-comp/scripts/quickshell/music/music_info.sh:94:                     
hypr-comp/scripts/quickshell/music/music_info.sh:95:                     echo "linear-gradient(45deg, $c1, $c2, $c3, $c1)" > "$colorPath"
hypr-comp/scripts/quickshell/music/music_info.sh:96:                     
hypr-comp/scripts/quickshell/music/music_info.sh:97:                     opp_raw=$(convert xc:"$c1" -alpha off -negate -depth 8 -format "%[hex:u]" info: 2>/dev/null | grep -E -o '[0-9A-Fa-f]{6}' | head -n 1)
hypr-comp/scripts/quickshell/music/music_info.sh:98:                     if [ -n "$opp_raw" ]; then
hypr-comp/scripts/quickshell/music/music_info.sh:99:                         echo "#$opp_raw" > "$textPath"
hypr-comp/scripts/quickshell/music/music_info.sh:100:                     else
hypr-comp/scripts/quickshell/music/music_info.sh:101:                         echo "#cdd6f4" > "$textPath"
hypr-comp/scripts/quickshell/music/music_info.sh:102:                     fi
hypr-comp/scripts/quickshell/music/music_info.sh:103:                 fi
hypr-comp/scripts/quickshell/music/music_info.sh:104: 
hypr-comp/scripts/quickshell/music/music_info.sh:105:                 mv "$tempBlur" "$blurPath"
hypr-comp/scripts/quickshell/music/music_info.sh:106:                 mv "$tempArt" "$finalArt"
hypr-comp/scripts/quickshell/music/music_info.sh:107: 
hypr-comp/scripts/quickshell/music/music_info.sh:108:                 rm "$lockFile"
hypr-comp/scripts/quickshell/music/music_info.sh:109:                 (cd "$TMP_DIR" && ls -1t | tail -n +21 | xargs -r rm 2>/dev/null)
hypr-comp/scripts/quickshell/music/music_info.sh:110:             ) </dev/null >/dev/null 2>&1 & 
hypr-comp/scripts/quickshell/music/music_info.sh:111:             # ^^^ This is the magic line that fixes the freeze.
hypr-comp/scripts/quickshell/music/music_info.sh:112:         fi
…
hypr-comp/scripts/quickshell/music/music_info.sh:115:     # --- 5. TIMING ---
hypr-comp/scripts/quickshell/music/music_info.sh:116:     len_micro=$($PT metadata mpris:length 2>/dev/null)
hypr-comp/scripts/quickshell/music/music_info.sh:117:     if [ -z "$len_micro" ] || [ "$len_micro" -eq 0 ]; then len_micro=1000000; fi
hypr-comp/scripts/quickshell/music/music_info.sh:118:     len_sec=$((len_micro / 1000000))
hypr-comp/scripts/quickshell/music/music_info.sh:119:     len_str=$(printf "%02d:%02d" $((len_sec/60)) $((len_sec%60)))
hypr-comp/scripts/quickshell/music/music_info.sh:120: 
hypr-comp/scripts/quickshell/music/music_info.sh:121:     if [ "$STATUS" = "Playing" ]; then
hypr-comp/scripts/quickshell/music/music_info.sh:122:         pos_micro=$($PT metadata --format '{{position}}' 2>/dev/null)
hypr-comp/scripts/quickshell/music/music_info.sh:123:         if [ -z "$pos_micro" ]; then pos_micro=0; fi
hypr-comp/scripts/quickshell/music/music_info.sh:124:         pos_sec=$((pos_micro / 1000000))
hypr-comp/scripts/quickshell/music/music_info.sh:125: 
hypr-comp/scripts/quickshell/music/music_info.sh:126:         jq -n -c \
hypr-comp/scripts/quickshell/music/music_info.sh:127:             --argjson pos_sec "$pos_sec" \
hypr-comp/scripts/quickshell/music/music_info.sh:128:             --argjson len_sec "$len_sec" \
hypr-comp/scripts/quickshell/music/music_info.sh:129:             '{pos_sec: $pos_sec, len_sec: $len_sec}' \
hypr-comp/scripts/quickshell/music/music_info.sh:130:             > "$STATE_FILE"
hypr-comp/scripts/quickshell/music/music_info.sh:131:     else
hypr-comp/scripts/quickshell/music/music_info.sh:132:         pos_sec=0
hypr-comp/scripts/quickshell/music/music_info.sh:133:         if [ -f "$STATE_FILE" ]; then
hypr-comp/scripts/quickshell/music/music_info.sh:134:             saved_pos=$(jq -r '.pos_sec' "$STATE_FILE")
hypr-comp/scripts/quickshell/music/music_info.sh:135:             saved_len=$(jq -r '.len_sec' "$STATE_FILE")
hypr-comp/scripts/quickshell/music/music_info.sh:136:             if [ "$saved_len" = "$len_sec" ] && [ -n "$saved_pos" ] && [ "$saved_pos" != "null" ]; then
hypr-comp/scripts/quickshell/music/music_info.sh:137:                 pos_sec=$saved_pos
hypr-comp/scripts/quickshell/music/music_info.sh:138:             fi
hypr-comp/scripts/quickshell/music/music_info.sh:139:         fi
…
hypr-comp/scripts/quickshell/music/music_info.sh:142:     percent=$((pos_sec * 100 / len_sec))
hypr-comp/scripts/quickshell/music/music_info.sh:143:     pos_str=$(printf "%02d:%02d" $((pos_sec/60)) $((pos_sec%60)))
hypr-comp/scripts/quickshell/music/music_info.sh:144:     time_str="${pos_str} / ${len_str}"
hypr-comp/scripts/quickshell/music/music_info.sh:145: 
hypr-comp/scripts/quickshell/music/music_info.sh:146:     # --- 6. DEVICE INFO ---
hypr-comp/scripts/quickshell/music/music_info.sh:147:     player_raw=$($PT status -f "{{playerName}}" 2>/dev/null | head -n 1)
hypr-comp/scripts/quickshell/music/music_info.sh:148:     player_nice="${player_raw^}"
hypr-comp/scripts/quickshell/music/music_info.sh:149: 
…
hypr-comp/scripts/quickshell/music/music_info.sh:151:     dev_icon="󰓃"; dev_name="Speaker"
hypr-comp/scripts/quickshell/music/music_info.sh:152:     node_name=$(timeout 0.5 wpctl inspect @DEFAULT_AUDIO_SINK@ 2>/dev/null | awk -F'"' '/node\.name/ {print $2}')
hypr-comp/scripts/quickshell/music/music_info.sh:153:     node_desc=$(timeout 0.5 wpctl inspect @DEFAULT_AUDIO_SINK@ 2>/dev/null | awk -F'"' '/node\.description/ {print $2}')
hypr-comp/scripts/quickshell/music/music_info.sh:154: 
hypr-comp/scripts/quickshell/music/music_info.sh:155:     if [[ "$node_name" == *"bluez"* ]]; then
hypr-comp/scripts/quickshell/music/music_info.sh:156:         dev_icon="󰂯"
hypr-comp/scripts/quickshell/music/music_info.sh:157:         [ -n "$node_desc" ] && dev_name="$node_desc" || dev_name="Bluetooth"
hypr-comp/scripts/quickshell/music/music_info.sh:158:     elif [[ "$node_name" == *"usb"* ]]; then
hypr-comp/scripts/quickshell/music/music_info.sh:159:         dev_icon="󰓃"; dev_name="USB Audio"
hypr-comp/scripts/quickshell/music/music_info.sh:160:     elif [[ "$node_name" == *"pci"* ]]; then
hypr-comp/scripts/quickshell/music/music_info.sh:161:         dev_icon="󰓃"; dev_name="System"
hypr-comp/scripts/quickshell/music/music_info.sh:162:     elif [ -n "$node_desc" ]; then
hypr-comp/scripts/quickshell/music/music_info.sh:163:         dev_name="$node_desc"
hypr-comp/scripts/quickshell/music/music_info.sh:164:     fi
hypr-comp/scripts/quickshell/music/music_info.sh:165: 
hypr-comp/scripts/quickshell/music/music_info.sh:166:     # --- 7. JSON OUTPUT ---
hypr-comp/scripts/quickshell/music/music_info.sh:167:     jq -n -c \
hypr-comp/scripts/quickshell/music/music_info.sh:168:         --arg title "$title" \
hypr-comp/scripts/quickshell/music/music_info.sh:169:         --arg artist "$artist" \
hypr-comp/scripts/quickshell/music/music_info.sh:170:         --arg status "$STATUS" \
hypr-comp/scripts/quickshell/music/music_info.sh:171:         --arg len "$len_sec" \
hypr-comp/scripts/quickshell/music/music_info.sh:172:         --arg pos "$pos_sec" \
hypr-comp/scripts/quickshell/music/music_info.sh:173:         --arg len_str "$len_str" \
hypr-comp/scripts/quickshell/music/music_info.sh:174:         --arg pos_str "$pos_str" \
hypr-comp/scripts/quickshell/music/music_info.sh:175:         --arg time_str "$time_str" \
hypr-comp/scripts/quickshell/music/music_info.sh:176:         --arg percent "$percent" \
hypr-comp/scripts/quickshell/music/music_info.sh:177:         --arg source "$player_nice" \
hypr-comp/scripts/quickshell/music/music_info.sh:178:         --arg pname "$player_raw" \
hypr-comp/scripts/quickshell/music/music_info.sh:179:         --arg blur "$displayBlur" \
hypr-comp/scripts/quickshell/music/music_info.sh:180:         --arg grad "$displayGrad" \
hypr-comp/scripts/quickshell/music/music_info.sh:181:         --arg txtColor "$displayText" \
hypr-comp/scripts/quickshell/music/music_info.sh:182:         --arg devIcon "$dev_icon" \
hypr-comp/scripts/quickshell/music/music_info.sh:183:         --arg devName "$dev_name" \
hypr-comp/scripts/quickshell/music/music_info.sh:184:         --arg finalArt "$displayArt" \
hypr-comp/scripts/quickshell/music/music_info.sh:185:         '{
hypr-comp/scripts/quickshell/music/music_info.sh:186:             title: $title,
hypr-comp/scripts/quickshell/music/music_info.sh:187:             artist: $artist,
hypr-comp/scripts/quickshell/music/music_info.sh:188:             status: $status,
hypr-comp/scripts/quickshell/music/music_info.sh:189:             length: $len,
hypr-comp/scripts/quickshell/music/music_info.sh:190:             position: $pos,
hypr-comp/scripts/quickshell/music/music_info.sh:191:             lengthStr: $len_str,
hypr-comp/scripts/quickshell/music/music_info.sh:192:             positionStr: $pos_str,
hypr-comp/scripts/quickshell/music/music_info.sh:193:             timeStr: $time_str,
hypr-comp/scripts/quickshell/music/music_info.sh:194:             percent: $percent,
hypr-comp/scripts/quickshell/music/music_info.sh:195:             source: $source,
hypr-comp/scripts/quickshell/music/music_info.sh:196:             playerName: $pname,
hypr-comp/scripts/quickshell/music/music_info.sh:197:             blur: $blur,
hypr-comp/scripts/quickshell/music/music_info.sh:198:             grad: $grad,
hypr-comp/scripts/quickshell/music/music_info.sh:199:             textColor: $txtColor,
hypr-comp/scripts/quickshell/music/music_info.sh:200:             deviceIcon: $devIcon,
hypr-comp/scripts/quickshell/music/music_info.sh:201:             deviceName: $devName,
hypr-comp/scripts/quickshell/music/music_info.sh:202:             artUrl: $finalArt
hypr-comp/scripts/quickshell/music/music_info.sh:203:         }'
hypr-comp/scripts/quickshell/music/music_info.sh:204: 
…
hypr-comp/scripts/quickshell/music/music_info.sh:206:     # --- FALLBACK (Stopped) ---
hypr-comp/scripts/quickshell/music/music_info.sh:207:     if [ -f "$STATE_FILE" ]; then
hypr-comp/scripts/quickshell/music/music_info.sh:208:         last_pos_sec=$(jq -r '.pos_sec' "$STATE_FILE")
hypr-comp/scripts/quickshell/music/music_info.sh:209:         last_len_sec=$(jq -r '.len_sec' "$STATE_FILE")
hypr-comp/scripts/quickshell/music/music_info.sh:210:     else
hypr-comp/scripts/quickshell/music/music_info.sh:211:         last_pos_sec=0; last_len_sec=0
…
hypr-comp/scripts/quickshell/music/music_info.sh:213: 
hypr-comp/scripts/quickshell/music/music_info.sh:214:     if [ -z "$last_pos_sec" ] || [ "$last_pos_sec" = "null" ]; then last_pos_sec=0; fi
hypr-comp/scripts/quickshell/music/music_info.sh:215:     if [ -z "$last_len_sec" ] || [ "$last_len_sec" = "null" ] || [ "$last_len_sec" -eq 0 ]; then last_len_sec=1; fi
hypr-comp/scripts/quickshell/music/music_info.sh:216: 
hypr-comp/scripts/quickshell/music/music_info.sh:217:     last_percent=$((last_pos_sec * 100 / last_len_sec))
hypr-comp/scripts/quickshell/music/music_info.sh:218:     last_pos_str=$(printf "%02d:%02d" $((last_pos_sec/60)) $((last_pos_sec%60)))
hypr-comp/scripts/quickshell/music/music_info.sh:219:     last_len_str=$(printf "%02d:%02d" $((last_len_sec/60)) $((last_len_sec%60)))
hypr-comp/scripts/quickshell/music/music_info.sh:220:     last_time_str="${last_pos_str} / ${last_len_str}"
hypr-comp/scripts/quickshell/music/music_info.sh:221: 
hypr-comp/scripts/quickshell/music/music_info.sh:222:     jq -n -c \
hypr-comp/scripts/quickshell/music/music_info.sh:223:     --arg placeholder "$PLACEHOLDER" \
hypr-comp/scripts/quickshell/music/music_info.sh:224:     --arg pos_str "$last_pos_str" \
hypr-comp/scripts/quickshell/music/music_info.sh:225:     --arg len_str "$last_len_str" \
hypr-comp/scripts/quickshell/music/music_info.sh:226:     --arg time_str "$last_time_str" \
hypr-comp/scripts/quickshell/music/music_info.sh:227:     --arg percent "$last_percent" \
hypr-comp/scripts/quickshell/music/music_info.sh:228:     '{
hypr-comp/scripts/quickshell/music/music_info.sh:229:         title: "Not Playing",
…
hypr-comp/scripts/quickshell/music/music_info.sh:231:         status: "Stopped",
hypr-comp/scripts/quickshell/music/music_info.sh:232:         percent: $percent,
hypr-comp/scripts/quickshell/music/music_info.sh:233:         lengthStr: $len_str,
hypr-comp/scripts/quickshell/music/music_info.sh:234:         positionStr: $pos_str,
hypr-comp/scripts/quickshell/music/music_info.sh:235:         timeStr: $time_str,
hypr-comp/scripts/quickshell/music/music_info.sh:236:         source: "Offline",
hypr-comp/scripts/quickshell/music/music_info.sh:237:         playerName: "",
hypr-comp/scripts/quickshell/music/music_info.sh:238:         blur: $placeholder,
hypr-comp/scripts/quickshell/music/music_info.sh:239:         grad: "linear-gradient(45deg, #cba6f7, #89b4fa, #f38ba8, #cba6f7)",
hypr-comp/scripts/quickshell/music/music_info.sh:240:         textColor: "#cdd6f4",
…
hypr-comp/scripts/quickshell/music/music_info.sh:242:         deviceName: "Speaker",
hypr-comp/scripts/quickshell/music/music_info.sh:243:         artUrl: $placeholder
hypr-comp/scripts/quickshell/music/music_info.sh:244:     }'
hypr-comp/scripts/quickshell/music/music_info.sh:245: fi
```

### `hypr-comp/scripts/quickshell/watchers/audio_fetch.sh`

Migration owner: `audio/spectrum`. Source `hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:1`, 46 lines, SHA-256 `9ebf6f019c72bb2d7a12f5d17c7c7ee04192f0d06cc6fc8a6b16fa6e987f8db7`.

```text
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:1: #!/usr/bin/env bash
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:2: get_volume() {
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:3:     local vol=""
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:4:     if command -v wpctl &> /dev/null; then 
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:5:         vol=$(LC_ALL=C wpctl get-volume @DEFAULT_AUDIO_SINK@ 2>/dev/null | awk '{print int($2*100)}')
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:6:     fi
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:7:     if [[ -z "$vol" ]] && command -v pamixer &> /dev/null; then 
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:8:         vol=$(LC_ALL=C pamixer --get-volume 2>/dev/null)
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:9:     fi
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:10:     echo "${vol:-0}"
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:11: }
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:12: 
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:13: is_muted() {
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:14:     if command -v wpctl &> /dev/null; then
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:15:         if LC_ALL=C wpctl get-volume @DEFAULT_AUDIO_SINK@ 2>/dev/null | grep -q "MUTED"; then echo "true"; else echo "false"; fi
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:16:     elif command -v pamixer &> /dev/null; then
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:17:         if LC_ALL=C pamixer --get-mute 2>/dev/null | grep -q "true"; then echo "true"; else echo "false"; fi
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:18:     else 
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:19:         echo "false"
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:20:     fi
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:21: }
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:22: 
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:23: get_volume_icon() {
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:24:     local vol=$(get_volume)
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:25:     local muted=$(is_muted)
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:26:     if [ "$muted" = "true" ]; then echo "󰝟"
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:27:     elif [ "$vol" -ge 70 ]; then echo "󰕾"
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:28:     elif [ "$vol" -ge 30 ]; then echo "󰖀"
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:29:     elif [ "$vol" -gt 0 ]; then echo "󰕿"
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:30:     else echo "󰝟"; fi
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:31: }
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:32: 
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:33: toggle_mute() {
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:34:     if command -v wpctl &> /dev/null; then
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:35:         LC_ALL=C wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:36:     elif command -v pamixer &> /dev/null; then
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:37:         LC_ALL=C pamixer --toggle-mute 2>/dev/null
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:38:     fi
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:39:     if [ "$(is_muted)" = "true" ]; then notify-send -u low -i audio-volume-muted "Volume" "Muted"
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:40:     else notify-send -u low -i audio-volume-high "Volume" "Unmuted ($(get_volume)%)"; fi
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:41: }
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:42: 
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:43: case $1 in
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:44:     --toggle) toggle_mute ;;
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:45:     *) jq -n -c --arg volume "$(get_volume)" --arg icon "$(get_volume_icon)" --arg is_muted "$(is_muted)" '{volume: $volume, icon: $icon, is_muted: $is_muted}' ;;
hypr-comp/scripts/quickshell/watchers/audio_fetch.sh:46: esac
```

### `hypr-comp/scripts/quickshell/watchers/audio_wait.sh`

Migration owner: `audio/spectrum`. Source `hypr-comp/scripts/quickshell/watchers/audio_wait.sh:1`, 13 lines, SHA-256 `55a475faedcde1de4a606c12793094cab8ec5bb548da2a7e5538dfda9b29aefe`.

```text
hypr-comp/scripts/quickshell/watchers/audio_wait.sh:1: #!/usr/bin/env bash
hypr-comp/scripts/quickshell/watchers/audio_wait.sh:2: source "$(dirname "${BASH_SOURCE[0]}")/../../caching.sh"
hypr-comp/scripts/quickshell/watchers/audio_wait.sh:3: 
hypr-comp/scripts/quickshell/watchers/audio_wait.sh:4: PIPE="$QS_RUN_DIR/qs_audio_wait_$$.fifo"
hypr-comp/scripts/quickshell/watchers/audio_wait.sh:5: mkfifo "$PIPE" 2>/dev/null
hypr-comp/scripts/quickshell/watchers/audio_wait.sh:6: 
hypr-comp/scripts/quickshell/watchers/audio_wait.sh:7: trap 'rm -f "$PIPE"; kill $MONITOR_PID 2>/dev/null; exit 0' EXIT INT TERM
hypr-comp/scripts/quickshell/watchers/audio_wait.sh:8: 
hypr-comp/scripts/quickshell/watchers/audio_wait.sh:9: # Run pactl isolated and capture its exact PID to prevent PipeWire connection exhaustion
hypr-comp/scripts/quickshell/watchers/audio_wait.sh:10: LC_ALL=C pactl subscribe 2>/dev/null > "$PIPE" &
hypr-comp/scripts/quickshell/watchers/audio_wait.sh:11: MONITOR_PID=$!
hypr-comp/scripts/quickshell/watchers/audio_wait.sh:12: 
hypr-comp/scripts/quickshell/watchers/audio_wait.sh:13: grep -m 1 -E "sink|server" < "$PIPE" > /dev/null
```

### `hypr-comp/scripts/quickshell/watchers/battery_fetch.sh`

Migration owner: `power/battery_history`. Source `hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:1`, 27 lines, SHA-256 `8122a50e444aa08999525d9252c869f7d6aee9a88f9dfe099e503449ce50d463`.

```text
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:1: #!/usr/bin/env bash
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:2: get_battery_percent() { LC_ALL=C cat /sys/class/power_supply/BAT*/capacity 2>/dev/null | head -n1 || echo "100"; }
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:3: get_battery_status() { LC_ALL=C cat /sys/class/power_supply/BAT*/status 2>/dev/null | head -n1 || echo "Full"; }
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:4: get_battery_icon() {
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:5:     local percent=$(get_battery_percent)
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:6:     local status=$(get_battery_status)
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:7:     if [ "$status" = "Charging" ] || [ "$status" = "Full" ]; then
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:8:         if [ "$percent" -ge 90 ]; then echo "󰂅"
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:9:         elif [ "$percent" -ge 80 ]; then echo "󰂋"
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:10:         elif [ "$percent" -ge 60 ]; then echo "󰂊"
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:11:         elif [ "$percent" -ge 40 ]; then echo "󰢞"
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:12:         elif [ "$percent" -ge 20 ]; then echo "󰂆"
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:13:         else echo "󰢜"; fi
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:14:     else
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:15:         if [ "$percent" -ge 90 ]; then echo "󰁹"
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:16:         elif [ "$percent" -ge 80 ]; then echo "󰂂"
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:17:         elif [ "$percent" -ge 70 ]; then echo "󰂁"
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:18:         elif [ "$percent" -ge 60 ]; then echo "󰂀"
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:19:         elif [ "$percent" -ge 50 ]; then echo "󰁿"
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:20:         elif [ "$percent" -ge 40 ]; then echo "󰁾"
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:21:         elif [ "$percent" -ge 30 ]; then echo "󰁽"
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:22:         elif [ "$percent" -ge 20 ]; then echo "󰁼"
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:23:         elif [ "$percent" -ge 10 ]; then echo "󰁻"
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:24:         else echo "󰁺"; fi
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:25:     fi
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:26: }
hypr-comp/scripts/quickshell/watchers/battery_fetch.sh:27: jq -n -c --arg percent "$(get_battery_percent)" --arg status "$(get_battery_status)" --arg icon "$(get_battery_icon)" '{percent: $percent, status: $status, icon: $icon}'
```

### `hypr-comp/scripts/quickshell/watchers/battery_wait.sh`

Migration owner: `power/battery_history`. Source `hypr-comp/scripts/quickshell/watchers/battery_wait.sh:1`, 15 lines, SHA-256 `a91308039bc1822eac96d7e72b9eef69e55b8afbd9da8380041a98271abbb4c2`.

```text
hypr-comp/scripts/quickshell/watchers/battery_wait.sh:1: #!/usr/bin/env bash
hypr-comp/scripts/quickshell/watchers/battery_wait.sh:2: source "$(dirname "${BASH_SOURCE[0]}")/../../caching.sh"
hypr-comp/scripts/quickshell/watchers/battery_wait.sh:3: 
hypr-comp/scripts/quickshell/watchers/battery_wait.sh:4: PIPE="$QS_RUN_DIR/qs_battery_wait_$$.fifo"
hypr-comp/scripts/quickshell/watchers/battery_wait.sh:5: mkfifo "$PIPE" 2>/dev/null
hypr-comp/scripts/quickshell/watchers/battery_wait.sh:6: 
hypr-comp/scripts/quickshell/watchers/battery_wait.sh:7: trap 'rm -f "$PIPE"; kill $MONITOR_PID 2>/dev/null; exit 0' EXIT INT TERM
hypr-comp/scripts/quickshell/watchers/battery_wait.sh:8: 
hypr-comp/scripts/quickshell/watchers/battery_wait.sh:9: # Run udevadm isolated and capture its exact PID
hypr-comp/scripts/quickshell/watchers/battery_wait.sh:10: LC_ALL=C udevadm monitor --subsystem-match=power_supply 2>/dev/null > "$PIPE" &
hypr-comp/scripts/quickshell/watchers/battery_wait.sh:11: MONITOR_PID=$!
hypr-comp/scripts/quickshell/watchers/battery_wait.sh:12: 
hypr-comp/scripts/quickshell/watchers/battery_wait.sh:13: # Blocks until udevadm catches a change, OR 30 seconds pass (your failsafe).
hypr-comp/scripts/quickshell/watchers/battery_wait.sh:14: # Either way, when this line finishes, the trap fires and cleans up perfectly.
hypr-comp/scripts/quickshell/watchers/battery_wait.sh:15: timeout 10 grep -m 1 "change" < "$PIPE" > /dev/null
```

### `hypr-comp/scripts/quickshell/watchers/bt_fetch.sh`

Migration owner: `bluetooth`. Source `hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:1`, 28 lines, SHA-256 `37cfa38f9069fcd099ea9db6bb5aab2fbd9436a3d4c284174d8b752e75fc8157`.

```text
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:1: #!/usr/bin/env bash
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:2: get_bt_status() {
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:3:     if LC_ALL=C timeout 0.5 bluetoothctl show 2>/dev/null | grep -q "Powered: yes"; then echo "on"; else echo "off"; fi
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:4: }
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:5: get_bt_connected_device() {
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:6:     if [ "$(get_bt_status)" = "on" ]; then
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:7:         local device=$(LC_ALL=C timeout 0.5 bluetoothctl devices Connected 2>/dev/null | head -n1 | cut -d' ' -f3-)
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:8:         if [ -n "$device" ]; then echo "$device"; else echo "Disconnected"; fi
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:9:     else echo "Off"; fi
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:10: }
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:11: get_bt_icon() {
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:12:     if [ "$(get_bt_status)" = "on" ]; then
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:13:         if LC_ALL=C timeout 0.5 bluetoothctl devices Connected 2>/dev/null | grep -q "^Device"; then echo "󰂱"; else echo "󰂯"; fi
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:14:     else echo "󰂲"; fi
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:15: }
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:16: toggle_bt() {
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:17:     if [ "$(get_bt_status)" = "on" ]; then
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:18:         LC_ALL=C timeout 0.5 bluetoothctl power off 2>/dev/null
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:19:         notify-send -u low -i bluetooth-disabled "Bluetooth" "Disabled"
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:20:     else
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:21:         LC_ALL=C timeout 0.5 bluetoothctl power on 2>/dev/null
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:22:         notify-send -u low -i bluetooth-active "Bluetooth" "Enabled"
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:23:     fi
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:24: }
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:25: case $1 in
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:26:     --toggle) toggle_bt ;;
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:27:     *) jq -n -c --arg status "$(get_bt_status)" --arg icon "$(get_bt_icon)" --arg connected "$(get_bt_connected_device)" '{status: $status, icon: $icon, connected: $connected}' ;;
hypr-comp/scripts/quickshell/watchers/bt_fetch.sh:28: esac
```

### `hypr-comp/scripts/quickshell/watchers/bt_wait.sh`

Migration owner: `bluetooth`. Source `hypr-comp/scripts/quickshell/watchers/bt_wait.sh:1`, 9 lines, SHA-256 `794d051a2c2d2f91ab59eb765238659bb4e12567deeafa720442942e8af241ee`.

```text
hypr-comp/scripts/quickshell/watchers/bt_wait.sh:1: #!/usr/bin/env bash
hypr-comp/scripts/quickshell/watchers/bt_wait.sh:2: source "$(dirname "${BASH_SOURCE[0]}")/../../caching.sh"
hypr-comp/scripts/quickshell/watchers/bt_wait.sh:3: 
hypr-comp/scripts/quickshell/watchers/bt_wait.sh:4: PIPE="$QS_RUN_DIR/qs_bt_wait_$$.fifo"
hypr-comp/scripts/quickshell/watchers/bt_wait.sh:5: mkfifo "$PIPE" 2>/dev/null
hypr-comp/scripts/quickshell/watchers/bt_wait.sh:6: trap 'rm -f "$PIPE"; kill $(jobs -p) 2>/dev/null; exit 0' EXIT INT TERM
hypr-comp/scripts/quickshell/watchers/bt_wait.sh:7: LC_ALL=C dbus-monitor --system "type='signal',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged',arg0='org.bluez.Device1'" 2>/dev/null | grep --line-buffered 'string "Connected"' > "$PIPE" &
hypr-comp/scripts/quickshell/watchers/bt_wait.sh:8: LC_ALL=C dbus-monitor --system "type='signal',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged',arg0='org.bluez.Adapter1'" 2>/dev/null | grep --line-buffered 'string "Powered"' > "$PIPE" &
hypr-comp/scripts/quickshell/watchers/bt_wait.sh:9: read -r _ < "$PIPE"
```

### `hypr-comp/scripts/quickshell/watchers/kb_fetch.sh`

Migration owner: `hyprland`. Source `hypr-comp/scripts/quickshell/watchers/kb_fetch.sh:1`, 4 lines, SHA-256 `79ab69eead5d53b17cfbf418497ca4f4b347279bba9e6767af526734dd786b7c`.

```text
hypr-comp/scripts/quickshell/watchers/kb_fetch.sh:1: #!/usr/bin/env bash
hypr-comp/scripts/quickshell/watchers/kb_fetch.sh:2: layout=$(LC_ALL=C hyprctl devices -j 2>/dev/null | jq -r '(.keyboards[] | select(.main == true) | .active_keymap) // .keyboards[0].active_keymap // empty' | head -n1)
hypr-comp/scripts/quickshell/watchers/kb_fetch.sh:3: [[ -z "$layout" || "$layout" == "null" ]] && layout="US"
hypr-comp/scripts/quickshell/watchers/kb_fetch.sh:4: echo "${layout:0:2}" | tr '[:lower:]' '[:upper:]'
```

### `hypr-comp/scripts/quickshell/watchers/kb_wait.sh`

Migration owner: `hyprland`. Source `hypr-comp/scripts/quickshell/watchers/kb_wait.sh:1`, 15 lines, SHA-256 `0cfb282b3543270edf900b0e1bef39f773226b1089ff338af57894f79373faf7`.

```text
hypr-comp/scripts/quickshell/watchers/kb_wait.sh:1: #!/usr/bin/env bash
hypr-comp/scripts/quickshell/watchers/kb_wait.sh:2: source "$(dirname "${BASH_SOURCE[0]}")/../../caching.sh"
hypr-comp/scripts/quickshell/watchers/kb_wait.sh:3: 
hypr-comp/scripts/quickshell/watchers/kb_wait.sh:4: PIPE="$QS_RUN_DIR/qs_kb_wait_$$.fifo"
hypr-comp/scripts/quickshell/watchers/kb_wait.sh:5: mkfifo "$PIPE" 2>/dev/null
hypr-comp/scripts/quickshell/watchers/kb_wait.sh:6: trap 'rm -f "$PIPE"; kill $(jobs -p) 2>/dev/null; exit 0' EXIT INT TERM
hypr-comp/scripts/quickshell/watchers/kb_wait.sh:7: 
hypr-comp/scripts/quickshell/watchers/kb_wait.sh:8: if [ -n "$HYPRLAND_INSTANCE_SIGNATURE" ]; then
hypr-comp/scripts/quickshell/watchers/kb_wait.sh:9:     LC_ALL=C socat -U - UNIX-CONNECT:$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock 2>/dev/null | grep --line-buffered "activelayout>>" > "$PIPE" &
hypr-comp/scripts/quickshell/watchers/kb_wait.sh:10: else
hypr-comp/scripts/quickshell/watchers/kb_wait.sh:11:     sleep 10 > "$PIPE" &
hypr-comp/scripts/quickshell/watchers/kb_wait.sh:12: fi
hypr-comp/scripts/quickshell/watchers/kb_wait.sh:13: 
hypr-comp/scripts/quickshell/watchers/kb_wait.sh:14: read -r _ < "$PIPE"
hypr-comp/scripts/quickshell/watchers/kb_wait.sh:15: sleep 0.05
```

### `hypr-comp/scripts/quickshell/watchers/network_fetch.sh`

Migration owner: `network`. Source `hypr-comp/scripts/quickshell/watchers/network_fetch.sh:1`, 105 lines, SHA-256 `f37b27b3d55f973976057cec141ee6aa9bc9fb48a58b06c32017e7433455c6f1`.

```text
…
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:2: 
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:3: get_wifi_radio() {
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:4:     LC_ALL=C nmcli radio wifi 2>/dev/null
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:5: }
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:6: 
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:7: get_wifi_ssid() {
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:8:     local ssid=""
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:9:     if command -v iw &>/dev/null; then
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:10:         ssid=$(LC_ALL=C iw dev 2>/dev/null | awk '/\s+ssid/ { $1=""; sub(/^ /, ""); print; exit }')
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:11:     fi
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:12:     if [ -z "$ssid" ]; then
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:13:         ssid=$(LC_ALL=C nmcli -t -f NAME,TYPE connection show --active 2>/dev/null | awk -F: '/802-11-wireless/ {print $1; exit}')
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:14:     fi
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:15:     echo "${ssid:-}"
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:16: }
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:17: 
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:18: get_wifi_strength() {
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:19:     local signal=$(LC_ALL=C awk 'NR==3 {gsub(/\./,"",$3); print int($3 * 100 / 70)}' /proc/net/wireless 2>/dev/null)
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:20:     echo "${signal:-0}"
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:21: }
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:22: 
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:23: get_network_data() {
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:24:     # Find the active interface routing internet traffic
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:25:     local active_iface=$(ip route show default 2>/dev/null | awk '/default/ {print $5; exit}')
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:26:     local iface_type=""
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:27:     
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:28:     if [ -n "$active_iface" ]; then
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:29:         iface_type=$(LC_ALL=C nmcli -t -f DEVICE,TYPE d 2>/dev/null | awk -F: -v dev="$active_iface" '$1==dev {print $2; exit}')
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:30:     fi
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:31: 
…
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:37:     # Scenario 1: Ethernet is actively providing internet
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:38:     if [ "$iface_type" = "ethernet" ]; then
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:39:         status="enabled"
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:40:         ssid="Ethernet"
…
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:44:     # Scenario 2: Wi-Fi is actively providing internet
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:45:     elif [ "$iface_type" = "wifi" ]; then
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:46:         status="enabled"
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:47:         ssid=$(get_wifi_ssid)
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:48:         local signal=$(get_wifi_strength)
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:49:         if [ "$signal" -ge 75 ]; then icon="󰤨"
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:50:         elif [ "$signal" -ge 50 ]; then icon="󰤥"
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:51:         elif [ "$signal" -ge 25 ]; then icon="󰤢"
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:52:         else icon="󰤟"; fi
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:53:         
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:54:         # Still check if an ethernet cable is plugged in silently in the background
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:55:         local eth_dev=$(LC_ALL=C nmcli -t -f DEVICE,TYPE,STATE d 2>/dev/null | awk -F: '$2=="ethernet" && $3=="connected" && $1 != "lo" {print $1; exit}')
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:56:         if [ -n "$eth_dev" ]; then eth_status="Connected"; fi
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:57:         
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:58:     # Scenario 3: No active internet connection
…
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:60:         local radio=$(get_wifi_radio)
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:61:         local wifi_dev=$(LC_ALL=C nmcli -t -f DEVICE,TYPE d 2>/dev/null | awk -F: '$2=="wifi" {print $1; exit}')
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:62:         
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:63:         if [ -z "$wifi_dev" ]; then
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:64:             # No Wi-Fi hardware exists, and Ethernet is unplugged
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:65:             status="disabled"
…
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:67:             icon="󰈂"
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:68:         elif [ "$radio" = "disabled" ]; then
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:69:             # Wi-Fi hardware exists, but the radio is turned off
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:70:             status="disabled"
…
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:80: 
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:81:     echo "$status|$ssid|$icon|$eth_status"
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:82: }
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:83: 
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:84: toggle_wifi() {
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:85:     if [ "$(get_wifi_radio)" = "enabled" ]; then
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:86:         LC_ALL=C nmcli radio wifi off
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:87:         notify-send -u low -i network-wireless-disabled "WiFi" "Disabled"
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:88:     else
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:89:         LC_ALL=C nmcli radio wifi on
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:90:         notify-send -u low -i network-wireless-enabled "WiFi" "Enabled"
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:91:     fi
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:92: }
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:93: 
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:94: case $1 in
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:95:     --toggle) toggle_wifi ;;
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:96:     *) 
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:97:         IFS='|' read -r status ssid icon eth <<< "$(get_network_data)"
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:98:         
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:99:         jq -n -c \
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:100:             --arg status "$status" \
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:101:             --arg ssid "$ssid" \
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:102:             --arg icon "$icon" \
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:103:             --arg eth "$eth" \
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:104:             '{status: $status, ssid: $ssid, icon: $icon, eth_status: $eth}' ;;
hypr-comp/scripts/quickshell/watchers/network_fetch.sh:105: esac
```

### `hypr-comp/scripts/quickshell/watchers/network_wait.sh`

Migration owner: `network`. Source `hypr-comp/scripts/quickshell/watchers/network_wait.sh:1`, 16 lines, SHA-256 `23bf169db4ac36f2ad342ecd1e1c64d01c7607c23cb1bef520d0b57aed44cceb`.

```text
hypr-comp/scripts/quickshell/watchers/network_wait.sh:1: #!/usr/bin/env bash
hypr-comp/scripts/quickshell/watchers/network_wait.sh:2: source "$(dirname "${BASH_SOURCE[0]}")/../../caching.sh"
hypr-comp/scripts/quickshell/watchers/network_wait.sh:3: 
hypr-comp/scripts/quickshell/watchers/network_wait.sh:4: PIPE="$QS_RUN_DIR/qs_network_wait_$$.fifo"
hypr-comp/scripts/quickshell/watchers/network_wait.sh:5: mkfifo "$PIPE" 2>/dev/null
hypr-comp/scripts/quickshell/watchers/network_wait.sh:6: 
hypr-comp/scripts/quickshell/watchers/network_wait.sh:7: # Trap ensures we delete the FIFO and specifically kill nmcli, leaving no zombie processes
hypr-comp/scripts/quickshell/watchers/network_wait.sh:8: trap 'rm -f "$PIPE"; kill $MONITOR_PID 2>/dev/null; exit 0' EXIT INT TERM
hypr-comp/scripts/quickshell/watchers/network_wait.sh:9: 
hypr-comp/scripts/quickshell/watchers/network_wait.sh:10: # Run nmcli completely isolated and capture its exact PID
hypr-comp/scripts/quickshell/watchers/network_wait.sh:11: LC_ALL=C nmcli monitor 2>/dev/null > "$PIPE" &
hypr-comp/scripts/quickshell/watchers/network_wait.sh:12: MONITOR_PID=$!
hypr-comp/scripts/quickshell/watchers/network_wait.sh:13: 
hypr-comp/scripts/quickshell/watchers/network_wait.sh:14: # Grep blocks until it reads the first match from the FIFO, then exits.
hypr-comp/scripts/quickshell/watchers/network_wait.sh:15: # Exiting triggers the trap, immediately killing nmcli and ending the script.
hypr-comp/scripts/quickshell/watchers/network_wait.sh:16: grep -m 1 -iwE "connected|disconnected|enabled|disabled|activated|deactivated|available|unavailable" < "$PIPE" > /dev/null
```

### `hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh`

Migration owner: `sysstats`. Source `hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:1`, 81 lines, SHA-256 `2f705e82b42211063da9994b5ae84d69636e41c8bb65434f87b9943a3c6fa439`.

```text
…
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:3: # 1. Read initial values for time-sensitive metrics (CPU and Network)
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:4: read -r _ u1 n1 s1 i1 io1 ir1 so1 st1 g1 gn1 <<< "$(grep '^cpu ' /proc/stat)"
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:5: read rx1 tx1 <<< "$(awk -v IGNORECASE=1 '/^ *[ew]/{rx+=$2; tx+=$10} END{print rx, tx}' /proc/net/dev)"
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:6: 
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:7: # 2. Small delay to calculate precise usage deltas
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:8: sleep 0.5
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:9: 
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:10: # 3. Read final values
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:11: read -r _ u2 n2 s2 i2 io2 ir2 so2 st2 g2 gn2 <<< "$(grep '^cpu ' /proc/stat)"
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:12: read rx2 tx2 <<< "$(awk -v IGNORECASE=1 '/^ *[ew]/{rx+=$2; tx+=$10} END{print rx, tx}' /proc/net/dev)"
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:13: 
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:14: # --- CPU Calculation ---
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:15: IDLE1=$i1; TOTAL1=$((u1 + n1 + s1 + i1 + io1 + ir1 + so1 + st1))
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:16: IDLE2=$i2; TOTAL2=$((u2 + n2 + s2 + i2 + io2 + ir2 + so2 + st2))
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:17: DIFF_IDLE=$((IDLE2 - IDLE1))
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:18: DIFF_TOTAL=$((TOTAL2 - TOTAL1))
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:19: if [ "$DIFF_TOTAL" -eq 0 ]; then CPU_USAGE=0; else CPU_USAGE=$(( 100 * (DIFF_TOTAL - DIFF_IDLE) / DIFF_TOTAL )); fi
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:20: 
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:21: # --- Network Calculation ---
…
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:27: while IFS=":" read -r key val; do
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:28:     case "$key" in
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:29:         MemTotal) TOTAL_MEM=$(echo "$val" | awk '{print $1}') ;;
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:30:         MemAvailable) AVAIL_MEM=$(echo "$val" | awk '{print $1}') ;;
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:31:     esac
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:32: done < /proc/meminfo
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:33: USED_MEM=$((TOTAL_MEM - AVAIL_MEM))
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:34: RAM_PCT=$(( 100 * USED_MEM / TOTAL_MEM ))
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:35: RAM_GB=$(awk "BEGIN {printf \"%.1f\", $USED_MEM / 1024 / 1024}")
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:36: 
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:37: # --- Temperature Calculation ---
…
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:40: # Attempt 1: Check hwmon for known CPU temperature drivers (Intel, AMD, ARM)
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:41: for hwmon in /sys/class/hwmon/hwmon*; do
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:42:     if [ -f "$hwmon/name" ]; then
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:43:         hwmon_name=$(cat "$hwmon/name" 2>/dev/null)
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:44:         if [[ "$hwmon_name" =~ ^(coretemp|k10temp|zenpower|cpu_thermal|bcm2835_thermal)$ ]]; then
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:45:             # Usually temp1_input is the main package/die temp
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:46:             if [ -f "$hwmon/temp1_input" ]; then
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:47:                 TEMP_RAW=$(cat "$hwmon/temp1_input" 2>/dev/null)
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:48:                 break
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:49:             fi
…
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:54: # Attempt 2: Fallback to thermal_zone for known CPU identifiers
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:55: if [ -z "$TEMP_RAW" ]; then
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:56:     for tz in /sys/class/thermal/thermal_zone*; do
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:57:         if [ -f "$tz/type" ]; then
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:58:             tz_type=$(cat "$tz/type" 2>/dev/null)
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:59:             if [[ "$tz_type" =~ ^(x86_pkg_temp|cpu_thermal|cpu-thermal)$ ]]; then
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:60:                 TEMP_RAW=$(cat "$tz/temp" 2>/dev/null)
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:61:                 break
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:62:             fi
…
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:67: # Attempt 3: Ultimate fallback to the first available hardware monitor or thermal zone
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:68: if [ -z "$TEMP_RAW" ]; then
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:69:     TEMP_RAW=$(cat /sys/class/hwmon/hwmon0/temp1_input 2>/dev/null || cat /sys/class/thermal/thermal_zone0/temp 2>/dev/null || echo 0)
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:70: fi
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:71: 
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:72: # Normalize to degrees Celsius
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:73: if [ "$TEMP_RAW" -gt 1000 ]; then
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:74:     TEMP=$((TEMP_RAW / 1000))
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:75: else
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:76:     TEMP=$TEMP_RAW
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:77: fi
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:78: 
…
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:80: # Format: CPU|RAM_PCT|RAM_GB|TEMP|RX_RATE|TX_RATE
hypr-comp/scripts/quickshell/watchers/sys_fetcher.sh:81: echo "$CPU_USAGE|$RAM_PCT|$RAM_GB|$TEMP|$RX_RATE|$TX_RATE"
```

### `hypr-comp/scripts/stop.sh`

Migration owner: `applications/lifecycle`. Source `hypr-comp/scripts/stop.sh:1`, 5 lines, SHA-256 `0a003c43538ed3714c51145ee2df6f9c9a689dc8d9bdd329724b512bcc816640`.

```text
hypr-comp/scripts/stop.sh:1: #!/usr/bin/env bash
hypr-comp/scripts/stop.sh:2: set -euo pipefail
hypr-comp/scripts/stop.sh:3: 
hypr-comp/scripts/stop.sh:4: config_dir="$HOME/.config/HyprV/quickshell/hypr-comp"
hypr-comp/scripts/stop.sh:5: quickshell kill -p "$config_dir" >/dev/null 2>&1 || true
```

### `hypr-comp/scripts/weather.sh`

Migration owner: `weather`. Source `hypr-comp/scripts/weather.sh:1`, 5 lines, SHA-256 `bf0d51ccbed4763f893792c1ca9ae94df7b5d1d961b5541b180ebfe214fd2d77`.

```text
hypr-comp/scripts/weather.sh:1: #!/usr/bin/env bash
hypr-comp/scripts/weather.sh:2: set -euo pipefail
hypr-comp/scripts/weather.sh:3: 
hypr-comp/scripts/weather.sh:4: script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
hypr-comp/scripts/weather.sh:5: exec "$script_dir/quickshell/calendar/weather.sh" "$@"
```

### `hypr-comp/scripts/workspaces.sh`

Migration owner: `hyprland`. Source `hypr-comp/scripts/workspaces.sh:1`, 108 lines, SHA-256 `014e52f1611edffad0ccff8c934446cff4c3f48b65969907830fad2bb332ea57`.

```text
…
hypr-comp/scripts/workspaces.sh:14: script_self="$(realpath "$0")"
hypr-comp/scripts/workspaces.sh:15: for pid in $(pgrep -f "$script_self"); do
hypr-comp/scripts/workspaces.sh:16:     if [ "$pid" != "$$" ] && [ "$pid" != "$PPID" ]; then
hypr-comp/scripts/workspaces.sh:17:         kill -9 "$pid" 2>/dev/null
hypr-comp/scripts/workspaces.sh:18:     fi
hypr-comp/scripts/workspaces.sh:19: done
…
hypr-comp/scripts/workspaces.sh:21: # Cleanly kill immediate children (like socat) when the script exits normally
hypr-comp/scripts/workspaces.sh:22: cleanup() {
hypr-comp/scripts/workspaces.sh:23:     pkill -P $$ 2>/dev/null
hypr-comp/scripts/workspaces.sh:24: }
hypr-comp/scripts/workspaces.sh:25: trap cleanup EXIT SIGTERM SIGINT
…
hypr-comp/scripts/workspaces.sh:28: # The network toggle starts a background bluetooth scan that must be killed explicitly.
hypr-comp/scripts/workspaces.sh:29: BT_PID_FILE="$QS_RUN_WORKSPACES/bt_scan_pid"
hypr-comp/scripts/workspaces.sh:30: 
hypr-comp/scripts/workspaces.sh:31: if [ -f "$BT_PID_FILE" ]; then
hypr-comp/scripts/workspaces.sh:32:     kill $(cat "$BT_PID_FILE") 2>/dev/null
hypr-comp/scripts/workspaces.sh:33:     rm -f "$BT_PID_FILE"
hypr-comp/scripts/workspaces.sh:34: fi
hypr-comp/scripts/workspaces.sh:35: 
hypr-comp/scripts/workspaces.sh:36: # Ensure bluetooth scan is explicitly turned off (timeout prevents deadlocks on fresh installs)
hypr-comp/scripts/workspaces.sh:37: (timeout 2 bluetoothctl scan off > /dev/null 2>&1) &
hypr-comp/scripts/workspaces.sh:38: # ---------------------------------------------
hypr-comp/scripts/workspaces.sh:39: 
hypr-comp/scripts/workspaces.sh:40: # Configuration: Parse from settings.json dynamically, fallback to 8
hypr-comp/scripts/workspaces.sh:41: SETTINGS_FILE="$HOME/.config/HyprV/quickshell/hypr-comp/settings.json"
hypr-comp/scripts/workspaces.sh:42: SEQ_END=$(jq -r '.workspaceCount // 8' "$SETTINGS_FILE" 2>/dev/null)
hypr-comp/scripts/workspaces.sh:43: # Double check it is a valid integer to prevent jq errors later
hypr-comp/scripts/workspaces.sh:44: if ! [[ "$SEQ_END" =~ ^[0-9]+$ ]]; then
hypr-comp/scripts/workspaces.sh:45:     SEQ_END=8
hypr-comp/scripts/workspaces.sh:46: fi
hypr-comp/scripts/workspaces.sh:47: 
hypr-comp/scripts/workspaces.sh:48: print_workspaces() {
hypr-comp/scripts/workspaces.sh:49:     # Get raw data with a timeout fallback
hypr-comp/scripts/workspaces.sh:50:     spaces=$(timeout 2 hyprctl workspaces -j 2>/dev/null)
hypr-comp/scripts/workspaces.sh:51:     active=$(timeout 2 hyprctl activeworkspace -j 2>/dev/null | jq '.id')
hypr-comp/scripts/workspaces.sh:52: 
hypr-comp/scripts/workspaces.sh:53:     # Failsafe if hyprctl crashes to prevent jq from outputting errors
hypr-comp/scripts/workspaces.sh:54:     if [ -z "$spaces" ] || [ -z "$active" ]; then return; fi
hypr-comp/scripts/workspaces.sh:55: 
hypr-comp/scripts/workspaces.sh:56:     # Generate the JSON and write it atomically to prevent UI flickering
hypr-comp/scripts/workspaces.sh:57:     echo "$spaces" | jq --unbuffered --argjson a "$active" --arg end "$SEQ_END" -c '
hypr-comp/scripts/workspaces.sh:58:         # Create a map of workspace ID -> workspace data for easy lookup
hypr-comp/scripts/workspaces.sh:59:         (map( { (.id|tostring): . } ) | add) as $s
hypr-comp/scripts/workspaces.sh:60:         |
hypr-comp/scripts/workspaces.sh:61:         # Iterate from 1 to SEQ_END
hypr-comp/scripts/workspaces.sh:62:         [range(1; ($end|tonumber) + 1)] | map(
hypr-comp/scripts/workspaces.sh:63:             . as $i |
hypr-comp/scripts/workspaces.sh:64:             # Determine state: active -> occupied -> empty
hypr-comp/scripts/workspaces.sh:65:             (if $i == $a then "active"
hypr-comp/scripts/workspaces.sh:66:              elif ($s[$i|tostring] != null and $s[$i|tostring].windows > 0) then "occupied"
hypr-comp/scripts/workspaces.sh:67:              else "empty" end) as $state |
hypr-comp/scripts/workspaces.sh:68: 
hypr-comp/scripts/workspaces.sh:69:             # Get window title for tooltip (if exists)
hypr-comp/scripts/workspaces.sh:70:             (if $s[$i|tostring] != null then $s[$i|tostring].lastwindowtitle else "Empty" end) as $win |
hypr-comp/scripts/workspaces.sh:71: 
hypr-comp/scripts/workspaces.sh:72:             {
hypr-comp/scripts/workspaces.sh:73:                 id: $i,
hypr-comp/scripts/workspaces.sh:74:                 state: $state,
hypr-comp/scripts/workspaces.sh:75:                 tooltip: $win
hypr-comp/scripts/workspaces.sh:76:             }
hypr-comp/scripts/workspaces.sh:77:         )
hypr-comp/scripts/workspaces.sh:78:     ' > "$QS_RUN_WORKSPACES/workspaces.tmp"
hypr-comp/scripts/workspaces.sh:79:     
hypr-comp/scripts/workspaces.sh:80:     mv "$QS_RUN_WORKSPACES/workspaces.tmp" "$QS_RUN_WORKSPACES/workspaces.json"
hypr-comp/scripts/workspaces.sh:81: }
hypr-comp/scripts/workspaces.sh:82: 
…
hypr-comp/scripts/workspaces.sh:90: while true; do
hypr-comp/scripts/workspaces.sh:91:     socat -u UNIX-CONNECT:$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock - | while read -r line; do
hypr-comp/scripts/workspaces.sh:92:         case "$line" in
hypr-comp/scripts/workspaces.sh:93:             workspace*|focusedmon*|activewindow*|createwindow*|closewindow*|movewindow*|destroyworkspace*)
hypr-comp/scripts/workspaces.sh:94:                 
hypr-comp/scripts/workspaces.sh:95:                 # -> THE FIX <-
…
hypr-comp/scripts/workspaces.sh:106:     done
hypr-comp/scripts/workspaces.sh:107:     sleep 1
hypr-comp/scripts/workspaces.sh:108: done
```

### `scripts/active-window-right-edge.sh`

Migration owner: `hyprland`. Source `scripts/active-window-right-edge.sh:1`, 28 lines, SHA-256 `a52ce4dd3291b1b2b7b296e60ac5c9821bbd25243f99ba329c5c3433729e0637`.

```text
scripts/active-window-right-edge.sh:1: #!/usr/bin/env bash
scripts/active-window-right-edge.sh:2: set -euo pipefail
scripts/active-window-right-edge.sh:3: 
scripts/active-window-right-edge.sh:4: command -v hyprctl >/dev/null 2>&1 || exit 1
scripts/active-window-right-edge.sh:5: command -v jq >/dev/null 2>&1 || exit 1
scripts/active-window-right-edge.sh:6: 
scripts/active-window-right-edge.sh:7: active="$(hyprctl activewindow -j 2>/dev/null || true)"
scripts/active-window-right-edge.sh:8: monitors="$(hyprctl monitors -j 2>/dev/null || true)"
scripts/active-window-right-edge.sh:9: 
scripts/active-window-right-edge.sh:10: if [[ -z "$active" || -z "$monitors" ]]; then
scripts/active-window-right-edge.sh:11:     exit 1
scripts/active-window-right-edge.sh:12: fi
scripts/active-window-right-edge.sh:13: 
scripts/active-window-right-edge.sh:14: jq -nr -e --argjson active "$active" --argjson monitors "$monitors" '
scripts/active-window-right-edge.sh:15:     ($active.monitor // null) as $monitorId
scripts/active-window-right-edge.sh:16:     | ($active.at[0] // null) as $windowX
scripts/active-window-right-edge.sh:17:     | ($active.size[0] // null) as $windowWidth
scripts/active-window-right-edge.sh:18:     | if $monitorId == null or $windowX == null or $windowWidth == null or $windowWidth <= 0 then
scripts/active-window-right-edge.sh:19:         empty
scripts/active-window-right-edge.sh:20:       else
scripts/active-window-right-edge.sh:21:         ($monitors | map(select((.id // -999999) == $monitorId)) | first) as $monitor
scripts/active-window-right-edge.sh:22:         | if $monitor == null then
scripts/active-window-right-edge.sh:23:             empty
scripts/active-window-right-edge.sh:24:           else
scripts/active-window-right-edge.sh:25:             (($windowX - ($monitor.x // 0)) + $windowWidth | round)
scripts/active-window-right-edge.sh:26:           end
scripts/active-window-right-edge.sh:27:       end
scripts/active-window-right-edge.sh:28: '
```

### `scripts/agent-island-action.py`

Migration owner: `agent_island`. Source `scripts/agent-island-action.py:1`, 147 lines, SHA-256 `0ece43bbaabfff55d94d2ea0f0cbe215d9c06309c3558a41d5e3c5c38611f32f`.

```text
…
scripts/agent-island-action.py:7: from pathlib import Path
scripts/agent-island-action.py:8: import subprocess
scripts/agent-island-action.py:9: 
scripts/agent-island-action.py:10: from agent_island import is_agent_focus_client, locked_state
…
scripts/agent-island-action.py:12: 
scripts/agent-island-action.py:13: def hypr_clients() -> list[dict]:
scripts/agent-island-action.py:14:     try:
scripts/agent-island-action.py:15:         raw = subprocess.run(
scripts/agent-island-action.py:16:             ["hyprctl", "clients", "-j"],
scripts/agent-island-action.py:17:             check=False,
scripts/agent-island-action.py:18:             stdout=subprocess.PIPE,
scripts/agent-island-action.py:19:             stderr=subprocess.DEVNULL,
scripts/agent-island-action.py:20:             text=True,
scripts/agent-island-action.py:21:             timeout=0.7,
scripts/agent-island-action.py:22:         ).stdout
scripts/agent-island-action.py:23:         data = json.loads(raw)
scripts/agent-island-action.py:24:     except Exception:
scripts/agent-island-action.py:25:         return []
…
scripts/agent-island-action.py:28: 
scripts/agent-island-action.py:29: def focus_address(address: str) -> None:
scripts/agent-island-action.py:30:     subprocess.run(["hyprctl", "dispatch", "focuswindow", f"address:{address}"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
scripts/agent-island-action.py:31: 
scripts/agent-island-action.py:32: 
scripts/agent-island-action.py:33: def kitty_focus_session(session: dict) -> bool:
scripts/agent-island-action.py:34:     terminal = session.get("terminal") if isinstance(session.get("terminal"), dict) else {}
scripts/agent-island-action.py:35:     kitty_window = terminal.get("kitty_window")
scripts/agent-island-action.py:36:     if not isinstance(kitty_window, str) or not kitty_window.strip():
scripts/agent-island-action.py:37:         return False
scripts/agent-island-action.py:38:     listen_on = terminal.get("kitty_listen_on")
scripts/agent-island-action.py:39:     if not isinstance(listen_on, str) or not listen_on.strip():
scripts/agent-island-action.py:40:         return False
…
scripts/agent-island-action.py:42:     try:
scripts/agent-island-action.py:43:         proc = subprocess.run(
scripts/agent-island-action.py:44:             [
scripts/agent-island-action.py:45:                 "kitty",
…
scripts/agent-island-action.py:52:             ],
scripts/agent-island-action.py:53:             stdout=subprocess.DEVNULL,
scripts/agent-island-action.py:54:             stderr=subprocess.DEVNULL,
scripts/agent-island-action.py:55:             timeout=0.8,
scripts/agent-island-action.py:56:         )
scripts/agent-island-action.py:57:     except Exception:
…
scripts/agent-island-action.py:61: 
scripts/agent-island-action.py:62: def session_terms(session: dict) -> list[str]:
scripts/agent-island-action.py:63:     terms: list[str] = []
scripts/agent-island-action.py:64:     for value in (
scripts/agent-island-action.py:65:         session.get("cwd"),
scripts/agent-island-action.py:66:         session.get("project"),
scripts/agent-island-action.py:67:         session.get("title"),
scripts/agent-island-action.py:68:         session.get("source_label"),
scripts/agent-island-action.py:69:         session.get("source"),
scripts/agent-island-action.py:70:     ):
scripts/agent-island-action.py:71:         if isinstance(value, str) and value.strip():
scripts/agent-island-action.py:72:             terms.append(value.strip())
scripts/agent-island-action.py:73:     cwd = session.get("cwd")
scripts/agent-island-action.py:74:     if isinstance(cwd, str) and cwd.strip():
scripts/agent-island-action.py:75:         name = Path(cwd).name
…
scripts/agent-island-action.py:88: 
scripts/agent-island-action.py:89: def client_score(client: dict, terms: list[str]) -> int:
scripts/agent-island-action.py:90:     if not is_agent_focus_client(client):
scripts/agent-island-action.py:91:         return -1
scripts/agent-island-action.py:92:     title = str(client.get("title") or client.get("initialTitle") or "").lower()
scripts/agent-island-action.py:93:     class_text = f"{client.get('class') or ''} {client.get('initialClass') or ''}".lower()
scripts/agent-island-action.py:94:     score = 5
scripts/agent-island-action.py:95:     if any(term in class_text for term in ("ghostty", "alacritty", "kitty", "wezterm", "foot", "konsole", "terminal")):
…
scripts/agent-island-action.py:107: 
scripts/agent-island-action.py:108: def focus_session(session: dict) -> int:
scripts/agent-island-action.py:109:     window = ((session.get("terminal") or {}).get("window") or {}) if isinstance(session.get("terminal"), dict) else {}
scripts/agent-island-action.py:110:     clients = hypr_clients()
scripts/agent-island-action.py:111:     address = window.get("address")
scripts/agent-island-action.py:112:     if isinstance(address, str) and address:
scripts/agent-island-action.py:113:         matched = next((client for client in clients if client.get("address") == address), None)
scripts/agent-island-action.py:114:         if is_agent_focus_client(matched):
scripts/agent-island-action.py:115:             focus_address(address)
…
scripts/agent-island-action.py:123:     scored = [(client_score(client, terms), client) for client in clients]
scripts/agent-island-action.py:124:     scored = [(score, client) for score, client in scored if score >= 25 and isinstance(client.get("address"), str)]
scripts/agent-island-action.py:125:     scored.sort(key=lambda item: item[0], reverse=True)
scripts/agent-island-action.py:126:     if scored:
…
scripts/agent-island-action.py:132: 
scripts/agent-island-action.py:133: def main() -> int:
scripts/agent-island-action.py:134:     parser = argparse.ArgumentParser()
scripts/agent-island-action.py:135:     parser.add_argument("action", choices=["focus"])
scripts/agent-island-action.py:136:     parser.add_argument("id")
scripts/agent-island-action.py:137:     args = parser.parse_args()
scripts/agent-island-action.py:138: 
…
scripts/agent-island-action.py:140:     with locked_state(write=False) as state:
scripts/agent-island-action.py:141:         session = state.get("sessions", {}).get(args.id)
scripts/agent-island-action.py:142:     return focus_session(session) if isinstance(session, dict) else 1
scripts/agent-island-action.py:143: 
…
scripts/agent-island-action.py:145: 
scripts/agent-island-action.py:146: if __name__ == "__main__":
scripts/agent-island-action.py:147:     raise SystemExit(main())
```

### `scripts/agent-island-codex-appserver.py`

Migration owner: `agent_island`. Source `scripts/agent-island-codex-appserver.py:1`, 161 lines, SHA-256 `93ab3718254cd0b4aadf64e13c95b82a2ffe136d62defb033a974f53de8671d2`.

```text
…
scripts/agent-island-codex-appserver.py:6: import shutil
scripts/agent-island-codex-appserver.py:7: import subprocess
scripts/agent-island-codex-appserver.py:8: import sys
scripts/agent-island-codex-appserver.py:9: import time
…
scripts/agent-island-codex-appserver.py:18: 
scripts/agent-island-codex-appserver.py:19: def status_from_thread_status(status: dict[str, Any] | None) -> tuple[str, str, float]:
scripts/agent-island-codex-appserver.py:20:     if not isinstance(status, dict):
scripts/agent-island-codex-appserver.py:21:         return "processing", "Starting", 0.2
scripts/agent-island-codex-appserver.py:22:     kind = status.get("type")
scripts/agent-island-codex-appserver.py:23:     if kind == "active":
scripts/agent-island-codex-appserver.py:24:         flags = status.get("activeFlags")
scripts/agent-island-codex-appserver.py:25:         flags = flags if isinstance(flags, list) else []
scripts/agent-island-codex-appserver.py:26:         if "waitingOnApproval" in flags:
…
scripts/agent-island-codex-appserver.py:30:         return "running", "Running", 0.46
scripts/agent-island-codex-appserver.py:31:     if kind == "idle":
scripts/agent-island-codex-appserver.py:32:         return "completed", "Idle", 1.0
scripts/agent-island-codex-appserver.py:33:     if kind in ("systemError", "notLoaded"):
…
scripts/agent-island-codex-appserver.py:37: 
scripts/agent-island-codex-appserver.py:38: def update_thread(state: dict[str, Any], thread: dict[str, Any]) -> None:
scripts/agent-island-codex-appserver.py:39:     thread_id = thread.get("id")
scripts/agent-island-codex-appserver.py:40:     if not isinstance(thread_id, str) or not thread_id:
scripts/agent-island-codex-appserver.py:41:         return
…
scripts/agent-island-codex-appserver.py:43:     now = time.time()
scripts/agent-island-codex-appserver.py:44:     status, detail, progress = status_from_thread_status(thread.get("status"))
scripts/agent-island-codex-appserver.py:45:     sessions = state.setdefault("sessions", {})
scripts/agent-island-codex-appserver.py:46:     session = sessions.get(session_id) if isinstance(sessions.get(session_id), dict) else {}
scripts/agent-island-codex-appserver.py:47:     session.setdefault("id", session_id)
scripts/agent-island-codex-appserver.py:48:     session["source"] = "codex"
…
scripts/agent-island-codex-appserver.py:51:     session["provider_session_id"] = thread_id
scripts/agent-island-codex-appserver.py:52:     session["title"] = thread.get("name") if isinstance(thread.get("name"), str) and thread.get("name") else thread_id[:8]
scripts/agent-island-codex-appserver.py:53:     cwd = thread.get("cwd") if isinstance(thread.get("cwd"), str) else session.get("cwd") or ""
scripts/agent-island-codex-appserver.py:54:     session["cwd"] = cwd
scripts/agent-island-codex-appserver.py:55:     session["project"] = project_name(cwd)
scripts/agent-island-codex-appserver.py:56:     if isinstance(thread.get("preview"), str) and thread["preview"]:
scripts/agent-island-codex-appserver.py:57:         session["detail"] = thread["preview"]
scripts/agent-island-codex-appserver.py:58:     else:
scripts/agent-island-codex-appserver.py:59:         session["detail"] = detail
scripts/agent-island-codex-appserver.py:60:     if isinstance(thread.get("path"), str) and thread["path"]:
scripts/agent-island-codex-appserver.py:61:         session["transcript_path"] = thread["path"]
scripts/agent-island-codex-appserver.py:62:     session["status"] = status
…
scripts/agent-island-codex-appserver.py:66:     session.setdefault("started_at", now)
scripts/agent-island-codex-appserver.py:67:     if status == "completed":
scripts/agent-island-codex-appserver.py:68:         session["completed_at"] = now
scripts/agent-island-codex-appserver.py:69:     sessions[session_id] = clean_for_json(session)
…
scripts/agent-island-codex-appserver.py:71: 
scripts/agent-island-codex-appserver.py:72: def update_thread_status(state: dict[str, Any], thread_id: str, status_payload: dict[str, Any] | None) -> None:
scripts/agent-island-codex-appserver.py:73:     session_id = APP_PREFIX + thread_id
scripts/agent-island-codex-appserver.py:74:     session = state.setdefault("sessions", {}).get(session_id)
scripts/agent-island-codex-appserver.py:75:     if not isinstance(session, dict):
scripts/agent-island-codex-appserver.py:76:         return
…
scripts/agent-island-codex-appserver.py:81:     session["last_activity"] = time.time()
scripts/agent-island-codex-appserver.py:82:     if status == "completed":
scripts/agent-island-codex-appserver.py:83:         session["completed_at"] = time.time()
scripts/agent-island-codex-appserver.py:84: 
scripts/agent-island-codex-appserver.py:85: 
scripts/agent-island-codex-appserver.py:86: def close_thread(state: dict[str, Any], thread_id: str) -> None:
scripts/agent-island-codex-appserver.py:87:     state.setdefault("sessions", {}).pop(APP_PREFIX + thread_id, None)
scripts/agent-island-codex-appserver.py:88: 
scripts/agent-island-codex-appserver.py:89: 
scripts/agent-island-codex-appserver.py:90: def handle_message(message: dict[str, Any]) -> None:
scripts/agent-island-codex-appserver.py:91:     method = message.get("method")
scripts/agent-island-codex-appserver.py:92:     params = message.get("params")
scripts/agent-island-codex-appserver.py:93:     if not isinstance(method, str) or not isinstance(params, dict):
scripts/agent-island-codex-appserver.py:94:         return
…
scripts/agent-island-codex-appserver.py:96:         if method == "thread/started":
scripts/agent-island-codex-appserver.py:97:             thread = params.get("thread")
scripts/agent-island-codex-appserver.py:98:             if isinstance(thread, dict):
scripts/agent-island-codex-appserver.py:99:                 update_thread(state, thread)
scripts/agent-island-codex-appserver.py:100:         elif method == "thread/status/changed":
scripts/agent-island-codex-appserver.py:101:             thread_id = params.get("threadId")
scripts/agent-island-codex-appserver.py:102:             if isinstance(thread_id, str):
scripts/agent-island-codex-appserver.py:103:                 status = params.get("status")
scripts/agent-island-codex-appserver.py:104:                 update_thread_status(state, thread_id, status if isinstance(status, dict) else None)
scripts/agent-island-codex-appserver.py:105:         elif method == "thread/closed":
scripts/agent-island-codex-appserver.py:106:             thread_id = params.get("threadId")
scripts/agent-island-codex-appserver.py:107:             if isinstance(thread_id, str):
scripts/agent-island-codex-appserver.py:108:                 close_thread(state, thread_id)
…
scripts/agent-island-codex-appserver.py:110: 
scripts/agent-island-codex-appserver.py:111: def run_once(codex_path: str) -> int:
scripts/agent-island-codex-appserver.py:112:     proc = subprocess.Popen(
scripts/agent-island-codex-appserver.py:113:         [codex_path, "app-server", "--listen", "stdio://"],
scripts/agent-island-codex-appserver.py:114:         stdin=subprocess.PIPE,
scripts/agent-island-codex-appserver.py:115:         stdout=subprocess.PIPE,
scripts/agent-island-codex-appserver.py:116:         stderr=subprocess.DEVNULL,
scripts/agent-island-codex-appserver.py:117:         text=True,
scripts/agent-island-codex-appserver.py:118:         bufsize=1,
…
scripts/agent-island-codex-appserver.py:122:     init = {
scripts/agent-island-codex-appserver.py:123:         "jsonrpc": "2.0",
scripts/agent-island-codex-appserver.py:124:         "id": 1,
scripts/agent-island-codex-appserver.py:125:         "method": "initialize",
scripts/agent-island-codex-appserver.py:126:         "params": {
scripts/agent-island-codex-appserver.py:127:             "clientInfo": {"name": "HyprV", "version": "agent-island"},
scripts/agent-island-codex-appserver.py:128:             "capabilities": None,
scripts/agent-island-codex-appserver.py:129:         },
scripts/agent-island-codex-appserver.py:130:     }
scripts/agent-island-codex-appserver.py:131:     proc.stdin.write(json.dumps(init, separators=(",", ":")) + "\n")
scripts/agent-island-codex-appserver.py:132:     proc.stdin.flush()
scripts/agent-island-codex-appserver.py:133:     for line in proc.stdout:
…
scripts/agent-island-codex-appserver.py:137:         try:
scripts/agent-island-codex-appserver.py:138:             message = json.loads(line)
scripts/agent-island-codex-appserver.py:139:         except Exception:
scripts/agent-island-codex-appserver.py:140:             continue
…
scripts/agent-island-codex-appserver.py:145: 
scripts/agent-island-codex-appserver.py:146: def main() -> int:
scripts/agent-island-codex-appserver.py:147:     codex_path = shutil.which("codex")
scripts/agent-island-codex-appserver.py:148:     if not codex_path:
scripts/agent-island-codex-appserver.py:149:         return 0
scripts/agent-island-codex-appserver.py:150:     while True:
…
scripts/agent-island-codex-appserver.py:155:         except Exception as exc:
scripts/agent-island-codex-appserver.py:156:             print(f"agent-island-codex-appserver: {exc}", file=sys.stderr)
scripts/agent-island-codex-appserver.py:157:         time.sleep(4)
scripts/agent-island-codex-appserver.py:158: 
scripts/agent-island-codex-appserver.py:159: 
scripts/agent-island-codex-appserver.py:160: if __name__ == "__main__":
scripts/agent-island-codex-appserver.py:161:     raise SystemExit(main())
```

### `scripts/agent-island-hook.py`

Migration owner: `agent_island`. Source `scripts/agent-island-hook.py:1`, 66 lines, SHA-256 `b06b5e11d5ba72c084725e60b0b73b5d940db267a5d6a2b267b8b166398f9d8a`.

```text
…
scripts/agent-island-hook.py:19: 
scripts/agent-island-hook.py:20: def parse_payload() -> dict:
scripts/agent-island-hook.py:21:     raw = sys.stdin.buffer.read()
scripts/agent-island-hook.py:22:     if not raw:
scripts/agent-island-hook.py:23:         return {}
scripts/agent-island-hook.py:24:     try:
scripts/agent-island-hook.py:25:         payload = json.loads(raw.decode("utf-8"))
scripts/agent-island-hook.py:26:     except Exception:
scripts/agent-island-hook.py:27:         return {}
…
scripts/agent-island-hook.py:30: 
scripts/agent-island-hook.py:31: def main() -> int:
scripts/agent-island-hook.py:32:     parser = argparse.ArgumentParser()
scripts/agent-island-hook.py:33:     parser.add_argument("--source", required=True)
scripts/agent-island-hook.py:34:     parser.add_argument("--event")
scripts/agent-island-hook.py:35:     args = parser.parse_args()
scripts/agent-island-hook.py:36: 
…
scripts/agent-island-hook.py:58:         if request:
scripts/agent-island-hook.py:59:             state.get("pending_requests", {}).pop(request["id"], None)
scripts/agent-island-hook.py:60:             state["sessions"][_session["id"]].pop("pending_request_id", None)
scripts/agent-island-hook.py:61: 
…
scripts/agent-island-hook.py:64: 
scripts/agent-island-hook.py:65: if __name__ == "__main__":
scripts/agent-island-hook.py:66:     raise SystemExit(main())
```

### `scripts/agent-island-install-hooks.py`

Migration owner: `agent_island`. Source `scripts/agent-island-install-hooks.py:1`, 180 lines, SHA-256 `03d7211d99903f5b77171d69a6359610a415baa051e031177f48b3de9e04ece7`.

```text
…
scripts/agent-island-install-hooks.py:10: 
scripts/agent-island-install-hooks.py:11: CONFIG_DIR = Path(os.environ.get("HYPRV_CONFIG_DIR", Path.home() / ".config" / "HyprV")).expanduser()
scripts/agent-island-install-hooks.py:12: HOOK = CONFIG_DIR / "quickshell" / "scripts" / "agent-island-hook.py"
scripts/agent-island-install-hooks.py:13: PYTHON = "/usr/bin/python3"
…
scripts/agent-island-install-hooks.py:16: CLAUDE_EVENTS = {
scripts/agent-island-install-hooks.py:17:     "UserPromptSubmit": 5,
scripts/agent-island-install-hooks.py:18:     "PreToolUse": 5,
scripts/agent-island-install-hooks.py:19:     "PostToolUse": 5,
scripts/agent-island-install-hooks.py:20:     "PostToolUseFailure": 5,
scripts/agent-island-install-hooks.py:21:     "PermissionRequest": 5,
scripts/agent-island-install-hooks.py:22:     "Stop": 5,
scripts/agent-island-install-hooks.py:23:     "SubagentStart": 5,
scripts/agent-island-install-hooks.py:24:     "SubagentStop": 5,
scripts/agent-island-install-hooks.py:25:     "SessionStart": 5,
scripts/agent-island-install-hooks.py:26:     "SessionEnd": 5,
scripts/agent-island-install-hooks.py:27:     "Notification": 5,
scripts/agent-island-install-hooks.py:28:     "PreCompact": 5,
scripts/agent-island-install-hooks.py:29: }
scripts/agent-island-install-hooks.py:30: 
scripts/agent-island-install-hooks.py:31: CODEX_EVENTS = {
scripts/agent-island-install-hooks.py:32:     "SessionStart": 5,
scripts/agent-island-install-hooks.py:33:     "SessionEnd": 5,
scripts/agent-island-install-hooks.py:34:     "UserPromptSubmit": 5,
scripts/agent-island-install-hooks.py:35:     "PreToolUse": 5,
scripts/agent-island-install-hooks.py:36:     "PostToolUse": 5,
scripts/agent-island-install-hooks.py:37:     "Stop": 5,
scripts/agent-island-install-hooks.py:38: }
scripts/agent-island-install-hooks.py:39: 
…
scripts/agent-island-install-hooks.py:42: 
scripts/agent-island-install-hooks.py:43: def load_json(path: Path) -> dict:
scripts/agent-island-install-hooks.py:44:     if not path.exists():
scripts/agent-island-install-hooks.py:45:         return {}
scripts/agent-island-install-hooks.py:46:     try:
scripts/agent-island-install-hooks.py:47:         data = json.loads(path.read_text(encoding="utf-8"))
scripts/agent-island-install-hooks.py:48:     except Exception:
scripts/agent-island-install-hooks.py:49:         raise SystemExit(f"Cannot parse JSON: {path}")
…
scripts/agent-island-install-hooks.py:54: 
scripts/agent-island-install-hooks.py:55: def save_json(path: Path, data: dict) -> None:
scripts/agent-island-install-hooks.py:56:     path.parent.mkdir(parents=True, exist_ok=True)
scripts/agent-island-install-hooks.py:57:     path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
scripts/agent-island-install-hooks.py:58: 
scripts/agent-island-install-hooks.py:59: 
scripts/agent-island-install-hooks.py:60: def command_for(source: str) -> str:
scripts/agent-island-install-hooks.py:61:     return f"{PYTHON} {HOOK} --source {source}"
scripts/agent-island-install-hooks.py:62: 
scripts/agent-island-install-hooks.py:63: 
scripts/agent-island-install-hooks.py:64: def hook_list_for(source: str, timeout: int) -> list[dict]:
scripts/agent-island-install-hooks.py:65:     return [{"type": "command", "command": command_for(source), "timeout": timeout}]
scripts/agent-island-install-hooks.py:66: 
scripts/agent-island-install-hooks.py:67: 
scripts/agent-island-install-hooks.py:68: def remove_managed(entries: list) -> list:
scripts/agent-island-install-hooks.py:69:     kept = []
scripts/agent-island-install-hooks.py:70:     for entry in entries:
…
scripts/agent-island-install-hooks.py:73:             continue
scripts/agent-island-install-hooks.py:74:         hooks = entry.get("hooks")
scripts/agent-island-install-hooks.py:75:         if isinstance(hooks, list):
scripts/agent-island-install-hooks.py:76:             hooks = [
scripts/agent-island-install-hooks.py:77:                 hook for hook in hooks
scripts/agent-island-install-hooks.py:78:                 if not (isinstance(hook, dict) and MANAGED_MARKER in str(hook.get("command", "")))
scripts/agent-island-install-hooks.py:79:             ]
scripts/agent-island-install-hooks.py:80:             if hooks:
…
scripts/agent-island-install-hooks.py:84:             continue
scripts/agent-island-install-hooks.py:85:         if MANAGED_MARKER not in str(entry.get("command", "")):
scripts/agent-island-install-hooks.py:86:             kept.append(entry)
scripts/agent-island-install-hooks.py:87:     return kept
…
scripts/agent-island-install-hooks.py:89: 
scripts/agent-island-install-hooks.py:90: def install_claude() -> None:
scripts/agent-island-install-hooks.py:91:     path = Path.home() / ".claude" / "settings.json"
scripts/agent-island-install-hooks.py:92:     data = load_json(path)
scripts/agent-island-install-hooks.py:93:     hooks = data.get("hooks")
scripts/agent-island-install-hooks.py:94:     if not isinstance(hooks, dict):
scripts/agent-island-install-hooks.py:95:         hooks = {}
scripts/agent-island-install-hooks.py:96:     for event, timeout in CLAUDE_EVENTS.items():
scripts/agent-island-install-hooks.py:97:         entries = remove_managed(hooks.get(event, []) if isinstance(hooks.get(event), list) else [])
scripts/agent-island-install-hooks.py:98:         entries.append({
scripts/agent-island-install-hooks.py:99:             "matcher": "",
scripts/agent-island-install-hooks.py:100:             "hooks": hook_list_for("claude", timeout),
scripts/agent-island-install-hooks.py:101:         })
scripts/agent-island-install-hooks.py:102:         hooks[event] = entries
…
scripts/agent-island-install-hooks.py:106: 
scripts/agent-island-install-hooks.py:107: def install_codex_hooks() -> None:
scripts/agent-island-install-hooks.py:108:     path = Path(os.environ.get("CODEX_HOME", Path.home() / ".codex")).expanduser() / "hooks.json"
scripts/agent-island-install-hooks.py:109:     data = load_json(path)
scripts/agent-island-install-hooks.py:110:     hooks = data.get("hooks")
scripts/agent-island-install-hooks.py:111:     if not isinstance(hooks, dict):
scripts/agent-island-install-hooks.py:112:         hooks = {}
scripts/agent-island-install-hooks.py:113:     for event in CODEX_PASSTHROUGH_EVENTS:
scripts/agent-island-install-hooks.py:114:         entries = remove_managed(hooks.get(event, []) if isinstance(hooks.get(event), list) else [])
scripts/agent-island-install-hooks.py:115:         if entries:
scripts/agent-island-install-hooks.py:116:             hooks[event] = entries
…
scripts/agent-island-install-hooks.py:118:             hooks.pop(event, None)
scripts/agent-island-install-hooks.py:119:     for event, timeout in CODEX_EVENTS.items():
scripts/agent-island-install-hooks.py:120:         entries = remove_managed(hooks.get(event, []) if isinstance(hooks.get(event), list) else [])
scripts/agent-island-install-hooks.py:121:         entries.append({
scripts/agent-island-install-hooks.py:122:             "matcher": "*",
scripts/agent-island-install-hooks.py:123:             "hooks": hook_list_for("codex", timeout),
scripts/agent-island-install-hooks.py:124:             "statusMessage": "HyprV island",
scripts/agent-island-install-hooks.py:125:         })
scripts/agent-island-install-hooks.py:126:         hooks[event] = entries
…
scripts/agent-island-install-hooks.py:130: 
scripts/agent-island-install-hooks.py:131: def enable_codex_hooks_feature() -> None:
scripts/agent-island-install-hooks.py:132:     config_home = Path(os.environ.get("CODEX_HOME", Path.home() / ".codex")).expanduser()
scripts/agent-island-install-hooks.py:133:     path = config_home / "config.toml"
scripts/agent-island-install-hooks.py:134:     text = path.read_text(encoding="utf-8") if path.exists() else ""
scripts/agent-island-install-hooks.py:135:     lines = text.splitlines()
scripts/agent-island-install-hooks.py:136:     section_re = re.compile(r"^\s*\[([^\]]+)\]\s*$")
…
scripts/agent-island-install-hooks.py:142:             continue
scripts/agent-island-install-hooks.py:143:         if match.group(1).strip() == "features":
scripts/agent-island-install-hooks.py:144:             features_start = idx
scripts/agent-island-install-hooks.py:145:             features_end = len(lines)
…
scripts/agent-island-install-hooks.py:164:     path.parent.mkdir(parents=True, exist_ok=True)
scripts/agent-island-install-hooks.py:165:     path.write_text("\n".join(lines) + "\n", encoding="utf-8")
scripts/agent-island-install-hooks.py:166: 
scripts/agent-island-install-hooks.py:167: 
scripts/agent-island-install-hooks.py:168: def main() -> int:
scripts/agent-island-install-hooks.py:169:     if not HOOK.exists():
scripts/agent-island-install-hooks.py:170:         raise SystemExit(f"Missing hook script: {HOOK}")
scripts/agent-island-install-hooks.py:171:     install_claude()
scripts/agent-island-install-hooks.py:172:     install_codex_hooks()
scripts/agent-island-install-hooks.py:173:     enable_codex_hooks_feature()
scripts/agent-island-install-hooks.py:174:     print("installed=claude,codex")
scripts/agent-island-install-hooks.py:175:     print(f"hook={HOOK}")
scripts/agent-island-install-hooks.py:176:     return 0
scripts/agent-island-install-hooks.py:177: 
scripts/agent-island-install-hooks.py:178: 
scripts/agent-island-install-hooks.py:179: if __name__ == "__main__":
scripts/agent-island-install-hooks.py:180:     raise SystemExit(main())
```

### `scripts/agent-island-status.py`

Migration owner: `agent_island`. Source `scripts/agent-island-status.py:1`, 42 lines, SHA-256 `f646b5b71a3c8670d76b38f8bbaf5ab71c91f4d57d50e4600b8717060b0f1387`.

```text
scripts/agent-island-status.py:1: #!/usr/bin/env python3
scripts/agent-island-status.py:2: 
scripts/agent-island-status.py:3: from __future__ import annotations
scripts/agent-island-status.py:4: 
scripts/agent-island-status.py:5: import argparse
scripts/agent-island-status.py:6: import json
scripts/agent-island-status.py:7: import time
scripts/agent-island-status.py:8: 
scripts/agent-island-status.py:9: from agent_island import locked_state, public_snapshot
scripts/agent-island-status.py:10: 
scripts/agent-island-status.py:11: 
scripts/agent-island-status.py:12: def main() -> int:
scripts/agent-island-status.py:13:     parser = argparse.ArgumentParser()
scripts/agent-island-status.py:14:     parser.add_argument("--json", action="store_true")
scripts/agent-island-status.py:15:     parser.add_argument("--watch", action="store_true", help="Stream task state without restarting Python")
scripts/agent-island-status.py:16:     args = parser.parse_args()
scripts/agent-island-status.py:17: 
scripts/agent-island-status.py:18:     previous = None
scripts/agent-island-status.py:19:     while True:
scripts/agent-island-status.py:20:         with locked_state(write=True) as state:
scripts/agent-island-status.py:21:             snapshot = public_snapshot(state)
scripts/agent-island-status.py:22: 
scripts/agent-island-status.py:23:         # The state file timestamp changes even when no task has changed.
scripts/agent-island-status.py:24:         # Keep it in one-shot output for compatibility, but omit it from streams.
scripts/agent-island-status.py:25:         if args.watch:
scripts/agent-island-status.py:26:             snapshot.pop("updated_at", None)
scripts/agent-island-status.py:27:             snapshot.pop("pending", None)
scripts/agent-island-status.py:28:             snapshot.pop("pending_count", None)
scripts/agent-island-status.py:29:         output = json.dumps(snapshot, ensure_ascii=False, separators=(",", ":")) if args.json else (
scripts/agent-island-status.py:30:             f"active={str(snapshot['active']).lower()}\ncount={snapshot['count']}"
scripts/agent-island-status.py:31:         )
scripts/agent-island-status.py:32:         if output != previous:
scripts/agent-island-status.py:33:             print(output, flush=True)
scripts/agent-island-status.py:34:             previous = output
scripts/agent-island-status.py:35:         if not args.watch:
scripts/agent-island-status.py:36:             break
scripts/agent-island-status.py:37:         time.sleep(0.65 if snapshot["active"] else 1.2)
scripts/agent-island-status.py:38:     return 0
scripts/agent-island-status.py:39: 
scripts/agent-island-status.py:40: 
scripts/agent-island-status.py:41: if __name__ == "__main__":
scripts/agent-island-status.py:42:     raise SystemExit(main())
```

### `scripts/agent-island-watch.py`

Migration owner: `agent_island`. Source `scripts/agent-island-watch.py:1`, 71 lines, SHA-256 `62321fc38a615f79266d2f2f12a131cb9fed76563e7c328fcab1623c5ccf5743`.

```text
…
scripts/agent-island-watch.py:11: 
scripts/agent-island-watch.py:12: def main() -> None:
scripts/agent-island-watch.py:13:     path = state_file()
scripts/agent-island-watch.py:14:     last_written = None
…
scripts/agent-island-watch.py:16: 
scripts/agent-island-watch.py:17:     def refresh() -> bool:
scripts/agent-island-watch.py:18:         nonlocal last_written, pending
scripts/agent-island-watch.py:19:         pending = 0
…
scripts/agent-island-watch.py:25:                 # never be mistaken for an already delivered snapshot.
scripts/agent-island-watch.py:26:                 last_written = path.read_bytes()
scripts/agent-island-watch.py:27:             print(json.dumps(snapshot, ensure_ascii=False, separators=(",", ":")), flush=True)
scripts/agent-island-watch.py:28:         except (OSError, ValueError) as error:
scripts/agent-island-watch.py:29:             print(f"agent state: {error}", file=sys.stderr, flush=True)
scripts/agent-island-watch.py:30:         return GLib.SOURCE_REMOVE
scripts/agent-island-watch.py:31: 
scripts/agent-island-watch.py:32:     def changed(_monitor, file, other, _event) -> None:
scripts/agent-island-watch.py:33:         nonlocal pending
scripts/agent-island-watch.py:34:         if not any(item and item.get_basename() == path.name for item in (file, other)):
…
scripts/agent-island-watch.py:36:         try:
scripts/agent-island-watch.py:37:             if path.read_bytes() == last_written:
scripts/agent-island-watch.py:38:                 return
scripts/agent-island-watch.py:39:         except OSError:
…
scripts/agent-island-watch.py:42:         if not pending:
scripts/agent-island-watch.py:43:             pending = GLib.timeout_add(150, refresh)
scripts/agent-island-watch.py:44: 
scripts/agent-island-watch.py:45:     monitor = Gio.File.new_for_path(str(state_dir())).monitor_directory(Gio.FileMonitorFlags.WATCH_MOVES, None)
scripts/agent-island-watch.py:46:     monitor.connect("changed", changed)
scripts/agent-island-watch.py:47:     refresh()
scripts/agent-island-watch.py:48: 
scripts/agent-island-watch.py:49:     def fallback() -> bool:
scripts/agent-island-watch.py:50:         nonlocal pending
scripts/agent-island-watch.py:51:         if pending:
…
scripts/agent-island-watch.py:53:             pending = 0
scripts/agent-island-watch.py:54:         refresh()
scripts/agent-island-watch.py:55:         return GLib.SOURCE_CONTINUE
scripts/agent-island-watch.py:56: 
scripts/agent-island-watch.py:57:     def requested(_fd, condition) -> bool:
scripts/agent-island-watch.py:58:         if condition & GLib.IOCondition.HUP:
scripts/agent-island-watch.py:59:             return GLib.SOURCE_REMOVE
scripts/agent-island-watch.py:60:         if not os.read(sys.stdin.fileno(), 4096):
scripts/agent-island-watch.py:61:             return GLib.SOURCE_REMOVE
scripts/agent-island-watch.py:62:         fallback()
scripts/agent-island-watch.py:63:         return GLib.SOURCE_CONTINUE
scripts/agent-island-watch.py:64: 
scripts/agent-island-watch.py:65:     GLib.io_add_watch(sys.stdin, GLib.IOCondition.IN | GLib.IOCondition.HUP, requested)
scripts/agent-island-watch.py:66:     GLib.timeout_add_seconds(5, fallback)
scripts/agent-island-watch.py:67:     GLib.MainLoop().run()
scripts/agent-island-watch.py:68: 
scripts/agent-island-watch.py:69: 
scripts/agent-island-watch.py:70: if __name__ == "__main__":
scripts/agent-island-watch.py:71:     main()
```

### `scripts/agent_island.py`

Migration owner: `agent_island`. Source `scripts/agent_island.py:1`, 950 lines, SHA-256 `80f9f2fb2fae00f7342755366eb89230102bb1218fd3fe44d3a8e0736a76e7e5`.

```text
…
scripts/agent_island.py:8: import os
scripts/agent_island.py:9: import subprocess
scripts/agent_island.py:10: import tempfile
scripts/agent_island.py:11: import time
…
scripts/agent_island.py:18: SOURCE_ALIASES = {
scripts/agent_island.py:19:     "claude-code": "claude",
scripts/agent_island.py:20:     "claudecode": "claude",
scripts/agent_island.py:21:     "claude_code": "claude",
scripts/agent_island.py:22: }
scripts/agent_island.py:23: AGENT_FOCUS_CLASS_KEYWORDS = (
…
scripts/agent_island.py:53: 
scripts/agent_island.py:54: def state_dir() -> Path:
scripts/agent_island.py:55:     raw = os.environ.get("HYPRV_AGENT_ISLAND_DIR", "~/.cache/hyprv/agent-island")
scripts/agent_island.py:56:     path = Path(raw).expanduser()
scripts/agent_island.py:57:     path.mkdir(parents=True, exist_ok=True)
…
scripts/agent_island.py:61: 
scripts/agent_island.py:62: def state_file() -> Path:
scripts/agent_island.py:63:     return state_dir() / "state.json"
scripts/agent_island.py:64: 
scripts/agent_island.py:65: 
scripts/agent_island.py:66: def response_file(request_id: str) -> Path:
scripts/agent_island.py:67:     safe = "".join(ch if ch.isalnum() or ch in "._-" else "_" for ch in request_id)
scripts/agent_island.py:68:     return state_dir() / "responses" / f"{safe}.json"
…
scripts/agent_island.py:70: 
scripts/agent_island.py:71: def default_state() -> dict[str, Any]:
scripts/agent_island.py:72:     return {
scripts/agent_island.py:73:         "version": STATE_VERSION,
scripts/agent_island.py:74:         "updated_at": time.time(),
scripts/agent_island.py:75:         "sessions": {},
scripts/agent_island.py:76:         "pending_requests": {},
scripts/agent_island.py:77:     }
scripts/agent_island.py:78: 
scripts/agent_island.py:79: 
scripts/agent_island.py:80: def read_state() -> dict[str, Any]:
scripts/agent_island.py:81:     path = state_file()
scripts/agent_island.py:82:     if not path.exists():
…
scripts/agent_island.py:84:     try:
scripts/agent_island.py:85:         data = json.loads(path.read_text(encoding="utf-8"))
scripts/agent_island.py:86:     except Exception:
scripts/agent_island.py:87:         return default_state()
…
scripts/agent_island.py:100: 
scripts/agent_island.py:101: def write_state(state: dict[str, Any]) -> None:
scripts/agent_island.py:102:     state["version"] = STATE_VERSION
scripts/agent_island.py:103:     state["updated_at"] = time.time()
…
scripts/agent_island.py:106:     with tempfile.NamedTemporaryFile("w", encoding="utf-8", dir=path.parent, delete=False) as tmp:
scripts/agent_island.py:107:         json.dump(state, tmp, ensure_ascii=False, separators=(",", ":"))
scripts/agent_island.py:108:         tmp.write("\n")
scripts/agent_island.py:109:         tmp_name = tmp.name
scripts/agent_island.py:110:     os.replace(tmp_name, path)
scripts/agent_island.py:111: 
scripts/agent_island.py:112: 
scripts/agent_island.py:113: @contextlib.contextmanager
scripts/agent_island.py:114: def locked_state(write: bool = True):
scripts/agent_island.py:115:     lock_path = state_dir() / "state.lock"
scripts/agent_island.py:116:     with lock_path.open("a+", encoding="utf-8") as lock:
scripts/agent_island.py:117:         fcntl.flock(lock.fileno(), fcntl.LOCK_EX)
scripts/agent_island.py:118:         state = read_state()
scripts/agent_island.py:119:         try:
…
scripts/agent_island.py:123:                 write_state(state)
scripts/agent_island.py:124:             fcntl.flock(lock.fileno(), fcntl.LOCK_UN)
scripts/agent_island.py:125: 
scripts/agent_island.py:126: 
scripts/agent_island.py:127: def normalize_source(value: str | None) -> str | None:
scripts/agent_island.py:128:     if not value:
scripts/agent_island.py:129:         return None
scripts/agent_island.py:130:     normalized = value.strip().lower()
scripts/agent_island.py:131:     normalized = SOURCE_ALIASES.get(normalized, normalized)
scripts/agent_island.py:132:     return normalized if normalized in SUPPORTED_SOURCES else None
scripts/agent_island.py:133: 
scripts/agent_island.py:134: 
scripts/agent_island.py:135: def source_label(source: str) -> str:
scripts/agent_island.py:136:     if source == "codex":
scripts/agent_island.py:137:         return "Codex"
scripts/agent_island.py:138:     if source == "claude":
scripts/agent_island.py:139:         return "Claude Code"
scripts/agent_island.py:140:     return source.title()
…
scripts/agent_island.py:142: 
scripts/agent_island.py:143: def project_name(cwd: str | None) -> str:
scripts/agent_island.py:144:     if not cwd:
scripts/agent_island.py:145:         return "Session"
…
scripts/agent_island.py:149: 
scripts/agent_island.py:150: def first_string(data: dict[str, Any], keys: list[str]) -> str | None:
scripts/agent_island.py:151:     for key in keys:
scripts/agent_island.py:152:         value = data.get(key)
scripts/agent_island.py:153:         if isinstance(value, str) and value.strip():
scripts/agent_island.py:154:             return value.strip()
…
scripts/agent_island.py:157: 
scripts/agent_island.py:158: def event_name(payload: dict[str, Any], event_arg: str | None = None) -> str | None:
scripts/agent_island.py:159:     return first_string(payload, ["hook_event_name", "hookEventName", "event_name", "eventName", "event"]) or event_arg
scripts/agent_island.py:160: 
scripts/agent_island.py:161: 
scripts/agent_island.py:162: def normalize_event(name: str | None) -> str:
scripts/agent_island.py:163:     if not name:
scripts/agent_island.py:164:         return ""
scripts/agent_island.py:165:     mapping = {
scripts/agent_island.py:166:         "session_start": "SessionStart",
scripts/agent_island.py:167:         "sessionEnd": "SessionEnd",
scripts/agent_island.py:168:         "session_end": "SessionEnd",
scripts/agent_island.py:169:         "userPromptSubmit": "UserPromptSubmit",
scripts/agent_island.py:170:         "user_prompt_submit": "UserPromptSubmit",
scripts/agent_island.py:171:         "preToolUse": "PreToolUse",
scripts/agent_island.py:172:         "pre_tool_use": "PreToolUse",
scripts/agent_island.py:173:         "postToolUse": "PostToolUse",
scripts/agent_island.py:174:         "post_tool_use": "PostToolUse",
scripts/agent_island.py:175:         "post_tool_use_failure": "PostToolUseFailure",
scripts/agent_island.py:176:         "permission_request": "PermissionRequest",
scripts/agent_island.py:177:         "notification": "Notification",
scripts/agent_island.py:178:         "stop": "Stop",
scripts/agent_island.py:179:         "pre_compact": "PreCompact",
scripts/agent_island.py:180:         "subagent_start": "SubagentStart",
scripts/agent_island.py:181:         "subagent_stop": "SubagentStop",
scripts/agent_island.py:182:     }
scripts/agent_island.py:183:     return mapping.get(name, name)
scripts/agent_island.py:184: 
scripts/agent_island.py:185: 
scripts/agent_island.py:186: def nested_dict(payload: dict[str, Any], keys: list[str]) -> dict[str, Any] | None:
scripts/agent_island.py:187:     for key in keys:
scripts/agent_island.py:188:         value = payload.get(key)
scripts/agent_island.py:189:         if isinstance(value, dict):
scripts/agent_island.py:190:             return value
…
scripts/agent_island.py:193: 
scripts/agent_island.py:194: def tool_name(payload: dict[str, Any]) -> str | None:
scripts/agent_island.py:195:     value = first_string(payload, ["tool_name", "toolName", "tool", "name"])
scripts/agent_island.py:196:     if value:
…
scripts/agent_island.py:203: 
scripts/agent_island.py:204: def tool_input(payload: dict[str, Any]) -> dict[str, Any] | None:
scripts/agent_island.py:205:     value = nested_dict(payload, ["tool_input", "toolInput", "input", "arguments", "args", "params"])
scripts/agent_island.py:206:     if value:
…
scripts/agent_island.py:213: 
scripts/agent_island.py:214: def tool_use_id(payload: dict[str, Any]) -> str | None:
scripts/agent_island.py:215:     value = first_string(payload, ["tool_use_id", "toolUseId"])
scripts/agent_island.py:216:     if value:
…
scripts/agent_island.py:223: 
scripts/agent_island.py:224: def tool_description(payload: dict[str, Any]) -> str:
scripts/agent_island.py:225:     name = tool_name(payload) or ""
scripts/agent_island.py:226:     inp = tool_input(payload) or {}
scripts/agent_island.py:227:     if name == "Bash":
scripts/agent_island.py:228:         desc = inp.get("description")
scripts/agent_island.py:229:         cmd = inp.get("command")
scripts/agent_island.py:230:         if isinstance(desc, str) and desc.strip() and isinstance(cmd, str) and cmd.strip() and desc.strip() != cmd.strip():
scripts/agent_island.py:231:             return f"{desc.strip()}: {cmd.strip()[:80]}"
…
scripts/agent_island.py:236:     for key in ("file_path", "path"):
scripts/agent_island.py:237:         value = inp.get(key)
scripts/agent_island.py:238:         if isinstance(value, str) and value.strip():
scripts/agent_island.py:239:             return Path(value).name or value.strip()
scripts/agent_island.py:240:     for key in ("pattern", "query", "prompt", "command", "description", "message", "text", "content"):
scripts/agent_island.py:241:         value = inp.get(key)
scripts/agent_island.py:242:         if isinstance(value, str) and value.strip():
scripts/agent_island.py:243:             return value.strip()[:120]
…
scripts/agent_island.py:249: 
scripts/agent_island.py:250: def session_id(payload: dict[str, Any], source: str) -> str:
scripts/agent_island.py:251:     explicit = first_string(payload, ["session_id", "sessionId"])
scripts/agent_island.py:252:     if explicit:
…
scripts/agent_island.py:258: 
scripts/agent_island.py:259: def session_title(payload: dict[str, Any], session_id_value: str) -> str | None:
scripts/agent_island.py:260:     return first_string(payload, ["session_title", "title", "name"]) or session_id_value[:8]
scripts/agent_island.py:261: 
scripts/agent_island.py:262: 
scripts/agent_island.py:263: def active_window_snapshot() -> dict[str, Any] | None:
scripts/agent_island.py:264:     try:
scripts/agent_island.py:265:         out = subprocess.run(
scripts/agent_island.py:266:             ["hyprctl", "activewindow", "-j"],
scripts/agent_island.py:267:             check=False,
scripts/agent_island.py:268:             stdout=subprocess.PIPE,
scripts/agent_island.py:269:             stderr=subprocess.DEVNULL,
scripts/agent_island.py:270:             text=True,
scripts/agent_island.py:271:             timeout=0.4,
scripts/agent_island.py:272:         ).stdout.strip()
scripts/agent_island.py:273:         if not out:
scripts/agent_island.py:274:             return None
scripts/agent_island.py:275:         data = json.loads(out)
scripts/agent_island.py:276:         if not isinstance(data, dict):
scripts/agent_island.py:277:             return None
scripts/agent_island.py:278:         return {
scripts/agent_island.py:279:             "address": data.get("address") or "",
scripts/agent_island.py:280:             "class": data.get("class") or data.get("initialClass") or "",
scripts/agent_island.py:281:             "initialClass": data.get("initialClass") or "",
scripts/agent_island.py:282:             "title": data.get("title") or data.get("initialTitle") or "",
scripts/agent_island.py:283:             "pid": data.get("pid") or 0,
scripts/agent_island.py:284:         }
scripts/agent_island.py:285:     except Exception:
…
scripts/agent_island.py:288: 
scripts/agent_island.py:289: def is_agent_focus_client(client: dict[str, Any] | None) -> bool:
scripts/agent_island.py:290:     if not isinstance(client, dict):
scripts/agent_island.py:291:         return False
scripts/agent_island.py:292:     class_text = f"{client.get('class') or ''} {client.get('initialClass') or ''}".lower()
scripts/agent_island.py:293:     return any(keyword in class_text for keyword in AGENT_FOCUS_CLASS_KEYWORDS)
scripts/agent_island.py:294: 
scripts/agent_island.py:295: 
scripts/agent_island.py:296: def terminal_meta(payload: dict[str, Any]) -> dict[str, Any]:
scripts/agent_island.py:297:     meta: dict[str, Any] = {}
scripts/agent_island.py:298:     for env_key, out_key in (
…
scripts/agent_island.py:306:     ):
scripts/agent_island.py:307:         value = os.environ.get(env_key)
scripts/agent_island.py:308:         if value:
scripts/agent_island.py:309:             meta[out_key] = value
scripts/agent_island.py:310:     window = active_window_snapshot()
scripts/agent_island.py:311:     if window and window.get("address") and is_agent_focus_client(window):
scripts/agent_island.py:312:         meta["window"] = window
scripts/agent_island.py:313:     if payload.get("_ppid"):
scripts/agent_island.py:314:         meta["pid"] = payload.get("_ppid")
scripts/agent_island.py:315:     else:
scripts/agent_island.py:316:         meta["pid"] = os.getppid()
…
scripts/agent_island.py:319: 
scripts/agent_island.py:320: def clean_for_json(value: Any) -> Any:
scripts/agent_island.py:321:     if isinstance(value, dict):
scripts/agent_island.py:322:         return {str(k): clean_for_json(v) for k, v in value.items()}
…
scripts/agent_island.py:329: 
scripts/agent_island.py:330: def response_allow() -> dict[str, Any]:
scripts/agent_island.py:331:     return {
scripts/agent_island.py:332:         "hookSpecificOutput": {
scripts/agent_island.py:333:             "hookEventName": "PermissionRequest",
scripts/agent_island.py:334:             "decision": {"behavior": "allow"},
scripts/agent_island.py:335:         }
scripts/agent_island.py:336:     }
…
scripts/agent_island.py:338: 
scripts/agent_island.py:339: def response_deny(message: str | None = None) -> dict[str, Any]:
scripts/agent_island.py:340:     decision: dict[str, Any] = {"behavior": "deny"}
scripts/agent_island.py:341:     if message:
scripts/agent_island.py:342:         decision["message"] = message
scripts/agent_island.py:343:     return {
scripts/agent_island.py:344:         "hookSpecificOutput": {
scripts/agent_island.py:345:             "hookEventName": "PermissionRequest",
scripts/agent_island.py:346:             "decision": decision,
scripts/agent_island.py:347:         }
scripts/agent_island.py:348:     }
…
scripts/agent_island.py:350: 
scripts/agent_island.py:351: def response_answer_notification(answer: str) -> dict[str, Any]:
scripts/agent_island.py:352:     return {
scripts/agent_island.py:353:         "hookSpecificOutput": {
scripts/agent_island.py:354:             "hookEventName": "Notification",
scripts/agent_island.py:355:             "answer": answer,
scripts/agent_island.py:356:         }
scripts/agent_island.py:357:     }
…
scripts/agent_island.py:359: 
scripts/agent_island.py:360: def response_answer_permission(answer_key: str, answer: str) -> dict[str, Any]:
scripts/agent_island.py:361:     return {
scripts/agent_island.py:362:         "hookSpecificOutput": {
scripts/agent_island.py:363:             "hookEventName": "PermissionRequest",
scripts/agent_island.py:364:             "decision": {
scripts/agent_island.py:365:                 "behavior": "allow",
scripts/agent_island.py:366:                 "updatedInput": {"answers": {answer_key: answer}},
scripts/agent_island.py:367:             },
scripts/agent_island.py:368:         }
…
scripts/agent_island.py:371: 
scripts/agent_island.py:372: def extract_question(payload: dict[str, Any]) -> dict[str, Any] | None:
scripts/agent_island.py:373:     if isinstance(payload.get("question"), str) and payload["question"].strip():
scripts/agent_island.py:374:         options = payload.get("options")
scripts/agent_island.py:375:         return {
scripts/agent_island.py:376:             "question": payload["question"].strip(),
scripts/agent_island.py:377:             "options": options if isinstance(options, list) else [],
scripts/agent_island.py:378:             "answer_key": "answer",
scripts/agent_island.py:379:             "from_permission": False,
scripts/agent_island.py:380:         }
scripts/agent_island.py:381:     inp = tool_input(payload) or {}
scripts/agent_island.py:382:     if isinstance(inp.get("questions"), list) and inp["questions"]:
scripts/agent_island.py:383:         first = inp["questions"][0]
scripts/agent_island.py:384:         if isinstance(first, dict):
scripts/agent_island.py:385:             question = first.get("question")
scripts/agent_island.py:386:             if isinstance(question, str) and question.strip():
scripts/agent_island.py:387:                 raw_options = first.get("options")
scripts/agent_island.py:388:                 options: list[str] = []
scripts/agent_island.py:389:                 if isinstance(raw_options, list):
…
scripts/agent_island.py:392:                             options.append(option)
scripts/agent_island.py:393:                         elif isinstance(option, dict) and isinstance(option.get("label"), str):
scripts/agent_island.py:394:                             options.append(option["label"])
scripts/agent_island.py:395:                 answer_key = first.get("header") if isinstance(first.get("header"), str) else "answer_1"
scripts/agent_island.py:396:                 return {
scripts/agent_island.py:397:                     "question": question.strip(),
scripts/agent_island.py:398:                     "options": options,
scripts/agent_island.py:399:                     "answer_key": answer_key,
scripts/agent_island.py:400:                     "from_permission": True,
scripts/agent_island.py:401:                 }
scripts/agent_island.py:402:     if isinstance(inp.get("question"), str) and inp["question"].strip():
scripts/agent_island.py:403:         raw_options = inp.get("options")
scripts/agent_island.py:404:         options = []
scripts/agent_island.py:405:         if isinstance(raw_options, list):
…
scripts/agent_island.py:408:                     options.append(option)
scripts/agent_island.py:409:                 elif isinstance(option, dict) and isinstance(option.get("label"), str):
scripts/agent_island.py:410:                     options.append(option["label"])
scripts/agent_island.py:411:         return {
scripts/agent_island.py:412:             "question": inp["question"].strip(),
scripts/agent_island.py:413:             "options": options,
scripts/agent_island.py:414:             "answer_key": "answer",
scripts/agent_island.py:415:             "from_permission": True,
scripts/agent_island.py:416:         }
scripts/agent_island.py:417:     return None
…
scripts/agent_island.py:419: 
scripts/agent_island.py:420: def is_done_notification(source: str, detail: str) -> bool:
scripts/agent_island.py:421:     lowered = detail.strip().lower()
scripts/agent_island.py:422:     if source == "claude" and "waiting for your input" in lowered:
…
scripts/agent_island.py:426: 
scripts/agent_island.py:427: def status_priority(status: str) -> int:
scripts/agent_island.py:428:     return {
scripts/agent_island.py:429:         "waitingApproval": 5,
scripts/agent_island.py:430:         "waitingQuestion": 4,
scripts/agent_island.py:431:         "running": 3,
scripts/agent_island.py:432:         "processing": 2,
scripts/agent_island.py:433:         "completed": 1,
scripts/agent_island.py:434:         "idle": 0,
scripts/agent_island.py:435:     }.get(status, 0)
scripts/agent_island.py:436: 
scripts/agent_island.py:437: 
scripts/agent_island.py:438: def proc_cmdline(pid: int) -> list[str]:
scripts/agent_island.py:439:     try:
scripts/agent_island.py:440:         raw = Path(f"/proc/{pid}/cmdline").read_bytes()
scripts/agent_island.py:441:     except Exception:
scripts/agent_island.py:442:         return []
…
scripts/agent_island.py:445: 
scripts/agent_island.py:446: def proc_stat(pid: int) -> tuple[int, int] | None:
scripts/agent_island.py:447:     try:
scripts/agent_island.py:448:         text = Path(f"/proc/{pid}/stat").read_text(encoding="utf-8", errors="replace")
scripts/agent_island.py:449:     except Exception:
scripts/agent_island.py:450:         return None
…
scripts/agent_island.py:462: 
scripts/agent_island.py:463: def proc_cwd(pid: int) -> str:
scripts/agent_island.py:464:     try:
scripts/agent_island.py:465:         return os.readlink(f"/proc/{pid}/cwd")
scripts/agent_island.py:466:     except Exception:
scripts/agent_island.py:467:         return ""
…
scripts/agent_island.py:469: 
scripts/agent_island.py:470: def list_processes() -> dict[int, dict[str, Any]]:
scripts/agent_island.py:471:     processes: dict[int, dict[str, Any]] = {}
scripts/agent_island.py:472:     try:
…
scripts/agent_island.py:480:         try:
scripts/agent_island.py:481:             with open(f"/proc/{pid}/stat", encoding="utf-8", errors="replace") as file:
scripts/agent_island.py:482:                 text = file.read()
scripts/agent_island.py:483:             close = text.rfind(")")
scripts/agent_island.py:484:             fields = text[close + 2:].split()
…
scripts/agent_island.py:490:         # participate in discovery; avoid opening argv for browsers and workers.
scripts/agent_island.py:491:         cmdline = proc_cmdline(pid) if name in ("codex", "claude", "systemd-inhibit") else []
scripts/agent_island.py:492:         processes[pid] = {"cmdline": cmdline, "ppid": ppid, "start_ticks": start_ticks}
scripts/agent_island.py:493:     return processes
scripts/agent_island.py:494: 
scripts/agent_island.py:495: 
scripts/agent_island.py:496: def is_codex_native_process(cmdline: list[str]) -> bool:
scripts/agent_island.py:497:     if not cmdline:
scripts/agent_island.py:498:         return False
…
scripts/agent_island.py:505: 
scripts/agent_island.py:506: def is_claude_native_process(cmdline: list[str]) -> bool:
scripts/agent_island.py:507:     if not cmdline:
scripts/agent_island.py:508:         return False
scripts/agent_island.py:509:     if Path(cmdline[0]).name != "claude":
scripts/agent_island.py:510:         return False
scripts/agent_island.py:511:     ignored_commands = {
…
scripts/agent_island.py:525: 
scripts/agent_island.py:526: def is_codex_active_inhibitor(cmdline: list[str]) -> bool:
scripts/agent_island.py:527:     if not cmdline:
scripts/agent_island.py:528:         return False
scripts/agent_island.py:529:     if Path(cmdline[0]).name != "systemd-inhibit":
scripts/agent_island.py:530:         return False
scripts/agent_island.py:531:     joined = " ".join(cmdline).lower()
…
scripts/agent_island.py:534: 
scripts/agent_island.py:535: def codex_ancestor_pid(pid: int, processes: dict[int, dict[str, Any]], codex_pids: set[int]) -> int | None:
scripts/agent_island.py:536:     seen: set[int] = set()
scripts/agent_island.py:537:     current = int(processes.get(pid, {}).get("ppid") or 0)
scripts/agent_island.py:538:     while current > 0 and current not in seen:
scripts/agent_island.py:539:         if current in codex_pids:
…
scripts/agent_island.py:541:         seen.add(current)
scripts/agent_island.py:542:         current = int(processes.get(current, {}).get("ppid") or 0)
scripts/agent_island.py:543:     return None
scripts/agent_island.py:544: 
scripts/agent_island.py:545: 
scripts/agent_island.py:546: def int_value(value: Any) -> int | None:
scripts/agent_island.py:547:     try:
scripts/agent_island.py:548:         return int(value)
…
scripts/agent_island.py:552: 
scripts/agent_island.py:553: def process_exists(pid: int | None, processes: dict[int, dict[str, Any]] | None = None) -> bool:
scripts/agent_island.py:554:     if pid is None or pid <= 0:
scripts/agent_island.py:555:         return False
…
scripts/agent_island.py:557:         return pid in processes
scripts/agent_island.py:558:     return Path(f"/proc/{pid}").exists()
scripts/agent_island.py:559: 
scripts/agent_island.py:560: 
scripts/agent_island.py:561: def session_pid(session: dict[str, Any]) -> int | None:
scripts/agent_island.py:562:     terminal = session.get("terminal") if isinstance(session.get("terminal"), dict) else {}
scripts/agent_island.py:563:     return int_value(terminal.get("pid"))
scripts/agent_island.py:564: 
scripts/agent_island.py:565: 
scripts/agent_island.py:566: def find_session_for_pid(sessions: dict[str, Any], pid: int, source: str = "codex") -> str | None:
scripts/agent_island.py:567:     matched: str | None = None
scripts/agent_island.py:568:     for sid, session in sessions.items():
…
scripts/agent_island.py:570:             continue
scripts/agent_island.py:571:         if session.get("source") != source:
scripts/agent_island.py:572:             continue
scripts/agent_island.py:573:         terminal = session.get("terminal")
scripts/agent_island.py:574:         if isinstance(terminal, dict) and int_value(terminal.get("pid")) == pid:
scripts/agent_island.py:575:             if session.get("provider") != "process-scan":
scripts/agent_island.py:576:                 return sid
scripts/agent_island.py:577:             matched = sid
…
scripts/agent_island.py:580: 
scripts/agent_island.py:581: def sync_codex_process_sessions(state: dict[str, Any]) -> None:
scripts/agent_island.py:582:     now = time.time()
scripts/agent_island.py:583:     try:
scripts/agent_island.py:584:         last_scan = float(state.get("last_process_scan") or 0)
scripts/agent_island.py:585:     except (TypeError, ValueError):
scripts/agent_island.py:586:         last_scan = 0
…
scripts/agent_island.py:594:         for pid, process in processes.items()
scripts/agent_island.py:595:         if is_codex_native_process(process.get("cmdline") or [])
scripts/agent_island.py:596:     }
scripts/agent_island.py:597:     active_pids: set[int] = set()
scripts/agent_island.py:598:     for pid, process in processes.items():
scripts/agent_island.py:599:         if not is_codex_active_inhibitor(process.get("cmdline") or []):
scripts/agent_island.py:600:             continue
scripts/agent_island.py:601:         ancestor = codex_ancestor_pid(pid, processes, codex_pids)
…
scripts/agent_island.py:608:     for pid in sorted(active_pids):
scripts/agent_island.py:609:         process = processes.get(pid) or {}
scripts/agent_island.py:610:         start_ticks = int(process.get("start_ticks") or 0)
scripts/agent_island.py:611:         cwd = proc_cwd(pid)
scripts/agent_island.py:612:         existing_sid = find_session_for_pid(sessions, pid, "codex")
scripts/agent_island.py:613:         sid = existing_sid or f"codexproc-{pid}-{start_ticks}"
scripts/agent_island.py:614:         session = sessions.get(sid) if isinstance(sessions.get(sid), dict) else {}
scripts/agent_island.py:615: 
scripts/agent_island.py:616:         session.setdefault("id", sid)
…
scripts/agent_island.py:618:         session["source_label"] = source_label("codex")
scripts/agent_island.py:619:         session["title"] = session.get("title") or f"Codex {pid}"
scripts/agent_island.py:620:         session["cwd"] = cwd or session.get("cwd") or ""
scripts/agent_island.py:621:         session["project"] = project_name(session.get("cwd"))
scripts/agent_island.py:622:         session["event"] = "ProcessScan"
scripts/agent_island.py:623:         session["status"] = "running"
…
scripts/agent_island.py:628:         try:
scripts/agent_island.py:629:             existing_progress = float(session.get("progress") or 0)
scripts/agent_island.py:630:         except (TypeError, ValueError):
scripts/agent_island.py:631:             existing_progress = 0
…
scripts/agent_island.py:639:             session["provider"] = "process-scan"
scripts/agent_island.py:640:         if session.get("provider") == "process-scan":
scripts/agent_island.py:641:             seen_synthetic.add(sid)
scripts/agent_island.py:642: 
scripts/agent_island.py:643:         terminal = session.get("terminal") if isinstance(session.get("terminal"), dict) else {}
scripts/agent_island.py:644:         terminal["pid"] = pid
scripts/agent_island.py:645:         session["terminal"] = clean_for_json(terminal)
…
scripts/agent_island.py:650:         for pid, process in processes.items()
scripts/agent_island.py:651:         if is_claude_native_process(process.get("cmdline") or [])
scripts/agent_island.py:652:     }
scripts/agent_island.py:653:     for pid in sorted(claude_pids):
scripts/agent_island.py:654:         process = processes.get(pid) or {}
scripts/agent_island.py:655:         start_ticks = int(process.get("start_ticks") or 0)
scripts/agent_island.py:656:         cwd = proc_cwd(pid)
scripts/agent_island.py:657:         existing_sid = find_session_for_pid(sessions, pid, "claude")
scripts/agent_island.py:658:         sid = existing_sid or f"claudeproc-{pid}-{start_ticks}"
scripts/agent_island.py:659:         session = sessions.get(sid) if isinstance(sessions.get(sid), dict) else {}
scripts/agent_island.py:660: 
scripts/agent_island.py:661:         session.setdefault("id", sid)
…
scripts/agent_island.py:663:         session["source_label"] = source_label("claude")
scripts/agent_island.py:664:         session["title"] = session.get("title") or f"Claude {pid}"
scripts/agent_island.py:665:         session["cwd"] = cwd or session.get("cwd") or ""
scripts/agent_island.py:666:         session["project"] = project_name(session.get("cwd"))
scripts/agent_island.py:667:         session["event"] = "ProcessScan"
scripts/agent_island.py:668:         session["status"] = "running"
scripts/agent_island.py:669:         session["detail"] = session.get("detail") or "Claude Code"
scripts/agent_island.py:670:         session["current_tool"] = session.get("current_tool") or "Claude Code"
scripts/agent_island.py:671:         session["had_activity"] = True
scripts/agent_island.py:672:         session.pop("completed_at", None)
scripts/agent_island.py:673:         try:
scripts/agent_island.py:674:             existing_progress = float(session.get("progress") or 0)
scripts/agent_island.py:675:         except (TypeError, ValueError):
scripts/agent_island.py:676:             existing_progress = 0
…
scripts/agent_island.py:684:             session["provider"] = "process-scan"
scripts/agent_island.py:685:         if session.get("provider") == "process-scan":
scripts/agent_island.py:686:             seen_synthetic.add(sid)
scripts/agent_island.py:687: 
scripts/agent_island.py:688:         terminal = session.get("terminal") if isinstance(session.get("terminal"), dict) else {}
scripts/agent_island.py:689:         terminal["pid"] = pid
scripts/agent_island.py:690:         session["terminal"] = clean_for_json(terminal)
…
scripts/agent_island.py:695:             continue
scripts/agent_island.py:696:         if session.get("provider") == "process-scan" and sid not in seen_synthetic:
scripts/agent_island.py:697:             pid_value = session_pid(session)
scripts/agent_island.py:698:             if not process_exists(pid_value, processes):
…
scripts/agent_island.py:701:         pid_value = session_pid(session)
scripts/agent_island.py:702:         source = session.get("source")
scripts/agent_island.py:703:         process_scan_active = (
scripts/agent_island.py:704:             source == "codex" and pid_value in active_pids
…
scripts/agent_island.py:707:         )
scripts/agent_island.py:708:         if session.get("event") == "ProcessScan" and not process_scan_active:
scripts/agent_island.py:709:             session["current_tool"] = ""
scripts/agent_island.py:710:             if process_exists(pid_value, processes) and session.get("had_activity"):
scripts/agent_island.py:711:                 session["status"] = "completed"
scripts/agent_island.py:712:                 session["detail"] = "Done"
…
scripts/agent_island.py:719: 
scripts/agent_island.py:720: def update_session_from_event(state: dict[str, Any], payload: dict[str, Any], source: str, event: str) -> tuple[dict[str, Any], dict[str, Any] | None]:
scripts/agent_island.py:721:     now = time.time()
scripts/agent_island.py:722:     sid = session_id(payload, source)
scripts/agent_island.py:723:     sessions = state.setdefault("sessions", {})
scripts/agent_island.py:724:     session = sessions.get(sid) if isinstance(sessions.get(sid), dict) else {}
scripts/agent_island.py:725:     session.setdefault("id", sid)
scripts/agent_island.py:726:     session["source"] = source
…
scripts/agent_island.py:728:     session["title"] = session_title(payload, sid)
scripts/agent_island.py:729:     cwd = first_string(payload, ["cwd"]) or session.get("cwd") or ""
scripts/agent_island.py:730:     session["cwd"] = cwd
scripts/agent_island.py:731:     session["project"] = project_name(cwd)
…
scripts/agent_island.py:741:         session["had_activity"] = True
scripts/agent_island.py:742:     elif event == "SessionStart":
scripts/agent_island.py:743:         session.setdefault("had_activity", False)
scripts/agent_island.py:744: 
…
scripts/agent_island.py:748: 
scripts/agent_island.py:749:     if event == "SessionStart":
scripts/agent_island.py:750:         session["status"] = "idle"
scripts/agent_island.py:751:         session["detail"] = "Session started"
scripts/agent_island.py:752:         session["progress"] = 0.0
scripts/agent_island.py:753:     elif event == "UserPromptSubmit":
scripts/agent_island.py:754:         session["status"] = "processing"
scripts/agent_island.py:755:         session["current_tool"] = ""
…
scripts/agent_island.py:766:             history.append({
scripts/agent_island.py:767:                 "tool": name,
scripts/agent_island.py:768:                 "detail": detail,
scripts/agent_island.py:769:                 "success": event != "PostToolUseFailure",
scripts/agent_island.py:770:                 "time": now,
scripts/agent_island.py:771:             })
scripts/agent_island.py:772:             del history[:-8]
…
scripts/agent_island.py:776:         session["progress"] = 0.66
scripts/agent_island.py:777:     elif event == "PermissionRequest":
scripts/agent_island.py:778:         question = extract_question(payload) if name == "AskUserQuestion" else None
scripts/agent_island.py:779:         request_id = f"{source}-{sid}-{tool_use_id(payload) or int(now * 1000)}"
…
scripts/agent_island.py:784:             request = {
scripts/agent_island.py:785:                 "id": request_id,
scripts/agent_island.py:786:                 "kind": "question_from_permission",
scripts/agent_island.py:787:                 "session_id": sid,
scripts/agent_island.py:788:                 "source": source,
scripts/agent_island.py:789:                 "question": question["question"],
scripts/agent_island.py:790:                 "options": question["options"],
scripts/agent_island.py:791:                 "answer_key": question["answer_key"],
scripts/agent_island.py:792:                 "created_at": now,
scripts/agent_island.py:793:             }
scripts/agent_island.py:794:         else:
…
scripts/agent_island.py:798:             request = {
scripts/agent_island.py:799:                 "id": request_id,
scripts/agent_island.py:800:                 "kind": "approval",
scripts/agent_island.py:801:                 "session_id": sid,
scripts/agent_island.py:802:                 "source": source,
scripts/agent_island.py:803:                 "tool": name or "Permission",
scripts/agent_island.py:804:                 "detail": detail,
scripts/agent_island.py:805:                 "created_at": now,
scripts/agent_island.py:806:             }
scripts/agent_island.py:807:         session["pending_request_id"] = request_id
scripts/agent_island.py:808:         session["progress"] = 0.82
scripts/agent_island.py:809:     elif event == "Notification":
scripts/agent_island.py:810:         question = extract_question(payload)
scripts/agent_island.py:811:         if question:
…
scripts/agent_island.py:816:             request = {
scripts/agent_island.py:817:                 "id": request_id,
scripts/agent_island.py:818:                 "kind": "question",
scripts/agent_island.py:819:                 "session_id": sid,
scripts/agent_island.py:820:                 "source": source,
scripts/agent_island.py:821:                 "question": question["question"],
scripts/agent_island.py:822:                 "options": question["options"],
scripts/agent_island.py:823:                 "answer_key": question["answer_key"],
scripts/agent_island.py:824:                 "created_at": now,
scripts/agent_island.py:825:             }
scripts/agent_island.py:826:             session["pending_request_id"] = request_id
…
scripts/agent_island.py:834:                 session.pop("pending_request_id", None)
scripts/agent_island.py:835:             elif session.get("status") in (None, "idle"):
scripts/agent_island.py:836:                 session["status"] = "processing"
scripts/agent_island.py:837:             session["progress"] = max(float(session.get("progress", 0.5) or 0.5), 0.5)
scripts/agent_island.py:838:     elif event == "PreCompact":
scripts/agent_island.py:839:         session["status"] = "processing"
scripts/agent_island.py:840:         session["current_tool"] = ""
…
scripts/agent_island.py:842:         session["progress"] = 0.55
scripts/agent_island.py:843:     elif event == "Stop":
scripts/agent_island.py:844:         session["status"] = "completed"
scripts/agent_island.py:845:         session["current_tool"] = ""
…
scripts/agent_island.py:849:         session.pop("pending_request_id", None)
scripts/agent_island.py:850:     elif event == "SessionEnd":
scripts/agent_island.py:851:         session["status"] = "completed"
scripts/agent_island.py:852:         session["current_tool"] = ""
…
scripts/agent_island.py:857:     else:
scripts/agent_island.py:858:         session["status"] = session.get("status") or "processing"
scripts/agent_island.py:859:         session["detail"] = detail
scripts/agent_island.py:860: 
…
scripts/agent_island.py:866: 
scripts/agent_island.py:867: def cleanup_state(state: dict[str, Any]) -> None:
scripts/agent_island.py:868:     now = time.time()
scripts/agent_island.py:869:     alive_cache: dict[int | None, bool] = {}
scripts/agent_island.py:870: 
scripts/agent_island.py:871:     def alive(pid: int | None) -> bool:
scripts/agent_island.py:872:         if pid not in alive_cache:
scripts/agent_island.py:873:             alive_cache[pid] = process_exists(pid)
…
scripts/agent_island.py:880:             continue
scripts/agent_island.py:881:         status = session.get("status")
scripts/agent_island.py:882:         pid = session_pid(session)
scripts/agent_island.py:883:         has_pending = bool(session.get("pending_request_id"))
scripts/agent_island.py:884:         if pid is not None and not alive(pid) and not has_pending:
scripts/agent_island.py:885:             sessions.pop(sid, None)
scripts/agent_island.py:886:             continue
scripts/agent_island.py:887:         if status == "completed" and session.get("had_activity") and alive(pid):
scripts/agent_island.py:888:             continue
scripts/agent_island.py:889:         last = float(session.get("last_activity") or 0)
scripts/agent_island.py:890:         completed = float(session.get("completed_at") or last or 0)
scripts/agent_island.py:891:         stale_after = 20 if status in ("completed", "idle") else 900
scripts/agent_island.py:892:         if now - max(last, completed) > stale_after:
…
scripts/agent_island.py:899:             continue
scripts/agent_island.py:900:         sid = request.get("session_id")
scripts/agent_island.py:901:         created = float(request.get("created_at") or 0)
scripts/agent_island.py:902:         if sid not in sessions or now - created > 86400:
scripts/agent_island.py:903:             pending.pop(rid, None)
…
scripts/agent_island.py:905: 
scripts/agent_island.py:906: def public_snapshot(state: dict[str, Any], *, scan_processes: bool = True) -> dict[str, Any]:
scripts/agent_island.py:907:     if scan_processes:
scripts/agent_island.py:908:         sync_codex_process_sessions(state)
…
scripts/agent_island.py:910:     sessions = []
scripts/agent_island.py:911:     for session in state.get("sessions", {}).values():
scripts/agent_island.py:912:         if not isinstance(session, dict):
scripts/agent_island.py:913:             continue
scripts/agent_island.py:914:         if session.get("event") == "SessionStart" and not session.get("pending_request_id"):
scripts/agent_island.py:915:             session["status"] = "idle"
scripts/agent_island.py:916:             session["detail"] = "Session started"
scripts/agent_island.py:917:         if session.get("event") == "Notification" and is_done_notification(
scripts/agent_island.py:918:             str(session.get("source") or ""),
scripts/agent_island.py:919:             str(session.get("detail") or ""),
scripts/agent_island.py:920:         ):
scripts/agent_island.py:921:             session["status"] = "completed"
…
scripts/agent_island.py:924:             session.setdefault("completed_at", time.time())
scripts/agent_island.py:925:         status = str(session.get("status") or "idle")
scripts/agent_island.py:926:         if status == "idle":
scripts/agent_island.py:927:             continue
scripts/agent_island.py:928:         item = dict(session)
scripts/agent_island.py:929:         base_progress = float(item.get("progress") or 0)
scripts/agent_island.py:930:         if status in ("running", "processing"):
scripts/agent_island.py:931:             phase = ((time.time() - float(item.get("last_activity") or time.time())) % 12.0) / 12.0
scripts/agent_island.py:932:             item["progress"] = min(0.94, max(base_progress, 0.22 + phase * 0.66))
scripts/agent_island.py:933:         sessions.append(item)
scripts/agent_island.py:934:     sessions.sort(key=lambda s: (status_priority(str(s.get("status") or "")), float(s.get("last_activity") or 0)), reverse=True)
scripts/agent_island.py:935: 
scripts/agent_island.py:936:     pending = []
scripts/agent_island.py:937:     for request in state.get("pending_requests", {}).values():
scripts/agent_island.py:938:         if isinstance(request, dict):
scripts/agent_island.py:939:             pending.append(dict(request))
scripts/agent_island.py:940:     pending.sort(key=lambda r: float(r.get("created_at") or 0))
scripts/agent_island.py:941: 
scripts/agent_island.py:942:     return {
scripts/agent_island.py:943:         "version": STATE_VERSION,
scripts/agent_island.py:944:         "active": bool(sessions),
scripts/agent_island.py:945:         "count": len(sessions),
scripts/agent_island.py:946:         "sessions": sessions,
scripts/agent_island.py:947:         "pending": pending[0] if pending else None,
scripts/agent_island.py:948:         "pending_count": len(pending),
scripts/agent_island.py:949:         "updated_at": state.get("updated_at", 0),
scripts/agent_island.py:950:     }
```

### `scripts/apply-ui-state.sh`

Migration owner: `theme`. Source `scripts/apply-ui-state.sh:1`, 716 lines, SHA-256 `f553d652c7434405815dab44724d8f51a602b8a0c5a5fe033fa769bb2f0ddfb9`.

```text
…
scripts/apply-ui-state.sh:5: script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
scripts/apply-ui-state.sh:6: state_script="$script_dir/ui-state.sh"
scripts/apply-ui-state.sh:7: 
scripts/apply-ui-state.sh:8: ensure_parent() {
scripts/apply-ui-state.sh:9:     mkdir -p "$(dirname -- "$1")"
scripts/apply-ui-state.sh:10: }
scripts/apply-ui-state.sh:11: 
scripts/apply-ui-state.sh:12: set_ini_value() {
scripts/apply-ui-state.sh:13:     local file section key value tmp
scripts/apply-ui-state.sh:14:     file="$1"
…
scripts/apply-ui-state.sh:18: 
scripts/apply-ui-state.sh:19:     ensure_parent "$file"
scripts/apply-ui-state.sh:20:     touch "$file"
scripts/apply-ui-state.sh:21:     tmp="$(mktemp)"
scripts/apply-ui-state.sh:22: 
scripts/apply-ui-state.sh:23:     awk -v section="$section" -v key="$key" -v value="$value" '
scripts/apply-ui-state.sh:24:         BEGIN {
scripts/apply-ui-state.sh:25:             target = "[" section "]"
…
scripts/apply-ui-state.sh:68:         }
scripts/apply-ui-state.sh:69:     ' "$file" > "$tmp"
scripts/apply-ui-state.sh:70: 
scripts/apply-ui-state.sh:71:     mv "$tmp" "$file"
scripts/apply-ui-state.sh:72: }
scripts/apply-ui-state.sh:73: 
scripts/apply-ui-state.sh:74: set_assignment() {
scripts/apply-ui-state.sh:75:     local file key value tmp
scripts/apply-ui-state.sh:76:     file="$1"
…
scripts/apply-ui-state.sh:79: 
scripts/apply-ui-state.sh:80:     ensure_parent "$file"
scripts/apply-ui-state.sh:81:     touch "$file"
scripts/apply-ui-state.sh:82:     tmp="$(mktemp)"
scripts/apply-ui-state.sh:83: 
scripts/apply-ui-state.sh:84:     awk -v key="$key" -v value="$value" '
scripts/apply-ui-state.sh:85:         BEGIN {
scripts/apply-ui-state.sh:86:             written = 0
…
scripts/apply-ui-state.sh:102:         }
scripts/apply-ui-state.sh:103:     ' "$file" > "$tmp"
scripts/apply-ui-state.sh:104: 
scripts/apply-ui-state.sh:105:     mv "$tmp" "$file"
scripts/apply-ui-state.sh:106: }
scripts/apply-ui-state.sh:107: 
scripts/apply-ui-state.sh:108: set_xsettings_value() {
scripts/apply-ui-state.sh:109:     local file key value tmp
scripts/apply-ui-state.sh:110:     file="$1"
…
scripts/apply-ui-state.sh:113: 
scripts/apply-ui-state.sh:114:     ensure_parent "$file"
scripts/apply-ui-state.sh:115:     touch "$file"
scripts/apply-ui-state.sh:116:     tmp="$(mktemp)"
scripts/apply-ui-state.sh:117: 
scripts/apply-ui-state.sh:118:     awk -v key="$key" -v value="$value" '
scripts/apply-ui-state.sh:119:         BEGIN {
scripts/apply-ui-state.sh:120:             written = 0
…
scripts/apply-ui-state.sh:136:         }
scripts/apply-ui-state.sh:137:     ' "$file" > "$tmp"
scripts/apply-ui-state.sh:138: 
scripts/apply-ui-state.sh:139:     mv "$tmp" "$file"
scripts/apply-ui-state.sh:140: }
scripts/apply-ui-state.sh:141: 
scripts/apply-ui-state.sh:142: restart_xsettingsd() {
scripts/apply-ui-state.sh:143:     if ! command -v xsettingsd >/dev/null 2>&1; then
scripts/apply-ui-state.sh:144:         return 0
scripts/apply-ui-state.sh:145:     fi
scripts/apply-ui-state.sh:146: 
scripts/apply-ui-state.sh:147:     pkill -x xsettingsd >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:148: 
scripts/apply-ui-state.sh:149:     if [[ -n "${DISPLAY:-}" ]]; then
scripts/apply-ui-state.sh:150:         nohup xsettingsd >/dev/null 2>&1 &
scripts/apply-ui-state.sh:151:     fi
scripts/apply-ui-state.sh:152: }
scripts/apply-ui-state.sh:153: 
scripts/apply-ui-state.sh:154: sync_activation_environment() {
scripts/apply-ui-state.sh:155:     local gtk_theme_name cursor_theme_name cursor_size
scripts/apply-ui-state.sh:156:     gtk_theme_name="$1"
…
scripts/apply-ui-state.sh:159: 
scripts/apply-ui-state.sh:160:     if command -v dbus-update-activation-environment >/dev/null 2>&1; then
scripts/apply-ui-state.sh:161:         dbus-update-activation-environment --systemd \
scripts/apply-ui-state.sh:162:             "GTK_THEME=$gtk_theme_name" \
scripts/apply-ui-state.sh:163:             "QT_QPA_PLATFORMTHEME=qt6ct" \
scripts/apply-ui-state.sh:164:             "XCURSOR_THEME=$cursor_theme_name" \
scripts/apply-ui-state.sh:165:             "XCURSOR_SIZE=$cursor_size" \
scripts/apply-ui-state.sh:166:             "XDG_CURRENT_DESKTOP=${XDG_CURRENT_DESKTOP:-Hyprland}" \
scripts/apply-ui-state.sh:167:             "XDG_SESSION_DESKTOP=${XDG_SESSION_DESKTOP:-Hyprland}" >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:168:     fi
scripts/apply-ui-state.sh:169: }
scripts/apply-ui-state.sh:170: 
scripts/apply-ui-state.sh:171: theme_suffix() {
scripts/apply-ui-state.sh:172:     case "${1:-}" in
scripts/apply-ui-state.sh:173:         dark)
scripts/apply-ui-state.sh:174:             printf '%s\n' '-dark'
scripts/apply-ui-state.sh:175:             ;;
scripts/apply-ui-state.sh:176:         light)
scripts/apply-ui-state.sh:177:             printf '%s\n' ''
scripts/apply-ui-state.sh:178:             ;;
scripts/apply-ui-state.sh:179:         *)
scripts/apply-ui-state.sh:180:             return 1
scripts/apply-ui-state.sh:181:             ;;
…
scripts/apply-ui-state.sh:184: 
scripts/apply-ui-state.sh:185: theme_label() {
scripts/apply-ui-state.sh:186:     case "${1:-}" in
scripts/apply-ui-state.sh:187:         dark)
scripts/apply-ui-state.sh:188:             printf 'Dark\n'
scripts/apply-ui-state.sh:189:             ;;
scripts/apply-ui-state.sh:190:         light)
scripts/apply-ui-state.sh:191:             printf 'Light\n'
scripts/apply-ui-state.sh:192:             ;;
scripts/apply-ui-state.sh:193:         *)
scripts/apply-ui-state.sh:194:             return 1
scripts/apply-ui-state.sh:195:             ;;
…
scripts/apply-ui-state.sh:198: 
scripts/apply-ui-state.sh:199: variant_prefix() {
scripts/apply-ui-state.sh:200:     case "${1:-}" in
scripts/apply-ui-state.sh:201:         v1|v2|v3|bt)
scripts/apply-ui-state.sh:202:             printf '%s\n' "$1"
scripts/apply-ui-state.sh:203:             ;;
scripts/apply-ui-state.sh:204:         *)
scripts/apply-ui-state.sh:205:             return 1
scripts/apply-ui-state.sh:206:             ;;
…
scripts/apply-ui-state.sh:209: 
scripts/apply-ui-state.sh:210: variant_label() {
scripts/apply-ui-state.sh:211:     case "${1:-}" in
scripts/apply-ui-state.sh:212:         v1)
scripts/apply-ui-state.sh:213:             printf 'HyprV1\n'
scripts/apply-ui-state.sh:214:             ;;
scripts/apply-ui-state.sh:215:         v2)
scripts/apply-ui-state.sh:216:             printf 'HyprV2\n'
scripts/apply-ui-state.sh:217:             ;;
scripts/apply-ui-state.sh:218:         v3)
scripts/apply-ui-state.sh:219:             printf 'HyprV3\n'
scripts/apply-ui-state.sh:220:             ;;
scripts/apply-ui-state.sh:221:         bt)
scripts/apply-ui-state.sh:222:             printf 'HyprV Battery\n'
scripts/apply-ui-state.sh:223:             ;;
scripts/apply-ui-state.sh:224:         *)
scripts/apply-ui-state.sh:225:             return 1
scripts/apply-ui-state.sh:226:             ;;
…
scripts/apply-ui-state.sh:229: 
scripts/apply-ui-state.sh:230: notify_ui_state() {
scripts/apply-ui-state.sh:231:     if command -v notify-send >/dev/null 2>&1; then
scripts/apply-ui-state.sh:232:         notify-send -h string:x-canonical-private-synchronous:sys-notify -u low "$1"
scripts/apply-ui-state.sh:233:     fi
scripts/apply-ui-state.sh:234: }
scripts/apply-ui-state.sh:235: 
scripts/apply-ui-state.sh:236: run_with_timeout() {
scripts/apply-ui-state.sh:237:     local seconds
scripts/apply-ui-state.sh:238:     seconds="$1"
…
scripts/apply-ui-state.sh:240: 
scripts/apply-ui-state.sh:241:     if command -v timeout >/dev/null 2>&1; then
scripts/apply-ui-state.sh:242:         timeout "$seconds" "$@" >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:243:     else
scripts/apply-ui-state.sh:244:         "$@" >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:245:     fi
scripts/apply-ui-state.sh:246: }
scripts/apply-ui-state.sh:247: 
scripts/apply-ui-state.sh:248: send_ghostty_reload_shortcut() {
scripts/apply-ui-state.sh:249:     local addr
scripts/apply-ui-state.sh:250:     addr="$1"
scripts/apply-ui-state.sh:251: 
scripts/apply-ui-state.sh:252:     [[ "$addr" =~ ^0x[0-9A-Fa-f]+$ ]] || return 0
scripts/apply-ui-state.sh:253: 
scripts/apply-ui-state.sh:254:     if command -v hyprctl >/dev/null 2>&1; then
scripts/apply-ui-state.sh:255:         hyprctl eval "hl.dispatch(hl.dsp.send_shortcut({ mods = 'CTRL SHIFT', key = 'comma', window = 'address:$addr' }))" >/dev/null 2>&1 \
scripts/apply-ui-state.sh:256:             || hyprctl dispatch sendshortcut "CTRL SHIFT, comma, address:$addr" >/dev/null 2>&1 \
scripts/apply-ui-state.sh:257:             || true
scripts/apply-ui-state.sh:258:     fi
…
scripts/apply-ui-state.sh:260: 
scripts/apply-ui-state.sh:261: lua_quote() {
scripts/apply-ui-state.sh:262:     [[ ${1:-} != *"'"* ]] || return 1
scripts/apply-ui-state.sh:263:     printf "'%s'" "$1"
scripts/apply-ui-state.sh:264: }
scripts/apply-ui-state.sh:265: 
scripts/apply-ui-state.sh:266: hypr_move_window_hidden() {
scripts/apply-ui-state.sh:267:     local addr workspace window_q workspace_q
scripts/apply-ui-state.sh:268:     addr="$1"
…
scripts/apply-ui-state.sh:270: 
scripts/apply-ui-state.sh:271:     [[ "$addr" =~ ^0x[0-9A-Fa-f]+$ ]] || return 1
scripts/apply-ui-state.sh:272:     command -v hyprctl >/dev/null 2>&1 || return 1
scripts/apply-ui-state.sh:273: 
scripts/apply-ui-state.sh:274:     window_q="$(lua_quote "address:$addr")" || return 1
scripts/apply-ui-state.sh:275:     workspace_q="$(lua_quote "$workspace")" || return 1
scripts/apply-ui-state.sh:276: 
scripts/apply-ui-state.sh:277:     hyprctl dispatch "hl.dsp.window.move({ workspace = ${workspace_q}, window = ${window_q}, follow = false })" >/dev/null 2>&1 \
scripts/apply-ui-state.sh:278:         || hyprctl dispatch movetoworkspacesilent "${workspace},address:${addr}" >/dev/null 2>&1
scripts/apply-ui-state.sh:279: }
scripts/apply-ui-state.sh:280: 
scripts/apply-ui-state.sh:281: hypr_focus_window_addr() {
scripts/apply-ui-state.sh:282:     local addr window_q
scripts/apply-ui-state.sh:283:     addr="$1"
scripts/apply-ui-state.sh:284: 
scripts/apply-ui-state.sh:285:     [[ "$addr" =~ ^0x[0-9A-Fa-f]+$ ]] || return 1
scripts/apply-ui-state.sh:286:     command -v hyprctl >/dev/null 2>&1 || return 1
scripts/apply-ui-state.sh:287: 
scripts/apply-ui-state.sh:288:     window_q="$(lua_quote "address:$addr")" || return 1
scripts/apply-ui-state.sh:289:     hyprctl dispatch "hl.dsp.focus({ window = ${window_q} })" >/dev/null 2>&1 \
scripts/apply-ui-state.sh:290:         || hyprctl dispatch focuswindow "address:$addr" >/dev/null 2>&1
scripts/apply-ui-state.sh:291: }
scripts/apply-ui-state.sh:292: 
scripts/apply-ui-state.sh:293: wechat_hypr_client_address_by_title() {
scripts/apply-ui-state.sh:294:     local title
scripts/apply-ui-state.sh:295:     title="$1"
scripts/apply-ui-state.sh:296: 
scripts/apply-ui-state.sh:297:     hyprctl clients -j 2>/dev/null | jq -r --arg title "$title" '
scripts/apply-ui-state.sh:298:         .[]
scripts/apply-ui-state.sh:299:         | select((.class // "") == "wechat" and (.title // "") == $title)
scripts/apply-ui-state.sh:300:         | .address
scripts/apply-ui-state.sh:301:     ' | tail -n 1
…
scripts/apply-ui-state.sh:303: 
scripts/apply-ui-state.sh:304: wechat_popup_hypr_address() {
scripts/apply-ui-state.sh:305:     hyprctl clients -j 2>/dev/null | jq -r '
scripts/apply-ui-state.sh:306:         .[]
scripts/apply-ui-state.sh:307:         | select(
…
scripts/apply-ui-state.sh:317: 
scripts/apply-ui-state.sh:318: wechat_xwindow_by_title() {
scripts/apply-ui-state.sh:319:     local title
scripts/apply-ui-state.sh:320:     title="$1"
scripts/apply-ui-state.sh:321: 
scripts/apply-ui-state.sh:322:     xdotool search --class wechat 2>/dev/null | while read -r wid; do
scripts/apply-ui-state.sh:323:         [[ "$(xdotool getwindowname "$wid" 2>/dev/null || true)" == "$title" ]] || continue
scripts/apply-ui-state.sh:324:         printf '%s\n' "$wid"
scripts/apply-ui-state.sh:325:     done | tail -n 1
scripts/apply-ui-state.sh:326: }
scripts/apply-ui-state.sh:327: 
scripts/apply-ui-state.sh:328: wechat_popup_xwindow() {
scripts/apply-ui-state.sh:329:     local name width height
scripts/apply-ui-state.sh:330: 
scripts/apply-ui-state.sh:331:     xdotool search --class wechat 2>/dev/null | while read -r wid; do
scripts/apply-ui-state.sh:332:         name="$(xdotool getwindowname "$wid" 2>/dev/null || true)"
scripts/apply-ui-state.sh:333:         [[ "$name" == "wechat" ]] || continue
scripts/apply-ui-state.sh:334:         width="$(xwininfo -id "$wid" 2>/dev/null | awk '/Width:/ { print $2; exit }')"
scripts/apply-ui-state.sh:335:         height="$(xwininfo -id "$wid" 2>/dev/null | awk '/Height:/ { print $2; exit }')"
scripts/apply-ui-state.sh:336:         [[ "$width" =~ ^[0-9]+$ && "$height" =~ ^[0-9]+$ ]] || continue
scripts/apply-ui-state.sh:337:         if (( width >= 250 && width <= 420 && height >= 150 && height <= 300 )); then
scripts/apply-ui-state.sh:338:             printf '%s\n' "$wid"
scripts/apply-ui-state.sh:339:         fi
scripts/apply-ui-state.sh:340:     done | tail -n 1
…
scripts/apply-ui-state.sh:342: 
scripts/apply-ui-state.sh:343: wait_for_wechat_settings() {
scripts/apply-ui-state.sh:344:     local i addr
scripts/apply-ui-state.sh:345: 
…
scripts/apply-ui-state.sh:347:         addr="$(wechat_hypr_client_address_by_title "设置")"
scripts/apply-ui-state.sh:348:         if [[ -n "$addr" ]]; then
scripts/apply-ui-state.sh:349:             printf '%s\n' "$addr"
scripts/apply-ui-state.sh:350:             return 0
scripts/apply-ui-state.sh:351:         fi
scripts/apply-ui-state.sh:352:         sleep 0.1
scripts/apply-ui-state.sh:353:     done
scripts/apply-ui-state.sh:354: 
…
scripts/apply-ui-state.sh:357: 
scripts/apply-ui-state.sh:358: wait_for_wechat_popup() {
scripts/apply-ui-state.sh:359:     local i addr
scripts/apply-ui-state.sh:360: 
…
scripts/apply-ui-state.sh:362:         addr="$(wechat_popup_hypr_address)"
scripts/apply-ui-state.sh:363:         if [[ -n "$addr" ]]; then
scripts/apply-ui-state.sh:364:             printf '%s\n' "$addr"
scripts/apply-ui-state.sh:365:             return 0
scripts/apply-ui-state.sh:366:         fi
scripts/apply-ui-state.sh:367:         sleep 0.05
scripts/apply-ui-state.sh:368:     done
scripts/apply-ui-state.sh:369: 
…
scripts/apply-ui-state.sh:372: 
scripts/apply-ui-state.sh:373: sync_wechat_appearance() {
scripts/apply-ui-state.sh:374:     local target_theme hidden_workspace active_addr main_wid settings_wid settings_addr popup_addr popup_wid menu_y
scripts/apply-ui-state.sh:375: 
scripts/apply-ui-state.sh:376:     target_theme="${1:-}"
scripts/apply-ui-state.sh:377:     [[ "$target_theme" == "light" || "$target_theme" == "dark" ]] || return 0
scripts/apply-ui-state.sh:378: 
scripts/apply-ui-state.sh:379:     command -v xdotool >/dev/null 2>&1 || return 0
scripts/apply-ui-state.sh:380:     command -v xwininfo >/dev/null 2>&1 || return 0
scripts/apply-ui-state.sh:381:     command -v hyprctl >/dev/null 2>&1 || return 0
scripts/apply-ui-state.sh:382:     command -v jq >/dev/null 2>&1 || return 0
scripts/apply-ui-state.sh:383: 
scripts/apply-ui-state.sh:384:     main_wid="$(wechat_xwindow_by_title "微信")"
scripts/apply-ui-state.sh:385:     [[ -n "$main_wid" ]] || return 0
scripts/apply-ui-state.sh:386: 
scripts/apply-ui-state.sh:387:     hidden_workspace="special:wechat-ui"
scripts/apply-ui-state.sh:388:     active_addr="$(hyprctl activewindow -j 2>/dev/null | jq -r '.address // empty' || true)"
scripts/apply-ui-state.sh:389: 
scripts/apply-ui-state.sh:390:     settings_wid="$(wechat_xwindow_by_title "设置")"
scripts/apply-ui-state.sh:391:     settings_addr="$(wechat_hypr_client_address_by_title "设置")"
scripts/apply-ui-state.sh:392:     if [[ -z "$settings_wid" || -z "$settings_addr" ]]; then
scripts/apply-ui-state.sh:393:         xdotool key --window "$main_wid" ctrl+comma >/dev/null 2>&1 || return 0
scripts/apply-ui-state.sh:394:         settings_addr="$(wait_for_wechat_settings || true)"
scripts/apply-ui-state.sh:395:         settings_wid="$(wechat_xwindow_by_title "设置")"
…
scripts/apply-ui-state.sh:397: 
scripts/apply-ui-state.sh:398:     [[ -n "$settings_wid" && -n "$settings_addr" ]] || return 0
scripts/apply-ui-state.sh:399: 
scripts/apply-ui-state.sh:400:     hypr_move_window_hidden "$settings_addr" "$hidden_workspace" || true
scripts/apply-ui-state.sh:401:     if [[ -n "$active_addr" ]]; then
scripts/apply-ui-state.sh:402:         hypr_focus_window_addr "$active_addr" || true
scripts/apply-ui-state.sh:403:     fi
scripts/apply-ui-state.sh:404: 
scripts/apply-ui-state.sh:405:     # Coordinates are XWayland window pixels from the current WeChat settings UI.
scripts/apply-ui-state.sh:406:     xdotool mousemove --window "$settings_wid" 120 225 click 1 >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:407:     sleep 0.2
scripts/apply-ui-state.sh:408:     xdotool mousemove --window "$settings_wid" 910 490 click 1 >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:409: 
scripts/apply-ui-state.sh:410:     popup_addr="$(wait_for_wechat_popup || true)"
scripts/apply-ui-state.sh:411:     if [[ -z "$popup_addr" ]]; then
scripts/apply-ui-state.sh:412:         cleanup_wechat_settings_window "$active_addr"
scripts/apply-ui-state.sh:413:         return 0
scripts/apply-ui-state.sh:414:     fi
scripts/apply-ui-state.sh:415: 
scripts/apply-ui-state.sh:416:     if [[ -n "$popup_addr" ]]; then
scripts/apply-ui-state.sh:417:         hypr_move_window_hidden "$popup_addr" "$hidden_workspace" || true
scripts/apply-ui-state.sh:418:     fi
scripts/apply-ui-state.sh:419:     if [[ -n "$active_addr" ]]; then
scripts/apply-ui-state.sh:420:         hypr_focus_window_addr "$active_addr" || true
scripts/apply-ui-state.sh:421:     fi
scripts/apply-ui-state.sh:422: 
scripts/apply-ui-state.sh:423:     sleep 0.1
scripts/apply-ui-state.sh:424:     popup_wid="$(wechat_popup_xwindow)"
scripts/apply-ui-state.sh:425:     if [[ -z "$popup_wid" ]]; then
scripts/apply-ui-state.sh:426:         cleanup_wechat_settings_window "$active_addr"
scripts/apply-ui-state.sh:427:         return 0
scripts/apply-ui-state.sh:428:     fi
scripts/apply-ui-state.sh:429: 
scripts/apply-ui-state.sh:430:     if [[ "$target_theme" == "dark" ]]; then
scripts/apply-ui-state.sh:431:         menu_y=150
scripts/apply-ui-state.sh:432:     else
…
scripts/apply-ui-state.sh:435: 
scripts/apply-ui-state.sh:436:     xdotool mousemove --window "$popup_wid" 150 "$menu_y" click 1 >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:437:     sleep 0.3
scripts/apply-ui-state.sh:438: 
scripts/apply-ui-state.sh:439:     # Close the hidden settings window so repeated theme applications start cleanly.
scripts/apply-ui-state.sh:440:     cleanup_wechat_settings_window "$active_addr"
scripts/apply-ui-state.sh:441: }
scripts/apply-ui-state.sh:442: 
scripts/apply-ui-state.sh:443: cleanup_wechat_settings_window() {
scripts/apply-ui-state.sh:444:     local active_addr settings_wid
scripts/apply-ui-state.sh:445:     active_addr="${1:-}"
…
scripts/apply-ui-state.sh:447:     settings_wid="$(wechat_xwindow_by_title "设置")"
scripts/apply-ui-state.sh:448:     if [[ -n "$settings_wid" ]]; then
scripts/apply-ui-state.sh:449:         xdotool windowclose "$settings_wid" >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:450:     fi
scripts/apply-ui-state.sh:451:     if [[ -n "$active_addr" ]]; then
scripts/apply-ui-state.sh:452:         hypr_focus_window_addr "$active_addr" || true
scripts/apply-ui-state.sh:453:     fi
scripts/apply-ui-state.sh:454: }
scripts/apply-ui-state.sh:455: 
scripts/apply-ui-state.sh:456: load_current_state() {
scripts/apply-ui-state.sh:457:     theme="$("$state_script" get-theme)"
scripts/apply-ui-state.sh:458:     variant="$("$state_script" get-variant)"
scripts/apply-ui-state.sh:459:     suffix="$(theme_suffix "$theme")"
scripts/apply-ui-state.sh:460:     mode_label="$(theme_label "$theme")"
scripts/apply-ui-state.sh:461:     variant_name="$(variant_prefix "$variant")"
scripts/apply-ui-state.sh:462: }
scripts/apply-ui-state.sh:463: 
scripts/apply-ui-state.sh:464: reload_kitty_theme() {
scripts/apply-ui-state.sh:465:     # Live-apply colors via remote control sockets, then SIGUSR1 for full reload.
scripts/apply-ui-state.sh:466:     local theme_file sock
…
scripts/apply-ui-state.sh:468: 
scripts/apply-ui-state.sh:469:     if command -v kitty >/dev/null 2>&1 && [[ -f "$theme_file" ]]; then
scripts/apply-ui-state.sh:470:         shopt -s nullglob
scripts/apply-ui-state.sh:471:         # New sockets live in XDG_RUNTIME_DIR; keep /tmp for instances started before the move
scripts/apply-ui-state.sh:472:         for sock in "${XDG_RUNTIME_DIR:-/tmp}"/kitty-*.sock "${TEMP:-/tmp}"/kitty-*.sock; do
scripts/apply-ui-state.sh:473:             run_with_timeout 2 kitty @ --to "unix:${sock}" set-colors --all --configured "$theme_file" >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:474:             run_with_timeout 2 kitty @ --to "unix:${sock}" load-config >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:475:         done
scripts/apply-ui-state.sh:476:         shopt -u nullglob
…
scripts/apply-ui-state.sh:478: 
scripts/apply-ui-state.sh:479:     if command -v pkill >/dev/null 2>&1; then
scripts/apply-ui-state.sh:480:         pkill -USR1 -x kitty >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:481:     fi
scripts/apply-ui-state.sh:482: }
scripts/apply-ui-state.sh:483: 
scripts/apply-ui-state.sh:484: apply_links_and_theme() {
scripts/apply-ui-state.sh:485:     local wofi_target background_target rofi_target alacritty_target ghostty_target kitty_target kitty_conf_target swaync_target
scripts/apply-ui-state.sh:486:     local orchis_suffix color_scheme_suffix prefer_dark
…
scripts/apply-ui-state.sh:492: 
scripts/apply-ui-state.sh:493:     wofi_target="$HOME/.config/HyprV/wofi/style/${variant_name}-style${suffix}.css"
scripts/apply-ui-state.sh:494:     background_target="$HOME/.config/HyprV/backgrounds/${variant_name}-background${suffix}.jpg"
scripts/apply-ui-state.sh:495:     rofi_target="$HOME/.config/HyprV/rofi/colors${suffix}.rasi"
scripts/apply-ui-state.sh:496:     alacritty_target="$HOME/.config/HyprV/alacritty/alacritty${suffix}.toml"
scripts/apply-ui-state.sh:497:     ghostty_target="$HOME/.config/HyprV/ghostty/ghostty${suffix}.toml"
scripts/apply-ui-state.sh:498:     kitty_target="$HOME/.config/HyprV/kitty/theme${suffix}.conf"
scripts/apply-ui-state.sh:499:     kitty_conf_target="$HOME/.config/HyprV/kitty/kitty.conf"
scripts/apply-ui-state.sh:500:     swaync_target="$HOME/.config/HyprV/swaync/style${suffix}.css"
scripts/apply-ui-state.sh:501: 
scripts/apply-ui-state.sh:502:     ensure_parent "$HOME/.config/wofi/style.css"
scripts/apply-ui-state.sh:503:     ensure_parent "$HOME/.config/rofi/colors.rasi"
scripts/apply-ui-state.sh:504:     ensure_parent "$HOME/.config/alacritty/alacritty.toml"
scripts/apply-ui-state.sh:505:     ensure_parent "$HOME/.config/ghostty/config"
scripts/apply-ui-state.sh:506:     ensure_parent "$HOME/.config/kitty/kitty.conf"
scripts/apply-ui-state.sh:507:     ensure_parent "$HOME/.config/swaync/style.css"
scripts/apply-ui-state.sh:508: 
scripts/apply-ui-state.sh:509:     if [[ -f "$wofi_target" ]]; then
scripts/apply-ui-state.sh:510:         ln -sfn "$wofi_target" "$HOME/.config/wofi/style.css"
scripts/apply-ui-state.sh:511:     fi
scripts/apply-ui-state.sh:512: 
scripts/apply-ui-state.sh:513:     if [[ -f "$rofi_target" ]]; then
scripts/apply-ui-state.sh:514:         ln -sfn "$rofi_target" "$HOME/.config/rofi/colors.rasi"
scripts/apply-ui-state.sh:515:     fi
scripts/apply-ui-state.sh:516: 
scripts/apply-ui-state.sh:517:     if [[ -f "$alacritty_target" ]]; then
scripts/apply-ui-state.sh:518:         ln -sfn "$alacritty_target" "$HOME/.config/alacritty/alacritty.toml"
scripts/apply-ui-state.sh:519:     fi
scripts/apply-ui-state.sh:520: 
scripts/apply-ui-state.sh:521:     if [[ -f "$ghostty_target" ]]; then
scripts/apply-ui-state.sh:522:         ln -sfn "$ghostty_target" "$HOME/.config/ghostty/config"
scripts/apply-ui-state.sh:523:     fi
scripts/apply-ui-state.sh:524: 
scripts/apply-ui-state.sh:525:     if [[ -f "$kitty_conf_target" ]]; then
scripts/apply-ui-state.sh:526:         ln -sfn "$kitty_conf_target" "$HOME/.config/kitty/kitty.conf"
scripts/apply-ui-state.sh:527:     fi
scripts/apply-ui-state.sh:528: 
scripts/apply-ui-state.sh:529:     if [[ -f "$kitty_target" ]]; then
scripts/apply-ui-state.sh:530:         ln -sfn "$kitty_target" "$HOME/.config/kitty/current-theme.conf"
scripts/apply-ui-state.sh:531:         reload_kitty_theme "$kitty_target"
scripts/apply-ui-state.sh:532:     fi
scripts/apply-ui-state.sh:533: 
scripts/apply-ui-state.sh:534:     if [[ -f "$swaync_target" ]]; then
scripts/apply-ui-state.sh:535:         ln -sfn "$swaync_target" "$HOME/.config/swaync/style.css"
scripts/apply-ui-state.sh:536:         if command -v swaync-client >/dev/null 2>&1; then
scripts/apply-ui-state.sh:537:             run_with_timeout 2 swaync-client -rs
scripts/apply-ui-state.sh:538:         fi
scripts/apply-ui-state.sh:539:     fi
scripts/apply-ui-state.sh:540: 
scripts/apply-ui-state.sh:541:     if command -v hyprctl >/dev/null 2>&1 && command -v jq >/dev/null 2>&1; then
scripts/apply-ui-state.sh:542:         hypr_clients_json="$(hyprctl clients -j 2>/dev/null || true)"
scripts/apply-ui-state.sh:543:         if [[ -n "$hypr_clients_json" ]] && printf '%s' "$hypr_clients_json" | jq -e . >/dev/null 2>&1; then
scripts/apply-ui-state.sh:544:             printf '%s' "$hypr_clients_json" | jq -r '
scripts/apply-ui-state.sh:545:                 .[]
scripts/apply-ui-state.sh:546:                 | select(
…
scripts/apply-ui-state.sh:551:             ' | while read -r addr; do
scripts/apply-ui-state.sh:552:                 [[ -n "$addr" ]] || continue
scripts/apply-ui-state.sh:553:                 send_ghostty_reload_shortcut "$addr"
scripts/apply-ui-state.sh:554:             done
scripts/apply-ui-state.sh:555:         fi
…
scripts/apply-ui-state.sh:564:     kde_look_and_feel="com.github.vinceliuice.Orchis"
scripts/apply-ui-state.sh:565:     if [[ "$theme" == "dark" ]]; then
scripts/apply-ui-state.sh:566:         orchis_suffix="Dark"
scripts/apply-ui-state.sh:567:         color_scheme_suffix="-dark"
…
scripts/apply-ui-state.sh:580: 
scripts/apply-ui-state.sh:581:     if command -v xfconf-query >/dev/null 2>&1; then
scripts/apply-ui-state.sh:582:         xfconf-query -c xsettings -p /Net/ThemeName -s "Adwaita${suffix}" >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:583:         xfconf-query -c xsettings -p /Net/IconThemeName -s "Adwaita${suffix}" >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:584:         xfconf-query -c xsettings -p /Gtk/CursorThemeName -s "$cursor_theme_name" >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:585:         xfconf-query -c xsettings -p /Gtk/CursorThemeSize -s "$cursor_size" >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:586:     fi
scripts/apply-ui-state.sh:587: 
scripts/apply-ui-state.sh:588:     if command -v gsettings >/dev/null 2>&1; then
scripts/apply-ui-state.sh:589:         gsettings set org.gnome.desktop.interface gtk-theme "$gtk_theme_name" >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:590:         gsettings set org.gnome.desktop.interface icon-theme "$icon_theme_name" >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:591:         gsettings set org.gnome.desktop.interface cursor-theme "$cursor_theme_name" >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:592:         gsettings set org.gnome.desktop.interface cursor-size "$cursor_size" >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:593:         gsettings set org.gnome.desktop.interface color-scheme "prefer${color_scheme_suffix}" >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:594:     fi
scripts/apply-ui-state.sh:595:     sync_activation_environment "$gtk_theme_name" "$cursor_theme_name" "$cursor_size"
scripts/apply-ui-state.sh:596:     if command -v hyprctl >/dev/null 2>&1; then
scripts/apply-ui-state.sh:597:         hyprctl setcursor "$cursor_theme_name" "$cursor_size" >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:598:     fi
scripts/apply-ui-state.sh:599: 
scripts/apply-ui-state.sh:600:     set_ini_value "$HOME/.config/gtk-3.0/settings.ini" Settings gtk-theme-name "$gtk_theme_name"
scripts/apply-ui-state.sh:601:     set_ini_value "$HOME/.config/gtk-3.0/settings.ini" Settings gtk-icon-theme-name "$icon_theme_name"
scripts/apply-ui-state.sh:602:     set_ini_value "$HOME/.config/gtk-3.0/settings.ini" Settings gtk-cursor-theme-name "$cursor_theme_name"
scripts/apply-ui-state.sh:603:     set_ini_value "$HOME/.config/gtk-3.0/settings.ini" Settings gtk-cursor-theme-size "$cursor_size"
scripts/apply-ui-state.sh:604:     set_ini_value "$HOME/.config/gtk-3.0/settings.ini" Settings gtk-application-prefer-dark-theme "$prefer_dark"
scripts/apply-ui-state.sh:605:     set_ini_value "$HOME/.config/gtk-4.0/settings.ini" Settings gtk-theme-name "$gtk_theme_name"
scripts/apply-ui-state.sh:606:     set_ini_value "$HOME/.config/gtk-4.0/settings.ini" Settings gtk-icon-theme-name "$icon_theme_name"
scripts/apply-ui-state.sh:607:     set_ini_value "$HOME/.config/gtk-4.0/settings.ini" Settings gtk-cursor-theme-name "$cursor_theme_name"
scripts/apply-ui-state.sh:608:     set_ini_value "$HOME/.config/gtk-4.0/settings.ini" Settings gtk-cursor-theme-size "$cursor_size"
scripts/apply-ui-state.sh:609:     set_ini_value "$HOME/.config/gtk-4.0/settings.ini" Settings gtk-application-prefer-dark-theme "$prefer_dark"
scripts/apply-ui-state.sh:610:     set_assignment "$HOME/.gtkrc-2.0" gtk-theme-name "\"$gtk_theme_name\""
scripts/apply-ui-state.sh:611:     set_assignment "$HOME/.gtkrc-2.0" gtk-icon-theme-name "\"$icon_theme_name\""
scripts/apply-ui-state.sh:612:     set_assignment "$HOME/.gtkrc-2.0" gtk-cursor-theme-name "\"$cursor_theme_name\""
scripts/apply-ui-state.sh:613:     set_assignment "$HOME/.gtkrc-2.0" gtk-cursor-theme-size "$cursor_size"
scripts/apply-ui-state.sh:614:     set_assignment "$HOME/.config/gtkrc-2.0" gtk-theme-name "\"$gtk_theme_name\""
scripts/apply-ui-state.sh:615:     set_assignment "$HOME/.config/gtkrc-2.0" gtk-icon-theme-name "\"$icon_theme_name\""
scripts/apply-ui-state.sh:616:     set_assignment "$HOME/.config/gtkrc-2.0" gtk-cursor-theme-name "\"$cursor_theme_name\""
scripts/apply-ui-state.sh:617:     set_assignment "$HOME/.config/gtkrc-2.0" gtk-cursor-theme-size "$cursor_size"
scripts/apply-ui-state.sh:618: 
scripts/apply-ui-state.sh:619:     set_xsettings_value "$HOME/.config/xsettingsd/xsettingsd.conf" Net/ThemeName "\"$gtk_theme_name\""
scripts/apply-ui-state.sh:620:     set_xsettings_value "$HOME/.config/xsettingsd/xsettingsd.conf" Net/IconThemeName "\"$icon_theme_name\""
scripts/apply-ui-state.sh:621:     set_xsettings_value "$HOME/.config/xsettingsd/xsettingsd.conf" Net/CursorThemeName "\"$cursor_theme_name\""
scripts/apply-ui-state.sh:622:     set_xsettings_value "$HOME/.config/xsettingsd/xsettingsd.conf" Net/CursorThemeSize "$cursor_size"
scripts/apply-ui-state.sh:623:     restart_xsettingsd
scripts/apply-ui-state.sh:624: 
scripts/apply-ui-state.sh:625:     set_ini_value "$HOME/.config/qt6ct/qt6ct.conf" Appearance style "kvantum"
scripts/apply-ui-state.sh:626:     set_ini_value "$HOME/.config/qt6ct/qt6ct.conf" Appearance icon_theme "$icon_theme_name"
scripts/apply-ui-state.sh:627:     set_ini_value "$HOME/.config/qt6ct/qt6ct.conf" Appearance color_scheme_path "$qt6_color_scheme"
scripts/apply-ui-state.sh:628:     set_ini_value "$HOME/.config/qt5ct/qt5ct.conf" Appearance style "kvantum"
scripts/apply-ui-state.sh:629:     set_ini_value "$HOME/.config/qt5ct/qt5ct.conf" Appearance icon_theme "$icon_theme_name"
scripts/apply-ui-state.sh:630:     set_ini_value "$HOME/.config/qt5ct/qt5ct.conf" Appearance color_scheme_path "$qt5_color_scheme"
scripts/apply-ui-state.sh:631:     set_ini_value "$HOME/.config/Kvantum/kvantum.kvconfig" General theme "$kvantum_theme"
scripts/apply-ui-state.sh:632: 
scripts/apply-ui-state.sh:633:     if command -v plasma-apply-colorscheme >/dev/null 2>&1; then
scripts/apply-ui-state.sh:634:         run_with_timeout 5 plasma-apply-colorscheme "$kde_color_scheme"
scripts/apply-ui-state.sh:635:     fi
scripts/apply-ui-state.sh:636: 
scripts/apply-ui-state.sh:637:     if command -v kwriteconfig6 >/dev/null 2>&1; then
scripts/apply-ui-state.sh:638:         kwriteconfig6 --file "$HOME/.config/kdeglobals" --group KDE --key LookAndFeelPackage "$kde_look_and_feel" >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:639:         kwriteconfig6 --file "$HOME/.config/kdeglobals" --group KDE --key DefaultLightLookAndFeel "com.github.vinceliuice.Orchis" >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:640:         kwriteconfig6 --file "$HOME/.config/kdeglobals" --group KDE --key DefaultDarkLookAndFeel "com.github.vinceliuice.Orchis-dark" >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:641:         kwriteconfig6 --file "$HOME/.config/kdeglobals" --group Icons --key Theme "$icon_theme_name" >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:642:         kwriteconfig6 --file "$HOME/.config/kdeglobals" --group KDE --key widgetStyle "qt6ct-style" >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:643:     fi
scripts/apply-ui-state.sh:644: 
scripts/apply-ui-state.sh:645:     if [[ -f "$background_target" ]] && command -v awww >/dev/null 2>&1; then
scripts/apply-ui-state.sh:646:         if ! pgrep -x awww-daemon >/dev/null 2>&1; then
scripts/apply-ui-state.sh:647:             nohup awww-daemon >/dev/null 2>&1 &
scripts/apply-ui-state.sh:648:             sleep 0.2
scripts/apply-ui-state.sh:649:         fi
scripts/apply-ui-state.sh:650:         awww img "$background_target" --transition-fps 180 --transition-type wipe --transition-duration 2 >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:651:     fi
scripts/apply-ui-state.sh:652: 
scripts/apply-ui-state.sh:653:     if command -v kwriteconfig6 >/dev/null 2>&1; then
scripts/apply-ui-state.sh:654:         kwriteconfig6 --file "$HOME/.config/konsolerc" --group "Desktop Entry" --key "DefaultProfile" "${mode_label}.profile" >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:655:     fi
scripts/apply-ui-state.sh:656: 
scripts/apply-ui-state.sh:657:     if command -v qdbus6 >/dev/null 2>&1; then
scripts/apply-ui-state.sh:658:         for service in $(qdbus6 | awk '/org.kde.konsole/ { print $1 }'); do
scripts/apply-ui-state.sh:659:             for session in $(qdbus6 "$service" | awk '/Sessions\// { print $1 }'); do
scripts/apply-ui-state.sh:660:                 qdbus6 "$service" "$session" org.kde.konsole.Session.setProfile "$mode_label" >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:661:             done
scripts/apply-ui-state.sh:662:             for window in $(qdbus6 "$service" | awk '/Windows\// { print $1 }'); do
scripts/apply-ui-state.sh:663:                 qdbus6 "$service" "$window" org.kde.konsole.Window.setDefaultProfile "$mode_label" >/dev/null 2>&1 || true
scripts/apply-ui-state.sh:664:             done
scripts/apply-ui-state.sh:665:         done
…
scripts/apply-ui-state.sh:667: 
scripts/apply-ui-state.sh:668:     sync_wechat_appearance "$theme"
scripts/apply-ui-state.sh:669: }
scripts/apply-ui-state.sh:670: 
scripts/apply-ui-state.sh:671: apply_with_state() {
scripts/apply-ui-state.sh:672:     load_current_state
scripts/apply-ui-state.sh:673:     apply_links_and_theme
…
scripts/apply-ui-state.sh:675: 
scripts/apply-ui-state.sh:676: toggle_theme() {
scripts/apply-ui-state.sh:677:     local new_theme
scripts/apply-ui-state.sh:678:     new_theme="$("$state_script" toggle-theme)"
scripts/apply-ui-state.sh:679:     notify_ui_state "switching to $(theme_label "$new_theme")"
scripts/apply-ui-state.sh:680:     apply_with_state
scripts/apply-ui-state.sh:681: }
scripts/apply-ui-state.sh:682: 
scripts/apply-ui-state.sh:683: set_variant() {
scripts/apply-ui-state.sh:684:     local new_variant
scripts/apply-ui-state.sh:685:     new_variant="${1:-}"
scripts/apply-ui-state.sh:686:     "$state_script" set-variant "$new_variant"
scripts/apply-ui-state.sh:687:     notify_ui_state "switching to $(variant_label "$new_variant")"
scripts/apply-ui-state.sh:688:     apply_with_state
scripts/apply-ui-state.sh:689: }
scripts/apply-ui-state.sh:690: 
scripts/apply-ui-state.sh:691: set_theme() {
scripts/apply-ui-state.sh:692:     local new_theme
scripts/apply-ui-state.sh:693:     new_theme="${1:-}"
scripts/apply-ui-state.sh:694:     "$state_script" set-theme "$new_theme"
scripts/apply-ui-state.sh:695:     notify_ui_state "switching to $(theme_label "$new_theme")"
scripts/apply-ui-state.sh:696:     apply_with_state
scripts/apply-ui-state.sh:697: }
scripts/apply-ui-state.sh:698: 
scripts/apply-ui-state.sh:699: case "${1:-apply}" in
scripts/apply-ui-state.sh:700:     apply)
scripts/apply-ui-state.sh:701:         apply_with_state
scripts/apply-ui-state.sh:702:         ;;
scripts/apply-ui-state.sh:703:     toggle-theme)
scripts/apply-ui-state.sh:704:         toggle_theme
scripts/apply-ui-state.sh:705:         ;;
scripts/apply-ui-state.sh:706:     set-theme)
scripts/apply-ui-state.sh:707:         set_theme "${2:-}"
scripts/apply-ui-state.sh:708:         ;;
scripts/apply-ui-state.sh:709:     set-variant)
scripts/apply-ui-state.sh:710:         set_variant "${2:-}"
scripts/apply-ui-state.sh:711:         ;;
scripts/apply-ui-state.sh:712:     *)
scripts/apply-ui-state.sh:713:         printf 'Usage: %s [apply|toggle-theme|set-theme <light|dark>|set-variant <v1|v2|v3|bt>]\n' "$0" >&2
scripts/apply-ui-state.sh:714:         exit 1
scripts/apply-ui-state.sh:715:         ;;
```

### `scripts/audio-spectrum.sh`

Migration owner: `audio/spectrum`. Source `scripts/audio-spectrum.sh:1`, 32 lines, SHA-256 `5254370f6d751e94f107866cf49bddb743375f7e8f475a639bb137df65266c05`.

```text
scripts/audio-spectrum.sh:1: #!/usr/bin/env bash
scripts/audio-spectrum.sh:2: 
scripts/audio-spectrum.sh:3: set -euo pipefail
scripts/audio-spectrum.sh:4: 
scripts/audio-spectrum.sh:5: if ! command -v cava >/dev/null 2>&1; then
scripts/audio-spectrum.sh:6:     exit 127
scripts/audio-spectrum.sh:7: fi
scripts/audio-spectrum.sh:8: 
scripts/audio-spectrum.sh:9: exec cava -p /dev/stdin <<'CAVACONF'
scripts/audio-spectrum.sh:10: [general]
scripts/audio-spectrum.sh:11: framerate=24
scripts/audio-spectrum.sh:12: bars=16
scripts/audio-spectrum.sh:13: autosens=1
scripts/audio-spectrum.sh:14: sensitivity=45
scripts/audio-spectrum.sh:15: lower_cutoff_freq=50
scripts/audio-spectrum.sh:16: higher_cutoff_freq=12000
scripts/audio-spectrum.sh:17: 
scripts/audio-spectrum.sh:18: [output]
scripts/audio-spectrum.sh:19: method=raw
scripts/audio-spectrum.sh:20: raw_target=/dev/stdout
scripts/audio-spectrum.sh:21: data_format=ascii
scripts/audio-spectrum.sh:22: ascii_max_range=100
scripts/audio-spectrum.sh:23: channels=mono
scripts/audio-spectrum.sh:24: mono_option=average
scripts/audio-spectrum.sh:25: 
scripts/audio-spectrum.sh:26: [smoothing]
scripts/audio-spectrum.sh:27: noise_reduction=35
scripts/audio-spectrum.sh:28: integral=88
scripts/audio-spectrum.sh:29: gravity=92
scripts/audio-spectrum.sh:30: ignore=2
scripts/audio-spectrum.sh:31: monstercat=1.4
scripts/audio-spectrum.sh:32: CAVACONF
```

### `scripts/battery-info.sh`

Migration owner: `power/battery_history`. Source `scripts/battery-info.sh:1`, 237 lines, SHA-256 `95e233acb165b2951ca1e16f5ea5a6a61ed4033e961286bb41c59d874b9a82dc`.

```text
…
scripts/battery-info.sh:5: script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
scripts/battery-info.sh:6: . "$script_dir/../../scripts/lib/battery.sh"
scripts/battery-info.sh:7: 
scripts/battery-info.sh:8: window_seconds=1800
…
scripts/battery-info.sh:10: battery_dir="$(hyprv_find_battery_dir || true)"
scripts/battery-info.sh:11: if [ -z "$battery_dir" ]; then
scripts/battery-info.sh:12:     jq -n '{available: false}'
scripts/battery-info.sh:13:     exit 0
scripts/battery-info.sh:14: fi
scripts/battery-info.sh:15: 
scripts/battery-info.sh:16: state_dir="$(hyprv_pick_state_dir || printf '/tmp/hyprv')"
scripts/battery-info.sh:17: mkdir -p "$state_dir"
scripts/battery-info.sh:18: state_file="$state_dir/battery-power-history.tsv"
scripts/battery-info.sh:19: 
scripts/battery-info.sh:20: present="$(hyprv_safe_int "$(hyprv_read_first_existing "$battery_dir" present || printf '1')")"
scripts/battery-info.sh:21: status="$(hyprv_read_first_existing "$battery_dir" status || printf 'Unknown')"
scripts/battery-info.sh:22: capacity="$(hyprv_safe_int "$(hyprv_read_first_existing "$battery_dir" capacity || printf '0')")"
scripts/battery-info.sh:23: voltage_now="$(hyprv_safe_int "$(hyprv_read_first_existing "$battery_dir" voltage_now || printf '0')")"
scripts/battery-info.sh:24: charge_now="$(hyprv_safe_int "$(hyprv_read_first_existing "$battery_dir" charge_now || printf '0')")"
scripts/battery-info.sh:25: charge_full="$(hyprv_safe_int "$(hyprv_read_first_existing "$battery_dir" charge_full || printf '0')")"
scripts/battery-info.sh:26: energy_now_raw="$(hyprv_read_first_existing "$battery_dir" energy_now || true)"
scripts/battery-info.sh:27: energy_full_raw="$(hyprv_read_first_existing "$battery_dir" energy_full || true)"
scripts/battery-info.sh:28: power_now_raw="$(hyprv_read_first_existing "$battery_dir" power_now || true)"
scripts/battery-info.sh:29: current_now_raw="$(hyprv_read_first_existing "$battery_dir" current_now || true)"
scripts/battery-info.sh:30: 
scripts/battery-info.sh:31: if [ -n "$energy_now_raw" ]; then
scripts/battery-info.sh:32:     energy_now_uwh="$(hyprv_safe_int "$energy_now_raw")"
scripts/battery-info.sh:33: elif [ "$charge_now" -gt 0 ] && [ "$voltage_now" -gt 0 ]; then
scripts/battery-info.sh:34:     energy_now_uwh="$(hyprv_mul_div_round "$charge_now" "$voltage_now" 1000000)"
scripts/battery-info.sh:35: else
scripts/battery-info.sh:36:     energy_now_uwh=0
…
scripts/battery-info.sh:38: 
scripts/battery-info.sh:39: if [ -n "$energy_full_raw" ]; then
scripts/battery-info.sh:40:     energy_full_uwh="$(hyprv_safe_int "$energy_full_raw")"
scripts/battery-info.sh:41: elif [ "$charge_full" -gt 0 ] && [ "$voltage_now" -gt 0 ]; then
scripts/battery-info.sh:42:     energy_full_uwh="$(hyprv_mul_div_round "$charge_full" "$voltage_now" 1000000)"
scripts/battery-info.sh:43: else
scripts/battery-info.sh:44:     energy_full_uwh=0
…
scripts/battery-info.sh:46: 
scripts/battery-info.sh:47: if [ -n "$power_now_raw" ]; then
scripts/battery-info.sh:48:     power_now_uw="$(hyprv_abs_int "$power_now_raw")"
scripts/battery-info.sh:49: else
scripts/battery-info.sh:50:     current_now_abs="$(hyprv_abs_int "$current_now_raw")"
scripts/battery-info.sh:51:     if [ "$current_now_abs" -gt 0 ] && [ "$voltage_now" -gt 0 ]; then
scripts/battery-info.sh:52:         power_now_uw="$(hyprv_mul_div_round "$current_now_abs" "$voltage_now" 1000000)"
scripts/battery-info.sh:53:     else
scripts/battery-info.sh:54:         power_now_uw=0
…
scripts/battery-info.sh:58: mode="idle"
scripts/battery-info.sh:59: case "$status" in
scripts/battery-info.sh:60:     Charging|Pending\ charge)
scripts/battery-info.sh:61:         mode="charging"
…
scripts/battery-info.sh:65:         ;;
scripts/battery-info.sh:66:     Full)
scripts/battery-info.sh:67:         mode="full"
scripts/battery-info.sh:68:         ;;
…
scripts/battery-info.sh:73: 
scripts/battery-info.sh:74: if [ "$mode" = "plugged" ] && [ "$capacity" -ge 99 ]; then
scripts/battery-info.sh:75:     mode="full"
scripts/battery-info.sh:76: fi
…
scripts/battery-info.sh:78: signed_power_uw=0
scripts/battery-info.sh:79: case "$mode" in
scripts/battery-info.sh:80:     charging)
scripts/battery-info.sh:81:         signed_power_uw="$power_now_uw"
scripts/battery-info.sh:82:         ;;
scripts/battery-info.sh:83:     discharging)
scripts/battery-info.sh:84:         signed_power_uw="-$(hyprv_abs_int "$power_now_uw")"
scripts/battery-info.sh:85:         ;;
scripts/battery-info.sh:86: esac
scripts/battery-info.sh:87: 
scripts/battery-info.sh:88: tmp_file="$(mktemp "$state_dir/battery-history.XXXXXX")"
scripts/battery-info.sh:89: cleanup() {
scripts/battery-info.sh:90:     rm -f "$tmp_file"
scripts/battery-info.sh:91: }
scripts/battery-info.sh:92: trap cleanup EXIT INT TERM
…
scripts/battery-info.sh:96: 
scripts/battery-info.sh:97: if [ -f "$state_file" ]; then
scripts/battery-info.sh:98:     awk -v cutoff="$cutoff" 'BEGIN {
scripts/battery-info.sh:99:         FS = "\t";
scripts/battery-info.sh:100:         OFS = "\t";
…
scripts/battery-info.sh:103:         print $1, $2;
scripts/battery-info.sh:104:     }' "$state_file" > "$tmp_file"
scripts/battery-info.sh:105: else
scripts/battery-info.sh:106:     : > "$tmp_file"
scripts/battery-info.sh:107: fi
scripts/battery-info.sh:108: 
scripts/battery-info.sh:109: printf '%s\t%s\n' "$now" "$signed_power_uw" >> "$tmp_file"
scripts/battery-info.sh:110: mv "$tmp_file" "$state_file"
scripts/battery-info.sh:111: 
scripts/battery-info.sh:112: avg_fields="$(awk -v mode="$mode" -F'\t' '
scripts/battery-info.sh:113: function include_value(value) {
scripts/battery-info.sh:114:     if (mode == "charging") {
scripts/battery-info.sh:115:         return value > 0;
…
scripts/battery-info.sh:140:     }
scripts/battery-info.sh:141:     printf "%d\t%.0f\t%d\n", first_timestamp, sum / count, count;
scripts/battery-info.sh:142: }' "$state_file")"
scripts/battery-info.sh:143: 
scripts/battery-info.sh:144: old_ifs=$IFS
scripts/battery-info.sh:145: IFS='	'
scripts/battery-info.sh:146: set -- $avg_fields
scripts/battery-info.sh:147: IFS=$old_ifs
scripts/battery-info.sh:148: avg_first_timestamp="${1:-0}"
scripts/battery-info.sh:149: avg_power_uw="${2:-0}"
…
scripts/battery-info.sh:156: 
scripts/battery-info.sh:157: avg_power_abs_uw="$(hyprv_abs_int "$avg_power_uw")"
scripts/battery-info.sh:158: estimate_seconds_json="null"
scripts/battery-info.sh:159: estimate_basis="none"
…
scripts/battery-info.sh:161: 
scripts/battery-info.sh:162: case "$mode" in
scripts/battery-info.sh:163:     charging)
scripts/battery-info.sh:164:         remaining_energy_uwh=$((energy_full_uwh - energy_now_uwh))
scripts/battery-info.sh:165:         if [ "$remaining_energy_uwh" -lt 0 ]; then
scripts/battery-info.sh:166:             remaining_energy_uwh=0
scripts/battery-info.sh:167:         fi
scripts/battery-info.sh:168:         if [ "$avg_power_abs_uw" -gt 0 ]; then
scripts/battery-info.sh:169:             estimate_power_uw="$avg_power_abs_uw"
scripts/battery-info.sh:170:             estimate_basis="average"
scripts/battery-info.sh:171:         elif [ "$power_now_uw" -gt 0 ]; then
scripts/battery-info.sh:172:             estimate_power_uw="$power_now_uw"
scripts/battery-info.sh:173:             estimate_basis="current"
scripts/battery-info.sh:174:         fi
scripts/battery-info.sh:175:         if [ "$remaining_energy_uwh" -le 0 ]; then
scripts/battery-info.sh:176:             estimate_seconds_json="0"
scripts/battery-info.sh:177:             estimate_basis="none"
scripts/battery-info.sh:178:         elif [ "$estimate_power_uw" -gt 0 ]; then
scripts/battery-info.sh:179:             estimate_seconds_json="$(awk -v e="$remaining_energy_uwh" -v p="$estimate_power_uw" 'BEGIN {
scripts/battery-info.sh:180:                 printf "%.0f", (e + 0) / (p + 0) * 3600;
scripts/battery-info.sh:181:             }')"
scripts/battery-info.sh:182:         fi
scripts/battery-info.sh:183:         ;;
scripts/battery-info.sh:184:     discharging)
scripts/battery-info.sh:185:         remaining_energy_uwh="$energy_now_uwh"
scripts/battery-info.sh:186:         if [ "$avg_power_abs_uw" -gt 0 ]; then
scripts/battery-info.sh:187:             estimate_power_uw="$avg_power_abs_uw"
scripts/battery-info.sh:188:             estimate_basis="average"
scripts/battery-info.sh:189:         elif [ "$power_now_uw" -gt 0 ]; then
scripts/battery-info.sh:190:             estimate_power_uw="$power_now_uw"
scripts/battery-info.sh:191:             estimate_basis="current"
scripts/battery-info.sh:192:         fi
scripts/battery-info.sh:193:         if [ "$remaining_energy_uwh" -gt 0 ] && [ "$estimate_power_uw" -gt 0 ]; then
scripts/battery-info.sh:194:             estimate_seconds_json="$(awk -v e="$remaining_energy_uwh" -v p="$estimate_power_uw" 'BEGIN {
scripts/battery-info.sh:195:                 printf "%.0f", (e + 0) / (p + 0) * 3600;
scripts/battery-info.sh:196:             }')"
scripts/battery-info.sh:197:         fi
scripts/battery-info.sh:198:         ;;
scripts/battery-info.sh:199:     full)
scripts/battery-info.sh:200:         estimate_seconds_json="0"
scripts/battery-info.sh:201:         ;;
…
scripts/battery-info.sh:204: available_json=false
scripts/battery-info.sh:205: if [ "$present" -gt 0 ]; then
scripts/battery-info.sh:206:     available_json=true
scripts/battery-info.sh:207: fi
scripts/battery-info.sh:208: 
scripts/battery-info.sh:209: jq -n \
scripts/battery-info.sh:210:     --arg status "$status" \
scripts/battery-info.sh:211:     --arg mode "$mode" \
scripts/battery-info.sh:212:     --arg estimateBasis "$estimate_basis" \
scripts/battery-info.sh:213:     --argjson available "$available_json" \
scripts/battery-info.sh:214:     --argjson capacity "$capacity" \
scripts/battery-info.sh:215:     --argjson powerW "$(hyprv_micro_to_decimal_units "$signed_power_uw")" \
scripts/battery-info.sh:216:     --argjson averagePowerW "$(hyprv_micro_to_decimal_units "$avg_power_uw")" \
scripts/battery-info.sh:217:     --argjson sampleCount "$sample_count" \
scripts/battery-info.sh:218:     --argjson sampleWindowSeconds "$sample_window_seconds" \
scripts/battery-info.sh:219:     --argjson windowComplete "$( [ "$sample_window_seconds" -ge "$window_seconds" ] && printf 'true' || printf 'false' )" \
scripts/battery-info.sh:220:     --argjson estimateSeconds "$estimate_seconds_json" \
scripts/battery-info.sh:221:     --argjson energyNowWh "$(hyprv_micro_to_decimal_units "$energy_now_uwh")" \
scripts/battery-info.sh:222:     --argjson energyFullWh "$(hyprv_micro_to_decimal_units "$energy_full_uwh")" \
scripts/battery-info.sh:223:     '{
scripts/battery-info.sh:224:         available: $available,
scripts/battery-info.sh:225:         status: $status,
scripts/battery-info.sh:226:         mode: $mode,
scripts/battery-info.sh:227:         capacity: $capacity,
scripts/battery-info.sh:228:         powerW: $powerW,
scripts/battery-info.sh:229:         averagePowerW: $averagePowerW,
scripts/battery-info.sh:230:         sampleCount: $sampleCount,
scripts/battery-info.sh:231:         sampleWindowSeconds: $sampleWindowSeconds,
scripts/battery-info.sh:232:         windowComplete: $windowComplete,
scripts/battery-info.sh:233:         estimateSeconds: $estimateSeconds,
scripts/battery-info.sh:234:         estimateBasis: $estimateBasis,
scripts/battery-info.sh:235:         energyNowWh: $energyNowWh,
scripts/battery-info.sh:236:         energyFullWh: $energyFullWh
scripts/battery-info.sh:237:     }'
```

### `scripts/bluetooth-action.sh`

Migration owner: `bluetooth`. Source `scripts/bluetooth-action.sh:1`, 122 lines, SHA-256 `675da33aafbac8123fd8a012a586c888d85bef43b5a3ea732ef47414087b707b`.

```text
…
scripts/bluetooth-action.sh:5: script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
scripts/bluetooth-action.sh:6: . "$script_dir/bluetooth-common.sh"
scripts/bluetooth-action.sh:7: 
scripts/bluetooth-action.sh:8: bt_fail() {
scripts/bluetooth-action.sh:9:     printf '%s\n' "${1:-Bluetooth action failed}" >&2
scripts/bluetooth-action.sh:10:     exit 1
scripts/bluetooth-action.sh:11: }
scripts/bluetooth-action.sh:12: 
scripts/bluetooth-action.sh:13: bt_require_controller() {
scripts/bluetooth-action.sh:14:     bt_has_controller || bt_fail "No Bluetooth controller found"
scripts/bluetooth-action.sh:15: }
scripts/bluetooth-action.sh:16: 
scripts/bluetooth-action.sh:17: bt_require_address() {
scripts/bluetooth-action.sh:18:     local address="${1:-}"
scripts/bluetooth-action.sh:19:     [[ -n "$address" ]] || bt_fail "Missing device address"
scripts/bluetooth-action.sh:20: }
scripts/bluetooth-action.sh:21: 
scripts/bluetooth-action.sh:22: bt_output_has_failure() {
scripts/bluetooth-action.sh:23:     local output="${1:-}"
scripts/bluetooth-action.sh:24:     grep -qiE 'No default controller available|Failed|not available|not ready|invalid|Missing|not connected|Operation not permitted' <<<"$output"
scripts/bluetooth-action.sh:25: }
scripts/bluetooth-action.sh:26: 
scripts/bluetooth-action.sh:27: bt_exec() {
scripts/bluetooth-action.sh:28:     local output
scripts/bluetooth-action.sh:29: 
scripts/bluetooth-action.sh:30:     output="$(timeout "${HYPRV_BLUETOOTH_TIMEOUT}s" bluetoothctl --timeout "$HYPRV_BLUETOOTH_TIMEOUT" "$@" 2>&1)" || {
scripts/bluetooth-action.sh:31:         printf '%s\n' "$output"
scripts/bluetooth-action.sh:32:         return 1
scripts/bluetooth-action.sh:33:     }
scripts/bluetooth-action.sh:34: 
scripts/bluetooth-action.sh:35:     if bt_output_has_failure "$output"; then
scripts/bluetooth-action.sh:36:         printf '%s\n' "$output"
scripts/bluetooth-action.sh:37:         return 1
scripts/bluetooth-action.sh:38:     fi
scripts/bluetooth-action.sh:39: 
scripts/bluetooth-action.sh:40:     printf '%s\n' "$output"
scripts/bluetooth-action.sh:41: }
scripts/bluetooth-action.sh:42: 
scripts/bluetooth-action.sh:43: bt_exec_script() {
scripts/bluetooth-action.sh:44:     local timeout_seconds="$1"
scripts/bluetooth-action.sh:45:     shift
scripts/bluetooth-action.sh:46: 
…
scripts/bluetooth-action.sh:48:     script_file="$(mktemp)"
scripts/bluetooth-action.sh:49:     printf '%s\n' "$@" > "$script_file"
scripts/bluetooth-action.sh:50: 
scripts/bluetooth-action.sh:51:     output="$(timeout "$((timeout_seconds + 2))s" bluetoothctl --timeout "$timeout_seconds" --init-script "$script_file" 2>&1)" || status=$?
scripts/bluetooth-action.sh:52:     rm -f "$script_file"
scripts/bluetooth-action.sh:53: 
scripts/bluetooth-action.sh:54:     if (( status != 0 )) || bt_output_has_failure "$output"; then
scripts/bluetooth-action.sh:55:         printf '%s\n' "$output"
scripts/bluetooth-action.sh:56:         return 1
scripts/bluetooth-action.sh:57:     fi
scripts/bluetooth-action.sh:58: 
scripts/bluetooth-action.sh:59:     printf '%s\n' "$output"
scripts/bluetooth-action.sh:60: }
scripts/bluetooth-action.sh:61: 
…
scripts/bluetooth-action.sh:63: 
scripts/bluetooth-action.sh:64: case "$command" in
scripts/bluetooth-action.sh:65:     toggle)
scripts/bluetooth-action.sh:66:         bt_require_controller
scripts/bluetooth-action.sh:67:         state="${2:-}"
scripts/bluetooth-action.sh:68:         [[ "$state" == "on" || "$state" == "off" ]] || bt_fail 'toggle expects "on" or "off"'
scripts/bluetooth-action.sh:69:         bt_exec power "$state" >/dev/null || bt_fail "Failed to change Bluetooth power state"
scripts/bluetooth-action.sh:70:         if [[ "$state" == "on" ]]; then
scripts/bluetooth-action.sh:71:             printf 'Bluetooth enabled\n'
scripts/bluetooth-action.sh:72:         else
scripts/bluetooth-action.sh:73:             printf 'Bluetooth disabled\n'
scripts/bluetooth-action.sh:74:         fi
scripts/bluetooth-action.sh:75:         ;;
scripts/bluetooth-action.sh:76:     scan)
scripts/bluetooth-action.sh:77:         bt_require_controller
scripts/bluetooth-action.sh:78:         if [[ "$(bt_property_bool "$(bt_show_capture)" "Powered")" != "true" ]]; then
…
scripts/bluetooth-action.sh:80:         fi
scripts/bluetooth-action.sh:81:         bt_exec scan on >/dev/null || bt_fail "Failed to start Bluetooth scan"
scripts/bluetooth-action.sh:82:         sleep 4
scripts/bluetooth-action.sh:83:         bt_exec scan off >/dev/null || true
scripts/bluetooth-action.sh:84:         printf 'Bluetooth scan complete\n'
scripts/bluetooth-action.sh:85:         ;;
scripts/bluetooth-action.sh:86:     connect)
scripts/bluetooth-action.sh:87:         bt_require_controller
scripts/bluetooth-action.sh:88:         address="${2:-}"
scripts/bluetooth-action.sh:89:         paired="${3:-false}"
scripts/bluetooth-action.sh:90:         bt_require_address "$address"
scripts/bluetooth-action.sh:91: 
scripts/bluetooth-action.sh:92:         if [[ "$paired" == "true" ]]; then
scripts/bluetooth-action.sh:93:             bt_exec connect "$address" >/dev/null || bt_fail "Failed to connect device"
scripts/bluetooth-action.sh:94:             printf 'Connected to %s\n' "$address"
scripts/bluetooth-action.sh:95:         else
scripts/bluetooth-action.sh:96:             bt_exec_script 35 \
…
scripts/bluetooth-action.sh:98:                 "default-agent" \
scripts/bluetooth-action.sh:99:                 "pair $address" \
scripts/bluetooth-action.sh:100:                 "trust $address" \
scripts/bluetooth-action.sh:101:                 "connect $address" >/dev/null || bt_fail "Failed to pair device"
scripts/bluetooth-action.sh:102:             printf 'Paired and connected %s\n' "$address"
scripts/bluetooth-action.sh:103:         fi
scripts/bluetooth-action.sh:104:         ;;
scripts/bluetooth-action.sh:105:     disconnect)
scripts/bluetooth-action.sh:106:         bt_require_controller
scripts/bluetooth-action.sh:107:         address="${2:-}"
scripts/bluetooth-action.sh:108:         bt_require_address "$address"
scripts/bluetooth-action.sh:109:         bt_exec disconnect "$address" >/dev/null || bt_fail "Failed to disconnect device"
scripts/bluetooth-action.sh:110:         printf 'Disconnected %s\n' "$address"
scripts/bluetooth-action.sh:111:         ;;
scripts/bluetooth-action.sh:112:     remove)
scripts/bluetooth-action.sh:113:         bt_require_controller
scripts/bluetooth-action.sh:114:         address="${2:-}"
scripts/bluetooth-action.sh:115:         bt_require_address "$address"
scripts/bluetooth-action.sh:116:         bt_exec remove "$address" >/dev/null || bt_fail "Failed to remove device"
scripts/bluetooth-action.sh:117:         printf 'Removed %s\n' "$address"
scripts/bluetooth-action.sh:118:         ;;
scripts/bluetooth-action.sh:119:     *)
scripts/bluetooth-action.sh:120:         bt_fail "Unknown Bluetooth command"
scripts/bluetooth-action.sh:121:         ;;
```

### `scripts/bluetooth-common.sh`

Migration owner: `bluetooth`. Source `scripts/bluetooth-common.sh:1`, 95 lines, SHA-256 `bb93bcb598132bbc7594400a30728a53d639d224c083d65a8bb3c79d23fe8cf6`.

```text
…
scripts/bluetooth-common.sh:2: 
scripts/bluetooth-common.sh:3: : "${HYPRV_BLUETOOTH_TIMEOUT:=5}"
scripts/bluetooth-common.sh:4: 
scripts/bluetooth-common.sh:5: bt_trim() {
scripts/bluetooth-common.sh:6:     local value="${1:-}"
scripts/bluetooth-common.sh:7:     value="${value#"${value%%[![:space:]]*}"}"
scripts/bluetooth-common.sh:8:     value="${value%"${value##*[![:space:]]}"}"
scripts/bluetooth-common.sh:9:     printf '%s\n' "$value"
scripts/bluetooth-common.sh:10: }
scripts/bluetooth-common.sh:11: 
scripts/bluetooth-common.sh:12: bt_property_value() {
scripts/bluetooth-common.sh:13:     local text="${1:-}"
scripts/bluetooth-common.sh:14:     local key="${2:-}"
scripts/bluetooth-common.sh:15: 
scripts/bluetooth-common.sh:16:     awk -F': ' -v key="$key" '
scripts/bluetooth-common.sh:17:         $1 ~ ("^[[:space:]]*" key "$") {
scripts/bluetooth-common.sh:18:             sub(/^[[:space:]]+/, "", $2);
…
scripts/bluetooth-common.sh:21:         }
scripts/bluetooth-common.sh:22:     ' <<<"$text"
scripts/bluetooth-common.sh:23: }
scripts/bluetooth-common.sh:24: 
scripts/bluetooth-common.sh:25: bt_property_bool() {
scripts/bluetooth-common.sh:26:     local text="${1:-}"
scripts/bluetooth-common.sh:27:     local key="${2:-}"
…
scripts/bluetooth-common.sh:29: 
scripts/bluetooth-common.sh:30:     value="$(bt_property_value "$text" "$key" | tr '[:upper:]' '[:lower:]')"
scripts/bluetooth-common.sh:31:     if [[ "$value" == "yes" ]]; then
scripts/bluetooth-common.sh:32:         printf 'true\n'
scripts/bluetooth-common.sh:33:     else
scripts/bluetooth-common.sh:34:         printf 'false\n'
scripts/bluetooth-common.sh:35:     fi
scripts/bluetooth-common.sh:36: }
scripts/bluetooth-common.sh:37: 
scripts/bluetooth-common.sh:38: bt_output_invalid() {
scripts/bluetooth-common.sh:39:     local output="${1:-}"
scripts/bluetooth-common.sh:40:     grep -qiE 'No default controller available|dbus_connection_get_object_path_data|D-Bus not built with -rdynamic' <<<"$output"
scripts/bluetooth-common.sh:41: }
scripts/bluetooth-common.sh:42: 
scripts/bluetooth-common.sh:43: bt_run() {
scripts/bluetooth-common.sh:44:     local output
scripts/bluetooth-common.sh:45: 
scripts/bluetooth-common.sh:46:     output="$({
scripts/bluetooth-common.sh:47:         timeout "${HYPRV_BLUETOOTH_TIMEOUT}s" bluetoothctl --timeout "$HYPRV_BLUETOOTH_TIMEOUT" "$@" 2>/dev/null || true
scripts/bluetooth-common.sh:48:     } 2>/dev/null)"
scripts/bluetooth-common.sh:49: 
scripts/bluetooth-common.sh:50:     if bt_output_invalid "$output"; then
scripts/bluetooth-common.sh:51:         return 0
scripts/bluetooth-common.sh:52:     fi
scripts/bluetooth-common.sh:53: 
scripts/bluetooth-common.sh:54:     printf '%s\n' "$output"
scripts/bluetooth-common.sh:55: }
scripts/bluetooth-common.sh:56: 
scripts/bluetooth-common.sh:57: bt_run_capture() {
scripts/bluetooth-common.sh:58:     local output
scripts/bluetooth-common.sh:59: 
scripts/bluetooth-common.sh:60:     output="$({
scripts/bluetooth-common.sh:61:         timeout "${HYPRV_BLUETOOTH_TIMEOUT}s" bluetoothctl --timeout "$HYPRV_BLUETOOTH_TIMEOUT" "$@" 2>&1 || true
scripts/bluetooth-common.sh:62:     } 2>/dev/null)"
scripts/bluetooth-common.sh:63: 
scripts/bluetooth-common.sh:64:     if bt_output_invalid "$output"; then
scripts/bluetooth-common.sh:65:         return 0
scripts/bluetooth-common.sh:66:     fi
scripts/bluetooth-common.sh:67: 
scripts/bluetooth-common.sh:68:     printf '%s\n' "$output"
scripts/bluetooth-common.sh:69: }
scripts/bluetooth-common.sh:70: 
scripts/bluetooth-common.sh:71: bt_first_controller() {
scripts/bluetooth-common.sh:72:     local output
scripts/bluetooth-common.sh:73: 
scripts/bluetooth-common.sh:74:     output="$(bt_run_capture list)"
scripts/bluetooth-common.sh:75:     awk '/^Controller / { print $2; exit }' <<<"$output"
scripts/bluetooth-common.sh:76: }
scripts/bluetooth-common.sh:77: 
scripts/bluetooth-common.sh:78: bt_show_capture() {
scripts/bluetooth-common.sh:79:     local output controller
scripts/bluetooth-common.sh:80: 
scripts/bluetooth-common.sh:81:     output="$(bt_run_capture show)"
scripts/bluetooth-common.sh:82:     if [[ -n "$output" ]]; then
scripts/bluetooth-common.sh:83:         printf '%s\n' "$output"
scripts/bluetooth-common.sh:84:         return 0
scripts/bluetooth-common.sh:85:     fi
…
scripts/bluetooth-common.sh:87:     controller="$(bt_first_controller)"
scripts/bluetooth-common.sh:88:     if [[ -n "$controller" ]]; then
scripts/bluetooth-common.sh:89:         bt_run_capture show "$controller"
scripts/bluetooth-common.sh:90:     fi
scripts/bluetooth-common.sh:91: }
scripts/bluetooth-common.sh:92: 
scripts/bluetooth-common.sh:93: bt_has_controller() {
scripts/bluetooth-common.sh:94:     [[ -n "$(bt_show_capture)" ]]
scripts/bluetooth-common.sh:95: }
```

### `scripts/bluetooth-status.sh`

Migration owner: `bluetooth`. Source `scripts/bluetooth-status.sh:1`, 112 lines, SHA-256 `ff34b7cfc012532f905056ee71a990b94c15802940adce996d6d56c3dd9b4f1c`.

```text
…
scripts/bluetooth-status.sh:5: script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
scripts/bluetooth-status.sh:6: . "$script_dir/bluetooth-common.sh"
scripts/bluetooth-status.sh:7: 
scripts/bluetooth-status.sh:8: show_output="$(bt_show_capture)"
…
scripts/bluetooth-status.sh:17: 
scripts/bluetooth-status.sh:18: cleanup() {
scripts/bluetooth-status.sh:19:     rm -f "$tmp_seed" "$tmp_devices"
scripts/bluetooth-status.sh:20: }
scripts/bluetooth-status.sh:21: 
…
scripts/bluetooth-status.sh:23: 
scripts/bluetooth-status.sh:24: if [[ -n "$show_output" ]]; then
scripts/bluetooth-status.sh:25:     present=true
scripts/bluetooth-status.sh:26:     [[ "$(bt_property_bool "$show_output" "Powered")" == "true" ]] && enabled=true
scripts/bluetooth-status.sh:27:     [[ "$(bt_property_bool "$show_output" "Discovering")" == "true" ]] && discovering=true
scripts/bluetooth-status.sh:28:     [[ "$(bt_property_bool "$show_output" "Pairable")" == "true" ]] && pairable=true
scripts/bluetooth-status.sh:29: 
scripts/bluetooth-status.sh:30:     {
…
scripts/bluetooth-status.sh:41:         }
scripts/bluetooth-status.sh:42:     ' | awk -F'\t' '!seen[$1]++ { print }' > "$tmp_seed"
scripts/bluetooth-status.sh:43: 
scripts/bluetooth-status.sh:44:     while IFS=$'\t' read -r address label; do
scripts/bluetooth-status.sh:45:         [[ -n "$address" ]] || continue
scripts/bluetooth-status.sh:46: 
scripts/bluetooth-status.sh:47:         info="$(bt_run_capture info "$address")"
scripts/bluetooth-status.sh:48:         name="$(bt_property_value "$info" "Alias")"
scripts/bluetooth-status.sh:49:         [[ -n "$name" ]] || name="$(bt_trim "$label")"
scripts/bluetooth-status.sh:50:         [[ -n "$name" ]] || name="$(bt_property_value "$info" "Name")"
scripts/bluetooth-status.sh:51:         [[ -n "$name" ]] || name="$address"
scripts/bluetooth-status.sh:52: 
scripts/bluetooth-status.sh:53:         icon="$(bt_property_value "$info" "Icon")"
scripts/bluetooth-status.sh:54:         [[ -n "$icon" ]] || icon="bluetooth"
scripts/bluetooth-status.sh:55: 
scripts/bluetooth-status.sh:56:         paired="$(bt_property_bool "$info" "Paired")"
scripts/bluetooth-status.sh:57:         trusted="$(bt_property_bool "$info" "Trusted")"
scripts/bluetooth-status.sh:58:         connected="$(bt_property_bool "$info" "Connected")"
scripts/bluetooth-status.sh:59:         blocked="$(bt_property_bool "$info" "Blocked")"
scripts/bluetooth-status.sh:60:         rssi="$(bt_property_value "$info" "RSSI")"
scripts/bluetooth-status.sh:61: 
scripts/bluetooth-status.sh:62:         if [[ ! "$rssi" =~ ^-?[0-9]+$ ]]; then
scripts/bluetooth-status.sh:63:             rssi=""
scripts/bluetooth-status.sh:64:         fi
…
scripts/bluetooth-status.sh:68: 
scripts/bluetooth-status.sh:69:         printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
scripts/bluetooth-status.sh:70:             "$address" "$name" "$icon" "$paired" "$trusted" "$connected" "$blocked" "$rssi" >> "$tmp_devices"
scripts/bluetooth-status.sh:71:     done < "$tmp_seed"
scripts/bluetooth-status.sh:72: fi
scripts/bluetooth-status.sh:73: 
scripts/bluetooth-status.sh:74: jq -Rn \
scripts/bluetooth-status.sh:75:     --arg present "$present" \
scripts/bluetooth-status.sh:76:     --arg enabled "$enabled" \
scripts/bluetooth-status.sh:77:     --arg discovering "$discovering" \
scripts/bluetooth-status.sh:78:     --arg pairable "$pairable" \
scripts/bluetooth-status.sh:79:     --rawfile devices "$tmp_devices" '
scripts/bluetooth-status.sh:80:     def bool($value):
scripts/bluetooth-status.sh:81:         $value == "true";
scripts/bluetooth-status.sh:82: 
scripts/bluetooth-status.sh:83:     def device_rows:
scripts/bluetooth-status.sh:84:         $devices
scripts/bluetooth-status.sh:85:         | split("\n")
scripts/bluetooth-status.sh:86:         | map(select(length > 0) | split("\t"))
…
scripts/bluetooth-status.sh:105:     {
scripts/bluetooth-status.sh:106:         present: bool($present),
scripts/bluetooth-status.sh:107:         enabled: bool($enabled),
scripts/bluetooth-status.sh:108:         discovering: bool($discovering),
scripts/bluetooth-status.sh:109:         pairable: bool($pairable),
scripts/bluetooth-status.sh:110:         devices: device_rows
scripts/bluetooth-status.sh:111:     }
```

### `scripts/cider-metadata-fallback.py`

Migration owner: `cider/mpris`. Source `scripts/cider-metadata-fallback.py:1`, 427 lines, SHA-256 `24def580269ea8c83f10a349311c62c7b3c193dda2ea6cdeb941a3f1d25ba8a5`.

```text
…
scripts/cider-metadata-fallback.py:10: 
scripts/cider-metadata-fallback.py:11: CIDER_API_BASE = os.environ.get("HYPRV_CIDER_API_BASE", "http://127.0.0.1:10767")
scripts/cider-metadata-fallback.py:12: CIDER_API_TIMEOUT = 0.7
scripts/cider-metadata-fallback.py:13: 
scripts/cider-metadata-fallback.py:14: 
scripts/cider-metadata-fallback.py:15: def encode_varint(value):
scripts/cider-metadata-fallback.py:16:     output = bytearray()
scripts/cider-metadata-fallback.py:17:     while True:
…
scripts/cider-metadata-fallback.py:26: 
scripts/cider-metadata-fallback.py:27: def read_varint(data, offset):
scripts/cider-metadata-fallback.py:28:     value = 0
scripts/cider-metadata-fallback.py:29:     shift = 0
…
scripts/cider-metadata-fallback.py:40: 
scripts/cider-metadata-fallback.py:41: def key_pattern(key, value_tag):
scripts/cider-metadata-fallback.py:42:     encoded = key.encode("utf-8")
scripts/cider-metadata-fallback.py:43:     return b'"' + encode_varint(len(encoded)) + encoded + value_tag
…
scripts/cider-metadata-fallback.py:45: 
scripts/cider-metadata-fallback.py:46: def read_string_field(window, key):
scripts/cider-metadata-fallback.py:47:     pattern = key_pattern(key, b'"')
scripts/cider-metadata-fallback.py:48:     index = window.find(pattern)
…
scripts/cider-metadata-fallback.py:61: 
scripts/cider-metadata-fallback.py:62: def read_integer_field(window, key):
scripts/cider-metadata-fallback.py:63:     pattern = key_pattern(key, b"I")
scripts/cider-metadata-fallback.py:64:     index = window.find(pattern)
…
scripts/cider-metadata-fallback.py:74: 
scripts/cider-metadata-fallback.py:75: def single_line(value):
scripts/cider-metadata-fallback.py:76:     return " ".join(str(value or "").replace("\r", " ").replace("\n", " ").split())
scripts/cider-metadata-fallback.py:77: 
scripts/cider-metadata-fallback.py:78: 
scripts/cider-metadata-fallback.py:79: def normalized(value):
scripts/cider-metadata-fallback.py:80:     return " ".join((value or "").casefold().split())
scripts/cider-metadata-fallback.py:81: 
scripts/cider-metadata-fallback.py:82: 
scripts/cider-metadata-fallback.py:83: def artist_matches(candidate, requested):
scripts/cider-metadata-fallback.py:84:     candidate = normalized(candidate)
scripts/cider-metadata-fallback.py:85:     requested = normalized(requested)
…
scripts/cider-metadata-fallback.py:90: 
scripts/cider-metadata-fallback.py:91: def is_generic_title(value):
scripts/cider-metadata-fallback.py:92:     return normalized(value) in ("", "cider", "chromium")
scripts/cider-metadata-fallback.py:93: 
scripts/cider-metadata-fallback.py:94: 
scripts/cider-metadata-fallback.py:95: def format_artwork_url(value):
scripts/cider-metadata-fallback.py:96:     if not value:
scripts/cider-metadata-fallback.py:97:         return ""
…
scripts/cider-metadata-fallback.py:105:     for key, replacement in replacements.items():
scripts/cider-metadata-fallback.py:106:         url = url.replace(key, replacement)
scripts/cider-metadata-fallback.py:107:     if url.startswith("//"):
scripts/cider-metadata-fallback.py:108:         url = f"https:{url}"
scripts/cider-metadata-fallback.py:109:     elif url.startswith("/"):
scripts/cider-metadata-fallback.py:110:         url = f"{CIDER_API_BASE.rstrip('/')}{url}"
scripts/cider-metadata-fallback.py:111:     return url
…
scripts/cider-metadata-fallback.py:113: 
scripts/cider-metadata-fallback.py:114: def maybe_number(value):
scripts/cider-metadata-fallback.py:115:     try:
scripts/cider-metadata-fallback.py:116:         return float(value)
…
scripts/cider-metadata-fallback.py:120: 
scripts/cider-metadata-fallback.py:121: def track_from_mapping(data):
scripts/cider-metadata-fallback.py:122:     if not isinstance(data, dict):
scripts/cider-metadata-fallback.py:123:         return {}
scripts/cider-metadata-fallback.py:124: 
scripts/cider-metadata-fallback.py:125:     attributes = data.get("attributes") if isinstance(data.get("attributes"), dict) else {}
scripts/cider-metadata-fallback.py:126:     source = attributes if attributes else data
scripts/cider-metadata-fallback.py:127: 
scripts/cider-metadata-fallback.py:128:     artwork = source.get("artwork")
scripts/cider-metadata-fallback.py:129:     if isinstance(artwork, dict):
scripts/cider-metadata-fallback.py:130:         artwork = artwork.get("url")
scripts/cider-metadata-fallback.py:131:     elif not artwork and isinstance(data.get("artwork"), dict):
scripts/cider-metadata-fallback.py:132:         artwork = data["artwork"].get("url")
scripts/cider-metadata-fallback.py:133: 
scripts/cider-metadata-fallback.py:134:     duration = (
scripts/cider-metadata-fallback.py:135:         source.get("durationInMillis")
scripts/cider-metadata-fallback.py:136:         or source.get("duration_ms")
scripts/cider-metadata-fallback.py:137:         or source.get("durationMs")
scripts/cider-metadata-fallback.py:138:         or source.get("duration")
scripts/cider-metadata-fallback.py:139:     )
scripts/cider-metadata-fallback.py:140:     duration_number = maybe_number(duration)
…
scripts/cider-metadata-fallback.py:144:     return {
scripts/cider-metadata-fallback.py:145:         "title": single_line(
scripts/cider-metadata-fallback.py:146:             source.get("name")
scripts/cider-metadata-fallback.py:147:             or source.get("title")
scripts/cider-metadata-fallback.py:148:             or data.get("name")
scripts/cider-metadata-fallback.py:149:             or data.get("title")
scripts/cider-metadata-fallback.py:150:         ),
scripts/cider-metadata-fallback.py:151:         "artist": single_line(
scripts/cider-metadata-fallback.py:152:             source.get("artistName")
scripts/cider-metadata-fallback.py:153:             or source.get("artist")
scripts/cider-metadata-fallback.py:154:             or data.get("artistName")
scripts/cider-metadata-fallback.py:155:             or data.get("artist")
scripts/cider-metadata-fallback.py:156:         ),
scripts/cider-metadata-fallback.py:157:         "album": single_line(
scripts/cider-metadata-fallback.py:158:             source.get("albumName")
scripts/cider-metadata-fallback.py:159:             or source.get("album")
scripts/cider-metadata-fallback.py:160:             or data.get("albumName")
scripts/cider-metadata-fallback.py:161:             or data.get("album")
scripts/cider-metadata-fallback.py:162:         ),
scripts/cider-metadata-fallback.py:163:         "art_url": format_artwork_url(
scripts/cider-metadata-fallback.py:164:             artwork
scripts/cider-metadata-fallback.py:165:             or source.get("artworkUrl")
scripts/cider-metadata-fallback.py:166:             or source.get("artworkURL")
scripts/cider-metadata-fallback.py:167:             or data.get("artworkUrl")
scripts/cider-metadata-fallback.py:168:             or data.get("artworkURL")
scripts/cider-metadata-fallback.py:169:         ),
scripts/cider-metadata-fallback.py:170:         "duration_ms": round(duration_number) if duration_number is not None else None,
scripts/cider-metadata-fallback.py:171:     }
scripts/cider-metadata-fallback.py:172: 
scripts/cider-metadata-fallback.py:173: 
scripts/cider-metadata-fallback.py:174: def iter_candidate_mappings(value):
scripts/cider-metadata-fallback.py:175:     if isinstance(value, dict):
scripts/cider-metadata-fallback.py:176:         yield value
…
scripts/cider-metadata-fallback.py:183: 
scripts/cider-metadata-fallback.py:184: def usable_track(track):
scripts/cider-metadata-fallback.py:185:     return bool(track.get("title") or track.get("artist") or track.get("album") or track.get("art_url"))
scripts/cider-metadata-fallback.py:186: 
scripts/cider-metadata-fallback.py:187: 
scripts/cider-metadata-fallback.py:188: def requested_track_score(track, artist, album, target_ms, current_title, source_rank):
scripts/cider-metadata-fallback.py:189:     score = source_rank
scripts/cider-metadata-fallback.py:190:     duration_ms = track.get("duration_ms")
scripts/cider-metadata-fallback.py:191:     if target_ms > 0 and duration_ms:
scripts/cider-metadata-fallback.py:192:         delta = abs(int(duration_ms) - target_ms)
…
scripts/cider-metadata-fallback.py:198: 
scripts/cider-metadata-fallback.py:199:     if artist and not artist_matches(track.get("artist") or "", artist):
scripts/cider-metadata-fallback.py:200:         return None
scripts/cider-metadata-fallback.py:201:     if album and normalized(track.get("album")) != normalized(album):
scripts/cider-metadata-fallback.py:202:         return None
scripts/cider-metadata-fallback.py:203: 
scripts/cider-metadata-fallback.py:204:     if current_title and not is_generic_title(current_title):
scripts/cider-metadata-fallback.py:205:         if normalized(track.get("title")) == normalized(current_title):
scripts/cider-metadata-fallback.py:206:             score -= 3000
scripts/cider-metadata-fallback.py:207:         else:
…
scripts/cider-metadata-fallback.py:209: 
scripts/cider-metadata-fallback.py:210:     if track.get("title"):
scripts/cider-metadata-fallback.py:211:         score -= 100
scripts/cider-metadata-fallback.py:212:     if track.get("artist"):
scripts/cider-metadata-fallback.py:213:         score -= 80
scripts/cider-metadata-fallback.py:214:     if track.get("art_url"):
scripts/cider-metadata-fallback.py:215:         score -= 60
scripts/cider-metadata-fallback.py:216:     return score
…
scripts/cider-metadata-fallback.py:218: 
scripts/cider-metadata-fallback.py:219: def read_token_file(path):
scripts/cider-metadata-fallback.py:220:     try:
scripts/cider-metadata-fallback.py:221:         text = Path(path).expanduser().read_text(encoding="utf-8").strip()
scripts/cider-metadata-fallback.py:222:     except OSError:
scripts/cider-metadata-fallback.py:223:         return ""
…
scripts/cider-metadata-fallback.py:226: 
scripts/cider-metadata-fallback.py:227: def cider_api_tokens():
scripts/cider-metadata-fallback.py:228:     tokens = []
scripts/cider-metadata-fallback.py:229:     for key in ("HYPRV_CIDER_APP_TOKEN", "CIDER_APP_TOKEN"):
scripts/cider-metadata-fallback.py:230:         value = os.environ.get(key, "").strip()
scripts/cider-metadata-fallback.py:231:         if value:
scripts/cider-metadata-fallback.py:232:             tokens.append(value)
scripts/cider-metadata-fallback.py:233:     for path in (
scripts/cider-metadata-fallback.py:234:         "~/.config/hyprv/cider-app-token",
scripts/cider-metadata-fallback.py:235:         "~/.config/HyprV/.cider-app-token",
scripts/cider-metadata-fallback.py:236:     ):
scripts/cider-metadata-fallback.py:237:         value = read_token_file(path)
…
scripts/cider-metadata-fallback.py:246: 
scripts/cider-metadata-fallback.py:247: def fetch_json(url, token):
scripts/cider-metadata-fallback.py:248:     headers = {"User-Agent": "HyprV"}
scripts/cider-metadata-fallback.py:249:     if token:
scripts/cider-metadata-fallback.py:250:         headers["apptoken"] = token
scripts/cider-metadata-fallback.py:251:     request = Request(url, headers=headers)
scripts/cider-metadata-fallback.py:252:     with urlopen(request, timeout=CIDER_API_TIMEOUT) as response:
scripts/cider-metadata-fallback.py:253:         return json.loads(response.read().decode("utf-8", errors="replace"))
scripts/cider-metadata-fallback.py:254: 
scripts/cider-metadata-fallback.py:255: 
scripts/cider-metadata-fallback.py:256: def find_api_track(artist, album, target_ms, current_title):
scripts/cider-metadata-fallback.py:257:     endpoints = (
scripts/cider-metadata-fallback.py:258:         "/api/v2/playback/now-playing",
…
scripts/cider-metadata-fallback.py:269:                 payload = fetch_json(url, token)
scripts/cider-metadata-fallback.py:270:             except (HTTPError, URLError, TimeoutError, OSError, ValueError):
scripts/cider-metadata-fallback.py:271:                 continue
scripts/cider-metadata-fallback.py:272:             if isinstance(payload, dict) and isinstance(payload.get("data"), dict):
scripts/cider-metadata-fallback.py:273:                 track = track_from_mapping(payload["data"])
scripts/cider-metadata-fallback.py:274:                 if usable_track(track):
…
scripts/cider-metadata-fallback.py:295: 
scripts/cider-metadata-fallback.py:296: def iter_indexeddb_files():
scripts/cider-metadata-fallback.py:297:     base = Path.home() / ".config" / "sh.cider.genten" / "IndexedDB"
scripts/cider-metadata-fallback.py:298:     for name in (
scripts/cider-metadata-fallback.py:299:         "http_127.0.0.1_10767.indexeddb.leveldb",
…
scripts/cider-metadata-fallback.py:309: 
scripts/cider-metadata-fallback.py:310: def find_cache_track(artist, album, target_ms, current_title):
scripts/cider-metadata-fallback.py:311:     duration_pattern = key_pattern("durationInMillis", b"I")
scripts/cider-metadata-fallback.py:312:     matches = []
…
scripts/cider-metadata-fallback.py:315:         try:
scripts/cider-metadata-fallback.py:316:             data = path.read_bytes()
scripts/cider-metadata-fallback.py:317:         except OSError:
scripts/cider-metadata-fallback.py:318:             continue
…
scripts/cider-metadata-fallback.py:341:             track = {
scripts/cider-metadata-fallback.py:342:                 "title": single_line(title),
scripts/cider-metadata-fallback.py:343:                 "artist": single_line(candidate_artist),
scripts/cider-metadata-fallback.py:344:                 "album": single_line(candidate_album),
scripts/cider-metadata-fallback.py:345:                 "art_url": format_artwork_url(art_url),
scripts/cider-metadata-fallback.py:346:                 "duration_ms": duration_ms,
scripts/cider-metadata-fallback.py:347:                 "source": "cache",
scripts/cider-metadata-fallback.py:348:             }
scripts/cider-metadata-fallback.py:349:             if not usable_track(track):
…
scripts/cider-metadata-fallback.py:368: 
scripts/cider-metadata-fallback.py:369: def find_matching_track(artist, album, target_ms, current_title):
scripts/cider-metadata-fallback.py:370:     api_track = find_api_track(artist, album, target_ms, current_title)
scripts/cider-metadata-fallback.py:371:     if api_track:
…
scripts/cider-metadata-fallback.py:375: 
scripts/cider-metadata-fallback.py:376: def find_matching_title(artist, album, target_ms, current_title):
scripts/cider-metadata-fallback.py:377:     track = find_matching_track(artist, album, target_ms, current_title)
scripts/cider-metadata-fallback.py:378:     title = track.get("title") if track else ""
scripts/cider-metadata-fallback.py:379:     if title and normalized(title) != normalized(current_title):
scripts/cider-metadata-fallback.py:380:         return title
…
scripts/cider-metadata-fallback.py:383: 
scripts/cider-metadata-fallback.py:384: def print_shell(track):
scripts/cider-metadata-fallback.py:385:     for key in ("title", "artist", "album", "art_url", "source"):
scripts/cider-metadata-fallback.py:386:         value = track.get(key) or ""
scripts/cider-metadata-fallback.py:387:         print(f"{key}={single_line(value)}")
scripts/cider-metadata-fallback.py:388: 
scripts/cider-metadata-fallback.py:389: 
scripts/cider-metadata-fallback.py:390: def main():
scripts/cider-metadata-fallback.py:391:     parser = argparse.ArgumentParser()
scripts/cider-metadata-fallback.py:392:     parser.add_argument("--artist", default="")
scripts/cider-metadata-fallback.py:393:     parser.add_argument("--album", default="")
scripts/cider-metadata-fallback.py:394:     parser.add_argument("--length", default="0")
scripts/cider-metadata-fallback.py:395:     parser.add_argument("--current-title", default="")
scripts/cider-metadata-fallback.py:396:     parser.add_argument("--json", action="store_true")
scripts/cider-metadata-fallback.py:397:     parser.add_argument("--shell", action="store_true")
scripts/cider-metadata-fallback.py:398:     args = parser.parse_args()
scripts/cider-metadata-fallback.py:399: 
…
scripts/cider-metadata-fallback.py:412:     if args.json:
scripts/cider-metadata-fallback.py:413:         print(json.dumps(track, ensure_ascii=False, separators=(",", ":")))
scripts/cider-metadata-fallback.py:414:         return 0
scripts/cider-metadata-fallback.py:415:     if args.shell:
…
scripts/cider-metadata-fallback.py:418: 
scripts/cider-metadata-fallback.py:419:     title = track.get("title") or ""
scripts/cider-metadata-fallback.py:420:     if title and normalized(title) != normalized(args.current_title):
scripts/cider-metadata-fallback.py:421:         print(title)
scripts/cider-metadata-fallback.py:422:         return 0
scripts/cider-metadata-fallback.py:423:     return 1
…
scripts/cider-metadata-fallback.py:425: 
scripts/cider-metadata-fallback.py:426: if __name__ == "__main__":
scripts/cider-metadata-fallback.py:427:     raise SystemExit(main())
```

### `scripts/control-panel-status.sh`

Migration owner: `UI/provider integration (domain sections above)`. Source `scripts/control-panel-status.sh:1`, 43 lines, SHA-256 `1c3df9bb8f2ce3d1b764a1586387770d2fbe9cd6a11aa1b5f2a4233bdf222879`.

```text
scripts/control-panel-status.sh:1: #!/usr/bin/env bash
scripts/control-panel-status.sh:2: 
scripts/control-panel-status.sh:3: set -euo pipefail
scripts/control-panel-status.sh:4: 
scripts/control-panel-status.sh:5: # Brightness is read by quickAdjustBrightnessProbe on panel open and after
scripts/control-panel-status.sh:6: # user adjustments. Periodic DDC/CI queries stall the NVIDIA import path.
scripts/control-panel-status.sh:7: # Omit brightness here so unrelated status polling keeps the current UI value.
scripts/control-panel-status.sh:8: 
scripts/control-panel-status.sh:9: # Screen recording detection
scripts/control-panel-status.sh:10: recording=false
scripts/control-panel-status.sh:11: if pgrep -f '(^|/)(wf-recorder|wl-screenrec|gpu-screen-recorder)( |$)' >/dev/null 2>&1; then
scripts/control-panel-status.sh:12:     recording=true
scripts/control-panel-status.sh:13: fi
scripts/control-panel-status.sh:14: printf 'recording=%s\n' "$recording"
scripts/control-panel-status.sh:15: 
scripts/control-panel-status.sh:16: # Prevent sleep state
scripts/control-panel-status.sh:17: prevent_sleep=false
scripts/control-panel-status.sh:18: if [[ -x "${HOME}/.config/HyprV/quickshell/scripts/prevent-sleep.sh" ]]; then
scripts/control-panel-status.sh:19:     inhibit_status="$("${HOME}/.config/HyprV/quickshell/scripts/prevent-sleep.sh" status 2>/dev/null || true)"
scripts/control-panel-status.sh:20:     if [[ "$inhibit_status" == *"enabled=true"* ]]; then
scripts/control-panel-status.sh:21:         prevent_sleep=true
scripts/control-panel-status.sh:22:     fi
scripts/control-panel-status.sh:23: fi
scripts/control-panel-status.sh:24: printf 'prevent_sleep=%s\n' "$prevent_sleep"
scripts/control-panel-status.sh:25: 
scripts/control-panel-status.sh:26: # Demo mode state
scripts/control-panel-status.sh:27: demo_mode=false
scripts/control-panel-status.sh:28: if [[ -x "${HOME}/.config/HyprV/quickshell/scripts/demo-mode.sh" ]]; then
scripts/control-panel-status.sh:29:     demo_status="$("${HOME}/.config/HyprV/quickshell/scripts/demo-mode.sh" status 2>/dev/null || true)"
scripts/control-panel-status.sh:30:     if [[ "$demo_status" == *"enabled=true"* ]]; then
scripts/control-panel-status.sh:31:         demo_mode=true
scripts/control-panel-status.sh:32:     fi
scripts/control-panel-status.sh:33: fi
scripts/control-panel-status.sh:34: printf 'demo_mode=%s\n' "$demo_mode"
scripts/control-panel-status.sh:35: 
scripts/control-panel-status.sh:36: # Chassis fan controller state. The helper resolves the dynamic hwmon path by
scripts/control-panel-status.sh:37: # chip name and only reports the two channels exposed by the control panel.
scripts/control-panel-status.sh:38: fan_helper="${HOME}/.config/HyprV/quickshell/scripts/fan-control-helper.sh"
scripts/control-panel-status.sh:39: if [[ -x "$fan_helper" ]]; then
scripts/control-panel-status.sh:40:     "$fan_helper" status 2>/dev/null || printf 'fan_available=false\n'
scripts/control-panel-status.sh:41: else
scripts/control-panel-status.sh:42:     printf 'fan_available=false\n'
scripts/control-panel-status.sh:43: fi
```

### `scripts/control-panel-status.sh.before-viewflow-ddc-20260909`

Migration owner: `viewflow`. Historical backup; not selected by current QML. Source `scripts/control-panel-status.sh.before-viewflow-ddc-20260909:1`, 85 lines, SHA-256 `fab3b1c1e46d3e0602d62e3dab2a01bab80f7ff06ffe4a084aaec0f130234e72`.

```text
…
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:4: 
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:5: run_quick() {
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:6:     timeout 2s "$@" 2>/dev/null || true
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:7: }
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:8: 
…
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:10: wifi_enabled=false
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:11: if command -v nmcli >/dev/null 2>&1; then
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:12:     state="$(run_quick nmcli radio wifi)"
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:13:     [[ "$state" == "enabled" ]] && wifi_enabled=true
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:14: fi
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:15: printf 'wifi_enabled=%s\n' "$wifi_enabled"
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:16: 
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:17: # Screen brightness
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:18: brightness=50
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:19: brightness_script="${HOME}/.config/HyprV/hypr/scripts/brightness"
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:20: if [[ -x "$brightness_script" ]]; then
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:21:     pct="$(run_quick "$brightness_script" --get-level)"
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:22:     if [[ -n "$pct" && "$pct" =~ ^[0-9]+$ ]]; then
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:23:         brightness="$pct"
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:24:     fi
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:25: elif command -v brightnessctl >/dev/null 2>&1; then
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:26:     pct="$(timeout 2s sh -lc "brightnessctl -m 2>/dev/null | cut -d, -f4 | tr -d '%'" 2>/dev/null || true)"
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:27:     if [[ -n "$pct" && "$pct" =~ ^[0-9]+$ ]]; then
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:28:         brightness="$pct"
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:29:     fi
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:30: fi
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:31: printf 'brightness=%s\n' "$brightness"
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:32: 
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:33: # DND (Do Not Disturb) state
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:34: dnd=false
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:35: if command -v swaync-client >/dev/null 2>&1; then
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:36:     dnd_state="$(run_quick swaync-client -D)"
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:37:     [[ "$dnd_state" == "true" ]] && dnd=true
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:38: fi
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:39: printf 'dnd=%s\n' "$dnd"
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:40: 
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:41: # Screen recording detection
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:42: recording=false
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:43: if pgrep -x 'wf-recorder' >/dev/null 2>&1 || \
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:44:    pgrep -x 'wl-screenrec' >/dev/null 2>&1 || \
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:45:    pgrep -x 'gpu-screen-recorder' >/dev/null 2>&1; then
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:46:     recording=true
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:47: fi
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:48: printf 'recording=%s\n' "$recording"
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:49: 
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:50: # Power profile
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:51: power_profile="balanced"
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:52: if command -v powerprofilesctl >/dev/null 2>&1; then
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:53:     power_profile="$(run_quick powerprofilesctl get)"
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:54:     [[ -z "$power_profile" ]] && power_profile="balanced"
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:55: fi
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:56: printf 'power_profile=%s\n' "$power_profile"
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:57: 
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:58: # Prevent sleep state
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:59: prevent_sleep=false
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:60: if [[ -x "${HOME}/.config/HyprV/quickshell/scripts/prevent-sleep.sh" ]]; then
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:61:     inhibit_status="$("${HOME}/.config/HyprV/quickshell/scripts/prevent-sleep.sh" status 2>/dev/null || true)"
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:62:     if [[ "$inhibit_status" == *"enabled=true"* ]]; then
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:63:         prevent_sleep=true
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:64:     fi
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:65: fi
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:66: printf 'prevent_sleep=%s\n' "$prevent_sleep"
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:67: 
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:68: # Demo mode state
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:69: demo_mode=false
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:70: if [[ -x "${HOME}/.config/HyprV/quickshell/scripts/demo-mode.sh" ]]; then
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:71:     demo_status="$("${HOME}/.config/HyprV/quickshell/scripts/demo-mode.sh" status 2>/dev/null || true)"
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:72:     if [[ "$demo_status" == *"enabled=true"* ]]; then
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:73:         demo_mode=true
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:74:     fi
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:75: fi
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:76: printf 'demo_mode=%s\n' "$demo_mode"
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:77: 
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:78: # Chassis fan controller state. The helper resolves the dynamic hwmon path by
…
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:80: fan_helper="/usr/local/libexec/hyprv-fanctl"
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:81: if [[ -x "$fan_helper" ]]; then
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:82:     "$fan_helper" status 2>/dev/null || printf 'fan_available=false\n'
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:83: else
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:84:     printf 'fan_available=false\n'
scripts/control-panel-status.sh.before-viewflow-ddc-20260909:85: fi
```

### `scripts/cpu-power-sampler.service`

Migration owner: `sysstats/helper`. Source `scripts/cpu-power-sampler.service:1`, 15 lines, SHA-256 `bb0a837363e459a2b1a362060381f7ea77be5ebf416723f105e01da30dd88bc8`.

```text
scripts/cpu-power-sampler.service:1: [Unit]
scripts/cpu-power-sampler.service:2: Description=Publish CPU package power for HyprV
scripts/cpu-power-sampler.service:3: After=systemd-modules-load.service
scripts/cpu-power-sampler.service:4: ConditionPathExists=/usr/bin/turbostat
scripts/cpu-power-sampler.service:5: 
scripts/cpu-power-sampler.service:6: [Service]
scripts/cpu-power-sampler.service:7: Type=simple
scripts/cpu-power-sampler.service:8: ExecStart=/usr/local/libexec/hyprv-cpu-power-sampler
scripts/cpu-power-sampler.service:9: Restart=on-failure
scripts/cpu-power-sampler.service:10: RestartSec=2
scripts/cpu-power-sampler.service:11: RuntimeDirectory=hyprv
scripts/cpu-power-sampler.service:12: RuntimeDirectoryMode=0755
scripts/cpu-power-sampler.service:13: 
scripts/cpu-power-sampler.service:14: [Install]
scripts/cpu-power-sampler.service:15: WantedBy=multi-user.target
```

### `scripts/cpu-power-sampler.sh`

Migration owner: `sysstats/helper`. Source `scripts/cpu-power-sampler.sh:1`, 22 lines, SHA-256 `a4fab8bd62ffdee00053c0c7881164adc1d7e59965d59cc6d910c969c6d4e59a`.

```text
scripts/cpu-power-sampler.sh:1: #!/usr/bin/env bash
scripts/cpu-power-sampler.sh:2: 
scripts/cpu-power-sampler.sh:3: set -euo pipefail
scripts/cpu-power-sampler.sh:4: 
scripts/cpu-power-sampler.sh:5: readonly output_dir="/run/hyprv"
scripts/cpu-power-sampler.sh:6: readonly output_file="${output_dir}/cpu-package-power-w"
scripts/cpu-power-sampler.sh:7: 
scripts/cpu-power-sampler.sh:8: umask 022
scripts/cpu-power-sampler.sh:9: mkdir -p "$output_dir"
scripts/cpu-power-sampler.sh:10: 
scripts/cpu-power-sampler.sh:11: while true; do
scripts/cpu-power-sampler.sh:12:     sample="$(LC_ALL=C /usr/bin/turbostat --quiet --interval 1 --num_iterations 2 --show PkgWatt 2>/dev/null || true)"
scripts/cpu-power-sampler.sh:13:     value="$(awk '/^[0-9]+([.][0-9]+)?$/ { last = $1 } END { if (last != "") print last }' <<<"$sample")"
scripts/cpu-power-sampler.sh:14:     if [[ "$value" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
scripts/cpu-power-sampler.sh:15:         temporary="${output_file}.$$"
scripts/cpu-power-sampler.sh:16:         printf '%s\n' "$value" >"$temporary"
scripts/cpu-power-sampler.sh:17:         chmod 0644 "$temporary"
scripts/cpu-power-sampler.sh:18:         mv -f "$temporary" "$output_file"
scripts/cpu-power-sampler.sh:19:     else
scripts/cpu-power-sampler.sh:20:         sleep 1
scripts/cpu-power-sampler.sh:21:     fi
scripts/cpu-power-sampler.sh:22: done
```

### `scripts/demo-mode.sh`

Migration owner: `demo`. Source `scripts/demo-mode.sh:1`, 341 lines, SHA-256 `a5c6a7869696efcfc4c6a210f8fed7536638f311f4ddf9402c0bbc48ddea533d`.

```text
…
scripts/demo-mode.sh:4: 
scripts/demo-mode.sh:5: STATE_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/hyprv/quickshell/demo-mode"
scripts/demo-mode.sh:6: ACTIVE_FILE="$STATE_DIR/active"
scripts/demo-mode.sh:7: SESSION_FILE="$STATE_DIR/session"
scripts/demo-mode.sh:8: SOURCE_FILE="$STATE_DIR/source"
scripts/demo-mode.sh:9: HEADLESS_FILE="$STATE_DIR/headless"
scripts/demo-mode.sh:10: CREATED_HEADLESS_FILE="$STATE_DIR/created-headless"
scripts/demo-mode.sh:11: PREVENT_SLEEP_FILE="$STATE_DIR/prevent-sleep-was-enabled"
scripts/demo-mode.sh:12: PREVENT_SLEEP_SCRIPT="${HOME}/.config/HyprV/quickshell/scripts/prevent-sleep.sh"
scripts/demo-mode.sh:13: 
scripts/demo-mode.sh:14: notify() {
scripts/demo-mode.sh:15:     command -v notify-send >/dev/null 2>&1 || return 0
scripts/demo-mode.sh:16:     notify-send "HyprV Demo mode" "$1" >/dev/null 2>&1 || true
scripts/demo-mode.sh:17: }
scripts/demo-mode.sh:18: 
scripts/demo-mode.sh:19: current_session() {
scripts/demo-mode.sh:20:     printf '%s' "${HYPRLAND_INSTANCE_SIGNATURE:-unknown}"
scripts/demo-mode.sh:21: }
scripts/demo-mode.sh:22: 
scripts/demo-mode.sh:23: state_belongs_to_current_session() {
scripts/demo-mode.sh:24:     [[ -f "$ACTIVE_FILE" && -f "$SESSION_FILE" ]] || return 1
scripts/demo-mode.sh:25:     [[ "$(<"$SESSION_FILE")" == "$(current_session)" ]]
scripts/demo-mode.sh:26: }
scripts/demo-mode.sh:27: 
scripts/demo-mode.sh:28: cleanup_stale_state() {
scripts/demo-mode.sh:29:     [[ -e "$ACTIVE_FILE" ]] || return 0
scripts/demo-mode.sh:30:     state_belongs_to_current_session && return 0
scripts/demo-mode.sh:31:     rm -rf -- "$STATE_DIR"
scripts/demo-mode.sh:32: }
scripts/demo-mode.sh:33: 
scripts/demo-mode.sh:34: status() {
scripts/demo-mode.sh:35:     cleanup_stale_state
scripts/demo-mode.sh:36:     if state_belongs_to_current_session; then
scripts/demo-mode.sh:37:         printf 'enabled=true\n'
scripts/demo-mode.sh:38:         printf 'source=%s\n' "$(<"$SOURCE_FILE")"
scripts/demo-mode.sh:39:         printf 'headless=%s\n' "$(<"$HEADLESS_FILE")"
scripts/demo-mode.sh:40:         return 0
scripts/demo-mode.sh:41:     fi
scripts/demo-mode.sh:42:     printf 'enabled=false\n'
scripts/demo-mode.sh:43:     printf 'source=\n'
scripts/demo-mode.sh:44:     printf 'headless=\n'
scripts/demo-mode.sh:45:     return 1
scripts/demo-mode.sh:46: }
scripts/demo-mode.sh:47: 
scripts/demo-mode.sh:48: monitor_eval() {
scripts/demo-mode.sh:49:     local output="$1"
scripts/demo-mode.sh:50:     local mode="$2"
…
scripts/demo-mode.sh:56:     expression="hl.monitor({ output = '${output}', mode = '${mode}', position = '${position}', scale = '1', transform = ${transform}"
scripts/demo-mode.sh:57:     if [[ -n "$mirror" ]]; then
scripts/demo-mode.sh:58:         expression+=", mirror = '${mirror}'"
scripts/demo-mode.sh:59:     fi
…
scripts/demo-mode.sh:61: 
scripts/demo-mode.sh:62:     result="$(hyprctl eval "$expression" 2>&1)" || {
scripts/demo-mode.sh:63:         printf '%s\n' "$result" >&2
scripts/demo-mode.sh:64:         return 1
scripts/demo-mode.sh:65:     }
scripts/demo-mode.sh:66:     if grep -Eqi '(^|[^a-z])(error|invalid|failed|cannot)([^a-z]|$)' <<<"$result"; then
scripts/demo-mode.sh:67:         printf '%s\n' "$result" >&2
scripts/demo-mode.sh:68:         return 1
scripts/demo-mode.sh:69:     fi
…
scripts/demo-mode.sh:71: 
scripts/demo-mode.sh:72: touch_eval() {
scripts/demo-mode.sh:73:     local name="$1"
scripts/demo-mode.sh:74:     local result
scripts/demo-mode.sh:75: 
scripts/demo-mode.sh:76:     result="$(hyprctl eval "hl.device({ name = '${name}', transform = 0 })" 2>&1)" || {
scripts/demo-mode.sh:77:         printf '%s\n' "$result" >&2
scripts/demo-mode.sh:78:         return 1
scripts/demo-mode.sh:79:     }
scripts/demo-mode.sh:80:     if grep -Eqi '(^|[^a-z])(error|invalid|failed|cannot)([^a-z]|$)' <<<"$result"; then
scripts/demo-mode.sh:81:         printf '%s\n' "$result" >&2
scripts/demo-mode.sh:82:         return 1
scripts/demo-mode.sh:83:     fi
…
scripts/demo-mode.sh:85: 
scripts/demo-mode.sh:86: portrait_mode_for() {
scripts/demo-mode.sh:87:     local monitor_json="$1"
scripts/demo-mode.sh:88:     if jq -e '.availableModes[]? | select(test("^1080x1920@(59\\.|60\\.)"))' \
scripts/demo-mode.sh:89:         <<<"$monitor_json" >/dev/null; then
scripts/demo-mode.sh:90:         printf '1080x1920@60|0\n'
scripts/demo-mode.sh:91:     else
scripts/demo-mode.sh:92:         # Most panels advertise their physical landscape mode and are made
scripts/demo-mode.sh:93:         # logically 1080x1920 with a compositor transform.
scripts/demo-mode.sh:94:         printf '1920x1080@60|1\n'
scripts/demo-mode.sh:95:     fi
scripts/demo-mode.sh:96: }
scripts/demo-mode.sh:97: 
scripts/demo-mode.sh:98: find_external_outputs() {
scripts/demo-mode.sh:99:     jq -r '
scripts/demo-mode.sh:100:         .[]
scripts/demo-mode.sh:101:         | select(.disabled != true)
…
scripts/demo-mode.sh:109: 
scripts/demo-mode.sh:110: find_headless_output() {
scripts/demo-mode.sh:111:     jq -r '[.[] | select(.disabled != true and (.name | startswith("HEADLESS-"))) | .name][0] // empty'
scripts/demo-mode.sh:112: }
scripts/demo-mode.sh:113: 
scripts/demo-mode.sh:114: external_position() {
scripts/demo-mode.sh:115:     local monitors_json="$1"
scripts/demo-mode.sh:116:     jq -r '
scripts/demo-mode.sh:117:         [
scripts/demo-mode.sh:118:             .[]
…
scripts/demo-mode.sh:136:           end
scripts/demo-mode.sh:137:     ' <<<"$monitors_json"
scripts/demo-mode.sh:138: }
scripts/demo-mode.sh:139: 
scripts/demo-mode.sh:140: prevent_sleep_enabled() {
scripts/demo-mode.sh:141:     [[ -x "$PREVENT_SLEEP_SCRIPT" ]] || return 1
scripts/demo-mode.sh:142:     "$PREVENT_SLEEP_SCRIPT" status 2>/dev/null | grep -q '^enabled=true$'
scripts/demo-mode.sh:143: }
scripts/demo-mode.sh:144: 
scripts/demo-mode.sh:145: rollback_enable() {
scripts/demo-mode.sh:146:     local created_headless="${1:-}"
scripts/demo-mode.sh:147:     hyprctl reload >/dev/null 2>&1 || true
scripts/demo-mode.sh:148:     if [[ -n "$created_headless" ]]; then
scripts/demo-mode.sh:149:         hyprctl output remove "$created_headless" >/dev/null 2>&1 || true
scripts/demo-mode.sh:150:     fi
scripts/demo-mode.sh:151:     if [[ -f "$PREVENT_SLEEP_FILE" && "$(<"$PREVENT_SLEEP_FILE")" == "false" ]]; then
scripts/demo-mode.sh:152:         "$PREVENT_SLEEP_SCRIPT" disable >/dev/null 2>&1 || true
scripts/demo-mode.sh:153:     fi
scripts/demo-mode.sh:154:     rm -rf -- "$STATE_DIR"
scripts/demo-mode.sh:155: }
scripts/demo-mode.sh:156: 
scripts/demo-mode.sh:157: enable() {
scripts/demo-mode.sh:158:     cleanup_stale_state
scripts/demo-mode.sh:159:     if state_belongs_to_current_session; then
…
scripts/demo-mode.sh:163: 
scripts/demo-mode.sh:164:     command -v hyprctl >/dev/null 2>&1 || {
scripts/demo-mode.sh:165:         printf 'hyprctl is required\n' >&2
scripts/demo-mode.sh:166:         return 1
scripts/demo-mode.sh:167:     }
scripts/demo-mode.sh:168:     command -v jq >/dev/null 2>&1 || {
scripts/demo-mode.sh:169:         printf 'jq is required\n' >&2
scripts/demo-mode.sh:170:         return 1
scripts/demo-mode.sh:171:     }
…
scripts/demo-mode.sh:174:     local headless created_headless="" source_mode source_transform
scripts/demo-mode.sh:175:     monitors_json="$(hyprctl monitors all -j)"
scripts/demo-mode.sh:176:     mapfile -t external_outputs < <(find_external_outputs <<<"$monitors_json")
scripts/demo-mode.sh:177:     if (( ${#external_outputs[@]} == 0 )); then
scripts/demo-mode.sh:178:         notify "没有检测到外接显示器，Demo mode 未启动"
scripts/demo-mode.sh:179:         printf 'no external display is connected\n' >&2
scripts/demo-mode.sh:180:         return 1
scripts/demo-mode.sh:181:     fi
…
scripts/demo-mode.sh:183:     source="${external_outputs[0]}"
scripts/demo-mode.sh:184:     source_json="$(jq -c --arg name "$source" '.[] | select(.name == $name)' <<<"$monitors_json")"
scripts/demo-mode.sh:185:     IFS='|' read -r source_mode source_transform < <(portrait_mode_for "$source_json")
scripts/demo-mode.sh:186:     source_position="$(external_position "$monitors_json")"
scripts/demo-mode.sh:187:     headless="$(find_headless_output <<<"$monitors_json")"
scripts/demo-mode.sh:188: 
scripts/demo-mode.sh:189:     mkdir -p "$STATE_DIR"
scripts/demo-mode.sh:190:     printf '%s\n' "$(current_session)" > "$SESSION_FILE"
scripts/demo-mode.sh:191:     printf '%s\n' "$source" > "$SOURCE_FILE"
scripts/demo-mode.sh:192:     printf '%s\n' "$headless" > "$HEADLESS_FILE"
scripts/demo-mode.sh:193:     touch "$ACTIVE_FILE"
scripts/demo-mode.sh:194:     if prevent_sleep_enabled; then
scripts/demo-mode.sh:195:         printf 'true\n' > "$PREVENT_SLEEP_FILE"
scripts/demo-mode.sh:196:     else
scripts/demo-mode.sh:197:         printf 'false\n' > "$PREVENT_SLEEP_FILE"
scripts/demo-mode.sh:198:         "$PREVENT_SLEEP_SCRIPT" enable >/dev/null
scripts/demo-mode.sh:199:     fi
scripts/demo-mode.sh:200: 
scripts/demo-mode.sh:201:     if [[ -z "$headless" ]]; then
scripts/demo-mode.sh:202:         local before_json after_json
scripts/demo-mode.sh:203:         before_json="$monitors_json"
scripts/demo-mode.sh:204:         hyprctl output create headless >/dev/null
scripts/demo-mode.sh:205:         sleep 0.2
scripts/demo-mode.sh:206:         after_json="$(hyprctl monitors all -j)"
scripts/demo-mode.sh:207:         headless="$(
scripts/demo-mode.sh:208:             jq -nr --argjson before "$before_json" --argjson after "$after_json" '
scripts/demo-mode.sh:209:                 ($before | map(.name)) as $old
scripts/demo-mode.sh:210:                 | [$after[] | select((.name | startswith("HEADLESS-")) and (.name as $name | $old | index($name) | not)) | .name][0] // empty
scripts/demo-mode.sh:211:             '
scripts/demo-mode.sh:212:         )"
scripts/demo-mode.sh:213:         if [[ -z "$headless" ]]; then
scripts/demo-mode.sh:214:             rollback_enable
scripts/demo-mode.sh:215:             notify "Headless 输出创建失败"
scripts/demo-mode.sh:216:             printf 'failed to create a headless output\n' >&2
scripts/demo-mode.sh:217:             return 1
scripts/demo-mode.sh:218:         fi
scripts/demo-mode.sh:219:         created_headless="$headless"
scripts/demo-mode.sh:220:         printf '%s\n' "$headless" > "$HEADLESS_FILE"
scripts/demo-mode.sh:221:         printf '%s\n' "$headless" > "$CREATED_HEADLESS_FILE"
scripts/demo-mode.sh:222:     fi
scripts/demo-mode.sh:223: 
scripts/demo-mode.sh:224:     if ! monitor_eval "$source" "$source_mode" "$source_position" "$source_transform"; then
scripts/demo-mode.sh:225:         rollback_enable "$created_headless"
scripts/demo-mode.sh:226:         notify "外接显示器配置失败，已恢复原布局"
scripts/demo-mode.sh:227:         return 1
…
scripts/demo-mode.sh:231:     for output in "${external_outputs[@]:1}"; do
scripts/demo-mode.sh:232:         output_json="$(jq -c --arg name "$output" '.[] | select(.name == $name)' <<<"$monitors_json")"
scripts/demo-mode.sh:233:         IFS='|' read -r mode transform < <(portrait_mode_for "$output_json")
scripts/demo-mode.sh:234:         if ! monitor_eval "$output" "$mode" "auto-right" "$transform" "$source"; then
scripts/demo-mode.sh:235:             rollback_enable "$created_headless"
scripts/demo-mode.sh:236:             notify "外接显示器配置失败，已恢复原布局"
scripts/demo-mode.sh:237:             return 1
…
scripts/demo-mode.sh:240: 
scripts/demo-mode.sh:241:     if ! monitor_eval "$headless" "$source_mode" "auto-right" "$source_transform" "$source"; then
scripts/demo-mode.sh:242:         rollback_enable "$created_headless"
scripts/demo-mode.sh:243:         notify "Headless 复制配置失败，已恢复原布局"
scripts/demo-mode.sh:244:         return 1
…
scripts/demo-mode.sh:247:     local devices_json touch
scripts/demo-mode.sh:248:     devices_json="$(hyprctl devices -j)"
scripts/demo-mode.sh:249:     while IFS= read -r touch; do
scripts/demo-mode.sh:250:         [[ -n "$touch" ]] || continue
scripts/demo-mode.sh:251:         touch_eval "$touch" || {
scripts/demo-mode.sh:252:             rollback_enable "$created_headless"
scripts/demo-mode.sh:253:             notify "触控方向配置失败，已恢复原布局"
scripts/demo-mode.sh:254:             return 1
scripts/demo-mode.sh:255:         }
scripts/demo-mode.sh:256:     done < <(jq -r '.touch[]?.name' <<<"$devices_json")
scripts/demo-mode.sh:257: 
scripts/demo-mode.sh:258:     sleep 0.3
scripts/demo-mode.sh:259:     local live_json
scripts/demo-mode.sh:260:     live_json="$(hyprctl monitors -j)"
scripts/demo-mode.sh:261:     if ! jq -e --arg source "$source" --arg headless "$headless" '
scripts/demo-mode.sh:262:         any(.[]; .name == $source and
scripts/demo-mode.sh:263:             ((.width == 1080 and .height == 1920 and .transform == 0) or
scripts/demo-mode.sh:264:              (.width == 1920 and .height == 1080 and .transform == 1))) and
scripts/demo-mode.sh:265:         any(.[]; .name == $headless and .mirrorOf == $source)
scripts/demo-mode.sh:266:     ' <<<"$live_json" >/dev/null; then
scripts/demo-mode.sh:267:         rollback_enable "$created_headless"
scripts/demo-mode.sh:268:         notify "Demo mode 验证失败，已恢复原布局"
scripts/demo-mode.sh:269:         printf 'demo monitor verification failed\n' >&2
scripts/demo-mode.sh:270:         return 1
scripts/demo-mode.sh:271:     fi
…
scripts/demo-mode.sh:273:     for output in "${external_outputs[@]:1}"; do
scripts/demo-mode.sh:274:         if ! jq -e --arg output "$output" --arg source "$source" '
scripts/demo-mode.sh:275:             any(.[]; .name == $output and .mirrorOf == $source and
scripts/demo-mode.sh:276:                 ((.width == 1080 and .height == 1920 and .transform == 0) or
scripts/demo-mode.sh:277:                  (.width == 1920 and .height == 1080 and .transform == 1)))
scripts/demo-mode.sh:278:         ' <<<"$live_json" >/dev/null; then
scripts/demo-mode.sh:279:             rollback_enable "$created_headless"
scripts/demo-mode.sh:280:             notify "Demo mode 逐屏验证失败，已恢复原布局"
scripts/demo-mode.sh:281:             printf 'external mirror verification failed for %s\n' "$output" >&2
scripts/demo-mode.sh:282:             return 1
scripts/demo-mode.sh:283:         fi
…
scripts/demo-mode.sh:289: 
scripts/demo-mode.sh:290: disable() {
scripts/demo-mode.sh:291:     cleanup_stale_state
scripts/demo-mode.sh:292:     if ! state_belongs_to_current_session; then
…
scripts/demo-mode.sh:296: 
scripts/demo-mode.sh:297:     local created_headless="" prevent_sleep_was_enabled="false"
scripts/demo-mode.sh:298:     [[ -f "$CREATED_HEADLESS_FILE" ]] && created_headless="$(<"$CREATED_HEADLESS_FILE")"
scripts/demo-mode.sh:299:     [[ -f "$PREVENT_SLEEP_FILE" ]] && prevent_sleep_was_enabled="$(<"$PREVENT_SLEEP_FILE")"
scripts/demo-mode.sh:300: 
scripts/demo-mode.sh:301:     hyprctl reload >/dev/null
scripts/demo-mode.sh:302:     if [[ -n "$created_headless" ]]; then
scripts/demo-mode.sh:303:         hyprctl output remove "$created_headless" >/dev/null 2>&1 || true
scripts/demo-mode.sh:304:     fi
scripts/demo-mode.sh:305:     if [[ "$prevent_sleep_was_enabled" != "true" ]]; then
scripts/demo-mode.sh:306:         "$PREVENT_SLEEP_SCRIPT" disable >/dev/null 2>&1 || true
scripts/demo-mode.sh:307:     fi
scripts/demo-mode.sh:308:     rm -rf -- "$STATE_DIR"
scripts/demo-mode.sh:309:     notify "已退出并恢复原显示器、触控与睡眠设置"
scripts/demo-mode.sh:310:     printf 'enabled=false\n'
scripts/demo-mode.sh:311:     printf 'source=\n'
scripts/demo-mode.sh:312:     printf 'headless=\n'
scripts/demo-mode.sh:313: }
scripts/demo-mode.sh:314: 
scripts/demo-mode.sh:315: toggle() {
scripts/demo-mode.sh:316:     cleanup_stale_state
scripts/demo-mode.sh:317:     if state_belongs_to_current_session; then
…
scripts/demo-mode.sh:323: 
scripts/demo-mode.sh:324: case "${1:-status}" in
scripts/demo-mode.sh:325:     status)
scripts/demo-mode.sh:326:         status
scripts/demo-mode.sh:327:         ;;
scripts/demo-mode.sh:328:     enable)
scripts/demo-mode.sh:329:         enable
scripts/demo-mode.sh:330:         ;;
scripts/demo-mode.sh:331:     disable)
scripts/demo-mode.sh:332:         disable
scripts/demo-mode.sh:333:         ;;
scripts/demo-mode.sh:334:     toggle)
scripts/demo-mode.sh:335:         toggle
scripts/demo-mode.sh:336:         ;;
scripts/demo-mode.sh:337:     *)
scripts/demo-mode.sh:338:         printf 'Usage: %s [status|enable|disable|toggle]\n' "$0" >&2
scripts/demo-mode.sh:339:         exit 1
scripts/demo-mode.sh:340:         ;;
```

### `scripts/deskflow-target.sh`

Migration owner: `deskflow`. Source `scripts/deskflow-target.sh:1`, 258 lines, SHA-256 `e79f7130319f016f4c42933c4cb7871b866c8c83aa67a81a127d0173fdaea7f9`.

```text
…
scripts/deskflow-target.sh:3: 
scripts/deskflow-target.sh:4: readonly config_dir="${XDG_CONFIG_HOME:-$HOME/.config}/Deskflow"
scripts/deskflow-target.sh:5: readonly config_file="$config_dir/deskflow-server.conf"
scripts/deskflow-target.sh:6: readonly log_file="${DESKFLOW_LOG_FILE:-$HOME/deskflow.log}"
scripts/deskflow-target.sh:7: readonly transform_file="${XDG_STATE_HOME:-$HOME/.local/state}/hyprv/monitor-transform"
scripts/deskflow-target.sh:8: readonly windows_screen=WindowsVM
scripts/deskflow-target.sh:9: readonly macos_screen=linhaikuodeMac-mini.local
scripts/deskflow-target.sh:10: script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
scripts/deskflow-target.sh:11: readonly script_dir
scripts/deskflow-target.sh:12: readonly ir_helper="$script_dir/hdmi-switch-ir.py"
scripts/deskflow-target.sh:13: 
scripts/deskflow-target.sh:14: current_target() {
scripts/deskflow-target.sh:15:   if [[ -r "$config_file" ]] && grep -Eq "right\\([0-9]+,[0-9]+\\) = ${macos_screen}\\(0,100\\)" "$config_file"; then
scripts/deskflow-target.sh:16:     printf '%s\n' macos
scripts/deskflow-target.sh:17:   else
scripts/deskflow-target.sh:18:     printf '%s\n' windows
scripts/deskflow-target.sh:19:   fi
scripts/deskflow-target.sh:20: }
scripts/deskflow-target.sh:21: 
scripts/deskflow-target.sh:22: portal_range() {
scripts/deskflow-target.sh:23:   local transform=0
scripts/deskflow-target.sh:24:   if [[ -r $transform_file ]]; then
scripts/deskflow-target.sh:25:     read -r transform <"$transform_file" || true
scripts/deskflow-target.sh:26:   else
scripts/deskflow-target.sh:27:     transform=$(hyprctl -j monitors all 2>/dev/null \
scripts/deskflow-target.sh:28:       | jq -r '.[] | select(.name == "DP-4") | .transform' \
scripts/deskflow-target.sh:29:       | head -n 1)
scripts/deskflow-target.sh:30:   fi
scripts/deskflow-target.sh:31: 
scripts/deskflow-target.sh:32:   case "$transform" in
scripts/deskflow-target.sh:33:     1|3) printf '%s %s\n' 38 71 ;;
scripts/deskflow-target.sh:34:     *) printf '%s %s\n' 28 87 ;;
scripts/deskflow-target.sh:35:   esac
scripts/deskflow-target.sh:36: }
scripts/deskflow-target.sh:37: 
scripts/deskflow-target.sh:38: write_config() {
scripts/deskflow-target.sh:39:   local target=$1 target_screen temp_file portal_start portal_end
scripts/deskflow-target.sh:40:   case "$target" in
scripts/deskflow-target.sh:41:     windows) target_screen=$windows_screen ;;
scripts/deskflow-target.sh:42:     macos) target_screen=$macos_screen ;;
scripts/deskflow-target.sh:43:     *) return 2 ;;
scripts/deskflow-target.sh:44:   esac
…
scripts/deskflow-target.sh:46: 
scripts/deskflow-target.sh:47:   mkdir -p "$config_dir"
scripts/deskflow-target.sh:48:   temp_file=$(mktemp "$config_dir/.deskflow-server.conf.XXXXXX")
scripts/deskflow-target.sh:49:   trap 'rm -f "$temp_file"' RETURN
scripts/deskflow-target.sh:50: 
scripts/deskflow-target.sh:51:   sed "s/__TARGET_SCREEN__/$target_screen/g" >"$temp_file" <<'EOF'
scripts/deskflow-target.sh:52: section: screens
scripts/deskflow-target.sh:53: 		WindowsVM:
…
scripts/deskflow-target.sh:59: 		switchCornerSize = 0
scripts/deskflow-target.sh:60: 	linhaikuodeMac-mini.local:
scripts/deskflow-target.sh:61: 		halfDuplexCapsLock = false
scripts/deskflow-target.sh:62: 		halfDuplexNumLock = false
…
scripts/deskflow-target.sh:89: 	win32KeepForeground = false
scripts/deskflow-target.sh:90: 	defaultLockToScreenState = false
scripts/deskflow-target.sh:91: 	disableLockToScreen = false
scripts/deskflow-target.sh:92: 	clipboardSharing = true
…
scripts/deskflow-target.sh:101:   sed -i \
scripts/deskflow-target.sh:102:     -e "s/__PORTAL_START__/$portal_start/g" \
scripts/deskflow-target.sh:103:     -e "s/__PORTAL_END__/$portal_end/g" \
scripts/deskflow-target.sh:104:     "$temp_file"
scripts/deskflow-target.sh:105: 
scripts/deskflow-target.sh:106:   chmod 600 "$temp_file"
scripts/deskflow-target.sh:107:   mv -f "$temp_file" "$config_file"
scripts/deskflow-target.sh:108:   trap - RETURN
scripts/deskflow-target.sh:109: }
scripts/deskflow-target.sh:110: 
scripts/deskflow-target.sh:111: find_deskflow_core_pid() {
scripts/deskflow-target.sh:112:   local control_group proc exe cmdline
scripts/deskflow-target.sh:113:   local -a candidates=()
scripts/deskflow-target.sh:114: 
scripts/deskflow-target.sh:115:   if [[ $(systemctl --user is-active deskflow.service 2>/dev/null || true) == active ]]; then
scripts/deskflow-target.sh:116:     control_group=$(systemctl --user show deskflow.service -p ControlGroup --value)
scripts/deskflow-target.sh:117: 
scripts/deskflow-target.sh:118:     if [[ -n "$control_group" && -r "/sys/fs/cgroup${control_group}/cgroup.procs" ]]; then
scripts/deskflow-target.sh:119:       while IFS= read -r proc; do
scripts/deskflow-target.sh:120:         [[ -r "/proc/$proc/exe" ]] || continue
scripts/deskflow-target.sh:121:         exe=$(readlink "/proc/$proc/exe" 2>/dev/null || true)
scripts/deskflow-target.sh:122:         exe=${exe% (deleted)}
scripts/deskflow-target.sh:123:         if [[ ${exe##*/} == deskflow-core ]]; then
scripts/deskflow-target.sh:124:           printf '%s\n' "$proc"
scripts/deskflow-target.sh:125:           return 0
scripts/deskflow-target.sh:126:         fi
scripts/deskflow-target.sh:127:       done <"/sys/fs/cgroup${control_group}/cgroup.procs"
scripts/deskflow-target.sh:128:     fi
scripts/deskflow-target.sh:129:   fi
…
scripts/deskflow-target.sh:131:   while IFS= read -r proc; do
scripts/deskflow-target.sh:132:     [[ -r "/proc/$proc/exe" && -r "/proc/$proc/cmdline" ]] || continue
scripts/deskflow-target.sh:133:     exe=$(readlink "/proc/$proc/exe" 2>/dev/null || true)
scripts/deskflow-target.sh:134:     exe=${exe% (deleted)}
scripts/deskflow-target.sh:135:     cmdline=$(tr '\0' ' ' <"/proc/$proc/cmdline")
scripts/deskflow-target.sh:136:     [[ ${exe##*/} == deskflow-core && " $cmdline " == *' server '* ]] || continue
scripts/deskflow-target.sh:137:     candidates+=("$proc")
scripts/deskflow-target.sh:138:   done < <(pgrep -x deskflow-core || true)
scripts/deskflow-target.sh:139: 
scripts/deskflow-target.sh:140:   ((${#candidates[@]} == 1)) || return 1
scripts/deskflow-target.sh:141:   printf '%s\n' "${candidates[0]}"
scripts/deskflow-target.sh:142: }
scripts/deskflow-target.sh:143: 
scripts/deskflow-target.sh:144: reload_deskflow() {
scripts/deskflow-target.sh:145:   local core_pid log log_offset=0
scripts/deskflow-target.sh:146: 
scripts/deskflow-target.sh:147:   if ! core_pid=$(find_deskflow_core_pid); then
scripts/deskflow-target.sh:148:     printf 'Deskflow is not service-managed; configuration will apply on its next start\n' >&2
scripts/deskflow-target.sh:149:     return 3
scripts/deskflow-target.sh:150:   fi
scripts/deskflow-target.sh:151: 
scripts/deskflow-target.sh:152:   [[ -f "$log_file" ]] && log_offset=$(stat -c %s "$log_file")
scripts/deskflow-target.sh:153:   kill -HUP "$core_pid"
scripts/deskflow-target.sh:154: 
scripts/deskflow-target.sh:155:   for _ in {1..20}; do
scripts/deskflow-target.sh:156:     log=$(tail -c "+$((log_offset + 1))" "$log_file" 2>/dev/null || true)
scripts/deskflow-target.sh:157:     grep -Fq 'reloaded configuration' <<<"$log" && return 0
scripts/deskflow-target.sh:158:     kill -0 "$core_pid" 2>/dev/null || break
scripts/deskflow-target.sh:159:     sleep 0.1
scripts/deskflow-target.sh:160:   done
scripts/deskflow-target.sh:161: 
scripts/deskflow-target.sh:162:   printf 'Deskflow did not confirm configuration reload\n' >&2
scripts/deskflow-target.sh:163:   return 1
scripts/deskflow-target.sh:164: }
scripts/deskflow-target.sh:165: 
scripts/deskflow-target.sh:166: switch_display_ir() {
scripts/deskflow-target.sh:167:   local target=$1 button
scripts/deskflow-target.sh:168:   case "$target" in
scripts/deskflow-target.sh:169:     windows) button=input1 ;;
scripts/deskflow-target.sh:170:     macos) button=input2 ;;
scripts/deskflow-target.sh:171:     *) printf 'Unknown display target: %s\n' "$target" >&2; return 2 ;;
scripts/deskflow-target.sh:172:   esac
scripts/deskflow-target.sh:173: 
scripts/deskflow-target.sh:174:   [[ -x "$ir_helper" ]] || {
scripts/deskflow-target.sh:175:     printf 'HDMI switch IR helper is not executable: %s\n' "$ir_helper" >&2
scripts/deskflow-target.sh:176:     return 1
scripts/deskflow-target.sh:177:   }
scripts/deskflow-target.sh:178:   "$ir_helper" send "$button" >/dev/null
scripts/deskflow-target.sh:179: }
scripts/deskflow-target.sh:180: 
scripts/deskflow-target.sh:181: queue_display_switch() {
scripts/deskflow-target.sh:182:   local target=$1
scripts/deskflow-target.sh:183:   local script_path
scripts/deskflow-target.sh:184:   script_path=$(readlink -f "$0")
scripts/deskflow-target.sh:185:   systemd-run --user --quiet --collect \
scripts/deskflow-target.sh:186:     --unit="hyprv-display-switch-$(date +%s%N)" \
scripts/deskflow-target.sh:187:     --property=Type=exec \
scripts/deskflow-target.sh:188:     bash "$script_path" display-switch "$target"
scripts/deskflow-target.sh:189: }
scripts/deskflow-target.sh:190: 
scripts/deskflow-target.sh:191: case "${1:-}" in
scripts/deskflow-target.sh:192:   status)
scripts/deskflow-target.sh:193:     current_target
scripts/deskflow-target.sh:194:     ;;
scripts/deskflow-target.sh:195:   set)
scripts/deskflow-target.sh:196:     requested_target=${2:-}
scripts/deskflow-target.sh:197:     case "$requested_target" in
scripts/deskflow-target.sh:198:       windows|macos) ;;
scripts/deskflow-target.sh:199:       *) printf 'Unknown Deskflow target: %s\n' "$requested_target" >&2; exit 2 ;;
scripts/deskflow-target.sh:200:     esac
scripts/deskflow-target.sh:201: 
scripts/deskflow-target.sh:202:     backup_file=$(mktemp "$config_dir/.deskflow-server.conf.backup.XXXXXX")
scripts/deskflow-target.sh:203:     trap 'rm -f "$backup_file"' EXIT
scripts/deskflow-target.sh:204:     cp -a "$config_file" "$backup_file"
scripts/deskflow-target.sh:205:     write_config "$requested_target"
scripts/deskflow-target.sh:206:     if reload_deskflow; then
scripts/deskflow-target.sh:207:       :
…
scripts/deskflow-target.sh:210:       if (( reload_status != 3 )); then
scripts/deskflow-target.sh:211:         mv -f "$backup_file" "$config_file"
scripts/deskflow-target.sh:212:         reload_deskflow || true
scripts/deskflow-target.sh:213:         printf 'Restored the previous Deskflow configuration\n' >&2
scripts/deskflow-target.sh:214:         exit 1
scripts/deskflow-target.sh:215:       fi
scripts/deskflow-target.sh:216:     fi
scripts/deskflow-target.sh:217:     rm -f "$backup_file"
scripts/deskflow-target.sh:218:     trap - EXIT
scripts/deskflow-target.sh:219:     queue_display_switch "$requested_target"
scripts/deskflow-target.sh:220:     current_target
scripts/deskflow-target.sh:221:     ;;
scripts/deskflow-target.sh:222:   reflow)
scripts/deskflow-target.sh:223:     requested_target=$(current_target)
scripts/deskflow-target.sh:224:     backup_file=$(mktemp "$config_dir/.deskflow-server.conf.backup.XXXXXX")
scripts/deskflow-target.sh:225:     trap 'rm -f "$backup_file"' EXIT
scripts/deskflow-target.sh:226:     cp -a "$config_file" "$backup_file"
scripts/deskflow-target.sh:227:     write_config "$requested_target"
scripts/deskflow-target.sh:228:     if reload_deskflow; then
scripts/deskflow-target.sh:229:       :
…
scripts/deskflow-target.sh:232:       if (( reload_status != 3 )); then
scripts/deskflow-target.sh:233:         mv -f "$backup_file" "$config_file"
scripts/deskflow-target.sh:234:         reload_deskflow || true
scripts/deskflow-target.sh:235:         printf 'Restored the previous Deskflow configuration\n' >&2
scripts/deskflow-target.sh:236:         exit 1
scripts/deskflow-target.sh:237:       fi
scripts/deskflow-target.sh:238:     fi
scripts/deskflow-target.sh:239:     rm -f "$backup_file"
scripts/deskflow-target.sh:240:     trap - EXIT
scripts/deskflow-target.sh:241:     ;;
scripts/deskflow-target.sh:242:   display-switch)
scripts/deskflow-target.sh:243:     case "${2:-}" in
scripts/deskflow-target.sh:244:       windows|macos)
scripts/deskflow-target.sh:245:         if ! switch_display_ir "$2"; then
scripts/deskflow-target.sh:246:           command -v notify-send >/dev/null 2>&1 \
scripts/deskflow-target.sh:247:             && notify-send "Win/macOS switch" "HDMI switch to $2 failed" || true
scripts/deskflow-target.sh:248:           exit 1
scripts/deskflow-target.sh:249:         fi
scripts/deskflow-target.sh:250:         ;;
scripts/deskflow-target.sh:251:       *) printf 'Unknown display target: %s\n' "${2:-}" >&2; exit 2 ;;
scripts/deskflow-target.sh:252:     esac
scripts/deskflow-target.sh:253:     ;;
scripts/deskflow-target.sh:254:   *)
scripts/deskflow-target.sh:255:     printf 'Usage: %s {status|set windows|set macos|reflow|display-switch macos|display-switch windows}\n' "$0" >&2
scripts/deskflow-target.sh:256:     exit 2
scripts/deskflow-target.sh:257:     ;;
```

### `scripts/desktop-state.py`

Migration owner: `sysstats/network/agent_island`. Source `scripts/desktop-state.py:1`, 211 lines, SHA-256 `7e952a9dff3e963a3a183f3904fd40a91a693a1866131b84e6dd607f50443f20`.

```text
…
scripts/desktop-state.py:18: class Publisher:
scripts/desktop-state.py:19:     def __init__(self):
scripts/desktop-state.py:20:         self.previous = {}
scripts/desktop-state.py:21: 
scripts/desktop-state.py:22:     def emit(self, kind, data):
scripts/desktop-state.py:23:         encoded = json.dumps(data, ensure_ascii=False, separators=(",", ":"))
scripts/desktop-state.py:24:         if self.previous.get(kind) != encoded:
scripts/desktop-state.py:25:             print('{"type":' + json.dumps(kind) + ',"data":' + encoded + '}', flush=True)
scripts/desktop-state.py:26:             self.previous[kind] = encoded
scripts/desktop-state.py:27: 
scripts/desktop-state.py:28: 
scripts/desktop-state.py:29: def read_text(path):
scripts/desktop-state.py:30:     try:
scripts/desktop-state.py:31:         return Path(path).read_text()
scripts/desktop-state.py:32:     except OSError:
scripts/desktop-state.py:33:         return ""
…
scripts/desktop-state.py:36: class KernelSampler:
scripts/desktop-state.py:37:     def __init__(self, publisher):
scripts/desktop-state.py:38:         self.publisher = publisher
scripts/desktop-state.py:39:         self.temperature = None
…
scripts/desktop-state.py:41: 
scripts/desktop-state.py:42:     def discover_temperature(self):
scripts/desktop-state.py:43:         zones = list(Path("/sys/class/thermal").glob("thermal_zone*"))
scripts/desktop-state.py:44:         for name in ("x86_pkg_temp", "TCPU", "cpu_thermal", "cpu-thermal", "soc_thermal"):
scripts/desktop-state.py:45:             for zone in zones:
…
scripts/desktop-state.py:49:                         return zone / "temp"
scripts/desktop-state.py:50:         for hwmon in Path("/sys/class/hwmon").glob("hwmon*"):
scripts/desktop-state.py:51:             if read_text(hwmon / "name").strip() not in ("coretemp", "k10temp", "zenpower"):
scripts/desktop-state.py:52:                 continue
scripts/desktop-state.py:53:             for label in hwmon.glob("temp*_label"):
scripts/desktop-state.py:54:                 if read_text(label).strip() in ("Package id 0", "Tctl", "CPU Package", "CPU"):
scripts/desktop-state.py:55:                     path = label.with_name(label.name.replace("_label", "_input"))
scripts/desktop-state.py:56:                     value = read_text(path).strip()
scripts/desktop-state.py:57:                     if value.isdigit() and 1 <= int(value) <= 125000:
…
scripts/desktop-state.py:60: 
scripts/desktop-state.py:61:     def tick(self):
scripts/desktop-state.py:62:         now = time.monotonic()
scripts/desktop-state.py:63:         if (self.temperature is None or not self.temperature.exists()) and now - self.last_discovery >= 30:
…
scripts/desktop-state.py:66:         # Interrupt counters and scheduler counters are never consumed by the UI.
scripts/desktop-state.py:67:         cpu = "\n".join(line for line in read_text("/proc/stat").splitlines() if line.startswith("cpu"))
scripts/desktop-state.py:68:         sections = [
scripts/desktop-state.py:69:             ("__STAT__", cpu), ("__MEM__", read_text("/proc/meminfo")),
scripts/desktop-state.py:70:             ("__TEMP__", read_text(self.temperature) if self.temperature else ""),
scripts/desktop-state.py:71:             ("__CPU_POWER__", read_text("/run/hyprv/cpu-package-power-w")),
scripts/desktop-state.py:72:             ("__ROUTE__", read_text("/proc/net/route")), ("__NET__", read_text("/proc/net/dev")),
scripts/desktop-state.py:73:         ]
scripts/desktop-state.py:74:         self.publisher.emit("system", "\n".join(name + "\n" + value.rstrip() for name, value in sections))
…
scripts/desktop-state.py:78: class NetworkObserver:
scripts/desktop-state.py:79:     def __init__(self, publisher):
scripts/desktop-state.py:80:         self.publisher = publisher
scripts/desktop-state.py:81:         self.client = NM.Client.new(None)
…
scripts/desktop-state.py:90: 
scripts/desktop-state.py:91:     def schedule(self, *_args):
scripts/desktop-state.py:92:         if not self.pending:
scripts/desktop-state.py:93:             self.pending = GLib.timeout_add(60, self.refresh)
scripts/desktop-state.py:94: 
scripts/desktop-state.py:95:     def watch(self, obj, signal):
scripts/desktop-state.py:96:         self.connections.append((obj, obj.connect(signal, self.schedule)))
scripts/desktop-state.py:97: 
scripts/desktop-state.py:98:     def refresh(self):
scripts/desktop-state.py:99:         self.pending = 0
scripts/desktop-state.py:100:         for obj, handler in self.connections:
…
scripts/desktop-state.py:106:         ssid = ""
scripts/desktop-state.py:107:         signal = 0
scripts/desktop-state.py:108:         security = ""
scripts/desktop-state.py:109:         connected = False
…
scripts/desktop-state.py:112:         for connection in settings:
scripts/desktop-state.py:113:             self.watch(connection, "changed")
scripts/desktop-state.py:114:             wireless = connection.get_setting_wireless()
scripts/desktop-state.py:115:             raw = wireless.get_ssid() if wireless else None
…
scripts/desktop-state.py:118:         if device:
scripts/desktop-state.py:119:             self.watch(device, "notify")
scripts/desktop-state.py:120:             self.watch(device, "access-point-added")
scripts/desktop-state.py:121:             self.watch(device, "access-point-removed")
scripts/desktop-state.py:122:             active = device.get_active_access_point()
scripts/desktop-state.py:123:             access_points = list(device.get_access_points())
…
scripts/desktop-state.py:128:             for ap in access_points:
scripts/desktop-state.py:129:                 self.watch(ap, "notify")
scripts/desktop-state.py:130:                 raw = ap.get_ssid()
scripts/desktop-state.py:131:                 name = NM.utils_ssid_to_utf8(raw.get_data()) if raw else ""
…
scripts/desktop-state.py:138:                 is_active = ap == active
scripts/desktop-state.py:139:                 entry = {"ssid": name, "signal": ap.get_strength(), "security": sec,
scripts/desktop-state.py:140:                          "secure": secure, "enterprise": enterprise, "active": is_active,
scripts/desktop-state.py:141:                          "known": name in known_ssids}
scripts/desktop-state.py:142:                 networks.append(entry)
scripts/desktop-state.py:143:                 if is_active:
scripts/desktop-state.py:144:                     ssid, signal, security, connected = name, entry["signal"], sec, True
scripts/desktop-state.py:145:         # Same strongest/active AP per SSID rule as wifi-status.sh.
scripts/desktop-state.py:146:         best = {}
scripts/desktop-state.py:147:         for network in networks:
scripts/desktop-state.py:148:             previous = best.get(network["ssid"])
scripts/desktop-state.py:149:             if previous is None or (network["active"], network["signal"]) > (previous["active"], previous["signal"]):
scripts/desktop-state.py:150:                 best[network["ssid"]] = network
scripts/desktop-state.py:151:         networks = sorted(best.values(), key=lambda n: (not n["active"], -n["signal"], n["ssid"]))
scripts/desktop-state.py:152:         self.publisher.emit("wifi", {
scripts/desktop-state.py:153:             "present": device is not None, "iface": device.get_iface() if device else "",
scripts/desktop-state.py:154:             "enabled": self.client.wireless_get_enabled(),
scripts/desktop-state.py:155:             "hardwareEnabled": self.client.wireless_hardware_get_enabled(),
scripts/desktop-state.py:156:             "connected": connected, "ssid": ssid, "signal": signal,
scripts/desktop-state.py:157:             "security": security, "networks": networks,
scripts/desktop-state.py:158:         })
scripts/desktop-state.py:159:         return GLib.SOURCE_REMOVE
…
scripts/desktop-state.py:162: class AgentObserver:
scripts/desktop-state.py:163:     def __init__(self, publisher):
scripts/desktop-state.py:164:         self.publisher = publisher
scripts/desktop-state.py:165:         self.state = None
…
scripts/desktop-state.py:168: 
scripts/desktop-state.py:169:     def tick(self):
scripts/desktop-state.py:170:         path = state_file()
scripts/desktop-state.py:171:         try:
…
scripts/desktop-state.py:174:             signature = None
scripts/desktop-state.py:175:         scan_due = self.state is None or time.time() - float(self.state.get("last_process_scan") or 0) >= PROCESS_SCAN_INTERVAL_SECONDS
scripts/desktop-state.py:176:         if self.state is None or signature != self.signature or scan_due:
scripts/desktop-state.py:177:             with locked_state(write=False) as state:
…
scripts/desktop-state.py:191:         sessions = [{k: s[k] for k in fields if k in s} for s in snapshot["sessions"]]
scripts/desktop-state.py:192:         self.publisher.emit("agent", {"active": snapshot["active"], "count": snapshot["count"], "sessions": sessions})
scripts/desktop-state.py:193:         GLib.timeout_add(650 if snapshot["active"] else 1200, self.tick)
scripts/desktop-state.py:194:         return GLib.SOURCE_REMOVE
scripts/desktop-state.py:195: 
scripts/desktop-state.py:196: 
scripts/desktop-state.py:197: def main():
scripts/desktop-state.py:198:     publisher = Publisher()
scripts/desktop-state.py:199:     kernel = KernelSampler(publisher)
scripts/desktop-state.py:200:     kernel.tick()
scripts/desktop-state.py:201:     GLib.timeout_add(1000, kernel.tick)
scripts/desktop-state.py:202:     network = NetworkObserver(publisher)
scripts/desktop-state.py:203:     agent = AgentObserver(publisher)
scripts/desktop-state.py:204:     GLib.MainLoop().run()
scripts/desktop-state.py:205: 
scripts/desktop-state.py:206: 
scripts/desktop-state.py:207: if __name__ == "__main__":
scripts/desktop-state.py:208:     try:
scripts/desktop-state.py:209:         main()
scripts/desktop-state.py:210:     except BrokenPipeError:
scripts/desktop-state.py:211:         sys.exit(0)
```

### `scripts/egpu-disconnect.sh`

Migration owner: `egpu/helper`. Source `scripts/egpu-disconnect.sh:1`, 153 lines, SHA-256 `8b34437ced6270f3cc53dec7d246ec4d5654de53c8e8ea1dd36e35f3666ef99b`.

```text
…
scripts/egpu-disconnect.sh:8: 
scripts/egpu-disconnect.sh:9: is_thunderbolt_device() {
scripts/egpu-disconnect.sh:10:     local dev_path="$1"
scripts/egpu-disconnect.sh:11:     local real_path
scripts/egpu-disconnect.sh:12:     real_path="$(readlink -f "$dev_path")"
scripts/egpu-disconnect.sh:13: 
scripts/egpu-disconnect.sh:14:     # Walk the sysfs path upward looking for a Thunderbolt domain indicator.
scripts/egpu-disconnect.sh:15:     local current="$real_path"
scripts/egpu-disconnect.sh:16:     while [[ "$current" != "/" && "$current" != "/sys" ]]; do
scripts/egpu-disconnect.sh:17:         if [[ -d "$current/domain0" ]] || [[ "$(basename "$(dirname "$current")")" == "thunderbolt" ]]; then
scripts/egpu-disconnect.sh:18:             return 0
scripts/egpu-disconnect.sh:19:         fi
scripts/egpu-disconnect.sh:20: 
scripts/egpu-disconnect.sh:21:         if [[ -f "$current/label" ]]; then
scripts/egpu-disconnect.sh:22:             local label
scripts/egpu-disconnect.sh:23:             label="$(cat "$current/label" 2>/dev/null || true)"
scripts/egpu-disconnect.sh:24:             if [[ "$label" == *[Tt]hunderbolt* ]]; then
scripts/egpu-disconnect.sh:25:                 return 0
scripts/egpu-disconnect.sh:26:             fi
…
scripts/egpu-disconnect.sh:28: 
scripts/egpu-disconnect.sh:29:         current="$(dirname "$current")"
scripts/egpu-disconnect.sh:30:     done
scripts/egpu-disconnect.sh:31: 
…
scripts/egpu-disconnect.sh:33:     # platform-integrated graphics devices.
scripts/egpu-disconnect.sh:34:     if [[ "$real_path" == */pci*/*:*:*/* ]]; then
scripts/egpu-disconnect.sh:35:         local bus_id
scripts/egpu-disconnect.sh:36:         local bus_num
…
scripts/egpu-disconnect.sh:38: 
scripts/egpu-disconnect.sh:39:         bus_id="$(basename "$dev_path")"
scripts/egpu-disconnect.sh:40:         bus_num="${bus_id#*:}"
scripts/egpu-disconnect.sh:41:         bus_num="${bus_num%%:*}"
scripts/egpu-disconnect.sh:42:         bus_dec=$((16#$bus_num))
scripts/egpu-disconnect.sh:43:         if (( bus_dec >= 3 )); then
scripts/egpu-disconnect.sh:44:             return 0
…
scripts/egpu-disconnect.sh:50: 
scripts/egpu-disconnect.sh:51: is_pci_bdf() {
scripts/egpu-disconnect.sh:52:     [[ "$1" =~ ^[[:xdigit:]]{4}:[[:xdigit:]]{2}:[[:xdigit:]]{2}\.[[:xdigit:]]$ ]]
scripts/egpu-disconnect.sh:53: }
scripts/egpu-disconnect.sh:54: 
scripts/egpu-disconnect.sh:55: remove_pci_device() {
scripts/egpu-disconnect.sh:56:     local bdf="$1"
scripts/egpu-disconnect.sh:57:     local dev_path="/sys/bus/pci/devices/$bdf"
scripts/egpu-disconnect.sh:58:     local remove_file="$dev_path/remove"
scripts/egpu-disconnect.sh:59: 
scripts/egpu-disconnect.sh:60:     [[ -e "$dev_path" ]] || return 0
scripts/egpu-disconnect.sh:61:     [[ -f "$remove_file" ]] || return 0
scripts/egpu-disconnect.sh:62: 
scripts/egpu-disconnect.sh:63:     printf 'device=%s\n' "$bdf"
scripts/egpu-disconnect.sh:64:     if pkexec sh -c "echo 1 > '$remove_file'" 2>/dev/null; then
scripts/egpu-disconnect.sh:65:         printf 'status=removed\n'
scripts/egpu-disconnect.sh:66:         return 0
scripts/egpu-disconnect.sh:67:     fi
scripts/egpu-disconnect.sh:68: 
scripts/egpu-disconnect.sh:69:     printf 'status=error\n'
scripts/egpu-disconnect.sh:70:     return 1
scripts/egpu-disconnect.sh:71: }
scripts/egpu-disconnect.sh:72: 
scripts/egpu-disconnect.sh:73: collect_slot_functions() {
scripts/egpu-disconnect.sh:74:     local dev_path="$1"
scripts/egpu-disconnect.sh:75:     local bdf
…
scripts/egpu-disconnect.sh:78: 
scripts/egpu-disconnect.sh:79:     bdf="$(basename "$dev_path")"
scripts/egpu-disconnect.sh:80:     slot_prefix="${bdf%.*}"
scripts/egpu-disconnect.sh:81: 
scripts/egpu-disconnect.sh:82:     for candidate in /sys/bus/pci/devices/"$slot_prefix".*; do
scripts/egpu-disconnect.sh:83:         [[ -e "$candidate" ]] || continue
scripts/egpu-disconnect.sh:84:         basename "$candidate"
scripts/egpu-disconnect.sh:85:     done | sort -rV
scripts/egpu-disconnect.sh:86: }
scripts/egpu-disconnect.sh:87: 
scripts/egpu-disconnect.sh:88: collect_parent_bridges() {
scripts/egpu-disconnect.sh:89:     local dev_path="$1"
scripts/egpu-disconnect.sh:90:     local current
…
scripts/egpu-disconnect.sh:95: 
scripts/egpu-disconnect.sh:96:     current="$(dirname "$(readlink -f "$dev_path")")"
scripts/egpu-disconnect.sh:97:     while [[ "$current" != "/sys" && "$current" != "/sys/devices" ]]; do
scripts/egpu-disconnect.sh:98:         current_name="$(basename "$current")"
scripts/egpu-disconnect.sh:99:         parent_dir="$(dirname "$current")"
scripts/egpu-disconnect.sh:100: 
scripts/egpu-disconnect.sh:101:         if is_pci_bdf "$current_name"; then
scripts/egpu-disconnect.sh:102:             class_file="$current/class"
scripts/egpu-disconnect.sh:103:             class_value="$(cat "$class_file" 2>/dev/null || true)"
scripts/egpu-disconnect.sh:104: 
scripts/egpu-disconnect.sh:105:             # Skip the platform root port directly attached to the host bus.
scripts/egpu-disconnect.sh:106:             if [[ "$class_value" == "0x060400" || "$class_value" == "0x060401" ]]; then
scripts/egpu-disconnect.sh:107:                 if [[ "$(basename "$parent_dir")" != pci0000:* ]]; then
scripts/egpu-disconnect.sh:108:                     printf '%s\n' "$current_name"
scripts/egpu-disconnect.sh:109:                 fi
scripts/egpu-disconnect.sh:110:             fi
…
scripts/egpu-disconnect.sh:112: 
scripts/egpu-disconnect.sh:113:         current="$parent_dir"
scripts/egpu-disconnect.sh:114:     done
scripts/egpu-disconnect.sh:115: }
scripts/egpu-disconnect.sh:116: 
scripts/egpu-disconnect.sh:117: remove_gpu_chain() {
scripts/egpu-disconnect.sh:118:     local dev_path="$1"
scripts/egpu-disconnect.sh:119:     local -A seen=()
…
scripts/egpu-disconnect.sh:122:     while IFS= read -r bdf; do
scripts/egpu-disconnect.sh:123:         [[ -n "$bdf" ]] || continue
scripts/egpu-disconnect.sh:124:         [[ -n "${seen[$bdf]:-}" ]] && continue
scripts/egpu-disconnect.sh:125:         seen["$bdf"]=1
scripts/egpu-disconnect.sh:126:         remove_pci_device "$bdf"
scripts/egpu-disconnect.sh:127:     done < <(
scripts/egpu-disconnect.sh:128:         collect_slot_functions "$dev_path"
scripts/egpu-disconnect.sh:129:         collect_parent_bridges "$dev_path"
scripts/egpu-disconnect.sh:130:     )
scripts/egpu-disconnect.sh:131: }
…
scripts/egpu-disconnect.sh:133: found=false
scripts/egpu-disconnect.sh:134: for dev in /sys/bus/pci/devices/*/; do
scripts/egpu-disconnect.sh:135:     class_file="$dev/class"
scripts/egpu-disconnect.sh:136:     [[ -f "$class_file" ]] || continue
scripts/egpu-disconnect.sh:137: 
scripts/egpu-disconnect.sh:138:     class="$(cat "$class_file" 2>/dev/null || true)"
scripts/egpu-disconnect.sh:139:     # VGA compatible controller (0x030000) or 3D controller (0x030200)
scripts/egpu-disconnect.sh:140:     if [[ "$class" == "0x030000" || "$class" == "0x030200" ]]; then
scripts/egpu-disconnect.sh:141:         if ! is_thunderbolt_device "$dev"; then
scripts/egpu-disconnect.sh:142:             continue
scripts/egpu-disconnect.sh:143:         fi
scripts/egpu-disconnect.sh:144: 
scripts/egpu-disconnect.sh:145:         remove_gpu_chain "$dev"
scripts/egpu-disconnect.sh:146:         found=true
scripts/egpu-disconnect.sh:147:         break
…
scripts/egpu-disconnect.sh:150: 
scripts/egpu-disconnect.sh:151: if [[ "$found" != "true" ]]; then
scripts/egpu-disconnect.sh:152:     printf 'status=not_found\n'
scripts/egpu-disconnect.sh:153: fi
```

### `scripts/fan-control-boot.service`

Migration owner: `fan/helper`. Source `scripts/fan-control-boot.service:1`, 19 lines, SHA-256 `c3de3cc0b9c1094a648abc905c5c36a04bcabd3a6a2e51ff54f3c72f4b1449fc`.

```text
scripts/fan-control-boot.service:1: [Unit]
scripts/fan-control-boot.service:2: Description=HyprV early chassis fan temperature control
scripts/fan-control-boot.service:3: DefaultDependencies=no
scripts/fan-control-boot.service:4: Requires=hyprv-fan-permissions.service
scripts/fan-control-boot.service:5: After=systemd-modules-load.service local-fs.target hyprv-fan-permissions.service
scripts/fan-control-boot.service:6: Before=sysinit.target display-manager.service shutdown.target
scripts/fan-control-boot.service:7: Conflicts=shutdown.target
scripts/fan-control-boot.service:8: StartLimitIntervalSec=0
scripts/fan-control-boot.service:9: 
scripts/fan-control-boot.service:10: [Service]
scripts/fan-control-boot.service:11: Type=exec
scripts/fan-control-boot.service:12: User=wilf
scripts/fan-control-boot.service:13: ExecStartPre=/usr/local/libexec/hyprv-fanctl boot
scripts/fan-control-boot.service:14: ExecStart=/usr/local/libexec/hyprv-fanctl run-system
scripts/fan-control-boot.service:15: Restart=always
scripts/fan-control-boot.service:16: RestartSec=1
scripts/fan-control-boot.service:17: 
scripts/fan-control-boot.service:18: [Install]
scripts/fan-control-boot.service:19: WantedBy=sysinit.target
```

### `scripts/fan-control-helper.sh`

Migration owner: `fan/helper`. Source `scripts/fan-control-helper.sh:1`, 256 lines, SHA-256 `269948e54d9e3bc79ee8ca7758853d45133c0645ebd6c2e3d1a8c4c112a33184`.

```text
…
scripts/fan-control-helper.sh:4: 
scripts/fan-control-helper.sh:5: readonly state_dir="/home/wilf/.local/state/hyprv"
scripts/fan-control-helper.sh:6: readonly state_file="${state_dir}/fan-percent"
scripts/fan-control-helper.sh:7: 
scripts/fan-control-helper.sh:8: find_hwmon() {
scripts/fan-control-helper.sh:9:     local candidate
scripts/fan-control-helper.sh:10:     for candidate in /sys/class/hwmon/hwmon*; do
scripts/fan-control-helper.sh:11:         [[ -r "${candidate}/name" ]] || continue
scripts/fan-control-helper.sh:12:         if [[ "$(<"${candidate}/name")" == "nct6799" ]]; then
scripts/fan-control-helper.sh:13:             printf '%s\n' "$candidate"
scripts/fan-control-helper.sh:14:             return 0
scripts/fan-control-helper.sh:15:         fi
…
scripts/fan-control-helper.sh:19: 
scripts/fan-control-helper.sh:20: channel_index() {
scripts/fan-control-helper.sh:21:     case "$1" in
scripts/fan-control-helper.sh:22:         chassis) printf '1\n' ;;
scripts/fan-control-helper.sh:23:         pump) printf '7\n' ;;
scripts/fan-control-helper.sh:24:         *) return 1 ;;
scripts/fan-control-helper.sh:25:     esac
…
scripts/fan-control-helper.sh:27: 
scripts/fan-control-helper.sh:28: read_number() {
scripts/fan-control-helper.sh:29:     local path="$1"
scripts/fan-control-helper.sh:30:     if [[ -r "$path" ]]; then
scripts/fan-control-helper.sh:31:         local value
scripts/fan-control-helper.sh:32:         value="$(<"$path")"
scripts/fan-control-helper.sh:33:         [[ "$value" =~ ^[0-9]+$ ]] && printf '%s\n' "$value" && return 0
scripts/fan-control-helper.sh:34:     fi
scripts/fan-control-helper.sh:35:     printf '0\n'
scripts/fan-control-helper.sh:36: }
scripts/fan-control-helper.sh:37: 
scripts/fan-control-helper.sh:38: # The stored chassis value is a limit, never the instantaneous PWM.
scripts/fan-control-helper.sh:39: load_state() {
scripts/fan-control-helper.sh:40:     local hwmon="$1" key value
scripts/fan-control-helper.sh:41:     chassis="$((($(read_number "${hwmon}/pwm1") * 100 + 127) / 255))"
scripts/fan-control-helper.sh:42:     pump="$((($(read_number "${hwmon}/pwm7") * 100 + 127) / 255))"
scripts/fan-control-helper.sh:43:     [[ -r "$state_file" ]] || return 0
scripts/fan-control-helper.sh:44:     while IFS='=' read -r key value; do
scripts/fan-control-helper.sh:45:         [[ "$value" =~ ^[0-9]{1,3}$ ]] || continue
scripts/fan-control-helper.sh:46:         value=$((10#$value))
scripts/fan-control-helper.sh:47:         ((value <= 100)) || continue
scripts/fan-control-helper.sh:48:         case "$key" in
scripts/fan-control-helper.sh:49:             chassis) chassis="$value" ;;
scripts/fan-control-helper.sh:50:             pump) pump="$value" ;;
scripts/fan-control-helper.sh:51:         esac
scripts/fan-control-helper.sh:52:     done <"$state_file"
scripts/fan-control-helper.sh:53: }
scripts/fan-control-helper.sh:54: 
scripts/fan-control-helper.sh:55: save_state() {
scripts/fan-control-helper.sh:56:     local temporary="${state_file}.$$"
scripts/fan-control-helper.sh:57:     printf 'chassis=%s\npump=%s\n' "$chassis" "$pump" >"$temporary"
scripts/fan-control-helper.sh:58:     mv -f "$temporary" "$state_file"
scripts/fan-control-helper.sh:59: }
scripts/fan-control-helper.sh:60: 
scripts/fan-control-helper.sh:61: read_cpu_temperature() {
scripts/fan-control-helper.sh:62:     local hwmon input label value maximum=0
scripts/fan-control-helper.sh:63:     for hwmon in /sys/class/hwmon/hwmon*; do
scripts/fan-control-helper.sh:64:         [[ -r "${hwmon}/name" ]] || continue
scripts/fan-control-helper.sh:65:         case "$(<"${hwmon}/name")" in
scripts/fan-control-helper.sh:66:             coretemp|k10temp|zenpower) ;;
scripts/fan-control-helper.sh:67:             *) continue ;;
…
scripts/fan-control-helper.sh:70:             label="${input%_input}_label"
scripts/fan-control-helper.sh:71:             [[ -r "$label" && -r "$input" ]] || continue
scripts/fan-control-helper.sh:72:             case "$(<"$label")" in
scripts/fan-control-helper.sh:73:                 "Package id "*|Tctl|Tdie|"CPU Package"|CPU) ;;
scripts/fan-control-helper.sh:74:                 *) continue ;;
scripts/fan-control-helper.sh:75:             esac
scripts/fan-control-helper.sh:76:             value="$(<"$input")"
scripts/fan-control-helper.sh:77:             [[ "$value" =~ ^[0-9]+$ ]] || continue
scripts/fan-control-helper.sh:78:             ((value > 0 && value <= 125000)) || continue
scripts/fan-control-helper.sh:79:             ((value <= maximum)) || maximum="$value"
scripts/fan-control-helper.sh:80:         done
scripts/fan-control-helper.sh:81:     done
scripts/fan-control-helper.sh:82:     ((maximum > 0)) || return 1
scripts/fan-control-helper.sh:83:     printf '%s\n' "$maximum"
scripts/fan-control-helper.sh:84: }
scripts/fan-control-helper.sh:85: 
scripts/fan-control-helper.sh:86: chassis_target_pwm() {
scripts/fan-control-helper.sh:87:     local temperature="$2" cap=$(( $1 * 255 / 100 ))
scripts/fan-control-helper.sh:88:     if ((temperature <= 70000)); then
scripts/fan-control-helper.sh:89:         printf '0\n'
scripts/fan-control-helper.sh:90:     elif ((temperature <= 88000)); then
scripts/fan-control-helper.sh:91:         # Floor the PWM so rounding cannot exceed the requested limit.
scripts/fan-control-helper.sh:92:         printf '%s\n' "$((cap * (temperature - 70000) / 18000))"
scripts/fan-control-helper.sh:93:     else
scripts/fan-control-helper.sh:94:         printf '%s\n' "$cap"
scripts/fan-control-helper.sh:95:     fi
scripts/fan-control-helper.sh:96: }
scripts/fan-control-helper.sh:97: 
scripts/fan-control-helper.sh:98: apply_chassis() {
scripts/fan-control-helper.sh:99:     local hwmon="$1" temperature index limit pwm
scripts/fan-control-helper.sh:100:     temperature="$(read_cpu_temperature)" || temperature=""
…
scripts/fan-control-helper.sh:102:     for index in 1 7; do
scripts/fan-control-helper.sh:103:         if [[ "$index" == 1 ]]; then limit="$chassis"; else limit="$pump"; fi
scripts/fan-control-helper.sh:104:         if [[ -n "$temperature" ]]; then
scripts/fan-control-helper.sh:105:             pwm="$(chassis_target_pwm "$limit" "$temperature")"
scripts/fan-control-helper.sh:106:         else
scripts/fan-control-helper.sh:107:             # Missing telemetry uses the current limit without bypassing it.
…
scripts/fan-control-helper.sh:110:         if [[ "$(read_number "${hwmon}/pwm${index}_enable")" != 1 ]]; then
scripts/fan-control-helper.sh:111:             printf '1' >"${hwmon}/pwm${index}_enable"
scripts/fan-control-helper.sh:112:         fi
scripts/fan-control-helper.sh:113:         if [[ "$(read_number "${hwmon}/pwm${index}")" != "$pwm" ]]; then
scripts/fan-control-helper.sh:114:             printf '%s' "$pwm" >"${hwmon}/pwm${index}"
scripts/fan-control-helper.sh:115:         fi
scripts/fan-control-helper.sh:116:     done
…
scripts/fan-control-helper.sh:118: 
scripts/fan-control-helper.sh:119: print_status() {
scripts/fan-control-helper.sh:120:     local hwmon chassis pump
scripts/fan-control-helper.sh:121:     if ! hwmon="$(find_hwmon)"; then
scripts/fan-control-helper.sh:122:         printf 'fan_available=false\n'
scripts/fan-control-helper.sh:123:         return 0
scripts/fan-control-helper.sh:124:     fi
scripts/fan-control-helper.sh:125:     load_state "$hwmon"
scripts/fan-control-helper.sh:126:     printf 'fan_available=true\n'
scripts/fan-control-helper.sh:127:     printf 'fan_chassis_percent=%s\n' "$chassis"
scripts/fan-control-helper.sh:128:     printf 'fan_chassis_rpm=%s\n' "$(read_number "${hwmon}/fan1_input")"
scripts/fan-control-helper.sh:129:     printf 'fan_pump_percent=%s\n' "$pump"
scripts/fan-control-helper.sh:130:     printf 'fan_pump_rpm=%s\n' "$(read_number "${hwmon}/fan7_input")"
scripts/fan-control-helper.sh:131: }
scripts/fan-control-helper.sh:132: 
…
scripts/fan-control-helper.sh:134: # The greeter, text logins and a lingering user manager keep the boot limits.
scripts/fan-control-helper.sh:135: desktop_session_available() {
scripts/fan-control-helper.sh:136:     local sessions session type
scripts/fan-control-helper.sh:137:     sessions="$(timeout 1 loginctl show-user wilf -p Sessions --value 2>/dev/null)" || return 1
scripts/fan-control-helper.sh:138:     for session in $sessions; do
scripts/fan-control-helper.sh:139:         type="$(timeout 1 loginctl show-session "$session" -p Type --value 2>/dev/null)" || continue
scripts/fan-control-helper.sh:140:         [[ "$type" == wayland || "$type" == x11 ]] && return 0
scripts/fan-control-helper.sh:141:     done
scripts/fan-control-helper.sh:142:     return 1
…
scripts/fan-control-helper.sh:145: # Serialize set/restore/tick so a stale sample cannot overwrite a new limit.
scripts/fan-control-helper.sh:146: update_control() (
scripts/fan-control-helper.sh:147:     local action="$1" channel="${2:-}" percent="${3:-}"
scripts/fan-control-helper.sh:148:     local hwmon chassis pump desktop=false
scripts/fan-control-helper.sh:149:     if [[ "$action" == system-tick ]] && desktop_session_available; then
scripts/fan-control-helper.sh:150:         desktop=true
scripts/fan-control-helper.sh:151:     fi
scripts/fan-control-helper.sh:152:     umask 077
scripts/fan-control-helper.sh:153:     mkdir -p "$state_dir"
scripts/fan-control-helper.sh:154:     exec 9>"${state_dir}/fan-percent.lock"
scripts/fan-control-helper.sh:155:     flock 9
scripts/fan-control-helper.sh:156:     hwmon="$(find_hwmon)" || return 1
scripts/fan-control-helper.sh:157:     load_state "$hwmon"
scripts/fan-control-helper.sh:158:     if [[ "$action" == set ]]; then
scripts/fan-control-helper.sh:159:         case "$channel" in
scripts/fan-control-helper.sh:160:             chassis) chassis="$percent" ;;
scripts/fan-control-helper.sh:161:             pump) pump="$percent" ;;
scripts/fan-control-helper.sh:162:         esac
scripts/fan-control-helper.sh:163:     fi
scripts/fan-control-helper.sh:164:     case "$action" in
scripts/fan-control-helper.sh:165:         set|restore|tick)
scripts/fan-control-helper.sh:166:             [[ -r "$state_file" && "$action" == tick ]] || save_state ;;
scripts/fan-control-helper.sh:167:     esac
scripts/fan-control-helper.sh:168:     if [[ "$action" == boot || ( "$action" == system-tick && "$desktop" == false ) ]]; then
scripts/fan-control-helper.sh:169:         chassis=80
scripts/fan-control-helper.sh:170:         pump=80
scripts/fan-control-helper.sh:171:     fi
scripts/fan-control-helper.sh:172:     apply_chassis "$hwmon"
scripts/fan-control-helper.sh:173: )
scripts/fan-control-helper.sh:174: 
scripts/fan-control-helper.sh:175: set_percent() {
scripts/fan-control-helper.sh:176:     [[ $# -eq 2 ]] || return 2
scripts/fan-control-helper.sh:177:     channel_index "$1" >/dev/null || return 2
scripts/fan-control-helper.sh:178:     [[ "$2" =~ ^[0-9]{1,3}$ ]] || return 2
scripts/fan-control-helper.sh:179:     local percent=$((10#$2))
scripts/fan-control-helper.sh:180:     ((percent <= 100)) || return 2
scripts/fan-control-helper.sh:181:     update_control set "$1" "$percent"
scripts/fan-control-helper.sh:182: }
scripts/fan-control-helper.sh:183: 
scripts/fan-control-helper.sh:184: restore_state() {
scripts/fan-control-helper.sh:185:     update_control restore
scripts/fan-control-helper.sh:186: }
scripts/fan-control-helper.sh:187: 
scripts/fan-control-helper.sh:188: run_control() {
scripts/fan-control-helper.sh:189:     local action="${1:-tick}"
scripts/fan-control-helper.sh:190:     umask 077
scripts/fan-control-helper.sh:191:     mkdir -p "$state_dir"
scripts/fan-control-helper.sh:192:     exec 8>"${state_dir}/fan-control.lock"
scripts/fan-control-helper.sh:193:     flock -n 8 || return 1
scripts/fan-control-helper.sh:194:     while true; do
scripts/fan-control-helper.sh:195:         update_control "$action"
scripts/fan-control-helper.sh:196:         sleep 1
scripts/fan-control-helper.sh:197:     done
scripts/fan-control-helper.sh:198: }
scripts/fan-control-helper.sh:199: 
scripts/fan-control-helper.sh:200: grant_permissions() {
scripts/fan-control-helper.sh:201:     [[ $EUID -eq 0 ]] || {
scripts/fan-control-helper.sh:202:         printf 'grant requires root privileges\n' >&2
scripts/fan-control-helper.sh:203:         return 1
scripts/fan-control-helper.sh:204:     }
…
scripts/fan-control-helper.sh:207:     hwmon="$(find_hwmon)" || {
scripts/fan-control-helper.sh:208:         printf 'nct6799 fan controller is unavailable\n' >&2
scripts/fan-control-helper.sh:209:         return 1
scripts/fan-control-helper.sh:210:     }
…
scripts/fan-control-helper.sh:212:         for path in "${hwmon}/pwm${index}_enable" "${hwmon}/pwm${index}"; do
scripts/fan-control-helper.sh:213:             [[ -e "$path" ]] || {
scripts/fan-control-helper.sh:214:                 printf 'missing fan control attribute: %s\n' "$path" >&2
scripts/fan-control-helper.sh:215:                 return 1
scripts/fan-control-helper.sh:216:             }
scripts/fan-control-helper.sh:217:             chown wilf "$path"
scripts/fan-control-helper.sh:218:             chmod 0600 "$path"
scripts/fan-control-helper.sh:219:         done
scripts/fan-control-helper.sh:220:     done
…
scripts/fan-control-helper.sh:222: 
scripts/fan-control-helper.sh:223: case "${1:-}" in
scripts/fan-control-helper.sh:224:     status)
scripts/fan-control-helper.sh:225:         [[ $# -eq 1 ]] || exit 2
scripts/fan-control-helper.sh:226:         print_status
scripts/fan-control-helper.sh:227:         ;;
scripts/fan-control-helper.sh:228:     set)
scripts/fan-control-helper.sh:229:         [[ $# -eq 3 ]] || exit 2
scripts/fan-control-helper.sh:230:         set_percent "$2" "$3"
scripts/fan-control-helper.sh:231:         ;;
scripts/fan-control-helper.sh:232:     grant)
scripts/fan-control-helper.sh:233:         [[ $# -eq 1 ]] || exit 2
scripts/fan-control-helper.sh:234:         grant_permissions
scripts/fan-control-helper.sh:235:         ;;
scripts/fan-control-helper.sh:236:     boot)
scripts/fan-control-helper.sh:237:         [[ $# -eq 1 ]] || exit 2
scripts/fan-control-helper.sh:238:         update_control boot
scripts/fan-control-helper.sh:239:         ;;
scripts/fan-control-helper.sh:240:     run-system)
scripts/fan-control-helper.sh:241:         [[ $# -eq 1 ]] || exit 2
scripts/fan-control-helper.sh:242:         run_control system-tick
scripts/fan-control-helper.sh:243:         ;;
scripts/fan-control-helper.sh:244:     run)
scripts/fan-control-helper.sh:245:         [[ $# -eq 1 ]] || exit 2
scripts/fan-control-helper.sh:246:         run_control
scripts/fan-control-helper.sh:247:         ;;
scripts/fan-control-helper.sh:248:     restore)
scripts/fan-control-helper.sh:249:         [[ $# -eq 1 ]] || exit 2
scripts/fan-control-helper.sh:250:         restore_state
scripts/fan-control-helper.sh:251:         ;;
scripts/fan-control-helper.sh:252:     *)
scripts/fan-control-helper.sh:253:         printf 'usage: %s status | set {chassis|pump} {0..100} | grant | restore | run | boot | run-system\n' "${0##*/}" >&2
scripts/fan-control-helper.sh:254:         exit 2
scripts/fan-control-helper.sh:255:         ;;
```

### `scripts/fan-control-permissions-early.conf`

Migration owner: `fan/helper`. Source `scripts/fan-control-permissions-early.conf:1`, 5 lines, SHA-256 `ebb33bf2483b84ea68c555f4c60ece664d7a5639a12cef1f6b14ddb4ed194144`.

```text
scripts/fan-control-permissions-early.conf:1: [Unit]
scripts/fan-control-permissions-early.conf:2: DefaultDependencies=no
scripts/fan-control-permissions-early.conf:3: After=systemd-modules-load.service local-fs.target
scripts/fan-control-permissions-early.conf:4: Before=sysinit.target shutdown.target
scripts/fan-control-permissions-early.conf:5: Conflicts=shutdown.target
```

### `scripts/fan-control-permissions.service`

Migration owner: `fan/helper`. Source `scripts/fan-control-permissions.service:1`, 11 lines, SHA-256 `517701df79ad83680be6adf689542d18aa8a34c6baa58853441222c1f9591bde`.

```text
scripts/fan-control-permissions.service:1: [Unit]
scripts/fan-control-permissions.service:2: Description=Grant HyprV access to chassis fan controls
scripts/fan-control-permissions.service:3: After=systemd-modules-load.service
scripts/fan-control-permissions.service:4: 
scripts/fan-control-permissions.service:5: [Service]
scripts/fan-control-permissions.service:6: Type=oneshot
scripts/fan-control-permissions.service:7: ExecStart=/usr/local/libexec/hyprv-fanctl grant
scripts/fan-control-permissions.service:8: RemainAfterExit=yes
scripts/fan-control-permissions.service:9: 
scripts/fan-control-permissions.service:10: [Install]
scripts/fan-control-permissions.service:11: WantedBy=multi-user.target
```

### `scripts/fan-control.modules.conf`

Migration owner: `fan/helper`. Source `scripts/fan-control.modules.conf:1`, 1 lines, SHA-256 `be62ed7cff83326735e1a7a7f4452828f1447065c727227deaa242cb64e95f8c`.

```text
scripts/fan-control.modules.conf:1: nct6775
```

### `scripts/fan-control.service`

Migration owner: `fan/helper`. Source `scripts/fan-control.service:1`, 13 lines, SHA-256 `02f9966afe7a15fe27f993e393cc58a7d1704414db2b7f821d97e986c87e3cab`.

```text
scripts/fan-control.service:1: [Unit]
scripts/fan-control.service:2: Description=HyprV temperature-based chassis fan control
scripts/fan-control.service:3: After=default.target
scripts/fan-control.service:4: StartLimitIntervalSec=0
scripts/fan-control.service:5: 
scripts/fan-control.service:6: [Service]
scripts/fan-control.service:7: Type=simple
scripts/fan-control.service:8: ExecStart=%h/.config/HyprV/quickshell/scripts/fan-control-helper.sh run
scripts/fan-control.service:9: Restart=always
scripts/fan-control.service:10: RestartSec=2
scripts/fan-control.service:11: 
scripts/fan-control.service:12: [Install]
scripts/fan-control.service:13: WantedBy=default.target
```

### `scripts/hdmi-switch-ir.py`

Migration owner: `ir`. Source `scripts/hdmi-switch-ir.py:1`, 237 lines, SHA-256 `52448399cc32f73baae701d4dcce7d654054e00e3136f8529ace2951aeee6dda`.

```text
…
scripts/hdmi-switch-ir.py:18: 
scripts/hdmi-switch-ir.py:19: DEFAULT_DATA = Path.home() / ".local/share/hyprv-ir/hdmi-switch.json"
scripts/hdmi-switch-ir.py:20: SERIAL_PATTERNS = (
scripts/hdmi-switch-ir.py:21:     "/dev/serial/by-id/*MicroPython*",
scripts/hdmi-switch-ir.py:22:     "/dev/serial/by-id/*Raspberry_Pi_Pico*",
scripts/hdmi-switch-ir.py:23: )
scripts/hdmi-switch-ir.py:24: 
…
scripts/hdmi-switch-ir.py:26: class Device:
scripts/hdmi-switch-ir.py:27:     def __init__(self, path: str):
scripts/hdmi-switch-ir.py:28:         self.path = path
scripts/hdmi-switch-ir.py:29:         self.fd = os.open(path, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
scripts/hdmi-switch-ir.py:30:         attrs = termios.tcgetattr(self.fd)
scripts/hdmi-switch-ir.py:31:         tty.setraw(self.fd)
…
scripts/hdmi-switch-ir.py:37: 
scripts/hdmi-switch-ir.py:38:     def close(self) -> None:
scripts/hdmi-switch-ir.py:39:         os.close(self.fd)
scripts/hdmi-switch-ir.py:40: 
scripts/hdmi-switch-ir.py:41:     def drain(self) -> None:
scripts/hdmi-switch-ir.py:42:         while select.select([self.fd], [], [], 0)[0]:
scripts/hdmi-switch-ir.py:43:             os.read(self.fd, 4096)
scripts/hdmi-switch-ir.py:44: 
scripts/hdmi-switch-ir.py:45:     def command(self, command: str, final_events: set[str], timeout: float):
scripts/hdmi-switch-ir.py:46:         self.drain()
scripts/hdmi-switch-ir.py:47:         os.write(self.fd, command.encode() + b"\n")
scripts/hdmi-switch-ir.py:48:         deadline = time.monotonic() + timeout
scripts/hdmi-switch-ir.py:49:         while time.monotonic() < deadline:
scripts/hdmi-switch-ir.py:50:             ready, _, _ = select.select([self.fd], [], [], min(0.25, deadline - time.monotonic()))
…
scripts/hdmi-switch-ir.py:52:                 continue
scripts/hdmi-switch-ir.py:53:             chunk = os.read(self.fd, 4096)
scripts/hdmi-switch-ir.py:54:             if not chunk:
scripts/hdmi-switch-ir.py:55:                 continue
…
scripts/hdmi-switch-ir.py:62:                 try:
scripts/hdmi-switch-ir.py:63:                     message = json.loads(raw)
scripts/hdmi-switch-ir.py:64:                 except json.JSONDecodeError:
scripts/hdmi-switch-ir.py:65:                     continue
scripts/hdmi-switch-ir.py:66:                 event = message.get("event")
scripts/hdmi-switch-ir.py:67:                 if event == "error":
scripts/hdmi-switch-ir.py:68:                     raise RuntimeError(message.get("message", "device error"))
scripts/hdmi-switch-ir.py:69:                 if event in final_events:
scripts/hdmi-switch-ir.py:70:                     return message
scripts/hdmi-switch-ir.py:71:         raise TimeoutError(f"Pico did not answer within {timeout:.0f}s")
scripts/hdmi-switch-ir.py:72: 
scripts/hdmi-switch-ir.py:73: 
scripts/hdmi-switch-ir.py:74: def find_device(explicit: str | None) -> str:
scripts/hdmi-switch-ir.py:75:     if explicit:
scripts/hdmi-switch-ir.py:76:         return explicit
…
scripts/hdmi-switch-ir.py:78:     for pattern in SERIAL_PATTERNS:
scripts/hdmi-switch-ir.py:79:         matches.extend(glob.glob(pattern))
scripts/hdmi-switch-ir.py:80:     matches = sorted(set(matches))
scripts/hdmi-switch-ir.py:81:     if len(matches) == 1:
…
scripts/hdmi-switch-ir.py:87: 
scripts/hdmi-switch-ir.py:88: def near(actual: int, expected: int, tolerance: float = 0.35) -> bool:
scripts/hdmi-switch-ir.py:89:     return expected * (1 - tolerance) <= actual <= expected * (1 + tolerance)
scripts/hdmi-switch-ir.py:90: 
scripts/hdmi-switch-ir.py:91: 
scripts/hdmi-switch-ir.py:92: def single_frame(durations: list[int]) -> list[int]:
scripts/hdmi-switch-ir.py:93:     """Drop an inter-frame gap and any repeat frame that follows it."""
scripts/hdmi-switch-ir.py:94:     for index in range(1, len(durations), 2):
…
scripts/hdmi-switch-ir.py:99: 
scripts/hdmi-switch-ir.py:100: def decode_nec(durations: list[int]):
scripts/hdmi-switch-ir.py:101:     if len(durations) < 66 or not near(durations[0], 9000) or not near(durations[1], 4500):
scripts/hdmi-switch-ir.py:102:         return None
…
scripts/hdmi-switch-ir.py:119:     return {
scripts/hdmi-switch-ir.py:120:         "protocol": "NEC",
scripts/hdmi-switch-ir.py:121:         "bytes_hex": [f"0x{value:02X}" for value in octets],
scripts/hdmi-switch-ir.py:122:         "address": address,
scripts/hdmi-switch-ir.py:123:         "command": command,
scripts/hdmi-switch-ir.py:124:         "address_inverse_valid": address ^ address_inverse == 0xFF,
scripts/hdmi-switch-ir.py:125:         "command_inverse_valid": command ^ command_inverse == 0xFF,
scripts/hdmi-switch-ir.py:126:     }
scripts/hdmi-switch-ir.py:127: 
scripts/hdmi-switch-ir.py:128: 
scripts/hdmi-switch-ir.py:129: def load_database(path: Path):
scripts/hdmi-switch-ir.py:130:     if not path.exists():
scripts/hdmi-switch-ir.py:131:         return {"schema": 1, "carrier_hz": 38000, "duty_cycle": 0.33, "buttons": {}}
scripts/hdmi-switch-ir.py:132:     return json.loads(path.read_text())
scripts/hdmi-switch-ir.py:133: 
scripts/hdmi-switch-ir.py:134: 
scripts/hdmi-switch-ir.py:135: def save_database(path: Path, database) -> None:
scripts/hdmi-switch-ir.py:136:     path.parent.mkdir(parents=True, exist_ok=True)
scripts/hdmi-switch-ir.py:137:     temporary = path.with_suffix(path.suffix + ".tmp")
scripts/hdmi-switch-ir.py:138:     temporary.write_text(json.dumps(database, ensure_ascii=False, indent=2) + "\n")
scripts/hdmi-switch-ir.py:139:     temporary.replace(path)
scripts/hdmi-switch-ir.py:140: 
scripts/hdmi-switch-ir.py:141: 
scripts/hdmi-switch-ir.py:142: def print_json(value) -> None:
scripts/hdmi-switch-ir.py:143:     print(json.dumps(value, ensure_ascii=False, indent=2))
scripts/hdmi-switch-ir.py:144: 
scripts/hdmi-switch-ir.py:145: 
scripts/hdmi-switch-ir.py:146: def main() -> int:
scripts/hdmi-switch-ir.py:147:     parser = argparse.ArgumentParser(description=__doc__)
scripts/hdmi-switch-ir.py:148:     parser.add_argument("--device", help="Pico serial path (auto-detected by default)")
scripts/hdmi-switch-ir.py:149:     parser.add_argument("--data", type=Path, default=DEFAULT_DATA)
scripts/hdmi-switch-ir.py:150:     subparsers = parser.add_subparsers(dest="command", required=True)
scripts/hdmi-switch-ir.py:151:     subparsers.add_parser("status")
scripts/hdmi-switch-ir.py:152:     scan_parser = subparsers.add_parser("scan", help="find which GPIO has the receiver output")
scripts/hdmi-switch-ir.py:153:     scan_parser.add_argument("--seconds", type=float, default=5)
scripts/hdmi-switch-ir.py:154:     pin_parser = subparsers.add_parser("set-pin")
scripts/hdmi-switch-ir.py:155:     pin_parser.add_argument("pin", type=int)
scripts/hdmi-switch-ir.py:156:     learn_parser = subparsers.add_parser("learn", help="capture and name one remote button")
scripts/hdmi-switch-ir.py:157:     learn_parser.add_argument("name")
scripts/hdmi-switch-ir.py:158:     learn_parser.add_argument("--force", action="store_true")
scripts/hdmi-switch-ir.py:159:     subparsers.add_parser("list")
scripts/hdmi-switch-ir.py:160:     show_parser = subparsers.add_parser("show")
scripts/hdmi-switch-ir.py:161:     show_parser.add_argument("name")
scripts/hdmi-switch-ir.py:162:     send_parser = subparsers.add_parser("send", help="transmit one learned button once")
scripts/hdmi-switch-ir.py:163:     send_parser.add_argument("name")
scripts/hdmi-switch-ir.py:164:     args = parser.parse_args()
scripts/hdmi-switch-ir.py:165: 
…
scripts/hdmi-switch-ir.py:167:         database = load_database(args.data)
scripts/hdmi-switch-ir.py:168:         if args.command == "list":
scripts/hdmi-switch-ir.py:169:             for name in sorted(database["buttons"]):
scripts/hdmi-switch-ir.py:170:                 decoded = database["buttons"][name].get("decoded")
scripts/hdmi-switch-ir.py:171:                 suffix = f"  {decoded['protocol']} command={decoded['command']}" if decoded else "  raw"
scripts/hdmi-switch-ir.py:172:                 print(name + suffix)
scripts/hdmi-switch-ir.py:173:             return 0
scripts/hdmi-switch-ir.py:174:         try:
…
scripts/hdmi-switch-ir.py:181:     try:
scripts/hdmi-switch-ir.py:182:         if args.command == "status":
scripts/hdmi-switch-ir.py:183:             print_json(device.command("status", {"status"}, 2))
scripts/hdmi-switch-ir.py:184:         elif args.command == "scan":
scripts/hdmi-switch-ir.py:185:             milliseconds = max(1000, int(args.seconds * 1000))
scripts/hdmi-switch-ir.py:186:             print(f"在接收头前持续按遥控器按键 {milliseconds / 1000:g} 秒…", file=sys.stderr)
scripts/hdmi-switch-ir.py:187:             result = device.command(f"scan {milliseconds}", {"scan"}, args.seconds + 3)
scripts/hdmi-switch-ir.py:188:             print_json(result)
scripts/hdmi-switch-ir.py:189:         elif args.command == "set-pin":
scripts/hdmi-switch-ir.py:190:             print_json(device.command(f"pin {args.pin}", {"pin"}, 2))
scripts/hdmi-switch-ir.py:191:         elif args.command == "learn":
scripts/hdmi-switch-ir.py:192:             database = load_database(args.data)
scripts/hdmi-switch-ir.py:193:             if args.name in database["buttons"] and not args.force:
scripts/hdmi-switch-ir.py:194:                 raise RuntimeError(f"{args.name!r} already exists; use --force to replace it")
scripts/hdmi-switch-ir.py:195:             print(f"请对准接收头，按一次遥控器的 {args.name!r} 键…", file=sys.stderr)
scripts/hdmi-switch-ir.py:196:             capture = device.command("capture 10000", {"capture", "capture_timeout"}, 13)
scripts/hdmi-switch-ir.py:197:             if capture["event"] == "capture_timeout":
scripts/hdmi-switch-ir.py:198:                 raise RuntimeError("10 秒内没有收到红外信号")
scripts/hdmi-switch-ir.py:199:             durations = single_frame(capture["durations_us"])
scripts/hdmi-switch-ir.py:200:             entry = {
scripts/hdmi-switch-ir.py:201:                 "captured_at": dt.datetime.now(dt.timezone.utc).astimezone().isoformat(),
scripts/hdmi-switch-ir.py:202:                 "source_pin": capture["pin"],
scripts/hdmi-switch-ir.py:203:                 "polarity": capture["polarity"],
scripts/hdmi-switch-ir.py:204:                 "durations_us": durations,
scripts/hdmi-switch-ir.py:205:                 "decoded": decode_nec(durations),
scripts/hdmi-switch-ir.py:206:             }
scripts/hdmi-switch-ir.py:207:             database["buttons"][args.name] = entry
…
scripts/hdmi-switch-ir.py:209:             print_json(entry)
scripts/hdmi-switch-ir.py:210:             print(f"已保存到 {args.data}", file=sys.stderr)
scripts/hdmi-switch-ir.py:211:         elif args.command == "send":
scripts/hdmi-switch-ir.py:212:             database = load_database(args.data)
scripts/hdmi-switch-ir.py:213:             try:
…
scripts/hdmi-switch-ir.py:216:                 raise RuntimeError(f"button not found: {args.name}")
scripts/hdmi-switch-ir.py:217:             carrier_hz = int(database.get("carrier_hz", 38000))
scripts/hdmi-switch-ir.py:218:             duty_x10000 = round(float(database.get("duty_cycle", 0.33)) * 10000)
scripts/hdmi-switch-ir.py:219:             durations = ",".join(str(value) for value in entry["durations_us"])
scripts/hdmi-switch-ir.py:220:             result = device.command(
…
scripts/hdmi-switch-ir.py:231: 
scripts/hdmi-switch-ir.py:232: if __name__ == "__main__":
scripts/hdmi-switch-ir.py:233:     try:
scripts/hdmi-switch-ir.py:234:         raise SystemExit(main())
scripts/hdmi-switch-ir.py:235:     except (OSError, RuntimeError, TimeoutError, ValueError) as error:
scripts/hdmi-switch-ir.py:236:         print(f"irctl: {error}", file=sys.stderr)
scripts/hdmi-switch-ir.py:237:         raise SystemExit(1)
```

### `scripts/install-fan-control-boot.sh`

Migration owner: `fan/helper`. Source `scripts/install-fan-control-boot.sh:1`, 20 lines, SHA-256 `9e72b2391f8fe67152482f730fb2a82b84bee817480dbd458c5ce2bdceba3e6a`.

```text
scripts/install-fan-control-boot.sh:1: #!/usr/bin/env bash
scripts/install-fan-control-boot.sh:2: set -euo pipefail
scripts/install-fan-control-boot.sh:3: [[ $EUID -eq 0 ]] || { echo 'Run this installer as root.' >&2; exit 1; }
scripts/install-fan-control-boot.sh:4: script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
scripts/install-fan-control-boot.sh:5: install -m 0755 "$script_dir/fan-control-helper.sh" /usr/local/libexec/hyprv-fanctl
scripts/install-fan-control-boot.sh:6: install -m 0644 "$script_dir/fan-control-boot.service" /etc/systemd/system/hyprv-fan-control.service
scripts/install-fan-control-boot.sh:7: install -d /etc/systemd/system/hyprv-fan-permissions.service.d
scripts/install-fan-control-boot.sh:8: install -m 0644 "$script_dir/fan-control-permissions-early.conf" /etc/systemd/system/hyprv-fan-permissions.service.d/early.conf
scripts/install-fan-control-boot.sh:9: systemctl daemon-reload
scripts/install-fan-control-boot.sh:10: systemctl enable hyprv-fan-control.service
scripts/install-fan-control-boot.sh:11: # Retire the desktop-only controller before acquiring its exclusive lock.
scripts/install-fan-control-boot.sh:12: fan_uid="$(id -u wilf)"
scripts/install-fan-control-boot.sh:13: if [[ -S "/run/user/${fan_uid}/bus" ]]; then
scripts/install-fan-control-boot.sh:14:     runuser -u wilf -- env XDG_RUNTIME_DIR="/run/user/${fan_uid}" \
scripts/install-fan-control-boot.sh:15:         DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/${fan_uid}/bus" \
scripts/install-fan-control-boot.sh:16:         systemctl --user disable --now hyprv-fan-control.service
scripts/install-fan-control-boot.sh:17: else
scripts/install-fan-control-boot.sh:18:     rm -f /home/wilf/.config/systemd/user/default.target.wants/hyprv-fan-control.service
scripts/install-fan-control-boot.sh:19: fi
scripts/install-fan-control-boot.sh:20: systemctl restart hyprv-fan-control.service
```

### `scripts/launch.sh`

Migration owner: `applications/lifecycle`. Source `scripts/launch.sh:1`, 17 lines, SHA-256 `15b751465372ab0052f80516a62e4222c5803ee42e028b54bef061691156aa91`.

```text
scripts/launch.sh:1: #!/usr/bin/env bash
scripts/launch.sh:2: 
scripts/launch.sh:3: set -euo pipefail
scripts/launch.sh:4: 
scripts/launch.sh:5: config_dir="$HOME/.config/HyprV/quickshell"
scripts/launch.sh:6: script_dir="$config_dir/scripts"
scripts/launch.sh:7: 
scripts/launch.sh:8: # These small windows share one render loop instead of per-window render threads.
scripts/launch.sh:9: # Keep an explicit session override available for profiling or other GPUs.
scripts/launch.sh:10: export QSG_RENDER_LOOP="${QSG_RENDER_LOOP:-basic}"
scripts/launch.sh:11: 
scripts/launch.sh:12: "$script_dir/ui-state.sh" ensure >/dev/null 2>&1 || true
scripts/launch.sh:13: # Keep manual theme changes intact across login; explicit theme toggles still call apply-ui-state.sh.
scripts/launch.sh:14: # "$script_dir/apply-ui-state.sh" apply >/dev/null 2>&1 || true
scripts/launch.sh:15: "$script_dir/fan-control-helper.sh" restore >/dev/null 2>&1 || true
scripts/launch.sh:16: 
scripts/launch.sh:17: exec quickshell -p "$config_dir" -n -d
```

### `scripts/media-status.sh`

Migration owner: `mpris`. Source `scripts/media-status.sh:1`, 128 lines, SHA-256 `70bca635af0a031d8e1c86f82e970145c6ed0821a9c6f3dc55dd14c0f19a9808`.

```text
…
scripts/media-status.sh:4: 
scripts/media-status.sh:5: if ! command -v playerctl >/dev/null 2>&1; then
scripts/media-status.sh:6:     printf 'available=false\nplaying=false\ntitle=\nartist=\nplayer=\nart_url=\nposition=0\nlength=0\n'
scripts/media-status.sh:7:     exit 1
scripts/media-status.sh:8: fi
…
scripts/media-status.sh:18: 
scripts/media-status.sh:19: if [[ -z "$selected_player" ]]; then
scripts/media-status.sh:20: while IFS= read -r player; do
scripts/media-status.sh:21:     [[ -z "$player" ]] && continue
scripts/media-status.sh:22:     if [[ -z "$fallback_player" ]]; then
scripts/media-status.sh:23:         fallback_player="$player"
scripts/media-status.sh:24:     fi
scripts/media-status.sh:25:     status="$(playerctl -p "$player" status 2>/dev/null || true)"
scripts/media-status.sh:26:     if [[ "$status" == "Playing" ]]; then
scripts/media-status.sh:27:         selected_player="$player"
scripts/media-status.sh:28:         break
scripts/media-status.sh:29:     fi
scripts/media-status.sh:30:     if [[ -z "$paused_player" && "$status" == "Paused" ]]; then
scripts/media-status.sh:31:         paused_player="$player"
scripts/media-status.sh:32:     fi
scripts/media-status.sh:33:     if [[ -z "$content_player" ]]; then
scripts/media-status.sh:34:         candidate_title="$(playerctl -p "$player" metadata xesam:title 2>/dev/null | head -n 1 || true)"
scripts/media-status.sh:35:         candidate_artist="$(playerctl -p "$player" metadata xesam:artist 2>/dev/null | paste -sd ', ' - || true)"
scripts/media-status.sh:36:         candidate_art_url="$(playerctl -p "$player" metadata mpris:artUrl 2>/dev/null | head -n 1 || true)"
scripts/media-status.sh:37:         if [[ -n "$candidate_title" || -n "$candidate_artist" || -n "$candidate_art_url" ]]; then
scripts/media-status.sh:38:             content_player="$player"
scripts/media-status.sh:39:             if [[ -z "$paused_content_player" && "$status" == "Paused" ]]; then
scripts/media-status.sh:40:                 paused_content_player="$player"
scripts/media-status.sh:41:             fi
scripts/media-status.sh:42:         fi
scripts/media-status.sh:43:     fi
scripts/media-status.sh:44: done < <(playerctl -l 2>/dev/null || true)
scripts/media-status.sh:45: fi
scripts/media-status.sh:46: 
scripts/media-status.sh:47: if [[ -z "$selected_player" ]]; then
scripts/media-status.sh:48:     selected_player="${paused_content_player:-${content_player:-${paused_player:-$fallback_player}}}"
scripts/media-status.sh:49: fi
scripts/media-status.sh:50: 
scripts/media-status.sh:51: if [[ -z "$selected_player" ]]; then
scripts/media-status.sh:52:     printf 'available=false\nplaying=false\ntitle=\nartist=\nplayer=\nart_url=\nposition=0\nlength=0\n'
scripts/media-status.sh:53:     exit 1
scripts/media-status.sh:54: fi
scripts/media-status.sh:55: 
scripts/media-status.sh:56: status="$(playerctl -p "$selected_player" status 2>/dev/null || true)"
scripts/media-status.sh:57: title="$(playerctl -p "$selected_player" metadata xesam:title 2>/dev/null | head -n 1 || true)"
scripts/media-status.sh:58: artist="$(playerctl -p "$selected_player" metadata xesam:artist 2>/dev/null | paste -sd ', ' - || true)"
scripts/media-status.sh:59: album="$(playerctl -p "$selected_player" metadata xesam:album 2>/dev/null | head -n 1 || true)"
scripts/media-status.sh:60: art_url="$(playerctl -p "$selected_player" metadata mpris:artUrl 2>/dev/null | head -n 1 || true)"
scripts/media-status.sh:61: position="$(playerctl -p "$selected_player" position 2>/dev/null | head -n 1 || true)"
scripts/media-status.sh:62: length_us="$(playerctl -p "$selected_player" metadata mpris:length 2>/dev/null | head -n 1 || true)"
scripts/media-status.sh:63: 
scripts/media-status.sh:64: if [[ ! "$position" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
scripts/media-status.sh:65:     position="0"
scripts/media-status.sh:66: fi
…
scripts/media-status.sh:68: length="0"
scripts/media-status.sh:69: if [[ "$length_us" =~ ^[0-9]+$ ]]; then
scripts/media-status.sh:70:     length="$(awk -v value="$length_us" 'BEGIN { printf "%.3f", value / 1000000 }')"
scripts/media-status.sh:71: fi
scripts/media-status.sh:72: 
scripts/media-status.sh:73: player_comm=""
scripts/media-status.sh:74: if [[ "$selected_player" == chromium.instance* ]] && command -v busctl >/dev/null 2>&1; then
scripts/media-status.sh:75:     player_comm="$(busctl --user status "org.mpris.MediaPlayer2.${selected_player}" 2>/dev/null | sed -n 's/^Comm=//p' | head -n 1 || true)"
scripts/media-status.sh:76: fi
scripts/media-status.sh:77: 
scripts/media-status.sh:78: printf 'cider=%s\n' "$([[ "$player_comm" == "Cider" || "$selected_player" == *cider* || "$selected_player" == *Cider* ]] && printf true || printf false)"
scripts/media-status.sh:79: 
scripts/media-status.sh:80: if [[ "$player_comm" == "Cider" && "$length" != "0" ]]; then
scripts/media-status.sh:81:     script_dir="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
scripts/media-status.sh:82:     fallback_output="$("$script_dir/cider-metadata-fallback.py" --artist "$artist" --album "$album" --length "$length" --current-title "$title" --shell 2>/dev/null || true)"
scripts/media-status.sh:83:     fallback_title=""
scripts/media-status.sh:84:     fallback_artist=""
…
scripts/media-status.sh:89:     while IFS= read -r line; do
scripts/media-status.sh:90:         [[ "$line" == *=* ]] || continue
scripts/media-status.sh:91:         key="${line%%=*}"
scripts/media-status.sh:92:         value="${line#*=}"
scripts/media-status.sh:93:         case "$key" in
scripts/media-status.sh:94:             title) fallback_title="$value" ;;
scripts/media-status.sh:95:             artist) fallback_artist="$value" ;;
scripts/media-status.sh:96:             album) fallback_album="$value" ;;
scripts/media-status.sh:97:             art_url) fallback_art_url="$value" ;;
scripts/media-status.sh:98:             source) fallback_source="$value" ;;
scripts/media-status.sh:99:         esac
scripts/media-status.sh:100:     done <<< "$fallback_output"
scripts/media-status.sh:101: 
scripts/media-status.sh:102:     if [[ "$fallback_source" == "api" ]]; then
scripts/media-status.sh:103:         [[ -n "$fallback_title" ]] && title="$fallback_title"
scripts/media-status.sh:104:         [[ -n "$fallback_artist" ]] && artist="$fallback_artist"
scripts/media-status.sh:105:         [[ -n "$fallback_album" ]] && album="$fallback_album"
scripts/media-status.sh:106:         [[ -n "$fallback_art_url" ]] && art_url="$fallback_art_url"
scripts/media-status.sh:107:     elif [[ -n "$fallback_title" && ( -z "$title" || "$title" == "Cider" || "$title" == "Chromium" ) ]]; then
scripts/media-status.sh:108:         title="$fallback_title"
scripts/media-status.sh:109:     fi
scripts/media-status.sh:110:     if [[ "$fallback_source" != "api" && -n "$fallback_artist" && -z "$artist" ]]; then
scripts/media-status.sh:111:         artist="$fallback_artist"
scripts/media-status.sh:112:     fi
scripts/media-status.sh:113:     if [[ "$fallback_source" != "api" && -n "$fallback_album" && -z "$album" ]]; then
scripts/media-status.sh:114:         album="$fallback_album"
scripts/media-status.sh:115:     fi
scripts/media-status.sh:116:     if [[ "$fallback_source" != "api" && -n "$fallback_art_url" && -z "$art_url" ]]; then
scripts/media-status.sh:117:         art_url="$fallback_art_url"
scripts/media-status.sh:118:     fi
scripts/media-status.sh:119: fi
scripts/media-status.sh:120: 
scripts/media-status.sh:121: printf 'available=true\n'
scripts/media-status.sh:122: printf 'playing=%s\n' "$([[ "$status" == "Playing" ]] && printf true || printf false)"
scripts/media-status.sh:123: printf 'title=%s\n' "${title//$'\n'/ }"
scripts/media-status.sh:124: printf 'artist=%s\n' "${artist//$'\n'/ }"
scripts/media-status.sh:125: printf 'player=%s\n' "${selected_player//$'\n'/ }"
scripts/media-status.sh:126: printf 'art_url=%s\n' "${art_url//$'\n'/ }"
scripts/media-status.sh:127: printf 'position=%s\n' "$position"
scripts/media-status.sh:128: printf 'length=%s\n' "$length"
```

### `scripts/mihomo-control.sh`

Migration owner: `mihomo/systemd`. Source `scripts/mihomo-control.sh:1`, 102 lines, SHA-256 `f4537662eab4037e86a76bfabeeab2f3d5c35060bb61dc40dd9ae34cf90ae19f`.

```text
…
scripts/mihomo-control.sh:4: 
scripts/mihomo-control.sh:5: config_file="${HOME}/.config/mihomo/config.yaml"
scripts/mihomo-control.sh:6: 
scripts/mihomo-control.sh:7: yaml_value() {
scripts/mihomo-control.sh:8:     local key="$1"
scripts/mihomo-control.sh:9:     [[ -r "$config_file" ]] || return 0
scripts/mihomo-control.sh:10:     sed -n -E "s/^[[:space:]]*${key}:[[:space:]]*['\"]?([^'\"#]+)['\"]?[[:space:]]*(#.*)?$/\1/p" "$config_file" \
scripts/mihomo-control.sh:11:         | head -n 1 \
scripts/mihomo-control.sh:12:         | sed -E 's/[[:space:]]+$//'
…
scripts/mihomo-control.sh:17: controller="${controller:-127.0.0.1:9090}"
scripts/mihomo-control.sh:18: case "$controller" in
scripts/mihomo-control.sh:19:     http://*|https://*) api_base="${controller%/}" ;;
scripts/mihomo-control.sh:20:     :*) api_base="http://127.0.0.1${controller}" ;;
scripts/mihomo-control.sh:21:     0.0.0.0:*) api_base="http://127.0.0.1:${controller##*:}" ;;
scripts/mihomo-control.sh:22:     \[*\]:*) api_base="http://127.0.0.1:${controller##*:}" ;;
scripts/mihomo-control.sh:23:     *) api_base="http://${controller%/}" ;;
scripts/mihomo-control.sh:24: esac
scripts/mihomo-control.sh:25: 
scripts/mihomo-control.sh:26: curl_args=(--silent --show-error --max-time 2)
scripts/mihomo-control.sh:27: [[ -n "$secret" ]] && curl_args+=(-H "Authorization: Bearer ${secret}")
scripts/mihomo-control.sh:28: 
scripts/mihomo-control.sh:29: api_get() {
scripts/mihomo-control.sh:30:     curl "${curl_args[@]}" "$api_base$1"
scripts/mihomo-control.sh:31: }
scripts/mihomo-control.sh:32: 
scripts/mihomo-control.sh:33: api_patch() {
scripts/mihomo-control.sh:34:     curl "${curl_args[@]}" -X PATCH -H 'Content-Type: application/json' -d "$2" "$api_base$1"
scripts/mihomo-control.sh:35: }
scripts/mihomo-control.sh:36: 
scripts/mihomo-control.sh:37: service_scope="none"
scripts/mihomo-control.sh:38: if systemctl --user cat mihomo.service >/dev/null 2>&1; then
scripts/mihomo-control.sh:39:     service_scope="user"
scripts/mihomo-control.sh:40: elif systemctl cat mihomo.service >/dev/null 2>&1; then
scripts/mihomo-control.sh:41:     service_scope="system"
scripts/mihomo-control.sh:42: elif pgrep -f 'clash-verge-service|verge-mihomo' >/dev/null 2>&1; then
scripts/mihomo-control.sh:43:     service_scope="verge"
scripts/mihomo-control.sh:44: fi
scripts/mihomo-control.sh:45: 
scripts/mihomo-control.sh:46: status_json() {
scripts/mihomo-control.sh:47:     local version_json configs_json connections_json traffic_json
scripts/mihomo-control.sh:48:     local online=false version="" mode="" connections=0 upload=0 download=0
scripts/mihomo-control.sh:49: 
scripts/mihomo-control.sh:50:     version_json="$(api_get /version 2>/dev/null || true)"
scripts/mihomo-control.sh:51:     if jq -e 'type == "object"' >/dev/null 2>&1 <<<"$version_json"; then
scripts/mihomo-control.sh:52:         online=true
scripts/mihomo-control.sh:53:         version="$(jq -r '.version // ""' <<<"$version_json")"
scripts/mihomo-control.sh:54:         configs_json="$(api_get /configs 2>/dev/null || true)"
scripts/mihomo-control.sh:55:         connections_json="$(api_get /connections 2>/dev/null || true)"
scripts/mihomo-control.sh:56:         # `/traffic` is a stream. One frame is the current rate; keeping curl
scripts/mihomo-control.sh:57:         # attached would make every panel refresh wait for its timeout.
scripts/mihomo-control.sh:58:         traffic_json="$(curl --no-buffer "${curl_args[@]}" "$api_base/traffic" 2>/dev/null | head -n 1 || true)"
scripts/mihomo-control.sh:59:         mode="$(jq -r '.mode // ""' <<<"$configs_json" 2>/dev/null || true)"
scripts/mihomo-control.sh:60:         connections="$(jq -r '(.connections // []) | length' <<<"$connections_json" 2>/dev/null || printf 0)"
scripts/mihomo-control.sh:61:         upload="$(jq -r '.up // 0' <<<"$traffic_json" 2>/dev/null || printf 0)"
scripts/mihomo-control.sh:62:         download="$(jq -r '.down // 0' <<<"$traffic_json" 2>/dev/null || printf 0)"
scripts/mihomo-control.sh:63:     fi
scripts/mihomo-control.sh:64: 
scripts/mihomo-control.sh:65:     jq -cn \
scripts/mihomo-control.sh:66:         --argjson online "$online" \
scripts/mihomo-control.sh:67:         --arg service "$service_scope" \
scripts/mihomo-control.sh:68:         --arg version "$version" \
scripts/mihomo-control.sh:69:         --arg mode "$mode" \
scripts/mihomo-control.sh:70:         --argjson connections "${connections:-0}" \
scripts/mihomo-control.sh:71:         --argjson upload "${upload:-0}" \
scripts/mihomo-control.sh:72:         --argjson download "${download:-0}" \
scripts/mihomo-control.sh:73:         --arg dashboard "${api_base}/ui" \
scripts/mihomo-control.sh:74:         '{online:$online,service:$service,version:$version,mode:$mode,connections:$connections,upload:$upload,download:$download,dashboard:$dashboard}'
scripts/mihomo-control.sh:75: }
scripts/mihomo-control.sh:76: 
scripts/mihomo-control.sh:77: action="${1:-status}"
scripts/mihomo-control.sh:78: case "$action" in
scripts/mihomo-control.sh:79:     status)
scripts/mihomo-control.sh:80:         status_json
scripts/mihomo-control.sh:81:         ;;
scripts/mihomo-control.sh:82:     mode)
scripts/mihomo-control.sh:83:         mode="${2:-}"
scripts/mihomo-control.sh:84:         case "$mode" in rule|global|direct) ;; *) exit 2 ;; esac
scripts/mihomo-control.sh:85:         api_patch /configs "{\"mode\":\"${mode}\"}" >/dev/null
scripts/mihomo-control.sh:86:         status_json
scripts/mihomo-control.sh:87:         ;;
scripts/mihomo-control.sh:88:     start|stop|restart)
scripts/mihomo-control.sh:89:         case "$service_scope" in
scripts/mihomo-control.sh:90:             user) systemctl --user "$action" mihomo.service ;;
scripts/mihomo-control.sh:91:             system) systemctl "$action" mihomo.service ;;
scripts/mihomo-control.sh:92:             *) exit 3 ;;
scripts/mihomo-control.sh:93:         esac
…
scripts/mihomo-control.sh:95:         ;;
scripts/mihomo-control.sh:96:     dashboard)
scripts/mihomo-control.sh:97:         printf '%s\n' "${api_base}/ui"
scripts/mihomo-control.sh:98:         ;;
scripts/mihomo-control.sh:99:     *)
scripts/mihomo-control.sh:100:         exit 2
scripts/mihomo-control.sh:101:         ;;
```

### `scripts/power-profile.sh`

Migration owner: `UI/provider integration (domain sections above)`. Source `scripts/power-profile.sh:1`, 29 lines, SHA-256 `a8a84b2aad64c99d99ca45b4660e9d1e0fcc49878e39b38567315c2128f8dfd9`.

```text
scripts/power-profile.sh:1: #!/bin/sh
scripts/power-profile.sh:2: 
scripts/power-profile.sh:3: current="$(powerprofilesctl get 2>/dev/null || printf 'balanced')"
scripts/power-profile.sh:4: 
scripts/power-profile.sh:5: case "$current" in
scripts/power-profile.sh:6:     performance)
scripts/power-profile.sh:7:         next="balanced"
scripts/power-profile.sh:8:         icon="⚡"
scripts/power-profile.sh:9:         ;;
scripts/power-profile.sh:10:     balanced)
scripts/power-profile.sh:11:         next="power-saver"
scripts/power-profile.sh:12:         icon="⚖️"
scripts/power-profile.sh:13:         ;;
scripts/power-profile.sh:14:     power-saver)
scripts/power-profile.sh:15:         next="performance"
scripts/power-profile.sh:16:         icon="🔋"
scripts/power-profile.sh:17:         ;;
scripts/power-profile.sh:18:     *)
scripts/power-profile.sh:19:         next="balanced"
scripts/power-profile.sh:20:         icon="❓"
scripts/power-profile.sh:21:         ;;
scripts/power-profile.sh:22: esac
scripts/power-profile.sh:23: 
scripts/power-profile.sh:24: if [ "${1:-}" = "toggle" ]; then
scripts/power-profile.sh:25:     powerprofilesctl set "$next"
scripts/power-profile.sh:26:     exit 0
scripts/power-profile.sh:27: fi
scripts/power-profile.sh:28: 
scripts/power-profile.sh:29: printf '%s\n' "$icon"
```

### `scripts/prevent-sleep.sh`

Migration owner: `inhibit`. Source `scripts/prevent-sleep.sh:1`, 118 lines, SHA-256 `945f034720b194d81a52334ac8b0a422984e47b10e6160a095f8f6098eb3861f`.

```text
…
scripts/prevent-sleep.sh:4: 
scripts/prevent-sleep.sh:5: STATE_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/hyprv/quickshell"
scripts/prevent-sleep.sh:6: PID_FILE="$STATE_DIR/prevent-sleep.pid"
scripts/prevent-sleep.sh:7: 
scripts/prevent-sleep.sh:8: ensure_state_dir() {
scripts/prevent-sleep.sh:9:     mkdir -p "$STATE_DIR"
scripts/prevent-sleep.sh:10: }
scripts/prevent-sleep.sh:11: 
scripts/prevent-sleep.sh:12: read_pid() {
scripts/prevent-sleep.sh:13:     [[ -f "$PID_FILE" ]] || return 1
scripts/prevent-sleep.sh:14:     tr -d '[:space:]' < "$PID_FILE"
scripts/prevent-sleep.sh:15: }
scripts/prevent-sleep.sh:16: 
scripts/prevent-sleep.sh:17: is_pid_active() {
scripts/prevent-sleep.sh:18:     local pid="$1"
scripts/prevent-sleep.sh:19:     [[ -n "$pid" ]] || return 1
scripts/prevent-sleep.sh:20:     kill -0 "$pid" 2>/dev/null || return 1
scripts/prevent-sleep.sh:21:     ps -o command= -p "$pid" 2>/dev/null | grep -Fq "systemd-inhibit --what=idle:sleep"
scripts/prevent-sleep.sh:22: }
scripts/prevent-sleep.sh:23: 
scripts/prevent-sleep.sh:24: cleanup_stale_state() {
scripts/prevent-sleep.sh:25:     local pid
scripts/prevent-sleep.sh:26:     pid="$(read_pid 2>/dev/null || true)"
scripts/prevent-sleep.sh:27:     if [[ -n "$pid" ]] && ! is_pid_active "$pid"; then
scripts/prevent-sleep.sh:28:         rm -f "$PID_FILE"
scripts/prevent-sleep.sh:29:     fi
scripts/prevent-sleep.sh:30: }
scripts/prevent-sleep.sh:31: 
scripts/prevent-sleep.sh:32: status() {
scripts/prevent-sleep.sh:33:     cleanup_stale_state
scripts/prevent-sleep.sh:34:     local pid
scripts/prevent-sleep.sh:35:     pid="$(read_pid 2>/dev/null || true)"
scripts/prevent-sleep.sh:36:     if [[ -n "$pid" ]] && is_pid_active "$pid"; then
scripts/prevent-sleep.sh:37:         printf 'enabled=true\n'
scripts/prevent-sleep.sh:38:         printf 'pid=%s\n' "$pid"
scripts/prevent-sleep.sh:39:         return 0
scripts/prevent-sleep.sh:40:     fi
scripts/prevent-sleep.sh:41:     printf 'enabled=false\n'
scripts/prevent-sleep.sh:42:     printf 'pid=\n'
scripts/prevent-sleep.sh:43:     return 1
scripts/prevent-sleep.sh:44: }
scripts/prevent-sleep.sh:45: 
scripts/prevent-sleep.sh:46: enable() {
scripts/prevent-sleep.sh:47:     ensure_state_dir
scripts/prevent-sleep.sh:48:     cleanup_stale_state
…
scripts/prevent-sleep.sh:50:     local pid
scripts/prevent-sleep.sh:51:     pid="$(read_pid 2>/dev/null || true)"
scripts/prevent-sleep.sh:52:     if [[ -n "$pid" ]] && is_pid_active "$pid"; then
scripts/prevent-sleep.sh:53:         printf 'enabled=true\n'
scripts/prevent-sleep.sh:54:         printf 'pid=%s\n' "$pid"
scripts/prevent-sleep.sh:55:         return 0
scripts/prevent-sleep.sh:56:     fi
scripts/prevent-sleep.sh:57: 
scripts/prevent-sleep.sh:58:     nohup systemd-inhibit \
scripts/prevent-sleep.sh:59:         --what=idle:sleep \
scripts/prevent-sleep.sh:60:         --mode=block \
scripts/prevent-sleep.sh:61:         --why="HyprV prevent sleep" \
scripts/prevent-sleep.sh:62:         sh -lc 'exec sleep infinity' >/dev/null 2>&1 &
scripts/prevent-sleep.sh:63:     pid="$!"
scripts/prevent-sleep.sh:64:     printf '%s\n' "$pid" > "$PID_FILE"
scripts/prevent-sleep.sh:65:     sleep 0.1
scripts/prevent-sleep.sh:66: 
scripts/prevent-sleep.sh:67:     if ! is_pid_active "$pid"; then
scripts/prevent-sleep.sh:68:         rm -f "$PID_FILE"
scripts/prevent-sleep.sh:69:         printf 'failed to enable prevent sleep\n' >&2
scripts/prevent-sleep.sh:70:         return 1
scripts/prevent-sleep.sh:71:     fi
scripts/prevent-sleep.sh:72: 
scripts/prevent-sleep.sh:73:     printf 'enabled=true\n'
scripts/prevent-sleep.sh:74:     printf 'pid=%s\n' "$pid"
scripts/prevent-sleep.sh:75: }
scripts/prevent-sleep.sh:76: 
scripts/prevent-sleep.sh:77: disable() {
scripts/prevent-sleep.sh:78:     cleanup_stale_state
scripts/prevent-sleep.sh:79:     local pid
scripts/prevent-sleep.sh:80:     pid="$(read_pid 2>/dev/null || true)"
scripts/prevent-sleep.sh:81:     if [[ -z "$pid" ]]; then
scripts/prevent-sleep.sh:82:         printf 'enabled=false\n'
scripts/prevent-sleep.sh:83:         printf 'pid=\n'
scripts/prevent-sleep.sh:84:         return 0
scripts/prevent-sleep.sh:85:     fi
scripts/prevent-sleep.sh:86: 
scripts/prevent-sleep.sh:87:     kill "$pid" 2>/dev/null || true
scripts/prevent-sleep.sh:88:     rm -f "$PID_FILE"
scripts/prevent-sleep.sh:89:     printf 'enabled=false\n'
scripts/prevent-sleep.sh:90:     printf 'pid=\n'
scripts/prevent-sleep.sh:91: }
scripts/prevent-sleep.sh:92: 
scripts/prevent-sleep.sh:93: toggle() {
scripts/prevent-sleep.sh:94:     if status >/dev/null 2>&1; then
scripts/prevent-sleep.sh:95:         disable
scripts/prevent-sleep.sh:96:     else
…
scripts/prevent-sleep.sh:100: 
scripts/prevent-sleep.sh:101: case "${1:-status}" in
scripts/prevent-sleep.sh:102:     status)
scripts/prevent-sleep.sh:103:         status
scripts/prevent-sleep.sh:104:         ;;
scripts/prevent-sleep.sh:105:     enable)
scripts/prevent-sleep.sh:106:         enable
scripts/prevent-sleep.sh:107:         ;;
scripts/prevent-sleep.sh:108:     disable)
scripts/prevent-sleep.sh:109:         disable
scripts/prevent-sleep.sh:110:         ;;
scripts/prevent-sleep.sh:111:     toggle)
scripts/prevent-sleep.sh:112:         toggle
scripts/prevent-sleep.sh:113:         ;;
scripts/prevent-sleep.sh:114:     *)
scripts/prevent-sleep.sh:115:         printf 'Usage: %s [status|enable|disable|toggle]\n' "$0" >&2
scripts/prevent-sleep.sh:116:         exit 1
scripts/prevent-sleep.sh:117:         ;;
```

### `scripts/reload.sh`

Migration owner: `applications/lifecycle`. Source `scripts/reload.sh:1`, 19 lines, SHA-256 `c3b10a558fb3653aa61a23d4021607341701df65f3172daa7f24179663d66d77`.

```text
scripts/reload.sh:1: #!/usr/bin/env bash
scripts/reload.sh:2: 
scripts/reload.sh:3: set -euo pipefail
scripts/reload.sh:4: 
scripts/reload.sh:5: config_dir="$HOME/.config/HyprV/quickshell"
scripts/reload.sh:6: script_dir="$config_dir/scripts"
scripts/reload.sh:7: 
scripts/reload.sh:8: # These small windows share one render loop instead of per-window render threads.
scripts/reload.sh:9: # Keep an explicit session override available for profiling or other GPUs.
scripts/reload.sh:10: export QSG_RENDER_LOOP="${QSG_RENDER_LOOP:-basic}"
scripts/reload.sh:11: 
scripts/reload.sh:12: "$script_dir/ui-state.sh" ensure >/dev/null 2>&1 || true
scripts/reload.sh:13: # Keep manual theme changes intact across reload; explicit theme toggles still call apply-ui-state.sh.
scripts/reload.sh:14: # "$script_dir/apply-ui-state.sh" apply >/dev/null 2>&1 || true
scripts/reload.sh:15: "$script_dir/fan-control-helper.sh" restore >/dev/null 2>&1 || true
scripts/reload.sh:16: 
scripts/reload.sh:17: quickshell kill -p "$config_dir" >/dev/null 2>&1 || true
scripts/reload.sh:18: sleep 0.2
scripts/reload.sh:19: exec quickshell -p "$config_dir" -d
```

### `scripts/set-variant.sh`

Migration owner: `theme`. Source `scripts/set-variant.sh:1`, 6 lines, SHA-256 `4e9be18b1a9cd1d9752d0931d975b92949fa8dc44130b7127f33a4e92ebe3e26`.

```text
scripts/set-variant.sh:1: #!/usr/bin/env bash
scripts/set-variant.sh:2: 
scripts/set-variant.sh:3: set -euo pipefail
scripts/set-variant.sh:4: 
scripts/set-variant.sh:5: script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
scripts/set-variant.sh:6: exec "$script_dir/apply-ui-state.sh" set-variant "${1:-}"
```

### `scripts/system-snapshot.sh`

Migration owner: `sysstats`. Source `scripts/system-snapshot.sh:1`, 71 lines, SHA-256 `06aece8c7defd2512776f42f4ab96423d0d98692e94fed5a7e1e3c4ea53398b5`.

```text
…
scripts/system-snapshot.sh:4: 
scripts/system-snapshot.sh:5: valid_temperature() {
scripts/system-snapshot.sh:6:     [[ "$1" =~ ^-?[0-9]+$ ]] && ((1 <= $1 && $1 <= 125000))
scripts/system-snapshot.sh:7: }
scripts/system-snapshot.sh:8: 
scripts/system-snapshot.sh:9: read_cpu_temperature() {
scripts/system-snapshot.sh:10:     local preferred zone type value hwmon name input base label
scripts/system-snapshot.sh:11: 
…
scripts/system-snapshot.sh:14:     for preferred in x86_pkg_temp TCPU cpu_thermal cpu-thermal soc_thermal; do
scripts/system-snapshot.sh:15:         for zone in /sys/class/thermal/thermal_zone*; do
scripts/system-snapshot.sh:16:             [[ -r "${zone}/type" && -r "${zone}/temp" ]] || continue
scripts/system-snapshot.sh:17:             type="$(<"${zone}/type")"
scripts/system-snapshot.sh:18:             [[ "$type" == "$preferred" ]] || continue
scripts/system-snapshot.sh:19:             value="$(<"${zone}/temp")"
scripts/system-snapshot.sh:20:             if valid_temperature "$value"; then
scripts/system-snapshot.sh:21:                 printf '%s\n' "$value"
scripts/system-snapshot.sh:22:                 return 0
scripts/system-snapshot.sh:23:             fi
…
scripts/system-snapshot.sh:28:     # CPU package sensors on systems without a useful thermal-zone entry.
scripts/system-snapshot.sh:29:     for hwmon in /sys/class/hwmon/hwmon*; do
scripts/system-snapshot.sh:30:         [[ -r "${hwmon}/name" ]] || continue
scripts/system-snapshot.sh:31:         name="$(<"${hwmon}/name")"
scripts/system-snapshot.sh:32:         [[ "$name" == coretemp || "$name" == k10temp || "$name" == zenpower ]] || continue
scripts/system-snapshot.sh:33:         for input in "${hwmon}"/temp*_input; do
scripts/system-snapshot.sh:34:             [[ -r "$input" ]] || continue
scripts/system-snapshot.sh:35:             base="${input%_input}"
scripts/system-snapshot.sh:36:             label=""
scripts/system-snapshot.sh:37:             [[ ! -r "${base}_label" ]] || label="$(<"${base}_label")"
scripts/system-snapshot.sh:38:             case "$label" in
scripts/system-snapshot.sh:39:                 "Package id 0"|Tctl|"CPU Package"|CPU)
scripts/system-snapshot.sh:40:                     value="$(<"$input")"
scripts/system-snapshot.sh:41:                     if valid_temperature "$value"; then
scripts/system-snapshot.sh:42:                         printf '%s\n' "$value"
scripts/system-snapshot.sh:43:                         return 0
scripts/system-snapshot.sh:44:                     fi
…
scripts/system-snapshot.sh:52: 
scripts/system-snapshot.sh:53: printf '__STAT__\n'
scripts/system-snapshot.sh:54: if [[ "${1:-}" == "--minimal" ]]; then
scripts/system-snapshot.sh:55:     head -n 1 /proc/stat
scripts/system-snapshot.sh:56: else
scripts/system-snapshot.sh:57:     cat /proc/stat
scripts/system-snapshot.sh:58: fi
scripts/system-snapshot.sh:59: printf '\n__MEM__\n'
scripts/system-snapshot.sh:60: cat /proc/meminfo
scripts/system-snapshot.sh:61: printf '\n__TEMP__\n'
scripts/system-snapshot.sh:62: if [[ "${1:-}" != "--minimal" ]]; then
scripts/system-snapshot.sh:63:     read_cpu_temperature || true
scripts/system-snapshot.sh:64: fi
scripts/system-snapshot.sh:65: printf '\n__CPU_POWER__\n'
scripts/system-snapshot.sh:66: [[ -r /run/hyprv/cpu-package-power-w ]] && cat /run/hyprv/cpu-package-power-w
scripts/system-snapshot.sh:67: printf '\n__ROUTE__\n'
scripts/system-snapshot.sh:68: cat /proc/net/route
scripts/system-snapshot.sh:69: [[ "${1:-}" != "--minimal" ]] || exit 0
scripts/system-snapshot.sh:70: printf '\n__NET__\n'
scripts/system-snapshot.sh:71: cat /proc/net/dev
```

### `scripts/toggle-theme.sh`

Migration owner: `theme`. Source `scripts/toggle-theme.sh:1`, 6 lines, SHA-256 `d906b4187fef75d7279eff0c6f8ee5fb9420bd7c185a4e3ccb03c861f6b2676b`.

```text
scripts/toggle-theme.sh:1: #!/usr/bin/env bash
scripts/toggle-theme.sh:2: 
scripts/toggle-theme.sh:3: set -euo pipefail
scripts/toggle-theme.sh:4: 
scripts/toggle-theme.sh:5: script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
scripts/toggle-theme.sh:6: exec "$script_dir/apply-ui-state.sh" toggle-theme
```

### `scripts/ui-state.sh`

Migration owner: `theme`. Source `scripts/ui-state.sh:1`, 150 lines, SHA-256 `a94ea2f8e133f2c19ee8ea31f00c9e413e69215c4a971d88880ed8e326027141`.

```text
…
scripts/ui-state.sh:4: 
scripts/ui-state.sh:5: STATE_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/hyprv/quickshell"
scripts/ui-state.sh:6: STATE_FILE="$STATE_DIR/ui-state.env"
scripts/ui-state.sh:7: 
scripts/ui-state.sh:8: normalize_theme() {
scripts/ui-state.sh:9:     case "${1:-}" in
scripts/ui-state.sh:10:         light|dark)
scripts/ui-state.sh:11:             printf '%s\n' "$1"
scripts/ui-state.sh:12:             ;;
scripts/ui-state.sh:13:         *)
scripts/ui-state.sh:14:             return 1
scripts/ui-state.sh:15:             ;;
…
scripts/ui-state.sh:18: 
scripts/ui-state.sh:19: normalize_variant() {
scripts/ui-state.sh:20:     case "${1:-}" in
scripts/ui-state.sh:21:         v1|v2|v3|bt)
scripts/ui-state.sh:22:             printf '%s\n' "$1"
scripts/ui-state.sh:23:             ;;
scripts/ui-state.sh:24:         *)
scripts/ui-state.sh:25:             return 1
scripts/ui-state.sh:26:             ;;
…
scripts/ui-state.sh:29: 
scripts/ui-state.sh:30: legacy_theme() {
scripts/ui-state.sh:31:     local legacy_style
scripts/ui-state.sh:32:     legacy_style="$(readlink -f "$HOME/.config/waybar/style.css" 2>/dev/null || true)"
scripts/ui-state.sh:33:     if [[ "$legacy_style" == *"-dark.css" ]]; then
scripts/ui-state.sh:34:         printf 'dark\n'
scripts/ui-state.sh:35:     else
scripts/ui-state.sh:36:         printf 'light\n'
scripts/ui-state.sh:37:     fi
scripts/ui-state.sh:38: }
scripts/ui-state.sh:39: 
scripts/ui-state.sh:40: legacy_variant() {
scripts/ui-state.sh:41:     local legacy_config legacy_style candidate
scripts/ui-state.sh:42:     legacy_config="$(readlink -f "$HOME/.config/waybar/config.jsonc" 2>/dev/null || true)"
scripts/ui-state.sh:43:     legacy_style="$(readlink -f "$HOME/.config/waybar/style.css" 2>/dev/null || true)"
scripts/ui-state.sh:44: 
scripts/ui-state.sh:45:     for candidate in bt v1 v2 v3; do
scripts/ui-state.sh:46:         if [[ "$legacy_config" == *"/${candidate}-config.jsonc" ]] || [[ "$legacy_style" == *"/${candidate}-style"* ]]; then
scripts/ui-state.sh:47:             printf '%s\n' "$candidate"
scripts/ui-state.sh:48:             return 0
scripts/ui-state.sh:49:         fi
…
scripts/ui-state.sh:51: 
scripts/ui-state.sh:52:     printf 'v2\n'
scripts/ui-state.sh:53: }
scripts/ui-state.sh:54: 
scripts/ui-state.sh:55: ensure_state() {
scripts/ui-state.sh:56:     mkdir -p "$STATE_DIR"
scripts/ui-state.sh:57:     if [[ ! -f "$STATE_FILE" ]]; then
scripts/ui-state.sh:58:         printf 'theme=%s\nvariant=%s\n' "$(legacy_theme)" "$(legacy_variant)" > "$STATE_FILE"
scripts/ui-state.sh:59:     fi
scripts/ui-state.sh:60: }
scripts/ui-state.sh:61: 
scripts/ui-state.sh:62: load_state() {
scripts/ui-state.sh:63:     local key value
scripts/ui-state.sh:64: 
…
scripts/ui-state.sh:70:     while IFS='=' read -r key value; do
scripts/ui-state.sh:71:         case "$key" in
scripts/ui-state.sh:72:             theme)
scripts/ui-state.sh:73:                 theme="$value"
scripts/ui-state.sh:74:                 ;;
scripts/ui-state.sh:75:             variant)
scripts/ui-state.sh:76:                 variant="$value"
scripts/ui-state.sh:77:                 ;;
scripts/ui-state.sh:78:         esac
scripts/ui-state.sh:79:     done < "$STATE_FILE"
scripts/ui-state.sh:80: 
scripts/ui-state.sh:81:     if ! theme="$(normalize_theme "$theme" 2>/dev/null)"; then
scripts/ui-state.sh:82:         theme="$(legacy_theme)"
scripts/ui-state.sh:83:     fi
scripts/ui-state.sh:84:     if ! variant="$(normalize_variant "$variant" 2>/dev/null)"; then
scripts/ui-state.sh:85:         variant="$(legacy_variant)"
scripts/ui-state.sh:86:     fi
…
scripts/ui-state.sh:88: 
scripts/ui-state.sh:89: write_state() {
scripts/ui-state.sh:90:     mkdir -p "$STATE_DIR"
scripts/ui-state.sh:91:     printf 'theme=%s\nvariant=%s\n' "$theme" "$variant" > "$STATE_FILE"
scripts/ui-state.sh:92: }
scripts/ui-state.sh:93: 
scripts/ui-state.sh:94: set_theme() {
scripts/ui-state.sh:95:     load_state
scripts/ui-state.sh:96:     theme="$(normalize_theme "${1:-}")"
…
scripts/ui-state.sh:99: 
scripts/ui-state.sh:100: set_variant() {
scripts/ui-state.sh:101:     load_state
scripts/ui-state.sh:102:     variant="$(normalize_variant "${1:-}")"
…
scripts/ui-state.sh:105: 
scripts/ui-state.sh:106: toggle_theme() {
scripts/ui-state.sh:107:     load_state
scripts/ui-state.sh:108:     if [[ "$theme" == "dark" ]]; then
scripts/ui-state.sh:109:         theme="light"
scripts/ui-state.sh:110:     else
…
scripts/ui-state.sh:113:     write_state
scripts/ui-state.sh:114:     printf '%s\n' "$theme"
scripts/ui-state.sh:115: }
scripts/ui-state.sh:116: 
scripts/ui-state.sh:117: print_state() {
scripts/ui-state.sh:118:     load_state
scripts/ui-state.sh:119:     printf 'theme=%s\nvariant=%s\n' "$theme" "$variant"
scripts/ui-state.sh:120: }
scripts/ui-state.sh:121: 
scripts/ui-state.sh:122: case "${1:-print}" in
scripts/ui-state.sh:123:     ensure)
scripts/ui-state.sh:124:         ensure_state
scripts/ui-state.sh:125:         ;;
scripts/ui-state.sh:126:     print)
scripts/ui-state.sh:127:         print_state
scripts/ui-state.sh:128:         ;;
scripts/ui-state.sh:129:     get-theme)
scripts/ui-state.sh:130:         load_state
scripts/ui-state.sh:131:         printf '%s\n' "$theme"
scripts/ui-state.sh:132:         ;;
scripts/ui-state.sh:133:     get-variant)
scripts/ui-state.sh:134:         load_state
scripts/ui-state.sh:135:         printf '%s\n' "$variant"
scripts/ui-state.sh:136:         ;;
scripts/ui-state.sh:137:     set-theme)
scripts/ui-state.sh:138:         set_theme "${2:-}"
scripts/ui-state.sh:139:         ;;
scripts/ui-state.sh:140:     set-variant)
scripts/ui-state.sh:141:         set_variant "${2:-}"
scripts/ui-state.sh:142:         ;;
scripts/ui-state.sh:143:     toggle-theme)
scripts/ui-state.sh:144:         toggle_theme
scripts/ui-state.sh:145:         ;;
scripts/ui-state.sh:146:     *)
scripts/ui-state.sh:147:         printf 'Usage: %s [ensure|print|get-theme|get-variant|set-theme <light|dark>|set-variant <v1|v2|v3|bt>|toggle-theme]\n' "$0" >&2
scripts/ui-state.sh:148:         exit 1
scripts/ui-state.sh:149:         ;;
```

### `scripts/viewflow-target.sh`

Migration owner: `viewflow`. Source `scripts/viewflow-target.sh:1`, 2 lines, SHA-256 `be39a8510c1b5bc9ae38a392118dee8c5c8de8f321cab277b0221d3ff99613cf`.

```text
scripts/viewflow-target.sh:1: #!/usr/bin/env bash
scripts/viewflow-target.sh:2: exec bash /home/wilf/data/viewflow/tools/viewflow-target.sh "$@"
```

### `scripts/viewflow-target.sh.backup-1788895486`

Migration owner: `viewflow`. Historical backup; not selected by current QML. Source `scripts/viewflow-target.sh.backup-1788895486:1`, 100 lines, SHA-256 `e47bea297f4e0fd73df018a7ab3703eadc400a5003b369fc11aa34089d02549a`.

```text
…
scripts/viewflow-target.sh.backup-1788895486:2: set -euo pipefail
scripts/viewflow-target.sh.backup-1788895486:3: readonly config_dir="${XDG_CONFIG_HOME:-$HOME/.config}/viewflow"
scripts/viewflow-target.sh.backup-1788895486:4: readonly state_dir="${XDG_STATE_HOME:-$HOME/.local/state}/hyprv"
scripts/viewflow-target.sh.backup-1788895486:5: readonly runtime="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/viewflow"
scripts/viewflow-target.sh.backup-1788895486:6: readonly script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
scripts/viewflow-target.sh.backup-1788895486:7: unit_for() {
scripts/viewflow-target.sh.backup-1788895486:8:   case "$1" in
scripts/viewflow-target.sh.backup-1788895486:9:     windows) echo viewflow-desktop.service ;;
scripts/viewflow-target.sh.backup-1788895486:10:     macos) echo viewflow-macos-input.service ;;
scripts/viewflow-target.sh.backup-1788895486:11:     *) return 2 ;;
scripts/viewflow-target.sh.backup-1788895486:12:   esac
scripts/viewflow-target.sh.backup-1788895486:13: }
scripts/viewflow-target.sh.backup-1788895486:14: current_target() {
scripts/viewflow-target.sh.backup-1788895486:15:   if systemctl --user is-active --quiet viewflow-macos-input.service; then echo macos
scripts/viewflow-target.sh.backup-1788895486:16:   elif systemctl --user is-active --quiet viewflow-desktop.service; then echo windows
scripts/viewflow-target.sh.backup-1788895486:17:   elif [[ -r "$state_dir/viewflow-target" ]]; then cat "$state_dir/viewflow-target"
scripts/viewflow-target.sh.backup-1788895486:18:   else echo windows
scripts/viewflow-target.sh.backup-1788895486:19:   fi
scripts/viewflow-target.sh.backup-1788895486:20: }
scripts/viewflow-target.sh.backup-1788895486:21: start_target() {
scripts/viewflow-target.sh.backup-1788895486:22:   if [[ "$1" == windows ]]; then
scripts/viewflow-target.sh.backup-1788895486:23:     # The current Windows desktop receiver exits on disconnect. Resume its
…
scripts/viewflow-target.sh.backup-1788895486:25:     python3 - <<'WINDOWS'
scripts/viewflow-target.sh.backup-1788895486:26: import base64, subprocess
scripts/viewflow-target.sh.backup-1788895486:27: script="if ((Get-ScheduledTask -TaskName 'ViewflowMain-Active').State -ne 'Running') { Start-ScheduledTask -TaskName 'ViewflowMain-Active' }"
scripts/viewflow-target.sh.backup-1788895486:28: encoded=base64.b64encode(script.encode('utf-16le')).decode()
scripts/viewflow-target.sh.backup-1788895486:29: subprocess.run(['ssh','-o','BatchMode=yes','-o','ConnectTimeout=5',
scripts/viewflow-target.sh.backup-1788895486:30:     'wilf@172.16.105.70','powershell.exe','-NoProfile','-NonInteractive',
scripts/viewflow-target.sh.backup-1788895486:31:     '-EncodedCommand',encoded],check=True,timeout=10,stdout=subprocess.DEVNULL)
scripts/viewflow-target.sh.backup-1788895486:32: WINDOWS
scripts/viewflow-target.sh.backup-1788895486:33:     [[ $? == 0 ]] || return 1
scripts/viewflow-target.sh.backup-1788895486:34:   fi
scripts/viewflow-target.sh.backup-1788895486:35:   systemctl --user start "$(unit_for "$1")"
scripts/viewflow-target.sh.backup-1788895486:36: }
scripts/viewflow-target.sh.backup-1788895486:37: wait_ready() {
scripts/viewflow-target.sh.backup-1788895486:38:   local target=$1 offset=$2
scripts/viewflow-target.sh.backup-1788895486:39:   python3 - "$target" "$runtime" "$offset" <<'CHECK'
scripts/viewflow-target.sh.backup-1788895486:40: import pathlib,sys,time,subprocess,os
scripts/viewflow-target.sh.backup-1788895486:41: name,root,offset=sys.argv[1],pathlib.Path(sys.argv[2]),int(sys.argv[3])
scripts/viewflow-target.sh.backup-1788895486:42: expected='vf-cursor-peer' if name=='macos' else 'vf-media-peer'
…
scripts/viewflow-target.sh.backup-1788895486:44:     try:
scripts/viewflow-target.sh.backup-1788895486:45:         pid=int((root/'desktop-drag/daemon.pid').read_text())
scripts/viewflow-target.sh.backup-1788895486:46:         executable=pathlib.Path(f'/proc/{pid}/exe').resolve(strict=True)
scripts/viewflow-target.sh.backup-1788895486:47:         valid=executable.name==expected and (root/'hyprland.sock').exists()
scripts/viewflow-target.sh.backup-1788895486:48:         if name=='macos':
scripts/viewflow-target.sh.backup-1788895486:49:             valid=valid and int((root/'cursor-ready').read_text())==pid
scripts/viewflow-target.sh.backup-1788895486:50:         else:
scripts/viewflow-target.sh.backup-1788895486:51:             with (root/'desktop-drag/source.log').open('rb') as f:
scripts/viewflow-target.sh.backup-1788895486:52:                 if f.seek(0,2)<offset: offset=0
scripts/viewflow-target.sh.backup-1788895486:53:                 f.seek(offset)
scripts/viewflow-target.sh.backup-1788895486:54:                 text=f.read().decode(errors='replace')
scripts/viewflow-target.sh.backup-1788895486:55:             valid=valid and 'phase=configured' in text and 'clock_decision=accepted' in text
scripts/viewflow-target.sh.backup-1788895486:56:         if valid: sys.exit(0)
scripts/viewflow-target.sh.backup-1788895486:57:     except (OSError,ValueError): pass
scripts/viewflow-target.sh.backup-1788895486:58:     time.sleep(.2)
scripts/viewflow-target.sh.backup-1788895486:59: raise SystemExit('Viewflow '+name+' 连接未就绪；请检查服务日志。')
scripts/viewflow-target.sh.backup-1788895486:60: CHECK
scripts/viewflow-target.sh.backup-1788895486:61: }
scripts/viewflow-target.sh.backup-1788895486:62: case "${1:-}" in
scripts/viewflow-target.sh.backup-1788895486:63:   status) current_target ;;
scripts/viewflow-target.sh.backup-1788895486:64:   set)
scripts/viewflow-target.sh.backup-1788895486:65:     requested=${2:-}
scripts/viewflow-target.sh.backup-1788895486:66:     requested_unit=$(unit_for "$requested") || { echo '目标必须是 windows 或 macos' >&2; exit 2; }
scripts/viewflow-target.sh.backup-1788895486:67:     mkdir -p "$config_dir" "$state_dir"
scripts/viewflow-target.sh.backup-1788895486:68:     exec 9>"$config_dir/target-switch.lock"
scripts/viewflow-target.sh.backup-1788895486:69:     flock 9
scripts/viewflow-target.sh.backup-1788895486:70:     previous=$(current_target)
scripts/viewflow-target.sh.backup-1788895486:71:     previous_unit=$(unit_for "$previous")
scripts/viewflow-target.sh.backup-1788895486:72:     if [[ "$requested" != "$previous" ]] || ! systemctl --user is-active --quiet "$requested_unit"; then
scripts/viewflow-target.sh.backup-1788895486:73:       offset=$(stat -c %s "$runtime/desktop-drag/source.log" 2>/dev/null || echo 0)
scripts/viewflow-target.sh.backup-1788895486:74:       systemctl --user stop "$previous_unit"
scripts/viewflow-target.sh.backup-1788895486:75:       if ! start_target "$requested" || ! wait_ready "$requested" "$offset"; then
scripts/viewflow-target.sh.backup-1788895486:76:         systemctl --user stop "$requested_unit" || true
scripts/viewflow-target.sh.backup-1788895486:77:         start_target "$previous" || true
scripts/viewflow-target.sh.backup-1788895486:78:         echo 'Viewflow 切换失败，已尝试恢复原目标。' >&2
scripts/viewflow-target.sh.backup-1788895486:79:         exit 1
scripts/viewflow-target.sh.backup-1788895486:80:       fi
…
scripts/viewflow-target.sh.backup-1788895486:82:     # Remember the selected input route across login/reboot.
scripts/viewflow-target.sh.backup-1788895486:83:     systemctl --user enable "$requested_unit" >/dev/null
scripts/viewflow-target.sh.backup-1788895486:84:     if [[ "$previous_unit" != "$requested_unit" ]]; then
scripts/viewflow-target.sh.backup-1788895486:85:       systemctl --user disable "$previous_unit" >/dev/null
scripts/viewflow-target.sh.backup-1788895486:86:     fi
scripts/viewflow-target.sh.backup-1788895486:87:     temporary=$(mktemp "$state_dir/.viewflow-target.XXXXXX")
scripts/viewflow-target.sh.backup-1788895486:88:     printf '%s\n' "$requested" > "$temporary"
scripts/viewflow-target.sh.backup-1788895486:89:     mv -f "$temporary" "$state_dir/viewflow-target"
scripts/viewflow-target.sh.backup-1788895486:90:     # Retain the button's HDMI behavior, after the input route is ready.
scripts/viewflow-target.sh.backup-1788895486:91:     case "$requested" in windows) button=input1 ;; macos) button=input2 ;; esac
scripts/viewflow-target.sh.backup-1788895486:92:     if ! "$script_dir/hdmi-switch-ir.py" send "$button" >/dev/null; then
scripts/viewflow-target.sh.backup-1788895486:93:       printf 'Viewflow 已切换到 %s，但 HDMI 切换失败。\n' "$requested" >&2
scripts/viewflow-target.sh.backup-1788895486:94:       exit 1
scripts/viewflow-target.sh.backup-1788895486:95:     fi
scripts/viewflow-target.sh.backup-1788895486:96:     printf '%s\n' "$requested" > "$state_dir/hdmi-target"
scripts/viewflow-target.sh.backup-1788895486:97:     current_target
scripts/viewflow-target.sh.backup-1788895486:98:     ;;
scripts/viewflow-target.sh.backup-1788895486:99:   *) echo "Usage: $0 {status|set windows|set macos}" >&2; exit 2 ;;
scripts/viewflow-target.sh.backup-1788895486:100: esac
```

### `scripts/viewflow-target.sh.before-discovery-1789254904270120150`

Migration owner: `viewflow`. Historical backup; not selected by current QML. Source `scripts/viewflow-target.sh.before-discovery-1789254904270120150:1`, 102 lines, SHA-256 `b876c14d68229ade2ab800b4dbc050e099eeee71ae3405c8cbd386665ceff1bb`.

```text
…
scripts/viewflow-target.sh.before-discovery-1789254904270120150:2: set -euo pipefail
scripts/viewflow-target.sh.before-discovery-1789254904270120150:3: readonly config_dir="${XDG_CONFIG_HOME:-$HOME/.config}/viewflow"
scripts/viewflow-target.sh.before-discovery-1789254904270120150:4: readonly state_dir="${XDG_STATE_HOME:-$HOME/.local/state}/hyprv"
scripts/viewflow-target.sh.before-discovery-1789254904270120150:5: readonly runtime="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/viewflow"
scripts/viewflow-target.sh.before-discovery-1789254904270120150:6: readonly script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
scripts/viewflow-target.sh.before-discovery-1789254904270120150:7: unit_for() {
scripts/viewflow-target.sh.before-discovery-1789254904270120150:8:   case "$1" in
scripts/viewflow-target.sh.before-discovery-1789254904270120150:9:     windows) echo viewflow-desktop.service ;;
scripts/viewflow-target.sh.before-discovery-1789254904270120150:10:     macos) echo viewflow-macos-input.service ;;
scripts/viewflow-target.sh.before-discovery-1789254904270120150:11:     *) return 2 ;;
scripts/viewflow-target.sh.before-discovery-1789254904270120150:12:   esac
scripts/viewflow-target.sh.before-discovery-1789254904270120150:13: }
scripts/viewflow-target.sh.before-discovery-1789254904270120150:14: current_target() {
scripts/viewflow-target.sh.before-discovery-1789254904270120150:15:   if systemctl --user is-active --quiet viewflow-macos-input.service; then echo macos
scripts/viewflow-target.sh.before-discovery-1789254904270120150:16:   elif systemctl --user is-active --quiet viewflow-windows-input.service || systemctl --user is-active --quiet viewflow-desktop.service; then echo windows
scripts/viewflow-target.sh.before-discovery-1789254904270120150:17:   elif [[ -r "$state_dir/viewflow-target" ]]; then cat "$state_dir/viewflow-target"
scripts/viewflow-target.sh.before-discovery-1789254904270120150:18:   else echo windows
scripts/viewflow-target.sh.before-discovery-1789254904270120150:19:   fi
scripts/viewflow-target.sh.before-discovery-1789254904270120150:20: }
scripts/viewflow-target.sh.before-discovery-1789254904270120150:21: start_target() {
scripts/viewflow-target.sh.before-discovery-1789254904270120150:22:   if [[ "$1" == windows ]]; then
scripts/viewflow-target.sh.before-discovery-1789254904270120150:23:     # The paired input receiver runs in session 0, including before login.
scripts/viewflow-target.sh.before-discovery-1789254904270120150:24:     python3 - <<'WINDOWS'
scripts/viewflow-target.sh.before-discovery-1789254904270120150:25: import base64, subprocess
scripts/viewflow-target.sh.before-discovery-1789254904270120150:26: script="if ((Get-ScheduledTask -TaskName 'ViewflowInput-Prelogin').State -ne 'Running') { Start-ScheduledTask -TaskName 'ViewflowInput-Prelogin' }"
scripts/viewflow-target.sh.before-discovery-1789254904270120150:27: encoded=base64.b64encode(script.encode('utf-16le')).decode()
scripts/viewflow-target.sh.before-discovery-1789254904270120150:28: subprocess.run(['ssh','-o','BatchMode=yes','-o','ConnectTimeout=5',
scripts/viewflow-target.sh.before-discovery-1789254904270120150:29:     'wilf@172.16.105.70','powershell.exe','-NoProfile','-NonInteractive',
scripts/viewflow-target.sh.before-discovery-1789254904270120150:30:     '-EncodedCommand',encoded],check=True,timeout=10,stdout=subprocess.DEVNULL)
scripts/viewflow-target.sh.before-discovery-1789254904270120150:31: WINDOWS
scripts/viewflow-target.sh.before-discovery-1789254904270120150:32:     [[ $? == 0 ]] || return 1
scripts/viewflow-target.sh.before-discovery-1789254904270120150:33:   fi
scripts/viewflow-target.sh.before-discovery-1789254904270120150:34:   systemctl --user start "$(unit_for "$1")"
scripts/viewflow-target.sh.before-discovery-1789254904270120150:35: }
scripts/viewflow-target.sh.before-discovery-1789254904270120150:36: wait_ready() {
scripts/viewflow-target.sh.before-discovery-1789254904270120150:37:   local target=$1 offset=$2
scripts/viewflow-target.sh.before-discovery-1789254904270120150:38:   python3 - "$target" "$runtime" "$offset" <<'CHECK'
scripts/viewflow-target.sh.before-discovery-1789254904270120150:39: import pathlib,sys,time,subprocess,os
scripts/viewflow-target.sh.before-discovery-1789254904270120150:40: name,root,offset=sys.argv[1],pathlib.Path(sys.argv[2]),int(sys.argv[3])
scripts/viewflow-target.sh.before-discovery-1789254904270120150:41: expected='vf-cursor-peer'
…
scripts/viewflow-target.sh.before-discovery-1789254904270120150:43:     try:
scripts/viewflow-target.sh.before-discovery-1789254904270120150:44:         pid=int((root/'desktop-drag/daemon.pid').read_text())
scripts/viewflow-target.sh.before-discovery-1789254904270120150:45:         executable=pathlib.Path(f'/proc/{pid}/exe').resolve(strict=True)
scripts/viewflow-target.sh.before-discovery-1789254904270120150:46:         valid=executable.name==expected and (root/'hyprland.sock').exists()
scripts/viewflow-target.sh.before-discovery-1789254904270120150:47:         valid=valid and int((root/'cursor-ready').read_text())==pid
scripts/viewflow-target.sh.before-discovery-1789254904270120150:48:         import json
scripts/viewflow-target.sh.before-discovery-1789254904270120150:49:         config=json.loads((root/'desktop-drag/config').read_text())
scripts/viewflow-target.sh.before-discovery-1789254904270120150:50:         expected_remote='172.16.105.70:44149' if name=='windows' else '172.16.105.83:44139'
scripts/viewflow-target.sh.before-discovery-1789254904270120150:51:         valid=valid and config['remote']==expected_remote
…
scripts/viewflow-target.sh.before-discovery-1789254904270120150:53:     except (OSError,ValueError): pass
scripts/viewflow-target.sh.before-discovery-1789254904270120150:54:     time.sleep(.2)
scripts/viewflow-target.sh.before-discovery-1789254904270120150:55: raise SystemExit('Viewflow '+name+' 连接未就绪；请检查服务日志。')
scripts/viewflow-target.sh.before-discovery-1789254904270120150:56: CHECK
scripts/viewflow-target.sh.before-discovery-1789254904270120150:57: }
scripts/viewflow-target.sh.before-discovery-1789254904270120150:58: case "${1:-}" in
scripts/viewflow-target.sh.before-discovery-1789254904270120150:59:   status)
scripts/viewflow-target.sh.before-discovery-1789254904270120150:60:     if [[ -r "$state_dir/hdmi-target" ]]; then cat "$state_dir/hdmi-target"
scripts/viewflow-target.sh.before-discovery-1789254904270120150:61:     else current_target
scripts/viewflow-target.sh.before-discovery-1789254904270120150:62:     fi
scripts/viewflow-target.sh.before-discovery-1789254904270120150:63:     ;;
scripts/viewflow-target.sh.before-discovery-1789254904270120150:64:   set)
scripts/viewflow-target.sh.before-discovery-1789254904270120150:65:     requested=${2:-}
scripts/viewflow-target.sh.before-discovery-1789254904270120150:66:     requested_unit=$(unit_for "$requested") || { echo '目标必须是 windows 或 macos' >&2; exit 2; }
scripts/viewflow-target.sh.before-discovery-1789254904270120150:67:     mkdir -p "$config_dir" "$state_dir"
scripts/viewflow-target.sh.before-discovery-1789254904270120150:68:     exec 9>"$config_dir/target-switch.lock"
scripts/viewflow-target.sh.before-discovery-1789254904270120150:69:     flock 9
scripts/viewflow-target.sh.before-discovery-1789254904270120150:70:     # HDMI must remain usable even when a Viewflow peer cannot connect.
scripts/viewflow-target.sh.before-discovery-1789254904270120150:71:     case "$requested" in windows) button=input1 ;; macos) button=input2 ;; esac
scripts/viewflow-target.sh.before-discovery-1789254904270120150:72:     if ! "$script_dir/hdmi-switch-ir.py" send "$button" >/dev/null; then
scripts/viewflow-target.sh.before-discovery-1789254904270120150:73:       printf 'HDMI 切换到 %s 失败。\n' "$requested" >&2
scripts/viewflow-target.sh.before-discovery-1789254904270120150:74:       exit 1
scripts/viewflow-target.sh.before-discovery-1789254904270120150:75:     fi
scripts/viewflow-target.sh.before-discovery-1789254904270120150:76:     temporary=$(mktemp "$state_dir/.hdmi-target.XXXXXX")
scripts/viewflow-target.sh.before-discovery-1789254904270120150:77:     printf '%s\n' "$requested" > "$temporary"
scripts/viewflow-target.sh.before-discovery-1789254904270120150:78:     mv -f "$temporary" "$state_dir/hdmi-target"
scripts/viewflow-target.sh.before-discovery-1789254904270120150:79:     previous=$(current_target)
scripts/viewflow-target.sh.before-discovery-1789254904270120150:80:     previous_unit=$(unit_for "$previous")
scripts/viewflow-target.sh.before-discovery-1789254904270120150:81:     if [[ "$requested" != "$previous" ]] || ! systemctl --user is-active --quiet "$requested_unit"; then
scripts/viewflow-target.sh.before-discovery-1789254904270120150:82:       offset=$(stat -c %s "$runtime/desktop-drag/source.log" 2>/dev/null || echo 0)
scripts/viewflow-target.sh.before-discovery-1789254904270120150:83:       systemctl --user stop "$previous_unit"
scripts/viewflow-target.sh.before-discovery-1789254904270120150:84:       if ! start_target "$requested" || ! wait_ready "$requested" "$offset"; then
scripts/viewflow-target.sh.before-discovery-1789254904270120150:85:         systemctl --user stop "$requested_unit" || true
scripts/viewflow-target.sh.before-discovery-1789254904270120150:86:         start_target "$previous" || true
scripts/viewflow-target.sh.before-discovery-1789254904270120150:87:         echo 'HDMI 已切换；Viewflow 连接失败，已尝试恢复原控制目标。' >&2
scripts/viewflow-target.sh.before-discovery-1789254904270120150:88:         exit 1
scripts/viewflow-target.sh.before-discovery-1789254904270120150:89:       fi
…
scripts/viewflow-target.sh.before-discovery-1789254904270120150:91:     # Remember the selected input route across login/reboot.
scripts/viewflow-target.sh.before-discovery-1789254904270120150:92:     systemctl --user enable "$requested_unit" >/dev/null
scripts/viewflow-target.sh.before-discovery-1789254904270120150:93:     if [[ "$previous_unit" != "$requested_unit" ]]; then
scripts/viewflow-target.sh.before-discovery-1789254904270120150:94:       systemctl --user disable "$previous_unit" >/dev/null
scripts/viewflow-target.sh.before-discovery-1789254904270120150:95:     fi
scripts/viewflow-target.sh.before-discovery-1789254904270120150:96:     temporary=$(mktemp "$state_dir/.viewflow-target.XXXXXX")
scripts/viewflow-target.sh.before-discovery-1789254904270120150:97:     printf '%s\n' "$requested" > "$temporary"
scripts/viewflow-target.sh.before-discovery-1789254904270120150:98:     mv -f "$temporary" "$state_dir/viewflow-target"
scripts/viewflow-target.sh.before-discovery-1789254904270120150:99:     cat "$state_dir/hdmi-target"
scripts/viewflow-target.sh.before-discovery-1789254904270120150:100:     ;;
scripts/viewflow-target.sh.before-discovery-1789254904270120150:101:   *) echo "Usage: $0 {status|set windows|set macos}" >&2; exit 2 ;;
scripts/viewflow-target.sh.before-discovery-1789254904270120150:102: esac
```

### `scripts/wifi-action.sh`

Migration owner: `network`. Source `scripts/wifi-action.sh:1`, 83 lines, SHA-256 `6a5d41ec109b215025b62bd12778f39d576dd8b86050d6f7833b4b57f2690e1a`.

```text
…
scripts/wifi-action.sh:5: script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
scripts/wifi-action.sh:6: . "$script_dir/wifi-common.sh"
scripts/wifi-action.sh:7: 
scripts/wifi-action.sh:8: iface="$(detect_wifi_iface || true)"
…
scripts/wifi-action.sh:10: 
scripts/wifi-action.sh:11: run_nmcli_connect() {
scripts/wifi-action.sh:12:     ssid="$1"
scripts/wifi-action.sh:13:     password="$2"
scripts/wifi-action.sh:14: 
scripts/wifi-action.sh:15:     if [ -n "$password" ]; then
scripts/wifi-action.sh:16:         run_nmcli dev wifi connect "$ssid" password "$password" ifname "$iface" >/dev/null
scripts/wifi-action.sh:17:     else
scripts/wifi-action.sh:18:         run_nmcli dev wifi connect "$ssid" ifname "$iface" >/dev/null
scripts/wifi-action.sh:19:     fi
scripts/wifi-action.sh:20: 
scripts/wifi-action.sh:21:     printf 'Connection requested for %s\n' "$ssid"
scripts/wifi-action.sh:22: }
scripts/wifi-action.sh:23: 
scripts/wifi-action.sh:24: case "$command" in
scripts/wifi-action.sh:25:     toggle)
scripts/wifi-action.sh:26:         state="${2:-}"
scripts/wifi-action.sh:27:         if [ "$state" != "on" ] && [ "$state" != "off" ]; then
scripts/wifi-action.sh:28:             printf 'toggle expects "on" or "off"\n' >&2
scripts/wifi-action.sh:29:             exit 2
scripts/wifi-action.sh:30:         fi
scripts/wifi-action.sh:31:         run_nmcli radio wifi "$state" >/dev/null
scripts/wifi-action.sh:32:         if [ "$state" = "on" ]; then
scripts/wifi-action.sh:33:             printf 'Wi-Fi enabled\n'
scripts/wifi-action.sh:34:         else
scripts/wifi-action.sh:35:             printf 'Wi-Fi disabled\n'
scripts/wifi-action.sh:36:         fi
scripts/wifi-action.sh:37:         ;;
scripts/wifi-action.sh:38:     rescan)
scripts/wifi-action.sh:39:         if [ -n "$iface" ]; then
scripts/wifi-action.sh:40:             run_nmcli dev wifi rescan ifname "$iface" >/dev/null
scripts/wifi-action.sh:41:         else
scripts/wifi-action.sh:42:             run_nmcli dev wifi rescan >/dev/null
scripts/wifi-action.sh:43:         fi
scripts/wifi-action.sh:44:         printf 'Scan started\n'
scripts/wifi-action.sh:45:         ;;
scripts/wifi-action.sh:46:     disconnect)
scripts/wifi-action.sh:47:         require_wifi_iface "$iface"
scripts/wifi-action.sh:48:         run_nmcli device disconnect "$iface" >/dev/null
scripts/wifi-action.sh:49:         printf 'Disconnected\n'
scripts/wifi-action.sh:50:         ;;
scripts/wifi-action.sh:51:     connect)
scripts/wifi-action.sh:52:         require_wifi_iface "$iface"
scripts/wifi-action.sh:53: 
scripts/wifi-action.sh:54:         ssid="${2:-}"
…
scripts/wifi-action.sh:57: 
scripts/wifi-action.sh:58:         if [ -z "$ssid" ]; then
scripts/wifi-action.sh:59:             printf 'Missing SSID\n' >&2
scripts/wifi-action.sh:60:             exit 2
scripts/wifi-action.sh:61:         fi
scripts/wifi-action.sh:62: 
scripts/wifi-action.sh:63:         if [ -n "$password" ]; then
scripts/wifi-action.sh:64:             run_nmcli_connect "$ssid" "$password"
scripts/wifi-action.sh:65:             exit 0
scripts/wifi-action.sh:66:         fi
scripts/wifi-action.sh:67: 
scripts/wifi-action.sh:68:         if run_nmcli_connect "$ssid" ""; then
scripts/wifi-action.sh:69:             exit 0
scripts/wifi-action.sh:70:         fi
scripts/wifi-action.sh:71: 
scripts/wifi-action.sh:72:         if printf '%s' "$security" | grep -Eq '802\.1X|EAP'; then
scripts/wifi-action.sh:73:             printf 'This network needs an 802.1X profile. Open the editor for first-time setup.\n' >&2
scripts/wifi-action.sh:74:         else
scripts/wifi-action.sh:75:             printf 'Failed to connect to %s\n' "$ssid" >&2
scripts/wifi-action.sh:76:         fi
scripts/wifi-action.sh:77:         exit 1
scripts/wifi-action.sh:78:         ;;
scripts/wifi-action.sh:79:     *)
scripts/wifi-action.sh:80:         printf 'Unknown Wi-Fi command\n' >&2
scripts/wifi-action.sh:81:         exit 2
scripts/wifi-action.sh:82:         ;;
```

### `scripts/wifi-common.sh`

Migration owner: `network`. Source `scripts/wifi-common.sh:1`, 108 lines, SHA-256 `2fb7381a4011dcabaadee07d9cd261f648232416b9f9ddd6f943405161c88fff`.

```text
…
scripts/wifi-common.sh:2: 
scripts/wifi-common.sh:3: : "${HYPRV_NMCLI_BROKEN_MARKER:=/tmp/hyprv-nmcli-broken}"
scripts/wifi-common.sh:4: : "${HYPRV_NMCLI_RETRY_SECONDS:=20}"
scripts/wifi-common.sh:5: 
scripts/wifi-common.sh:6: mark_nmcli_failed() {
scripts/wifi-common.sh:7:     failed_at="$(date +%s 2>/dev/null || printf '0')"
scripts/wifi-common.sh:8:     printf '%s\n' "$failed_at" > "$HYPRV_NMCLI_BROKEN_MARKER" 2>/dev/null || :
scripts/wifi-common.sh:9: }
scripts/wifi-common.sh:10: 
scripts/wifi-common.sh:11: clear_nmcli_failed() {
scripts/wifi-common.sh:12:     rm -f "$HYPRV_NMCLI_BROKEN_MARKER"
scripts/wifi-common.sh:13: }
scripts/wifi-common.sh:14: 
scripts/wifi-common.sh:15: nmcli_allowed() {
scripts/wifi-common.sh:16:     [ ! -e "$HYPRV_NMCLI_BROKEN_MARKER" ] && return 0
scripts/wifi-common.sh:17: 
scripts/wifi-common.sh:18:     failed_at="$(cat "$HYPRV_NMCLI_BROKEN_MARKER" 2>/dev/null || true)"
scripts/wifi-common.sh:19:     case "$failed_at" in
scripts/wifi-common.sh:20:         ''|*[!0-9]*)
scripts/wifi-common.sh:21:             clear_nmcli_failed
scripts/wifi-common.sh:22:             return 0
scripts/wifi-common.sh:23:             ;;
…
scripts/wifi-common.sh:25: 
scripts/wifi-common.sh:26:     now="$(date +%s 2>/dev/null || printf '0')"
scripts/wifi-common.sh:27:     case "$now" in
scripts/wifi-common.sh:28:         ''|*[!0-9]*)
scripts/wifi-common.sh:29:             return 1
…
scripts/wifi-common.sh:32: 
scripts/wifi-common.sh:33:     if [ $((now - failed_at)) -ge "$HYPRV_NMCLI_RETRY_SECONDS" ]; then
scripts/wifi-common.sh:34:         clear_nmcli_failed
scripts/wifi-common.sh:35:         return 0
scripts/wifi-common.sh:36:     fi
…
scripts/wifi-common.sh:40: 
scripts/wifi-common.sh:41: run_nmcli() {
scripts/wifi-common.sh:42:     if nmcli "$@"; then
scripts/wifi-common.sh:43:         clear_nmcli_failed
scripts/wifi-common.sh:44:         return 0
scripts/wifi-common.sh:45:     fi
scripts/wifi-common.sh:46: 
scripts/wifi-common.sh:47:     mark_nmcli_failed
scripts/wifi-common.sh:48:     return 1
scripts/wifi-common.sh:49: }
scripts/wifi-common.sh:50: 
scripts/wifi-common.sh:51: detect_wifi_iface_nmcli() {
scripts/wifi-common.sh:52:     nmcli_allowed || return 1
scripts/wifi-common.sh:53:     run_nmcli -t -f DEVICE,TYPE dev status 2>/dev/null | awk -F: '$2 == "wifi" { print $1; exit }'
scripts/wifi-common.sh:54: }
scripts/wifi-common.sh:55: 
scripts/wifi-common.sh:56: detect_wifi_iface_sysfs() {
scripts/wifi-common.sh:57:     for path in /sys/class/net/*; do
scripts/wifi-common.sh:58:         [ -d "$path/wireless" ] || continue
scripts/wifi-common.sh:59:         basename "$path"
scripts/wifi-common.sh:60:         return 0
scripts/wifi-common.sh:61:     done
…
scripts/wifi-common.sh:64: 
scripts/wifi-common.sh:65: detect_wifi_iface() {
scripts/wifi-common.sh:66:     iface="$(detect_wifi_iface_nmcli || true)"
scripts/wifi-common.sh:67:     if [ -n "$iface" ]; then
scripts/wifi-common.sh:68:         printf '%s\n' "$iface"
scripts/wifi-common.sh:69:         return 0
scripts/wifi-common.sh:70:     fi
…
scripts/wifi-common.sh:72:     iface="$(detect_wifi_iface_sysfs || true)"
scripts/wifi-common.sh:73:     if [ -n "$iface" ]; then
scripts/wifi-common.sh:74:         printf '%s\n' "$iface"
scripts/wifi-common.sh:75:         return 0
scripts/wifi-common.sh:76:     fi
…
scripts/wifi-common.sh:80: 
scripts/wifi-common.sh:81: wifi_radio_status() {
scripts/wifi-common.sh:82:     if nmcli_allowed; then
scripts/wifi-common.sh:83:         run_nmcli -t -f WIFI,WIFI-HW general status 2>/dev/null || printf 'enabled:enabled'
scripts/wifi-common.sh:84:     else
scripts/wifi-common.sh:85:         printf 'enabled:enabled'
scripts/wifi-common.sh:86:     fi
scripts/wifi-common.sh:87: }
scripts/wifi-common.sh:88: 
scripts/wifi-common.sh:89: saved_wifi_ssids() {
scripts/wifi-common.sh:90:     nmcli_allowed || return 0
scripts/wifi-common.sh:91:     run_nmcli -t -f UUID,TYPE connection show 2>/dev/null \
scripts/wifi-common.sh:92:         | awk -F: '$2 == "802-11-wireless" { print $1 }' \
scripts/wifi-common.sh:93:         | while IFS= read -r uuid; do
scripts/wifi-common.sh:94:             run_nmcli -g 802-11-wireless.ssid connection show "$uuid" 2>/dev/null || true
scripts/wifi-common.sh:95:         done \
scripts/wifi-common.sh:96:         | sed '/^$/d' \
…
scripts/wifi-common.sh:99: 
scripts/wifi-common.sh:100: require_wifi_iface() {
scripts/wifi-common.sh:101:     iface="${1:-}"
scripts/wifi-common.sh:102:     if [ -n "$iface" ]; then
scripts/wifi-common.sh:103:         return 0
scripts/wifi-common.sh:104:     fi
scripts/wifi-common.sh:105: 
scripts/wifi-common.sh:106:     printf 'No Wi-Fi device found\n' >&2
scripts/wifi-common.sh:107:     exit 1
scripts/wifi-common.sh:108: }
```

### `scripts/wifi-status.sh`

Migration owner: `network`. Source `scripts/wifi-status.sh:1`, 182 lines, SHA-256 `8b6b619ba4f005dc87749217ff51b7ada9b0df4dfa306cf4ac6ca0d9ae4f38ae`.

```text
…
scripts/wifi-status.sh:5: script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
scripts/wifi-status.sh:6: . "$script_dir/wifi-common.sh"
scripts/wifi-status.sh:7: 
scripts/wifi-status.sh:8: iface="$(detect_wifi_iface || true)"
…
scripts/wifi-status.sh:10: radio_status="$(wifi_radio_status)"
scripts/wifi-status.sh:11: wifi_enabled="$(printf '%s' "$radio_status" | cut -d: -f1)"
scripts/wifi-status.sh:12: wifi_hw_enabled="$(printf '%s' "$radio_status" | cut -d: -f2)"
scripts/wifi-status.sh:13: 
scripts/wifi-status.sh:14: tmp_networks="$(mktemp)"
…
scripts/wifi-status.sh:17: 
scripts/wifi-status.sh:18: cleanup() {
scripts/wifi-status.sh:19:     rm -f "$tmp_networks" "$tmp_networks_raw" "$tmp_known"
scripts/wifi-status.sh:20: }
scripts/wifi-status.sh:21: 
…
scripts/wifi-status.sh:23: 
scripts/wifi-status.sh:24: fallback_current_network() {
scripts/wifi-status.sh:25:     iw dev "$iface" link 2>/dev/null | awk '
scripts/wifi-status.sh:26:         function trim(text) {
scripts/wifi-status.sh:27:             sub(/^[[:space:]]+/, "", text);
scripts/wifi-status.sh:28:             sub(/[[:space:]]+$/, "", text);
…
scripts/wifi-status.sh:41: 
scripts/wifi-status.sh:42:         /^[[:space:]]*signal:/ {
scripts/wifi-status.sh:43:             dbm = trim(substr($0, index($0, ":") + 1)) + 0;
scripts/wifi-status.sh:44:             signal = int((dbm + 100) * 2);
scripts/wifi-status.sh:45:             if (signal < 0) {
scripts/wifi-status.sh:46:                 signal = 0;
scripts/wifi-status.sh:47:             }
scripts/wifi-status.sh:48:             if (signal > 100) {
scripts/wifi-status.sh:49:                 signal = 100;
scripts/wifi-status.sh:50:             }
scripts/wifi-status.sh:51:             next;
…
scripts/wifi-status.sh:55:             if (connected && ssid != "") {
scripts/wifi-status.sh:56:                 printf "1\t%s\t%d\t\t\n", ssid, signal;
scripts/wifi-status.sh:57:             }
scripts/wifi-status.sh:58:         }
…
scripts/wifi-status.sh:61: 
scripts/wifi-status.sh:62: if [ -n "$iface" ]; then
scripts/wifi-status.sh:63:     if nmcli_allowed \
scripts/wifi-status.sh:64:         && run_nmcli -m multiline -f IN-USE,SSID,SIGNAL,SECURITY,BARS dev wifi list ifname "$iface" --rescan no > "$tmp_networks_raw" 2>/dev/null; then
scripts/wifi-status.sh:65:         awk '
scripts/wifi-status.sh:66:             function trim(text) {
scripts/wifi-status.sh:67:                 sub(/^[[:space:]]+/, "", text);
scripts/wifi-status.sh:68:                 sub(/[[:space:]]+$/, "", text);
…
scripts/wifi-status.sh:71: 
scripts/wifi-status.sh:72:             function emit() {
scripts/wifi-status.sh:73:                 if (!seen || ssid == "" || ssid == "--") {
scripts/wifi-status.sh:74:                     active = "";
scripts/wifi-status.sh:75:                     ssid = "";
scripts/wifi-status.sh:76:                     signal = "0";
scripts/wifi-status.sh:77:                     security = "";
scripts/wifi-status.sh:78:                     bars = "";
…
scripts/wifi-status.sh:85:                 gsub(/\t/, " ", bars);
scripts/wifi-status.sh:86:                 printf "%s\t%s\t%s\t%s\t%s\n", (active == "*" ? "1" : "0"), ssid, signal + 0, security, bars;
scripts/wifi-status.sh:87: 
scripts/wifi-status.sh:88:                 active = "";
scripts/wifi-status.sh:89:                 ssid = "";
scripts/wifi-status.sh:90:                 signal = "0";
scripts/wifi-status.sh:91:                 security = "";
scripts/wifi-status.sh:92:                 bars = "";
…
scripts/wifi-status.sh:96:             /^IN-USE:/ {
scripts/wifi-status.sh:97:                 emit();
scripts/wifi-status.sh:98:                 active = trim(substr($0, index($0, ":") + 1));
scripts/wifi-status.sh:99:                 seen = 1;
…
scripts/wifi-status.sh:107: 
scripts/wifi-status.sh:108:             /^SIGNAL:/ {
scripts/wifi-status.sh:109:                 signal = trim(substr($0, index($0, ":") + 1));
scripts/wifi-status.sh:110:                 next;
scripts/wifi-status.sh:111:             }
…
scripts/wifi-status.sh:123:             END {
scripts/wifi-status.sh:124:                 emit();
scripts/wifi-status.sh:125:             }
scripts/wifi-status.sh:126:         ' < "$tmp_networks_raw" > "$tmp_networks"
scripts/wifi-status.sh:127:         saved_wifi_ssids > "$tmp_known"
scripts/wifi-status.sh:128:     else
scripts/wifi-status.sh:129:         mark_nmcli_failed
scripts/wifi-status.sh:130:         fallback_current_network > "$tmp_networks"
scripts/wifi-status.sh:131:     fi
scripts/wifi-status.sh:132: fi
scripts/wifi-status.sh:133: 
scripts/wifi-status.sh:134: jq -Rn \
scripts/wifi-status.sh:135:     --arg iface "$iface" \
scripts/wifi-status.sh:136:     --arg wifiEnabled "$wifi_enabled" \
scripts/wifi-status.sh:137:     --arg wifiHwEnabled "$wifi_hw_enabled" \
scripts/wifi-status.sh:138:     --rawfile networks "$tmp_networks" \
scripts/wifi-status.sh:139:     --rawfile known "$tmp_known" '
scripts/wifi-status.sh:140:     def known_ssids:
scripts/wifi-status.sh:141:         $known
scripts/wifi-status.sh:142:         | split("\n")
scripts/wifi-status.sh:143:         | map(select(length > 0));
scripts/wifi-status.sh:144: 
scripts/wifi-status.sh:145:     def parsed_networks:
scripts/wifi-status.sh:146:         $networks
scripts/wifi-status.sh:147:         | split("\n")
scripts/wifi-status.sh:148:         | map(select(length > 0) | split("\t"))
scripts/wifi-status.sh:149:         | map({
scripts/wifi-status.sh:150:             active: (.[0] == "1"),
scripts/wifi-status.sh:151:             ssid: .[1],
scripts/wifi-status.sh:152:             signal: (.[2] | tonumber),
scripts/wifi-status.sh:153:             security: (if .[3] == "--" then "" else .[3] end),
scripts/wifi-status.sh:154:             bars: (.[4] // "")
scripts/wifi-status.sh:155:         })
scripts/wifi-status.sh:156:         | map(. as $network | . + {
scripts/wifi-status.sh:157:             secure: ($network.security | length > 0),
scripts/wifi-status.sh:158:             known: (known_ssids | index($network.ssid) != null),
scripts/wifi-status.sh:159:             enterprise: (($network.security | contains("802.1X")) or ($network.security | contains("EAP")))
scripts/wifi-status.sh:160:         })
scripts/wifi-status.sh:161:         | sort_by(.ssid)
scripts/wifi-status.sh:162:         | group_by(.ssid)
scripts/wifi-status.sh:163:         | map(sort_by([if .active then 0 else 1 end, -(.signal)]) | .[0])
scripts/wifi-status.sh:164:         | sort_by([if .active then 0 else 1 end, -(.signal), .ssid]);
scripts/wifi-status.sh:165: 
scripts/wifi-status.sh:166:     def active_network:
scripts/wifi-status.sh:167:         parsed_networks
scripts/wifi-status.sh:168:         | map(select(.active))
…
scripts/wifi-status.sh:171:     {
scripts/wifi-status.sh:172:         present: ($iface | length > 0),
scripts/wifi-status.sh:173:         iface: $iface,
scripts/wifi-status.sh:174:         enabled: ($wifiEnabled == "enabled"),
scripts/wifi-status.sh:175:         hardwareEnabled: ($wifiHwEnabled == "enabled"),
scripts/wifi-status.sh:176:         connected: (active_network != null),
scripts/wifi-status.sh:177:         ssid: (active_network.ssid // ""),
scripts/wifi-status.sh:178:         signal: (active_network.signal // 0),
scripts/wifi-status.sh:179:         security: (active_network.security // ""),
scripts/wifi-status.sh:180:         networks: parsed_networks
```

### `scripts/windows-vm-control.sh`

Migration owner: `vm/systemd`. Source `scripts/windows-vm-control.sh:1`, 24 lines, SHA-256 `b2f8d077a52dd57da26458b72278e5d58d1bd5883192b26e1355295864183dcf`.

```text
scripts/windows-vm-control.sh:1: #!/usr/bin/env bash
scripts/windows-vm-control.sh:2: set -euo pipefail
scripts/windows-vm-control.sh:3: 
scripts/windows-vm-control.sh:4: readonly unit=windows-autoshop.service
scripts/windows-vm-control.sh:5: 
scripts/windows-vm-control.sh:6: case "${1:-}" in
scripts/windows-vm-control.sh:7:   status)
scripts/windows-vm-control.sh:8:     if systemctl is-active --quiet "$unit"; then
scripts/windows-vm-control.sh:9:       printf '%s\n' running
scripts/windows-vm-control.sh:10:     else
scripts/windows-vm-control.sh:11:       printf '%s\n' stopped
scripts/windows-vm-control.sh:12:     fi
scripts/windows-vm-control.sh:13:     ;;
scripts/windows-vm-control.sh:14:   start)
scripts/windows-vm-control.sh:15:     exec pkexec /usr/bin/systemctl start "$unit"
scripts/windows-vm-control.sh:16:     ;;
scripts/windows-vm-control.sh:17:   hibernate)
scripts/windows-vm-control.sh:18:     exec pkexec /usr/bin/systemctl stop "$unit"
scripts/windows-vm-control.sh:19:     ;;
scripts/windows-vm-control.sh:20:   *)
scripts/windows-vm-control.sh:21:     printf 'Usage: %s {status|start|hibernate}\n' "$0" >&2
scripts/windows-vm-control.sh:22:     exit 2
scripts/windows-vm-control.sh:23:     ;;
scripts/windows-vm-control.sh:24: esac
```

### `tests/fixtures/quickshell/scripts/windows-vm-control.sh`

Migration owner: `vm/systemd`. Source `tests/fixtures/quickshell/scripts/windows-vm-control.sh:1`, 8 lines, SHA-256 `81b92f4f0983b316ef1bae3ac97922d60faa2efba1c04b6ff4335f775046becd`.

```text
tests/fixtures/quickshell/scripts/windows-vm-control.sh:1: #!/usr/bin/env bash
tests/fixtures/quickshell/scripts/windows-vm-control.sh:2: # Simulate waiting for authentication without touching a real VM or polkit.
tests/fixtures/quickshell/scripts/windows-vm-control.sh:3: case "$1" in
tests/fixtures/quickshell/scripts/windows-vm-control.sh:4:   status) printf '%s\n' stopped ;;
tests/fixtures/quickshell/scripts/windows-vm-control.sh:5:   start) sleep 0.2 ;;
tests/fixtures/quickshell/scripts/windows-vm-control.sh:6:   hibernate) sleep 0.2; exit 126 ;;
tests/fixtures/quickshell/scripts/windows-vm-control.sh:7:   *) exit 2 ;;
tests/fixtures/quickshell/scripts/windows-vm-control.sh:8: esac
```

### `BarMotion.qml`

Migration owner: `UI/provider integration (domain sections above)`. Source `BarMotion.qml:1`, 49 lines, SHA-256 `dc4e3367ed81b02e043490872b5b44c691e19f6714809ab72003309a922fe135`.

```text
BarMotion.qml:1: import QtQuick
BarMotion.qml:2: import Quickshell.Io
BarMotion.qml:3: 
BarMotion.qml:4: // Read effective windowsMove settings at startup and on config reload, without
BarMotion.qml:5: // polling. Unset leaves inherit windows/global, just as in Hyprland's tree.
BarMotion.qml:6: Item {
BarMotion.qml:7:     id: motion
BarMotion.qml:8:     property int duration: 700
BarMotion.qml:9:     property var bezierCurve: [0.1, 0.9, 0.1, 1.05, 1, 1]
BarMotion.qml:10:     property string curveName: "myBezier"
BarMotion.qml:11:     property bool refreshQueued: false
BarMotion.qml:12: 
BarMotion.qml:13:     function refresh() {
BarMotion.qml:14:         if (query.running) refreshQueued = true;
BarMotion.qml:15:         else query.running = true;
BarMotion.qml:16:     }
BarMotion.qml:17:     function applySnapshot(text) {
BarMotion.qml:18:         try {
BarMotion.qml:19:             const snapshot = JSON.parse(text);
BarMotion.qml:20:             const animations = snapshot[0];
BarMotion.qml:21:             const curves = snapshot[1];
BarMotion.qml:22:             const config = ["windowsMove", "windows", "global"]
BarMotion.qml:23:                 .map(name => animations.find(animation => animation.name === name))
BarMotion.qml:24:                 .find(animation => animation && animation.overridden);
BarMotion.qml:25:             if (!config) return;
BarMotion.qml:26:             const curve = curves.find(curve => curve.name === config.bezier);
BarMotion.qml:27:             if (!curve) return;
BarMotion.qml:28:             duration = config.enabled ? Math.max(0, Math.round(config.speed * 100)) : 0;
BarMotion.qml:29:             curveName = config.bezier;
BarMotion.qml:30:             bezierCurve = [curve.X0, curve.Y0, curve.X1, curve.Y1, 1, 1];
BarMotion.qml:31:         } catch (error) {
BarMotion.qml:32:             console.warn("Cannot read bar motion settings:", error);
BarMotion.qml:33:         }
BarMotion.qml:34:     }
BarMotion.qml:35:     Process {
BarMotion.qml:36:         id: query
BarMotion.qml:37:         running: true
BarMotion.qml:38:         command: ["hyprctl", "-j", "animations"]
BarMotion.qml:39:         stdout: StdioCollector { id: snapshot }
BarMotion.qml:40:         stderr: StdioCollector {}
BarMotion.qml:41:         onExited: function(exitCode) {
BarMotion.qml:42:             if (exitCode === 0) motion.applySnapshot(snapshot.text);
BarMotion.qml:43:             if (motion.refreshQueued) {
BarMotion.qml:44:                 motion.refreshQueued = false;
BarMotion.qml:45:                 query.running = true;
BarMotion.qml:46:             }
BarMotion.qml:47:         }
BarMotion.qml:48:     }
BarMotion.qml:49: }
```

### `BarServices.qml`

Migration owner: `UI/provider integration (domain sections above)`. Source `BarServices.qml:1`, 221 lines, SHA-256 `44af3919cc9e21e2b581439b00f6daa70e08bb45a5cc5df74032c5c35a6208e6`.

```text
…
BarServices.qml:23:         const networks = device ? device.networks.values.map(network => ({
BarServices.qml:24:             active: network.connected,
BarServices.qml:25:             ssid: network.name,
BarServices.qml:26:             signal: Math.round(network.signalStrength * 100),
BarServices.qml:27:             security: network.security === WifiSecurityType.Open ? "" : WifiSecurityType.toString(network.security),
BarServices.qml:28:             secure: network.security !== WifiSecurityType.Open,
BarServices.qml:29:             known: network.known,
BarServices.qml:30:             enterprise: [WifiSecurityType.Wpa2Eap, WifiSecurityType.WpaEap,
…
BarServices.qml:33:             bars: ""
BarServices.qml:34:         })).sort((a, b) => Number(b.active) - Number(a.active) || b.signal - a.signal) : [];
BarServices.qml:35:         const active = networks.find(network => network.active);
BarServices.qml:36:         return {
…
BarServices.qml:39:             connected: !!active, ssid: active ? active.ssid : "",
BarServices.qml:40:             signal: active ? active.signal : 0, security: active ? active.security : "",
BarServices.qml:41:             networks: networks
BarServices.qml:42:         };
…
BarServices.qml:44:     onWifiSnapshotChanged: syncWifi()
BarServices.qml:45:     function syncWifi() {
BarServices.qml:46:         if (nativeWifi) {
BarServices.qml:47:             shellRoot.applyWifiStatus(wifiSnapshot);
…
BarServices.qml:50:     Binding {
BarServices.qml:51:         target: services.wifiDevice
BarServices.qml:52:         property: "scannerEnabled"
BarServices.qml:53:         value: services.wifiPageVisible
…
BarServices.qml:57:     readonly property var audioNode: Pipewire.defaultAudioSink
BarServices.qml:58:     readonly property bool audioAvailable: Pipewire.ready && !!audioNode && audioNode.ready && !!audioNode.audio
BarServices.qml:59:     readonly property bool audioMuted: audioAvailable && audioNode.audio.muted
BarServices.qml:60:     readonly property int audioVolume: audioAvailable ? Math.round(audioNode.audio.volume * 100) : 0
…
BarServices.qml:64:     onAudioVolumeChanged: syncAudio()
BarServices.qml:65:     function syncAudio() {
BarServices.qml:66:         shellRoot.audioAvailable = audioAvailable;
BarServices.qml:67:         shellRoot.audioMuted = audioMuted;
…
BarServices.qml:70: 
BarServices.qml:71:     readonly property string powerProfile: PowerProfiles.profile === PowerProfile.Performance
BarServices.qml:72:         ? "performance" : PowerProfiles.profile === PowerProfile.PowerSaver ? "power-saver" : "balanced"
BarServices.qml:73:     onPowerProfileChanged: syncPowerProfile()
BarServices.qml:74:     function syncPowerProfile() {
BarServices.qml:75:         shellRoot.powerProfile = powerProfile;
BarServices.qml:76:         shellRoot.powerProfileText = powerProfile === "performance" ? "⚡" : powerProfile === "power-saver" ? "🔋" : "⚖️";
…
BarServices.qml:79:     readonly property var battery: UPower.displayDevice
BarServices.qml:80:     readonly property bool batteryPresent: !!battery && battery.ready && battery.isPresent && battery.isLaptopBattery
BarServices.qml:81:     readonly property real batteryPercent: batteryPresent ? battery.percentage * 100 : 0
BarServices.qml:82:     readonly property string batteryMode: !batteryPresent ? "unknown"
…
BarServices.qml:84:         : battery.state === UPowerDeviceState.Discharging || battery.state === UPowerDeviceState.PendingDischarge ? "discharging"
BarServices.qml:85:         : battery.state === UPowerDeviceState.FullyCharged ? "full" : UPower.onBattery ? "idle" : "plugged"
BarServices.qml:86: 
BarServices.qml:87:     readonly property var player: {
…
BarServices.qml:102:     }
BarServices.qml:103:     readonly property string playerName: player ? player.dbusName.replace(/^org\.mpris\.MediaPlayer2\./, "") : ""
BarServices.qml:104:     // Metadata is event driven. Position is read from Quickshell's local clock.
BarServices.qml:105:     readonly property string mediaSnapshot: JSON.stringify(player ? {
BarServices.qml:106:         name: playerName, title: player.trackTitle, artist: player.trackArtist,
BarServices.qml:107:         art: player.trackArtUrl, album: player.trackAlbum, length: player.lengthSupported ? player.length : 0,
…
BarServices.qml:110:     onMediaSnapshotChanged: {
BarServices.qml:111:         syncMedia();
BarServices.qml:112:         compatibilityRefresh.restart();
BarServices.qml:113:     }
…
BarServices.qml:115:     property string correctedMediaOutput: ""
BarServices.qml:116:     function syncMedia() {
BarServices.qml:117:         if (!player) {
BarServices.qml:118:             shellRoot.resetMediaState();
…
BarServices.qml:125:             shellRoot.updateMediaState(correctedMediaOutput);
BarServices.qml:126:             syncPosition();
BarServices.qml:127:             return;
BarServices.qml:128:         }
BarServices.qml:129:         const snapshot = JSON.parse(mediaSnapshot);
BarServices.qml:130:         shellRoot.updateMediaState([
BarServices.qml:131:             "available=true", "playing=" + snapshot.playing, "player=" + snapshot.name,
…
BarServices.qml:136:     }
BarServices.qml:137:     function syncPosition() {
BarServices.qml:138:         if (player && player.positionSupported) shellRoot.mediaPositionSeconds = Math.max(0, player.position);
BarServices.qml:139:     }
BarServices.qml:140:     Connections {
BarServices.qml:141:         target: services.player
BarServices.qml:142:         function onPositionChanged() { services.syncPosition(); }
BarServices.qml:143:     }
BarServices.qml:144:     Timer {
BarServices.qml:145:         interval: 1000
BarServices.qml:146:         repeat: true
BarServices.qml:147:         running: services.mediaVisible && !!services.player && services.player.isPlaying
BarServices.qml:148:         onTriggered: services.syncPosition()
BarServices.qml:149:     }
BarServices.qml:150:     onMediaVisibleChanged: if (mediaVisible) {
BarServices.qml:151:         syncPosition();
BarServices.qml:152:         compatibilityRefresh.restart();
BarServices.qml:153:     }
…
BarServices.qml:160:     property bool compatibilityQueued: false
BarServices.qml:161:     function refreshCompatibility() {
BarServices.qml:162:         if (!player || !/chromium|cider/i.test(playerName + " " + player.identity)) return;
BarServices.qml:163:         if (compatibilityProcess.running) {
…
BarServices.qml:168:         compatibilityPlayer = playerName;
BarServices.qml:169:         compatibilityProcess.command = [shellRoot.configDir + "/quickshell/scripts/media-status.sh", "--player", playerName];
BarServices.qml:170:         compatibilityProcess.running = true;
BarServices.qml:171:     }
BarServices.qml:172:     Timer {
BarServices.qml:173:         id: compatibilityRefresh
BarServices.qml:174:         interval: 200
BarServices.qml:175:         onTriggered: services.refreshCompatibility()
BarServices.qml:176:     }
BarServices.qml:177:     Timer {
BarServices.qml:178:         interval: 30000
BarServices.qml:179:         repeat: true
BarServices.qml:180:         running: services.mediaVisible && services.playerName === services.ciderPlayer && services.ciderPlayer.length > 0
BarServices.qml:181:             && !!services.player && services.player.isPlaying
BarServices.qml:182:         onTriggered: services.refreshCompatibility()
…
BarServices.qml:207:         id: themeState
BarServices.qml:208:         path: (Quickshell.env("XDG_STATE_HOME") || services.shellRoot.homeDir + "/.local/state") + "/hyprv/quickshell/ui-state.env"
BarServices.qml:209:         watchChanges: true
BarServices.qml:210:         onFileChanged: reload()
BarServices.qml:211:         onLoaded: services.shellRoot.darkMode = /^theme=dark$/m.test(text())
BarServices.qml:212:     }
BarServices.qml:213:     function refreshTheme() { themeState.reload(); }
BarServices.qml:214: 
BarServices.qml:215:     Component.onCompleted: {
BarServices.qml:216:         syncAudio();
BarServices.qml:217:         syncPowerProfile();
BarServices.qml:218:         syncMedia();
BarServices.qml:219:         syncWifi();
BarServices.qml:220:     }
BarServices.qml:221: }
```

### `CalendarBridge.qml`

Migration owner: `UI/provider integration (domain sections above)`. Source `CalendarBridge.qml:1`, 242 lines, SHA-256 `047a9893620f02bcb1a2558c9949bcebd4697fe707c8a0cfaf541ed11ad426e7`.

```text
…
CalendarBridge.qml:7: 
CalendarBridge.qml:8:     property string dcalPath: (Quickshell.env("HOME") || "") + "/.local/bin/dcal"
CalendarBridge.qml:9:     property string socketPath: ""
CalendarBridge.qml:10:     property bool connected: false
CalendarBridge.qml:11:     property bool subscribed: false
CalendarBridge.qml:12:     property bool loading: false
CalendarBridge.qml:13:     property int weekRequestSerial: 0
…
CalendarBridge.qml:20: 
CalendarBridge.qml:21:     signal childWindowRequested()
CalendarBridge.qml:22: 
CalendarBridge.qml:23:     Component.onCompleted: discoverSocket()
CalendarBridge.qml:24: 
CalendarBridge.qml:25:     function startOfWeek(value) {
CalendarBridge.qml:26:         const d = new Date(value);
CalendarBridge.qml:27:         d.setHours(0, 0, 0, 0);
…
CalendarBridge.qml:32: 
CalendarBridge.qml:33:     function discoverSocket() {
CalendarBridge.qml:34:         if (!socketDiscovery.running)
CalendarBridge.qml:35:             socketDiscovery.running = true;
CalendarBridge.qml:36:     }
CalendarBridge.qml:37: 
CalendarBridge.qml:38:     function nextId() {
CalendarBridge.qml:39:         requestCounter++;
CalendarBridge.qml:40:         return Date.now() + requestCounter;
…
CalendarBridge.qml:42: 
CalendarBridge.qml:43:     function sendRequest(method, params, callback) {
CalendarBridge.qml:44:         if (!requestSocket.linkUp) {
CalendarBridge.qml:45:             lastError = "日历服务未连接";
CalendarBridge.qml:46:             if (callback)
CalendarBridge.qml:47:                 callback({"error": lastError});
CalendarBridge.qml:48:             return;
CalendarBridge.qml:49:         }
…
CalendarBridge.qml:52:             pendingRequests[id] = callback;
CalendarBridge.qml:53:         requestSocket.send({"id": id, "method": method, "params": params || {}});
CalendarBridge.qml:54:     }
CalendarBridge.qml:55: 
CalendarBridge.qml:56:     function handleResponse(line) {
CalendarBridge.qml:57:         if (!line || line.length === 0)
CalendarBridge.qml:58:             return;
…
CalendarBridge.qml:60:         try {
CalendarBridge.qml:61:             message = JSON.parse(line);
CalendarBridge.qml:62:         } catch (e) {
CalendarBridge.qml:63:             return;
…
CalendarBridge.qml:74:         delete pendingRequests[message.id];
CalendarBridge.qml:75:         callback(message.error ? {"error": message.error} : {"result": message.result});
CalendarBridge.qml:76:     }
CalendarBridge.qml:77: 
CalendarBridge.qml:78:     function handleSubscription(message) {
CalendarBridge.qml:79:         switch (message.event) {
CalendarBridge.qml:80:         case "accounts":
CalendarBridge.qml:81:         case "calendars":
CalendarBridge.qml:82:             refreshCalendars();
CalendarBridge.qml:83:             refreshDebounce.restart();
CalendarBridge.qml:84:             break;
CalendarBridge.qml:85:         case "events":
CalendarBridge.qml:86:         case "sync":
CalendarBridge.qml:87:             refreshDebounce.restart();
CalendarBridge.qml:88:             break;
…
CalendarBridge.qml:91: 
CalendarBridge.qml:92:     function refreshCalendars() {
CalendarBridge.qml:93:         sendRequest("calendars.list", {}, response => {
CalendarBridge.qml:94:             if (response.error) {
…
CalendarBridge.qml:101: 
CalendarBridge.qml:102:     function loadWeek(value) {
CalendarBridge.qml:103:         weekStart = startOfWeek(value);
CalendarBridge.qml:104:         const requestSerial = ++weekRequestSerial;
…
CalendarBridge.qml:112:         sendRequest("events.list", {
CalendarBridge.qml:113:             "from": from.toISOString(),
CalendarBridge.qml:114:             "to": to.toISOString(),
CalendarBridge.qml:115:             "limit": 2000
CalendarBridge.qml:116:         }, response => {
CalendarBridge.qml:117:             if (requestSerial !== weekRequestSerial)
…
CalendarBridge.qml:129: 
CalendarBridge.qml:130:     function previousWeek() {
CalendarBridge.qml:131:         const d = new Date(weekStart);
CalendarBridge.qml:132:         d.setDate(d.getDate() - 7);
…
CalendarBridge.qml:135: 
CalendarBridge.qml:136:     function nextWeek() {
CalendarBridge.qml:137:         const d = new Date(weekStart);
CalendarBridge.qml:138:         d.setDate(d.getDate() + 7);
…
CalendarBridge.qml:141: 
CalendarBridge.qml:142:     function today() {
CalendarBridge.qml:143:         loadWeek(new Date());
CalendarBridge.qml:144:     }
CalendarBridge.qml:145: 
CalendarBridge.qml:146:     function openEvent(event) {
CalendarBridge.qml:147:         if (!event || !event.uid)
CalendarBridge.qml:148:             return;
CalendarBridge.qml:149:         childWindowRequested();
CalendarBridge.qml:150:         sendRequest("ui.openEvent", {
CalendarBridge.qml:151:             "uid": event.uid,
CalendarBridge.qml:152:             "start": event.start || "",
CalendarBridge.qml:153:             "editorOnly": true
CalendarBridge.qml:154:         });
CalendarBridge.qml:155:     }
CalendarBridge.qml:156: 
CalendarBridge.qml:157:     function createEvent() {
CalendarBridge.qml:158:         childWindowRequested();
CalendarBridge.qml:159:         sendRequest("ui.newEvent", {"editorOnly": true});
CalendarBridge.qml:160:     }
CalendarBridge.qml:161: 
CalendarBridge.qml:162:     function showFullApp() {
CalendarBridge.qml:163:         sendRequest("ui.show", {"view": "week"});
CalendarBridge.qml:164:     }
CalendarBridge.qml:165: 
CalendarBridge.qml:166:     Process {
CalendarBridge.qml:167:         id: socketDiscovery
CalendarBridge.qml:168:         command: [root.dcalPath, "--json", "socket"]
CalendarBridge.qml:169:         running: false
CalendarBridge.qml:170:         stdout: StdioCollector {
CalendarBridge.qml:171:             onStreamFinished: {
CalendarBridge.qml:172:                 try {
CalendarBridge.qml:173:                     const result = JSON.parse(text);
CalendarBridge.qml:174:                     if (!result.path)
CalendarBridge.qml:175:                         throw new Error("missing socket path");
CalendarBridge.qml:176:                     root.socketPath = result.path;
CalendarBridge.qml:177:                     requestSocket.connected = true;
CalendarBridge.qml:178:                 } catch (e) {
CalendarBridge.qml:179:                     root.lastError = "无法连接日历服务";
…
CalendarBridge.qml:189: 
CalendarBridge.qml:190:     Timer {
CalendarBridge.qml:191:         id: discoveryRetry
CalendarBridge.qml:192:         interval: 1800
CalendarBridge.qml:193:         repeat: false
CalendarBridge.qml:194:         onTriggered: root.discoverSocket()
CalendarBridge.qml:195:     }
CalendarBridge.qml:196: 
CalendarBridge.qml:197:     Timer {
CalendarBridge.qml:198:         id: refreshDebounce
CalendarBridge.qml:199:         interval: 350
CalendarBridge.qml:200:         repeat: false
CalendarBridge.qml:201:         onTriggered: root.loadWeek(root.weekStart)
…
CalendarBridge.qml:203: 
CalendarBridge.qml:204:     CalendarSocket {
CalendarBridge.qml:205:         id: requestSocket
CalendarBridge.qml:206:         path: root.socketPath
CalendarBridge.qml:207:         connected: false
CalendarBridge.qml:208:         parser: SplitParser {
…
CalendarBridge.qml:213:             if (linkUp) {
CalendarBridge.qml:214:                 subscribeSocket.connected = true;
CalendarBridge.qml:215:                 root.refreshCalendars();
CalendarBridge.qml:216:                 root.loadWeek(root.weekStart);
CalendarBridge.qml:217:             } else {
CalendarBridge.qml:218:                 root.subscribed = false;
CalendarBridge.qml:219:                 root.pendingRequests = ({});
CalendarBridge.qml:220:             }
…
CalendarBridge.qml:223: 
CalendarBridge.qml:224:     CalendarSocket {
CalendarBridge.qml:225:         id: subscribeSocket
CalendarBridge.qml:226:         path: root.socketPath
CalendarBridge.qml:227:         connected: false
CalendarBridge.qml:228:         parser: SplitParser {
…
CalendarBridge.qml:231:         onConnectionStateChanged: {
CalendarBridge.qml:232:             root.subscribed = linkUp;
CalendarBridge.qml:233:             if (linkUp) {
CalendarBridge.qml:234:                 send({
CalendarBridge.qml:235:                     "id": root.nextId(),
CalendarBridge.qml:236:                     "method": "subscribe",
CalendarBridge.qml:237:                     "params": {"topics": ["accounts", "calendars", "events", "sync"]}
CalendarBridge.qml:238:                 });
CalendarBridge.qml:239:             }
```

### `CalendarSocket.qml`

Migration owner: `UI/provider integration (domain sections above)`. Source `CalendarSocket.qml:1`, 52 lines, SHA-256 `02d05a77d2a9d1acbbbecc98b9c62658e7cdfbfc9e1acb9cdf90d1c0d729664a`.

```text
CalendarSocket.qml:1: import QtQuick
CalendarSocket.qml:2: import Quickshell.Io
CalendarSocket.qml:3: 
CalendarSocket.qml:4: Item {
CalendarSocket.qml:5:     id: root
CalendarSocket.qml:6: 
CalendarSocket.qml:7:     property alias path: socket.path
CalendarSocket.qml:8:     property alias parser: socket.parser
CalendarSocket.qml:9:     property bool connected: false
CalendarSocket.qml:10:     property bool linkUp: false
CalendarSocket.qml:11:     property int reconnectAttempt: 0
CalendarSocket.qml:12: 
CalendarSocket.qml:13:     signal connectionStateChanged()
CalendarSocket.qml:14: 
CalendarSocket.qml:15:     onConnectedChanged: socket.connected = connected
CalendarSocket.qml:16: 
CalendarSocket.qml:17:     function send(data) {
CalendarSocket.qml:18:         const json = typeof data === "string" ? data : JSON.stringify(data);
CalendarSocket.qml:19:         socket.write(json.endsWith("\n") ? json : json + "\n");
CalendarSocket.qml:20:         socket.flush();
CalendarSocket.qml:21:     }
CalendarSocket.qml:22: 
CalendarSocket.qml:23:     function scheduleReconnect() {
CalendarSocket.qml:24:         const base = Math.min(400 * Math.pow(2, Math.min(reconnectAttempt, 7)), 12000);
CalendarSocket.qml:25:         reconnectTimer.interval = base + Math.floor(Math.random() * Math.max(1, base / 4));
CalendarSocket.qml:26:         reconnectAttempt++;
CalendarSocket.qml:27:         reconnectTimer.restart();
CalendarSocket.qml:28:     }
CalendarSocket.qml:29: 
CalendarSocket.qml:30:     Socket {
CalendarSocket.qml:31:         id: socket
CalendarSocket.qml:32: 
CalendarSocket.qml:33:         onConnectionStateChanged: {
CalendarSocket.qml:34:             root.linkUp = connected;
CalendarSocket.qml:35:             root.connectionStateChanged();
CalendarSocket.qml:36:             if (connected) {
CalendarSocket.qml:37:                 root.reconnectAttempt = 0;
CalendarSocket.qml:38:             } else if (root.connected) {
CalendarSocket.qml:39:                 root.scheduleReconnect();
CalendarSocket.qml:40:             }
CalendarSocket.qml:41:         }
CalendarSocket.qml:42:     }
CalendarSocket.qml:43: 
CalendarSocket.qml:44:     Timer {
CalendarSocket.qml:45:         id: reconnectTimer
CalendarSocket.qml:46:         repeat: false
CalendarSocket.qml:47:         onTriggered: {
CalendarSocket.qml:48:             socket.connected = false;
CalendarSocket.qml:49:             Qt.callLater(() => socket.connected = true);
CalendarSocket.qml:50:         }
CalendarSocket.qml:51:     }
CalendarSocket.qml:52: }
```

### `ControlPanelPopup.qml`

Migration owner: `UI/provider integration (domain sections above)`. Source `ControlPanelPopup.qml:1`, 922 lines, SHA-256 `cf8646b86e1dc4d595631f3423be264fb0bc3a157378dedf64237b8591e36b12`.

```text
…
ControlPanelPopup.qml:42:         shellRoot: popupRoot.shellRoot
ControlPanelPopup.qml:43:         pollingActive: popupRoot.popupRequested
ControlPanelPopup.qml:44:         onActionStarting: {
ControlPanelPopup.qml:45:             popupRoot.popupRequested = false;
…
ControlPanelPopup.qml:50: 
ControlPanelPopup.qml:51:     function resetExpandedState() {
ControlPanelPopup.qml:52:         currentPage = "main";
ControlPanelPopup.qml:53:         pendingPage = "";
…
ControlPanelPopup.qml:57: 
ControlPanelPopup.qml:58:     function openFor(source, window) {
ControlPanelPopup.qml:59:         if (!source || !window) {
ControlPanelPopup.qml:60:             return;
…
ControlPanelPopup.qml:65:         animatingClose = false;
ControlPanelPopup.qml:66:         resetExpandedState();
ControlPanelPopup.qml:67:         positionTimer.restart();
ControlPanelPopup.qml:68:         if (popupWindow.visible) {
ControlPanelPopup.qml:69:             popupCard.prepareOpenAnimation();
ControlPanelPopup.qml:70:             openAnimationPending = true;
ControlPanelPopup.qml:71:             popupOpenTimer.restart();
ControlPanelPopup.qml:72:             popupWindow.updatePopupPosition();
ControlPanelPopup.qml:73:         } else {
…
ControlPanelPopup.qml:77: 
ControlPanelPopup.qml:78:     function closePopup() {
ControlPanelPopup.qml:79:         if ((!popupRequested && !animatingClose) || !popupWindow.visible) {
ControlPanelPopup.qml:80:             popupRequested = false;
…
ControlPanelPopup.qml:93: 
ControlPanelPopup.qml:94:     function animateCurrentPageOpen() {
ControlPanelPopup.qml:95:         if (!popupWindow.visible || animatingClose) {
ControlPanelPopup.qml:96:             return;
…
ControlPanelPopup.qml:99:         popupCard.prepareOpenAnimation();
ControlPanelPopup.qml:100:         positionTimer.restart();
ControlPanelPopup.qml:101:         popupOpenTimer.restart();
ControlPanelPopup.qml:102:     }
ControlPanelPopup.qml:103: 
ControlPanelPopup.qml:104:     function switchPageWithAnimation(targetPage) {
ControlPanelPopup.qml:105:         if (!targetPage || currentPage === targetPage) {
ControlPanelPopup.qml:106:             return;
…
ControlPanelPopup.qml:113:         openAnimationPending = false;
ControlPanelPopup.qml:114:         popupOpenTimer.stop();
ControlPanelPopup.qml:115:         popupCard.stopAnimations();
ControlPanelPopup.qml:116:         popupCard.playCloseAnimation();
…
ControlPanelPopup.qml:118: 
ControlPanelPopup.qml:119:     function toggleFor(source, window) {
ControlPanelPopup.qml:120:         if (popupWindow.visible && sourceItem === source && parentWindow === window) {
ControlPanelPopup.qml:121:             closePopup();
ControlPanelPopup.qml:122:             return;
ControlPanelPopup.qml:123:         }
…
ControlPanelPopup.qml:126: 
ControlPanelPopup.qml:127:     function toggleCentered(window) {
ControlPanelPopup.qml:128:         if (popupWindow.visible) {
ControlPanelPopup.qml:129:             closePopup();
ControlPanelPopup.qml:130:             return;
ControlPanelPopup.qml:131:         }
…
ControlPanelPopup.qml:138:         animatingClose = false;
ControlPanelPopup.qml:139:         resetExpandedState();
ControlPanelPopup.qml:140:         popupWindow.visible = true;
ControlPanelPopup.qml:141:     }
ControlPanelPopup.qml:142: 
ControlPanelPopup.qml:143:     function runAndRefresh(command) {
ControlPanelPopup.qml:144:         if (!shellRoot) {
ControlPanelPopup.qml:145:             return;
…
ControlPanelPopup.qml:147:         shellRoot.runDetached(command);
ControlPanelPopup.qml:148:         controlPanelRefreshTimer.restart();
ControlPanelPopup.qml:149:     }
ControlPanelPopup.qml:150: 
ControlPanelPopup.qml:151:     function clampPercent(value) {
ControlPanelPopup.qml:152:         return Math.max(0, Math.min(100, Math.round(value)));
ControlPanelPopup.qml:153:     }
ControlPanelPopup.qml:154: 
ControlPanelPopup.qml:155:     function setBrightnessPercent(value) {
ControlPanelPopup.qml:156:         if (!shellRoot) {
ControlPanelPopup.qml:157:             return;
…
ControlPanelPopup.qml:161: 
ControlPanelPopup.qml:162:     function setAudioVolumePercent(value) {
ControlPanelPopup.qml:163:         const nextValue = clampPercent(value);
ControlPanelPopup.qml:164:         if (shellRoot) {
…
ControlPanelPopup.qml:167:         if (shellRoot) {
ControlPanelPopup.qml:168:             shellRoot.runDetached(["hyprctl", "eval", 'require("hypr.controls").volume("set",' + nextValue + ')']);
ControlPanelPopup.qml:169:             shellRoot.scheduleAudioRefresh();
ControlPanelPopup.qml:170:         }
…
ControlPanelPopup.qml:172: 
ControlPanelPopup.qml:173:     function setFanPercent(channel, value) {
ControlPanelPopup.qml:174:         if (!shellRoot) {
ControlPanelPopup.qml:175:             return;
…
ControlPanelPopup.qml:179: 
ControlPanelPopup.qml:180:     function toggleAudioMute() {
ControlPanelPopup.qml:181:         if (!shellRoot) {
ControlPanelPopup.qml:182:             return;
…
ControlPanelPopup.qml:185:         shellRoot.audioMuted = !shellRoot.audioMuted;
ControlPanelPopup.qml:186:         shellRoot.runDetached(["hyprctl", "eval", 'require("hypr.controls").volume("toggle")']);
ControlPanelPopup.qml:187:         shellRoot.scheduleAudioRefresh();
ControlPanelPopup.qml:188:     }
ControlPanelPopup.qml:189: 
ControlPanelPopup.qml:190:     function toggleWifiEnabled() {
ControlPanelPopup.qml:191:         if (!shellRoot) {
ControlPanelPopup.qml:192:             return;
…
ControlPanelPopup.qml:197:         shellRoot.wifiSetRadio(enabled);
ControlPanelPopup.qml:198:         controlPanelRefreshTimer.restart();
ControlPanelPopup.qml:199:     }
ControlPanelPopup.qml:200: 
ControlPanelPopup.qml:201:     function toggleBluetoothEnabled() {
ControlPanelPopup.qml:202:         if (!shellRoot) {
ControlPanelPopup.qml:203:             return;
…
ControlPanelPopup.qml:207: 
ControlPanelPopup.qml:208:     function toggleDnd() {
ControlPanelPopup.qml:209:         if (shellRoot) {
ControlPanelPopup.qml:210:             shellRoot.dndEnabled = !shellRoot.dndEnabled;
…
ControlPanelPopup.qml:214: 
ControlPanelPopup.qml:215:     function toggleTheme() {
ControlPanelPopup.qml:216:         if (!shellRoot) {
ControlPanelPopup.qml:217:             return;
…
ControlPanelPopup.qml:219:         shellRoot.darkMode = !shellRoot.darkMode;
ControlPanelPopup.qml:220:         shellRoot.runDetached([shellRoot.configDir + "/quickshell/scripts/toggle-theme.sh"]);
ControlPanelPopup.qml:221:     }
ControlPanelPopup.qml:222: 
ControlPanelPopup.qml:223:     function toggleRecording() {
ControlPanelPopup.qml:224:         if (!shellRoot) {
ControlPanelPopup.qml:225:             return;
…
ControlPanelPopup.qml:229:         if (nextState) {
ControlPanelPopup.qml:230:             runAndRefresh([shellRoot.homeDir + "/.config/hypr/scripts/record-script.sh", "--fullscreen-sound"]);
ControlPanelPopup.qml:231:         } else {
ControlPanelPopup.qml:232:             runAndRefresh(["sh", "-c", "pkill -INT wf-recorder || pkill -INT wl-screenrec || pkill -INT gpu-screen-recorder"]);
ControlPanelPopup.qml:233:         }
ControlPanelPopup.qml:234:     }
ControlPanelPopup.qml:235: 
ControlPanelPopup.qml:236:     function togglePreventSleep() {
ControlPanelPopup.qml:237:         if (!shellRoot) {
ControlPanelPopup.qml:238:             return;
ControlPanelPopup.qml:239:         }
ControlPanelPopup.qml:240:         shellRoot.preventSleepEnabled = !shellRoot.preventSleepEnabled;
ControlPanelPopup.qml:241:         runAndRefresh([shellRoot.configDir + "/quickshell/scripts/prevent-sleep.sh", "toggle"]);
ControlPanelPopup.qml:242:     }
ControlPanelPopup.qml:243: 
ControlPanelPopup.qml:244:     function toggleDemoMode() {
ControlPanelPopup.qml:245:         if (!shellRoot) {
ControlPanelPopup.qml:246:             return;
…
ControlPanelPopup.qml:248:         shellRoot.demoModeEnabled = !shellRoot.demoModeEnabled;
ControlPanelPopup.qml:249:         runAndRefresh([shellRoot.configDir + "/quickshell/scripts/demo-mode.sh", "toggle"]);
ControlPanelPopup.qml:250:     }
ControlPanelPopup.qml:251: 
ControlPanelPopup.qml:252:     function toggleWifiExpanded() {
ControlPanelPopup.qml:253:         if (!shellRoot) {
ControlPanelPopup.qml:254:             return;
…
ControlPanelPopup.qml:258:         popupWindow.visible = false;
ControlPanelPopup.qml:259:         shellRoot.openWifiPanel();
ControlPanelPopup.qml:260:     }
ControlPanelPopup.qml:261: 
ControlPanelPopup.qml:262:     function toggleBluetoothExpanded() {
ControlPanelPopup.qml:263:         const wasExpanded = bluetoothExpanded;
ControlPanelPopup.qml:264:         if (wasExpanded) {
ControlPanelPopup.qml:265:             showMainPage();
ControlPanelPopup.qml:266:             return;
ControlPanelPopup.qml:267:         }
…
ControlPanelPopup.qml:273: 
ControlPanelPopup.qml:274:     function toggleMihomoExpanded() {
ControlPanelPopup.qml:275:         if (mihomoExpanded) {
ControlPanelPopup.qml:276:             showMainPage();
ControlPanelPopup.qml:277:             return;
ControlPanelPopup.qml:278:         }
…
ControlPanelPopup.qml:281: 
ControlPanelPopup.qml:282:     function showMainPage() {
ControlPanelPopup.qml:283:         powerExpanded = false;
ControlPanelPopup.qml:284:         switchPageWithAnimation("main");
…
ControlPanelPopup.qml:286: 
ControlPanelPopup.qml:287:     function powerProfileLabel(profile) {
ControlPanelPopup.qml:288:         if (profile === "power-saver") {
ControlPanelPopup.qml:289:             return "Battery";
…
ControlPanelPopup.qml:296: 
ControlPanelPopup.qml:297:     function powerProfileIcon(profile) {
ControlPanelPopup.qml:298:         if (profile === "power-saver") {
ControlPanelPopup.qml:299:             return "󰾆";
…
ControlPanelPopup.qml:306: 
ControlPanelPopup.qml:307:     function setPowerProfile(profile) {
ControlPanelPopup.qml:308:         if (!shellRoot || !profile) {
ControlPanelPopup.qml:309:             return;
…
ControlPanelPopup.qml:312:         shellRoot.runDetached(["powerprofilesctl", "set", profile]);
ControlPanelPopup.qml:313:         controlPanelRefreshTimer.restart();
ControlPanelPopup.qml:314:         powerProfileFollowupRefresh.restart();
ControlPanelPopup.qml:315:     }
ControlPanelPopup.qml:316: 
ControlPanelPopup.qml:317:     function cyclePowerProfile() {
ControlPanelPopup.qml:318:         if (!shellRoot) {
ControlPanelPopup.qml:319:             return;
…
ControlPanelPopup.qml:326: 
ControlPanelPopup.qml:327:     function togglePowerExpanded() {
ControlPanelPopup.qml:328:         powerExpanded = !powerExpanded;
ControlPanelPopup.qml:329:     }
ControlPanelPopup.qml:330: 
ControlPanelPopup.qml:331:     function handleMediaAction(command) {
ControlPanelPopup.qml:332:         if (!shellRoot || !shellRoot.mediaAvailable) {
ControlPanelPopup.qml:333:             return;
ControlPanelPopup.qml:334:         }
ControlPanelPopup.qml:335:         if (Array.isArray(command) && command.length >= 2 && command[0] === "playerctl" && command[1] === "play-pause") {
ControlPanelPopup.qml:336:             shellRoot.mediaPlaying = !shellRoot.mediaPlaying;
ControlPanelPopup.qml:337:         }
…
ControlPanelPopup.qml:341: 
ControlPanelPopup.qml:342:     function currentWifiTitle() {
ControlPanelPopup.qml:343:         return "WIFI";
ControlPanelPopup.qml:344:     }
ControlPanelPopup.qml:345: 
ControlPanelPopup.qml:346:     function currentWifiSubtitle() {
ControlPanelPopup.qml:347:         if (!shellRoot) {
ControlPanelPopup.qml:348:             return "";
…
ControlPanelPopup.qml:358: 
ControlPanelPopup.qml:359:     function currentBluetoothTitle() {
ControlPanelPopup.qml:360:         if (!shellRoot || !Array.isArray(shellRoot.bluetoothDevices)) {
ControlPanelPopup.qml:361:             return "Bluetooth";
…
ControlPanelPopup.qml:371: 
ControlPanelPopup.qml:372:     function currentBluetoothSubtitle() {
ControlPanelPopup.qml:373:         if (!shellRoot) {
ControlPanelPopup.qml:374:             return "";
…
ControlPanelPopup.qml:407: 
ControlPanelPopup.qml:408:     function confirmEgpuAction() {
ControlPanelPopup.qml:409:         if (popupRoot.egpuConfirmPending) {
ControlPanelPopup.qml:410:             popupRoot.egpuConfirmPending = false;
ControlPanelPopup.qml:411:             egpuConfirmResetTimer.stop();
ControlPanelPopup.qml:412:             if (popupRoot.shellRoot) {
ControlPanelPopup.qml:413:                 popupRoot.shellRoot.runDetached([popupRoot.shellRoot.configDir + "/quickshell/scripts/egpu-disconnect.sh"]);
ControlPanelPopup.qml:414:             }
ControlPanelPopup.qml:415:             return;
…
ControlPanelPopup.qml:417:         popupRoot.egpuConfirmPending = true;
ControlPanelPopup.qml:418:         egpuConfirmResetTimer.restart();
ControlPanelPopup.qml:419:     }
ControlPanelPopup.qml:420: 
ControlPanelPopup.qml:421:     Timer {
ControlPanelPopup.qml:422:         id: controlPanelRefreshTimer
ControlPanelPopup.qml:423: 
ControlPanelPopup.qml:424:         interval: 400
ControlPanelPopup.qml:425:         repeat: false
ControlPanelPopup.qml:426:         onTriggered: if (shellRoot) {
…
ControlPanelPopup.qml:430: 
ControlPanelPopup.qml:431:     Timer {
ControlPanelPopup.qml:432:         id: mediaFollowupRefresh
ControlPanelPopup.qml:433: 
ControlPanelPopup.qml:434:         interval: 250
ControlPanelPopup.qml:435:         repeat: false
ControlPanelPopup.qml:436:         onTriggered: if (shellRoot) {
…
ControlPanelPopup.qml:440: 
ControlPanelPopup.qml:441:     Timer {
ControlPanelPopup.qml:442:         id: powerProfileFollowupRefresh
ControlPanelPopup.qml:443: 
ControlPanelPopup.qml:444:         interval: 350
ControlPanelPopup.qml:445:         repeat: false
ControlPanelPopup.qml:446:         onTriggered: if (shellRoot) {
…
ControlPanelPopup.qml:450: 
ControlPanelPopup.qml:451:     Timer {
ControlPanelPopup.qml:452:         id: egpuConfirmResetTimer
ControlPanelPopup.qml:453: 
ControlPanelPopup.qml:454:         interval: 3000
ControlPanelPopup.qml:455:         repeat: false
ControlPanelPopup.qml:456:         onTriggered: popupRoot.egpuConfirmPending = false
…
ControlPanelPopup.qml:458: 
ControlPanelPopup.qml:459:     Timer {
ControlPanelPopup.qml:460:         id: positionTimer
ControlPanelPopup.qml:461: 
ControlPanelPopup.qml:462:         interval: 0
ControlPanelPopup.qml:463:         repeat: false
ControlPanelPopup.qml:464:         onTriggered: popupWindow.updatePopupPosition()
…
ControlPanelPopup.qml:466: 
ControlPanelPopup.qml:467:     Timer {
ControlPanelPopup.qml:468:         id: popupOpenTimer
ControlPanelPopup.qml:469: 
ControlPanelPopup.qml:470:         interval: 16
ControlPanelPopup.qml:471:         repeat: false
ControlPanelPopup.qml:472:         onTriggered: {
ControlPanelPopup.qml:473:             if (!popupWindow.visible || !popupRoot.popupRequested || popupRoot.animatingClose) {
ControlPanelPopup.qml:474:                 popupRoot.openAnimationPending = false;
ControlPanelPopup.qml:475:                 return;
ControlPanelPopup.qml:476:             }
ControlPanelPopup.qml:477:             if (popupContent.implicitHeight <= 0) {
ControlPanelPopup.qml:478:                 popupOpenTimer.restart();
ControlPanelPopup.qml:479:                 return;
ControlPanelPopup.qml:480:             }
…
ControlPanelPopup.qml:539:                         subtitle: popupRoot.currentWifiSubtitle()
ControlPanelPopup.qml:540:                         active: popupRoot.shellRoot ? popupRoot.shellRoot.wifiRadioEnabled : false
ControlPanelPopup.qml:541:                         expanded: popupRoot.wifiExpanded
ControlPanelPopup.qml:542:                         onLeftClicked: popupRoot.toggleWifiEnabled()
…
ControlPanelPopup.qml:552:                         subtitle: popupRoot.currentBluetoothSubtitle()
ControlPanelPopup.qml:553:                         active: popupRoot.shellRoot ? popupRoot.shellRoot.bluetoothEnabled : false
ControlPanelPopup.qml:554:                         expanded: popupRoot.bluetoothExpanded
ControlPanelPopup.qml:555:                         onLeftClicked: popupRoot.toggleBluetoothEnabled()
…
ControlPanelPopup.qml:587:                             label: "Screen\nrecord"
ControlPanelPopup.qml:588:                             active: popupRoot.shellRoot ? popupRoot.shellRoot.screenRecording : false
ControlPanelPopup.qml:589:                             onClicked: popupRoot.toggleRecording()
ControlPanelPopup.qml:590:                         }
…
ControlPanelPopup.qml:597:                             iconOnly: true
ControlPanelPopup.qml:598:                             label: "Prevent\nsleep"
ControlPanelPopup.qml:599:                             active: popupRoot.shellRoot ? popupRoot.shellRoot.preventSleepEnabled : false
ControlPanelPopup.qml:600:                             onClicked: popupRoot.togglePreventSleep()
ControlPanelPopup.qml:601:                         }
ControlPanelPopup.qml:602:                     }
…
ControlPanelPopup.qml:618:                         artUrl: popupRoot.shellRoot ? popupRoot.shellRoot.mediaArtUrl : ""
ControlPanelPopup.qml:619:                         onPreviousClicked: popupRoot.handleMediaAction(["playerctl", "previous"])
ControlPanelPopup.qml:620:                         onPlayPauseClicked: popupRoot.handleMediaAction(["playerctl", "play-pause"])
ControlPanelPopup.qml:621:                         onNextClicked: popupRoot.handleMediaAction(["playerctl", "next"])
ControlPanelPopup.qml:622:                     }
ControlPanelPopup.qml:623: 
…
ControlPanelPopup.qml:634:                             label: popupRoot.shellRoot && popupRoot.shellRoot.darkMode ? "Dark\nmode" : "Light\nmode"
ControlPanelPopup.qml:635:                             active: popupRoot.shellRoot ? popupRoot.shellRoot.darkMode : false
ControlPanelPopup.qml:636:                             onClicked: popupRoot.toggleTheme()
ControlPanelPopup.qml:637:                         }
…
ControlPanelPopup.qml:645:                             label: "No\ndisturb"
ControlPanelPopup.qml:646:                             active: popupRoot.shellRoot ? popupRoot.shellRoot.dndEnabled : false
ControlPanelPopup.qml:647:                             onClicked: popupRoot.toggleDnd()
ControlPanelPopup.qml:648:                         }
…
ControlPanelPopup.qml:660:                             iconOnly: true
ControlPanelPopup.qml:661:                             active: popupRoot.shellRoot ? popupRoot.shellRoot.demoModeEnabled : false
ControlPanelPopup.qml:662:                             onClicked: popupRoot.toggleDemoMode()
ControlPanelPopup.qml:663:                         }
…
ControlPanelPopup.qml:670:                             iconOnly: true
ControlPanelPopup.qml:671:                             active: false
ControlPanelPopup.qml:672:                             onClicked: popupRoot.cyclePowerProfile()
ControlPanelPopup.qml:673:                         }
…
ControlPanelPopup.qml:758:         onWidthChanged: if (visible) {
ControlPanelPopup.qml:759:             updatePopupPosition();
ControlPanelPopup.qml:760:         }
ControlPanelPopup.qml:761:         onHeightChanged: if (visible) {
ControlPanelPopup.qml:762:             updatePopupPosition();
ControlPanelPopup.qml:763:         }
ControlPanelPopup.qml:764: 
ControlPanelPopup.qml:765:         function updatePopupPosition() {
ControlPanelPopup.qml:766:             if (!visible || !screen) {
ControlPanelPopup.qml:767:                 return;
…
ControlPanelPopup.qml:779:                 if (visible) {
ControlPanelPopup.qml:780:                     updatePopupPosition();
ControlPanelPopup.qml:781:                     popupFocusScope.forceActiveFocus();
ControlPanelPopup.qml:782:                     if (shellRoot) {
…
ControlPanelPopup.qml:789:                 if (!popupRoot.animatingClose) {
ControlPanelPopup.qml:790:                     popupRoot.openAnimationPending = true;
ControlPanelPopup.qml:791:                     popupCard.prepareOpenAnimation();
ControlPanelPopup.qml:792:                     popupOpenTimer.restart();
ControlPanelPopup.qml:793:                 }
ControlPanelPopup.qml:794:             } else {
ControlPanelPopup.qml:795:                 popupRoot.animatingClose = false;
ControlPanelPopup.qml:796:                 popupRoot.openAnimationPending = false;
ControlPanelPopup.qml:797:                 popupRoot.resetExpandedState();
ControlPanelPopup.qml:798:                 popupOpenTimer.stop();
ControlPanelPopup.qml:799:                 popupCard.stopAnimations();
ControlPanelPopup.qml:800:                 popupCard.resetAnimationState();
…
ControlPanelPopup.qml:843:             onFullPanelHeightChanged: {
ControlPanelPopup.qml:844:                 if (popupRoot.openAnimationPending) {
ControlPanelPopup.qml:845:                     positionTimer.restart();
ControlPanelPopup.qml:846:                     if (!popupCard.openAnimationRunning) {
ControlPanelPopup.qml:847:                         popupOpenTimer.restart();
ControlPanelPopup.qml:848:                     }
ControlPanelPopup.qml:849:                     return;
…
ControlPanelPopup.qml:851:                 if (popupWindow.visible && !popupRoot.animatingClose) {
ControlPanelPopup.qml:852:                     if (popupCard.openAnimationRunning || popupCard.closeAnimationRunning) {
ControlPanelPopup.qml:853:                         positionTimer.restart();
ControlPanelPopup.qml:854:                         return;
ControlPanelPopup.qml:855:                     }
…
ControlPanelPopup.qml:858:                     contentOffset = 0;
ControlPanelPopup.qml:859:                 } else if (!popupCard.openAnimationRunning && !popupCard.closeAnimationRunning) {
ControlPanelPopup.qml:860:                     revealHeight = fullPanelHeight;
ControlPanelPopup.qml:861:                     if (!popupWindow.visible) {
…
ControlPanelPopup.qml:865:                 }
ControlPanelPopup.qml:866:                 positionTimer.restart();
ControlPanelPopup.qml:867:             }
ControlPanelPopup.qml:868: 
ControlPanelPopup.qml:869:             onOpenAnimationFinished: {
ControlPanelPopup.qml:870:                 popupRoot.openAnimationPending = false;
ControlPanelPopup.qml:871:                 if (!popupWindow.visible || popupRoot.animatingClose) {
ControlPanelPopup.qml:872:                     return;
ControlPanelPopup.qml:873:                 }
ControlPanelPopup.qml:874:                 positionTimer.restart();
ControlPanelPopup.qml:875:             }
ControlPanelPopup.qml:876: 
…
ControlPanelPopup.qml:880:                     popupRoot.pendingPage = "";
ControlPanelPopup.qml:881:                     positionTimer.restart();
ControlPanelPopup.qml:882:                     popupRoot.animateCurrentPageOpen();
ControlPanelPopup.qml:883:                     return;
…
ControlPanelPopup.qml:902:                 onImplicitHeightChanged: {
ControlPanelPopup.qml:903:                     if (popupRoot.openAnimationPending && !popupCard.openAnimationRunning) {
ControlPanelPopup.qml:904:                         popupOpenTimer.restart();
ControlPanelPopup.qml:905:                     }
ControlPanelPopup.qml:906:                 }
…
ControlPanelPopup.qml:910: 
ControlPanelPopup.qml:911:                     active: popupWindow.visible
ControlPanelPopup.qml:912:                     anchors.left: parent.left
ControlPanelPopup.qml:913:                     anchors.right: parent.right
```

### `DeskflowTargetControl.qml`

Migration owner: `UI/provider integration (domain sections above)`. Source `DeskflowTargetControl.qml:1`, 56 lines, SHA-256 `edbf3cd79dde11633265ef30250d1ac0813e10c441b24157622d313098e1927a`.

```text
DeskflowTargetControl.qml:1: import QtQuick
DeskflowTargetControl.qml:2: import Quickshell
DeskflowTargetControl.qml:3: import Quickshell.Io
DeskflowTargetControl.qml:4: 
DeskflowTargetControl.qml:5: ControlPanelToggle {
DeskflowTargetControl.qml:6:     id: root
DeskflowTargetControl.qml:7: 
DeskflowTargetControl.qml:8:     property string target: "windows"
DeskflowTargetControl.qml:9:     property bool busy: false
DeskflowTargetControl.qml:10:     readonly property string helper: shellRoot ? shellRoot.configDir + "/quickshell/scripts/deskflow-target.sh" : ""
DeskflowTargetControl.qml:11: 
DeskflowTargetControl.qml:12:     function applyStatus(output) {
DeskflowTargetControl.qml:13:         const nextTarget = output.trim();
DeskflowTargetControl.qml:14:         if (nextTarget === "macos" || nextTarget === "windows") {
DeskflowTargetControl.qml:15:             target = nextTarget;
DeskflowTargetControl.qml:16:         }
DeskflowTargetControl.qml:17:     }
DeskflowTargetControl.qml:18: 
DeskflowTargetControl.qml:19:     function toggleTarget() {
DeskflowTargetControl.qml:20:         if (busy || !helper) return;
DeskflowTargetControl.qml:21:         busy = true;
DeskflowTargetControl.qml:22:         action.command = ["bash", helper, "set", target === "windows" ? "macos" : "windows"];
DeskflowTargetControl.qml:23:         action.running = true;
DeskflowTargetControl.qml:24:     }
DeskflowTargetControl.qml:25: 
DeskflowTargetControl.qml:26:     icon: target === "macos" ? "" : "󰖳"
DeskflowTargetControl.qml:27:     iconOnly: true
DeskflowTargetControl.qml:28:     label: busy ? "Switching" : (target === "macos" ? "macOS" : "Windows")
DeskflowTargetControl.qml:29:     active: false
DeskflowTargetControl.qml:30:     toggleEnabled: !busy
DeskflowTargetControl.qml:31:     onClicked: toggleTarget()
DeskflowTargetControl.qml:32: 
DeskflowTargetControl.qml:33:     PollCommand {
DeskflowTargetControl.qml:34:         id: poll
DeskflowTargetControl.qml:35:         active: root.visible
DeskflowTargetControl.qml:36:         interval: 2000
DeskflowTargetControl.qml:37:         command: root.helper ? ["bash", root.helper, "status"] : []
DeskflowTargetControl.qml:38:         onUpdated: function(output, exitCode) {
DeskflowTargetControl.qml:39:             if (exitCode === 0) root.applyStatus(output);
DeskflowTargetControl.qml:40:         }
DeskflowTargetControl.qml:41:     }
DeskflowTargetControl.qml:42: 
DeskflowTargetControl.qml:43:     Process {
DeskflowTargetControl.qml:44:         id: action
DeskflowTargetControl.qml:45:         stdout: StdioCollector {}
DeskflowTargetControl.qml:46:         stderr: StdioCollector { id: actionError }
DeskflowTargetControl.qml:47:         onExited: function(exitCode) {
DeskflowTargetControl.qml:48:             root.busy = false;
DeskflowTargetControl.qml:49:             poll.refresh();
DeskflowTargetControl.qml:50:             if (exitCode !== 0) {
DeskflowTargetControl.qml:51:                 const message = actionError.text.trim() || "Switch failed";
DeskflowTargetControl.qml:52:                 Quickshell.execDetached(["notify-send", "Win/macOS switch", message]);
DeskflowTargetControl.qml:53:             }
DeskflowTargetControl.qml:54:         }
DeskflowTargetControl.qml:55:     }
DeskflowTargetControl.qml:56: }
```

### `MihomoControlPanel.qml`

Migration owner: `UI/provider integration (domain sections above)`. Source `MihomoControlPanel.qml:1`, 237 lines, SHA-256 `b81a13e2b30a51f7113c8610aefcbbaf7ebfb7c7f28879e44c2d296b64723e4c`.

```text
…
MihomoControlPanel.qml:9:     property bool useExternalPanelBackground: false
MihomoControlPanel.qml:10:     signal closeRequested()
MihomoControlPanel.qml:11: 
MihomoControlPanel.qml:12:     property bool online: false
…
MihomoControlPanel.qml:29:     readonly property color upColor: shellRoot ? shellRoot.microphoneColor : "#cba6f7"
MihomoControlPanel.qml:30:     readonly property string helper: shellRoot ? shellRoot.configDir + "/quickshell/scripts/mihomo-control.sh" : ""
MihomoControlPanel.qml:31: 
MihomoControlPanel.qml:32:     radius: 19
MihomoControlPanel.qml:33:     color: useExternalPanelBackground ? "transparent" : cardFill
MihomoControlPanel.qml:34:     border.width: useExternalPanelBackground ? 0 : 1
MihomoControlPanel.qml:35:     border.color: useExternalPanelBackground ? "transparent" : cardStroke
MihomoControlPanel.qml:36:     implicitHeight: content.implicitHeight + 20
MihomoControlPanel.qml:37: 
MihomoControlPanel.qml:38:     function formatRate(bytes) {
MihomoControlPanel.qml:39:         const value = Math.max(0, Number(bytes) || 0);
MihomoControlPanel.qml:40:         if (value >= 1048576) return (value / 1048576).toFixed(value >= 10485760 ? 0 : 1) + " MB/s";
…
MihomoControlPanel.qml:44: 
MihomoControlPanel.qml:45:     function applyStatus(text) {
MihomoControlPanel.qml:46:         try {
MihomoControlPanel.qml:47:             const data = JSON.parse(text);
MihomoControlPanel.qml:48:             online = data.online === true;
MihomoControlPanel.qml:49:             service = String(data.service || "none");
…
MihomoControlPanel.qml:61: 
MihomoControlPanel.qml:62:     function runAction(args) {
MihomoControlPanel.qml:63:         if (!helper || actionProcess.running) return;
MihomoControlPanel.qml:64:         busy = true;
…
MihomoControlPanel.qml:73:         id: statusPoll
MihomoControlPanel.qml:74:         active: root.visible
MihomoControlPanel.qml:75:         interval: 1500
MihomoControlPanel.qml:76:         command: root.helper ? ["bash", root.helper, "status"] : []
MihomoControlPanel.qml:77:         onUpdated: function(output, exitCode) {
MihomoControlPanel.qml:78:             if (exitCode === 0) root.applyStatus(output);
…
MihomoControlPanel.qml:148:                 color: root.online ? (root.shellRoot ? root.shellRoot.withAlpha(root.downColor, 0.22) : "#335544") : root.cardStrongFill
MihomoControlPanel.qml:149:                 Text { anchors.centerIn: parent; text: root.online ? "ON" : "OFF"; color: root.online ? root.downColor : root.mutedText; font.pixelSize: 9; font.bold: true }
MihomoControlPanel.qml:150:                 MouseArea {
MihomoControlPanel.qml:151:                     anchors.fill: parent
…
MihomoControlPanel.qml:153:                     cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
MihomoControlPanel.qml:154:                     onClicked: root.runAction([root.online ? "stop" : "start"])
MihomoControlPanel.qml:155:                 }
MihomoControlPanel.qml:156:             }
…
MihomoControlPanel.qml:180:                     SequentialAnimation on border.color {
MihomoControlPanel.qml:181:                         running: root.online && Number(modelData.label === "Download" ? root.download : root.upload) > 1024
MihomoControlPanel.qml:182:                         loops: Animation.Infinite
MihomoControlPanel.qml:183:                         ColorAnimation { to: modelData.tone; duration: 650 }
```

### `MihomoControlTile.qml`

Migration owner: `UI/provider integration (domain sections above)`. Source `MihomoControlTile.qml:1`, 72 lines, SHA-256 `78dbac1ef8fcf086c8e4df0ccfc6cbf5080da275b7664f329c186c960de1e59b`.

```text
…
MihomoControlTile.qml:12:     property bool busy: false
MihomoControlTile.qml:13:     readonly property string helper: shellRoot ? shellRoot.configDir + "/quickshell/scripts/mihomo-control.sh" : ""
MihomoControlTile.qml:14: 
MihomoControlTile.qml:15:     signal detailsClicked()
MihomoControlTile.qml:16: 
MihomoControlTile.qml:17:     function applyStatus(output) {
MihomoControlTile.qml:18:         try {
MihomoControlTile.qml:19:             const data = JSON.parse(output);
MihomoControlTile.qml:20:             online = data.online === true;
MihomoControlTile.qml:21:             service = String(data.service || "none");
…
MihomoControlTile.qml:27: 
MihomoControlTile.qml:28:     function toggleProxy() {
MihomoControlTile.qml:29:         if (busy || (service !== "user" && service !== "system")) return;
MihomoControlTile.qml:30:         busy = true;
MihomoControlTile.qml:31:         action.command = ["bash", helper, online ? "stop" : "start"];
MihomoControlTile.qml:32:         action.running = true;
MihomoControlTile.qml:33:     }
…
MihomoControlTile.qml:38:         id: poll
MihomoControlTile.qml:39:         active: root.visible
MihomoControlTile.qml:40:         interval: 2000
MihomoControlTile.qml:41:         command: root.helper ? ["bash", root.helper, "status"] : []
MihomoControlTile.qml:42:         onUpdated: function(output, exitCode) {
MihomoControlTile.qml:43:             if (exitCode === 0) root.applyStatus(output);
…
MihomoControlTile.qml:63:         subtitle: root.busy ? "Switching…" : (root.online ? ((root.mode || "Rule") + " mode") : "Off")
MihomoControlTile.qml:64:         active: root.online
MihomoControlTile.qml:65:         expanded: root.expanded
MihomoControlTile.qml:66:         leftEnabled: !root.busy && (root.service === "user" || root.service === "system")
```

### `PollCommand.qml`

Migration owner: `UI/provider integration (domain sections above)`. Source `PollCommand.qml:1`, 56 lines, SHA-256 `d71762b71e3c1c6d1a1c1b3d87cde321e8cf8593beeb04fea15f003d8a6a3553`.

```text
PollCommand.qml:1: import QtQuick
PollCommand.qml:2: import Quickshell.Io
PollCommand.qml:3: 
PollCommand.qml:4: Item {
PollCommand.qml:5:     id: poll
PollCommand.qml:6: 
PollCommand.qml:7:     property var command: []
PollCommand.qml:8:     property int interval: 1000
PollCommand.qml:9:     property bool active: true
PollCommand.qml:10:     property string output: ""
PollCommand.qml:11:     property bool _refreshQueued: false
PollCommand.qml:12: 
PollCommand.qml:13:     signal updated(string output, int exitCode)
PollCommand.qml:14: 
PollCommand.qml:15:     function refresh(queueIfRunning) {
PollCommand.qml:16:         if (!active || !command || command.length === 0) {
PollCommand.qml:17:             return;
PollCommand.qml:18:         }
PollCommand.qml:19:         if (proc.running) {
PollCommand.qml:20:             if (queueIfRunning) _refreshQueued = true;
PollCommand.qml:21:             return;
PollCommand.qml:22:         }
PollCommand.qml:23:         proc.command = command;
PollCommand.qml:24:         proc.running = true;
PollCommand.qml:25:     }
PollCommand.qml:26: 
PollCommand.qml:27:     Timer {
PollCommand.qml:28:         interval: poll.interval
PollCommand.qml:29:         repeat: true
PollCommand.qml:30:         running: poll.active
PollCommand.qml:31:         triggeredOnStart: true
PollCommand.qml:32:         onTriggered: poll.refresh()
PollCommand.qml:33:     }
PollCommand.qml:34: 
PollCommand.qml:35:     Process {
PollCommand.qml:36:         id: proc
PollCommand.qml:37: 
PollCommand.qml:38:         running: false
PollCommand.qml:39:         stdout: StdioCollector {
PollCommand.qml:40:             id: collector
PollCommand.qml:41:         }
PollCommand.qml:42:         stderr: StdioCollector {}
PollCommand.qml:43: 
PollCommand.qml:44:         onExited: function(exitCode) {
PollCommand.qml:45:             const text = (collector.text || "").trim();
PollCommand.qml:46:             if (exitCode === 0) {
PollCommand.qml:47:                 poll.output = text;
PollCommand.qml:48:             }
PollCommand.qml:49:             poll.updated(text, exitCode);
PollCommand.qml:50:             if (poll._refreshQueued) {
PollCommand.qml:51:                 poll._refreshQueued = false;
PollCommand.qml:52:                 poll.refresh();
PollCommand.qml:53:             }
PollCommand.qml:54:         }
PollCommand.qml:55:     }
PollCommand.qml:56: }
```

### `QuickAdjustPopup.qml`

Migration owner: `UI/provider integration (domain sections above)`. Source `QuickAdjustPopup.qml:1`, 222 lines, SHA-256 `0da7b513285d6fd2ce89ed27f5e60bfa2096d99cf75fc9cf8001282e297adaab`.

```text
…
QuickAdjustPopup.qml:21: 
QuickAdjustPopup.qml:22:     function normalizedMode(kind) {
QuickAdjustPopup.qml:23:         return kind === "volume" ? "volume" : "brightness";
QuickAdjustPopup.qml:24:     }
QuickAdjustPopup.qml:25: 
QuickAdjustPopup.qml:26:     function show(kind) {
QuickAdjustPopup.qml:27:         mode = normalizedMode(kind);
QuickAdjustPopup.qml:28:         closing = false;
QuickAdjustPopup.qml:29:         closeFinalizeTimer.stop();
QuickAdjustPopup.qml:30:         hideTimer.restart();
QuickAdjustPopup.qml:31: 
QuickAdjustPopup.qml:32:         if (!popupWindow.visible) {
…
QuickAdjustPopup.qml:42: 
QuickAdjustPopup.qml:43:     function restartAutoHide() {
QuickAdjustPopup.qml:44:         hideTimer.restart();
QuickAdjustPopup.qml:45:     }
QuickAdjustPopup.qml:46: 
QuickAdjustPopup.qml:47:     function hide() {
QuickAdjustPopup.qml:48:         if (!popupWindow.visible) {
QuickAdjustPopup.qml:49:             return;
…
QuickAdjustPopup.qml:51:         closing = true;
QuickAdjustPopup.qml:52:         hideTimer.stop();
QuickAdjustPopup.qml:53:         cardOpacity = 0;
QuickAdjustPopup.qml:54:         cardScale = hiddenScale;
QuickAdjustPopup.qml:55:         closeFinalizeTimer.restart();
QuickAdjustPopup.qml:56:     }
QuickAdjustPopup.qml:57: 
QuickAdjustPopup.qml:58:     function applyValue(newValue) {
QuickAdjustPopup.qml:59:         if (!shellRoot) {
QuickAdjustPopup.qml:60:             return;
…
QuickAdjustPopup.qml:63:         const nextValue = Math.max(0, Math.min(100, Math.round(newValue)));
QuickAdjustPopup.qml:64:         restartAutoHide();
QuickAdjustPopup.qml:65: 
QuickAdjustPopup.qml:66:         if (mode === "brightness") {
…
QuickAdjustPopup.qml:75:         }
QuickAdjustPopup.qml:76:         shellRoot.runDetached(["hyprctl", "eval", 'require("hypr.controls").volume("set",' + nextValue + ')']);
QuickAdjustPopup.qml:77:         shellRoot.scheduleAudioRefresh();
QuickAdjustPopup.qml:78:     }
QuickAdjustPopup.qml:79: 
QuickAdjustPopup.qml:80:     function toggleAudioMute() {
QuickAdjustPopup.qml:81:         if (!shellRoot || mode !== "volume") {
QuickAdjustPopup.qml:82:             return;
QuickAdjustPopup.qml:83:         }
QuickAdjustPopup.qml:84:         restartAutoHide();
QuickAdjustPopup.qml:85:         shellRoot.audioAvailable = true;
QuickAdjustPopup.qml:86:         shellRoot.audioMuted = !shellRoot.audioMuted;
QuickAdjustPopup.qml:87:         shellRoot.runDetached(["hyprctl", "eval", 'require("hypr.controls").volume("toggle")']);
QuickAdjustPopup.qml:88:         shellRoot.scheduleAudioRefresh();
QuickAdjustPopup.qml:89:     }
QuickAdjustPopup.qml:90: 
QuickAdjustPopup.qml:91:     Timer {
QuickAdjustPopup.qml:92:         id: hideTimer
QuickAdjustPopup.qml:93: 
QuickAdjustPopup.qml:94:         interval: 3000
QuickAdjustPopup.qml:95:         repeat: false
QuickAdjustPopup.qml:96:         onTriggered: popupRoot.hide()
…
QuickAdjustPopup.qml:98: 
QuickAdjustPopup.qml:99:     Timer {
QuickAdjustPopup.qml:100:         id: closeFinalizeTimer
QuickAdjustPopup.qml:101: 
QuickAdjustPopup.qml:102:         interval: 140
QuickAdjustPopup.qml:103:         repeat: false
QuickAdjustPopup.qml:104:         onTriggered: {
…
QuickAdjustPopup.qml:137:         onWidthChanged: if (visible) {
QuickAdjustPopup.qml:138:             updatePopupPosition();
QuickAdjustPopup.qml:139:         }
QuickAdjustPopup.qml:140:         onHeightChanged: if (visible) {
QuickAdjustPopup.qml:141:             updatePopupPosition();
QuickAdjustPopup.qml:142:         }
QuickAdjustPopup.qml:143:         onScreenChanged: if (visible) {
QuickAdjustPopup.qml:144:             updatePopupPosition();
QuickAdjustPopup.qml:145:         }
QuickAdjustPopup.qml:146:         onVisibleChanged: if (visible) {
QuickAdjustPopup.qml:147:             updatePopupPosition();
QuickAdjustPopup.qml:148:         }
QuickAdjustPopup.qml:149: 
QuickAdjustPopup.qml:150:         function updatePopupPosition() {
QuickAdjustPopup.qml:151:             if (!visible || !screen) {
QuickAdjustPopup.qml:152:                 return;
…
QuickAdjustPopup.qml:189:                 iconLeftInset: 24
QuickAdjustPopup.qml:190:                 label: popupRoot.mode === "volume" ? "Volume" : "Brightness"
QuickAdjustPopup.qml:191:                 value: popupRoot.mode === "volume"
QuickAdjustPopup.qml:192:                     ? (popupRoot.shellRoot ? popupRoot.shellRoot.audioVolumePercent : 0)
```

### `StartupConfirm.qml`

Migration owner: `UI/provider integration (domain sections above)`. Source `StartupConfirm.qml:1`, 158 lines, SHA-256 `2c1a04b9570543d1125bdc8f81315964a5dfad755a55b7c56d2bd888ffa3ac31`.

```text
…
StartupConfirm.qml:17: 
StartupConfirm.qml:18:     function finish(launch: bool) {
StartupConfirm.qml:19:         if (decided) return;
StartupConfirm.qml:20:         decided = true;
…
StartupConfirm.qml:30:         id: approval
StartupConfirm.qml:31:         path: Quickshell.env("HYPRV_STARTUP_APPROVAL") || ""
StartupConfirm.qml:32:         blockWrites: true
StartupConfirm.qml:33:         onSaved: Qt.quit()
StartupConfirm.qml:34:         onSaveFailed: Qt.quit()
StartupConfirm.qml:35:     }
StartupConfirm.qml:36:     Timer {
StartupConfirm.qml:37:         interval: 150
StartupConfirm.qml:38:         running: true
StartupConfirm.qml:39:         onTriggered: {
StartupConfirm.qml:40:             skip.forceActiveFocus(Qt.TabFocusReason);
```

### `StreamCommand.qml`

Migration owner: `UI/provider integration (domain sections above)`. Source `StreamCommand.qml:1`, 30 lines, SHA-256 `2039f0f63609c108749bc5951d51e98145a2d7df490c444bcedee3b637f4c4e3`.

```text
StreamCommand.qml:1: import QtQuick
StreamCommand.qml:2: import Quickshell.Io
StreamCommand.qml:3: 
StreamCommand.qml:4: // Keep subscriptions alive without spawning a command for every update.
StreamCommand.qml:5: Item {
StreamCommand.qml:6:     id: stream
StreamCommand.qml:7: 
StreamCommand.qml:8:     property var command: []
StreamCommand.qml:9:     signal received(string line)
StreamCommand.qml:10:     signal stopped()
StreamCommand.qml:11: 
StreamCommand.qml:12:     Process {
StreamCommand.qml:13:         id: process
StreamCommand.qml:14:         command: stream.command
StreamCommand.qml:15:         running: true
StreamCommand.qml:16:         stdout: SplitParser {
StreamCommand.qml:17:             onRead: line => stream.received(line)
StreamCommand.qml:18:         }
StreamCommand.qml:19:         onExited: {
StreamCommand.qml:20:             stream.stopped();
StreamCommand.qml:21:         }
StreamCommand.qml:22:     }
StreamCommand.qml:23: 
StreamCommand.qml:24:     Timer {
StreamCommand.qml:25:         interval: 5000
StreamCommand.qml:26:         repeat: true
StreamCommand.qml:27:         running: !process.running
StreamCommand.qml:28:         onTriggered: process.running = true
StreamCommand.qml:29:     }
StreamCommand.qml:30: }
```

### `ViewflowTargetControl.qml`

Migration owner: `UI/provider integration (domain sections above)`. Source `ViewflowTargetControl.qml:1`, 56 lines, SHA-256 `82bec968ded83ad280be7f5183b423732dd827eed01c159aab7824fd13615515`.

```text
ViewflowTargetControl.qml:1: import QtQuick
ViewflowTargetControl.qml:2: import Quickshell
ViewflowTargetControl.qml:3: import Quickshell.Io
ViewflowTargetControl.qml:4: 
ViewflowTargetControl.qml:5: ControlPanelToggle {
ViewflowTargetControl.qml:6:     id: root
ViewflowTargetControl.qml:7: 
ViewflowTargetControl.qml:8:     property string target: "windows"
ViewflowTargetControl.qml:9:     property bool busy: false
ViewflowTargetControl.qml:10:     readonly property string helper: shellRoot ? shellRoot.configDir + "/quickshell/scripts/viewflow-target.sh" : ""
ViewflowTargetControl.qml:11: 
ViewflowTargetControl.qml:12:     function applyStatus(output) {
ViewflowTargetControl.qml:13:         const nextTarget = output.trim();
ViewflowTargetControl.qml:14:         if (nextTarget === "macos" || nextTarget === "windows") {
ViewflowTargetControl.qml:15:             target = nextTarget;
ViewflowTargetControl.qml:16:         }
ViewflowTargetControl.qml:17:     }
ViewflowTargetControl.qml:18: 
ViewflowTargetControl.qml:19:     function toggleTarget() {
ViewflowTargetControl.qml:20:         if (busy || !helper) return;
ViewflowTargetControl.qml:21:         busy = true;
ViewflowTargetControl.qml:22:         action.command = ["bash", helper, "set", target === "windows" ? "macos" : "windows"];
ViewflowTargetControl.qml:23:         action.running = true;
ViewflowTargetControl.qml:24:     }
ViewflowTargetControl.qml:25: 
ViewflowTargetControl.qml:26:     icon: target === "macos" ? "" : "󰖳"
ViewflowTargetControl.qml:27:     iconOnly: true
ViewflowTargetControl.qml:28:     label: busy ? "Switching HDMI / Viewflow" : (target === "macos" ? "HDMI → macOS" : "HDMI → Windows")
ViewflowTargetControl.qml:29:     active: false
ViewflowTargetControl.qml:30:     toggleEnabled: !busy
ViewflowTargetControl.qml:31:     onClicked: toggleTarget()
ViewflowTargetControl.qml:32: 
ViewflowTargetControl.qml:33:     PollCommand {
ViewflowTargetControl.qml:34:         id: poll
ViewflowTargetControl.qml:35:         active: root.visible
ViewflowTargetControl.qml:36:         interval: 2000
ViewflowTargetControl.qml:37:         command: root.helper ? ["bash", root.helper, "status"] : []
ViewflowTargetControl.qml:38:         onUpdated: function(output, exitCode) {
ViewflowTargetControl.qml:39:             if (exitCode === 0) root.applyStatus(output);
ViewflowTargetControl.qml:40:         }
ViewflowTargetControl.qml:41:     }
ViewflowTargetControl.qml:42: 
ViewflowTargetControl.qml:43:     Process {
ViewflowTargetControl.qml:44:         id: action
ViewflowTargetControl.qml:45:         stdout: StdioCollector {}
ViewflowTargetControl.qml:46:         stderr: StdioCollector { id: actionError }
ViewflowTargetControl.qml:47:         onExited: function(exitCode) {
ViewflowTargetControl.qml:48:             root.busy = false;
ViewflowTargetControl.qml:49:             poll.refresh();
ViewflowTargetControl.qml:50:             if (exitCode !== 0) {
ViewflowTargetControl.qml:51:                 const message = actionError.text.trim() || "Switch failed";
ViewflowTargetControl.qml:52:                 Quickshell.execDetached(["notify-send", "Viewflow Apple/Windows", message]);
ViewflowTargetControl.qml:53:             }
ViewflowTargetControl.qml:54:         }
ViewflowTargetControl.qml:55:     }
ViewflowTargetControl.qml:56: }
```

### `WindowsVmController.qml`

Migration owner: `UI/provider integration (domain sections above)`. Source `WindowsVmController.qml:1`, 47 lines, SHA-256 `47d7e67b1e22c54f9e8bc197d4f8c4be31eb5e2e6168824c1dd5306e7b722d3b`.

```text
WindowsVmController.qml:1: import QtQuick
WindowsVmController.qml:2: import Quickshell.Io
WindowsVmController.qml:3: 
WindowsVmController.qml:4: Item {
WindowsVmController.qml:5:     id: root
WindowsVmController.qml:6: 
WindowsVmController.qml:7:     property var shellRoot: null
WindowsVmController.qml:8:     property bool pollingActive: false
WindowsVmController.qml:9:     signal actionStarting()
WindowsVmController.qml:10: 
WindowsVmController.qml:11:     property bool running: false
WindowsVmController.qml:12:     property bool busy: false
WindowsVmController.qml:13:     readonly property string helper: shellRoot ? shellRoot.configDir + "/quickshell/scripts/windows-vm-control.sh" : ""
WindowsVmController.qml:14: 
WindowsVmController.qml:15:     function applyStatus(output) {
WindowsVmController.qml:16:         running = output.trim() === "running";
WindowsVmController.qml:17:     }
WindowsVmController.qml:18: 
WindowsVmController.qml:19:     function toggleVm() {
WindowsVmController.qml:20:         if (busy || !helper) return;
WindowsVmController.qml:21:         busy = true;
WindowsVmController.qml:22:         action.command = ["bash", helper, running ? "hibernate" : "start"];
WindowsVmController.qml:23:         // Release the panel before pkexec can open its authentication window.
WindowsVmController.qml:24:         actionStarting();
WindowsVmController.qml:25:         action.running = true;
WindowsVmController.qml:26:     }
WindowsVmController.qml:27: 
WindowsVmController.qml:28:     PollCommand {
WindowsVmController.qml:29:         id: poll
WindowsVmController.qml:30:         active: root.pollingActive
WindowsVmController.qml:31:         interval: 2000
WindowsVmController.qml:32:         command: root.helper ? ["bash", root.helper, "status"] : []
WindowsVmController.qml:33:         onUpdated: function(output, exitCode) {
WindowsVmController.qml:34:             if (exitCode === 0) root.applyStatus(output);
WindowsVmController.qml:35:         }
WindowsVmController.qml:36:     }
WindowsVmController.qml:37: 
WindowsVmController.qml:38:     Process {
WindowsVmController.qml:39:         id: action
WindowsVmController.qml:40:         stdout: StdioCollector {}
WindowsVmController.qml:41:         stderr: StdioCollector {}
WindowsVmController.qml:42:         onExited: function(exitCode) {
WindowsVmController.qml:43:             root.busy = false;
WindowsVmController.qml:44:             poll.refresh();
WindowsVmController.qml:45:         }
WindowsVmController.qml:46:     }
WindowsVmController.qml:47: }
```

### `hypr-comp/MatugenColors.qml`

Migration owner: `UI/provider integration (domain sections above)`. Source `hypr-comp/MatugenColors.qml:1`, 79 lines, SHA-256 `6bd820a8f91e63f2573ae75f252fe27f2c512e80ad5f919c4d06eb7198fdcb00`.

```text
…
hypr-comp/MatugenColors.qml:35:         id: themeReader
hypr-comp/MatugenColors.qml:36:         command: ["cat", "/tmp/qs_colors.json"]
hypr-comp/MatugenColors.qml:37:         stdout: StdioCollector {
hypr-comp/MatugenColors.qml:38:             onStreamFinished: {
…
hypr-comp/MatugenColors.qml:42:                     try {
hypr-comp/MatugenColors.qml:43:                         let c = JSON.parse(txt);
hypr-comp/MatugenColors.qml:44:                         if (c.base) root.base = c.base;
hypr-comp/MatugenColors.qml:45:                         if (c.mantle) root.mantle = c.mantle;
…
hypr-comp/MatugenColors.qml:71: 
hypr-comp/MatugenColors.qml:72:     Timer {
hypr-comp/MatugenColors.qml:73:         interval: 1000 
hypr-comp/MatugenColors.qml:74:         running: true
hypr-comp/MatugenColors.qml:75:         repeat: true
hypr-comp/MatugenColors.qml:76:         triggeredOnStart: true
```

### `hypr-comp/Scaler.qml`

Migration owner: `UI/provider integration (domain sections above)`. Source `hypr-comp/Scaler.qml:1`, 56 lines, SHA-256 `302476da128d3e3e80ed214dcdfec5a56c64be820e159dd5a4ca152ad7e2c1c4`.

```text
hypr-comp/Scaler.qml:1: import QtQuick
hypr-comp/Scaler.qml:2: import Quickshell
hypr-comp/Scaler.qml:3: import Quickshell.Io
hypr-comp/Scaler.qml:4: import "WindowRegistry.js" as LayoutMath 
hypr-comp/Scaler.qml:5: 
hypr-comp/Scaler.qml:6: Item {
hypr-comp/Scaler.qml:7:     id: root
hypr-comp/Scaler.qml:8:     visible: false
hypr-comp/Scaler.qml:9: 
hypr-comp/Scaler.qml:10:     property real currentWidth: 1920.0
hypr-comp/Scaler.qml:11:     property real currentHeight: 1080.0 // <-- ADDED
hypr-comp/Scaler.qml:12:     property real uiScale: 1.0
hypr-comp/Scaler.qml:13: 
hypr-comp/Scaler.qml:14:     // FIXED: Now passes both Width and Height to respect aspect ratio
hypr-comp/Scaler.qml:15:     property real baseScale: LayoutMath.getScale(currentWidth, currentHeight, uiScale)
hypr-comp/Scaler.qml:16:     
hypr-comp/Scaler.qml:17:     function s(val) { 
hypr-comp/Scaler.qml:18:         return LayoutMath.s(val, baseScale); 
hypr-comp/Scaler.qml:19:     }
hypr-comp/Scaler.qml:20: 
hypr-comp/Scaler.qml:21:     Process {
hypr-comp/Scaler.qml:22:         id: scaleReader
hypr-comp/Scaler.qml:23:         command: ["bash", "-c", "settings=\"$HOME/.config/HyprV/quickshell/hypr-comp/settings.json\"; [ -f \"$settings\" ] && cat \"$settings\" || echo '{}'"]
hypr-comp/Scaler.qml:24:         running: true
hypr-comp/Scaler.qml:25:         stdout: StdioCollector {
hypr-comp/Scaler.qml:26:             onStreamFinished: {
hypr-comp/Scaler.qml:27:                 try {
hypr-comp/Scaler.qml:28:                     if (this.text && this.text.trim().length > 0 && this.text.trim() !== "{}") {
hypr-comp/Scaler.qml:29:                         let parsed = JSON.parse(this.text);
hypr-comp/Scaler.qml:30:                         if (parsed.uiScale !== undefined && root.uiScale !== parsed.uiScale) {
hypr-comp/Scaler.qml:31:                             root.uiScale = parsed.uiScale;
hypr-comp/Scaler.qml:32:                         }
hypr-comp/Scaler.qml:33:                     }
hypr-comp/Scaler.qml:34:                 } catch (e) {}
hypr-comp/Scaler.qml:35:             }
hypr-comp/Scaler.qml:36:         }
hypr-comp/Scaler.qml:37:     }
hypr-comp/Scaler.qml:38: 
hypr-comp/Scaler.qml:39:     // EVENT-DRIVEN WATCHER
hypr-comp/Scaler.qml:40:     Process {
hypr-comp/Scaler.qml:41:         id: scaleWatcher
hypr-comp/Scaler.qml:42:         // -qq keeps it completely silent. It waits for the file to exist, listens for a write, and then exits.
hypr-comp/Scaler.qml:43:         command: ["bash", "-c", "settings=\"$HOME/.config/HyprV/quickshell/hypr-comp/settings.json\"; [ -f \"$settings\" ] && inotifywait -qq -e modify,close_write \"$settings\" || sleep 86400"]
hypr-comp/Scaler.qml:44:         running: true
hypr-comp/Scaler.qml:45:         stdout: StdioCollector {
hypr-comp/Scaler.qml:46:             onStreamFinished: {
hypr-comp/Scaler.qml:47:                 // 1. Read the new data
hypr-comp/Scaler.qml:48:                 scaleReader.running = false;
hypr-comp/Scaler.qml:49:                 scaleReader.running = true;
hypr-comp/Scaler.qml:50:                 // 2. Restart the watcher for the next event
hypr-comp/Scaler.qml:51:                 scaleWatcher.running = false;
hypr-comp/Scaler.qml:52:                 scaleWatcher.running = true;
hypr-comp/Scaler.qml:53:             }
hypr-comp/Scaler.qml:54:         }
hypr-comp/Scaler.qml:55:     }
hypr-comp/Scaler.qml:56: }
```

### `hypr-comp/applauncher/applauncher.qml`

Migration owner: `applications/lifecycle`. Source `hypr-comp/applauncher/applauncher.qml:1`, 788 lines, SHA-256 `c50f4a339f9e594fca389bc6df2e91ce99c72ccdb47ca622cb0862bc42286ce8`.

```text
…
hypr-comp/applauncher/applauncher.qml:24: 
hypr-comp/applauncher/applauncher.qml:25:     function s(val) {
hypr-comp/applauncher/applauncher.qml:26:         let res = scaler.s(val);
hypr-comp/applauncher/applauncher.qml:27:         return res > 0 ? res : val;
…
hypr-comp/applauncher/applauncher.qml:98: 
hypr-comp/applauncher/applauncher.qml:99:     NumberAnimation { id: searchRotXAnim; target: window; property: "rotX"; duration: 700; easing.type: Easing.OutCubic }
hypr-comp/applauncher/applauncher.qml:100:     NumberAnimation { id: searchRotYAnim; target: window; property: "rotY"; duration: 700; easing.type: Easing.OutCubic }
hypr-comp/applauncher/applauncher.qml:101: 
hypr-comp/applauncher/applauncher.qml:102:     property var projCache: []
hypr-comp/applauncher/applauncher.qml:103:     property bool projDirty: true
hypr-comp/applauncher/applauncher.qml:104: 
hypr-comp/applauncher/applauncher.qml:105:     function rebuildProjCache() {
hypr-comp/applauncher/applauncher.qml:106:         if (!projDirty) return;
hypr-comp/applauncher/applauncher.qml:107:         projDirty = false;
…
hypr-comp/applauncher/applauncher.qml:141:     // Keep the original project3D for centerOnApp (called rarely)
hypr-comp/applauncher/applauncher.qml:142:     function project3D(bx, by, bz) {
hypr-comp/applauncher/applauncher.qml:143:         let rx = window.rotX;
hypr-comp/applauncher/applauncher.qml:144:         let ry = window.rotY;
…
hypr-comp/applauncher/applauncher.qml:151: 
hypr-comp/applauncher/applauncher.qml:152:     Timer {
hypr-comp/applauncher/applauncher.qml:153:         interval: 16
hypr-comp/applauncher/applauncher.qml:154:         running: !sceneMouse.pressed && !searchRotXAnim.running && !searchRotYAnim.running
hypr-comp/applauncher/applauncher.qml:155:         repeat: true
hypr-comp/applauncher/applauncher.qml:156:         onTriggered: window.rotY -= 0.002
…
hypr-comp/applauncher/applauncher.qml:158: 
hypr-comp/applauncher/applauncher.qml:159:     function centerOnApp(index) {
hypr-comp/applauncher/applauncher.qml:160:         if (index < 0 || index >= appModel.count) return;
hypr-comp/applauncher/applauncher.qml:161: 
…
hypr-comp/applauncher/applauncher.qml:190:         id: introPhaseAnim
hypr-comp/applauncher/applauncher.qml:191:         from: 0.0; to: 1.0; duration: 800; easing.type: Easing.OutExpo; running: true
hypr-comp/applauncher/applauncher.qml:192:     }
hypr-comp/applauncher/applauncher.qml:193: 
hypr-comp/applauncher/applauncher.qml:194:     Connections {
hypr-comp/applauncher/applauncher.qml:195:         target: window
hypr-comp/applauncher/applauncher.qml:196:         function onVisibleChanged() {
hypr-comp/applauncher/applauncher.qml:197:             if (window.visible) {
hypr-comp/applauncher/applauncher.qml:198:                 searchInput.text    = "";
…
hypr-comp/applauncher/applauncher.qml:209:         sequence: "Escape"
hypr-comp/applauncher/applauncher.qml:210:         onActivated: closeSequence.start()
hypr-comp/applauncher/applauncher.qml:211:     }
hypr-comp/applauncher/applauncher.qml:212: 
…
hypr-comp/applauncher/applauncher.qml:214:         id: closeSequence
hypr-comp/applauncher/applauncher.qml:215:         NumberAnimation { target: window; property: "introPhase"; to: 0.0; duration: 400; easing.type: Easing.OutQuint }
hypr-comp/applauncher/applauncher.qml:216:         ScriptAction { script: Quickshell.execDetached(["bash", paths.serpantinumDir + "/scripts/qs_manager.sh", "close"]) }
hypr-comp/applauncher/applauncher.qml:217:     }
…
hypr-comp/applauncher/applauncher.qml:228:         id: appFetcher
hypr-comp/applauncher/applauncher.qml:229:         running: true
hypr-comp/applauncher/applauncher.qml:230:         command: ["bash", "-c", "python3 " + paths.qsDir + "/applauncher/app_fetcher.py"]
hypr-comp/applauncher/applauncher.qml:231:         stdout: StdioCollector {
hypr-comp/applauncher/applauncher.qml:232:             onStreamFinished: {
…
hypr-comp/applauncher/applauncher.qml:234:                     if (this.text && this.text.trim().length > 0) {
hypr-comp/applauncher/applauncher.qml:235:                         window.allApps = JSON.parse(this.text);
hypr-comp/applauncher/applauncher.qml:236:                         // Batch-append in chunks to avoid a single long block on
hypr-comp/applauncher/applauncher.qml:237:                         // the main thread that freezes the intro animation.
…
hypr-comp/applauncher/applauncher.qml:240:                         let idx   = 0;
hypr-comp/applauncher/applauncher.qml:241:                         function appendChunk() {
hypr-comp/applauncher/applauncher.qml:242:                             let end = Math.min(idx + chunk, apps.length);
hypr-comp/applauncher/applauncher.qml:243:                             for (; idx < end; idx++) appModel.append(apps[idx]);
…
hypr-comp/applauncher/applauncher.qml:246:                         }
hypr-comp/applauncher/applauncher.qml:247:                         appendChunk();
hypr-comp/applauncher/applauncher.qml:248:                     }
hypr-comp/applauncher/applauncher.qml:249:                 } catch(e) { console.log(e); }
…
hypr-comp/applauncher/applauncher.qml:255: 
hypr-comp/applauncher/applauncher.qml:256:     function handleSearch(query) {
hypr-comp/applauncher/applauncher.qml:257:         window.searchQuery = query.toLowerCase();
hypr-comp/applauncher/applauncher.qml:258:         if (window.searchQuery === "") {
…
hypr-comp/applauncher/applauncher.qml:267:         for (let i = 0; i < appModel.count; i++) {
hypr-comp/applauncher/applauncher.qml:268:             if (appModel.get(i).name.toLowerCase().includes(window.searchQuery)) {
hypr-comp/applauncher/applauncher.qml:269:                 window.selectedAppIndex = i;
hypr-comp/applauncher/applauncher.qml:270:                 window.selectedAppName  = appModel.get(i).name;
hypr-comp/applauncher/applauncher.qml:271:                 window.selectedAppIcon  = appModel.get(i).icon || "";
hypr-comp/applauncher/applauncher.qml:272:                 window.selectedAppExec  = appModel.get(i).exec || "";
hypr-comp/applauncher/applauncher.qml:273:                 centerOnApp(i);
hypr-comp/applauncher/applauncher.qml:274:                 window.sphereZoom = 1.65;
…
hypr-comp/applauncher/applauncher.qml:284: 
hypr-comp/applauncher/applauncher.qml:285:     function launchApp(appName, execStr) {
hypr-comp/applauncher/applauncher.qml:286:         Quickshell.execDetached(["python3", paths.qsDir + "/applauncher/app_fetcher.py", "--log", appName]);
hypr-comp/applauncher/applauncher.qml:287:         Quickshell.execDetached(["bash", "-c", execStr]);
hypr-comp/applauncher/applauncher.qml:288:         closeSequence.start();
hypr-comp/applauncher/applauncher.qml:289:     }
hypr-comp/applauncher/applauncher.qml:290:     Item {
…
hypr-comp/applauncher/applauncher.qml:425:                                 source: model.icon
hypr-comp/applauncher/applauncher.qml:426:                                     ? (model.icon.startsWith("/") ? "file://" + model.icon : "image://icon/" + model.icon)
hypr-comp/applauncher/applauncher.qml:427:                                     : "image://icon/application-x-executable"
hypr-comp/applauncher/applauncher.qml:428:                                 fillMode: Image.PreserveAspectFit
…
hypr-comp/applauncher/applauncher.qml:462:                         anchors.centerIn: parent
hypr-comp/applauncher/applauncher.qml:463:                         active: appNode.isSelected
hypr-comp/applauncher/applauncher.qml:464:                         opacity: appNode.isSelected ? 1.0 : 0.0
hypr-comp/applauncher/applauncher.qml:465:                         scale:   appNode.isSelected ? 1.5 : 0.4
…
hypr-comp/applauncher/applauncher.qml:572:                                                 source: window.selectedAppIcon
hypr-comp/applauncher/applauncher.qml:573:                                                     ? (window.selectedAppIcon.startsWith("/")
hypr-comp/applauncher/applauncher.qml:574:                                                        ? "file://" + window.selectedAppIcon
hypr-comp/applauncher/applauncher.qml:575:                                                        : "image://icon/" + window.selectedAppIcon)
…
hypr-comp/applauncher/applauncher.qml:661:                         cursorShape: Qt.PointingHandCursor
hypr-comp/applauncher/applauncher.qml:662:                         onClicked: window.launchApp(model.name, model.exec)
hypr-comp/applauncher/applauncher.qml:663:                     }
hypr-comp/applauncher/applauncher.qml:664:                 }
…
hypr-comp/applauncher/applauncher.qml:729:                     for (let i = window.selectedAppIndex + 1; i < appModel.count; i++) {
hypr-comp/applauncher/applauncher.qml:730:                         if (appModel.get(i).name.toLowerCase().includes(window.searchQuery)) {
hypr-comp/applauncher/applauncher.qml:731:                             window.selectedAppIndex = i;
hypr-comp/applauncher/applauncher.qml:732:                             window.selectedAppName  = appModel.get(i).name;
hypr-comp/applauncher/applauncher.qml:733:                             window.selectedAppIcon  = appModel.get(i).icon || "";
hypr-comp/applauncher/applauncher.qml:734:                             window.selectedAppExec  = appModel.get(i).exec || "";
hypr-comp/applauncher/applauncher.qml:735:                             window.centerOnApp(i);
hypr-comp/applauncher/applauncher.qml:736:                             window.sphereZoom = 1.65;
…
hypr-comp/applauncher/applauncher.qml:743:                     for (let i = window.selectedAppIndex - 1; i >= 0; i--) {
hypr-comp/applauncher/applauncher.qml:744:                         if (appModel.get(i).name.toLowerCase().includes(window.searchQuery)) {
hypr-comp/applauncher/applauncher.qml:745:                             window.selectedAppIndex = i;
hypr-comp/applauncher/applauncher.qml:746:                             window.selectedAppName  = appModel.get(i).name;
hypr-comp/applauncher/applauncher.qml:747:                             window.selectedAppIcon  = appModel.get(i).icon || "";
hypr-comp/applauncher/applauncher.qml:748:                             window.selectedAppExec  = appModel.get(i).exec || "";
hypr-comp/applauncher/applauncher.qml:749:                             window.centerOnApp(i);
hypr-comp/applauncher/applauncher.qml:750:                             window.sphereZoom = 1.65;
…
hypr-comp/applauncher/applauncher.qml:758:                         window.launchApp(
hypr-comp/applauncher/applauncher.qml:759:                             appModel.get(window.selectedAppIndex).name,
hypr-comp/applauncher/applauncher.qml:760:                             appModel.get(window.selectedAppIndex).exec
hypr-comp/applauncher/applauncher.qml:761:                         );
hypr-comp/applauncher/applauncher.qml:762:                     }
…
hypr-comp/applauncher/applauncher.qml:765:                 Keys.onEscapePressed: {
hypr-comp/applauncher/applauncher.qml:766:                     closeSequence.start();
hypr-comp/applauncher/applauncher.qml:767:                     event.accepted = true;
hypr-comp/applauncher/applauncher.qml:768:                 }
```

### `hypr-comp/moon.qml`

Migration owner: `UI/provider integration (domain sections above)`. Source `hypr-comp/moon.qml:1`, 2337 lines, SHA-256 `761aab25547963b3ec561e490faf2690b8e6ccc8115758422038493fc5b08a29`.

```text
…
hypr-comp/moon.qml:67: 
hypr-comp/moon.qml:68:     property bool weatherRingsActive: false
hypr-comp/moon.qml:69:     property var detailedWeatherData: null
hypr-comp/moon.qml:70: 
…
hypr-comp/moon.qml:117:         id: weatherPoller
hypr-comp/moon.qml:118:         command: [
hypr-comp/moon.qml:119:             "bash", "-c",
hypr-comp/moon.qml:120:             "echo \"$(" + paths.serpantinumDir + "/scripts/weather.sh --current-icon)\"\n" +
hypr-comp/moon.qml:121:             "echo \"$(" + paths.serpantinumDir + "/scripts/weather.sh --current-temp)\"\n" +
hypr-comp/moon.qml:122:             "echo \"$(" + paths.serpantinumDir + "/scripts/weather.sh --current-hex)\""
hypr-comp/moon.qml:123:         ]
hypr-comp/moon.qml:124:         stdout: StdioCollector {
…
hypr-comp/moon.qml:137:         id: detailedWeatherPoller
hypr-comp/moon.qml:138:         command: ["bash", paths.serpantinumDir + "/scripts/weather.sh", "--json"]
hypr-comp/moon.qml:139:         running: true
hypr-comp/moon.qml:140:         stdout: StdioCollector {
hypr-comp/moon.qml:141:             onStreamFinished: {
…
hypr-comp/moon.qml:143:                 if (txt !== "") {
hypr-comp/moon.qml:144:                     try { root.detailedWeatherData = JSON.parse(txt); } catch(e) {}
hypr-comp/moon.qml:145:                 }
hypr-comp/moon.qml:146:             }
…
hypr-comp/moon.qml:149: 
hypr-comp/moon.qml:150:     Timer {
hypr-comp/moon.qml:151:         interval: 150000; running: true; repeat: true; triggeredOnStart: true
hypr-comp/moon.qml:152:         onTriggered: {
hypr-comp/moon.qml:153:             weatherPoller.running = false; weatherPoller.running = true;
…
hypr-comp/moon.qml:157: 
hypr-comp/moon.qml:158:     Timer {
hypr-comp/moon.qml:159:         interval: 1000; running: true; repeat: true; triggeredOnStart: true
hypr-comp/moon.qml:160:         onTriggered: {
hypr-comp/moon.qml:161:             let d = new Date();
…
hypr-comp/moon.qml:185:     property bool showEthernet: ethStatus === "Connected" || (isDesktop && !isWifiOn)
hypr-comp/moon.qml:186:     property bool isSoundActive: !isMuted && parseInt(volPercent) > 0
hypr-comp/moon.qml:187:     property int   batCap:       parseInt(batPercent) || 0
hypr-comp/moon.qml:188:     property bool isCharging:   batStatus === "Charging" || batStatus === "Full"
…
hypr-comp/moon.qml:195:     Process {
hypr-comp/moon.qml:196:         id: kbPoller; running: true
hypr-comp/moon.qml:197:         command: ["bash", "-c", paths.qsDir + "/watchers/kb_fetch.sh"]
hypr-comp/moon.qml:198:         stdout: StdioCollector {
hypr-comp/moon.qml:199:             onStreamFinished: {
…
hypr-comp/moon.qml:205:     }
hypr-comp/moon.qml:206:     Process { id: kbWaiter; command: ["bash", "-c", paths.qsDir + "/watchers/kb_wait.sh"]; onExited: { kbPoller.running = false; kbPoller.running = true; } }
hypr-comp/moon.qml:207: 
hypr-comp/moon.qml:208:     Process {
hypr-comp/moon.qml:209:         id: audioPoller; running: true
hypr-comp/moon.qml:210:         command: ["bash", "-c", paths.qsDir + "/watchers/audio_fetch.sh"]
hypr-comp/moon.qml:211:         stdout: StdioCollector {
hypr-comp/moon.qml:212:             onStreamFinished: {
…
hypr-comp/moon.qml:215:                     try {
hypr-comp/moon.qml:216:                         let d = JSON.parse(t);
hypr-comp/moon.qml:217:                         root.volPercent = d.volume.toString() + "%";
hypr-comp/moon.qml:218:                         root.volIcon    = d.icon;
…
hypr-comp/moon.qml:225:     }
hypr-comp/moon.qml:226:     Process { id: audioWaiter; command: ["bash", "-c", paths.qsDir + "/watchers/audio_wait.sh"]; onExited: { audioPoller.running = false; audioPoller.running = true; } }
hypr-comp/moon.qml:227: 
hypr-comp/moon.qml:228:     Process {
hypr-comp/moon.qml:229:         id: networkPoller; running: true
hypr-comp/moon.qml:230:         command: ["bash", "-c", paths.qsDir + "/watchers/network_fetch.sh"]
hypr-comp/moon.qml:231:         stdout: StdioCollector {
hypr-comp/moon.qml:232:             onStreamFinished: {
…
hypr-comp/moon.qml:235:                     try {
hypr-comp/moon.qml:236:                         let d = JSON.parse(t);
hypr-comp/moon.qml:237:                         root.wifiStatus = d.status;
hypr-comp/moon.qml:238:                         root.wifiIcon   = d.icon;
…
hypr-comp/moon.qml:246:     }
hypr-comp/moon.qml:247:     Process { id: networkWaiter; command: ["bash", "-c", paths.qsDir + "/watchers/network_wait.sh"]; onExited: { networkPoller.running = false; networkPoller.running = true; } }
hypr-comp/moon.qml:248: 
hypr-comp/moon.qml:249:     Process {
hypr-comp/moon.qml:250:         id: btPoller; running: true
hypr-comp/moon.qml:251:         command: ["bash", "-c", paths.qsDir + "/watchers/bt_fetch.sh"]
hypr-comp/moon.qml:252:         stdout: StdioCollector {
hypr-comp/moon.qml:253:             onStreamFinished: {
…
hypr-comp/moon.qml:256:                     try {
hypr-comp/moon.qml:257:                         let d = JSON.parse(t);
hypr-comp/moon.qml:258:                         root.btStatus = d.status;
hypr-comp/moon.qml:259:                         root.btIcon   = d.icon;
…
hypr-comp/moon.qml:266:     }
hypr-comp/moon.qml:267:     Process { id: btWaiter; command: ["bash", "-c", paths.qsDir + "/watchers/bt_wait.sh"]; onExited: { btPoller.running = false; btPoller.running = true; } }
hypr-comp/moon.qml:268: 
hypr-comp/moon.qml:269:     Process {
hypr-comp/moon.qml:270:         id: batteryPoller; running: true
hypr-comp/moon.qml:271:         command: ["bash", "-c", paths.qsDir + "/watchers/battery_fetch.sh"]
hypr-comp/moon.qml:272:         stdout: StdioCollector {
hypr-comp/moon.qml:273:             onStreamFinished: {
…
hypr-comp/moon.qml:276:                     try {
hypr-comp/moon.qml:277:                         let d = JSON.parse(t);
hypr-comp/moon.qml:278:                         root.batPercent = d.percent.toString() + "%";
hypr-comp/moon.qml:279:                         root.batIcon    = d.icon;
…
hypr-comp/moon.qml:286:     }
hypr-comp/moon.qml:287:     Process { id: batteryWaiter; command: ["bash", "-c", paths.qsDir + "/watchers/battery_wait.sh"]; onExited: { batteryPoller.running = false; batteryPoller.running = true; } }
hypr-comp/moon.qml:288: 
hypr-comp/moon.qml:289:     ListModel {
…
hypr-comp/moon.qml:295:         id: wsDaemon
hypr-comp/moon.qml:296:         command: ["bash", "-c", paths.serpantinumDir + "/scripts/workspaces.sh"]
hypr-comp/moon.qml:297:         running: true
hypr-comp/moon.qml:298:     }
hypr-comp/moon.qml:299: 
…
hypr-comp/moon.qml:301:         id: wsReader
hypr-comp/moon.qml:302:         running: true
hypr-comp/moon.qml:303:         command: ["cat", paths.getRunDir("workspaces") + "/workspaces.json"]
hypr-comp/moon.qml:304:         stdout: StdioCollector {
hypr-comp/moon.qml:305:             onStreamFinished: {
…
hypr-comp/moon.qml:308:                     try {
hypr-comp/moon.qml:309:                         let newData = JSON.parse(txt);
hypr-comp/moon.qml:310:                         while (workspacesModel.count < newData.length)
hypr-comp/moon.qml:311:                             workspacesModel.append({ "wsId": "", "wsState": "" });
hypr-comp/moon.qml:312:                         while (workspacesModel.count > newData.length)
hypr-comp/moon.qml:313:                             workspacesModel.remove(workspacesModel.count - 1);
…
hypr-comp/moon.qml:316:                             if (newData[i].state === "active") newActive = i;
hypr-comp/moon.qml:317:                             if (workspacesModel.get(i).wsState !== newData[i].state)
hypr-comp/moon.qml:318:                                 workspacesModel.setProperty(i, "wsState", newData[i].state);
hypr-comp/moon.qml:319:                             if (workspacesModel.get(i).wsId !== newData[i].id.toString())
hypr-comp/moon.qml:320:                                 workspacesModel.setProperty(i, "wsId", newData[i].id.toString());
hypr-comp/moon.qml:321:                         }
…
hypr-comp/moon.qml:330:     Process {
hypr-comp/moon.qml:331:         id: wsWatcher
hypr-comp/moon.qml:332:         running: true
hypr-comp/moon.qml:333:         command: ["bash", "-c", "inotifywait -qq -e close_write,modify " + paths.getRunDir("workspaces") + "/workspaces.json"]
hypr-comp/moon.qml:334:         onExited: {
hypr-comp/moon.qml:335:             wsReader.running = false; wsReader.running = true;
…
hypr-comp/moon.qml:339: 
hypr-comp/moon.qml:340:     property var musicData: { "status": "Stopped", "title": "", "artUrl": "", "timeStr": "" }
hypr-comp/moon.qml:341:     property bool isMusicActive: musicData.status === "Playing" && musicData.title !== ""
hypr-comp/moon.qml:342: 
hypr-comp/moon.qml:343:     Process {
hypr-comp/moon.qml:344:         id: musicPoller
hypr-comp/moon.qml:345:         command: ["bash", "-c", "bash " + paths.qsDir + "/music/music_info.sh"]
hypr-comp/moon.qml:346:         running: false
hypr-comp/moon.qml:347:         stdout: StdioCollector {
hypr-comp/moon.qml:348:             onStreamFinished: {
…
hypr-comp/moon.qml:351:                     try {
hypr-comp/moon.qml:352:                         let d = JSON.parse(txt);
hypr-comp/moon.qml:353:                         if (d.title !== root.musicData.title ||
hypr-comp/moon.qml:354:                             d.status !== root.musicData.status ||
…
hypr-comp/moon.qml:364:     Process {
hypr-comp/moon.qml:365:         id: mprisWatcher
hypr-comp/moon.qml:366:         running: true
hypr-comp/moon.qml:367:         command: ["bash", "-c", "dbus-monitor --session \"type='signal',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged',arg0='org.mpris.MediaPlayer2.Player'\" \"type='signal',interface='org.mpris.MediaPlayer2.Player',member='Seeked'\" 2>/dev/null | grep -m 1 'member=' > /dev/null || sleep 2"]
hypr-comp/moon.qml:368:         onExited: {
hypr-comp/moon.qml:369:             musicPoller.running = false;
…
hypr-comp/moon.qml:389: 
hypr-comp/moon.qml:390:     Timer {
hypr-comp/moon.qml:391:         id: globalEngine
hypr-comp/moon.qml:392:         interval: 16
hypr-comp/moon.qml:393:         running: true
hypr-comp/moon.qml:394:         repeat: true
hypr-comp/moon.qml:395:         onTriggered: {
…
hypr-comp/moon.qml:449: 
hypr-comp/moon.qml:450:     function project3D(bx, by, bz, isSatellite) {
hypr-comp/moon.qml:451:         let x = bx;
hypr-comp/moon.qml:452:         let y = by;
…
hypr-comp/moon.qml:470: 
hypr-comp/moon.qml:471:     function repositionWeatherRing() {
hypr-comp/moon.qml:472:         let satAngles = [
hypr-comp/moon.qml:473:             root.gaugeAngle + root.gaugeRepositionOffset,   
…
hypr-comp/moon.qml:505:         
hypr-comp/moon.qml:506:     function repositionGaugeSatellite() {
hypr-comp/moon.qml:507:         const numNodes = 8;
hypr-comp/moon.qml:508:         const spacing = 2 * Math.PI / numNodes;
…
hypr-comp/moon.qml:530:         if (weatherRingsActive) {
hypr-comp/moon.qml:531:             repositionWeatherRing();
hypr-comp/moon.qml:532:             repositionGaugeSatellite();
hypr-comp/moon.qml:533:         } else {
hypr-comp/moon.qml:534:             gaugeBaseY = 0.15;
…
hypr-comp/moon.qml:547: 
hypr-comp/moon.qml:548:         Component.onCompleted: entranceAnimation.start()
hypr-comp/moon.qml:549: 
hypr-comp/moon.qml:550:         ParallelAnimation {
hypr-comp/moon.qml:551:             id: entranceAnimation
hypr-comp/moon.qml:552:             NumberAnimation { target: windowContent; property: "opacity"; to: 1.0; duration: 500; easing.type: Easing.OutExpo }
hypr-comp/moon.qml:553:             NumberAnimation { target: windowContent; property: "scale";   to: 1.0; duration: 600; easing.type: Easing.OutExpo }
hypr-comp/moon.qml:554:         }
hypr-comp/moon.qml:555: 
…
hypr-comp/moon.qml:1070:                     anchors.right: parent.right; anchors.rightMargin: 8
hypr-comp/moon.qml:1071:                     SequentialAnimation on opacity { loops: Animation.Infinite; running: true
hypr-comp/moon.qml:1072:                         NumberAnimation { from: 1; to: 0.15; duration: 900; easing.type: Easing.InOutSine }
hypr-comp/moon.qml:1073:                         NumberAnimation { from: 0.15; to: 1; duration: 900; easing.type: Easing.InOutSine }
…
hypr-comp/moon.qml:1400:                                 Rectangle { width: 3; height: 3; radius: 1.5; color: root.blue
hypr-comp/moon.qml:1401:                                     SequentialAnimation on opacity { loops: Animation.Infinite; running: true
hypr-comp/moon.qml:1402:                                         NumberAnimation { from: 1; to: 0.1; duration: 250 }
hypr-comp/moon.qml:1403:                                         NumberAnimation { from: 0.1; to: 1; duration: 250 }
…
hypr-comp/moon.qml:1407:                                 Rectangle { width: 3; height: 3; radius: 1.5; color: root.teal
hypr-comp/moon.qml:1408:                                     SequentialAnimation on opacity { loops: Animation.Infinite; running: true
hypr-comp/moon.qml:1409:                                         PauseAnimation { duration: 300 }
hypr-comp/moon.qml:1410:                                         NumberAnimation { from: 1; to: 0.0; duration: 350 }
…
hypr-comp/moon.qml:1618:                         anchors.right: parent.right; anchors.rightMargin: 7
hypr-comp/moon.qml:1619:                         SequentialAnimation on opacity { loops: Animation.Infinite; running: true
hypr-comp/moon.qml:1620:                             NumberAnimation { from: 1; to: 0.1; duration: 800; easing.type: Easing.InOutSine }
hypr-comp/moon.qml:1621:                             NumberAnimation { from: 0.1; to: 1; duration: 800; easing.type: Easing.InOutSine }
…
hypr-comp/moon.qml:1643:                             }
hypr-comp/moon.qml:1644:                             MouseArea { id: kbPillMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: Quickshell.execDetached(["hyprctl", "switchxkblayout", "main", "next"]) }
hypr-comp/moon.qml:1645:                         }
hypr-comp/moon.qml:1646: 
…
hypr-comp/moon.qml:2073:                         SequentialAnimation on opacity {
hypr-comp/moon.qml:2074:                             loops: Animation.Infinite; running: true
hypr-comp/moon.qml:2075:                             NumberAnimation { from: 1; to: 0.2; duration: 500; easing.type: Easing.InOutSine }
hypr-comp/moon.qml:2076:                             NumberAnimation { from: 0.2; to: 1; duration: 500; easing.type: Easing.InOutSine }
…
hypr-comp/moon.qml:2228:                         anchors.right: parent.right; anchors.rightMargin: 7
hypr-comp/moon.qml:2229:                         SequentialAnimation on opacity { loops: Animation.Infinite; running: true
hypr-comp/moon.qml:2230:                             NumberAnimation { from: 1; to: 0.1; duration: 700; easing.type: Easing.OutSine }
hypr-comp/moon.qml:2231:                             NumberAnimation { from: 0.1; to: 1; duration: 700; easing.type: Easing.OutSine }
…
hypr-comp/moon.qml:2295:                                             if (modelData.isMenuOnly || modelData.onlyMenu) {
hypr-comp/moon.qml:2296:                                                 trayMenuAnchor.open();
hypr-comp/moon.qml:2297:                                             } else if (typeof modelData.activate === "function") {
hypr-comp/moon.qml:2298:                                                 modelData.activate();
…
hypr-comp/moon.qml:2305:                                             if (modelData.menu) {
hypr-comp/moon.qml:2306:                                                 trayMenuAnchor.open();
hypr-comp/moon.qml:2307:                                             } else if (typeof modelData.activate === "function") {
hypr-comp/moon.qml:2308:                                                 modelData.activate();
```

### `hypr-comp/shell.qml`

Migration owner: `UI/provider integration (domain sections above)`. Source `hypr-comp/shell.qml:1`, 59 lines, SHA-256 `aabaaa6cca5031e59de82aa06d68f0f6aa2bd0fc0ea5019e38b2233345707f9e`.

```text
hypr-comp/shell.qml:1: //@ pragma UseQApplication
hypr-comp/shell.qml:2: 
hypr-comp/shell.qml:3: import QtQuick
hypr-comp/shell.qml:4: import Quickshell
hypr-comp/shell.qml:5: import Quickshell.Io
hypr-comp/shell.qml:6: import Quickshell.Wayland
hypr-comp/shell.qml:7: 
hypr-comp/shell.qml:8: ShellRoot {
hypr-comp/shell.qml:9:     id: root
hypr-comp/shell.qml:10: 
hypr-comp/shell.qml:11:     property bool launcherVisible: false
hypr-comp/shell.qml:12: 
hypr-comp/shell.qml:13:     IpcHandler {
hypr-comp/shell.qml:14:         target: "hyprComp"
hypr-comp/shell.qml:15:         enabled: true
hypr-comp/shell.qml:16: 
hypr-comp/shell.qml:17:         function openLauncher(): void {
hypr-comp/shell.qml:18:             root.launcherVisible = true;
hypr-comp/shell.qml:19:         }
hypr-comp/shell.qml:20: 
hypr-comp/shell.qml:21:         function closeLauncher(): void {
hypr-comp/shell.qml:22:             root.launcherVisible = false;
hypr-comp/shell.qml:23:         }
hypr-comp/shell.qml:24: 
hypr-comp/shell.qml:25:         function toggleLauncher(): void {
hypr-comp/shell.qml:26:             root.launcherVisible = !root.launcherVisible;
hypr-comp/shell.qml:27:         }
hypr-comp/shell.qml:28:     }
hypr-comp/shell.qml:29: 
hypr-comp/shell.qml:30:     Loader { source: "Stars.qml" }
hypr-comp/shell.qml:31:     Loader { source: "moon.qml" }
hypr-comp/shell.qml:32: 
hypr-comp/shell.qml:33:     PanelWindow {
hypr-comp/shell.qml:34:         id: appLauncherWindow
hypr-comp/shell.qml:35: 
hypr-comp/shell.qml:36:         WlrLayershell.namespace: "hypr-comp-launcher"
hypr-comp/shell.qml:37:         WlrLayershell.layer: WlrLayer.Overlay
hypr-comp/shell.qml:38: 
hypr-comp/shell.qml:39:         exclusionMode: ExclusionMode.Ignore
hypr-comp/shell.qml:40:         focusable: visible
hypr-comp/shell.qml:41:         color: "transparent"
hypr-comp/shell.qml:42:         visible: root.launcherVisible
hypr-comp/shell.qml:43: 
hypr-comp/shell.qml:44:         implicitWidth: screen.width
hypr-comp/shell.qml:45:         implicitHeight: screen.height
hypr-comp/shell.qml:46: 
hypr-comp/shell.qml:47:         Loader {
hypr-comp/shell.qml:48:             anchors.fill: parent
hypr-comp/shell.qml:49:             active: appLauncherWindow.visible
hypr-comp/shell.qml:50:             source: "applauncher/applauncher.qml"
hypr-comp/shell.qml:51:         }
hypr-comp/shell.qml:52: 
hypr-comp/shell.qml:53:         Shortcut {
hypr-comp/shell.qml:54:             sequence: "Escape"
hypr-comp/shell.qml:55:             enabled: appLauncherWindow.visible
hypr-comp/shell.qml:56:             onActivated: root.launcherVisible = false
hypr-comp/shell.qml:57:         }
hypr-comp/shell.qml:58:     }
hypr-comp/shell.qml:59: }
```

### `shell.qml`

Migration owner: `UI/provider integration (domain sections above)`. Source `shell.qml:1`, 4411 lines, SHA-256 `8ec08f7332bd9ad1a3ec8f8632c52f6f12ac7b2f30eda87a72a372eb820fb20c`.

```text
…
shell.qml:17: 
shell.qml:18:     readonly property string homeDir: Quickshell.env("HOME") || ""
shell.qml:19:     readonly property string configDir: homeDir + "/.config/HyprV"
shell.qml:20: 
shell.qml:21:     property var barScreenStates: ({})
…
shell.qml:24:     BarMotion { id: barMotion }
shell.qml:25:     function registerBarScreenState(name, state) {
shell.qml:26:         const next = Object.assign({}, barScreenStates);
shell.qml:27:         next[name] = state;
…
shell.qml:29:     }
shell.qml:30:     function unregisterBarScreenState(name, state) {
shell.qml:31:         if (barScreenStates[name] !== state) return;
shell.qml:32:         const next = Object.assign({}, barScreenStates);
…
shell.qml:39:     property int openWifiPanelCount: 0
shell.qml:40:     signal barStateChanged()
shell.qml:41:     // Desktop always uses the standard bar. Keep the IPC method compatible.
shell.qml:42:     function barExpanded(name) { return true; }
shell.qml:43:     function toggleBar(name) {}
shell.qml:44:     IpcHandler {
shell.qml:45:         target: "bar"
shell.qml:46:         function toggle(monitor: string): void { root.toggleBar(monitor); }
shell.qml:47:         function layoutStatus(monitor: string): string {
shell.qml:48:             const state = root.barScreenStates[monitor];
shell.qml:49:             return JSON.stringify(state ? {expanded: state.expanded, fullscreen: state.fullscreen,
shell.qml:50:                 reveal: state.reveal, fullscreenHide: state.fullscreenHide,
shell.qml:51:                 compactOffset: state.compactOffset, topCornerOffset: state.topCornerOffset,
…
shell.qml:54:         }
shell.qml:55:         function inputStatus(): string {
shell.qml:56:             const bar = root.primaryBarWindow;
shell.qml:57:             return JSON.stringify(bar ? {
shell.qml:58:                 monitor: bar.screen.name, reveal: bar.reveal,
shell.qml:59:                 contentY: -bar.standardTravel * (1 - bar.reveal) + bar.standardOffset * bar.reveal,
…
shell.qml:63:         }
shell.qml:64:         function dataStatus(): string {
shell.qml:65:             return JSON.stringify({expanded: root.anyBarExpanded,
shell.qml:66:                 systemInterval: 1000, detailedStats: root.detailedStatsVisible,
shell.qml:67:                 nativeAudio: root.audioAvailable, volume: root.audioVolumePercent,
shell.qml:68:                 nativeWifi: true, wifiConnected: root.wifiConnected,
…
shell.qml:75:                 spectrum: audioSpectrumProcess.running, notifications: true,
shell.qml:76:                 agentWatcher: true, powerProfile: root.powerProfile,
shell.qml:77:                 batteryAvailable: root.batteryAvailable, batteryPercent: root.batteryPercent,
shell.qml:78:                 darkMode: root.darkMode, panelPolling: controlPanelStatusPoll.active,
…
shell.qml:83:     Connections {
shell.qml:84:         target: Hyprland
shell.qml:85:         function onRawEvent(event) {
shell.qml:86:             if (event.name === "fullscreen") Hyprland.refreshToplevels();
shell.qml:87:             else if (event.name === "configreloaded") barMotion.refresh();
…
shell.qml:108:     property bool wifiConnected: false
shell.qml:109:     property real wifiSignalStrength: 0
shell.qml:110:     property string wifiInterface: ""
shell.qml:111:     property string wifiSsid: ""
…
shell.qml:128:     property string notificationTooltip: ""
shell.qml:129:     property string powerProfileText: "⚖️"
shell.qml:130:     property string fluentLightIconDir: ""
shell.qml:131:     property string fluentDarkIconDir: ""
shell.qml:132:     property string fluentBaseIconDir: ""
shell.qml:133:     property var trayMenuController: null
shell.qml:134:     property bool calendarOpen: false
…
shell.qml:136:     property bool calendarChildOpening: false
shell.qml:137:     property bool calendarChildActive: false
shell.qml:138: 
shell.qml:139:     signal calendarChildReleased()
shell.qml:140: 
shell.qml:141:     property bool wifiEnabled: true
…
shell.qml:147:         : []
shell.qml:148:     readonly property string fanControlHelperPath: configDir + "/quickshell/scripts/fan-control-helper.sh"
shell.qml:149:     readonly property int brightnessUiStepPercent: 2
shell.qml:150:     property int brightnessPercent: 50
…
shell.qml:157:     property bool screenRecording: false
shell.qml:158:     property string powerProfile: "balanced"
shell.qml:159:     property bool preventSleepEnabled: false
shell.qml:160:     property bool demoModeEnabled: false
shell.qml:161:     readonly property var mediaPlayers: Array.from(Mpris.players.values || [])
…
shell.qml:175:     property var audioSpectrumValues: []
shell.qml:176:     property bool agentIslandActive: false
shell.qml:177:     property int agentIslandCount: 0
shell.qml:178:     property var agentIslandSessions: []
…
shell.qml:194:     property bool _pendingBrightnessFinish: false
shell.qml:195:     property bool _brightnessInteractionActive: false
shell.qml:196:     property int _brightnessRequestSequence: 0
shell.qml:197:     property int _pendingChassisFanPercent: -1
…
shell.qml:207:     readonly property string activeNetworkType: networkTypeForInterface(defaultInterface)
shell.qml:208:     readonly property bool wifiConnectionActive: activeNetworkType === "wifi"
shell.qml:209:     readonly property bool wiredConnectionActive: activeNetworkType === "wired"
shell.qml:210:     readonly property bool otherConnectionActive: activeNetworkType === "other"
shell.qml:211: 
shell.qml:212:     readonly property real pillOpacity: 0.8
…
shell.qml:245:         const filtered = values.filter(ws => (ws?.id ?? -1) > -1
shell.qml:246:             && !String(ws?.name ?? "").startsWith("special:")
shell.qml:247:             && ws?.name !== "viewflow-underlay");
shell.qml:248:         filtered.sort((a, b) => (a?.id ?? 0) - (b?.id ?? 0));
…
shell.qml:252:                 urgent: false,
shell.qml:253:                 active: true,
shell.qml:254:                 activate: function () {}
shell.qml:255:             }];
…
shell.qml:352:         }
shell.qml:353:         const basis = batteryInfo?.estimateBasis === "current" ? "按当前功率" : "按均值";
shell.qml:354:         return formatDuration(seconds) + " (" + basis + ")";
shell.qml:355:     }
…
shell.qml:389:     Component.onCompleted: syncBluetoothStatusFromModel()
shell.qml:390:     function resetMediaState() {
shell.qml:391:         root.mediaAvailable = false;
shell.qml:392:         root.mediaPlaying = false;
…
shell.qml:399:     }
shell.qml:400:     function updateMediaState(output) {
shell.qml:401:         let available = false;
shell.qml:402:         let playing = false;
…
shell.qml:411:             const line = lines[i].trim();
shell.qml:412:             if (line.startsWith("available=")) {
shell.qml:413:                 available = line.slice(10).trim() === "true";
shell.qml:414:             } else if (line.startsWith("playing=")) {
shell.qml:415:                 playing = line.slice(8).trim() === "true";
shell.qml:416:             } else if (line.startsWith("title=")) {
shell.qml:417:                 title = line.slice(6).trim();
shell.qml:418:             } else if (line.startsWith("artist=")) {
shell.qml:419:                 artist = line.slice(7).trim();
shell.qml:420:             } else if (line.startsWith("player=")) {
shell.qml:421:                 player = line.slice(7).trim();
shell.qml:422:             } else if (line.startsWith("art_url=")) {
shell.qml:423:                 artUrl = line.slice(8).trim();
shell.qml:424:             } else if (line.startsWith("position=")) {
shell.qml:425:                 const parsed = Number(line.slice(9).trim());
shell.qml:426:                 positionSeconds = isFinite(parsed) ? parsed : 0;
shell.qml:427:             } else if (line.startsWith("length=")) {
shell.qml:428:                 const parsed = Number(line.slice(7).trim());
shell.qml:429:                 lengthSeconds = isFinite(parsed) ? parsed : 0;
…
shell.qml:454:     }
shell.qml:455:     function resetAgentIslandState() {
shell.qml:456:         root.agentIslandActive = false;
shell.qml:457:         root.agentIslandCount = 0;
…
shell.qml:459:     }
shell.qml:460:     function updateAgentIslandState(raw) {
shell.qml:461:         if (!raw) {
shell.qml:462:             resetAgentIslandState();
shell.qml:463:             return;
shell.qml:464:         }
shell.qml:465:         try {
shell.qml:466:             const data = typeof raw === "string" ? JSON.parse(raw) : raw;
shell.qml:467:             root.agentIslandActive = !!data.active;
shell.qml:468:             root.agentIslandCount = Number(data.count) || 0;
…
shell.qml:470:         } catch (_) {
shell.qml:471:             resetAgentIslandState();
shell.qml:472:         }
shell.qml:473:     }
shell.qml:474:     function isWifiInterfaceName(name) {
shell.qml:475:         const iface = (name || "").toLowerCase();
shell.qml:476:         return iface.startsWith("wl") || iface.startsWith("wlan") || iface.startsWith("wifi");
shell.qml:477:     }
shell.qml:478:     function isWiredInterfaceName(name) {
shell.qml:479:         const iface = (name || "").toLowerCase();
shell.qml:480:         return iface.startsWith("en") || iface.startsWith("eth");
shell.qml:481:     }
shell.qml:482:     function networkTypeForInterface(name) {
shell.qml:483:         if (isWifiInterfaceName(name)) {
shell.qml:484:             return "wifi";
…
shell.qml:488:         }
shell.qml:489:         return name ? "other" : "offline";
shell.qml:490:     }
shell.qml:491:     readonly property string networkIcon: {
…
shell.qml:534:         property real moduleHeight: 38
shell.qml:535:         property bool interactive: false
shell.qml:536:         property bool wheelInteractive: false
shell.qml:537:         property bool hoverable: false
shell.qml:538:         property bool highlighted: false
…
shell.qml:547: 
shell.qml:548:         signal leftClicked()
shell.qml:549:         signal rightClicked()
shell.qml:550:         signal wheelUp()
shell.qml:551:         signal wheelDown()
shell.qml:552: 
shell.qml:553:         implicitWidth: collapseWhenHidden && !visible ? 0 : naturalImplicitWidth
…
shell.qml:633: 
shell.qml:634:         function topEntry() {
shell.qml:635:             return entryStack.count ? entryStack.get(entryStack.count - 1).handle : null;
shell.qml:636:         }
shell.qml:637: 
shell.qml:638:         function hydrateMenu(handle) {
shell.qml:639:             if (!handle) {
shell.qml:640:                 return;
…
shell.qml:648:             submenuHydrator.menu = handle;
shell.qml:649:             submenuHydrator.open();
shell.qml:650:             hydratorOpen = true;
shell.qml:651:             Qt.callLater(function() {
…
shell.qml:663: 
shell.qml:664:         function entryIndicator(entry) {
shell.qml:665:             if (!entry || entry.buttonType === undefined || entry.buttonType === 0) {
shell.qml:666:                 return "";
…
shell.qml:676: 
shell.qml:677:         function scheduleOpenAnimation() {
shell.qml:678:             openAnimationPending = true;
shell.qml:679:             menuChrome.prepareOpenAnimation();
shell.qml:680:             openAnimationTimer.restart();
shell.qml:681:         }
shell.qml:682: 
shell.qml:683:         function openFor(item, source, window) {
shell.qml:684:             if (!item || !item.hasMenu || !source || !window) {
shell.qml:685:                 return;
…
shell.qml:693:             menuVisible = true;
shell.qml:694:             positionTimer.restart();
shell.qml:695:             if (trayMenuWindow.visible) {
shell.qml:696:                 if (rootMenuEntry && typeof rootMenuEntry.updateLayout === "function") {
…
shell.qml:703:                 trayMenuWindow.updateMenuPosition();
shell.qml:704:                 scheduleOpenAnimation();
shell.qml:705:             } else {
shell.qml:706:                 trayMenuWindow.visible = true;
…
shell.qml:709: 
shell.qml:710:         function closeMenu() {
shell.qml:711:             if ((!menuVisible && !animatingClose) || !trayMenuWindow.visible) {
shell.qml:712:                 menuVisible = false;
…
shell.qml:720:             animatingClose = true;
shell.qml:721:             closeTimer.stop();
shell.qml:722:             menuChrome.playCloseAnimation();
shell.qml:723:         }
shell.qml:724: 
shell.qml:725:         function showSubMenu(entry) {
shell.qml:726:             if (!entry || !entry.hasChildren) {
shell.qml:727:                 return;
…
shell.qml:736:             hydrateMenu(handle);
shell.qml:737:             positionTimer.restart();
shell.qml:738:         }
shell.qml:739: 
shell.qml:740:         function goBack() {
shell.qml:741:             if (!entryStack.count) {
shell.qml:742:                 return;
…
shell.qml:744:             entryStack.remove(entryStack.count - 1);
shell.qml:745:             positionTimer.restart();
shell.qml:746:         }
shell.qml:747: 
shell.qml:748:         function triggerEntry(entry) {
shell.qml:749:             if (!entry || entry.isSeparator || entry.enabled === false) {
shell.qml:750:                 return;
…
shell.qml:760:             }
shell.qml:761:             closeTimer.restart();
shell.qml:762:         }
shell.qml:763: 
shell.qml:764:         Timer {
shell.qml:765:             id: positionTimer
shell.qml:766: 
shell.qml:767:             interval: 0
shell.qml:768:             repeat: false
shell.qml:769:             onTriggered: trayMenuWindow.updateMenuPosition()
…
shell.qml:771: 
shell.qml:772:         Timer {
shell.qml:773:             id: openAnimationTimer
shell.qml:774: 
shell.qml:775:             interval: 16
shell.qml:776:             repeat: false
shell.qml:777:             onTriggered: {
shell.qml:778:                 if (!trayMenuWindow.visible || !trayMenuPopupRoot.menuVisible || trayMenuPopupRoot.animatingClose) {
shell.qml:779:                     trayMenuPopupRoot.openAnimationPending = false;
shell.qml:780:                     return;
shell.qml:781:                 }
shell.qml:782:                 if (menuContent.implicitHeight <= 0) {
shell.qml:783:                     openAnimationTimer.restart();
shell.qml:784:                     return;
shell.qml:785:                 }
shell.qml:786:                 trayMenuPopupRoot.openAnimationPending = false;
shell.qml:787:                 trayMenuWindow.updateMenuPosition();
shell.qml:788:                 menuChrome.playOpenAnimation();
…
shell.qml:791: 
shell.qml:792:         Timer {
shell.qml:793:             id: closeTimer
shell.qml:794: 
shell.qml:795:             interval: 80
shell.qml:796:             repeat: false
shell.qml:797:             onTriggered: trayMenuPopupRoot.closeMenu()
…
shell.qml:799: 
shell.qml:800:         Timer {
shell.qml:801:             id: clearTimer
shell.qml:802: 
shell.qml:803:             interval: 120
shell.qml:804:             repeat: false
shell.qml:805:             onTriggered: {
…
shell.qml:860: 
shell.qml:861:             function updateMenuPosition() {
shell.qml:862:                 if (!visible || !trayMenuPopupRoot.sourceItem || !screen) {
shell.qml:863:                     return;
…
shell.qml:893:                     menuFocusScope.forceActiveFocus();
shell.qml:894:                     updateMenuPosition();
shell.qml:895:                     if (!trayMenuPopupRoot.animatingClose) {
shell.qml:896:                         trayMenuPopupRoot.scheduleOpenAnimation();
…
shell.qml:904:                     trayMenuPopupRoot.hydratorOpen = false;
shell.qml:905:                     trayMenuPopupRoot.openAnimationPending = false;
shell.qml:906:                     openAnimationTimer.stop();
shell.qml:907:                     menuChrome.stopAnimations();
shell.qml:908:                     menuChrome.resetAnimationState();
shell.qml:909:                     clearTimer.restart();
shell.qml:910:                 }
shell.qml:911:             }
…
shell.qml:954:                 onFullPanelHeightChanged: {
shell.qml:955:                     if (trayMenuPopupRoot.openAnimationPending) {
shell.qml:956:                         positionTimer.restart();
shell.qml:957:                         openAnimationTimer.restart();
shell.qml:958:                         return;
shell.qml:959:                     }
shell.qml:960:                     if (trayMenuWindow.visible && !trayMenuPopupRoot.animatingClose) {
shell.qml:961:                         if (menuChrome.openAnimationRunning || menuChrome.closeAnimationRunning) {
shell.qml:962:                             positionTimer.restart();
shell.qml:963:                             return;
shell.qml:964:                         }
…
shell.qml:967:                         contentOffset = 0;
shell.qml:968:                     } else if (!menuChrome.openAnimationRunning && !menuChrome.closeAnimationRunning) {
shell.qml:969:                         revealHeight = fullPanelHeight;
shell.qml:970:                         if (!trayMenuWindow.visible) {
…
shell.qml:974:                     }
shell.qml:975:                     positionTimer.restart();
shell.qml:976:                 }
shell.qml:977: 
…
shell.qml:981:                     }
shell.qml:982:                     positionTimer.restart();
shell.qml:983:                 }
shell.qml:984: 
…
shell.qml:1004:                         onImplicitHeightChanged: {
shell.qml:1005:                             positionTimer.restart();
shell.qml:1006:                             if (trayMenuPopupRoot.openAnimationPending) {
shell.qml:1007:                                 openAnimationTimer.restart();
shell.qml:1008:                             }
shell.qml:1009:                         }
…
shell.qml:1157: 
shell.qml:1158:         function openFor(source, window, items) {
shell.qml:1159:             const nextItems = Array.isArray(items) ? items : [];
shell.qml:1160:             if (!source || !window || nextItems.length <= 0) {
…
shell.qml:1166:             popupRequested = true;
shell.qml:1167:             positionTimer.restart();
shell.qml:1168:             if (overflowWindow.visible) {
shell.qml:1169:                 overflowWindow.updatePopupPosition();
…
shell.qml:1174: 
shell.qml:1175:         function closePopup() {
shell.qml:1176:             popupRequested = false;
shell.qml:1177:             overflowWindow.visible = false;
…
shell.qml:1179: 
shell.qml:1180:         function toggleFor(source, window, items) {
shell.qml:1181:             if (overflowWindow.visible && sourceItem === source && parentWindow === window) {
shell.qml:1182:                 closePopup();
shell.qml:1183:                 return;
shell.qml:1184:             }
…
shell.qml:1189:             if (!trayItems || trayItems.length <= 0) {
shell.qml:1190:                 closePopup();
shell.qml:1191:             } else {
shell.qml:1192:                 positionTimer.restart();
shell.qml:1193:             }
shell.qml:1194:         }
shell.qml:1195: 
shell.qml:1196:         Timer {
shell.qml:1197:             id: positionTimer
shell.qml:1198: 
shell.qml:1199:             interval: 0
shell.qml:1200:             repeat: false
shell.qml:1201:             onTriggered: overflowWindow.updatePopupPosition()
…
shell.qml:1223:             onWidthChanged: if (visible) {
shell.qml:1224:                 updatePopupPosition();
shell.qml:1225:             }
shell.qml:1226:             onHeightChanged: if (visible) {
shell.qml:1227:                 updatePopupPosition();
shell.qml:1228:             }
shell.qml:1229:             onVisibleChanged: {
…
shell.qml:1231:                     overflowFocus.forceActiveFocus();
shell.qml:1232:                     updatePopupPosition();
shell.qml:1233:                 } else {
shell.qml:1234:                     overflowPopupRoot.sourceItem = null;
…
shell.qml:1239: 
shell.qml:1240:             function updatePopupPosition() {
shell.qml:1241:                 if (!visible || !overflowPopupRoot.sourceItem || !screen) {
shell.qml:1242:                     return;
…
shell.qml:1324: 
shell.qml:1325:         function openFor(source, window) {
shell.qml:1326:             if (!source || !window) {
shell.qml:1327:                 return;
…
shell.qml:1333:             batteryInfoPoll.refresh();
shell.qml:1334:             positionTimer.restart();
shell.qml:1335:             if (popupWindow.visible) {
shell.qml:1336:                 popupCard.prepareOpenAnimation();
shell.qml:1337:                 openAnimationPending = true;
shell.qml:1338:                 popupOpenTimer.restart();
shell.qml:1339:                 popupWindow.updatePopupPosition();
shell.qml:1340:             } else {
…
shell.qml:1344: 
shell.qml:1345:         function closePopup() {
shell.qml:1346:             if ((!popupRequested && !animatingClose) || !popupWindow.visible) {
shell.qml:1347:                 popupRequested = false;
…
shell.qml:1358: 
shell.qml:1359:         function toggleFor(source, window) {
shell.qml:1360:             if (popupWindow.visible && sourceItem === source && parentWindow === window) {
shell.qml:1361:                 closePopup();
shell.qml:1362:                 return;
shell.qml:1363:             }
…
shell.qml:1366: 
shell.qml:1367:         Timer {
shell.qml:1368:             id: positionTimer
shell.qml:1369: 
shell.qml:1370:             interval: 0
shell.qml:1371:             repeat: false
shell.qml:1372:             onTriggered: popupWindow.updatePopupPosition()
…
shell.qml:1374: 
shell.qml:1375:         Timer {
shell.qml:1376:             id: popupOpenTimer
shell.qml:1377: 
shell.qml:1378:             interval: 16
shell.qml:1379:             repeat: false
shell.qml:1380:             onTriggered: {
shell.qml:1381:                 if (!popupWindow.visible || !batteryPopupRoot.popupRequested || batteryPopupRoot.animatingClose) {
shell.qml:1382:                     batteryPopupRoot.openAnimationPending = false;
shell.qml:1383:                     return;
shell.qml:1384:                 }
shell.qml:1385:                 if (popupContent.implicitHeight <= 0) {
shell.qml:1386:                     popupOpenTimer.restart();
shell.qml:1387:                     return;
shell.qml:1388:                 }
shell.qml:1389:                 batteryPopupRoot.openAnimationPending = false;
shell.qml:1390:                 popupWindow.updatePopupPosition();
shell.qml:1391:                 popupCard.playOpenAnimation();
…
shell.qml:1414:             onWidthChanged: if (visible) {
shell.qml:1415:                 updatePopupPosition();
shell.qml:1416:             }
shell.qml:1417:             onHeightChanged: if (visible) {
shell.qml:1418:                 updatePopupPosition();
shell.qml:1419:             }
shell.qml:1420: 
shell.qml:1421:             function updatePopupPosition() {
shell.qml:1422:                 if (!visible || !batteryPopupRoot.sourceItem || !screen) {
shell.qml:1423:                     return;
…
shell.qml:1445:                 if (visible) {
shell.qml:1446:                     updatePopupPosition();
shell.qml:1447:                     popupFocusScope.forceActiveFocus();
shell.qml:1448:                     if (!batteryPopupRoot.animatingClose) {
shell.qml:1449:                         batteryPopupRoot.openAnimationPending = true;
shell.qml:1450:                         popupCard.prepareOpenAnimation();
shell.qml:1451:                         popupOpenTimer.restart();
shell.qml:1452:                     }
shell.qml:1453:                 } else {
shell.qml:1454:                     batteryPopupRoot.animatingClose = false;
shell.qml:1455:                     batteryPopupRoot.openAnimationPending = false;
shell.qml:1456:                     popupOpenTimer.stop();
shell.qml:1457:                     popupCard.stopAnimations();
shell.qml:1458:                     popupCard.resetAnimationState();
…
shell.qml:1500:                 onFullPanelHeightChanged: {
shell.qml:1501:                     if (batteryPopupRoot.openAnimationPending) {
shell.qml:1502:                         positionTimer.restart();
shell.qml:1503:                         popupOpenTimer.restart();
shell.qml:1504:                         return;
shell.qml:1505:                     }
shell.qml:1506:                     if (popupWindow.visible && !batteryPopupRoot.animatingClose) {
shell.qml:1507:                         if (popupCard.openAnimationRunning || popupCard.closeAnimationRunning) {
shell.qml:1508:                             positionTimer.restart();
shell.qml:1509:                             return;
shell.qml:1510:                         }
…
shell.qml:1513:                         contentOffset = 0;
shell.qml:1514:                     } else if (!popupCard.openAnimationRunning && !popupCard.closeAnimationRunning) {
shell.qml:1515:                         revealHeight = fullPanelHeight;
shell.qml:1516:                         if (!popupWindow.visible) {
…
shell.qml:1520:                     }
shell.qml:1521:                     positionTimer.restart();
shell.qml:1522:                 }
shell.qml:1523: 
…
shell.qml:1527:                     }
shell.qml:1528:                     positionTimer.restart();
shell.qml:1529:                 }
shell.qml:1530: 
…
shell.qml:1549:                     onImplicitHeightChanged: {
shell.qml:1550:                         if (batteryPopupRoot.openAnimationPending) {
shell.qml:1551:                             popupOpenTimer.restart();
shell.qml:1552:                         }
shell.qml:1553:                     }
…
shell.qml:1619: 
shell.qml:1620:     function updateControlPanelState(output) {
shell.qml:1621:         const lines = (output || "").split("\n");
shell.qml:1622:         for (let i = 0; i < lines.length; i++) {
shell.qml:1623:             const line = lines[i].trim();
shell.qml:1624:             if (line.startsWith("wifi_enabled=")) {
shell.qml:1625:                 continue;
shell.qml:1626:             } else if (line.startsWith("brightness=")) {
shell.qml:1627:                 const parsed = Number(line.slice(11).trim());
shell.qml:1628:                 if (isFinite(parsed)) {
…
shell.qml:1630:                 }
shell.qml:1631:             } else if (line.startsWith("dnd=")) {
shell.qml:1632:                 root.dndEnabled = line.slice(4).trim() === "true";
shell.qml:1633:             } else if (line.startsWith("recording=")) {
shell.qml:1634:                 root.screenRecording = line.slice(10).trim() === "true";
shell.qml:1635:             } else if (line.startsWith("power_profile=")) {
shell.qml:1636:                 const profile = line.slice(14).trim();
shell.qml:1637:                 if (profile.length > 0) {
…
shell.qml:1639:                 }
shell.qml:1640:             } else if (line.startsWith("prevent_sleep=")) {
shell.qml:1641:                 root.preventSleepEnabled = line.slice(14).trim() === "true";
shell.qml:1642:             } else if (line.startsWith("demo_mode=")) {
shell.qml:1643:                 root.demoModeEnabled = line.slice(10).trim() === "true";
shell.qml:1644:             } else if (line.startsWith("fan_available=")) {
shell.qml:1645:                 root.fanControlAvailable = line.slice(14).trim() === "true";
shell.qml:1646:             } else if (line.startsWith("fan_chassis_percent=")) {
shell.qml:1647:                 const parsed = Number(line.slice(20).trim());
shell.qml:1648:                 if (isFinite(parsed) && Date.now() - root._lastFanInteractionTimestamp > 300) {
…
shell.qml:1650:                 }
shell.qml:1651:             } else if (line.startsWith("fan_chassis_rpm=")) {
shell.qml:1652:                 const parsed = Number(line.slice(16).trim());
shell.qml:1653:                 if (isFinite(parsed)) {
…
shell.qml:1655:                 }
shell.qml:1656:             } else if (line.startsWith("fan_pump_percent=")) {
shell.qml:1657:                 const parsed = Number(line.slice(17).trim());
shell.qml:1658:                 if (isFinite(parsed) && Date.now() - root._lastFanInteractionTimestamp > 300) {
…
shell.qml:1660:                 }
shell.qml:1661:             } else if (line.startsWith("fan_pump_rpm=")) {
shell.qml:1662:                 const parsed = Number(line.slice(13).trim());
shell.qml:1663:                 if (isFinite(parsed)) {
…
shell.qml:1669: 
shell.qml:1670:     function withAlpha(colorString, alpha) {
shell.qml:1671:         const color = Qt.color(colorString);
shell.qml:1672:         return Qt.rgba(color.r, color.g, color.b, alpha);
…
shell.qml:1674: 
shell.qml:1675:     function formatPower(value, signed) {
shell.qml:1676:         const number = Number(value);
shell.qml:1677:         if (!isFinite(number)) {
…
shell.qml:1692: 
shell.qml:1693:     function formatDuration(totalSeconds) {
shell.qml:1694:         const value = Number(totalSeconds);
shell.qml:1695:         if (!isFinite(value) || value < 0) {
…
shell.qml:1712: 
shell.qml:1713:     function fileUrl(path) {
shell.qml:1714:         return path ? "file://" + path : "";
shell.qml:1715:     }
shell.qml:1716: 
shell.qml:1717:     function wifiSignalBucket(signalPercent) {
shell.qml:1718:         if (signalPercent < 20) {
shell.qml:1719:             return "0";
shell.qml:1720:         }
shell.qml:1721:         if (signalPercent < 40) {
shell.qml:1722:             return "25";
shell.qml:1723:         }
shell.qml:1724:         if (signalPercent < 60) {
shell.qml:1725:             return "50";
shell.qml:1726:         }
shell.qml:1727:         if (signalPercent < 80) {
shell.qml:1728:             return "75";
shell.qml:1729:         }
…
shell.qml:1732: 
shell.qml:1733:     function fluentWifiIconSource(iconName, useDarkVariant) {
shell.qml:1734:         const themeDir = useDarkVariant ? (fluentDarkIconDir || fluentBaseIconDir) : (fluentLightIconDir || fluentBaseIconDir);
shell.qml:1735:         if (!themeDir || !iconName) {
…
shell.qml:1743: 
shell.qml:1744:     function wifiTrayIconSource(enabled, hardwareEnabled, connected, strength, secure) {
shell.qml:1745:         const signalPercent = Math.round((strength || 0) * 100);
shell.qml:1746:         if (darkMode) {
shell.qml:1747:             if (!hardwareEnabled) {
…
shell.qml:1755:             }
shell.qml:1756:             return fluentWifiIconSource("nm-signal-" + wifiSignalBucket(signalPercent) + (secure ? "-secure" : ""), true);
shell.qml:1757:         }
shell.qml:1758: 
…
shell.qml:1767:         }
shell.qml:1768:         return fluentWifiIconSource("nm-signal-" + wifiSignalBucket(signalPercent) + (secure ? "-secure" : ""), false);
shell.qml:1769:     }
shell.qml:1770: 
shell.qml:1771:     function wiredTrayIconSource(connected) {
shell.qml:1772:         if (darkMode) {
shell.qml:1773:             return fluentWifiIconSource(connected ? "network-wired" : "network-wired-disconnected", true);
shell.qml:1774:         }
shell.qml:1775:         return fileUrl(configDir + "/quickshell/assets/tray/" + (connected ? "network-wired-light.svg" : "network-wired-offline-light.svg"));
…
shell.qml:1777: 
shell.qml:1778:     function networkTrayIconSource() {
shell.qml:1779:         if (wiredConnectionActive || otherConnectionActive) {
shell.qml:1780:             return wiredTrayIconSource(true);
…
shell.qml:1782:         if (wifiConnectionActive) {
shell.qml:1783:             return wifiTrayIconSource(true, wifiHardwareEnabled, true, wifiSignalStrength, wifiSecure);
shell.qml:1784:         }
shell.qml:1785:         return wifiTrayIconSource(wifiRadioEnabled, wifiHardwareEnabled, wifiConnected, wifiSignalStrength, wifiSecure);
shell.qml:1786:     }
shell.qml:1787: 
shell.qml:1788:     function humanRate(bytesPerSecond) {
shell.qml:1789:         const value = Math.max(0, bytesPerSecond || 0);
shell.qml:1790:         if (value < 1024) {
…
shell.qml:1801: 
shell.qml:1802:     function usageSeverityColor(value) {
shell.qml:1803:         const number = Number(value);
shell.qml:1804:         if (!isFinite(number)) {
…
shell.qml:1815: 
shell.qml:1816:     function appendHistory(history, value, limit) {
shell.qml:1817:         const next = Array.isArray(history) ? history.slice(0) : [];
shell.qml:1818:         next.push(Math.max(0, Number(value) || 0));
…
shell.qml:1824: 
shell.qml:1825:     function splitSections(text) {
shell.qml:1826:         const sections = {};
shell.qml:1827:         let current = "";
…
shell.qml:1830:             const line = lines[i];
shell.qml:1831:             if (line.startsWith("__") && line.endsWith("__")) {
shell.qml:1832:                 current = line;
shell.qml:1833:                 sections[current] = [];
…
shell.qml:1840: 
shell.qml:1841:     function batteryGlyph(percent) {
shell.qml:1842:         const icons = ["󰂎", "󰁺", "󰁻", "󰁼", "󰁽", "󰁾", "󰁿", "󰂀", "󰂁", "󰂂", "󰁹"];
shell.qml:1843:         const index = Math.max(0, Math.min(icons.length - 1, Math.round(percent / 10)));
…
shell.qml:1846: 
shell.qml:1847:     function resetBatteryInfo() {
shell.qml:1848:         batteryInfo = {
shell.qml:1849:             available: false,
…
shell.qml:1864: 
shell.qml:1865:     function updateBatteryInfo(raw) {
shell.qml:1866:         if (!raw) {
shell.qml:1867:             resetBatteryInfo();
shell.qml:1868:             return;
shell.qml:1869:         }
shell.qml:1870:         try {
shell.qml:1871:             const data = JSON.parse(raw);
shell.qml:1872:             if (data && typeof data === "object") {
shell.qml:1873:                 batteryInfo = data;
…
shell.qml:1876:         } catch (_) {}
shell.qml:1877:         resetBatteryInfo();
shell.qml:1878:     }
shell.qml:1879: 
shell.qml:1880:     function parseNumberMap(text) {
shell.qml:1881:         const result = {};
shell.qml:1882:         const lines = (text || "").split("\n");
…
shell.qml:1891: 
shell.qml:1892:     function parseDefaultInterface(text) {
shell.qml:1893:         const lines = (text || "").trim().split("\n");
shell.qml:1894:         for (let i = 1; i < lines.length; i++) {
…
shell.qml:1902: 
shell.qml:1903:     function interfaceCounters(text, iface) {
shell.qml:1904:         if (!iface) {
shell.qml:1905:             return null;
…
shell.qml:1909:             const line = lines[i].trim();
shell.qml:1910:             if (!line.startsWith(iface + ":")) {
shell.qml:1911:                 continue;
shell.qml:1912:             }
shell.qml:1913:             const parts = line.replace(":", " ").trim().split(/\s+/);
shell.qml:1914:             if (parts.length < 10) {
shell.qml:1915:                 return null;
…
shell.qml:1924: 
shell.qml:1925:     function wifiListIconSource(signalPercent, secure) {
shell.qml:1926:         return fluentWifiIconSource("nm-signal-" + wifiSignalBucket(signalPercent) + (secure ? "-secure" : ""), darkMode);
shell.qml:1927:     }
shell.qml:1928: 
shell.qml:1929:     function wifiSignalGlyph(signalPercent) {
shell.qml:1930:         if (signalPercent < 20) {
shell.qml:1931:             return "󰤯";
shell.qml:1932:         }
shell.qml:1933:         if (signalPercent < 40) {
shell.qml:1934:             return "󰤟";
shell.qml:1935:         }
shell.qml:1936:         if (signalPercent < 60) {
shell.qml:1937:             return "󰤢";
shell.qml:1938:         }
shell.qml:1939:         if (signalPercent < 80) {
shell.qml:1940:             return "󰤥";
shell.qml:1941:         }
…
shell.qml:1944: 
shell.qml:1945:     function wifiTrayGlyph(enabled, connected, strength) {
shell.qml:1946:         if (!enabled) {
shell.qml:1947:             return "󰤭";
…
shell.qml:1951:         }
shell.qml:1952:         return wifiSignalGlyph(Math.round((strength || 0) * 100));
shell.qml:1953:     }
shell.qml:1954: 
shell.qml:1955:     function networkTrayGlyph() {
shell.qml:1956:         if (wiredConnectionActive || otherConnectionActive) {
shell.qml:1957:             return "󰈀";
…
shell.qml:1959:         if (wifiConnectionActive) {
shell.qml:1960:             return wifiTrayGlyph(true, true, wifiSignalStrength);
shell.qml:1961:         }
shell.qml:1962:         return wifiTrayGlyph(wifiRadioEnabled, wifiConnected, wifiSignalStrength);
shell.qml:1963:     }
shell.qml:1964: 
shell.qml:1965:     function updateNotificationState(raw) {
shell.qml:1966:         if (!raw) {
shell.qml:1967:             notificationAlt = "none";
…
shell.qml:1971:         try {
shell.qml:1972:             const data = JSON.parse(raw);
shell.qml:1973:             notificationAlt = data.alt || "none";
shell.qml:1974:             root.dndEnabled = notificationAlt.indexOf("dnd") >= 0;
…
shell.qml:1982: 
shell.qml:1983:     function resetWifiStatus() {
shell.qml:1984:         wifiDevicePresent = false;
shell.qml:1985:         wifiRadioEnabled = false;
…
shell.qml:1990:         wifiSecure = false;
shell.qml:1991:         wifiSignalStrength = 0;
shell.qml:1992:         wifiNetworks = [];
shell.qml:1993:     }
shell.qml:1994: 
shell.qml:1995:     function resetBluetoothStatus() {
shell.qml:1996:         bluetoothPresent = false;
shell.qml:1997:         bluetoothEnabled = false;
…
shell.qml:2002: 
shell.qml:2003:     function cloneWifiNetworks(networks) {
shell.qml:2004:         if (!Array.isArray(networks)) {
shell.qml:2005:             return [];
…
shell.qml:2007:         return networks.map(network => ({
shell.qml:2008:             active: !!network.active,
shell.qml:2009:             ssid: network.ssid || "",
shell.qml:2010:             signal: Number(network.signal) || 0,
shell.qml:2011:             security: network.security || "",
shell.qml:2012:             bars: network.bars || "",
…
shell.qml:2018: 
shell.qml:2019:     function resolveWifiNetworks(data) {
shell.qml:2020:         const nextNetworks = Array.isArray(data.networks) ? data.networks : [];
shell.qml:2021:         if (nextNetworks.length > 0) {
…
shell.qml:2045: 
shell.qml:2046:     function applyWifiStatus(data) {
shell.qml:2047:         const devicePresent = !!data.present;
shell.qml:2048:         const iface = data.iface || "";
shell.qml:2049: 
shell.qml:2050:         resetWifiStatus();
shell.qml:2051:         wifiHardwareEnabled = data.hardwareEnabled !== false;
shell.qml:2052:         wifiDevicePresent = devicePresent;
…
shell.qml:2069:         wifiSecure = (data.security || "").trim().length > 0;
shell.qml:2070:         wifiSignalStrength = wifiConnected ? Math.max(0, Math.min(1, (Number(data.signal) || 0) / 100)) : 0;
shell.qml:2071:         wifiNetworks = resolveWifiNetworks(data);
shell.qml:2072:         _wifiStatusInitialized = true;
…
shell.qml:2074: 
shell.qml:2075:     function updateWifiStatus(raw) {
shell.qml:2076:         if (!raw) {
shell.qml:2077:             if (!_wifiStatusInitialized) {
shell.qml:2078:                 resetWifiStatus();
shell.qml:2079:             }
shell.qml:2080:             return;
…
shell.qml:2082:         try {
shell.qml:2083:             applyWifiStatus(JSON.parse(raw));
shell.qml:2084:         } catch (_) {
shell.qml:2085:             if (!_wifiStatusInitialized) {
shell.qml:2086:                 resetWifiStatus();
shell.qml:2087:             }
shell.qml:2088:         }
…
shell.qml:2090: 
shell.qml:2091:     function normalizeBluetoothIdentifier(value) {
shell.qml:2092:         return (value || "").trim().toUpperCase().replace(/-/g, ":");
shell.qml:2093:     }
shell.qml:2094: 
shell.qml:2095:     function bluetoothNameLooksLikeAddress(name, address) {
shell.qml:2096:         const trimmedName = (name || "").trim();
shell.qml:2097:         if (trimmedName.length === 0) {
…
shell.qml:2107: 
shell.qml:2108:     function bluetoothDeviceName(device) {
shell.qml:2109:         if (!device) {
shell.qml:2110:             return "";
…
shell.qml:2118: 
shell.qml:2119:     function bluetoothDeviceSnapshot(device) {
shell.qml:2120:         if (!device) {
shell.qml:2121:             return null;
…
shell.qml:2137: 
shell.qml:2138:     function sortedBluetoothSnapshots() {
shell.qml:2139:         const devices = Array.isArray(root.bluetoothDeviceObjects) ? root.bluetoothDeviceObjects : [];
shell.qml:2140:         const snapshots = [];
…
shell.qml:2156:             }
shell.qml:2157:             return (a.displayName || a.address).localeCompare(b.displayName || b.address, undefined, {
shell.qml:2158:                 sensitivity: "base"
shell.qml:2159:             });
…
shell.qml:2163: 
shell.qml:2164:     function findBluetoothDevice(address) {
shell.qml:2165:         if (!address) {
shell.qml:2166:             return null;
…
shell.qml:2176: 
shell.qml:2177:     function syncBluetoothStatusFromModel() {
shell.qml:2178:         const adapter = root.bluetoothAdapterObject;
shell.qml:2179:         if (!adapter) {
shell.qml:2180:             resetBluetoothStatus();
shell.qml:2181:             _bluetoothStatusInitialized = true;
shell.qml:2182:             maybeFinishBluetoothAction();
shell.qml:2183:             return;
shell.qml:2184:         }
…
shell.qml:2191:         _bluetoothStatusInitialized = true;
shell.qml:2192:         maybeFinishBluetoothAction();
shell.qml:2193:     }
shell.qml:2194: 
shell.qml:2195:     function resetBluetoothActionState() {
shell.qml:2196:         _bluetoothActionKind = "";
shell.qml:2197:         _bluetoothActionAddress = "";
…
shell.qml:2202:         _bluetoothScanStopRequested = false;
shell.qml:2203:         bluetoothActionTimeout.stop();
shell.qml:2204:         bluetoothScanStopTimer.stop();
shell.qml:2205:     }
shell.qml:2206: 
shell.qml:2207:     function beginBluetoothAction(actionKind, address, label, pendingMessage, successMessage, failureMessage, timeoutMs) {
shell.qml:2208:         if (bluetoothActionBusy) {
shell.qml:2209:             return false;
…
shell.qml:2219:         _bluetoothScanStopRequested = false;
shell.qml:2220:         bluetoothActionTimeout.interval = timeoutMs || 12000;
shell.qml:2221:         bluetoothActionTimeout.restart();
shell.qml:2222:         return true;
shell.qml:2223:     }
shell.qml:2224: 
shell.qml:2225:     function finishBluetoothActionSuccess(message) {
shell.qml:2226:         bluetoothActionBusy = false;
shell.qml:2227:         bluetoothActionMessage = message || _bluetoothActionSuccessMessage || "Bluetooth action complete";
shell.qml:2228:         resetBluetoothActionState();
shell.qml:2229:     }
shell.qml:2230: 
shell.qml:2231:     function finishBluetoothActionFailure(message) {
shell.qml:2232:         bluetoothActionBusy = false;
shell.qml:2233:         bluetoothActionMessage = message || _bluetoothActionFailureMessage || "Bluetooth action failed";
shell.qml:2234:         resetBluetoothActionState();
shell.qml:2235:     }
shell.qml:2236: 
shell.qml:2237:     function maybeFinishBluetoothAction() {
shell.qml:2238:         if (!bluetoothActionBusy || !_bluetoothActionKind) {
shell.qml:2239:             return;
…
shell.qml:2247:             if (adapter && adapter.enabled) {
shell.qml:2248:                 finishBluetoothActionSuccess();
shell.qml:2249:             }
shell.qml:2250:             return;
…
shell.qml:2254:             if (!adapter || !adapter.enabled) {
shell.qml:2255:                 finishBluetoothActionSuccess();
shell.qml:2256:             }
shell.qml:2257:             return;
…
shell.qml:2260:         if (_bluetoothActionKind === "scan") {
shell.qml:2261:             if (adapter && adapter.discovering && !_bluetoothScanStopRequested && !bluetoothScanStopTimer.running) {
shell.qml:2262:                 bluetoothScanStopTimer.restart();
shell.qml:2263:             }
shell.qml:2264:             if (_bluetoothScanStopRequested && (!adapter || !adapter.discovering)) {
shell.qml:2265:                 finishBluetoothActionSuccess();
shell.qml:2266:             }
shell.qml:2267:             return;
…
shell.qml:2271:             if (device && device.connected) {
shell.qml:2272:                 finishBluetoothActionSuccess();
shell.qml:2273:             }
shell.qml:2274:             return;
…
shell.qml:2294:             if (device.connected) {
shell.qml:2295:                 finishBluetoothActionSuccess();
shell.qml:2296:             }
shell.qml:2297:             return;
…
shell.qml:2301:             if (!device || !device.connected) {
shell.qml:2302:                 finishBluetoothActionSuccess();
shell.qml:2303:             }
shell.qml:2304:             return;
…
shell.qml:2313: 
shell.qml:2314:     function updateSystemStats() {
shell.qml:2315:         const sections = splitSections(root.systemSnapshotOutput);
shell.qml:2316:         const statLines = sections.__STAT__ || [];
…
shell.qml:2366:         const memTotal = mem.MemTotal || 0;
shell.qml:2367:         const memAvailable = mem.MemAvailable || (mem.MemFree || 0) + (mem.Buffers || 0) + (mem.Cached || 0);
shell.qml:2368:         if (memTotal > 0) {
shell.qml:2369:             memoryUsage = ((memTotal - memAvailable) / memTotal) * 100;
…
shell.qml:2382: 
shell.qml:2383:         defaultInterface = parseDefaultInterface((sections.__ROUTE__ || []).join("\n"));
shell.qml:2384:         if (!sections.__NET__) {
shell.qml:2385:             _previousRxBytes = -1;
…
shell.qml:2422: 
shell.qml:2423:     function trayItemPriority(item) {
shell.qml:2424:         const id = (item?.id || "").toLowerCase();
shell.qml:2425:         const title = (item?.title || "").toLowerCase();
…
shell.qml:2457: 
shell.qml:2458:     function traySlotCount(visibleCount, totalCount) {
shell.qml:2459:         const total = Math.max(0, Math.floor(Number(totalCount) || 0));
shell.qml:2460:         const visible = Math.max(0, Math.min(total, Math.floor(Number(visibleCount) || 0)));
…
shell.qml:2463: 
shell.qml:2464:     function trayFixedButtonWidth(visibleCount, totalCount) {
shell.qml:2465:         const total = Math.max(0, Math.floor(Number(totalCount) || 0));
shell.qml:2466:         const visible = Math.max(0, Math.min(total, Math.floor(Number(visibleCount) || 0)));
…
shell.qml:2469: 
shell.qml:2470:     function trayItemsWidth(count) {
shell.qml:2471:         return collapsedTrayWidthForSpacing(count, count, trayButtonSpacing);
shell.qml:2472:     }
shell.qml:2473: 
shell.qml:2474:     function collapsedTrayWidthForSpacing(visibleCount, totalCount, spacing) {
shell.qml:2475:         const slots = traySlotCount(visibleCount, totalCount);
shell.qml:2476:         const gaps = Math.max(0, slots - 1);
…
shell.qml:2479: 
shell.qml:2480:     function collapsedTrayWidth(visibleCount, totalCount) {
shell.qml:2481:         return collapsedTrayWidthForSpacing(visibleCount, totalCount, trayButtonSpacing);
shell.qml:2482:     }
shell.qml:2483: 
shell.qml:2484:     function collapsedTrayMinWidth(visibleCount, totalCount) {
shell.qml:2485:         return collapsedTrayWidthForSpacing(visibleCount, totalCount, trayMinButtonSpacing);
shell.qml:2486:     }
shell.qml:2487: 
shell.qml:2488:     function traySpacingForWidth(visibleCount, totalCount, width) {
shell.qml:2489:         const gaps = Math.max(0, traySlotCount(visibleCount, totalCount) - 1);
shell.qml:2490:         if (gaps <= 0) {
…
shell.qml:2495: 
shell.qml:2496:     function trayVisibleCountForBudget(totalCount, budget) {
shell.qml:2497:         const total = Math.max(0, Math.floor(Number(totalCount) || 0));
shell.qml:2498:         if (total <= 0) {
…
shell.qml:2511: 
shell.qml:2512:     function trayIconSource(item) {
shell.qml:2513:         // Use the bundled LAIS icon independently of the installed icon theme.
shell.qml:2514:         if (item?.id === "local-ai-service") {
…
shell.qml:2524:                 let fileName = split[0].substring(split[0].lastIndexOf("/") + 1);
shell.qml:2525:                 if (fileName.startsWith("dropboxstatus")) {
shell.qml:2526:                     fileName = "hicolor/16x16/status/" + fileName;
shell.qml:2527:                 }
…
shell.qml:2530:         }
shell.qml:2531:         if (icon.startsWith("/") && !icon.startsWith("file://")) {
shell.qml:2532:             return "file://" + icon;
shell.qml:2533:         }
shell.qml:2534:         if (icon.indexOf("image://") === 0 || icon.indexOf("qrc:/") === 0 || icon.indexOf("file://") === 0 || icon.indexOf("http://") === 0 || icon.indexOf("https://") === 0) {
shell.qml:2535:             return icon;
shell.qml:2536:         }
…
shell.qml:2539: 
shell.qml:2540:     function openTrayMenu(item, sourceItem, parentWindow) {
shell.qml:2541:         if (!item || !item.hasMenu || !sourceItem || !parentWindow) {
shell.qml:2542:             return;
…
shell.qml:2544:         if (trayMenuController) {
shell.qml:2545:             trayMenuController.openFor(item, sourceItem, parentWindow);
shell.qml:2546:         }
shell.qml:2547:     }
shell.qml:2548: 
shell.qml:2549:     function openWifiManager() {
shell.qml:2550:         runDetached(["sh", "-lc", "if command -v nm-connection-editor >/dev/null 2>&1; then exec nm-connection-editor; elif command -v iwgtk >/dev/null 2>&1; then exec iwgtk; else exec alacritty -t nmtui -e nmtui; fi"]);
shell.qml:2551:     }
shell.qml:2552: 
shell.qml:2553:     function openWifiPanel() {
shell.qml:2554:         if (wifiPanelController && wifiPanelController.available && wifiPanelController.openPopup) {
shell.qml:2555:             wifiPanelController.openPopup();
shell.qml:2556:         }
shell.qml:2557:     }
shell.qml:2558: 
shell.qml:2559:     function refreshWifiStatus() {
shell.qml:2560:         // NetworkManager pushes cached state changes through desktopState.
shell.qml:2561:     }
shell.qml:2562: 
shell.qml:2563:     function openBluetoothManager() {
shell.qml:2564:         runDetached(["sh", "-lc", "if command -v blueman-manager >/dev/null 2>&1; then exec blueman-manager; elif command -v blueberry >/dev/null 2>&1; then exec blueberry; else exec alacritty -t bluetoothctl -e bluetoothctl; fi"]);
shell.qml:2565:     }
shell.qml:2566: 
shell.qml:2567:     function refreshBluetoothStatus() {
shell.qml:2568:         syncBluetoothStatusFromModel();
shell.qml:2569:     }
shell.qml:2570: 
shell.qml:2571:     function startWifiAction(command, pendingMessage, successMessage) {
shell.qml:2572:         if (!command || command.length === 0 || wifiActionRunner.running) {
shell.qml:2573:             return;
…
shell.qml:2581: 
shell.qml:2582:     function wifiSetRadio(enabled) {
shell.qml:2583:         startWifiAction(["sh", root.configDir + "/quickshell/scripts/wifi-action.sh", "toggle", enabled ? "on" : "off"], enabled ? "Turning Wi-Fi on..." : "Turning Wi-Fi off...", enabled ? "Wi-Fi enabled" : "Wi-Fi disabled");
shell.qml:2584:     }
shell.qml:2585: 
shell.qml:2586:     function wifiRescan() {
shell.qml:2587:         startWifiAction(["sh", root.configDir + "/quickshell/scripts/wifi-action.sh", "rescan"], "Scanning for networks...", "Scan started");
shell.qml:2588:     }
shell.qml:2589: 
shell.qml:2590:     function wifiDisconnect() {
shell.qml:2591:         startWifiAction(["sh", root.configDir + "/quickshell/scripts/wifi-action.sh", "disconnect"], "Disconnecting...", "Disconnected");
shell.qml:2592:     }
shell.qml:2593: 
shell.qml:2594:     function wifiConnect(ssid, password, security) {
shell.qml:2595:         const command = ["sh", root.configDir + "/quickshell/scripts/wifi-action.sh", "connect", ssid || "", password || "", security || ""];
shell.qml:2596:         startWifiAction(command, "Connecting to " + (ssid || "network") + "...", "Connection requested for " + (ssid || "network"));
shell.qml:2597:     }
shell.qml:2598: 
shell.qml:2599:     function bluetoothSetPower(enabled) {
shell.qml:2600:         const adapter = root.bluetoothAdapterObject;
shell.qml:2601:         if (!adapter) {
…
shell.qml:2606:             bluetoothActionMessage = enabled ? "Bluetooth already enabled" : "Bluetooth already disabled";
shell.qml:2607:             syncBluetoothStatusFromModel();
shell.qml:2608:             return;
shell.qml:2609:         }
shell.qml:2610:         if (!beginBluetoothAction(enabled ? "toggle-on" : "toggle-off", "", "", enabled ? "Turning Bluetooth on..." : "Turning Bluetooth off...", enabled ? "Bluetooth enabled" : "Bluetooth disabled", "Failed to change Bluetooth power state", 8000)) {
shell.qml:2611:             return;
shell.qml:2612:         }
…
shell.qml:2614:             adapter.enabled = enabled;
shell.qml:2615:             syncBluetoothStatusFromModel();
shell.qml:2616:         } catch (error) {
shell.qml:2617:             finishBluetoothActionFailure(String(error));
…
shell.qml:2620: 
shell.qml:2621:     function bluetoothScan() {
shell.qml:2622:         const adapter = root.bluetoothAdapterObject;
shell.qml:2623:         if (!adapter) {
…
shell.qml:2639:             adapter.discovering = true;
shell.qml:2640:             syncBluetoothStatusFromModel();
shell.qml:2641:             maybeFinishBluetoothAction();
shell.qml:2642:         } catch (error) {
shell.qml:2643:             finishBluetoothActionFailure(String(error));
…
shell.qml:2646: 
shell.qml:2647:     function bluetoothConnect(address, paired, label) {
shell.qml:2648:         const device = findBluetoothDevice(address);
shell.qml:2649:         const name = label || bluetoothDeviceName(device) || address || "device";
…
shell.qml:2651:             bluetoothActionMessage = "Bluetooth device not found";
shell.qml:2652:             syncBluetoothStatusFromModel();
shell.qml:2653:             return;
shell.qml:2654:         }
…
shell.qml:2656:             bluetoothActionMessage = name + " is already connected";
shell.qml:2657:             syncBluetoothStatusFromModel();
shell.qml:2658:             return;
shell.qml:2659:         }
shell.qml:2660:         const needsPairing = !device.paired;
shell.qml:2661:         if (!beginBluetoothAction(needsPairing ? "pair-connect" : "connect", address || "", name, "Connecting to " + name + "...", needsPairing ? "Paired and connected " + name : "Connected to " + name, "Failed to connect " + name, needsPairing ? 45000 : 15000)) {
shell.qml:2662:             return;
shell.qml:2663:         }
…
shell.qml:2669:             }
shell.qml:2670:             syncBluetoothStatusFromModel();
shell.qml:2671:             maybeFinishBluetoothAction();
shell.qml:2672:         } catch (error) {
shell.qml:2673:             finishBluetoothActionFailure(String(error));
…
shell.qml:2676: 
shell.qml:2677:     function bluetoothDisconnect(address, label) {
shell.qml:2678:         const device = findBluetoothDevice(address);
shell.qml:2679:         const name = label || bluetoothDeviceName(device) || address || "device";
…
shell.qml:2681:             bluetoothActionMessage = "Bluetooth device not found";
shell.qml:2682:             syncBluetoothStatusFromModel();
shell.qml:2683:             return;
shell.qml:2684:         }
…
shell.qml:2686:             bluetoothActionMessage = name + " is already disconnected";
shell.qml:2687:             syncBluetoothStatusFromModel();
shell.qml:2688:             return;
shell.qml:2689:         }
…
shell.qml:2694:             device.connected = false;
shell.qml:2695:             syncBluetoothStatusFromModel();
shell.qml:2696:             maybeFinishBluetoothAction();
shell.qml:2697:         } catch (error) {
shell.qml:2698:             finishBluetoothActionFailure(String(error));
…
shell.qml:2701: 
shell.qml:2702:     function bluetoothRemove(address, label) {
shell.qml:2703:         const device = findBluetoothDevice(address);
shell.qml:2704:         const name = label || bluetoothDeviceName(device) || address || "device";
…
shell.qml:2706:             bluetoothActionMessage = "Bluetooth device not found";
shell.qml:2707:             syncBluetoothStatusFromModel();
shell.qml:2708:             return;
shell.qml:2709:         }
…
shell.qml:2714:             device.forget();
shell.qml:2715:             syncBluetoothStatusFromModel();
shell.qml:2716:             maybeFinishBluetoothAction();
shell.qml:2717:         } catch (error) {
shell.qml:2718:             finishBluetoothActionFailure(String(error));
…
shell.qml:2721: 
shell.qml:2722:     function runDetached(command) {
shell.qml:2723:         if (!command || command.length === 0) {
shell.qml:2724:             return;
…
shell.qml:2726:         detachedRunner.command = command;
shell.qml:2727:         detachedRunner.startDetached();
shell.qml:2728:     }
shell.qml:2729: 
shell.qml:2730:     function hasDankCalendarWindow() {
shell.qml:2731:         const model = ToplevelManager.toplevels;
shell.qml:2732:         if (!model || !model.values)
…
shell.qml:2740: 
shell.qml:2741:     function toggleCalendar(screen) {
shell.qml:2742:         if (calendarOpen && calendarScreen === screen) {
shell.qml:2743:             requestCalendarClose();
shell.qml:2744:             return;
shell.qml:2745:         }
…
shell.qml:2749: 
shell.qml:2750:     function requestCalendarClose(force) {
shell.qml:2751:         if (!force && (calendarChildOpening || calendarChildActive))
shell.qml:2752:             return false;
…
shell.qml:2757: 
shell.qml:2758:     function beginCalendarChild() {
shell.qml:2759:         calendarChildOpening = true;
shell.qml:2760:         calendarChildLaunchTimeout.restart();
shell.qml:2761:         requestCalendarClose(true);
shell.qml:2762:     }
shell.qml:2763: 
shell.qml:2764:     function refreshControlPanelStatus() {
shell.qml:2765:         controlPanelStatusPoll.refresh();
shell.qml:2766:     }
shell.qml:2767: 
shell.qml:2768:     function clampBrightnessPercent(value) {
shell.qml:2769:         return Math.max(0, Math.min(100, Math.round(value)));
shell.qml:2770:     }
shell.qml:2771: 
shell.qml:2772:     function snapBrightnessPercent(value) {
shell.qml:2773:         return clampBrightnessPercent(Math.round(clampBrightnessPercent(value) / brightnessUiStepPercent) * brightnessUiStepPercent);
shell.qml:2774:     }
shell.qml:2775: 
shell.qml:2776:     function updateBrightnessPercentLocally(value) {
shell.qml:2777:         const nextValue = snapBrightnessPercent(value);
shell.qml:2778:         brightnessPercent = nextValue;
…
shell.qml:2781: 
shell.qml:2782:     function applyBrightnessPercent(value) {
shell.qml:2783:         const nextValue = updateBrightnessPercentLocally(value);
shell.qml:2784:         _brightnessInteractionActive = true;
…
shell.qml:2788:         brightnessProbeDebounce.stop();
shell.qml:2789:         submitPendingBrightness();
shell.qml:2790:     }
shell.qml:2791: 
shell.qml:2792:     function finishBrightnessPercent(value) {
shell.qml:2793:         const nextValue = updateBrightnessPercentLocally(value);
shell.qml:2794:         _brightnessInteractionActive = false;
…
shell.qml:2797:         _pendingBrightnessFinish = true;
shell.qml:2798:         submitPendingBrightness();
shell.qml:2799:     }
shell.qml:2800: 
shell.qml:2801:     function submitPendingBrightness() {
shell.qml:2802:         if (_pendingBrightnessPercent < 0 || brightnessSetProcess.running) {
shell.qml:2803:             return;
…
shell.qml:2810:         brightnessSetProcess.requestSequence = _brightnessRequestSequence;
shell.qml:2811:         brightnessSetProcess.command = ["hyprctl", "eval", finishing
shell.qml:2812:             ? 'require("hypr.controls").brightness_finish(' + nextValue + ')'
shell.qml:2813:             : 'require("hypr.controls").brightness_set(' + nextValue + ', false, true)'];
…
shell.qml:2816: 
shell.qml:2817:     function clampFanPercent(value) {
shell.qml:2818:         return Math.max(0, Math.min(100, Math.round(value)));
shell.qml:2819:     }
shell.qml:2820: 
shell.qml:2821:     function applyFanPercent(channel, value) {
shell.qml:2822:         if (!fanControlAvailable) {
shell.qml:2823:             return;
…
shell.qml:2835:         }
shell.qml:2836:         if (!fanApplyTimer.running) {
shell.qml:2837:             fanApplyTimer.start();
shell.qml:2838:         }
shell.qml:2839:         fanStatusDebounce.restart();
…
shell.qml:2841: 
shell.qml:2842:     function previewBrightnessDelta(delta) {
shell.qml:2843:         const nextValue = updateBrightnessPercentLocally(brightnessPercent + delta);
shell.qml:2844:         quickAdjustPopup.show("brightness");
…
shell.qml:2848: 
shell.qml:2849:     function refreshBrightnessStatus(showQuickAdjust) {
shell.qml:2850:         if (showQuickAdjust) {
shell.qml:2851:             _showQuickAdjustAfterBrightnessProbe = true;
…
shell.qml:2859: 
shell.qml:2860:     function refreshAudioStatus() {
shell.qml:2861:         root.syncAudioStatus();
shell.qml:2862:     }
shell.qml:2863: 
shell.qml:2864:     function scheduleAudioRefresh() {
shell.qml:2865:         audioFollowupRefresh.restart();
shell.qml:2866:     }
shell.qml:2867: 
shell.qml:2868:     function refreshMediaStatus() {
shell.qml:2869:         mediaStatusPoll.refresh();
shell.qml:2870:     }
shell.qml:2871: 
shell.qml:2872:     function seekMedia(positionSeconds) {
shell.qml:2873:         const rawTarget = Number(positionSeconds);
shell.qml:2874:         if (!isFinite(rawTarget)) {
…
shell.qml:2881:         const command = playerName.length > 0
shell.qml:2882:             ? ["playerctl", "-p", playerName, "position", positionArgument]
shell.qml:2883:             : ["playerctl", "position", positionArgument];
shell.qml:2884: 
shell.qml:2885:         mediaPositionSeconds = target;
…
shell.qml:2889: 
shell.qml:2890:     function focusMediaApp() {
shell.qml:2891:         const playerName = (mediaPlayerName || "").trim();
shell.qml:2892:         if (playerName.length === 0) {
…
shell.qml:2898:             + "pid=\n"
shell.qml:2899:             + "if command -v busctl >/dev/null 2>&1; then\n"
shell.qml:2900:             + "    pid=$(busctl --user status \"org.mpris.MediaPlayer2.${player}\" 2>/dev/null | sed -n 's/^PID=//p' | head -n 1)\n"
shell.qml:2901:             + "fi\n"
shell.qml:2902:             + "case \"$pid\" in ''|*[!0-9]*) pid= ;; esac\n"
shell.qml:2903:             + "if [ -z \"$pid\" ]; then\n"
shell.qml:2904:             + "    case \"$player\" in *instance[0-9]*) pid=${player##*instance} ;; esac\n"
shell.qml:2905:             + "    case \"$pid\" in ''|*[!0-9]*) pid= ;; esac\n"
shell.qml:2906:             + "fi\n"
shell.qml:2907:             + "addr=\n"
shell.qml:2908:             + "if [ -n \"$pid\" ] && command -v hyprctl >/dev/null 2>&1 && command -v jq >/dev/null 2>&1; then\n"
shell.qml:2909:             + "    addr=$(hyprctl clients -j | jq -r --arg pid \"$pid\" '.[] | select((.pid | tostring) == $pid) | .address' | head -n 1)\n"
shell.qml:2910:             + "fi\n"
shell.qml:2911:             + "if [ -n \"$addr\" ] && [ \"$addr\" != null ]; then\n"
shell.qml:2912:             + "    hyprctl dispatch focuswindow \"address:$addr\" >/dev/null\n"
shell.qml:2913:             + "    exit 0\n"
shell.qml:2914:             + "fi\n"
shell.qml:2915:             + "case \"$player\" in\n"
shell.qml:2916:             + "    *cider*|*Cider*) hyprctl dispatch focuswindow 'class:^(Cider)$' >/dev/null ;;\n"
shell.qml:2917:             + "    *spotify*|*Spotify*) hyprctl dispatch focuswindow 'class:^(Spotify)$' >/dev/null ;;\n"
shell.qml:2918:             + "    *) exit 1 ;;\n"
shell.qml:2919:             + "esac\n";
…
shell.qml:2923: 
shell.qml:2924:     function focusAgentIslandSession(sessionId) {
shell.qml:2925:         if (!sessionId || String(sessionId).length === 0) {
shell.qml:2926:             return;
shell.qml:2927:         }
shell.qml:2928:         runDetached(["/usr/bin/python3", root.configDir + "/quickshell/scripts/agent-island-action.py", "focus", String(sessionId)]);
shell.qml:2929:     }
shell.qml:2930: 
shell.qml:2931:     function refreshPowerProfileStatus() {
shell.qml:2932:         root.syncPowerProfile();
shell.qml:2933:     }
…
shell.qml:2937: 
shell.qml:2938:         running: true
shell.qml:2939:         command: ["sh", "-lc", "for base in \"$HOME/.local/share/icons\" /usr/local/share/icons /usr/share/icons; do [ -z \"$light\" ] && [ -d \"$base/Fluent-light\" ] && light=\"$base/Fluent-light\"; [ -z \"$dark\" ] && [ -d \"$base/Fluent-dark\" ] && dark=\"$base/Fluent-dark\"; [ -z \"$base_theme\" ] && [ -d \"$base/Fluent\" ] && base_theme=\"$base/Fluent\"; done; printf 'light=%s\\ndark=%s\\nbase=%s\\n' \"$light\" \"$dark\" \"$base_theme\""]
shell.qml:2940:         stdout: StdioCollector {
shell.qml:2941:             id: fluentIconLocatorStdout
…
shell.qml:2950:                 const line = lines[i];
shell.qml:2951:                 if (line.startsWith("light=")) {
shell.qml:2952:                     root.fluentLightIconDir = line.slice(6).trim();
shell.qml:2953:                 } else if (line.startsWith("dark=")) {
shell.qml:2954:                     root.fluentDarkIconDir = line.slice(5).trim();
shell.qml:2955:                 } else if (line.startsWith("base=")) {
shell.qml:2956:                     root.fluentBaseIconDir = line.slice(5).trim();
shell.qml:2957:                 }
…
shell.qml:2964: 
shell.qml:2965:         running: false
shell.qml:2966:     }
shell.qml:2967: 
…
shell.qml:2973: 
shell.qml:2974:         running: false
shell.qml:2975:         stdout: StdioCollector {
shell.qml:2976:             id: wifiActionStdout
…
shell.qml:2997:         property int requestSequence: 0
shell.qml:2998:         running: false
shell.qml:2999:         onRunningChanged: if (running) requestSequence = root._brightnessRequestSequence
shell.qml:3000:         command: ["hyprctl", "repl", 'return require("hypr.controls").brightness_get()']
shell.qml:3001:         stdout: StdioCollector {
shell.qml:3002:             id: quickAdjustBrightnessProbeStdout
…
shell.qml:3029:         property int requestSequence: 0
shell.qml:3030:         running: false
shell.qml:3031: 
shell.qml:3032:         onExited: function(exitCode) {
…
shell.qml:3043: 
shell.qml:3044:     Timer {
shell.qml:3045:         id: brightnessProbeDebounce
shell.qml:3046: 
shell.qml:3047:         interval: 90
shell.qml:3048:         repeat: false
shell.qml:3049:         onTriggered: root.refreshBrightnessStatus(false)
…
shell.qml:3051: 
shell.qml:3052:     Timer {
shell.qml:3053:         id: fanApplyTimer
shell.qml:3054: 
shell.qml:3055:         interval: 35
shell.qml:3056:         repeat: false
shell.qml:3057:         onTriggered: {
…
shell.qml:3068: 
shell.qml:3069:     Timer {
shell.qml:3070:         id: fanStatusDebounce
shell.qml:3071: 
shell.qml:3072:         interval: 250
shell.qml:3073:         repeat: false
shell.qml:3074:         onTriggered: root.refreshControlPanelStatus()
…
shell.qml:3076: 
shell.qml:3077:     Timer {
shell.qml:3078:         id: bluetoothActionTimeout
shell.qml:3079: 
shell.qml:3080:         interval: 12000
shell.qml:3081:         repeat: false
shell.qml:3082:         onTriggered: finishBluetoothActionFailure()
…
shell.qml:3084: 
shell.qml:3085:     Timer {
shell.qml:3086:         id: bluetoothScanStopTimer
shell.qml:3087: 
shell.qml:3088:         interval: 4000
shell.qml:3089:         repeat: false
shell.qml:3090:         onTriggered: {
…
shell.qml:3100: 
shell.qml:3101:     Timer {
shell.qml:3102:         id: audioFollowupRefresh
shell.qml:3103: 
shell.qml:3104:         interval: 150
shell.qml:3105:         repeat: false
shell.qml:3106:         onTriggered: root.syncAudioStatus()
…
shell.qml:3108: 
shell.qml:3109:     Timer {
shell.qml:3110:         id: mediaFollowupRefresh
shell.qml:3111: 
shell.qml:3112:         interval: 180
shell.qml:3113:         repeat: false
shell.qml:3114:         onTriggered: mediaStatusPoll.refresh()
…
shell.qml:3117:     Connections {
shell.qml:3118:         target: root.bluetoothAdapterObject
shell.qml:3119:         ignoreUnknownSignals: true
shell.qml:3120: 
shell.qml:3121:         function onEnabledChanged() {
shell.qml:3122:             root.syncBluetoothStatusFromModel();
shell.qml:3123:         }
shell.qml:3124: 
shell.qml:3125:         function onDiscoveringChanged() {
shell.qml:3126:             root.syncBluetoothStatusFromModel();
shell.qml:3127:         }
shell.qml:3128: 
shell.qml:3129:         function onPairableChanged() {
shell.qml:3130:             root.syncBluetoothStatusFromModel();
shell.qml:3131:         }
shell.qml:3132: 
shell.qml:3133:         function onStateChanged() {
shell.qml:3134:             root.syncBluetoothStatusFromModel();
shell.qml:3135:         }
…
shell.qml:3138:     Instantiator {
shell.qml:3139:         id: bluetoothDeviceSignalInstantiator
shell.qml:3140: 
shell.qml:3141:         model: root.bluetoothDeviceObjects
…
shell.qml:3145: 
shell.qml:3146:             target: modelData
shell.qml:3147:             ignoreUnknownSignals: true
shell.qml:3148: 
shell.qml:3149:             function onConnectedChanged() {
shell.qml:3150:                 root.syncBluetoothStatusFromModel();
shell.qml:3151:             }
shell.qml:3152: 
shell.qml:3153:             function onPairedChanged() {
shell.qml:3154:                 root.syncBluetoothStatusFromModel();
shell.qml:3155:             }
shell.qml:3156: 
shell.qml:3157:             function onTrustedChanged() {
shell.qml:3158:                 root.syncBluetoothStatusFromModel();
shell.qml:3159:             }
shell.qml:3160: 
shell.qml:3161:             function onBlockedChanged() {
shell.qml:3162:                 root.syncBluetoothStatusFromModel();
shell.qml:3163:             }
shell.qml:3164: 
shell.qml:3165:             function onNameChanged() {
shell.qml:3166:                 root.syncBluetoothStatusFromModel();
shell.qml:3167:             }
shell.qml:3168: 
shell.qml:3169:             function onDeviceNameChanged() {
shell.qml:3170:                 root.syncBluetoothStatusFromModel();
shell.qml:3171:             }
shell.qml:3172: 
shell.qml:3173:             function onIconChanged() {
shell.qml:3174:                 root.syncBluetoothStatusFromModel();
shell.qml:3175:             }
shell.qml:3176: 
shell.qml:3177:             function onStateChanged() {
shell.qml:3178:                 root.syncBluetoothStatusFromModel();
shell.qml:3179:             }
shell.qml:3180: 
shell.qml:3181:             function onPairingChanged() {
shell.qml:3182:                 root.syncBluetoothStatusFromModel();
shell.qml:3183:             }
shell.qml:3184: 
shell.qml:3185:             function onBondedChanged() {
shell.qml:3186:                 root.syncBluetoothStatusFromModel();
shell.qml:3187:             }
…
shell.qml:3201:         id: desktopState
shell.qml:3202:         command: ["/usr/bin/python3", root.configDir + "/quickshell/scripts/desktop-state.py"]
shell.qml:3203:         onReceived: line => {
shell.qml:3204:             try {
shell.qml:3205:                 const message = JSON.parse(line);
shell.qml:3206:                 if (message.type === "system") {
shell.qml:3207:                     root.systemSnapshotOutput = message.data;
…
shell.qml:3221: 
shell.qml:3222:         path: (Quickshell.env("XDG_STATE_HOME") || root.homeDir + "/.local/state") + "/hyprv/quickshell/ui-state.env"
shell.qml:3223:         watchChanges: true
shell.qml:3224:         onFileChanged: reload()
shell.qml:3225:         onLoaded: root.darkMode = text().split("\n").some(line => line.trim() === "theme=dark")
…
shell.qml:3230: 
shell.qml:3231:         running: true
shell.qml:3232:         command: ["sh", "-c", "command -v cava >/dev/null 2>&1"]
shell.qml:3233:         onExited: function(exitCode) {
shell.qml:3234:             root.audioSpectrumCavaAvailable = exitCode === 0;
…
shell.qml:3240: 
shell.qml:3241:         running: root.mediaUiVisible && root.audioSpectrumCavaAvailable && root.mediaAvailable && root.mediaPlaying
shell.qml:3242:         command: [root.configDir + "/quickshell/scripts/audio-spectrum.sh"]
shell.qml:3243: 
shell.qml:3244:         onRunningChanged: {
…
shell.qml:3280: 
shell.qml:3281:         command: ["swaync-client", "-swb"]
shell.qml:3282:         onReceived: line => root.updateNotificationState(line)
shell.qml:3283:     }
…
shell.qml:3291: 
shell.qml:3292:     function syncAudioStatus() {
shell.qml:3293:         root.audioAvailable = !!(Pipewire.ready && defaultAudioSink?.ready && defaultSinkAudio);
shell.qml:3294:         root.audioMuted = root.audioAvailable && defaultSinkAudio.muted;
shell.qml:3295:         root.audioVolumePercent = root.audioAvailable ? Math.max(0, Math.round(defaultSinkAudio.volume * 100)) : 0;
…
shell.qml:3298:     Connections {
shell.qml:3299:         target: Pipewire
shell.qml:3300:         function onReadyChanged() { root.syncAudioStatus(); }
shell.qml:3301:     }
shell.qml:3302:     onDefaultAudioSinkChanged: root.syncAudioStatus()
…
shell.qml:3304:     Connections {
shell.qml:3305:         target: root.defaultAudioSink
shell.qml:3306:         function onReadyChanged() { root.syncAudioStatus(); }
shell.qml:3307:     }
shell.qml:3308:     Connections {
shell.qml:3309:         target: root.defaultSinkAudio
shell.qml:3310:         function onMutedChanged() { root.syncAudioStatus(); }
shell.qml:3311:         function onVolumesChanged() { root.syncAudioStatus(); }
shell.qml:3312:     }
shell.qml:3313: 
…
shell.qml:3316: 
shell.qml:3317:         active: root.mediaUiVisible && root.mediaPlayers.length > 0
shell.qml:3318:         interval: 1000
shell.qml:3319:         command: ["sh", root.configDir + "/quickshell/scripts/media-status.sh"]
shell.qml:3320:         onUpdated: function(output, exitCode) {
shell.qml:3321:             if (exitCode === 0) {
…
shell.qml:3328: 
shell.qml:3329:     readonly property int currentPowerProfile: PowerProfiles.profile
shell.qml:3330:     onCurrentPowerProfileChanged: root.syncPowerProfile()
shell.qml:3331: 
shell.qml:3332:     function syncPowerProfile() {
shell.qml:3333:         root.powerProfile = currentPowerProfile === PowerProfile.Performance ? "performance"
shell.qml:3334:             : (currentPowerProfile === PowerProfile.PowerSaver ? "power-saver" : "balanced");
shell.qml:3335:         root.powerProfileText = currentPowerProfile === PowerProfile.Performance ? "⚡"
shell.qml:3336:             : (currentPowerProfile === PowerProfile.PowerSaver ? "🔋" : "⚖️");
…
shell.qml:3341: 
shell.qml:3342:         active: batteryInfoPopup.openVisible
shell.qml:3343:         interval: 1000
shell.qml:3344:         command: ["sh", root.configDir + "/quickshell/scripts/battery-info.sh"]
shell.qml:3345:         onUpdated: function(output, exitCode) {
shell.qml:3346:             if (exitCode === 0) {
…
shell.qml:3380: 
shell.qml:3381:         active: controlPanelPopup.popupRequested
shell.qml:3382:         interval: 1500
shell.qml:3383:         command: ["sh", root.configDir + "/quickshell/scripts/control-panel-status.sh"]
shell.qml:3384:         onUpdated: function(output, exitCode) {
shell.qml:3385:             if (exitCode === 0) {
…
shell.qml:3403:     IpcHandler {
shell.qml:3404:         target: "controlPanel"
shell.qml:3405:         enabled: true
shell.qml:3406: 
shell.qml:3407:         function toggle() {
shell.qml:3408:             controlPanelPopup.toggleCentered(root.primaryBarWindow);
shell.qml:3409:         }
…
shell.qml:3412:     IpcHandler {
shell.qml:3413:         target: "calendar"
shell.qml:3414:         enabled: true
shell.qml:3415: 
shell.qml:3416:         function toggle() {
shell.qml:3417:             if (root.primaryBarWindow) {
shell.qml:3418:                 root.toggleCalendar(root.primaryBarWindow.modelData);
…
shell.qml:3423:     IpcHandler {
shell.qml:3424:         target: "quickAdjust"
shell.qml:3425:         enabled: true
shell.qml:3426: 
shell.qml:3427:         function showBrightness() {
shell.qml:3428:             quickAdjustPopup.show("brightness");
shell.qml:3429:             root.refreshBrightnessStatus(false);
…
shell.qml:3431: 
shell.qml:3432:         function showBrightnessLevel(level: int) {
shell.qml:3433:             const parsed = Number(level);
shell.qml:3434:             if (isFinite(parsed)) {
…
shell.qml:3441: 
shell.qml:3442:         function showBrightnessIncrease() {
shell.qml:3443:             root.previewBrightnessDelta(root.brightnessUiStepPercent);
shell.qml:3444:         }
shell.qml:3445: 
shell.qml:3446:         function showBrightnessDecrease() {
shell.qml:3447:             root.previewBrightnessDelta(-root.brightnessUiStepPercent);
shell.qml:3448:         }
shell.qml:3449: 
shell.qml:3450:         function showVolume() {
shell.qml:3451:             quickAdjustPopup.show("volume");
shell.qml:3452:             root.refreshAudioStatus();
…
shell.qml:3565:             property real islandCurrentHeight: 38
shell.qml:3566:             readonly property bool calendarVisible: root.calendarOpen && root.calendarScreen === modelData
shell.qml:3567:             property bool calendarPanelPresented: false
shell.qml:3568:             property real calendarProgress: 0
…
shell.qml:3601:                 if (calendarVisible) {
shell.qml:3602:                     calendarLeaveTimer.stop();
shell.qml:3603:                     calendarPanelCloseTimer.stop();
shell.qml:3604:                     calendarDismissArmed = false;
shell.qml:3605:                     calendarDismissArmTimer.restart();
shell.qml:3606:                     calendarPanelPresented = true;
shell.qml:3607:                     Qt.callLater(() => calendarProgress = 1);
shell.qml:3608:                 } else {
shell.qml:3609:                     calendarLeaveTimer.stop();
shell.qml:3610:                     calendarDismissArmTimer.stop();
shell.qml:3611:                     calendarDismissArmed = false;
shell.qml:3612:                     calendarProgress = 0;
shell.qml:3613:                     if (calendarPanelPresented)
shell.qml:3614:                         calendarPanelCloseTimer.restart();
shell.qml:3615:                 }
shell.qml:3616:             }
…
shell.qml:3636: 
shell.qml:3637:             implicitHeight: Math.min(modelData.height, barWindow.calendarPanelPresented || barWindow.islandExpanded ? 820 : 58)
shell.qml:3638:             exclusiveZone: Math.round(25 + 33 * reveal)
shell.qml:3639:             focusable: false
…
shell.qml:3667: 
shell.qml:3668:             Timer {
shell.qml:3669:                 id: islandCollapseTimer
shell.qml:3670: 
shell.qml:3671:                 interval: 230
shell.qml:3672:                 repeat: false
shell.qml:3673:                 onTriggered: barWindow.islandExpanded = false
…
shell.qml:3675: 
shell.qml:3676:             Timer {
shell.qml:3677:                 id: calendarLeaveTimer
shell.qml:3678: 
shell.qml:3679:                 interval: 220
shell.qml:3680:                 repeat: false
shell.qml:3681:                 onTriggered: {
shell.qml:3682:                     if (barWindow.calendarVisible
shell.qml:3683:                             && !calendarPanel.pointerInside
shell.qml:3684:                             && !root.calendarChildOpening
shell.qml:3685:                             && !root.calendarChildActive)
shell.qml:3686:                         root.requestCalendarClose();
shell.qml:3687:                 }
…
shell.qml:3689: 
shell.qml:3690:             Timer {
shell.qml:3691:                 id: calendarDismissArmTimer
shell.qml:3692: 
shell.qml:3693:                 interval: 260
shell.qml:3694:                 repeat: false
shell.qml:3695:                 onTriggered: barWindow.calendarDismissArmed = barWindow.calendarVisible
shell.qml:3696:             }
shell.qml:3697: 
shell.qml:3698:             Timer {
shell.qml:3699:                 id: calendarPanelCloseTimer
shell.qml:3700: 
shell.qml:3701:                 interval: 210
shell.qml:3702:                 repeat: false
shell.qml:3703:                 onTriggered: {
shell.qml:3704:                     if (barWindow.calendarVisible) {
shell.qml:3705:                         barWindow.calendarProgress = 1;
shell.qml:3706:                         return;
shell.qml:3707:                     }
shell.qml:3708:                     barWindow.calendarProgress = 0;
shell.qml:3709:                     barWindow.calendarPanelPresented = false;
shell.qml:3710:                 }
shell.qml:3711:             }
…
shell.qml:3713:             Connections {
shell.qml:3714:                 target: root
shell.qml:3715:                 function onCalendarChildReleased() {
shell.qml:3716:                     if (barWindow.calendarVisible && !calendarPanel.pointerInside)
shell.qml:3717:                         calendarLeaveTimer.restart();
shell.qml:3718:                 }
shell.qml:3719:             }
…
shell.qml:3742:                     width: parent.width
shell.qml:3743:                     height: barWindow.calendarVisible ? parent.height : 0
shell.qml:3744:                     anchors.top: parent.top
shell.qml:3745:                     enabled: barWindow.calendarVisible
shell.qml:3746:                     hoverEnabled: true
shell.qml:3747:                     acceptedButtons: Qt.LeftButton
shell.qml:3748:                     onPositionChanged: {
shell.qml:3749:                         if (barWindow.calendarVisible
shell.qml:3750:                                 && barWindow.calendarDismissArmed
shell.qml:3751:                                 && !root.calendarChildOpening
shell.qml:3752:                                 && !root.calendarChildActive)
shell.qml:3753:                             calendarLeaveTimer.restart();
shell.qml:3754:                     }
shell.qml:3755:                     onClicked: root.requestCalendarClose()
…
shell.qml:3776:                             moduleHeight: 38
shell.qml:3777:                             interactive: true
shell.qml:3778:                             paddingLeft: 7
shell.qml:3779:                             paddingRight: 4
…
shell.qml:3790:                             moduleHeight: 38
shell.qml:3791:                             interactive: true
shell.qml:3792:                             hoverable: true
shell.qml:3793:                             paddingLeft: 7
…
shell.qml:3797:                                 const point = viewflowWindowsButton.mapToGlobal(0, viewflowWindowsButton.height);
shell.qml:3798:                                 root.runDetached([root.homeDir + "/.local/bin/viewflow-windows", "start", "--anchor", String(Math.round(point.x)), String(Math.round(point.y + 8))]);
shell.qml:3799:                             }
shell.qml:3800:                             onRightClicked: root.runDetached([root.homeDir + "/.local/bin/viewflow-windows", "menu"])
shell.qml:3801:                         }
shell.qml:3802: 
…
shell.qml:3821:                                         textColor: root.mutedWorkspaceText
shell.qml:3822:                                         interactive: true
shell.qml:3823:                                         hoverable: true
shell.qml:3824:                                         moduleHeight: 32
…
shell.qml:3847:                             label: " " + Math.round(root.cpuUsage) + "%"
shell.qml:3848:                             interactive: true
shell.qml:3849:                             paddingLeft: 12
shell.qml:3850:                             paddingRight: 4
shell.qml:3851:                             onLeftClicked: systemStatsPopup.toggleFor(cpuTrigger, barWindow)
shell.qml:3852:                             onRightClicked: root.runDetached(["alacritty", "-t", "btop", "-o", "window.startup_mode=Fullscreen", "-e", "btop"])
shell.qml:3853:                         }
shell.qml:3854: 
…
shell.qml:3858:                             label: " " + Math.round(root.memoryUsage) + "%"
shell.qml:3859:                             interactive: true
shell.qml:3860:                             paddingLeft: 6
shell.qml:3861:                             paddingRight: 4
shell.qml:3862:                             onLeftClicked: systemStatsPopup.toggleFor(memoryTrigger, barWindow)
shell.qml:3863:                             onRightClicked: root.runDetached(["alacritty", "-t", "btop", "-o", "window.startup_mode=Fullscreen", "-e", "btop"])
shell.qml:3864:                         }
shell.qml:3865: 
…
shell.qml:3869:                             label: root.networkIcon + " " + root.networkText
shell.qml:3870:                             interactive: true
shell.qml:3871:                             paddingLeft: 6
shell.qml:3872:                             paddingRight: 12
…
shell.qml:3881: 
shell.qml:3882:                     readonly property real attachedCenterOffset: barWindow.calendarPanelPresented ? 0 : dynamicIsland.attachedCenterOffset
shell.qml:3883:                     width: barWindow.calendarPanelPresented ? calendarPanel.width : dynamicIsland.width
shell.qml:3884:                     height: barWindow.calendarPanelPresented ? calendarPanel.height : dynamicIsland.height
shell.qml:3885:                     anchors.horizontalCenter: parent.horizontalCenter
shell.qml:3886:                     anchors.horizontalCenterOffset: centerSection.attachedCenterOffset
…
shell.qml:3889:                     z: 50
shell.qml:3890:                     radius: barWindow.calendarVisible ? 30 : 24
shell.qml:3891:                     color: barWindow.calendarProgress > 0.001 ? root.moduleBackground : "transparent"
shell.qml:3892:                     border.width: barWindow.calendarProgress > 0.001 ? 1 : 0
shell.qml:3893:                     border.color: root.withAlpha(root.primaryText, root.darkMode ? 0.11 : 0.09)
shell.qml:3894:                     antialiasing: true
…
shell.qml:3897:                     Behavior on width {
shell.qml:3898:                         enabled: barWindow.calendarPanelPresented
shell.qml:3899:                         NumberAnimation { duration: 210; easing.type: Easing.OutCubic }
shell.qml:3900:                     }
…
shell.qml:3902:                     Behavior on height {
shell.qml:3903:                         enabled: barWindow.calendarPanelPresented
shell.qml:3904:                         NumberAnimation { duration: 210; easing.type: Easing.OutCubic }
shell.qml:3905:                     }
…
shell.qml:3907:                     Behavior on radius {
shell.qml:3908:                         enabled: barWindow.calendarPanelPresented
shell.qml:3909:                         NumberAnimation { duration: 210; easing.type: Easing.OutCubic }
shell.qml:3910:                     }
…
shell.qml:3917:                         shellRoot: root
shell.qml:3918:                         calendarOpen: barWindow.calendarPanelPresented
shell.qml:3919:                         visible: opacity > 0.01
shell.qml:3920:                         opacity: 1 - barWindow.calendarProgress
shell.qml:3921:                         now: root.now
shell.qml:3922:                         mediaAvailable: root.mediaAvailable
…
shell.qml:3934:                             if (expanded) {
shell.qml:3935:                                 islandCollapseTimer.stop();
shell.qml:3936:                                 barWindow.islandExpanded = true;
shell.qml:3937:                             } else {
shell.qml:3938:                                 islandCollapseTimer.restart();
shell.qml:3939:                             }
shell.qml:3940:                         }
…
shell.qml:3951:                         }
shell.qml:3952:                         onPreviousClicked: root.runDetached(["playerctl", "previous"])
shell.qml:3953:                         onPlayPauseClicked: {
shell.qml:3954:                             root.mediaPlaying = !root.mediaPlaying;
shell.qml:3955:                             root.runDetached(["playerctl", "play-pause"]);
shell.qml:3956:                         }
shell.qml:3957:                         onNextClicked: root.runDetached(["playerctl", "next"])
shell.qml:3958:                     }
shell.qml:3959: 
…
shell.qml:3963:                         readonly property bool pointerInside: item?.pointerInside ?? false
shell.qml:3964:                         active: barWindow.calendarPanelPresented
shell.qml:3965:                         anchors.top: parent.top
shell.qml:3966:                         anchors.horizontalCenter: parent.horizontalCenter
…
shell.qml:3969:                         visible: active
shell.qml:3970:                         opacity: barWindow.calendarProgress
shell.qml:3971:                         transformOrigin: Item.Top
shell.qml:3972:                         sourceComponent: WeekCalendarPanel {
…
shell.qml:3984:                             onPointerInsideChanged: {
shell.qml:3985:                                 if (!barWindow.calendarVisible) return;
shell.qml:3986:                                 if (pointerInside) calendarLeaveTimer.stop();
shell.qml:3987:                                 else if (!root.calendarChildOpening && !root.calendarChildActive)
shell.qml:3988:                                     calendarLeaveTimer.restart();
shell.qml:3989:                             }
shell.qml:3990:                             onCloseRequested: root.requestCalendarClose()
…
shell.qml:4052:                             textColor: root.temperatureC >= 70 ? root.criticalColor : root.primaryText
shell.qml:4053:                             interactive: true
shell.qml:4054:                             visible: barWindow.supplementaryInfoVisible
shell.qml:4055:                             collapseWhenHidden: true
…
shell.qml:4057:                             paddingRight: 5
shell.qml:4058:                             onLeftClicked: root.runDetached(["alacritty", "-t", "btop", "-o", "window.startup_mode=Fullscreen", "-e", "btop"])
shell.qml:4059:                         }
shell.qml:4060: 
…
shell.qml:4076:                                     : root.usageSeverityColor(root.cpuPackagePowerW)
shell.qml:4077:                                 interactive: root.batteryAvailable
shell.qml:4078:                                 paddingLeft: barWindow.supplementaryInfoVisible ? 5 : 12
shell.qml:4079:                                 paddingRight: 12
…
shell.qml:4094:                             label: ""
shell.qml:4095:                             interactive: true
shell.qml:4096:                             paddingLeft: barWindow.supplementaryInfoVisible ? 9 : 12
shell.qml:4097:                             paddingRight: 5
shell.qml:4098:                             onLeftClicked: root.runDetached(["playerctl", "previous"])
shell.qml:4099:                         }
shell.qml:4100: 
…
shell.qml:4104:                             label: root.mediaPlaying ? "" : ""
shell.qml:4105:                             interactive: true
shell.qml:4106:                             paddingLeft: 5
shell.qml:4107:                             paddingRight: 0
…
shell.qml:4109:                                 root.mediaPlaying = !root.mediaPlaying;
shell.qml:4110:                                 root.runDetached(["playerctl", "play-pause"]);
shell.qml:4111:                             }
shell.qml:4112:                         }
…
shell.qml:4117:                             label: ""
shell.qml:4118:                             interactive: true
shell.qml:4119:                             paddingLeft: 5
shell.qml:4120:                             paddingRight: barWindow.supplementaryInfoVisible ? 0 : 17.5
shell.qml:4121:                             onLeftClicked: root.runDetached(["playerctl", "next"])
shell.qml:4122:                         }
shell.qml:4123: 
…
shell.qml:4129:                             fontFamily: root.iconFont
shell.qml:4130:                             interactive: true
shell.qml:4131:                             wheelInteractive: true
shell.qml:4132:                             visible: barWindow.supplementaryInfoVisible
shell.qml:4133:                             collapseWhenHidden: true
…
shell.qml:4136:                             onLeftClicked: {
shell.qml:4137:                                 root.runDetached(["hyprctl", "eval", 'require("hypr.controls").volume("toggle")']);
shell.qml:4138:                                 audioFollowupRefresh.restart();
shell.qml:4139:                             }
…
shell.qml:4141:                             onWheelUp: {
shell.qml:4142:                                 root.runDetached(["hyprctl", "eval", 'require("hypr.controls").volume("dec")']);
shell.qml:4143:                                 audioFollowupRefresh.restart();
shell.qml:4144:                             }
shell.qml:4145:                             onWheelDown: {
shell.qml:4146:                                 root.runDetached(["hyprctl", "eval", 'require("hypr.controls").volume("inc")']);
shell.qml:4147:                                 audioFollowupRefresh.restart();
shell.qml:4148:                             }
…
shell.qml:4179:                                 anchors.fill: parent
shell.qml:4180:                                 active: root.networkWidgetVisible
shell.qml:4181:                                 source: Qt.resolvedUrl("WifiNative.qml")
shell.qml:4182: 
…
shell.qml:4369: 
shell.qml:4370:     Timer {
shell.qml:4371:         id: calendarChildPoll
shell.qml:4372: 
shell.qml:4373:         interval: 180
shell.qml:4374:         repeat: true
shell.qml:4375:         running: root.calendarOpen || root.calendarChildOpening || root.calendarChildActive
shell.qml:4376:         onTriggered: {
shell.qml:4377:             const active = root.hasDankCalendarWindow();
shell.qml:4378:             if (active) {
shell.qml:4379:                 root.calendarChildOpening = false;
shell.qml:4380:                 root.calendarChildActive = true;
shell.qml:4381:                 calendarChildLaunchTimeout.stop();
shell.qml:4382:                 return;
shell.qml:4383:             }
shell.qml:4384:             if (root.calendarChildActive) {
shell.qml:4385:                 root.calendarChildActive = false;
shell.qml:4386:                 root.calendarChildReleased();
shell.qml:4387:             }
shell.qml:4388:         }
…
shell.qml:4390: 
shell.qml:4391:     Timer {
shell.qml:4392:         id: calendarChildLaunchTimeout
shell.qml:4393: 
shell.qml:4394:         interval: 2600
shell.qml:4395:         repeat: false
shell.qml:4396:         onTriggered: {
shell.qml:4397:             if (!root.hasDankCalendarWindow()) {
shell.qml:4398:                 root.calendarChildOpening = false;
shell.qml:4399:                 root.calendarChildReleased();
shell.qml:4400:             }
shell.qml:4401:         }
…
shell.qml:4403: 
shell.qml:4404:     Timer {
shell.qml:4405:         id: powerProfileRefresh
shell.qml:4406: 
shell.qml:4407:         interval: 350
shell.qml:4408:         repeat: false
shell.qml:4409:         onTriggered: root.syncPowerProfile()
```

### `WeekCalendarPanel.qml`

Migration owner: `UI/provider integration (domain sections above)`. Source `WeekCalendarPanel.qml:1`, 789 lines, SHA-256 `1d51bcb9d6d974344c5066a8a1debba7e8b6e5ee14396570a1a1e7d5de3144b7`.

```text
…
WeekCalendarPanel.qml:33: 
WeekCalendarPanel.qml:34:     signal closeRequested()
WeekCalendarPanel.qml:35:     signal expandRequested()
WeekCalendarPanel.qml:36: 
WeekCalendarPanel.qml:37:     radius: embeddedSurface ? 0 : 28
WeekCalendarPanel.qml:38:     color: embeddedSurface ? "transparent" : (shellRoot ? shellRoot.moduleBackground : "#1e1e2e")
WeekCalendarPanel.qml:39:     border.width: embeddedSurface ? 0 : 1
WeekCalendarPanel.qml:40:     border.color: lineColor
…
WeekCalendarPanel.qml:47:         property bool emphasized: false
WeekCalendarPanel.qml:48:         signal clicked()
WeekCalendarPanel.qml:49: 
WeekCalendarPanel.qml:50:         implicitWidth: Math.max(36, buttonLabel.implicitWidth + 22)
…
WeekCalendarPanel.qml:80: 
WeekCalendarPanel.qml:81:     function localDay(value) {
WeekCalendarPanel.qml:82:         const d = new Date(value);
WeekCalendarPanel.qml:83:         d.setHours(0, 0, 0, 0);
…
WeekCalendarPanel.qml:86: 
WeekCalendarPanel.qml:87:     function allDayDate(value) {
WeekCalendarPanel.qml:88:         const d = new Date(value);
WeekCalendarPanel.qml:89:         return new Date(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate());
…
WeekCalendarPanel.qml:91: 
WeekCalendarPanel.qml:92:     function dayDiff(left, right) {
WeekCalendarPanel.qml:93:         const a = Date.UTC(left.getFullYear(), left.getMonth(), left.getDate());
WeekCalendarPanel.qml:94:         const b = Date.UTC(right.getFullYear(), right.getMonth(), right.getDate());
…
WeekCalendarPanel.qml:97: 
WeekCalendarPanel.qml:98:     function calendarFor(event) {
WeekCalendarPanel.qml:99:         if (!bridge)
WeekCalendarPanel.qml:100:             return null;
WeekCalendarPanel.qml:101:         const list = bridge.calendars || [];
WeekCalendarPanel.qml:102:         for (let i = 0; i < list.length; i++) {
WeekCalendarPanel.qml:103:             if (list[i].id === event.calendarId)
WeekCalendarPanel.qml:104:                 return list[i];
WeekCalendarPanel.qml:105:         }
…
WeekCalendarPanel.qml:108: 
WeekCalendarPanel.qml:109:     function eventVisible(event) {
WeekCalendarPanel.qml:110:         const calendar = calendarFor(event);
WeekCalendarPanel.qml:111:         return !calendar || (!calendar.hidden && !calendar.holdsTasks);
…
WeekCalendarPanel.qml:113: 
WeekCalendarPanel.qml:114:     function eventColor(event) {
WeekCalendarPanel.qml:115:         const calendar = calendarFor(event);
WeekCalendarPanel.qml:116:         return calendar && calendar.color ? calendar.color : root.accentColor;
…
WeekCalendarPanel.qml:118: 
WeekCalendarPanel.qml:119:     function showEventTooltip(event, item) {
WeekCalendarPanel.qml:120:         const top = item.mapToItem(root, 0, 0);
WeekCalendarPanel.qml:121:         const bottom = item.mapToItem(root, 0, item.height);
…
WeekCalendarPanel.qml:127: 
WeekCalendarPanel.qml:128:     function eventTimeText(event) {
WeekCalendarPanel.qml:129:         if (!event)
WeekCalendarPanel.qml:130:             return "";
WeekCalendarPanel.qml:131:         if (event.allDay)
WeekCalendarPanel.qml:132:             return "全天";
WeekCalendarPanel.qml:133:         return Qt.formatDateTime(new Date(event.start), "M月d日 HH:mm")
WeekCalendarPanel.qml:134:             + " – " + Qt.formatTime(new Date(event.end), "HH:mm");
WeekCalendarPanel.qml:135:     }
WeekCalendarPanel.qml:136: 
WeekCalendarPanel.qml:137:     function eventCalendarText(event) {
WeekCalendarPanel.qml:138:         const calendar = event ? calendarFor(event) : null;
WeekCalendarPanel.qml:139:         return calendar ? (calendar.summary || calendar.name || "") : "";
WeekCalendarPanel.qml:140:     }
WeekCalendarPanel.qml:141: 
WeekCalendarPanel.qml:142:     function rebuildLayouts() {
WeekCalendarPanel.qml:143:         if (!bridge)
WeekCalendarPanel.qml:144:             return;
…
WeekCalendarPanel.qml:151:             const event = raw[i];
WeekCalendarPanel.qml:152:             if (event.allDay) {
WeekCalendarPanel.qml:153:                 const startDay = allDayDate(event.start);
WeekCalendarPanel.qml:154:                 const endDay = allDayDate(event.end);
WeekCalendarPanel.qml:155:                 const startIndex = Math.max(0, dayDiff(startDay, week));
WeekCalendarPanel.qml:156:                 const endIndex = Math.min(bufferDayCount, Math.max(startIndex + 1, dayDiff(endDay, week)));
…
WeekCalendarPanel.qml:158:                     allDay.push({
WeekCalendarPanel.qml:159:                         "event": event,
WeekCalendarPanel.qml:160:                         "startDay": startIndex,
WeekCalendarPanel.qml:161:                         "span": Math.max(1, endIndex - startIndex),
WeekCalendarPanel.qml:162:                         "lane": 0
WeekCalendarPanel.qml:163:                     });
WeekCalendarPanel.qml:164:                 }
…
WeekCalendarPanel.qml:167: 
WeekCalendarPanel.qml:168:             const eventStart = new Date(event.start);
WeekCalendarPanel.qml:169:             const eventEnd = new Date(event.end);
WeekCalendarPanel.qml:170:             for (let day = 0; day < bufferDayCount; day++) {
WeekCalendarPanel.qml:171:                 const dayStart = new Date(week);
…
WeekCalendarPanel.qml:181:                 timed.push({
WeekCalendarPanel.qml:182:                     "event": event,
WeekCalendarPanel.qml:183:                     "day": day,
WeekCalendarPanel.qml:184:                     "startMinute": startMinute,
WeekCalendarPanel.qml:185:                     "endMinute": endMinute,
WeekCalendarPanel.qml:186:                     "lane": 0,
WeekCalendarPanel.qml:187:                     "lanes": 1
WeekCalendarPanel.qml:188:                 });
WeekCalendarPanel.qml:189:             }
…
WeekCalendarPanel.qml:191: 
WeekCalendarPanel.qml:192:         allDay.sort((a, b) => a.startDay - b.startDay || b.span - a.span);
WeekCalendarPanel.qml:193:         const allDayEnds = [];
WeekCalendarPanel.qml:194:         for (let i = 0; i < allDay.length; i++) {
WeekCalendarPanel.qml:195:             let lane = 0;
WeekCalendarPanel.qml:196:             while (lane < allDayEnds.length && allDayEnds[lane] > allDay[i].startDay)
WeekCalendarPanel.qml:197:                 lane++;
WeekCalendarPanel.qml:198:             if (lane === allDayEnds.length)
…
WeekCalendarPanel.qml:200:             allDay[i].lane = lane;
WeekCalendarPanel.qml:201:             allDayEnds[lane] = allDay[i].startDay + allDay[i].span;
WeekCalendarPanel.qml:202:         }
WeekCalendarPanel.qml:203: 
WeekCalendarPanel.qml:204:         for (let day = 0; day < bufferDayCount; day++) {
WeekCalendarPanel.qml:205:             const rows = timed.filter(item => item.day === day).sort((a, b) => a.startMinute - b.startMinute || a.endMinute - b.endMinute);
WeekCalendarPanel.qml:206:             const laneEnds = [];
WeekCalendarPanel.qml:207:             for (let i = 0; i < rows.length; i++) {
WeekCalendarPanel.qml:208:                 let lane = 0;
WeekCalendarPanel.qml:209:                 while (lane < laneEnds.length && laneEnds[lane] > rows[i].startMinute)
WeekCalendarPanel.qml:210:                     lane++;
WeekCalendarPanel.qml:211:                 if (lane === laneEnds.length)
…
WeekCalendarPanel.qml:213:                 rows[i].lane = lane;
WeekCalendarPanel.qml:214:                 laneEnds[lane] = rows[i].endMinute;
WeekCalendarPanel.qml:215:             }
WeekCalendarPanel.qml:216:             const lanes = Math.max(1, laneEnds.length);
…
WeekCalendarPanel.qml:225: 
WeekCalendarPanel.qml:226:     function bufferStartDate() {
WeekCalendarPanel.qml:227:         const d = new Date(bridge ? bridge.weekStart : currentTime);
WeekCalendarPanel.qml:228:         d.setDate(d.getDate() - bufferLeadingDays);
…
WeekCalendarPanel.qml:231: 
WeekCalendarPanel.qml:232:     function dayAt(index) {
WeekCalendarPanel.qml:233:         const d = bufferStartDate();
WeekCalendarPanel.qml:234:         d.setDate(d.getDate() + index);
…
WeekCalendarPanel.qml:237: 
WeekCalendarPanel.qml:238:     function isToday(value) {
WeekCalendarPanel.qml:239:         const now = currentTime;
WeekCalendarPanel.qml:240:         return value.getFullYear() === now.getFullYear()
…
WeekCalendarPanel.qml:244: 
WeekCalendarPanel.qml:245:     function rangeLabel() {
WeekCalendarPanel.qml:246:         if (!bridge)
WeekCalendarPanel.qml:247:             return "";
…
WeekCalendarPanel.qml:259: 
WeekCalendarPanel.qml:260:     function positionNearNow() {
WeekCalendarPanel.qml:261:         const hour = dayDiff(currentTime, bufferStartDate()) >= 0
WeekCalendarPanel.qml:262:                 && dayDiff(currentTime, bufferStartDate()) < bufferDayCount
…
WeekCalendarPanel.qml:266: 
WeekCalendarPanel.qml:267:     function initializeHorizontalScroll() {
WeekCalendarPanel.qml:268:         if (horizontalScrollInitialized || dayWidth <= 0)
WeekCalendarPanel.qml:269:             return;
…
WeekCalendarPanel.qml:275: 
WeekCalendarPanel.qml:276:     function rebaseHorizontalScroll() {
WeekCalendarPanel.qml:277:         if (!horizontalScrollInitialized || rebasingHorizontalScroll || !bridge || dayWidth <= 0)
WeekCalendarPanel.qml:278:             return;
…
WeekCalendarPanel.qml:294: 
WeekCalendarPanel.qml:295:     function scrollByWeek(direction) {
WeekCalendarPanel.qml:296:         initializeHorizontalScroll();
WeekCalendarPanel.qml:297:         timeline.contentX = Math.max(0, Math.min(timeline.contentWidth - timeline.width,
WeekCalendarPanel.qml:298:             timeline.contentX + direction * 7 * dayWidth));
…
WeekCalendarPanel.qml:300: 
WeekCalendarPanel.qml:301:     function handleCalendarScroll(event) {
WeekCalendarPanel.qml:302:         const pixelX = event.pixelDelta ? event.pixelDelta.x : 0;
WeekCalendarPanel.qml:303:         const pixelY = event.pixelDelta ? event.pixelDelta.y : 0;
…
WeekCalendarPanel.qml:324:     onBridgeChanged: {
WeekCalendarPanel.qml:325:         rebuildLayouts();
WeekCalendarPanel.qml:326:         horizontalScrollInitialized = false;
WeekCalendarPanel.qml:327:         Qt.callLater(initializeHorizontalScroll);
…
WeekCalendarPanel.qml:329:     onVisibleChanged: if (visible) Qt.callLater(() => {
WeekCalendarPanel.qml:330:         initializeHorizontalScroll();
WeekCalendarPanel.qml:331:         positionNearNow();
WeekCalendarPanel.qml:332:     })
WeekCalendarPanel.qml:333: 
WeekCalendarPanel.qml:334:     Connections {
WeekCalendarPanel.qml:335:         target: root.bridge
WeekCalendarPanel.qml:336:         function onEventsChanged() { root.rebuildLayouts(); }
WeekCalendarPanel.qml:337:         function onCalendarsChanged() { root.rebuildLayouts(); }
WeekCalendarPanel.qml:338:         function onWeekStartChanged() { root.rebuildLayouts(); Qt.callLater(root.positionNearNow); }
WeekCalendarPanel.qml:339:     }
WeekCalendarPanel.qml:340: 
…
WeekCalendarPanel.qml:417:         WheelHandler {
WeekCalendarPanel.qml:418:             target: null
WeekCalendarPanel.qml:419:             acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
WeekCalendarPanel.qml:420:             onWheel: event => root.handleCalendarScroll(event)
…
WeekCalendarPanel.qml:473:         width: parent.width - root.timeColumnWidth - 12
WeekCalendarPanel.qml:474:         height: root.allDayHeight
WeekCalendarPanel.qml:475:         clip: true
WeekCalendarPanel.qml:476: 
WeekCalendarPanel.qml:477:         WheelHandler {
WeekCalendarPanel.qml:478:             target: null
WeekCalendarPanel.qml:479:             acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
WeekCalendarPanel.qml:480:             onWheel: event => root.handleCalendarScroll(event)
…
WeekCalendarPanel.qml:488:             Repeater {
WeekCalendarPanel.qml:489:                 model: root.allDayLayout
WeekCalendarPanel.qml:490:                 delegate: Rectangle {
WeekCalendarPanel.qml:491:                     required property var modelData
WeekCalendarPanel.qml:492:                     x: modelData.startDay * root.dayWidth + 3
WeekCalendarPanel.qml:493:                     y: 6 + modelData.lane * 24
WeekCalendarPanel.qml:494:                     width: modelData.span * root.dayWidth - 6
…
WeekCalendarPanel.qml:504:                         anchors.rightMargin: 5
WeekCalendarPanel.qml:505:                         text: modelData.event.summary || "(无标题)"
WeekCalendarPanel.qml:506:                         color: "#ffffff"
WeekCalendarPanel.qml:507:                         font.family: root.shellRoot ? root.shellRoot.baseFont : "sans-serif"
…
WeekCalendarPanel.qml:519:                         onExited: if (root.hoveredEvent === modelData.event) root.hoveredEvent = null
WeekCalendarPanel.qml:520:                         onClicked: if (root.bridge) root.bridge.openEvent(modelData.event)
WeekCalendarPanel.qml:521:                     }
WeekCalendarPanel.qml:522:                 }
…
WeekCalendarPanel.qml:562:         WheelHandler {
WeekCalendarPanel.qml:563:             target: null
WeekCalendarPanel.qml:564:             acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
WeekCalendarPanel.qml:565:             onWheel: event => root.handleCalendarScroll(event)
…
WeekCalendarPanel.qml:606:                     x: modelData.day * root.dayWidth + 3 + modelData.lane * laneWidth
WeekCalendarPanel.qml:607:                     y: modelData.startMinute / 60 * root.hourHeight + 1
WeekCalendarPanel.qml:608:                     width: Math.max(18, laneWidth - 2)
WeekCalendarPanel.qml:609:                     height: Math.max(18, (modelData.endMinute - modelData.startMinute) / 60 * root.hourHeight - 2)
WeekCalendarPanel.qml:610:                     radius: 7
WeekCalendarPanel.qml:611:                     color: root.shellRoot ? root.shellRoot.withAlpha(root.eventColor(modelData.event), 0.84) : root.eventColor(modelData.event)
…
WeekCalendarPanel.qml:621:                             width: parent.width
WeekCalendarPanel.qml:622:                             text: modelData.event.summary || "(无标题)"
WeekCalendarPanel.qml:623:                             color: "#ffffff"
WeekCalendarPanel.qml:624:                             font.family: root.shellRoot ? root.shellRoot.baseFont : "sans-serif"
…
WeekCalendarPanel.qml:631:                             visible: parent.parent.height >= 34
WeekCalendarPanel.qml:632:                             text: Qt.formatTime(new Date(modelData.event.start), "HH:mm")
WeekCalendarPanel.qml:633:                             color: "#f7f7fb"
WeekCalendarPanel.qml:634:                             opacity: 0.84
…
WeekCalendarPanel.qml:644:                         onExited: if (root.hoveredEvent === modelData.event) root.hoveredEvent = null
WeekCalendarPanel.qml:645:                         onClicked: if (root.bridge) root.bridge.openEvent(modelData.event)
WeekCalendarPanel.qml:646:                     }
WeekCalendarPanel.qml:647:                 }
…
WeekCalendarPanel.qml:700:                     width: root.timeColumnWidth - 14
WeekCalendarPanel.qml:701:                     text: index < 24 ? (index < 10 ? "0" : "") + index + ":00" : ""
WeekCalendarPanel.qml:702:                     color: root.mutedColor
WeekCalendarPanel.qml:703:                     font.family: root.shellRoot ? root.shellRoot.baseFont : "sans-serif"
…
WeekCalendarPanel.qml:752:                 width: parent.width
WeekCalendarPanel.qml:753:                 text: root.hoveredEvent ? (root.hoveredEvent.summary || "(无标题)") : ""
WeekCalendarPanel.qml:754:                 color: root.textColor
WeekCalendarPanel.qml:755:                 font.family: root.shellRoot ? root.shellRoot.baseFont : "sans-serif"
…
WeekCalendarPanel.qml:769:                 visible: text.length > 0
WeekCalendarPanel.qml:770:                 text: root.hoveredEvent ? (root.hoveredEvent.location || root.eventCalendarText(root.hoveredEvent)) : ""
WeekCalendarPanel.qml:771:                 color: root.mutedColor
WeekCalendarPanel.qml:772:                 font.family: root.shellRoot ? root.shellRoot.baseFont : "sans-serif"
```

### `/home/wilf/.config/HyprV/hypr/controls.lua`

Migration owner: `UI/provider integration (domain sections above)`. Source `/home/wilf/.config/HyprV/hypr/controls.lua:1`, 220 lines, SHA-256 `7b568fadd802c961542f1215351720dd2ad9b20a1653a08effece2efb76bd70f`.

```text
/home/wilf/.config/HyprV/hypr/controls.lua:1: -- Native Hyprland control logic. No synchronous subprocesses in the compositor.
/home/wilf/.config/HyprV/hypr/controls.lua:2: local M = {}
/home/wilf/.config/HyprV/hypr/controls.lua:3: local home = assert(os.getenv("HOME"))
/home/wilf/.config/HyprV/hypr/controls.lua:4: local root = home .. "/.config/HyprV"
/home/wilf/.config/HyprV/hypr/controls.lua:5: local cfg = require("hypr.controls-profile")
/home/wilf/.config/HyprV/hypr/controls.lua:6: local state = (os.getenv("XDG_STATE_HOME") or home .. "/.local/state") .. "/hyprv/brightness-hdr.json"
/home/wilf/.config/HyprV/hypr/controls.lua:7: local runtime = (os.getenv("XDG_RUNTIME_DIR") or "/tmp") .. "/hyprv-controls"
/home/wilf/.config/HyprV/hypr/controls.lua:8: local function read(path)
/home/wilf/.config/HyprV/hypr/controls.lua:9:   local f=io.open(path); if not f then return nil end
/home/wilf/.config/HyprV/hypr/controls.lua:10:   local s=f:read("*a"); f:close(); return s
/home/wilf/.config/HyprV/hypr/controls.lua:11: end
…
/home/wilf/.config/HyprV/hypr/controls.lua:37: end
/home/wilf/.config/HyprV/hypr/controls.lua:38: function M.plan(level)
/home/wilf/.config/HyprV/hypr/controls.lua:39:   local minimum=cfg.backend=="backlight" and 1 or 0
/home/wilf/.config/HyprV/hypr/controls.lua:40:   return clamp(round(maximum*hardwarePercent(level)/100),minimum,maximum)
…
/home/wilf/.config/HyprV/hypr/controls.lua:47: local function notice(text)
/home/wilf/.config/HyprV/hypr/controls.lua:48:   hl.notification.create({text=text,timeout=4000,icon="warning"})
/home/wilf/.config/HyprV/hypr/controls.lua:49: end
/home/wilf/.config/HyprV/hypr/controls.lua:50: local function notify(kind,level)
…
/home/wilf/.config/HyprV/hypr/controls.lua:65: local function persist(level)
/home/wilf/.config/HyprV/hypr/controls.lua:66:   local f=assert(io.open(state .. ".tmp","w")); f:write('{"level":' .. level .. '}'); f:close(); assert(os.rename(state .. ".tmp",state))
/home/wilf/.config/HyprV/hypr/controls.lua:67: end
/home/wilf/.config/HyprV/hypr/controls.lua:68: local function startJob(level,verify)
…
/home/wilf/.config/HyprV/hypr/controls.lua:74:   if verify then M.stats.verification_reads=M.stats.verification_reads+1 end
/home/wilf/.config/HyprV/hypr/controls.lua:75:   -- Bound the entire worker so timeout recovery cannot overlap a stuck job.
/home/wilf/.config/HyprV/hypr/controls.lua:76:   hl.exec_cmd("timeout --kill-after=1s 10s lua " .. quote(root .. "/hypr/scripts/control-worker.lua") .. " hardware " .. quote(path) .. (target and (" " .. target .. " " .. maximum) or ""))
/home/wilf/.config/HyprV/hypr/controls.lua:77: end
/home/wilf/.config/HyprV/hypr/controls.lua:78: local function applyLevel(level,save)
…
/home/wilf/.config/HyprV/hypr/controls.lua:81:     if save then persist(level);saved=level end
/home/wilf/.config/HyprV/hypr/controls.lua:82:   end)
/home/wilf/.config/HyprV/hypr/controls.lua:83:   if not ok then M.stats.last_error=tostring(err);notice(tostring(err)) end
/home/wilf/.config/HyprV/hypr/controls.lua:84: end
…
/home/wilf/.config/HyprV/hypr/controls.lua:108: end
/home/wilf/.config/HyprV/hypr/controls.lua:109: function M.brightness_get()
/home/wilf/.config/HyprV/hypr/controls.lua:110:   if cfg.backend=="backlight" then
/home/wilf/.config/HyprV/hypr/controls.lua:111:     raw=tonumber(read("/sys/class/backlight/intel_backlight/brightness")) or raw
/home/wilf/.config/HyprV/hypr/controls.lua:112:     maximum=tonumber(read("/sys/class/backlight/intel_backlight/max_brightness")) or maximum
/home/wilf/.config/HyprV/hypr/controls.lua:113:   elseif not busy and not wanted and not dragging and not verifyTicks and not confirmFailed and os.time()-lastRead>=2 then startJob(nil) end
/home/wilf/.config/HyprV/hypr/controls.lua:114:   -- Keep user intent stable during writing, confirmation, and failed retries.
…
/home/wilf/.config/HyprV/hypr/controls.lua:123: end
/home/wilf/.config/HyprV/hypr/controls.lua:124: function M.brightness_raw() return known and raw or nil end
/home/wilf/.config/HyprV/hypr/controls.lua:125: function M.brightness_status()
/home/wilf/.config/HyprV/hypr/controls.lua:126:   return {target=goal,confirmed_raw=known and raw or nil,commanded_raw=commandedRaw,
/home/wilf/.config/HyprV/hypr/controls.lua:127:     dragging=dragging,busy=busy~=nil,confirming=verifyTicks~=nil or (busy and busy.verify) or false,
…
/home/wilf/.config/HyprV/hypr/controls.lua:129: end
/home/wilf/.config/HyprV/hypr/controls.lua:130: function M.brightness_set(level,show,deferVerify)
/home/wilf/.config/HyprV/hypr/controls.lua:131:   assert(type(level)=="number" and level==level,"Invalid brightness")
/home/wilf/.config/HyprV/hypr/controls.lua:132:   level=clamp(round(level),0,100)
…
/home/wilf/.config/HyprV/hypr/controls.lua:139:   if show then notify("showBrightnessLevel",level) end
/home/wilf/.config/HyprV/hypr/controls.lua:140:   pump()
/home/wilf/.config/HyprV/hypr/controls.lua:141: end
/home/wilf/.config/HyprV/hypr/controls.lua:142: function M.brightness_finish(level)
/home/wilf/.config/HyprV/hypr/controls.lua:143:   M.brightness_set(level,false,true)
/home/wilf/.config/HyprV/hypr/controls.lua:144:   dragging=false;verifyTicks=0
/home/wilf/.config/HyprV/hypr/controls.lua:145:   pump()
/home/wilf/.config/HyprV/hypr/controls.lua:146: end
/home/wilf/.config/HyprV/hypr/controls.lua:147: function M.brightness_step(delta)
/home/wilf/.config/HyprV/hypr/controls.lua:148:   local level=M.brightness_get()
/home/wilf/.config/HyprV/hypr/controls.lua:149:   if level then M.brightness_set(level+delta,true) else notice("Reading display brightness; try again shortly") end
/home/wilf/.config/HyprV/hypr/controls.lua:150: end
/home/wilf/.config/HyprV/hypr/controls.lua:151: function M.restore() forceSDR=true; M.brightness_set(saved,false) end
/home/wilf/.config/HyprV/hypr/controls.lua:152: function M.volume(action,value)
/home/wilf/.config/HyprV/hypr/controls.lua:153:   local cmd
/home/wilf/.config/HyprV/hypr/controls.lua:154:   if action=="set" then cmd="wpctl set-volume -l 1 @DEFAULT_AUDIO_SINK@ " .. string.format("%.2f",clamp(assert(tonumber(value)),0,100)/100)
/home/wilf/.config/HyprV/hypr/controls.lua:155:   elseif action=="inc" then cmd="wpctl set-volume -l 1 @DEFAULT_AUDIO_SINK@ 5%+"
/home/wilf/.config/HyprV/hypr/controls.lua:156:   elseif action=="dec" then cmd="wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-"
/home/wilf/.config/HyprV/hypr/controls.lua:157:   elseif action=="toggle" then cmd="wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle"
/home/wilf/.config/HyprV/hypr/controls.lua:158:   elseif action=="toggle-mic" then cmd="wpctl set-mute @DEFAULT_AUDIO_SOURCE@ toggle"
/home/wilf/.config/HyprV/hypr/controls.lua:159:   elseif action=="mic-inc" then cmd="wpctl set-volume -l 1 @DEFAULT_AUDIO_SOURCE@ 5%+"
/home/wilf/.config/HyprV/hypr/controls.lua:160:   elseif action=="mic-dec" then cmd="wpctl set-volume @DEFAULT_AUDIO_SOURCE@ 5%-"
/home/wilf/.config/HyprV/hypr/controls.lua:161:   else error("Unknown audio action") end
/home/wilf/.config/HyprV/hypr/controls.lua:162:   hl.exec_cmd(cmd)
/home/wilf/.config/HyprV/hypr/controls.lua:163:   if action=="toggle-mic" then
/home/wilf/.config/HyprV/hypr/controls.lua:164:     hl.exec_cmd("notify-send -h string:x-canonical-private-synchronous:sys-notify 'Microphone mute toggled'")
/home/wilf/.config/HyprV/hypr/controls.lua:165:   elseif not action:find("mic",1,true) and action~="set" then
/home/wilf/.config/HyprV/hypr/controls.lua:166:     hl.timer(function() notify("showVolume") end,{timeout=80,type="oneshot"})
/home/wilf/.config/HyprV/hypr/controls.lua:167:   end
/home/wilf/.config/HyprV/hypr/controls.lua:168: end
/home/wilf/.config/HyprV/hypr/controls.lua:169: function M.keyboard_step(delta)
/home/wilf/.config/HyprV/hypr/controls.lua:170:   hl.exec_cmd("brightnessctl -d '*::kbd_backlight' set 33%" .. (delta>0 and "+" or "-"))
/home/wilf/.config/HyprV/hypr/controls.lua:171: end
/home/wilf/.config/HyprV/hypr/controls.lua:172: M.timer=hl.timer(function()
/home/wilf/.config/HyprV/hypr/controls.lua:173:   if busy then
/home/wilf/.config/HyprV/hypr/controls.lua:174:     local s=read(busy.path)
…
/home/wilf/.config/HyprV/hypr/controls.lua:206:       end
/home/wilf/.config/HyprV/hypr/controls.lua:207:     elseif os.time()-busy.started>12 then
/home/wilf/.config/HyprV/hypr/controls.lua:208:       local job=busy;busy=nil;commandedRaw=nil
/home/wilf/.config/HyprV/hypr/controls.lua:209:       local reason="Brightness hardware operation timed out"
…
/home/wilf/.config/HyprV/hypr/controls.lua:216:   end
/home/wilf/.config/HyprV/hypr/controls.lua:217:   pump()
/home/wilf/.config/HyprV/hypr/controls.lua:218: end,{timeout=25,type="repeat"})
/home/wilf/.config/HyprV/hypr/controls.lua:219: if cfg.backend=="ddc" then startJob(nil) end
/home/wilf/.config/HyprV/hypr/controls.lua:220: return M
```

### `/home/wilf/.config/HyprV/hypr/hyprland-bind.lua`

Migration owner: `UI/provider integration (domain sections above)`. Source `/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:1`, 76 lines, SHA-256 `daf41d0c24f54d5a4b166fdcbae5ac7b8bd97adf852e766a4f052164e2b9cef6`.

```text
…
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:11: lib.bind_exec(main .. " + M", "wlogout --protocol layer-shell -b 5")
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:12: lib.bind_exec(main .. " + ALT + M", "bash ~/.config/HyprV/hypr/scripts/toggle-monitor-power.sh", { allow_input_capture = true })
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:13: lib.bind_exec(main .. " + F", "code")
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:14: lib.bind_exec(main .. " + SHIFT + F", "cursor")
…
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:25:   "fullscreen",
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:26:   "hyprctl dispatch fullscreen 1 toggle",
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:27:   { mode = "maximized", action = "toggle" }
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:28: )
…
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:33: hl.bind(main .. " + Y", hl.dsp.window.move({ x = 1772, y = 861 }))
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:34: lib.bind_exec(main .. " + Y", "quickshell kill -p $HOME/.config/HyprV/quickshell")
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:35: lib.bind_plugin_fn_or_exec(
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:36:   main .. " + SHIFT + O",
…
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:38:   "fullscreen",
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:39:   "hyprctl dispatch fullscreen 0 toggle",
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:40:   { mode = "fullscreen", action = "toggle" }
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:41: )
…
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:45: 
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:46: lib.bind_exec(main .. " + I", "~/.config/HyprV/hypr/scripts/browser-wl-relabel.sh zen-browser")
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:47: lib.bind_exec(main .. " + B", "~/.config/HyprV/hypr/scripts/toggle-battery-mode.sh")
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:48: lib.bind_exec(main .. " + T", "~/.config/HyprV/hypr/scripts/rotate-screen-ccw.sh")
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:49: lib.bind_exec(main .. " + R", "~/.config/hypr/scripts/record-script.sh --fullscreen-sound")
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:50: lib.bind_exec(main .. " + SHIFT + R", "~/.config/HyprV/quickshell/scripts/reload.sh && fcitx5")
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:51: lib.bind_exec(main .. " + SHIFT + S", "~/.config/hypr/scripts/ocr.sh")
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:52: lib.bind_exec(main .. " + N", "swaync-client -t -sw")
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:53: hl.bind(main .. " + SHIFT + b", function() require("hypr.controls").brightness_set(35, true) end)
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:54: lib.bind_exec(main .. " + A", "quickshell ipc call -p $HOME/.config/HyprV/quickshell controlPanel toggle")
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:55: 
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:56: hl.bind(main .. " + left", hl.dsp.focus({ direction = "left" }))
…
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:62:   local key = tostring(i % 10)
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:63:   lib.bind_exec(main .. " + " .. key, "~/.config/hypr/scripts/workspace_action.sh workspace " .. i)
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:64:   lib.bind_exec(main .. " + SHIFT + " .. key, "~/.config/hypr/scripts/workspace_action.sh movetoworkspace " .. i)
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:65: end
/home/wilf/.config/HyprV/hypr/hyprland-bind.lua:66: 
```

### `/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua`

Migration owner: `UI/provider integration (domain sections above)`. Source `/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:1`, 47 lines, SHA-256 `cc79c2116007fea344d609a6993eccb687897fc3c3a1c7a3839a2ae7ca448f52`.

```text
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:1: local lib = require("hypr.lib")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:2: 
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:3: -- Make the Xwayland IM frontend available to both new X11 clients and D-Bus apps.
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:4: hl.env("GTK_IM_MODULE", "fcitx")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:5: hl.env("QT_IM_MODULE", "fcitx")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:6: hl.env("XMODIFIERS", "@im=fcitx")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:7: 
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:8: lib.exec_once("~/.config/hypr/xdg-portal-hyprland")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:9: lib.exec_once("dbus-update-activation-environment --systemd WAYLAND_DISPLAY XDG_CURRENT_DESKTOP DBUS_SESSION_BUS_ADDRESS DISPLAY XDG_RUNTIME_DIR HYPRLAND_INSTANCE_SIGNATURE GTK_IM_MODULE QT_IM_MODULE XMODIFIERS")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:10: lib.exec_once("systemctl --user import-environment WAYLAND_DISPLAY XDG_CURRENT_DESKTOP DBUS_SESSION_BUS_ADDRESS DISPLAY XDG_RUNTIME_DIR HYPRLAND_INSTANCE_SIGNATURE GTK_IM_MODULE QT_IM_MODULE XMODIFIERS")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:11: lib.exec_once("systemctl --user start plasma-kded6.service")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:12: lib.exec_once("sleep 0.5 && busctl --user call org.kde.kded6 /kded org.kde.kded6 loadModule s statusnotifierwatcher")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:13: lib.exec_once("/usr/lib/polkit-gnome/polkit-gnome-authentication-agent-1")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:14: lib.exec_once("~/.config/HyprV/quickshell/scripts/launch.sh")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:15: lib.exec_once("sleep 1 && nm-applet --indicator")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:16: lib.exec_once("kdeconnectd")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:17: lib.exec_once("fcitx5")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:18: lib.exec_once("wl-paste --watch cliphist store")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:19: lib.exec_once("sleep 0.3&&awww-daemon")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:20: lib.exec_once("kbuildsycoca6")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:21: lib.exec_once("uxplay")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:22: lib.exec_once("hyprpm reload -n && /home/wilf/.local/bin/hypr-edgehover-local && hyprctl reload && hyprctl eval 'require(\"hypr.controls\").restore()'")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:23: lib.exec_once("hypridle")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:24: lib.exec_once("ydotoold")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:25: lib.exec_once("zw")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:26: lib.exec_once("rm .config/chromium/SingletonLock")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:27: lib.exec_once("rm .config/vivaldi/SingletonLock")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:28: lib.exec_once("sleep 5 && keepassxc --minimized")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:29: -- Start Sunshine in the graphical session, preserving its NVIDIA environment.
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:30: lib.exec_once("sleep 5 && env LIBVA_DRIVER_NAME=nvidia DRI_PRIME=1 __GLX_VENDOR_LIBRARY_NAME=nvidia __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/10_nvidia.json /usr/bin/sunshine")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:31: lib.exec_once("sleep 5 && $HOME/data/model/local_ai_service/launch.sh --tray-only")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:32: lib.exec_once("sleep 2 && $HOME/.local/share/lzc-client-desktop/lzc-client-desktop --password-store=gnome-libsecret --ozone-platform-hint=wayland --gtk-version=4 --ignore-gpu-blocklist --enable-features=TouchpadOverscrollHistoryNavigation --enable-wayland-ime --auto-startup")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:33: lib.exec_once("~/.config/HyprV/hypr/scripts/battery-notify.sh &")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:34: lib.exec_once("~/.config/HyprV/hypr/scripts/restore-session-layout.sh")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:35: 
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:36: hl.env("trashAppUnsafe", "1")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:37: hl.env("QT_QPA_PLATFORM", "wayland")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:38: hl.env("QT_WAYLAND_DISABLE_WINDOWDECORATION", "1")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:39: hl.env("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "1")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:40: -- SuperPower: keep DRM buffer modifiers enabled.
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:41: -- hl.env("AQ_NO_MODIFIERS", "1")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:42: hl.env("QT_QPA_PLATFORMTHEME", "qt6ct")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:43: hl.env("QT_AUTO_SCREEN_SCALE_FACTOR", "1")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:44: hl.env("ELECTRON_OZONE_PLATFORM_HINT", "auto")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:45: -- SuperPower: keep atomic modesetting enabled.
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:46: -- hl.env("WLR_DRM_NO_ATOMIC", "1")
/home/wilf/.config/HyprV/hypr/hyprland-exec-once.lua:47: hl.env("XCURSOR_SIZE", "24")
```

### `/home/wilf/.config/HyprV/scripts/lib/battery.sh`

Migration owner: `power/battery_history`. Source `/home/wilf/.config/HyprV/scripts/lib/battery.sh:1`, 83 lines, SHA-256 `e75a9ea2fc1a763e0fffd97ae221b3a647eab760293c728ef75b2f1c142fda1a`.

```text
…
/home/wilf/.config/HyprV/scripts/lib/battery.sh:2: 
/home/wilf/.config/HyprV/scripts/lib/battery.sh:3: hyprv_pick_state_dir() {
/home/wilf/.config/HyprV/scripts/lib/battery.sh:4:     for candidate in "${XDG_RUNTIME_DIR:-}" /tmp; do
/home/wilf/.config/HyprV/scripts/lib/battery.sh:5:         [ -n "$candidate" ] || continue
/home/wilf/.config/HyprV/scripts/lib/battery.sh:6:         [ -d "$candidate" ] || continue
/home/wilf/.config/HyprV/scripts/lib/battery.sh:7: 
/home/wilf/.config/HyprV/scripts/lib/battery.sh:8:         dir="$candidate/hyprv"
/home/wilf/.config/HyprV/scripts/lib/battery.sh:9:         if mkdir -p "$dir" 2>/dev/null; then
/home/wilf/.config/HyprV/scripts/lib/battery.sh:10:             probe="$dir/.battery-write-test.$$"
/home/wilf/.config/HyprV/scripts/lib/battery.sh:11:             if touch "$probe" 2>/dev/null; then
/home/wilf/.config/HyprV/scripts/lib/battery.sh:12:                 rm -f "$probe"
/home/wilf/.config/HyprV/scripts/lib/battery.sh:13:                 printf '%s\n' "$dir"
/home/wilf/.config/HyprV/scripts/lib/battery.sh:14:                 return 0
/home/wilf/.config/HyprV/scripts/lib/battery.sh:15:             fi
…
/home/wilf/.config/HyprV/scripts/lib/battery.sh:21: 
/home/wilf/.config/HyprV/scripts/lib/battery.sh:22: hyprv_find_battery_dir() {
/home/wilf/.config/HyprV/scripts/lib/battery.sh:23:     for path in /sys/class/power_supply/BAT*; do
/home/wilf/.config/HyprV/scripts/lib/battery.sh:24:         [ -d "$path" ] || continue
/home/wilf/.config/HyprV/scripts/lib/battery.sh:25:         printf '%s\n' "$path"
/home/wilf/.config/HyprV/scripts/lib/battery.sh:26:         return 0
/home/wilf/.config/HyprV/scripts/lib/battery.sh:27:     done
…
/home/wilf/.config/HyprV/scripts/lib/battery.sh:30: 
/home/wilf/.config/HyprV/scripts/lib/battery.sh:31: hyprv_read_first_existing() {
/home/wilf/.config/HyprV/scripts/lib/battery.sh:32:     dir="$1"
/home/wilf/.config/HyprV/scripts/lib/battery.sh:33:     shift
…
/home/wilf/.config/HyprV/scripts/lib/battery.sh:35:     for name in "$@"; do
/home/wilf/.config/HyprV/scripts/lib/battery.sh:36:         if [ -r "$dir/$name" ]; then
/home/wilf/.config/HyprV/scripts/lib/battery.sh:37:             tr -d '\n' < "$dir/$name"
/home/wilf/.config/HyprV/scripts/lib/battery.sh:38:             return 0
/home/wilf/.config/HyprV/scripts/lib/battery.sh:39:         fi
…
/home/wilf/.config/HyprV/scripts/lib/battery.sh:44: 
/home/wilf/.config/HyprV/scripts/lib/battery.sh:45: hyprv_safe_int() {
/home/wilf/.config/HyprV/scripts/lib/battery.sh:46:     value="${1:-0}"
/home/wilf/.config/HyprV/scripts/lib/battery.sh:47:     case "$value" in
/home/wilf/.config/HyprV/scripts/lib/battery.sh:48:         ''|*[!0-9-]*)
/home/wilf/.config/HyprV/scripts/lib/battery.sh:49:             printf '0\n'
/home/wilf/.config/HyprV/scripts/lib/battery.sh:50:             ;;
/home/wilf/.config/HyprV/scripts/lib/battery.sh:51:         *)
/home/wilf/.config/HyprV/scripts/lib/battery.sh:52:             printf '%s\n' "$value"
/home/wilf/.config/HyprV/scripts/lib/battery.sh:53:             ;;
/home/wilf/.config/HyprV/scripts/lib/battery.sh:54:     esac
…
/home/wilf/.config/HyprV/scripts/lib/battery.sh:56: 
/home/wilf/.config/HyprV/scripts/lib/battery.sh:57: hyprv_abs_int() {
/home/wilf/.config/HyprV/scripts/lib/battery.sh:58:     value="$(hyprv_safe_int "${1:-0}")"
/home/wilf/.config/HyprV/scripts/lib/battery.sh:59:     case "$value" in
/home/wilf/.config/HyprV/scripts/lib/battery.sh:60:         -*)
/home/wilf/.config/HyprV/scripts/lib/battery.sh:61:             printf '%s\n' "${value#-}"
/home/wilf/.config/HyprV/scripts/lib/battery.sh:62:             ;;
/home/wilf/.config/HyprV/scripts/lib/battery.sh:63:         *)
/home/wilf/.config/HyprV/scripts/lib/battery.sh:64:             printf '%s\n' "$value"
/home/wilf/.config/HyprV/scripts/lib/battery.sh:65:             ;;
/home/wilf/.config/HyprV/scripts/lib/battery.sh:66:     esac
…
/home/wilf/.config/HyprV/scripts/lib/battery.sh:68: 
/home/wilf/.config/HyprV/scripts/lib/battery.sh:69: hyprv_mul_div_round() {
/home/wilf/.config/HyprV/scripts/lib/battery.sh:70:     awk -v a="$1" -v b="$2" -v d="$3" 'BEGIN {
/home/wilf/.config/HyprV/scripts/lib/battery.sh:71:         if ((d + 0) == 0) {
…
/home/wilf/.config/HyprV/scripts/lib/battery.sh:74:         }
/home/wilf/.config/HyprV/scripts/lib/battery.sh:75:         printf "%.0f\n", (a + 0) * (b + 0) / d;
/home/wilf/.config/HyprV/scripts/lib/battery.sh:76:     }'
/home/wilf/.config/HyprV/scripts/lib/battery.sh:77: }
/home/wilf/.config/HyprV/scripts/lib/battery.sh:78: 
/home/wilf/.config/HyprV/scripts/lib/battery.sh:79: hyprv_micro_to_decimal_units() {
/home/wilf/.config/HyprV/scripts/lib/battery.sh:80:     awk -v value="$1" 'BEGIN {
/home/wilf/.config/HyprV/scripts/lib/battery.sh:81:         printf "%.3f\n", (value + 0) / 1000000;
/home/wilf/.config/HyprV/scripts/lib/battery.sh:82:     }'
/home/wilf/.config/HyprV/scripts/lib/battery.sh:83: }
```

### `/home/wilf/data/viewflow/tools/viewflow-target.sh`

Migration owner: `viewflow`. Source `/home/wilf/data/viewflow/tools/viewflow-target.sh:1`, 116 lines, SHA-256 `d8f0706ca57d637d810d61e9f8d43fcc0b6f9d0bfc350615d4b10079d04b7a4f`.

```text
…
/home/wilf/data/viewflow/tools/viewflow-target.sh:2: set -euo pipefail
/home/wilf/data/viewflow/tools/viewflow-target.sh:3: readonly config_dir="${XDG_CONFIG_HOME:-$HOME/.config}/viewflow"
/home/wilf/data/viewflow/tools/viewflow-target.sh:4: readonly state_dir="${XDG_STATE_HOME:-$HOME/.local/state}/hyprv"
/home/wilf/data/viewflow/tools/viewflow-target.sh:5: readonly runtime="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/viewflow"
/home/wilf/data/viewflow/tools/viewflow-target.sh:6: readonly script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
/home/wilf/data/viewflow/tools/viewflow-target.sh:7: readonly hdmi_helper="${VIEWFLOW_HDMI_HELPER:-${XDG_CONFIG_HOME:-$HOME/.config}/HyprV/quickshell/scripts/hdmi-switch-ir.py}"
/home/wilf/data/viewflow/tools/viewflow-target.sh:8: unit_for() {
/home/wilf/data/viewflow/tools/viewflow-target.sh:9:   case "$1" in
/home/wilf/data/viewflow/tools/viewflow-target.sh:10:     windows) echo viewflow-windows-input.service ;;
/home/wilf/data/viewflow/tools/viewflow-target.sh:11:     macos) echo viewflow-macos-input.service ;;
/home/wilf/data/viewflow/tools/viewflow-target.sh:12:     *) return 2 ;;
/home/wilf/data/viewflow/tools/viewflow-target.sh:13:   esac
/home/wilf/data/viewflow/tools/viewflow-target.sh:14: }
/home/wilf/data/viewflow/tools/viewflow-target.sh:15: current_target() {
/home/wilf/data/viewflow/tools/viewflow-target.sh:16:   if systemctl --user is-active --quiet viewflow-macos-input.service; then echo macos
/home/wilf/data/viewflow/tools/viewflow-target.sh:17:   elif systemctl --user is-active --quiet viewflow-windows-input.service || systemctl --user is-active --quiet viewflow-desktop.service; then echo windows
/home/wilf/data/viewflow/tools/viewflow-target.sh:18:   elif [[ -r "$state_dir/viewflow-target" ]]; then cat "$state_dir/viewflow-target"
/home/wilf/data/viewflow/tools/viewflow-target.sh:19:   else echo windows
/home/wilf/data/viewflow/tools/viewflow-target.sh:20:   fi
/home/wilf/data/viewflow/tools/viewflow-target.sh:21: }
/home/wilf/data/viewflow/tools/viewflow-target.sh:22: target_config() {
/home/wilf/data/viewflow/tools/viewflow-target.sh:23:   case "$1" in
/home/wilf/data/viewflow/tools/viewflow-target.sh:24:     windows) echo "$config_dir/windows-input-source.json" ;;
/home/wilf/data/viewflow/tools/viewflow-target.sh:25:     macos) echo "$config_dir/macos-input-source.json" ;;
/home/wilf/data/viewflow/tools/viewflow-target.sh:26:   esac
/home/wilf/data/viewflow/tools/viewflow-target.sh:27: }
/home/wilf/data/viewflow/tools/viewflow-target.sh:28: prepare_target() {
/home/wilf/data/viewflow/tools/viewflow-target.sh:29:   local args=(--config "$(target_config "$1")")
/home/wilf/data/viewflow/tools/viewflow-target.sh:30:   if [[ "$1" == windows ]]; then args+=(--start-windows); fi
/home/wilf/data/viewflow/tools/viewflow-target.sh:31:   python3 "$script_dir/desktop_peer_discovery.py" "${args[@]}"
/home/wilf/data/viewflow/tools/viewflow-target.sh:32: }
/home/wilf/data/viewflow/tools/viewflow-target.sh:33: wait_ready() {
/home/wilf/data/viewflow/tools/viewflow-target.sh:34:   local target=$1 expected_remote=$2
/home/wilf/data/viewflow/tools/viewflow-target.sh:35:   python3 - "$(target_config "$target")" "$runtime" "$expected_remote" <<'CHECK'
/home/wilf/data/viewflow/tools/viewflow-target.sh:36: import pathlib,sys,time,json
/home/wilf/data/viewflow/tools/viewflow-target.sh:37: source,root,expected_remote=pathlib.Path(sys.argv[1]),pathlib.Path(sys.argv[2]),sys.argv[3]
/home/wilf/data/viewflow/tools/viewflow-target.sh:38: template=json.loads(source.read_text())
/home/wilf/data/viewflow/tools/viewflow-target.sh:39: for _ in range(100):
/home/wilf/data/viewflow/tools/viewflow-target.sh:40:     try:
/home/wilf/data/viewflow/tools/viewflow-target.sh:41:         pid=int((root/'desktop-drag/daemon.pid').read_text())
/home/wilf/data/viewflow/tools/viewflow-target.sh:42:         executable=pathlib.Path(f'/proc/{pid}/exe').resolve(strict=True)
/home/wilf/data/viewflow/tools/viewflow-target.sh:43:         valid=executable.name=='vf-cursor-peer' and (root/'hyprland.sock').exists()
/home/wilf/data/viewflow/tools/viewflow-target.sh:44:         valid=valid and int((root/'cursor-ready').read_text())==pid
/home/wilf/data/viewflow/tools/viewflow-target.sh:45:         config=json.loads((root/'desktop-drag/config').read_text())
/home/wilf/data/viewflow/tools/viewflow-target.sh:46:         valid=valid and config['remote']==expected_remote
/home/wilf/data/viewflow/tools/viewflow-target.sh:47:         valid=valid and config['pointer']['devices']==template['pointer']['devices']
…
/home/wilf/data/viewflow/tools/viewflow-target.sh:50:     except (OSError,ValueError,KeyError): pass
/home/wilf/data/viewflow/tools/viewflow-target.sh:51:     time.sleep(.2)
/home/wilf/data/viewflow/tools/viewflow-target.sh:52: raise SystemExit('Viewflow 连接未就绪；请检查服务日志。')
/home/wilf/data/viewflow/tools/viewflow-target.sh:53: CHECK
/home/wilf/data/viewflow/tools/viewflow-target.sh:54: }
/home/wilf/data/viewflow/tools/viewflow-target.sh:55: case "${1:-}" in
/home/wilf/data/viewflow/tools/viewflow-target.sh:56:   status)
/home/wilf/data/viewflow/tools/viewflow-target.sh:57:     if [[ -r "$state_dir/hdmi-target" ]]; then cat "$state_dir/hdmi-target"
/home/wilf/data/viewflow/tools/viewflow-target.sh:58:     else current_target
/home/wilf/data/viewflow/tools/viewflow-target.sh:59:     fi
/home/wilf/data/viewflow/tools/viewflow-target.sh:60:     ;;
/home/wilf/data/viewflow/tools/viewflow-target.sh:61:   set)
/home/wilf/data/viewflow/tools/viewflow-target.sh:62:     requested=${2:-}
/home/wilf/data/viewflow/tools/viewflow-target.sh:63:     requested_unit=$(unit_for "$requested") || { echo '目标必须是 windows 或 macos' >&2; exit 2; }
/home/wilf/data/viewflow/tools/viewflow-target.sh:64:     mkdir -p "$config_dir" "$state_dir"
/home/wilf/data/viewflow/tools/viewflow-target.sh:65:     exec 9>"$config_dir/target-switch.lock"
/home/wilf/data/viewflow/tools/viewflow-target.sh:66:     flock 9
/home/wilf/data/viewflow/tools/viewflow-target.sh:67:     # HDMI must remain usable even when a Viewflow peer cannot connect.
/home/wilf/data/viewflow/tools/viewflow-target.sh:68:     case "$requested" in windows) button=input1 ;; macos) button=input2 ;; esac
/home/wilf/data/viewflow/tools/viewflow-target.sh:69:     if ! "$hdmi_helper" send "$button" >/dev/null; then
/home/wilf/data/viewflow/tools/viewflow-target.sh:70:       printf 'HDMI 切换到 %s 失败。\n' "$requested" >&2
/home/wilf/data/viewflow/tools/viewflow-target.sh:71:       exit 1
/home/wilf/data/viewflow/tools/viewflow-target.sh:72:     fi
/home/wilf/data/viewflow/tools/viewflow-target.sh:73:     temporary=$(mktemp "$state_dir/.hdmi-target.XXXXXX")
/home/wilf/data/viewflow/tools/viewflow-target.sh:74:     printf '%s\n' "$requested" > "$temporary"
/home/wilf/data/viewflow/tools/viewflow-target.sh:75:     mv -f "$temporary" "$state_dir/hdmi-target"
/home/wilf/data/viewflow/tools/viewflow-target.sh:76:     # The topology supervisor reads the selector after the hardware command.
/home/wilf/data/viewflow/tools/viewflow-target.sh:77:     # Its next probe reconciles Sidecar and both Mac viewports without a second
/home/wilf/data/viewflow/tools/viewflow-target.sh:78:     # button path racing service ownership.
/home/wilf/data/viewflow/tools/viewflow-target.sh:79:     if systemctl --user is-active --quiet viewflow-display-topology.service; then
/home/wilf/data/viewflow/tools/viewflow-target.sh:80:       cat "$state_dir/hdmi-target"
/home/wilf/data/viewflow/tools/viewflow-target.sh:81:       exit 0
/home/wilf/data/viewflow/tools/viewflow-target.sh:82:     fi
/home/wilf/data/viewflow/tools/viewflow-target.sh:83:     previous=$(current_target)
/home/wilf/data/viewflow/tools/viewflow-target.sh:84:     previous_unit=$(unit_for "$previous")
/home/wilf/data/viewflow/tools/viewflow-target.sh:85:     # Discover before releasing the current input route; HDMI has already switched.
/home/wilf/data/viewflow/tools/viewflow-target.sh:86:     if ! expected_remote=$(prepare_target "$requested"); then
/home/wilf/data/viewflow/tools/viewflow-target.sh:87:       echo 'HDMI 已切换；未找到目标电脑，保留当前控制通道。' >&2
/home/wilf/data/viewflow/tools/viewflow-target.sh:88:       exit 1
/home/wilf/data/viewflow/tools/viewflow-target.sh:89:     fi
/home/wilf/data/viewflow/tools/viewflow-target.sh:90:     runtime_remote=$(python3 - "$runtime/desktop-drag/config" <<'REMOTE'
/home/wilf/data/viewflow/tools/viewflow-target.sh:91: import json,sys
/home/wilf/data/viewflow/tools/viewflow-target.sh:92: try: print(json.load(open(sys.argv[1]))['remote'])
/home/wilf/data/viewflow/tools/viewflow-target.sh:93: except (OSError,ValueError,KeyError): pass
/home/wilf/data/viewflow/tools/viewflow-target.sh:94: REMOTE
/home/wilf/data/viewflow/tools/viewflow-target.sh:95: )
/home/wilf/data/viewflow/tools/viewflow-target.sh:96:     if [[ "$requested" != "$previous" || "$runtime_remote" != "$expected_remote" ]] || ! systemctl --user is-active --quiet "$requested_unit"; then
/home/wilf/data/viewflow/tools/viewflow-target.sh:97:       systemctl --user stop "$previous_unit"
/home/wilf/data/viewflow/tools/viewflow-target.sh:98:       if ! systemctl --user restart "$requested_unit" || ! wait_ready "$requested" "$expected_remote"; then
/home/wilf/data/viewflow/tools/viewflow-target.sh:99:         systemctl --user stop "$requested_unit" || true
/home/wilf/data/viewflow/tools/viewflow-target.sh:100:         systemctl --user start "$previous_unit" || true
/home/wilf/data/viewflow/tools/viewflow-target.sh:101:         echo 'HDMI 已切换；Viewflow 连接失败，已尝试恢复原控制目标。' >&2
/home/wilf/data/viewflow/tools/viewflow-target.sh:102:         exit 1
/home/wilf/data/viewflow/tools/viewflow-target.sh:103:       fi
…
/home/wilf/data/viewflow/tools/viewflow-target.sh:105:     # Remember the selected input route across login/reboot.
/home/wilf/data/viewflow/tools/viewflow-target.sh:106:     systemctl --user enable "$requested_unit" >/dev/null
/home/wilf/data/viewflow/tools/viewflow-target.sh:107:     if [[ "$previous_unit" != "$requested_unit" ]]; then
/home/wilf/data/viewflow/tools/viewflow-target.sh:108:       systemctl --user disable "$previous_unit" >/dev/null
/home/wilf/data/viewflow/tools/viewflow-target.sh:109:     fi
/home/wilf/data/viewflow/tools/viewflow-target.sh:110:     temporary=$(mktemp "$state_dir/.viewflow-target.XXXXXX")
/home/wilf/data/viewflow/tools/viewflow-target.sh:111:     printf '%s\n' "$requested" > "$temporary"
/home/wilf/data/viewflow/tools/viewflow-target.sh:112:     mv -f "$temporary" "$state_dir/viewflow-target"
/home/wilf/data/viewflow/tools/viewflow-target.sh:113:     cat "$state_dir/hdmi-target"
/home/wilf/data/viewflow/tools/viewflow-target.sh:114:     ;;
/home/wilf/data/viewflow/tools/viewflow-target.sh:115:   *) echo "Usage: $0 {status|set windows|set macos}" >&2; exit 2 ;;
/home/wilf/data/viewflow/tools/viewflow-target.sh:116: esac
```

### External recording dependency

```text
/home/wilf/.config/hypr/scripts/record-script.sh:1: #!/usr/bin/env bash
/home/wilf/.config/hypr/scripts/record-script.sh:2: 
/home/wilf/.config/hypr/scripts/record-script.sh:3: getdate() {
/home/wilf/.config/hypr/scripts/record-script.sh:4:     date '+%Y-%m-%d_%H.%M.%S'
/home/wilf/.config/hypr/scripts/record-script.sh:5: }
/home/wilf/.config/hypr/scripts/record-script.sh:6: getaudiooutput() {
/home/wilf/.config/hypr/scripts/record-script.sh:7:     pactl list sources | grep 'Name' | grep 'monitor' | cut -d ' ' -f2
/home/wilf/.config/hypr/scripts/record-script.sh:8: }
/home/wilf/.config/hypr/scripts/record-script.sh:9: getactivemonitor() {
/home/wilf/.config/hypr/scripts/record-script.sh:10:     hyprctl monitors -j | jq -r '.[] | select(.focused == true) | .name'
/home/wilf/.config/hypr/scripts/record-script.sh:11: }
/home/wilf/.config/hypr/scripts/record-script.sh:12: 
/home/wilf/.config/hypr/scripts/record-script.sh:13: mkdir -p "$(xdg-user-dir VIDEOS)"
/home/wilf/.config/hypr/scripts/record-script.sh:14: cd "$(xdg-user-dir VIDEOS)" || exit
/home/wilf/.config/hypr/scripts/record-script.sh:15: if pgrep wf-recorder > /dev/null; then
/home/wilf/.config/hypr/scripts/record-script.sh:16:     notify-send "Recording Stopped" "Stopped" -a 'record-script.sh' &
/home/wilf/.config/hypr/scripts/record-script.sh:17:     pkill wf-recorder &
/home/wilf/.config/hypr/scripts/record-script.sh:18: else
/home/wilf/.config/hypr/scripts/record-script.sh:19:     notify-send "Starting recording" 'recording_'"$(getdate)"'.mp4' -a 'record-script.sh'
/home/wilf/.config/hypr/scripts/record-script.sh:20:     if [[ "$1" == "--sound" ]]; then
/home/wilf/.config/hypr/scripts/record-script.sh:21:         wf-recorder --pixel-format yuv420p -f './recording_'"$(getdate)"'.mp4' -t --geometry "$(slurp)" --audio="$(getaudiooutput)" & disown
/home/wilf/.config/hypr/scripts/record-script.sh:22:     elif [[ "$1" == "--fullscreen-sound" ]]; then
/home/wilf/.config/hypr/scripts/record-script.sh:23:         wf-recorder -o $(getactivemonitor) --pixel-format yuv420p -f './recording_'"$(getdate)"'.mp4' -t --audio="$(getaudiooutput)" & disown
/home/wilf/.config/hypr/scripts/record-script.sh:24:     elif [[ "$1" == "--fullscreen" ]]; then
/home/wilf/.config/hypr/scripts/record-script.sh:25:         wf-recorder -o $(getactivemonitor) --pixel-format yuv420p -f './recording_'"$(getdate)"'.mp4' -t & disown
/home/wilf/.config/hypr/scripts/record-script.sh:26:     else
/home/wilf/.config/hypr/scripts/record-script.sh:27:         wf-recorder --pixel-format yuv420p -f './recording_'"$(getdate)"'.mp4' -t --geometry "$(slurp)" & disown
/home/wilf/.config/hypr/scripts/record-script.sh:28:     fi
/home/wilf/.config/hypr/scripts/record-script.sh:29: fi
```
