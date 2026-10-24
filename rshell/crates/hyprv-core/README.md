# Core infrastructure milestone

Implemented without changing QML, Hyprland, or the rendering crate.

- `store`: `StateStore` keys topics by marker type (`TopicKey`), with cloned typed `Topic<T>` handles. Equality suppresses publication. Revisions serialize concurrent publishers; watch subscriptions coalesce slow consumers. `publish` returns a before/after diff and `changed_diff` reports the difference since the subscriber's last delivered revision. Subscribers own demand guards; dropping the last subscription deactivates sampling.
- `service`: async start/stop contract and `run_sampled` lifecycle driven by subscriber transitions and shutdown. Services must release actor tasks and timers on stop. No automatic retries or idle polling. A topic `Subscription` is the sampling lease: the first subscriber starts sampling, dropping the last stops it. `gated_demand(demand, gates, shutdown)` combines a topic's subscriber count with extra boolean gates (e.g. spectrum: visible **and** playing) into the `watch<usize>` that `run_sampled` takes; spawn the returned driver future next to it.
- `runtime`: one Tokio reactor thread, generic calloop channel bridge, explicit shutdown signal, setup context and actor join support. UI thread callbacks own all UI state; services must not render. Graceful callers request shutdown and join their actors before dropping the runtime (drop otherwise cancels remaining tasks); `shutdown_and_join(self)` does both explicitly from the controller thread. If the last `Arc<ServiceRuntime>` is released on the runtime thread itself (an actor held the final owner), drop signals shutdown and lets the thread exit without self-joining (CORE-1). Prefer not to move the runtime into actor tasks at all.
- `clock`: minute-aligned absolute realtime timerfd with `TFD_TIMER_CANCEL_ON_SET`, plus logind `PrepareForSleep` signal subscription. `run_clock` only owns a timer while its topic has subscribers; no invisible-clock timer. Clock events carry epoch seconds; the shell topic currently stores epoch minutes. Resume refreshes the retained time immediately. `SleepTopic` (`SleepState { sleeping, resume_generation }`) is the shared suspend/resume signal: the shell host runs `run_sleep_topic` **once**; services subscribe to the store topic and re-snapshot when `resume_generation` changes instead of each subscribing to logind. Kernel clock-setting and actual suspend/resume have not been exercised.
- `config`: strict JSON config at `$XDG_CONFIG_HOME/hyprv-shell/config.json` (or `$HOME/.config/hyprv-shell/config.json`), currently `{ "theme_file": "/tmp/qs_colors.json" }`. Watches parent directories using nonblocking inotify, including atomic replacement and later creation. Invalid reloads preserve the last valid state; invalid initial config is a startup error. Parents missing at startup are watched through their nearest existing ancestor. Inotify events are decoded and filtered by watch descriptor and entry name (the file, or the missing ancestor directory), so unrelated siblings such as other files in `/tmp` cause no wakeup-to-reload (CORE-3); overflow and loss of a watched directory force a rebuild. File deletion resets config to defaults, while absent/invalid theme files retain colors.

Theme defaults and partial updates follow `quickshell/hypr-comp/MatugenColors.qml:9-30` and `quickshell/hypr-comp/MatugenColors.qml:44-65`. The source file is `/tmp/qs_colors.json` (`quickshell/hypr-comp/MatugenColors.qml:36`). Runtime JSON contains the same named color fields, validated as `#RRGGBB` or Qt-style `#AARRGGBB`; unknown theme fields of any JSON type are ignored; a known colour that is not a valid string rejects the whole update atomically (CORE-4). The old one-second helper process (`quickshell/hypr-comp/MatugenColors.qml:74-79`) is replaced with inotify.

## IPC and integration contract

`$XDG_RUNTIME_DIR/hyprv/shell.sock` accepts one newline-delimited JSON request per connection:

```json
{"target":"quickAdjust","function":"showBrightnessLevel","args":["50"]}
```

Response: `{"ok":true,"result":"..."}` or `{"ok":false,"result":"..."}` plus newline. CLI syntax is `hyprv-shell ipc call <target> <function> [args..]`; `hyprv-shell kill` requests graceful shutdown and waits for acknowledgement. This mirrors command names/arguments, not Quickshell's private wire protocol. Same-user peer credentials, 0700 directory, 0600 socket, 64 KiB frames, 64 simultaneous clients and 10-second request deadlines are enforced. On drop the server unlinks the socket path only if it is still a socket with the device/inode it bound (CORE-2), while the instance lock is still held. `flock` is held across the instance lifetime; a stale socket can be removed only after acquiring the lock, and a regular file/symlink is never removed as a stale socket.

| Target | Functions | QML source |
|---|---|---|
| bar | toggle(monitor), layoutStatus(monitor), inputStatus(), dataStatus() | quickshell/shell.qml:44-80 |
| controlPanel | toggle() | quickshell/shell.qml:3403-3410 |
| calendar | toggle() | quickshell/shell.qml:3412-3421 |
| quickAdjust | showBrightness(), showBrightnessLevel(integer), showBrightnessIncrease(), showBrightnessDecrease(), showVolume() | quickshell/shell.qml:3423-3454 |

`ipc::ui_handler` sends a `UiRequest` through calloop; the UI replies over a oneshot after processing. The binary skeleton currently returns an explicit unavailable error for UI actions and status queries, and implements only transport, lifecycle and kill. It does not fabricate layout/data status or claim UI actions succeeded. Replace `ShellState::dispatch` when components land. Hold topic subscriptions only while consumers need updates; drive rendering damage from changed revisions. `ServiceRuntime::bridge` is intentionally independent of the engine crate and shares calloop 0.13.

## Verification

Run `cargo test -p hyprv-core -p hyprv-shell -p hyprv-privhelper`. Tests cover state diffs, retained state, subscriber counts, sampled service start/stop, IPC roundtrip, single-instance locking, non-socket preservation, actual CLI lifecycle/kill acknowledgement, strict helper inputs and atomic theme replacement. Run the standalone parity tool tests with `cargo test --manifest-path xtask/Cargo.toml`.

Validation completed: 8 core unit tests, 2 helper unit tests, 1 shell CLI integration test and 1 parity unit test passed (12 total). Clippy with `--all-targets -- -D warnings` passed for these three crates and xtask. A generated PNG fixture exercised the actual `cargo xtask` wrapper: inclusive tolerance pass, mismatch failure, missing-file failure and HTML/Markdown report output. D-Bus and polkit packaging XML parsed successfully.

Build/static and isolated local IPC evidence are available. UI, pixels, system-bus activation/polkit, hardware control, actual suspend and wall-clock changes remain pending. Nothing is installed or reloaded.

## Ownership note (2026-10-23)

Maintained by the svc-media agent after the original author moved to UI work. Resolved
requests: CORE-1…CORE-4 (`docs/requests/hyprv-core.md`) and svc-media's three requests
(`docs/requests/core.md`), each with a regression test (`runtime::tests`,
`ipc::tests::drop_preserves_replacement_file`, `config::tests::watch_ignores_unrelated_sibling_but_sees_target_and_missing_parent`,
`config::tests::unknown_theme_metadata_is_ignored`, `service::tests::gated_demand_requires_subscribers_and_gate`).
