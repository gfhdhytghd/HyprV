# rshell: the HyprV shell in Rust

`rshell` replaces the Quickshell (QML + shell/Python scripts) desktop shell in `../quickshell` with one Rust program, `hyprv-shell`. It covers:
- the bar;
- the dynamic island;
- the panels and popups: control panel, Wi-Fi, quick-adjust OSD, system stats, week calendar, startup confirmation;
- all of their system services.

A small root helper, `hyprv-privhelper`, takes over fan control from the legacy scripts.

The goals, in order (see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)):
1. **Feature parity** with the QML shell, including every IPC entry point the Hyprland binds use (`bar`, `controlPanel`, `calendar`, `quickAdjust`).
2. **Pixel parity**: the same geometry, colours, fonts, radii, shadows, animation durations and easing curves. The specs in [docs/spec/](docs/spec/) are derived from the QML and are the source of truth.
3. **Performance**: purely event-driven, with zero wakeups when idle and no helper processes forked at runtime. It redraws only on a state change or while an animation runs.

> **Status (2026-10-23):** the cutover is prepared but **not applied**, and the readiness gate is **NOT READY**. The staged candidate passes parity; its performance uses the explicit user acceptance dated 2026-10-23. Privhelper installation remains blocked; see [Verification](#verification). The cutover scripts refuse to switch until `cutover/preflight.sh --installed` is green.

## Architecture

One binary with two halves:
- **UI thread.** A calloop event loop with smithay-client-toolkit (wlr-layer-shell, fractional-scale-v1, viewporter). It owns every surface and renders a retained scene graph with skia-safe on EGL/GLES. It tracks damage and repaints only dirty regions.
- **Service runtime.** tokio actors for D-Bus (zbus), udev, PipeWire, the Hyprland socket2, websockets and so on. They publish typed state diffs over a channel; the UI thread applies them and marks damage.
- **IPC.** A Unix socket at `$XDG_RUNTIME_DIR/hyprv/shell.sock`, driven by `hyprv-shell ipc call <target> <function> [args..]` with the same semantics as `quickshell ipc call`. `hyprv-shell kill` stops the running instance. Besides the Quickshell targets (`bar`, `controlPanel`, `calendar`, `quickAdjust`), the target `desktop` offers `setVariant <v1|v2|v3|bt>` (UI variant) and `deskflowReflow` (Deskflow topology). Both answer only once the service has finished, and report its error.
- **Script entry point.** The patched hypr scripts call `hyprv-shell desktop setVariant|deskflowReflow`. That uses the running shell over IPC and, if no shell is reachable, runs the same action in-process, as the legacy scripts did. Rotation and variant switching therefore never depend on the shell process.
- **Startup dialog.** `hyprv-shell startup-confirm` is a one-shot login dialog (the counterpart of `qs --path StartupConfirm.qml`). It writes `launch` to `$HYPRV_STARTUP_APPROVAL` on Launch and exits 0 once decided. It runs beside the main shell.
- **Privileged helper.** `hyprv-privhelper` is a system D-Bus service, activated by systemd, with every action checked by polkit (modelled on KDE KAuth). It writes the fan PWM controls. An independent `hyprv-cooling-failsafe` sets maximum cooling around every helper start and stop.

| Crate | Role |
|---|---|
| `hyprv-ui` | Wayland layer-shell engine: Skia GL rendering, retained scene, damage tracking, input, and Qt-exact animation and easing (QEasingCurve, Behavior, SpringAnimation). Can render offscreen to PNG for parity. |
| `hyprv-widgets` | Shared Qt-equivalent widgets and containers (reveal, flickable, …). |
| `hyprv-core` | State store and typed topics, the service lifecycle with subscriber-counted sampling, the calloop↔tokio bridge, the IPC server and CLI, config and theme loading. |
| `hyprv-svc-net` | NetworkManager (including a secret agent) and BlueZ (including a pairing agent), with a keyring for Wi-Fi secrets. |
| `hyprv-svc-media` | PipeWire audio, MPRIS, notifications, the audio spectrum. |
| `hyprv-svc-power` | UPower, power profiles, backlight/DDC, fan control (through the helper), idle inhibitors, system statistics. |
| `hyprv-svc-sys` | systemd units, the Windows VM, Mihomo, the calendar. |
| `hyprv-svc-desktop` | Hyprland state, agent island hooks, Deskflow/Viewflow, IR, eGPU, demo mode, recording, theme, applications, startup. |
| `hyprv-ui-bar`, `hyprv-ui-island`, `hyprv-ui-panels` | The ported UI components, each with fixture replays for parity. |
| `hyprv-shell` | The binary: assembly, host, IPC, nested tests. |
| `hyprv-privhelper` | The root helper and the cooling failsafe. |
| `hyprv-svc-probe` | A read-only service probe for equivalence and idle-CPU measurement. |
| `hyprv-ui-bench` | Offscreen CPU and GPU frame benchmarks. |

**Runtime subprocess policy.** No subprocess runs on a timer, for status reads, or as a monitor. A one-shot exec is allowed only for a **user-initiated action** that has no native API, and only where the legacy shell did the same (for example `plasma-apply-colorscheme`, `awww`, `xsettingsd`). Each such exec is documented in its crate's README.

**Hyprland integration.** The native bar keeps the layer namespace `hyprv-quickshell`, and popups use `shell:hyprv-*`, so the existing layer rules in `../hypr/hyprland-winrule.lua` (blur and so on) still apply. Cutover changes six files, three Lua configs and three helper scripts; see [cutover/hypr.patch](cutover/hypr.patch).

## Build and test

```sh
cargo build --release -p hyprv-shell -p hyprv-privhelper --bins   # development build
packaging/build-release.sh            # production release: fat LTO, plus a SHA-256 receipt and a staged release dir
packaging/build-release.sh --dry-run  # print the build plan only

cargo test --locked --workspace --all-targets
cargo test --locked --workspace --doc
cargo test --locked --manifest-path xtask/Cargo.toml
python3 -B -m unittest discover -s cutover/tests -p 'test*.py'                    # cutover and installers (50 tests)
python3 -m unittest discover -s crates/hyprv-svc-probe/scripts -p 'test*.py'
cargo clippy --workspace --all-targets

cutover/preflight.sh --run-tests      # runs the full configured suite above and records a receipt
```

`build-release.sh` writes `packaging/dist/`:
- the three binaries and `build.json` (source and binary digests);
- `release/`: everything the installers need, hash-listed in `MANIFEST.sha256`. **The SHA-256 of `MANIFEST.sha256` is the release id**, and the build prints it.

The build refuses to write a receipt if any source changed while it ran.

Useful `hyprv-shell` flags:
- `--read-only`: observe only, never act;
- `--no-exclusive`: don't reserve screen space;
- `--headless`;
- `--exit-after-ms N`;
- `--nested-test [DIR] [--fixture FILE|--fixtures DIR]`: a self-contained nested end-to-end run.

Integration CI status is kept by svc-sys in [docs/verify/ci.md](docs/verify/ci.md).

Output rotation regression (nested compositor, all 8 transforms × scales 1/1.5/2, popups open), run in an isolated sandbox: `crates/hyprv-ui/tests/run-nested-rotation.sh [report]`. Results are in [docs/verify/e2e-rotation.md](docs/verify/e2e-rotation.md), which is also part of [docs/verify/e2e.md](docs/verify/e2e.md).

## Verification

### Pixel parity

The references are rendered from the **original QML**, never from the live desktop, by Qt's own RHI/OpenGL scene graph at the real output scale with a virtual clock. See [parity/README.md](parity/README.md).

```sh
parity/capture.sh                          # all references (904 images; needs weston, Qt 6.12 dev files, fonts)
parity/capture.sh panels cp-main-idle      # a single fixture
python3 parity/verify/verify.py --backend gl   # the gated production EGL/GL path: render, diff, write docs/verify/parity.md
python3 parity/verify/verify.py --backend gl --component bar
hyprv-shell --nested-test OUT --fixtures parity/fullscreen/fixtures --references parity/fullscreen/reference
python3 -I parity/fullscreen/tools/compare.py OUT                  # assembled full-screen gate (64 captures)
python3 crates/hyprv-shell/tools/verify_corners.py OUT             # strict non-text check of the four screen corners
python3 parity/verify/zoom.py <component> <file> x y w h out.png   # reference | ours | heatmap close-up
cargo run --manifest-path xtask/Cargo.toml -- parity <reference-dir> <actual-dir> --tolerance 2   # generic PNG directory diff
```

The gate works on premultiplied RGBA:
- **Exact match** for images without distance-field text (those references are bit-identical from run to run).
- At most 8/255 max and 0.5/255 mean over painted pixels for images with distance-field text.
- No structural difference in any image.

Current results are in [docs/verify/parity.md](docs/verify/parity.md). Fresh-snapshot runs by both final reviewers on 2026-10-23 ([final-review-svc-desktop.md](docs/verify/final-review-svc-desktop.md), [final-review-svc-net.md](docs/verify/final-review-svc-net.md)), production GL path:
- **island:** 380/380 pass.
- **panels:** 300/300 pass.
- **bar:** 292 pass, 0 fail. 12 references are **excluded**: battery-popup states that live Quickshell can never reach (`QUIRK_BATTERY_POPUP_NEVER_OPENS`). 8 are **missing**: `calendar-host` and `island-expanded-host`, which the bar crate delegates to the island crate's host composition (the bar's own exporter reports 304/304 exact). Both count against the strict gate, so **bar is red**.
- **Full screen** (assembled shell, 6144×3456@2, both themes): 64/64 pass the gate.
- **Strict corner exactness is not met.** `verify_corners.py` finds all 256 assembled screen corners differing by at most 4/255. That is invisible, but non-text images must be exact.
- **Gate wiring:** preflight's parity gate still reads the CPU-slot results and does not apply the exclusion and delegation policy, so a green GL run cannot yet produce READY (FR-NET-2 in the svc-net review).

