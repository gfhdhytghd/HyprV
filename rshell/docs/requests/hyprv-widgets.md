
Implementation claim (reviewer): WIDGETS-R2-1/2 only in src/icon_theme.rs: bounded file/cache parsing and chain traversal plus malformed-cache regressions. Leaving tray rendering and other widget paths to spec-island.

## Status: spec-panels, R1 tray-strip work, run 3 in progress (2026-10-23, interrupted by usage limit)

**Root cause of R1, found with real Qt 6.12** (a QIcon probe on a private headless weston, plus qtbase 6.12 `qiconloader.cpp`). My run-2 port had both the request size and the algorithm wrong.
- **Request:** Quickshell's `IconImage` asks for 36 px and calls `QIcon::pixmap(36,36)` at qApp DPR 2. Pixmaps are therefore up to 72 px (chrome 64), drawn with `mipmap: true`.
- **Algorithm (`icon_theme.rs`, re-ported verbatim):**
  - directories come in sorted `QSettings::allKeys()` order, or GTK-cache order when the cache is valid;
  - PNG entries are inserted at the front, so they end up in reverse discovery order;
  - `entryForSize`/`directorySizeDelta` are copied as-is, quirks included.

  A test asserts all 108 choices real Qt made on a synthetic theme. The tray now requests 36 at scale 2 (`hyprv-shell/src/assets.rs`). Idle tray result: chrome max 3, discord 0, wechat 3.
- **New `qt_svg.rs` (QtSvg-faithful icon raster):**
  - usvg parse;
  - Qt-raster coverage through `hyprv_ui::qt_painter`;
  - Qt gradient tables, focal radial gradients, spread modes;
  - BYTE_MUL blending and multiplicative opacity;
  - QtSvg treats `rgba()`/`hsl()` as opaque black (measured);
  - `QCosmeticStroker` subdivision for pens ≤ 1 device px.

  keepassxc vs Qt's 72 px pixmap: **max 2**. fcitx: max 138, 90 px > 8 (was 359), mostly on stroked outlines. Next suspects: `QCosmeticStroker` cap/joint handling for flattened polylines in `qt_painter`'s fast-pen path, and the clip path.
- **Not yet done:**
  - wire the hyprv-shell side: `svg_icon` already calls `qt_svg`, but `hyprv-shell` didn't compile at the time because of the engine's in-progress `functional/suite.rs`;
  - re-run the matrix;
  - A1/A2 (waiting on spec-bar).
- **Tools:** `#[ignore]` dump tests `qt_svg::tests::dump` (`HYPRV_QT_SVG_DUMP`) and `assets::tests::dump_icon_file`. The Qt probe sources are only in my scratchpad (`qicon/`); the parity-host probe scene is in `parity/fullscreen/probe/`.

Handoff update: retaining concurrent icon_theme test/QtSvg changes. Adding regression tests in separate src/icon_theme_regressions.rs, not rewriting the owner's current test module. GTK 0xffffffff sentinel fixed; hard chain cap/cycle checks and bounded reads retained.

Fixed WIDGETS-R2-1/2: src/icon_theme.rs now has bounded regular-file reads (libc O_NONBLOCK, explicit dependency/Cargo.lock edge), checked table spans/count and C-string limits, cycle detection and 4096-hop cap; GTK 0xffffffff termination preserved. New src/icon_theme_regressions.rs covers cycles, work budget, bad-offset full fallback and FIFO/size bounds. Also bounded the concurrently added tray_raster's file read/dimensions; no production call sites currently use it. Kept spec-island's size/QtSvg code and test changes. Final: widgets64 + shell46 + desktop72 =182 library passes; strict scoped all-targets Clippy passes. Receipt: ../verify/discovery-icons-r2-fixes.md.


CI round07 attribution withdrawn (16:39UTC): svc-desktop established that the four reference-parity failures were solely an incomplete copy during the 15:19:56–15:22:28Z recapture, with no pixel mismatches. Do not treat these as crate regressions or exempt the tests. Full evidence/correction: docs/requests/hyprv-svc-sys.md, “CI round07”. CI now requires stable build/test/reference hashes across the copy and every manifest-listed PNG present/complete; invalid copies are removed and retried before any checks.

## Receipt: spec-panels, R1 closed (2026-10-23, run 3)

Evidence: `docs/verify/parity-fullscreen.md` (Run 3), `docs/verify/raw/parity-fullscreen/run3/`. The full-screen gate is now 52/64 and every tray icon is within ≤ 4.
- `icon_theme.rs`: verbatim Qt 6.12 lookup and selection (108-case real-Qt test).
- `qt_svg.rs`: QtSvg-faithful icon raster. QtSvg ignores `clip-path` and treats `rgba()`/`hsl()` as opaque black (both measured). Thin pens go through `QCosmeticStroker` subdivision.
- `usvg 0.45.1` (default-features off, already in Cargo.lock via resvg) added to Cargo.toml; `pub mod qt_svg` added to lib.rs.
- 64 lib tests pass; clippy `-D warnings` clean.


### Snapshot CI — 2026-10-23T16:53:06+00:00 — workspace tests

REPRODUCIBLE: failed twice on same snapshot. Snapshot `/tmp/hci-sys/round-09-20261023T164451Z/rshell`; fingerprint `8d755f8c8d79aee523e188c2cb84afcb541288330255c58d1bf3ee0708fb2a8b`. Concurrent live edits cannot change this source copy.

