# Popup readiness and incremental-bar idle follow-up — 2026-10-23

All input/rendering checks run against private nested KWin/Hyprland, with mocked
services for popup latency and the existing read-only real-service graph for
controlled idle. No live desktop, configuration, provider or device was changed.

Latest final release: **12.59ms median /14.62ms max** input receipt to first GPU
buffer across five fresh processes; **0.2163% controlled-quiet idle CPU**,47.99%
below0.4159%. First-buffer timing excludes the declared reveal animation.

## Battery: separate transaction duration from readiness

The earlier1.19s number measured a complete functional transaction: fixture reset,
450ms settling, native press/release, duplicate-action observation windows, outside
close and400ms closing wait. It did not measure input-to-ready latency. The popup
remains latent under QUIRK_BATTERY_POPUP_NEVER_OPENS; the seeded fixture exercises
its real controller, surface and GPU code without changing that quirk.

There was nevertheless a cold-path defect. Opt-in monotonic UI-thread timestamps
showed about132ms from native release receipt to surface creation, then224ms to
first GPU-buffer submission in debug. Font wrapping's first call cost47.6ms;
text shaping/metrics and first renderer glyphs caused most of the remaining work.
No synchronous provider/history request was found on this UI path.

Fixes:

- Reuse the system Skia FontMgr initialized during startup rather than constructing
  it during font resolution and wrapping.
- Share resolved typefaces between measurement and rendering engines. Variable
  named font weights clone one bounded backing font stream by file/TTC index,
  rather than duplicating the20MiB CJK stream per weight.
- Reuse opened FreeType faces for last-glyph metrics and native masks. Size/load
  flags are explicit and the mask transform is reset on exit, including errors.
- Prepare the battery's500/600/700 native/paragraph font styles at shell startup.
  This moves font matching out of the click path. QML constructs the hidden popup
  and its BatteryInfoLine/Text objects at component construction
  (`shell.qml:1395-1412`; `docs/spec/ui-bar-popups.md` §3).

Initial five fresh release processes (initial font preparation performed normally at
startup, first battery opening in each process):

| Metric | Median | Range |
| --- | ---: | ---: |
| Native release receipt → surface creation/content prepared |2.33ms |2.02–2.66ms |
| Native release receipt → first GPU-buffer submission |10.29ms |9.33–11.56ms |

All five samples in that series are below16ms. Measurement begins when the UI
receives the native release, so it includes controller/model work, layer configure,
EGL creation and first render/swap, but excludes input transport before UI receipt.
Submission is not compositor presentation/physical display latency. Declared
staging,20ms open pause and reveal/close animations remain unchanged
(`AnimatedGlassPanel.qml:83-97,142`; `ui-bar-popups.md` §2); this does not claim a
fully revealed card within16ms.

Raw traces: `target/perf-followup/battery-latency.json` and
`target/perf-followup/battery-release-{0..4}/run-*/fixture/battery-open-profile.csv`.
The final memory-sharing build was repeated after the interactive E2E workload
stopped. Five fresh processes prepared in2.03–3.24ms (median2.76ms), and submitted
in11.28–14.62ms (median12.59ms), all below16ms. One earlier test overlapping the
interactive E2E workload prepared in3.54ms but submitted in29.36ms; its compositor/
EGL scheduling delay is retained, not hidden as a fast sample.

A separate whole-open/close trace also recorded one35.10ms EGL draw
(34.90ms inside renderer.paint) during the reveal, after the first buffer.
Thus the readiness figures do not establish that every animation frame meets
16ms; that isolated cold rendering spike remains unresolved. The trace is retained
in `target/perf-followup/battery-complete.csv`; its cause is not established.

An additional first release sample submitted in9.70ms. Intermediate debug traces
are retained with their manifests/logs; redundant debug executable copies were
removed after the filesystem exhausted space (hash ledger retained).

## Controlled-quiet idle

The same `docs/verify/measure-shell.py --quiet-agents --idle-only` case runs each
archived production release for60s after warmup. Each has zero native hook events,
private PID namespace/empty agent input, all service actors retained, a verified
6144×3456 physical/3072×1728 logical output at DPR2, and real EGL/GL rendering.

| Release checkpoint | Process CPU | Summed thread CPU | Scheduling proxy/s |
| --- | ---: | ---: | ---: |
| Historical measured release |0.4159% |0.4359% |12.71 |
| Incremental bar before text work, fresh measurement |0.5487% |0.5434% |12.77 |
| Opened-face reuse |0.2160% |0.2090% |11.08 |
| Prepared/shared-font release repeat |0.2156% |0.2365% |12.45 |
| Final shared backing streams, concurrent interactive E2E |0.2992% |0.2867% |16.67 |
| Same final release, quiet repeat |**0.2163%** |**0.2261%** |18.20 |

