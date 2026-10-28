
- [SHELL-1] [P1] Output removal retains orphan panels/islands and expired deadlines — [verified review](../reviews/hyprv-shell.md#shell-1).
- [SHELL-2] [P2] Native surface disappearance is ignored while its logical role remains desired — [verified review](../reviews/hyprv-shell.md#shell-2).
- [SHELL-3] [P2] Brightness preview IPC performs hardware writes and persistence — [verified review](../reviews/hyprv-shell.md#shell-3).

## [ISL-SHELL-1] [P2] Map native pointer cancel to `hyprv_ui_island::Pointer::Cancel` (from svc-media, 2026-10-23)

`crates/hyprv-shell/src/mapping.rs` `island_pointer` maps `PointerEvent::Leave | PointerEvent::Cancel => P::Leave`.
For ISLAND-3, the island now has `Pointer::Cancel` (grab lost → ends press/seek without a click
and restarts the seek preview reset). `Leave` deliberately keeps an outside drag/seek alive.
Change: `PointerEvent::Leave => P::Leave, PointerEvent::Cancel => P::Cancel`. Hiding through the
calendar is already handled inside `set_inputs`, so nothing else is needed.


### Integration CI — 2026-10-23T08:15:04+00:00 — Preflight release binaries

Non-installing `cutover/preflight.sh --run-tests --json`: [Errno 2] No such file or directory: '/home/wilf/.config/HyprV/rshell/packaging/dist/build.json'. See `docs/verify/raw/ci/round-01/preflight.log`. Please regenerate valid current evidence after the underlying owner changes pass; do not relax the gate. This is gate routing, not proof that every routed crate caused the failure.


### Integration CI — 2026-10-23T08:15:04+00:00 — Preflight automated tests

Non-installing `cutover/preflight.sh --run-tests --json`: automated test receipt failed or stale; run preflight --run-tests. See `docs/verify/raw/ci/round-01/preflight.log`. Please regenerate valid current evidence after the underlying owner changes pass; do not relax the gate. This is gate routing, not proof that every routed crate caused the failure.

## UI rendering performance (svc-media, 2026-10-23) — see [docs/verify/ui-perf.md](../verify/ui-perf.md)

- [SHELL-PERF-1] [P2] **Island per-frame cost.** `assembly.rs:604-609` clones the full island
  `Inputs` (agents, strings, spectrum) every frame just to change the calendar fields. Use the new
  `Island::set_calendar(open, progress, now)`. Call `frame()` only while `animating()`, at
  `next_deadline()` or after input. Rebuild/reconcile the island only when `Island::visual_rev()`
  changed or is `None`. Measured with that host policy (bench): idle island 428 → 0 allocations and
  no rebuild; compact spectrum 49 → 7 allocations per update; agent list 611 → 56.
- [SHELL-PERF-2] [P3] **Calendar morph:** keep the built `CalendarPanel` subtree across frames while
  only the morph container animates (see PANELS-PERF-4).
- [SHELL-PERF-3] [P1] **Popup surface size / VRAM** (engine PERF-UI-7): full-output popup layer
  surfaces cost 162 MiB of VRAM each before swapchain buffers at 6144×3456. Prefer card-sized surfaces.

## From svc-net review round 5: Wi-Fi empty password (2026-10-23, applied by svc-desktop at the user's request)

`src/services.rs:359-360` now maps `WifiConnect { password: "" }` to `Connect { password: None }` instead of `Some("")`.

Why: an empty field means "use the saved profile secret", as the legacy `wifi-action.sh:63` `[ -n "$password" ]` did. Sending `Some("")` was the trigger for review NET-6, where a saved PSK was overwritten with "". `hyprv-svc-net` now also ignores empty input defensively.

The edit is that one expression. The workspace builds and `cargo test -p hyprv-shell` passes.

### Round-6 review → hyprv-shell (2026-10-23)

These come from [docs/reviews/hyprv-ui-panels.md](../reviews/hyprv-ui-panels.md):
- `mapping.rs:130` `_ => K::Text(text?)`: map `KP_Enter` to `Key::Enter` instead of passing `"\r"` text (PANEL-3).
- `mapping.rs` `panel_pointer`: `PointerEvent::Cancel` and `Leave` both become `Pointer::Leave`. Once ui-panels adds a cancel path, forward a distinct cancel (PANEL-2).
- `services.rs` `WifiConnect`: the action is cloned and dropped without zeroizing. Zeroize the panel action's password after routing (PANEL-1).


### Integration CI — 2026-10-23T08:50:01+00:00 — production release staging failed

Local, non-installing `packaging/build-release.sh` exited 1. See `docs/verify/raw/ci/round-04/stage-release.log`. Build output:

```text
cargo build --manifest-path /home/wilf/.config/HyprV/rshell/Cargo.toml --locked --release --target-dir /home/wilf/.config/HyprV/rshell/target --config /home/wilf/.config/HyprV/rshell/packaging/release.toml -p hyprv-shell -p hyprv-privhelper --bins
Stage hyprv-shell, hyprv-privhelper, hyprv-cooling-failsafe and build.json under /home/wilf/.config/HyprV/rshell/packaging/dist
    Updating crates.io index
warning: failed to write cache, path: /home/wilf/.cargo/registry/index/index.crates.io-1949cf8c6b5b557f/.cache/li/bz/libz-sys, error: Permission denied (os error 13)
   Compiling hyprv-ui v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui)
   Compiling hyprv-widgets v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-widgets)
   Compiling hyprv-ui-panels v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-panels)
   Compiling hyprv-ui-bar v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-bar)
   Compiling hyprv-ui-island v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-island)
   Compiling hyprv-shell v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-shell)
    Finished `release` profile [optimized] target(s) in 1m 35s
sources changed during build; refusing an ambiguous receipt, rerun

```


### Integration CI — 2026-10-23T08:50:09+00:00 — Preflight release binaries

Non-installing `cutover/preflight.sh --run-tests --json`: candidate source changed since release build. See `docs/verify/raw/ci/round-04/preflight.log`. Please regenerate valid current evidence after the underlying owner changes pass; do not relax the gate. This is gate routing, not proof that every routed crate caused the failure.

### Engine correction receipt — 2026-10-23

SHELL-1/2/3 now have production Assembly regressions: output ownership/deadlines
are dropped together, missing native mounts/binding changes recreate surfaces,
and brightness notification IPC produces no service action/persistence. Preview
lifetime is local; explicit sliders still dispatch typed actions. ISL-SHELL-1 maps
Cancel distinctly and native device/seat loss emits PointerCancel.

SHELL-PERF-1: set_calendar avoids Inputs clones, frame runs only while animated/due,
and immutable island trees cache by visual_rev. SHELL-PERF-2: calendar trees retain
model/geometry/input keys and rebuild for content animations, not morph-only frames.
SHELL-PERF-3 / PERF-UI-7: card-sized EGL targets plus transparent 1×1 outside-input
surfaces; whole-device-pixel crop origins preserve text phase. Regressions verify
coverage, identity, nonempty GPU PNGs and buffer sizes. Production controllers get
per-frame update paths; compatibility Host::Component remains only a fixture/demo.

Native touchpad transport is connected: pointer v9 value120/relative_direction,
pixel residuals and Qt Begin/Update/End reach island/menu Flickables. Initial SCTK
seats are registered before capability events (previously absent); keyboards stay
on SCTK. No fabricated Wayland momentum phase. KP_Enter maps Enter. WifiConnect
moves the secret into the provider's zeroizing Action instead of cloning/dropping
an unzeroized copy; read-only rejection zeroizes it directly.

Validation: workspace build, 62 shell/UI tests, 19 selected control tests and owned
strict Clippy pass. Six-scene private nested GPU capture test passes. See
crates/hyprv-ui/VALIDATION.md and crates/hyprv-shell/README.md for commands/evidence.


### Integration CI — 2026-10-23T09:46:02+00:00 — preflight parity freshness includes generated artifacts

`cutover/preflight.py:86` scans every component file except four directory names. Current stale findings are caused by generated files, not evidence of a newer Rust source change:

- bar: `crates/hyprv-ui-bar/tools/__pycache__/check_rect_effect.cpython-314.pyc` mtime 2026-10-23T09:07:43.785660+00:00, SHA-256 `f22576d6107adf3a36ef4b6376ac2ab07bb0baf66a954634e8e46a296fe090c4` exceeds render crate_mtime 1792744878.0077024. Pixel-gate result counts remain {'excluded': 12, 'fail': 68, 'missing': 8, 'pass': 224}.
- panels: `crates/hyprv-ui-panels/target-png/trend-ours@2.png` mtime 2026-10-23T09:10:25.395414+00:00, SHA-256 `049bdb1fa8534145539475038f7bc77152402a3f668abb2040a4235d6121a857` exceeds render crate_mtime 1792745304.853707. Pixel-gate result counts remain {'fail': 184, 'missing': 52, 'pass': 64}.

Please make freshness track actual candidate source inputs (including shared renderer/widgets), excluding bytecode/generated render outputs, rather than touching or accepting stale pixel results. This is a checker defect routed to the shell/cutover owner; no gate or evidence was changed by CI. Reproduce with `cutover/preflight.sh --json`.

### Real-shell performance measurement — read-only production entry point

Added `--read-only` to `hyprv-shell` options and passed it to the existing `assembly::run(..., read_only)` parameter. This exposes the same provider/UI graph used by the isolated smoke harness without its 1.6-second timeout or fixture scene. It suppresses native action dispatch and network agent registration during nested measurements; defaults remain production behavior. Release build: `cargo build --locked --release -p hyprv-shell`, log `docs/verify/raw/perf-shell-build.log`. No replacement renderer or probe JSON streaming is used. The private compositor, private IPC runtime, and live-provider/read-only service boundaries are recorded by `docs/verify/measure-shell.py`.

### Mechanical strict-Clippy cleanup — 2026-10-23

The user extended the UI lint assignment to remaining workspace diagnostics. Only formatting was changed in these shell files; each was re-read immediately before editing:

- `src/fixture.rs`, `Fixture::apply`: separated the two intentionally independent props/root initialization guards onto separate lines. Both still execute in order; neither became an else branch.
- `src/replay.rs`, `Op::Capture`: separated the first-capture and 40 ms readiness guards onto separate lines; conditions, early returns, queue operations and capture order are unchanged.
- `src/nested.rs`, `real_outputs`: separated the read-error and JSON parse guards onto separate lines; socket operations, timeout, parsing and output filtering are unchanged.

The owner concurrently made the non-exhaustive Layer match in `fullscreen.rs` exhaustive; that fallback was left intact and is not attributed to this cleanup. The earlier Action/Drop route fixes were also made by their owner, not here.

`cargo clippy --workspace --all-targets --locked -- -D warnings` now passes. Raw receipt: `docs/reviews/scratch/clippy-mechanical/workspace-clippy-final-3.{jsonl,stderr}`. No nested/live session, socket probe, replay, install or compositor reload was run by this cleanup; only build/tests and isolated offscreen UI parity captures were requested.

Mechanical-cleanup final verification: workspace all-targets strict Clippy passes; 67 focused existing tests pass. The isolated canonical verifier produced 1,236 before/after PNG pairs with identical PNG and RGBA bytes, and unchanged Qt-reference gate metrics. Existing reference failures remain; absent island/panels GPU hooks were not counted. [Receipt and scope](../reviews/scratch/clippy-mechanical/README.md).

### svc-desktop rehearsal → hyprv-shell: no degraded mode without a system bus (2026-10-23)

In the isolated cutover rehearsal ([docs/verify/cutover-rehearsal.md](../verify/cutover-rehearsal.md), `raw/cutover-rehearsal/steps/21-*.log`) the native shell exited at start with `Failed to connect to address unix:path=/var/run/dbus/system_bus_socket`. Legacy Quickshell only warns and keeps the bar. A dbus-broker restart or an early-login race would take the native bar down: `Restart=on-failure` gives 3 tries in 30 s, then it stays down. Please treat system-bus services (NM, BlueZ, UPower, logind, privhelper) as optional at startup: watch for the bus and reconnect, like the session-bus services. With a private system bus at the standard path, the native shell came up in under 1 s on both nested outputs.

### Real-shell idle CPU: noncanonical clip reconciliation loop (fixed)

A second independent loop remained after Shared equality: zero native agent-input events still cost 4.7394% CPU, 90.20 scheduling timeslices/s. A frozen diagnostic copy logged the center clip alternating between a 120.15625-wide Rectangle and its device-snapped 120-wide Group with an inserted Rectangle child on every update. See `docs/verify/raw/perf-shell/damage-trace/shell-damage-trace-idle.log` (instrumented debug data, not a performance result).

`assembly.rs` compared a freshly built unsnapped bar against a retained tree previously mutated by `panels::qrect::finalize`, then finalized it again after each difference. The mismatch perpetually invalidated the bar and requested another callback. The host now builds the same canonical clip geometry before initial mounting and before reconciliation (`bar_tree` / `update_bar_scene`). The regression `fractional_bar_clip_is_normalized_before_reconciliation` fails on the old ordering (four edits on an equal model) and verifies both idle no-damage and redraw on a genuine CPU-stat change. All 24 shell library tests pass. Logs: `docs/verify/raw/perf-shell/clip-{before,after}.log`. Final release remeasurement is recorded in `docs/verify/perf-shell.md`; the workspace is changing concurrently, so whole-binary comparisons are not attributed solely to one patch.


### Real-shell residual idle UI cost [P1]

After fixing Shared identity and clip canonicalization, the real release binary still uses **0.4159% process CPU** (0.4359% summed thread runtime) over 60.1166 s with zero agent-hook inputs, no media, no spectrum, and panels closed. Main UI thread consumes **0.221571 CPU s**; service thread **0.036702 s**; audio RT threads consume zero. Sampling/procfs counts fall to **12.71 scheduling timeslices/s**, so the perpetual frame feedback is substantially reduced, but <=0.1% is not met. See `docs/verify/raw/perf-shell/final-quiet/final-quiet-idle.json` and its `-data-status.json`.

Remaining path to optimize: `Assembly::sync` still rebuilds the full desired tree and calls `update_bar_scene` on ordinary schedule callbacks, including the follow-up callback after a genuine 1 Hz stats change. `bar::build` rebuilds all modules/layout even for a single changed stat; scene reconciliation then repaints the damaged bar. Preserve clock, stats, tray and animation changes while avoiding redundant rebuilds and retaining unchanged module trees. This localizes residual CPU to the UI update/paint path; build-versus-paint sub-costs are not claimed as separately sampled. Do not mark the 0.1% budget green from the event-excluded run.


### Integration CI — 2026-10-23T12:29:00+00:00 — release: cannot find type `OutputInfo` in this scope

Reproduce: `cargo build --workspace --release --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-shell/src/startup_confirm.rs:48:75`.

`crates/hyprv-shell/src/startup_confirm.rs`: mtime 2026-10-23T12:28:51+00:00; SHA-256 `a2724bc4e8825b3f7b7ec7a33fd59359b18ace6e78c10601d4a63ed04bac7189`; changed during command: True.

```text
error[E0425]: cannot find type `OutputInfo` in this scope
  --> crates/hyprv-shell/src/startup_confirm.rs:48:75
   |
48 |     fn output_added(&mut self, output: &wl_output::WlOutput, info: Option<OutputInfo>) -> Vec<(LayerConfig, Scene)> {
   |                                                                           ^^^^^^^^^^ not found in this scope
   |
help: you might be missing a type parameter
   |
41 | impl<OutputInfo> UiHandler for Handler {
   |     ++++++++++++


```

Full log: `docs/verify/raw/ci/round-13/release.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T12:29:00+00:00 — release: cannot borrow `d` as immutable because it is also borrowed as mutable

Reproduce: `cargo build --workspace --release --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-shell/src/startup_confirm.rs:78:58`.

`crates/hyprv-shell/src/startup_confirm.rs`: mtime 2026-10-23T12:28:51+00:00; SHA-256 `a2724bc4e8825b3f7b7ec7a33fd59359b18ace6e78c10601d4a63ed04bac7189`; changed during command: True.

```text
error[E0502]: cannot borrow `d` as immutable because it is also borrowed as mutable
  --> crates/hyprv-shell/src/startup_confirm.rs:78:58
   |
78 |                     let scene = Scene::new(d.panel.build(d.scale));
   |                                            -       ----- ^ immutable borrow occurs here
   |                                            |       |
   |                                            |       mutable borrow later used by call
   |                                            mutable borrow occurs here


```

Full log: `docs/verify/raw/ci/round-13/release.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T12:29:00+00:00 — release: cannot find module or crate `wl_output` in this scope

Reproduce: `cargo build --workspace --release --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-shell/src/startup_confirm.rs:48:41`.

`crates/hyprv-shell/src/startup_confirm.rs`: mtime 2026-10-23T12:28:51+00:00; SHA-256 `a2724bc4e8825b3f7b7ec7a33fd59359b18ace6e78c10601d4a63ed04bac7189`; changed during command: True.

```text
error[E0433]: cannot find module or crate `wl_output` in this scope
  --> crates/hyprv-shell/src/startup_confirm.rs:48:41
   |
48 |     fn output_added(&mut self, output: &wl_output::WlOutput, info: Option<OutputInfo>) -> Vec<(LayerConfig, Scene)> {
   |                                         ^^^^^^^^^ use of unresolved module or unlinked crate `wl_output`
   |
   = help: if you wanted to use a crate named `wl_output`, use `cargo add wl_output` to add it to your `Cargo.toml`


```

Full log: `docs/verify/raw/ci/round-13/release.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T12:32:34+00:00 — tests failed without compiler diagnostic

Command `cargo test --workspace --locked --message-format=json` exited 1. See `docs/verify/raw/ci/round-13/tests.log`. Tail:

```text
ng_that_doesnt_exist::foo`\n\n// or:\n\ntrait Foo {\n    fn bar() {\n        Self; // error: unresolved name `Self`\n    }\n}\n\n// or:\n\nlet x = unknown_variable;  // error: unresolved name `unknown_variable`\n```\n\nPlease verify that the name wasn't misspelled and ensure that the\nidentifier being referred to is valid for the given situation. Example:\n\n```\nenum something_that_does_exist {\n    Foo,\n}\n```\n\nOr:\n\n```\nmod something_that_does_exist {\n    pub static foo : i32 = 0i32;\n}\n\nsomething_that_does_exist::foo; // ok!\n```\n\nOr:\n\n```\nlet unknown_variable = 12u32;\nlet x = unknown_variable; // ok!\n```\n\nIf the item is not defined in the current module, it must be imported using a\n`use` statement, like so:\n\n```\n# mod foo { pub fn bar() {} }\n# fn main() {\nuse foo::bar;\nbar();\n# }\n```\n\nIf the item you are importing is not defined in some super-module of the\ncurrent module, then it must also be declared as public (e.g., `pub fn`).\n"},"level":"error","spans":[{"file_name":"crates/hyprv-shell/src/fixture/translate.rs","byte_start":22781,"byte_end":22787,"line_start":560,"line_end":560,"column_start":25,"column_end":31,"is_primary":true,"text":[{"text":"fn mihomo(out: &str) -> Mihomo {","highlight_start":25,"highlight_end":31}],"label":"not found in this scope","suggested_replacement":null,"suggestion_applicability":null,"expansion":null}],"children":[],"rendered":"error[E0425]: cannot find type `Mihomo` in this scope\n   --> crates/hyprv-shell/src/fixture/translate.rs:560:25\n    |\n560 | fn mihomo(out: &str) -> Mihomo {\n    |                         ^^^^^^ not found in this scope\n\n"}
{"$message_type":"diagnostic","message":"cannot find type `Bluetooth` in this scope","code":{"code":"E0425","explanation":"An unresolved name was used.\n\nErroneous code examples:\n\n```compile_fail,E0425\nsomething_that_doesnt_exist::foo;\n// error: unresolved name `something_that_doesnt_exist::foo`\n\n// or:\n\ntrait Foo {\n    fn bar() {\n        Self; // error: unresolved name `Self`\n    }\n}\n\n// or:\n\nlet x = unknown_variable;  // error: unresolved name `unknown_variable`\n```\n\nPlease verify that the name wasn't misspelled and ensure that the\nidentifier being referred to is valid for the given situation. Example:\n\n```\nenum something_that_does_exist {\n    Foo,\n}\n```\n\nOr:\n\n```\nmod something_that_does_exist {\n    pub static foo : i32 = 0i32;\n}\n\nsomething_that_does_exist::foo; // ok!\n```\n\nOr:\n\n```\nlet unknown_variable = 12u32;\nlet x = unknown_variable; // ok!\n```\n\nIf the item is not defined in the current module, it must be imported using a\n`use` statement, like so:\n\n```\n# mod foo { pub fn bar() {} }\n# fn main() {\nuse foo::bar;\nbar();\n# }\n```\n\nIf the item you are importing is not defined in some super-module of the\ncurrent module, then it must also be declared as public (e.g., `pub fn`).\n"},"level":"error","spans":[{"file_name":"crates/hyprv-shell/src/fixture/translate.rs","byte_start":23745,"byte_end":23754,"line_start":583,"line_end":583,"column_start":30,"column_end":39,"is_primary":true,"text":[{"text":"fn bluetooth(cfg: &Value) -> Bluetooth {","highlight_start":30,"highlight_end":39}],"label":"not found in this scope","suggested_replacement":null,"suggestion_applicability":null,"expansion":null}],"children":[],"rendered":"error[E0425]: cannot find type `Bluetooth` in this scope\n   --> crates/hyprv-shell/src/fixture/translate.rs:583:30\n    |\n583 | fn bluetooth(cfg: &Value) -> Bluetooth {\n    |                              ^^^^^^^^^ not found in this scope\n\n"}
{"$message_type":"diagnostic","message":"aborting due to 11 previous errors","code":null,"level":"error","spans":[],"children":[],"rendered":"error: aborting due to 11 previous errors\n\n"}
{"$message_type":"diagnostic","message":"Some errors have detailed explanations: E0425, E0432, E0463.","code":null,"level":"failure-note","spans":[],"children":[],"rendered":"Some errors have detailed explanations: E0425, E0432, E0463.\n"}
{"$message_type":"diagnostic","message":"For more information about an error, try `rustc --explain E0425`.","code":null,"level":"failure-note","spans":[],"children":[],"rendered":"For more information about an error, try `rustc --explain E0425`.\n"}
error: doctest failed, to rerun pass `-p hyprv-shell --doc`

Caused by:
  process didn't exit successfully: `/home/wilf/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustdoc --edition=2021 --crate-type lib --color auto --crate-name hyprv_shell --test crates/hyprv-shell/src/lib.rs --test-run-directory /home/wilf/.config/HyprV/rshell/crates/hyprv-shell -L /home/wilf/.config/HyprV/rshell/target/debug/build/skia-bindings-3c0a7a97a83db03e/out/skia -L native=/home/wilf/.config/HyprV/rshell/target/debug/build/hyprv-svc-media-b10c868e26f26510/out -L native=/home/wilf/.config/HyprV/rshell/target/debug/build/hyprv-ui-c6500342ef1d62f6/out -L native=/home/wilf/.config/HyprV/rshell/target/debug/build/libspa-f7d1f06bd59e2db6/out -L native=/home/wilf/.config/HyprV/rshell/target/debug/build/libspa-sys-9e21934df09b60a9/out -L native=/home/wilf/.config/HyprV/rshell/target/debug/build/ring-8de8b32927bad5dd/out -L native=/usr/lib --extern chrono=/home/wilf/.config/HyprV/rshell/target/debug/deps/libchrono-affef68a2383c3e0.rlib --extern hyprv_core=/home/wilf/.config/HyprV/rshell/target/debug/deps/libhyprv_core-ec392f03106ddcdf.rlib --extern hyprv_shell=/home/wilf/.config/HyprV/rshell/target/debug/deps/libhyprv_shell-a88615979ed0ab7c.rlib --extern hyprv_svc_desktop=/home/wilf/.config/HyprV/rshell/target/debug/deps/libhyprv_svc_desktop-0217a750de0aa5cf.rlib --extern hyprv_svc_media=/home/wilf/.config/HyprV/rshell/target/debug/deps/libhyprv_svc_media-e0b1bdbb2b862769.rlib --extern hyprv_svc_net=/home/wilf/.config/HyprV/rshell/target/debug/deps/libhyprv_svc_net-2975473f1a8550c6.rlib --extern hyprv_svc_power=/home/wilf/.config/HyprV/rshell/target/debug/deps/libhyprv_svc_power-2ea92b1b39be6605.rlib --extern hyprv_svc_sys=/home/wilf/.config/HyprV/rshell/target/debug/deps/libhyprv_svc_sys-5c9c012b016ff57d.rlib --extern hyprv_ui=/home/wilf/.config/HyprV/rshell/target/debug/deps/libhyprv_ui-6c6213c0d80dd611.rlib --extern hyprv_ui_bar=/home/wilf/.config/HyprV/rshell/target/debug/deps/libhyprv_ui_bar-b6bbda6adb857a4d.rlib --extern hyprv_ui_island=/home/wilf/.config/HyprV/rshell/target/debug/deps/libhyprv_ui_island-0f495b5b10992476.rlib --extern hyprv_ui_panels=/home/wilf/.config/HyprV/rshell/target/debug/deps/libhyprv_ui_panels-a26321378202f0c2.rlib --extern hyprv_widgets=/home/wilf/.config/HyprV/rshell/target/debug/deps/libhyprv_widgets-cad99819f9c30f4b.rlib --extern reqwest=/home/wilf/.config/HyprV/rshell/target/debug/deps/libreqwest-659a4752330d9517.rlib --extern serde=/home/wilf/.config/HyprV/rshell/target/debug/deps/libserde-ae12d920ef6ddb39.rlib --extern serde_json=/home/wilf/.config/HyprV/rshell/target/debug/deps/libserde_json-0118e1cbbd5450d8.rlib --extern wayland_backend=/home/wilf/.config/HyprV/rshell/target/debug/deps/libwayland_backend-6186d0a8e1e91a11.rlib --extern wayland_client=/home/wilf/.config/HyprV/rshell/target/debug/deps/libwayland_client-36544df8f27151f5.rlib --extern wayland_protocols_wlr=/home/wilf/.config/HyprV/rshell/target/debug/deps/libwayland_protocols_wlr-6d8a1f3cbd44149e.rlib --extern wayland_scanner=/home/wilf/.config/HyprV/rshell/target/debug/deps/libwayland_scanner-24c7b6106d4486d9.so --extern xkbcommon=/home/wilf/.config/HyprV/rshell/target/debug/deps/libxkbcommon-12e61562accbfcfd.rlib --extern zbus=/home/wilf/.config/HyprV/rshell/target/debug/deps/libzbus-1b3bc0db93070e11.rlib --extern zeroize=/home/wilf/.config/HyprV/rshell/target/debug/deps/libzeroize-9917054cf4892abb.rlib -L dependency=/home/wilf/.config/HyprV/rshell/target/debug/deps -C embed-bitcode=no --check-cfg 'cfg(docsrs,test)' --check-cfg 'cfg(feature, values())' --error-format json` (exit status: 1)
note: test exited abnormally; to see the full output pass --no-capture to the harness.

```

Source fingerprints at failure:
`crates/hyprv-shell/src/assembly.rs`: {'sha256': '740c1225de9dcc5bda641d5dc7603328f98c4bd82a99568fea700b6566d07851', 'mtime': '2026-10-23T12:30:49+00:00'}
`crates/hyprv-shell/src/fixture.rs`: {'sha256': '974a4656d44df0feab37bc9eee4cd6f203c845aeaac1692b35e82dcac61e4102', 'mtime': '2026-10-23T12:08:06+00:00'}
`crates/hyprv-shell/src/fixture/translate.rs`: {'sha256': 'd85456b0bd9309e5c47acfac5734d36a47bfc97d5493977d598c757f6ec79075', 'mtime': '2026-10-23T12:08:06+00:00'}
`crates/hyprv-shell/src/functional/native.rs`: {'sha256': '5f59ad5db8750e08fc7ec21def16f44eb8528eba5382a680187cb02f8b91f423', 'mtime': '2026-10-23T12:20:54+00:00'}
`crates/hyprv-shell/src/lib.rs`: {'sha256': '2b5b9a290cd58de79845ebc33a0a3bae382ed23465e534820915709bff4324b3', 'mtime': '2026-10-23T12:28:42+00:00'}
`crates/hyprv-shell/src/main.rs`: {'sha256': '95b29906555639a23cadd511a81b006f4018ab18a8bb3c120c6b330d08754cb0', 'mtime': '2026-10-23T12:28:42+00:00'}
`crates/hyprv-shell/src/mapping.rs`: {'sha256': 'd43e8006655ff51441cf8cc8e3d9235063fa1eb304811c4e176ef58b0819c797', 'mtime': '2026-10-23T08:52:50+00:00'}
`crates/hyprv-shell/src/startup_confirm.rs`: {'sha256': 'a2724bc4e8825b3f7b7ec7a33fd59359b18ace6e78c10601d4a63ed04bac7189', 'mtime': '2026-10-23T12:28:51+00:00'}
`crates/hyprv-shell/tests/cli.rs`: {'sha256': '6e336ca428e81a8a325ee07af3cda8bcb1a87763fc892d6263983471e2bea915', 'mtime': '2026-10-23T12:13:22+00:00'}
`crates/hyprv-shell/tests/no_system_bus.rs`: {'sha256': '69eddc5da9b10703a3d37fae4891bb18b4761931a557af4f05799ca340a80c10', 'mtime': '2026-10-23T12:26:21+00:00'}

Integration CI follow-up at 12:32 UTC: re-reading `startup_confirm.rs` shows the owner has already changed the reported signature to `WlOutput` / `MonitorInfo` and introduced local scale variables. The release command observed an earlier in-progress file. No additional edit was made by integration CI; next round will revalidate the completed source.

### svc-desktop → engine: system-bus degradation + native ports of the legacy hypr scripts (2026-10-23)

At the user's request I made **minimal edits in hyprv-shell**, re-reading each region right before editing. Please keep them when you refactor, or tell me where you want them moved.
- **New `src/system_bus.rs`.** If the system bus is unreachable, `Hub::spawn` no longer fails:
  - System-bus services get an in-process peer-to-peer placeholder connection. Calls fail, so those services report unavailable; the UI and session services keep running.
  - A supervisor retries with capped backoff (0.5 s doubling to 30 s, only while degraded) and publishes `SystemBusTopic`.
  - A healthy connection is watched through the bus's own `NameOwnerChanged` stream (matched on `org.freedesktop.DBus` only, so no wakeups); loss is detected when that stream ends.
  - Once the bus is back, the real app re-execs itself (`/proc/self/exe`, same PID, argv and arg0), because zbus connections cannot be re-targeted under the actors. Re-exec only happens with `Recovery::ReExec`, which `assembly::run` sets; it is the default nowhere else.
  - An in-place rebuild of the system-bus actors would avoid the re-exec. That needs swappable handles in `Hub` and is yours if you want it.
- **`services.rs`.** `Hub::spawn` builds `power::Context` from `system_bus::connect()` instead of `power::Context::system()?`, and starts the logind sleep topic only on a real bus. New `Hub::desktop_ipc()` serves IPC `desktop setVariant <v1|v2|v3|bt>` and `desktop deskflowReflow`: it awaits the `svc-desktop` result and goes through the mock `intercept` like the other actions.
- **`assembly.rs`.** `set_recovery(ReExec)` before `Hub::spawn`. The IPC handler routes target `desktop` to `hub.desktop_ipc` on the tokio side, so the caller gets the real service result; everything else still goes through `ui_handler`.
- **New `src/startup_confirm.rs` and the `main.rs` subcommand `hyprv-shell startup-confirm`.** A one-shot dialog, the counterpart of `qs --path StartupConfirm.qml`. It reuses `panels::startup::StartupConfirm` and `svc-desktop::startup::decide` on `$HYPRV_STARTUP_APPROVAL`. It binds no IPC socket, takes no lock and starts no services, so it runs beside the main shell. It exits 0 once decided and 1 otherwise. The assembly's own startup-dialog path is unchanged: it still binds the instance lock and quits after the decision, so it cannot run next to a running shell.
- **`Cargo.toml`.** Added `zbus` 5 with `tokio` and `p2p` (the placeholder needs p2p). `Cargo.lock` gained `zbus` under `hyprv-shell`.
- **Tests.** `tests/no_system_bus.rs` uses private dbus-daemons only. It starts `Hub::spawn` with no system bus, recovers when the bus appears, and detects the loss of a working bus and its return. `tests/desktop_ipc.rs` covers the mocked hub. There is also a unit test for the backoff schedule.

**Update, same day.** The scripts call **`hyprv-shell desktop setVariant|deskflowReflow`** (new `src/desktop_cli.rs` plus a `main.rs` dispatch line), not `ipc call` directly. It uses the running shell over IPC when reachable; on NotFound or ConnectionRefused it runs the same `svc-desktop` code in-process (`theme::run_once`, `deskflow::reflow_once`, added to my crate). A shell crash, such as the rotation crash in `docs/requests/hyprv-ui.md`, therefore no longer breaks SUPER+T. Test: `tests/desktop_cli.rs`.


### Integration CI — 2026-10-23T12:46:28+00:00 — production release staging failed

Local, non-installing `packaging/build-release.sh` exited 1. See `docs/verify/raw/ci/round-14/stage-release.log`. Build output:

```text
cargo build --manifest-path /home/wilf/.config/HyprV/rshell/Cargo.toml --locked --release --target-dir /home/wilf/.config/HyprV/rshell/target --config /home/wilf/.config/HyprV/rshell/packaging/release.toml -p hyprv-shell -p hyprv-privhelper --bins
Stage hyprv-shell, hyprv-privhelper, hyprv-cooling-failsafe, build.json and the hash-manifested release/ (MANIFEST.sha256) under /home/wilf/.config/HyprV/rshell/packaging/dist
    Updating crates.io index
warning: failed to write cache, path: /home/wilf/.cargo/registry/index/index.crates.io-1949cf8c6b5b557f/.cache/li/bz/libz-sys, error: Permission denied (os error 13)
   Compiling hyprv-ui v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui)
   Compiling hyprv-svc-desktop v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop)
   Compiling hyprv-widgets v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-widgets)
   Compiling hyprv-ui-bar v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-bar)
   Compiling hyprv-ui-island v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-island)
   Compiling hyprv-ui-panels v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-panels)
   Compiling hyprv-shell v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-shell)
    Finished `release` profile [optimized] target(s) in 2m 00s
