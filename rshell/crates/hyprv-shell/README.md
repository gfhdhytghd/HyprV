# Shell host and production assembly

Normal `hyprv-shell` runs `assembly::Assembly` on the UI thread with EGL/GL
RGBA8 surfaces and one `hyprv-core::ServiceRuntime` / StateStore. `services::Hub`
spawns all current svc-media/net/power/sys/desktop modules, including the shared
inhibitor and exactly one `clock::run_sleep_topic`. `mapping` converts typed topics
to the published bar, panels and island models; actions route back to typed service
Action enums. The IPC targets `bar`, `controlPanel`, `calendar` and `quickAdjust`
change those controllers directly. Brightness notification IPC is a local preview;
explicit slider actions alone write provider state.

Bar/corner controllers are per output. Wi-Fi and embedded calendars are per output;
control/stats/OSD and bar popups are global controllers migrated to their selected
output. Output removal drops controllers, animation/deadline owners and native
mounts. Closing surfaces live through their final frame. Sampling subscriptions
are held only while a consumer is visible, including the expanded music position
clock. Idle island/calendar trees are retained through immutable `Content::Shared`
slots; animated content rebuilds on presentation frames and declared deadlines.
Native pointer v9 scroll conversion preserves Qt pixel/angle deltas, source,
Begin/Update/End and inversion metadata. Bar menu and island Flickables receive the
full phased wheel; panel adapters receive both delta axes. Wayland has no separate
momentum phase, so the host does not invent one.

Popup GPU targets cover the card plus padding, with crop origins and extents aligned
to whole logical and device pixels. Outside dismissal uses a separate full-output
input region backed by a transparent **1×1** buffer (viewporter required; otherwise
it falls back to a conventional surface). Input coordinates are translated back to
output coordinates for the published component APIs. QuickAdjust passes outside
input through. Captures include origins, output, DPR and logical size for composition.

```sh
cargo run -p hyprv-shell -- --nested-test target/nested-test
cargo test -p hyprv-shell --test cli nested_test_captures_real_gpu_surfaces -- --ignored
```

The bounded harness starts its own virtual KWin parent and nested Hyprland, with
private runtime, HOME/config/cache/state, and a private D-Bus without service
activation directories. Provider writes are disabled. It never attaches surfaces to
the real desktop. Dependencies: `Hyprland`, `kwin_wayland`, `dbus-run-session`, and a C compiler (`cc`).
Six scenes (bar, calendar, control, Wi-Fi, stats, volume) dump real retained GPU
surface PNGs under a unique `run-PID/`; the root `manifest.json` points to that run.
The test checks nonempty card content, card-sized popup targets and 1×1 dismissal
targets. This is runtime/capture evidence, not full-screen QML parity acceptance.

## Full-screen fixture replay

```sh
cargo run -p hyprv-shell -- --nested-test target/fullscreen \
  --fixture crates/hyprv-shell/tests/fixtures/assembled-desktop.json
# Capture-only mode exports without claiming reference acceptance:
cargo run -p hyprv-shell -- --nested-test target/fullscreen \
  --fixture crates/hyprv-shell/tests/fixtures/assembled-desktop.json --capture-only
cargo test -p hyprv-shell --test cli nested_fixture_ -- --ignored
```

`--fixtures DIR` replays every scene fixture JSON (metadata JSON is skipped).
Default runs both themes. `--theme light|dark` selects one; `--references DIR`
selects the reference directory (default parity/fullscreen/reference).
Live output identity and DPR are discovered by a native read-only `j/monitors`
request, respecting HYPRLAND_INSTANCE_SIGNATURE when available. Fixture screens
specify logical size. `--scale DPR` is tagged as a synthetic diagnostic override;
unknown live output names otherwise fail. Each output is replayed independently;
native multi-output interactions are verified separately by the functional suite.

