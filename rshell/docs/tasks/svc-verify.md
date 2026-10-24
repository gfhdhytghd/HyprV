# Task: svc-verify
You own crates/hyprv-svc-probe (new binary crate) and docs/verify/.
1. `hyprv-svc-probe` starts every hyprv-svc-* crate's modules (net, media, power, sys, desktop — add each as it becomes buildable; others are still being written) in READ-ONLY mode (never dispatch actions), subscribes to all topics and streams state as JSON.
2. Equivalence check vs the legacy implementation: for each QML-consumed value, run the ORIGINAL read-only status script / QML data path from ../quickshell/scripts (only status/read scripts, never action scripts) and compare field-by-field with the Rust state (after mapping per services-api.md). Write results to docs/verify/equivalence.md, with mismatches explained and reported to the owning agent by appending to docs/requests/<crate>.md.
3. Wakeup/CPU measurement: run the probe for 60s idle and record wakeups (/proc/<pid>/schedstat, /proc/<pid>/task/*/stat ctx switches, or perf if available unprivileged), CPU time, RSS; compare with the current quickshell process + its children over the same window (read-only observation). Results in docs/verify/perf.md. Target: ~0 wakeups/s idle beyond genuine events.
Do not edit other crates. No sudo, no git commit, no actions that change system state.
Reply with a short summary when done; rerun as more crates land.
