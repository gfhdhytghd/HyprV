# parity/fullscreen — whole-output references from the ORIGINAL shell

`parity/capture.sh` renders QML components into offscreen textures with mock Quickshell
modules. This directory does the opposite: it runs the **unmodified** quickshell config
(`quickshell -p ~/.config/HyprV/quickshell`) as a real Wayland client of a real Hyprland,
and captures **whole outputs** at the user's real layout and scale through the compositor
(grim screencopy). These references show what assembled layer-shell windows actually look
like after composition: every PanelWindow, popup and overlay, in Hyprland's layer order,
on a transparent background.

```sh
parity/fullscreen/tools/discover-outputs.sh       # read-only: live layout -> fixtures/outputs.json
python3 -I parity/fullscreen/tools/gen-fixtures.py      # component fixtures -> fixtures/*.json
python3 -I parity/fullscreen/tools/resolve-targets.py   # Qt-resolved pointer targets (parity-host)
parity/fullscreen/capture.sh                      # every fixture, both themes -> reference/
parity/fullscreen/capture.sh idle calendar-morph  # some fixtures
```

| env | meaning |
|---|---|
| `FS_THEMES="light dark"` | themes |
| `FS_OUT=dir` | output dir (default `reference/`) |
| `FS_SKIP_HOST=1` | skip the parity-host pass (distance_field_text then defaults to `true`, flagged) |
| `QUICKSHELL_SRC=dir` | QML source (default `~/.config/HyprV/quickshell`) |

Needs `kwin_wayland`, `Hyprland` (0.56), `grim`, `quickshell`, `dbus-run-session`,
`pipewire`/`pw-cli`/`pw-metadata`, `cc`, `wayland-scanner`, python3 with numpy, PIL,
dbus-python and gi. One fixture × theme takes about 30–60 s, mostly compositor and shell
startup.

## Isolation (never the live session)

`capture.sh` is the only entry point. It creates a private runtime dir
`/tmp/claude-1000/fs.XXXX` and runs everything under one `dbus-run-session` that has **no
service activation**. That bus also serves as the system bus, so nothing can reach the
real logind, BlueZ, NetworkManager, UPower or PipeWire. Inside it, `tools/run.py`:

- starts a headless **KWin** (`--virtual`, the real output's pixel size) as the outer
  compositor, with a window rule that puts the nested compositor fullscreen at 0,0;
- starts **Hyprland** on KWin with its wayland backend, `LIBSEAT_BACKEND=seatd` (no
  seatd socket, so it never asks logind for the session), `AQ_DRM_DEVICES=<nonexistent>`
  and Xwayland disabled. Its only monitor `WAYLAND-1` gets the real pixel size and scale,
  and the harness asserts both. The cursor is invisible: the nested compositor would
  otherwise draw a software cursor into the frame. Blur, shadows, borders, gaps and
  animations are off. Hyprland waits only 2 s for its first output, so KWin gets 1 s to
  settle first and GL shader caches are shared across runs. A start that still misses the
  output is retried (up to 3 times);
- runs every `hyprctl` with `XDG_RUNTIME_DIR` and the nested signature set inline, after
  asserting that the signature differs from every live one (`/run/user/$UID/hypr/*`);
- guards every `rm` to the private runtime dir.

The shell's environment is the live one: `QSG_RENDER_LOOP=basic` (launch.sh),
`LANG=zh_CN.UTF-8`, `QT_QPA_PLATFORMTHEME=qt6ct`, `QT_IM_MODULE=fcitx`, and the rest from
`/proc/<live quickshell>/environ`. It also gets a **fake `$HOME`**:

- the user's fontconfig, fonts and icons are symlinked, read-only;
- `qt6ct.conf` is derived per theme exactly as `parity/capture.sh` derives it;
- `ui-state.env` is `theme=<theme> variant=v2`;
- the QML's assets dir is symlinked;
- every script under `scripts/`, `~/.local/bin/{dcal,viewflow-windows}` and
  `~/.config/hypr/scripts/record-script.sh` is replaced by a stub wrapper.

`PATH` contains only a shim directory, so `sh`, `bash`, `hyprctl`, `playerctl`,
`swaync-client` and every other command the QML runs by name also reach the stub. **No
real helper ever runs.** `tools/stub.py` answers with the fixture's `processes` entries,
using the same semantics as parity-host's Process mock (substring `match`, `stdout`,
`lines` at virtual ms, `stream`, `exitCode`, `delay_ms`). Unmatched commands exit 1 and
are logged to `reference/logs/<state>--<theme>/commands.jsonl`.

