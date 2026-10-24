# Release idle CPU repair and same-window comparison

Release builds use Cargo opt-level=3 (isolated workspace default release profile; no fat LTO). Before/after dependencies are frozen; only power actor.rs changes. Runtime traces are from a separate instrumented debug copy and are excluded from all performance intervals. No native actions were dispatched.

The live main Quickshell process and its descendants include rendering and UI work. The probe contains services and JSON publication only: parity with Quickshell CPU would not satisfy the services budget. Target: **at most 0.1% of one core**, with only genuine event activity at idle. The separate DankCal Quickshell config is excluded from both families.

The first before/after pair changes only power actor.rs. `release-latest` additionally includes concurrently landed desktop improvements; it is a separate checkpoint. `release-latest-no-agent` excludes the active agent actor. `release-frozen-agent` retains every actor but freezes hook inputs after one initial copy and points the agent app-server observer at a probe-owned absent endpoint. It is an explicit idle fixture, not a claim that the live active desktop consumes that amount.

## Release measurements

| Mode | Family | Seconds | CPU seconds | CPU % of one core | RSS start/end KiB | Scheduling proxy / s | Context switches / s |
|---|---|---|---|---|---|---|---|
| release-before | probe | 60.108 | 1.580 | 2.6286 | 35064 / 37336 | 882.538 | 882.554 |
| release-before | quickshell | 60.108 | 1.770 | 2.9447 | 329460 / 335372 | 79.490 | 79.490 |
| release-after | probe | 60.145 | 0.550 | 0.9145 | 32692 / 32692 | 194.797 | 194.780 |
| release-after | quickshell | 60.145 | 1.680 | 2.7933 | 337072 / 337112 | 74.953 | 74.953 |
| release-power-before | probe | 10.144 | 0.150 | 1.4787 | 9464 / 9656 | 838.494 | 838.494 |
| release-power-before | quickshell | 10.144 | 0.250 | 2.4644 | 336844 / 336956 | 47.613 | 47.613 |
| release-power-after | probe | 60.151 | 0.000 | 0.0000 | 9504 / 9696 | 1.795 | 1.795 |
| release-power-after | quickshell | 60.151 | 1.630 | 2.7098 | 336972 / 337564 | 66.615 | 66.615 |
| release-latest | probe | 60.138 | 0.210 | 0.3492 | 25548 / 25804 | 117.580 | 117.597 |
| release-latest | quickshell | 60.138 | 1.600 | 2.6606 | 323576 / 322472 | 49.453 | 49.453 |
| release-latest-no-agent | probe | 60.236 | 0.010 | 0.0166 | 23936 / 23936 | 3.885 | 3.885 |
| release-latest-no-agent | quickshell | 60.236 | 1.710 | 2.8388 | 322468 / 322636 | 66.388 | 66.388 |
| release-frozen-agent | probe | 60.150 | 0.070 | 0.1164 | 24500 / 24696 | 65.170 | 65.170 |
| release-frozen-agent | quickshell | 60.150 | 1.610 | 2.6766 | 322616 / 323092 | 72.535 | 72.535 |

Each interval begins after 8 seconds warmup. Demand-driven guards are released at 5 seconds; event subscriptions remain. Power-only excludes net/media/sys/desktop; fan helper was absent and was never activated. Baseline power-only is a 10-second diagnosis, after power-only is 60 seconds. Full-probe before and after are 60 seconds.

release-before: state publications `{'agent_island': 111, 'tray': 4, 'hyprland': 4}`, exclusions `[]`, frozen agent inputs `False`. [Raw counters, binary digest and source hashes](raw/perf-release-before.json).
release-after: state publications `{'agent_island': 94, 'systemd': 2, 'hyprland': 7}`, exclusions `[]`, frozen agent inputs `False`. [Raw counters, binary digest and source hashes](raw/perf-release-after.json).
release-power-before: state publications `{}`, exclusions `['net', 'media', 'sys', 'desktop']`, frozen agent inputs `False`. [Raw counters, binary digest and source hashes](raw/perf-release-power-before.json).
release-power-after: state publications `{}`, exclusions `['net', 'media', 'sys', 'desktop']`, frozen agent inputs `False`. [Raw counters, binary digest and source hashes](raw/perf-release-power-after.json).
release-latest: state publications `{'agent_island': 59, 'hyprland': 4}`, exclusions `[]`, frozen agent inputs `False`. [Raw counters, binary digest and source hashes](raw/perf-release-latest.json).
release-latest-no-agent: state publications `{'hyprland': 4, 'systemd': 1}`, exclusions `['agent_island']`, frozen agent inputs `False`. [Raw counters, binary digest and source hashes](raw/perf-release-latest-no-agent.json).
release-frozen-agent: state publications `{'hyprland': 4, 'systemd': 2}`, exclusions `[]`, frozen agent inputs `True`. [Raw counters, binary digest and source hashes](raw/perf-release-frozen-agent.json).

Latest live full-probe CPU budget: **NOT MET** (0.3492%). Event-conditioned interpretation and power-only measurements remain separate from this numerical budget.

release-power-after: **PASS** against 0.1% process CPU; named-thread schedstat CPU 0.003265 s (0.0054%). Genuine input activity and excluded/frozen inputs are listed separately above.

