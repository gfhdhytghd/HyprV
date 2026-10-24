# Shell memory during popup activity (svc-media, 2026-10-23)

Setup: `docs/verify/measure-shell.py` nested KWin + Hyprland (6144×3456@2), read-only release
shell from a source snapshot (10:16 EDT) with scratch-only MEMSTAT instrumentation (every 10 s:
Skia GPU resource cache, mallinfo2, renderer caches). Activity = calendar ×2 / control panel ×2
toggles every 5 s. The planned 60/660/180 s run was cut at ~340 s when the root FS filled.

| t (s, from shell start) | phase | Skia GPU cache | of which purgeable | malloc in use |
|---|---|---|---|---|
| 10–70 | idle | 83.9 MiB | 0.1 | 98 → 99.8 MiB |
| 80 | first popups | 122.8 | 39.1 | 169.6 |
| 90 | activity | 131.6 | 47.8 | 193.4 |
| 340 | activity | 131.6 | 47.8 | 196.0 |

Findings so far:
- **Plateaus** within the first popup cycle (GPU flat from 90 s; heap +2.6 MiB over 250 s).
- **Renderer caches are not the holder** (my areas): qt_rect rasters/coverage 0 B (GPU path),
  glyph atlas ≈ 37 KB, image sources ≈ 4 MB, no effect-cache entries, bounds cache cycles via prune.
- **GPU**: 84 MiB at idle ≈ one full-output 6144×3456 RGBA target already exists before any
  popup. After popups, 47.8 MiB stays as *purgeable* Skia resources (freed render targets of
  closed popups kept under the default 256 MiB budget) → request ENG-MEM-1.
- **Heap**: +95 MiB at the first popup open is outside all instrumented caches (candidates:
  NVIDIA driver allocations for new EGL surfaces/textures, Skia CPU-side GPU bookkeeping, panel
  asset decode). Attribution pending: an LD_PRELOAD live-heap profiler run timed out on shell IPC.

Next: heap attribution (profiler with sampling instead of every ≥64 KiB backtrace), then a full
10+ min run incl. 3 min closed phase, after ENG-MEM-1.

## Engine handoff: calendar budget,18:28UTC

[Calendar memory report](calendar-memory-budget.md): limited warmup + shared FT
file backing + no synthetic wrapped-paragraph preload now312.2MiB initial and
339.4MiB after calendar/control BOTHclose. Budget remains FAIL by19.4MiB even
before the broader all-popup workload.38/38 render frames pass16.7ms,max11.565ms.
Private smaps identifies repeated CJK mappings and qtext's retained~20MiB font
buffer as the next concrete consolidation request. PSS is lower; budget is RSS.
Engine preserved your mapped_typeface and qtext edits; lifetime regressions and
strict scoped Clippy pass. Exact ELF/hash/raw paths are in the linked report.


## Update: plateau, attribution, fixes (svc-media, 2026-10-23 afternoon)

**Plateau (15 min, binary from 13:07 snapshot, 60 s idle / 660 s activity / 180 s closed, 132 toggles):**
RSS 318 → 497 MiB at the first popup cycle, then 497 → 507 MiB over 13 min (≈0.8 MiB/min creep),
**no return** after all popups close. GPU (nvidia-smi) 306 MiB idle, 358–438 while popups come
and go, 358 closed.

**Attribution** (`LD_PRELOAD` live-heap profiler, allocations ≥ 64 KiB with stacks,
`addr2line` on the unstripped release binary; scratch tool, not committed):
- font file bytes: 19.66 MiB HarmonyOS_Sans_SC ×6 (+ 2.46 MiB ×6): hyprv-ui text engine ×2
  (Skia `new_from_data` copies), **panels `qtext::face` ×4 (one per weight)**;
- engine native-GL text context creation 24 MiB (324 driver allocations ≈ shader compile);
- 14 MiB without frames (driver); everything else < 5 MiB. Renderer caches (qt_rect, images,
  glyph atlas, effects) are small and bounded (MEMSTAT, first section).

**Fixes:**
- panels `qtext.rs` (minimal, spec-bar notified): one shared `Rc<Vec<u8>>` per identical font
  file (named instances only differ in face index) → 4 copies → 1 (−59 MiB heap).
- hyprv-ui `text.rs`: typefaces over a memory-mapped font file (`mapped_typeface`, no heap
  copy); engine extended it (`mapped_font_bytes`) so FreeType faces of every weight share the
  one mapping. Island parity: 380/380 captures byte-identical with mmap vs heap fonts.

**RSS against the 320 MiB budget** (nested 6144×3456@2, 30 s idle / 120 s activity / 60 s closed):

| binary (source snapshot) | idle | after all popups | after close |
|---|---|---|---|
| before (13:07, perf-shell activity case) | 317.5 | 497 | 507 |
| font fixes (14:14:08) | 352–360 | 394.6 | 394.7 |
| latest incl. engine mapping + calendar prep (14:53:15) | 310–318 | **351.6** | **352.3** |

Latest breakdown: file-backed 123 MiB (HarmonyOS_Sans_SC **85.5**, libnvidia-gpucomp 36.6,
/dev/nvidiactl 22.6, binary 21.6, libnvidia-eglcore 13.6), heap 80, NVIDIA anon 75, anon 61.

**Remaining to reach ≤ 320 MiB:** the HarmonyOS file is still ~4× resident: panels `qtext`
matches fonts through Skia's fontconfig manager (a mapping per weight, fully touched by
`to_font_data`) and keeps a 20 MiB heap copy → PANELS-MEM-1 (expected −60…80 MiB). GPU-side
purge on popup close → ENG-MEM-1. Driver/library mappings (≈ 95 MiB) are not reducible
without fewer GL programs (ENG-MEM-2).

## Engine final memory checkpoint (ENG-MEM-1/2)

[Engine report](engine-memory.md), pinned release ELF81ed2a60ac90, private
6144×3456@2 compositor, read-only real services,40s idle /40s activity with two
calendar/control cycles: **203.1MiB RSS /40MiB GPU idle;205.2MiB RSS /38MiB GPU
after both close**. Sampled activity peak RSS207.2MiB.85/85submitted frames pass
16.7ms,max9.439ms (inclusive engine.paint9.952ms); no effect compile in activity.
Closed RSS is114.8MiB below320MiB. This is bounded two-cycle evidence, not a new
15minute all-popup soak.

The idle GPU jump was dominated by the host's full-output corner overlay:81MiB
perRGBA target before EGL swap/stencil buffers. Four tiny host crops of the
canonical corner nodes restore the old GPU footprint without changing pixels.
Keep closed-target scratch purging (also preparation/shrink), avoid redundant
retirement submission, and compile native/effect programs only when used.
Needed masked control material is exercised before input, with a GPU regression
asserting it really ran (transparent warmup content would be culled).

RAM improvement includes svc-media's `mapped_font`/panels qtext sharing over the
engine mapping; public `font_mapping` also exposes file/index/bytes. Candidate
smaps CJK residency6.85–6.98MiB vs85.5MiB in the older report. Do not attribute
the combined RAM reduction entirely to shader changes. Matched before run was
312.4/341.3MiB RSS and307/305MiB GPU idle/closed; historical358closed was already
not reproduced with the existing EGL Drop purge.

Canonical GL controls unchanged:304exported bar images exact,380island exact,
panels298/300 with the same two known max1 QA failures. CPU/private-GL corner
composition matches full-output rendering incl.actual6144×3456@2;10surfaceless+
3privateEGL regressions and strict workspace all-target Clippy pass. Native
outputs add/remove/reorder/scale1→1.5→2 pass7/7 after fixing test hover setup.
