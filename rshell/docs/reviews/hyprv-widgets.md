# Adversarial review: hyprv-widgets

Reviewed 2026-10-23. Review-only: production crates and real user configuration were not edited. [Scratch evidence](scratch/round3/README.md), [source fingerprints](scratch/round3/source-hashes.json), [results](scratch/round3/results.json), and [hyprv-widgets probe log](scratch/round3/hyprv-widgets.log) identify the tested source and expected failures. Shared source changed during review; affected snapshots were refreshed. A panel-borrow panic fixed concurrently was rechecked and excluded.

P1 denotes a high-priority data-loss, unintended-execution, configuration-availability or idle-loop defect; P2 denotes correctness, ownership, privacy or parity failure. These are deliberately failing regression probes, not failures attributed to the original normal unit suite. No real settings/hooks, desktop theme, service, GPU or compositor was changed.

<a id="widgets-1"></a>
## WIDGETS-1 [P2] Disabling a slider during a drag leaves an active gesture after button release

**Location:** `crates/hyprv-widgets/src/controls/slider_tile.rs:132` and `crates/hyprv-widgets/src/controls/slider_tile.rs:147`.

**Failure scenario:** An active slider drag sets Interaction.pressed and adjusting. If availability disables the widget before release, pointer returns immediately and ignores that release or even Cancel. Re-enabling leaves the old pressed/adjusting state intact, so ordinary button-up motion emits another value change. A disconnected/reconnected fan or a temporarily disabled control can thus receive unsolicited changes, and a gesture may never finish cleanly. Several other controls use the same early-return pattern.

**Verification:** review_widgets_disabled_drag_cannot_resume_with_no_button uses the public SliderTile API: press, disable, release, enable, move. With no button down, the final move emits ValueChangeRequested(82.0) and adjusting stays true. This is the actual widget implementation, with no hardware/provider action dispatched. The source QML already handles cancellation at quickshell/ControlPanelSlider.qml:160–164.

**Suggested fix:** Cancel/reconcile interaction state on disable and always process grab cancellation/release cleanup even when action generation is disabled. Ensure adjustmentFinished occurs once under the intended cancellation contract and no new value changes occur until a new accepted press. Test service availability changes mid-gesture across the shared controls.

<a id="widgets-2"></a>
## WIDGETS-2 [P2] First dark-theme switch mount animates from hardcoded light colors

**Location:** `crates/hyprv-widgets/src/controls/switch.rs:39`; animation first-assignment logic in `crates/hyprv-widgets/src/controls/kit.rs:64`.

**Failure scenario:** Switch::new initializes its color behaviors by updating with a light Tok. The first real dark-theme update is consequently treated as a later target change, starting 140 ms color animations from the light palette. A dark shell mounting the switch gets wrong initial colors and unnecessary animation frames rather than the QML-style initial binding value. This is a first-mount lifecycle/pixel-parity defect, not a claim about the complete rendered panel's screenshot score.

**Verification:** review_widgets_initial_dark_switch_does_not_animate_from_light confirms a newly constructed switch is initially idle, then its first update with Tok::new(true) immediately makes animating(Duration::ZERO) true. No service, live surface or external display is involved. kit.rs itself promises snapping the first binding assignment and distinguishes it from subsequent retargeting.

**Suggested fix:** Defer behavior initialization until the first actual theme/binding update, or accept the initial Tok/theme in the constructor. Seed initial values without transitions; continue animating later real theme/state changes. Add first-light/first-dark and subsequent-toggle cases.

