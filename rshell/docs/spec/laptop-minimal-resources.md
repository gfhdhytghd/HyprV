# Laptop minimal-state resources — Phase A

2026-10-24; engine. Analysis and measurement only. No production edits, Cargo builds,
install, live input, service control, or live compositor changes. The freeze remains in
force. This is a resource/lifetime design, not a claim that the laptop profile exists.

## Contract and an important visible-data constraint

Use the same binary and renderer, selecting a runtime policy rather than a compilation
fork. Preserve desktop defaults and all desktop pixels. Laptop source reviewed directly
with `git show origin/laptop:quickshell/shell.qml`, branch commit
`24972f711735ce92e72ef1db5bdf27f85ae8cfc9`; no branch checkout.

Minimal means a collapsed bar, no hover or popup, no animation/agent transition, no
playing/expanded media consumer, battery operation, settled providers. Targets from
`docs/tasks/laptop.md`: RSS <=120 MiB, controlled idle <=0.1%, no unchanged GPU work,
and no timers for invisible data. The current desktop binary has no laptop profile:
its `bar.toggle` is intentionally a no-op (`assembly.rs:813`). The measured desktop
closed-popup state below is a baseline, **not** a laptop collapsed/on-battery test.

The laptop strip is not just a clock. `origin/laptop:quickshell/shell.qml:3590–3656`
shows workspaces, CPU/memory percentages, minute clock, battery, Wi-Fi, control and
notification indicators. `:3277–3283` explicitly samples CPU/memory with the minimal
snapshot every **5 seconds**, versus detailed samples every second. Therefore retain
a `{CpuTotal, Memory}` 5 s lease while those labels are visible; remove temperature,
package-power, per-core and network-rate sampling unless their actual consumer is
visible. A minute-only timer promise would silently freeze the visible percentages.
If both percentages are absent on a particular output/configuration, there is no
stats lease at all. Battery uses UPower events. No battery/history polling is justified
by the strip. Desktop's current broader 1 Hz lease remains unchanged by this plan.

## Current allocation and lifetime inventory

Source paths below are relative to `crates/`; exact reviewed hashes are retained in
[the source receipt](../verify/raw/laptop-minimal-resources/source-hashes.json).
Cache ceilings are limits, **not measured occupancy** and must not be added as RSS.

