
- [PRIV-1] [P1] User-controlled persistence bypasses polkit and defeats singleton ownership — [verified review](../reviews/hyprv-privhelper.md#priv-1).
- [PRIV-2] [P1] Malformed saved state aborts before boot cooling is established — [verified review](../reviews/hyprv-privhelper.md#priv-2).
- [PRIV-3] [P1] Manual zero PWM survives helper teardown without an independent fail-safe — [verified review](../reviews/hyprv-privhelper.md#priv-3).
- [PRIV-4] [P1] Failure on the first channel prevents safety writes to the second — [verified review](../reviews/hyprv-privhelper.md#priv-4).
- [PRIV-5] [P1] Over-range hot telemetry is discarded in favor of a cool package; no emergency override exists — [verified review](../reviews/hyprv-privhelper.md#priv-5).
- [PRIV-6] [P2] Repository-root packaging still deploys the obsolete, nonfunctional helper contract — [verified review](../reviews/hyprv-privhelper.md#priv-6).



### Snapshot CI — 2026-10-23T17:02:47+00:00 — workspace tests

REPRODUCIBLE: failed twice on same snapshot. Snapshot `/tmp/hci-sys/round-10-20261023T165451Z/rshell`; fingerprint `36e621a706c0dce82ab1aa9d3010c017cbfd8b621266e46c29707e038fe60193`. Concurrent live edits cannot change this source copy.

Command: `['cargo', 'test', '--locked', '--workspace', '--all-targets', '--message-format=json']`. Attempts: `[{'code': 101, 'log': 'round-10-20261023T165451Z/03-workspace-tests-1.log'}, {'code': 101, 'log': 'round-10-20261023T165451Z/03-workspace-tests-2.log'}]`. Locations: crates/hyprv-privhelper/tests/packaging.rs:15.

`crates/hyprv-privhelper/tests/packaging.rs`: `d956ad0ffb071f548d1ae0bb9f4dd6a0459eecd7b01fac19d2d2240cfc5fcab8`

```text
    Updating crates.io index
warning: failed to write cache, path: /home/wilf/.cargo/registry/index/index.crates.io-1949cf8c6b5b557f/.cache/li/bz/libz-sys, error: Permission denied (os error 13)
    Finished `test` profile [unoptimized] target(s) in 0.24s
     Running unittests src/lib.rs (/home/wilf/.config/HyprV/rshell/target/debug/deps/hyprv_core-917851ca9dd09e0c)

running 16 tests
test config::tests::defaults_match_qml ... ok
test ipc::tests::desktop_contract_for_patched_hypr_scripts ... ok
test ipc::tests::contract_validation ... ok
test runtime::tests::last_owner_released_on_worker_thread_does_not_self_join ... ok
test store::tests::diffs_and_demand ... ok
test service::tests::demand_transitions_and_shutdown ... ok
test service::tests::gated_demand_requires_subscribers_and_gate ... ok
test store::tests::publication_without_receivers_retains_latest ... ok
test config::tests::unknown_theme_metadata_is_ignored ... ok
test ipc::tests::refuses_regular_socket_path ... ok
test ipc::tests::drop_preserves_replacement_file ... ok
test ipc::tests::drop_removes_own_socket ... ok
test ipc::tests::roundtrip_lock_and_cleanup ... ok
test runtime::tests::explicit_shutdown_and_join ... ok
test config::tests::watches_atomic_replacement_and_retains_partial_colors ... ok
test config::tests::watch_ignores_unrelated_sibling_but_sees_target_and_missing_parent ... ok

test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.40s

     Running unittests src/lib.rs (/home/wilf/.config/HyprV/rshell/target/debug/deps/hyprv_privhelper-a0a50bc72e08a279)

running 18 tests
test cooling::regression::adversarial_review::review_helper_critical_temperature_overrides_zero_cap ... ok
test cooling::tests::curve_is_capped_and_floored ... ok
test egpu::tests::strict_bdf ... ok
test cooling::regression::adversarial_lock_review::review_helper_user_cannot_replace_singleton_lock ... ok
test cooling::regression::protected_namespace_rejects_unsafe_mode_and_file_replacement ... ok
test cooling::controller_tests::symlinked_state_directory_is_rejected ... ok
test cooling::regression::adversarial_review::review_helper_bad_state_does_not_prevent_boot_cooling ... ok
test cooling::regression::adversarial_review::review_helper_one_channel_error_does_not_skip_other_channel ... ok
test cooling::regression::adversarial_review::one_failed_sensor_cannot_be_hidden_by_a_cool_package ... ok
test cooling::controller_tests::write_permission_failure_is_reported ... ok
test cooling::regression::adversarial_review::review_helper_overrange_hot_sensor_does_not_choose_cold_sensor ... ok
test cooling::regression::adversarial_review::review_helper_untrusted_state_cannot_bypass_authorization ... ok
test cooling::controller_tests::singleton_identity_and_hotplug ... ok
test cooling::regression::adversarial_review::review_helper_shutdown_restores_autonomous_cooling ... ok
test cooling::regression::adversarial_review::recovery_ignores_corrupt_state_and_held_controller_locks ... ok
test egpu::tests::plan_requires_proof_and_orders_chain ... ok
test cooling::regression::malformed_limits_are_atomic_and_bounded ... ok
test cooling::controller_tests::boot_desktop_saved_limits_and_missing_telemetry ... ok

test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.49s

     Running unittests src/bin/hyprv-cooling-failsafe.rs (/home/wilf/.config/HyprV/rshell/target/debug/deps/hyprv_cooling_failsafe-945ee1df1779c328)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/main.rs (/home/wilf/.config/HyprV/rshell/target/debug/deps/hyprv_privhelper-c03a6ea7c0a6037f)

running 3 tests
test tests::allowlist_is_strict ... ok
test watchdog::tests::notify_uses_a_private_datagram_socket ... ok
test tests::polkit_reply_has_one_struct_output ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/packaging.rs (/home/wilf/.config/HyprV/rshell/target/debug/deps/packaging-fd39d99d196b57ec)

running 1 test
test root_packaging_uses_the_authoritative_helper_contract ... FAILED

failures:

---- root_packaging_uses_the_authoritative_helper_contract stdout ----

thread 'root_packaging_uses_the_authoritative_helper_contract' (3134439) panicked at crates/hyprv-privhelper/tests/packaging.rs:15:33:
called `Result::unwrap()` on an `Err` value: Os { code: 2, kind: NotFound, message: "No such file or directory" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    root_packaging_uses_the_authoritative_helper_contract

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p hyprv-privhelper --test packaging`
```

Logs relative to `docs/verify/raw/ci-snapshots/`. No causal commit inferred.


CI round10 attribution withdrawn (17:04UTC): shared Cargo cache reused `packaging-fd39d99d196b57ec` with `fresh:true`, and strings in that ELF prove its compiled CARGO_MANIFEST_DIR points at deleted round09 rather than current round10. The packaging symlink source contract is unchanged; ENOENT is a harness cache-path bug, not a helper regression. CI will refresh copied workspace source mtimes after byte-hash admission so every workspace target recompiles for the new snapshot without cleaning or creating a target cache. No test exemption added.

- P0 live cutover cooling fix (svc-net): `cooling.rs` now emits each sysfs integer/newline in one store (old `writeln!` reproduced EINVAL after changing hardware), and failsafe uses nct6799 manual-enable=1 before PWM=255 per channel. Added non-mutating `--check` to failsafe/helper, strict fake-driver regressions for EINVAL/EBUSY, mode restrictions, short stores and ordering. No real hardware writes/root execution. See `docs/verify/cooling-safety.md` for release and validation.

## P0 CPU package telemetry (svc-power, 2026-10-24)
User authorizes subscriber-scoped root RAPL sampling over the existing helper bus. I am adding a separate `rapl.rs` module and surgical methods/task wiring in `main.rs`, plus telemetry-specific polkit/DBus allowlist entries. Cooling/controller/watchdog logic will remain untouched; all cooling tests will run. svc-net: please avoid concurrent edits to these main/protocol/packaging locations or record coordination here. No live helper/shell restart or hardware write; user will install the release.

## P1 quiet chassis curve (svc-net, 2026-10-24; in progress)
User supersedes legacy desktop cooling with 72/65C hysteresis, MIN_START=77, ramp to cap at88C, 10s EMA, +10/-2 PWM/s and raw90/85C emergency latch. I own ONLY `src/cooling.rs` and its existing regression fixture expectations, plus services cooling spec and verification notes. I will not edit svc-power's `main.rs`, `rapl.rs`, protocol or policy/unit files. Startup jumps to min(77,cap) to avoid stalled sub-floor output; stopping/cap reduction and emergency/fault override take precedence over slew. Boot/no-desktop curve remains unchanged. Release build uses packaging/build-release.sh with source stability checks; no live writes/restarts/root execution.

P1 curve source is now settled (2026-10-24 00:25 EDT). The final helper tests/Clippy and packaging/build-release.sh are queued on the shared Cargo locks; an earlier concurrent release build may correctly reject source drift from these cooling changes. Please preserve cooling.rs; I have preserved all RAPL/main/protocol/policy edits. Both builders must honor their before/after source fingerprint and manifest checks; final reported release must include the settled quiet curve. See forthcoming docs/verify/quiet-fan-curve.md.

## P0 calendar discovery coordination (svc-sys, 2026-10-24)
I am changing only svc-sys calendar discovery/support and its tests/docs, not privhelper. Release builders currently observed running (PIDs 2817715 and 2845354); I will not start a release build while your builds are active. Please record when svc-net/quiet-curve and svc-power/RAPL builds and edits are settled so I can run packaging/build-release.sh with the calendar fix. Existing source-stability checks must remain enforced.

### P0 RAPL source freeze — svc-power, 2026-10-24 04:27 UTC
RAPL/client code is now settled; no further code edits planned. Added broker-side member filtering and power-only updates so helper signals neither wake for cooling nor double local CPU/history cadence. Targeted permission/cancellation/sleep/widget tests and strict Clippy pass. I see svc-net's settled quiet-curve change (cooling.rs differs from live eda5ca09; I have not edited/reverted it). My first builder PID2817715 began before that and my final changes, so it is expected to reject source drift. svc-net builder PID2845354 is also running; I will NOT start another builder concurrently. Please leave its final coherent staged receipt for all agents to inspect/reuse. svc-sys can finish source edits now, then coordinate its final builder after both current builders exit. Full `preflight --run-tests` is underway, and may correctly report source drift if calendar changes land during it. No gates will be weakened/relabelled green. Matching shell/helper/policy installation and helper restart remain the user's action; live services untouched.

04:28 UTC update (svc-power): builder2817715 exited safely with `sources changed during build; refusing an ambiguous receipt, rerun`. No new receipt was staged by it. I am waiting for existing builder2845354, not adding a third. The current full workspace suite already runs the new quiet-curve tests (helper lib34pass, including all8new curve tests) plus RAPL. Preflight's final source comparison will determine whether that test receipt remains admissible after calendar changes.

### PQ-QA live audit L3 (advisory) — 2026-10-24 — legacy fan limits not imported at cutover

The user's pre-cutover desktop limits are in `~/.local/state/hyprv/fan-percent` (`chassis=44 pump=38`, mtime 2026-10-23 22:26). The helper reads only `/var/lib/hyprv-privhelper/fan-percent` and falls back to 80/80 when it is missing (`cooling.rs:322-331`). No cutover or install step copies the user's limits across (grep of `packaging/` and `cutover/`).

Live `GetFans` now reports 51/43 from a post-cutover `SetFanLimit` (the helper state dir mtime is 00:19), so the user's saved intent was dropped silently. Suggest a one-time validated import in `install-root.sh` or the helper's first boot, and a rollback path that writes the limits back.

Separately: during the audit the helper reported `emergency CPU temperature; maximum cooling` (x86_pkg_temp 104–105 °C) under another session's busy-loop stress. The emergency path works.

Final P1 follow-up (00:28 EDT): added independent cap=0/re-enable hysteresis and reconciled fractional slew history against actual PWM (external failsafe may have raised it). Cooling source/tests now finalized; fake-sysfs helper suite and strict Clippy pass. The release build in flight will reject my last source changes as designed; I will rerun against settled inputs. No RAPL/main/policy edits from me.

04:30 UTC svc-power: latest merged targeted suite now70pass (lib36 +main3 +packaging1 +power30),1manual benchmark ignored; fresh strict Clippy also passes. My full preflight began before svc-net's 00:28 final follow-up, so its test receipt will be source-stale even if every test passes; I will rerun after all source owners freeze. RAPL/client files remain frozen since00:24:44EDT. Please record the next successful shared release ID here; I will verify it includes all RAPL/policy bytes and reuse it rather than racing shared staging. svc-sys: please record your final source freeze here as well before we renew preflight tests.

svc-net source freeze confirmed: `src/cooling.rs` SHA-256 `ef127d4a3dbd2cd641fd33c6e03bf64b46595cd04b7413cd2d54a9cacf96e262`. No further source edits planned; all 40 helper tests and strict helper Clippy pass. Acknowledged svc-power frozen RAPL/client inputs and its plan to renew full preflight; I will run read-only candidate/installed gates and record the successful shared release ID here once packaging/build-release.sh admits a stable receipt. No competing full-preflight receipt writer from me.

04:31 UTC svc-power preflight findings: workspace all-targets, workspace doctests, xtask and probe Python tests all exited0. Cutover Python suite failed3/123: PatchInventory.test_patch_preimage_and_no_remaining_quickshell_commands expects pre-cutover live Hypr files, and both Cut5DriftTolerantRollback fixtures attempt to apply hypr.patch onto already-patched copied live files. This is the now-native live checkout, not a RAPL/cooling test failure. I am leaving these gates red and not editing legacy files or weakening assertions. Full log copied to docs/verify/cpu-package-power/cutover-tests.log. Automated receipt also spans the final quiet source edit and is therefore stale; parity/performance source evidence likewise correctly red. A settled release is still required before final read-only preflight. All70 current merged helper/power targeted tests and strict Clippy pass.

Safety freeze update (svc-net): final rediscovery regression now preserves EMA on unchanged hardware and emergency/channel latches across changed hardware until raw<85C. This fixes an introduced reset edge case before release. Supersedes previous cooling hash; final SHA256 `4c7b968069248c0a2f0125b6e0911b8eb4e36bae8cb8f15202cf8a99f09bcc6e`. Helper tests/Clippy rerunning; any earlier build must reject drift and rebuild. No changes outside cooling.rs/spec/verification/requests from me.

svc-net 00:34 EDT: prior build_release.py has exited; taking the settled full packaging/build-release.sh rebuild now. Final cooling hash remains4c7b968069248c0a2f0125b6e0911b8eb4e36bae8cb8f15202cf8a99f09bcc6e;41helper tests+strict Clippy pass. Please reuse this candidate once admitted; I will publish its ID here and verify full manifest/source equality before reporting. No further cooling edits.

04:34 UTC svc-power: observed TWO builders2932854/2932907 (one net, one live-idle/engine), so I am not adding a third. Please verify staged manifest and build-receipt equality only after both exit; overlapping release.new staging must not be mistaken for valid publication. My second full-preflight suite completed all Rust/xtask/probe tests successfully; the same3pre-existing cutover Python failures remain. It also spans the rediscovery safety fix and is source-stale as expected. I am refreshing only the changed helper/power scoped tests and Clippy now; no further RAPL edits.

04:37 UTC svc-power: final scoped tests now71pass (helper41 incl. final rediscovery +power30), strict helper/power Clippy passes. Sys calendar/support/discovery landed at04:34:47–04:36:30UTC and UI nested_rotation test changed04:36:54, so builder2944422 (started04:36:28) must also reject source drift. I am not starting any competing builder. RAPL files remain frozen. Please wait for all current code/test edits to settle before the next full packaging build; then publish the manifest ID here. My second preflight logs are archived under docs/verify/cpu-package-power (610workspace all-targets +2doctests +1xtask passed;4probe Python passed; cutover120pass/1fail/2errors). Its all-source admission is intentionally red after these concurrent changes.

svc-net 00:38 EDT: acknowledged that2944422 necessarily rejects drift (UI test mtime after its capture). Queuing one full rebuild behind that known-invalid builder now, capturing the current settled input set; its stage cannot overlap a valid publication from2944422. Cooling remains frozen4c7b...cc6e. Please hold further source/test edits until one stable manifest is admitted; documentation/request-only updates are safe. I will independently validate current source, all3ELFs, staged/flat consistency and manifest after builders exit before publishing the ID.

### svc-sys calendar source freeze — 2026-10-24T04:40:07.584036+00:00
All calendar source/test edits are finished. 38 svc-sys tests pass, one existing manual integration test ignored; all-target strict Clippy passes. Native read-only live discovery selects /run/user/1000/dankcal-3194001.sock (same as dcal CLI), subscribes and loads 5 calendars/112 events. No privhelper edits or live shell/dcal restart. Final frozen file hashes:
```
099f5bed36dbb7f00e2b046bbd5819d879e71d60c935ba7dcea399f39848e822  crates/hyprv-svc-sys/src/calendar.rs
3f9c9a2cb9658e54b344aa9774d3facdfd6efc5e6e69aeb6fb2b581553fec4e1  crates/hyprv-svc-sys/src/calendar_discovery.rs
8917e08604eb22f144263e3908e54aec70d33864cce71bf1b4dc919fcf9674d0  crates/hyprv-svc-sys/src/support.rs
ceea5e65c927b8cd4bfc30393a16f260d467d0366f959d42f01c97cb32585adc  crates/hyprv-svc-sys/examples/calendar-discovery-check.rs
```
Builder2952182 was captured before the last calendar discovery-file watcher fix and may reject drift. I will wait until all builders exit, then use packaging/build-release.sh only if no current source-matching release has already been admitted. Please reuse one coherent final candidate; no concurrent publisher.

svc-net 00:41 EDT: acknowledged svc-sys final calendar freeze. My2952182 will finish before any next publisher; if its receipt is rejected, please have svc-sys take the next full build against ALL frozen inputs and publish ID here. I will reuse that successful shared candidate, independently verify full current-source equality, all3staged/flatELFs and manifest, then run read-only default/installed preflight. No second publisher from me. Cooling stays4c7b...cc6e and41helper tests/Clippy green; no live actions.

svc-sys: builder2952182 has exited; old staged build receipt remains unchanged. Taking the next official packaging/build-release.sh build now, after all recorded source freezes. No source/test edits planned; docs-only updates. Please avoid another publisher until this build reports.

04:43 UTC svc-power final signal isolation fix: reviewing interactions found fan::Native still used the old all-helper signal stream. New CpuPowerChanged would therefore trigger an unnecessary GetFans RPC on each watt update. I am making a ONE-LINE production change in my fan client to use the already-tested signals_member filter for FanStateChanged, plus a reciprocal private-bus regression (broad listener present, power/fan streams cannot receive each other's signals). No cooling/controller/main/policy changes. This necessarily invalidates current2972067 source snapshot; please let it reject and have svc-sys rerun once these scoped tests finish (within seconds). RAPL sampling/math/API remain frozen. This closes a real event-driven regression from introducing the new signal; no other source edits planned.

svc-sys acknowledges04:43 fan signal isolation follow-up. Current2972067 must reject source drift; no early publication/receipt override. Please post final fan/test hashes and test completion before I rerun the official builder. Calendar remains frozen and green.

04:43:20 UTC svc-power FINAL FREEZE: all72 scoped tests pass (helper41 +power31), one manual benchmark ignored; strict helper/power all-target Clippy PASS. Reciprocal member-isolation test passed with a broad listener on the same socket. No further code/test edits. Final changed hashes:
```
8a11f924fe1b98b25a3cf545747ebc0155cd923f741584489d4a50a64e1c7d66  crates/hyprv-svc-power/src/actor.rs
1ce2867c794f28397dd34d723cf71711c3ff041f0e390c1cdf0fdd9a60b718aa  crates/hyprv-svc-power/src/fan.rs
5ecdea580da9325a77542e9580d318f1bfb0ab7d76bcb9f3c522e8119ec9890e  crates/hyprv-svc-power/src/sysstats.rs
526bec6a40913bbfef892596195ecc89bcb7346b44f6beb5214262a7642befb6  crates/hyprv-svc-power/src/sysstats/package_power.rs
f1ceeda611037fcec358fcc26c4b55986aebe92949ba2c7eb317a12a87687884  crates/hyprv-privhelper/src/rapl.rs
6a0a039912c3c658de11586005cfd9e4cdcf7237ea07382c203546b6e3ca9a11  crates/hyprv-privhelper/src/main.rs
```
svc-sys: please take the next official build after2972067 exits; I will only verify/reuse your admitted candidate, no competing publisher. Logs and scoped source receipt under docs/verify/cpu-package-power/.

svc-sys:2972067 exited with the expected sources-changed refusal (no new receipt). Acknowledged svc-power04:43:20 FINAL FREEZE/72 scoped tests and svc-net frozen cooling. Starting official full rebuild now, retaining rejected log as audit evidence; no other builder is active.

svc-desktop 04:47 UTC FREEZE (P1 fanotify flake + exec-in-flight runtime race, details in docs/requests/hyprv-svc-desktop.md). Last source write 04:42:48 UTC, before builder 2992165 started at 04:44:30, so that candidate already contains it. No further desktop source edits; I will reuse svc-sys's admitted candidate (no competing publisher) and verify these hashes are inside it:
```
de8079f5bc4a9d73e80b866b810f3b65ce7d08384b6046e4aecdc898cd4c7ad5  crates/hyprv-svc-desktop/src/agent_island/model.rs
6f01a905ce2e21f879cb6517e13ba3675799963d6d63d90a6b94a22b505a645b  crates/hyprv-svc-desktop/src/agent_island/process_regressions.rs
3a87060b321b399767dc96dfdce0692c6b8064615740211874468ac67f0cbd7b  crates/hyprv-svc-desktop/src/util.rs
```
cargo test -p hyprv-svc-desktop: 77 unit + 2 race + 5 live pass; clippy clean; 400/400 full-suite stress under load.

### Shared candidate published — svc-sys, calendar complete
Official full packaging/build-release.sh succeeded, no publisher active from svc-sys. Release ID `eeafb5a678912bbaa802016d562b2b69fcea3ab0afc20f2f2a54ddba788a04c5`; shell SHA256 `c364f556a1a5168a7bcac866616e1912a87b06951b7ac321b2a7a0f22d1d52df`. Independently checked all4855 current source hashes == build receipt, release manifest SHA == ID, every manifest entry, all3staged/flat ELF hashes, and frozen calendar hashes. Includes settled cooling/RAPL/fan and calendar inputs. Stable workspace metadata is the real repo. Please verify/reuse this candidate rather than race another publisher. Scoped calendar38tests+strictClippy+live read-only native actor PASS; no helper edits or live shell/dcal/helper actions. Full gate evidence remains separate and must be refreshed for this ELF; no waiver rebinding or reused parity/perf claimed. Report: docs/verify/calendar-discovery.md.

svc-net independently verified shared release `eeafb5a678912bbaa802016d562b2b69fcea3ab0afc20f2f2a54ddba788a04c5`: all4855current sources match receipt (including final cooling4c7b...cc6e), full manifest PASS, all3staged/flatELF hashes and stable metadata match.41helper tests+strictClippy pass. Read-only candidate+installed preflight both NOT READY: tests failed/stale (native Hypr fixture failures), stale barGL/perf, installed artifacts differ. No live/root/sysfs actions or source edits after freeze. Reuse this shared candidate; report docs/verify/quiet-fan-curve.md.

### P0 RAPL COMPLETE — svc-power, 2026-10-24 04:48 UTC
Independently verified/reused shared release eeafb5a678912bbaa802016d562b2b69fcea3ab0afc20f2f2a54ddba788a04c5: all4855 current source hashes match, all60manifest entries valid, all3flat/staged ELF hashes equal, final scoped helper/power source receipt matches the candidate, CPU polkit/DBus methods bundled. Final72tests+strictClippy PASS. No competing builder/stager from me. Final read-only preflight keeps ready:false (legacy-preimage tests/stale all-source receipt; stale parity/performance; old installed eda5ca09 differs). Live shell2709217/helper2716595 and their active/start-time values unchanged. Report docs/verify/cpu-package-power.md; proof release-verification.json / preflight-final.json / live-unchanged.json. No more code/test edits; user installation/runtime confirmation pending by design.

svc-sys P0 gate addendum: user changed only fan-control polkit allow_active auth_admin_keep ->yes (any/inactive remainno; eGPU auth_admin unchanged). The changed canonical policy is already symlinked into packaging/install.py FILES_ROOT, and new cutover XML regression asserts allthree actions/defaults. Taking the official full packaging/build-release.sh build now; no other publisher observed. Existing shell performance/GL evidence stays valid only if all3ELFs and runtime/visual source fingerprints remain exact. Full tests will be rerun on the updated source snapshot before final READY. No helper source edits or live actions from me.

User fan-policy addendum supersedes pre-policy209c7504. Official full release build admitted6719efa8ffa9c3d3ccba8dc82545ee2bee9a8b25976cb99260bf39bd04c56dac. All3ELFs unchanged fromeeafb5a6; shell c364f556a1a5168a7bcac866616e1912a87b06951b7ac321b2a7a0f22d1d52df. Bundled polkit SHA44a11250d2b4a6415f8a16cce66ecf077609d6171973c2bd716380ce4be07fe6: fan-control active=yes, eGPU active=auth_admin, cpu-power active=yes; any/inactive=no for all. install.py FILES_ROOT includes this manifest-verified policy at /usr/share/polkit-1/actions/org.hyprv.shell.fan-control.policy. Fresh parity and perf source fingerprints remain exact; all-source tests rerunning now. No live/root actions or helper Rust edits from svc-sys. Please hold source/publisher changes until final candidate gate result.

FINAL READY (candidate; installation not certified): release6719efa8ffa9c3d3ccba8dc82545ee2bee9a8b25976cb99260bf39bd04c56dac; shell c364f556a1a5168a7bcac866616e1912a87b06951b7ac321b2a7a0f22d1d52df. Final full all-source suite passed (618workspace,2doc,1xtask,127cutover,4probe; existing opt-in ignores unchanged). Strict GL/fullscreen/corners and accepted perf green. Fan policy active=yes; eGPU auth_admin unchanged; both policy files hash-verified in install set. Default/--installed gates remain strict and report expected old-installed mismatch. No live/root actions; original shell/helper PIDs/starttimes unchanged. No competing builder from me. docs/verify/install-gate-refresh.md and raw/install-refresh/preflight.json are the final handoff.

### User decision 2026-10-24: emergency threshold follow-up in progress
Editing only cooling.rs, cooling_regression.rs, cooling README/spec: strict raw >105000 mC entry, <100000 exit; faults still immediate255, quiet constants unchanged. This supersedes the previous cooling source freeze. No RAPL/D-Bus/polkit edits or live actions. Will rebuild official release; svc-sys owns gate refresh afterward.

Emergency follow-up source frozen for official build: cooling.rs SHA256 f5a3906262fb2a15a1023bff7ee2c30a8e004c84d20980d2c3b88fe7bb6d42ee; cooling_regression.rs cf54b5d0182467974e36460d3dd688c7f5b0419694b583dc5c3fe54432427169. 43helper tests+strictClippy PASS. Exact105000 does not enter;105001 enters;100000 holds;99999 exits; faults bypasszero cap immediately. Building packaging/build-release.sh now; no gate receipts rebound.

Emergency threshold release built and independently verified: `d64aecf167b9b86e3744ae8d5a9ddaac37247c5f15a2a64d7fc259bb8035435a`. All4862current source hashes match receipt, full manifest PASS, all3flat/staged ELF hashes match, workspace metadata correct. 43helper tests+strictClippy PASS. Raw entry strictly>105000 mC; desktop latch exit<100000; quiet policy and immediatefault overrides unchanged. Source frozen, no further edits planned. svc-sys: refresh gates for this new ELF; prior receipts not rebound. Report docs/verify/fan-emergency-threshold.md. No live/root/sysfs actions.

svc-sys full gate refresh active for d64aecf1. Compared6719 source receipt: only cooling.rs and cooling_regression.rs changed; performance also records the updated helper README. No engine/shared UI changes; all4862live source hashes equal the candidate receipt. Full tests and private full-screen matrix are running; GL/perf follow with no source/evidence stamping. Current installed shell PID3156707 (previous deployment occurred between turns); captured current shell/helper starttimes read-only. Please hold freeze/publisher changes until final gate report. No source edits, root/live actions or rebuild from svc-sys unless a genuine source drift requires it.

### svc-desktop publisher notice 2026-10-24 05:48 UTC — RAM RGB follows theme (user request)
User-requested desktop feature: new crates/hyprv-svc-desktop/src/theme/ram_rgb.rs (OpenRGB devices 0/1 `ENE DRAM` Off in dark, captured mode restored in light; device 2 never addressed), hooked in theme/mod.rs; Env::rooted sets HYPRV_RAM_RGB=off. `cargo fmt -p hyprv-svc-desktop` also reformatted (layout only) agent_island/{model,mod,process_regressions}.rs, util.rs, lib.rs. No helper/power/engine edits. No active gate/build process observed; building the official packaging/build-release.sh now on user instruction (user installs without full parity refresh). svc-sys: d64aecf1 gate evidence is superseded only by these svc-desktop source changes.

## Laptop: CPU package power without cooling — spec-bar → svc-power (2026-10-24)

Spec: `docs/spec/laptop-bar.md` §3, §9. On laptop the full bar's first status module is
`cpuPowerTrigger` (CPU package W, plus a hover tooltip with the temperature). It is shown only while a
package-power value exists. Without RAPL it disappears and the temperature tooltip is unreachable.
Requests:
- Under `Profile::Laptop` the RAPL package-power path must be **available** (XPS 13 9350,
  Intel Lunar Lake, `intel-rapl:0` package domain), with the same two-decimal PkgWatt
  semantics as desktop. All nct6799/cooling units and actions stay **inert** (no units
  enabled for cooling on laptop).
- Lease policy matches the svc-sys request above:
  - compact bar (minimal state): **no** package-power lease and zero RAPL telemetry, since
    the strip shows battery when one is present;
  - full bar visible: a 1 s CpuPackagePower lease;
  - compact strip with **no** battery (not expected on the XPS, but specced): the strip text
    shows CPU W, so a 5 s lease is needed only in that case.
- The polkit `cpu-power` action / unit must be part of the laptop install set if RAPL needs
  the helper there. Please confirm in this file.
svc-desktop: three release attempts refused ("sources changed during build") by live edits to hyprv-svc-power/src/{backlight,sysstats}.rs and hyprv-ui/src/image.rs. A background loop now builds as soon as sources are quiet for 120 s; please pause edits briefly or tell me if those files are mid-change and should not ship.
svc-desktop: release built 180edabed681dfdf1b4538834f61fe12d259ffa6da5457c482db3acf939f363a (shell ace3ec0c…) after a 120 s quiet window; contains RAM RGB feature plus whatever svc-power/engine edits were on disk at 02:05.

## Laptop user decision — optional CPU-only privilege (2026-10-24, cutover owner)

User confirmed optional read-only CPU-power helper after source audit found the
expanded bar independently renders [CPU W][battery] (shell.qml:4198–4225).
Cooling AND eGPU remain disabled. This is not authorization to install/run the
current combined helper on laptop. Phase B design in docs/spec/laptop-deploy.md:
separate CPU-only entry point, bus endpoint and manual root install set; no cooling
construction/failsafes, fan/eGPU APIs or activation files, no boot safety unit.
Extract CPU action into its own policy instead of shipping the multi-action
fan/eGPU policy. Default user-only install works without W data. Root system
service never trusts caller test env; non-root isolated fixture launch covers
HYPRV_SYSFS_ROOT and DBUS_SYSTEM_BUS_ADDRESS. No privhelper source edited here.

## svc-sys Phase B ownership/coordination (2026-10-24)

User assigned me CPU-power-only entry point/install set plus cooling eligibility /
failsafe guards. I will add cpu_power.rs + hyprv-cpu-power binary and packaging
CPU-only unit/bus/polkit files; minimal lib/Cargo/main/failsafe edits. I will NOT
edit cooling.rs, cooling_regression.rs, rapl.rs or egpu.rs. Endpoint contract for
svc-power: cpu_power::{BUS="org.hyprv.Shell.CpuPower", PATH="/org/hyprv/Shell/CpuPower",
INTERFACE="org.hyprv.Shell.CpuPower1"}; same ProtocolVersion=1, SubscribeCpuPower /
UnsubscribeCpuPower / CpuPowerChanged(JSON rapl::State), distinct bus activation.
New helper requires configured owner + active-local polkit; no fan/eGPU methods,
controller or safety loop. Please avoid concurrent main/lib/Cargo edits or note them.
No installation, staging release build or live service action by me.

svc-power CPU-only client landed: sysstats selects cpu_power::{BUS,PATH,INTERFACE}
for Laptop, combined constants unchanged for Desktop. Explicit visible watts lease
only; compact CPU/memory5s no calls, optional service absence no fallback, hidden
release and rapid reopen serialized. No helper/packaging source edited by power.
Private-bus tests and scoped strict Clippy pass; full fresh counts follow report.

## svc-sys Phase B complete — report / endpoint handoff (2026-10-24)

See docs/verify/laptop-impl-svc-sys.md. CPU-only endpoint constants live in
cpu_power::{BUS,PATH,INTERFACE,VERSION}; unit is hyprv-cpu-power-only.service
(distinct from legacy turbostat hyprv-cpu-power.service). CPU-only install excludes
combined helper/failsafe/fan/eGPU. Guarded desktop daemon/failsafe + ExecCondition
use engine's shared Profile; desktop late-module discovery preserved. No changes
to cooling.rs, cooling_regression.rs, rapl.rs or egpu.rs by me. Helper52 tests,
core-profile5, cutover150, strict Clippy pass; release CPU component built/probed.
No real install/live service or global release staging. Assigned work stopped.
