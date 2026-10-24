# Deployment workspace repair — 2026-10-23

Release ID: `5abee3cbf1fde6136b432ea51d275d5f8c37a460ba6f7d3bd0801130863cff53`.

Shell SHA-256 (unchanged): `9a9a43e4e7e94f22fee6a70b6482f7a7cf26bf9c3507994b1462c57760361e5f`.

The previous release recorded its deleted frozen build workspace. The rebuilt manifest records `/home/wilf/.config/HyprV/rshell`. The shared path validator checks the workspace, required files, Hyprland configuration directory and owner HOME; rejects absent, relative, temporary, snapshot and symlink-to-temporary references; and runs during assembly, install verification, preflight and the staged root handoff. Installed metadata now records and checks workspace/owner as well.

Packaging-only rebuild used `packaging/build-release.sh --repackage`: every non-packaging input (including runtime assets and release profile) and all three ELF digests matched the prior receipt. Original build receipts are retained by SHA-256 as historical evidence; they are not active deployment references. No Rust runtime changes or binary changes occurred, so existing exact-binary GL/performance evidence remains valid. Future snapshot builds must explicitly supply HYPRV_DEPLOY_WORKSPACE; normal receipts use source-relative build arguments rather than embedded snapshot paths.

| Gate | Status |
|---|---|
| release paths | PASS |
| release binaries | PASS |
| runtime assets | PASS |
| automated tests | PASS |
| parity gate | PASS |
| performance gate | PASS (accepted: user) |
| privhelper installed | BLOCKED — replace old installed release |

Full configured tests passed: workspace all-targets, doctests, xtask, 112 cutover tests, and probe tests. The first run hit the existing no-system-bus test’s 700 ms reconnect assertion; its focused retry and the complete rerun passed without changing Rust sources.

Audit: all 60 manifest files exist and hash-verify. Active release metadata, build receipt command, units, catalog and staged tools have no dependency on the deleted build tree. Asset/catalog paths are bundle-relative and verified; unit paths use the stable user installation. Remaining CARGO_MANIFEST_DIR references in the ELF belong to explicit replay/nested/functional diagnostic modes, not ordinary startup; the normal installed asset loader was re-proven in scratch HOME.

No-root proof: `packaging/rehearse-handoff.py` imports the actual staged control.py, calls root_handoff(switch) with only the root/trust context simulated, intercepts its first command, resolves TREE to the real repo, and executes that repo’s actual --installed preflight as wilf. No runuser/systemctl/root/hardware command executes. All non-installing gates pass there too; exit 1 retains the genuine old-installed-release blocker. `packaging/rehearse-assets.py` also passed with a scratch HOME and stubbed systemctl.

Evidence: `cutover/evidence/deployment-path-{audit,handoff,assets,preflight}.json` and `deployment-path-preflight.log`.

**Deployment action required:** restage and reinstall this new release ID before root step 2b. The old installed release is deliberately rejected for its /tmp/hrc-1342242/rshell metadata. No live install, switch, rollback, compositor reload or privileged operation was performed for this repair.
