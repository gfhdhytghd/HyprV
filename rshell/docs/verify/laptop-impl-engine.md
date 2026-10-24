# Laptop Phase B — engine implementation

Current RSS follow-up artifact: **`87f5ca39`**, staged only.
See the matched-ELF attribution/Mesa before/after table at the end; the original
Phase B release and measurements below remain historical evidence.

2026-10-24. Runtime selector, production assembly, resource lifetimes and private
functional tests are implemented. **The 120 MiB RSS target is not yet met on this
NVIDIA desktop host.** CPU and functional/visual gates are reported separately;
this is not XPS hardware, HDR/plugin, install or deployment acceptance.

## Shared API and host behavior

- `hyprv_core::profile::{Profile, profile, sys_root, sys_path}` is the single latched
  selector. Explicit `desktop|laptop` wins; invalid overrides fail. Autodetection
  accepts reviewed exact portable DMI names or a present system Battery, rejects
  UPS/peripheral/ambiguous batteries, and diagnoses desktop fallback. Injected
  filesystem roots never fall back to host sysfs; root-escaping links are rejected.
  `HYPRV_SYSFS_ROOT` means filesystem root: fixtures live at `ROOT/sys/...`.
- Startup chooses the profile before constructing actors. No compilation forks.
  Laptop graph omits desktop cooling/eGPU/VM/IR/RAM-LED/Deskflow/Viewflow workers;
  typed unavailable state replaces unsupported providers without idle retry actors.
  Optional CPU-only watts are acquired only for an actual detailed consumer.
- Adaptive bar toggle has the shared500ms gate and per-output state, hotplug reset,
  native compact-strip input, minute clock and700ms reveal. CPU-power tooltip uses
  its350ms deadline and a cropped input-less surface, destroyed on leave.
- CLI accepts legacy `ipc -p PATH call ...`, `-p PATH ipc call ...`, and the
  StartupConfirm path. Laptop config deployment renames executable calls; no global
  quickshell replacement is installed. Brightness/Super+A target actual native UI;
  brightness notifications never write hardware or persist a preview.
- Laptop brightness uses one-percent Sets, no Finish, and one read-only90ms probe
  after the last Set. The transient notification preview also expires after90ms.
  Recording state is projected from the actual desktop recorder provider.

## Resource and demand changes

One UI-thread EGL context/Skia context remains shared by all outputs/popups. Hidden
native popup roles are destroyed after their final exit frame, releasing their EGL
window, render target and window-owned native images/glyph/corner-atlas ownership.
Hidden calendar trees/layout payloads are discarded; only small semantic/preparation
keys remain. Skia's GPU scratch cache is capped at8MiB; native ordinary textures
retain their separate8MiB budget and glyph masks useR8. These limits are ceilings,
not additive claims about observed residency. Full calendar precaching is skipped
in laptop compact state.

The original full-output corner namespace and exact Qt atlas sampling are retained.
Visible corner/strip pixels cannot be destroyed merely because the bar is compact.
After a sparse full-redraw surface settles, its EGL drawable/swapchain is retired;
the compositor retains the last committed buffer. A later damaged frame recreates
it. Compact hidden full-bar role retains only a transparent1px input backing and
its required25px exclusive zone; it has no hidden full-size component target.
Compositor-held GPU memory is reported below, rather than counted as freed system
memory. There is no periodic cache purge or empty animation timer.

Compact visible CPU/memory share one5s lease, as the legacy laptop actually does.
Temperature/watts/network-detail/history/Wi-Fi scans/spectrum/position clocks are
visibility gated. Battery detailed history and Wi-Fi scan guards drop at close
request, before exit animation finishes. Media presentation is separate from
spectrum demand. Hidden island agent events still update retained model inputs;
agent discovery sampling and visual animation are separate gates. Unchanged agent
revisions do not resample synthetic progress on unrelated stats/minute callbacks.
The first reveal refreshes current provider state.

## Verification

The release was built through `packaging/build-release.sh`; its strict source
fingerprint guard passed. No build/preflight guard was bypassed.
The tests use private KWin/Hyprland, private D-Bus/sysfs, protocol-only private
PipeWire, isolated input devices and private hook state. No live UI/service/helper
was modified, restarted, stopped or installed; no sudo or Git commit was used.