### Performance

```sh
python3 crates/hyprv-svc-probe/scripts/build.py build --release
python3 crates/hyprv-svc-probe/scripts/verify.py idle            # services idle CPU vs. live Quickshell (same window)
python3 crates/hyprv-svc-probe/scripts/verify.py equivalence     # field-by-field state comparison with the legacy scripts
python3 docs/verify/measure-shell.py --seconds 60                # the real release shell in a private virtual KWin + Hyprland
./target/release/ui-bench 2 --json out.json                      # per-frame CPU/GPU render cost (hyprv-ui-bench)
```

Two budgets, two scopes:
- **Idle aspiration:** the **whole real shell process** (UI, renderer and all services) at idle, at most 0.1% of one core. For deployment, the user explicitly accepts the exact candidate in [cutover/perf-acceptance.json](cutover/perf-acceptance.json): controlled quiet ≤0.5%, live-hook idle ≤2.0%, with hook events reported. Preflight requires matching measured/staged/installed binary hashes, current runtime fingerprints and measurements no older than 24 hours; changed binaries, missing evidence or exceeded bounds fail. Measured with `docs/verify/measure-shell.py` (release build, private nested KWin + Hyprland at 6144×3456@2, 60 s windows).
- **Service-runtime probe** (`hyprv-svc-probe … idle`): services only, without UI. It is an older, narrower number and does not substitute for the gate.

