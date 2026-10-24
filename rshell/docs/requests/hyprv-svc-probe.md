

### Integration CI — 2026-10-23T08:15:04+00:00 — Preflight performance gate

Non-installing `cutover/preflight.sh --run-tests --json`: full service CPU exceeds 0.1% or invalid: 0.34919832902451553. See `docs/verify/raw/ci/round-01/preflight.log`. Please regenerate valid current evidence after the underlying owner changes pass; do not relax the gate. This is gate routing, not proof that every routed crate caused the failure.


### Integration CI — 2026-10-23T12:32:43+00:00 — Preflight performance gate

Non-installing `cutover/preflight.sh --run-tests --json`: real-shell performance source evidence stale; rerun perf-shell measurement. See `docs/verify/raw/ci/round-13/preflight.log`. Please regenerate valid current evidence after the underlying owner changes pass; do not relax the gate. This is gate routing, not proof that every routed crate caused the failure.

Snapshot CI observation (14:40 UTC): CalendarCorrelation.test_greeting_is_not_a_method_reply failed twice at scripts/test_verify.py:33 on snapshot 0fbe42ab02e66c2bf330d872acb7f74e135cceef1b5c912968920f0ad6249f47 because its repository-relative Unix socket path exceeds the kernel path limit under long scratch roots. Logs docs/verify/raw/ci-snapshots/round-02-20261023T143232Z/07-probe-tests-{1,2}.log. CI will use shorter timestamped snapshot roots; test semantics remain unchanged. Optional robustness improvement: allocate the socket in a short private tempfile directory.
