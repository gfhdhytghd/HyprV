# parity — reference images rendered from the original QML

This directory renders the **original** Quickshell QML (`~/.config/HyprV/quickshell`,
read-only) into PNG references, so the Rust port can be diffed pixel by pixel
(`cargo xtask parity`). Nothing here touches the live desktop: no window is
mapped, nothing connects to Hyprland or the running quickshell, and no shell
command the QML would run is ever executed.

```sh
parity/capture.sh                  # every component under fixtures/
parity/capture.sh island           # one component
parity/capture.sh panels cp-main-idle   # one fixture
```

Publishing is atomic per component: renders go to `reference/.<component>.staging.<pid>` (a single-fixture run starts from a copy of the published set) and replace `reference/<component>` with one `mv -T --exchange` (renameat2 RENAME_EXCHANGE) only if every run succeeded; on failure the published set is untouched and staging is removed. Readers never see a half-written set.

A full run renders 904 images (3 components × both themes × scales 1 and 2) in
about 2½ minutes. It needs `weston`, `cmake`, Qt 6.12 dev files and the fonts
below. Exit status is non-zero if any run failed; all failures are listed at
the end.

| env | meaning |
|---|---|
| `PARITY_SCALES="1 2"` | output scales (default: 1 + every scale `hyprctl monitors -j` reports, read-only; today `1 2`) |
| `PARITY_THEMES="light dark"` | themes |
| `QUICKSHELL_SRC=dir` | QML source (default `~/.config/HyprV/quickshell`) |
| `PARITY_DISPLAY=live` | create the GL context on the live compositor's display instead of a private headless Weston (still nothing mapped); see "Rendering path" |
| `PARITY_TICK_MS=16.667` | virtual frame interval (default 60 Hz, a fixture may set `tick_ms`) |
| `PARITY_FIXTURES=dir PARITY_OUT=dir` | debug runs outside the tree |
| `PARITY_DEBUG=1` | print GL device, DPR, palette |
| `PARITY_DUMP_TEXT=1` | print every visible Text/Image with its scene rect at each capture (to author pointer targets) |

## Output contract (for `cargo xtask parity`)

```
reference/<component>/<state>[--<frame>]--<theme>@<scale>.png
reference/<component>/manifest.json
```

- `<state>` is the fixture file name (`fixtures/<component>/<state>.json`).
- `<frame>` is the capture name; frame sequences are `<name>-tNNNN`, where NNNN is the
  virtual time in ms since the triggering step (e.g. `expand-t0105`). It is empty for a
  plain capture.
- `<scale>` is the device pixel ratio (`1`, `2`, or fractional such as `1.5`).
- **Pixels:** 8-bit RGBA PNG with **straight (non-premultiplied) alpha**, sRGB, and a
  transparent background where no surface paints. There is no wallpaper and no compositor
  blur (Hyprland blurs behind `hyprv-quickshell`/`shell:*` surfaces; the Rust renderer
  should also output unblurred premultiplied pixels).
- **Size:** `width_px = round(width_logical × scale)`. Island references are a 640-px-wide
  canvas centred on the bar (canvas x = output centre − 320, y = 0). Bar/panel references
  are crops of the whole output (`crop_logical` = `[x, y, w, h]` in output logical px).
- `manifest.json` holds:
  - `source_sha256`: the hash of every `*.qml` in the source dir at capture time. Re-capture
    when the QML changes.
  - `renderer` and `tolerance`.
  - one entry per image: `file`, `fixture`, `state`, `frame`, `theme`, `scale`,
    `virtual_ms`, logical and pixel sizes, optional `crop_logical`, and
    `distance_field_text`.

**Recommended diff:** convert both images (reference and Rust render) to *premultiplied*
RGBA and compare per channel.
- Images with `distance_field_text: false` are bit-exact run to run. Expect an exact match
  and report the max delta and the count of differing pixels.
- Images with `distance_field_text: true` (visible `Text.QtRendering` text inside the
  crop) show Qt's own GPU noise: ±1 alpha on a few dozen pixels, max premultiplied
  channel delta 1.88 measured over two full runs. Use a per-channel tolerance of **2**.