Command: `['cargo', 'test', '--locked', '--workspace', '--all-targets', '--message-format=json']`. Attempts: `[{'code': 101, 'log': 'round-09-20261023T164451Z/03-workspace-tests-1.log'}, {'code': 101, 'log': 'round-09-20261023T164451Z/03-workspace-tests-2.log'}]`. Locations: crates/hyprv-widgets/src/controls/testutil.rs:130.

`crates/hyprv-widgets/src/controls/testutil.rs`: `e078519f6823b278315730f7c3af6fee7d8d37c9973c541a0e8e9cad9111ba65`

```text
amples/export_fixtures-39566c65114c755c)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests examples/shots.rs (/home/wilf/.config/HyprV/rshell/target/debug/examples/shots-363af66346b3997b)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/lib.rs (/home/wilf/.config/HyprV/rshell/target/debug/deps/hyprv_widgets-d15fc92f353d3ed8)

running 65 tests
test controls::kit::tests::hcenter_matches_qt_for_fractional_sizes ... ok
test controls::kit::tests::lighter_on_white_is_noop_and_darker_matches_qt ... ok
test controls::tests::disabled_binding_cancels_even_if_no_pointer_event_arrives_while_disabled ... ok
test controls::tests::disabled_drag_finishes_once_and_cannot_resume ... ok
test controls::tests::disabled_shared_controls_forget_the_previous_press ... ok
test controls::tests::slider_pointer_semantics ... ok
test controls::tests::switch_initial_binding_snaps_then_real_changes_animate ... ok
test controls::tests::text_module_signals ... ok
test controls::tests::switch_knob_animates_160ms_inoutquad ... ok
test controls::tests::text_module_and_split_tile_forget_press_when_disabled ... ok
test controls::tests::tray_button_actions ... ok
test flickable::tests::cancel_and_shrink_leave_no_wakeups ... ok
test flickable::tests::discrete_angles_take_priority_in_unphased_axis_frames ... ok
test flickable::tests::exactly_fitting_content_does_not_steal_child_clicks ... ok
test flickable::tests::pixel_gesture_is_not_a_mouse_press ... ok
test flickable::tests::qt_612_frame_traces ... ok
test icon_theme::review_regressions::chain_work_is_capped_even_without_a_cycle ... ok
test icon_theme::review_regressions::corrupt_cache_falls_back_to_existing_icon ... ok
test icon_theme::review_regressions::cyclic_and_out_of_range_chains_are_invalid_not_empty ... ok
test icon_theme::review_regressions::file_reader_bounds_regular_files_and_never_opens_a_fifo_blocking ... ok
test icon_theme::tests::entry_choice_matches_real_qt_6_12_on_a_synthetic_theme ... ok
test icon_theme::tests::gtk_cache_reader_matches_qt_on_the_system_cache ... ok
test icon_theme::tests::missing_icon_is_quickshells_checker ... ok
test icon_theme::tests::pixmaps_are_downscaled_keeping_aspect_but_never_upscaled ... ok
test icon_theme::tests::real_hicolor_matches_real_qt_on_this_machine ... ok
test icon_theme::tests::smooth_scale_down_golden_against_the_reference_validated_port ... ok
test icon_theme::tests::smooth_scale_down_matches_qt_area_averaging ... ok
test icon_theme::tests::system_theme_is_qt_generic_fallback ... ok
test icon_theme::tests::theme_chain_dash_and_unthemed_fallbacks ... ok
test qt_svg::tests::dump ... ignored
test qt_svg::tests::gradients_follow_qt_tables_and_spread ... ok
test qt_svg::tests::opacity_multiplies_like_qtsvg_instead_of_compositing_a_layer ... ok
test qt_svg::tests::solid_square_is_opaque_and_edges_follow_qt_coverage ... ok
test controls::tests::header_pill_first_theme_snaps_then_theme_changes_animate ... ok
test viz::tests::calendar_layout_rules ... ok
test controls::tests::pill_behaviors_animate_like_qml ... ok
test controls::kit::tests::measured_qt_metrics ... ok
test controls::kit::tests::cjk_fallback_is_one_em ... ok
test viz::tests::corners_match_qml ... ok
test controls::tests::switch_matches_qml_bluetooth_header_crop ... ok
test viz::tests::progress_ring_matches_island ... ok
test viz::tests::spectrum_tick_contract ... ok
test controls::tests::media_buttons_match_qml_card_crop ... ok
test viz::tests::track_seek_cancels_when_length_drops_to_zero ... ok
test viz::tests::track_seek_contract ... ok
test viz::tests::trend_chart_autoscale ... ok
test controls::shell_ref_tests::tray_fallback_letter_matches_bar ... FAILED
test controls::tests::text_modules_match_qml ... ok
test viz::tests::island_pill_buttons_match_island ... ok
test controls::tests::spec_only_snapshots_render ... ok
test viz::tests::circle_buttons_match_island ... ok
test viz::tests::spectrum_matches_qml ... ok
test viz::tests::progress_track_matches_island ... ok
test controls::shell_ref_tests::notification_glyphs_match_bar ... ok
test controls::tests::action_chips_match_qml ... ok
test controls::tests::header_buttons_match_qml ... ok
test controls::tests::toggle_tiles_match_qml ... ok
test controls::tests::split_tiles_match_qml ... ok
test controls::tests::sliders_match_qml ... ok
test controls::tests::dialog_buttons_match_qml ... ok
test viz::tests::calendar_body_matches_qml ... ok
test viz::tests::trend_charts_match_qml ... ok
test controls::shell_ref_tests::quick_adjust_slider_matches_shell ... ok
test controls::shell_ref_tests::segmented_row_matches_mihomo_page ... ok
test controls::shell_ref_tests::dialog_buttons_match_startup_confirm ... ok

failures:

---- controls::shell_ref_tests::tray_fallback_letter_matches_bar stdout ----

thread 'controls::shell_ref_tests::tray_fallback_letter_matches_bar' (3078586) panicked at crates/hyprv-widgets/src/controls/testutil.rs:130:5:
4 parity failure(s):
shell:bar/tray-fallback-letter@(3006.0,10.0)--light@1: mean 7.578 (≤4.5) max 140 bad 0.1009 (≤0.045) — see /tmp/hyprv-ci-sys-tmp/hyprv-widgets-test
shell:bar/tray-fallback-letter@(3005.5,10.0)--light@2: mean 7.583 (≤4.5) max 145 bad 0.0691 (≤0.045) — see /tmp/hyprv-ci-sys-tmp/hyprv-widgets-test
shell:bar/tray-fallback-letter@(3006.0,10.0)--dark@1: mean 9.937 (≤4.5) max 200 bad 0.1067 (≤0.045) — see /tmp/hyprv-ci-sys-tmp/hyprv-widgets-test
shell:bar/tray-fallback-letter@(3005.5,10.0)--dark@2: mean 9.967 (≤4.5) max 207 bad 0.0746 (≤0.045) — see /tmp/hyprv-ci-sys-tmp/hyprv-widgets-test
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    controls::shell_ref_tests::tray_fallback_letter_matches_bar

test result: FAILED. 63 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out; finished in 4.02s

error: test failed, to rerun pass `-p hyprv-widgets --lib`
```

