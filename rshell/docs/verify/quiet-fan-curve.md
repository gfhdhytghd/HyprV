# Quiet chassis fan curve — 2026-10-24

Approved deliberate departure from legacy, implementing the user's **superseding quiet settings**. Production changes are confined to `crates/hyprv-privhelper/src/cooling.rs`; the cooling section of `docs/spec/services.md` now specifies this exception. RAPL/main/D-Bus/policy edits owned by svc-power were left untouched; coordination recorded in `docs/requests/hyprv-privhelper.md`.

| Setting | Named constant / value |
|---|---|
| Start | `FAN_START_MC = 72000` |
| Stop | `FAN_STOP_MC = 65000` (strictly below) |
| Ramp reaches cap | `FAN_FULL_MC = 88000` |
| Reliable floor | `MIN_START = 77` |
| EMA time constant | `TEMPERATURE_EMA_SECONDS = 10` |
| Running rise/fall rates | `PWM_RISE_PER_SECOND = 10`, `PWM_FALL_PER_SECOND = 2` |
| Raw emergency enter/exit | `EMERGENCY_MC = 90000`, `EMERGENCY_EXIT_MC = 85000` (exit strictly below) |

Normal cap is floor(control-center percentage *255/100), independently per channel. For a running fan the target is linear from min(77,cap) at72C to cap at88C, saturated at the floor/cap; a zero cap stays off and clears only its own running latch, so re-enabling below72C waits for the start threshold again. Start/stop hysteresis uses smoothed temperature. EMA uses `alpha = 1-exp(-dt/10)` with monotonic elapsed time, seeded by the first valid sample. Short spikes below the raw emergency threshold do not immediately spin the fans. Additional limit calls do not add fake seconds to smoothing or slew. Fractional PWM credits are retained between subsecond updates.

Safety and floor precedence are explicit: off->running jumps to min(77,cap), never ramping through stalled single-digit duties; stop below65C goes to0. A lowered cap, including0 and the normal cap on emergency exit, clamps immediately. During normal running the output then slews at +10/-2 PWM per elapsed second. Raw>=90C, failed telemetry or invalid saved state immediately overrides filter/cap/slew to255, retaining the emergency until the fault clears and raw<85C. Individual write faults set a channel-local high-duty latch; healthy channels still run independently. At low temperatures fault recovery clears the latch and honors the normal cap. Slew tracks actual hardware duty, including out-of-band failsafe raises, while retaining fractional credit when hardware still matches the previous command; shutdown/crash maximum-speed recovery is unchanged.

Boot/non-desktop still uses the legacy raw-temperature curve with80% limit: zero through70C, linear70->88C, cap above88C, and immediate255 on raw>=90C/fault. It does not apply the new desktop EMA/slew/hysteresis or change saved desktop limits. Returning to desktop seeds a fresh curve from current telemetry/duty. Rediscovery preserves EMA/output history when topology is unchanged; changed topology resets that history but preserves safety latches until healthy raw temperature is below85C. Single-store integer writes and manual-enable-before-PWM ordering remain intact.

Verification uses fake sysfs and deterministic elapsed time only. No sudo, installation, live sysfs store or live helper stop/restart was performed. Shared Cargo target is reused with incremental disabled. Release/preflight receipts follow after completion; no stale parity/performance evidence is relabelled.

## Verification