Earlier whole-process measurements (2026-10-23, release ELF `d15f44f9…`, raw data in `docs/verify/raw/perf-shell/b2-*`):

| Case | CPU |
|---|---:|
| Controlled quiet (private PID namespace, no agent input): the gate case | **0.100%** |
| Frozen agent hooks, real host `/proc` (agent discovery active, 19 agents, ~1,700 processes) | **0.265%** (repeat 0.266%) |

Live-hook idle and UI activity cost more; see [docs/verify/perf-shell.md](docs/verify/perf-shell.md).

A side-by-side Quickshell figure is deliberately not quoted here. The live legacy observer (`desktop-state.py`) appears to have stopped its agent scanning since the 2026-10-23 disk-full incident, and now measures about 0.2% instead of the earlier 2.5–2.9%. Comparisons against it are not like for like until it is restarted.

See also [docs/verify/perf.md](docs/verify/perf.md), [docs/verify/ui-perf.md](docs/verify/ui-perf.md) and [docs/verify/equivalence.md](docs/verify/equivalence.md).

## QUIRK flags

Every reproduced QML quirk is a named compile-time constant. The defaults reproduce what the **live** Quickshell actually does, including its bugs, because the user-facing decision was that parity with live behaviour wins. To change one:
1. Edit the constant in the listed file: `true` → `false`, or a new value for the non-boolean one.
2. Rebuild.
3. Re-run the crate's parity fixtures. References that capture the quirk will then differ, by design.