The laptop suite distinguishes UI/mock-action tests from explicit native-power
provider tests. Its hidden-agent case runs the real hook and production public-row
parser, then injects those rows into the mocked UI graph; it is not represented as
an entirely native agent-to-UI service test. The brightness plan executes the real
pinned laptop `controls.lua` M.plan inside private Hyprland with an inert timer and
fake profile,101levels; no hardware/action function is called.

## Measurement method and limits

Reproducer: `docs/verify/scratch/laptop-phase-b/resources.py NAME BINARY 90`.
Actual native service actors run in a quiescent private PID namespace, fake
UPower/PPD/login1 and fake `ROOT/sys`, at2880x1800@2. Startup/provider hydration is
outside the measured window. Primary sampling uses smaps_rollup and thread counters;
full mapping attribution is a separate `HYPRV_RESOURCE_MAPS=1` probe. The shell ELF
is byte-pinned and hashed; the temporary measured copy is deleted afterward.
Voluntary context switches/schedule-ins are **wakeup proxies**, not kernel
sched_wakeup source joins. Unmatched reactor wakeups remain explicit.

The Phase A comparison is a desktop6144x3456@2 build, native read-only host providers,
not the same laptop fixture. It measured0.2107%CPU,20.9719voluntary switches/s,
RSS162.14→94.50MiB (unstable file-backed residency), PSS96.82→79.62MiB,
GPU257→257MiB. Do not attribute the whole difference to one code change or claim
that this is a same-input causal A/B. The raw baseline and accounting caveats are
in `docs/spec/laptop-minimal-resources.md`.

The larger remaining RSS includes approximately27.1MiB NVIDIA compiler code,
15.5MiB LLVM,19MiB `/dev/nvidiactl`,24MiB executable and6.5MiB shared CJK mapping
(separate mid-window attribution, not endpoint totals). Standard
`glReleaseShaderCompiler` in an isolated diagnostic process saved no memory; an
explicit EGL-Wayland platform-selection experiment also retained the compiler
mapping. The latter source experiment was restored byte-for-byte; neither is a
production memory workaround. Unsafe driver-page eviction/context destruction is
not used to make an RSS number appear smaller or risk first-frame/parity behavior.

Earlier live-idle findings/proposed service diffs were filed in
`docs/requests/hyprv-svc-desktop.md` and `docs/requests/hyprv-svc-power.md`; those
owners implement their service changes. No engine edits crossed into their crates.


## Final release and resource results

Release ID: `b811aacfc0ea0ff0407f2e8ac0b2f1ff196e3d4608435428a5a79dead2f511b7`. Shell SHA-256:
`baa31fa7449d9419cf9cfe9dac6619d53022e7a28cd2cca6936891e76a2fb103`. [Build receipt](raw/laptop-phase-b/candidate-build.json),
[release log](raw/laptop-phase-b/build-release-final.log). The staged deployment
profile is **desktop**; this same ELF is selected as laptop by runtime policy.
This is not a certification that a laptop deployment payload was installed.

Primary uninstrumented window: **90.096s**.
[Raw window](raw/laptop-phase-b/final-minimal/minimal.json),
[summary](raw/laptop-phase-b/final-minimal/summary.json),
[initial demand](raw/laptop-phase-b/final-minimal/minimal-data-status.json).

| Metric | Measured | Target/result |
|---|---:|---|
| Whole shell CPU | 0.0444% | ≤0.1% **PASS** |
| RSS start → end | 150.56 → 150.32 MiB | ≤120 MiB **FAIL** |
| RSS sampled min → max | 149.78 → 150.72 MiB | no RSS budget waiver |
| PSS start → end | 79.37 → 79.22 MiB | informational; not RSS |
| Swap |0 MiB | none observed |
| Shell GPU |29 →29 MiB | no growth in this window |
| Private Hyprland GPU |111 →133 MiB | compositor-retained output buffer included |
| Private KWin GPU |47 →47 MiB | separate compositor |
| Voluntary switches / schedule-in proxy |4.129/s | not exact kernel wakeups |
| Hook-file events |0 | quiescent fixture |

The different-input Phase A baseline was0.2107%CPU and20.972switches/s. These
measurements show the present laptop policy under its specified visible5s stats
cadence; they do not establish a same-geometry regression result for desktop.

