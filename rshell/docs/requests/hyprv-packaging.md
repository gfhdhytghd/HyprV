
## Frozen RC asset portability (2026-10-23)

Static review during FR-NET-2 candidate validation: `crates/hyprv-shell/src/assembly.rs::load_assets` defaults to `CARGO_MANIFEST_DIR/../../../quickshell`. The frozen ELF therefore points into the disposable source snapshot; `packaging/hyprv-shell.service` currently sets no `HYPRV_ASSET_ROOT`, and the release manifest does not bundle these assets. The performance harness already sets an explicit source asset root. This candidate retains the 420 KiB source assets under its archive `runtime-quickshell/assets` for replay, but an installed frozen candidate needs an explicit stable asset root or an installed asset bundle before cutover. No source/packaging mutation was folded into the already pinned candidate. No install was attempted.

## Repair in progress (2026-10-23)

The release now bundles all 44 runtime image/SVG payloads, including the Fluent subset, with an ELF-embedded SHA-256 catalog. User install puts catalog/assets/icons under `~/.local/share/hyprv-shell` in the existing binary/unit transaction, with post-copy digest validation before daemon reload. The unit sets the stable root; normal startup verifies it before services or Wayland and fails on absent/corrupt payload, with no checkout fallback. Preflight has staged/installed asset checks; `packaging/rehearse-assets.py` exercises the actual user installer and installed unit environment in scratch HOME with systemctl stubbed. Focused results: 90 cutover tests and 2 Rust integrity/root-resolution tests passed. Frozen release re-stage and full gate refresh follow.

## CLOSED — published candidate / post-snapshot rehearsal

Candidate `9a9a43e4e7e94f22fee6a70b6482f7a7cf26bf9c3507994b1462c57760361e5f` is now in `packaging/dist`, release id `1f5d20e2ff30090e7a21ae6a165638f88cd3d35ab9ec6053f5eb64e0df048071`. The default non-installing preflight's release, runtime-assets, automated-test and parity gates PASS. The actual installed-unit asset rehearsal passed again in a disposable HOME **after the compiled source snapshot was deleted**, with all 44 payloads hash-verified, explicit/default roots working, idempotent installation, and missing/corrupt asset/catalog failures. Normal startup also fails with the exact missing payload path before providers/Wayland, despite a checkout being present. No host install or real systemctl call. Full report: `docs/verify/runtime-assets.md`; proof: `docs/verify/raw/final-candidate/20261023T234946Z/assets-rehearsal-published.json`. Remaining red gates are performance (0.294618% precise quiet CPU) and uninstalled helper, independent of this packaging repair.


## Deployment workspace blocker — CLOSED in rebuilt release

Fixed frozen `/tmp/hrc-1342242/rshell` deployment metadata with shared path validation at assembly/install/preflight/root handoff; installed records bind workspace and owner. New release `5abee3cbf1fde6136b432ea51d275d5f8c37a460ba6f7d3bd0801130863cff53`; exact shell ELF unchanged. Full tests, manifest audit, scratch-HOME assets and actual staged-controller no-root handoff simulation pass. Old installed release remains blocked until the user restages/reinstalls the new ID. See [deployment-paths.md](../verify/deployment-paths.md).

### PQ-QA live audit L2 (P0) — 2026-10-24 — calendar unconfigured in the installed unit (copy; owner svc-sys)

`hyprv-shell.service` provides no dcal discovery configuration, so the live calendar never connects (`docs/verify/live-audit.md`, `docs/requests/hyprv-svc-sys.md` L2). If svc-sys's fix needs unit environment or a discovery file, packaging must install it.

Process gap: no preflight gate checks provider *availability* on the real host. That is why CPU power (L1) and the calendar (L2) shipped empty. Proposal: run `hyprv-svc-probe --duration 20` in the session environment as a non-installing preflight step, and fail if any topic the legacy shell populated reports `Failed` or `null` (power, sysstats.package_power_w, calendar, fan, network, audio, notifications, tray).

## P0 install gate refresh in progress — svc-sys, 2026-10-24
Candidate eeafb5a6 is retained as exact shell input. Three failures reproduced: tests depended on live pre-cutover Hypr inputs, now native. Replaced with owned immutable legacy fixtures recovered read-only from cutover backup, every file matched existing hypr-inputs before_sha256, with forward/reverse exact hash assertions and original rollback conflict/drift assertions intact. All124cutover tests now pass, including added mocked helper restart/health-failure file rollback tests. Test file bytes before my changes matched candidate hashes (mtimes alone were misleading). Added .lua fixture inputs to release provenance. GL component and full-screen/corner refresh underway; perf follows when GPU/build jobs quiet. I own next official release re-stage (packaging-only if compiled inputs stay exact); please do not publish concurrently or edit frozen sources. No live/root actions.