| Resource | Current ownership/allocation | Hidden/minimal treatment proposed |
|---|---|---|
| Bar layer | One `hyprv-quickshell` layer per output; current nested desktop is 3072x58 logical at DPR2. Ordinary EGL views hold a retained RGBA8 target plus EGL-managed swap buffers. | Laptop expanded standard bar unmaps after closing motion; collapsed strip alone retains its actual painted band and input mask. No transparent full standard-bar target just to accept input. |
| Corner layer | `hyprv-shell/src/assembly.rs:1479–1492` mounts **one full-output** overlay and sets `full_redraw=true`. At 6144x3456 each RGBA8 buffer is **81 MiB**. This path wraps framebuffer 0, avoiding the additional 81 MiB retained-copy target; the EGL swapchain still exists. | Do not mistake the historical four tiny surfaces in `engine-memory.md` for today's topology. Keep current exact path until the small-buffer topology below proves exact. Never destroy visible corners just because other UI is hidden. |
| Corner texture atlas | `hyprv-ui-bar/src/surfaces.rs:124–153`, `hyprv-ui/src/qt_canvas.rs:8–98`: four tiny masks packed into a viewport-derived power-of-two **R8** atlas. 6144x3456 =>8192x4096 =>**32 MiB**, additional to swap buffers. Ordinary 8 MiB native-image eviction excludes these atlases; up to four viewport/scale variants may survive (`native_gl.rs:1295–1322`). | Keep only active output/scale keys. Dropping an output or scale must delete obsolete atlas textures with the context current. Compact/sparse atlas is a separate parity-gated change, never a linear-sampling substitution. |
| Panel/popup native surfaces | Control, Wi-Fi, stats, quick-adjust, battery, menu/overflow and startup surfaces appear only in desired mounted roles. `assembly.rs:1243–1253` calls `engine.close` for stale views. Views own native EGL windows. | Preserve existing destruction after the last closing frame. Release focus/pressed/touch state and leases; zero hidden `wl_surface`, EGL drawable, swapchain or FBO. Do not destroy halfway through closing animation. |
| CPU panel state | `OutputOwners` retains controllers/models; `CalendarTree` retains its last tree and model key; island tree is revision-cached (`assembly.rs:94–160`). A closed GPU window does not imply these trees or their images have died. | Retain only semantic state needed on reopen (selection, user edits, provider snapshots). Drop hidden render trees, effects and copied model/layout payloads; retain small geometry/style keys. A close/reopen must preserve behavior. |
| Shared graphics | Exactly one UI-thread `SharedEgl`, one EGL context, one Skia DirectContext, 1x1 pbuffer; lazy native material context (`egl.rs:23–42,70–94,302–358`). Every surface uses the same context. | Retain this context while any visible band/corner exists. No per-panel context, background clone or per-output font atlas. Warm only first-use materials; context destruction/recompilation on every click is too expensive. |
| GPU programs / scratch | Native programs are lazy by variant. Startup executes a 64x64 real material sample, including masked compose (`egl.rs:97–170`); first calendar header/row is prepared per scale/theme (`assembly.rs:1043–1068`). | Preserve a small fixed first-reveal program set; destroy temporary targets immediately. Do not cache complete calendar days, themes, output histories or all panel content. Cold first-frame materials need a measured budget before trimming more aggressively. |
| Retired GPU allocations | `EglWindow::drop` releases retained target, switches to pbuffer, purges unlocked **scratch** resources, destroys EGL surface (`egl.rs:763–785`). Driver/compositor may retain in-flight buffers until release. | Retirement is event-driven, after the last use/fence/release. No synchronous `glFinish`, no periodic purge. Track client-owned and compositor-held buffers separately; memory need not fall before the compositor releases a submitted buffer. |
| Native glyph atlas | Shared `qt_gl_glyphs.rs:9–10,65–74,122–163`: up to four 512x512 **RGBA8** pages, **1 MiB/page**, plus a 256 KiB streaming VBO initially. A8 masks are expanded for the current shader; this is not a 256 KiB/page allocation. | Pin only strip/reveal glyphs and trim unused pages on minimal transition. Keep integer quads, coverage, filter, shader precision and exact colors. Optional R8 glyph storage needs an independent pixel proof; do not assume alpha-swizzle parity. |
| CPU text / font state | `text.rs:580–614`: masks, distance fields and raster masks; native atlas capped by entries (8192), shaping <=2048 hash buckets/4 MiB (`:708–712`); wrapped measurements <=512 entries/1 MiB (`:1068–1106`). Shared font mappings/typefaces (`:133–165`) avoid duplicate CJK file copies; process-lifetime bytes are intentionally retained for Skia stream safety. | Byte-budget caches, with visible glyphs pinned, hidden layouts evicted. Reuse `font_mapping`/`mapped_font`; do not unmap bytes while FT/Skia can reference them. File length/VIRT is not resident font cost. Panels' own qtext caches need the same close/trim hook, coordinated with their owner. |
| Rect / Canvas / image caches | Renderer-owned effects/bounds/Qt rect meshes; rect coverage ceiling16 MiB and raster ceiling32 MiB (`qt_rect.rs:654–658`); Canvas raster cache16 MiB/256 (`canvas.rs:414`); image cache64 MiB/64 (`image.rs:326–333`). Native ordinary images/masks8 MiB/512, separate from glyph/corner atlases. | No CPU raster fallback cache needs to be populated by the production GPU path. Trim hidden effects/coverage/images on state transition, preserving live material identities and exact sampling. Enforce a combined budget, not independent large maxima. |
| Tray / art / assets | Async TrayLoader64 MiB input+image limit, two jobs (`hyprv-shell/src/assets/tray_loader.rs:58–59`), retained by output scale/item. Art input limit16 MiB, decoded extents up to4096x4096 (`assets.rs:10–60`). Shared assets load through panels' asset loader (`assembly.rs:1444–1447`). | Collapsed laptop has no tray or album art: drop their pixel copies/textures and stop new image jobs; keep inexpensive service metadata needed for future actions. Retain only current live scale requests and required strip icons. Cancel/ignore stale completions by generation; no completion-induced reload loop. |

`Renderer::clear_caches` (`render.rs:174–190`) clears broadly and invalidates native
caches for the next draw. `Engine::clear_render_caches`/`SharedEgl::clear_caches`
(`wayland.rs:490`, `egl.rs:57`) performs immediate GL deletion with its context current.
These are existing emergency/full-reset hooks, **not** a selective hidden-owner trim API.
Calling them on each close would also evict visible glyphs and recompile materials.
Phase B needs an owner-scoped, byte-accounted trim operation instead.

