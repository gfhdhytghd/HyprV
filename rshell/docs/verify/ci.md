# Snapshot integration CI

Updated 2026-10-23T20:36:48+00:00. Window 2026-10-23T18:36:47+00:00–2026-10-23T20:36:47+00:00.

Round 30: **TWO-HOUR LIMIT; NOT GREEN**.

Snapshot: `/tmp/hci-sys/round-30-20261023T203551Z/rshell`.
SHA-256 fingerprint: `500884ed04962745e2fac71668d35cb08358e3d2857dd035a5efbfeb50b96292`.
Existing shared target cache: `/home/wilf/.config/HyprV/rshell/target`. No private target directory is created or cleaned. Incremental compilation and debug symbols are disabled for dev/test builds to fit storage; these remain unoptimized debug builds with debug assertions. Release profile is unchanged. Cargo uses four build jobs. Source files use copy-on-write reflinks, never hard links to live files. `target/` and Git metadata are excluded at every depth. Sibling Hypr/Quickshell fixture inputs are copied too.

Snapshot admission requires equal build/test/reference hashes before and after copying and in the copied tree. Every manifest-listed reference PNG must exist and be complete; incomplete recaptures are recopied. A failing test command is rerun once on the same snapshot before filing; both outcomes and logs are retained. A retry pass is labelled flaky. Timeout/deadline alone is not a crate attribution.

| Check | Result | Evidence |
|---|---|---|
| debug | PASS | [attempt 1](raw/ci-snapshots/round-30-20261023T203551Z/00-debug-1.log) (0) |
| release | TIMEOUT / INCOMPLETE | [attempt 1](raw/ci-snapshots/round-30-20261023T203551Z/01-release-1.log) (124) |

| Crate | Debug | Release | Tests | Clippy |
|---|---|---|---|---|
| hyprv-core | PASS | BLOCKED | PENDING | PENDING |
| hyprv-privhelper | PASS | BLOCKED | PENDING | PENDING |
| hyprv-shell | PASS | BLOCKED | PENDING | PENDING |
| hyprv-svc-desktop | PASS | BLOCKED | PENDING | PENDING |
| hyprv-svc-media | PASS | BLOCKED | PENDING | PENDING |
| hyprv-svc-net | PASS | BLOCKED | PENDING | PENDING |
| hyprv-svc-power | PASS | BLOCKED | PENDING | PENDING |
| hyprv-svc-probe | PASS | BLOCKED | PENDING | PENDING |
| hyprv-svc-sys | PASS | BLOCKED | PENDING | PENDING |
| hyprv-ui | PASS | BLOCKED | PENDING | PENDING |
| hyprv-ui-bar | PASS | BLOCKED | PENDING | PENDING |
| hyprv-ui-bench | PASS | BLOCKED | PENDING | PENDING |
| hyprv-ui-island | PASS | BLOCKED | PENDING | PENDING |
| hyprv-ui-panels | PASS | BLOCKED | PENDING | PENDING |
| hyprv-widgets | PASS | BLOCKED | PENDING | PENDING |

Engine idle-wakeup regression discovery: `[{"command": ["cargo", "test", "-p", "hyprv-shell", "--locked", "--test", "idle", "--", "--nocapture"], "ignored": false, "name": "quiet_nested_shell_has_no_repaints_or_extra_main_dispatches", "path": "crates/hyprv-shell/tests/idle.rs"}]`. Ignored live tests require an isolated harness and are never enabled against the live session.

All workspace tests are required; no reference-image tests are exempt or skipped. BLOCKED is not a defect attribution.

Completion requires both builds, full workspace tests, doctests, Clippy with warnings denied, xtask/cutover/probe tests, staged release verification and performance preflight with idle <=0.1%. Only helper installation and the separate parity preflight gate are exempt; performance is required. No install or live actions are run.

Full manifests, per-crate diagnostics and historical round results: [snapshot CI evidence](raw/ci-snapshots/). Source fingerprints identify the copied candidate, not the moving live workspace.

Disk policy: this completed source snapshot was removed after summarizing evidence. The complete input hash manifest, command logs, build receipt and release manifest remain in the evidence directory. No new target cache is created.

