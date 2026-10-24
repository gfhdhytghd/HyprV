# Engine resource-lifetime implementation — laptop Phase B

2026-10-24. Default-on, profile-independent changes in hyprv-ui/hyprv-shell.
No live input, service restart, install, root action or git commit. Service implementation
remains with svc-power/svc-desktop; requests and handoffs are in their request logs.

## Changes

- Keep one full-output corner layer and Qt's original atlas/viewport coordinates.
  After a settled full-redraw submission, retire the EGL surface and wl_egl_window;
  the compositor retains the last attached buffer. There is no null attach, replacement
  role, extra commit, readback or maintenance timer. Recreate on actual damage.
- A shared host animation hint keeps these drawables across related-surface transitions.
  Retirement after the final transition requires no new paint, including when its final
  state produces no damage. Ordinary retained surfaces keep buffer-age/damage repair.
- Window-scoped texture/glyph references release orphan images, old Canvas scale variants
  and empty trailing atlas pages. Shared images/glyphs and compiled material programs
  survive another window's retirement. GPU glyph pages use normalized R8/alpha swizzle
  on GL3+, preserving the original mask shader; GL2 retains the existing RGBA path.
- Cap unlocked Skia GPU caches at8MiB. CPU image sampling/source/affine-mip caches share
  a2MiB family; Canvas, Qt-rect coverage and Qt-rect rasters have2MiB each. Oversized
  results render uncached. Fixed the previous4x undercharge of RGBAF32 sampled images.
  These limits exclude active targets/images, metadata, fonts and transient allocations;
  they are not a claim that whole-process RSS is8MiB.
- Surface close prunes cached effects/bounds against all remaining scenes. Closed calendar
  render trees/model keys are released while semantic state and bounded material
  preparation remain. Existing native popup destruction still follows the last close frame.
- Actual painted bar-stat rectangles own their demand; fully translated-out labels release
  it, partial reveal keeps it. `BarStatsDemand::{Hidden,Compact,Detailed}` supplies no
  lease / CPU+memory5s / desktop's existing five metrics1s. Clock visibility is independent.
  No cache/purge/animation timer was added. Unchanged formatted values retain zero damage.

Visible corner artwork remains visible. Destroying its **layer** in desktop minimal
state would change pixels; this implementation retires its native drawable instead.
The old four tiny layers and a compact-atlas approximation are not reintroduced.

## Verification

- Private native direct-window retirement controls:8/8 intrinsic GPU captures and8/8
  compositor-screen comparisons byte-exact between retained-drawable and retired-drawable
  cases, no masks; all output transforms at scales1/1.5/2 pass. A host transition retains
  its drawable, then releases it with zero extra frames when the hint clears.
- The first screen comparisons were invalidated by Hyprland's animated watchdog warning,
  which overlapped the test card. The private compositor disables that banner; reruns
  compare every screen pixel. The live compositor/configuration was not changed.
- New GPU regressions verify orphan Canvas variants are deleted, shared textures survive
  retirement without upload, R8 glyph pages reproduce original shader pixels, and shared
  glyph pages survive one owner then disappear after their last owner. Oversized CPU
  F32 samples/mips are not retained and their returned pixels remain exact.
- An initial pinned-debug functional run passes204/204. Final release/full-screen/current
  component receipts and measurements are recorded below after completion. No stale receipt
  is promoted to current simply because the pixel results are green.

Resource plan and evidence boundaries: [Phase A design](../spec/laptop-minimal-resources.md).
Raw reports: `docs/verify/raw/laptop-minimal-resources/`.

## Measurement status

Final release verification and matched nested before/after measurement in progress.
The laptop compact UI is owned by its component agent and is not invented by this
resource change. Comparisons use the same desktop closed-popup state as Phase A,
6144x3456@2, quiet agent fixture/private PID namespace, read-only native providers.
They do not establish an XPS/on-battery result. Required visible desktop statistics
still change; a real-provider desktop run is not a zero-source mock idle test.

## Desktop install checkpoint (2026-10-24)

Shared production edits are complete and held for the orchestrator's build/gate/install.
No unfinished production hunk remains to stash. `packaging/build-release.sh` succeeds;
staged release `dc22d67f66e7eb875053aacf5e058068721edf0f497b75389d5284cd88071398`
and its entire source manifest match the shared tree at this checkpoint.

- Pinned final release native-input E2E: **204/204 PASS**.
- Current-source canonical GL parity: island380/380; panels300/300; bar300/300
  reachable crops plus12 predeclared live-unreachable exclusions (owner GL gate312/312).
  No new exclusion or tolerance change. All three source fingerprints are current.
- Scoped all-target Clippy with `-D warnings`: PASS. Native GPU regressions25/25;
  output transform/drawable retirement regressions2/2 PASS.

Compact evidence: `raw/laptop-minimal-resources/checkpoint.json`.
The broader assembled fullscreen/corner matrix continues on the immutable pinned ELF.
Matched resource measurements remain pending; this checkpoint does not certify the laptop
memory/CPU budget or replace the orchestrator's remaining release preflight gates.
Neither task requires further shared production edits. No live action or install performed.