| Thread / source class | Voluntary-switch proxy/s | Thread runtime CPU | Attribution limit |
|---|---:|---:|---|
| UI/main |0.955|0.02800%|Visible stats/minute/Wayland FD events; diagnostic source marks below.|
| Service reactor |3.130|0.00692%|Native provider, clock/stats and IPC reactor; residual not individually kernel-joined.|
| Blocking worker |0.311|0.00114%|Occasional native filesystem/provider work; not a periodic UI paint.|
| PipeWire audio |0.000|0.00000%|No streams/spectrum demand.|
| PipeWire RT |0.000|0.00000%|No streams/spectrum demand.|

Per-thread estimates include worker lifetime handling and thread-runtime accounting;
they are not an exact additive source partition of the process-level proxy. No
claim of zero total kernel wakeups is made. Hidden visual deadlines/sampling
consumers are disabled; required visible stats and minute-clock work remain.

## Integrated gates on the final candidate

| Gate | Result / evidence |
|---|---|
| Release source guard | PASS; candidate source map matches shared sources |
| Desktop native functional suite | **204/204 PASS**; [manifest](raw/laptop-phase-b/desktop-e2e-final/manifest.json) |
| Laptop native functional suite | **39/39 PASS**; [manifest](raw/laptop-phase-b/laptop-e2e-final/manifest.json) |
| Core library | **22 passed**; shared selector/root/latching regressions included |
| Shell library | **65 passed**,2 manual tests ignored; lifecycle/demand regressions included |
| UI library | **65 passed**,25 manual/GPU tests ignored in the ordinary unit invocation; actual GL fixture harness is separate |
| Strict Clippy, all targets | PASS for core/shell/ui/widgets, `-D warnings` |

Laptop coverage now includes native compact clicks/hover, gated toggle/per-output
state, transient brightness preview, ordered stepped slider Sets plus exactly one
Refresh/no Finish, stale-provider drag pixels,101 real-Lua brightness plans,
model updates from actual hook rows while compact with frozen visual revision/no
deadline, first-reveal fresh inputs, native power-provider projections/lid/resume/
thermal selection, optional-service absence, Wi-Fi demand release, list scroll
retention and eGPU/VM absence. Native system-provider cases have their own labelled
fake buses/sysfs; the action matrix remains a mocked service sink by design.

A separate successful20.091s instrumented run measured0.0498%CPU (not substituted
for the90s primary). [Diagnostic counters/events](raw/laptop-phase-b/final-diagnostic-complete/diagnostics.csv)
show exactly **3 frames**, each directly paired with a changed visible stats event;
**1 metadata-only sample** was suppressed. There were **0 hidden-island input
rebuilds,0 glyph uploads,0 popup frames and0 compositor frame-callback requests** in this
window. The minute boundary did not fall in this particular window; a preceding
probe independently recorded its one expected clock repaint. No frame occurs
between changed visible events in the successful diagnostic trace.

Maximum stats model/reconcile CPU was **0.166ms**. Maximum draw wall time was
**2.751ms**, draw CPU1.646ms, swap wall1.427ms (included in draw). These are warm
minimal-strip frames, not a claim about every cold reveal or physical XPS frame.
The corner renderer remains a sparse full-output direct draw on changed frames;
it does not retain a full-output repair target. Ordinary retained windows keep
damage/buffer-age rendering. This tradeoff preserves exact atlas sampling and
reduces idle allocations, but is not a claim that the corner EGL repair area is
a small text-only rectangle.

Two diagnostic wrapper attempts failed during optional post-measurement cleanup
(socket/PID had already disappeared). Their counters are not used for the primary
CPU/RSS gate. The wrapper now relies on its existing private-process context cleanup;
the successful `final-diagnostic-complete` run is the cited source trace. No live
PID signaling or changed production shutdown behavior was involved.

Fresh desktop **GL** parity was run by engine after the final build:
**island380/380, bar300/300 reachable (312 total,12 existing live-unreachable battery
exclusions), panels300/300 PASS**. No new exclusions or gate changes. The verifier
reports all three gates PASS; every GL render source fingerprint matches current
sources. CPU comparison rows retained by the canonical mixed-backend report are
not being presented as GL failures or as newly rerun CPU proof.
[Immutable result copy](raw/laptop-phase-b/desktop-parity-results.json),
[report snapshot](raw/laptop-phase-b/desktop-parity-report.md),
[freshness receipt](raw/laptop-phase-b/final-parity-freshness.json),
[rerender log](raw/laptop-phase-b/desktop-parity-final-gl.log).

