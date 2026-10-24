# Integration CI diagnostic failures

These checks supplement the scheduled full-workspace commands. They do not satisfy the workspace test or preflight gate. Both ran on 2026-10-23 at approximately 09:11 UTC; current-source validity must be established by a subsequent full pass or rerun.

| Crate | Diagnostic | Result | Evidence |
|---|---|---|---|
| hyprv-ui-panels | Workspace tests with only hyprv-ui-bench excluded | FAIL: `panels_match_shared_qml_references`; `qa-show-hide--show-t0060--dark@1.png` and `@2.png`, mean 0.113 | [log](raw/ci/round-06/tests-minus-bench.log), [scope/result](raw/ci/round-06/tests-minus-bench.json) |
| hyprv-widgets | Focused rerun of `controls::shell_ref_tests::dialog_buttons_match_startup_confirm` | FAIL: 14 startup-confirm/hover/press crop differences | [log](raw/ci/round-06/widgets-startup-focused.log), [scope/result](raw/ci/round-06/widgets-startup-focused.json) |

Detailed reproduction commands, primary source locations, mtimes and SHA-256 values are appended to [panels requests](../requests/hyprv-ui-panels.md) and [widgets requests](../requests/hyprv-widgets.md). The benchmark API compile failure prevents the scheduled workspace test command from reaching these runtime failures. No gate thresholds or reference images were changed by CI.

At 09:45 UTC, parity freshness was also verified to include generated artifacts: `hyprv-ui-bar/tools/__pycache__/check_rect_effect.cpython-314.pyc` and `hyprv-ui-panels/target-png/trend-ours@2.png` are newer than their render records. This is a checker defect in `cutover/preflight.py`'s `newest` calculation, independently of the strict pixel failures. It is recorded with timestamps/digests in [shell/cutover requests](../requests/hyprv-shell.md); the checker and thresholds were left unchanged.
