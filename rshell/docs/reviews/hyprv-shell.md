# Adversarial review: hyprv-shell

Reviewed 2026-10-23. Review-only: production crates and real user configuration were not edited. [Scratch evidence](scratch/round3/README.md), [source fingerprints](scratch/round3/source-hashes.json), [results](scratch/round3/results.json), and [hyprv-shell probe log](scratch/round3/hyprv-shell.log) identify the tested source and expected failures. Shared source changed during review; affected snapshots were refreshed. A panel-borrow panic fixed concurrently was rechecked and excluded.

P1 denotes a high-priority data-loss, unintended-execution, configuration-availability or idle-loop defect; P2 denotes correctness, ownership, privacy or parity failure. These are deliberately failing regression probes, not failures attributed to the original normal unit suite. No real settings/hooks, desktop theme, service, GPU or compositor was changed.

Host evidence uses exact source-extracted cleanup/IPC expressions with lightweight stand-ins for native handles and recording provider actions. The hotplug probe uses the real panel controller. It does not launch the full Hub, connect to Wayland or test physical hotplug; those runtime checks remain pending. Assembly is the production path; the compatibility Host is not used to infer production behavior.

<a id="shell-1"></a>
## SHELL-1 [P1] Output removal retains orphan panels/islands and expired deadlines

**Location:** `crates/hyprv-shell/src/assembly.rs:203`; deadline collection at `crates/hyprv-shell/src/assembly.rs:200` and panel ticking at `crates/hyprv-shell/src/assembly.rs:152`.

**Failure scenario:** The callback removes outputs and bars but leaves per-output panels and islands. sync only ticks panels for current output names, while next_deadline and visibility decisions still enumerate all retained panel/island objects. Unplug an output just after opening its panel: the open timer remains 16 ms, is never consumed by tick, and keeps being returned after it is already past. The engine schedules expired deadlines with saturating_sub(now), yielding repeated zero-delay updates. Retained mapped stats/panel objects can also keep demand flags alive after their display disappears.

**Verification:** review_shell_output_removal_releases_children executes the exact extracted production callback. WlOutput and unused bar/island payloads are stand-ins; the panel is the real hyprv-ui-panels::Panels. The control output/bar entries disappear, but its panel/island entries remain and the real panel reports Some(16ms) even when queried at one second. Engine timer scheduling and output-only tick enumeration were traced in source. This proves orphan ownership/deadline behavior without claiming a measured live Wayland CPU percentage.

**Suggested fix:** Remove/cancel every output-owned controller, popup, gesture, scene closure and timer on unplug, close its native mounts and release leases before recomputing visibility. Collect deadlines only from live output owners. Add unplug-during-open/close and last-output-removal tests, including an assertion that no expired orphan deadline remains.

<a id="shell-2"></a>
## SHELL-2 [P2] Native surface disappearance is ignored while its logical role remains desired

**Location:** `crates/hyprv-shell/src/assembly.rs:159` and `crates/hyprv-shell/src/assembly.rs:162` .

**Failure scenario:** Mount reconciliation compares roles only. If the compositor closes a layer surface independently, Engine removes its native SurfaceId but Assembly still finds that role in mounts. sync then calls configure/scene updates on the dead ID and never creates a replacement. On primary-output removal, a global BarPopup role can remain desired on the new primary while its old output-bound surface was closed, producing the same failure. The existing-ID path also ignores output binding differences; output binding requires recreating a layer surface, not resizing it.

**Verification:** review_shell_dead_surface_is_reconciled executes the production stale-selection expression extracted verbatim, with a desired role whose old ID is absent from the supplied live set. It returns no stale mounts. Source inspection confirms the existing-role path never checks engine.surfaces/scene availability or config.output. Engine::output_destroyed/closed remove native surfaces independently. No real compositor was asked to close a surface.

**Suggested fix:** Reconcile against native live IDs and desired output bindings as well as roles. Remove dead mounts and recreate required surfaces; recreate on an output change and choose whether a compositor-closed popup should also close its logical owner. Include compositor-close and primary-migration cases in host lifecycle tests.

<a id="shell-3"></a>
## SHELL-3 [P2] Brightness preview IPC performs hardware writes and persistence

**Location:** `crates/hyprv-shell/src/assembly.rs:109` and `crates/hyprv-shell/src/services.rs:152` .

**Failure scenario:** showBrightnessLevel and showBrightnessIncrease/Decrease are documented as OSD/local-preview notifications. Assembly instead dispatches BrightnessFinish, which Hub translates into native backlight Set followed by Finish, hardware confirmation and persistence. Existing controls.lua changes brightness first and then sends showBrightnessLevel; this now creates a second competing hardware controller. A preview-only increase/decrease can also change the real level again. It violates the IPC contract even though the socket caller has the same UID.

**Verification:** review_shell_preview_ipc_does_not_mutate_hardware replays the exact extracted quickAdjust dispatch arm with a recording Hub and no devices. Level 25 and the delta calls emit BrightnessFinish(25), BrightnessFinish(52), BrightnessFinish(48). Hub's native action translation was inspected. docs/spec/ui-panels.md:299–300 specifies local preview; quickshell/shell.qml:2839–2845,3432–3447 and hypr/controls.lua:139 establish the existing notification-after-change path.

**Suggested fix:** Keep these IPC calls local: update/snapped OSD preview state, show the surface and request one read-only reconciliation. Put hardware adjustment behind a separately named explicit action and give controller ownership a clear handoff. Add IPC tests that record zero mutating provider actions for every showBrightness* notification.