Logs relative to `docs/verify/raw/ci-snapshots/`. No causal commit inferred.


### Snapshot CI — 2026-10-23T17:17:05+00:00 — workspace tests

REPRODUCIBLE: failed twice on same snapshot. Snapshot `/tmp/hci-sys/round-12-20261023T170715Z/rshell`; fingerprint `9e150d7738ec99254489af59391635c6f0ada416051ceb76c6a651c3a68ef33e`. Concurrent live edits cannot change this source copy.

Command: `['cargo', 'test', '--locked', '--workspace', '--all-targets', '--message-format=json']`. Attempts: `[{'code': 101, 'log': 'round-12-20261023T170715Z/03-workspace-tests-1.log'}, {'code': 101, 'log': 'round-12-20261023T170715Z/03-workspace-tests-2.log'}]`. Locations: crates/hyprv-widgets/src/controls/testutil.rs:130.

`crates/hyprv-widgets/src/controls/testutil.rs`: `e078519f6823b278315730f7c3af6fee7d8d37c9973c541a0e8e9cad9111ba65`

```text
/export_fixtures.rs (/home/wilf/.config/HyprV/rshell/target/debug/examples/export_fixtures-39566c65114c755c)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests examples/shots.rs (/home/wilf/.config/HyprV/rshell/target/debug/examples/shots-363af66346b3997b)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/lib.rs (/home/wilf/.config/HyprV/rshell/target/debug/deps/hyprv_widgets-d15fc92f353d3ed8)

running 65 tests
test controls::kit::tests::hcenter_matches_qt_for_fractional_sizes ... ok
test controls::kit::tests::lighter_on_white_is_noop_and_darker_matches_qt ... ok
test controls::tests::disabled_drag_finishes_once_and_cannot_resume ... ok
test controls::tests::disabled_shared_controls_forget_the_previous_press ... ok
test controls::tests::disabled_binding_cancels_even_if_no_pointer_event_arrives_while_disabled ... ok
test controls::tests::slider_pointer_semantics ... ok
test controls::tests::switch_initial_binding_snaps_then_real_changes_animate ... ok
test controls::tests::text_module_and_split_tile_forget_press_when_disabled ... ok
test controls::tests::text_module_signals ... ok
test controls::tests::switch_knob_animates_160ms_inoutquad ... ok
test controls::tests::tray_button_actions ... ok
test flickable::tests::cancel_and_shrink_leave_no_wakeups ... ok
test flickable::tests::discrete_angles_take_priority_in_unphased_axis_frames ... ok
test flickable::tests::exactly_fitting_content_does_not_steal_child_clicks ... ok
test flickable::tests::pixel_gesture_is_not_a_mouse_press ... ok
test flickable::tests::qt_612_frame_traces ... ok
test icon_theme::review_regressions::chain_work_is_capped_even_without_a_cycle ... ok
test icon_theme::review_regressions::corrupt_cache_falls_back_to_existing_icon ... ok
test icon_theme::review_regressions::cyclic_and_out_of_range_chains_are_invalid_not_empty ... ok
test icon_theme::review_regressions::file_reader_bounds_regular_files_and_never_opens_a_fifo_blocking ... ok
test icon_theme::tests::entry_choice_matches_real_qt_6_12_on_a_synthetic_theme ... ok
test icon_theme::tests::gtk_cache_reader_matches_qt_on_the_system_cache ... ok
test icon_theme::tests::missing_icon_is_quickshells_checker ... ok
test icon_theme::tests::pixmaps_are_downscaled_keeping_aspect_but_never_upscaled ... ok
test icon_theme::tests::real_hicolor_matches_real_qt_on_this_machine ... ok
test icon_theme::tests::smooth_scale_down_golden_against_the_reference_validated_port ... ok
test icon_theme::tests::smooth_scale_down_matches_qt_area_averaging ... ok
test icon_theme::tests::system_theme_is_qt_generic_fallback ... ok
test icon_theme::tests::theme_chain_dash_and_unthemed_fallbacks ... ok
test qt_svg::tests::dump ... ignored
test qt_svg::tests::gradients_follow_qt_tables_and_spread ... ok
test qt_svg::tests::opacity_multiplies_like_qtsvg_instead_of_compositing_a_layer ... ok
test qt_svg::tests::solid_square_is_opaque_and_edges_follow_qt_coverage ... ok
test controls::tests::header_pill_first_theme_snaps_then_theme_changes_animate ... ok
test viz::tests::calendar_layout_rules ... ok
test controls::tests::pill_behaviors_animate_like_qml ... ok
test controls::kit::tests::cjk_fallback_is_one_em ... ok
test controls::kit::tests::measured_qt_metrics ... ok
test viz::tests::corners_match_qml ... ok
test controls::tests::switch_matches_qml_bluetooth_header_crop ... ok
test viz::tests::progress_ring_matches_island ... ok
test viz::tests::spectrum_tick_contract ... ok
test viz::tests::track_seek_cancels_when_length_drops_to_zero ... ok
test viz::tests::track_seek_contract ... ok
test controls::tests::media_buttons_match_qml_card_crop ... ok
test viz::tests::trend_chart_autoscale ... ok
test viz::tests::progress_track_matches_island ... ok
test viz::tests::circle_buttons_match_island ... ok
test controls::tests::text_modules_match_qml ... ok
test viz::tests::spectrum_matches_qml ... ok
test controls::tests::spec_only_snapshots_render ... ok
test viz::tests::island_pill_buttons_match_island ... ok
test controls::shell_ref_tests::tray_fallback_letter_matches_bar ... FAILED
test controls::shell_ref_tests::notification_glyphs_match_bar ... ok
test controls::tests::action_chips_match_qml ... ok
test controls::tests::header_buttons_match_qml ... ok
test controls::tests::toggle_tiles_match_qml ... ok
test controls::tests::split_tiles_match_qml ... ok
test controls::tests::sliders_match_qml ... ok
test controls::tests::dialog_buttons_match_qml ... ok
test viz::tests::calendar_body_matches_qml ... ok
test viz::tests::trend_charts_match_qml ... ok
test controls::shell_ref_tests::quick_adjust_slider_matches_shell ... ok
test controls::shell_ref_tests::segmented_row_matches_mihomo_page ... ok
test controls::shell_ref_tests::dialog_buttons_match_startup_confirm ... ok

failures:

---- controls::shell_ref_tests::tray_fallback_letter_matches_bar stdout ----

thread 'controls::shell_ref_tests::tray_fallback_letter_matches_bar' (3214973) panicked at crates/hyprv-widgets/src/controls/testutil.rs:130:5:
4 parity failure(s):
shell:bar/tray-fallback-letter@(3006.0,10.0)--light@1: mean 7.578 (≤4.5) max 140 bad 0.1009 (≤0.045) — see /tmp/hyprv-widgets-test
shell:bar/tray-fallback-letter@(3005.5,10.0)--light@2: mean 7.583 (≤4.5) max 145 bad 0.0691 (≤0.045) — see /tmp/hyprv-widgets-test
shell:bar/tray-fallback-letter@(3006.0,10.0)--dark@1: mean 9.937 (≤4.5) max 200 bad 0.1067 (≤0.045) — see /tmp/hyprv-widgets-test
shell:bar/tray-fallback-letter@(3005.5,10.0)--dark@2: mean 9.967 (≤4.5) max 207 bad 0.0746 (≤0.045) — see /tmp/hyprv-widgets-test
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    controls::shell_ref_tests::tray_fallback_letter_matches_bar

test result: FAILED. 63 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out; finished in 3.42s

error: test failed, to rerun pass `-p hyprv-widgets --lib`
```

