# Laptop Phase B — svc-net

2026-10-24. Service implementation complete in `crates/hyprv-svc-net`; cross-owner shell visibility acceptance and whole-shell laptop budget/parity remain separate gates. No sudo, installation, commit, live bus mutations, live session use or private Cargo target. All actor tests use in-memory transports or private D-Bus peers; the opt-in host inspection test remains ignored. Builds used shared `target`, `CARGO_INCREMENTAL=0`; disk had62 GiB free at final checks.

## Implementation

- Consume engine's landed `hyprv_core::profile::{profile, Profile}`. No independent profile detector. No sysfs reader was added.
- `network::Handle::scan_lease.acquire()` returns a core refcounted subscription guard; only requested Wi-Fi pages hold it. Passive network/bar subscriptions do not grant scan authority. Several outputs share one scanner.
- Laptop scanner projects `scannerEnabled` as State.scanning when a radio exists, requests scans only with visible demand and enabled/available hardware, and parks without deadlines while hidden/read-only/provider-absent/radio-off. Final guard drop clears the deadline and aborts the client request. A hardware scan already accepted by NM cannot be undone by closing its page.
- Quickshell's10001 ms interval throttles both recent cached NM LastScan and this client's requests, including rapid reopen. LastScan uses CLOCK_BOOTTIME milliseconds, not Unix epoch. No extra D-Bus polling reads the timestamp. Reappearance uses owner-change events, not retries; RequestScan has NO_AUTO_START. Explicit laptop Rescan checks page demand again before transport access.
- Laptop radio selection follows `origin/laptop@24972f7:quickshell/BarServices.qml:18–20`: first connected radio, else first radio. Desktop retains its original selection, explicit Rescan without a lease, and serialized model. Scanning demand follows the same file's Binding (`:50–55`) and shell.qml:3292.
- Bluetooth has no laptop-specific QML delta. Its production code remains unchanged: passive ObjectManager/property signals, existing user-initiated four-second discovery, no minimal-state scan timer. Added a private BlueZ regression, rather than a second Bluetooth policy.
- Native NM/BlueZ construction respects DBUS_SYSTEM_BUS_ADDRESS. A fresh-child malformed-address test proves both fail without falling back to the host. Private bus tests bypass host rfkill; production rfkill remains event-driven with no retry timer.

Upstream scanner source inspected read-only: [wireless.cpp](https://github.com/quickshell-mirror/quickshell/blob/master/src/network/nm/wireless.cpp#L155) and [wireless.hpp](https://github.com/quickshell-mirror/quickshell/blob/master/src/network/nm/wireless.hpp#L86), read2026-10-24. Current source hashes for this implementation: [receipt](laptop-impl-svc-net/source-hashes.json).

## Verification

| Check | Result |
|---|---|
| `cargo test --locked -p hyprv-svc-net -j4` |47 passed (43 library+4 actor integration),1 host-only test ignored |
| Same suite with `HYPRV_PROFILE=laptop` |47 passed, same intentional ignore |
| `cargo clippy --locked -p hyprv-svc-net --all-targets -j4 -- -D warnings` |PASS |
| Existing owner `cargo test --locked -p hyprv-svc-power sysstats:: -j4` |20 passed,1 intentional ignore; no owner source edited |

All commands used incremental=0 and shared debug/test profiles with debug information disabled. A pre-existing Cargo registry index-cache permission warning did not prevent successful gates. Logs: [default](laptop-impl-svc-net/tests.log), [forced laptop](laptop-impl-svc-net/laptop-profile-tests.log), [Clippy](laptop-impl-svc-net/clippy.log), [rate owner](laptop-impl-svc-net/rate-owner-tests.log).

Private NM tests cover observer-only/minimal idle, hidden explicit-rescan rejection, visible acquisition, multiple leases, interval boundary, rapid reopen, final close, shutdown, read-only mode, owner loss and reappearance. Across a virtual idle hour, discovery calls and snapshot reads do not increase. A scanner unit test confirms an outstanding request future is dropped and the deadline cleared on final close. Clock tests exercise LastScan's missing/future/boundary values. Desktop tests verify model JSON bytes and snapshot read count remain unchanged by a scan lease, with exactly one explicit Rescan call. Private BlueZ tests verify zero unsolicited discovery/read polling, exactly one requested start and one stop at four seconds, then no repeated reads or stop calls over another virtual hour.

The first scanner probe incorrectly advanced the test clock while the production supervisor had its own runtime clock. Final tests run the identical production actor on the test runtime and allow the timer wheel's sub-millisecond rounding. This was corrected in the harness; passing virtual time is not a physical wakeup measurement.

## Network-rate ownership and integration

Rate sampling is implemented in `hyprv-svc-power::sysstats`, not svc-net. Reused the existing typed Metric::Network lease and shared scheduler; did not duplicate counter reading or edit that owner's crate. Read/ran its fake-tree/private-bus tests: hidden/passive demand does not sample, compact CPU+RAM has no network/route subscription, network plans disappear with demand, and reshow primes baselines. Laptop route selection preserves the pinned script's first-default-row semantics (even without UP), and negative deltas clamp to zero.

Filed exact wiring requirements in `docs/requests/hyprv-shell.md` and `docs/requests/hyprv-svc-power.md`. Shell owner has concurrently added the page scanner hook and removed Network/Temperature from the laptop generic-panel lease. At report time `assembly.rs` still aggregates Consumer::Wifi via `wifi.is_mapped()`, which can outlive requested visibility during closing; the owner received a concrete request to use `popup_visible` for Laptop and retain Desktop behavior. Rust's existing CP Wi-Fi action closes CP and opens the standalone popup, so there is no Page::Wifi enum to invent. Aggregate all outputs and release demand on requested close/suspend, not surface destruction.

Full nested lap.wifi.scan.visible, close/suspend/multi-output integration, complete desktop byte-identical UI parity,204-case desktop e2e and measured laptop CPU/RSS/wakeup budgets remain QA/shell-owner gates. This report proves service calls/model behavior under private fakes, not those end-to-end gates or physical radio behavior.
