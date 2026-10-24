# P0 cooling release gate refresh

## Why the shell ELF changed

Baseline shell SHA-256: `9a9a43e4e7e94f22fee6a70b6482f7a7cf26bf9c3507994b1462c57760361e5f`.
Cooling candidate shell SHA-256: `2073ea266e34c5aee6a69038baeaaeb75a2a287da636ee55176469f7d54c89d0`.

The exact old and new build-receipt source maps differ at four Rust paths:

| Path | Before SHA-256 | After SHA-256 |
|---|---|---|
| `crates/hyprv-privhelper/src/bin/hyprv-cooling-failsafe.rs` | `6cdf381aac2ef7210ac96da9b121d4f1bf8d59c5fe1afeb5cb1433c66840a085` | `162317322646f418f3b598222ad78365e56cf158afd37e9c72757c3529984e07` |
| `crates/hyprv-privhelper/src/cooling.rs` | `2e9f197aa303da96fa412a11749a580a3b435dc74bb271149224c43d351c9112` | `325265b59f4c661846dd0a00c8e6ba58bdbf4d67f609c3cff3f355994dca9640` |
| `crates/hyprv-privhelper/src/cooling_regression.rs` | `4347bccfad1c2190c3e1def43ab828ae28ec9c04a0a04c7b5d11697e1f6f8fe0` | `261487478fc537acf109489447d02b046b5d4123c12952c3d04fc5684ac92a99` |
| `crates/hyprv-privhelper/src/main.rs` | `1478a47a63678894f0da554c38e675b1ba86114a8096443b655d0469f9f1d7ae` | `a86c5b9cb64fd02d358488cb3d1ff31387c3b66baec9ae2b0eecf94a9b237418` |

All shell/UI/service sources, runtime assets, Cargo.toml/Cargo.lock and release profile match the old receipt. The production library change is cooling.rs: integer-plus-newline stores now use one syscall, short stores fail without fragmented retries, enable=1 precedes duty, recovery uses 255 independently per channel, duplicate chips are refused and check mode is read-only. cooling_regression.rs is cfg(test); main.rs and the failsafe source are separate executable inputs. These changes are intended and match the reviewed cooling-safety.md repair.

`cargo tree --invert hyprv-privhelper -e normal` confirms the helper library is a normal dependency through svc-power and ui-panels, reaching the shell and bar. The old build used /tmp/hrc-1342242/rshell, while the new one used the persistent repo. Both ELFs contain their respective diagnostic build paths. Thus the changed helper-library input and build location explain why an identical shell-source tree does not promise an identical ELF; the fingerprint evidence does not isolate each input’s byte-level contribution. Python packaging/recovery changes do not link into the shell.

Exact before/after maps and build commands: `cutover/evidence/cooling-refresh-source-diff.json`. The old source snapshot was retired under disk policy, so this is a content-fingerprint comparison plus the retained repair contract, not a fabricated textual diff against deleted source.

The performance fingerprint previously omitted the linked helper library. This refresh adds it, with a regression test, so future cooling source edits cannot reuse performance receipts. No runtime behavior or acceptance threshold is changed by that readiness correction.

## Fresh evidence

Parity and nested performance regeneration completed successfully. No sudo, live desktop/configuration/service changes, installation or git commit. One shared Cargo target; incremental disabled.

## Final candidate

Release ID: **`eda5ca090166790a41ddcd171f79008f01f589c766816b0d3040b41c07be0f03`**.

Shell SHA-256: **`2073ea266e34c5aee6a69038baeaaeb75a2a287da636ee55176469f7d54c89d0`**.

Both official `packaging/build-release.sh` builds retained the exact cooling-release ELFs. The final rebuild includes only three changed readiness inputs since release 5080284888fedc55: preflight.py adds the helper fingerprint, test_perf_acceptance.py adds its regression, and perf-acceptance.json rebinds only binary_sha256. All other acceptance fields, including 0.5% / 2.0% thresholds, user/date/reason and hook policy, are unchanged. Exact before/after hashes: `cutover/evidence/cooling-refresh/readiness-only-changes.json`. Compiler/linker identification sections are identical between the old and new ELFs.

| Gate | Status |
|---|---|
| release paths | PASS |
| release binaries | PASS |
| runtime assets | PASS |
| automated tests | PASS |
| parity gate | PASS |
| performance gate | PASS (accepted: user) |
| privhelper installed | BLOCKED: installed release differs from candidate |

Fresh production GL: bar 300 passing plus 12 canonical unreachable battery exclusions, island 380/380, panels 300/300. All 992 canonical actual PNGs exist and hash-match, including eight delegated host crops and all excluded PNGs. Full screen: 26 private compositor jobs, 64/64 images pass both native and independent comparisons; 256/256 unmasked corners exact. No tolerance, exclusion or missing-image policy changed.

Fresh performance: controlled quiet **0.098131%** (0 hooks); live-hook idle **0.945095%** (114 hook-file notifications). Four >=60s uninstrumented windows from the pinned release ELF: quiet, frozen hooks/host discovery, live-hook idle and activity. All shell windows used private virtual KWin + nested Hyprland, with quiet additionally in a private PID namespace. Live legacy Quickshell was observed read-only. Raw events remain separate; none are subtracted. See [perf-shell.md](perf-shell.md).

Configured tests passed on the first final run: all-targets {'passed': 591, 'failed': 0, 'ignored': 52}, doctests {'passed': 2, 'failed': 0, 'ignored': 1}, xtask {'passed': 1, 'failed': 0, 'ignored': 0}, cutover 123 PASS, probe 4 PASS. Ignored opt-in device/live tests are excluded. The receipt, all source fingerprints, staged ELFs and manifest ID were rechecked after verification.

The final candidate preflight output is retained verbatim in `cutover/evidence/cooling-refresh/preflight.log`, with JSON and test-backed output alongside it. The only blocker is the genuinely installed old helper release; no gate was bypassed. No sudo, installation, live Quickshell/Hyprland/service change, hardware-control write or git commit occurred. All builds reused the shared target with CARGO_INCREMENTAL=0.

```text
release paths: PASS: stable deployment workspace and preflight exist: /home/wilf/.config/HyprV/rshell
release binaries: PASS: release candidate and digests verified
runtime assets: PASS: 44 runtime payloads and catalog hash-verified; unit asset root configured (staged)
automated tests: PASS: workspace all-targets + doctests + xtask + cutover/probe tests passed; opt-in live tests excluded
parity gate: PASS: production GL components and complete assembled full-screen matrix pass; battery QUIRK exclusions only; unmasked corners exact
performance gate: PASS (accepted: user 2026-10-23; controlled_idle 0.098131% <= 0.5%, hook events 0; live_idle 0.945095% <= 2%, hook events 114; User accepts current release performance for deployment; legacy Quickshell was reported at 2.7–2.9% under the same hooks. Hook events are allowed and must be reported separately; 0.1% remains the idle aspiration.)
privhelper installed: BLOCKED: installed release 5abee3cbf1fde613 differs from candidate eda5ca090166790a in packaging/dist/release
NOT READY — no cutover is authorized by this report
```