## Proposed minimal graphics layout and memory envelope

First preserve one full-output layer-shell **role**, namespace, anchors and input rules.
For laptop, the QML corner overlay also owns the interactive 15 logical-pixel black
compact strip (`origin/laptop:quickshell/shell.qml:3546–3587`): treating this layer as
universally input-empty would break clicks. Expanded mode remains click-through.

Prototype a full-output parent backed by one transparent texel through viewporter,
with desynchronized, device-aligned top and bottom subsurfaces carrying the actual
painted bands. The parent keeps compositor-visible layer identity and the exact
input region; children take no independent input. Top reserves the maximum reveal
extent (42 logical pixels in the existing desktop motion); bottom reserves27. At
DPR2, 6144x84 +6144x54 RGBA8 is **3.234375 MiB per set**, versus81 MiB per full set;
allow two/three buffered sets and compositor ownership, not an assumed fixed count.
Laptop strip must be rendered as part of the top band, not overlaid twice.

This is a **candidate, not an approved optimization**: previous cropped corners changed
alpha fringes. Carry original full-output device coordinates, matrix, UVs and atlas
allocation into child rendering. Verify framebuffer/viewport transformation arithmetic
as well as final compositor sampling, output transforms, fractional scale, blur and
layer-rule behavior. A one-texel parent can change blur/rule interpretation and child
stacking; fail closed to the present topology if those differ. In particular do not
reuse the earlier four-surface result as proof. No desktop topology switch without a
separate exact full-screen gate. If this prototype fails, retain the full overlay and
report the GPU cost as unresolved rather than claiming a saving.

Treat the32 MiB corner atlas independently. An extension-dependent sparse allocation
could preserve its exact dimensions/filtering, but cannot be the portable baseline.
A compact atlas with shader emulation of original UV interpolation/filter precision
is a research option only; existing baked small masks do not prove equal sampling.
Until proven, budget the actual32 MiB, one per active viewport class, never hide it
inside the ordinary8 MiB cache. Laptop physical output dimensions must determine this
budget; no XPS resolution/driver is assumed from the desktop6144 fixture.

Provisional CPU resident envelope (MiB, **design allocations rather than a forecast**):

| Account | Ceiling |
|---|---:|
| Executable, libraries and graphics-driver resident pages | 70 |
| All actor/controller/model state, stacks and general heap | 20 |
| Resident shared fonts and text working set | 10 |
| Decoded visible assets, image/rect/Canvas/layout caches combined | 8 |
| CPU staging/transient first-frame reserve | 8 |
| Unattributed/measurement margin | 4 |
| Total RSS | **120** |

The driver/library ceiling may be infeasible on this NVIDIA desktop; it must be
measured on XPS later. PSS is additionally reported, not substituted for the RSS gate.
GPU allocations overlap CPU mappings on unified-memory hardware: report GPU bytes
separately and do not double-add them to RSS. Reserve a small Skia GPU resource limit
(initial laptop policy4–8 MiB, excluding live external swapchains and separately-owned
native textures) and a measured CPU font-cache limit; neither limit exists as a laptop
policy today. Do not promise that `purge_unlocked_resources` frees locked live resources.

## Transition and frame-budget rules

1. On show intent, acquire only that consumer's leases/subscriptions and snapshot
   latest retained data. Decode files/query history off the UI thread; cache semantic
   results, not complete hidden scenes. Generation-tag completions after close/unplug.
2. Build the first visible frame using bounded header/first-row content, previously
   warmed material programs and pinned common glyphs. Create the cropped native
   drawable at its final device-aligned size. Do not wait for network/history or
   enumerate icon themes inside the input-to-first-present path.
3. Closing animation keeps its native resources until its final submitted frame.
   Then cancel pointer/scroll grabs, drop demand and deadlines, close the surface,
   release owner-tagged effects/textures/layouts and purge only unlocked scratch.
   Keep user-entered form state only where legacy keeps it. No hidden cursor blink,
   switches, pulses, calendar scrolling or media-seek timers.
4. Keep one shared context/program set. Never compile a shader or upload an entire
   font on reveal. A profile-specific startup preparation may exercise a64x64 material
   target once and then destroy it. Smaller glyph cache misses are allowed only after
   proving every first/repeated reveal frame <=16.7 ms; eviction must not trade memory
   for the earlier35/38 ms cold calendar frames.