Fixtures use scene=desktop.qml, entry=shell.qml (or absent), original mock service
and process streams, props.root, and steps with set services/root/ipc, advance,
frames/capture, hover/click/press/release/rightclick/wheel/drag. Components and
layouts are the actual production Assembly controllers; inputs go through their
native adapters with retained hit targets and pointer grabs. Explicit
`targets: {"STEP_INDEX/click": [x,y]}` disambiguates baked labels. Unsupported
steps/targets fail. A virtual clock drives builds, controller frames and deadlines;
only this isolated mode disables native animation callbacks. Its timer emulation
keeps microsecond wall epochs and integer Qt animation epochs, including the
pause-only to continuous-driver restart. Stationary pointer reconciliation sends
leave/enter when mapped surfaces change focus, preserving same-surface hover.
Captures require every surface configured/rendered at the requested mode and DPR,
then use private Hyprland screencopy (`grim`) with stable black/white/black mattes,
matching the Qt references including compositor sampling. Raw per-surface EGL PNGs
remain diagnostic evidence; CPU SourceOver is not the assembled acceptance path.
The corner overlay uses one full-output passive surface with direct full redraw,
no retained offscreen target or blit, and a shared R8 Qt Canvas atlas. Direct-window
readback redraws before reading rather than trusting a recycled post-swap buffer.

`fullscreen-results.json` links each job's manifest, which links its unique
`capture_manifest`; PNG file names in captures resolve relative to that manifest.
Captures record actual GL renderer, RGBA8, logical/pixel sizes, output and virtual
milliseconds. Reference images/entries match fixture/frame/theme/scale/output,
require matching reference time within0.01ms and distance_field_text, and use ARCHITECTURE's
unchanged painted-mask, footprint and channel-error gates. Missing references
always keep pass=false; --capture-only bypasses only comparison exit failure.
Runtime errors always fail. Sequence references use each `frames` block's relative
time; ordinary captures use absolute virtual time. Receipts retain absolute
`virtual_ms` and the compared `reference_clock_ms`. Calendar wall time matches
legacy `SystemClock.Minutes`, with seconds and subseconds zero, in fixtures and
production mapping.

The installed Aquamarine0.15.1 omits an initial Wayland flush needed for private
KWin configure delivery. The harness builds `nested-flush.c` and preloads it only
in its private Hyprland child: wl_display_get_fd first flushes pending requests.
A private KWin sizing rule plus strict native monitor validation prevents false
6K captures backed by fallback640×360/1280×720 modes. Production and the live
compositor never load this test shim. XDG config/cache/data/state paths are private.

Verified both themes, ten frames each, DP-4 real DPR2,6144×3456 RGBA8 on NVIDIA
RTX PRO6000 EGL. Receipts: target/fullscreen-assembled2 (dark) and
target/fullscreen-final-light (light). Those receipts establish GPU capture, not parity acceptance. QA has since published
`parity/fullscreen/{fixtures,reference}` and `docs/verify/parity-fullscreen.md`.
Replay with `--fixtures parity/fullscreen/fixtures`; references default to the
published directory, or use `--references` explicitly. Existing QA failures remain
visible in that report.

`--headless` retains the compatibility Host as a synthetic IPC/model test fixture.
`--demo-host --no-exclusive --exit-after-ms 3000` is the separate compatibility
visual demo. Its historical attachment API is documented below; production
components are mounted by Assembly through their own published controller APIs.

## Compatibility Host component attachment

Implement `host::Component::build(&ViewContext) -> Node`, optionally
`input(&ViewContext, &InputEvent) -> Vec<HostAction>`, then call
`host.register(Slot, component)` before connecting. Slots: `Bar`, `Island`,
`Calendar`, `ControlPanel`, `Wifi`, `Stats`, `QuickAdjust`, `TrayMenu`,
`TrayOverflow`, `Battery`. Bounds are local logical pixels. ViewContext exposes
output name, bounds, `Palette`, minute, core typed `StateStore`, retained service
JSON data, OSD mode and optional brightness/volume. A component's build is called
on mount/theme/data revision changes, not every animation frame. Return a Node
with children and keep interactive node IDs in your component state per output.
Call `Host::invalidate()` after a changed service snapshot; store your typed
service topics in `Host::store`. ServiceRuntime's calloop bridge callback can
capture Rc Host on the UI thread, apply a diff, then `engine.request_redraw()`.

`HostAction`: Open { slot, output, anchor: Option<Rect> }, Close(slot),
CloseOn { slot, output }, Calendar { output }, CloseCalendar { force }, CalendarChild { opening, active },
Island { output, expanded, bounds, offset }, Fullscreen { output, value },
PopupSize { slot, width, height }, TraySubmenuDepth(usize), QuickValue(u8).
Use PopupSize after measuring real content (defaults only bootstrap empty slots).
Anchors are output-local trigger rectangles. The host owns full-output popup
layers, positioning, outside-click, Escape, map/unmap and animation lifetimes.
Calendar is embedded inside the bar's center morph; island/calendar components
must not create their own layer surfaces. Bar exclusive zone remains 58 through
fullscreen slides; the bar surface grows to 820 only while expanded content is
presented, with the specified calendar/island close holds. Four tiny input-empty
Overlay corner surfaces use the original namespace prefix.

