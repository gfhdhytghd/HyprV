# Native system services

Implements `systemd`, `vm`, `mihomo`, `hyprland`, `calendar`, and `weather`. Each module exports a serde `State`, serde `Action`, topic `Key`, and `spawn(Context) -> Handle<State, Action>`. Spawn exactly once per domain, sharing one core `StateStore` and `ServiceRuntime` across all outputs. The crate does not execute helper programs.

```rust,ignore
use std::sync::Arc;
use hyprv_core::{runtime::ServiceRuntime, store::StateStore, service::Service};
use hyprv_svc_sys::{Context, vm};
let runtime = Arc::new(ServiceRuntime::new()?);
let ctx = Context { runtime: runtime.clone(), store: Arc::new(StateStore::default()) };
let mut vm = vm::spawn(ctx);
let visible = vm.state.subscribe(); // retain only while this consumer needs state
// Dispatch survives dropping `visible`; the shell retains the service handle.
let request = vm.dispatch(vm::Action::Toggle).await?;
// UI must unmap overlays and release exclusive input before acknowledging:
vm.dispatch(vm::Action::InputReleased { request: request["request"].as_u64().unwrap() }).await?;
vm.stop().await?;
```

Handles use the core `Service` start/stop contract; spawning starts immediately, `start` checks liveness, and `stop` cancels pending IO and joins. A stopped handle must be replaced with a new spawn. Global shutdown also cancels/joins through the runtime. Dropping only a topic subscription never cancels an action. UI updates use retained, equality-filtered core topics; dispatch returns a request result, not optimistic hardware state. No module starts a second runtime or renders UI.

Core currently exposes `Topic::demand()` and subscriber guards rather than a named `SamplingLease` type. These guards are the sampling leases used here: only transitions between zero and nonzero demand start/stop streams or freshness deadlines. Additional subscribers do not restart a provider. No generic core change is required. No core files were changed.

## Providers and actions

| Module | Native source and actions |
|---|---|
| systemd | One subscribed connection per user/system scope, shared through a store-owned bus pool. Provider-only signals plus filtered `NameOwnerChanged`, `Subscribe`, `LoadUnit`, unit properties, Unit/Job lifecycle. `Watch`, `Operate(Start/Stop/Restart)`, `Refresh`; unit names and job IDs retain scope. Bounded completed-job history. |
| vm | Exact **system** unit `windows-autoshop.service`. `Toggle`, `Start`, `Hibernate` prepare an input-release request; `InputReleased` starts/stops using interactive systemd polkit authorization. `Cancel` is allowed before release; `Refresh` reads unit state. Busy/phase/action IDs survive panel subscription loss. JobRemoved(`done`) plus unit readback completes success. The “hibernate” operation is StopUnit. |
| mihomo | YAML controller/secret, parent inotify, `/version`, `/configs`, `/traffic`, `/connections`; capability-tested WebSocket with HTTP NDJSON fallback. `Mode(rule/global/direct)`, `Service(Start/Stop/Restart)`, `OpenDashboard` via OpenURI portal, `Refresh`. Unit signals remain active without a panel. `legacy_json()` provides the source's `online/service/version/mode/connections/upload/download/dashboard` fields. |
| hyprland | Request `.socket.sock` and event `.socket2.sock`, subscribed before initial snapshot. Clients, monitors, active window, workspaces (including empty 1..8), keymap, motion inheritance, geometry. `FocusWindow`, `WorkspaceActivate`, `WorkspaceMoveWindow`, `Refresh`; no hyprctl. Compositor command acceptance is distinct from observed state. |
| calendar | Separate request/subscription NDJSON channels. `Week`, `PreviousWeek`, `NextWeek`, `Today`, `Refresh`, `OpenEvent`, `NewEvent`, `Show`, `ConfigureSocket`. Request IDs, bounded frames, 10-second deadlines, 350-ms event refresh debounce. Hidden calendars/task calendars retain visibility flags; date-only/offset timestamps and recurrence UID+start remain intact. Subscription stays connected while week fetching follows demand. `child_window_requested` signals the UI handoff. |
| weather | Native HTTPS OpenWeather forecast/current requests; `.env`, weather/view cache inotify; selected-unit values, icons/colors, UTC forecast-date grouping with local hourly labels, today/tomorrow slot retention. `Day(0..4)`, `Next`, `Previous`, `Refresh`. `legacy_json()` reproduces string-valued source JSON. Preserves good cache on HTTP errors. |

Source semantics: `WindowsVmController.qml:19-44`, `scripts/windows-vm-control.sh:4-23`, `CalendarBridge.qml:62-163`, `CalendarSocket.qml:23-40`, `BarMotion.qml:16-31`, `scripts/mihomo-control.sh:5-97`, `hypr-comp/scripts/quickshell/calendar/weather.sh:45-265`, and the `services.md` adapter contract. Paths above are relative to the original read-only `quickshell` tree.

Systemd signal streams drain independently of actor RPCs; property invalidations coalesce, while unit-filtered VM job completions have a separate bounded queue. Job overflow surfaces an explicit unavailable/error outcome instead of silently evicting completions. Calendar pending request IDs, write offsets, deadlines and invalidations survive cancelled event futures. Rejected unit watches do not enter the retained watch list. File watches filter basenames and narrow missing-parent watches.

