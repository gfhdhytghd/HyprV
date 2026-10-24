# Independent review: engine reconcile checkpoint

Reviewed 2026-10-23 against docs/verify/engine-reconcile.md and current source. [Source fingerprints, logs and reproduction](scratch/discovery-icons-reconcile/README.md). This report covers engine offscreen consolidation, renderer/cache invalidation, measurement reuse, retained bar scheduling and host integration. Companion reviews cover desktop discovery, widgets icon lookup and shell asset resource bounds.

No additional engine reconcile correctness defect was verified in this round. Inspected the roleless EGL endpoint/target lifecycle; explicit CPU/GL backend selection; coordinated cache generations and current-context ownership; renderer bounds/reachability caching; wrapped metric keys/budgets; RetainedView module signatures, stable slot identity, normalization and image/theme/scale invalidation; and mount teardown. Absence of a finding is bounded by the checks below, not proof that every rendering or driver lifecycle is correct.

Fresh execution against shared target, with CARGO_INCREMENTAL=0:

- Shell library: 34 passed, including scheduled_bar_updates_retain_modules_and_match_fresh_trees and removal/close lifecycle controls.
- UI library: 39 passed, 11 explicit GPU/EGL cases initially ignored by the ordinary runner.
- Widgets library: 57 passed.
- Explicit GPU launcher: 10 surfaceless renderer checks plus one private-Weston EGL flush check passed on Mesa; all 11 passed again on NVIDIA. Coverage includes upload failure recovery/partial glyph fallback, full mip charging, cache release and scale/rotation/theme transitions.

Total: **130 ordinary passes + 22 GPU/EGL passes**, zero unexpected failures. The review's separate intentionally failing probes are not included in these passing totals. Logs: `scratch/discovery-icons-reconcile/engine-tests.log`, `engine-gpu-mesa.log`, `engine-gpu-nvidia.log`.

The earlier 1968-image reconcile claims were read as historical checkpoint evidence. Their retired binaries/captures were not reconstructed, and this round does not claim a new full parity run, new soak, or independent causal proof for the prior GPU retention. No production source was edited, no live compositor was used, and no new private build tree was created. No owner pointer was filed for this no-finding report.