Compatibility Host default slot content is empty except the fallback clock;
production Assembly mounts the actual component crates. Demo mode draws labelled cards so host geometry can be
inspected independently. These are not claims of final component pixel parity.

## Source contracts

- `shell.qml:3551-3660`: bar layer, size, exclusive zone and input union.
- `shell.qml:3467-3546`: screen corner overlay, four 27px inverse corners.
- `shell.qml:3604-3714`: calendar arm/leave/close and island size holds.
- `ControlPanelPopup.qml:739-920`: full-output focusable popup + glass lifecycle.
- `QuickAdjustPopup.qml:112-219`: click-through OSD outside its slider card.
- `BarScreenState.qml:26-37`: bar fullscreen motion uses the native Bezier Behavior.

## Per-output popup state and service integration

Wi-Fi state is per output (`WifiFallback.qml:11-12,245-249`); the other popup
slots remain singletons. `CloseOn` closes only the specified output instance;
`Close(slot)` closes all instances. Wi-Fi reopen reverses a partial close without
resetting its reveal. Tray overflow maps/unmaps immediately and draws the direct
18px glass rectangle (`shell.qml:1142-1308`). `PopupSize` updates the first live
instance of its slot; `PopupSizeOn { slot, output, width, height }` targets the
specific Wi-Fi instance when output content sizes differ (bootstrap: 384×400).

Theme changes use file notifications and the core topic bridge. The minute clock
subscribes only while outputs exist; resume/clock changes resync it. The fallback
clock formats local time. Animation frame requests and one nearest deadline drive
all host lifetimes; an unmeasured popup waits without a polling timer.
`dataStatus` provides all specified diagnostic keys; unattached service values
are null and availability flags remain false. Domain actors/actions and final
composite content are supplied by their owning agents through the published ports.

## Historical compatibility Host validation — 2026-10-23

- `cargo build --workspace`: passes with the shared root lock/target.
- `cargo test -p hyprv-shell --all-targets`: 7 model tests and the headless IPC
  single-instance/validation/shutdown test pass; one live test is opt-in.
- Focused strict Clippy passes for engine, host and owned widget modules.
- `WAYLAND_DISPLAY=wayland-1 cargo test -p hyprv-shell --test cli -- --ignored --nocapture`:
  bounded real-output smoke covers calendar/control-panel/OSD IPC, popup creation
  under fractional scaling, final close frames and automatic clean teardown.
  Every surface reserves zero space and the test has an independent IPC socket.
- Engine live smoke passes on one output at scale 2, including shared EGL across
  two surfaces and no extra frames after settling. No physical input is injected.

Physical input and distinct-output live testing remain outside these compatibility
checks. Production private native-input/hotplug results are in docs/verify/e2e.md;
full-screen comparison results are in docs/verify/parity-fullscreen.md.
Compatibility empty slots and demo labels are API scaffolding.

A separate 2.5-second bounded demo-host run recorded **0 process CPU ticks** over
1.2 seconds after settling (Linux tick resolution); it exited successfully with
empty stderr. This excludes a minute-clock boundary and does not claim infinitely
precise 0.000% CPU. The live regression exposed and now covers two startup/teardown
cases: preferred-scale feedback before first configure must not emit a zero-size
Resize, and a closing popup's last frame must tolerate its model already unmapping.

## Native functional E2E

```sh
cargo run -p hyprv-shell -- --nested-functional-test target/functional-test
# Explicit opt-in regression (same complete command; takes several minutes):
cargo test -p hyprv-shell --test cli nested_functional_matrix_ -- --ignored
```

This creates the private KWin/Hyprland/D-Bus environment above, then drives the
actual assembled shell through `zwlr_virtual_pointer_v1` and
`zwp_virtual_keyboard_v1` (a private US XKB map). No input is attached to the live
session. Native output-management hooks use only this compositor's guarded IPC
socket. All services are mocked: `Hub::mocked` starts **no provider actors** and
holds no native provider handles. The normal typed routing constructs each service
Action; the mock sink records it immediately before execution. Ordered exact
command arrays reject missing, extra, duplicate and wrong-payload actions.