sources changed during build; refusing an ambiguous receipt, rerun

```


Integration CI clarification (12:54 UTC): round 14 normal test executables completed successfully before `cargo test --workspace` failed at shell rustdoc compilation. The first E0463 errors are inability to resolve `hyprv_ui_panels`, `hyprv_ui_bar` and `hyprv_ui_island` rlibs; the later missing model imports/types are cascading, not eleven independent API defects. Source and shared build artifacts were changing during this run. Preserve the doctest failure as unresolved until a stable complete workspace command passes; log `docs/verify/raw/ci/round-14/tests.log`.


### Integration CI — 2026-10-23T12:55:37+00:00 — clippy: this method chain can be written more clearly with `if .. else ..`

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-shell/src/assembly.rs:198:23`.

`crates/hyprv-shell/src/assembly.rs`: mtime 2026-10-23T12:54:44+00:00; SHA-256 `15d8dc1804d1f57ec2c68bbd822d793717eadfbfa5019740d51c9a9a5a987417`; changed during command: True.

```text
error: this method chain can be written more clearly with `if .. else ..`
   --> crates/hyprv-shell/src/assembly.rs:198:23
    |
198 |           let startup = (!hub.is_mock())
    |  _______________________^
199 | |             .then(|| hub.desktop.startup.state().approval_path.is_some())
200 | |             .unwrap_or(false)
    | |_____________________________^ help: try: `if !hub.is_mock() { hub.desktop.startup.state().approval_path.is_some() } else { false }`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.95.0/index.html#obfuscated_if_else
    = note: `-D clippy::obfuscated-if-else` implied by `-D warnings`
    = help: to override `-D warnings` add `#[allow(clippy::obfuscated_if_else)]`


```

Full log: `docs/verify/raw/ci/round-15/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T12:55:37+00:00 — clippy: struct update has no effect, all the fields in the struct have already been specified

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-shell/src/assembly.rs:1461:27`.

`crates/hyprv-shell/src/assembly.rs`: mtime 2026-10-23T12:54:44+00:00; SHA-256 `15d8dc1804d1f57ec2c68bbd822d793717eadfbfa5019740d51c9a9a5a987417`; changed during command: True.

```text
error: struct update has no effect, all the fields in the struct have already been specified
    --> crates/hyprv-shell/src/assembly.rs:1461:27
     |
1461 |                         ..Default::default()
     |                           ^^^^^^^^^^^^^^^^^^
     |
     = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.95.0/index.html#needless_update
     = note: `-D clippy::needless-update` implied by `-D warnings`
     = help: to override `-D warnings` add `#[allow(clippy::needless_update)]`


```

Full log: `docs/verify/raw/ci/round-15/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T12:57:41+00:00 — production release staging failed

Local, non-installing `packaging/build-release.sh` exited 1. See `docs/verify/raw/ci/round-15/stage-release.log`. Build output:

```text
cargo build --manifest-path /home/wilf/.config/HyprV/rshell/Cargo.toml --locked --release --target-dir /home/wilf/.config/HyprV/rshell/target --config /home/wilf/.config/HyprV/rshell/packaging/release.toml -p hyprv-shell -p hyprv-privhelper --bins
Stage hyprv-shell, hyprv-privhelper, hyprv-cooling-failsafe, build.json and the hash-manifested release/ (MANIFEST.sha256) under /home/wilf/.config/HyprV/rshell/packaging/dist
    Updating crates.io index
warning: failed to write cache, path: /home/wilf/.cargo/registry/index/index.crates.io-1949cf8c6b5b557f/.cache/li/bz/libz-sys, error: Permission denied (os error 13)
   Compiling hyprv-ui v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui)
   Compiling hyprv-widgets v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-widgets)
   Compiling hyprv-ui-island v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-island)
   Compiling hyprv-ui-panels v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-panels)
   Compiling hyprv-ui-bar v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-bar)
   Compiling hyprv-shell v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-shell)
    Finished `release` profile [optimized] target(s) in 2m 03s
sources changed during build; refusing an ambiguous receipt, rerun

```


### Integration CI — 2026-10-23T13:09:31+00:00 — production release staging failed

Local, non-installing `packaging/build-release.sh` exited 1. See `docs/verify/raw/ci/round-16/stage-release.log`. Build output:

```text
cargo build --manifest-path /home/wilf/.config/HyprV/rshell/Cargo.toml --locked --release --target-dir /home/wilf/.config/HyprV/rshell/target --config /home/wilf/.config/HyprV/rshell/packaging/release.toml -p hyprv-shell -p hyprv-privhelper --bins
Stage hyprv-shell, hyprv-privhelper, hyprv-cooling-failsafe, build.json and the hash-manifested release/ (MANIFEST.sha256) under /home/wilf/.config/HyprV/rshell/packaging/dist
    Updating crates.io index
warning: failed to write cache, path: /home/wilf/.cargo/registry/index/index.crates.io-1949cf8c6b5b557f/.cache/li/bz/libz-sys, error: Permission denied (os error 13)
   Compiling hyprv-ui v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui)
   Compiling hyprv-widgets v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-widgets)
   Compiling hyprv-ui-island v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-island)
   Compiling hyprv-ui-panels v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-panels)
   Compiling hyprv-ui-bar v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-bar)
   Compiling hyprv-shell v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-shell)
    Finished `release` profile [optimized] target(s) in 1m 59s
sources changed during build; refusing an ambiguous receipt, rerun

```

### Soak → host owner: SOAK-1 [P3 investigation, renderer primary] (2026-10-23)

