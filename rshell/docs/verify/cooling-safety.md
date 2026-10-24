# P0 cooling handoff safety repair — 2026-10-23

No root execution, live systemd mutation, sysfs store, installation, compositor reload or live journal modification was performed. The user-restored legacy units were left alone. Hardware inspection used read-only sysfs and the installed module ELF.

## Confirmed failure and driver contract

`Chip::write` used `writeln!(file, "{value}")`. The formatting implementation issues a write for the integer and another for the newline. Sysfs interprets **each syscall as a complete store**: the integer can already change hardware, then newline-only parsing fails with EINVAL. The stateful fake-driver regression reproduces this exact partial-effect/error sequence. Previous regular-file fixtures accepted fragmented writes and missed it. The fallback also attempted PWM before selecting manual mode, which the driver rejects while automatic control is selected.

Driver evidence: [upstream `store_pwm` / `store_pwm_enable`](https://github.com/torvalds/linux/blob/master/drivers/hwmon/nct6775-core.c), [kernel attributes](https://docs.kernel.org/hwmon/nct6775.html#sysfs-attributes), the retained [source excerpt](cooling-safety/driver-semantics.txt), and [installed-module disassembly](cooling-safety/installed-driver-mode-checks.txt). The installed module's mode checks match the source: values above 5 rejected; 4 restricted to nct6775; 5 checks trip-point consistency. nct6799 channels 1 and 7 expose the same store callbacks: 0 (maximum), 1 (manual), 2 (Thermal Cruise), 3 (Speed Cruise), 5 (SmartFan IV, conditional on valid points). Mode 4 is invalid. PWM is clamped to 0..255; manual duty writes in auto mode return EBUSY. Unsupported-mode or malformed input returns EINVAL. We do not select automatic modes or assume their firmware settings are safe.

Read-only [host receipt](cooling-safety/hardware-readonly.json): nct6799 at `/sys/devices/platform/nct6775.656/hwmon/hwmon3`, both channels expose mode/enable/duty, enable=1 and PWM output mode=1. No attempt was made to empirically switch modes: operational automatic-mode acceptance cannot be established without writes. Driver validation establishes the accepted enum; only manual 1 and duty 255 are used by recovery.

## Changes

- Failsafe and normal manual transitions select enable=1 **before** writing duty. High-duty recovery writes exactly 255, using one integer-plus-newline syscall per changed attribute. Short stores fail without retrying a second fragment. Channels remain independent; corrupt persistence, failed sensors and teardown still trigger high duty. Duplicate nct6799 chips are refused.
- Failsafe/helper `--check` reads identity and attributes; privileged callers additionally check write-open permission without storing anything. No bus connection, state lock or controller startup occurs in check mode.
- Root switch verifies readiness and both capabilities, then actually runs failsafe while legacy is still available, before stopping/disabling any legacy unit. It journals the baseline before mutation. Stop, failsafe, permission, helper-start/readiness and final journal failures enter immediate legacy restoration before returning an error.
- Rollback/recovery attempt capability and high duty before stopping the helper. They disable/mask it and confirm actual inactive/failed state before granting legacy access. A broken old failsafe is a warning, never a veto on restoring old cooling. Permission and unit errors are accumulated while remaining restoration steps continue. Actual active states and unit-file states are checked; incomplete recovery retains the original baseline and a retryable phase.
- `recover --scope system` reconciles a stuck `switching` journal, including when the user has already restored legacy manually. It does not stop those legacy units first, recapture a partial baseline, require readiness, alter the user journal/UI, or proceed to native cutover. A switch retry on `switching` recovers and returns 2, requiring a deliberate fresh switch later.
- Root installer treats `switching` as pre-cutover and keeps an inactive helper masked. A running helper in that phase requires recovery first. Only native phase permits automatic helper restarts on upgrade.

## Recovery command

After staging and verifying **this corrected release** with the system-tool commands in [install-plan.txt](cooling-safety/install-plan.txt), invoke its exact path (this also works before installation; do not invoke the old `current` tool):

```sh
sudo /usr/bin/python3 -I -B /usr/local/lib/hyprv-cutover/releases/5080284888fedc55/cutover/control.py recover --scope system --apply
```

This command is provided for the user; it was not run here. Do not delete `/var/lib/hyprv-cutover/state.json`. Expect phase `legacy` only after baseline links/permissions and actual unit states agree. Persistent stop/permissions/start failures retain `rollback-incomplete`; rerun the same command after resolving the reported error. If helper cannot be confirmed stopped, legacy access is not granted to avoid two simultaneous controllers. SIGKILL/power loss cannot execute in-process rollback; the durable original journal supports this explicit recovery.

## Validation and release

Release ID: **`5080284888fedc55ee8eb7bf50f83077d32aedebc8c83006c555832d9f6296de`**. Candidate: `packaging/dist/release`. Root staging directory: `/usr/local/lib/hyprv-cutover/releases/5080284888fedc55`. Full source/binary receipt remains `packaging/dist/build.json`; compact [release receipt](cooling-safety/release.json) records critical input hashes and ELFs.

| Artifact | SHA-256 |
|---|---|
| hyprv-shell | `2073ea266e34c5aee6a69038baeaaeb75a2a287da636ee55176469f7d54c89d0` |
| hyprv-privhelper | `b5aff1e245724bf8be9f8da70ec5841e4063ea498c09a9a4c2ae578cef33cc2e` |
| hyprv-cooling-failsafe | `3531ff43078aa3771d6d246f873610aa331dc22e6d7015d9da7bef45502bb92d` |

The shell ELF changed despite no shell/UI source edits: it depends on the helper crate through svc-power/UI-panels. Old shell performance/installation receipts must not be relabelled for this ELF.

- Production `packaging/build-release.sh`: PASS. The initial build refused a source change while the new tests were being completed; the settled-source rerun passed. Manifest `sha256sum --strict --quiet -c`: PASS, manifest SHA equals the release ID above.
- Helper crate tests: **26 PASS** (22 library, 3 main, 1 packaging contract), including four stateful driver-store regressions. [Log](cooling-safety/rust-tests.log).
- Cutover/installer tests: **122 PASS**. New cases cover first/partial stop failures, post-stop EINVAL, permission errors, helper startup/readiness failures, journal failure, exact link recreation, repeated/manual recovery, old failed failsafe, and installer behavior in `switching`. [Log](cooling-safety/python-tests.log).
- Strict helper `cargo clippy --locked -p hyprv-privhelper --all-targets -- -D warnings`: PASS. [Log](cooling-safety/clippy.log).
- Both exact staged release ELFs passed `--check` on the real host as uid 1000, with no sysfs stores, root actions or D-Bus activation. [Failsafe](cooling-safety/failsafe-check.txt), [helper](cooling-safety/helper-check.txt).
- Read-only post-change [unit states](cooling-safety/live-units-readonly.txt): all three legacy services active/enabled; helper inactive/masked.

Final preflight results follow below. Recovery is independent of cutover readiness; no check was bypassed or acceptance receipt rewritten.


Preflight was run with `--run-tests --json`, then rechecked both as a candidate and with `--installed`. Fresh configured-suite receipt is **PASS**: workspace all-targets **591 passed, 52 ignored, 0 failed**; doctests **2 passed, 1 ignored**; xtask **1 passed**; cutover **122 passed**; probe Python **4 passed**. Opt-in device/live tests remain excluded. The initial workspace attempt failed the unchanged desktop `real_late_exec_is_signalled_by_fanotify` assertion; isolated rerun passed, then the complete configured suite passed. The [initial failure](cooling-safety/workspace-first-failure.log) and [isolated rerun](cooling-safety/fanotify-isolated-rerun.log) are retained; this does not claim that desktop race has been diagnosed or fixed. [Final suite receipt](cooling-safety/tests-final-receipt.json).

| Gate | Candidate | Installed |
|---|---|---|
| release paths | PASS | PASS |
| release binaries | PASS | BLOCKED: installed user binary/unit differs from candidate |
| runtime assets | PASS | PASS |
| automated tests | PASS | PASS |
| parity gate | BLOCKED: bar-gl: render dependency hashes stale or changed during render/diff | BLOCKED: bar-gl: render dependency hashes stale or changed during render/diff |
| performance gate | BLOCKED: measured real-shell binary differs from staged candidate | BLOCKED: measured real-shell binary differs from staged candidate |
| privhelper installed | BLOCKED: installed release 5abee3cbf1fde613 differs from candidate 5080284888fedc55 in packaging/dist/release | BLOCKED: installed release 5abee3cbf1fde613 differs from candidate 5080284888fedc55 in packaging/dist/release |

**NOT READY for another native cutover.** [Candidate JSON](cooling-safety/preflight.json), [installed JSON](cooling-safety/preflight-installed.json). The corrected release is built and verified locally, not installed. Stale bar render fingerprints and old-ELF performance/acceptance receipts were deliberately not rewritten or bypassed. User binary and root release installation must also match before a future switch. None of these readiness gates prevents the explicit root-owned recovery command above from reconciling the interrupted journal to legacy.

Only the helper cooling implementation/check entry points, system cutover/installer recovery, their tests, the recovery guide, owner-request notes, and this verification evidence were edited. Shared Cargo target was reused with incremental disabled; free disk stayed above 87 GiB. Temporary driver copies/scripts were deleted after retaining compact source/hash evidence. The live root journal and session remain untouched.


Subsequent full parity/performance refresh: [cooling-gate-refresh.md](cooling-gate-refresh.md). The current staged candidate is `eda5ca090166790a41ddcd171f79008f01f589c766816b0d3040b41c07be0f03`, retaining exactly the same three cooling-release ELFs. This refresh changed readiness fingerprints/acceptance binding only; it performed no deployment or root recovery. The earlier release and commands above remain historical cooling-repair evidence.
