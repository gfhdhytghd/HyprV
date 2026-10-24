# Native dcal discovery — 2026-10-24

Fixed the live calendar connection blocker in hyprv-svc-sys. Without either discovery override, the actor now discovers the backend in XDG_RUNTIME_DIR, subscribes to its NDJSON protocol, and loads calendars/events. Discovery executes no subprocess. No live shell or dcal process was restarted.

## Actual selection rule

The installed `dcal --json socket` selects `/run/user/1000/dankcal-3194001.sock`, with `running: true` ([captured output](calendar-discovery/dcal-socket.json)). Backend PID3194001 is owned by UID1000. The adjacent `.pid` contains3194130, the Quickshell UI child, so requiring the marker contents to equal the backend PID would reject the live endpoint. `dankcal.path` identifies UI extraction assets, never the IPC socket; `.session` markers are also irrelevant to socket discovery.

Installed Go build metadata identifies dcal revision `123dc2c1b723cd8a41a5f75209f824b4e33ecc11` (modified build) and dankgo dependency `a4ef23371e05`. The dependency's [FindRunningSocket implementation](https://github.com/AvengeMedia/dankgo/blob/a4ef23371e05/ipc/client.go) lists the runtime directory in lexicographic filename order, filters `dankcal-*.sock`, and returns the first endpoint accepting a connection within500ms. It does not consult pid/session/path markers. The [UI pidfile implementation](https://github.com/AvengeMedia/dankgo/blob/a4ef23371e05/shellapp/pidfile.go) explains the different marker PID.

Because the installed build is modified, the CLI behavior was also checked in an isolated temporary runtime: two listening sockets named `dankcal-100.sock` and `dankcal-20.sock`, with misleading stale pid/session/path markers, select100 first and20 after100 is removed. This confirms filename order rather than newest/highest PID or marker selection. The real running CLI and native actor select the same host endpoint.

The native implementation preserves this order for valid endpoints and additionally rejects nonnumeric/dead/zombie PIDs, wrong owners, non-listening stale socket inodes, symlinks, and peer credentials inconsistent with the filename PID/current UID. It checks inode identity across the connection probe. These checks intentionally reject unsafe/misnamed endpoints that the generic CLI could accept.

## Reconnection and idle behavior

The runtime directory inotify watch is armed before the initial scan. Socket create/remove/move/attribute events trigger rediscovery, including replacement at the same pathname while old accepted channels remain alive. Stale session/pid/path files and unrelated service files do not trigger actor reconnection. When no backend exists, automatic discovery waits for filesystem events rather than running a retry timer. Established connection failures retain bounded backoff. Existing `HYPRV_DCAL_SOCKET` and `HYPRV_DCAL_DISCOVERY_FILE` hooks retain precedence and watch their selected socket/discovery file.

## Verification

- `cargo test --locked -p hyprv-svc-sys --all-targets`:38 passed; one existing manual bus integration test ignored. Includes stale-file/PID/owner/non-listening/symlink cases, lexicographic order, filtered directory events, provider appearing after a parked start, same-path server restart with old connections alive, and explicit override precedence. [Log](calendar-discovery/tests.log).
- `cargo clippy --locked -p hyprv-svc-sys --all-targets -- -D warnings`: passed. [Log](calendar-discovery/clippy.log).
- Native example `calendar-discovery-check --read-only`, with default configuration and no configured endpoint, connected/subscribed to the CLI-selected live socket and loaded5 calendars/112 events, `loading:false`, request serial1. [Receipt](calendar-discovery/live.json). Only subscribe, calendars.list and events.list requests are issued; no event content is retained in evidence.
- Live restart was not attempted; restart behavior is covered by isolated mock servers. Privhelper files and live services were untouched.

Source fingerprints: [sources.sha256](calendar-discovery/sources.sha256). Tests and the example used the shared target directory with incremental compilation disabled. Release coordination and publication are recorded in docs/requests/hyprv-privhelper.md. Existing parity/performance receipts are not recertified by these focused service checks.

## Rebuilt candidate

Official `packaging/build-release.sh` completed after all recorded source freezes. Release ID: `eeafb5a678912bbaa802016d562b2b69fcea3ab0afc20f2f2a54ddba788a04c5`. Shell SHA-256: `c364f556a1a5168a7bcac866616e1912a87b06951b7ac321b2a7a0f22d1d52df`. Independent verification matched all4855 current source inputs, the full hash manifest, allthree staged/flat ELF pairs, and the four frozen calendar file fingerprints. Deployment workspace is `/home/wilf/.config/HyprV/rshell`. [Verification receipt](calendar-discovery/release-check.json), [build log](calendar-discovery/release-build.log).

The preceding build correctly refused publication after the concurrent fan signal-filter fix; its [rejected log](calendar-discovery/release-build-rejected-drift.log) is retained. This candidate has not been installed or started. Full parity/performance/preflight evidence has not been regenerated for this new ELF; previous receipts must not be presented as current or the performance acceptance rebound without fresh measurement.