Official --repackage admitted release209c75048cb018a9212cdbf4e8bfab9a7b76ab707bf2a2cb0263c7d270d1ca60, preserving allthree exact parent ELFs; shell c364f556a1a5168a7bcac866616e1912a87b06951b7ac321b2a7a0f22d1d52df. All4862current source hashes, full manifest, staged/flat/parent binaries and perf source fingerprints independently verified. Fresh strict parity: island380/380, panels300/300, bar300pass+12documented QUIRK exclusions with all312PNGs present, full-screen64/64, exactcorners256/256. Unconditional final quiet after scoring0.154686%, zero hooks; live idle0.465925%,16hooks; unchanged .5/2caps rebound only after pass. Full preflight --candidate --run-tests is running; NOT YET final READY until its all-source receipt passes. No publisher/source edits planned.

User fan-policy addendum supersedes pre-policy209c7504. Official full release build admitted6719efa8ffa9c3d3ccba8dc82545ee2bee9a8b25976cb99260bf39bd04c56dac. All3ELFs unchanged fromeeafb5a6; shell c364f556a1a5168a7bcac866616e1912a87b06951b7ac321b2a7a0f22d1d52df. Bundled polkit SHA44a11250d2b4a6415f8a16cce66ecf077609d6171973c2bd716380ce4be07fe6: fan-control active=yes, eGPU active=auth_admin, cpu-power active=yes; any/inactive=no for all. install.py FILES_ROOT includes this manifest-verified policy at /usr/share/polkit-1/actions/org.hyprv.shell.fan-control.policy. Fresh parity and perf source fingerprints remain exact; all-source tests rerunning now. No live/root actions or helper Rust edits from svc-sys. Please hold source/publisher changes until final candidate gate result.

FINAL READY (candidate; installation not certified): release6719efa8ffa9c3d3ccba8dc82545ee2bee9a8b25976cb99260bf39bd04c56dac; shell c364f556a1a5168a7bcac866616e1912a87b06951b7ac321b2a7a0f22d1d52df. Final full all-source suite passed (618workspace,2doc,1xtask,127cutover,4probe; existing opt-in ignores unchanged). Strict GL/fullscreen/corners and accepted perf green. Fan policy active=yes; eGPU auth_admin unchanged; both policy files hash-verified in install set. Default/--installed gates remain strict and report expected old-installed mismatch. No live/root actions; original shell/helper PIDs/starttimes unchanged. No competing builder from me. docs/verify/install-gate-refresh.md and raw/install-refresh/preflight.json are the final handoff.

2026-10-24 user visual decision: stopped the owned active parity producer tree and removed unfinished emergency full-screen and current GL scratch output. New parity-acceptance.json pins user2026-10-24 sign-off, exact c364shell and hashed last fully passing6719preflight/release receipts. On-demand strict tooling unchanged; no new visual-render PASS claimed. Preflight/test/acceptance are the only3readiness changes, captured in the d64 build-receipt/binary-bound readiness amendment; no installed release or binary modified. Full automated tests and fresh installed-shell perf are being finished. User installed d64 during run; post-user-install PID baseline captured read-only.

## 2026-10-24 — Phase A laptop deployment draft (svc-sys/cutover)

`docs/spec/laptop-deploy.md` and unapplied `cutover/laptop/hypr.patch` are ready for
review; base origin/laptop 24972f711735ce92e72ef1db5bdf27f85ae8cfc9. Includes brightness,
Super+A, StartupConfirm, calendar, edge bar, variant/rotation mappings. Five Lua and
three shell syntax checks plus isolated git apply --check passed. No install/root/live
operation. Current desktop installer/controller are NOT laptop-ready; Phase B needs
user-only profile payload/drop-in and no helper handoff. New draft patch/manifest
are not shipped by d64 and are not added to its readiness-only source exemption.

## 2026-10-24 — installed d64 gate freshness versus concurrent laptop work

All five preflight test commands returned 0, including 135 cutover tests, but the
all-source collector rejected its receipt because inputs changed while running.
No runtime changes were added to the readiness-only exemption; no rebuild/install
was performed against the installed d64 release. The old binary is still c364f556.
See `docs/verify/raw/emergency-gate-refresh/post-test-source-drift.json`.
Changes observed during/after that run (mtime UTC at audit):

