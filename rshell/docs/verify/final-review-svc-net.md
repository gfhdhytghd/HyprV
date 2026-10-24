# Independent final cutover review — svc-net

**Verdict: NO-GO for cutover.** The frozen candidate builds and its ordinary, GPU and functional tests pass, but the verified config lost-update race, strict corner parity failure, incomplete readiness evidence and remaining live-action validation gaps prevent acceptance. This review is independent; I did not read or coordinate with the other final reviewer.

Fresh snapshot: `/tmp/hvfn-rjzg8a6u/rshell`; build-source SHA-256 `76239197220d07fc4a5d0517fa3c044267401b1267c1f1cfadc26f5eee11b461`. Read-only checkout except this report; all generated verification files are isolated.

## Verified findings during independent execution

### FR-NET-1 [P2] Hook installer still loses a non-cooperating writer's final edit

Locations: `crates/hyprv-svc-desktop/src/util.rs:160` (content check), `:165`
(unconditional `renameat`); `agent_island/install.rs:380`–`:393` (backup and
commit), `:436` (cooperating-installer lock). The content check and replacement
are separate syscalls, not an atomic compare-and-swap. A Claude/Codex process
can update settings after the check and before rename. Installation returns
success, overwrites that change, and its already-created backup contains the
older pre-edit document. Existing tests inject concurrency before `commit`,
which does not exercise this last window.

Fresh probe: a scratch Rust program compiled the exact current `Expect` through
`replace_checked` source excerpt. A C `LD_PRELOAD` `renameat` interposer writes
`concurrent_user_setting=must-survive` to the disposable settings file just
before executing the real rename. The interposer confirms injection on stderr;
the helper returns 0 and the final JSON has only `original=true,installed=true`.
The no-injection control succeeds. uid=1000, only `/tmp/hvfn-rjzg8a6u/evidence/`
is written; no real user settings are modified. Current util source SHA-256:
`8ee70d08a91544b8c6f92f9a698052eac04998dd7a5433491c88931f3084fd0a`;
verbatim excerpt SHA-256:
`94aed03bcd57120562643bd64514a89b96f91e36ede5a7c9f2e542feb6d06efc`.
Full-installer confirmation also reproduces this using the actual fresh-built `hyprv-agent-hook install`: exit 0, `installed=claude,codex`, injected concurrent setting absent from both final settings and the pre-edit backup. Its ELF SHA-256 is `34d84c76ab3c69fccafce85b5a39ec33c8083bc7a8b65556ca3a64724d50926d`. Control and race use separate fake HOME/CODEX_HOME/XDG directories. Receipts: `evidence/hook-race-result.json`, `hook-race-probe.py`, `rename-race.c` under the snapshot parent. This reopens the final replacement window of DESKTOP-1; the existing earlier-window conflict checks do work.

Suggested fix: do not claim arbitrary external writers are protected by this
check-then-rename. Use a protocol shared by all writers or an atomic exchange
that durably preserves the actually displaced generation and reports/rebases a
conflict without silently losing it. At minimum, preserve every displaced
version and stop for conflict resolution; a stale pre-check backup is inadequate.
Test a writer specifically between the final check and the replacement.

### FR-NET-2 [P2] Readiness parity gate consumes a different backend/result policy

Locations: `cutover/preflight.py:114`–`:121` and `:40`–`:49`;
`parity/verify/verify.py:536`, `:552`–`:556`. Preflight selects plain component
slots (`bar`, `island`, `panels`) while a production GL run writes `*-gl` slots.
It also requires every canonical manifest entry to be `pass`, whereas the
verifier explicitly marks the unreachable battery fixtures `excluded`, and the
bar exporter explicitly delegates eight island/calendar host-composition images.
Thus a fresh GL exporter pass cannot produce READY with this consumer. A direct fresh `parity_gate` probe rejects the unmodified result with `island: no successful nonempty render`; aliasing only GL slots in memory then rejects `battery-charging--dark@1.png`. The probe never rewrites the result file or accepts the exclusions.

Fresh GL run: island 380/380 pass; panels 300/300 pass. The bar-owned exporter
reports 304/304 exact. The shared verifier reports bar 292 pass, 12 unreachable
battery exclusions, eight missing foreign host images. No remaining exported bar
pixel mismatch was observed. These are **coverage/gate-wiring failures**, not
proof that those eight assembled states have broken runtime implementations.
The full-screen rerun below independently checks assembly.

