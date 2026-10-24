# Engine memory lifetime and native GL (2026-10-23)

Setup: `docs/verify/measure-shell.py`, private KWin + Hyprland,
6144×3456@2, release shell, `--quiet-agents`, read-only production services.
The live session is observed only. Measurements use pinned executables; GPU is
per-process NVIDIA `nvidia-smi -q -x`, RSS is resident memory (not PSS).

## Changes and attribution

- The assembled host mounted `bar::surfaces::corners` as a full-output overlay.
  One 6144×3456 RGBA8 target is 81 MiB, before EGL swap buffers. This accounts for
  the bulk of the 38→306 MiB idle GPU jump.
  `assembly::corner_surfaces` now crops the canonical corner children to four
  tiny passthrough Overlay surfaces, as explicitly permitted by ui-bar.md §9.1
  (`shell.qml:3467–3546`). Origins/extents align to whole device and logical
  pixels, retaining the original fractional animation offset within the scene.
  Top surfaces reserve 42 logical pixels for the entire reveal motion and never
  resize during reveal. The component's source pixels are unchanged.
- ENG-MEM-1: preserve the existing EGL-window Drop scratch purge; submit queued
  work in the actual paint/preparation paths, purge after temporary preparation
  targets are dropped, and purge retired targets for direct in-place shrink users. Production size changes
  recreate the EGL window and therefore also run Drop. Scratch-only eviction
  retains other outputs' live targets, glyphs and programs. The historical
  358 MiB closed plateau was **not reproduced** in the matched before run: it
  already returned to 305 MiB after both popups closed.
- ENG-MEM-2: creating a native GL context compiles **zero** shaders. Mask/image/
  rectangle programs are independent, and each of the four effect variants is
  compiled only when used. Effect creation previously compiled all four.
  Production preparation exercises the actual reveal materials before input.
  The first candidate exposed a 12.060 ms masked-compose compile on first control
  reveal; preparation now exercises that specific material as well. Unused
  variants remain lazy. Shader source/arithmetic and PNG conversion are unchanged.
- Public `text::font_mapping` returns the selected file, full named-instance index
  and shared process-lifetime bytes. svc-media also added `mapped_font` including
  a shared Skia typeface and switched panels qtext to it. Those concurrent font
  changes are included in the candidate and credited here: the total RSS saving
  must not be attributed solely to lazy shaders or GPU target cropping.

## Initial matched checkpoint

| Pinned build | idle RSS | closed RSS | idle GPU | closed GPU |
| --- | ---: | ---: | ---: | ---: |
| before, f9c3ad983b6a | 312.4 MiB | 341.3 MiB | 307 MiB | 305 MiB |
| candidate, 46687969da4a | 202.2 MiB | 205.7 MiB | 40 MiB | 39 MiB |

Twenty-second idle and twenty-second activity windows; activity opens calendar,
closes calendar, opens control, closes control at 5 s intervals. Both are closed
at the endpoint. The candidate's 47 submitted frames are all below 16.7 ms,
max 16.616 ms. Inclusive engine.paint reaches 16.923 ms; this prompted the additional
masked-compose preparation described above to improve the margin.
The final post-preparation release measurement supersedes this checkpoint.

## Final release result

Pinned ELF `81ed2a60ac9006b67597970637af6e09548534919a0ff9537ee85aba4a96ec2a`,
`target/parity-rounding/mem-eng-complete/hyprv-shell`. Forty-second idle and
forty-second activity windows, eight toggles / two complete popup cycles; both
calendar and control are closed at the endpoint.

| Phase | RSS | GPU |
| --- | ---: | ---: |
| idle endpoint | 203.1 MiB | 40 MiB |
| activity peak sampled RSS | 207.2 MiB | not sampled continuously |
| after both popups close | 205.2 MiB | 38 MiB |

This is 114.8 MiB below the 320 MiB RSS budget at the closed endpoint. It is a
bounded calendar/control workload, not a new fifteen-minute all-popup soak.
The older matched baseline was 312.4 / 341.3 MiB RSS and 307 / 305 MiB GPU at
idle / closed. The GPU reduction is 267 MiB in both phases and restores the
historical approximately 38 MiB closed footprint.

All **85/85 submitted frames** are below 16.7 ms: **max 9.439 ms**. Inclusive
engine.paint max is 9.952 ms; shell.sync max 4.020 ms. There are **zero effect
program compilations during activity**. Window Drop max 3.343 ms, scratch purge
2.219 ms, EGL surface destruction 1.144 ms, creation 0.832 ms. Preparation now
actually executes the masked material (nontransparent synthetic content).
Retirement uses the already-submitted paint work without a redundant shared
context submission. A prior candidate's single 51.746 ms resize outside draw
was not reproduced in the final run; no causal claim about external driver
contention is made. The diagnostic debug run separately had 84/84 frames pass,
max 12.836 ms, retirement max 2.257 ms.

The shared-font candidate's smaps receipt has only 6.85–6.98 MiB resident in the
HarmonyOS CJK file and approximately 53 MiB heap; svc-media's older report had
85.5 MiB of CJK-file residency. These are distinct snapshots, not a controlled
estimate of shader-only savings.


## Verification

- CPU and private production EGL/GL cropped-vs-full-output comparison is byte
  identical at DPR 1/1.25/1.5/2 and actual 6144×3456@2, at three reveal states.
  The test also asserts stable native geometry across the animation.
- Ten surfaceless NVIDIA rendering regressions and three private Weston EGL tests
  pass. Tests verify zero programs at context creation, per-variant lazy compile,
  reuse/deletion, release of a closed 4 MiB scratch target, and live-target pixels.
  A preparation regression asserts that the masked compose program is populated before input (transparent synthetic content is culled).
- Canonical `parity/verify/verify.py --backend gl`: all 304 exported bar images exact;
  island 380/380 exact; panels 298/300 pass, unchanged QA show-t0120 dark/light@2
  max 1 pair. Aggregate bar is 292 pass + 12 excluded + 8 missing host-scene exports;
  this is not a pixel regression. No references, masks or tolerances changed.
  Subsequent material-preparation change is startup only; offscreen parity does
  not call `prepare_renderer`.
- Quiet native regression: 10 samples / 10 projections, zero repaints, zero metadata
  UI wakes, one dispatch/voluntary switch for the RPC boundary over 10.1 s.
- Native output add/input/reorder/remove and scale 1→1.5→2 pass 7/7 including shell kill.
  The scale test now parks the pointer and resets hover popups before mutation;
  otherwise remapping the previous pointer can open stats during its 800 ms wait.
  The original 204-case matrix was not rerun in this memory checkpoint.
- Strict workspace all-target Clippy passes; affected library tests 170 pass.
  The newly added preparation test is explicitly exercised by the private EGL
  launcher; ordinary library runs keep GPU/display diagnostics ignored.
  The final focused corner test passed again.

Compact receipts: `target/parity-rounding/mem-eng-*-summary.json`, GPU/unit logs,
`mem-eng-source-checkpoint.json`, `mem-eng-idle/manifest.json`,
`mem-eng-outputs-fixed/manifest.json`; matched raw service/perf JSON under
`docs/verify/raw/perf-shell/mem-eng-before/`, `mem-eng-after/`, and
`mem-eng-complete/`. The complete summary includes retirement-phase timings and
zero activity shader-compilation count.
Old engine frozen executables and nested test copies were removed after SHA receipts.
