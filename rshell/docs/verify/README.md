# Service verification checkpoint

- [Real assembled release shell](perf-shell.md): 60-second nested idle/activity, same-window live Quickshell CPU/scheduling/RSS/GPU memory, separate agent-input conditions, and real-shell idle-loop regressions.
All five service crates build in the isolated probe workspace. 33 state/error topics were observed; unavailable fan is explicitly skipped to avoid privileged autoactivation. No actions were dispatched.

- [Equivalence](equivalence.md): 3,648 PASS, 1 MISMATCH, 17 TEMPORAL, 28 UNAVAILABLE. Laptop battery selection remains a mismatch; Rust selects a Bluetooth peripheral while legacy reports no BAT* battery. Repeated records inflate counts; see [coverage](coverage.md).
- [Performance](perf.md): **release** before/after and named-thread evidence. Power-only improves from 1.4787% to ~0.0054% CPU; latest full live services use 0.3492% versus Quickshell 2.6606%. Whole live <=0.1% budget remains unmet; no-agent is 0.0166%, all-actor frozen-agent fixture is 0.1164% with genuine systemd/Hyprland events. [Repair details](idle-cpu-repair.md).
- [Sys review repairs](sys-review-fixes.md): all seven findings fixed with regressions; 30 tests pass, strict clippy passes, both systemd Version reads pass.
- [Validation](validation.md): full probe build, strict probe clippy, one demand lifecycle test and four verifier tests pass.

Owner follow-ups are appended under docs/requests for power, desktop, media and sys. Full raw references, JSONL topic streams, comparison values, process endpoints, build hashes and logs are in raw/. Final captured build hashes identify the tested checkpoint; concurrent later owner edits are not implicitly verified. Original source/status paths and all excluded action/visual/provider branches are listed in coverage.md. Nothing was installed, committed, restarted or controlled.

- [62-minute isolated release soak](soak.md): activity/hotplug replay, RSS/PSS/fd/thread/GPU/kernel-timer trends, render/submission p50/p99, retention follow-up and raw evidence.