Fixture setup and read-only state observations use a separate private
`functional.sock`; the public core IPC socket keeps its normal validation rules.
The test never calls `Engine::replay_input` or component pointer methods directly.
Target coordinates come from real retained bar geometry or spec layout positions;
panel coordinates are checked through native hover. Guarded setup mounts the
legacy-unreachable Mihomo page; all its tested controls still use native input.
The explicit test clock drives controller animations/deadlines and mock timestamps,
without requesting paints for read-only observations or idle fixtures.

`manifest.json` contains every action's expected/observed service commands and
native input receipts. A complete run writes `docs/verify/e2e.md`; diagnostic
`HYPRV_FUNCTIONAL_FILTER='wifi.'` (or comma-separated substrings) runs write a subset report in their own output
directory. Runs pin the executable inode before starting children, including
restart probes, so another agent's Cargo build cannot replace the test binary.
Dependencies additionally include the compositor virtual-input protocols and
libxkbcommon. The original fixture screenshot mode remains available separately.

Output cases exercise native hotplug, monitor positions/reorder, live scale
changes1→1.5→2 and input after those changes. Connection-loss policy is explicit:
the shell exits with a Wayland diagnostic and releases its IPC instance; the
harness restarts **its own compositor** and verifies a fresh shell reconnects.
There is no automatic in-process reconnect loop. External app behavior, hardware
and service-actor semantics are outside this mocked UI boundary.

The functional watchdog retains `checkpoint.json` after every case; an incomplete
run fails and cannot become a passing matrix. `HYPRV_FUNCTIONAL_LIFECYCLE=1` also
runs compositor disconnect/restart probes when using a diagnostic filter.

## Retained updates and degraded system bus

Each mounted bar owns a `hyprv_ui_bar::view::RetainedView`. It computes layout,
compares module rendering inputs, and constructs/reconciles only changed module
subtrees. Callback timestamps, sampling generation/history and values formatting
to the same label do not rebuild modules or damage the scene. Geometry, theme,
assets, hover, notification dots and membership changes remain observable. The
small chrome scaffold and island/calendar slots update separately; stable module
NodeIds survive ordinary updates. Removing a surface drops its retained-view state.
Regression coverage compares incremental and fresh raster output across theme,
scale, width, hover, calendar and network transitions and checks localized damage.

Battery popup wrapping uses the engine's bounded paragraph metrics cache. Hidden
battery popups do no wrapped measurement, including while a tray menu animates.
Closing visible battery cards continue to update through their final frame.

If the system bus is unavailable, `system_bus` provides an isolated local peer
connection without well-known services and retries the real bus with exponential
backoff (0.5 s to 30 s). A recovered production connection causes a process re-exec
so all services share the real bus; the test-only Report mode avoids re-exec.
The sleep topic starts exactly once per real-bus service runtime. Desktop CLI
commands use the running shell's IPC, falling back to the same in-process desktop
provider when its socket is absent/refused. `startup-confirm` is an independent
one-shot dialog and does not start the normal service assembly.

Popup readiness measurements are separate from whole functional-case durations.
`HYPRV_UI_PROFILE=1 HYPRV_FUNCTIONAL_FILTER=battery.open-close hyprv-shell
--nested-functional-test OUT` writes `battery-open-profile.csv` in that private
run. It timestamps native release receipt, surface creation/configure and GPU
submission; it does not measure physical display presentation. Common font styles
are prepared during startup, with shared font streams and opened FreeType faces.
See docs/verify/perf-followup.md for current numbers, raw traces and the declared
Qt animation timing boundary. Profile RPCs observe state without requesting frames.

Idle host scheduling: settled frame acknowledgements skip assembly reconciliation.
Service notifications coalesce into stats/other bits; a lone stats update, with
no popup, animation or due deadline, updates retained bar modules directly. Other
notifications and coalesced stats+other notifications use the normal full path.
Tray title/tooltip changes notify the UI only if sorting/network classification
changes; menu layouts and click routing flags remain live. Icon decoding cache
identity follows image inputs rather than tooltip text.

For a strict whole battery open/reveal/close timing check in the private compositor:
set `HYPRV_UI_PROFILE=1 HYPRV_FRAME_BUDGET_MS=16.7
HYPRV_FUNCTIONAL_FILTER=battery.open-close` and run `hyprv-shell
--nested-functional-test OUT`. The fixture directory contains first-buffer and
whole-cycle CSVs plus `battery-frame-budget.json`; any measured frame beyond the
budget fails the case. This is wall time from frame begin to GPU submission,
not compositor presentation or a physical input latency measurement.

