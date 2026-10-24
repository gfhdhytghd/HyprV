# Adversarial review: hyprv-svc-media

Reviewed 2026-10-23. Production crates and packaging were read-only. Findings were verified with source inspection and isolated probes under [scratch/round2](scratch/round2/README.md); [source fingerprints](scratch/round2/source-hashes.json) and [results](scratch/round2/results.json) identify the audited snapshot. Shared core/media lifecycle code changed during the review; affected copies were refreshed and probes rerun against the final fingerprinted source.

P1 means a high-priority security, cooling-safety, starvation, deadlock or resource-lifetime defect; P2 means correctness or deployment risk. Expected regression assertions deliberately fail; this is not a claim that the original unit suite failed. No root execution, live hardware writes, real-bus mutations, installs or service/compositor restarts occurred. Physical fan/thermal behavior, real polkit prompts and systemd sandbox execution remain unverified. Run `python3 docs/reviews/scratch/round2/verify.py` to reproduce.

<a id="media-1"></a>
## MEDIA-1 [P1] Cancelling spectrum lifecycle detaches its active worker and timer

**Location:** `crates/hyprv-svc-media/src/spectrum/mod.rs:43` (outer select), `crates/hyprv-svc-media/src/spectrum/mod.rs:50` and `crates/hyprv-svc-media/src/spectrum/mod.rs:60`.

**Failure scenario:** The gated-demand driver and run_sampled are sibling select branches. Closing the gate sender or triggering shutdown can finish the driver first, dropping run_sampled while Capture is active. Capture/Worker have no Drop cleanup; dropping std::thread::JoinHandle detaches it, and dropping the quit sender does not call main_loop.quit. The PipeWire receiver remains attached to its loop, retaining capture and the 24Hz timer despite cancellation. A normal stop sends Quit, but that path is skipped when the future is cancelled.

**Verification:** `review_media_capture_drop_stops_worker` constructs the exact Capture/Worker with a real PipeWire loop/channel and a harmless 5ms counter timer, without connecting to PipeWire or capturing audio. Dropping Capture leaves 11 timer callbacks in the following50ms. A separately retained sender then quits the test worker and the probe waits for its exit before asserting, so the test itself leaves no worker behind. Source tracing verifies the outer select reaches that destructor path. See [hyprv-svc-media probe log](scratch/round2/hyprv-svc-media.log).

**Suggested fix:** Give the worker cancellation-safe RAII quit signaling and explicit asynchronous join ownership. When the driver completes, close effective demand and await run_sampled shutdown rather than dropping it; handle cancellation during stop as well. Clear running state on all exits. Keep blocking joins off the service/UI threads. This violates the no-inactive-spectrum-worker/timer requirement in docs/spec/services-api.md:339.

<a id="media-2"></a>
## MEDIA-2 [P2] Late position reads can overwrite a newer Seeked or track anchor

**Location:** `crates/hyprv-svc-media/src/mpris/mod.rs:381` and `crates/hyprv-svc-media/src/mpris/mod.rs:348`.

**Failure scenario:** Position reads are spawned independently and their completions are accepted using only bus name and owner. An old Get Position result can be queued before a newer Seeked arrives, then be selected after Seeked and overwrite its anchor. A read spanning a track change has the same issue because no track/request generation is carried. Position can jump backward and extrapolate from stale state indefinitely when no subsequent event corrects it.

**Verification:** `review_media_old_position_result_cannot_undo_seeked` gives an actor a newer90-second anchor at time2000, then delivers an older1-second completion at time1000 for the same name/owner. The actor stores1second; the expected90-second assertion fails. This tests the production completion handler directly on a private connection. See [hyprv-svc-media probe log](scratch/round2/hyprv-svc-media.log).

**Suggested fix:** Carry owner generation, track identity and a monotonically increasing position-request/anchor generation through read_position. Discard stale completions after seek/track/playback rebasing; compare timestamps where appropriate, but a request started before a seek can finish later, so completion time alone is insufficient. Serialize/coalesce position reads per player.