5. Initial planning allowance for first reveal: model/layout <=3 ms, drawable/first
   glyph uploads <=2 ms, paint/submit <=5 ms, remaining6.7 ms driver/scheduling margin.
   These are **budgets, not measured guarantees**. Validate input-to-first-present as
   well as CPU paint time, cold/warm and under memory-pressure eviction.

## Damage and idle policy

Current `egl.rs:461–640` already supports disjoint logical damage, device-pixel
outward rounding, buffer-age repair history, `eglSetDamageRegionKHR` and swap-with-
damage when advertised. Ordinary targets retain unchanged pixels; age0/lost history
must copy/repair the full target. Full-redraw corners bypass retained-copy storage.
Keep these correctness fallbacks; no unsupported extension may silently leave stale
pixels. Update only modules whose **formatted** display key changes. Scale/rotation,
position, effect padding, clips and output removal invalidate the appropriate area.

Minimal strip percentages change at most on their5 s sample; the clock at minute
boundaries; UPower/network/notification/workspace signals only on visible changes.
No damage =>no layout, shaping, glyph upload, GL bind/draw/swap or wl_surface commit.
No idle frame callback requests. An unused single-shot deadline must be removed,
not rearmed with a long delay. No maintenance timer for eviction or shader warmup.
Keep GL as the production rendering path; the earlier CPU-wl_shm text overlay waiver
is not a shortcut to laptop parity. New split bands require native GL + compositor
full-screen comparison, with every corner and collapsed-strip input region unmasked.

| Source | Laptop minimal requirement |
|---|---|
| Clock | One absolute minute deadline while a clock is visible; timezone/resume signals rebase it. No duplicate clock-topic/bridge repaints. |
| Visible CPU/memory | One shared5 s lease, not one timer per output; formatted equal =>zero UI wake/paint. Hidden detailed metrics, histories, network rate, per-core, temperature and RAPL =>zero sampling. |
| Battery / Wi-Fi / notification / workspace | Passive native signals, projected to visible values; unchanged/metadata-only state =>zero UI work. UPower battery polling and Wi-Fi scanning =>zero. |
| Media | No compact-strip media: zero position tick, spectrum/audio capture and art fetches. Retain event subscriptions only if needed for an actual visible island/indicator; playing media is a different state and needs its own budget. |
| Panels / quick adjust / calendar / hover / overflow | Zero mapped hidden surfaces, samples, animation/debounce/expiry/cursor timers after close. User-initiated pending operation may retain a bounded timeout until completion. |
| Agent discovery | No1.5 s probe timer justified merely by a hidden consumer. Preserve late-exec/exit/reuse behavior via event sources where available. If platform permissions force a periodic discovery fallback for a visible agent indicator, expose/count it and resolve the budget/behavior conflict explicitly; do not disable discovery silently. |
| Fan/helper / desktop-only features | No laptop nct6799 helper observer/control unit. No fan, VM, Deskflow/Viewflow or Mihomo keepalive solely for hidden UI. Any user-requested background task remains an explicit service responsibility. |
| D-Bus / reconnect / IPC | Blocking FD waits are fine. No disconnected-provider retry spin or periodic property refresh; retry backoff only while genuinely disconnected/pending. Keep reconnect/resume correctness. No UI timer for socket2 reads or shell IPC. |
| Graphics/cache | Zero periodic purge, shader compilation, EGL swaps, compositor callbacks or empty commits while pixels are unchanged. |

The current visibility gate is too broad for this policy: `Consumer::Bar` drives audio,
MPRIS, tray and the five-metric1 Hz stats lease (`services.rs:434–457`). Add effective
rendered-consumer demand for compact stats, detailed stats, tray, media/position and
strip icons; default desktop demand remains identical. Existing `forward_with`
(`services.rs:87–117`) correctly drops subscriptions when the visibility epoch ends.
Reuse it instead of keeping a hidden watcher merely to relay unchanged snapshots.

## Baseline measurement

Installed shell ELF SHA-256:
`c364f556a1a5168a7bcac866616e1912a87b06951b7ac321b2a7a0f22d1d52df`
(6719efa8 shell; emergency helper-only candidate does not enter this private run).
[Binary receipt](../verify/raw/laptop-minimal-resources/binary.json),
[raw uninstrumented window](../verify/raw/laptop-minimal-resources/desktop-minimal.json),
[accounting summary](../verify/raw/laptop-minimal-resources/summary.json).

