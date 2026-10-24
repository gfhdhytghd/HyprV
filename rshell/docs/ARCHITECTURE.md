# rshell — HyprV shell rewrite in Rust

Goal: replace `../quickshell` (QML + shell/python scripts) with one Rust program.
Hard requirements:
1. **Feature parity** with the QML shell, including IPC targets used by Hyprland binds
   (`bar`, `controlPanel`, `calendar`, `quickAdjust`, see `hypr/*.lua`).
2. **Pixel parity**: same geometry, colors, fonts, radii, shadows, animation durations
   and easing curves. Specs in `docs/spec/*.md` are the source of truth, derived from QML.
3. **Performance**: pure event-driven. Zero wakeups when idle. No fork/exec of helper
   tools at runtime. Redraw only on state change or while an animation runs.

## Process model
Single binary `hyprv-shell`:
- **UI thread**: calloop event loop + smithay-client-toolkit (wlr-layer-shell,
  fractional-scale-v1, viewporter, xdg-output). Owns all surfaces.
- **Service runtime**: tokio (current_thread or 2 workers) running service actors
  (zbus, udev, pipewire, hyprland socket2, websockets). Services publish typed state
  diffs into a calloop channel → UI thread updates reactive state → marks damage.
- **IPC**: Unix socket `$XDG_RUNTIME_DIR/hyprv/shell.sock`; CLI `hyprv-shell ipc call
  <target> <fn> [args..]` mirrors `quickshell ipc call` semantics.
- **Privileged helper** `hyprv-privhelper` (fan control, etc.), polkit/systemd-activated,
  minimal surface, modelled after KDE KAuth.

## Crates (`crates/`)
| crate | owner | content |
|---|---|---|
| `hyprv-ui` | engine agent | layer-shell surfaces, skia-safe GL (EGL) rendering, retained scene, reactive properties, layout, text (skia textlayout), Qt-exact animation/easing (port QEasingCurve incl. bezier, OutBack overshoot, SpringAnimation, Behavior semantics), input/hover/click/scroll, damage tracking, frame callbacks, offscreen render-to-PNG for parity tests |
| `hyprv-core` | core agent | state store, typed topics, service trait + lifecycle, subscriber-counted sampling, calloop↔tokio bridge, IPC server+CLI, config/theme loading |
| `hyprv-services` | later | one module per domain (network/NM, bluetooth/BlueZ, audio/pipewire, mpris, upower+power-profiles, backlight/udev+DDC, systemd units, mihomo ws, hyprland socket2, sysstats sampler, fan, VM, calendar, weather, agent-island, deskflow/viewflow…) — KDE-style event sources, see services spec |
| `hyprv-shell` | later | the actual bar / panels / island ported component by component |
| `hyprv-privhelper` | later | root helper |

## Rendering decisions
- skia-safe with `gl` + `textlayout` features, EGL via wayland-egl; one GL context shared
  across surfaces. Fallback CPU raster only for tests.
- Background blur is done by Hyprland (layerrule on namespace `hyprv-quickshell`), so we
  keep that namespace and just output premultiplied translucent pixels.
- Text: same font families/sizes/weights as QML; match Qt's FreeType hinting/AA settings.
- Fractional scaling handled natively; render at buffer scale, never resample.

## Parity testing
Every component must be renderable offscreen at a given state to PNG. Reference images are
rendered from the original QML (never from the live desktop) into `parity/reference/` by
`parity/capture.sh`; fixtures live in `parity/fixtures/`. Each UI crate replays those fixtures;
`parity/verify/verify.py` diffs the results (see "Parity acceptance" below).

## Rules for all agents
- Only write inside your assigned paths. Never edit `../quickshell` or `../hypr`.
- No sudo, no git commit/push, never kill/restart quickshell or Hyprland.
- Write findings in English Markdown; cite source as `file.qml:line`.

## Policy clarification: subprocesses
- Forbidden: any subprocess on a timer, for status/reads, or kept running for monitoring.
- Allowed: a one-shot exec for a **user-initiated action** when no native API exists
  (e.g. plasma-apply-colorscheme, awww, xsettingsd), and only if the legacy shell did the same
  thing. Feature parity wins over purity here; document each such exec in the crate README.