[Completed 62-minute report](../verify/soak.md), primary [renderer follow-up](hyprv-ui.md). 534 successful steps, 20 private output-add/remove cycles, 37 verified island expansions, 18 stats-popup opens; exit 0. No fd/thread/kernel-timer accumulation or confirmed prolonged stall. Matching phases retained 220 KiB private dirty and 5 MiB NVML graphics accounting; RSS declined. Please help attribute retained host mounts/scenes versus renderer/driver caches using bounded diagnostics; **not a confirmed leak or causal source attribution**. Whole UI-update/presentation timing and logical timers are not exported, so report labels the measured EGL submission and kernel-timer proxies explicitly. Artifact hash and isolated repro are retained; later workspace builds are not certified by this run.


### Integration CI — 2026-10-23T13:17:45+00:00 — production release staging failed

Local, non-installing `packaging/build-release.sh` exited 1. See `docs/verify/raw/ci/round-17/stage-release.log`. Build output:

```text
cargo build --manifest-path /home/wilf/.config/HyprV/rshell/Cargo.toml --locked --release --target-dir /home/wilf/.config/HyprV/rshell/target --config /home/wilf/.config/HyprV/rshell/packaging/release.toml -p hyprv-shell -p hyprv-privhelper --bins
Stage hyprv-shell, hyprv-privhelper, hyprv-cooling-failsafe, build.json and the hash-manifested release/ (MANIFEST.sha256) under /home/wilf/.config/HyprV/rshell/packaging/dist
    Updating crates.io index
warning: failed to write cache, path: /home/wilf/.cargo/registry/index/index.crates.io-1949cf8c6b5b557f/.cache/li/bz/libz-sys, error: Permission denied (os error 13)
   Compiling hyprv-ui v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui)
   Compiling hyprv-widgets v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-widgets)
   Compiling hyprv-ui-bar v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-bar)
   Compiling hyprv-ui-island v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-island)
   Compiling hyprv-ui-panels v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-panels)
   Compiling hyprv-shell v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-shell)
    Finished `release` profile [optimized] target(s) in 2m 00s
sources changed during build; refusing an ambiguous receipt, rerun

```


### Integration CI — 2026-10-23T13:30:44+00:00 — production release staging failed

Local, non-installing `packaging/build-release.sh` exited 1. See `docs/verify/raw/ci/round-18/stage-release.log`. Build output:

```text
cargo build --manifest-path /home/wilf/.config/HyprV/rshell/Cargo.toml --locked --release --target-dir /home/wilf/.config/HyprV/rshell/target --config /home/wilf/.config/HyprV/rshell/packaging/release.toml -p hyprv-shell -p hyprv-privhelper --bins
Stage hyprv-shell, hyprv-privhelper, hyprv-cooling-failsafe, build.json and the hash-manifested release/ (MANIFEST.sha256) under /home/wilf/.config/HyprV/rshell/packaging/dist
    Updating crates.io index
warning: failed to write cache, path: /home/wilf/.cargo/registry/index/index.crates.io-1949cf8c6b5b557f/.cache/li/bz/libz-sys, error: Permission denied (os error 13)
   Compiling hyprv-ui v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui)
   Compiling hyprv-widgets v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-widgets)
   Compiling hyprv-ui-island v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-island)
   Compiling hyprv-ui-panels v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-panels)
   Compiling hyprv-ui-bar v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-bar)
   Compiling hyprv-shell v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-shell)
    Finished `release` profile [optimized] target(s) in 2m 00s
sources changed during build; refusing an ambiguous receipt, rerun

```


### Integration CI — 2026-10-23T13:39:36+00:00 — production release staging failed

Local, non-installing `packaging/build-release.sh` exited 1. See `docs/verify/raw/ci/round-19/stage-release.log`. Build output:

```text
cargo build --manifest-path /home/wilf/.config/HyprV/rshell/Cargo.toml --locked --release --target-dir /home/wilf/.config/HyprV/rshell/target --config /home/wilf/.config/HyprV/rshell/packaging/release.toml -p hyprv-shell -p hyprv-privhelper --bins
Stage hyprv-shell, hyprv-privhelper, hyprv-cooling-failsafe, build.json and the hash-manifested release/ (MANIFEST.sha256) under /home/wilf/.config/HyprV/rshell/packaging/dist
    Updating crates.io index
warning: failed to write cache, path: /home/wilf/.cargo/registry/index/index.crates.io-1949cf8c6b5b557f/.cache/li/bz/libz-sys, error: Permission denied (os error 13)
   Compiling hyprv-ui v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui)
   Compiling hyprv-widgets v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-widgets)
   Compiling hyprv-ui-island v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-island)
   Compiling hyprv-ui-panels v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-panels)
   Compiling hyprv-ui-bar v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-bar)
   Compiling hyprv-shell v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-shell)
    Finished `release` profile [optimized] target(s) in 2m 13s
sources changed during build; refusing an ambiguous receipt, rerun

```

## From spec-panels (full-screen parity QA, 2026-10-23): `--nested-test` vs parity/fullscreen/reference — 0/66 pass

Evidence: `docs/verify/parity-fullscreen.md`, `docs/verify/raw/parity-fullscreen/` (compare.json/residual.json, heat/, zoom/). References: `parity/fullscreen/reference` (original shell, nested Hyprland, whole output).

1. **T1 fixture loader drops tray icons** (blocks every image). `src/fixture.rs:204` writes fixture `icon` into `TrayItem.icon`, but `assets::themed_icon` resolves only `icon_name`, so every replayed item draws its letter fallback. Fill `icon_name` too, as the SNI provider does.
2. **I1 album art never reaches the capture.** media-playing, island-expanded and control show the ♪ placeholder instead of the `media-status.sh` cover (`@ASSETS@/cover-*.png`). Await the `art_receive` decode before capturing (parity-host awaits async images), and check the `@ASSETS@` resolution in replay.
3. **H1 replay bound.** All 6 tray jobs (tray-menu, tray-submenu, tray-overflow × both themes) end with `fixture replay exceeded 25-second bound`. tray-menu light and tray-overflow dark captured before the bound hit; the rest captured nothing. The menu itself renders correctly. Make the bound virtual-time based or larger, and report the op it stalled on (the loop waits while any surface has zero size).
4. **A1 (shared with panels) popup animation start.** control-open-close open-t0060 still equals the closed frame, while the original already shows the opening panel and backdrop (IoU 0.558). The original starts its animation at the first frame after the input. Check that popup surface creation/configure in replay doesn't delay the timeline's start.
5. Optional: support `targets_by_theme` (the wifi dark target is [2846, 27] vs light [2846, 29]); the light point still hits the icon today.


### Integration CI — 2026-10-23T13:57:22+00:00 — tests failed without compiler diagnostic

Command `cargo test --workspace --locked --message-format=json` exited -15. See `docs/verify/raw/ci/round-20/tests.log`. Tail:

```text
ts::over_100_percent_is_reported ... ok
test audio::model::tests::balance_is_preserved ... ok
test audio::model::tests::set_increase_decrease_limits ... ok
test audio::model::tests::state_is_cubic_and_rounded ... ok
test audio::model::tests::unavailable_without_default_or_props ... ok
test audio::model::tests::mute_toggle_and_route_target ... ok
test cider::tests::artwork_formatting ... ok
test cider::tests::api_payload_shapes ... ok
test cider::tests::scoring_rules ... ok
test audio::model::tests::write_objects_serialize ... ok
test cider::tests::mapping_prefers_attributes_and_python_or ... ok
test common::tests::backoff_grows_and_caps ... ok
test common::tests::single_line_collapses ... ok
test mpris::model::tests::anchor_extrapolates_only_while_playing_and_clamps ... ok
test mpris::model::tests::flatten_basic_and_cached_paused_content ... ok
test mpris::model::tests::metadata_parsing ... ok
test mpris::model::tests::cider_merge_rules_and_key_validity ... ok
test mpris::model::tests::selection_priority ... ok
test cider::tests::varints_and_cache_scan ... ok
test mpris::tests::player_name_filter ... ok
test notifications::tests::waybar_format_matches_swaync_client ... ok
test spectrum::cava::tests::monstercat_matches_c_semantics ... ok
test mpris::tests::property_application ... ok
test tray::model::tests::item_props_and_icon_string ... ok
test tray::model::tests::keys ... ok
test tray::model::tests::mnemonics ... ok
test tray::model::tests::layout_depth_is_bounded ... ok
test tray::model::tests::layout_parsing ... ok
test cider::tests::http_lookup_against_fake_server ... ok
test tray::model::tests::pixmaps_are_validated ... ok
test spectrum::tests::closing_demand_stops_an_active_capture_through_run_sampled ... ok
test spectrum::tests::dropping_capture_quits_its_loop_and_timer ... ok
test spectrum::cava::tests::plan_and_mode ... ok
test spectrum::cava::tests::overflow_discards_like_cava ... ok
test spectrum::cava::tests::silence_and_tone ... ok

test result: ok. 36 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.30s

     Running tests/fake_bus.rs (target/debug/deps/fake_bus-1eb00357bd29a707)

running 5 tests
test mpris_discovery_signals_actions_and_owner_loss ... ok
test tray_serves_watcher_tracks_items_and_menus ... ok
test mpris_read_only_refuses_actions ... ok
test swaync_snapshot_signal_action_and_loss ... ok
test tray_read_only_never_owns_watcher ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s

     Running tests/live_readonly.rs (target/debug/deps/live_readonly-613674410c2e3874)

running 3 tests
test live_audio_observes_default_sink ... ignored, needs the live PipeWire session
test live_mpris_notifications_tray ... ignored, needs the live session bus
test live_spectrum_demand_lifecycle ... ignored, needs live PipeWire; creates a passive capture stream only while running

test result: ok. 0 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/review_regressions.rs (target/debug/deps/review_regressions-87bba47e30a06dd2)

running 4 tests
test media4_signal_from_non_owner_is_ignored ... ok
test media5_snapshot_from_departed_owner_is_discarded ... ok
test media2_late_position_read_cannot_undo_seeked ... ok
test media3_layout_revision_never_regresses_and_settles_on_latest ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.59s

     Running unittests src/lib.rs (target/debug/deps/hyprv_svc_net-1aa1920f084da112)

running 34 tests
test dh::tests::aes_cbc_matches_nist_sp800_38a ... ok
test bluetooth::tests::qml_display_fields_serialize ... ok
test bluetooth::tests::object_manager_selection_fields_sort_and_removal ... ok
test network::tests::net7_new_profile_secret_is_system_owned_like_nmcli ... ok
test bluetooth::deadline_tests::scan_stops_at_four_seconds_without_idle_timer ... ok
test bluetooth::deadline_tests::pairing_timeout_cancels_provider_and_prompt ... ok
test network::tests::only_agent_owned_secrets_are_stored ... ok
test network::tests::not_required_secret_is_not_requested ... ok
test network::tests::qml_fields_are_camel_case_and_secrets_are_not_state ... ok
test network::tests::open_psk_sae_wep_and_enterprise_profiles ... ok
test network::tests::explicit_reject_is_user_canceled_and_timeout_is_no_secrets ... ok
test network::tests::net4_not_saved_secret_is_never_served_from_keyring ... ok
test network::tests::net1_secret_buffers_are_zeroized ... ok
test network::tests::strongest_ap_and_active_priority_preserve_raw_ssid_identity ... ok
test rfkill::tests::change_delete_and_change_all ... ok
test prompts::tests::cancelled_future_removes_prompt_and_display_updates_coalesce ... ok
test prompts::tests::response_cancellation_and_redaction ... ok
test network::tests::staged_secret_answers_next_request_once ... ok
test network::tests::secrets_noninteractive_keyring_and_new_secret_prompt ... ok
test dh::tests::both_sides_derive_the_same_key_and_roundtrip ... ok
test network::tests::save_secrets_syncs_ownership ... ok
test bluetooth::agent_tests::pin_code_reply_roundtrip ... ok
test network::tests::net3_cancelled_request_reports_agent_canceled ... ok
test network::tests::net6_empty_password_keeps_saved_secret ... ok
test network::tests::net5_existing_profile_keeps_secret_ownership ... ok
test keyring::tests::net8_net9_networkmanager_schema_over_encrypted_session ... ok
test bluetooth::agent_tests::control_foreign_caller_is_rejected ... ok
test network::tests::net2_cancel_before_prompt_cancels_request ... ok
test network::tests::control_reregisters_after_nm_restart ... ok
test bluetooth::agent_tests::control_authorization_requires_explicit_consent ... ok
dbus-daemon[1834869]: [session uid=1000 pid=1834869 pidfd=4] Unable to reload configuration: Failed to open "/tmp/hyprv-net-bt-start-1834641/services-1834641/bus.conf": No such file or directory
dbus-daemon[1834869]: Unable to reload configuration: Failed to open "/tmp/hyprv-net-bt-start-1834641/services-1834641/bus.conf": No such file or directory
dbus-daemon[1834869]: [session uid=1000 pid=1834869 pidfd=4] Unable to reload configuration: Failed to open "/tmp/hyprv-net-bt-start-1834641/services-1834641/bus.conf": No such file or directory
dbus-daemon[1834869]: Unable to reload configuration: Failed to open "/tmp/hyprv-net-bt-start-1834641/services-1834641/bus.conf": No such file or diretest bluetooth::agent_tests::startup_without_daemon_does_not_activate ... ok
dbus-daemon[1834977]: [session uid=1000 pid=1834977 pidfd=4] Unable to reload configuration: Failed to open "/tmp/hyprv-net-nm-act-1834641/services-1834641/bus.conf": No such file or directory
test network::tests::nm_stop_does_not_reactivate_networkmanager ... ok
dbus-daemon[1834868]: [session uid=1000 pid=1834868 pidfd=4] Unable to reload configuration: Failed to open "/tmp/hyprv-net-bt-act-1834641/services-1834641/bus.conf": No such file or directory
dbus-daemon[1834868]: Unable to reload configuration: Failed to open "/tmp/hyprv-net-bt-act-1834641/services-1834641/bus.conf": No such file or directory
dbus-daemon[1834868]: [session uid=1000 pid=1834868 pidfd=4] Unable to reload configuration: Failed to open "/tmp/hyprv-net-bt-act-1834641/services-1834641/bus.conf": No such file or directory
dbus-daemon[1834868]: Unable to reload configuration: Failed to open "/tmp/hyprv-net-bt-act-1834641/services-1834641/bus.conf": No such file or directory
dbus-daemon[1834868]: [session uid=1000 pid=1834868 pidfd=4] Unable to reload configuration: Failed to open "/tmp/hyprv-net-bt-act-1834641/services-1834641/bus.conf": No such file or directorytest bluetooth::agent_tests::bt1_daemon_stop_does_not_reactivate_bluez ... ok
test network::tests::control_foreign_caller_is_rejected has been running for over 60 seconds

INTEGRATION CI: stopped own test command after >10 minutes: network::tests::control_foreign_caller_is_rejected did not finish; see docs/requests/hyprv-svc-net.md. No PASS established.

```

Source fingerprints at failure:
`crates/hyprv-shell/src/lib.rs`: {'sha256': 'f180c772bf59361f0b9cfbb5052afa4b7ae86d73e22eba23be36633c773c81d8', 'mtime': '2026-10-23T13:34:59+00:00'}
`crates/hyprv-shell/src/main.rs`: {'sha256': 'ed6d3e3bc85a97349c3e4797df12e7abb8ab9e34edc8f0fed3bc02de032e8c0e', 'mtime': '2026-10-23T13:01:35+00:00'}
`crates/hyprv-shell/tests/cli.rs`: {'sha256': 'ffd41a82384a5f5878d8d80e094e1d0ceba73b4525fbf7e62ef18e29ba3a8734', 'mtime': '2026-10-23T13:50:11+00:00'}
`crates/hyprv-shell/tests/desktop_cli.rs`: {'sha256': '252d1f766040f21325a7bc37aafb666ac68197fb93ee3e2a41c264ce68561ea3', 'mtime': '2026-10-23T13:34:59+00:00'}
`crates/hyprv-shell/tests/desktop_ipc.rs`: {'sha256': '686e542a7cb11962f2912f93a79e68f27ddc6d9f04a3bdea4956145ac83fdb32', 'mtime': '2026-10-23T13:34:59+00:00'}
`crates/hyprv-shell/tests/no_system_bus.rs`: {'sha256': 'e68d53b76657d2f2cb14f4cb2e22e7c6ec4afd007d955be8df880b087e6835b2', 'mtime': '2026-10-23T13:34:59+00:00'}


### Integration CI — 2026-10-23T13:59:57+00:00 — production release staging failed

Local, non-installing `packaging/build-release.sh` exited 1. See `docs/verify/raw/ci/round-20/stage-release.log`. Build output:

```text
cargo build --manifest-path /home/wilf/.config/HyprV/rshell/Cargo.toml --locked --release --target-dir /home/wilf/.config/HyprV/rshell/target --config /home/wilf/.config/HyprV/rshell/packaging/release.toml -p hyprv-shell -p hyprv-privhelper --bins
Stage hyprv-shell, hyprv-privhelper, hyprv-cooling-failsafe, build.json and the hash-manifested release/ (MANIFEST.sha256) under /home/wilf/.config/HyprV/rshell/packaging/dist
    Updating crates.io index
warning: failed to write cache, path: /home/wilf/.cargo/registry/index/index.crates.io-1949cf8c6b5b557f/.cache/li/bz/libz-sys, error: Permission denied (os error 13)
   Compiling hyprv-ui v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui)
   Compiling hyprv-widgets v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-widgets)
   Compiling hyprv-ui-island v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-island)
   Compiling hyprv-ui-bar v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-bar)
   Compiling hyprv-ui-panels v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-panels)
   Compiling hyprv-shell v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-shell)
    Finished `release` profile [optimized] target(s) in 2m 31s
sources changed during build; refusing an ambiguous receipt, rerun

```


### Integration CI — 2026-10-23T14:10:27+00:00 — clippy: struct update has no effect, all the fields in the struct have already been specified

Reproduce: `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings`. Exit 101.

Primary location(s): `crates/hyprv-shell/src/fixture.rs:349:19`.

`crates/hyprv-shell/src/fixture.rs`: mtime 2026-10-23T14:08:16+00:00; SHA-256 `57a660678d7dd8c497f5de25d4f3d0198a50460ebcac5b0e6be98afa6404b633`; changed during command: True.

```text
error: struct update has no effect, all the fields in the struct have already been specified
   --> crates/hyprv-shell/src/fixture.rs:349:19
    |
349 |                 ..Default::default()
    |                   ^^^^^^^^^^^^^^^^^^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.95.0/index.html#needless_update
    = note: `-D clippy::needless-update` implied by `-D warnings`
    = help: to override `-D warnings` add `#[allow(clippy::needless_update)]`


```

Full log: `docs/verify/raw/ci/round-21/clippy.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T14:28:20+00:00 — clippy failed without compiler diagnostic

Command `cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings` exited 124. See `docs/verify/raw/ci/round-23/clippy.log`. Tail:

