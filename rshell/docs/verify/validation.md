# Verification commands and evidence

2026-10-23, isolated build workspace; shared workspace lockfile untouched.

| Check | Result | Retained log |
|---|---|---|
| Full default probe build, all five service crates | PASS | [build](raw/probe-build.log) |
| Probe strict all-target clippy (`--no-deps -- -D warnings`) | PASS | [clippy](raw/probe-clippy.log) |
| Probe subscription-demand regression | 1 passed | [Rust tests](raw/probe-tests.log) |
| Procfs arithmetic, partial JSONL and calendar greeting/correlation tests | 4 passed | [Python tests](raw/probe-python-tests.log) |
| svc-sys tests including all seven review regressions | 30 passed | [sys tests](raw/sys-review-tests.log) |
| svc-sys strict all-target clippy | PASS | [sys clippy](raw/sys-review-clippy.log) |
| systemd Version read on user + system bus | 1 passed | [read-only buses](raw/sys-review-read-only-bus.log) |

Sys repair details: [sys-review-fixes.md](sys-review-fixes.md). Equivalence and performance are separate observed outcomes, not implied by build/test success. Their raw source fingerprints identify the precise captured build. The task intentionally dispatched zero native actions and performed no installation, commits, sudo, compositor/shell reloads or service control.

Concurrent source drift after measurement is listed in [post-checkpoint-drift.json](raw/post-checkpoint-drift.json). It is mainly privileged-helper implementation and desktop follow-up work; the tested source snapshot remains frozen.

## Release idle CPU repair validation

Only power actor.rs changed in the controlled comparison. Private-bus noise regression fails on before source and passes after; absent provider, owner filtering/replacement and all three concurrent review regressions remain passing. 17 power tests pass, strict all-target power clippy passes. The latest full probe also passes its demand regression and strict clippy; four Python verifier tests pass. Builds use Cargo --release opt-level=3, without instrumentation or copied root fat-LTO settings. [Power evidence](idle-cpu-repair.md), [latest power tests](raw/power-idle-latest-tests.log), [latest power clippy](raw/power-idle-latest-clippy.log), [probe tests](raw/idle-fixture-tests.log), [probe clippy](raw/idle-fixture-clippy.log). No controls/actions or service restarts were performed.