The final quiet0.2163% result is47.99% below the historical0.4159%, and60.58% below the fresh
0.5487% checkpoint. These are whole-binary sequential observations, not attribution
of every difference to one patch. Process CPU has roughly0.0167 percentage-point
resolution per CPU tick over60s. Thread runtime supplies a higher-resolution
cross-check. The≤0.1% full-shell idle budget remains NOT MET.

The UI-side cost was localized with an explicitly instrumented release, separately
from the uninstrumented CPU results. Inclusive wall-time scopes over60s:

| Scope | Before | After |
| --- | ---: | ---: |
| Shaping/metric cache misses |216 calls /198.22ms |212 calls /15.15ms |
| Public text measurement |109.40ms |12.38ms |
| Assembly sync |140.95ms |46.83ms |
| Renderer paint |111.41ms |24.52ms |

The largest recurring text cost was `line_metrics` opening/parsing a fresh FreeType
face for the final glyph on each new stats label; rendering repeated it, and native
masks opened another face per glyph. Reusing opened faces cuts shaping/metrics
miss time92.4%. Scopes are inclusive and overlap; do not sum them as exclusive CPU
costs. Idle wake frequency is largely unchanged: this fixes work per update.

Raw measured runs and archived-ELF hashes:
`docs/verify/raw/perf-shell/incremental-controlled-baseline`,
`incremental-face-cache`, `incremental-text-final`, `incremental-text-shared-data`,
and `incremental-text-quiet-repeat`.
Instrumented runs use the `-profile` labels; their CSVs are under
`target/perf-followup/{profile,final}` and are not treated as budget evidence.

The measurement harness initially negotiated a FALLBACK/zero-sized output and
correctly rejected that run. It now uses the existing compositor-only
`nested-flush.c` shim and private KWin sizing rule, as the functional/fullscreen
harness does. Neither software-rendering overrides nor the preload reach the
measured shell. The original resize validation remains mandatory.

## Regression checks

- 18 independent Qt text comparisons retain native exact alpha and distance-field
  tolerance. The new mixed-mask test forces fresh metric evaluation after varied
  weight/size/DPR/subpixel rasterization and cache clears; metrics and pixels stay
  equal, guarding against retained FreeType transforms.
- Ordinary owned tests: **162 passed,0 failed,22 ignored**. Strict owned
  all-targets Clippy: **PASS**. Private EGL native goldens,
  including64 color/opacity cells and the new batching material case, pass;
  all84 Canvas stroke cells pass.
- Final production-release functional input/mapping coverage: **204/204 PASS**
  (`target/perf-followup/functional-verified/manifest.json`). One earlier run
  aborted on stderr ENOSPC, not an assertion failure; its raw log is retained.

Reproduce (set HYPRV_SHELL_BINARY to the archived ELF):

```sh
python3 -B docs/verify/measure-shell.py --label REPEAT --quiet-agents --idle-only
HYPRV_UI_PROFILE=1 HYPRV_FUNCTIONAL_FILTER=battery.open-close hyprv-shell --nested-functional-test OUT
```

`HYPRV_UI_PROFILE` enables bounded in-memory wall-time scopes/events, with no timer
or per-frame filesystem writes. `bar dataStatus` resets scopes immediately before
the measurement window; `HYPRV_UI_PROFILE_OUTPUT` writes CSV on clean Engine drop.
Functional profile RPCs are read-only and do not request frames. Profiling is off
by default; instrumented runs are explicitly marked and excluded from budget claims.

The final shared-font-data build reduces startup/steady RSS from approximately
272MiB in the prepared-font prototype to205MiB at the start of the steady window
(208MiB at its end). This is a font-stream allocation observation, not proof of
long-run activity memory behavior. The font-stream change is startup-only.

## Retention and disk policy

The final verified release ELF remains at
`target/perf-followup/final-shared-font-data/hyprv-shell` (SHA256
`ba1358473290e77682228a013df2a0f047cf9bc6594fdb8cc4df6556af3b3b62`).
Superseded executable copies were deleted after verification; hashes are in
`target/perf-followup/retired-release-binaries.json`. Small traces and receipts
remain. Future Cargo jobs reuse `target` with `CARGO_INCREMENTAL=0`.

Latest engine measurements and limits: [reveal-idle.md](reveal-idle.md).
The corrected production tray path and callback/stat scheduling pass204/204
functional cases;285/285 battery reveal/hold/close frame intervals are below
16.7ms (max6.346ms), and the final controlled-quiet process measurement is0.1496%.
Broader cold-calendar rendering still reaches38.30ms and is explicitly pending.
Previous frozen ELFs were retired per disk policy; compact hash/size receipts
remain in `target/perf-followup/retired-elf-receipts.json`.