```text
.config/HyprV/rshell/crates/hyprv-ui-bar/tests/parity.rs","edition":"2021","doc":false,"doctest":false,"test":true},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":true},"features":[],"filenames":["/home/wilf/.config/HyprV/rshell/target/debug/deps/libparity-0b9aa6c7595576c9.rmeta"],"executable":null,"fresh":false}
{"reason":"compiler-artifact","package_id":"path+file:///home/wilf/.config/HyprV/rshell/crates/hyprv-ui-bar#0.1.0","manifest_path":"/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-bar/Cargo.toml","target":{"kind":["example"],"crate_types":["bin"],"name":"gl_stress","src_path":"/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-bar/examples/gl_stress.rs","edition":"2021","doc":false,"doctest":false,"test":false},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":false},"features":[],"filenames":["/home/wilf/.config/HyprV/rshell/target/debug/examples/libgl_stress-c521537f11e4ff60.rmeta"],"executable":null,"fresh":false}
{"reason":"compiler-artifact","package_id":"path+file:///home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop#0.1.0","manifest_path":"/home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop/Cargo.toml","target":{"kind":["example"],"crate_types":["bin"],"name":"proc_probe_tmp","src_path":"/home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop/examples/proc_probe_tmp.rs","edition":"2021","doc":false,"doctest":false,"test":false},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":false},"features":[],"filenames":["/home/wilf/.config/HyprV/rshell/target/debug/examples/libproc_probe_tmp-4ab82c912892b74f.rmeta"],"executable":null,"fresh":false}
{"reason":"compiler-artifact","package_id":"path+file:///home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop#0.1.0","manifest_path":"/home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop/Cargo.toml","target":{"kind":["lib"],"crate_types":["lib"],"name":"hyprv_svc_desktop","src_path":"/home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop/src/lib.rs","edition":"2021","doc":true,"doctest":true,"test":true},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":true},"features":[],"filenames":["/home/wilf/.config/HyprV/rshell/target/debug/deps/libhyprv_svc_desktop-f3b3e064035d1c51.rmeta"],"executable":null,"fresh":false}
    Checking hyprv-svc-probe v0.1.0 (/home/wilf/.config/HyprV/rshell/crates/hyprv-svc-probe)
{"reason":"compiler-artifact","package_id":"path+file:///home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop#0.1.0","manifest_path":"/home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop/Cargo.toml","target":{"kind":["example"],"crate_types":["bin"],"name":"agent_bench","src_path":"/home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop/examples/agent_bench.rs","edition":"2021","doc":false,"doctest":false,"test":false},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":false},"features":[],"filenames":["/home/wilf/.config/HyprV/rshell/target/debug/examples/libagent_bench-4e3d8e30d775fff8.rmeta"],"executable":null,"fresh":false}
{"reason":"compiler-artifact","package_id":"path+file:///home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop#0.1.0","manifest_path":"/home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop/Cargo.toml","target":{"kind":["example"],"crate_types":["bin"],"name":"agent_cost","src_path":"/home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop/examples/agent_cost.rs","edition":"2021","doc":false,"doctest":false,"test":false},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":false},"features":[],"filenames":["/home/wilf/.config/HyprV/rshell/target/debug/examples/libagent_cost-3040415d5ac4ee5a.rmeta"],"executable":null,"fresh":false}
{"reason":"compiler-artifact","package_id":"path+file:///home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop#0.1.0","manifest_path":"/home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop/Cargo.toml","target":{"kind":["bin"],"crate_types":["bin"],"name":"hyprv-agent-hook","src_path":"/home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop/src/bin/hyprv-agent-hook.rs","edition":"2021","doc":true,"doctest":false,"test":true},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":false},"features":[],"filenames":["/home/wilf/.config/HyprV/rshell/target/debug/deps/libhyprv_agent_hook-5ebc2dff604bdb68.rmeta"],"executable":null,"fresh":false}
{"reason":"compiler-artifact","package_id":"path+file:///home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop#0.1.0","manifest_path":"/home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop/Cargo.toml","target":{"kind":["bin"],"crate_types":["bin"],"name":"hyprv-agent-hook","src_path":"/home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop/src/bin/hyprv-agent-hook.rs","edition":"2021","doc":true,"doctest":false,"test":true},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":true},"features":[],"filenames":["/home/wilf/.config/HyprV/rshell/target/debug/deps/libhyprv_agent_hook-5509c3e7bc49c5e0.rmeta"],"executable":null,"fresh":false}
{"reason":"compiler-artifact","package_id":"path+file:///home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop#0.1.0","manifest_path":"/home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop/Cargo.toml","target":{"kind":["example"],"crate_types":["bin"],"name":"idle_probe","src_path":"/home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop/examples/idle_probe.rs","edition":"2021","doc":false,"doctest":false,"test":false},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":false},"features":[],"filenames":["/home/wilf/.config/HyprV/rshell/target/debug/examples/libidle_probe-70477dd1f550d1e4.rmeta"],"executable":null,"fresh":false}
{"reason":"compiler-artifact","package_id":"path+file:///home/wilf/.config/HyprV/rshell/crates/hyprv-ui-bar#0.1.0","manifest_path":"/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-bar/Cargo.toml","target":{"kind":["example"],"crate_types":["bin"],"name":"reference","src_path":"/home/wilf/.config/HyprV/rshell/crates/hyprv-ui-bar/examples/reference.rs","edition":"2021","doc":false,"doctest":false,"test":false},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":false},"features":[],"filenames":["/home/wilf/.config/HyprV/rshell/target/debug/examples/libreference-e356d812ec3be186.rmeta"],"executable":null,"fresh":false}
{"reason":"compiler-artifact","package_id":"path+file:///home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop#0.1.0","manifest_path":"/home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop/Cargo.toml","target":{"kind":["test"],"crate_types":["bin"],"name":"live_readonly","src_path":"/home/wilf/.config/HyprV/rshell/crates/hyprv-svc-desktop/tests/live_readonly.rs","edition":"2021","doc":false,"doctest":false,"test":true},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":true},"features":[],"filenames":["/home/wilf/.config/HyprV/rshell/target/debug/deps/liblive_readonly-85559ab44d760aac.rmeta"],"executable":null,"fresh":false}
{"reason":"compiler-artifact","package_id":"path+file:///home/wilf/.config/HyprV/rshell/crates/hyprv-svc-probe#0.1.0","manifest_path":"/home/wilf/.config/HyprV/rshell/crates/hyprv-svc-probe/Cargo.toml","target":{"kind":["bin"],"crate_types":["bin"],"name":"hyprv-svc-probe","src_path":"/home/wilf/.config/HyprV/rshell/crates/hyprv-svc-probe/src/main.rs","edition":"2021","doc":true,"doctest":false,"test":true},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":false},"features":["default","desktop","media","net","power","sys"],"filenames":["/home/wilf/.config/HyprV/rshell/target/debug/deps/libhyprv_svc_probe-76c1256d7f375351.rmeta"],"executable":null,"fresh":false}

```

Source fingerprints at failure:



CI deadline clarification (14:29 UTC): round 23 Clippy exit124 was termination at the requested two-hour limit, with no emitted compiler error. The automated fallback shell failure attribution/request is withdrawn; this phase establishes no new pass. See ci.md/latest.json.

Engine reconciliation checkpoint: cause4 retained bar API is integrated; battery
native open/close1.151s vsprior~55s. Ordinary owned tests and strict Clippy pass.
A concurrent SVG regression string in assets.rs used a one-hash raw delimiter
around fill="#ff0000"; corrected only its delimiter to r##...## (test preserved).
Private full E2E passed196 interaction cases but second output arrived after the
fixed900ms wait, cascading5 lifecycle failures. Replaced that wait with bounded
polling for actual host output registration. Targeted six output cases + quit +
disconnect/restart now9/9 PASS; full204 rerun is underway. No live-session action.

Output test follow-up: the full native interaction run still leaves the newly
created nested Wayland output unconfigured until the later explicit reorder rule,
although the isolated six-case run passes. The10s bounded wait confirmed this is
not merely slow output delivery. Registering a deterministic3072x1728@60 monitor
rule for E2E-SECOND through the PRIVATE compositor before its native hotplug;
no fake host output injection or production retry change. Full run repeated.


### Snapshot CI — 2026-10-23T14:38:42+00:00 — probe tests

REPRODUCIBLE: failed twice on same snapshot. Snapshot `/tmp/hyprv-svc-sys-ci-20261023T142927Z/round-02-20261023T143232Z/rshell`; fingerprint `0fbe42ab02e66c2bf330d872acb7f74e135cceef1b5c912968920f0ad6249f47`. Concurrent live edits cannot change this source copy.

Command: `['/usr/bin/python3', '-m', 'unittest', 'discover', '-s', 'crates/hyprv-svc-probe/scripts', '-p', 'test*.py']`. Attempts: `[{'code': 1, 'log': 'round-02-20261023T143232Z/07-probe-tests-1.log'}, {'code': 1, 'log': 'round-02-20261023T143232Z/07-probe-tests-2.log'}]`. Locations: .



```text
E...
======================================================================
ERROR: test_greeting_is_not_a_method_reply (test_verify.CalendarCorrelation.test_greeting_is_not_a_method_reply)
----------------------------------------------------------------------
Traceback (most recent call last):
  File "/tmp/hyprv-svc-sys-ci-20261023T142927Z/round-02-20261023T143232Z/rshell/crates/hyprv-svc-probe/scripts/test_verify.py", line 33, in test_greeting_is_not_a_method_reply
    server.bind(str(path));server.listen(1)
    ~~~~~~~~~~~^^^^^^^^^^^
OSError: AF_UNIX path too long

----------------------------------------------------------------------
Ran 4 tests in 0.001s

FAILED (errors=1)
```

Logs relative to `docs/verify/raw/ci-snapshots/`. No causal commit inferred.

Snapshot CI correction (14:40 UTC): preceding probe Python socket failure belongs to the probe test/harness path-length interaction, not hyprv-shell. It reproduced twice on the unchanged snapshot because its absolute AF_UNIX pathname exceeds 108 bytes. Snapshot roots shortened for subsequent rounds; no production/test semantic change. The shell attribution is withdrawn.

Full-suite native pointer calibration: with only E2E-SECOND's explicit rule, the
wildcard-auto first monitor moves to x6144, so the desktop starts at x3072.
The virtual-pointer absolute extent assumed zero-origin; cursor evidence was
x6293 for the intended second-bar workspace. Explicitly pin BOTH initial outputs
(WAYLAND-1 at0, SECOND at3072) and assert host/native origins agree before input.
Later reorder still moves them through native IPC, then removal verifies the
open panel/owner cleanup. This is deterministic test setup, not a host-coordinate
workaround or synthetic action replay. Complete rerun underway; failed receipts
remain under target/functional-consolidated.

README audit: removed stale production-slot scaffolding/full-screen-reference-
missing claims; those sections now distinguish the compatibility Host. QA's
published parity/fullscreen/{fixtures,reference} and parity-fullscreen.md are linked.
The fixture harness default reference path now points at the published directory;
explicit missing-reference tests still use their own nonexistent path and fail closed.

Final complete native run:204/204 PASS in one receipt,
target/functional-consolidated-verified/manifest.json. Both native/host output
origins are calibrated before second-output input. Open panel removal, reorder,
scales1.5/2, IPC kill and compositor disconnect/restart all pass. Battery native
open/close1,186ms. docs/verify/e2e.md was regenerated by the real harness.
Incremental stats regression + ordinary owned tests160/160 pass (21 opt-in ignored).
README reference default now matches the published QA directory.

## Receipt: spec-panels fixed T1.1/T1.2b/T1.3, I1, I2, H1 in hyprv-shell (2026-10-23, user-assigned while engine is busy)

Evidence: `docs/verify/parity-fullscreen.md` (Run 2), `docs/verify/raw/parity-fullscreen/after-t1/`. All items re-read before editing. `cargo test -p hyprv-shell --lib`: 34/34; clippy `-D warnings` clean; touched files rustfmt-clean.
- `src/fixture.rs`:
  - tray items set `icon_name`, with `@asset:` mapped to an absolute path (T1.1);
  - island inputs carry `art_url` (I1);
  - the island spectrum falls back to the translator's stream;
  - fixture asset images are decoded once (`fixture_asset`), so the per-frame bar model no longer gives the menu a new image identity.

  Tests: `tray_items_carry_icon_name_for_theme_lookup`, `island_gets_art_url_and_the_streamed_spectrum`, `spectrum_needs_cava_like_audio_spectrum_process_running`.
- `src/fixture/translate.rs`: `parse_spectrum` (shell.qml:3258 rules) and `audio-spectrum.sh` replay, gated like `audioSpectrumProcess.running` (I2).
- `src/assets.rs`: `themed_icon` goes through `hyprv_widgets::icon_theme` (Qt QIconLoader port):
  - raster icons: Qt smooth downscale, never upscaled;
  - SVG icons: `QSvgIconEngine` sizing (`svg_icon`);
  - unresolvable names: Quickshell's missing-icon checker (T1.3); only an empty name returns None (letter).

  Tests: `unresolvable_icon_names_draw_quickshells_missing_icon_not_a_letter`, `raster_icons_are_downscaled_qt_style_but_never_upscaled`.
- `src/nested.rs`: the fixture child gets the live icon search roots (`HYPRV_ICON_SEARCH_PATHS`/`HYPRV_ICON_FALLBACK_PATHS`, read-only) like the reference shell.
- `src/replay.rs` (H1): the 25 s total cap became a 10 s stall bound naming the pending op and unconfigured surfaces, plus a budget of 25 s + 40 ms/op; `HYPRV_REPLAY_TRACE=1` traces ops and surface counts; `Op` derives Debug.
- Result: all 26/26 full-screen jobs complete. With the tray strip masked, 38/64 images pass. Open for you: R1 draw stage (see hyprv-widgets.md) and A1 timing (item 4 above).
- Note: run 1's 0.3–0.9 s/frame tray-menu slowdown disappeared with your concurrent hyprv-ui change (tray-menu now 0.64 s). perf/ptrace are blocked in my sandbox, so it isn't attributed. If it reappears, `HYPRV_REPLAY_TRACE` shows it.


### Snapshot CI — 2026-10-23T15:06:08+00:00 — workspace tests

REPRODUCIBLE: failed twice on same snapshot. Snapshot `/tmp/hci-sys/round-05-20261023T150010Z/rshell`; fingerprint `05e6b8b0ddad31af476cb42ccc5905a9cff1b10a3ef28f810a30f940fcae6621`. Concurrent live edits cannot change this source copy.

Command: `['cargo', 'test', '--locked', '--workspace', '--all-targets', '--message-format=json']`. Attempts: `[{'code': 101, 'log': 'round-05-20261023T150010Z/02-workspace-tests-1.log'}, {'code': 101, 'log': 'round-05-20261023T150010Z/02-workspace-tests-2.log'}]`. Locations: .



```text
    Updating crates.io index
warning: failed to write cache, path: /home/wilf/.cargo/registry/index/index.crates.io-1949cf8c6b5b557f/.cache/li/bz/libz-sys, error: Permission denied (os error 13)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.79s
     Running unittests src/lib.rs (/tmp/hyprv-svc-sys-ci-20261023T142927Z/target/debug/deps/hyprv_core-1a7c32b0ccb214ed)

running 16 tests
test ipc::tests::contract_validation ... ok
test config::tests::defaults_match_qml ... ok
test ipc::tests::desktop_contract_for_patched_hypr_scripts ... ok
test store::tests::publication_without_receivers_retains_latest ... ok
test store::tests::diffs_and_demand ... ok
test runtime::tests::last_owner_released_on_worker_thread_does_not_self_join ... ok
test service::tests::demand_transitions_and_shutdown ... ok
test service::tests::gated_demand_requires_subscribers_and_gate ... ok
test runtime::tests::explicit_shutdown_and_join ... ok
test ipc::tests::drop_removes_own_socket ... ok
test ipc::tests::drop_preserves_replacement_file ... ok
test ipc::tests::roundtrip_lock_and_cleanup ... ok
test ipc::tests::refuses_regular_socket_path ... ok
test config::tests::unknown_theme_metadata_is_ignored ... ok
test config::tests::watches_atomic_replacement_and_retains_partial_colors ... ok
test config::tests::watch_ignores_unrelated_sibling_but_sees_target_and_missing_parent ... ok

test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.41s

     Running unittests src/lib.rs (/tmp/hyprv-svc-sys-ci-20261023T142927Z/target/debug/deps/hyprv_privhelper-822ed4ee7fad53d0)

running 18 tests
test cooling::regression::adversarial_review::review_helper_critical_temperature_overrides_zero_cap ... ok
test cooling::tests::curve_is_capped_and_floored ... ok
test egpu::tests::strict_bdf ... ok
test cooling::regression::adversarial_lock_review::review_helper_user_cannot_replace_singleton_lock ... ok
test cooling::regression::protected_namespace_rejects_unsafe_mode_and_file_replacement ... ok
test cooling::regression::malformed_limits_are_atomic_and_bounded ... ok
test egpu::tests::plan_requires_proof_and_orders_chain ... ok
test cooling::controller_tests::symlinked_state_directory_is_rejected ... ok
test cooling::regression::adversarial_review::recovery_ignores_corrupt_state_and_held_controller_locks ... ok
test cooling::regression::adversarial_review::review_helper_one_channel_error_does_not_skip_other_channel ... ok
test cooling::regression::adversarial_review::review_helper_shutdown_restores_autonomous_cooling ... ok
test cooling::regression::adversarial_review::review_helper_bad_state_does_not_prevent_boot_cooling ... ok
test cooling::regression::adversarial_review::review_helper_untrusted_state_cannot_bypass_authorization ... ok
test cooling::regression::adversarial_review::review_helper_overrange_hot_sensor_does_not_choose_cold_sensor ... ok
test cooling::controller_tests::boot_desktop_saved_limits_and_missing_telemetry ... ok
test cooling::controller_tests::write_permission_failure_is_reported ... ok
test cooling::regression::adversarial_review::one_failed_sensor_cannot_be_hidden_by_a_cool_package ... ok
test cooling::controller_tests::singleton_identity_and_hotplug ... ok

test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.43s

error: No space left on device (os error 28)
```

Logs relative to `docs/verify/raw/ci-snapshots/`. No causal commit inferred.

## A1/A2 (parity-fullscreen) — Qt animation-driver clock in fixture replay (spec-bar, 2026-10-23)

Surgical change in `assembly.rs` + `fixture.rs`, at the user's request (A1: "fix against the
full-screen references").

**`AnimationDriver` (fixture replay only; production timing unchanged).** The references were
captured with `QSG_RENDER_LOOP=basic` and Qt's default timer-mode animation driver under the
virtual clock. While any animation runs, a 16 ms `Qt::PreciseTimer` ticks it and the scene shows
the state of the last tick. The timer starts when an animation starts from idle, and only moves
at fixture ticks.
- `Assembly::sync` keeps raw time while idle. While `animations_running()` (bars, islands,
  panels + calendar, popups), it advances only when raw ≥ deadline: `deadline += 16`, or
  `now + 16` when behind (QTimerInfoList).
- `Fixture::clock` carries that time into the `now` of the panel and bar models (`Fixture::time`
  stays raw for services and wall clock).
- Measured against the frozen references (nested run, light, tray strip T1 masked):

| frame | before | after |
|---|---|---|
| control-open-close open-t0060 | 11,572 px > 8 | 0 (max 4) |
| control-open-close close-t0060 | 152,421 px > 8 | 0 (max 4) |
| calendar-morph open-t0100 | 13,241 px > 8 | 0 (max 6, with the bar quirk below) |

- Residual: open-t0120 (2,102 px) and open-t0220 (416 px) are one reveal-clip edge row. The real
  shell's state there lags by about 1 ms and 7–8 ms. The driver model above doesn't reproduce
  those sub-frame lags; fitted against dense component frames.
- Why not in production: whether the live Quickshell uses the basic loop with this driver is
  unverified. The harness forces it for the virtual clock.

Perf follow-up: controlled-quiet60s release measurements are0.5487% with the
incremental bar before text-cache work,0.2160% after opened-face reuse, and0.2156%
on the final prepared-font build (historical0.4159%). Zero native hook events;
GPU surfaces verified6144x3456@2. A stale measurement harness needed the existing
nested-flush.c compositor-only shim and private KWin sizing rule; failed FALLBACK
geometry run was rejected. No live-session config or service write occurred.
Five fresh release battery processes: UI native release→first GPU buffer submit
median10.29ms/max11.56ms; preparation median2.33ms/max2.66ms.1.19s was a whole
functional reset/assert/open/close/wait transaction, not popup latency.
An E2E rerun aborted when stderr's filesystem ran out of space; retained logs and
removed only redundant own debug diagnostic executables (~2.28GB logical).
Fresh rerun follows after the final shared-font-data memory cleanup build.

Snapshot CI correction,15:09UTC: round05 storage exhaustion invalidates the automatic generic workspace/doctest failure attribution. The doctest retry PASS follows linker ENOSPC, so is infrastructure recovery, not source-test flakiness. Clippy retry was terminated deliberately for private-cache relocation and establishes no second source reproduction. Earlier round04 excessive_precision finding remains reproduced twice independently. Round05 timing failure seen only before storage exhaustion is unconfirmed in that snapshot. Raw logs preserved; no source regression is inferred from ENOSPC/interruption.


### Snapshot CI — 2026-10-23T15:14:38+00:00 — workspace tests

REPRODUCIBLE: failed twice on same snapshot. Snapshot `/tmp/hci-sys/round-06-20261023T151010Z/rshell`; fingerprint `197d6a2d09054145648827f74a6d32f37aa097a5b4d7a60976a36177bc68d27a`. Concurrent live edits cannot change this source copy.