| Flag | File | Reproduces (default) | Flipped |
|---|---|---|---|
| `QUIRK_BAR_TOGGLE_IS_NOOP` | `hyprv-ui-bar/src/model.rs` | IPC `bar toggle` does nothing; the bar stays expanded (`shell.qml:41-43`). | The toggle hides and reveals the bar. |
| `QUIRK_INVERT_VOLUME_WHEEL` | `hyprv-ui-bar/src/model.rs` | Wheel up over the volume module *lowers* the volume. | Wheel up raises it. |
| `QUIRK_HORIZONTAL_TRAY_WHEEL_SENDS_MINUS_ONE` | `hyprv-ui-bar/src/model.rs` | A horizontal-only wheel event on a tray item sends a vertical scroll of −1. | It sends 0. |
| `QUIRK_TRAY_EXCLUDED_FROM_SUPPLEMENTARY_THRESHOLD` | `hyprv-ui-bar/src/model.rs` | The tray width is not counted when deciding whether temperature and volume fit, so they can show on outputs that are really too narrow. | Tray width is counted. |
| `QUIRK_GLASS_ANCHORED_CONTENT_IGNORES_SLIDE` | `hyprv-ui-bar/src/model.rs` | Bar popups' content slide offset has no effect (anchors win over `y`). | Content slides during open and close. |
| `QUIRK_WINDOW_TITLE_GLOBAL_ACROSS_OUTPUTS` | `hyprv-ui-bar/src/model.rs` | Every output's bar shows the globally active window's title. | Each bar shows only windows on its own output. |
| `QUIRK_UNAVAILABLE_POWER_RETAINS_SEVERITY` | `hyprv-ui-bar/src/model.rs` | When CPU package power becomes unavailable ("-- W"), its colour keeps the last value's severity. | The colour resets. |
| `QUIRK_BATTERY_POPUP_NEVER_OPENS` | `hyprv-ui-bar/src/model.rs` | The battery popup never opens: details are polled only while it is open, but opening needs details (`shell.qml:3340-3349`). | Uses UPower details, so the popup works. |
| `QUIRK_SWIPE_CANNOT_RESTORE` | `hyprv-ui-island/src/island.rs` | After a right swipe hides music, a left swipe cannot restore it until music content goes away. | A left swipe restores it. |
| `QUIRK_SWIPE_TRANSLATION_RESET` | `hyprv-ui-island/src/island.rs` | Swipe-to-hide never fires: Qt 6.12 resets the drag translation before release is handled. | The intended swipe gesture works. |
| `QUIRK_HOTSPOTS_BLOCK_BUTTON_HOVER` | `hyprv-ui-island/src/island.rs` | The idle lock and power buttons never show a hover fill, because the side hotspots take hover. | Hover reaches the buttons. |
| `QUIRK_SCROLL_INDICATOR_IN_CONTENT` | `hyprv-ui-panels/src/calendar.rs` | The calendar's scroll indicator scrolls with the content and is visible only at the far right. | The indicator stays fixed. |
| `QUIRK_WEEK_CHANGE_RESETS_VERTICAL` | `hyprv-ui-panels/src/calendar.rs` | Paging a week also jumps the vertical scroll back to "now − 2 h". | The vertical position is kept. |
| `QUIRK_PROFILE_TOGGLE_NEVER_ACTIVE` | `hyprv-ui-panels/src/control_panel.rs` | The power-profile toggle is never drawn active. | Drawn active by profile. |
| `QUIRK_VIEWFLOW_NEVER_ACTIVE` | `hyprv-ui-panels/src/control_panel.rs` | The Viewflow toggle is never drawn active. | Drawn active by target. |
| `QUIRK_SLIDER_LABELS_HIDDEN` | `hyprv-ui-panels/src/control_panel.rs` | The fan slider labels ("Chassis 1 Limit · N RPM") are computed but not drawn. | Labels are drawn. |
| `QUIRK_GLASS_CONTENT_SLIDE_IS_NOOP` | `hyprv-ui-panels/src/popup.rs` | Panel content does not slide during open and close. | Content slides. |
| `QUIRK_PAGE_SWITCH_TIMER_RESTARTS_NEXT_FRAME` | `hyprv-ui-panels/src/popup.rs` | After a control-panel page switch, the reopen starts one frame late. | Starts immediately. |
| `QUIRK_CLIP_IS_DEVICE_SCISSOR` | `hyprv-ui-panels/src/qrect.rs` | `clip: true` is a hard scissor rounded to device pixels, as in Qt (no anti-aliased edge). | Skia anti-aliased clip. |
| `QUIRK_KERNING` | `hyprv-ui-panels/src/qtext.rs` | Qt keeps HarfBuzz GPOS kerning on top of design advances. | No kerning. |
| `QUIRK_FONTCONFIG_HINTSLIGHT_AT_DPR1` | `hyprv-ui-panels/src/qtext.rs` | On scale-1 outputs, glyphs are light-autohinted per the user's fontconfig. | Unhinted. |
| `QUIRK_NO_BRIGHTNESS_FINISH` | `hyprv-ui-panels/src/quick_adjust.rs` | Dragging in the quick-adjust OSD never sends `brightness_finish` (signal not connected in QML). | Release sends `BrightnessFinish`. |
| `QUIRK_STARTUP_CONFIRM_ALWAYS_LIGHT` | `hyprv-ui-panels/src/startup.rs` | The startup confirmation dialog is always light (it follows the Qt system palette, not the shell theme). | Follows the shell theme. |
| `QUIRK_MISSING_CORE_SHOWS_ZERO` | `hyprv-ui-panels/src/stats.rs` | A missing CPU core shows "0%", never "--". | Shows "--". |
| `QUIRK_X_ALWAYS_10` (`f32`) | `hyprv-ui-panels/src/stats.rs` | The stats card x position is always 10 (the QML clamp collapses). | Another value moves the card. |