Remaining limits: RSS exceeds120MiB on this NVIDIA host; exact kernel wake-source
joins, physical XPS resources/input/HDR, and a laptop install remain unverified.
No service cross-owner edits or live actions were used to obtain these results.

## RSS attribution and Mesa follow-up

The follow-up adds endpoint `smaps`/`smaps_rollup`/`status` accounting, not another
RSS estimate from VmSize. Raw mapping rows carry RSS, Anonymous, PSS and path.
`RssAnon` is an upper bound on shell-controlled anonymous residency: it includes
Rust, Skia, glibc and driver allocations. It cannot honestly be labelled Rust-only.
Own-executable **file pages** are executable-mapping RSS minus its Anonymous/COW
pages. Driver mapping totals include their private COW pages, so the RssAnon and
driver rows overlap and must not be added. Clean shared CJK/font mappings are
reported separately from anonymous heap.

Production change is confined to the shell allocator bootstrap. Before workers
or GL it sets glibc arena_max=2, mmap_threshold=128KiB and trim_threshold=2MiB;
explicit corresponding MALLOC_* or GLIBC_TUNABLES entries retain precedence.
After constructing fonts/materials it calls malloc_trim(0) **once before app.run**.
No trim/purge timer, hidden polling, cache invalidation, glyph eviction or trim in
a reveal/close frame is introduced. Fonts remain the shared, borrowed file mappings;
Skia and CPU cache ceilings stay unchanged to preserve warmed first-use paths.

The first in-process trial (arena/trim only) did not reproduce the environment
trial's NVIDIA anonymous reduction; that intermediate result is retained rather
than called a win. The final version explicitly restores the initial mmap boundary
and releases free startup scratch. Final-binary figures and gates follow below.

Mesa experiments change the **measured shell only** to the Mesa vendor JSON,
LIBGL_ALWAYS_SOFTWARE=1 and GALLIUM_DRIVER=llvmpipe. Private compositors keep their
existing backend. The paired final Mesa windows use LP_NUM_THREADS=2; it is a test
setting, not a production/XPS profile override. Thread names and mapping paths
confirm llvmpipe workers/libgallium/libLLVM, with no NVIDIA mappings in the measured
shell. CPU software rendering is not an Intel-iris performance proxy.

Reproduction uses the existing resources.py with HYPRV_RESOURCE_MESA=1 and
LP_NUM_THREADS=2. The mapping parser is `memory_account.py` beside it. Endpoint
capture runs before measure-shell.py intentionally terminates its private child.
This also explains the earlier optional post-measurement diagnostic cleanup
failures: the shared harness had already stopped its child, not an unexpected
production exit. Those wrapper failures did not affect the retained measured
windows. Successful final runs include both endpoints and summary receipts.

### Matched-ELF before/after results

Each window is90s,2880x1800@2, the native laptop graph with private providers.
The before control uses the **same final ELF** and scene with a private, caller-
scoped interposer bypassing only our3mallopt calls and startup trim. Driver/library
calls are forwarded. The control maps20KiB of shim RSS, included in its total.
The bypass is not shipped. [Control receipt](raw/laptop-phase-b/rss-baseline-control.json),
[full accounting table](raw/laptop-phase-b/rss-before-after.json).

| Endpoint metric | NVIDIA before | NVIDIA after | Mesa/llvmpipe before | Mesa/llvmpipe after |
|---|---:|---:|---:|---:|
| smaps_rollup RSS, MiB | 151.31 | 151.25 | 237.30 | 209.66 |
| RssAnon, MiB | 30.77 | 30.45 | 127.40 | 99.82 |
| Own executable file pages, MiB | 23.93 | 24.18 | 23.84 | 23.75 |
| NVIDIA mappings, MiB | 59.16 | 59.16 | 0.00 | 0.00 |
| LLVM mappings, MiB | 15.42 | 15.42 | 48.47 | 48.42 |
| Mesa mappings, MiB | 3.74 | 3.74 | 13.53 | 13.52 |
| Font file pages, MiB | 8.42 | 8.42 | 8.42 | 8.42 |
| PSS, MiB | 84.94 | 80.24 | 183.47 | 141.63 |
| Process CPU, % | 0.0554 | 0.0333 | 0.1886 | 0.2885 |
| Voluntary-switch proxy/s | 4.391 | 3.805 | 4.382 | 4.438 |