Only desired-provider reconnection uses exponential backoff (400 ms to 12 s plus jitter). Post-connect stream failures also respect that deadline; retry history resets after a successful provider event. Connected hidden services have no periodic timers. Mihomo's two-second external-config fallback and live streams exist only with demand. Weather freshness is 900 s for good data, 3600 s for missing/pending credentials, with bounded offline backoff; its timer disappears without subscribers. Logind sleep signals pause Mihomo streams/config deadlines and weather refresh deadlines; resume resnapshots/rechecks freshness. Socket/file/D-Bus readiness is event driven. An explicit user refresh is an action, not background sampling.

HTTP secrets never enter public state, legacy payloads, Debug output, or error URLs. Frame/body sizes, event counts, completed-job history and action queues are bounded. Actions are internal APIs: the shell's public IPC layer must independently authorize operations.

## Configuration

- Hyprland: `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE`; `spawn_with_config` supports an explicit directory and workspace count.
- Calendar: `HYPRV_DCAL_SOCKET` is an absolute native socket path. Alternatively `HYPRV_DCAL_DISCOVERY_FILE` points to watched JSON `{"path":"/absolute/socket"}`. Explicit `Config`/`ConfigureSocket` are also available. Without overrides, native discovery scans `XDG_RUNTIME_DIR` for live, owned `dankcal-PID.sock` endpoints in dcal filename order, validates peer credentials, and watches socket changes with inotify. Stale `.session`/`.pid` markers and the UI-only `dankcal.path` are ignored. No `dcal --json socket` execution or periodic discovery polling occurs.
- Mihomo: `~/.config/mihomo/config.yaml`, override through `spawn_with_config`. YAML quoting and secrets containing `#` are preserved. Wildcard controllers are normalized to loopback. HTTP credentials in controller URLs are rejected.
- Weather: original `hypr-comp/scripts/quickshell/calendar/.env` (`OPENWEATHER_KEY`, `OPENWEATHER_CITY_ID`, `OPENWEATHER_UNIT`), with environment fallback; `QS_CACHE_WEATHER`, else `QS_CACHE_DIR/weather`, else `~/.cache/hyprv/hypr-comp/weather`. Cached weather carries `_hyprv` unit/city metadata. Old values retain their original unit/location and data generation while changed settings await replacement. Legacy caches without identity are not imported as current data; a fresh successful fetch replaces them. Daily slot caches are not reused across configuration changes. Cache names remain `weather.json`, `daily_weather_cache.json`, `next_day_precache.json`, `view_id`. `spawn_with_config` permits isolated paths/endpoints.

## Verification

All development artifacts are inside this crate. `.check/` is an ignored isolated workspace containing a copy of core and this crate, preserving the shared workspace lockfile while other agents work. The check script refreshes those copies before each check. Validation commands (from rshell root):

```sh
crates/hyprv-svc-sys/scripts/check.sh test
crates/hyprv-svc-sys/scripts/check.sh clippy --all-targets -- -D warnings
crates/hyprv-svc-sys/scripts/check.sh test --test read_only_bus -- --ignored
```

Validated on 2026-10-23: **30 unit/mock/regression tests passed**, the opt-in **read-only bus test passed on both scopes**, rustfmt checks passed, and clippy passed with `--all-targets -- -D warnings`. Root workspace integration was not run because other agents own parallel service crates; all checks used the isolated workspace.

Tests cover private mock D-Bus systemd properties/jobs and interactive-auth flags, VM release ordering and completion, two native calendar sockets/response correlation/event refresh, Hyprland socket request framing, WebSocket and HTTP NDJSON fallback, secret exclusion, weather HTTP normalization/cache retention, inotify atomic replacement, frame limits, subscriber transitions and shutdown. The opt-in real-bus test reads only systemd `Version` on both scopes; it performs no unit loading or mutations.

Manual verification after shell integration:

1. Keep services hidden and inspect runtime wakeups; open/close weather and Mihomo consumers and confirm streams/timers disappear on last subscription. Mock lifecycle evidence does not prove host-level wakeup counts.
2. Supply the verified native dcal socket/discovery contract, check hidden subscription updates, rapid week changes, recurring/all-day events, and editor/full-app RPCs.
3. Compare compositor geometry, monitor transform/scale, keymap, motion curves and workspace dispatch against the running compositor in an explicitly authorized UI session.
4. In an explicitly authorized VM test, verify panel unmap/input release before polkit, cancellation, failed jobs, external service changes and popup closure while a job is running.
5. With explicitly supplied provider access, compare Mihomo settings/rates/dashboard and weather forecast/cache rollover; test suspend/resume and network reconnection.

## Gaps and evidence limits

- Native dcal discovery/activation is not specified by the audited source. A configured socket or watched discovery file is required; automatic CLI discovery has intentionally not been substituted with a guessed path.
- Unmanaged Clash Verge process recognition has no proven event source in this contract. The API can still report online, but `legacy_service_scope` is `none` unless an actual user/system Mihomo unit is found; unmanaged service actions return unavailable.
- Portal acceptance for OpenDashboard returns a portal request path, not proof that a browser opened. VM/editor handoff requires the UI to consume the exposed request/phase fields. UI wiring belongs to the shell owner.
- Actual authentication dialogs, VM mutations, external providers, visual parity, suspend/resume and measured host idle wakeups remain **PENDING**. Build/mock and read-only bus evidence do not claim those outcomes. Nothing was installed, committed, or restarted.