- Straight-alpha colour channels at very low alpha amplify that noise (up to ±19 at
  α ≈ 14), so never compare straight colour.

## Verifying the Rust crates

```sh
python3 parity/verify/verify.py                    # render all UI crates, diff, write docs/verify/parity.md
python3 parity/verify/verify.py --component bar    # one crate
python3 parity/verify/verify.py --no-render        # re-diff the last renders (e.g. after recapturing)
python3 parity/verify/verify.py --backend gl        # GPU path only. Runs are serialized by out/.verify.lock (a second
                                                   # run waits): concurrent runs share out/ and crate artifact dirs.
python3 parity/verify/zoom.py <component> <file> x y w h out.png [zoom]   # reference/ours/heat close-up
```

Renders come from each crate's own fixture replay, unchanged: island `tests/parity.rs`, bar
`scripts/parity.py`, panels `tests/shared_parity.rs`. Results land in `verify/out/<component>/`
(`actual/`, `heat/`, `tri/` = reference | ours | heat) and `verify/out/results.json`. The report
`docs/verify/parity.md` applies the gate from `docs/ARCHITECTURE.md` and gives an automatic
root-cause class per image, based on the regions sidecars, edges, and frame sequences.
Human triage and owner items are in `docs/verify/parity-findings.md` and
`docs/requests/hyprv-ui-{island,bar,panels}.md` and `hyprv-ui.md`.

## Rendering path (why the references are trustworthy)

`docs/spec/text-rendering.md` §7 shows that the software scene graph
(`QT_QPA_PLATFORM=offscreen`) gets distance-field and fractionally positioned text 18–38 %
wrong. This harness therefore always renders with **Qt's real RHI/OpenGL scene graph at
the true output DPR**:

- `host/main.cpp` (`parity-host`) drives Qt Quick through `QQuickRenderControl` into an
  offscreen `QRhiTexture`. The render target carries the device pixel ratio, so
  `QQuickWindow::effectiveDevicePixelRatio()` matches the live window. That covers text,
  Canvas, layers and Image `sourceSize`, and the host asserts it.
- `QT_SCALE_FACTOR` is **not** used. At dpr 1 it would switch native text from hintslight
  to unhinted. `QHighDpiScaling` stays inactive, as on the live Wayland window.
- The GL context comes from a private headless Weston started for each run on its own
  socket (`--renderer=gl`). Qt gets the same NVIDIA GPU and driver as the live session
  (measured: `NVIDIA RTX PRO 6000 … OpenGL ES 3.2 NVIDIA 615.78.08`). No surface is ever
  created. With `PARITY_DISPLAY=live`, the live compositor's display is used for the
  context instead, as `experiments/text` does. Both modes produce the same pixels, apart
  from the DF noise above.
- Cross-check against `experiments/text/qt-ref` (unmapped `QQuickView` + `grabWindow` on
  the live compositor): `scene-light/dark.qml` at dpr 2 differ in 11 / 4 of 320 000
  pixels, all on distance-field glyphs, max ±1 alpha.
- The host refuses to render without a hardware RHI backend, without the required fonts,
  or with a DPR mismatch.
- The environment is the live quickshell's: `QT_QPA_PLATFORM=wayland`,
  `QT_QPA_PLATFORMTHEME=qt6ct`, `QSG_RENDER_LOOP=basic`,
  `QT_WAYLAND_DISABLE_WINDOWDECORATION=1`, the user's fontconfig (hinting/AA) and `LANG`.
  It is a `QApplication` when the entry file has `//@ pragma UseQApplication`
  (shell.qml), as Quickshell does. Each theme gets a private qt6ct.conf derived from the
  user's: `simple.conf`/`Fluent-light` vs `darker.conf`/`Fluent-dark`, as
  `apply-ui-state.sh` switches them. **But:** the installed `qt6ct-kde 0.11` fails to load
  under Qt 6.12 (Qt falls back to the `generic` theme), in the live quickshell as well as
  here. So today both use Qt's default palette and the `hicolor` icon fallback, and the
  references match that.

