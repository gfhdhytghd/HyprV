# FR-NET-2 readiness repair

Latest candidate: [runtime asset packaging repair and current gates](runtime-assets.md). The report below records the preceding ELF and its then-unresolved asset packaging issue; that issue is fixed in the replacement candidate.

Status: **FR-NET-2 closed; frozen release candidate NOT READY.** Full non-installing preflight finished `2026-10-23T23:40:57.526017+00:00`. No installation or cutover. No compiled/test/parity input changed in the snapshot or moving tree during this run.

Candidate ELF: [`packaging/.build/final-candidate-20261023T231008Z/dist/hyprv-shell`](../../packaging/.build/final-candidate-20261023T231008Z/dist/hyprv-shell). SHA-256:

```text
31bebad0450c520def4e1448168d940f8203ddf53def4bbbd07c715d87731edc
```

Snapshot fingerprint: `f17e92a957d28092e0d0f8197b85c9dc5dbc2ecda2252195c107ab5053eacf46`. [Full result](raw/final-candidate/20261023T231008Z/result.json), [snapshot hashes](raw/final-candidate/20261023T231008Z/snapshot-manifest.json), [copy consistency](raw/final-candidate/20261023T231008Z/copy-consistency.json), [preflight JSON](raw/final-candidate/20261023T231008Z/preflight-1.log), [retention audit](raw/final-candidate/20261023T231008Z/retention-audit.json).

| Gate | Result | Evidence / limits |
|---|---|---|
| Release binaries | PASS | `packaging/build-release.sh`; all three staged ELF digests verified |
| Automated tests | PASS | Workspace all-targets, doctests, xtask, cutover 84, probe Python suite; all five configured commands passed first attempt |
| Clippy | PASS | Workspace/all-targets, `-D warnings` |
| Same-ELF functional replay | PASS | 204/204 native input / ordered mock-service cases |
| Same-ELF idle regression | PASS | 10.1 s, 10 sampler ticks, zero metadata UI wakes, zero paints; clean IPC shutdown |
| Production GL component parity | PASS | Island 380/380; panels 300/300; bar 300 passing + 12 documented battery exclusions. All 992 images, including eight delegated crops and excluded PNGs, exist and hash-match |
| Assembled full-screen / corners | PASS | Same staged ELF, 26 jobs / 64 images; 64/64 independent and native comparisons; 256/256 exact corners, zero masks |
| Performance | **FAIL** | Controlled quiet 0.205699% thread CPU, 0.199681% process CPU, 60.095757 s, zero hook notifications; target ≤0.1% |
| Privhelper installed | **BLOCKED** | `/usr/local/lib/hyprv-cutover/installed.json` absent; no root action attempted |

[Performance report](perf-shell.md) includes CPU, scheduling proxies, RSS, GPU memory and read-only live Quickshell observations. Frozen-hook host discovery: 0.402195% precise CPU, zero hook events. Live idle: 0.331982% process CPU, 15 hook notifications with observed non-ProcessScan changes. Activity: 60 s / 12 UI toggles, 10 such notifications. Event counts are separate; no cost is subtracted. Two-popup activity does not certify every popup's warmed working set. Ordinary unit/test harnesses are **source-level evidence**, not the production ELF; the 204-case replay, idle regression, assembled full-screen replay, and all four acceptance performance windows use the pinned ELF. [Runtime identity receipt](raw/final-candidate/20261023T231008Z/pinned-runtime.json).

The separate 30-second instrumented diagnostic is excluded from acceptance. It localizes most paint CPU to `egl.swap`: 76.524 ms of 83.562 ms paint CPU across 33 frames, following 31 stats ticks. Quiet acceptance per-thread CPU is UI 0.183547%, services 0.020228%, agent discovery 0.001925%. [Diagnostic CSV](raw/final-candidate/20261023T231008Z/ui-diagnostic.csv); precise follow-up filed in `docs/requests/hyprv-ui.md`. This is presentation-scope localization, not proof of a particular driver defect.

**Additional packaging limitation:** frozen ELF asset fallback embeds the disposable checkout path. The unit does not set `HYPRV_ASSET_ROOT`. Before installing this candidate, supply a stable asset root or package it; the small frozen assets remain in the candidate archive at `runtime-quickshell/assets`. See `docs/requests/hyprv-packaging.md`. This limitation is not silently counted as an existing preflight gate.

Per disk policy the 12 GiB disposable source tree and unreferenced raw EGL captures were removed after validation. Retained archive is about 318 MiB: release, all 992 component actual PNGs, all 64 actual/reference full-screen gate images, canonical component references, test/runtime/performance receipts and independent comparers. Artifact path relocation is recorded separately; original full-screen receipt and command provenance remain in the evidence directory. Captured coverage is DP-4 @2 (6144×3456), with component @1/@2; no other-driver/output/fractional-scale proof is inferred.

Implementation and regression coverage:


- Preflight selects only `bar-gl`, `island-gl`, and `panels-gl`, with explicit GL backend metadata. CPU slots and old mtime-only evidence cannot qualify.
- The verifier records content hashes before rendering and after comparison for all transitive workspace Rust/native/assets/replay inputs, canonical references, and shared gate policy. Every published actual PNG is hash-listed. Missing, changed, partial, duplicate and failed renders block the gate. Shared producer/consumer locks prevent in-progress publication from passing, and end-of-inspection hashes reject concurrent source edits.
- The verifier and preflight import the same documented unreachable battery-state policy. Only battery-charging/critical/full use the QUIRK exclusion. Their PNG coverage is still mandatory. Reference metadata, rather than a result row's self-declared flag, controls text tolerance; non-text maximum and mean must both be zero.
- All eight component host crops must exist and pass, with both themes and both component scales. A separate same-staged-ELF assembled receipt requires the complete canonical full-output matrix, no masks, independent pixel comparison and four exact corner crops per image. Current assembled references cover DP-4 @2; no @1 full-output proof is inferred.
- Build/test receipts now include the review's externally stored flickable and Qt oracle inputs, plus parity gate scripts/reference inputs. A changed oracle cannot leave a test receipt current.

Regression command: `python3 -B -m unittest discover -s cutover/tests -p 'test*.py'`. Latest completed run: **84 passed**, including the actual-pixel negative control: a one-level corner change passes the broad text-tolerant full-image scorer but is rejected by the exact unmasked corner gate.

Reproduction on a coherent candidate:

```sh
packaging/build-release.sh
python3 -B parity/verify/verify.py --backend gl
python3 -B cutover/verify_fullscreen.py --binary packaging/dist/hyprv-shell --run cutover/evidence/fullscreen-run
cutover/preflight.sh --run-tests --json
```

The run directory must be empty. The full-screen runner writes an in-progress receipt before launching private compositors, so interruption cannot silently retain an older green receipt. It records the staged ELF hash and source hashes across render/scoring. Nothing installs or starts host services. Helper installation and performance remain independent checks and are never bypassed.
