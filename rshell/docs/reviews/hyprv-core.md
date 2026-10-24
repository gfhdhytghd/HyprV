# Adversarial review: hyprv-core

Reviewed 2026-10-23. Findings below were reproduced against source copies under `docs/reviews/scratch/`; production crates were not edited. Source fingerprints are in [source-hashes.json](scratch/source-hashes.json), and exact failing test names/results are in [results.json](scratch/results.json). UI source changed concurrently during the review; the scratch UI snapshot was refreshed and all UI probes rerun against the refreshed implementation, including the new text module.

`P1` means a high-priority lifecycle/deadlock or lost-update defect. `P2` means a correctness, resource-lifetime, or unnecessary-wakeup defect. These are deliberately failing regression probes, not a claim that the normal crate test suite failed. No live D-Bus mutations, compositor/shell restarts, or live Wayland surfaces were used. Run `python3 docs/reviews/scratch/verify.py` from rshell root to reproduce the audited snapshot; the runner checks source fingerprints and expects the listed assertion failures.

<a id="core-1"></a>
## CORE-1 [P1] Last runtime owner dropped on its worker thread panics during self-join

**Location:** `crates/hyprv-core/src/runtime.rs:63` (`Drop`, lines 58–65); runtime is publicly shareable through `Arc<ServiceRuntime>` in the service contexts.

**Failure scenario:** An actor owns the final runtime `Arc` after the UI/controller drops its copy. When that actor releases its context, `ServiceRuntime::drop` runs on `hyprv-services`, closes the runtime completion channel, and tries to join that same thread. `JoinHandle::join` panics with `Resource deadlock avoided (os error 35)`; this tears down the runtime instead of completing normal actor cleanup. This is a runtime-thread ownership defect, not a claimed arbitrary cross-thread lock cycle.

**Verification:** `review_core_last_runtime_arc_may_drop_on_actor_thread` transfers the final owner to a real core actor and catches the destructor panic. The assertion that final-owner release is safe fails; [recorded test output](scratch/logs/hyprv-core.log) includes the self-join error. No external service is involved.

**Suggested fix:** Separate the clonable spawn/shutdown context from the UI-owned runtime/join owner. At minimum, compare thread IDs before joining and detach/defer joining on the worker thread; provide an explicit asynchronous shutdown/join operation for the controller. Keep synchronous blocking teardown off the calloop callback path. Add the final-Arc-on-worker regression case to core tests. This follows the architecture's tokio/calloop ownership and service-lifecycle boundary.

<a id="core-2"></a>
## CORE-2 [P2] IPC cleanup unlinks an unrelated replacement inode

**Location:** `crates/hyprv-core/src/ipc.rs:194` (`IpcServer::drop`).

**Failure scenario:** After successful bind, the socket pathname is removed or replaced by same-user recovery/cleanup tooling while the original listener is still alive. When that listener drops, it unconditionally unlinks whatever now occupies the pathname, including a regular file or another listener. Bind-time refusal to remove non-sockets does not protect the shutdown path. A regular replacement file's contents are lost.

**Verification:** `review_core_drop_preserves_replacement_file` binds an actual Unix listener entirely in scratch storage, replaces its filesystem entry with a regular file, then drops the old server. The expected `replacement data` becomes `None`; [recorded test output](scratch/logs/hyprv-core.log) records the assertion. A directory-FD alias keeps the socket path below `SUN_LEN` while all data remains inside scratch.

**Suggested fix:** Record the bound pathname's device/inode and use a retained directory FD. Remove only a socket entry still owned by this server; do not remove a regular file, symlink or replacement socket. Keep the single-instance lock held through cleanup. The test establishes same-user pathname ownership loss, **not** a cross-user privilege escalation; the existing 0700 directory and server-side peer-UID check were inspected and no bypass of those checks is claimed.

<a id="core-3"></a>
## CORE-3 [P2] Unrelated sibling file activity invalidates the config/theme watch

**Location:** `crates/hyprv-core/src/config.rs:126` (lines 126–145), with parent watches installed at lines 99–117 and full config/theme reload at lines 156–178.

**Failure scenario:** The default `/tmp/qs_colors.json` watch subscribes to its whole parent directory. Any unrelated file create/close-write/delete is read and returned as a relevant change; event names/watch IDs are never decoded. `watch_config` then rebuilds watches and reloads both config and theme. A build or logging workload in `/tmp` wakes the service and performs repeated filesystem/JSON work although its inputs are unchanged. Equality suppression only prevents the final topic publication; it does not remove the wakeup/work.

**Verification:** `review_core_watch_ignores_unrelated_sibling` watches `config.json`, writes only `unrelated.log`, and expects no relevant change. It immediately receives one; see [recorded test output](scratch/logs/hyprv-core.log). This proves false invalidations, not an internally installed periodic timer or an empirical whole-shell idle-wakeup count.

**Suggested fix:** Decode `inotify_event` and filter by watch descriptor/basename, handling directory-self events/overflow separately. Watch existing files directly for content writes and retain appropriately filtered parent watches for atomic replacement/later creation. Rebuild only when the watched path/ancestor actually changes. This addresses the services spec's parent-watch requirement without treating every sibling as a source update.

<a id="core-4"></a>
## CORE-4 [P2] Unknown non-string theme metadata rejects valid known colors

**Location:** `crates/hyprv-core/src/config.rs:68` (lines 68–69), before the unknown-key filter at lines 71–83.

**Failure scenario:** A valid theme contains `{"base":"#112233","metadata":{"generator":"test"}}`. The QML reader applies `base` and ignores `metadata`; Rust first deserializes every entry as a string, so it rejects the entire update. The later occupied-key check cannot implement the documented unknown-field ignore behavior once deserialization has failed.

**Verification:** `review_core_unknown_theme_metadata_is_ignored` uses exactly that JSON in scratch. `Theme::reload` returns `invalid type: map, expected a string`; the expected valid `base` update is not applied. See [recorded test output](scratch/logs/hyprv-core.log). The source consumer is `quickshell/hypr-comp/MatugenColors.qml:44-65`; core README also explicitly promises unknown fields are ignored.

**Suggested fix:** Parse an object of `serde_json::Value`, discard unknown keys first, then validate strings/colors for known fields; retain atomic rejection for malformed *known* colors if that is the intended contract. Add unknown string/number/object metadata cases beside the current partial-theme tests.