Tray `IconImage` requests `round(18 × DPR)`; Qt's default `QIcon::pixmap`
overload also uses the screen scale (ceil DPR). The bounded asynchronous loader
resolves `(request, screen scale)` then rasterizes the selected file up to
`request × scale` physical pixels. At DPR2 this gives Chrome64 and WeChat72,
minified with mipmaps into18 logical pixels. Supplied SNI pixmaps retain their
`ceil(18 × DPR)` selection target. Both request and screen scale belong to the
bounded cache key. The offline Qt6.12 oracle is `tools/qt_tray_probe.cpp`; runtime
remains Qt-free.

At real-service output initialization the host prepares the hidden calendar's
header, day labels and first timeline row (150 logical pixels high), without
provider event summaries, on the production EGL context, keyed by scale/theme. Preparation
creates no native window, action or sampling lease, and purges its temporary GPU
target. Warmup work is independent of provider event count; it does not preload
the full 500-pixel calendar. The architecture RSS budget applies to the retained
font/pipeline caches too (see docs/verify/calendar-memory-budget.md). Fixture
replay remains deterministic and independent of this production-only preparation.

`hyprv-shell --nested-idle-test OUT` runs the assembled native shell against a
private compositor with stable mocked providers for10.1seconds. Native frame
callbacks and host deadlines stay enabled. After fixture animations settle, its16ms test animation
clock is removed; a1Hz mock sampler publishes changing generation/history
through the production projected bridge on the service runtime. The gate requires
zero paints, zero metadata-driven UI wakes and no main dispatches or voluntary
context switches beyond the RPC boundaries. It also verifies samples reached
the projector, so an accidentally dropped subscription cannot produce a pass. The real minute clock stays installed;
the worker starts the window away from a minute edge. Linux scheduling counters
are wakeup proxies, not kernel sched_wakeup trace events. The parent watchdog bounds the private child to60seconds. Reports include idle.csv
and manifest.json. This is an ordinary non-ignored integration test in
`cargo test --workspace --all-targets`, so snapshot CI runs it automatically. It
is a deterministic UI wakeup gate, separate from real-provider60s CPU budgets.

Bar stats notifications use display labels and actual icon/color thresholds;
history/generation and sub-display changes do not wake an unchanged bar. Visible
Stats/Panel consumers retain raw-sample forwarding for graphs. No sampling timer
or provider action is removed.

`HYPRV_IDLE_RAW_STATS_CONTROL=1 hyprv-shell --nested-idle-test OUT` is an explicit
negative control using the old unprojected bridge; it must fail on redundant
metadata wakes even when repaints stay zero. Never used by the production host.

The corner host keeps one full-output, input-transparent layer per output and
Qt's original viewport-sized R8 Canvas atlas. Four tiny corner surfaces are a
historical experiment; they are not the current topology. Static direct-window
EGL drawables are retired after submission while the compositor retains the last
attached buffer. New damage recreates them in the shared context; namespace,
layer rules, exclusive zone, input region and on-screen geometry are unchanged.


Stats-only updates first compare the five rendered labels and severity styles.
Stable measured widths edit only changed Text nodes; unchanged rounded values
produce no damage. Width/style changes use the retained component update. Until
the bar exports its own stats patch API, the host invalidates its retained-view
signature after direct edits so a later full update cannot reuse stale labels.
Regression tests compare patched and fresh pixels at DPR 1, 1.5 and 2. The host
requests presentation feedback during animation and initially for refresh discovery;
settled stats/agent data frames paint directly without an empty commit/frame
callback. Animations keep compositor pacing and fixture clocks are unchanged.
`HYPRV_STATIC_CALLBACK_CONTROL=1` restores the callback path for an isolated
same-binary A/B measurement; it is read once and adds no timer. See
`docs/verify/live-idle.md` for measured CPU/scheduling results and live-profile
permission limits.


`services::Visibility` separates clock visibility and effective bar statistics
from broad provider observation. `BarStatsDemand::{Hidden,Compact,Detailed}` owns
no lease / CpuTotal+Memory at 5 s / the existing desktop five metrics at 1 s,
respectively. Mode replacement drops the previous lease before reacquiring.
Desktop fully translated-out labels release stats demand; partially visible labels
retain it. Panel/Stats demands remain independent. The laptop compact-view owner
must aggregate actual per-output painted demand using this API; this resource
change itself introduces no runtime profile flag or laptop layout.