Logs relative to `docs/verify/raw/ci-snapshots/`. No causal commit inferred.


### Snapshot CI — 2026-10-23T17:25:48+00:00 — workspace tests

REPRODUCIBLE: failed twice on same snapshot. Snapshot `/tmp/hci-sys/round-13-20261023T171724Z/rshell`; fingerprint `c801f745a14519e122f90bfb6508b31e3119df78f689722a493685325b1809ce`. Concurrent live edits cannot change this source copy.

Command: `['cargo', 'test', '--locked', '--workspace', '--all-targets', '--message-format=json']`. Attempts: `[{'code': 101, 'log': 'round-13-20261023T171724Z/03-workspace-tests-1.log'}, {'code': 101, 'log': 'round-13-20261023T171724Z/03-workspace-tests-2.log'}]`. Locations: crates/hyprv-widgets/src/controls/testutil.rs:130.

`crates/hyprv-widgets/src/controls/testutil.rs`: `e078519f6823b278315730f7c3af6fee7d8d37c9973c541a0e8e9cad9111ba65`

```text
/export_fixtures.rs (/home/wilf/.config/HyprV/rshell/target/debug/examples/export_fixtures-39566c65114c755c)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests examples/shots.rs (/home/wilf/.config/HyprV/rshell/target/debug/examples/shots-363af66346b3997b)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/lib.rs (/home/wilf/.config/HyprV/rshell/target/debug/deps/hyprv_widgets-d15fc92f353d3ed8)

running 65 tests
test controls::kit::tests::hcenter_matches_qt_for_fractional_sizes ... ok
test controls::tests::disabled_binding_cancels_even_if_no_pointer_event_arrives_while_disabled ... ok
test controls::kit::tests::lighter_on_white_is_noop_and_darker_matches_qt ... ok
test controls::tests::disabled_shared_controls_forget_the_previous_press ... ok
test controls::tests::slider_pointer_semantics ... ok
test controls::tests::switch_knob_animates_160ms_inoutquad ... ok
test controls::tests::text_module_and_split_tile_forget_press_when_disabled ... ok
test controls::tests::disabled_drag_finishes_once_and_cannot_resume ... ok
test controls::tests::switch_initial_binding_snaps_then_real_changes_animate ... ok
test controls::tests::text_module_signals ... ok
test controls::tests::tray_button_actions ... ok
test flickable::tests::cancel_and_shrink_leave_no_wakeups ... ok
test flickable::tests::discrete_angles_take_priority_in_unphased_axis_frames ... ok
test flickable::tests::exactly_fitting_content_does_not_steal_child_clicks ... ok
test flickable::tests::pixel_gesture_is_not_a_mouse_press ... ok
test flickable::tests::qt_612_frame_traces ... ok
test icon_theme::review_regressions::chain_work_is_capped_even_without_a_cycle ... ok
test icon_theme::review_regressions::corrupt_cache_falls_back_to_existing_icon ... ok
test icon_theme::review_regressions::cyclic_and_out_of_range_chains_are_invalid_not_empty ... ok
test icon_theme::review_regressions::file_reader_bounds_regular_files_and_never_opens_a_fifo_blocking ... ok
test icon_theme::tests::entry_choice_matches_real_qt_6_12_on_a_synthetic_theme ... ok
test icon_theme::tests::gtk_cache_reader_matches_qt_on_the_system_cache ... ok
test icon_theme::tests::missing_icon_is_quickshells_checker ... ok
test icon_theme::tests::pixmaps_are_downscaled_keeping_aspect_but_never_upscaled ... ok
test icon_theme::tests::real_hicolor_matches_real_qt_on_this_machine ... ok
test icon_theme::tests::smooth_scale_down_golden_against_the_reference_validated_port ... ok
test icon_theme::tests::smooth_scale_down_matches_qt_area_averaging ... ok
test icon_theme::tests::system_theme_is_qt_generic_fallback ... ok
test icon_theme::tests::theme_chain_dash_and_unthemed_fallbacks ... ok
test qt_svg::tests::dump ... ignored
test qt_svg::tests::gradients_follow_qt_tables_and_spread ... ok
test qt_svg::tests::opacity_multiplies_like_qtsvg_instead_of_compositing_a_layer ... ok
test qt_svg::tests::solid_square_is_opaque_and_edges_follow_qt_coverage ... ok
test controls::kit::tests::cjk_fallback_is_one_em ... ok
test viz::tests::calendar_layout_rules ... ok
test controls::tests::pill_behaviors_animate_like_qml ... ok
test controls::tests::header_pill_first_theme_snaps_then_theme_changes_animate ... ok
test controls::kit::tests::measured_qt_metrics ... ok
test viz::tests::corners_match_qml ... ok
test controls::tests::switch_matches_qml_bluetooth_header_crop ... ok
test viz::tests::progress_ring_matches_island ... ok
test viz::tests::spectrum_tick_contract ... ok
test viz::tests::track_seek_cancels_when_length_drops_to_zero ... ok
test viz::tests::track_seek_contract ... ok
test viz::tests::trend_chart_autoscale ... ok
test viz::tests::progress_track_matches_island ... ok
test viz::tests::spectrum_matches_qml ... ok
test controls::shell_ref_tests::tray_fallback_letter_matches_bar ... FAILED
test controls::tests::media_buttons_match_qml_card_crop ... ok
test viz::tests::circle_buttons_match_island ... ok
test controls::shell_ref_tests::notification_glyphs_match_bar ... ok
test controls::tests::text_modules_match_qml ... ok
test controls::tests::spec_only_snapshots_render ... ok
test viz::tests::island_pill_buttons_match_island ... ok
test controls::tests::header_buttons_match_qml ... ok
test controls::tests::action_chips_match_qml ... ok
test controls::tests::toggle_tiles_match_qml ... ok
test controls::tests::sliders_match_qml ... ok
test controls::tests::split_tiles_match_qml ... ok
test controls::tests::dialog_buttons_match_qml ... ok
test viz::tests::calendar_body_matches_qml ... ok
test viz::tests::trend_charts_match_qml ... ok
test controls::shell_ref_tests::quick_adjust_slider_matches_shell ... ok
test controls::shell_ref_tests::segmented_row_matches_mihomo_page ... ok
test controls::shell_ref_tests::dialog_buttons_match_startup_confirm ... ok

failures:

---- controls::shell_ref_tests::tray_fallback_letter_matches_bar stdout ----

thread 'controls::shell_ref_tests::tray_fallback_letter_matches_bar' (3268816) panicked at crates/hyprv-widgets/src/controls/testutil.rs:130:5:
4 parity failure(s):
shell:bar/tray-fallback-letter@(3006.0,10.0)--light@1: mean 7.578 (≤4.5) max 140 bad 0.1009 (≤0.045) — see /tmp/hyprv-widgets-test
shell:bar/tray-fallback-letter@(3005.5,10.0)--light@2: mean 7.583 (≤4.5) max 145 bad 0.0691 (≤0.045) — see /tmp/hyprv-widgets-test
shell:bar/tray-fallback-letter@(3006.0,10.0)--dark@1: mean 9.937 (≤4.5) max 200 bad 0.1067 (≤0.045) — see /tmp/hyprv-widgets-test
shell:bar/tray-fallback-letter@(3005.5,10.0)--dark@2: mean 9.967 (≤4.5) max 207 bad 0.0746 (≤0.045) — see /tmp/hyprv-widgets-test
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    controls::shell_ref_tests::tray_fallback_letter_matches_bar

test result: FAILED. 63 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out; finished in 3.51s

error: test failed, to rerun pass `-p hyprv-widgets --lib`
```

