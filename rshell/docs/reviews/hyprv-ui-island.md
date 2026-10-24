# Adversarial review: hyprv-ui-island

Reviewed 2026-10-23. Review-only: production crates, real user configuration, services and the compositor were not changed. Each finding below has a deliberately failing probe in an isolated source copy; these failures are additional adversarial cases, not failures of the existing normal suite. See [reproduction and boundaries](scratch/round4/README.md), [source fingerprints](scratch/round4/source-hashes.json), and [results](scratch/round4/results.json).

The seven finding probes fail as expected. All 23 existing behavior tests pass, and a positive control confirms the paused-spectrum deadline stops when the island is hidden. A small Qt 6.12 offscreen delivery probe distinguishes disable from hide cancellation. Relevant finding-source fingerprints were checked before and after execution. Concurrent renderer changes outside those finding paths are listed separately in results; dependency copies remain frozen. No full live Wayland, physical input, CPU-budget or screenshot-parity result is claimed.

Coverage includes compact/expanded hover targeting, button/row action identity, seek/swipe/list gestures, service-to-model adapters, animation completion and spectrum timer visibility. The named measured QML swipe/hover quirks were preserved. No additional model-field mapping defect was established.

<a id="island-1"></a>
## ISLAND-1 [P2] An agent reorder retargets an in-flight click to another session

**Location:** `crates/hyprv-ui-island/src/island.rs:144` and `crates/hyprv-ui-island/src/island.rs:917`.

**Failure scenario:** Press the first session row while session A is primary. Before release, a status/activity update reorders the sessions so B becomes first. The controller stores only `Area::Row(0)` and resolves the session ID from the new vector on release. Releasing at the same screen position now focuses B, although the press belonged to A. Session removal can similarly substitute a different row. This is especially relevant because sessions are deliberately sorted by changing status/activity.

**Verification:** `review_island_reordered_row_never_focuses_another_session` expands two same-sized rows, presses session0, swaps the input vector, and releases without pointer movement. The action is `FocusAgent("session1")`. The test forbids focusing the substituted session; it allows an implementation either to preserve the original session grab or to cancel it.

**Contract and suggested fix:** `docs/spec/ui-island.md` §5 requires stable row identity keyed by ID, and §3.14 routes `row.id` (`quickshell/AgentTaskModel.qml:1-22`, `quickshell/DynamicIsland.qml:1402-1405`). Bind the pressed row to its stable session ID and validate release against that same logical delegate. Preserve/cancel that identity when rows move, and cancel when the session disappears; never look up a pressed row solely by its current numeric index.

<a id="island-2"></a>
## ISLAND-2 [P2] Overflowing agent lists cannot be dragged or flicked

**Location:** `crates/hyprv-ui-island/src/island.rs:768` and `crates/hyprv-ui-island/src/island.rs:870`.

**Failure scenario:** With many sessions, the list is taller than its viewport. A vertical pointer drag over the list does not scroll at all. The shared Drag state handles only horizontal music swipes, and the sole agent-list offset mutation is the wheel branch. The required ListView drag/flick gesture is absent.

**Verification:** `review_island_agent_list_drag_scrolls_and_cancels_click` uses 15 sessions, expands the agent view, asserts a positive scroll range, then drags upward 100 px within the list. Actual private controller state remains `scroll_y=0`; no action is dispatched. Access to private state is through a test module appended only to the copied source.

**Contract and suggested fix:** `docs/spec/ui-island.md` §3.14 explicitly requires wheel and drag-flick scrolling, stop-at-bounds, deceleration and maximum velocity (`quickshell/DynamicIsland.qml:1318-1330`). Implement a list-owned vertical grab that cancels row activation once it crosses threshold, clamps to bounds and decelerates after release. Stop all motion/deadlines on rest, hide and data shrink. Preserve scroll offset across stable-ID model updates.

<a id="island-3"></a>
## ISLAND-3 [P2] Hiding the island during a seek leaves a permanent grab and preview

**Location:** `crates/hyprv-ui-island/src/island.rs:301` and `crates/hyprv-ui-island/src/island.rs:795`; cancellation helper at `crates/hyprv-ui-island/src/island.rs:880`.

**Failure scenario:** Begin seeking, then open the calendar so the island becomes fully hidden before the mouse is released. The input snapshot clears expansion but does not cancel the grab, button-down state or seeking state; Leave merely clears pointer position. If release is routed to the newly visible calendar/native owner, the island never receives it. Its 700 ms preview-reset deadline was canceled on press and is never restarted, so the stale seeking/grab state persists indefinitely. After restoring the island, button-down hover suppression can also block ordinary expansion until another press/release cycle.

**Verification:** `review_island_hidden_seek_releases_grab_and_preview` starts a real seek, sets `calendar_open=true` and `calendar_progress=1`, delivers Leave, then advances beyond 700 ms. The state is still `seeking=true, buttons_down=true, grab=Some(Seek), deadline=None`. In the independent local Qt 6.12 offscreen baseline, hiding the MouseArea's parent during a press emits one cancellation and zero releases. Merely disabling MouseArea is different and was deliberately excluded as a parity finding.

**Contract and suggested fix:** The host hides the island at opacity ≤0.01 (`docs/spec/ui-island.md` §1/§7; `quickshell/shell.qml:3912-3932`), and seek cancellation restarts its 700 ms reset (`quickshell/DynamicIsland.qml:1097,1437-1442`). Add an explicit Cancel/ungrab API and reconcile effective visibility/owner changes in `set_inputs`; clear button/grab/drag state and restart or safely clear the preview once. Wire native grab loss to cancellation. Ordinary Leave during a legitimate outside drag must remain distinct from cancellation.