## Known differences from Quickshell

**Intentional:**

- **Corner overlay topology.** The historical four-tiny-surface optimization was
  superseded by the exact Qt-window implementation. The host uses one full-output
  `hyprv-screen-corner-overlay-<output>` Overlay layer per output, exclusive zone−1,
  no keyboard focus and an empty input region. Namespace/rule inventories retain
  legacy topology. Full-output viewport coordinates and the R8 Canvas atlas preserve
  sampling; four-surface native equality did not prove Qt equality. The engine now
  retires the idle EGL drawable after submission while retaining the role and the
  compositor's last attached buffer; no null attach or extra commit occurs. See
  [resource plan](docs/spec/laptop-minimal-resources.md) and
  [assembled corner verification](docs/verify/assembled-corners.md).

- **Fan control.** A root helper (`hyprv-privhelper`, polkit-checked) owns the fan PWM controls instead of user scripts with user-writable sysfs grants. The `turbostat` CPU-power sampler (`hyprv-cpu-power.service`) is replaced by native statistics.
- **No polling.** State comes from D-Bus, udev, PipeWire, netlink and Hyprland events rather than scripts on timers.
- **Wi-Fi.** New Wi-Fi profiles store their secrets system-owned, like `nmcli`, matching the legacy behaviour rather than plasma-nm's per-user agent storage. Secret buffers are zeroized.
- **No system bus.** Like Quickshell, the shell keeps running without one. Services that need it report unavailable, and the bar, panels, IPC and session-bus services keep working. The shell retries with backoff (0.5 s doubling to 30 s), and also notices if a working bus goes away (dbus-broker restart). When the bus is reachable again it re-executes itself (same PID, about a second of UI restart), because the bus connections cannot be re-targeted in place. Quickshell instead never reconnects.