Command: `['cargo', 'test', '--locked', '--workspace', '--all-targets', '--message-format=json']`. Attempts: `[{'code': 101, 'log': 'round-06-20261023T151010Z/02-workspace-tests-1.log'}, {'code': 101, 'log': 'round-06-20261023T151010Z/02-workspace-tests-2.log'}]`. Locations: .



```text
error: failed to write /tmp/hyprv-svc-sys-ci-20261023T142927Z/target/debug/examples/rustcdo1ebK/symbols.o: Disk quota exceeded (os error 122)


```

Logs relative to `docs/verify/raw/ci-snapshots/`. No causal commit inferred.


### Snapshot CI — 2026-10-23T15:14:48+00:00 — doctests

REPRODUCIBLE: failed twice on same snapshot. Snapshot `/tmp/hci-sys/round-06-20261023T151010Z/rshell`; fingerprint `197d6a2d09054145648827f74a6d32f37aa097a5b4d7a60976a36177bc68d27a`. Concurrent live edits cannot change this source copy.

Command: `['cargo', 'test', '--locked', '--workspace', '--doc', '--message-format=json']`. Attempts: `[{'code': 101, 'log': 'round-06-20261023T151010Z/03-doctests-1.log'}, {'code': 101, 'log': 'round-06-20261023T151010Z/03-doctests-2.log'}]`. Locations: .



```text
error: failed to write to `/tmp/hyprv-svc-sys-ci-20261023T142927Z/target/debug/deps/rmetaLAUpbg/full.rmeta`: Disk quota exceeded (os error 122)


```

Logs relative to `docs/verify/raw/ci-snapshots/`. No causal commit inferred.


### Snapshot CI — 2026-10-23T15:14:50+00:00 — clippy

REPRODUCIBLE: failed twice on same snapshot. Snapshot `/tmp/hci-sys/round-06-20261023T151010Z/rshell`; fingerprint `197d6a2d09054145648827f74a6d32f37aa097a5b4d7a60976a36177bc68d27a`. Concurrent live edits cannot change this source copy.

Command: `['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '--message-format=json', '--', '-D', 'warnings']`. Attempts: `[{'code': 101, 'log': 'round-06-20261023T151010Z/04-clippy-1.log'}, {'code': 101, 'log': 'round-06-20261023T151010Z/04-clippy-2.log'}]`. Locations: .



```text
    Updating crates.io index
warning: failed to write cache, path: /home/wilf/.cargo/registry/index/index.crates.io-1949cf8c6b5b557f/.cache/li/bz/libz-sys, error: Permission denied (os error 13)
    Checking hyprv-core v0.1.0 (/tmp/hci-sys/round-06-20261023T151010Z/rshell/crates/hyprv-core)
error: failed to write `/tmp/hyprv-svc-sys-ci-20261023T142927Z/target/debug/.fingerprint/hyprv-core-12bb50285892b3e5/invoked.timestamp`

Caused by:
  Disk quota exceeded (os error 122)
warning: build failed, waiting for other jobs to finish...
```

Logs relative to `docs/verify/raw/ci-snapshots/`. No causal commit inferred.

Snapshot CI correction15:16UTC: round06 failures containing EDQUOT/Disk quota exceeded are private-cache infrastructure failures; automatic crate-defect attribution is withdrawn. Tmpfs reported free space but enforced a per-user quota. The cache was reset and dev/test debug symbols disabled for subsequent rounds (unoptimized/debug assertions retained); release profile unchanged. No source defect is inferred from these failed writes.

Engine performance follow-up final: controlled-quiet final release0.2163% CPU
(47.99% below historical0.4159%; budget≤0.1% still unmet). Five fresh battery
openings: first-buffer median12.59ms/max14.62ms; prepare max3.24ms. Declared
reveal timing is excluded, and a separate whole-cycle trace retains one35.10ms
render spike.162 ordinary tests/strict owned Clippy and204/204 native E2E pass.
Details and boundaries: `docs/verify/perf-followup.md`. Disk policy applied:
stale engine reconcile/functional bulk deleted; compact receipts archived under
`docs/verify/raw/engine-cleanup`. No private Cargo target directories.

Engine active follow-up: profiling first visible battery reveal effect preparation
and idle wake sources. Editing diagnostics.rs, effect/render timing hooks and
shell assembly scheduling; no numeric/shader parity changes intended. All
builds use shared target with CARGO_INCREMENTAL=0.