## How fixture state reaches the real shell

| fixture field | real-shell mechanism |
|---|---|
| `processes` | stub.py (above) |
| `services.hyprland.workspaces` / `focusedWorkspace` | persistent workspaces in the nested config + `dispatch workspace` |
| `services.hyprland.activeToplevel.title` | `native/titlewin`: a fully transparent (alpha 0) xdg toplevel with that title, mapped after the shell starts; border, gaps, rounding, shadow and dim are 0 |
| `services.tray` | `mocks/services.py`: one StatusNotifierItem per entry (own bus connection, registered in fixture order with the shell's own watcher) + `com.canonical.dbusmenu` from `menu.entries` |
| `services.mpris.players` | MPRIS players `org.mpris.MediaPlayer2.parity<N>` (Stopped, no metadata; media details come from `media-status.sh` as in the fixtures) |
| `services.bluetooth` | `org.bluez` ObjectManager: adapter `hci0` + `Device1`/`Battery1` |
| `services.upower`, `powerProfile` | `org.freedesktop.UPower` (+ DisplayDevice), `net.hadess.PowerProfiles` and `org.freedesktop.UPower.PowerProfiles` |
| `services.audio` | a private PipeWire (no ALSA/BlueZ/D-Bus modules) with one null sink `parity-sink` as `default.audio.sink`; `channelVolumes = volume³` (Quickshell reports the cube root) |
| `sockets` | `mocks/sockets.py`: line JSON-RPC peers in the run dir; `dcal --json socket` output is rewritten to point there |
| `set.ipc` | `quickshell ipc -p <source> call …` (the shell's own IpcHandler) |
| pointer steps | `native/vpointer` (zwlr_virtual_pointer_v1, absolute in output-logical px) at `targets` |
| `now`, `advance`, `frames`, `settle_ms`, `tick_ms` | `native/vclock.so` (LD_PRELOAD into the shell and its children) |

`set.root`, `set.services` and `invoke` cannot be replayed in a real shell, and
`gen-fixtures.py` refuses fixtures that use them.

### Virtual time

`vclock.so` replaces `CLOCK_MONOTONIC*`/`CLOCK_BOOTTIME` with the shell's start time plus a
virtual offset that the harness writes to an mmapped file. It replaces `CLOCK_REALTIME`,
`gettimeofday` and `time` with fixture `now` plus that offset. It also caps finite
poll/epoll timeouts at 4 ms, so a loop sleeping toward a frozen-clock deadline wakes when
time moves.

`QSG_RENDER_LOOP=basic` and Qt's default timer-mode animation driver read that clock, so
Behaviors, Animations, QML Timers and `Date` all run on virtual time. The stubs read the
same clock (from the file) and sleep with relative timeouts. `time.sleep` under the
preload would wait for an absolute virtual deadline.

The harness advances in `tick_ms` steps (default 16.667), one step per ~20 ms real time
(`FS_TICK_REAL`). That way the shell renders about one frame per tick, as parity-host
does. Faster ticking skipped frames and broke frame-driven QML: callLater chains and
polish passes. Order of a run:
1. settle `settle_ms`;
2. park the pointer in the far corner and add one tick;
3. replay the steps. After every input or IPC step, the harness waits at frozen time until
   the output is stable. New popup surfaces are then created, configured and drawn before
   time moves on.

A capture is taken at a **frozen** virtual time. grim repeats until two consecutive frames
are identical. `virtual_ms` follows parity-host: absolute time since shell start for
plain captures, and the offset from the triggering step for `frames`. Two complete runs
of the animated fixtures were identical to within ±1 on ≤12 px per frame.

### Pointer targets

Semantic targets (`{text}`, `{contains}`, `{image}`) cannot be resolved inside the real
shell. `tools/resolve-targets.py` replays each fixture prefix in **parity-host**
(spec-island's harness, run unchanged through `parity/capture.sh`), dumps the visible
items, and picks the target the way `pointFor()` does. It writes the result as
`targets: {"STEP/key": [x, y]}` in output-logical px, which is the
`tests/shared_common/targets.json` convention. Where the dark theme resolves elsewhere,
the dark point is stored in `targets_by_theme.dark`. The Wi-Fi icon is the one case so
far: its dark SVG sits 2 px higher.

## Output contract

```
reference/<state>[--<frame>]--<theme>@<scale>.png     whole output, no crop
reference/manifest.json
reference/host/fullscreen/…                           parity-host render of the same fixtures (cross-check)
reference/logs/<state>--<theme>/                      compositor/shell/mocks logs, commands.jsonl
```

- Pixels are 8-bit RGBA with straight alpha, sRGB, on a transparent background. There is
  no wallpaper, no blur and no compositor animation. Compare **premultiplied**.
- Alpha comes from a matte: each capture is grabbed over Hyprland `background_color`
  black, then white, then black again, and the two black grabs must be identical.
  Black gives the premultiplied colour exactly. `255 − (white − black)` gives alpha.
  Hyprland's blend rounds some edge pixels differently per channel, typically 150–1300
  of ~150 000 non-transparent pixels, almost all by 1–2. Alpha is taken as the per-pixel
  median channel, and those pixels are listed per image in `matte.uncertain_px_coords`.
  A capture fails if more than 64 px disagree by more than 2, or any pixel by more than 8.
  If the output changes between the black and the second black grab, the whole matte is
  retried (up to 3 times).
- Each manifest image adds these fields to the parity-host entry:
  - `output`: real name, make and model, position, refresh, and the nested name `WAYLAND-1`;
  - `width/height_logical`: the real output's logical size;
  - `layers`: every layer surface with level, order within the level, namespace and
    geometry, in Hyprland's composition order;
  - `matte`;
  - `distance_field_text`: from parity-host's render of the same frame;
  - `host_crosscheck`: premultiplied max delta and differing pixels against that render.
  - Top-level `composition`, `outputs`, `unreachable` and `source_sha256` (every `*.qml`).

## States

| fixture | from | content |
|---|---|---|
| idle | bar/baseline | bar + idle island |
| media-playing | bar/media-playing | island with a playing track |
| island-expanded | bar/island-expanded-host | island expanded by hover |
| calendar | panels/calendar-week | calendar presented |
| calendar-morph | panels/calendar-morph | island→calendar morph frames, open + close |
| control | panels/cp-main-idle | control panel open |
| control-open-close | panels/cp-open-close | control panel open/close frames |
| wifi | panels/wifi-connected-4 | Wi-Fi popup, 4 networks |
| stats | panels/stats-120 | stats popup after 120 s of samples |
| volume | panels/qa-volume-30 | volume OSD |
| tray-menu, tray-submenu | bar/tray-menu-root/-submenu | tray context menu (+ submenu) |
| tray-overflow | bar/baseline + 96 tray items | overflow grid at the real 3072-px width |

### Unreachable states (not captured, see `manifest.unreachable`)

- **Battery popup.** The trigger is interactive only while `batteryInfo.available`, and
  `battery-info.sh` is polled only while the popup is open (`shell.qml:3342`,
  `4078-4080`). The component references `bar/battery-*` are harness-seeded through
  `set.root`.
- **Mihomo control-panel page.** It is reachable only through parity-host `invoke`.

## Findings from the real shell

- **Unresolvable tray icon.** An SNI `IconName` that no icon theme resolves (fixture
  `telegram`) draws Quickshell's magenta/black missing-icon placeholder, not the "T"
  letter fallback that parity-host's mock shows: Quickshell's icon provider returns a
  valid placeholder image, so the QML fallback never triggers. Component fixture
  `bar/tray-fallback-letter` therefore shows a state the real shell reaches only when the
  icon *source* fails to load.
- **Popup animations start one frame later than in parity-host.** In a real compositor
  the animation clock starts at the first frame rendered after the input, not at the
  event. `control-open-close` open-t0016/-t0060 and close-t0060 therefore differ from
  parity-host's frames: at close-t0060 the full-screen backdrop is at alpha 39 vs 31.
  calendar-morph open-t0100 shows the same effect. From about 120 ms on, frames agree
  with parity-host up to the baseline differences. This is deterministic run to run, so
  it is a property of the real shell, not harness noise.
- **Tray overflow at the real width.** It needs about 50 or more items on a 3072-px bar.
  48 items still fit at a 23.5-px pitch.
