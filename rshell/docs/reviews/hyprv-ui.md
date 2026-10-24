# Adversarial review: hyprv-ui

Reviewed 2026-10-23. Findings below were reproduced against source copies under `docs/reviews/scratch/`; production crates were not edited. Source fingerprints are in [source-hashes.json](scratch/source-hashes.json), and exact failing test names/results are in [results.json](scratch/results.json). UI source changed concurrently during the review; the scratch UI snapshot was refreshed and all UI probes rerun against the refreshed implementation, including the new text module.

`P1` means a high-priority lifecycle/deadlock or lost-update defect. `P2` means a correctness, resource-lifetime, or unnecessary-wakeup defect. These are deliberately failing regression probes, not a claim that the normal crate test suite failed. No live D-Bus mutations, compositor/shell restarts, or live Wayland surfaces were used. Run `python3 docs/reviews/scratch/verify.py` from rshell root to reproduce the audited snapshot; the runner checks source fingerprints and expects the listed assertion failures.

<a id="ui-1"></a>
## UI-1 [P2] Damage bounds omit path stroke extents and leave permanent pixels

**Location:** `crates/hyprv-ui/src/scene.rs:223` (`Node::bounds`, frame bounds plus effects/AA only); actual stroked drawing at `crates/hyprv-ui/src/render.rs:273`.

**Failure scenario:** A path uses a 10×10 node frame and a 20px stroke. The first full render correctly draws outside the frame. Hiding, moving or removing that node only invalidates its frame plus the small AA/effect outset; much of its old stroke is never cleared from the retained target. The resulting pixels remain until an unrelated full redraw. The same issue applies to stroked paths extending outside the declared frame and to stroke joins/caps.

**Verification:** `review_ui_damage_covers_wide_stroke` compares a partial repaint after hiding the path against a fresh full CPU render. It finds **704 differing pixels**, with damage only `Rect { x:18, y:18, width:14, height:14 }`. See [recorded test output](scratch/logs/hyprv-ui.log). This is a deterministic raster correctness failure, not a live QML screenshot parity claim.

**Suggested fix:** Compute actual local paint bounds for each content variant, including path geometry, stroke width, caps/miter joins and AA, before transforming/unifying descendants and effect outsets. Use Skia's conservative stroke/path bounds where possible. Test hide/move/remove and compare incremental output to a full render. This violates the retained damage-correctness contract needed for the architecture's pixel parity.

<a id="ui-2"></a>
## UI-2 [P2] A permitted node-edit closure can change identity and panic the renderer

**Location:** `crates/hyprv-ui/src/scene.rs:297` followed by `crates/hyprv-ui/src/scene.rs:300`.

**Failure scenario:** `Scene::edit(id, |node| *node = Node::new(...))` is accepted by the public `FnOnce(&mut Node)` interface. The replacement has a new private ID, so the post-edit lookup using the original ID is `None` and unconditionally unwraps. Rebuilding a retained subtree through the general editor therefore crashes instead of returning a safe result. The API says edits use stable IDs but neither restricts whole-node replacement nor documents this panic precondition.

**Verification:** `review_ui_edit_can_replace_a_node_without_panicking` performs that replacement through the public API and panics in `damage_bounds(id).unwrap()`. See [recorded test output](scratch/logs/hyprv-ui.log).

**Suggested fix:** Preserve the target node ID across the edit, or expose a restricted editor/replacement operation that maintains identity and damages the old and new bounds explicitly. If replacement is forbidden, enforce the invariant with an actionable API contract rather than a post-mutation unwrap; an error must not leave the scene partially invalid.

<a id="ui-3"></a>
## UI-3 [P2] Dropped effects leave weak subscriptions and allocations pinned indefinitely

**Location:** `crates/hyprv-ui/src/reactive.rs:23` and `crates/hyprv-ui/src/reactive.rs:32`; `EffectInner` has no final-drop unsubscription path.

**Failure scenario:** A panel/row repeatedly creates and drops a binding reading a long-lived property such as theme mode. Each binding inserts a Weak entry in that property's listener vector. Dropping the final Effect handle drops the strong owner but leaves that Weak reference, its vector slot and the Rc allocation's control block. Cleanup happens only when the source property next changes. An unchanged global property can therefore retain an unbounded number of dead subscribers throughout a session.

**Verification:** `review_ui_dropped_effect_removes_subscription_storage` creates/drops 1,000 effects while retaining one unchanged source property. Listener storage remains **1,000**, expected zero; see [recorded test output](scratch/logs/hyprv-ui.log). No callbacks/timers are needed to demonstrate the leak.

**Suggested fix:** Implement final-drop cleanup that removes the effect's Weak identity from every recorded dependency, or use disposable subscription tokens/slab entries with deterministic removal. Keep dependency recollection behavior for live effects. This contradicts the engine README's RAII disconnect/lifetime claim and matters for repeatedly mounted shell components.

<a id="ui-4"></a>
## UI-4 [P2] Text drawing discards parent rotation, shear and nonuniform transforms

**Location:** `crates/hyprv-ui/src/text.rs:568` and `crates/hyprv-ui/src/text.rs:580` (`TextEngine::paint`).

**Failure scenario:** The new text painter extracts only the transformed origin and one scalar DPR from the full canvas matrix, resets that matrix, then paints glyph masks with axis-aligned coordinates. A parent/node 90° rotation therefore rotates rectangles/paths but leaves its text horizontal. Shear, reflected axes and independently scaled axes are likewise discarded. The problem affects both mask positioning paths that follow the reset, not font-rasterization tolerance.

**Verification:** `review_ui_text_respects_parent_rotation` renders `ROTATE` in a 90° rotated text node using NativeRendering. It first confirms ink exists, then finds **284 ink pixels** far to the right of the rotated bounds, where the glyphs cannot belong. See [recorded test output](scratch/logs/hyprv-ui.log). The probe was added after the text module appeared during this review, and rerun against that module's captured source.

**Suggested fix:** Separate Qt snapping/device-mask generation from final geometry composition: retain/reapply the complete item-to-device transform, or explicitly transform glyph quads/masks through it. Restrict any axis-aligned snapping optimization to matrices for which it is valid. Add rotated/nonuniformly scaled/reflected text tests for both render types. The architecture and engine scene contract require transformed subtree parity; this is independent of the text spec's acknowledged wrapping/color-emoji acceptance gaps.
