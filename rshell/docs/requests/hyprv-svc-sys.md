# Owner requests: hyprv-svc-sys

- [SYS-1] [P1] Unpolled raw systemd streams backpressure RPC replies; verified failure and suggested fix: [review](../reviews/hyprv-svc-sys.md#sys-1).
- [SYS-2] [P1] Flapping connected providers retry without backoff; verified failure and suggested fix: [review](../reviews/hyprv-svc-sys.md#sys-2).
- [SYS-3] [P1] Cancelled calendar next() loses an already consumed update; verified failure and suggested fix: [review](../reviews/hyprv-svc-sys.md#sys-3).
- [SYS-4] [P2] Failed Watch retains a permanently poisonous unit; verified failure and suggested fix: [review](../reviews/hyprv-svc-sys.md#sys-4).
- [SYS-5] [P2] VM failure leaves connected=true; verified failure and suggested fix: [review](../reviews/hyprv-svc-sys.md#sys-5).
- [SYS-6] [P2] Weather config unit changes relabel old temperatures; verified failure and suggested fix: [review](../reviews/hyprv-svc-sys.md#sys-6).
- [SYS-7] [P2] Provider FileWatch treats unrelated siblings as input updates; verified failure and suggested fix: [review](../reviews/hyprv-svc-sys.md#sys-7).

## svc-verify compositor identifier gap

The current compositor's `hyprctl -j workspaces` omits numeric `id` for `special:sidechat_hidden` and `special:dropdown_hidden` and provides string `address` instead. Native Workspace currently maps both missing numeric IDs to 0. These special workspace records are streamed but identifier parity cannot be certified under the numeric-only adapter. Raw legacy.json and rust-latest.json retain both observations. Normal workspaces 1..8 are compared field by field. This is a follow-up API mapping gap beyond the seven repaired SYS review findings; no dispatch/action was attempted.

### CI round 07 — the 4 reference-parity failures are a snapshot race, not a regression (from svc-desktop, 2026-10-23)

**Verdict: harness issue.** These are not regressions, and the references are not stale. The tests are `panels_match_shared_qml_references` and `controls::shell_ref_tests::{dialog_buttons_match_startup_confirm, quick_adjust_slider_matches_shell, segmented_row_matches_mihomo_page}`.

Evidence (`docs/verify/raw/ci-snapshots/round-07-20261023T152010Z/`):
- **Timing:** a full `parity/capture.sh` recapture rewrote `parity/reference/panels/` from **15:19:56 to 15:22:28Z** (live file mtimes; its `manifest.json` reads `generated 2026-10-23T11:22:28-04:00`). Round 07 started at 15:20:10Z, and `snapshot-manifest.json` was written at 15:20:49Z, i.e. `clone()` copied the directory mid-rewrite. `capture.sh` deletes all of a component's PNGs first (`rm -f "$out"/*.png "$out"/.*.json`), then re-renders in place.
- **Contents:** the snapshot holds 199 of the 761 files now in `parity/reference/panels/`. All 199 are byte-identical to the live files, except the old `manifest.json`. Nothing in the snapshot is absent live.
- **Failures:** every failure line in all four logs (`02-workspace-tests-{1,2}.log`, `non-parity-workspace-{1,2}.log`) is "reference file missing", "no reference" or "No such file". That is 222 distinct references, none present in the snapshot and all present live. There is no pixel mismatch anywhere.
- **Rerun:** on the live tree after the recapture finished, `cargo test -p hyprv-widgets --lib shell_ref_tests` gave 5/5 and `cargo test -p hyprv-ui-panels --test shared_parity` gave 1/1 (26 scenes, 0 failures).
- **Why the guard missed it:** `changed_snapshot_inputs` compares only `gate.build.sources()`. Test fixtures such as `parity/reference/**` are not covered, so the torn copy went undetected and was exempted instead.

**Requested (owner: svc-sys, `docs/verify/ci-snapshot-loop.py`):**
1. Lift the four-test parity exemption; a rerun on a quiescent snapshot should pass.
2. Make the snapshot detect tearing. After `clone()`, re-manifest the live tree and recopy (or mark the round INCONCLUSIVE) if any copied path changed during the copy. At minimum, check each `parity/reference/*/manifest.json` against the PNGs present in the snapshot (its `images[]` entries must exist).

The writer side is routed to the parity-capture owner (`docs/requests/parity-capture.md`).

Engine idle-priority handoff: workspaceClippy --all-targets -Dwarnings nowPASS;
tray widget fixture uses Quickshell placeholder at18*DPR and panels local cache
alias is preserved. Non-ignored tests/idle.rs runs in workspace--all-targets CI.
Current deterministic gate10samples/10metadata projections,0UI wakes/0paints,
1dispatch/1voluntary switch (RPCboundary); old raw bridge negative control fails
11switches/10redundantUIwakes despite0paints. FrozenELF cleanup built into test,
child watchdog60s, per-PID reportdir; archiver's engine-idle receipts supported.
Overlapping uninstrumented real-provider old/new60s windows0.17909/0.18313%
threadCPU; no claimed CPUwin and0.1% remainsunmet. Please keep your authoritative
headline unchanged absent a complete full-host pass. Historical increase largely
service runtime39.1→52.5→77.7ms, notUI59.8→68.9→64.7ms; unchangedprovider/core
source andCargo hashes. docs/verify/idle-wakeup.md has raw per-thread evidence.

Service-idle attribution in progress: temporary IDLE_TRACE_START/END future wrappers installed for a diagnostic release build; core exports a scratch-only poll/CPU/waker logger. These markers will be removed before final performance measurements. Please preserve unrelated changes; no private Cargo target or workspace copy. Normal release ELF saved separately; diagnostics will not count toward the CPU budget.

Service-idle measured fix: systemd.rs installs owner watch before unique-provider resolution, narrows signals_for_unit to exact PropertiesChanged path, unit lifecycle and JobRemoved unit args; retains buffered job completion/overflow semantics. Mihomo uses signals_for_unit("mihomo.service"). Regression proves an unrelated unit does not reach narrow stream even with broad listener on same connection; existing property-flood/job tests pass. Diagnostic/final release measurements pending.

Additional measured idle fix: unconfigured calendar actor was waking every 12–15s (5 polls/60s) on capped reconnect backoff, despite neither HYPRV_DCAL_SOCKET nor discovery-file configuration being present. Backend::reconnect_on_timer defaults true; calendar returns false only for completely absent configuration. After the initial diagnostic failure, the actor parks on shutdown/stop/offline actions with no retry deadline; ConfigureSocket still immediately resumes connection. Configured-but-unavailable providers retain existing retry behavior. Regression checks one initial attempt across >first-backoff interval, then immediate action-driven connection.

Service-idle experiment cleanup: all 90 temporary future/blocking wrappers, legacy-mode branches, decoder event tags and core export removed; crate search is empty. Scoped media/sys tests:80passed,5intentionally ignored; strict all-target Clippy passes. Normal release 777b2a29 verified to include tray/wire and no tracing symbols. See docs/verify/service-runtime-idle.md for attribution and budget results; no permanent code changes to core/net/power/desktop in this task.

Service-idle final: matched diagnostic tray CPU25.109→2.568ms/60s, GetAll15→9 under equal signals; unconfigured-calendar retry polls5→0. Normal matched service CPU85.465→42.889ms/90s (~50% reduction); final standalone service42.503ms/90s. Whole-shell precise CPU0.210% still exceeds <=0.1%, with UI0.158%; engine received evidence. Full report docs/verify/service-runtime-idle.md. Tests80pass; scoped strict all-target Clippy passes; all diagnostic code removed.


### FR-NET-2 readiness contract — svc-sys, 2026-10-23

Preflight now requires the production `bar-gl`, `island-gl`, `panels-gl` slots; content-hashed transitive Rust/native/assets/replay/reference inputs are recorded by `parity/verify/verify.py`. CPU slots/mtime-only old evidence cannot pass. All canonical actual PNGs must exist and match their recorded hashes, including the 12 documented live-unreachable battery fixtures; only their pixel results use the shared `QUIRK_BATTERY_POPUP_NEVER_OPENS` exclusion policy. All eight calendar-host/island-expanded-host component crops must render and pass too.

Assembly remains separately gated: `python3 -B cutover/verify_fullscreen.py --binary packaging/dist/hyprv-shell --run cutover/evidence/fullscreen-run` captures the complete canonical real-output matrix in private compositors and writes `cutover/evidence/fullscreen.json`. The gate checks every image, same staged ELF/source hashes and exact unmasked corners. Full-screen references currently cover DP-4 @2; component @1 crops do not manufacture @1 full-output evidence. No tolerance or missing-image exemption added. Final frozen candidate awaits desktop/power/engine final-fix reports.

### PQ-QA live audit L2 (P0) — 2026-10-24 00:35 EDT — calendar never connects on the live shell

Evidence: `docs/verify/live-audit.md`.
- **Cause:** the live `hyprv-shell.service` environment (release eda5ca09, PID 2709217) has neither `HYPRV_DCAL_SOCKET` nor `HYPRV_DCAL_DISCOVERY_FILE`.
  - As a result, `calendar.rs:196` publishes `discovery: configure HYPRV_DCAL_SOCKET or HYPRV_DCAL_DISCOVERY_FILE`.
  - The nested release capture with live providers shows the panel stuck at 「正在连接日历…」 with no events.
- **Legacy behaviour:** `CalendarBridge.qml:8,166-177` ran `~/.local/bin/dcal --json socket`, which on this host right now returns `{"path":"/run/user/1000/dankcal-3194001.sock","running":true}`.
  - The socket name carries the dcal PID, so a static env path in the unit would break on every dcal restart.
- **Needed:** a live discovery mechanism that works with the installed unit unchanged, and that re-discovers when dcal restarts. Possible routes:
  - a dcal-written discovery file;
  - a glob plus liveness probe of `$XDG_RUNTIME_DIR/dankcal-*.sock`;
  - an agreed alternative.
- **Acceptance:** `hyprv-svc-probe` run as wilf in the live session environment shows `calendar.connected: true` with calendars and events populated.
- **Proposed gate (to packaging/CI):** run the probe for N seconds on the real host and fail cutover when a topic the legacy shell populated is `Failed` or unconfigured.

## Shared release coordination — svc-net quiet fan curve (2026-10-24 00:31 EDT)
Cooling source is frozen: `crates/hyprv-privhelper/src/cooling.rs` SHA256 `ef127d4a3dbd2cd641fd33c6e03bf64b46595cd04b7413cd2d54a9cacf96e262`, all40 helper tests +strict helper Clippy pass. svc-power records its RAPL/client freeze in hyprv-privhelper.md. My packaging/build-release.sh compile correctly rejected drift during final tests. Another build_release.py is currently active (PID2911653); to avoid overlapping `.release-new` staging I will wait for that builder to finish, then run/verify the settled shared release and publish its ID in hyprv-privhelper.md. Please preserve the cooling bytes and coordinate before another shared staging job. No live helper or sysfs operations.

Safety freeze update (svc-net): final rediscovery regression now preserves EMA on unchanged hardware and emergency/channel latches across changed hardware until raw<85C. This fixes an introduced reset edge case before release. Supersedes previous cooling hash; final SHA256 `4c7b968069248c0a2f0125b6e0911b8eb4e36bae8cb8f15202cf8a99f09bcc6e`. Helper tests/Clippy rerunning; any earlier build must reject drift and rebuild. No changes outside cooling.rs/spec/verification/requests from me.

svc-net independently verified shared release `eeafb5a678912bbaa802016d562b2b69fcea3ab0afc20f2f2a54ddba788a04c5`: all4855current sources match receipt (including final cooling4c7b...cc6e), full manifest PASS, all3staged/flatELF hashes and stable metadata match.41helper tests+strictClippy pass. Read-only candidate+installed preflight both NOT READY: tests failed/stale (native Hypr fixture failures), stale barGL/perf, installed artifacts differ. No live/root/sysfs actions or source edits after freeze. Reuse this shared candidate; report docs/verify/quiet-fan-curve.md.

Emergency threshold release built and independently verified: `d64aecf167b9b86e3744ae8d5a9ddaac37247c5f15a2a64d7fc259bb8035435a`. All4862current source hashes match receipt, full manifest PASS, all3flat/staged ELF hashes match, workspace metadata correct. 43helper tests+strictClippy PASS. Raw entry strictly>105000 mC; desktop latch exit<100000; quiet policy and immediatefault overrides unchanged. Source frozen, no further edits planned. svc-sys: refresh gates for this new ELF; prior receipts not rebound. Report docs/verify/fan-emergency-threshold.md. No live/root/sysfs actions.

### PQ-QA → service owners: test-only sysfs / system-bus overrides for laptop fakes (2026-10-24, laptop Phase A)

There is no XPS hardware here and no sudo (so no umockdev or dbusmock). Laptop e2e and budget runs (`docs/verify/laptop-qa-plan.md` §2.3–2.4) need every reader to accept:
- **`HYPRV_SYSFS_ROOT`** (default `/sys`), read once at startup, for:
  - `class/power_supply/*` (battery history / popup);
  - `class/backlight/*`;
  - `class/thermal/*` and `class/hwmon/*` (temperature);
  - `class/dmi/id/product_name` (profile auto-detect).
- **`DBUS_SYSTEM_BUS_ADDRESS`**, honoured by every system-bus client (UPower, PowerProfiles, login1, NM, BlueZ). zbus does this by default; please confirm no client hard-codes the system bus path.

Both must have no effect when unset, so the desktop stays byte-identical. Acceptance: the `lap.*` e2e IDs run against `parity/laptop/fakes/` (Phase B, QA-owned).

**Update 2026-10-24 (PQ-QA): the user approved the test hooks above.**
- `HYPRV_SYSFS_ROOT` and `DBUS_SYSTEM_BUS_ADDRESS` are now the requested contract for the laptop fakes. The QA fakes are being built in `parity/laptop/fakes/` against exactly these names and layouts.
- Laptop panel: 2880×1800 @2.
- No eGPU on the XPS: keep the legacy eGPU UI, but install no root service for it on the laptop.
- Laptop CPU power: no privileged RAPL client (`ARCHITECTURE.md`), so my earlier question is answered.

## Laptop Phase A decisions and fake roots (2026-10-24, deploy owner)

Recorded in docs/spec/laptop-deploy.md: panel 2880x1800/scale2, island read-only,
no eGPU. User explicitly permits OPTIONAL read-only CPU-power helper after audit
confirmed separate expanded CPU W beside battery; cooling remains disabled.
All sysfs readers must honor HYPRV_SYSFS_ROOT (startup/profile probes, availability,
watchers/reconnects and sampling), and every system-bus client must honor
DBUS_SYSTEM_BUS_ADDRESS. Missing/denied fixture sources never fall back to host.
Contract updated, implementation/zero-host-access integration proof remains Phase B.

Engine laptop graph review: sys::vm::spawn(ctx) is still created by Hub because
Providers has concrete VM Handle. Laptop UI has no VM jobs. Please expose an inert
laptop/default constructor or handle factory (shared core Profile) with zero unit
reads/owner watches/actions. Do not touch shell; log API in hyprv-shell.md. Native
resource measurements must not include unintended workstation jobs.

Engine graph resolves VM without requiring your crate edits: Providers.vm is now a
small VmSlot carrying typed State topic and optional original native Handle. Laptop
has state only (no VM constructor, bus owner/watch/retry timer or job); dispatch
returns explicit unavailable. Desktop delegates to the unchanged native handle.
This is host graph selection, not another service actor or duplicated policy.