## Parity acceptance (decided 2026-10-23, tightened 2026-10-23 by parity QA)
References: `parity/reference/<component>/` (Qt's RHI/OpenGL scene graph rendering the original
QML at the true output DPR with a deterministic virtual clock; see `parity/README.md`).
Measured reference noise floor over two full capture runs (904 images): images whose manifest
entry has `distance_field_text: false` are **bit-identical** run to run; images with
`distance_field_text: true` (visible `Text.QtRendering` text inside the crop) vary by
≤ 1.88/255 on a few dozen pixels (Qt's own GPU glyph-atlas noise).
- Gate (must pass), measured on PREMULTIPLIED RGBA (what the compositor composites), per image
  and for every animation sample frame:
  - no structural differences (geometry/position/size/color token/text content identical;
    verifier check: painted footprint within 1 logical px, painted-mask IoU ≥ 0.98);
  - `distance_field_text: false` → **exact** (max channel diff 0): the reference carries no
    noise, so any difference is a real rendering difference;
  - `distance_field_text: true` → max channel diff ≤ 8/255 and mean abs error over painted
    pixels ≤ 0.5/255.
- Interim tracking metric (not a pass): the earlier "≤ 8/255 max, ≤ 0.5/255 painted mean on
  every image" gate, reported as `gate v1` in `docs/verify/parity.md`.
- Verification: `python3 parity/verify/verify.py` renders every UI crate's fixture replay and
  writes `docs/verify/parity.md` (per state pass/fail, max/mean, heatmaps, root-cause class);
  owners receive actionable items in `docs/requests/<crate>.md`.
- Stretch (pursue after gate): bit-exact, by porting Qt Quick's own rasterization paths —
  QSGBasicInternalRectangleNode / QSGRectangleNode vertex antialiasing geometry for (rounded)
  rectangles and borders, Qt's SVG renderer behavior (QSvgRenderer/ QtSvg) for icons, and
  distance-field text (engine is on it).

## Disk policy (2026-10-23, after the root FS hit 100%)
- Never create additional full Cargo target dirs in scratch/snapshots; snapshots must reuse
  one shared CARGO_TARGET_DIR (e.g. rshell/target) with CARGO_INCREMENTAL=0.
- Delete your own scratch build dirs, frozen copies and large raw captures as soon as the
  evidence you need is summarized (keep only small receipts/hashes/PNGs actually referenced).
- Check `df -h /` before any job that can write > 5 GB; abort if free space < 30 GB.

## Memory budget (2026-10-23)
Steady-state RSS after every popup has been opened at least once must be ≤ the live
Quickshell's (~320 MiB measured), and idle RSS before any popup well below it. Startup
precaching is allowed only if it stays within this budget.

## Runtime desktop/laptop profile contract (Phase A, 2026-10-24)

Runtime contract (implemented by the shared selector below; deployment is separately gated): choose one immutable runtime profile
before constructing services/UI. `HYPRV_PROFILE=desktop|laptop` overrides autodetection;
invalid present values are startup errors. Otherwise use a reviewed exact portable-XPS
DMI table or a present physical system battery; never match all `XPS` names or accept
UPS/peripheral batteries. Missing/ambiguous evidence logs a desktop fallback. Latch the
selection across AC/hotplug and legacy variant changes. No compilation forks; desktop
default behavior stays unchanged. Laptop service policy cannot activate desktop cooling
or eGPU. The user permits optional read-only CPU-power telemetry through a separate
CPU-only entry point/install set, never the combined desktop helper. Cooling/failsafe
eligibility independently requires validated desktop hardware. All sysfs readers honor
test-only HYPRV_SYSFS_ROOT; system-bus clients honor DBUS_SYSTEM_BUS_ADDRESS, without
host fallback in isolated tests. Full rule, trust boundary and deployment prerequisites are
in [laptop-deploy.md](spec/laptop-deploy.md). Engine implementation and measured limits
are recorded in [laptop-impl-engine.md](verify/laptop-impl-engine.md).

### Implemented shared selector (engine, 2026-10-24)

`hyprv_core::profile::{Profile, profile, sys_root, sys_path}` is the single shared
selector. `profile()` latches once; invalid explicit overrides fail startup. Exact
case-normalized portable names XPS 13 9350 / XPS 9350 or a present kernel Battery
with scope=System select Laptop; missing/ambiguous evidence defaults to Desktop
with diagnostics. No substring match, UPS/peripheral battery or AC-driven change.
Readers use the latched HYPRV_SYSFS_ROOT (default /), never host fallback. Detection
rejects symlinks escaping that root. Missing battery scope is deliberately ambiguous
and requires an explicit override until corroborating internal identity is available.
System-bus clients honor DBUS_SYSTEM_BUS_ADDRESS. No profile compilation forks.

Laptop cutover uses the reviewed laptop config patch to rename executable calls to
hyprv-shell; no global quickshell executable is replaced. Native CLI additionally
accepts both `ipc -p PATH call ...` and `-p PATH ipc call ...`, and maps
`--path .../StartupConfirm.qml` to the existing one-shot startup-confirm command.
The newer shared API's HYPRV_SYSFS_ROOT is a filesystem root (default /), so injected
sysfs is at ROOT/sys/class/...; this supersedes Phase A deploy text saying default /sys.