- Final helper suite: **41 passed, 0 failed** (37 library,3 main,1 packaging contract). Includes steady ramp/cap checks for every percentage0..100, start/stop boundaries, low/zero caps, EMA short-spike rejection and time composition, no artificial advancement on repeated requests, rise/fall/subsecond slew, raw emergency and fault latching, independent channel faults, zero-cap/re-enable hysteresis, out-of-band failsafe duty reconciliation, and emergency/EMA preservation on rediscovery. A fake-sysfs controller test exercises actual writes and return telemetry at deterministic instants; legacy boot output and saved-limit persistence remain covered. Existing single-store EINVAL and enable-before-PWM regressions still pass. [Test log](quiet-fan-curve/helper-tests.log).
- Strict helper Clippy: **PASS**, `cargo clippy --locked -p hyprv-privhelper --all-targets -- -D warnings`. [Log](quiet-fan-curve/helper-clippy.log).
- Source freeze: `cooling.rs` SHA-256 `4c7b968069248c0a2f0125b6e0911b8eb4e36bae8cb8f15202cf8a99f09bcc6e`. Both svc-net and svc-power recorded ownership/source freezes in requests; no RAPL/main/protocol/policy changes from this task.
- Full configured preflight tests were run by svc-power in parallel, with no competing receipt writer here. It reported workspace all-targets,doctests,xtask,probe Python success, but cutover Python **120 pass /123**: one preimage assertion plus two drift-rollback fixtures assume legacy Hypr files, whereas the live checkout is now patched/native. [Owner log](cpu-package-power/cutover-tests.log) confirms1 failure,2 errors. That run also spanned the final cooling edits, so its source receipt is stale. These gates remain red; no edits to legacy Hypr files or test exemptions were made.


## Admitted shared release

Release ID: **`eeafb5a678912bbaa802016d562b2b69fcea3ab0afc20f2f2a54ddba788a04c5`**. Built by the official full `packaging/build-release.sh` (svc-sys, after the recorded source freezes); this task's earlier full-build attempts correctly rejected concurrent source drift. Reused the single admitted shared candidate without competing publication. Payload: `packaging/dist/release`. Nothing installed here.

Independent verification after every builder exited: all **4855** current source hashes equal `packaging/dist/build.json`; its cooling hash is the final frozen value above; manifest SHA equals release ID; **every manifest entry passes** `sha256sum --strict --quiet -c`; all three staged ELFs equal the flat dist ELFs and receipt hashes; stable workspace metadata and source-map digest match the actual checkout. [Compact release receipt](quiet-fan-curve/release.json).

| Binary | SHA-256 |
|---|---|
| hyprv-shell | `c364f556a1a5168a7bcac866616e1912a87b06951b7ac321b2a7a0f22d1d52df` |
| hyprv-privhelper | `76921a20326e113878748be5202e59da2e42b014c10f778f3e363e1a042ef7b1` |
| hyprv-cooling-failsafe | `bc33d2bf9d8982051e74346137776c9d6b8b33453d311decce9a016d8d4b9e9a` |

## Fresh read-only preflight

Both candidate and `--installed` checks returned exit1, **NOT READY**. No waiver, screenshot/performance receipt rebinding, test weakening, installation, or live mutation.

| Gate | Candidate | Installed |
|---|---|---|
| release paths | PASS | PASS |
| release binaries | PASS | BLOCKED: installed user binary/unit differs from candidate |
| runtime assets | PASS | PASS |
| automated tests | BLOCKED: automated test receipt failed or stale; run preflight --run-tests | BLOCKED: automated test receipt failed or stale; run preflight --run-tests |
| parity gate | BLOCKED: bar-gl: render dependency hashes stale or changed during render/diff | BLOCKED: bar-gl: render dependency hashes stale or changed during render/diff |
| performance gate | BLOCKED: real-shell performance source evidence stale; rerun perf-shell measurement | BLOCKED: real-shell performance source evidence stale; rerun perf-shell measurement |
| privhelper installed | BLOCKED: installed release eda5ca090166790a differs from candidate eeafb5a678912bba in packaging/dist/release | BLOCKED: installed release eda5ca090166790a differs from candidate eeafb5a678912bba in packaging/dist/release |

[Candidate JSON](quiet-fan-curve/preflight.json), [installed JSON](quiet-fan-curve/preflight-installed.json). Automated all-source tests remain failed/stale because the recorded full suite spans source edits and has three native-checkout fixture failures, despite the 41 helper tests passing. Bar GL evidence and whole-shell performance evidence require regeneration for the merged sources/ELF. Installed shell/root release still differ. These are honestly retained blockers, not a claim that the full shell passes deployment readiness. The user will perform installation.

No live sysfs writes, sudo/root execution, or live helper stop/restart were performed. No private Cargo targets, frozen source trees or large captures were created; shared target reused with incremental disabled and free space stayed above72GiB. Other owners' RAPL/main/policy and renderer/calendar/desktop changes were preserved.
