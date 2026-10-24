# Preparation validation — 2026-10-23

No install, switch, rollback, root handoff, live Lua edit or compositor reload was applied. Only dry-run variants of those scripts were invoked. Build/test tools ran locally; their outputs are not readiness approval.

| Check | Result |
|---|---|
| Cutover/installer regression tests | **17 passed** (`python3 -B -m unittest discover -s cutover/tests -p 'test*.py'`) |
| Bash syntax + ShellCheck, all prepared wrappers | PASS |
| Python compilation, prepared scripts | PASS |
| Lua syntax of all three staged postimages | PASS, via hyprland-lua check script; live files untouched |
| `git -C .. apply --check "$PWD/cutover/hypr.patch"` | PASS; check only |
| Original Lua hashes vs hypr-inputs.json | PASS; exact preimages still present |
| User-unit structural/order verification | PASS on an owned staged copy with ExecStart/ExecStop replaced by /usr/bin/true, because the real executable is not installed; no unit was loaded or started |
| Production release compilation, including helper + failsafe | Cargo release compile succeeded with fat LTO; **staging receipt refused** because input sources changed during compilation |
| Workspace all-targets and doctests attempt | FAILED to compile during concurrent shell-host edits: recorded E0308 cursor `Option` type mismatches in hyprv-shell/src/assembly.rs; no global test-pass claim |
| xtask tests | 1 passed |
| Existing probe Python tests | 4 passed |
| Current pre-flight | **NOT READY**: release receipt missing, automated suite failed/stale, parity red/stale, full live service CPU 0.3492% > 0.1%, privileged helper absent |
| A/B requested-output mode | Explicit UNSUPPORTED, exit 4, no actions |
| Live Hyprland runtime read-back | PENDING: read-only version/status queries could not connect using the old Quickshell process's compositor socket; no new Lua APIs were introduced |

The 17 tooling tests exercise dry-run no-effects (including root plans), strict parity boundaries and complete image coverage, rejection of debug/short/excluded/frozen/high-CPU performance, preflight-before-stop ordering, journal persistence/symlink refusal, original unit-state restoration, maximum-cooling request on stop failure, root rollback rejection while native runs, exact main-Quickshell selection, unapplied patch inventory, and delayed IPC/output readiness.

Detailed local logs/receipts: `.check/tests.log`, `.check/systemd-verify.log`, `.check/release-build.log`, `.check/preflight-current.json`, and `evidence/test-*.log` / `evidence/tests.json`. These runtime artifacts are ignored. Current receipts intentionally do not turn green merely because one component/test/isolated performance run passes. Later concurrent fixes require rebuilding and regenerating the full receipts.

Canonical privhelper packaging symlinks and their targets were read, not edited. The root installer and transition plan consume the existing independent cooling failsafe, protected state directory, notify/watchdog unit and polkit policy. Actual systemd sandbox execution, authorization, early boot, hardware behavior, compositor lifecycle and rollback execution remain untested; those are deferred manual operations after gates pass.

## Round-6 review fixes — 2026-10-23

CUT-1..7 from `docs/reviews/packaging-cutover.md` are fixed. Only dry-runs were executed. `docs/reviews/scratch/round6/fix-dryruns.log` has every README dry-run, with before/after snapshots of the state dirs, `/var/lib`, `/usr/local/lib`, `/usr/libexec`, the helper mask link, unit states, hypr lua hashes and `git status`: **no mutation**.

| Check | Result |
|---|---|
| Cutover/installer tests | **38 passed** (`python3 -B -m unittest discover -s cutover/tests -p 'test*.py'`), incl. `test_review_fixes.py` (21 regressions for CUT-1..7, release assembly and dry-runs) |
| Live staged root install, switch fallback, rollback | NOT executed (dry-runs only); covered by mocks/temp trees |