**Open:**
- **Pixel parity gate is red:** bar (8 delegated host images, 12 unreachable exclusions), strict corner exactness (≤ 4/255 on all 256 corners), and preflight's parity-gate wiring (FR-NET-2). See the parity numbers above.
- **Idle CPU depends on agent traffic:** the cooling candidate measures 0.098131% controlled quiet and 0.945095% live-hook idle (114 hook-file notifications). Deployment uses the binary-specific user acceptance above; ≤0.1% remains the aspiration. See [fresh gate evidence](docs/verify/cooling-gate-refresh.md).
- **Service actions are verified only up to a mock.** Fan, backlight and audio writes, tray-menu activation, Wi-Fi joins, VM/Mihomo/IR/eGPU and theme apply have not run against their real side effects. The privileged helper has never run as root on this machine.
- **Readiness receipts are stale** until the tree is frozen and `packaging/build-release.sh`, `cutover/preflight.sh --run-tests` and the parity, performance and equivalence reports are regenerated.
- **No A/B mode.** Running native and legacy side by side on different outputs is unsupported: `switch.sh --ab-output` exits 4 and does nothing.

## Cutover (Quickshell → native) and rollback

Everything is a **printed plan** until you add `--apply`. Nothing calls sudo for you, and the root steps never run files from this user-writable checkout. A Chinese step-by-step guide is in [CUTOVER.zh-CN.md](CUTOVER.zh-CN.md); [cutover/README.md](cutover/README.md) has the full detail. The whole user-level flow, including automatic fallback and byte-identical rollback, was rehearsed for real in an isolated nested Hyprland: [docs/verify/cutover-rehearsal.md](docs/verify/cutover-rehearsal.md).

### 0. Readiness

```sh
packaging/build-release.sh          # note the printed release id
cutover/preflight.sh --run-tests    # every gate PASS except "privhelper installed"
```

Before the root install, `privhelper installed` is necessarily BLOCKED, so this run cannot say READY yet. Every other gate (release binaries, automated tests, parity, performance) must PASS. After the install, `cutover/preflight.sh --installed` must end with **READY**. Step 2a runs that same check itself and refuses to switch otherwise.

### 1. Install

**User part** (no root): `packaging/install-user.sh --apply` installs:
- `~/.local/bin/hyprv-shell`
- `~/.config/systemd/user/hyprv-shell.service`

The unit is static and only starts once the cutover marker `~/.local/state/hyprv-cutover/native.enabled` exists.

**Root part.** Run `packaging/install-root.sh` *as your user*. It prints these commands with the id filled in; check them, then run them:

```sh
sudo /usr/bin/install -d -o root -g root -m 0755 /usr/local/lib/hyprv-cutover/releases
sudo /usr/bin/cp -R -P -T -- packaging/dist/release /usr/local/lib/hyprv-cutover/releases/<id16>
/usr/bin/sha256sum /usr/local/lib/hyprv-cutover/releases/<id16>/MANIFEST.sha256      # must equal the release id
/usr/bin/sh -c 'cd /usr/local/lib/hyprv-cutover/releases/<id16> && /usr/bin/sha256sum --strict --quiet -c MANIFEST.sha256'
sudo /usr/bin/python3 -I -B /usr/local/lib/hyprv-cutover/releases/<id16>/packaging/install.py root --release <release-id> --apply
```

The root install changes:
- `/usr/libexec/hyprv-privhelper` and `/usr/libexec/hyprv-cooling-failsafe` (0755);
- `/usr/lib/systemd/system/hyprv-privhelper.service`, `/usr/share/dbus-1/system-services/org.hyprv.Shell.PrivHelper.service`, `/etc/dbus-1/system.d/org.hyprv.Shell.PrivHelper.conf` and `/usr/share/polkit-1/actions/org.hyprv.shell.fan-control.policy` (0644);
- before cutover only: `systemctl mask hyprv-privhelper.service`;
- `/var/lib/hyprv-cutover/`: the install journal and backups (first-ever originals are kept);
- `/usr/local/lib/hyprv-cutover/installed.json` and `current → releases/<id16>`;
- `systemctl daemon-reload`.

It enables or starts nothing and leaves cooling alone. The install is all-or-nothing: an interruption or error restores the previous files. Re-running it is idempotent. After cutover it never masks the helper again; an upgrade restarts the helper and rolls back if the helper fails.

Then run `cutover/preflight.sh --installed`: it must be green, and it checks that the installed release equals your build.