<a id="media-3"></a>
## MEDIA-3 [P2] Out-of-order dbusmenu replies regress the displayed layout revision

**Location:** `crates/hyprv-svc-media/src/tray/mod.rs:449` and `crates/hyprv-svc-media/src/tray/mod.rs:464`.

**Failure scenario:** Every LayoutUpdated/ItemsPropertiesUpdated can spawn a separate GetLayout. Replies have no in-flight coalescing or request generation. If revision2 arrives before a slower revision1 response, on_internal unconditionally replaces the live menu with revision1. The menu can remain stale until a later signal; entry IDs/actions can refer to removed or changed items. A reply from an earlier close/open or replacement item is also accepted by key alone.

**Verification:** `review_media_layout_reply_cannot_regress_revision` creates one open menu and delivers the exact production Internal::Layout events in revision2, revision1 order. Its final revision is1; the monotonic-layout assertion fails. See [hyprv-svc-media probe log](scratch/round2/hyprv-svc-media.log).

**Suggested fix:** Coalesce layout refreshes per item and carry menu-open/item-owner generation plus request serial. Accept only the newest applicable request; respect revision ordering with the protocol's wrap semantics. Invalidate outstanding replies on item disappearance and menu close/open. Await AboutToShow completion before the dependent initial layout fetch.

<a id="media-4"></a>
## MEDIA-4 [P2] SwayNC subscription accepts signals from any session-bus peer

**Location:** `crates/hyprv-svc-media/src/notifications.rs:146` and `crates/hyprv-svc-media/src/notifications.rs:174`.

**Failure scenario:** Subscribe/SubscribeV2 match only interface/member/path, and handlers never verify sender against the current SwayNC owner. Another process on the session bus can emit that signal and replace notification count, DND, visibility and inhibited flags, even while no SwayNC owner exists. This is local UI state spoofing within the session trust boundary, not a root privilege escalation. The owner stream does not authenticate these other streams.

**Verification:** `review_media_notifications_reject_unowned_signal` starts run on private Unix-pair zbus connections and emits SubscribeV2 with count777 from a peer that never acquired SwayNC's name. The public topic becomes count777. The production match rule has no sender constraint, so a real bus-assigned unrelated unique sender is equally unfiltered; the test does not contact the live session bus. See [hyprv-svc-media probe log](scratch/round2/hyprv-svc-media.log).

**Suggested fix:** Resolve and track the authoritative unique owner, authenticate every subscription signal, and update subscriptions/sender checks atomically on NameOwnerChanged. Use an owner generation for the snapshot and buffered signals. Do not treat an interface/path convention as sender identity.

<a id="media-5"></a>
## MEDIA-5 [P2] A snapshot from a departed notification daemon resurrects stale availability

**Location:** `crates/hyprv-svc-media/src/notifications.rs:137` and `crates/hyprv-svc-media/src/notifications.rs:200`.

**Failure scenario:** Snapshots run as detached tasks, and Internal::Snapshot carries neither owner nor generation. If SwayNC loses its name while GetSubscribeData is outstanding, the owner event correctly marks unavailable and resets generation_has_signal=false. A delayed successful reply from the old owner is then accepted as fresh, restoring available=true and old count/flags although the daemon is gone. Across replacement, old success or error can overwrite the new owner's state; the error branch marks unavailable even after a newer valid signal.

**Verification:** `review_media_snapshot_from_dead_owner_is_discarded` supplies private fake DBus/SwayNC interfaces. It holds GetSubscribeData, emits owner loss and confirms unavailable, then releases the old reply. The topic incorrectly becomes available=true/count9. The assertion fails after the worker is explicitly aborted; only private transports and fake interfaces are involved. See [hyprv-svc-media probe log](scratch/round2/hyprv-svc-media.log).

**Suggested fix:** Tag every snapshot with unique owner and an incrementing owner epoch; discard success and error completions that no longer belong to the active epoch. Cancel old tasks where practical. Fresh signals should supersede earlier snapshot errors as well as earlier successes. Reconcile subscribe-before-snapshot without permitting cross-owner results.

