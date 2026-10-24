# Adversarial review: hyprv-svc-sys

Reviewed 2026-10-23. Findings below were reproduced against source copies under `docs/reviews/scratch/`; production crates were not edited. Source fingerprints are in [source-hashes.json](scratch/source-hashes.json), and exact failing test names/results are in [results.json](scratch/results.json). UI source changed concurrently during the review; the scratch UI snapshot was refreshed and all UI probes rerun against the refreshed implementation, including the new text module.

`P1` means a high-priority lifecycle/deadlock or lost-update defect. `P2` means a correctness, resource-lifetime, or unnecessary-wakeup defect. These are deliberately failing regression probes, not a claim that the normal crate test suite failed. No live D-Bus mutations, compositor/shell restarts, or live Wayland surfaces were used. Run `python3 docs/reviews/scratch/verify.py` from rshell root to reproduce the audited snapshot; the runner checks source fingerprints and expects the listed assertion failures.

<a id="sys-1"></a>
## SYS-1 [P1] Undrained systemd signal streams can block the shared D-Bus connection

**Location:** `crates/hyprv-svc-sys/src/systemd.rs:173` (256-entry raw MessageStreams at lines 173–193); RPC snapshots at lines 127–151; initial `Native::connect` subscribes then awaits snapshots at lines 235–246. Also affects VM/Mihomo users of the pooled Manager connection.

**Failure scenario:** A broad systemd provider stream is retained but not continuously polled while an actor awaits a D-Bus RPC, initial snapshot, action/authentication or another provider. Once more than 256 provider messages arrive, zbus's non-overflow receiver backpressures its socket-reader task. That reader cannot deliver the outstanding method reply on the same connection. Awaiting the reply while waiting to resume draining is a deadlock until a timeout or cancellation drops the relevant stream. Since the connection is shared, one stalled consumer can stall other systemd actors too. Installing matches before snapshots avoids a missed-initial-state race but does not make this blocking pattern safe.

**Verification:** `review_sys_unpolled_signals_do_not_block_snapshot_replies` uses a private P2P fake systemd server, verifies a baseline unit snapshot succeeds, installs the real `Manager::signals()`, emits 300 property signals and attempts another unit snapshot. It stalls past 100ms. A control independently drains the same streams and the next snapshot succeeds within one second: `unpolled stalled=true; independent draining restored replies=true`. See [recorded test output](scratch/logs/hyprv-svc-sys.log). No real bus or unit was mutated.

**Suggested fix:** Continuously drain each raw zbus stream in an independent task, feeding a bounded/coalescing domain-event channel or retained per-unit/job state that remains drainable while RPCs wait. Preserve lossless correlation for tracked JobRemoved outcomes. Do not fix this by subscribing after the snapshot, increasing the queue alone or silently dropping required job completions. This implements the services spec's subscribe-before-snapshot requirement without blocking the transport.

<a id="sys-2"></a>
## SYS-2 [P1] A connect-success/read-failure loop retries without backoff and monopolizes the reactor

**Location:** `crates/hyprv-svc-sys/src/support.rs:172` and `crates/hyprv-svc-sys/src/support.rs:244` (next-error path at lines 244–245; reconnect backoff exists only inside the connect-error branch).

**Failure scenario:** A provider accepts a connection and initial snapshot, then immediately closes/fails its event stream. `next()` marks it disconnected, and the loop immediately calls connect again. Every short-lived success resets retry history; none of these failures enter the backoff branch. A perpetually ready failure path can consume the entire single-thread Tokio reactor, hammer the provider and prevent other services from making progress even with no UI demand.

**Verification:** `review_sys_flapping_provider_has_reconnect_backoff` uses an in-memory transport with exactly this behavior. It records over **11,000 reconnects in 40ms** rather than at most the initial attempt. Exact run count is timing-dependent and retained in [recorded test output](scratch/logs/hyprv-svc-sys.log); no network traffic is used.

**Suggested fix:** Apply a reconnect deadline to failures after connect as well as failures during connect. Reset backoff only after sustained healthy operation or a meaningful provider event, and ensure retry paths yield and honor shutdown/actions. Retain the zero-demand/no-unneeded-wakeups policy for providers that need not be connected while hidden. This violates both the zero-wakeup intent and the documented 400ms–12s reconnection policy.

<a id="sys-3"></a>
## SYS-3 [P1] Cancelling Backend::next loses a consumed calendar update

**Location:** `crates/hyprv-svc-sys/src/support.rs:244` within the competing action/demand select; calendar `crates/hyprv-svc-sys/src/calendar.rs:328` awaits `calendars.list` after consuming the notification.

**Failure scenario:** A calendar-list notification is consumed and `next()` sends its refresh RPC. A popup subscription transition or action then wins the launcher's select, dropping the in-progress `next` future. The reply may arrive, but no pending operation owns it anymore. The calendar notification is gone, the calendars array stays old, and opening the panel only fetches events, not calendars. A later request skips the orphaned reply because its ID is not the new request ID. The same general cancellation hazard applies to other multi-step `next()` implementations; this finding is specifically verified for calendar state loss.

**Verification:** `review_sys_calendar_refresh_survives_next_cancellation` uses private UnixStream pairs, an old calendar list, a `calendars` event and a correctly correlated new-list reply. It cancels `next` only after the peer sees the RPC (the precise point at which a demand transition can cancel it), then delivers the reply and resumes the backend. State remains `old`, expected `new`; see [recorded test output](scratch/logs/hyprv-svc-sys.log).