Suggested fix: gate the production GL results with full dependency freshness,
use the documented unreachable-state policy consistently, and assign delegated
host captures to an explicit assembled-scene gate. Never silently accept missing
images or substitute the older tolerance for non-text exactness.

## Initial preflight and documentation observations

The live checkout's read-only `cutover/preflight.sh --json` returned NOT READY:
release source receipt stale; automated-test receipt stale; island render stale;
performance source receipt stale; `/usr/local/lib/hyprv-cutover/installed.json`
missing. A fresh staged build in the review snapshot succeeded with release id
`993b9ea84eb5cea3dae25d4bb2fc746aaea717aa8d18516b82ccd0f8f4d0b12f`.

The first snapshot suite exposed a review-snapshot omission: tests include
`docs/reviews/scratch/flickable-implementation/cases.rs` and read its CSVs, plus
`panels-flickable` JSON/CSV fixtures. These 214 small input files were copied,
hash-verified and admitted before the final rerun; the initial missing-file
failure is **not a product failure**. This also reveals that `build.sources()`
does not fingerprint those actual test inputs; a current test receipt cannot
detect their subsequent alteration. Move permanent regression data under the
crate tests tree or include these dependencies in receipt fingerprints.

README's parity/performance numbers and peripheral-battery mismatch are stale
relative to current source: battery selection now uses legacy BAT* directory
semantics (`battery_history.rs:218`–`:225`) with a regression test. README and
CUTOVER.zh-CN.md correctly say NOT READY and require explicit apply, but the
Chinese preparation sequence requires READY before installing while default
preflight unconditionally requires an installed helper (`preflight.py:146`).
That bootstrap sequence cannot succeed literally on a fresh machine. Distinguish
pre-install checks from the mandatory installed check immediately before switch.



## Fresh execution and provenance

Reviewed the snapshot taken at epoch 1792790838 (2026-10-23), not a moving shared-target executable. `snapshot.json` records identical before/after/copied build-input hashes, plus QML/reference hashes. Production build: `packaging/build-release.sh`, release id `993b9ea84eb5cea3dae25d4bb2fc746aaea717aa8d18516b82ccd0f8f4d0b12f`; staged shell SHA-256 `f49780190bd8945c39200abf3b94fee8c0511cb4d76bbae60fbc048de29ce67f`. Every shell integration/measurement invocation uses that staged ELF. The shared target can change under other agents and is never treated as the pinned release.

| Fresh check | Result | Boundary |
|---|---|---|
| Release build and staged manifest | PASS | Source and binary digests verified; no installation |
| `cargo test --locked --workspace --all-targets` | 569 pass, 49 ignored, zero failures | Opt-in/device cases are not ordinary passes |
| Workspace doctests | 2 pass, 1 ignored | No device claim |
| xtask tests | 1 pass | Separate manifest |
| Cutover/installer tests | 54 pass | Disposable filesystem/mocked services, not a real switch |
| Probe Python tests | 4 pass | Harness tests |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | PASS | Fresh source, no suppressions added |
| Explicit renderer GPU/EGL launcher | 20 pass | 14 surfaceless renderer, one partial-frame, three private-Weston cache, two resize/fault-recovery checks |
| Native nested functional replay | 204/204 PASS | Native input/local IPC; exact ordered mock service-command sink |
| GL component parity | island 380/380, panels 300/300; bar 292 pass / 12 excluded / 8 missing | Bar-owned export independently 304/304 exact; foreign-host coverage explained in FR-NET-2 |
| Assembled full-screen capture | 26/26 jobs, 64/64 broad image comparisons PASS | Light/dark, actual discovered DP-4 @2, 6144×3456; animations sampled |
| Exact unmasked assembled corners | **0/256 exact; FAIL** | All four corners of every capture have max channel difference 4/255 |
| Build/install/switch/rollback plans | All seven dry-run commands exit 0 | No apply, no service reload/start/stop, no real sysfs writes |
| Hypr cutover patch | `git apply --check` PASS | Against copied sibling config only |

The minimal initial snapshot omitted test-only golden inputs in scratch, experiments/text and crate out trees. After copying and hash-checking 214 scratch inputs, 36 Qt fixtures and 72 out-tree references, the complete suite passed. Those initial missing-fixture runs are discarded as snapshot setup failures. Additional input receipts are retained separately because the production source digest does not cover all of them.

