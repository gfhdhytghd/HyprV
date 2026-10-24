# Live data-source parity audit

2026-10-24 00:10–00:35 EDT, parity QA lead. Live release `eda5ca090166790a` (`~/.local/bin/hyprv-shell` sha256 `2073ea26…`), `hyprv-shell.service` PID 2709217, `hyprv-privhelper` active, legacy `hyprv-cpu-power` / `hyprv-fan-permissions` / `hyprv-fan-control` disabled, Quickshell gone.

Fixture parity cannot see live-only data gaps. This audit checks every displayed field against what the native source actually yields on this host, as user wilf.

## Method (all read-only)

1. **Live shell:** `hyprv-shell ipc call bar dataStatus`, `/proc/2709217/environ` key names, the shell journal since start, `strings` on the installed binary, and a `grim` of the live bar (3072×58 at scale 2).
2. **Native providers as wilf:** `target/release/hyprv-svc-probe --duration 25` (observation-only; zero actions; private state dir). It ran with the session's `HYPRLAND_INSTANCE_SIGNATURE` corrected to the running instance `…_1792607222_…`. The inherited terminal value pointed at a dead instance, which explains the first run's `hyprland: Connection refused`; that was an environment artifact, not a defect.
3. **Panels without touching the live session:**
   - Ran the installed release binary as `--nested-child <dump> <scene>` (read-only, real providers, no fixture) for scenes `control`, `wifi`, `stats`, `calendar` and `volume`.
   - It ran inside a private virtual KWin plus nested Hyprland, under runtime dir `/run/user/1000/hva`. That dir held symlinks to the real session `bus`/`pipewire-0`/`pulse`; the system bus was the real one.
   - Output was 1280×720 at scale 1, so the bar falls back to its compact layout there.
   - No files outside the scratch dir were written by these runs. The only newer files were live hook writes by other processes.
4. **Ground truth for comparison:**
   - `/proc/meminfo`, `/sys/class/thermal`, `sensors`, `nmcli`, `busctl`, `powerprofilesctl`, `swaync-client` and `hyprctl` (live instance).
   - The legacy state files `~/.local/state/hyprv/{brightness-hdr.json,fan-percent}`.
   - `dcal --json socket`.

Host condition during the audit: another session was running 16+ `bash -c 'while :; do :; done'` loops plus test jobs. CPU was at 100%, package temperature 103–105 °C, and the privhelper reported `emergency CPU temperature; maximum cooling` (PWM 255). That is real host load, not a shell defect. It does explain the red 100 °C in the bar.

## Field table

Verdicts:
- **OK:** the native source yields the same value as the system or legacy source.
- **OK=legacy:** both shells show the same empty or placeholder state on this host.
- **DEFECT:** the live shell is missing data that the legacy shell had.
- **N/E:** not exercisable on this host right now.

