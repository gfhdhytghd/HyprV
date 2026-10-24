# Install gate refresh — 2026-10-24

**READY for candidate installation; installed state is not certified.**

Release ID: `6719efa8ffa9c3d3ccba8dc82545ee2bee9a8b25976cb99260bf39bd04c56dac`. Shell SHA-256: `c364f556a1a5168a7bcac866616e1912a87b06951b7ac321b2a7a0f22d1d52df`.
Official `packaging/build-release.sh` rebuilt/staged the final user-approved policy revision. All three release ELFs remain byte-identical to original candidate eeafb5a6; all4862 current source hashes and every manifest entry match. [Independent release check](raw/install-refresh/release-check.json).

## Test-fixture cause and attribution

The three failures were pre-existing dependence on the live Hypr checkout being legacy: one test expected preimage hashes and two applied the patch to files already patched during deployment. Before this repair, both test files still matched the original candidate's content hashes despite their newer04:49:11UTC mtimes. No post-build content editor can be attributed from those timestamps; available tool-call history did not identify who changed the metadata. The original svc-sys fixtures assumed a legacy live checkout. [Attribution and original hashes](raw/install-refresh/fixture-attribution.json), [reproduced failures](raw/install-refresh/cutover-before.log).

Owned fixtures were recovered read-only from the cutover journal backup and verified against every existing before_sha256. The patch, original hashes, rollback drift/conflict assertions, namespace checks and mode-preservation assertions were retained. Tests now apply/reverse the real patch in a temporary tree and require every recorded postimage/preimage hash. Lua fixture inputs were added to release/test provenance. No live Hypr file was changed, and no test was weakened or exempted.

## Final gates

| Gate | Result |
|---|---|
| Persistent release paths, binaries, full manifest | PASS |
| Runtime assets | PASS:44 payloads and catalog |
| Workspace all-targets | PASS: 618 passed, 53 existing opt-in tests ignored |
| Workspace doctests | PASS: 2 passed, 1 existing opt-in ignored |
| xtask | PASS: 1 passed |
| Cutover / probe Python | PASS:127 /4 tests |
| Production component GL | PASS: island380/380; panels300/300; bar300 pass +12 documented battery QUIRK exclusions; all312 bar PNGs present/hash-verified |
| Assembled full-screen / unmasked corners | PASS:64/64 images;256/256 corners exact |
| Accepted performance | PASS: quiet0.154686% <=0.5%,0hooks; live0.465925% <=2%,16hooks |
| Candidate helper installation set | PASS: all6 manifest-verified helper/unit/D-Bus/polkit inputs |
| Installed-artifact admission | BLOCKED as expected: installation is the user's next step |

All five configured test commands returned0 on the final source revision. No source changes occurred across the final receipt; explicitly ignored live/device/stress tests remain separate, never presented as executed. [Full test receipt](raw/install-refresh/tests.json), [final full-suite preflight](raw/install-refresh/preflight-policy-run-tests.log), [candidate JSON](raw/install-refresh/preflight.json), [installed JSON](raw/install-refresh/installed-preflight.json).

Fresh rendering used private compositors and the exact candidate ELF. Every image and dependency is hash-verified; non-text exactness and existing text tolerance are unchanged. No absent image, arbitrary exclusion, copied stale receipt or masked corner is accepted. [Parity check](raw/install-refresh/parity-check.json), [full-screen producer](raw/install-refresh/fullscreen.log), [GL producer](raw/install-refresh/parity-gl.log).

Performance used the archived actual release shell under private KWin/Hyprland, uninstrumented >=60s windows, read-only providers and no host input. Calendar traffic was explicitly routed read-only to the existing live socket across the private runtime; the installed helper was not replaced, so new RAPL runtime behavior remains installation-dependent. No current legacy family exists; the historical comparison is identified as historical. The first quiet run overlapped CPU-only scoring and was retained as diagnostic; the selected replacement ran after all scoring and other windows, unconditionally rather than selecting a minimum. The accepted .5/2limits, user/date/reason were unchanged; only the exact binary binding changed after measurements passed. The0.1% aspiration remains unmet and documented. [Binding receipt](raw/install-refresh/perf-acceptance-binding.json), [performance report](perf-shell.md).

## Fan password-prompt policy and root installer

User changed the canonical fan action to allow_active=yes, with any/inactive=no; message updated. eGPU removal remains auth_admin; CPU telemetry remains active=yes. The new XML regression checks all three actions and all session defaults. No remaining current test/doc asserts auth_admin_keep. The packaging symlink resolves to this canonical policy; the staged regular file matches SHA-256 `44a11250d2b4a6415f8a16cce66ecf077609d6171973c2bd716380ce4be07fe6` and is installed by FILES_ROOT at `/usr/share/polkit-1/actions/org.hyprv.shell.fan-control.policy`. The new D-Bus policy is also in FILES_ROOT at `/etc/dbus-1/system.d/org.hyprv.Shell.PrivHelper.conf`. Caller UID checks and the helper's polkit authorization call are unchanged. Runtime absence of a prompt has not been exercised on the old installed policy.

`packaging/install.py:229` confirms native-phase upgrade behavior: changed files are journalled atomically; for an enabled helper it reloads systemd, restarts hyprv-privhelper and checks active state before publication. Restart/health failure restores old files, reloads and restarts the previous helper, without publishing the new release. Added mocked regressions test both failures and every file; existing interruption/mid-commit rollback cases still pass. Read-only observation confirms the actual old helper is enabled/active. No root installer was applied.

Explicit `--candidate` checks the staged helper install set and labels installation unverified; default and `--installed` retain their strict actual-installed checks. The user preflight invoked by cutover still uses `--installed`. [Installer tests](raw/install-refresh/test-3.log).

## Final candidate preflight output

```text
release paths: PASS: stable deployment workspace and preflight exist: /home/wilf/.config/HyprV/rshell
release binaries: PASS: release candidate and digests verified
runtime assets: PASS: 44 runtime payloads and catalog hash-verified; unit asset root configured (staged)
automated tests: PASS: workspace all-targets + doctests + xtask + cutover/probe tests passed; opt-in live tests excluded
parity gate: PASS: production GL components and complete assembled full-screen matrix pass; battery QUIRK exclusions only; unmasked corners exact
performance gate: PASS (accepted: user 2026-10-23; controlled_idle 0.154686% <= 0.5%, hook events 0; live_idle 0.465925% <= 2%, hook events 16; User accepts current release performance for deployment; legacy Quickshell was reported at 2.7–2.9% under the same hooks. Hook events are allowed and must be reported separately; 0.1% remains the idle aspiration.)
privhelper install set: PASS: 6 manifest-verified helper/unit/D-Bus/polkit install inputs; installed state NOT certified (candidate mode)
READY (candidate; installation not certified)
```

No sudo, install, commit, live shell/helper/Hyprland restart/reload, or hardware write was performed. Live shell2709217 and helper2716595 retain their original process start times. [Proof](raw/install-refresh/live-unchanged.json). Deleted two obsolete owned frozen candidate copies (~639MiB); no private Cargo target was created. Shared target, CARGO_INCREMENTAL=0; root free space remains about72GB, above the30GB floor.