### FR-NET-3 [P2 acceptance blocker] Exact non-text assembled corner parity still fails

Fresh `verify_corners.py` exits 1: 64 captures, 256 corners, no masked pixels, maximum difference 4/255. Every corner fails exactness. Example: `calendar--dark@2.png`, top-left `[0,0,54,84]`, 48 different pixels, maximum 4. This is not a catastrophic visual defect, but the non-text exactness requirement in ARCHITECTURE.md does not permit the global text tolerance to excuse it. Broad full-screen comparison passes all 64 images (maximum 6/255, worst painted MAE 0.0165, minimum IoU 0.99987); that is a different gate.

Relevant paths: `crates/hyprv-shell/tools/verify_corners.py:24`–`:60` (unmasked crop and exact predicate), assembled corner construction in `crates/hyprv-shell/src/assembly.rs:1489`, QML `ScreenCornerShade.qml` / `shell.qml:3467`–`:3546`. `host.rs:1457` contains a separate corner helper; this review does not assign the measured production residual causally to that helper. Reproduce with the staged shell's `--nested-test` fixtures/references, then run both `compare.py` and `verify_corners.py`; fix the actual assembled rendering/composition arithmetic and require exact crops before closure. Do not increase tolerance or mask the corners.

Full-screen coverage is the current output's @2 matrix, not proof of every monitor/driver/fractional scale. Component fixtures cover their declared scales. The residual/matrix script's tray-masked diagnostic is not acceptance evidence.

## Feature coverage and remaining integration limits

I traced the runtime/dead distinction in `docs/spec/COVERAGE.md` against the shell assemblies, service adapters and fresh functional IDs. A native path exists for the listed runtime surfaces and the bind-facing IPC; I did not verify an outright absent runtime QML component. That is narrower than claiming every provider/action is proven equivalent.

| QML/spec family | Native evidence | Missing or mock-only proof |
|---|---|---|
| Bar pills, workspaces, title, tray, overflow, battery, notifications, corners | Fresh seat-input replay, typed service mapping, GL fixtures and assembled captures | Tray menu activation/notification mutations and workspace dispatch captured before execution; physical laptop battery branch absent on this desktop; exact corners fail |
| Island hover/press/drag/seek/agent modes/spectrum | Fresh replay and all 380 GL samples; actor/model regressions | Populated media/spectrum and Cider external lookup not established by empty/read-only live provider states; real agent focus actions mocked |
| Control, Wi-Fi, Bluetooth, quick-adjust, stats, fan sliders | Fresh replay, 300 panel GL samples, native service paths | Wi-Fi joins/secrets handoff, pairing, audio/backlight/fan writes, profiles and recording are not live-executed; synthetic slider/service tests prove payload/order only |
| Week calendar, calendar morph, startup confirmation | Fresh calendar RPC reads, calendar replay/captures; startup code and golden/test paths present | Real calendar UI activation/change and login approval-file handoff not exercised end-to-end in a real session |
| VM, Viewflow, theme, launchers, IR, eGPU, demo | Typed adapters, source routing, regression tests; VM auth/release ordering test passes | Real polkit/unit/portal/serial/host-switch/theme/app processes and eGPU removal remain unexecuted; no hardware safety claim |
| `bar`, `controlPanel`, `calendar`, `quickAdjust` IPC | Fresh local socket calls, keyboard/pointer cases, quick-adjust variants, kill/restart/output lifecycle | Real Hyprland binds deliberately not dispatched; copied six-file patch applies cleanly |
| Native `desktop setVariant` / `deskflowReflow`, rotation script integration | Fresh ordered-command IPC cases for v1/v2/v3/bt and reflow; source/patch trace | Host topology and installed script execution not mutated |
| Legacy unreachable/dead components | COVERAGE's distinction retained: Mihomo page has setup-only tests; DeskflowTargetControl, Wi-Fi details, eGPU confirmation/power expansion have no live legacy caller | Do not count setup-only pages as reachable legacy features or mark dead components as missing runtime behavior |