| Measured quantity | Uninstrumented desktop closed-popup baseline |
|---|---:|
| Duration | 90.168 s |
| Whole-process CPU, including exited workers, process-tick resolution | **0.2107%** |
| Sum of sampled thread runtime | 0.2017% |
| Voluntary context-switch proxy | **20.9719/s** |
| Schedule-in proxy | 21.1383/s |
| smaps_rollup RSS start / end | **162.14 /94.50 MiB** |
| smaps_rollup RSS sampled min / max | 94.50 /162.79 MiB |
| smaps_rollup PSS start / end | **96.82 /79.62 MiB** |
| proc/stat RSS start / end (different kernel accounting) | **185.63 /119.31 MiB** |
| Swap start / end | 0 /0 MiB |
| Shell GPU allocation start / end, NVIDIA per-process | **257 /257 MiB** |
| Fixture hook-file changes | **0** |
| Mapped layers | **2**, bar3072x58 +corners3072x1728 logical, DPR2 |

RSS did **not** stabilize at its initial value: file-backed residency/PSS changed
substantially. The difference between `smaps_rollup` and proc/stat RSS is retained
rather than silently choosing the smaller number; its exact kernel/driver accounting
cause is not established. The pinned executable is a temporary byte-copy with a
private inode, so executable-page PSS sharing differs from an installed hardlink.
These values do not certify a steady <=120 MiB profile. The host was concurrently
loaded (post-run loadavg21.59/13.12/9.73, memory PSI some avg60=1.16%);
[host context](../verify/raw/laptop-minimal-resources/host-context.json). No shell swap
was observed. Do not attribute the RSS drop to a cache optimization: no code changed.
The pinned41 MiB executable and disposable compositor runtime were removed after
the run; compact receipts remain. No private Cargo target was created.

A sampled mid-window mapping summary has font RSS8.61 MiB (CJK alone6.85),
heap16.51 MiB, anonymous8.73 MiB and NVIDIA/driver62-ish MiB, plus code/libraries.
It is a different instant from the endpoints. Top paths include NVIDIA gpucomp27.27
MiB, private executable25.66 MiB, `/dev/nvidiactl`20.64 MiB and LLVM15.48 MiB.
[Mapping totals](../verify/raw/laptop-minimal-resources/mapping-summary.json),
[top mappings](../verify/raw/laptop-minimal-resources/mapping-top.json).
Do not call all driver mappings shader caches or assign the complete257 MiB GPU
figure to the corner target: per-resource driver counters are unavailable. The known
81 MiB/buffer +32 MiB atlas account is a source-derived lower-bound allocation model;
swapchain depth, in-flight/compositor ownership and other driver overhead are not
individually measured. `/proc/PID/fdinfo` exposed no DRM memory counters on this driver.

### Wakeup attribution and its measurement limits

These rates are **voluntary context switches, not sched_wakeup counts**. Summing
source notifications to them would double-count a single event through the reactor,
UI bridge, GPU submit and buffer release. Kernel waker attribution was unavailable
under the permissions documented in `docs/verify/live-idle.md`; no privilege changes
were attempted. Report unmatched reactor work explicitly.

| Thread / source class | Baseline voluntary-switch proxy/s | CPU % | Attribution |
|---|---:|---:|---|
| UI/main | 4.3474 | 0.14711 | Stats projections, paint/submit and Wayland events. Separate counters below identify paints; exact switch-to-source joins unavailable. |
| Service reactor | 15.9591 | 0.05249 | Shared native provider/clock/stats/IPC FD handling; **residual not individually attributed** to each D-Bus/socket/timer. Not15.96 samples/s. |
| Agent discovery | 0.6654 | 0.00213 | 60 switches/90.168 s, consistent with retained1.5 s scan cadence despite quiet hooks/PID namespace. |
| PipeWire audio + module-rt | 0 | 0 | Both threads present, no counter increments in this window; spectrum off. |

A **separate instrumented**45.166 s private run confirms sources without treating its
CPU0.2435% as the uninstrumented result. [Raw diagnostic run](../verify/raw/laptop-minimal-resources/desktop-minimal-diagnostic.json),
[source counters/timings](../verify/raw/laptop-minimal-resources/diagnostics.csv).
Counters span `dataStatus` reset through graceful shutdown, including small boundary
work outside the timed process snapshots; rates below are deliberately approximate.