- [SHELL-R2-1](../reviews/hyprv-shell-r2.md#shell-r2-1) — P1: unbounded tray filename read/copies on UI thread; 128 MiB invalid file raises RSS from 19 MiB to 398 MiB.
- [SHELL-R2-2](../reviews/hyprv-shell-r2.md#shell-r2-2) — P2: same tray item at DPR 1.25→1.30 reuses 29×29 instead of fresh 30×30; cache key omits DPR-dependent target/theme scale.

## From parity QA (2026-10-23) — tray icon request is resolved at the wrong scale
`src/assets.rs:64-77` calls `themed_icon(item, request = 18·scale, dpr = scale)`, so at DPR 2 it
targets 72-px pixmaps from scale-2 theme entries. The live shell (spec-panels' nested
measurement, `docs/requests/parity-capture.md`) resolves the 36-device-px request at **scale 1**:
chrome stays hicolor 32×32, discord 256 → 36. Pass `dpr = 1.0` for theme icons (keep the request
in device px), or call the new `hyprv_widgets::icon_theme::tray_raster(name, request)` for raster
entries and your `svg_icon` for scalable ones. The parity references were recaptured with the
live rule; the bar's replay already uses it.

Implementation claim (reviewer): SHELL-R2-1/2 in src/assets.rs and narrow assembly.rs tray-delivery integration: bounded off-UI file/theme loading, byte-only async replies and DPR-complete cache keys. Preserve spec-island tray sizing/material decisions; re-read before edits.

Engine active-edit coordination for SHELL-R2 owner: current assets.rs now has
`same_tray_icon` and a regression ensuring title/tooltip-only updates retain the
SkImage ID. Please preserve that behavior when adding the byte-only async loader;
otherwise dynamic tray tooltips cause unrelated idle paints. services.rs now drops
tooltip-only notifications after preserving tray priority/network classification,
menus and click flags. Assembly has a stats-only retained-bar path and skips
settled frame acknowledgements. Please avoid overwriting those sections; re-read
current files. My next measurements use a pinned shared-target release.

Engine compile checkpoint during R2 edits: release trace build hit E0425 at
fixture.rs:254 (`crate::assets::tray` now cfg(test) only). Please retain/restore
a synchronous bounded fixture-only decoder entry point so nested fixture/E2E
remains deterministic; it need not be reachable in the live production graph.
Current pinned pre-R2 engine release passes191/191 measured battery frames
(max2.747ms, driver disk shader cache disabled); live-source re-verification
will follow the async-loader handoff. Also a new ordinary widgets test fails:
icon_theme::tests::gtk_cache_reader_matches_qt_on_the_system_cache, line906,
None vs Some([]) for telegram on this machine. This is separate from engine timing.

Handoff update: private fixture.rs now uses bounded fixture_tray; cache misses run file/theme I/O on a scoped worker and wait only in the isolated fixture path, preserving deterministic capture. Live Assembly uses async TrayLoader. GTK sentinel 0xffffffff now accepted alongside zero; malformed nonzero offsets still invalidate. QtSvg edits were re-read and preserved.

Engine caught and fixed a post-handoff native-provider access in refresh: the new
TrayLoader::retain unconditionally dereferenced mocked Hub during initial output
assembly. Guarded it to the production, non-fixture graph; nested functional
startup had aborted before any input. Native-provider boundary regression added.

Fixed SHELL-R2-1/2: assets.rs + new assets/tray_loader.rs route live file/theme loading/rasterization through a bounded two-job blocking-pool path and send only RGBA bytes via Ready over the existing calloop channel. Async replies use generation checks; DPR-complete keys, bounded cache, stable saturation placeholders and output/item pruning prevent wrong-size reuse/reload loops. Encoded files16MiB, SVG1MiB, decoded sides4096; opened-handle reads reject FIFO/device files and cap growth. Fixture.rs uses bounded fixture_tray, whose cache misses run on a scoped worker and wait only for private deterministic fixtures. Preserved concurrent QtSvg and metadata identity changes. Eight new shell regressions pass. Repeated128MiB invalid-file probe: peak19356KiB vs control19164KiB (before407912KiB). Final182 library passes + strict scoped all-targets Clippy. Details: ../verify/discovery-icons-r2-fixes.md.

Engine tray scale correction: production async loading and deterministic fixture
loading now share `themed_icon(item, device_pixel_request)`. Theme rasters use
`hyprv_widgets::icon_theme::tray_raster`; SVG/custom paths use that request too,
with theme lookup fixed at scale 1. DPR 2 produces 36 px rather than 72 px.
Preserved bounded workers, metadata identity, stale-reply rejection and supplied
pixmap DPR cache keys. Updated fractional-scale expectations and added a DPR 2
async completion regression. Validation follows in the engine report.
Validation for the tray correction: assets tests13 passed,0 failed,1 diagnostic
ignored; complete shell library48 passed,0 failed,1 diagnostic ignored. Scoped
all-targets Clippy for hyprv-ui/hyprv-shell/hyprv-widgets passed with -D warnings.
Current aggregate controls reference mismatch is recorded in widgets.md and is
outside the engine-owned primitive paths. Logs: target/reveal-idle/tray-tests.log,
shell-tests-final.log and clippy-final.log.

## Receipt: spec-panels, R1 (2026-10-23, run 3)

Evidence: `docs/verify/parity-fullscreen.md` (Run 3), `docs/verify/raw/parity-fullscreen/run3/`.
- `assets.rs`: tray icons are requested like Quickshell (`round(18·dpr)` at `ceil(dpr)` → pixmaps up to 72 px), and SVGs go through `hyprv_widgets::qt_svg` first. Your concurrent `tray_loader` kept these semantics.
- Added an `#[ignore]` `dump_icon_file` diagnostic.
- 47 lib tests pass; clippy clean.
- Note: a rebuild of the shared `target/release` during a `--nested-test --fixtures` run makes the parent's `current_exe()` respawn fail ("No such file or directory"). Consider pinning the executable at the start of the multi-job loop, as `nested::run` already does for its child.

Engine final checkpoint: corrected tray production release passes204/204 nested
native-input cases. Six full battery cycles pass285/285 frame intervals below
16.7ms (max6.346ms). Clean60s controlled-quiet CPU0.1496%, UI thread0.0977%;
static frame acknowledgements no longer call host update. Broader cold-calendar
frame38.30ms and a19.63ms release-to-first-buffer outlier remain explicitly
unresolved; see docs/verify/reveal-idle.md for evidence and scope boundaries.


### Hourly real-shell idle budget — 2026-10-23T17:05UTC

Hourly real release refresh ci-20261023t165719z (snapshot 36e621a706c0dce82ab1aa9d3010c017cbfd8b621266e46c29707e038fe60193): controlled quiet is 0.16857% summed-thread CPU over60.13585s with0 native hook events. Main UI thread0.059813 CPU s (~0.09946%); service thread0.039107s (~0.06503%); remaining workers ~0.00245s. <=0.1% still unmet. This is a clean production assembly --read-only release, matched6144x3456@2; live-hook interval1.43472%/44 file notifications is separate. Current raw metrics and source receipt are linked from docs/verify/perf-shell.md. Initial main-thread cost0.221571s/60 has improved, but attributing the whole reduction to any one concurrent change is not justified.


### Hourly real-shell idle budget — 2026-10-23T17:46UTC

Second hourly real-release sample ci-20261023t174038z (snapshot 472960c66901692361bf4d1cf84050f125f209c1611f009237b6742a434c80c6): quiet CPU0.20602%,0 native hook events; frozen-hook host CPU1.1744%,0 hooks; live-hook CPU1.37195%,21 notifications. <=0.1% remains unmet. Per-thread CPU seconds: quiet: [('hyprv-shell', 0.068882), ('hyprv-services', 0.05245), ('tokio-rt-worker', 0.000512), ('hyprv-pw-audio', 0.0), ('tokio-rt-worker', 0.000522), ('tokio-rt-worker', 0.000499), ('tokio-rt-worker', 0.000525), ('tokio-rt-worker', 0.000505), ('module-rt', 0.0)]; no-hooks: [('hyprv-shell', 0.068898), ('hyprv-services', 0.041086), ('tokio-rt-worker', 0.11955), ('hyprv-pw-audio', 0.0), ('tokio-rt-worker', 0.119923), ('tokio-rt-worker', 0.117609), ('tokio-rt-worker', 0.117348), ('tokio-rt-worker', 0.1235), ('module-rt', 0.0)]; live: [('hyprv-shell', 0.102904), ('hyprv-services', 0.063767), ('tokio-rt-worker', 0.171013), ('hyprv-pw-audio', 0.0), ('tokio-rt-worker', 0.167267), ('tokio-rt-worker', 0.139558), ('tokio-rt-worker', 0.180586), ('module-rt', 0.0)]. See docs/verify/perf-shell.md hourly history. Different concurrent source snapshots and input/event conditions are integration observations, not proof of any one change causing the difference.

Engine cold-calendar checkpoint: real CalendarPanel startup preparation replaces
synthetic-only mask warming. Final measured release29/29 render frames under
16.7ms, max12.618ms event interval (12.623ms inclusive scope), versus historical
38.301ms. Same-binary disabled preparation has84.678ms bar/model work; enabled
1.872ms, final1.579ms. Preparation uses no window/action/visibility lease, is keyed
by output scale/theme and purges its temporary GPU target. Existing cache memory
is loaded earlier: +~139MiB startupRSS, post-activity peak unchanged~444MiB.
Allocator-trim experiment removed. See docs/verify/pq-qa-rounding.md and raw
receipts target/parity-rounding/. Final ELF retained; old test ELFs retired.


### Real-shell final idle checkpoint — 2026-10-23T18:30UTC

Final pre-deadline refresh ci-20261023t182400z (snapshot 2b7f008d4baeab9ff32471b1b48a6c532bea3d9f8fdf12ae9b711edf64bdf3df): controlled-quiet thread CPU 0.24114% with0 native hook notifications; frozen-hook host discovery 1.27011%; live-hook idle 1.44477% with21 notifications. Every interval>=60s, uninstrumented release actual-shell, same archived ELF for these windows. Idle target <=0.1% remains unmet; see docs/verify/perf-shell.md hourly history and raw per-thread counters. No inter-window event cost subtraction.

Engine calendar-budget checkpoint: warmup restricted150logicalpx without provider
events, cached engine FT faces share the Skia mapping, no synthetic wrapped
paragraphs, closed EGL scratch targets purged. Final release312.2MiB idle,339.4MiB
after calendar/control both close;38/38render frames<16.7ms,max11.565ms. Same
ELF disabled calendar preparation199.95MiB idle, so112.23MiB frontload remains.
Budget NOT met; qtext mapped/heap font backing handed to panels/svc-media with
smaps attribution.105UI/shell library tests +12GPU/privateEGL tests pass; scoped
all-targetClippyclean. No bar change from mismatched snapshot matrices; QA
verify.py authoritative. Details docs/verify/calendar-memory-budget.md.

## Full-screen exactness — bar/island owner,18:33UTC

User authorizes host edits for composition, A1/A2 mid-frames and tray strip after spec-panels Run4 lands. I will re-read before every edit and preserve engine memory/async-loader changes. Waiting for authoritative Run4; inspecting current replay/composition read-only meanwhile. Please publish Run4 path/binary/source receipt here or in parity-fullscreen.md. Engine: coordinate here for any large host changes; I will keep initial changes surgical. Component baselines bar304/island380 exact, panels ownshare184/184 gate; native-only regions will not be masked to manufacture exactness. Canonical captures/reference fixtures stay QA-owned.

18:37UTC inspection: Run3 and current reference receipts use mixed time origins: calendar-morph open-t0000 has reference virtual_ms0 while Rust absolute fixture time2516.6667; `compare.py` measures pixels independently but native comparison refuses with reference_time_mismatch. QA/spec-panels: please document/reference the normalized animation origin in Run4 rather than changing acceptance thresholds. Also verify tray requests against Run3 independent Qt size probe: current engine scale1 correction reverses Run3 semantics. I am running a pinned, private Wi-Fi baseline while waiting; no source edit yet. Wi-Fi candidate is replay pointer focus: native compositor sends leave when a popup dismissal surface covers the stationary pointer, while replay currently only resolves focus on explicit input.

### spec-bar: `fixture/translate.rs` calendar week (2026-10-23)

`calendar_sockets()` sets the Monday `week_start` only when a dcal socket mock exists. Offline it
stays empty, so the panel shows the week starting today (shell.qml `CalendarBridge { weekStart:
new Date() }`). `cargo test -p hyprv-shell` passes.

18:37UTC pinned current release Wi-Fi baseline completed, both themes max239 (Run3 max56/46); screenshot proves WeChat uses green-square app icon while original uses two speech bubbles. Receipts: crates/hyprv-shell/artifacts/fullscreen-core/baseline-wifi/compare/compare.json and qt-strip.png/rust-strip.png. This confirms current device-request/scale1 change regressed full-screen tray selection. Proposed surgical restoration: Sizes retains logical provider request + ceil(DPR) theme scale in cache key; resolve lookup(request,theme_scale), rasterize chosen file at request*theme_scale; supplied SNI pixmaps keep 18*DPR selection; preserve bounded worker/stale-generation/security checks. Engine: please flag conflicts; Run3 independent Qt probe already supports this. Waiting Run4 before mutating.

18:37UTC change log BEFORE edits: independent Qt6.12 offline tool tools/qt_tray_probe.cpp and artifacts/fullscreen-core/qt-tray.log reproduce Run3: default pixmap36 at DPR2 gives Chrome64, Discord/Keepass/Fcitx/WeChat72; explicitDPR1 gives32/36. Current full-screen WeChat green-square is scale1, canonical two-bubbles matchesDPR2. Restoring host-only lookup(request,ceilDPR) + bounded icon_file at request*ceilDPR in assets.rs/tray_loader.rs, keeping SNI pixmap target and async security/generation bounds. Regression tests will assert measured72px and fractional46px, not oldincorrect36/23. No widget API or component replay change.

18:38UTC additional surgical replay-only change planned: before a capture, re-hit-test a known, ungrabbed stationary pointer against newly mapped surfaces and send native leave/enter through Engine::replay_input. This emulates compositor focus recomputation after map/unmap, rather than hiding Wi-Fi hover whenever open (which would change actual behavior). No production input path changes. Tests will use actual private GPU fixture capture for both themes.

18:40UTC integration notice to engine: release build completed2m04, but current debug regression rebuild hits your concurrent functional.rs:63 `engine.event_loop_handle()` missing API. I will preserve that edit; please finish the matching engine API. My own prior48 shell library tests pass. Private unmodified Qt overflow recapture finished; scoring frozen order stability now.

18:42UTC T2 causal result: private UNMODIFIED original Qt overflow recapture equals canonical bit-for-bit, so references are sound. Upstream Quickshell `src/services/status_notifier/qml.cpp:28-31` compares category OR case-insensitive Id; `src/core/model.hpp:89-99` inserts before first false comparator, BEFORE shell.qml:503-521 stable priority sort. Our provider/model feed registration order, hence Chrome/Telegram alternate. Implementing host adapter mapping/tray.rs for both production Assembly and fixture input; no svc-media or component fixture edits. Preserve Qt's unusual OR category predicate explicitly rather than assuming lexicographic category sort. Sources https://raw.githubusercontent.com/quickshell-mirror/quickshell/master/src/services/status_notifier/qml.cpp and ../core/model.hpp. Current tray+focus Wi-Fi2/2 gate max4, MAE0.002008/0.004572, versus baseline239.

18:43UTC replay comparison protocol correction: QA run.py:563 ordinary captures record absolute virtual time, but :571 frame sequences deliberately record time relative to each frames block. Native replay always compared absolute time, making correct animation images report reference_time_mismatch. Carry explicit optional frame offset in Op::Capture and use that for reference-time validation, keeping actual absolute virtual_ms in receipts. No clock/animation pixels change and time validation remains strict (not bypassed). Add timestamp regression.

18:44UTC diagnostic: unmodified original-source private capture (only on-property-change console tracing added in a symlinked source overlay) reproduces every completed canonical control frame bit-for-bit. Qt control fullheight422; open-t0120 reveal337.51491425/opacity0.6975; open-t0220 reveal421.74083925/opacity1. Adding capture-time controller sample to existing HYPRV_REPLAY_TRACE output (fixture-only) to compare animation-clock samples without changing runtime.

18:48UTC focus refinement: the first matrix caught a new submenu max23/24 regression. Re-hit-testing the *surface* is correct, but sending a synthetic Motion inside the same surface changes Qt's retained submenu hover. Reconcile only leave/enter when selected native surface changes; keep pointer position/hover untouched when surface focus is unchanged. This keeps the measured Wi-Fi fix without inventing motion. Re-running both cohorts.

## Hand-off: spec-panels → svc-power, full-screen parity to the gate (Run 4, 2026-10-23)

Per the orchestrator, svc-power now drives full-screen parity to the gate.

**Run 4** (fresh release build): **0/64** pass. With the tray strip masked, 52/64 pass, the same as Run 3's 52/64 unmasked. Details and per-image matrix: `docs/verify/parity-fullscreen.md` "Run 4"; evidence: `docs/verify/raw/parity-fullscreen/run4/`.

**How to run** (isolated; never touches the live session):
```sh
# release build, then pin a copy: a concurrent rebuild of target/release breaks current_exe() respawns
target/release/hyprv-shell --nested-test OUT --fixtures parity/fullscreen/fixtures \
    --references parity/fullscreen/reference                    # TMPDIR=/tmp, no HYPRLAND_INSTANCE_SIGNATURE
python3 -I parity/fullscreen/tools/compare.py OUT                 # ARCHITECTURE gate on premultiplied RGBA + triage
python3 parity/fullscreen/tools/residual.py OUT && python3 -I parity/fullscreen/tools/matrix.py OUT
```

**Remaining causes, in priority order:**
1. **R1 regression: nested tray images (fails all 64).**
   - **Symptom:** after the post-Run-3 changes (14:37–14:49: `assets/tray_loader.rs`, `assets.rs`, `fixture.rs`, `mapping/tray.rs`, `assembly.rs`, `replay.rs`), the nested bar draws a system hicolor wechat instead of Qt's user-level `~/.local/share/icons/hicolor/256x256/apps/wechat.png`. chrome, keepassxc and fcitx are back to max 28/23/39; Run 3 had all ≤ 4.
   - **Already verified:**
     - the child env carries the right `HYPRV_ICON_SEARCH_PATHS`;
     - `icon_theme::lookup("wechat", 36, 2)` picks the user file (`cargo test -p hyprv-widgets --lib lookup_dump -- --ignored --nocapture` with `HYPRV_ICON_LOOKUP=wechat`);
     - `assets::fixture_tray(item, 2.0)` in-process returns the correct pixmap (`HYPRV_DUMP_TRAY=wechat:2:/tmp/x.png cargo test -p hyprv-shell --lib dump_fixture_tray -- --ignored`).
   - **Suspect:** the nested bar now gets its tray images from another path or scale, e.g. `Assembly::tray_assets` → `TrayLoader::get` overriding the fixture's `m.assets.images`, or a cache keyed without the DPR.
   - **Exit:** the tray strip ≤ 4 again restores 52/64.
2. **A1 (hyprv-ui-panels): control-open-close open-t0120/open-t0220, 4 images.** The slider row at logical y 392/476 is 2 logical px high; a 4 px shift removes 96.7 % of the error.
3. **P1 (hyprv-ui-panels): dark calendar event-block corner, 4 images** (calendar, morph open-t0210/t0500, close-t0000). 7 px at logical (1593–1595, 267–274), max 9 (limit 8).
4. **T2 (hyprv-ui-bar): tray overflow order, 2 images, max 255.** The original takes `sortedTrayItems.slice(visibleTrayCount)` (shell.qml:4211–4212); Rust alternates the items.
5. **B1 (hyprv-ui-bar): Wi-Fi trigger hover pill while its popup is open, 2 images,** max 56/46 at logical (2828, 8).

**Reference facts (don't regress):**
- tray icons are requested as Quickshell does: `IconImage` 18 → request 36, `QIcon::pixmap` at DPR 2, mipmapped;
- `hyprv_widgets::icon_theme` is verbatim Qt 6.12 (108-case real-Qt test);
- `hyprv_widgets::qt_svg` follows QtSvg 6.12 (ignores clip-path; `rgba()`/`hsl()` paint opaque black; cosmetic pens ≤ 1 px);
- references are the live shell: the telegram placeholder and the unreachable battery popup are intentional.

## Engine idle priority receipt (18:52UTC)

Historical quiet service runtime39.1→52.5→77.7ms/60s while UI59.8→68.9→64.7ms.
No core/provider Rust source hash changed across those three receipts. Current
service runtime remains the largest unassigned variation: one post-fix instrumented
window85.7ms/60s and2711voluntary switches vs UI53.1ms/244switches; only61stats
notifications and ZEROtray UI notifications. Please attribute remaining actor/IO
polls before blaming nativeGL/calendar/font paths. Same oldELF today0.226→0.306%
threadCPU with13→22true traypixmap updates; overlapping uninstrumented old/new
ELFs0.1791/0.1831%, no significant change isolated from renderer features.

Engine fixed metadata-only stats UI wakes (history/generation/sub-display
precision). Visible graphs still use raw forwarding. Native assembled CI test
with production projected bridge:10samples,10metadata projections,0stats UI
wakes,0paints,1dispatch/1voluntary switch (measurementRPC). Negative control old
raw bridge:10samples,11dispatches/11switches,0paints, correctly FAILS. Workspace
all-targetClippy -Dwarnings now passes, including existing panelsFontDataCache
alias. Widget placeholder comparison passes with true DPR-sized checker.
Details forthcoming docs/verify/idle-wakeup.md; pinnedELF f9c3ad983b6a5bdb83b12ff8c55e99733f05513ad36a52dce9861cad204c95d6.

18:53UTC Run4 RECEIVED: its max239 tray exactly matches my pinned pre-fix14:35 ELF baseline, not my restored host. Independent current fixture_tray probe was run after my assets edit, explaining the apparent source-vs-render contradiction. Our patched pinned binary yields Wi-Fi2/2 max4 and overflow2/2 max6 (full, unmasked), plus51 library tests and ignored Wi-Fi native-replay regression pass. The first intermediate matrix also caught same-surface synthetic motion changing submenu hover; fixed narrowly to send only native focus transitions when surface changes. Receipts under crates/hyprv-shell/artifacts/fullscreen-core. Source hashes and pinned ELF hashes will accompany final full matrix; please do not chase a second tray-source bypass based on probes against a newer working tree.
- baseline-shell: f73c3062459c6acbbfba3886ef05ba8c9fea07bae61b6cfb304a0557d608d933
- tray-focus-shell: 5e343479ee380c29fad26c35b378b8c52013b9d644005b8d7afb530b3ed56603
- order-shell: 44b8d091362f5ccbe6d725e275255a50849064f37ae1c50964494d640234a37d
- debug-order-shell: c22c26b767a17e751b987e837709c67e5194b93dbd0b9ec27440cc782fbdd000
- replay-shell: f9c3ad983b6a5bdb83b12ff8c55e99733f05513ad36a52dce9861cad204c95d6

Engine idle-priority final: production projected stats bridge ignores unchanged
bar labels/severity; visible graphs keep raw samples. Native CI gate now uses
a1Hz service-runtime sampler through that exact bridge, with native callbacks
and host deadlines active:10samples/10projections,0metadata UI wakes/0paints,
1dispatch/1voluntary switch (RPC boundary). Old raw bridge negative control
fails10metadata wakes/11switches while paints stay0. Non-ignored integration
test is included in workspace --all-targets CI; pinned debug ELFs auto-cleaned.
Exact workspaceClippy --all-targets -Dwarnings PASS;181affected-library tests
PASS. Tray test supplies actual18*DPR checker pixmap; panels FontDataCache alias
already fixed by owner and preserved. No canonical fixture/reference edits.
Uninstrumented overlapping old/new60s CPU0.17909/0.18313%: no significant CPU
win claimed; reported0.241% was not reproduced. <=0.1% remains unmet. Service
thread is the remaining attribution target, not an invented renderer regression.
Report docs/verify/idle-wakeup.md; raw/hash receipts target/idle-priority/.

18:55UTC A1 ROOT CAUSE proven with an offline Qt QML plugin (tools/qt_clock_trace.cpp): QUnifiedTimer switches from pause-only time to a fresh QElapsedTimer driver when the reveal's20ms PauseAnimation ends. At raw2566.000ms, its logical elapsed is49ms from epoch2516.667; driver_start=49. Later raw2636.667/time.elapsed120 => driver elapsed119, not120. Our driver floors absolute ms, loses the fractional epoch and omits this restart, advancing the open animation1ms too far. Trace canonical images remain bit-identical. Plan: fixture-only AnimationDriver keeps precise microsecond wall epochs, integer logical epochs, and pause-only→continuous restart; native production scheduling unchanged. Engine: this is the one larger host timing change, logged here before editing; preserve current idle-notification work. Add oracle table regression and all64 control.

18:58UTC P1 cause corrected: the7 max9 pixels at(3186..3189,534..548) are the current-time circle OVER the event rectangle, not the event-border AA. shell.qml:3189-3195 uses SystemClock.Minutes; Quickshell core/clock.cpp:42-51 zeros seconds/nanoseconds. mapping::panels used Local::now with seconds; fixture.advance_to advanced wall seconds too. CalendarSlot retained the first visible second in its minute cache, displacing the dot by a fraction of a pixel. Fixing host wall adapter to minute precision in both production mapping and fixture (no calendar or renderer edits); adding minute-boundary regression. Source https://raw.githubusercontent.com/quickshell-mirror/quickshell/master/src/core/clock.cpp. A1 driver corrected: all20 completed debug captures pass max4; final2 hit debug-only34s watchdog, so final all64 uses release unchanged budget.

19:00UTC composition audit: replay composes every captured surface regardless of native output. Current matrix isolates one output/job, so this is latent there; multi-output captures would overlay unrelated surfaces and reject their different DPR. Restrict compose to selected native output, fail unknown placement rather than silently duplicate, record output in per-surface receipt; add two-output/different-DPR unit regression. Also enforce reference manifest output.name when it is an object, preserving strict output identity. No production topology or painting change.

19:07UTC pinned Run5 COMPLETE: native gate64/64,26/26 jobs, all comparisons enforce reference time/output and no masks. Independent compare.py now running. Source guard detected concurrent engine memory work in assembly.rs/native_gl.rs/qt_gl_effect.rs/text.rs plus panels zz_fonts test; canonical full-screen inputs unchanged. This pass is for pinned artifacts/fullscreen-core/final-shell, not an assertion that later memory edits were tested. Engine/svc-media: please publish settled handoff hashes here; I will rebuild and repeat combined final64 + component controls after your checkpoint. Preserve our AnimationDriver, minute_wall, tray helper and per-output compose changes. My own host52 library + CLI/service all-target tests and scoped strict Clippy pass.

Engine ENG-MEM settled handoff: source hashes in
`target/parity-rounding/mem-eng-source-checkpoint.json`. Only host behavior change
is `corner_surfaces` cropping canonical bar children into four tiny overlay windows
(ui-bar.md §9.1); fixed42px top travel prevents native recreation during reveal;
whole-device-pixel margins preserve fractional DPR. Assembly's AnimationDriver,
minute_wall, tray and per-output compose changes preserved. Renderer edits are
independent lazy GL program allocation and scratch purge; shader source/arithmetic
unchanged. CPU crop/full-output regression passes incl.actual6144×3456@2; private
GL passed small DPR1/1.25/1.5/2 matrix, full-size GL repeat pending. Canonical GL
verify.py running. Please repeat combined full-screen controls for settled checkpoint.

19:11UTC Engine settled memory checkpoint acknowledged; all5 hashes currently match target/parity-rounding/mem-eng-source-checkpoint.json. Rebuilding combined release now, then final64 matrix and UI controls against that source. I preserve spec-permitted tiny-corner allocation (ui-bar.md9.1 explicit performance exception); will verify actual composition at DPR2. My pre-memory pinned Run5 independently compare.py64/64 already. Runtime/fixture time fixes are unchanged.
Engine memory validation caught a pre-existing harness interaction in filtered
outputs test:1.5→2 remaps the prior bar pointer onto CPU, and its800ms settle opens
a stats popup whose dismissal layer consumes the workspace click. Native scale and
corner geometry were correct. Tests now reset/park pointer off bar before EACH
scale mutation, as other independent input cases do. Only functional/suite.rs test
setup changes; production input behavior untouched. First filtered run add/input/
reorder/remove/1.5 PASS;2 input FAIL with open stats in manifest. Rerun pending.
Production prepare_renderer also warms actual masked control material (measured
12ms first-use compile) for frame margin; no rendering arithmetic changes.
Engine filtered native output rerun PASS: add/input-second/reorder/remove/scale1.5/
scale2 +shellkill,7/7. Reset-before-scale fixes harness hover contamination; source
production scale/input behavior unchanged. Receipt target/parity-rounding/
mem-eng-outputs-fixed/manifest.json (debug current build). Pinned test ELF cleaned
with SHA receipt per disk policy. Earlier failed receipt retained for diagnosis.

19:25UTC FINAL ACCEPTANCE HOLD REQUEST through19:32UTC: all64 native comparisons and independent scorer pass on both pinned pre-memory and first merged-memory releases. Bar304/island380 exact, panels298 unchanged across repeated controls. Every source guard has been invalidated solely by ongoing memory preparation/profiling edits; the latest release build now compiles those changes. Please queue further hyprv-ui/host/panels rendering-source diagnostics until this short acceptance run ends, and profile existing pinned ELFs meanwhile. I will run64 + all3 component controls, publish receipts, then explicitly release this hold here. Provider implementation work can continue; canonical inputs and presentation types stay stable. No renderer changes from me.
Engine memory handoff final: tiny corner surfaces restore idle/closed GPU40/38MiB
from307/305; RSS203.1/205.2MiB incl.svc-media font sharing.85/85submitted frames
pass,max9.439ms across2popup cycles; engine.paint9.952ms; no activity shadercompile.
Settled ELF81ed2a60ac90 (target/parity-rounding/mem-eng-complete/hyprv-shell),
source checkpoint mem-eng-source-checkpoint.json; report docs/verify/engine-memory.md.
Native output filtered7/7pass; corner CPU/GL compositing exact incl6144×3456@2.
QA full-screen owner may resume settled-source replay. Current service changes
from coowners are preserved; no production timing/IPC/theme/tray changes here.

Engine/user follow-up: accepted four-tiny-corners topology is now documented as
an intentional deviation in root README, including same namespace on4layers,
empty input region, no keyboard/exclusive zone, compositor rules/bounds impact.
User asks svc-power full-screen parity to cover ALL FOUR corner crops explicitly,
without masks. Request also logged in hyprv-ui-bar.md. Engine is investigating
remaining2QuickAdjust@2 precision crops, renderer-only; respecting19:32hold.

19:40UTC FULL-SCREEN HANDOFF — SOURCE HOLD RELEASED. Pinned combined release
4f83022262c0c4e6d7654d41595575aa838e9ff7a1df3d75cd9480756ce28d81:
26/26 jobs,64/64 unmasked native + independent gate passes; max6, worst painted
MAE0.0165, minimum IoU0.99987. Full frames are0/64 bit-exact. Explicit four-corner
coverage:256 additional128px crops, each corner max4 across all64 images; no masks.
Fresh stable controls:bar304/304 exact,island380/380 exact,panels298/300 pass
(unchanged two QuickAdjust show-t0120@2 max1;242 panel images exact).
Host all-target tests57pass/0fail/7ignored; scoped strict Clippy passes; explicit
Wi-Fi GPU regression passes both themes. Current mapped GPU stress6000frames,
4surfaces,1974resizes,63recreates,776readbacks,0mismatches. Earlier GPU-corruption
risk remains OPEN. Host/UI/reference hashes do not drift during accepted matrix;
provider implementation work continued and fixtures use mocked Hub.
Report:crates/hyprv-shell/VALIDATION-fullscreen.md; all receipts under
crates/hyprv-shell/artifacts/fullscreen-core/. QA/spec-panels:please fold this
accepted table into docs/verify/parity-fullscreen.md; I leave that QA-owned report
untouched. Engine/spec-bar can resume the two QuickAdjust diagnostics immediately.
Obsolete private ELF aliases/plugins/failed probes removed; cleanup.json records
hashes. No shared target or another agent's files removed. Runtime stays Qt-free.

GPU-RISK follow-up (svc-power, current task): extending private lifecycle stress
and adding an opt-in DEBUG-only pre-swap retained-vs-default-framebuffer checksum
self-check. Surgical egl.rs hook + new frame_audit.rs; native_gl.rs arithmetic,
shader paths and cache policy stay untouched. New private harness will read every
submitted buffer, compare deterministic CPU scenes, exercise surface destruction,
scale/transform, cache eviction, context recreation/reset simulation, callback
pauses and compositor restart. Historical two named PNGs already have exact asset
race interventions in icon-race-reproduction/comparison.json; will reconcile the
stale OPEN status only after fresh lifecycle evidence. Engine:please avoid egl.rs
for this task or log overlapping work here; large host changes will be queued.

Engine follow-up validation at19:46UTC: panels+calendar300 / island380 renders
complete; comparing against pre-task QA exports gives exactly2 changed images
(the requested show-t0120@2 pair), all other678 byte-identical. Both pair now
max0 in premultiplied RGBA. Fresh bar render attempted twice but blocked by
current svc-power backlight.rs:127/359 and inhibit.rs:23 #[async_trait] without
attribute import. Please finish the ongoing refactor/import update; I did not
edit service code. Bar log target/qa-precision/final-gl/bar.log.
No conclusions about bar parity regressions drawn from this failed build.

Sampling optimization: sysstats bounded series/shared iface storage requires BarStatsKey iface Arc<str>. Next minimally wiring typed metric leases into existing visibility-gated stats forwards; preserve existing projection behavior.

Engine validation update: svc-power imports are restored. Bar GL replay now
completed at19:46UTC:304/304 exact, owner gate JSON at
target/qa-precision/final-gl/bar-report/gate/gate.json. QA comparator code
passes panels300/300, island380/380. Of680 panel/calendar/island exports,
678 byte-identical vs pre-task QA and the requested2 QuickAdjust crops now
exact. Engine tests pass:6batch,59ordinary library,11surfacelessGPU+3private
Wayland EGL,2corner checker; all-target UI clippy clean. Strict legacy corner
max4 is separately flagged (pre-existing, crop-vs-full native unchanged);
see docs/verify/corner-precision.md.

GPU-RISK helper change:tools/component_controls.py accepts an optional output
path, so this task's fresh all-three-crate controls write artifacts/gpu-risk-controls
without overwriting accepted full-screen receipts. No host production source
changes are needed for the checksum self-check; graphics errors already fail closed.

- 2026-10-23 sampling-cost implementation: Sysstats visibility forwarding now holds typed metric leases, releasing them when hidden. Bar/control panel retain every value their QML displays (including temperature/power); expanded stats additionally requests the eight core tiles. Shared Arc<str> interface keys and inline Series adapters avoid sampling snapshot allocations. Existing forward APIs and UI projection semantics retained. Strict Clippy passes; controlled-quiet per-thread comparison in docs/verify/sampling-cost.md.

GPU-RISK validation:13/13 actual-shell private output/navigation cases passed,
192 pre-swap checks exact. Full-screen parent matrix launched from shared target
stopped after three successful jobs with ENOENT before creating the fourth job.
The per-job nested::run pins its inode, but fixture_matrix::current_exe spawns are
still vulnerable to pathname/dentry replacement/deletion during shared builds.
Re-running with an explicitly copied artifact ELF outside target. Engine:please
consider pinning the parent fixture_matrix executable once, as run already does;
no host production or harness source edits from me for this issue.

20:19UTC GPU-RISK CLOSED final handoff: independent full-screen scorer also64/64
(max6),native26/26jobs+924window audits exact. All3component controls zero drift,
304bar/380island exact,300panels pass. docs/STATUS.md reconciles the stale OPEN
line and links docs/verify/gpu-corruption.md. All stress/test/report work complete;
source hold is released. No live reload/cutover. Final diagnostics use debug-only
audit/fault hooks; EGL drawable resize guarantee is the sole production fix.

- 2026-10-23 sampling result: Final controlled-quiet normal release comparison: service + background worker CPU 0.0521% -> 0.0314% (including hyprv-agentproc); whole shell 0.2130% -> 0.2567% because UI runtime grew. Initial pair was 0.1817% -> 0.1576%. No stable total reduction or <=0.1% pass is claimed. Exact per-thread/ELF receipts in docs/verify/sampling-cost.md.

### FR-NET-3 strict assembled corners — bar owner investigation (2026-10-23)

User queued closure after B3. No shared host/UI source edits yet. Re-read assembly.rs
corner_surfaces and host.rs corner_node: assembled production uses bar baked images;
host.rs's separate Skia path is not assumed to cause measured production residual.
Offline QPainter corner, a live private Qt6 Canvas.save, and its GL grabToImage agree
exactly at DPR2. Full-screen reference bottom-left has39 alpha differences, all
positive (+1/+2/+4), including zero→4 pixels just outside the ideal curve. Thus
investigating full-surface sampling/compositor black-white matte arithmetic before
changing the source curve. No masks, tolerance changes, or reference edits.

Further causal isolation: minimal original Qt Canvas in an isolated nested
Hyprland produces the full-screen reference's corner matte exactly. Its Canvas
QImage matches the existing bake; its actual pre-swap EGL window has39 +1 alpha
pixels. The compositor matte adds another1–3 at16 of those pixels. Tiny Qt surface
changes that second-stage result (max2 vs original4), proving surface sampling
phase matters, independent of the curve. QSG GL observation shows the Canvas uses
an8192×4096 texture atlas (UV1/8192,1/4096 through55/8192,55/4096), whereas native
uploads a standalone54×54 texture with UV0..1. Investigating exact texture-atlas
sampling and compositor-stage replay; no production host changes yet.

User clarification: engine is only changing the1Hz stats wl_subsurface/SHM path;
exact on-screen parity wins, restore one full-output corner overlay if needed,
minimise GPU cost, report memory tradeoff, and compare through the same compositor
sampling as Qt. Planned surgical shared edits (re-read immediately before editing):
assembly::corner_surfaces restore one overlay, use a GPU Qt-atlas corner builder;
LayerConfig/EglWindow gain full-redraw direct-window presentation for passive static
artwork (no extra retained offscreen target/blit); capture explicitly redraws direct
surfaces before readback. Avoid all stats/SHM host paths. Harness composition must
run actual compositor sampling/matting, not compare CPU SourceOver against matte.
Will run component GL controls, actual assembled strict256corners, and memory audit.

### From svc-desktop (2026-10-23): SHELL-R2-1 regressions now compiled

`src/assets/regressions.rs` was never included, so the size-limit, decode-budget and DPR-key tests never ran. I added `#[cfg(test)] mod regressions;` before `mod tests` in `assets.rs`, at the user's request. `cargo test -p hyprv-shell --lib assets::`: 13 pass, including the 3 regressions; clippy clean.

Applied topology change only inside corner_surfaces: return one full-output
all-edge-anchored overlay with production corner_window atlas builder and opt-in
full_redraw. No stats/SHM sections touched. Replaced the old cropped-versus-full
self-comparison regression (which never established Qt equality) with assertions
for one legacy viewport, passive overlay, four atlas sprites and direct presentation.

Legacy Host fallback also restores a single Corner(0) full-output passive overlay,
using the same atlas builder instead of its separate Skia arc. Config/full layout
and focused topology assertions updated; output scale is tracked privately from
MonitorInfo without changing Host::Output’s public shape. No stats input logic edited.

Harness edit: replay capture now calls a new compositor_capture module, guarded
by the existing private fixture marker, /tmp/hv-* runtime, explicit nested signature
and session==system private bus. It uses the same black/white/black stability matte
as the Qt reference capture, restores black on failure, rejects changing/inconsistent
mattes, and never runs outside explicit fixture replay. Private compositor config
now explicitly hides the cursor and uses the reference's black background/no wallpaper.
Raw surface PNGs remain evidence; final assembled PNGs now use real screencopy.
No production subprocess/timer added. Added matte fringe/changing-content regressions.

Added corner-only retained scene reconciliation in assembly: existing corner mounts
previously ignored their newly built desired scene. The restored full-output window
must update sprite phase/atlas on reveal, rotation and scale changes; unchanged
corner trees remain clean. same_binding also includes full_redraw. These branches
are separate from engine's stats/SHM code. First real idle capture: all4unmasked
corner crops exact; whole image PASS(max4,MAE0.000827,IoU1.0).

Full-matrix replay hit its old34s wall budget after successful compositor captures;
raised only the isolated fixture replay base budget25→120s (surface-stall bound
remains10s, parent fixture deadline180s). Actual matte capture cost is included;
no production timing changed. Rebuilding and restarting the complete matrix.

Root README and engine-memory report still describe four tiny overlays: please
update their current-status claims after FR-NET-3 matrix completes. We restored
one full-output overlay by explicit user policy, using R8 atlas/direct window
presentation to minimise its cost. Historical four-tiny memory measurements remain
valid history, not the final topology. Details: docs/verify/assembled-corners.md.

Memory receipt now complete: normal release private6144×3456@2, quiet agents,
30s idle /40s activity (eight successful calendar/control toggles), idle214.2MiB RSS
/257MiB GPU, both closed257.0MiB RSS/256MiB GPU. Historical tiny-surface baseline
GPU40/38MiB; exact sampling topology therefore costs~217/218MiB GPU. Not a matched
same-checkout A/B, and not a new all-popup soak. Allocation savings:81MiB retained
target avoided, R8 atlas32vs128MiB. Full report: docs/verify/assembled-corners.md.

Final release memory refresh22:58UTC, after engine SHM rollback
and the bar batch-root fix: idle214.1MiB RSS/257MiB GPU; after eight successful
calendar/control toggles, both closed247.5MiB RSS/255MiB GPU, sampled RSS peak247.46.
This supersedes the earlier prototype result. ELF4d3a41fb56fa7757; full hashes,
source/build receipt and raw process records in ui-bar/artifacts/corner-memory-final.
Historical tiny-surface GPU40/38 means~217MiB extra in both phases. Still below
320MiB RSS for this bounded workload, not a new all-popup soak or quiet CPU gate.

23:00UTC source-final checkpoint: batch-root follow-up removes the tray-overflow
max1 AA residual without a clip or mask; actual original-window negative-control
regression proves its cause. Sources frozen after engine SHM rollback. Fresh canonical
GPU controls: island380/380, panels300/300, bar300pass+12declared exclusions,
0missing/all312rendered. All three content-hash receipts are current and copied to
ui-bar/artifacts/corner-exact/component-controls.json (+ component-inputs.json).
Full26job/64image final-ELF compositor matrix is still running, expected~23:09UTC.
No more rendering/source changes intended; report/receipt/cleanup only remain.

23:11UTC FR-NET-3 CLOSED in source: final pinned-ELF26/26jobs,64/64full-screen
images PASS in native and independent comparison;256/256strict rectangular corner
crops bit-exact (max0, no masks), versus0/256/max4 before. Final full-screen max5,
worst painted MAE0.0112, minIoU1.0. Source dependencies unchanged from before first
job; all three component GPU receipts current. Debug validation ELFf464a001c0b97fdd;
release memory ELF4d3a41fb56fa7757. This is renderer evidence, not a substituted
staged-release receipt. Report: docs/verify/assembled-corners.md. Main artifacts:
ui-bar/artifacts/corner-exact/{summary,corner-comparison,component-controls,freshness}.json.
Historical four-tiny topology/memory descriptions should now be labelled history.

### Engine LIVE idle work, 2026-10-24: staging coordination

UI static-frame prototype is frozen (wayland.rs + Assembly opt-in; native nested
regression8/8PNG exact, callback requests8->0, resumed animation clock preserved).
I noticed a concurrent packaging/build_release.py writing
/tmp/hyprv-quiet-curve-release-final.log while my own duplicate build was waiting.
Stopped ONLY my own duplicate staging process group to avoid two scripts racing
packaging/dist/release.new. Please keep the packaging owner as the single staging
writer for that build. My initial build completed but correctly refused a receipt
because service owners changed inputs. I will rerun the required build script
once no other staging job is active; no live lifecycle action.

Engine preflight observation during LIVE idle task (2026-10-24): current
cutover/evidence/tests.json is FAIL despite workspace all-targets + doctests +
xtask returning0. cutover/evidence/test-3.log has123tests with1failure/2errors:
PatchInventory::test_patch_preimage_and_no_remaining_quickshell_commands expects
old pre-cutover Lua hashes, and Cut5DriftTolerantRollback::{test_drift_never_blocks_quiesce_and_full_rollback_reverts_by_content,
test_unresolvable_line_is_reported_but_ui_still_restored} copy the now-patched
live tree then try to apply the original cutover patch again. No live Lua edit
is appropriate. Packaging owner should make these tests construct both
pre-/post-cutover fixtures without depending on live tree state. Engine leaves
cutover/ sources and the honest red receipt untouched; will report NOT READY
unless the owner refreshes it.

## P0 RAPL/release coordination — svc-power, 2026-10-24 04:37 UTC
Power/root-helper code frozen since04:24:44UTC; new root RAPL service runs only for active package-power leases;71merged scoped tests pass including svc-net's final cooling logic, strict Clippy green. I have not touched engine's UI/host files. Release builders keep correctly refusing source drift while sys calendar and the nested_rotation test land. Current2944422 began04:36:28; calendar landed04:36:30 and rotation test04:36:54, so it must be retried after source freeze. I am reusing the next admitted shared packaging release rather than launching another writer. My two preflights preserve the same3live-preimage cutover test failures and stale all-source/parity/perf evidence; no gate edits from me. Please publish final release ID in requests/hyprv-privhelper.md after all staging writers exit so net/power/sys can verify/reuse one bundle.

2026-10-24 svc-sys automated gate flake: unchanged frozen source, full workspace idle::quiet_nested_shell_has_no_repaints_or_extra_main_dispatches failed once:11stats ticks,11metadata_only,1dispatch,0paints,19main voluntary switches (limit2), target/idle-regression/3270756. Same-source targeted retry with the identical HYPRV_ASSET_ROOT passed (3275495). A first retry omitted asset env and failed startup; excluded from regression attribution. No engine source edits, test relaxation or exemptions. Full suite rerunning normally. Small failure/pass receipts and logs: docs/verify/raw/emergency-gate-refresh/idle-*. Please investigate voluntary-context-switch threshold environmental sensitivity separately; keep production/source freeze.

## Laptop Phase B services integration — svc-power (2026-10-24)

Deploy spec not yet present. Power is exposing `hyprv_svc_power::Profile::{Desktop,Laptop}` and `spawn_for_profile(ctx, profile)` constructors in power/battery_history/backlight/sysstats/fan. Existing `spawn` retains desktop policy. Host/profile owner: pass the selected profile; laptop compact visible stats need only CpuTotal+Memory at 5 s, expanded/visible per-metric demands at 1 s. Drop leases when actually unpainted, not merely when parent `visible` changes. Laptop raw topic observers will not acquire sampling; use explicit leases. Battery detail guards belong only to popupRequested/open card (drop at close request). Fan constructor will be inert for laptop. Power exposes separate panel cycle action and lid fields. Please preserve crate edits; final public API/test receipt follows here/README.

## Engine laptop Phase B started (2026-10-24)
Freeze lifted. Engine edits UI/shell resource lifetimes/caches only; svc-power and
svc-desktop keep their service requests. No live lifecycle/input changes. Plan:
retire idle full-redraw EGL drawables after swap while preserving the compositor's
last attached buffer and full-output layer/viewport/Canvas sampling; recreate on
actual damage, with native screenshot/full-screen parity as a hard gate. Also
owner-scoped native texture retirement and bounded Skia resources, without shader
program/visible glyph eviction on every close. Desktop defaults get only proven
pixel-identical changes. Re-read these files before parallel edits. Laptop bar
layout/profile remains coordinated with bar owner; engine will expose demand hooks
without editing the bar crate.

## Laptop bar requests from spec-bar (2026-10-24), spec `docs/spec/laptop-bar.md` §1, §13

1. **`bar toggle <monitor>` must work on laptop.** `crates/hyprv-shell/src/host.rs:708`
   currently maps `("bar","toggle") => {}`. Under `Profile::Laptop`:
   - flip that output's compact/full state (`expandedBarScreens[name]`; a missing name
     counts as compact; every output starts compact, nothing is persisted);
   - apply **one global 500 ms debounce** across all outputs (`lastBarToggle`, wall clock),
     not one per output;
   - emit the bar state change so `BarScreenState.reveal` animates (700 ms Hyprland curve).

   On desktop it stays a no-op (`QUIRK_BAR_TOGGLE_IS_NOOP`), keeping output byte-identical.
   `bar dataStatus` / `layoutStatus` must report the live `expanded` / `reveal` values on
   laptop (§11).
2. **`quickshell ipc` compatibility entry point on laptop.** The laptop hypr config (which we
   may not edit) invokes the `quickshell` binary in **two argument orders**:
   - `quickshell ipc -p /home/wilf/.config/HyprV/quickshell call bar toggle '<monitor>'`
     (UltraEdge top-edge rule, `origin/laptop:hypr/hyprland-plugins.lua:196-208`);
   - `quickshell -p '<cfg>/quickshell' ipc call quickAdjust showBrightnessLevel <n>` /
     `showVolume` (`origin/laptop:hypr/controls.lua:46-47`), and `controlPanel toggle`
     (Super+A).

   At cutover these must reach `hyprv-shell ipc call <target> <fn> [args]`. Either the
   laptop cutover patch rewrites them, as was done for the desktop Lua, or a `quickshell`
   shim accepts both orders and forwards them. Cutover/packaging owner's choice. Please
   record which one in `docs/ARCHITECTURE.md`.
3. Bar surface height on laptop: use main's dynamic 58/820 sizing (user decision). The
   laptop QML's fixed 820 is intentionally not reproduced.

Engine resource Phase B checkpoint: UI changes now include idle EGL/native-window
retirement, R8 glyph pages, per-window texture/glyph ownership,8MiB Skia GPU cache,
8MiB combined CPU raster shares, live-scene cache pruning, hidden calendar-tree release,
and effective bar-stats/clock demand. Native screenshot negative control passes8/8
(no masks); prior red samples were Hyprland's animated startup-warning progress bar,
not shell pixels. Private compositor now disables that banner. Ordinary tests pass.
I see packaging/build_release.py PID3333423 active; I will not race its staging writer.
Its source guard may correctly reject my current edits. Engine is entering fresh
GL parity +204-case E2E; please do not install this resource candidate before those
receipts and before/after memory/CPU measurements are finished. No live actions.

Engine native GL resource production edits are frozen for verification now.
`--nested-functional-test` running on a pinned debug ELF; canonical verify.py GL
rerenders all three UI components. Please preserve own files while these jobs run.
No profile auto-detection/UI port is bundled in this resource checkpoint: Compact
lease API is ready for the laptop bar owner, desktop remains Detailed when painted.
After shared staging writer exits I will run/reuse build-release.sh with current
source guard, then measure only pinned release ELFs. Service owners may continue
but any overlap with measurement is reported, not credited as uncontended evidence.

Engine FINAL production source checkpoint (after last image-cache accounting fix):
all own production edits complete; only verification/docs now. Image sample cache
had charged RGBAF32 as4B/pixel instead of16; corrected with an oversize uncached
regression. Three image caches share one2MiB family, so stated8MiB CPU raster limit
is real pixel-buffer charging, not four large independent limits. No more engine
source edits planned unless tests uncover an actual defect. Current component
render may need its island slot refreshed because that final change landed while
bar/panels were running. Canonical rerender will be source-current before report.

Engine staging writer claim: no build_release.py process active at this checkpoint;
starting required packaging/build-release.sh now (CARGO_TARGET_DIR=shared target,
CARGO_INCREMENTAL=0). Please reuse/await it rather than starting a second staging
writer. If another writer appears I will stop only my own duplicate before staging.
Source guard stays strict; any service owner edits require a clean rerun.

### Laptop services ready for host wiring (2026-10-24)

All five profile-aware constructors are now available and documented in
`crates/hyprv-svc-power/README.md`. Map the deployment selector explicitly to
`hyprv_svc_power::Profile`. Host was not edited. Desktop `spawn` is unchanged.
Laptop: `stats.laptop_strip(painted)` owns the CPU+Memory5s guard; raw stats topic
subscriptions are passive; detailed/tooltip leases specify only visible metrics
at1s. No laptop CpuPackagePower lease. Drop the battery guard at popupRequested
false (not surface destruction); fan is inert for laptop. Use
`power::Action::ProfileCyclePanel` for panel cycling; ProfileToggle is CLI order.
Backlight Set-only QuickAdjust confirms/persists; explicit Restore is available.
UPower state has optional lid fields; stats has optional temperature_available
(legacy retained text can be stale). Corrected no-BAT short JSON helper is
`battery_history::State::legacy_json_for_profile`. No selector exists in core/shell
at this checkpoint; do not silently leave laptop constructors on desktop.

Power+helper all-target tests93passed; strict Clippy passes. Source/proof receipt
is in docs/requests/hyprv-svc-power.md. Live shell/session untouched.

Engine animation-boundary correction during validation: Assembly drives related
surfaces from one animation clock but individual frame() returns false. Added
UiHandler::resource_transition_active (Adapter uses animation_active), so an EGL
corner drawable is retained throughout a reveal, not reallocated every frame.
The scheduler retires held direct drawables when that transition ends even if
there is no further damage; no repaint/timer/commit is added. Native regression
now checks this boundary explicitly. This is the last production correction;
refreshing affected source receipts and rebuilding release after the boundary test.
Earlier180edabe is an intermediate candidate, not the final gate/install target.

Final resource release staged successfully via packaging/build-release.sh:
`dc22d67f66e7eb875053aacf5e058068721edf0f497b75389d5284cd88071398`, shell SHA
`31fe89c9cb254e58d41cc2e2e1ff1f9236744c3233f2deeabc4c5ec78e4c1d2b`.
This supersedes intermediate180edabe. Pinned byte-copy is driving fresh release
204-case E2E +26-job assembled GL/corner matrix now; fresh component GL rerenders
continue. Do not treat staging as a gate/install approval. No live lifecycle action.
GPU measurement will include private compositor PID rows as well as the client,
so a reduction cannot be presented solely by moving ownership to the compositor.

### Engine install checkpoint ready (2026-10-24)

All engine shared production edits are complete and held; no unfinished hunk to stash.
Final staged dc22d67f (full ID in the prior entry) compiles, complete staged source
manifest matches current tree, pinned release native E2E204/204 passes. Current-source
canonical GL: island380/380, panels300/300, bar300 reachable/300 plus12 existing
exclusions; owner bar GL gate312/312. Scoped strict Clippy passes, GPU regressions25/25,
native transform/retirement2/2. Evidence: docs/verify/raw/laptop-minimal-resources/checkpoint.json.
Ready for orchestrator build/preflight/install; this is not a replacement for its gates.
Remaining assembled fullscreen/resource measurements use the pinned ELF, require no
shared production edits and do not hold the requested source checkpoint. Live untouched.

### PQ-QA → hyprv-shell owner: laptop nested e2e hooks (2026-10-24, laptop Phase B)

The scaffold is `parity/laptop/e2e/run-laptop-e2e.py`; its report is `docs/verify/laptop-e2e.md`. The fakes are ready and self-tested (6/6):
- `parity/laptop/fakes/fake-system-bus.py`: UPower, PowerProfiles under both names, and login1.
- `make-sysfs.py`.
- the `hypr/controls.lua` stub.

Of the 33 `lap.*` cases, 32 are PENDING on shell hooks. Please add:
1. **`HYPRV_PROFILE`** honoured by `--nested-functional-test`, plus the profile auto-detect (`ARCHITECTURE.md` contract). The scaffold probes the binary for the string.
2. **Laptop suite:** the `lap.*` case IDs from `docs/verify/laptop-qa-plan.md` §4 in the functional suite, selected when `HYPRV_PROFILE=laptop`. The case IDs as strings in the binary are how the scaffold detects the suite.
3. **Nested Hyprland config:**
   - When `HYPRV_FAKE_CONTROLS_LUA=<dir>` is set, prepend `package.path = "<dir>/?.lua;" .. package.path` so `require("hypr.controls")` resolves to the stub.
   - Forward `HYPRV_FAKE_CONTROLS_DIR` to the compositor; the stub logs calls there.
   - Use `HYPRV_NESTED_WIDTH=1440 HYPRV_NESTED_HEIGHT=900 HYPRV_NESTED_SCALE=2` (already supported).
4. **Buses:** the driver already forwards `DBUS_SESSION_BUS_ADDRESS` as `DBUS_SYSTEM_BUS_ADDRESS`. The scaffold starts the fakes on a private `dbus-daemon` and passes that address. Keep the behaviour as it is.
5. **Sysfs:** pass `HYPRV_SYSFS_ROOT` through to the child. The service side of this was requested in `hyprv-svc-power.md` and `hyprv-svc-sys.md`.

Acceptance: `python3 -I parity/laptop/e2e/run-laptop-e2e.py` reports every case PASS. The desktop 204/204 must be unchanged.

### Shared runtime profile API LANDED (engine, laptop Phase B)

`hyprv_core::profile::{Profile, profile, sys_root, sys_path}` now exists.
`Profile::{Desktop,Laptop}`, `profile() -> Profile` is process-latched; exact
HYPRV_PROFILE override (invalid is fatal), otherwise exact portable DMI or present
System-scope Battery; ambiguous defaults Desktop. `sys_root() -> &Path` latches
HYPRV_SYSFS_ROOT or /; `sys_path(rel: impl AsRef<Path>) -> PathBuf` accepts root-relative
paths, strips leading /, rejects ..; no fallback. Code against this shared API now.
Shell startup/profile/service/UI wiring follows. Do not duplicate detection.

### Laptop Phase B svc-net visibility contract (2026-10-24)
Implementing network::Handle::scan_lease.acquire() -> core Subscription<()> (refcounted; hold ONLY while Wi-Fi page requested: control.popupRequested && wifiExpanded OR any standalone Wi-Fi popup requested; not generic Panel/Wifi surface closing animation). Laptop scans and explicit Rescan require this lease; desktop actions unchanged. Please wire the lease in shell-owned visibility routing; svc-net owns only its crate. No state subscription grants scan authority. Multiple outputs share one scanner, hidden drops cancel deadlines.

### spec-island → hyprv-shell owner: wire the laptop island hide (2026-10-24, laptop Phase B)

`hyprv-ui-island` now has `Island::set_bar_reveal(reveal: f64, now)` and `Island::bar_hidden()`
(laptop-island.md §4). At `reveal <= 0` the island:
- ends grabs and hover;
- stops the 66 ms idle waves, so `next_deadline` is `None` apart from pending one-shots;
- reports `animating() == false`;
- freezes `visual_rev()` at the last revealed tree, so the island tree cache does not rebuild.

`set_inputs` keeps the model current. The first call with `reveal > 0` bumps the revision, so the
first revealed frame renders the current state. Desktop never calls it, and `reveal == 1` is a no-op.

Please, in `assembly.rs`, wherever `island.set_inputs` / `set_calendar` run:
1. Call `island.set_bar_reveal(bar.model.reveal, now)` for that output's bar controller. On
   desktop `reveal` is always 1, so this is profile-agnostic.
2. `Consumer::Media` (spectrum and position) and `Consumer::Island` should be false when every
   island is `bar_hidden()` and no control panel is open. This is the laptop `mediaUiVisible`
   rule (`LSH:37`).
3. While all islands are hidden, skip the per-sync `island_inputs()` agent `progress_at`
   resampling. The agent topic still updates the model on events.

svc-net follow-up: inspected services.rs forward_stats(...bar_metrics, Consumer::Panel) currently includes Metric::Network/Temperature. For Laptop this generic CP lease violates laptop-services.md:443 (no network/core/thermal lease merely for CP). Please gate those metrics on actual painted modules/stats popup, and bind network.scan_lease to page Consumer::Wifi plus expandedWiFi requested visibility. No shell edits by svc-net.

Engine Phase B integration checkpoint: bar Model.profile/expanded wired, global
ToggleGate used (one gate across monitors), compact stats5s + independent clock,
corner input regions preserved, hidden island set_bar_reveal wired. Network scan
lease follows Wifi visibility; laptop CP metrics omit hidden Network/Temperature/
CpuPackagePower. Laptop battery detail releases at popup requested=false, before
close animation. Brightness IPC preview1%, never provider writes. Legacy -p/--path
IPC argument orders and StartupConfirm path map to native commands; deployment
patch still owns executable-name rewriting (no live/global quickshell shim).
Nested driver now preserves the explicit fake system-bus address across its own
private session-bus wrapper; previously the wrapper silently replaced the scaffold's
bus. Scope remains private nested. First focused laptop E2E run is in progress.

svc-net review of landed hook: services.rs now binds scan_lease to Consumer::Wifi and laptop Panel metrics exclude Network, thanks. HOWEVER assembly.rs ~1426 overwrites Consumer::Wifi every sync with wifi.is_mapped(), which remains true during closing animation and ignores CP Wi-Fi expanded page. For laptop use requested/actual page visibility: any p.wifi.popup_visible (public) OR CP requested && expanded==Wifi. Keep desktop branch as-is. Add multi-output aggregation (one closing output cannot clear another open page) and close-boundary regression. Otherwise scanner lease persists past requested-close and CP page never enables it.

Clarification after inspecting control_panel.rs: current Rust Page enum has only Main/Bluetooth/Mihomo; WifiText unmaps CP and emits OpenWifiPanel (lines862–865), so p.wifi.popup_visible already covers that existing path. Do NOT invent Page::Wifi for this lease. If a future laptop inline CP Wi-Fi page is added, include its requested visibility then. Immediate required fix is laptop Consumer::Wifi using popup_visible rather than is_mapped while closing.

## svc-power shared API landed — integration blocker to filter (2026-10-24)

Power now re-exports engine's `hyprv_core::profile::Profile`, and every default
constructor dispatches via shared `profile()`. `Paths::default().sys` uses
`sys_path("sys")`. Fresh-process tests prove auto/override/default factories,
private system-bus selection, rooted brightness/thermal/BAT reads and zero laptop
fan RPCs. Typed constructors still accept the shared enum for explicit host wiring.

**Current BarStatsDemand::Detailed still includes CpuPackagePower. Laptop lease()
rejects that metric; forwarding's expect() will panic on laptop expansion.** Filter
CpuPackagePower in the laptop branch of BarStatsDemand::request and chart/panel
leases. Optional CPU-only helper is a distinct future capability; current desktop
combined helper must never activate for laptop. Host ownership is yours; power
will not silently weaken lease validation or edit services.rs. Compact CPU+Memory
5s is already correct. Final report: docs/verify/laptop-impl-svc-power.md.

svc-net service frozen/complete:47tests pass under default and laptop profiles; strictClippy PASS. Source/API/report docs/verify/laptop-impl-svc-net.md. Please close Consumer::Wifi requested-vs-mapped issue above before accepting lap.wifi.scan.visible; core Subscription<()> guards are safe to drop immediately. No sysstats sampler duplicated.

### svc-media → hyprv-shell owner: laptop media demand wiring (2026-10-24, laptop Phase B)

The provider side of `lap.media.compact.idle` / `lap.media.expand` is done and tested in
`hyprv-svc-media` (`tests/laptop_media.rs`, fake MPRIS player on a private bus; report
`docs/verify/laptop-impl-svc-media.md`). MPRIS never polls position (the event-kept
anchor is current), and cava runs only while `SpectrumTopic` has a subscriber and the
player is playing. So zero media work in compact depends only on the host not
subscribing. Under `Profile::Laptop` only, with desktop unchanged:
1. **Spectrum** (`assembly.rs` `v.set(Consumer::Media, …)`): AND in "this island's bar is
   revealed" (`reveal > 0`, i.e. `contentRoot.visible`). Compact hides the island
   (`QUIRK_COMPACT_HIDES_ISLAND`), so `music_active` alone would keep cava running.
   Legacy also runs cava with the CP open and the bar compact, which is invisible work.
   Spec §6 I9 says not to preserve it.
2. **Position clock** (`assembly.rs` `self.position.step(now, expanded, …)`): use
   `music_expanded && revealed`. `PositionClock` already samples on the first visible
   step, so the first revealed frame shows the current position.
3. **Presentation lease:** hold `hyprv_svc_media::mpris::present(&hub.store)` while
   `mediaUiVisible` (any bar expanded, or the control panel `popupRequested`), and drop it
   otherwise. This gives the legacy `onMediaVisibleChanged` Cider re-repair and the 30 s
   Cider fallback only while presented. Desktop holds no lease today, so leave desktop as is.
4. **MPRIS → UI forwarder** (`observe!(hub.mpris.topic, Bar)`): on laptop, gate it on the
   same `mediaUiVisible`, so track changes do not wake or repaint the UI while compact.
   `forward_with` notifies on (re)acquire, so expand still paints the fresh snapshot on
   its first frame.
5. **Attribution for e2e:** keep `mc.counters.clone()` (the new `media::Context::counters`,
   `Arc<Counters>`) and report `counters.snapshot()` (`mpris_wakeups`, `position_reads`,
   `compat_refreshes`, `spectrum_starts`; serde) in `dataStatus`. Then `lap.media.compact.idle`
   can assert "attributed wakeups 0" as a delta of 0 over the window.
API added (additive, desktop untouched): `Context.counters`, `Counters`, `CounterSnapshot`,
`mpris::Presentation`, `mpris::present(&StateStore)`.

### svc-power ready / earlier detailed forwarding blocker resolved

Observed your laptop metric filtering at forward_bar_stats and panel/chart
construction; prior CpuPackagePower lease panic risk is resolved. All constructors
now use the shared enum; default factories also call core profile(). Sysfs root
is core sys_path("sys"), with guarded fake paths and no host fallback. Default
laptop brightness also works with an empty private HOME (pinned XPS config),
without accidentally selecting workstation DDC. Source stable; report
docs/verify/laptop-impl-svc-power.md. Desktop full e2e/parity are yours/QA's; svc-power
ran124 service/core/helper tests +3 panel checks and strict Clippy successfully.

### svc-power → engine: CP recording projection + optional CPU-only capability (open, 2026-10-24)

Following spec-panels' CP data request: mapping.rs::panels calls controls() with
power/backlight correctly, but never assigns `model.controls.screen_recording`.
Please project `h.desktop.recording.state().active` into that field (runtime
laptop branch if desktop must remain unchanged). svc-desktop owns exact-name
laptop process detection; requested in its file. PPD signal state/cycles already
work; brightness unguarded laptop readback fix underway in power.

Also wiring the landed distinct CPU-only endpoint now: laptop typed
lease([Metric::CpuPackagePower], period) will be supported. Remove the blanket
laptop watt filter for actually painted expanded W/tooltip consumers; compact
with a present battery must still omit W. No automatic fallback to combined
helper, no hidden acquisition. Optional endpoint absence yields unavailable
telemetry/log, not a failed lease. Detailed/chart demand remains visibility scoped.

svc-power API update: optional laptop CpuPackagePower lease is now implemented
and tests pass. Endpoint selected by shared Profile, never desktop BUS. Compact
CPU+memory5s still makes zero telemetry calls. Added release barrier for rapid
hide/reopen and cancellation-safe lazy signal setup; 100 consecutive private-bus
compact→watts→compact tests pass. Awaiting your CP recording projection / visible-W
lease wiring; please note completion here for the final svc-power report.

svc-power final reread: observed engine's laptop-only controls.screen_recording
assignment at mapping.rs:46 and removed laptop Detailed watts filter at
services.rs::forward_bar_stats. These two source handoffs are now landed; integrated
host tests remain yours. Exact-name recording service detection remains open in
svc-desktop (recording.rs still uses cmdline scan); it is the remaining CP data gap.

### spec-bar → hyprv-shell: remaining laptop bar/QuickAdjust host wiring (2026-10-24)

API is logged in `docs/requests/hyprv-ui-bar.md` ("laptop bar API landed"). Gaps I see in
`assembly.rs` (yours; I did not edit shell files):
1. Route corner-window pointer events to `Controller::pointer_compact` and apply the
   corner Requirement's `corner_input(m)` region (input not passthrough on laptop).
2. Map `hyprv_ui_bar::tooltip::build(&model)` as an input-less popup over the bar while
   `tooltip_visible` (CPU-power hover, 350 ms; deadline comes from `Controller::next_deadline`).
3. Laptop brightness probe (`laptop-quickadjust.md` §3, verified against the laptop
   reference `lap-qa-step`): 90 ms after any `quickAdjust showBrightness*` IPC, restore the
   OSD value to the provider's level. `showBrightnessIncrease/Decrease` previews are
   therefore transient (50 → 51 → back to 50 after 90 ms without a set). Today
   `preview_brightness` keeps the preview.
4. `HostAction::ToggleBar` from the strip clock is already handled; keep the calendar
   toggle even when the gate drops it (QUIRK_STRIP_CALENDAR_DEBOUNCE_GAP).

### svc-desktop → engine: recording/theme honor shared Profile (2026-10-24)
`desktop::recording::spawn(ctx)` and `desktop::theme::spawn(ctx)` now read the latched `hyprv_core::profile::profile()`:
- recording: Laptop uses exact-comm, event-driven detection, with no 1.5 s lease tick.
- theme: the RAM-RGB openrgb worker is skipped on Laptop.

No signature changes; keep calling the ordinary factories. `screen_recording` = `RecordingState.active`. Details: docs/verify/laptop-impl-svc-desktop.md.

Engine added the final bar host handoffs: compact native input is wired; WallClock
uses actual local minute/weekday (fixture wall for replay); CPU-power hover tooltip
now advances Input.tooltip on its350ms deadline and mounts a cropped input-less
surface, destroys it on leave. 90ms brightness preview map already expires back to
provider without a hardware action; added explicit nested regression for it.
Native laptop suite now includes compact clicks, tooltip lifetime and probe tests.
Current source touched while our first release builder was in flight; its strict
source guard must reject any stale staging. A clean final rerun follows, no waiver.

### PQ-QA → hyprv-shell / engine: laptop suite results and coverage gaps (2026-10-24)

First real run on the pinned 11:26 release (sha256 `81068a07b9e06479…`):
- **36 laptop cases, `suite_complete`, all PASS.** Across the scaffold that is 37/38 PASS; `lap.desktop.regression` is
  running.
- Two scaffold fixes made it possible:
  - The runner stages a private `HYPRV_ASSET_ROOT` from the compiled catalog, verified by `--check-assets`. The nested
    shell has no installed assets in its private HOME.
  - `--keep DIR` preserves the nested logs.
- Two of my expectations were wrong and the suite was right; both are corrected in the QA plan:
  - `lap.brightness.wheel`: wheel is ignored on the laptop CP slider.
  - `lap.egpu.ui`: there is no eGPU card on the laptop CP.

**Coverage gaps.** The cases pass, but the evidence does not yet cover the whole spec. Please extend:
1. **`lap.island.hidden.compact`.** The fixture has music only (`agent_active: false`). The case is "agent active, bar
   compact; model still updates". Please:
   - add an active agent session;
   - change the agent state while compact (`hyprv-agent-hook` event) and assert `island.inputs().agents` changed with
     `visual_rev` unchanged and `next_deadline == null`;
   - toggle the bar and assert the first revealed frame shows the new state (`laptop-island.md` §4).
2. **`lap.brightness.slider`.** Assert that no `Finish` is sent (this already holds). Also assert:
   - **one read-back 90 ms after the last Set** (`QUIRK_LAPTOP_BRIGHTNESS_PROBE_AFTER_SET`);
   - **no read-back overwrite during the drag**, for example with a stubbed `brightness_get` returning a stale value
     mid-drag.
   The drag currently produces only Set(50) and Set(71) (pointer jump). That is fine, but a stepped drag would also prove
   ordering.
3. **`lap.brightness.plan`.** The Lua `plan()` is checked only as a stub table. Evaluating the real laptop `controls.lua`
   `M.plan` inside the nested Hyprland (QA plan §3.4) is still open.

Engine QA coverage follow-up: agent events now have a model-only forwarding gate,
independent of the visible island sampling lease; compact inputs refresh on agent
revision changes without sampling progress at every stats/minute callback. Added
native-hook → production public-session projection → compact/reveal assertions.
Laptop slider now schedules one debounced read-only Refresh 90ms after the last
Set (no Finish), with stepped ordering/stale-provider pixel regression. Added a
real pinned laptop controls.lua M.plan oracle evaluated in private Hyprland using
an inert timer and fake profile, never brightness_set/restore. Rerunning gates.

Engine final release b811aacf: desktop204/204 and laptop39/39 PASS (including
real-Lua101levels, stepped Set×4→one Refresh/no Finish, stale-drag pixel guard,
active hook-model updates while compact with frozen visual revision and first
reveal fresh). 90.096s native laptop minimal run: CPU0.0444%, RSS150.32MiB,
PSS79.22MiB, shell GPU29MiB, voluntary-context-switch proxy4.129/s. CPU target
passes; RSS120MiB target DOES NOT pass. Full evidence/known limits are in
`docs/verify/laptop-impl-engine.md`; no installation or live session changes.

Final desktop GL rerender completed: island380/380, bar300 reachable/300 (312total,
12existing unreachable battery exclusions), panels300/300. All gates PASS and all
GL source fingerprints current. Full result/report snapshots are linked from the
engine report. Engine Phase B checkpoint is complete; RSS120MiB remains explicitly
unmet, not waived. Release b811aacf is staged, not installed.

Engine RSS follow-up complete, staged release87f5ca39. Shared allocator bootstrap
only (all profiles):2arenas, initial128KiB mmap boundary,2MiB trim threshold,
explicit glibc settings preserved; one0.100ms startup trim before the UI loop,
no runtime trim/cache-eviction timer or frame-path trim. Same-ELF private controls
show NVIDIA RssAnon30.77→30.45MiB and Mesa/llvmpipe127.40→99.82MiB. RawNVIDIA
RSS151.25MiB includes59.16MiB namedNVIDIA mappings+15.42MiB LLVM; driver-excluded
bookkeeping footprint76.67MiB. Mesa code-adjusted proxy175.96→148.38MiB, not a
physical-XPS or120MiB pass. CPU0.0333%NVIDIA; CPU0.2885%softwareMesa (not iris).
Desktop204/204, laptop39/39; fresh GL island380/380, bar300reachable/300 with the
existing12exclusions, panels300/300; all current source fingerprints PASS.
Before/after table and timing/proxy limitations: docs/verify/laptop-impl-engine.md.

### engine — XPS Ready callback regression and scheduling audit (2026-10-26)

Preserved the user-supplied `drop(a)` fix in production `art_receive`. Added
real-channel art and real-worker tray completion tests with the live Assembly
Adapter in private desktop/laptop compositors; both profiles pass 2/2. The
separate expected-failure child reproduces `RefCell already borrowed`. Queue and
status RPCs never redraw, so completion must reconcile itself. Reusable runner:
`crates/hyprv-shell/test-ready-callbacks.py`; audit and evidence:
`docs/verify/ready-callbacks.md`. Audited assembly/compatibility-host service, IPC,
clock, smoke, replay and functional scheduling paths; no additional unsafe
production path found. Engine API docs now describe synchronous handler calls.
Shell library65 passed (2 opt-in dump diagnostics ignored); scoped Clippy passes.
No live shell/compositor/helper was modified or restarted.

Final checkpoint: desktop nested functional **204/204 pass**; final focused
runner rechecked both Ready variants on both profiles and the expected panic.
Raw manifests and SHA256 receipts are linked from the audit report.