Fresh service comparisons: **3502 PASS, 16 UNAVAILABLE, 98 TEMPORAL, 48 MISMATCH**. Counts are fields/records, not percent feature coverage. All 48 mismatches belong to agent island: three project labels and nine absent synthetic sessions (five fields each). The fresh probe's `desktop()` subscribes to the agent topic but never acquires the separate `scan_lease` (`crates/hyprv-svc-probe/src/main.rs:175`–`:222`); legacy `agent-island-status.py` performs a process scan. The shell does acquire that lease while Island is visible (`crates/hyprv-shell/src/services.rs:475`–`:485`). Therefore these numbers do **not** verify a shell discovery regression: they expose an invalid discovery-equivalence setup. The hook/process scan can also change project CWD labels. Fix the probe's lease/visibility contract and compare aligned frozen inputs before declaring these differences fixed or harmless. TEMPORAL and UNAVAILABLE remain unproven, not passes. Live provider reads are sequential, not atomic.

Read-only gaps explicitly include fan helper absent, laptop battery unavailable, legacy brightness action deliberately not invoked, spectrum capture disabled, weather credential/dummy branch only, action-only eGPU/startup operations, real authorization/reconnect/suspend and populated-provider transitions. Service regression tests cover many branches using private buses; the 204-case end-to-end harness omits real service actors and uses its command sink. These two evidence layers must not be conflated.

## Safety and prior-review disposition

The current full suite executes the cooling adversarial regressions, network/secret and panel regressions, media owner/generation regressions, desktop filesystem/process/icon regressions, core/shell lifecycle checks and cutover recovery tests. Source inspection confirms the corresponding fixes remain present. This is not permission to skip installed/physical validation.

| Prior findings | Current disposition in this snapshot |
|---|---|
| PRIV-1..6 / POWER-1..3 | Root-only persistence and singleton state, cooling-first startup, independent channels, emergency override, teardown failsafe and authoritative packaging are present; regression tests pass. Physical firmware/thermal response unverified |
| NET-1..8 / BT-1, PANEL-1/2 | Zeroizing action/secret storage, masked cache keys, NM secret flags and explicit authorization/cancel paths present; tests pass. No real credential-store, join or pairing mutation |
| MEDIA-1..5 | RAII worker quit/join, position-anchor generations, menu generations/serials/revisions, unique-owner signals and epoch snapshots present; corresponding fresh regression tests pass |
| DESKTOP-1..7 / DESKTOP-R2-1/2 | Earlier conflict/backup, shell argument, hook validation and incremental-process safeguards present; **DESKTOP-1's final syscall window remains open as FR-NET-1** |
| Core/sys, UI/UI-R2, shell/widgets and their R2, bar/island/panels | Existing regression tests and source guards remain; explicit GPU tests pass. No additional verified security/lifecycle regression from this round; visual acceptance gaps remain separate |
| CUT-1..7 / CUT-R2-1..6 | Fresh 54-case suite and plans pass; shared lock ordering, interrupted commit recovery, confirmed native-stop before fallback, retryable rollback and bounded readiness are present. Real apply/root recovery intentionally unexecuted |

I found no newly verified unresolved P1 in this scope. FR-NET-1 is a verified P2 data-loss issue; FR-NET-2 is a P2 readiness-contract issue; FR-NET-3 is an explicit parity blocker. The historical SOAK-1 5 MiB retention remains an attribution/repeat-workload question, not proven unbounded leakage. Owner closure notes alone were not treated as executed proof.

Root helper checks: all mutating D-Bus methods authorize the actual unique sender through polkit; eGPU requests validate a BDF, derive/revalidate the sysfs plan, and accept no arbitrary path. Cooling has no user-writable state input that bypasses authorization. The systemd unit runs independent ExecStartPre/ExecStopPost cooling, uses a watchdog, root-only state and restricted filesystem access. Unit/policy source and fake-sysfs failure tests are evidence; actual systemd sandbox, system-bus policy/polkit and crash/hot-sensor hardware behavior are **UNVERIFIED**, because this review never installs or runs as root.

Cutover safety scope: build, both installers, user/system switch and user/system rollback default plans returned zero; patch check succeeded on the copied configs. I did not execute any printed sudo command, real apply, live reload, live action or root helper. The only hook install probe ran unprivileged against disposable homes. Existing user configuration and the running Quickshell/Hyprland were not changed.

## Non-installing preflight, gate by gate

Both final commands exit 1 / `ready:false`. This is expected for an uninstalled candidate and must not be bypassed.

