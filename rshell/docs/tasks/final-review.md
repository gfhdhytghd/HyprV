# Task: final go/no-go review (independent)
Two reviewers (svc-desktop = Claude, svc-net = Codex) independently assess whether rshell is ready for the user to cut over. Work read-only except your own report file.
Check, with evidence (run things, don't just read reports):
1. Feature parity: every QML feature/IPC/bind in docs/spec and docs/spec/COVERAGE.md has a working Rust path (e2e matrix docs/verify/e2e.md, equivalence docs/verify/equivalence.md). List anything missing or only mocked.
2. Visual parity: re-run parity/verify/verify.py (GL) and the full-screen matrix on a fresh snapshot; confirm numbers.
3. Safety: privhelper/fan failsafe, network secrets, user config edits (~/.claude, ~/.codex), cutover/rollback scripts (dry-run only). Any unresolved P1/P2 in docs/reviews/*?
4. Performance/memory vs Quickshell (docs/verify/perf-shell.md, engine-memory.md, ui-tick.md); stability (soak.md, gpu-corruption.md).
5. Preflight: run cutover/preflight (non-installing) and report each gate.
6. Docs: README.md and CUTOVER.zh-CN.md accurate vs current scripts.
Output: docs/verify/final-review-<your-name>.md with a GO / NO-GO verdict, blockers, and non-blocking risks. Reply with the verdict and top 5 points.
