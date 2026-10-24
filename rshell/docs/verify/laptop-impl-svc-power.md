# Laptop implementation — svc-power

2026-10-24. Implemented against engine's shared `hyprv-core/src/profile.rs`.
Power source bundle SHA-256 (sorted workspace-relative src paths, NUL, then bytes): `4ad536c1038a27eba02ff1b1cc4e89c65fb4febe71aa03503f3bc4a23e44c03e`.

## Changes

- Removed the local profile enum: `hyprv_svc_power::Profile` re-exports the core
  type. Default power, battery detail, backlight, fan and sysstats constructors
  dispatch through latched `hyprv_core::profile::profile()`. Explicit-profile
  constructors accept that same type for host/test wiring. Desktop selects the
  existing desktop implementations; laptop-only behavior is runtime gated.
- `Paths::default().sys` uses `sys_path("sys")`. The announced root contract means
  `$HYPRV_SYSFS_ROOT/sys/class/...`, normal `/sys/class/...`. Discovery, reads,
  availability, readback and writes use that selected root or explicit Context
  fixtures; symlink escapes/missing paths never fall back to host sysfs. Fixture
  contexts omit host udev/netlink and support atomic backlight/thermal updates.
- Existing signal-cached UPower/profile/lid/sleep and leased battery details now
  work through default constructors. A closed popup records no history rows.
  Laptop fan remains unavailable and makes zero fan/helper RPCs even subscribed;
  optional package watts now use the distinct CPU-only capability described below.
- Laptop brightness retains native153/400-nit HDR and the laptop raw curve. If
  HOME lacks a profile or contains workstation DDC settings, use the pinned XPS
  sysfs profile; never open workstation I2C from a forced laptop fake. An actual
  laptop literal profile still supplies its monitor overrides.

## Validation

Shared CARGO_TARGET_DIR; CARGO_INCREMENTAL=0; private buses/filesystems only.

| Check | Result |
|---|---|
| `HYPRV_PROFILE=desktop cargo test --locked -p hyprv-svc-power -p hyprv-privhelper -p hyprv-core --all-targets` | **124 passed**,0 failed;2 manual benchmarks ignored |
| Same crates, `cargo clippy --locked ... --all-targets -- -D warnings` | **PASS** |
| `cargo test --locked -p hyprv-ui-panels --lib control_panel::laptop_tests` | **3/3 PASS**, including no chassis/pump zones |

New fresh-process tests cover forced desktop/laptop, XPS autodetection, shared
enum identity, default constructors and Context::system using the private system
bus. Invalid DBUS_SYSTEM_BUS_ADDRESS fails without fallback. Default laptop
constructors read fake raw brightness/max, BAT detail and45.678°C thermal data;
closing the popup stops1Hz history updates. An owned fake helper receives zero
laptop fan calls; forced desktop hydrates its existing fan observer. Symlink
escape and atomic thermal-replacement regressions pass. Existing history/Lua HDR
oracles, signal/profile/lid/sleep and visibility tests remain passing.

## Handoff / limits

Engine has wired shared-profile constructors and filtered laptop CpuPackagePower
from detailed forwarding. Panel owner gates fan rendering out for laptop. QA was
notified to build fake sysfs under the root's `sys/` directory. See the public API
in `crates/hyprv-svc-power/README.md` and coordination requests.

Desktop204/204 nested e2e and full parity counts were **not rerun by svc-power in
this checkpoint**; their integrated reruns belong to engine/QA. This report does
not claim those gates, full-shell resource targets, nested HDR/plugin acceptance,
or physical XPS validation. No installation, root-helper/unit edits, Git commits,
live UI actions, live IPC or service/compositor restarts were performed.

## CP/open-request follow-up (2026-10-24)

Audited `docs/requests/`, including the power, privhelper, shell, panels and network
handoffs. Source changes are confined to power; no helper, desktop, panel or shell
source was edited. This follow-up supersedes the earlier unsupported-watts status.

