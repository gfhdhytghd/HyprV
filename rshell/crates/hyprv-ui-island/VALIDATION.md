# Island exactness acceptance — 2026-10-23

The production GPU replay is **380/380 bit-exact**, up from368/380 at the start
of this follow-up (0/380 before the first box). All380 crops are included;
maximum premultiplied RGBA difference and total absolute channel error are zero.
The canonical comparator, masks and acceptance thresholds are unchanged.

## Fixed-cohort progress

Cohorts retain each image’s initial QA label. Counts compare the start of this
follow-up with the accepted run, rather than moving images between classes.

| Initial QA class | Exact before | Exact after |
|---|---:|---:|
| Fill/color | 140/140 | 140/140 |
| Canvas | 98/102 | 102/102 |
| Text | 54/54 | 54/54 |
| Animation | 42/44 | 44/44 |
| Rect AA | 29/35 | 35/35 |
| SVG/icons | 5/5 | 5/5 |
| **Total** | **368/380** | **380/380** |

Eleven max-1 crops contained49 integer premultiplied channel-error units. The
remaining stale-hover reference contributed119,752. Rendering fixes closed all
49 units; the separately approved deterministic fixture correction closed the
input-state mismatch. First-box history and its limitations are retained in
[VALIDATION-first-box.md](VALIDATION-first-box.md).

## Rendering changes and independent proof

- Preserve the QML controls Row instead of flattening its translation. This
  retains the float matrix operation order without changing button hit geometry.
- Track direct clip-relative transforms, avoiding loss from subtracting world
  coordinates and dividing by scale. Scoped matrices restore at batch boundaries.
- Match QSG smooth-color batch coordinate selection: a compatible scaled sibling
  requires full-matrix geometry. Per-element decisions respect opacity, clip roots
  and intervening overlap; rendering order and shader programs are unchanged.
- Preserve the animated QRectF source size until Qt’s corner-center assignment.
  `Node.qt_rect_size` is optional; only the two animated island frames opt in.
  The consolidated bounded GPU rectangle cache remains the rendering path.

An installed-Qt independent oracle checks all256 vertices and indices of a
fractional animated rectangle. A separate QML scaled-sibling probe supplies GPU
pixel goldens at both DPRs. Unit regressions cover coordinate cancellation,
metadata reconciliation and opacity/clip/overlap batch boundaries. These are
ports of Qt’s staging rules, rather than image-specific pixel substitutions.

## Approved canonical recapture

The user explicitly authorized the fixture/reference scope extension and required
`parity/capture.sh`’s atomic staging swap. The sole fixture edit parks the pointer
at island-local(26,43), over album art, then advances300ms before the final
`music-expanded-playing` capture. This removes an implicit Previous-button hover.
Two private canonical-tool captures were bit-identical and matched Rust4/4.

`bash parity/capture.sh island music-expanded-playing` published the approved
change atomically. Exactly one PNG changed (`music-expanded-playing--dark@2.png`);
the380-image manifest and four capture sidecars update for the explicit input/time.
No image was added or removed. Original QML and runtime behavior are unchanged.
Reason, authorization and hashes: `docs/requests/parity-capture.md` and
[approved-recapture/result.json](artifacts/approved-recapture/result.json).

## Final verification receipts

[Aggregate acceptance receipt](artifacts/accepted-summary.json). All three final
replays have stable source hashes matching the final tree; island/bar reference
hashes are also stable.

- Full GL replay: [accepted-final/metrics.json](artifacts/accepted-final/metrics.json).
  Binary, source and reference fingerprints accompany the receipt.
- Recaptured bar control: **304/304 exact**, all category errors zero:
  [accepted-bar/gate/gate.json](artifacts/accepted-bar/gate/gate.json).
- Panels control: **255/300**, all255 pre-refinement passes retained:
  [panels-cross/results.json](artifacts/panels-cross/results.json). Other owners’
  panel/engine changes also landed; the panel improvement is not attributed solely
  to island work.
- All-target tests and strict Clippy for engine, island and bar:
  [accepted-tests-current.log](artifacts/accepted-tests-current.log),
  [accepted-clippy-scoped.log](artifacts/accepted-clippy-scoped.log).
- Ordinary tests: **143 passed,0 failed,21 intentionally ignored**. Strict Clippy
  uses `--all-targets --no-deps -- -D warnings` for the three changed crates. The
  broader dependency traversal flags a concurrent panels `type_complexity` warning;
  its source is outside this owner’s edit scope.
- Six native GL goldens: [accepted GPU tests](../hyprv-ui-bar/artifacts/accepted-ui-goldens/run.log).
- Private mapped-surface stress:6,000 frames,4 surfaces,1,974 resizes,63 recreates,
  2,662 partial-damage frames and776 sampled readbacks, zero mismatches:
  [stress receipt](../hyprv-ui-bar/artifacts/island-bar-accepted-stress/results.json).

Runtime remains Qt-free; Qt is used only for offline reference/oracle generation.
The earlier unreproduced GPU-corruption risk remains OPEN in `docs/STATUS.md`;
finite stress results do not prove its absence on every driver/workload.