Logs relative to `docs/verify/raw/ci-snapshots/`. No causal commit inferred.


### Snapshot CI — 2026-10-23T18:05:52+00:00 — workspace tests

REPRODUCIBLE: failed twice on same snapshot. Snapshot `/tmp/hci-sys/round-17-20261023T175909Z/rshell`; fingerprint `66cc9be7ef478d193a53870725750641b729e214b690776ed24f714edbcb33d2`. Concurrent live edits cannot change this source copy.

Command: `['cargo', 'test', '--locked', '--workspace', '--all-targets', '--message-format=json']`. Attempts: `[{'code': 101, 'log': 'round-17-20261023T175909Z/03-workspace-tests-1.log'}, {'code': 101, 'log': 'round-17-20261023T175909Z/03-workspace-tests-2.log'}]`. Locations: crates/hyprv-widgets/src/controls/testutil.rs:130.

`crates/hyprv-widgets/src/controls/testutil.rs`: `e078519f6823b278315730f7c3af6fee7d8d37c9973c541a0e8e9cad9111ba65`

```text
/export_fixtures.rs (/home/wilf/.config/HyprV/rshell/target/debug/examples/export_fixtures-39566c65114c755c)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests examples/shots.rs (/home/wilf/.config/HyprV/rshell/target/debug/examples/shots-363af66346b3997b)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/lib.rs (/home/wilf/.config/HyprV/rshell/target/debug/deps/hyprv_widgets-d15fc92f353d3ed8)

running 65 tests
test controls::kit::tests::lighter_on_white_is_noop_and_darker_matches_qt ... ok
test controls::tests::disabled_drag_finishes_once_and_cannot_resume ... ok
test controls::kit::tests::hcenter_matches_qt_for_fractional_sizes ... ok
test controls::tests::disabled_shared_controls_forget_the_previous_press ... ok
test controls::tests::disabled_binding_cancels_even_if_no_pointer_event_arrives_while_disabled ... ok
test controls::tests::slider_pointer_semantics ... ok
test controls::tests::switch_initial_binding_snaps_then_real_changes_animate ... ok
test controls::tests::switch_knob_animates_160ms_inoutquad ... ok
test controls::tests::text_module_and_split_tile_forget_press_when_disabled ... ok
test controls::tests::text_module_signals ... ok
test controls::tests::tray_button_actions ... ok
test flickable::tests::cancel_and_shrink_leave_no_wakeups ... ok
test flickable::tests::discrete_angles_take_priority_in_unphased_axis_frames ... ok
test flickable::tests::exactly_fitting_content_does_not_steal_child_clicks ... ok
test flickable::tests::pixel_gesture_is_not_a_mouse_press ... ok
test flickable::tests::qt_612_frame_traces ... ok
test icon_theme::review_regressions::chain_work_is_capped_even_without_a_cycle ... ok
test icon_theme::review_regressions::corrupt_cache_falls_back_to_existing_icon ... ok
test icon_theme::review_regressions::cyclic_and_out_of_range_chains_are_invalid_not_empty ... ok
test icon_theme::review_regressions::file_reader_bounds_regular_files_and_never_opens_a_fifo_blocking ... ok
test icon_theme::tests::entry_choice_matches_real_qt_6_12_on_a_synthetic_theme ... ok
test icon_theme::tests::gtk_cache_reader_matches_qt_on_the_system_cache ... ok
test icon_theme::tests::missing_icon_is_quickshells_checker ... ok
test icon_theme::tests::pixmaps_are_downscaled_keeping_aspect_but_never_upscaled ... ok
test icon_theme::tests::real_hicolor_matches_real_qt_on_this_machine ... ok
test icon_theme::tests::smooth_scale_down_golden_against_the_reference_validated_port ... ok
test icon_theme::tests::smooth_scale_down_matches_qt_area_averaging ... ok
test icon_theme::tests::system_theme_is_qt_generic_fallback ... ok
test icon_theme::tests::theme_chain_dash_and_unthemed_fallbacks ... ok
test qt_svg::tests::dump ... ignored
test qt_svg::tests::gradients_follow_qt_tables_and_spread ... ok
test qt_svg::tests::opacity_multiplies_like_qtsvg_instead_of_compositing_a_layer ... ok
test qt_svg::tests::solid_square_is_opaque_and_edges_follow_qt_coverage ... ok
test controls::tests::pill_behaviors_animate_like_qml ... ok
test viz::tests::calendar_layout_rules ... ok
test controls::kit::tests::cjk_fallback_is_one_em ... ok
test controls::tests::header_pill_first_theme_snaps_then_theme_changes_animate ... ok
test controls::kit::tests::measured_qt_metrics ... ok
test viz::tests::corners_match_qml ... ok
test controls::tests::switch_matches_qml_bluetooth_header_crop ... ok
test viz::tests::progress_ring_matches_island ... ok
test viz::tests::spectrum_tick_contract ... ok
test viz::tests::track_seek_cancels_when_length_drops_to_zero ... ok
test viz::tests::track_seek_contract ... ok
test viz::tests::trend_chart_autoscale ... ok
test viz::tests::circle_buttons_match_island ... ok
test controls::tests::media_buttons_match_qml_card_crop ... ok
test viz::tests::progress_track_matches_island ... ok
test viz::tests::spectrum_matches_qml ... ok
test controls::tests::text_modules_match_qml ... ok
test viz::tests::island_pill_buttons_match_island ... ok
test controls::tests::spec_only_snapshots_render ... ok
test controls::shell_ref_tests::tray_fallback_letter_matches_bar ... FAILED
test controls::shell_ref_tests::notification_glyphs_match_bar ... ok
test controls::tests::header_buttons_match_qml ... ok
test controls::tests::action_chips_match_qml ... ok
test controls::tests::sliders_match_qml ... ok
test controls::tests::toggle_tiles_match_qml ... ok
test controls::tests::split_tiles_match_qml ... ok
test controls::tests::dialog_buttons_match_qml ... ok
test viz::tests::calendar_body_matches_qml ... ok
test viz::tests::trend_charts_match_qml ... ok
test controls::shell_ref_tests::quick_adjust_slider_matches_shell ... ok
test controls::shell_ref_tests::segmented_row_matches_mihomo_page ... ok
test controls::shell_ref_tests::dialog_buttons_match_startup_confirm ... ok

failures:

---- controls::shell_ref_tests::tray_fallback_letter_matches_bar stdout ----

thread 'controls::shell_ref_tests::tray_fallback_letter_matches_bar' (3499334) panicked at crates/hyprv-widgets/src/controls/testutil.rs:130:5:
4 parity failure(s):
shell:bar/tray-fallback-letter@(3006.0,10.0)--light@1: mean 7.578 (≤4.5) max 140 bad 0.1009 (≤0.045) — see /tmp/hyprv-widgets-test
shell:bar/tray-fallback-letter@(3005.5,10.0)--light@2: mean 7.583 (≤4.5) max 145 bad 0.0691 (≤0.045) — see /tmp/hyprv-widgets-test
shell:bar/tray-fallback-letter@(3006.0,10.0)--dark@1: mean 9.937 (≤4.5) max 200 bad 0.1067 (≤0.045) — see /tmp/hyprv-widgets-test
shell:bar/tray-fallback-letter@(3005.5,10.0)--dark@2: mean 9.967 (≤4.5) max 207 bad 0.0746 (≤0.045) — see /tmp/hyprv-widgets-test
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    controls::shell_ref_tests::tray_fallback_letter_matches_bar

test result: FAILED. 63 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out; finished in 3.44s

error: test failed, to rerun pass `-p hyprv-widgets --lib`
```

