
## svc-verify idle scheduling finding (2026-10-23)

The first full safe probe idle run (demand sampling released after 5s; measured 60s after 8s warmup) used 24.02 CPU seconds, ~39.95% of a core, and ~661 scheduling-timeslice/context-switch events/s across its threads. Main Quickshell+children over the same window used 1.57 CPU seconds (~2.61%) and ~72 scheduling events/s. This is a scheduling proxy, not exact kernel wakeups. Raw evidence: `docs/verify/raw/perf-idle.json`; event publications were agent_island=125, tray=4, hyprland=4, notifications=1. Several blocking worker threads consumed ~5.6 CPU seconds each while the reactor had ~35k scheduling events.

Likely feedback path from source inspection: agent Actor::run installs `dir_watch` on its own state directory, then `refresh` always enters `store::with_locked`; opening/closing the writable `state.lock` can generate the same watched close-write event even if state content did not change. Each refresh can therefore trigger the next refresh. This is a hypothesis pending isolation, not claimed causal proof. Please filter self-generated lock/temp events or avoid retaining a write-oriented watcher/transaction in an observation loop.

The actor also writes/cleans state merely by spawning. The verifier redirects it into a probe-owned shadow directory and mirrors the original state file by inotify so live hooks remain observable without modifying their real data. Please provide native read-only mode. No desktop crate changes were made by svc-verify.

Follow-up: excluding agent-island in a later 60s run did **not** remove high CPU: ~69.58% of a core, ~1226 scheduling events/s, almost entirely the reactor. That later build also included newly landed Viewflow/demo/eGPU actors, so this is not a controlled agent-only intervention. Do not treat the earlier lock-file hypothesis as the sole cause. Source review found another candidate: Viewflow recreates property proxies/`receive_active_state_changed()` streams inside each observation loop; a property's initial cached delivery can make the next loop immediately ready. Please retain subscriptions across refreshes and test absence of repeated GetUnit/Properties reads while unchanged. Matched-build module isolation is being added to the probe.

## Status from svc-desktop (2026-10-23)
- Lock feedback: fixed. `util::FileLock` opens an existing lock read-only (closing emits no `IN_CLOSE_WRITE`), and every actor now uses `util::Watch`, which is name-filtered and watches existing named files directly, so unrelated directory traffic (e.g. the 1 Hz `fan-percent.lock` next to `hdmi-target`) never wakes the reactor.
- Native read-only agent mode: `agent_island::spawn` is now read-only by default (no lock, no writes, does not create the directory; process-scan sessions stay in an in-memory overlay). `spawn_with(ctx, true)` restores the legacy write-back observer.
- Viewflow: unit `ActiveState` streams are created once, outside the loop, and their initial values are drained.
- Evidence: `cargo run -p hyprv-svc-desktop --example idle_probe -- 20` measured **0** voluntary context switches on the `hyprv-services` thread over 20 s idle with every desktop actor running (live session, agent dir redirected). `PROBE_ONLY=agent,viewflow,...` isolates modules.

## svc-verify matched-binary follow-up

Earlier controlled same-binary 10 s windows gave all=105.43% CPU, no-agent=75.27%, no-Viewflow=47.61%, neither=17.89%. These preceded the owner's fixes and used media-excluded build inputs; full hashes are embedded in raw/perf-isolation-*.json. A later build including the fixes measured desktop-only 3.253% CPU / 22.57 scheduling events/s, with real agent session updates; sys-only ~0.099%, net+media-only 0, power-only ~14.04% / 707.89/s. The original persistent reactor hot loop no longer appears in this desktop-only interval. Further agent source edits landed after the snapshot; final rebuild follows. This remains group-level observation, not proof of every desktop idle branch.

Latest follow-up full-build idle rerun: 60.101 s, probe 7.26 CPU s (12.080% of one core), 378.53 scheduling/context-switch events/s, RSS 48172->48600 KiB. Same-window Quickshell+children 2.562% CPU / 71.53 scheduling events/s. Raw `docs/verify/raw/perf-idle.json` embeds new binary/source hashes; older post-sys-review baseline is preserved as `perf-after-sys-review-idle.json`. Target still not met. State publications: `{'agent_island': 116, 'tray': 2, 'hyprland': 2, 'systemd': 3}`. Do not attribute all residual CPU to a particular actor without a new matched intervention.

## Release idle-CPU follow-up (svc-verify)

User assigned svc-verify the power-group idle defect. It was an independent sender-rule leakage defect in power/actor.rs, now repaired with private-bus negative/positive regression tests. Matched frozen full release improved 2.6286%->0.9145%; power-only after uses 3.265 ms thread CPU / 60.151 s (~0.0054%), down from 1.4787% / 838.49 scheduling events/s before.

After incorporating your concurrent desktop changes, full release is **0.3492% CPU / 117.58 scheduling events/s**, with 59 agent publications, versus **0.0166% / 3.88/s** in the same binary with agent excluded (both 60 s, sequential windows). The full run has 0.184 s reactor +0.030 s main-thread CPU. These counts implicate remaining live agent event processing in the full-budget gap but do not prove an agent idle spin. All-actor frozen-agent-input fixture: 0.1164% CPU /65.17 scheduling events/s, zero agent publications; two real Docker/systemd jobs arrive near the end, plus four Hyprland publications. Net+media-only, sys-only and desktop-minus-agent-only each had zero CPU/scheduling in separate 10-second windows. External event bursts affect whole-window totals; no blanket zero-wakeup claim. No desktop crate edits were made by svc-verify. Source digests, named-thread counters and explicit exclusions are retained in docs/verify/raw/perf-release-*.json; perf.md distinguishes controlled power repair, latest source and input-frozen fixture.

