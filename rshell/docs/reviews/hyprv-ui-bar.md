# Adversarial review: hyprv-ui-bar

Reviewed 2026-10-23. Review-only: production crates, real user configuration, services and the compositor were not changed. Each finding below has a deliberately failing probe in an isolated source copy; these failures are additional adversarial cases, not failures of the existing normal suite. See [reproduction and boundaries](scratch/round4/README.md), [source fingerprints](scratch/round4/source-hashes.json), and [results](scratch/round4/results.json).

The seven finding probes fail as expected. All 23 existing behavior tests pass, and a positive control confirms the paused-spectrum deadline stops when the island is hidden. A small Qt 6.12 offscreen delivery probe distinguishes disable from hide cancellation. Relevant finding-source fingerprints were checked before and after execution. Concurrent renderer changes outside those finding paths are listed separately in results; dependency copies remain frozen. No full live Wayland, physical input, CPU-budget or screenshot-parity result is claimed.

Coverage includes bar hover/press/wheel routing, service-backed module fields, tray menu/overflow/battery input, animation staging/cleanup and one-shot deadlines. The inherited inverted volume wheel and horizontal tray-wheel quirks were preserved. A suspected calendar wheel-blocking defect was excluded after checking Qt wheel acceptance.

<a id="bar-1"></a>
## BAR-1 [P2] An empty or vanished tray menu polls invisibly every 16 ms

**Location:** `crates/hyprv-ui-bar/src/popups.rs:67` (`SurfaceState::advance`); readiness/deadline handling at `crates/hyprv-ui-bar/src/popups.rs:341` and `crates/hyprv-ui-bar/src/popups.rs:365`.

**Failure scenario:** Open an item whose DBusMenu has no visible entries, whose hydration fails, or whose service disappears before its layout arrives. The menu remains mapped with zero reveal height and no running animation, but every expired open deadline is replaced by another deadline 16 ms later. There is no content-change subscription wait, bounded retry or absent-item cleanup in this controller. The host continues waking indefinitely despite no visible animation or changing UI.

**Verification:** `review_bar_empty_menu_waits_without_polling` calls the real controller 100 times at 16 ms intervals with an empty menu. At 1600 ms, `animating()` is false but `next_deadline()` is still `Some(1.616s)`. This is scheduling evidence, not a measured live CPU percentage.

**Contract and suggested fix:** `docs/spec/ui-bar-popups.md` §3.3 records the original QML retry (`quickshell/shell.qml:772-789`); this is an inherited polling pattern that conflicts with the Rust architecture's event-driven/zero-idle-wakeup requirement, rather than an undocumented QML mismatch. Start the open animation when a menu snapshot/layout revision arrives. After one bounded failure timeout, close/show an error or remain subscribed without a polling deadline; cancel pending work when the item disappears. Preserve the first 16 ms open staging turn if necessary for parity, without repeating it while data are absent.

<a id="bar-2"></a>
## BAR-2 [P2] Tray menu hit testing activates rows before they are revealed

**Location:** `crates/hyprv-ui-bar/src/popups.rs:774` and `crates/hyprv-ui-bar/src/popups.rs:810`.

**Failure scenario:** After opening a populated menu, the reveal height is zero until the open staging deadline fires. The full-output popup already receives input. Hit testing nevertheless uses the final full panel viewport, so a fast press/release in a future row can dispatch its tray action while that row is completely clipped away. The same discrepancy affects unrevealed portions during opening and closing: input does not follow the painted clip.

**Verification:** `review_bar_unrevealed_rows_cannot_activate` asserts actual reveal height zero, presses/releases the first row at 1/2 ms after open, and receives `Tray(MenuEvent { item: "app", entry: 1, event: "clicked" })`. No D-Bus method is called; the test records the actual UI action.

**Contract and suggested fix:** `docs/spec/ui-bar-popups.md` §2.2 requires the reveal viewport to clip content; §3.2/3.3 makes entry MouseAreas the clickable content and the remaining surface a close backdrop (`quickshell/AnimatedGlassPanel.qml:158-219`, `quickshell/shell.qml:992-997`). Intersect hit regions with the current reveal clip and native mapped/visible state. Reject/cancel row grabs when a transition clips them out, while retaining outside-click dismissal. Add tests for zero-height, partially revealed, closing and unmapped menus.

<a id="bar-3"></a>
## BAR-3 [P2] Closing and reopening overflow retains an old item press

**Location:** `crates/hyprv-ui-bar/src/popups.rs:914` (overflow release), `crates/hyprv-ui-bar/src/popups.rs:209` (reopen), and the outside-close branch at `crates/hyprv-ui-bar/src/popups.rs:953`.

**Failure scenario:** Press an overflow item, then release outside its grid. The popup closes, but its stored `(item, button)` press survives because `pressed.take()` is only reached for a release that hits an item. Reopening does not reset this state. If a release reaches that item in the next mapping without a new accepted item press, it activates using the stale previous-map press. This is a controller remapping/grab-boundary defect; it does not mean every ordinary click duplicates an action.

**Verification:** `review_bar_overflow_close_cancels_old_press` performs precisely that sequence with a 30-item model. It confirms the popup unmapped, reopens it, supplies a release without a fresh press, and observes `Tray(Activate { item: "app3", x: 540, y: 104 })`. The payload is recorded only; no application is activated. The Qt offscreen baseline separately confirms that hiding an item cancels its active MouseArea press.

**Contract and suggested fix:** `docs/spec/ui-bar-layout.md` §1.6 requires a matching press/release; `docs/spec/ui-bar-popups.md` §4 requires unmap cleanup (`quickshell/shell.qml:1142-1308`). Clear pressed/hovered state on every close/unmap, escape, toggle-off, list-empty transition and fresh open. Consume/take the stored press on every release, including outside releases. Consider a mapping generation token so a grab can never cross a remap.

<a id="bar-4"></a>
## BAR-4 [P2] Long tray menus omit drag and flick scrolling

**Location:** `crates/hyprv-ui-bar/src/popups.rs:792` (menu motion) and `crates/hyprv-ui-bar/src/popups.rs:863` (the implemented scroll path).

**Failure scenario:** A menu with more than 420 px of content cannot be dragged upward to reach later entries. Motion only changes row hover; it never updates scroll position or transfers the row grab to a vertical scrolling gesture. Only wheel events move the content. Users relying on pointer drag/touch-style interaction lose the QML Flickable behavior.

**Verification:** `review_bar_long_menu_supports_drag_scroll` opens and fully reveals 30 entries, presses inside the viewport, drags upward 100 px, then releases. `menu.scroll` remains exactly zero. The setup uses a scrollable menu and keeps the gesture inside the viewport.

**Contract and suggested fix:** `docs/spec/ui-bar-popups.md` §3.2 specifies a vertical Flickable, matching `quickshell/shell.qml:992-997`. Implement a vertical drag recognizer with threshold, bounds and row-click cancellation, then a finite decelerating flick if released with velocity. Request frames/deadlines only while motion is active; clear them at bounds, rest and unmap. Keep wheel scrolling as a separate supported path.
