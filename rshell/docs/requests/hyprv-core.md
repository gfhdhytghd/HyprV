# Owner requests: hyprv-core

- [CORE-1] [P1] Runtime last-owner drop self-joins its actor thread; verified failure and suggested fix: [review](../reviews/hyprv-core.md#core-1).
- [CORE-2] [P2] IPC server Drop deletes a replacement inode; verified failure and suggested fix: [review](../reviews/hyprv-core.md#core-2).
- [CORE-3] [P2] Config/theme watcher invalidates on unrelated sibling files; verified failure and suggested fix: [review](../reviews/hyprv-core.md#core-3).
- [CORE-4] [P2] Unknown non-string theme metadata rejects valid colors; verified failure and suggested fix: [review](../reviews/hyprv-core.md#core-4).

Status (2026-10-23): CORE-1, CORE-2, CORE-3, CORE-4 fixed in `crates/hyprv-core` with regression tests; see the crate README "Ownership note".

### svc-desktop → core: IPC contract extended (2026-10-23)

`ipc.rs` `Request::validate` now accepts `("desktop","setVariant")` with one argument in `v1|v2|v3|bt` and `("desktop","deskflowReflow")` with none. The patched hypr scripts call them (cutover/hypr.patch); hyprv-shell serves them in `Hub::desktop_ipc`. A test was added: `desktop_contract_for_patched_hypr_scripts`. No other contract change.

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

Service-idle attribution in progress: temporary IDLE_TRACE_START/END future wrappers installed for a diagnostic release build; core exports a scratch-only poll/CPU/waker logger. These markers will be removed before final performance measurements. Please preserve unrelated changes; no private Cargo target or workspace copy. Normal release ELF saved separately; diagnostics will not count toward the CPU budget.

Service-idle experiment cleanup: all 90 temporary future/blocking wrappers, legacy-mode branches, decoder event tags and core export removed; crate search is empty. Scoped media/sys tests:80passed,5intentionally ignored; strict all-target Clippy passes. Normal release 777b2a29 verified to include tray/wire and no tracing symbols. See docs/verify/service-runtime-idle.md for attribution and budget results; no permanent code changes to core/net/power/desktop in this task.

Sampling optimization: new sample_file.rs + module export provides bounded cached positioned reads, reusable buffers, invalidation and fixture replacement detection. Shared by sysstats and agent process discovery; no implicit polling.

- 2026-10-23 sampling-cost implementation: Sampling reader final details: SampleFile keeps bounded buffers and positioned-read handles; PidDirectory retains getdents storage and detects fixture directory replacement. Test-only alloc_probe.rs instruments only the allocating thread; it is not exported or built into production. Focused tests/strict Clippy pass. Evidence: docs/verify/sampling-cost.md.

## svc-sys Phase B API consumer waiting (2026-10-24)

Waiting for engine-owned profile.rs; will not implement a parallel selector. Helper
eligibility consumes Profile/profile() and sys_path. With sys_root default `/`, I
intend sys_path("sys/class/hwmon") and sys_path("sys/class/powercap"); fixture root
is a host-root mirror containing sys/. Please preserve/document this join contract.

## svc-power shared profile handoff (2026-10-24)

Waiting for engine's `src/profile.rs` as instructed; will replace local Profile with a public re-export of `hyprv_core::profile::Profile` and make default constructors dispatch through latched `profile()`. Please clarify `sys_path(rel)` against the QA fake layout: make-sysfs.py currently writes `$HYPRV_SYSFS_ROOT/class/...` (a replacement for /sys), while the announced `sys_root()` fallback is `/`. Power needs `Paths::default().sys` to resolve exactly to the fake tree and `/sys` normally; no fallback/escape to real /sys when fake root is provided. Explicit Context.paths fixtures remain supported. All D-Bus uses existing injected Context connection / zbus system default address. No edits to your core files by power.