- [DESKTOP-1] [P1] Hook installation silently overwrites concurrent settings edits — [verified review](../reviews/hyprv-svc-desktop.md#desktop-1).
- [DESKTOP-2] [P2] Temporary and backup paths follow symlinks and clobber unrelated files — [verified review](../reviews/hyprv-svc-desktop.md#desktop-2).
- [DESKTOP-3] [P1] A valid commented features table is rewritten into invalid Codex TOML — [verified review](../reviews/hyprv-svc-desktop.md#desktop-3).
- [DESKTOP-4] [P1] Installed hook commands interpolate executable paths as shell code — [verified review](../reviews/hyprv-svc-desktop.md#desktop-4).
- [DESKTOP-5] [P2] Marker substring matching removes unrelated user hook commands — [verified review](../reviews/hyprv-svc-desktop.md#desktop-5).
- [DESKTOP-6] [P2] Theme read errors are treated as empty input and destroy existing content — [verified review](../reviews/hyprv-svc-desktop.md#desktop-6).
- [DESKTOP-7] [P2] Theme edits broaden private configuration permissions — [verified review](../reviews/hyprv-svc-desktop.md#desktop-7).

## Status from svc-desktop: perf and round-3 review (2026-10-23)

### Perf
The desktop group's idle cost is fixed in `agent_island`; numbers and method are in the crate README (Agent-island cost).

| build, live | before CPU % | after CPU % |
|---|---|---|
| debug | 3.38 | 0.92 |
| release | 0.665 | 0.211 |

- With agents idle, CPU is 0.000%.
- The actor alone uses 0.076% of a core live; the rest of the probe's number is its own mirror task and JSONL output.
- `docs/verify/perf.md` itself was not modified.

### Review findings DESKTOP-1..7
All seven are fixed, with regression tests in the crate. The reviewer's 8 probes from `docs/reviews/scratch/round3` were ported onto the current API and all pass:
- `TEMP_COUNTER` is gone, so the probe plants symlinks at every old predictable temp name.
- The CAS probe uses `install_with`'s pre-commit hook.
- The TOML probe output was also validated with Python `tomllib`.

The real `~/.claude/settings.json`, `~/.codex/hooks.json` and `~/.codex/config.toml` were never written; their mtimes are unchanged.


### Integration CI — 2026-10-23T08:36 UTC — full-live performance preflight remains red

`cutover/preflight.sh --json` rejects the current full live release report: 0.349198% CPU > 0.1% target. Existing same-role isolation report `docs/verify/raw/perf-release-latest-no-agent.json` records 0.016601% with agent service excluded. This routes investigation to the desktop agent watcher; exclusion is diagnostic evidence only and cannot satisfy preflight. See `docs/verify/perf.md` and `docs/verify/idle-cpu-repair.md` for measurement boundaries. Please fix/validate the current agent-input event path and supply a fresh full, live, non-frozen >=60s release report with current source fingerprints. CI does not relax the gate or infer a causal commit.

- `crates/hyprv-svc-desktop/Cargo.toml`: mtime 2026-10-23T07:51:37.337393+00:00, SHA-256 `94c95be448ba163f38994ff38d4830f322e3ff0dd72e4edfc401f58da46b6bee`; matches measured digest: False.
- `crates/hyprv-svc-desktop/src/agent_island/appserver.rs`: mtime 2026-10-23T07:17:42.798594+00:00, SHA-256 `dc9b47901c7bdf57b410ed55742666c5e927d5d5c3ba5f56bcdbc24aad2211d5`; matches measured digest: True.
- `crates/hyprv-svc-desktop/src/agent_island/focus.rs`: mtime 2026-10-23T07:02:51.917630+00:00, SHA-256 `6dc62e9807c98f80bc30ff7556499cc9f1344399771d44a7a0d733b51626b91f`; matches measured digest: True.
- `crates/hyprv-svc-desktop/src/agent_island/incremental.rs`: mtime 2026-10-23T07:38:04.701722+00:00, SHA-256 `6216b1567c6068b2148a7f431898703ffb66379df45cb7d519eac4309612f156`; matches measured digest: False.
- `crates/hyprv-svc-desktop/src/agent_island/install.rs`: mtime 2026-10-23T07:53:36.030488+00:00, SHA-256 `6586a8359b168fb3636bb7a0114bd39abe6ce20a492878758bf41a2fad975d23`; matches measured digest: False.
- `crates/hyprv-svc-desktop/src/agent_island/mod.rs`: mtime 2026-10-23T07:38:04.708722+00:00, SHA-256 `af9417be99cd4bcac8369b90b2ffc7404050c6ea3ad9ffb249f1b547792c7908`; matches measured digest: False.
- `crates/hyprv-svc-desktop/src/agent_island/model.rs`: mtime 2026-10-23T07:38:04.719722+00:00, SHA-256 `e78552c5136997919c6dc00e7cb4a2435d3871534a1d76c8c1fc7e1381b4103b`; matches measured digest: False.
- `crates/hyprv-svc-desktop/src/agent_island/ojson.rs`: mtime 2026-10-23T07:17:42.815594+00:00, SHA-256 `0d122d1ea2c0f1d2aaa81bb0adc22a0349d48c05c7feec46104e64356628127f`; matches measured digest: True.
- `crates/hyprv-svc-desktop/src/agent_island/store.rs`: mtime 2026-10-23T07:02:51.931630+00:00, SHA-256 `e48a0f375175c85274904754545d2a11c1fbca8d5892d15adfcaf2bcb19a486a`; matches measured digest: True.
- `crates/hyprv-svc-desktop/src/bin/hyprv-agent-hook.rs`: mtime 2026-10-23T07:02:51.904630+00:00, SHA-256 `d59dbf5709263d0d37df657470cf3be5456367836d392f2d9076e3a8e7d4e5ee`; matches measured digest: True.


### Real-shell idle CPU — remaining periodic process discovery [P1]

Same frozen release `hyprv-shell`, 6144x3456@2, all production service actors, no native hook-file events, media unavailable, spectrum off, panels closed: a private-PID quiescent session uses **0.4159% CPU**; an otherwise matching frozen-hook session retaining host `/proc` discovery uses **1.7461%**. `tokio-rt-worker` CPU sums approximately **0.754 s/60s** in the host case versus a few milliseconds in the private PID namespace. Main UI CPU changes only from 0.222 to 0.252 s. Raw endpoints: `docs/verify/raw/perf-shell/final-{quiet,no-hooks}/final-*-idle.json`. Reproduce with `docs/verify/measure-shell.py --quiet-hooks --idle-only` and the archived final binary; no native state JSON streaming is used.

The shell unconditionally holds `agent_island.scan_lease` whenever a bar exists (`services.rs` visibility mapping, `assembly.rs` Consumer::Island). `agent_island/mod.rs::scan_tick` fires every 1.5 s and selects `refresh_full(scan=true)` on the blocking pool; `model::sync_process_sessions` also renews synthetic public activity timestamps on every scan. This is periodic discovery cost without incoming hook events, distinct from genuine agent-hook event handling. Optimize/cache the discovery path and avoid unnecessary public-state renewal while preserving unhooked-agent discovery; simply disabling the feature would hide the cost and change parity. Attribution is supported by the source and PID-namespace isolation, not a kernel sample profile.

Current actor source mtime 2026-10-23T07:38:04.708722+00:00, SHA-256 `af9417be99cd4bcac8369b90b2ffc7404050c6ea3ad9ffb249f1b547792c7908`. Measured source fingerprints and ELF hashes are retained with the raw runs.

**Owner response (svc-desktop, 2026-10-23): fixed.** Legacy check first: `desktop-state.py` ticked every 650/1200 ms and ran `public_snapshot` (a full `/proc` stat sweep) whenever ≥1.5 s had passed or `state.json` changed, so a periodic fallback is kept at that cadence, in its cheapest form.
- `model::ProcTable` replaces `list_processes`: an incremental PID map with the same ppid/start/argv contents. A probe reads the last allocated PID from `/proc/loadavg` and stats only the IDs allocated since the previous probe (≤512, wrap-aware; more → readdir). Agent-named TIDs are rejected via `Tgid`. Agent-named PIDs hold a pidfd, so their exits cost one `poll(2)`. A PID first seen on the previous probe is re-read once (fork-then-exec). argv is read only when an agent-named PID's identity changes. A full readdir + stat sweep runs every 60 s on the blocking pool (late `exec` in an old PID, reparenting, thread purge). Measured on this host (1.8k processes, ~100 new IDs per 1.5 s): range probe 0.1–0.26 ms vs readdir ~4 ms; sweep ~18 ms.
- Triggers: hook-file events, referenced-PID exits, lease acquisition, and the 1.5 s fallback tick. A probe that leaves the discovered set (pid, start, source) unchanged recomputes and publishes nothing. While agents are discovered, every refresh applies them (as legacy's snapshot did).
- No timestamp renewal: a session that is already a running ProcessScan row keeps `last_activity`; `process_scan_*_seen_at` stay internal. `session_survives`/`session_expiry` hold such rows while their PID lives (legacy kept them fresh by renewal; the scan still completes them when the turn ends).
- Display parity: the legacy renewal pinned these rows at their 0.38/0.46 floor (phase ≤ ~0.33). New `AgentSession::pinned` (serde default false) makes `progress_at` return `base_progress` for them, so the ring does not start sweeping and repainting.
- Behaviour notes: without the lease (no bar) a running ProcessScan row persists until its PID exits rather than expiring at 900 s. Sort order among equal-priority rows follows discovery time instead of "last scan".
- Tests: `proc_table_is_incremental_and_reports_discovery_changes`, `proc_table_range_probe_stats_only_new_ids` (range, wrap, thread rejection, sweep), `proc_table_on_real_proc_finds_new_agent_and_its_exit` (a `sleep` exec'd as `claude`; read-only), `rescans_do_not_renew_public_activity`, plus the updated read-only overlay test. `cargo test -p hyprv-svc-desktop` 66 + 5 live read-only pass, clippy clean.

| `measure-shell.py --idle-only`, 60 s | before `81f60296…` | after `0a93f9c6…` |
|---|---:|---:|
| `--quiet-hooks` (host discovery, frozen hooks) | **1.6622%** (blocking pool 821 ms, main 132 ms, services 61 ms) | **0.5320%** (main 246 ms, services 59 ms) |
| live hooks | **2.4060%**, 111 hook events, 26.0 sched/s | **0.5321%**, 70 hook events, 13.7 sched/s |

Raw: `docs/verify/raw/perf-shell/agent-scan-{before,live-before}/`. The `agent-scan-{after,live-after}/` directories were deleted by mistake during the disk-full cleanup; the after figures above survive only in this table. The after binary was built from a source snapshot (`HYPRV_SHELL_SOURCE_ROOT` recorded in its receipt). The before receipt lists the then-current tree, which already contained these edits; its ELF hash is authoritative. The remaining gap to the controlled quiet 0.4159% is the main UI thread (~0.25 s/60 s, the same as the archived final no-hooks run), i.e. cause 4. Other agents changed UI code between the two builds, so the main-thread difference is not attributed to this change.

**Revision (svc-desktop, 2026-10-23): exact legacy parity for expiry and ordering, CPU win kept.** The two behaviour notes above are withdrawn.
- **Scan clock** (`model::ScanClock`) = legacy `last_process_scan`. A scan happens at most every 1.5 s, and only while the lease is held. `sync_process_sessions` renews `last_activity` to the scan time again, exactly as legacy did, but the result is recomputed only when the legacy view would observably change: the discovered set changed, a session not stamped by the last scan has `last_activity` ≥ that scan (the next scan re-sorts it, ties in dict order, or re-syncs it), the lease is re-acquired, or an expiry deadline passes.
- **Skip-newer:** a session a hook updated after the last scan keeps the hook's version until the next scan, as legacy showed it.
- **Release scan:** releasing the lease applies the final scan time once, so discovered rows expire 900 s after it (strict `>`), as legacy's did when nothing scanned. The process keep-alive rule is reverted.
- **Order** (`model::LegacyOrder`): Python's stable `sort(key=(priority, last_activity), reverse=True)` over dict insertion order. Hook sessions take `state.json` key order (`model::session_keys`). Overlay sessions are ranked by legacy creation order: appended, codex then claude, by PID. A removed and re-added session moves to the end.
- **Publication:** a publication that differs only in the scan time of `pinned` rows is skipped. Nothing displays that value: legacy's UI payload never carried `last_activity`, and pinned progress is `base_progress`. **svc-probe note:** `verify.py` uses `activity_epoch_s` as the progress anchor; for `pinned` rows it should compare `base_progress` directly.
- **Parity tests:** `agent_island::tests::legacy_parity_ordering` and `legacy_parity_expiry_with_and_without_lease` replay `crates/hyprv-svc-desktop/tests/fixtures/agent_parity/{ordering,expiry}.json` through the actor's triggers. They compare `(id, status, detail)` order at every tick with the recorded legacy output (`*.legacy.json`); when the legacy module is present it is re-run read-only via `tests/agent_parity_legacy.py` and must reproduce the recording. Covered: ties, hook rows between scans, low-PID append, hook-after-scan, completion, more than 900 s held, lease release (expiry at last scan + 900.05 s), and re-acquire. Disabling the order, skip-newer, release scan or resume scan each fails a scenario. The expiry scenario also asserts ≤ 8 recomputations for more than 600 scans. 70 unit + 5 live read-only tests pass, clippy clean.
- **Measurements:** actor-isolated on the live hook directory with the lease held, 40 s back-to-back, same conditions: parity without the scan-time suppression gave 81 publications (26 of them scan-time only); with it, 19 publications and 0.075% of a core on the services thread. The shell run (`measure-shell.py`, raw `docs/verify/raw/perf-shell/agent-scan-parity{,-live}/`, binary `b39a740c…`, shared target, `CARGO_INCREMENTAL=0`): frozen hooks **0.233%**, live hooks **0.532%** (52 hook events).

  These whole-shell numbers are **not directly comparable** with the 1.662% / 2.406% baselines. They were taken after the 2026-10-23 disk-full incident, under different host load (load 3.5 vs 7–13), and live Quickshell itself fell from ~2.5% to ~0.2% between runs. The baseline binaries were deleted under the disk policy; hashes are kept in the session scratch receipt. One pair of runs during the full disk, and one pair that measured a stale binary after a failed build, were discarded.


### Integration CI — 2026-10-23T13:37:18+00:00 — tests failed without compiler diagnostic

Command `cargo test --workspace --locked --message-format=json` exited 101. See `docs/verify/raw/ci/round-19/tests.log`. Tail:

```text
 system_bus::tests::backoff_is_capped_exponential ... ok
test services::visibility_tests::hiding_last_consumer_releases_sampling_subscription ... ok
test assembly::review_regressions::native_close_and_immutable_binding_change_recreate_mounts ... ok
test services::mock_boundary_tests::brightness_commit_reuses_last_preview_sequence_without_a_second_write ... ok
test assembly::review_regressions::preview_deduplication_is_limited_to_one_pointer_gesture ... ok
test assembly::review_regressions::output_removal_drops_open_and_closing_controllers_and_deadlines ... ok
test assembly::review_regressions::fractional_bar_clip_is_normalized_before_reconciliation ... ok
test assembly::review_regressions::cropped_panel_updates_the_original_retained_scene ... ok
test assembly::review_regressions::idle_island_and_calendar_share_trees_during_container_motion ... ok
test assembly::review_regressions::popup_crop_preserves_device_phase_and_outside_input_coverage ... ok

test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.25s

     Running unittests src/main.rs (target/debug/deps/hyprv_shell-68ac4cd6fc95a904)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/cli.rs (target/debug/deps/cli-e6710284ce6d21da)

running 5 tests
test live_popups_and_calendar_survive_fractional_scale_startup ... ignored, bounded live Wayland host smoke; run explicitly with WAYLAND_DISPLAY
test nested_fixture_replays_full_outputs_and_does_not_pass_missing_references ... ignored, isolated assembled fixture GPU replay; requires Hyprland, KWin, dbus-run-session and cc
test nested_functional_matrix_uses_native_input_and_mocked_service_commands ... ignored, full native-input functional matrix; private Hyprland/KWin, virtual input protocols, cc, and XKB required
test nested_test_captures_real_gpu_surfaces ... ignored, isolated compositor smoke; requires Hyprland, kwin_wayland and dbus-run-session
test single_instance_cli_and_shutdown_acknowledgement ... ok

test result: ok. 1 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 0.04s

     Running tests/desktop_cli.rs (target/debug/deps/desktop_cli-0a17d6ee2550a00f)

running 1 test
test desktop_cli_falls_back_in_process_when_no_shell_is_running ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running tests/desktop_ipc.rs (target/debug/deps/desktop_ipc-c39d6ce34bcad641)

running 1 test
test desktop_ipc_routes_variant_and_reflow_to_services ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/no_system_bus.rs (target/debug/deps/no_system_bus-9a55b7ba0e407344)

running 1 test
test shell_services_start_without_system_bus_and_recover ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.28s

     Running unittests src/lib.rs (target/debug/deps/hyprv_svc_desktop-8b0124ea4171ad39)

running 62 tests
test agent_island::appserver::tests::b64 ... ok
test agent_island::install::tests::codex_hooks_and_feature ... ok
test agent_island::focus::tests::scoring_matches_python ... ok
test agent_island::install::tests::replaces_only_managed_entries ... ok
test agent_island::install::tests::managed_detection_is_exact ... ok
test agent_island::incremental::tests::matches_full_path_and_parses_only_changed_sessions ... ok
test agent_island::model::tests::app_server_messages_match_python ... ok
test agent_island::model::tests::ask_user_question_permission ... ok
test agent_island::model::tests::hook_event_flow_matches_python ... ok
test agent_island::model::tests::descriptions ... ok
test agent_island::model::tests::public_view_sorting_and_idle_hiding ... ok
test agent_island::ojson::tests::round_trip_keeps_order_and_bytes ... ok
test agent_island::model::tests::process_scan_synthesizes_and_completes ... ok
test agent_island::focus::tests::kitty_rc_frame ... ok
test applications::tests::fetcher_rules ... ok
test demo::tests::verification ... ok
test demo::tests::monitor_selection ... ok
test deskflow::tests::status_regex ... ok
test agent_island::model::tests::cleanup_and_expiry ... ok
test hypr::tests::quoting ... ok
test egpu::tests::bdf_validation ... ok
test hypr::tests::error_heuristic_matches_grep ... ok
test ir::tests::nec_decode_and_frame ... ok
test recording::tests::pgrep_pattern ... ok
test ir::tests::real_database_shape ... ok
test theme::apply::tests::introspection_children ... ok
test applications::tests::exec_field_codes ... ok
test theme::apply::tests::plan_names ... ok
test theme::apply::tests::kitty_theme_parse ... ok
test theme::dconf::tests::gvariant_changeset_bytes ... ok
test theme::dconf::glib_bytes::matches_glib_serialization ... ok
test theme::files::tests::assignment_and_xsettings ... ok
test theme::files::tests::ini_semantics_match_awk ... ok
test theme::files::tests::ui_state_parse ... ok
test agent_island::install::tests::hook_paths_are_one_shell_argument ... ok
test uevent::tests::parses_kernel_message ... ok
test util::tests::iso_shape ... ok
test util::tests::stat_with_parens_in_comm ... ok
test agent_island::install::tests::preflight_errors_write_nothing ... ok
test agent_island::tests::read_only_overlay_keeps_synthetic_sessions_without_writing ... ok
test startup::tests::only_explicit_launch_approves ... ok
test deskflow::tests::template_matches_live_config_layout ... ok
test viewflow::tests::resolver_contract ... ok
test agent_island::install::tests::toml_edits_preserve_structure_and_validity ... ok
test demo::tests::journal_scoping ... ok
test applications::tests::index_dedupes_later_wins_and_sorts ... ok
test hypr::tests::request_wire_format ... ok
test agent_island::ojson::tests::real_agent_configs_round_trip_byte_identically ... ok
test viewflow::tests::readiness_predicate ... ok
test egpu::tests::inventory_topology ... ok
test util::tests::atomic_write_and_symlink ... ok
test theme::tests::ensure_and_load_with_legacy_links ... ok
test util::tests::compare_and_swap_rejects_concurrent_change ... ok
test theme::files::tests::read_errors_preserve_bytes_and_mode_is_kept ... ok
test agent_island::appserver::tests::attaches_initializes_and_forwards_notifications ... ok
test ir::tests::serial_protocol_over_socketpair ... ok
test util::tests::temp_and_backup_never_follow_symlinks_and_mode_is_kept ... ok
test theme::legacy::tests::oneshot_exec_semantics ... ok
test agent_island::install::tests::concurrent_settings_edit_is_preserved ... ok
test agent_island::install::tests::install_in_temp_home_writes_once_then_never ... ok
test util::watch_tests::one_watch_survives_repeated_atomic_replaces ... ok
test util::watch_tests::filters_by_name_and_sees_missing_dirs ... ok

test result: ok. 62 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.33s

     Running unittests src/bin/hyprv-agent-hook.rs (target/debug/deps/hyprv_agent_hook-67aa044391a5d292)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/live_readonly.rs (target/debug/deps/live_readonly-851e9f93edccf288)

running 5 tests
test hyprland_socket_readonly_queries ... ok
test systemd_user_manager_readonly ... ok
test agent_island_follows_hook_writes ... FAILED
test hook_binary_records_observation_only ... ok
test all_actors_start_and_publish ... ok

failures:

---- agent_island_follows_hook_writes stdout ----

thread 'agent_island_follows_hook_writes' (1752386) panicked at crates/hyprv-svc-desktop/tests/live_readonly.rs:130:5:
assertion failed: (s.sessions[0].progress_at(now) - 0.42).abs() < 1e-9
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    agent_island_follows_hook_writes

test result: FAILED. 4 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s

error: test failed, to rerun pass `-p hyprv-svc-desktop --test live_readonly`

```

Source fingerprints at failure:
`crates/hyprv-svc-desktop/examples/agent_bench.rs`: {'sha256': '356061a41c527fbac5e1f2f0d93c50bcacb613cd248fda39179c9844acc56c3a', 'mtime': '2026-10-23T07:38:04+00:00'}
`crates/hyprv-svc-desktop/examples/agent_cost.rs`: {'sha256': 'd520b5c9adeb4986fe0f675c50da3f91e4f2d1607f6937e26023c4563097ec54', 'mtime': '2026-10-23T07:38:04+00:00'}
`crates/hyprv-svc-desktop/examples/idle_probe.rs`: {'sha256': '849f14d2e427a81cceaf6fa159db3329bf95fa0d8cc4292b3989c1bfbb776494', 'mtime': '2026-10-23T07:19:54+00:00'}
`crates/hyprv-svc-desktop/src/bin/hyprv-agent-hook.rs`: {'sha256': 'd59dbf5709263d0d37df657470cf3be5456367836d392f2d9076e3a8e7d4e5ee', 'mtime': '2026-10-23T07:02:51+00:00'}
`crates/hyprv-svc-desktop/src/lib.rs`: {'sha256': '37610d398a15a6373452a5294897d6559e6c909483548ee51e88d6f486a51bfa', 'mtime': '2026-10-23T07:02:51+00:00'}
`crates/hyprv-svc-desktop/tests/live_readonly.rs`: {'sha256': '6d1ad82191defc5685e93a2b8982ff74fce00934117786fb52eef1f6c3e99542', 'mtime': '2026-10-23T07:02:51+00:00'}


### Integration CI — 2026-10-23T14:21:12+00:00 — clippy: mismatched types

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-svc-desktop/src/agent_island/mod.rs:331:39`.

`crates/hyprv-svc-desktop/src/agent_island/mod.rs`: mtime 2026-10-23T14:21:02+00:00; SHA-256 `ff37413e67ca347f7308029b1c7a7605ad59da0bcbb0f5a0371302b27db50648`; changed during command: True.

```text
error[E0308]: mismatched types
   --> crates/hyprv-svc-desktop/src/agent_island/mod.rs:331:39
    |
331 |                     self.refresh_full(scan, &mut synthetic, messages).await.ok()
    |                          ------------ ^^^^ expected `Option<&mut ProcTable>`, found `bool`
    |                          |
    |                          arguments to this method are incorrect
    |
    = note: expected enum `std::option::Option<&mut agent_island::model::ProcTable>`
               found type `bool`
note: method defined here
   --> crates/hyprv-svc-desktop/src/agent_island/mod.rs:253:14
    |
253 |     async fn refresh_full(
    |              ^^^^^^^^^^^^
254 |         &self,
255 |         scan: Option<&mut model::ProcTable>,
    |         -----------------------------------


```

Full log: `docs/verify/raw/ci/round-22/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T14:21:12+00:00 — clippy: mismatched types

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-svc-desktop/src/agent_island/mod.rs:350:43`.

`crates/hyprv-svc-desktop/src/agent_island/mod.rs`: mtime 2026-10-23T14:21:02+00:00; SHA-256 `ff37413e67ca347f7308029b1c7a7605ad59da0bcbb0f5a0371302b27db50648`; changed during command: True.

```text
error[E0308]: mismatched types
   --> crates/hyprv-svc-desktop/src/agent_island/mod.rs:350:43
    |
350 | ...                   .refresh_full(false, &mut synthetic, Vec::new())
    |                        ------------ ^^^^^ expected `Option<&mut ProcTable>`, found `bool`
    |                        |
    |                        arguments to this method are incorrect
    |
    = note: expected enum `std::option::Option<&mut agent_island::model::ProcTable>`
               found type `bool`
note: method defined here
   --> crates/hyprv-svc-desktop/src/agent_island/mod.rs:253:14
    |
253 |     async fn refresh_full(
    |              ^^^^^^^^^^^^
254 |         &self,
255 |         scan: Option<&mut model::ProcTable>,
    |         -----------------------------------


```

Full log: `docs/verify/raw/ci/round-22/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T14:21:12+00:00 — clippy: mismatched types

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-svc-desktop/src/agent_island/mod.rs:524:38`.

`crates/hyprv-svc-desktop/src/agent_island/mod.rs`: mtime 2026-10-23T14:21:02+00:00; SHA-256 `ff37413e67ca347f7308029b1c7a7605ad59da0bcbb0f5a0371302b27db50648`; changed during command: True.

```text
error[E0308]: mismatched types
   --> crates/hyprv-svc-desktop/src/agent_island/mod.rs:524:38
    |
524 |         let r = refresh(&dir, &proc, true, &mut overlay, false, &[]).unwrap();
    |                 -------              ^^^^ expected `Option<&ProcTable>`, found `bool`
    |                 |
    |                 arguments to this function are incorrect
    |
    = note: expected enum `std::option::Option<&agent_island::model::ProcTable>`
               found type `bool`
note: function defined here
   --> crates/hyprv-svc-desktop/src/agent_island/mod.rs:139:4
    |
139 | fn refresh(
    |    ^^^^^^^
...
142 |     scan: Option<&model::ProcTable>,
    |     -------------------------------


```

Full log: `docs/verify/raw/ci/round-22/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T14:21:12+00:00 — clippy: mismatched types

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-svc-desktop/src/agent_island/mod.rs:529:38`.

`crates/hyprv-svc-desktop/src/agent_island/mod.rs`: mtime 2026-10-23T14:21:02+00:00; SHA-256 `ff37413e67ca347f7308029b1c7a7605ad59da0bcbb0f5a0371302b27db50648`; changed during command: True.

```text
error[E0308]: mismatched types
   --> crates/hyprv-svc-desktop/src/agent_island/mod.rs:529:38
    |
529 |         let r = refresh(&dir, &proc, true, &mut overlay, false, &[]).unwrap();
    |                 -------              ^^^^ expected `Option<&ProcTable>`, found `bool`
    |                 |
    |                 arguments to this function are incorrect
    |
    = note: expected enum `std::option::Option<&agent_island::model::ProcTable>`
               found type `bool`
note: function defined here
   --> crates/hyprv-svc-desktop/src/agent_island/mod.rs:139:4
    |
139 | fn refresh(
    |    ^^^^^^^
...
142 |     scan: Option<&model::ProcTable>,
    |     -------------------------------


```

Full log: `docs/verify/raw/ci/round-22/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T14:21:12+00:00 — clippy: mismatched types

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-svc-desktop/src/agent_island/mod.rs:532:38`.

`crates/hyprv-svc-desktop/src/agent_island/mod.rs`: mtime 2026-10-23T14:21:02+00:00; SHA-256 `ff37413e67ca347f7308029b1c7a7605ad59da0bcbb0f5a0371302b27db50648`; changed during command: True.

```text
error[E0308]: mismatched types
   --> crates/hyprv-svc-desktop/src/agent_island/mod.rs:532:38
    |
532 |         let r = refresh(&dir, &proc, false, &mut overlay, false, &[]).unwrap();
    |                 -------              ^^^^^ expected `Option<&ProcTable>`, found `bool`
    |                 |
    |                 arguments to this function are incorrect
    |
    = note: expected enum `std::option::Option<&agent_island::model::ProcTable>`
               found type `bool`
note: function defined here
   --> crates/hyprv-svc-desktop/src/agent_island/mod.rs:139:4
    |
139 | fn refresh(
    |    ^^^^^^^
...
142 |     scan: Option<&model::ProcTable>,
    |     -------------------------------


```

Full log: `docs/verify/raw/ci/round-22/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T14:21:18+00:00 — production release staging failed

Local, non-installing `packaging/build-release.sh` exited 1. See `docs/verify/raw/ci/round-22/stage-release.log`. Build output:

```text
cargo build --manifest-path /home/wilf/.config/HyprV/rshell/Cargo.toml --locked --release --target-dir /home/wilf/.config/HyprV/rshell/target --config /home/wilf/.config/HyprV/rshell/packaging/release.toml -p hyprv-shell -p hyprv-privhelper --bins
Stage hyprv-shell, hyprv-privhelper, hyprv-cooling-failsafe, build.json and the hash-manifested release/ (MANIFEST.sha256) under /home/wilf/.config/HyprV/rshell/packaging/dist
    Updating crates.io index
warning: failed to write cache, path: /home/wilf/.cargo/registry/index/index.crates.io-1949cf8c6b5b557f/.cache/li/bz/libz-sys, error: Permission denied (os error 13)
   Compiling hyprv-ui v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui)
   Compiling hyprv-svc-desktop v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop)
error[E0308]: mismatched types
   --> crates/hyprv-svc-desktop/src/agent_island/mod.rs:331:39
    |
331 |                     self.refresh_full(scan, &mut synthetic, messages).await.ok()
    |                          ------------ ^^^^ expected `Option<&mut ProcTable>`, found `bool`
    |                          |
    |                          arguments to this method are incorrect
    |
    = note: expected enum `std::option::Option<&mut ProcTable>`
               found type `bool`
note: method defined here
   --> crates/hyprv-svc-desktop/src/agent_island/mod.rs:253:14
    |
253 |     async fn refresh_full(
    |              ^^^^^^^^^^^^
254 |         &self,
255 |         scan: Option<&mut model::ProcTable>,
    |         -----------------------------------

error[E0308]: mismatched types
   --> crates/hyprv-svc-desktop/src/agent_island/mod.rs:350:43
    |
350 | ...                   .refresh_full(false, &mut synthetic, Vec::new())
    |                        ------------ ^^^^^ expected `Option<&mut ProcTable>`, found `bool`
    |                        |
    |                        arguments to this method are incorrect
    |
    = note: expected enum `std::option::Option<&mut ProcTable>`
               found type `bool`
note: method defined here
   --> crates/hyprv-svc-desktop/src/agent_island/mod.rs:253:14
    |
253 |     async fn refresh_full(
    |              ^^^^^^^^^^^^
254 |         &self,
255 |         scan: Option<&mut model::ProcTable>,
    |         -----------------------------------

   Compiling hyprv-widgets v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-widgets)
For more information about this error, try `rustc --explain E0308`.
error: could not compile `hyprv-svc-desktop` (lib) due to 2 previous errors
warning: build failed, waiting for other jobs to finish...
Traceback (most recent call last):
  File "/home/wilf/.config/HyprV/rshell/packaging/build_release.py", line 64, in <module>
    if __name__=='__main__':main()
                            ~~~~^^
  File "/home/wilf/.config/HyprV/rshell/packaging/build_release.py", line 53, in main
    before=sources();subprocess.run(cmd,cwd=ROOT,check=True)
                     ~~~~~~~~~~~~~~^^^^^^^^^^^^^^^^^^^^^^^^^
  File "/usr/lib/python3.14/subprocess.py", line 578, in run
    raise CalledProcessError(retcode, process.args,
                             output=stdout, stderr=stderr)
subprocess.CalledProcessError: Command '['cargo', 'build', '--manifest-path', '/home/wilf/.config/HyprV/rshell/Cargo.toml', '--locked', '--release', '--target-dir', '/home/wilf/.config/HyprV/rshell/target', '--config', '/home/wilf/.config/HyprV/rshell/packaging/release.toml', '-p', 'hyprv-shell', '-p', 'hyprv-privhelper', '--bins']' returned non-zero exit status 101.

```


### Integration CI — 2026-10-23T14:28:16+00:00 — tests failed without compiler diagnostic

Command `cargo test --workspace --locked --message-format=json` exited 101. See `docs/verify/raw/ci/round-23/tests.log`. Tail:

```text
ssets::tests::unresolvable_icon_names_draw_quickshells_missing_icon_not_a_letter ... ok
test assembly::review_regressions::preview_deduplication_is_limited_to_one_pointer_gesture ... ok
test assembly::review_regressions::output_removal_drops_open_and_closing_controllers_and_deadlines ... ok
test assembly::review_regressions::fractional_bar_clip_is_normalized_before_reconciliation ... ok
test assembly::review_regressions::cropped_panel_updates_the_original_retained_scene ... ok
test assembly::review_regressions::idle_island_and_calendar_share_trees_during_container_motion ... ok
test assembly::review_regressions::popup_crop_preserves_device_phase_and_outside_input_coverage ... ok
test assembly::review_regressions::scheduled_bar_updates_retain_modules_and_match_fresh_trees ... ok

test result: ok. 34 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.61s

     Running unittests src/main.rs (target/debug/deps/hyprv_shell-68ac4cd6fc95a904)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/cli.rs (target/debug/deps/cli-e6710284ce6d21da)

running 5 tests
test live_popups_and_calendar_survive_fractional_scale_startup ... ignored, bounded live Wayland host smoke; run explicitly with WAYLAND_DISPLAY
test nested_fixture_replays_full_outputs_and_does_not_pass_missing_references ... ignored, isolated assembled fixture GPU replay; requires Hyprland, KWin, dbus-run-session and cc
test nested_functional_matrix_uses_native_input_and_mocked_service_commands ... ignored, full native-input functional matrix; private Hyprland/KWin, virtual input protocols, cc, and XKB required
test nested_test_captures_real_gpu_surfaces ... ignored, isolated compositor smoke; requires Hyprland, kwin_wayland and dbus-run-session
test single_instance_cli_and_shutdown_acknowledgement ... ok

test result: ok. 1 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 0.03s

     Running tests/desktop_cli.rs (target/debug/deps/desktop_cli-0a17d6ee2550a00f)

running 1 test
test desktop_cli_falls_back_in_process_when_no_shell_is_running ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running tests/desktop_ipc.rs (target/debug/deps/desktop_ipc-c39d6ce34bcad641)

running 1 test
test desktop_ipc_routes_variant_and_reflow_to_services ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/no_system_bus.rs (target/debug/deps/no_system_bus-9a55b7ba0e407344)

running 1 test
test shell_services_start_without_system_bus_and_recover ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.28s

     Running unittests src/lib.rs (target/debug/deps/hyprv_svc_desktop-8b0124ea4171ad39)

running 64 tests
test agent_island::appserver::tests::b64 ... ok
test agent_island::model::tests::process_scan_synthesizes_and_completes ... ok
test agent_island::focus::tests::scoring_matches_python ... ok
test agent_island::focus::tests::kitty_rc_frame ... ok
test agent_island::install::tests::codex_hooks_and_feature ... ok
test agent_island::incremental::tests::matches_full_path_and_parses_only_changed_sessions ... ok
test agent_island::install::tests::managed_detection_is_exact ... ok
test applications::tests::fetcher_rules ... ok
test applications::tests::exec_field_codes ... ok
test agent_island::model::tests::public_view_sorting_and_idle_hiding ... ok
test agent_island::ojson::tests::round_trip_keeps_order_and_bytes ... ok
test agent_island::model::tests::rescans_do_not_renew_public_activity ... ok
test agent_island::model::tests::descriptions ... ok
test agent_island::model::tests::ask_user_question_permission ... ok
test agent_island::install::tests::replaces_only_managed_entries ... ok
test agent_island::model::tests::hook_event_flow_matches_python ... ok
test agent_island::model::tests::app_server_messages_match_python ... ok
test demo::tests::verification ... ok
test deskflow::tests::status_regex ... ok
test egpu::tests::bdf_validation ... ok
test deskflow::tests::template_matches_live_config_layout ... ok
test agent_island::model::tests::cleanup_and_expiry ... ok
test hypr::tests::error_heuristic_matches_grep ... ok
test demo::tests::monitor_selection ... ok
test ir::tests::nec_decode_and_frame ... ok
test hypr::tests::quoting ... ok
test ir::tests::real_database_shape ... ok
test agent_island::ojson::tests::real_agent_configs_round_trip_byte_identically ... ok
test recording::tests::pgrep_pattern ... ok
test theme::apply::tests::introspection_children ... ok
test theme::apply::tests::plan_names ... ok
test theme::apply::tests::kitty_theme_parse ... ok
test theme::dconf::glib_bytes::matches_glib_serialization ... ok
test theme::files::tests::assignment_and_xsettings ... ok
test theme::dconf::tests::gvariant_changeset_bytes ... ok
test theme::files::tests::ini_semantics_match_awk ... ok
test theme::files::tests::ui_state_parse ... ok
test uevent::tests::parses_kernel_message ... ok
test util::tests::iso_shape ... ok
test util::tests::stat_with_parens_in_comm ... ok
test startup::tests::only_explicit_launch_approves ... ok
test agent_island::install::tests::hook_paths_are_one_shell_argument ... ok
test demo::tests::journal_scoping ... ok
test agent_island::install::tests::toml_edits_preserve_structure_and_validity ... ok
test applications::tests::index_dedupes_later_wins_and_sorts ... ok
test viewflow::tests::resolver_contract ... ok
test hypr::tests::request_wire_format ... ok
test agent_island::install::tests::preflight_errors_write_nothing ... ok
test agent_island::tests::read_only_overlay_keeps_synthetic_sessions_without_writing ... ok
test viewflow::tests::readiness_predicate ... ok
test agent_island::model::tests::proc_table_is_incremental_and_reports_discovery_changes ... ok
test egpu::tests::inventory_topology ... ok
test util::tests::atomic_write_and_symlink ... ok
test theme::tests::ensure_and_load_with_legacy_links ... ok
test util::tests::compare_and_swap_rejects_concurrent_change ... ok
test theme::files::tests::read_errors_preserve_bytes_and_mode_is_kept ... ok
test util::tests::temp_and_backup_never_follow_symlinks_and_mode_is_kept ... ok
test ir::tests::serial_protocol_over_socketpair ... ok
test agent_island::appserver::tests::attaches_initializes_and_forwards_notifications ... ok
test theme::legacy::tests::oneshot_exec_semantics ... ok
test agent_island::install::tests::install_in_temp_home_writes_once_then_never ... ok
test agent_island::install::tests::concurrent_settings_edit_is_preserved ... ok
test util::watch_tests::filters_by_name_and_sees_missing_dirs ... ok
test util::watch_tests::one_watch_survives_repeated_atomic_replaces ... ok

test result: ok. 64 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.43s

     Running unittests src/bin/hyprv-agent-hook.rs (target/debug/deps/hyprv_agent_hook-67aa044391a5d292)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/live_readonly.rs (target/debug/deps/live_readonly-851e9f93edccf288)

running 5 tests
test hyprland_socket_readonly_queries ... ok
test systemd_user_manager_readonly ... ok
test hook_binary_records_observation_only ... ok
test agent_island_follows_hook_writes ... FAILED
test all_actors_start_and_publish ... ok

failures:

---- agent_island_follows_hook_writes stdout ----

thread 'agent_island_follows_hook_writes' (2153607) panicked at crates/hyprv-svc-desktop/tests/live_readonly.rs:130:5:
assertion failed: (s.sessions[0].progress_at(now) - 0.42).abs() < 1e-9
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    agent_island_follows_hook_writes

test result: FAILED. 4 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s

error: test failed, to rerun pass `-p hyprv-svc-desktop --test live_readonly`

```

Source fingerprints at failure:
`crates/hyprv-svc-desktop/examples/agent_bench.rs`: {'sha256': '356061a41c527fbac5e1f2f0d93c50bcacb613cd248fda39179c9844acc56c3a', 'mtime': '2026-10-23T07:38:04+00:00'}
`crates/hyprv-svc-desktop/examples/agent_cost.rs`: {'sha256': 'd520b5c9adeb4986fe0f675c50da3f91e4f2d1607f6937e26023c4563097ec54', 'mtime': '2026-10-23T07:38:04+00:00'}
`crates/hyprv-svc-desktop/examples/idle_probe.rs`: {'sha256': '849f14d2e427a81cceaf6fa159db3329bf95fa0d8cc4292b3989c1bfbb776494', 'mtime': '2026-10-23T07:19:54+00:00'}
`crates/hyprv-svc-desktop/examples/proc_probe_tmp.rs`: {'sha256': 'd7c613748c836be5fd3c59fab554bdaa19411671deaca5617b6de7caa3eb3527', 'mtime': '2026-10-23T14:26:31+00:00'}
`crates/hyprv-svc-desktop/src/bin/hyprv-agent-hook.rs`: {'sha256': 'd59dbf5709263d0d37df657470cf3be5456367836d392f2d9076e3a8e7d4e5ee', 'mtime': '2026-10-23T07:02:51+00:00'}
`crates/hyprv-svc-desktop/src/lib.rs`: {'sha256': '37610d398a15a6373452a5294897d6559e6c909483548ee51e88d6f486a51bfa', 'mtime': '2026-10-23T07:02:51+00:00'}
`crates/hyprv-svc-desktop/tests/live_readonly.rs`: {'sha256': '6d1ad82191defc5685e93a2b8982ff74fce00934117786fb52eef1f6c3e99542', 'mtime': '2026-10-23T07:02:51+00:00'}

- [DESKTOP-R2-1](../reviews/hyprv-svc-desktop-r2.md#desktop-r2-1) — P2: cached reused/reparented non-agent ancestry falsely attributes an inhibitor to Codex; failing fake-proc probe.
- [DESKTOP-R2-2](../reviews/hyprv-svc-desktop-r2.md#desktop-r2-2) — P2: initial-snapshot fork/exec is missed until the 60-second sweep; failing equivalence probe.

Implementation claim (reviewer): DESKTOP-R2-1/2 only in agent_island/model.rs ProcTable identity/ancestry refresh and regressions, plus Actor::probe blocking-pool dispatch in mod.rs. Re-stat cached identities at the leased discovery cadence to catch exec/reparent/reuse, keep new-PID window and 60-second readdir sweep. Leaving session normalization/hooks/appserver edits alone.

Fixed DESKTOP-R2-1/2: ProcTable now revalidates cached stat/ppid and named argv on every existing leased probe, including initial and old PIDs; rejects observed stat/argv/pidfd identity changes. Preserved new-PID window and60s readdir reconciliation. Actor::probe always performs that filesystem work on the blocking pool; no unleased timer added. This increases per-leased-scan stat work intentionally to preserve exec/reparent/reuse correctness; no new idle performance claim. Separate process_regressions.rs verifies initial/late/same-name exec and reused/reparented ancestry against a fresh scan; original probes now pass. Session normalization/hooks/appserver paths untouched. Final desktop72 + shell46 + widgets64 =182 library passes; strict scoped all-targets Clippy passes. Details: ../verify/discovery-icons-r2-fixes.md.


### Hourly real-shell idle budget — 2026-10-23T17:05UTC

Hourly real release refresh ci-20261023t165719z (snapshot 36e621a706c0dce82ab1aa9d3010c017cbfd8b621266e46c29707e038fe60193): host process namespace with frozen external agent-hook files still costs1.12074% summed-thread CPU over60.137925s, with0 native hook notifications. Five blocking workers consume0.115896+0.128283+0.105602+0.118430+0.119656=0.587867 CPU s; mainUI0.052995s/services0.033129s. Controlled private-PID quiet is0.16857%. The current residual remains concentrated in blocking workers with host discovery retained; this observation does not claim a fresh sampled attribution to a specific scanner line. Please track proc-discovery optimization against these real-shell receipts, preserving unhooked discovery semantics. Live-hook CPU1.43472% and44 notifications are reported separately; do not subtract inter-window CPU as causal hook cost.


### Hourly real-shell idle budget — 2026-10-23T17:46UTC

Second hourly real-release sample ci-20261023t174038z (snapshot 472960c66901692361bf4d1cf84050f125f209c1611f009237b6742a434c80c6): quiet CPU0.20602%,0 native hook events; frozen-hook host CPU1.1744%,0 hooks; live-hook CPU1.37195%,21 notifications. <=0.1% remains unmet. Per-thread CPU seconds: quiet: [('hyprv-shell', 0.068882), ('hyprv-services', 0.05245), ('tokio-rt-worker', 0.000512), ('hyprv-pw-audio', 0.0), ('tokio-rt-worker', 0.000522), ('tokio-rt-worker', 0.000499), ('tokio-rt-worker', 0.000525), ('tokio-rt-worker', 0.000505), ('module-rt', 0.0)]; no-hooks: [('hyprv-shell', 0.068898), ('hyprv-services', 0.041086), ('tokio-rt-worker', 0.11955), ('hyprv-pw-audio', 0.0), ('tokio-rt-worker', 0.119923), ('tokio-rt-worker', 0.117609), ('tokio-rt-worker', 0.117348), ('tokio-rt-worker', 0.1235), ('module-rt', 0.0)]; live: [('hyprv-shell', 0.102904), ('hyprv-services', 0.063767), ('tokio-rt-worker', 0.171013), ('hyprv-pw-audio', 0.0), ('tokio-rt-worker', 0.167267), ('tokio-rt-worker', 0.139558), ('tokio-rt-worker', 0.180586), ('module-rt', 0.0)]. See docs/verify/perf-shell.md hourly history. Different concurrent source snapshots and input/event conditions are integration observations, not proof of any one change causing the difference.


### Real-shell final idle checkpoint — 2026-10-23T18:30UTC

Final pre-deadline refresh ci-20261023t182400z (snapshot 2b7f008d4baeab9ff32471b1b48a6c532bea3d9f8fdf12ae9b711edf64bdf3df): controlled-quiet thread CPU 0.24114% with0 native hook notifications; frozen-hook host discovery 1.27011%; live-hook idle 1.44477% with21 notifications. Every interval>=60s, uninstrumented release actual-shell, same archived ELF for these windows. Idle target <=0.1% remains unmet; see docs/verify/perf-shell.md hourly history and raw per-thread counters. No inter-window event cost subtraction.

Service-idle attribution in progress: temporary IDLE_TRACE_START/END future wrappers installed for a diagnostic release build; core exports a scratch-only poll/CPU/waker logger. These markers will be removed before final performance measurements. Please preserve unrelated changes; no private Cargo target or workspace copy. Normal release ELF saved separately; diagnostics will not count toward the CPU budget.

Service-idle experiment cleanup: all 90 temporary future/blocking wrappers, legacy-mode branches, decoder event tags and core export removed; crate search is empty. Scoped media/sys tests:80passed,5intentionally ignored; strict all-target Clippy passes. Normal release 777b2a29 verified to include tray/wire and no tracing symbols. See docs/verify/service-runtime-idle.md for attribution and budget results; no permanent code changes to core/net/power/desktop in this task.

Agent probe optimization in progress: cached proc/stat + cmdline/loadavg/pid_max readers, borrowed stat parser, reused PID/poll/discovery scratch sets, cached argv classification. Per-probe identity/ancestry/named-argv checks, 60s sweep and PID-reuse guards remain. Existing desktop regressions pass. Coordinating changes solely through requests; please preserve these surgical model/util edits.


### Snapshot CI — 2026-10-23T19:58:27+00:00 — clippy

REPRODUCIBLE: failed twice on same snapshot. Snapshot `/tmp/hci-sys/round-26-20261023T195304Z/rshell`; fingerprint `f6433223079504ccc3a83f216bbe62e58d1dd94e476021239956c8872d344529`. Concurrent live edits cannot change this source copy.

Command: `['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '--message-format=json', '--', '-D', 'warnings']`. Attempts: `[{'code': 101, 'log': 'round-26-20261023T195304Z/05-clippy-1.log'}, {'code': 101, 'log': 'round-26-20261023T195304Z/05-clippy-2.log'}]`. Locations: crates/hyprv-svc-desktop/src/agent_island/model.rs:1215, crates/hyprv-svc-desktop/src/agent_island/model.rs:1215, crates/hyprv-svc-desktop/src/agent_island/model.rs:1215, crates/hyprv-svc-desktop/src/agent_island/model.rs:1215.

`crates/hyprv-svc-desktop/src/agent_island/model.rs`: `6d39cbb38fdc3215cb8298664161e817e19cca2c33a6cc91e1a443f0ed988cf0`

```text
error: this boolean expression can be simplified
    --> crates/hyprv-svc-desktop/src/agent_island/model.rs:1215:20
     |
1215 | ...            if !seen.stat.prefix().ok().and_then(crate::util::stat_fields).is_some_and(|(_,_,t)|t==start){self.forget(pid);return}
     |                   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ help: try: `seen.stat.prefix().ok().and_then(crate::util::stat_fields).is_none_or(|(_,_,t)| t != start)`
     |
     = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.95.0/index.html#nonminimal_bool
     = note: `-D clippy::nonminimal-bool` implied by `-D warnings`
     = help: to override `-D warnings` add `#[allow(clippy::nonminimal_bool)]`


```

Logs relative to `docs/verify/raw/ci-snapshots/`. No causal commit inferred.

- 2026-10-23 sampling-cost implementation: Agent discovery optimization: cached bounded stat/cmdline/loadavg/pid_max readers and directory FD/getdents buffer, reused key/poll/sweep scratch storage and argv classification. Identity/PPID/argv revalidation, pidfds, PID reuse checks and 60 s reconciliation retained. The leased 1.5 s probe uses a single sleeping hyprv-agentproc worker/reusable slot instead of boxed spawn_blocking jobs; creation failure retains the pool fallback. Zero-allocation regressions cover stable named agents plus sweep; worker completion/drop test verifies handoff and ownership release. Release quiet-namespace refresh 1.7 us/probe, zero allocations; report docs/verify/sampling-cost.md.

- 2026-10-23 sampling validation: Final allocation edge case fixed: stable non-UTF-8 argv no longer allocates temporary lossy strings on each equality check. Regression failed with 162 allocations before fix, now zero. Real 1.5 s paced quiet-namespace refresh mean 24.477 us/p99 27.319 us; zero allocations over 40 probes including reconciliation. All 74 desktop tests/strict Clippy pass. See docs/verify/sampling-cost.md.

### B2 (final reviews): agent discovery cost proportional to agents, not to /proc — svc-desktop, 2026-10-23

**Cause.** `ProcTable::probe` re-stat'ed every known PID on every 1.5 s leased scan, to keep DESKTOP-R2-1/R2-2 correct. That came to 1,736 PIDs on this host and **10.44 ms/probe** (p50 10.34, p99 13.0; `real_probe_cpu`, paced, real `/proc`), so `hyprv-agentproc` used 416–494 ms per 60 s. The "1.7 µs/probe" figure came from an empty PID namespace.

**Fix** (`agent_island/model.rs`, `util.rs::ExecWatch`). Each probe now:
- polls the pidfds of agent-named PIDs for exits;
- stats the IDs allocated since the previous probe;
- re-checks **once** every PID first seen by the previous refresh, including the whole initial snapshot (DESKTOP-R2-2);
- re-stats agent-named PIDs: an exec away from an agent, or an argv change;
- re-validates every link of each inhibitor's ancestry chain (`validate_chains`, DESKTOP-R2-1).

A late `exec` of an agent binary in an old non-agent PID is signalled by **unprivileged fanotify `FAN_OPEN_EXEC`** (`FAN_REPORT_FID`, inode marks on the agent executables found on `PATH` and on discovered agents' `/proc/PID/exe`; re-marked at each sweep). It is answered by one full re-stat, rate-limited to every `EXEC_RESCAN_SECONDS` = 15 s, which skips kernel threads.

Why fanotify and not inotify `IN_OPEN`:
- inotify `IN_OPEN` also fires on plain reads. The Codex binary got 7–8 opens per 30–40 s with no new processes.
- `FAN_OPEN_EXEC` fires only on exec. This host still execs Codex helpers 11–14 times a minute; unprivileged events carry no PID, hence the rate limit.

The 60 s full sweep remains the backstop, also for agents started from unmarked paths. The stat re-read after an unchanged argv was dropped, since it only guards a change.

**Contract change.** A late exec in an old non-agent PID is now found at the next probe when signalled (≤ 15 s under heavy exec churn), otherwise at the 60 s sweep; previously every probe. An exec right after fork (R2-2) and reparented or reused ancestors (R2-1) keep the per-probe guarantee.

**Tests** (`process_regressions.rs`):
- `initial_and_late_exec_are_observed_without_pid_allocation`: the late step is now signalled;
- new `unsignalled_late_exec_waits_for_the_sweep_and_signals_are_rate_limited`;
- new `real_late_exec_is_signalled_by_fanotify`: real `/proc`, a copied `sleep` named `claude`, a waiting shell that execs it;
- `reused_and_reparented_ancestors_match_a_fresh_scan` and the zero-allocation test are unchanged and pass.

`cargo test -p hyprv-svc-desktop`: 76 unit + 5 live read-only pass, clippy clean.

**Numbers** (real host, real `/proc`, 19 agents):

| Measure | Before | After |
|---|---:|---:|
| `real_probe_cpu` mean, paced (40 × 1.5 s) | 10.44 ms | **2.13 ms** (p50 1.29, p99 9.94 = the rate-limited full pass) |

Real shell, `measure-shell.py`, release ELF `d15f44f9…`, 60 s:

| Run | CPU | `hyprv-agentproc` |
|---|---:|---:|
| `--quiet-hooks` (frozen hooks) | **0.265% / 0.266%** (two runs) | 84 / 99 ms per 60 s (was 416–494) |
| `--quiet-agents` (controlled quiet) | **0.100%** | 1 ms |

Raw: `docs/verify/raw/perf-shell/b2-{nohooks,nohooks-r2,quiet}/`. Live Quickshell is not a valid comparison right now: its agent observer appears stalled since the disk-full incident; see the final reviews.

### FR-NET-1 (svc-net final review): installer final check→rename window — fixed (svc-desktop, 2026-10-23)

`util::replace_checked`, used by the hook installer and the theme writer, no longer does a check followed by an unconditional `renameat`.

| `Expect` | Commit |
|---|---|
| `Bytes` | Write the temp file and fsync, then `renameat2(RENAME_EXCHANGE)` it with the target. Compare the **displaced** file with the bytes the caller read. |
| `Absent` | `RENAME_NOREPLACE` (a file created meanwhile wins). |
| `Any` | Plain rename. |

For `Bytes`, after the exchange:
- **Displaced bytes match:** success. The displaced generation equals the caller's pre-commit versioned backup, so nothing is lost.
- **Displaced bytes differ:** another writer landed after the read. The exchange is reversed, so the user's newer content is the target again. `Conflict` is returned, and `install::commit` rebases (re-reads, re-applies the edit) up to 5 times.
- **A third writer during the swap back:** if the file returned to the temp name is not the installer's own bytes, it is kept as `<name>.hyprv-conflict.<ms>`, never deleted.

A temp name is unlinked only while it provably holds the writer's own bytes. Filesystems that reject the flags fall back to the previous behaviour.

**Regression test:** `tests/hook_install_race.rs` builds an `LD_PRELOAD` shim interposing `renameat2` and `renameat`. Just before the installer's swap of `settings.json`, the shim writes a concurrent user edit into it, and optionally again before the swap back. It runs the real `hyprv-agent-hook install` with a scratch HOME/CODEX_HOME/XDG only. Asserted:
- the edit survives and the hook is rebased onto it;
- the displaced user generation is kept as the `.hyprv-bak.` backup;
- no temp file is left;
- the third generation survives as `.hyprv-conflict.`.

**Mutation check:** with the old check-then-rename restored, the test fails with "concurrent edit lost", the reviewer's finding.

`cargo test -p hyprv-svc-desktop`: 76 unit + 2 race + 5 live read-only pass, clippy clean. Also fixed per the same review: the CUTOVER.zh-CN.md / README bootstrap order (pre-install preflight is expected to show only `privhelper installed` BLOCKED; READY is required from `--installed`, which step 2a re-runs), and the README parity/performance/open items.

### FR-NET-1 checkpoint follow-up — svc-sys (static inspection, not a new runtime proof)

The supported-filesystem exchange path and new final-window tests are present. The current `util.rs:183,191` compatibility branches still turn EINVAL/ENOSYS/ENOTSUP into `swap(0)`, restoring the exact check-then-unconditional-rename window identified by FR-NET-1; the doc comment explicitly says so. `Expect::Absent` can also overwrite a file created in that gap. Please fail closed for unsupported checked replacement, or preserve displaced generations there too. Add an interposer case that rejects the nonzero flags and injects a writer before the flags=0 fallback; current race tests only forward successful nonzero flags. No production desktop source changed by svc-sys.

## P1 engine: LIVE idle attribution, 2026-10-24 (scope kept with desktop owner)

User explicitly asked engine to leave your active service edits alone and file
exact findings/proposed changes here. Live PID **2709217**, installed release
**eda5ca090166790a41ddcd171f79008f01f589c766816b0d3040b41c07be0f03**,
ELF **2073ea266e34c5aee6a69038baeaaeb75a2a287da636ee55176469f7d54c89d0**.
No live input/restart/provider write. Source is a moving shared checkout: compare
against installed build.json before assigning this result to your latest edits.

Uninstrumented 180.0017 s live interval:
- Whole process, summed thread runtime: **0.943490% CPU**.
- `hyprv-agentproc`: **0.358488% CPU**, **120 voluntary switches / 180 s =
  0.666660/s**, 324 scheduling slices = **1.799983/s**. ProbeWorker's single
  condition-variable request per discovery tick identifies those 120 voluntary
  wakes as the required 1.5 s discovery cadence; the extra 204 switches were
  involuntary preemption, NOT additional timer wakeups.
- `tokio-rt-worker`: **0.264310% CPU**, **295 voluntary switches = 1.638873/s**,
  408 slices = **2.266645/s**. 180 s user CPU samples show sync_process_sessions,
  session_pid, public_row, JSON deserialization/BTreeMap clone. This identifies
  full agent normalization as a real blocking-pool cost, but the worker is shared:
  do NOT claim that every one of its 295 switches belongs to this actor.
- `hyprv-services`: 0.123308%; UI: 0.197359%.

Receipts: docs/verify/raw/live-idle/{before.json,perf.data,perf-flat.txt,
perf-report.txt,switch.txt}. A separate permitted 30 s PERF_RECORD_SWITCH trace
records agentproc **117 schedule-ins**, worker **147** (including preemption;
not an uninstrumented CPU window). ptrace/strace is denied by Yama;
sched_wakeup tracefs is unreadable, so no exact source-by-source wakeup ledger
is claimed. The kernel records DO count all schedule-ins for that interval.

Concrete optimization proposals (preserve the 1.5 s scan and PID-reuse/exec,
parent-chain, expiry, hook ordering and process-derived session semantics):

1. `model.rs::ProcTable::refresh/probe`: keep the current numeric-ID fast path
   for small ranges, but select the reusable `/proc` directory sweep when the
   allocated-ID interval is large relative to live entries. The current loop
   tries `restat` for *every* newly allocated thread ID, including vanished TIDs.
   Suggested diff shape at `match range`: guard `Some(ids)` using its exact
   iterator size_hint/count; for a large range run `sweep(proc, now)` instead.
   Do not advance `last_sweep` unless its existing full-sweep deadline is due;
   keep validate_chains/exec rechecks equivalent in the directory fallback.
   Add a large-ID-gap/short-lived-TID fixture and compare ProcTable agent keys
   with the exhaustive oracle. Measure syscall count and CPU on host /proc.
2. `mod.rs`, `Actor::run` full-refresh branch: `scan_at.is_some()` currently
   routes **every hook refresh** through refresh_full/spawn_blocking whenever
   any discovered agent exists. That bypasses Incremental even when the last
   scan/table is unchanged. Proposed diff: cache the scan-derived overlay by
   (discovery/scan revision, file bytes/session revisions); apply unchanged
   process-derived mutations to the borrowed per-session cache and use
   refresh_fast for unrelated hook changes. Keep resync_after semantics: a newer
   hook may require the next scan to re-stamp/re-sort process sessions.
   Regression: identical full-path rows/order/expiry/pids/pending across changing
   one of 99 hook sessions, a scan, PID exit/reuse and lease release/reacquire.

The live hook file was 158112 bytes / 99 sessions at a metadata-only observation
(15 process-scan, 84 other); do not archive its contents. A private-compositor
profile using real hooks gives 129 AgentIslandState UI notifications and 192
bar paints / ~60 s, with 114 hook-file events. Those are real state inputs, not
proof of a renderer idle loop. Please return measured before/after costs and
source hashes for your minimal fix; engine will integrate after your checkpoint.

Proposed range-selection diff (NOT APPLIED or compiled by engine; adapt to your
current concurrent changes). This refines the existing fixed MAX_RANGE guard:

```diff
 match range {
-    Some(ids) => self.probe(proc, ids, now),
+    Some(ids) if ids.size_hint().1.is_some_and(|n|
+        n <= self.seen.len().saturating_mul(2).max(256)) =>
+        self.probe(proc, ids, now),
-    None => self.sweep(proc, now),
+    _ => self.sweep(proc, now),
 }
```

This is a profiling hypothesis, not a measured improvement: choose crossover
from real syscall timing, and prove the sweep fallback retains parent/exec
validation before landing. The blocking-pool incremental-scan proposal above is
separate and needs your model oracle; engine will not propose skipping scan
cadence or deleting completion sessions as an idle shortcut.

## P1 svc-desktop: `real_late_exec_is_signalled_by_fanotify` flake — root cause and fix (2026-10-24)

**Reproduced** with the pre-fix lib test binary: full `hyprv-svc-desktop` lib suite, 200 iterations, `--test-threads=16`, under 20 niced busy loops: **9/200** and **10/200** failures, all this test.

**Root cause (test, shared state):** diagnostics on every failure showed that an exec-triggered full pass had already run at t0+1.5, *before* the test's exec. So the test's own signal at t0+3.0 sat pending behind `EXEC_RESCAN_SECONDS` (15 s). The trigger is shared process state, not the test's own exec. The first `/proc` sweep's `mark_agent_binaries` marks the host's `/usr/bin/claude` and `/usr/bin/codex` and the exe of every agent-named process alive at that moment, including `/usr/bin/sleep` while `proc_table_on_real_proc_*` runs `sleep` under the name `claude` in parallel. Any exec of those between the mark and the t0+1.5 drain consumes the budget. The test asserted "next probe", which the documented contract (served at most every 15 s; a pending signal waits out the limit) never promised.

**Test fix (deterministic):** a private `ExecWatch` that marks only the test's own copy proves the signal itself (`!drain()` before the exec, `drain()` after). The table is then held to the contract:
- found at the next probe when no exec pass has used the budget;
- otherwise still pending, and found at `last_exec_scan + EXEC_RESCAN_SECONDS`, before the sweep.

The child is killed and the dir removed by a drop guard. Waiting for the exec now has a 30 s deadline that is asserted, rather than silently giving up after 2 s.

**Runtime race found and fixed:** `FAN_OPEN_EXEC` is queued when `execve` opens the binary, before `begin_new_exec` installs the new comm and argv. A signalled full pass (or a fresh-PID re-check) that lands in that window reads the old image. No further signal follows, so the agent was missed until the 60 s sweep.

Fix (`model.rs` `restat`): any stat that sees a task in `R` or `D` queues that PID for one more stat at the next probe, chained while it stays `R`/`D`. A task inside `execve` after opening the binary is running or in uninterruptible/killable sleep. An exec blocked in an interruptible filesystem wait (FUSE) is left to the sweep, as documented. Kernel threads are excluded. New helper `util::stat_busy`, parsed from the stat already read, so no extra syscall.

New test `exec_in_flight_during_the_signalled_pass_is_seen_at_the_next_probe` covers both `R` and `D`, and checks that a sleeping PID still waits for the sweep. Mutation check: with the busy re-stat removed, it fails at `process_regressions.rs:107`.

**Proof:**
- Post-fix stress: the same 20-load loop, **400/400 full-suite iterations pass**. The rate-limited branch of the real test was taken 11 times and the next-probe branch 389 times.
- `cargo test -p hyprv-svc-desktop`: 77 unit + 2 race + 5 live pass, clippy clean.
- Cost: typically 2–3 tasks are `R`/`D` host-wide. In a paced `real_probe_cpu` A/B at the same time (about 1,800 PIDs, 23 agents), it was 2.76 and 2.93 ms with vs 2.81 and 3.51 ms without: inside noise.

## Laptop Phase A engine — discovery demand handoff (2026-10-24, analysis only)

Earlier live discovery findings and proposed diffs are already filed at line814;
no service edits by engine. New controlled private nested90.168 s window (installed
shell ELF `c364f556a1a5168a7bcac866616e1912a87b06951b7ac321b2a7a0f22d1d52df`,
empty hooks/private PID namespace) still observes **60 discovery-thread voluntary
switches,0.6654/s**, CPU0.00213%. This isolates the retained1.5 s cadence from host
agent workload; it does not repeat the much larger real-/proc CPU attribution above.

Laptop budget requires zero hidden-consumer timers. Please review an explicit
visible-agent demand/event-source design with engine; preserve late exec, completion,
PID reuse, expiry and reconnect semantics. Do not silently gate discovery by an
empty current result (new agents still must appear). Where fanotify/pidfd permissions
cannot supply equivalent discovery, document the required polling fallback and its
budget exception before implementation. Laptop compact strip has no agent content;
expanded bar/island behavior must follow the laptop spec owner's visibility decisions.

Raw: `docs/verify/raw/laptop-minimal-resources/desktop-minimal.json`; accounting and
limits: `docs/spec/laptop-minimal-resources.md`. These are scheduling proxies,
not exact kernel sched_wakeup records. Freeze respected; no service source patch.

### PQ-QA → svc-desktop: laptop agent-island data path (2026-10-24, laptop Phase A)

Spec: `docs/spec/laptop-island.md` §3, §5.
- **D1 (pending orchestrator):** laptop legacy hooks block until the island answers:
  - the `PermissionRequest`/`Notification` installer timeout is 86400 s;
  - the hook polls `responses/<id>.json` every 0.18 s;
  - with no answer it emits deny "No HyprV island response".
  If D1 = (a), `hyprv-agent-hook` needs a profile-gated waiting mode (inotify on `responses/`) and `install.rs` profile timeouts. Action JSON must be byte-identical (§2.4). QA recommends (b): keep read-only.
- **Regardless of D1:**
  - Laptop transport cadence: the sawtooth is sampled at hook events plus a 5 s tick, only while the island is visible. Expiry uses deadline timers. With the bar compact there must be no 650 ms tick (minimal-state budget).
  - Do **not** spawn `codex app-server` on laptop. The legacy spawn yields no sessions; attach-only stays (§5.3).
- **E2E hooks:** `lap.island.*` in `docs/verify/laptop-qa-plan.md` §4 use a private `HYPRV_AGENT_ISLAND_DIR`.

## 2026-10-24 — RAM RGB follows theme (user request, desktop only) — svc-desktop
`theme/ram_rgb.rs`: on every theme change (actor action or external `ui-state.env` edit) and once at start, a single debounced (750 ms) worker runs `openrgb --noautoconnect`: `-ld`, then at most one `-d 0 -m M -d 1 -m M`. Devices 0/1 must be detected as `ENE DRAM` or nothing is changed; device 2 (ASUS board) is never addressed and colors are never written. Dark: non-Off mode captured to `~/.local/state/hyprv/ram-rgb.env` before Off. Light: only devices still Off are restored to the saved mode (missing save → stay off, logged). flock `$XDG_RUNTIME_DIR/hyprv-ram-rgb.lock` serializes across processes; 30 s timeout; failures go to stderr (journal) only. No SDK server exists on this host, so no native SDK client. Disabled when state_home is not `~/.local/state`, `HYPRV_NESTED_*` is set, `HYPRV_RAM_RGB=off` (Env::rooted sets it); `HYPRV_OPENRGB` overrides the binary.
Tests: 6 fake-openrgb tests (capture/off/restore, motherboard untouched, no colors, user change kept, wrong-device guard, failure/missing binary, debounce = 1 list + 1 set per burst, no overlap). Real LEDs round trip (ignored, `HYPRV_RAM_RGB_LIVE=1`): Rainbow→Off→Rainbow on 0/1, device 2 stayed Direct.

**Update 2026-10-24 (PQ-QA): D1 resolved by the user — read-only island on laptop, same as main.** Do not implement the pending card, blocking hooks, laptop hook timeouts, the 5 s cadence quirk or the list-reset quirk. Still required: the island is hidden and does no work while the laptop bar is compact, and there is no tick while hidden. No `codex app-server` spawn. Spec updated: `docs/spec/laptop-island.md`.

## Laptop deploy decision alignment (2026-10-24)

User confirms island READ-ONLY (D1 resolved), no eGPU on laptop, panel2880x1800
scale2. Preserve read-only hooks; do not install legacy blocking permission/action
writers. Required fake contract: all sysfs readers use HYPRV_SYSFS_ROOT; all system
bus clients (including login1/inhibition/profile detection and reconnect paths)
respect DBUS_SYSTEM_BUS_ADDRESS with no host fallback. User permits optional
CPU-only read-only telemetry, not combined desktop cooling/eGPU activation.
See docs/spec/laptop-deploy.md; no desktop source edits made here.

### Shared runtime profile API LANDED (engine, laptop Phase B)

`hyprv_core::profile::{Profile, profile, sys_root, sys_path}` now exists.
`Profile::{Desktop,Laptop}`, `profile() -> Profile` is process-latched; exact
HYPRV_PROFILE override (invalid is fatal), otherwise exact portable DMI or present
System-scope Battery; ambiguous defaults Desktop. `sys_root() -> &Path` latches
HYPRV_SYSFS_ROOT or /; `sys_path(rel: impl AsRef<Path>) -> PathBuf` accepts root-relative
paths, strips leading /, rejects ..; no fallback. Code against this shared API now.
Shell startup/profile/service/UI wiring follows. Do not duplicate detection.

Engine Phase B: shell currently creates DesktopServices through local spawn_desktop
(ctx,inhibit). Please expose/document laptop-safe constructors for deskflow/viewflow/ir/
egpu/demo (no discovery, activation, timers or writes) against shared core Profile.
Can your existing spawn(ctx) select latched profile internally? Please log exact API;
I will not create independent no-op actors that duplicate your service policy.

### svc-power → svc-desktop: laptop CP exact-name recorder input (open, 2026-10-24)

User follow-up asks to close spec-panels' svc-power data requests. Recording is your
owned crate: please select laptop `pgrep -x` semantics against `/proc/PID/comm`
(exact names wf-recorder, wl-screenrec, gpu-screen-recorder; preserve Linux's
15-byte comm limit, do not substitute argv basename). Desktop cmdline matching
must remain unchanged. Current recording.rs::scan uses cmdline_matches on both
profiles. Please add fake-process regression tests (wrapper argv false positive,
exact comm true, truncated long comm false), and report completion here. I will
not edit your crate. Host screen_recording forwarding separately filed to engine.

Engine native laptop services/resources test is approaching final run. Need confirmation
that default DesktopServices factories now honor shared Profile::Laptop with inert
IR/Deskflow/Viewflow/eGPU/demo paths. No matches yet in your src for core profile();
I will not accept a laptop memory/idle reading if desktop discovery jobs still run.
Please log exact ready API or pending status in hyprv-shell.md. Host spawn_desktop
still calls your ordinary factory functions, per the prior request here.

Engine graph integration decision while your factory changes are pending: laptop
spawn_desktop now skips construction of IR (incl RAM LED), Deskflow, Viewflow and
eGPU actors entirely. It supplies default typed topics with closed command channels
(no dormant actor/timer/probe); explicit unavailable actions fail. Demo/recording/
agent/theme/applications/startup retain their actual shared services as required by
laptop UI. deskflowReflow laptop IPC is successful no-op, as deployment contract
requires. Desktop construction remains unchanged. Please log any preferred public
inactive-handle factory later; no duplicated profile detection or service loops added.

### svc-desktop → svc-power / engine: laptop exact-name recorder input DONE (2026-10-24)
`recording::spawn(ctx)` now selects from the shared latched `hyprv_core::profile::profile()`:
- Laptop → `Match::Comm`: exact `/proc/PID/comm` == wf-recorder | wl-screenrec | gpu-screen-recorder. The 15-byte comm is kept, so gpu-screen-recorder never matches; argv is never used.
- Desktop → `Match::Cmdline`: unchanged.

Laptop is event-driven with no period: fanotify exec on the recorder binaries on PATH, inotify re-marking on install or upgrade, bounded 50/250/1000 ms follow-ups per exec, pidfd exits, and one snapshot on `scan_lease`. The fake/real process tests svc-power asked for are in place: wrapper argv false, exact comm true, truncated false. Report: docs/verify/laptop-impl-svc-desktop.md.
- Still `RecordingState.active`, same topic. Engine only needs to forward it to the laptop CP.
- The RAM RGB theme worker no longer starts on Laptop.