| Gate | Fresh candidate `--json` | Fresh candidate `--installed --json` |
|---|---|---|
| Release binaries | PASS: candidate digests verified | FAIL: `~/.local/bin/hyprv-shell` absent |
| Automated tests | PASS: all five configured commands current and successful | PASS |
| Parity | FAIL: `island: render evidence stale` | Same |
| Performance | FAIL: `real-shell performance source evidence stale` | Same |
| Privhelper installed | FAIL: `/usr/local/lib/hyprv-cutover/installed.json` absent | Same |

The moving checkout's initial read-only preflight also rejected its release and test receipts as stale. The snapshot rebuild/rerun makes those two gates pass; it does not make installation or visual/performance gates pass. The fresh measurement below is independently reported, not substituted for a complete eligible full-host `perf-shell-latest.json`. FR-NET-2 explains why the fresh GL result is not consumed by the existing gate. In addition, parity freshness currently examines each UI crate's mtime and ignores the verifier's shared engine/widget dependency stamp; fix that too rather than merely aliasing slots.

## Documentation accuracy

README and CUTOVER.zh-CN correctly say NOT READY, default to plans, require explicit apply and staged root-owned/hash-verified deployment, and describe the user/system split. Their status must remain red.

Required corrections before following the guide:

- Resolve the Chinese preparation-step 2/install-step 4 bootstrap contradiction described above; keep the installed preflight mandatory immediately before switch.
- Replace README's old bar 228/304, panels 64 exact and island 0 exact numbers with current results and the exclusions/delegation/strict-corner qualifications. Show the production `--backend gl` command and make its consumer consistent.
- Replace the old 0.35%/2.66% CPU summary with explicitly scoped, source/ELF-pinned current measurements. README's service-only wording differs from the real-shell preflight's whole-process <=0.1% requirement; document both scopes clearly.
- Remove the now-fixed peripheral battery mismatch from open items; preserve unavailable laptop hardware validation. Fresh observed BAT* selection matches legacy.
- The test command comment says 50 cutover tests; the fresh suite has 54. Do not call ignored opt-in cases tested.
- Keep current-state visual, benchmark, soak and installation claims tied to their own binary. The old 62-minute soak is not a soak of this release.

No implementation or documentation fixes were made during this review, apart from this report.

## Performance, memory and stability

### Fresh pinned-release measurement

Ran the repository's `measure-shell.py` with the staged production ELF, `--seconds 60 --quiet-agents`, idle **and** activity windows in private KWin + Hyprland, observing the real Quickshell family concurrently. The wrapper changes only binary/output selection and the lookup of the actual live Quickshell environment (the snapshot path cannot identify its argv); it retains the script's five-key environment whitelist, read-only shell flag, all service actors, empty hook fixture and private PID namespace. No profiling/preload instrumentation is added to the shell. Our builds and image comparison jobs had ended before measurement. This remains a busy shared host, not a lab-isolated machine.

| Fresh case | Seconds | Native process CPU | Native precise thread CPU | Quickshell process CPU | Native RSS start/end | Native GPU start/end |
|---|---:|---:|---:|---:|---:|---:|
| Controlled quiet idle | 60.138 | 0.1829% | 0.1995% | 0.2162% | 143.94 / 145.81 MiB | 40 / 40 MiB |
| Controlled popup activity | 60.093 | 0.4992% | 0.4899% | 0.2330% | 144.71 / 211.97 MiB | 40 / 39 MiB |

Idle per-thread precise CPU (% of one core): UI `hyprv-shell` 0.1751%; `hyprv-services` 0.02228%; `hyprv-agentproc` 0.00215%; PipeWire/module threads 0. No exited/new native threads. The scheduling proxy is 12.39 timeslices/s, not an exact wakeup count. There are zero fixture hook events but 48 live-host hook events: native and legacy therefore have different agent inputs. Native process CPU comes from coarse 10 ms proc ticks; precise thread CPU sums schedstat deltas. Both exceed the **whole-shell** 0.1% target, while the service-only budget is met in this controlled case. This is not causal proof of a regression relative to earlier timings and is not an eligible full-host idle PASS.