### 2. Switch

Run these from a terminal **inside the graphical session**. The bar disappears at step 2a and returns at step 2c.

```sh
cutover/switch.sh --apply --quiesce-legacy                       # 2a: journal unit states, stop legacy user fan units, quit Quickshell
sudo /usr/bin/python3 -I -B /usr/local/lib/hyprv-cutover/current/cutover/control.py switch --scope system --apply   # 2b
cutover/switch.sh --apply                                        # 2c
```

- **2b (root)** stops the legacy system units (`hyprv-fan-control`, `hyprv-fan-permissions`, `hyprv-cpu-power`). It runs the maximum-cooling failsafe (the fans briefly run at full speed), revokes the user's PWM grants, and starts `hyprv-privhelper`.
- **2c** applies the hypr patch (three Lua files and three helper scripts, which now call `hyprv-shell desktop …` and `hyprv-shell startup-confirm` instead of legacy Quickshell scripts), reloads Hyprland, starts `hyprv-shell.service`, and waits up to 20 s for it to be ready. Ready means IPC answers *and* the native process owns a `hyprv-quickshell` bar layer on every enabled monitor.
- **If 2c fails,** it falls back automatically: it stops native, reverses the patch, reloads Hyprland and relaunches Quickshell, and exits 1 with `phase=fallback`. Cooling stays with the helper. Fix the problem and rerun 2c, or roll back.

### 3. Verify

```sh
~/.local/bin/hyprv-shell ipc call bar dataStatus                 # JSON status = IPC alive
systemctl --user status hyprv-shell.service
systemctl status hyprv-privhelper.service
# Every monitor must have a bar layer owned by the native unit's process (legacy Quickshell uses the same namespace):
python3 -c 'import json,subprocess as s;pid=int(s.check_output(["systemctl","--user","show","-p","MainPID","--value","hyprv-shell.service"]));L=json.loads(s.check_output(["hyprctl","-j","layers"]));print({m:any(l["namespace"]=="hyprv-quickshell" and l["pid"]==pid for lv in v["levels"].values() for l in lv) for m,v in L.items()})'   # all True
python3 -c 'import json;print(json.load(open("'$HOME'/.local/state/hyprv-cutover/state.json"))["phase"])'   # native
```

You can also press SUPER+A (native control panel) or a brightness key (native OSD).

### 4. Rollback

Rollback works even while the gates are red.

```sh
cutover/rollback.sh --apply --quiesce-native                     # 4a: stop native, disarm autostart
sudo /usr/bin/python3 -I -B /usr/local/lib/hyprv-cutover/current/cutover/control.py rollback --scope system --apply   # 4b
cutover/rollback.sh --apply                                      # 4c
```

- **4b (root)** stops and masks the helper, runs the failsafe (fans briefly at full speed), and confirms the helper is gone. It then restores the original PWM ownership and the legacy units' exact symlinks and run state.
- **4c** reverses the Lua patch. If you have edited those files since, it reverts the patched lines by content, keeps your edits, and prints any line you must revert by hand. It then reloads Hyprland, restores the user units and launches Quickshell.

Only **exit 0 with `phase=legacy`** means done. The other phases:
- `rollback-incomplete` (exit 2): rerun the same command until it exits 0.
- `ui-restored`: the root step was skipped; run 4b, then 4c again.
- `fallback-blocked`: native could not be stopped. Stop it, then rerun.

Rollback leaves the installed files in place for a later cutover. Root install, root switch and root rollback share one lock, so they never run concurrently.

## Repository map

| Path | Content |
|---|---|
| `crates/` | All Rust crates. |
| `docs/ARCHITECTURE.md` | Goals, process model, rules, parity gate. |
| `docs/spec/` | Specifications derived from the QML. |
| `docs/verify/` | Parity, performance, equivalence, CI and rehearsal reports. |
| `docs/reviews/`, `docs/requests/` | Review reports and per-owner requests. |
| `parity/` | Reference capture from the original QML, the verifier and fixtures. |
| `packaging/` | Release build and installers. |
| `cutover/` | Switch, rollback, preflight, the Lua patch, tests. |
| `xtask/` | `cargo xtask parity` (PNG directory diff). |