Logs relative to `docs/verify/raw/ci-snapshots/`. No causal commit inferred.


### Snapshot CI — 2026-10-23T18:15:44+00:00 — workspace tests

REPRODUCIBLE: failed twice on same snapshot. Snapshot `/tmp/hci-sys/round-18-20261023T180909Z/rshell`; fingerprint `4ad212dcfd78336faa3dd26f041e24aadc8299fc0f31455ee3e9f2b6371c71ab`. Concurrent live edits cannot change this source copy.

Command: `['cargo', 'test', '--locked', '--workspace', '--all-targets', '--message-format=json']`. Attempts: `[{'code': 101, 'log': 'round-18-20261023T180909Z/03-workspace-tests-1.log'}, {'code': 101, 'log': 'round-18-20261023T180909Z/03-workspace-tests-2.log'}]`. Locations: crates/hyprv-widgets/src/controls/testutil.rs:130.

`crates/hyprv-widgets/src/controls/testutil.rs`: `e078519f6823b278315730f7c3af6fee7d8d37c9973c541a0e8e9cad9111ba65`

```text
/export_fixtures.rs (/home/wilf/.config/HyprV/rshell/target/debug/examples/export_fixtures-39566c65114c755c)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests examples/shots.rs (/home/wilf/.config/HyprV/rshell/target/debug/examples/shots-363af66346b3997b)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/lib.rs (/home/wilf/.config/HyprV/rshell/target/debug/deps/hyprv_widgets-d15fc92f353d3ed8)

running 65 tests
test controls::kit::tests::lighter_on_white_is_noop_and_darker_matches_qt ... ok
test controls::tests::disabled_drag_finishes_once_and_cannot_resume ... ok
test controls::tests::disabled_binding_cancels_even_if_no_pointer_event_arrives_while_disabled ... ok
test controls::kit::tests::hcenter_matches_qt_for_fractional_sizes ... ok
test controls::tests::disabled_shared_controls_forget_the_previous_press ... ok
test controls::tests::slider_pointer_semantics ... ok
test controls::tests::switch_knob_animates_160ms_inoutquad ... ok
test controls::tests::text_module_and_split_tile_forget_press_when_disabled ... ok
test controls::tests::text_module_signals ... ok
test controls::tests::switch_initial_binding_snaps_then_real_changes_animate ... ok
test controls::tests::tray_button_actions ... ok
test flickable::tests::cancel_and_shrink_leave_no_wakeups ... ok
test flickable::tests::discrete_angles_take_priority_in_unphased_axis_frames ... ok
test flickable::tests::exactly_fitting_content_does_not_steal_child_clicks ... ok
test flickable::tests::pixel_gesture_is_not_a_mouse_press ... ok
test flickable::tests::qt_612_frame_traces ... ok
test icon_theme::review_regressions::chain_work_is_capped_even_without_a_cycle ... ok
test icon_theme::review_regressions::corrupt_cache_falls_back_to_existing_icon ... ok
test icon_theme::review_regressions::cyclic_and_out_of_range_chains_are_invalid_not_empty ... ok
test icon_theme::review_regressions::file_reader_bounds_regular_files_and_never_opens_a_fifo_blocking ... ok
test icon_theme::tests::entry_choice_matches_real_qt_6_12_on_a_synthetic_theme ... ok
test icon_theme::tests::gtk_cache_reader_matches_qt_on_the_system_cache ... ok
test icon_theme::tests::missing_icon_is_quickshells_checker ... ok
test icon_theme::tests::pixmaps_are_downscaled_keeping_aspect_but_never_upscaled ... ok
test icon_theme::tests::real_hicolor_matches_real_qt_on_this_machine ... ok
test icon_theme::tests::smooth_scale_down_golden_against_the_reference_validated_port ... ok
test icon_theme::tests::smooth_scale_down_matches_qt_area_averaging ... ok
test icon_theme::tests::system_theme_is_qt_generic_fallback ... ok
test icon_theme::tests::theme_chain_dash_and_unthemed_fallbacks ... ok
test qt_svg::tests::dump ... ignored
test qt_svg::tests::gradients_follow_qt_tables_and_spread ... ok
test qt_svg::tests::opacity_multiplies_like_qtsvg_instead_of_compositing_a_layer ... ok
test qt_svg::tests::solid_square_is_opaque_and_edges_follow_qt_coverage ... ok
test controls::kit::tests::cjk_fallback_is_one_em ... ok
test viz::tests::calendar_layout_rules ... ok
test controls::tests::pill_behaviors_animate_like_qml ... ok
test controls::tests::header_pill_first_theme_snaps_then_theme_changes_animate ... ok
test controls::kit::tests::measured_qt_metrics ... ok
test viz::tests::corners_match_qml ... ok
test controls::tests::switch_matches_qml_bluetooth_header_crop ... ok
test viz::tests::progress_ring_matches_island ... ok
test viz::tests::spectrum_tick_contract ... ok
test viz::tests::track_seek_cancels_when_length_drops_to_zero ... ok
test viz::tests::track_seek_contract ... ok
test viz::tests::trend_chart_autoscale ... ok
test controls::tests::media_buttons_match_qml_card_crop ... ok
test controls::tests::text_modules_match_qml ... ok
test viz::tests::circle_buttons_match_island ... ok
test viz::tests::progress_track_matches_island ... ok
test viz::tests::island_pill_buttons_match_island ... ok
test controls::tests::spec_only_snapshots_render ... ok
test viz::tests::spectrum_matches_qml ... ok
test controls::shell_ref_tests::notification_glyphs_match_bar ... ok
test controls::shell_ref_tests::tray_fallback_letter_matches_bar ... FAILED
test controls::tests::header_buttons_match_qml ... ok
test controls::tests::action_chips_match_qml ... ok
test controls::tests::toggle_tiles_match_qml ... ok
test controls::tests::sliders_match_qml ... ok
test controls::tests::split_tiles_match_qml ... ok
test controls::tests::dialog_buttons_match_qml ... ok
test viz::tests::calendar_body_matches_qml ... ok
test viz::tests::trend_charts_match_qml ... ok
test controls::shell_ref_tests::quick_adjust_slider_matches_shell ... ok
test controls::shell_ref_tests::segmented_row_matches_mihomo_page ... ok
test controls::shell_ref_tests::dialog_buttons_match_startup_confirm ... ok

failures:

---- controls::shell_ref_tests::tray_fallback_letter_matches_bar stdout ----

thread 'controls::shell_ref_tests::tray_fallback_letter_matches_bar' (3557257) panicked at crates/hyprv-widgets/src/controls/testutil.rs:130:5:
4 parity failure(s):
shell:bar/tray-fallback-letter@(3006.0,10.0)--light@1: mean 7.578 (≤4.5) max 140 bad 0.1009 (≤0.045) — see /tmp/hyprv-widgets-test
shell:bar/tray-fallback-letter@(3005.5,10.0)--light@2: mean 7.583 (≤4.5) max 145 bad 0.0691 (≤0.045) — see /tmp/hyprv-widgets-test
shell:bar/tray-fallback-letter@(3006.0,10.0)--dark@1: mean 9.937 (≤4.5) max 200 bad 0.1067 (≤0.045) — see /tmp/hyprv-widgets-test
shell:bar/tray-fallback-letter@(3005.5,10.0)--dark@2: mean 9.967 (≤4.5) max 207 bad 0.0746 (≤0.045) — see /tmp/hyprv-widgets-test
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    controls::shell_ref_tests::tray_fallback_letter_matches_bar

test result: FAILED. 63 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out; finished in 3.59s

error: test failed, to rerun pass `-p hyprv-widgets --lib`
```

Logs relative to `docs/verify/raw/ci-snapshots/`. No causal commit inferred.

### From svc-desktop (2026-10-23): WIDGETS-R2-1/R2-2 regressions now compiled

`src/icon_theme_regressions.rs` was never included, so the WIDGETS-R2-1 (cache cycle / chain cap) and R2-2 tests never ran. I added `#[cfg(test)] #[path = "icon_theme_regressions.rs"] mod icon_theme_regressions;` before `mod tests` in `icon_theme.rs`, at the user's request. `cargo test -p hyprv-widgets --lib icon_theme`: 13 pass, including the 4 regressions; clippy clean.
