# Calendar preparation and RSS budget

The architecture requires steady RSS <=320MiB after all popups have been used,
with initial idle well below that. The previous whole-calendar warmup moved
~139MiB into startup and does not satisfy that requirement by itself.

The host now prepares only the 150-logical-pixel header/day-label/first-row
slice, without event summaries. Font and shader styles stay warm while work is
independent of provider event count. EGL window destruction purges unlocked
scratch render targets, retaining other live surfaces and warm shader/glyph
caches. The purge has a private-Weston GPU regression that allocates a 4MiB
closed target and verifies both reclamation and an unchanged live target pixel.

Validation is private nested KWin/Hyprland,3072x1728 logical at DPR2, real
providers read-only, quiet agent inputs, production NVIDIA EGL/RGBA8. The
benchmark uses docs/verify/measure-shell.py. Instrumented frame intervals are
render-begin to submitted-buffer; model work and compositor presentation are
separate, so these are not universal input-to-present latency guarantees.

Baseline pinned ELF ad5ae63d0652631d5f887d86ddb9a9cb97cf06a9a3fe14b5422fb3bf2a8851d5:
initial idle356.23→356.30MiB; calendar/control activity355.13→386.09MiB,
48/48 render frames <=16.7ms, max6.435ms. This is an integrated checkpoint,
not a causal comparison with the older~497MiB observation; other owners are
changing font residency concurrently. Raw receipts:
docs/verify/raw/perf-shell/calendar-memory-baseline/ and
target/parity-rounding/memory-baseline-summary.json.

Final measured ELF `f73c3062459c6acbbfba3886ef05ba8c9fea07bae61b6cfb304a0557d608d933`:

| Checkpoint | Initial idle RSS, MiB | After calendar/control both close, MiB | Render frames <=16.7ms | Max render, ms |
| --- | ---: | ---: | ---: | ---: |
| Reduced warmup + scratch purge |356.17 |379.02 |52/52 |8.773 |
| Also share engine FT font backing |332.77 |363.81 |51/51 |8.154 |
| Also omit synthetic wrapped paragraphs |312.18 |339.44 |38/38 |11.565 |

Same final ELF with diagnostic `HYPRV_SKIP_CALENDAR_PREPARE=1` measures
199.95MiB settled idle (no popups). Enabled is312.18MiB: preparation still
frontloads **112.23MiB**, rather than the historical~139MiB. The smaller geometry
alone does not make font residency small. This idle-only control makes no cold
calendar timing claim; disabling preparation is not presented as a frame fix.
Receipt: docs/verify/raw/perf-shell/calendar-memory-no-prepare/ and
 target/parity-rounding/calendar-memory-no-prepare-summary.json.

The **320MiB budget still fails**, already on this two-popup workload. Initial
idle is only7.8MiB below the steady limit, not the requested "well below" margin.
The older~497MiB observation used a different snapshot; these are integrated
checkpoints, not a claim that engine alone saved the full difference.

The final candidate also shares one immutable font-file mapping between Skia
and cached FreeType named instances instead of mmap per weight. A regression
checks four weights' actual stream pointers plus identical glyph bytes/advances
against independently opened FreeType file faces. Font preparation no longer
shapes synthetic300px wrapped paragraphs that are never displayed. Real wrapped
content retains its original lazy cache and geometry; regression coverage
verifies this. Final105 UI/shell library tests and scoped all-target Clippy pass;
10 surfaceless renderer and2 private EGL lifetime tests pass.

A private-process `/proc/PID/smaps` breakdown identifies CJK mappings as a large
RSS holder: the reduced-warmup checkpoint maps the same font eleven times;
final initial mapped RSS67,424KiB rises to87,560KiB after activity. These pages
have much lower PSS because they are shared file pages; the acceptance budget
explicitly measures RSS. This is not evidence of an unbounded leak. The panels
qtext path still calls `to_font_data`, hashes the full file and permanently
retains a ~20MiB heap buffer in addition to its Skia typeface mappings. Svc-media
and the panels owner have the receipts and the request to consolidate that
backing. Its prospective savings are **not** counted as achieved.

Final raw receipts: docs/verify/raw/perf-shell/calendar-memory-final/;
target/parity-rounding/calendar-memory-final-summary.json, calendar-memory-final.csv,
and memory-final-mappings.json. The retained ELF is
`target/parity-rounding/memory-final/hyprv-shell`. Earlier candidate ELF copies
are retired after retaining their hashes/metrics. The activity's4 IPC responses
succeeded (calendar open/close, control open/close). Model max1.605ms, inclusive
host sync max33.192ms; the latter includes other host work and is not covered by
the render-frame gate. No universal input-to-buffer guarantee is claimed.

QA lead's verify.py run is authoritative. The earlier engine280/312 and
svc-power304/304 bar matrices are snapshot/cohort receipts; no bar rendering
change or regression conclusion is based on their difference.