After the last calendar closing frame, the host releases its cached render tree
and model key, retaining only the bounded first-frame preparation key and component
semantic state. Stale native popup roles are closed, not retained at zero opacity.
No hidden animation/cache maintenance timer is introduced.

### Runtime desktop/laptop assembly

The process latches `hyprv_core::profile::profile()` before actors/UI construction.
`HYPRV_PROFILE=desktop|laptop` overrides portable DMI/internal-battery detection;
invalid values fail startup. `--profile-status` prints the selection and rooted
filesystem without starting actors, IPC or Wayland. Native sysfs fixtures live at
`$HYPRV_SYSFS_ROOT/sys/...` (default filesystem root `/`). System-bus overrides
remain separate from the session bus; nested fake worlds preserve both explicitly.

Laptop bar targets start compact per output, use one global 500 ms wall-clock
ToggleGate, and animate through the shared bar controller. The corner window owns
compact-strip input. The settled hidden full bar retains its logical layer role,
namespace and exclusive zone with a transparent one-pixel backing; its full-size GL
resources are destroyed. The original output-sized corner Canvas viewport remains
pixel-exact and retires its drawable after a settled submission. Closing native
popups destroys their roles/targets after their exit animation. Compact stats edits
reconcile only the corner strip; the hidden full-bar tree is not rebuilt.

Visible compact CPU/memory negotiate a 5 s lease; hidden metrics negotiate none.
Expanded/stat-card consumers request their painted metrics at 1 s. Laptop CP alone
adds no stats lease. Battery detail demand ends at the close request, Wi-Fi scanning
at `popup_visible=false`, and media presentation at the last expanded bar/CP close
request. Hidden islands stop waves/position/spectrum. Optional visible CPU watts
use power's separate CPU-only endpoint; compact CPU/memory never activate it.

Laptop omits IR/RAM LED, Deskflow, Viewflow, eGPU and VM actors. Unsupported state
has typed default topics and commands fail explicitly; no replacement polling task
is created. `desktop deskflowReflow` is a successful laptop no-op even without a
running shell. Shared demo/recording/theme/application/startup/agent services remain.
Desktop selects the original actors, metrics and UI defaults.

The reviewed laptop deployment patch renames legacy executable calls; no global
Quickshell shim is installed. Native CLI also accepts both legacy `-p/--path` IPC
argument orders, and the standalone StartupConfirm path maps to `startup-confirm`.
Brightness notification IPC previews whole percent on laptop and writes no hardware.

`HYPRV_PROFILE=laptop hyprv-shell --nested-functional-test DIR` selects laptop-only
cases at 1440x900 logical/DPR2, including private native UPower/PPD/login1/sysfs
worlds. Desktop remains the original 204-case suite. Both use native virtual input;
component fixture projections and actual provider cases are labelled separately.
The slider's native wheel is ignored as in the pinned QML; +/-1 is a local
brightness-notification IPC preview, not a new hardware-set wheel action.

Compact laptop island models continue receiving actual agent events while their
visual revision, animation and sampling deadlines remain frozen. The model-only
subscription is separate from the visible scan/spectrum lease; expansion refreshes
all retained provider inputs before the reveal. Laptop slider Sets restart one
90 ms read-only provider probe; release emits no Finish. No periodic probe is kept
when hidden. The private functional suite also evaluates the pinned laptop
`controls.lua` `M.plan` for all101 levels with an inert timer/fake profile; it never
calls that module's hardware/action functions.

The executable configures glibc before starting actors/GL: at most two allocation
arenas, the initial128KiB mmap boundary and a fixed2MiB automatic trim threshold.
Explicit `MALLOC_ARENA_MAX`, `MALLOC_MMAP_THRESHOLD_`, `MALLOC_TRIM_THRESHOLD_`, or matching `GLIBC_TUNABLES` entries keep precedence.
This bounds retained free heap pages after large temporary work; it does not evict
live font/glyph/Skia caches or add a timer. One `malloc_trim(0)` releases freed
font/material startup scratch before entering the UI loop; no trim runs on a
reveal/close frame. The setting is shared by desktop/laptop and does not change
rendering arithmetic. Memory measurements distinguish `RssAnon`, executable pages,
driver mappings and PSS; llvmpipe software allocations are not physical-XPS proof.