| Source / work | Observed diagnostic count | Minimal-state consequence |
|---|---:|---|
| Projected sysstats ->UI stats pass | 90 /90 (~2/s) | Notifications pair initially~0.21/0.79 s apart. DataStatus advertises1000 ms. Likely local stats +independent package-power changes; current `sysstats.rs:329–336` already skips local sampling on power-only events. **Not proof of2 Hz /proc reads.** Laptop CPU/memory-only lease removes hidden power demand. |
| Tray projected state | 2 | Native external signal inputs; compact laptop has no tray consumer. |
| Metadata-only projections suppressed | 2 | Already no corresponding forwarded state wake. |
| GL paints / swaps / immediate-static scheduling | 92 /92 /92 | Nearly all visible detailed stats changes, not an autonomous animation loop. Compact profile must cut this demand rather than merely make full-output swaps cheaper. |
| UI dispatches | 184 | Dispatches are not paints or exact kernel wakeups. Keep residual Wayland/FD dispatches visible in future tracing. |
| Glyph atlas uploads / other native texture uploads | 8 /2 | Counted actual uploads; not an idle zero-upload guarantee when detailed labels change. Pin/reuse strip glyphs; unchanged labels must upload0. |
| Fixture hook changes | 0 | Real host hooks are isolated from the measured shell. |
| Hidden panels / spectrum / Wi-Fi scan / battery detail | All reported inactive before measurement | No hidden mapped native layer in compositor layer inventory. Internal provider wake counts are not individually exposed, so inactivity is not a measured zero-RPC proof. |
| Minute clock, socket2, individual D-Bus senders, buffer-release events | Not separately counted in this baseline | **Unresolved exact rates**; no claim that absent source counters mean no events. Future private mock/syscall-source instrumentation must close this attribution gap. |

The final instrumented frame CPU/paint budgets are not reveal tests. There was no
input or popup opening in either window. Its GPU endpoint261 MiB versus257 initial
is reported separately from the uninstrumented257 plateau. This baseline fails the
laptop CPU aspiration and does not certify laptop minimal state, RSS or zero-wakeup
requirements. It identifies the first demand and large-buffer work to remove.
Reproducer: `python3 -B docs/verify/scratch/laptop-minimal-resources/baseline.py`.
This pins the **installed** executable, starts private virtual KWin + nested Hyprland,
uses6144x3456@2, read-only native providers, empty hook fixture and private PID namespace,
and never builds Cargo or writes shared target. The existing compositor-only flush
shim is compiled in a disposable `/tmp` runtime; no shell production code is built.
Private shell IPC/cache/state paths isolate writes. Host buses are read-only providers,
so spontaneous external D-Bus events remain possible and must be counted separately.

## Acceptance and ownership handoff

After freeze, implement by lifetime/demand dependency, not by parallel renderer paths:
first compact consumer leases and lazy CPU assets; then owner-scoped cache budgets and
surface retirement; then parity-gated corner storage experiments. Coordinate bar demand
with its owner, panel text-cache trimming with the panel owner, and service timer changes
with service owners. Phase A changes none of these sources.

Earlier exact idle findings and proposed diffs are already filed:
[svc-desktop](../requests/hyprv-svc-desktop.md#p1-engine-live-idle-attribution-2026-10-24-scope-kept-with-desktop-owner)
(line814: live discovery0.3585% CPU,120 voluntary switches/180 s; shared worker0.2643%,
not wholly attributable), and
[svc-power](../requests/hyprv-svc-power.md#p1-engine-live-hidden-fan-observer-2026-10-24-power-owner-keeps-edits)
(line490:45 FanStateChanged broadcasts in~60 s, distinct from RPC/wakeup counts).
Those are historical attribution windows, not today's nested baseline or proof that
owners have not since changed the paths. No service source edits by engine.

Future gate: laptop fake UPower/battery/lid/power-profile/sysfs; zero hidden metric reads
and zero hardware writes; >=180 s settled minimal baseline with RSS/PSS/GPU/process CPU
and source counters; minute-boundary and CPU/memory-only damage; cold/repeated popup
frames; all-output scale1/1.5/2/hotplug/reorder/reconnect; native GL full-screen laptop
references and unchanged desktop exact parity +204/204 functional actions. No XPS
hardware result or newly measured animation guarantee is claimed in Phase A.

## Phase B implementation follow-up

This file preserves the Phase A baseline and proposed budget. Current implementation,
release identity, integrated gates and measured budget shortfalls are recorded in
[the engine implementation report](../verify/laptop-impl-engine.md). In particular,
the original desktop baseline uses a different geometry/provider world and must not
be presented as a controlled same-input laptop A/B experiment.
