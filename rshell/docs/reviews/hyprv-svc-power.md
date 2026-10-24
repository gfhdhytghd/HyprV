# Adversarial review: hyprv-svc-power

Reviewed 2026-10-23. Production crates and packaging were read-only. Findings were verified with source inspection and isolated probes under [scratch/round2](scratch/round2/README.md); [source fingerprints](scratch/round2/source-hashes.json) and [results](scratch/round2/results.json) identify the audited snapshot. Shared core/media lifecycle code changed during the review; affected copies were refreshed and probes rerun against the final fingerprinted source.

P1 means a high-priority security, cooling-safety, starvation, deadlock or resource-lifetime defect; P2 means correctness or deployment risk. Expected regression assertions deliberately fail; this is not a claim that the original unit suite failed. No root execution, live hardware writes, real-bus mutations, installs or service/compositor restarts occurred. Physical fan/thermal behavior, real polkit prompts and systemd sandbox execution remain unverified. Run `python3 docs/reviews/scratch/round2/verify.py` to reproduce.

<a id="power-1"></a>
## POWER-1 [P1] Undrained provider signal streams can block the RPC they are meant to refresh

**Location:** `crates/hyprv-svc-power/src/actor.rs:238` (signals), `crates/hyprv-svc-power/src/actor.rs:214` and `crates/hyprv-svc-power/src/actor.rs:228` (awaiting snapshot outside stream polling).

**Failure scenario:** The actor awaits backend.snapshot/apply while its raw provider and owner MessageStreams remain unpolled. zbus queues are bounded and backpressure its shared connection reader. A provider emitting more than 128 matching signals during a snapshot/action fills the stream; the reader then cannot reach the method reply that would finish that operation. Power.snapshot performs many UPower/profile RPCs, and signal rules include every signal from a provider rather than only relevant properties. A signal burst can therefore stall state/actions until RPC timeouts and can affect other actors sharing ctx.bus. This is a single-runtime async wait cycle, not a claimed std::Mutex/calloop cross-thread deadlock.

**Verification:** `review_power_signal_queue_cannot_block_snapshot_rpc` uses the production signals() function on private zbus Unix-pair connections, emits 180 matching signals, and shows a Ping reply cannot complete within 100ms. An independent drainer immediately restores replies within the control timeout. The expected nonblocking assertion fails; no real UPower daemon or system bus was touched. See [hyprv-svc-power probe log](scratch/round2/hyprv-svc-power.log).

**Suggested fix:** Continuously drain subscribed signals in a separate task into a bounded/coalesced dirty-state channel, or poll the stream concurrently with outstanding RPCs. Coalesce updates without backpressuring the socket reader; narrow the match rules and bound RPCs. Increasing capacity only moves the burst threshold. Preserve subscribe-before-snapshot semantics.

<a id="power-2"></a>
## POWER-2 [P1] Event failure reconnects immediately when connect itself succeeds

**Location:** `crates/hyprv-svc-power/src/actor.rs:207` and `crates/hyprv-svc-power/src/actor.rs:226` .

**Failure scenario:** Only connect errors enter the backoff. An event source that repeatedly errors after successful registration (for example repeated kernel queue failures while new netlink registration still succeeds) goes event Err → ready=false → connect Ok → snapshot → event Err with no delay. Immediate futures can monopolize the current-thread runtime and repeatedly publish default/live/error transitions, delaying all other service/UI bridge work. The ready path also resets retry before the event source proves healthy.

**Verification:** `review_power_event_failure_has_reconnect_backoff` runs the actual generic actor on ServiceRuntime with connect/snapshot succeeding and event immediately failing. It observes 1001 event/connect cycles in 30ms. The fake source parks after 1001 to keep the probe bounded and allow shutdown. A retained sleep sender prevents the harness from accidentally testing a closed-watch busy loop. See [hyprv-svc-power probe log](scratch/round2/hyprv-svc-power.log).

**Suggested fix:** Apply bounded backoff to failed established sessions as well as failed setup; reset the attempt counter only after a useful healthy session/event or stability interval. Preserve action replies and shutdown responsiveness during recovery. Avoid biased always-ready branches starving unrelated runtime tasks.

<a id="power-3"></a>
## POWER-3 [P2] CPU utilization double-counts guest and guest_nice counters

**Location:** `crates/hyprv-svc-power/src/sysstats.rs:72`.

**Failure scenario:** Linux /proc/stat user/nice already include guest/guest_nice. Summing all columns adds guest execution twice. With delta user=100, idle=100, guest=100, the real utilization is 100/(100+100)=50%; the parser computes total300 and reports66.67%. Running the Windows VM makes this visible in the bar and histories.

**Verification:** `review_power_guest_time_is_not_counted_twice` feeds those exact before/after /proc/stat records into production parse_cpu and cpu_delta; the 50% assertion fails. All input is in-memory. See [hyprv-svc-power probe log](scratch/round2/hyprv-svc-power.log).

**Suggested fix:** Compute total from user through steal only (the first eight fields), or subtract the guest fields exactly once with checked arithmetic. Retain iowait handling and counter-reset checks. Add guest and guest_nice examples to the parser tests; this is a CPU telemetry correctness deviation from the ksystemstats-style contract in docs/spec/services.md:78.