Activity performed 12 successful calendar/control toggles with zero fixture hook events, 70 host hook events, and no new/exited native threads. Peak sampled native RSS was 212.56 MiB; both popups were closed at the endpoint. The simultaneous observed Quickshell family used ~381 MiB RSS and 451 MiB GPU accounting throughout, but its UI was not driven through the same workload: this is not a like-for-like activity speedup comparison. Opening only calendar/control is not the required all-popup working set. RSS here is not PSS; NVIDIA accounting includes driver allocations/reservations and is not a live-object leak measurement. The test does not infer zero wakeups from a zero CPU tick or certify every provider under quiet fixtures.

### Historical evidence, with its actual limits

- `ui-tick.md`: the later owner checkpoint reports controlled-quiet whole-shell precise CPU 0.14165%, UI 0.11830%, services 0.02335%, ending RSS 174.41 MiB. Different ELF/workload/time; not substituted for this fresh result. The older `perf-shell.md` selected checkpoint reports controlled quiet 0.2201% precise CPU, host discovery ~0.9953%, live hooks 1.0246%; no eligible full-host <=0.1% proof is current.
- `engine-memory.md`: post-cropping/lazy-shader candidate returned to ~205.2 MiB RSS, GPU 40→38 MiB after calendar/control activity, peak RSS ~207.2 MiB. The former full-output corner allocation was attributed and removed. That supports the optimization, but does not establish current steady RSS after **every** popup. Fresh idle and two-popup RSS are below the same-window Quickshell RSS; a full fixed working set is still required by ARCHITECTURE.md.
- `soak.md`: historical pinned ELF `a06f20239891…` ran 3720 seconds, 534 successful replay steps and 20 output cycles. No crash, prolonged submission stall or unbounded fd/thread/timer growth was observed. It retained +220 KiB private dirty and +5 MiB NVIDIA accounting in comparable warmed phases, later plateauing. Old RSS 575→564 MiB exceeds today's memory requirement but predates the memory fixes. Neither that old failure nor the newer short improvement may be projected onto this release without a repeat.
- `gpu-corruption.md`: historical resize/backbuffer defect was reproduced and repaired; stress evidence reports 21,952 submissions / 17,952 CPU comparisons. Fresh GPU regression launcher passed all 20 cases. This is useful short regression evidence, not a new hour-long, current-binary stress/thermal/suspend test.

**Remaining acceptance work:** whole-shell <=0.1% eligible idle evidence; same-binary all-popup memory working set; repeat long workload after warming that fixed set to resolve SOAK-1's retained bytes and confirm the current renderer under output cycles. Do not label the 5 MiB observation an unbounded GPU leak without attribution.

## Evidence retained and acceptance actions

Compact execution receipts, transcripts, exact failing-probe sources and the example actual/reference corner PNGs remain at `/tmp/hvfn-rjzg8a6u/evidence/` (about 9 MB). `retained-sha256.json` fingerprints them. Useful entries:

- `receipts/build.json`, `MANIFEST.sha256`, `snapshot.json`: source/staged-artifact provenance;
- `receipts/tests/`, `clippy.log`, `gpu-regressions.log`, `receipts/functional.json`: executed checks;
- `receipts/gl-results.json`, `bar-export-coverage.json`, `fullscreen-comparisons.json`, `corners.json`, `corner-example-{reference,actual}.png`: unmasked visual results;
- `hook-race-probe.py`, `rename-race.c`, `hook-race-result.json`, `cas-probe.rs`, `cas-result.json`: two independent reproductions of the last-window lost update;
- `preflight-parity-probe.py` / `.json`, `preflight-final.json`, `preflight-installed-final.json`: observed gate failures;
- `receipts/equivalence/`, `receipts/performance/`, `performance-summary.json`, `fresh-performance.py`: fresh comparison and same-window process/thread/GPU counters.

Per Disk policy I removed this review's frozen source tree, copied executables, disposable homes and bulk raw captures after retaining those receipts. I did not remove other agents' work or the shared Cargo target. The reports describe the pinned snapshot, not later concurrent changes. The tray-masked residual helper imports an absolute checkout path; its diagnostic table is deliberately excluded from acceptance, which uses the frozen comparer, engine manifests and unmasked corner check.

Before GO: close FR-NET-1 with a last-window regression, close exact corners, repair the GL/delegated-image readiness contract, produce current eligible performance/all-popup-memory evidence, correct the guide's bootstrap ordering, and complete the installed validation gates in the separately authorized deployment phase. A current-binary long soak and physical polkit/fan/suspend/provider validation remain explicit risks rather than inherited passes. No root installation or live cutover is authorized or attempted by this report.
