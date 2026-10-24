# Quiet UI wakeups and the reported CPU regression

The stats bridge now ignores history/generation and sub-display changes when the
bar's displayed text, interface or severity is unchanged. Visible Stats/Panel
consumers still receive every raw sample. This fixes a reproducible redundant
UI wake; it does **not** establish that the three historical CPU measurements
were caused by a renderer feature.

The ordinary, non-ignored integration test in
`crates/hyprv-shell/tests/idle.rs` runs under `cargo test --workspace --all-targets`,
which is already the snapshot CI workspace-test command. Explicit reproduction:
`hyprv-shell --nested-idle-test OUT`.

## Deterministic native compositor regression

The production assembly, native EGL frames/callbacks and host deadlines run in
a private compositor with mocked services. A 1Hz sampler changes generation,
elapsed time and history on the service runtime, through the production
`forward_projected` bridge. Bar display values remain unchanged. After animations
settle, the fixture-only16ms animation driver is dropped; no production deadline
is disabled. The real minute clock stays installed; measurement starts away
from its minute boundary.

| 10.1-second case | Samples | Metadata-driven UI wakes | Main dispatches | Main voluntary switches | Paints | Gate |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Projected bridge |10 |0 |1 |1 |0 |PASS |
| Old raw bridge negative control |10 |10 |11 |11 |0 |FAIL, expected |

The single positive-case dispatch/switch is the measurement RPC boundary. The
gate also checks that samples reached the projector; a dropped subscription
cannot pass silently. Voluntary switches are Linux scheduling proxies, not exact
`sched_wakeup` trace events. The negative control is
`HYPRV_IDLE_RAW_STATS_CONTROL=1 hyprv-shell --nested-idle-test OUT` and must return
nonzero despite painting zero frames. This demonstrates why repaint-only tests
missed the redundant wakes.

Receipts: `target/idle-priority/ci-manifest.json`, `ci-idle.csv`,
`negative-control/manifest.json`, `ci-idle-final.log`. Test artifacts use unique
PID directories and a60second child watchdog; frozen test executables are
removed after completion. The
full204-action E2E report is preserved; these filtered checks write local reports.

## Source-window audit and real-provider measurements

The hourly receipts from `docs/verify/perf-shell.md` decompose as follows:

| Hourly receipt, UTC | Summed thread CPU | Main UI runtime /60s | Service runtime /60s | Service voluntary switches |
| --- | ---: | ---: | ---: | ---: |
|16:57 |0.1686% |59.813ms |39.107ms |559 |
|17:40 |0.2060% |68.882ms |52.450ms |519 |
|18:24 |0.2411% |64.708ms |77.713ms |1884 |

The latest rise is primarily the service thread, while main UI time fell.
Core/provider Rust source hashes, Cargo.lock, core Cargo.toml and shell
Cargo.toml are unchanged across these three archived source manifests. The changed-source windows include UI/host/panel/renderer work;
older source copies/ELFs were retired under disk policy, so a full historical
commit-by-commit rebuild bisect cannot be claimed from the remaining hashes.

Same-old-ELF profiling controls today (`f73c3062459c6acbbfba3886ef05ba8c9fea07bae61b6cfb304a0557d608d933`):

| Case,60s | Summed thread CPU | Tray pixmap changes | Stats notifications | Paints |
| --- | ---: | ---: | ---: | ---: |
| Calendar prep enabled |0.2263% |13 |61 |75 |
| Calendar prep disabled |0.3060% |22 |61 |84 |

Those are instrumented, sequential observations, not proof that disabling
preparation increases idle cost: genuine tray inputs differ. Preparation itself
has no calls in either measured interval; it runs during initialization. This
variation on one ELF exceeds the claimed historical increase. No native-GL,
font-mapping or calendar rollback is justified by those snapshots alone. The
trace accounts for actual stats and tray changes, rather than a repaint or
icon-loader feedback loop.

An overlapping, **uninstrumented**60s old/new release comparison using the same
read-only real service graph and quiet agent fixtures gives:

| ELF | Summed thread CPU | UI runtime | Service runtime | Main voluntary switches |
| --- | ---: | ---: | ---: | ---: |
| Previous `f73c3062` |0.17909% |54.154ms |50.842ms |247 |
| Stats-projected `f9c3ad98` |0.18313% |54.987ms |52.623ms |248 |

These whole-source checkpoints include concurrent owner changes, so the0.004%
difference is not assigned to this patch. **No significant real-provider CPU
improvement is claimed**, and the<=0.1% total CPU budget remains unmet. The
0.241% regression was not reproduced in the matched windows. The deterministic
negative control isolates and proves the metadata-wake fix independently.

A separate new-ELF instrumented window records UI53.111ms/244voluntary switches,
service85.724ms/2711switches,61stats notifications and zero tray UI notifications.
Residual service-thread IO/task-poll attribution is still open; evidence was
filed for svc-media/core and svc-power, not mislabeled as native rendering work.

Raw real-provider receipts: `docs/verify/raw/perf-shell/idle-priority-{before,
no-prepare,after,paired-before,paired-after}/`; compact summary
`target/idle-priority/measurements.json`. Current pinned release:
`target/idle-priority/current/hyprv-shell`, SHA256
`f9c3ad983b6a5bdb83b12ff8c55e99733f05513ad36a52dce9861cad204c95d6`.
Only private compositor processes were driven; providers were read-only.

## CI failures and validation

The tray fixture retains its historical `tray-fallback-letter` filename, but
Quickshell successfully resolves a missing theme icon to its checker image.
The widget comparison now supplies the checker at18×DPR pixels. The no-image
letter fallback remains unchanged. Original behavior: `TrayButton.qml:15-38`.

The panels cache already contains spec-bar's minimal local `FontDataCache` type
alias. It was preserved and verified, rather than overwritten; the receipt is
logged in `docs/requests/hyprv-ui-panels.md`.

`cargo clippy --workspace --all-targets -- -D warnings` passes. Latest affected
library run:181passed,0failed (48shell,57UI,16panels,60widgets;16intentional
ignored diagnostics). The native idle integration test passes separately. The
bar stats projection regression checks the actual public bar layout, including
same-rounded-temperature/different-icon and same-power-text/different-severity
boundaries, and nonfinite-versus-unavailable power. Original rounded labels:
`shell.qml:3847-3859`; implementation contract:
`hyprv-ui-bar/src/layout.rs:209-225,441-450` and `format.rs:5-15,61-77`.
