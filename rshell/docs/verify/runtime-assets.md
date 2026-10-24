# Runtime asset packaging repair

**Repair complete; candidate re-staged. Overall preflight: NOT READY (performance and helper installation).** Completed 2026-10-24T00:24:06.791962+00:00. Only disposable scratch HOME installs were performed, with systemctl stubbed; no host install, service activation or cutover.

Staged ELF: [`packaging/dist/hyprv-shell`](../../packaging/dist/hyprv-shell). SHA-256:

```text
9a9a43e4e7e94f22fee6a70b6482f7a7cf26bf9c3507994b1462c57760361e5f
```

Release manifest ID: `1f5d20e2ff30090e7a21ae6a165638f88cd3d35ab9ec6053f5eb64e0df048071`. Frozen input fingerprint: `1f71e398dd97e417718440289b80895d40d51f5b7fb1907ae79dc87d5dad9d23`. Build/parity inputs remained unchanged through validation and publication. The compiled snapshot is now deleted; retained source hashes, logs and required actual/reference PNGs remain with the candidate archive. No private Cargo target was created.

| Gate | Status | Result |
|---|---|---|
| Release build / default staging | PASS | `packaging/build-release.sh`; all ELF and release manifest digests verified |
| Runtime assets preflight | PASS | 44 payloads + catalog; staged and scratch-installed checks |
| Installed-unit rehearsal | PASS | Actual installer, installed unit environment/ExecStart expanded for scratch HOME, installed ELF from `/`; passed again after compiled snapshot deletion |
| Automated tests | PASS | Workspace all-targets, doctests, xtask, 90 cutover Python tests, probe Python suite; ignored opt-in live/device tests excluded |
| Clippy | PASS | Workspace/all-targets `-D warnings` |
| Pinned native replay / idle | PASS | 204/204 functional cases; 10.1 s mock stats regression, zero metadata UI wakes and zero paints |
| Production GL components | PASS | All 992 images; island 380/380, panels 300/300, bar 300 pass + 12 documented battery QUIRK exclusions; all eight host crops included |
| Assembled full-screen / corners | PASS | 26 jobs, 64/64 native + independent comparisons, 256/256 exact corners, zero masks |
| Performance | **PASS (accepted: user, 2026-10-23)** | Exact candidate acceptance: quiet 0.294618% ≤0.5% (0 hook events); live-hook idle 0.465623% ≤2.0% (7 events). The ≤0.1% aspiration remains documented. |
| Privhelper installed | **BLOCKED** | Installed release record absent; no root operation attempted |

The release bundles the original static image assets and the Fluent SVG subset used by the production loader. The ELF embeds their SHA-256 catalog. User installation puts binary, unit, catalog, `assets/`, and `icons/` in one journalled transaction under HOME; destination hashes are checked before daemon reload, with restoration on failure. Data root: `~/.local/share/hyprv-shell`; the unit sets `HYPRV_ASSET_ROOT=%h/.local/share/hyprv-shell`.

Normal UI startup verifies the installed catalog against its embedded copy and every payload before providers, IPC or Wayland start. Missing, corrupt and non-regular payloads fail with a named error. There is no checkout asset fallback; overrides must be absolute. `--check-assets` runs the same verification without starting providers or UI. Help and IPC remain usable without UI assets.

Default preflight validates the staged bundle and configured unit; `--installed` additionally checks HOME's files. The scratch rehearsal substitutes only systemctl, uses the real install transaction, and checks unit/default roots, idempotence, missing/corrupt payloads, and missing catalog. It installed 47 files on the first run, zero on the second. A separate normal-startup negative check exits 1 with the exact missing asset path while the checkout is present. The published rehearsal then passes with the compiled checkout already gone. These are file/CLI/unit-environment checks, not activation of a real systemd user unit.

[Published candidate and hash audit](raw/final-candidate/20261023T234946Z/published-candidate.json), [current preflight JSON](raw/final-candidate/20261023T234946Z/published-preflight.json), [published installed-unit rehearsal](raw/final-candidate/20261023T234946Z/assets-rehearsal-published.json), [rehearsal transcript](raw/final-candidate/20261023T234946Z/published-assets-rehearsal.log), [normal-startup negative](raw/final-candidate/20261023T234946Z/normal-startup-negative.json), [install-user dry-run](raw/final-candidate/20261023T234946Z/install-user-plan.log), [frozen execution phases](raw/final-candidate/20261023T234946Z/result.json), [snapshot hashes](raw/final-candidate/20261023T234946Z/snapshot-manifest.json).

[Fresh performance report](perf-shell.md) records CPU, scheduling proxies, RSS, GPU memory and read-only legacy-family observations. Live idle has 7 hook-file notifications; activity has 5. Event costs are not subtracted. The higher quiet CPU is an observation on a shared host, not a proven packaging regression: only packaging/startup inputs changed relative to the prior candidate, and current per-thread CPU remains dominated by the UI (0.271657%; services 0.021029%; agent discovery 0.001932%). Earlier presentation profiling belongs to the prior ELF and is not relabelled as fresh causal proof. The current data has been appended to engine's request.

References cover DP-4 @2 for the assembled matrix and component @1/@2. Two-popup activity does not certify every popup's warmed working set. The measurement windows explicitly pin the archived candidate and its bundle; the helper's default now also selects the staged release. Source-level unit harnesses are separate from the pinned production ELF runtime checks.

To repeat the packaging rehearsal without touching the host installation:

```sh
python3 -B packaging/rehearse-assets.py --out /tmp/hyprv-assets-rehearsal.json
```

## Explicit performance acceptance, 2026-10-23

The user accepted deployment performance for the exact SHA-256 above in [`cutover/perf-acceptance.json`](../../cutover/perf-acceptance.json). Preflight checks the measured, staged and (when requested) installed binary hashes, fresh runtime fingerprints, updated report, <=24-hour measurement age, uninstrumented >=60s windows, scheduling counters and both accepted CPU bounds. Hook events are allowed only in the live window and remain reported. Invalid/missing evidence, changed hashes and exceeded bounds fail closed. No binary or original build receipt was changed; `cutover/evidence/readiness-amendment.json` separately pins the three readiness-only policy/test changes.

Validation completed: `HYPRV_ASSET_ROOT="$PWD/packaging/dist/release/runtime" cutover/preflight.sh --run-tests` passed all configured workspace targets, doctests, xtask, 100 cutover Python tests and probe Python tests. Final preflight: release binaries PASS; runtime assets PASS; automated tests PASS; production GL/full-screen/corners parity PASS; performance PASS (accepted: user, 2026-10-23); privhelper installed BLOCKED because `/usr/local/lib/hyprv-cutover/installed.json` is absent. Exit 1 reflects only that installation blocker. Full output and JSON are retained in `cutover/evidence/perf-acceptance-preflight.{log,json}`. No deployment actions were executed.