### Determinism
- **Virtual clock.** A custom `QAnimationDriver` reports virtual ms. One frame is rendered
  per virtual tick (polish → sync → render), as the live basic render loop does on each
  vsync. Polish also flushes Qt Quick's frame-synchronous hover events, so hover changes
  caused by moving geometry happen at the same virtual frames every run. An infinite
  dummy animation keeps the driver running, so QML `Timer`s never fall back to Qt's
  real-time pause timer.
- **Wall clock.** The host interposes `clock_gettime(CLOCK_REALTIME)`, `gettimeofday` and
  `time`: wall time = fixture `now` + virtual time. `Date.now()` deltas (network rates),
  `new Date()` (calendar week) and `SystemClock` are therefore fixed. Monotonic clocks
  stay real, so Qt's event loop is unaffected. The time zone is the system's (`TZ` is
  passed only if set).
- Asynchronous image loads are awaited (in real time) before every capture. Each capture
  renders until two consecutive frames are identical, because layers and `MultiEffect`
  pick up new textures one sync later.
- Two full runs give identical bytes for every image without distance-field text; see the
  tolerance above for the rest.

## Layout

```
capture.sh                   one command; builds host, starts private weston, runs every fixture
host/                        parity-host (C++/Qt): render control, virtual clock, input, crop, manifest fragments
  wayland_mocks.h            C++ mock of Quickshell.Wayland (attached WlrLayershell, enums,
                             ToplevelManager), layer inspector, image://icon provider
scenes/island.qml            hosts DynamicIsland.qml like shell.qml:3879-3958 (+ calendar host morph)
scenes/desktop.qml           loads shell.qml (or fixture "entry") and composites every mock
                             PanelWindow of the first screen by layer, like a compositor
scenes/ShellRootMock.qml     theme tokens for the island scene (copied from shell.qml:212-233)
qml-mocks/                   mock Quickshell modules (Quickshell, .Io, .Hyprland, .Wayland (qmldir only),
                             .Widgets, .Bluetooth, .Networking, .Services.{Mpris,Pipewire,UPower,SystemTray})
qml-mocks/ParityHost/        ParityState singleton: fixture services, processes, files, sockets, registries
fixtures/<component>/*.json  inputs (generated by tools/gen-*-fixtures.py; the JSON is the source of truth)
fixtures/assets/             deterministic album-art images (tools/make-assets.py)
reference/<component>/       outputs + manifest.json
tools/pngstat.py             minimal PNG reader for self-tests
```

The mocks implement only what the QML reads. A `PanelWindow` is a `QtObject` whose
children are parented to `contentItem` (as in Quickshell). Geometry is computed from
`anchors`/`margins`/`implicit*` and the screen; `mask` regions are recorded but input is
not masked. Screens are QObjects (`calendarScreen === modelData` relies on identity).

## Fixture format

```jsonc
{
  "component": "bar",                 // output directory
  "scene": "desktop.qml",             // or "island.qml"; absolute paths allowed
  "entry": "shell.qml",               // desktop scene: file to load from the source dir
  "now": "2026-10-23T09:41:00",       // local wall-clock time (virtual clock origin)
  "canvas": {"width": 640, "height": 58},   // island scene; desktop default = screen size
  "crop": [0, 0, 3072, 70],           // logical [x, y, w, h] saved from the canvas
  "settle_ms": 2500,                  // virtual time before the first step (default 1500)
  "tick_ms": 50,                      // optional coarser frames (long histories)
  "fonts": ["JetBrainsMono Nerd Font"],     // must resolve exactly, else exit 3
  "props": {...},                     // scene.applyProps() before settling
  "services": {...},                  // mock service data (below)
  "processes": [...],                 // canned command outputs (below)
  "files": {"suffix": "content"},     // FileView contents by path suffix
  "sockets": {"substr": {"methods": {"method": result}}},  // JSON-RPC Socket peers
  "env": {"NAME": "value"},           // Quickshell.env()
  "steps": [ ... ]
}
```

**Island scene `props`/`set` keys:** any `DynamicIsland` property (`mediaPlaying`,
`agentSessions`, …), plus `now: "HH:MM"`, `calendar: bool` and `"@asset:file.png"` URLs.

