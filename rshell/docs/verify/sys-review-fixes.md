# svc-sys review repairs

2026-10-23: all seven findings from [the review](../reviews/hyprv-svc-sys.md) repaired in the production crate. The original scratch snapshot and its expected-failure runner remain unchanged.

| Finding | Change | Regression evidence |
|---|---|---|
| SYS-1 | Independent raw D-Bus drain tasks; coalesced invalidations; separate completion queues and VM unit filter; explicit overflow failure | Unpolled-stream snapshot regression; 600-property flood preserves exact JobRemoved completion and RPC progress |
| SYS-2 | Persistent reconnect deadline after connect, read, demand and transport failures; retry history survives short connects | Flapping provider cannot reconnect repeatedly in 40 ms; bounded exponential delay test |
| SYS-3 | Actor-owned pending RPC ID, byte offset and absolute timeout; retained calendar-list invalidation and resumed work | Cancel next after RPC transmission, deliver response, resume and observe new calendars |
| SYS-4 | Resolve before committing Watch; per-unit refresh errors cannot poison valid units | Failed missing-unit watch followed by successful valid refresh |
| SYS-5 | General VM failure reducer clears connected | Transport-failure connectivity regression |
| SYS-6 | Data unit/city/generation remain distinct from new configuration; unknown cache identity not guessed; cross-configuration daily caches discarded | Unit-switch watcher regression; failed replacement preserves old identity; legacy unknown cache; successful HTTP replacement excludes old-location slots |
| SYS-7 | Decode/filter inotify descriptors and names; rearm relevant topology changes and remove obsolete watches | Unrelated sibling regression; missing-parent narrowing; atomic replacement; no rearm self-wakeup |

Validation: 30 unit/mock/regression tests passed; strict all-target clippy passed; live systemd Version read passed on both user and system buses. Commands are in the [crate README](../../crates/hyprv-svc-sys/README.md). All used the isolated `.check` workspace. Live tests only read Version; no unit actions, authentication dialogs, compositor commands or service restarts were performed. Property/job tests use private mock buses. Hardware, action and visual outcomes are outside this evidence.

Signal completion storage is bounded: overflow explicitly fails the stream so consumers must treat the outcome as uncertain. This does not promise recovery of a historical systemd job result after transport loss. No required completion is silently replaced by a property invalidation.