| Request | Result / remaining owner |
|---|---|
| CP power profile | Already supplied by signal-cached PPD state; host passes it to `controls.profile`. Private-bus regression verifies modern-provider preference, legacy-provider fallback, both cycle orders, policy denial, external signals and rejection of stale queued values. |
| Laptop brightness read-back | **Fixed:** `Engine::refresh` accepts laptop read-back during dragging and pending/unconfirmed requests, including unchanged raw values. Desktop keeps both guards. Regression tests both profiles; set-only confirmation/persistence remains passing. |
| Optional CPU-only telemetry | **Implemented:** laptop typed CpuPackagePower leases select `org.hyprv.Shell.CpuPower` only. Compact CPU+memory/5s never subscribes. Missing optional service logs unavailable, never falls back to desktop helper or user-side RAPL. |
| Telemetry lifetime | Added cancellation-safe lazy signal setup, release on last demand/sleep, and a release barrier preventing a late unsubscribe from removing a rapid reopen's new lease. Both cancellation points have regression tests. |
| svc-net visible-only rates | Existing typed gating retained. Added regression removing route/counter files while hidden, simulating a route reset, then reopening on a new interface: no hidden reads and a newly primed baseline. |
| Engine hidden fan request | Earlier demand gating/member filtering retained; private-bus fan tests remain green, including zero laptop fan calls. |
| Exact-name laptop recorder | **OPEN, svc-desktop ownership:** current recorder still matches argv. Filed the precise `/proc/PID/comm`/`pgrep -x` change and false-positive/truncation regressions in `docs/requests/hyprv-svc-desktop.md`. No change outside assigned source paths. |
| CP recording field | **Engine source landed during this follow-up:** `mapping.rs::panels` now projects recorder active state under the laptop branch. Host integration acceptance remains engine's; exact-name recorder semantics are still open. |
| Visible laptop watts host wiring | **Engine source landed during this follow-up:** `forward_bar_stats` now permits Detailed watts leases; Compact remains CPU+memory/5s. This matches the service endpoint contract. Full host acquisition/visibility acceptance remains engine/QA's. |
| Older glass-shadow request | Panel-owned, not a service change. Panel owner's `docs/verify/laptop-impl-spec-panels.md` records no visible delta because the empty shadow layer is a sibling of content. Routed to the current panel owner; no power implementation needed. |

Latest checks use shared target, `CARGO_INCREMENTAL=0`, forced desktop profile
with explicit laptop/fake branches exercised by the tests, and private buses/files.

| Check | Follow-up result |
|---|---|
| Power/core/helper all targets | **134 passed**, 0 failed; 2 manual benchmarks ignored. Core22; helper47 unit +5 integration; power58 unit +2 integration. Child-process test summaries are not double-counted. |
| Same crates strict Clippy, all targets, `-D warnings` | **PASS** |
| Panels library | **19/19 PASS**, including laptop fan absence |
| Optional endpoint compact→watts→compact | **100/100 consecutive private-bus runs PASS** |

Logs: `/tmp/svc-power-followup-all-tests.log`,
`/tmp/svc-power-followup-clippy.log`, `/tmp/svc-power-followup-panels.log`.
The earlier experimental lazy setup did fail a repeat run; the failure was traced
to marking acquisition attempted before an await could be cancelled. The added
`cancelled_lazy_signal_setup_remains_retryable` test covers the repaired ordering;
the 100-run result is after that repair. The network test initially had an
incomplete route fixture; it now uses the full legacy mask columns and preserves
`None` for the initial unknown rate rather than inventing a zero measurement.

No release was built/installed and no live session/helper/compositor was touched.
Integrated desktop e2e/parity, laptop full-shell resource targets and physical XPS
acceptance remain with engine/QA. The open cross-owner
recorder handoff above is explicit remaining work, not a service-level pass.
