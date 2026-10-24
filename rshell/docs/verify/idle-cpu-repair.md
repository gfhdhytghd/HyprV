# Power idle CPU repair

User authorized a minimal power-crate fix alongside svc-net's concurrent review repairs. Only `crates/hyprv-svc-power/src/actor.rs` was edited in that crate; existing drain/backoff/guest-time repairs remain passing. All test buses are private child dbus-daemon processes, destroyed at test end. No native actions, live service mutations or privileged tracing occurred.

## Exact cause

The `hyprv-services` thread accounted for 52,719 of 53,048 baseline scheduling timeslices. Scratch event counters identify `hyprv_svc_power::power::Native`: more than 256 snapshots in eight seconds, driven by inboxes incorrectly accepting global NameOwnerChanged and cross-provider PropertiesChanged. No matching power state was published. All provider labels accepted the same unrelated signals. See [before trace](raw/power-event-trace.log).

The installed zbus 5.19.0 source (`MatchRule::matches`) documents that a well-known sender always matches locally because that matcher has no bus owner knowledge. A second rule on the shared connection admits unrelated traffic that the provider-only rule also accepts. The daemon's own rule filtering alone cannot protect multiple local streams sharing the socket.

## Minimal fix

- Resolve a provider to its unique owner before constructing the raw stream. Its unique name is enforced locally and by the broker.
- For an absent provider, keep a pending inbox; do not create a ready EOF or reconnect timer. NameOwnerChanged for that provider triggers recovery.
- Watch owner changes with explicit interface/member/path and arg0 per provider, eliminating unrelated names at both broker and stream match layers. Install these before connect/snapshot; reconnect resolves a replacement owner.
- Keep bounded independent drains, explicit overflow failure, cooldowns and owner-watch reinstallation from svc-net's repairs. P2P test transports retain their previous behavior.

[Patch](raw/power-idle-fix.patch). The two new tests use a private daemon so they exercise real unique/well-known name routing and shared-connection rule overlap. The noise regression [fails on the original source](raw/power-idle-before-regression.log) and passes after the fix. The other regression covers unrelated owner changes, absence and owner replacement. All **17 power tests**, including three concurrent review regressions, pass; strict all-target power clippy passes. [Latest tests](raw/power-idle-latest-tests.log), [clippy](raw/power-idle-latest-clippy.log).

## Evidence boundary

Both controlled binaries use the same frozen dependencies; only actor.rs changes. Source hashes and executable digests are embedded in raw/perf-release-before.json and raw/perf-release-after.json. Separate release-latest results incorporate newer desktop-owner work and are not attributed causally to the power patch. Cargo release uses opt-level=3 and no debug assertions; the isolated workspace does not copy root fat-LTO settings. No instrumented binary contributes a performance result.

After trace: a real UPower peripheral battery PropertiesChanged produces one power event and one snapshot; unrelated global owner traffic does not create refreshes. [After trace](raw/power-event-trace-after.log). Initial sysstats/battery samples occur before demand release and then stop. The 60-second power-only release after interval has 3.265 ms of thread CPU (~0.0054%), zero process CPU ticks, 108 scheduling events (~1.80/s), and no power state publications. Genuine provider signals can still wake the service and do not necessarily change its public state.

Full-service performance, rendering-inclusive Quickshell comparisons and residual event activity are in [perf.md](perf.md). A power-only pass is not presented as blanket whole-service acceptance.