- `crates/hyprv-shell/src/assembly.rs` — 2026-10-24T05:49:21.419546+00:00
- `crates/hyprv-svc-desktop/src/agent_island/mod.rs` — 2026-10-24T05:45:45.204380+00:00
- `crates/hyprv-svc-desktop/src/agent_island/model.rs` — 2026-10-24T05:45:45.219380+00:00
- `crates/hyprv-svc-desktop/src/agent_island/process_regressions.rs` — 2026-10-24T05:45:45.223380+00:00
- `crates/hyprv-svc-desktop/src/lib.rs` — 2026-10-24T05:45:32.528356+00:00
- `crates/hyprv-svc-desktop/src/theme/mod.rs` — 2026-10-24T05:45:38.878374+00:00
- `crates/hyprv-svc-desktop/src/theme/ram_rgb.rs` — 2026-10-24T05:46:32.777301+00:00
- `crates/hyprv-svc-desktop/src/util.rs` — 2026-10-24T05:45:45.263380+00:00
- `crates/hyprv-svc-power/src/actor.rs` — 2026-10-24T05:47:36.157146+00:00
- `crates/hyprv-svc-power/src/backlight.rs` — 2026-10-24T05:48:27.651823+00:00
- `crates/hyprv-svc-power/src/battery_history.rs` — 2026-10-24T05:49:09.399238+00:00
- `crates/hyprv-svc-power/src/fan.rs` — 2026-10-24T05:46:31.840281+00:00
- `crates/hyprv-svc-power/src/lib.rs` — 2026-10-24T05:46:02.408396+00:00
- `crates/hyprv-svc-power/src/power.rs` — 2026-10-24T05:47:08.299053+00:00
- `crates/hyprv-svc-power/src/power/laptop.rs` — 2026-10-24T05:47:08.412458+00:00
- `crates/hyprv-svc-power/src/sysstats/laptop.rs` — 2026-10-24T05:49:09.641568+00:00
- `crates/hyprv-svc-power/src/sysstats/sample.rs` — 2026-10-24T05:49:09.400462+00:00
- `crates/hyprv-ui/src/canvas.rs` — 2026-10-24T05:48:50.115050+00:00
- `crates/hyprv-ui/src/egl.rs` — 2026-10-24T05:49:21.417578+00:00
- `crates/hyprv-ui/src/image.rs` — 2026-10-24T05:48:50.115289+00:00
- `crates/hyprv-ui/src/native_gl.rs` — 2026-10-24T05:47:34.348482+00:00
- `crates/hyprv-ui/src/qt_gl_glyphs.rs` — 2026-10-24T05:47:34.349226+00:00
- `crates/hyprv-ui/src/qt_rect.rs` — 2026-10-24T05:48:50.114357+00:00
- `crates/hyprv-ui/src/render.rs` — 2026-10-24T05:48:50.113007+00:00
- `crates/hyprv-ui/src/renderer_regressions.rs` — 2026-10-24T05:49:21.409957+00:00
- `crates/hyprv-ui/src/wayland.rs` — 2026-10-24T05:48:50.113572+00:00
- `crates/hyprv-ui/tests/nested_rotation.rs` — 2026-10-24T05:49:21.418947+00:00
- `cutover/laptop/hypr-inputs.json` — 2026-10-24T05:47:47.474951+00:00
- `cutover/laptop/hypr.patch` — 2026-10-24T05:48:16.944673+00:00

Final checkpoint: `docs/verify/emergency-gate-refresh.md` and exact installed
preflight output are recorded. Parity PASS (user acceptance), assets/paths/helper
PASS. Current source drift blocks binaries/tests and invalidates perf provenance;
fresh c364 measurements independently meet 0.5%/2.0% bounds (0.190864% quiet,
0.997420% live with 41 hook events). No new release/install or exemptions for
concurrent runtime/laptop work. Freeze/integrate those changes before renewing
current-checkout readiness; do not re-render parity under the user's acceptance.

## Laptop Phase B implemented — svc-sys stopping (2026-10-24)

Report docs/verify/laptop-impl-svc-sys.md; build --profile laptop optionally
--with-cpu-power; install --profile checks the manifest. Default desktop selection
retained. Laptop CPU-only unit uses unique hyprv-cpu-power-only.service; five-file
root set omits cooling/fan/eGPU. User-only transactional cutover is plan-only by
default, and laptop preflight never reuses desktop waivers. README documents QA
receipt bindings; joint204/laptop parity/perf not stamped or claimed. Helper52,
core5, cutover150, strictClippy and release CPU probe pass; patch stays unapplied.
No global candidate build/stage/install or live operation by this agent.