Anonymous RSS falls by **0.32MiB on NVIDIA** and **27.58MiB on llvmpipe**. The
NVIDIA improvement is small; the earlier environment-only ~2MiB observation was
not reproduced as a same-ELF gain and is not the claimed result. File-backed
residency and PSS vary with sharing/page reclamation, so their differences are
not all assigned to allocator savings.

The requested Mesa code-adjusted proxy is **175.96 →148.38MiB**, subtracting only
resident libLLVM+libgallium mappings. Subtracting LLVM alone gives **161.25MiB**
after. Anonymous software framebuffer/JIT/driver allocations remain included;
there is no defensible smaps-only way to call those Rust heap or subtract them
as if their physical-XPS cost were known. The adjusted NVIDIA footprint excluding
NVIDIA+LLVM named mappings is **76.67MiB** after, also a bookkeeping proxy.

RawRSS120MiB is still **not met** on this host; the conservative Mesa code-
adjusted proxy also remains above120MiB. These results do not certify Intel iris
or physical-XPS memory. Both Mesa runs have no NVIDIA mapping/process GPU row.
The software-rendering CPU increase is reported above; it is not claimed to
represent Intel GPU CPU cost. NVIDIA controlled idle remains below0.1%.

The NVIDIA status VmRSS is ~170.5MiB versus151.25MiB from smaps_rollup; the
~19MiB difference is retained in raw accounting rather than mixing the two
interfaces or double-adding device mappings. RssAnon agrees with rollup Anonymous.


### Final follow-up gates and first-use check

The final follow-up is release `87f5ca395d0d95769617f842b9701888a092c9ea29fe79aa215eff675d3a2d2c`, shell SHA-256
`3cd717d848388776aed1753ece26a803183f17a0498064c20d192ec76e3dc9d3`.
[Build receipt](raw/laptop-phase-b/rss-complete-build.json),
[build log](raw/laptop-phase-b/build-release-rss-complete.log).
The source guard passed; no preflight/source check was bypassed. Intermediate
source-drift refusals and the cancelled superseded allocator experiment are not
reported as successful releases. This artifact remains staged, not installed.

- Desktop functional **204/204 PASS**:
  [manifest](raw/laptop-phase-b/rss-complete-desktop-e2e/manifest.json).
- Laptop functional **39/39 PASS**:
  [manifest](raw/laptop-phase-b/rss-complete-laptop-e2e/manifest.json).
- Shell library **65 passed**,2 manual tests ignored; strict Clippy all targets
  `-D warnings` passed. Logs are retained with the raw follow-up evidence.

The startup trim measured **0.100ms wall /0.100ms CPU**, once before the UI loop
([startup trace](raw/laptop-phase-b/rss-complete-reveal/startup-diagnostics.csv)).
A private calendar/control-panel reveal/close exercise retained the warmed fonts,
materials and glyphs. Maximum first-use sync work was **38.052→35.820ms**; maximum
draw was7.620→9.284ms, both draw maxima below16.7ms. This sample shows no new
first-use stall from trimming; it is not a claim that the existing cold calendar
layout spike meets16.7ms—the ~36ms first-use sync remains a separate limitation.
[Before trace](raw/laptop-phase-b/rss-reveal-before/diagnostics.csv),
[after trace](raw/laptop-phase-b/rss-complete-reveal/diagnostics.csv).
Those25s activity runs end with calendar open and are not closed-popup RSS budget
measurements. The90s minimal windows above are the memory/idle results.

Fresh desktop GL parity on the final sources: **island380/380, bar300/300 reachable
(312total,12unchanged live-unreachable exclusions), panels300/300 PASS**. All GL
source fingerprints match current sources; the release source map also matches.
[Report snapshot](raw/laptop-phase-b/rss-complete-parity-report.md),
[results](raw/laptop-phase-b/rss-complete-parity-results.json),
[freshness receipt](raw/laptop-phase-b/rss-complete-parity-freshness.json).
No desktop reference, tolerance or exclusion was changed for this memory work.
No live shell/helper/compositor was stopped, restarted, installed or driven.