| Surface · field | Legacy source (spec) | Native source | Live yield as wilf | Verdict |
|---|---|---|---|---|
| Bar · CPU % | `desktop-state.py` /proc/stat | sysstats /proc/stat | 100% = host load | OK |
| Bar · RAM % | `desktop-state.py` MemAvailable | sysstats meminfo | 75.75 vs /proc 75.8 | OK |
| Bar · network rate, iface | `desktop-state.py` route + net/dev | sysstats rtnetlink route + sysfs | br0, 70.6 kB/s | OK |
| Bar · CPU temperature | `scripts/desktop-state.py:43-51` x86_pkg_temp → TCPU → … | sysstats same preference | 104–105 = x86_pkg_temp | OK |
| **Bar · CPU power (W)** | `hyprv-cpu-power.service` (root, turbostat) publishing to `/run/hyprv` | installed release: direct read of `/sys/class/powercap/intel-rapl:0/energy_uj` | **file is `0400 root` → `package_power_w: null` → bar shows `-- W`**. `/run/hyprv` no longer exists (sampler unit disabled). The installed privhelper exposes only `GetFans`/`SetFanLimit`/`RemoveExternalGpu`/`ProtocolVersion`; no `SubscribeCpuPower`. | **DEFECT L1** (already owned by svc-power) |
| Bar · workspaces, active ws, window title | Hyprland | hyprland socket + socket2 | ws 5 active = `hyprctl activeworkspace`; title shown | OK |
| Bar · media buttons | always shown (`ui-bar-layout.md:358`) | — | shown, no player | OK |
| Bar · volume / mute | PipeWire | audio | 25%, muted = sink | OK |
| Bar · tray | SNI watcher | tray | 3 items (Sunshine, TelegramDesktop, Claude) = watcher's 3 | OK |
| Bar · notifications bell, DND, dot | SwayNC | notifications | 207, DND on = `swaync-client -c/-D` | OK |
| Bar · network icon (wired) | NM | network | wired br0 + Wi-Fi Superpower | OK |
| Bar · battery | UPower | power | no battery on desktop → hidden | OK=legacy |
| Bar · power profile (dataStatus) | power-profiles-daemon | power | performance = `powerprofilesctl get` | OK |
| Island · agent session pill | hook state + app-server | agent_island | live Claude session shown, timer running | OK |
| Island · media / spectrum | MPRIS / cava | mpris, spectrum | no MPRIS player present | N/E |
| Control panel · Wi-Fi tile | NM | network | "Superpower" connected | OK |
| Control panel · Bluetooth tile | BlueZ | bluetooth | Magic Trackpad connected = BlueZ | OK |
| Control panel · media card | MPRIS | mpris | "Idle" (no player) | OK |
| Control panel · deskflow / viewflow targets | state files + units | deskflow, viewflow | windows / hdmi macos | OK |
| Control panel · DND, theme tiles | SwayNC, theme state | notifications, theme | DND on, light | OK |
| Control panel · brightness | `brightness-hdr.json` + DDC | backlight (DDC bus 9) | level 68 = legacy JSON `{"level":68}` | OK |
| Control panel · volume slider | PipeWire | audio | 25%, muted | OK |
| Control panel · fan sliders | user `~/.local/state/hyprv/fan-percent` (`hyprv-fanctl`) | privhelper `GetFans` (`/var/lib/hyprv-privhelper/fan-percent`) | live readout works (chassis 4891 rpm, pump 5212 rpm). Limits 51/43 vs legacy file 44/38. The helper file was written 00:19, after cutover, and the helper default is 80/80, so a post-cutover `SetFanLimit` set them. There is no import of the legacy limits. | OK (data); advisory L3 |
| Stats panel · CPU, cores C1–C8, RAM, network | as bar | sysstats lease | all populated | OK |
| Wi-Fi panel · AP list, signal, security | NM | network | "Superpower · WEP · 0%" = NM `Strength 0`, privacy-only flags (`nmcli` same) | OK=legacy |
| VM tile | systemd `windows-autoshop.service` | vm | inactive/dead | OK |
| Mihomo | `mihomo.service` + HTTP | mihomo | unit inactive (system and user), :9090 closed → offline | OK=legacy |
| **Calendar panel · week events, calendars** | `CalendarBridge.qml:8,166-177` runs `~/.local/bin/dcal --json socket` → `/run/user/1000/dankcal-3194001.sock` (works now) | `calendar.rs:89-90,196` needs `HYPRV_DCAL_SOCKET` or `HYPRV_DCAL_DISCOVERY_FILE` | **neither is set in the live unit's environment** → `last_error: discovery "configure HYPRV_DCAL_SOCKET or HYPRV_DCAL_DISCOVERY_FILE"`. The panel stays on 「正在连接日历…」 forever with no events (nested capture). The pid-suffixed socket name means a static path cannot work. | **DEFECT L2** |
| Calendar · weather strip | weather.sh + `.env` `OPENWEATHER_KEY` | weather, same `.env` path | no `.env` exists → "No API Key" dummy in both | OK=legacy |
| Quick adjust (volume OSD) | PipeWire | audio | muted 25% | OK |
| Inhibit, demo, recording, eGPU, IR DB | various | desktop actors | idle/empty; IR DB 2 buttons loaded | OK |

## Defects

- **L1 (P0, svc-power, already assigned):** CPU package power is always `-- W` in the live bar.
  - The installed release reads root-only RAPL directly.
  - The legacy root sampler is disabled.
  - The installed privhelper has no CPU-power method.
  - The fix (helper `SubscribeCpuPower` + sysstats client) exists in the tree but is not in `eda5ca09`.
  - The shell journal stays silent about the failure, so the missing data was invisible in logs.
- **L2 (P0, svc-sys + packaging):** the calendar never connects live.
  - The native calendar requires env configuration that cutover never provides.
  - Legacy discovery via `dcal --json socket` was not replaced by any live mechanism.
  - Filed in `docs/requests/hyprv-svc-sys.md` with a copy in `hyprv-packaging.md`.
- **L3 (advisory, privhelper / cutover):** the legacy fan limits (`chassis=44 pump=38`) were not imported into `/var/lib/hyprv-privhelper/fan-percent`.
  - A fresh helper falls back to 80/80.
  - The current 51/43 came from a later `SetFanLimit`.
  - User intent from before cutover is lost. Filed in `docs/requests/hyprv-privhelper.md`.

Process gap behind L1/L2: the cutover preflight gates prove fixture parity and helper liveness, but nothing asserts that each data-bearing topic is *available* on the real host. A follow-up gate is proposed in the L2 request: run `hyprv-svc-probe` for N seconds and fail when a field the legacy shell populated is null or `Failed`.

## Evidence (scratch, not retained)

- Probe NDJSON, nested dumps and the bar screenshot are in the session scratchpad and will be deleted once the owners acknowledge.
- Key values are quoted above.
- Nested panel PNGs: control, stats, wifi, calendar (bar + panel) and volume.