**Suggested fix:** Keep outstanding RPC futures/request correlation as actor-owned state across select iterations, or split event acquisition into a cancellation-safe transport stage and a persistent processing stage. Mark calendar-list invalidation independently and clear it only after a completed refresh; demand changes must not discard it. Do not rely on a request serial in a future that the driver has already dropped. This violates the calendar subscription/latest-state contract in `docs/spec/services.md` and `ui-panels-calendar.md`.

<a id="sys-4"></a>
## SYS-4 [P2] A rejected unit watch permanently poisons all subsequent refreshes

**Location:** `crates/hyprv-svc-sys/src/systemd.rs:298` before `refresh` can fail; refresh returns early on the first missing/error unit at lines 213–220.

**Failure scenario:** The service already watches a valid unit. A caller requests another syntactically valid but nonexistent unit. The action returns an error, but its name has already been retained in `watched`. Every later refresh fails on it, and reconnect's mandatory refresh fails too. One rejected watch turns a healthy shared monitor into a repeated reconnecting/unavailable service, freezing updates for the previously valid units. There is no Unwatch action to repair the poisoned list.

**Verification:** `review_sys_failed_watch_does_not_poison_existing_units` uses the private mock manager: the existing `windows-autoshop.service` watch succeeds, `missing.service` fails, and a subsequent otherwise-valid refresh still fails. See [recorded test output](scratch/logs/hyprv-svc-sys.log).

**Suggested fix:** Resolve/validate the new unit before committing the watch, roll back on failure, or represent each watched unit's unavailable state separately from the bus connection. Existing valid watches must continue updating. Consider explicit removal and eventual creation handling for intentionally absent units.

<a id="sys-5"></a>
## SYS-5 [P2] VM transport failure leaves connected=true

**Location:** `crates/hyprv-svc-sys/src/vm.rs:49` (lines 49–53), unlike the calendar/systemd failure handlers.

**Failure scenario:** A VM actor has successfully connected and its transport subsequently fails. The launcher calls `State::failure`, but this method only writes last_error/busy/phase; it never invalidates `connected`. Throughout failed reconnect attempts, consumers see `connected=true` with retained old unit/running values. The special NameOwnerChanged branch explicitly clears the flag, but other bus/RPC/stream failures do not use that branch.

**Verification:** `review_sys_vm_failure_invalidates_connected` starts with connected=true and calls the actual failure reducer with a bus-loss error. The connected=false assertion fails; see [recorded test output](scratch/logs/hyprv-svc-sys.log).

**Suggested fix:** Clear connected in the general failure reducer, retain old unit values only as explicitly stale data, and require successful refresh to restore connectivity. Test EOF, transport failure and failed reconnect separately from an owner-change signal. The services API requires unavailable observations to be distinguishable from current hardware/service state.

<a id="sys-6"></a>
## SYS-6 [P2] Weather settings changes relabel old Celsius data as Fahrenheit

**Location:** `crates/hyprv-svc-sys/src/weather.rs:624` (line 624), especially `self.state.unit=self.unit()` before a successful replacement fetch.

**Failure scenario:** Good Celsius weather values are retained. The .env changes OPENWEATHER_UNIT from metric to imperial. The watcher changes the public state's unit to Fahrenheit immediately but leaves every numeric temperature, feels-like value and wind speed from the previous unit system. With no subscribers, fetching waits indefinitely; with a network failure, the mislabeled retained data persists. Changing city/key can similarly leave old-location data without a configuration-generation identity, although this probe specifically verifies unit corruption.

**Verification:** `review_sys_unit_switch_does_not_relabel_old_temperature` runs the actual watcher branch with scratch .env changes and a private idle D-Bus transport. `current_temperature` stays **21.0**, while unit becomes Fahrenheit instead of retaining Celsius or converting to 69.8°F. See [recorded test output](scratch/logs/hyprv-svc-sys.log). It performs no HTTP requests.

**Suggested fix:** Store the unit/provider identity alongside the cached data. Retain that data's original unit while a replacement fetch is pending/failed, explicitly mark it stale, or convert all affected values consistently. A new settings generation must not relabel an old data generation. Migrate legacy caches conservatively because they contain numeric strings without embedded units.

<a id="sys-7"></a>
## SYS-7 [P2] Provider FileWatch wakes on unrelated siblings

**Location:** `crates/hyprv-svc-sys/src/support.rs:342` (lines 342–368), which discards raw inotify records and unconditionally rearms/returns success.

**Failure scenario:** A change anywhere in a watched parent directory is treated as a provider-input change. An unrelated sibling log/build file causes calendar discovery checks, Mihomo YAML reads or weather settings/cache reads even while hidden. A missing configuration parent can widen the watch to a busier ancestor. Retaining old watches also accumulates irrelevant directories after ancestor/path moves rather than narrowing to the live paths. The verified defect is the false invalidation; unbounded lifetime of every possible watch topology was not measured.

**Verification:** `review_sys_watch_ignores_unrelated_sibling` watches only config.yaml, writes unrelated.log and immediately receives a relevant-change notification. See [recorded test output](scratch/logs/hyprv-svc-sys.log). This is separate code from CORE-3 and needs a separate owner fix; no periodic timer is alleged by this probe.

**Suggested fix:** Filter decoded inotify records by current descriptor/path/basename and rearm only on relevant topology/self/overflow events. Narrow direct file/parent watches and remove obsolete watches without turning IN_IGNORED into a reload loop. This removes unnecessary hidden-service wakeups/work while preserving atomic replacement and missing-parent handling.