release-latest: **NOT MET** against 0.1% process CPU; named-thread schedstat CPU 0.213801 s (0.3555%). Genuine input activity and excluded/frozen inputs are listed separately above.

release-latest-no-agent: **PASS** against 0.1% process CPU; named-thread schedstat CPU 0.009528 s (0.0158%). Genuine input activity and excluded/frozen inputs are listed separately above.

release-frozen-agent: **NOT MET** against 0.1% process CPU; named-thread schedstat CPU 0.066454 s (0.1105%). Genuine input activity and excluded/frozen inputs are listed separately above.

## Exact task and cause

Named-thread /proc counters attribute the baseline hot loop to `hyprv-services`. Scratch tracing identifies `hyprv_svc_power::power::Native`: all four provider inboxes receive global DBus NameOwnerChanged and each other's PropertiesChanged. Power refreshes exceed 256 in eight seconds without a matching power-state change. [Before event trace](raw/power-event-trace.log), [after event trace](raw/power-event-trace-after.log). After demand release the after trace has one genuine UPower PropertiesChanged and one refresh, not a sequence of unrelated-owner refreshes. `strace` was unavailable; no tracing was attached to Quickshell or another process.

zbus MessageStream matching cannot resolve a well-known sender locally. With multiple rules on one shared connection, the broad provider inboxes receive signals admitted by unrelated rules. Each accepted event triggers the expensive power snapshot. This is event amplification, not a timer polling loop.

Repair in power actor.rs: resolve providers to unique owners, keep absent-provider inboxes pending, and subscribe to NameOwnerChanged with interface/member/path and per-provider arg0 filters. Owner watches precede connect/snapshot; replacement rebuilds streams. The independent drain, bounded overflow behavior and reconnect backoff from the concurrent review repairs remain intact.

Regression: unrelated PropertiesChanged admitted by a second rule triggers a refresh on the before source and is rejected by the repaired source; genuine provider signals still arrive. A second private-bus test covers irrelevant owner traffic, initially absent providers and owner replacement. 17 power tests and strict all-target clippy pass. [Negative control](raw/power-idle-before-regression.log), [tests](raw/power-idle-tests.log), [clippy](raw/power-idle-clippy.log).

## Per-thread evidence

| Mode | Thread | CPU seconds (schedstat) | Scheduling timeslices |
|---|---|---|---|
| release-before | probe-release-b | 0.068696 | 142 |
| release-before | hyprv-services | 1.187608 | 52719 |
| release-before | tokio-rt-worker | 0.082281 | 51 |
| release-before | hyprv-pw-audio | 0.000000 | 0 |
| release-before | tokio-rt-worker | 0.078459 | 43 |
| release-before | tokio-rt-worker | 0.073993 | 42 |
| release-before | tokio-rt-worker | 0.082409 | 51 |
| release-before | module-rt | 0.000000 | 0 |
| release-after | probe-release-a | 0.048895 | 133 |
| release-after | hyprv-services | 0.239423 | 11442 |
| release-after | tokio-rt-worker | 0.077803 | 48 |
| release-after | hyprv-pw-audio | 0.000073 | 4 |
| release-after | tokio-rt-worker | 0.080917 | 43 |
| release-after | module-rt | 0.000000 | 0 |
| release-after | tokio-rt-worker | 0.086330 | 46 |
| release-power-before | probe-release-b | 0.000000 | 0 |
| release-power-before | hyprv-services | 0.142714 | 8506 |
| release-power-after | probe-release-a | 0.000000 | 0 |
| release-power-after | hyprv-services | 0.003265 | 108 |
| release-latest | probe-release-l | 0.030107 | 89 |
| release-latest | hyprv-services | 0.183694 | 6982 |
| release-latest | hyprv-pw-audio | 0.000000 | 0 |
| release-latest | module-rt | 0.000000 | 0 |
| release-latest-no-agent | probe-release-l | 0.000463 | 6 |
| release-latest-no-agent | hyprv-services | 0.009065 | 228 |
| release-latest-no-agent | hyprv-pw-audio | 0.000000 | 0 |
| release-latest-no-agent | module-rt | 0.000000 | 0 |
| release-frozen-agent | probe-release-f | 0.000548 | 14 |
| release-frozen-agent | hyprv-services | 0.065906 | 3906 |
| release-frozen-agent | hyprv-pw-audio | 0.000000 | 0 |
| release-frozen-agent | module-rt | 0.000000 | 0 |

## Measurement limits

schedstat scheduling timeslices and voluntary/involuntary context switches are **proxies, not exact kernel wakeups**. Endpoint scans are not atomic. Exited threads/short-lived children have incomplete scheduling counts; process CPU includes reaped child time. CPU tick resolution is 0.01 seconds on this host; a zero-tick 60-second interval means below roughly 0.017%, not mathematical zero. RSS is a family sum and includes shared pages more than once.

This desktop has active agent hooks, tray and compositor events. Their publications are recorded rather than suppressed. No panels were hidden, services restarted, credentials used, hardware controls changed or privileged tracing enabled. Debug historical measurements are retained in [perf-debug-history.md](perf-debug-history.md) and are not the release before/after evidence. Build snapshots are in [before](raw/release-before-build-snapshot.json) and [after](raw/release-after-build-snapshot.json).
