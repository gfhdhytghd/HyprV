# Laptop Phase B — svc-media implementation report (2026-10-24)

Cases: `lap.media.compact.idle`, `lap.media.expand` (`docs/verify/laptop-qa-plan.md` §4;
spec `docs/spec/laptop-island.md` §6, `laptop-minimal-resources.md` "Media").

## Result

| case | provider (svc-media) | host wiring |
|---|---|---|
| `lap.media.compact.idle` | **PASS**: over a 3 s compact window with a fake player playing, the counters show no MPRIS actor turn, no `Get Position` call (also counted at the fake), no spectrum start, and no context switch on the service thread | requested (`docs/requests/hyprv-shell.md`, "laptop media demand wiring") |
| `lap.media.expand` | **PASS**: the first snapshot read right after `mpris::present`, with no waiting, shows the title, artist, playing and length. Position is within 50 ms of the player's own clock after 3 s compact. No position polling while presented; exactly one Cider-repair turn on reveal | the same request (position clock and spectrum must also require the bar to be revealed) |

Tests: `cargo test -p hyprv-svc-media` passes: 40 lib, `fake_bus`, `review_regressions`,
`laptop_media` (passed 3 of 3 repeated runs). The new tests are:
- `tests/laptop_media.rs::lap_media_compact_idle_then_expand`: a private dbus-daemon and
  a fake MPRIS player whose position follows its own clock and counts reads.
- `spectrum::tests::capture_needs_a_presenting_subscriber_not_just_playing_media`: a
  fake capture backend. Playing alone gives no start; a presenting subscriber starts
  capture; dropping it stops capture and clears the values; presented but paused gives
  no start.

## Changes (only `crates/hyprv-svc-media`)
- `common.rs`: `Context.counters: Arc<Counters>` (MPRIS wakeups, position reads, compat
  refreshes, spectrum starts) and `CounterSnapshot` (serde).
- `mpris/mod.rs`: counters, plus the `Presentation` lease and `present(&StateStore)`. The
  laptop `mediaUiVisible` maps to holding it.
- `spectrum/mod.rs`: a private backend seam (`spawn_with`) for the fake capture test. It
  counts starts. `spawn` is unchanged.
- README: documents the presentation lease and the attribution.

There is no profile branch in svc-media. The services were already demand-driven:
- position is an event-kept anchor;
- cava runs only while subscribed and playing;
- the Cider fallback runs only while presented.

So laptop needs no different service policy, only different host demand. Desktop
behaviour is unchanged: the default constructors are the same, and the new counters are
`Relaxed` atomics with no allocation.

## Multi-artist joining (spec §6 open item)
Checked against the real Quickshell: `qs` offscreen on a private bus, with a fake player
built on Gio. `trackArtist` gives `Alpha, Beta`, `Alpha, Beta, Gamma` and `Solo`, so it
joins with `", "`. `Metadata::artist()` joins the same way (unit test
`metadata_parsing`). There is no delta.

## Notes
- The first Quickshell probe attempt failed to start its private dbus-daemon (socket path
  too long). For about 1 s, the fake player and an offscreen `qs` (no surfaces) may have
  fallen back to the default session bus before they were killed. I saw no lasting
  effect. Later runs used an abstract socket and an empty-address guard.
- The host wiring (spectrum and position gated on the bar being revealed, the
  presentation lease, the gated MPRIS forwarder, `dataStatus` counters) is engine's.
  Until it lands, the nested e2e for these IDs stays PENDING.