**Desktop scene `props`/`set` keys:** `services` (merged into the mock services),
`ipc: [target, fn, args…]` (calls the shell's own `IpcHandler`), `root: {prop: value}`.

**`services`:**

| key | shape |
|---|---|
| `screens` | `[{name, width, height, x, y, scale}]` |
| `hyprland` | `{workspaces: [{id, name, urgent}], focusedWorkspace, activeToplevel: {title}\|null, fullscreen}` |
| `audio` | `{available, volume, muted}` |
| `mpris` | `{players: [...]}` (only the count matters) |
| `tray` | `[{id, title, icon, tooltipTitle, tooltipDescription, hasMenu, onlyMenu, menu}]` (theme icon name, path or `@asset:`) |
| `bluetooth` | `null` or `{enabled, discovering, pairable, devices: [{name, address, connected, paired, trusted, blocked, icon, battery}]}` |
| `upower`, `powerProfile`, `wifi` | see `qml-mocks/` |

**`processes`:** the first entry whose `match` is a substring of the command line answers
it. It carries `stdout` (whole), `lines: [{at, data}]` (streamed at virtual ms), `exitCode`,
`delay_ms`, and `stream: true` (keeps running). `@HOME@`/`@ASSETS@` are substituted.
Unmatched commands exit 1 with no output and are logged.

**Steps** (keys of one step object run in this order):
`set`, `invoke {type, method, args}` (call a function on the first instance of a QML
component, for states the UI cannot reach), `hover`, `press`, `click`, `rightclick`,
`drag` (12 moves, one frame apart, button held), `wheel {…, dy}`, `release` (`true` =
here), `advance` (ms), `dump`, `capture` (name), and
`frames {name, at: [ms…]}` (captures at offsets from now).

**Pointer targets:** `[x, y]` (canvas), `{"item": objectName, x, y}`, `{"text": "exact"}`,
`{"contains": "substr"}` or `{"image": "source substring"}`. A target picks the
`index`-th visible match in top-to-bottom, left-to-right order, offset by `x`/`y` from
its centre. Pointer events are delivered at integer logical positions.

Regenerate fixtures after editing a generator:
`python3 -I tools/gen-island-fixtures.py fixtures/island` (also `gen-bar-…`, `gen-panels-…`).

## Coverage

| component | fixtures | spec list |
|---|---|---|
| island | 35 (incl. frame sequences of every morph) | `ui-island.md` §10, all 17 items |
| bar | 46 | `ui-bar.md` §14 items 1–21 (strip, slide frames, calendar/island host, corners) |
| panels | 40 | `ui-panels.md` §5 items 1–21 |

Not covered yet:
- `ui-bar.md` §14 items 22–24: tray menu, tray overflow grid and battery popup. These need
  QsMenu entry fixtures and a battery machine.
- Panel open/close animation of the Wi-Fi popup.
- Multi-monitor layouts.

## Findings for spec authors (from the references)

- **Swipe-to-hide music is dead under Qt 6.12** (`DynamicIsland.qml:1524-1540`).
  `DragHandler` resets `translation` before `activeChanged(false)`, so `dx` is always 0.
  This was confirmed with a handler trace: translation reaches 70 px during the drag and
  reads 0 at release. The fixture `island/music-swipe-hide` shows no change.
- **Idle island width is 120 px** (120.157), not 113.2/116 (`ui-island.md` §9
  "idle-width"). Qt sizes text by fractional, lightly hinted advances ("09:41" = 47.969
  px at 16 px Bold), and Nerd-Font icon Texts report their *ink* width, not their advance
  (lock U+F023 = 12.594, power U+F011 = 13.594). Measured text heights: 16 px → 22,
  18 → 24, 13 → 18, 12 → 16, 11 → 15, 10 → 14, 9 → 12, 17 → 23, 21 → 28, 32 → 43.
- **The idle lock/power hover fill never shows** (`ui-island.md` §4.2): the z-99 side
  hotspot takes the hover. The references with and without hover are identical.
- **A zero-progress agent ring draws no dot** (`ui-island.md` §3.5).
- **StartupConfirm's dark variant is unreachable today.** qt6ct fails to load (above), so
  `SystemPalette` is always Qt's light default.
- The Mihomo control-panel page is only reachable through `invoke`, matching
  `ui-panels-control.md`.