## Deadline audit

Window ended at 2026-10-23T20:36:47Z. Overall **NOT GREEN**. The final round 30 was stopped at the deadline; its incomplete phase is not a crate defect. Last fully executed round 29 (`9e638c5b6040e0a5f6acacbffda43468994660ca098320fea07a6d4973636136`) passed every validation command, including both workspace builds, full workspace tests/doctests, strict Clippy, staged release verification, auxiliary checks and the explicit engine idle-wakeup regression. No copied build inputs changed during validation. Its non-installing preflight is blocked by stale real-shell performance source evidence; helper installation and the separate parity gate remain the only exemptions.

| Last complete round 29 check | Status |
|---|---|
| debug | PASS |
| release | PASS |
| stage release | PASS |
| workspace tests | PASS |
| doctests | PASS |
| clippy | PASS |
| xtask tests | PASS |
| cutover tests | PASS |
| probe tests | PASS |
| snapshot guard tests | PASS |
| engine idle-wakeup: quiet_nested_shell_has_no_repaints_or_extra_main_dispatches | PASS |

Engine regression was included beginning with round 22, and passed every completed focused execution; round 25 was blocked by the upstream compilation error. Its archived native-compositor receipts demonstrate 10 sampled metadata projections, zero stats UI wakes, zero paints and one measurement-RPC dispatch. Rounds 25/26 reproduced transient owner sysstats compile failures twice on immutable source; round 27 and later confirmed their repair. Round 21 was interrupted only to reload discovery; round 23 is explicitly INCONCLUSIVE because temporary tracing wrappers referenced an external live module. It is excluded from accepted CI/performance evidence. Admission now waits for those wrappers to disappear; six guard regressions pass.

Five complete release-shell refreshes were published in this window (18:49, 19:06, 19:37, 20:09 and 20:20 UTC start tags). Latest valid measured source is snapshot `2fb4b4a85ae0e9e3ab014434cb81454584c48aac5984c3499fe065a6c9e48ab4`, not the later validation snapshot. Controlled-quiet CPU **0.2201% > 0.1%**; live-hook CPU **1.0246%**, with **11** hook notifications separately reported. See [perf-shell.md](perf-shell.md). The newer candidate needs a fresh measurement; no stale or isolated result is accepted as a full-host idle pass.

All owned frozen source copies have been removed. Shared target cache and only the current measured ELF are retained, with compact logs/hashes/receipts. No installation, cutover or host-service mutation was performed.

## Final frozen candidate — 2026-10-23 23:40 UTC

This is a one-candidate validation after final owner handoffs, not a restarted timed CI loop. Snapshot `f17e92a957d28092e0d0f8197b85c9dc5dbc2ecda2252195c107ab5053eacf46`, ELF `31bebad0450c520def4e1448168d940f8203ddf53def4bbbd07c715d87731edc`. Build / configured tests / clippy / GL component + assembled parity PASS; same-ELF 204 functional cases and idle wakeup regression PASS. Performance FAIL (controlled quiet 0.205699% precise CPU); helper installation BLOCKED. No input changes; disposable snapshot removed. Full per-gate table and evidence: [fr-net2-readiness.md](fr-net2-readiness.md). No installation or cutover.

## Packaged-assets frozen candidate — 2026-10-24T00:24:06.791962+00:00

Snapshot `1f71e398dd97e417718440289b80895d40d51f5b7fb1907ae79dc87d5dad9d23`; ELF `9a9a43e4e7e94f22fee6a70b6482f7a7cf26bf9c3507994b1462c57760361e5f`. Release build/stage, runtime-assets gate and post-snapshot scratch install rehearsal, full configured tests (90 cutover tests), clippy, pinned 204 functional cases/idle regression, GL component + full-screen/corners: PASS. Performance FAIL (0.294618% precise controlled quiet CPU); helper installation BLOCKED. Default `packaging/dist` and its non-installing preflight now consume the same pinned candidate. No host install/cutover; shared target only; frozen source tree removed. [Gate table and proof](runtime-assets.md). This was one RC refresh, not a restarted timed CI loop.
