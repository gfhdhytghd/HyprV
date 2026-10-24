
## svc-verify live observation (2026-10-23)

Read-only probe found a battery-history parity mismatch: the original `scripts/battery-info.sh` reports `{"available":false}` on this host (no `/sys/class/power_supply/BAT*`), while `battery_history::spawn` publishes available=true, capacity=69, mode=discharging from `hid-04:41:a5:8c:f8:53-battery-144` (Bluetooth trackpad). `battery_history::Native::discover` currently accepts any supply whose type is Battery. Please preserve the legacy laptop-battery selection scope (and UPower present/type gating) rather than treating peripherals as the laptop detail battery. Evidence: `docs/verify/raw/legacy.json`, `rust-latest.json`, `comparisons.json`.

Probe also cannot safely observe an absent fan helper: `fan::snapshot` calls properties/GetFans without checking name ownership, so simply spawning the actor could D-Bus-activate the privileged cooling owner. The verifier skips that actor when absent. Legacy fan status is available (saved chassis=44%, pump=38% in the captured run). Please add a read-only/no-autoactivation construction option and owner-presence gating.

Backlight is skipped in this probe to avoid opening `/dev/i2c-*` for writing (DDC read transaction). Please provide a read-only observation/config-only path or explicit transport selection for host verification; no hardware mutation or control action is authorized in this task. No changes to this crate were made by svc-verify.

Clarification after full transport audit: native backlight `Engine::refresh` sends only DDC Get VCP (opcode 0x01) or sysfs reads; restoring saved values changes local State only. This is read-only hardware observation even though DDC uses an O_RDWR descriptor. The final verifier therefore includes the default native backlight actor, with no actions/Set VCP/reference/persist calls. The initial “not spawned” result above was conservative probe policy, not evidence of a mutating refresh bug. Helper autoactivation and the peripheral-battery mismatch remain separate findings.

- [POWER-1] [P1] Undrained provider signal streams can block the RPC they are meant to refresh — [verified review](../reviews/hyprv-svc-power.md#power-1).
- [POWER-2] [P1] Event failure reconnects immediately when connect itself succeeds — [verified review](../reviews/hyprv-svc-power.md#power-2).
- [POWER-3] [P2] CPU utilization double-counts guest and guest_nice counters — [verified review](../reviews/hyprv-svc-power.md#power-3).


## svc-verify residual idle scheduling (later frozen build)

After the sys and desktop review fixes, the full 60.102 s idle probe still used 10.52 CPU s (17.504% of one core), 676.62 scheduling/context-switch events/s; same-window main Quickshell+children used 1.48 CPU s (2.462%), 55.44/s. Sampling demand had been released. A matched-binary group-only diagnostic (10.117 s) isolated power at 1.42 CPU s (14.035%), 707.89 scheduling/s; sys-only was 0.099% / 3.95/s, desktop-only 3.253% / 22.57/s with genuine agent events, net+media-only 0 CPU / 0 scheduling in its window. No power state publications occurred in the hot interval. This implicates the power observation group, not a specific actor or causal mechanism. Raw: `docs/verify/raw/perf-final-isolation-power.json` and other final-isolation files; exact source hashes and binary digest are embedded.

Power/helper sources changed again after this frozen build, so these numbers do not certify or reject the just-landed follow-up repairs. Verifier is rebuilding and rerunning. Please test all unchanged event sources and readiness loops, not only externally published topic counts. No changes to power were made by svc-verify.

Latest follow-up full-build idle rerun: 60.101 s, probe 7.26 CPU s (12.080% of one core), 378.53 scheduling/context-switch events/s, RSS 48172->48600 KiB. Same-window Quickshell+children 2.562% CPU / 71.53 scheduling events/s. Raw `docs/verify/raw/perf-idle.json` embeds new binary/source hashes; older post-sys-review baseline is preserved as `perf-after-sys-review-idle.json`. Target still not met. State publications: `{'agent_island': 116, 'tray': 2, 'hyprland': 2, 'systemd': 3}`. Do not attribute all residual CPU to a particular actor without a new matched intervention.

## Idle CPU investigation ownership (svc-verify, 2026-10-23)

User assigned svc-verify the remaining idle CPU defect, including minimal power-crate repair. I am measuring a fresh release snapshot and will instrument a scratch copy before editing production. I will preserve svc-net's concurrent POWER-1/2/3 review repairs, re-read before edits, and append exact touched files/tests here. No hardware/service actions are part of the investigation.

Exact idle root cause found: `power::Native` refreshes on unrelated signals because `MessageStream::for_match_rule` cannot locally match well-known sender names. A trace of the frozen shared connection shows login1, old/new profile and UPower inboxes all receiving global DBus NameOwnerChanged and each other's PropertiesChanged; power refresh count exceeded 256 in eight seconds. The power-only release baseline still used ~1.48% CPU / 838 scheduling events/s (10 s), while full release was 2.63% / 883/s (60 s). Named `hyprv-services` reactor accounted for 52,719 of 53,048 full-probe scheduling events. `strace` is absent; scratch actor/provider event counters supplied the exact attribution (`docs/verify/raw/power-event-trace.log`).

Minimal production edit: **actor.rs only**. Resolve each real-bus provider to its unique owner before constructing its raw stream; absent providers retain a pending inbox until owner recovery. Replace broad DBus owner streams with per-provider NameOwnerChanged + arg0 filters, both broker-side and local. Retain svc-net's bounded independently drained inboxes, overflow errors and reconnect cooldown/owner reinstallation changes. P2P mocks keep their existing sender behavior. Two private-bus regressions cover cross-rule noise leakage, owner filtering, absent provider and owner replacement. The noise regression fails on the before source with `unrelated sender triggered a provider refresh` (negative control retained). After build uses the exact before dependencies with only actor.rs changed, preventing concurrent edits from confounding the comparison. Validation/performance rerun follows.

Repair validation complete: **17 power tests pass**, strict all-target power clippy passes, all three svc-net review regressions retained/passing. Matched frozen release after: full services 0.9145% CPU / 194.80 scheduling events/s over 60.145 s, versus before 2.6286% / 882.54. Full after still has 94 genuine agent publications and is not counted as satisfying the global 0.1% budget. **Power-only after: 0 process CPU ticks, 3.265 ms aggregate thread runtime in 60.151 s (~0.0054% CPU), 1.795 scheduling events/s**, versus release power-before 1.4787% / 838.49. No power state publications in either interval. Follow-up diagnostics separate live agent work from quiescent-service costs. Evidence: perf-release-*.json and power-idle-fix.patch in docs/verify/raw/.

Final checkpoint after incorporating the desktop owner's concurrent updates: full release 0.3492% CPU versus same-window rendering-inclusive Quickshell 2.6606%; same binary excluding live agent inputs 0.0166% over 60.236 s. An explicit all-actor fixture freezes agent hook/app-server inputs and measures 0.1164% over 60.150 s with two real Docker/systemd jobs and four Hyprland publications; this is retained as above budget, not reclassified as a pass. Power-only 0.0054% high-resolution thread CPU meets the requested budget. Full live <=0.1% acceptance remains unmet. Details and named-thread raw counters are in docs/verify/perf.md. Production power changes remain actor.rs only; final formatted source and tests are in the latest snapshot.

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


### Snapshot CI — 2026-10-23T19:43:22+00:00 — debug

REPRODUCIBLE: failed twice on same snapshot. Snapshot `/tmp/hci-sys/round-25-20261023T194304Z/rshell`; fingerprint `64ee34c084ee98ed39959f42573192bdeef828abb85ad2c3b185e0f842c6427e`. Concurrent live edits cannot change this source copy.

Command: `['cargo', 'build', '--workspace', '--locked', '--message-format=json']`. Attempts: `[{'code': 101, 'log': 'round-25-20261023T194304Z/00-debug-1.log'}, {'code': 101, 'log': 'round-25-20261023T194304Z/00-debug-2.log'}]`. Locations: crates/hyprv-svc-power/src/sysstats.rs:1, crates/hyprv-svc-power/src/sysstats.rs:375, /home/wilf/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/alloc/src/macros.rs:44, crates/hyprv-svc-power/src/sysstats.rs:428, crates/hyprv-svc-power/src/sysstats.rs:1, crates/hyprv-svc-power/src/sysstats.rs:375, crates/hyprv-svc-power/src/sysstats/series.rs:4, crates/hyprv-svc-power/src/sysstats.rs:368, crates/hyprv-svc-power/src/sysstats.rs:377, crates/hyprv-svc-power/src/sysstats.rs:316, crates/hyprv-svc-power/src/sysstats.rs:428.

`crates/hyprv-svc-power/src/sysstats.rs`: `24a0994ff815db233245a43c249704f6401062112328bf5836f88de017e0faf2`
`crates/hyprv-svc-power/src/sysstats/series.rs`: `ef9fe0066cb8b734881dffbbd54ea53445d7d6ff6db1acd4669ea49de7b45ff7`

```text
error[E0583]: file not found for module `sample`
 --> crates/hyprv-svc-power/src/sysstats.rs:1:1
  |
1 | mod sample;
  | ^^^^^^^^^^^
  |
  = help: to create the module `sample`, create file "crates/hyprv-svc-power/src/sysstats/sample.rs" or "crates/hyprv-svc-power/src/sysstats/sample/mod.rs"
  = note: if there is a `mod sample` elsewhere in the crate already, import it with `use crate::...` instead


```

Logs relative to `docs/verify/raw/ci-snapshots/`. No causal commit inferred.


### Snapshot CI — 2026-10-23T19:44:06+00:00 — release

REPRODUCIBLE: failed twice on same snapshot. Snapshot `/tmp/hci-sys/round-25-20261023T194304Z/rshell`; fingerprint `64ee34c084ee98ed39959f42573192bdeef828abb85ad2c3b185e0f842c6427e`. Concurrent live edits cannot change this source copy.

Command: `['cargo', 'build', '--workspace', '--release', '--locked', '--message-format=json']`. Attempts: `[{'code': 101, 'log': 'round-25-20261023T194304Z/01-release-1.log'}, {'code': 101, 'log': 'round-25-20261023T194304Z/01-release-2.log'}]`. Locations: crates/hyprv-svc-power/src/sysstats.rs:1, crates/hyprv-svc-power/src/sysstats.rs:375, /home/wilf/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/alloc/src/macros.rs:44, crates/hyprv-svc-power/src/sysstats.rs:428, crates/hyprv-svc-power/src/sysstats.rs:1, crates/hyprv-svc-power/src/sysstats.rs:375, crates/hyprv-svc-power/src/sysstats/series.rs:4, crates/hyprv-svc-power/src/sysstats.rs:368, crates/hyprv-svc-power/src/sysstats.rs:377, crates/hyprv-svc-power/src/sysstats.rs:316, crates/hyprv-svc-power/src/sysstats.rs:428.

`crates/hyprv-svc-power/src/sysstats.rs`: `24a0994ff815db233245a43c249704f6401062112328bf5836f88de017e0faf2`
`crates/hyprv-svc-power/src/sysstats/series.rs`: `ef9fe0066cb8b734881dffbbd54ea53445d7d6ff6db1acd4669ea49de7b45ff7`

```text
error[E0583]: file not found for module `sample`
 --> crates/hyprv-svc-power/src/sysstats.rs:1:1
  |
1 | mod sample;
  | ^^^^^^^^^^^
  |
  = help: to create the module `sample`, create file "crates/hyprv-svc-power/src/sysstats/sample.rs" or "crates/hyprv-svc-power/src/sysstats/sample/mod.rs"
  = note: if there is a `mod sample` elsewhere in the crate already, import it with `use crate::...` instead


```

Logs relative to `docs/verify/raw/ci-snapshots/`. No causal commit inferred.


### Snapshot CI — 2026-10-23T19:44:11+00:00 — stage release

REPRODUCIBLE: failed twice on same snapshot. Snapshot `/tmp/hci-sys/round-25-20261023T194304Z/rshell`; fingerprint `64ee34c084ee98ed39959f42573192bdeef828abb85ad2c3b185e0f842c6427e`. Concurrent live edits cannot change this source copy.

Command: `['packaging/build-release.sh']`. Attempts: `[{'code': 1, 'log': 'round-25-20261023T194304Z/02-stage-release-1.log'}, {'code': 1, 'log': 'round-25-20261023T194304Z/02-stage-release-2.log'}]`. Locations: crates/hyprv-svc-power/src/sysstats.rs:1, crates/hyprv-svc-power/src/sysstats.rs:375, crates/hyprv-svc-power/src/sysstats/series.rs:4, crates/hyprv-svc-power/src/sysstats.rs:368, crates/hyprv-svc-power/src/sysstats.rs:377, crates/hyprv-svc-power/src/sysstats.rs:316, crates/hyprv-svc-power/src/sysstats.rs:428.

`crates/hyprv-svc-power/src/sysstats.rs`: `24a0994ff815db233245a43c249704f6401062112328bf5836f88de017e0faf2`
`crates/hyprv-svc-power/src/sysstats/series.rs`: `ef9fe0066cb8b734881dffbbd54ea53445d7d6ff6db1acd4669ea49de7b45ff7`

```text
e --target-dir /home/wilf/.config/HyprV/rshell/target --config /tmp/hci-sys/round-25-20261023T194304Z/rshell/packaging/release.toml -p hyprv-shell -p hyprv-privhelper --bins
Stage hyprv-shell, hyprv-privhelper, hyprv-cooling-failsafe, build.json and the hash-manifested release/ (MANIFEST.sha256) under /tmp/hci-sys/round-25-20261023T194304Z/rshell/packaging/dist
    Updating crates.io index
warning: failed to write cache, path: /home/wilf/.cargo/registry/index/index.crates.io-1949cf8c6b5b557f/.cache/li/bz/libz-sys, error: Permission denied (os error 13)
   Compiling hyprv-svc-power v0.1.0 (/tmp/hci-sys/round-25-20261023T194304Z/rshell/crates/hyprv-svc-power)
error[E0583]: file not found for module `sample`
 --> crates/hyprv-svc-power/src/sysstats.rs:1:1
  |
1 | mod sample;
  | ^^^^^^^^^^^
  |
  = help: to create the module `sample`, create file "crates/hyprv-svc-power/src/sysstats/sample.rs" or "crates/hyprv-svc-power/src/sysstats/sample/mod.rs"
  = note: if there is a `mod sample` elsewhere in the crate already, import it with `use crate::...` instead

error[E0277]: a value of type `Series<8>` cannot be built from an iterator over elements of type `std::option::Option<f64>`
    --> crates/hyprv-svc-power/src/sysstats.rs:375:18
     |
 375 |                 .collect()
     |                  ^^^^^^^ value of type `Series<8>` cannot be built from `std::iter::Iterator<Item=std::option::Option<f64>>`
     |
help: the trait `FromIterator<std::option::Option<f64>>` is not implemented for `Series<8>`
    --> crates/hyprv-svc-power/src/sysstats/series.rs:4:1
     |
   4 | pub struct Series<const N:usize> { values:[Option<f64>;N], start:usize, len:usize }
     | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
note: the method call chain might not have had the expected associated types
    --> crates/hyprv-svc-power/src/sysstats.rs:368:18
     |
 340 |           let cpu = if self.metrics.contains(&Metric::CpuTotal)
     |  ___________________-
 341 | |             || self.metrics.contains(&Metric::CpuCores)
 342 | |         {
 343 | |             parse_cpu(&std::fs::read_to_string(self.ctx.paths.proc.join("stat"))?)
 344 | |         } else {
 345 | |             BTreeMap::new()
 346 | |         };
     | |_________- this expression has type `BTreeMap<String, Cpu>`
...
 357 |                   .iter()
     |                    ------ `Iterator::Item` is `(&String, &Cpu)` here
 358 |                   .filter_map(|(k, v)| {
     |  __________________-
 359 | |                     k.strip_prefix("cpu")
 360 | |                         .filter(|s| !s.is_empty())
 361 | |                         .and_then(|s| s.parse::<u32>().ok())
 362 | |                         .map(|index| (index, *v, k))
 363 | |                 })
     | |__________________- `Iterator::Item` changed to `(u32, Cpu, &String)` here
...
 367 |                   .into_iter()
     |                    ----------- `Iterator::Item` is `(u32, Cpu, &String)` here
 368 |                   .map(|(_, v, k)| {
     |  __________________^
 369 | |                     self.baseline
 370 | |                         .as_ref()?
 371 | |                         .cpu
 372 | |                         .get(k)
 373 | |                         .and_then(|b| cpu_delta(*b, v))
 374 | |                 })
     | |__________________^ `Iterator::Item` changed to `Option<f64>` here
note: required by a bound in `std::iter::Iterator::collect`
    --> /home/wilf/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/core/src/iter/traits/iterator.rs:2051:19
     |
2051 |     fn collect<B: FromIterator<Self::Item>>(self) -> B
     |                   ^^^^^^^^^^^^^^^^^^^^^^^^ required by this bound in `Iterator::collect`

error[E0308]: mismatched types
   --> crates/hyprv-svc-power/src/sysstats.rs:377:13
    |
377 |             vec![]
    |             ^^^^^^ expected `Series<8>`, found `Vec<_>`
    |
    = note: expected struct `Series<8>`
               found struct `Vec<_>`
note: return type inferred to be `Series<8>` here
   --> crates/hyprv-svc-power/src/sysstats.rs:316:1
    |
316 | #[async_trait]
    | ^^^^^^^^^^^^^^
    = note: this error originates in the macro `vec` which comes from the expansion of the attribute macro `async_trait` (in Nightly builds, run with -Z macro-backtrace for more info)

error[E0308]: mismatched types
   --> crates/hyprv-svc-power/src/sysstats.rs:428:28
    |
428 |         self.state.iface = net.as_ref().map(|n| n.0.clone());
    |         ----------------   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ expected `Option<Arc<str>>`, found `Option<String>`
    |         |
    |         expected due to the type of this binding
    |
    = note: expected enum `std::option::Option<Arc<str>>`
               found enum `std::option::Option<std::string::String>`

Some errors have detailed explanations: E0277, E0308, E0583.
For more information about an error, try `rustc --explain E0277`.
error: could not compile `hyprv-svc-power` (lib) due to 4 previous errors
Traceback (most recent call last):
  File "/tmp/hci-sys/round-25-20261023T194304Z/rshell/packaging/build_release.py", line 64, in <module>
    if __name__=='__main__':main()
                            ~~~~^^
  File "/tmp/hci-sys/round-25-20261023T194304Z/rshell/packaging/build_release.py", line 53, in main
    before=sources();subprocess.run(cmd,cwd=ROOT,check=True)
                     ~~~~~~~~~~~~~~^^^^^^^^^^^^^^^^^^^^^^^^^
  File "/usr/lib/python3.14/subprocess.py", line 578, in run
    raise CalledProcessError(retcode, process.args,
                             output=stdout, stderr=stderr)
subprocess.CalledProcessError: Command '['cargo', 'build', '--manifest-path', '/tmp/hci-sys/round-25-20261023T194304Z/rshell/Cargo.toml', '--locked', '--release', '--target-dir', '/home/wilf/.config/HyprV/rshell/target', '--config', '/tmp/hci-sys/round-25-20261023T194304Z/rshell/packaging/release.toml', '-p', 'hyprv-shell', '-p', 'hyprv-privhelper', '--bins']' returned non-zero exit status 101.
```

Logs relative to `docs/verify/raw/ci-snapshots/`. No causal commit inferred.


### Snapshot CI — 2026-10-23T19:44:27+00:00 — workspace tests

REPRODUCIBLE: failed twice on same snapshot. Snapshot `/tmp/hci-sys/round-25-20261023T194304Z/rshell`; fingerprint `64ee34c084ee98ed39959f42573192bdeef828abb85ad2c3b185e0f842c6427e`. Concurrent live edits cannot change this source copy.

Command: `['cargo', 'test', '--locked', '--workspace', '--all-targets', '--message-format=json']`. Attempts: `[{'code': 101, 'log': 'round-25-20261023T194304Z/03-workspace-tests-1.log'}, {'code': 101, 'log': 'round-25-20261023T194304Z/03-workspace-tests-2.log'}]`. Locations: crates/hyprv-svc-power/src/sysstats.rs:1, crates/hyprv-svc-power/src/sysstats.rs:375, /home/wilf/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/alloc/src/macros.rs:44, crates/hyprv-svc-power/src/sysstats.rs:428, crates/hyprv-svc-power/src/sysstats.rs:1, crates/hyprv-svc-power/src/sysstats.rs:375, crates/hyprv-svc-power/src/sysstats/series.rs:4, crates/hyprv-svc-power/src/sysstats.rs:368, crates/hyprv-svc-power/src/sysstats.rs:377, crates/hyprv-svc-power/src/sysstats.rs:316, crates/hyprv-svc-power/src/sysstats.rs:428.

`crates/hyprv-svc-power/src/sysstats.rs`: `24a0994ff815db233245a43c249704f6401062112328bf5836f88de017e0faf2`
`crates/hyprv-svc-power/src/sysstats/series.rs`: `ef9fe0066cb8b734881dffbbd54ea53445d7d6ff6db1acd4669ea49de7b45ff7`

```text
error[E0583]: file not found for module `sample`
 --> crates/hyprv-svc-power/src/sysstats.rs:1:1
  |
1 | mod sample;
  | ^^^^^^^^^^^
  |
  = help: to create the module `sample`, create file "crates/hyprv-svc-power/src/sysstats/sample.rs" or "crates/hyprv-svc-power/src/sysstats/sample/mod.rs"
  = note: if there is a `mod sample` elsewhere in the crate already, import it with `use crate::...` instead


```

Logs relative to `docs/verify/raw/ci-snapshots/`. No causal commit inferred.


### Snapshot CI — 2026-10-23T19:44:31+00:00 — doctests

REPRODUCIBLE: failed twice on same snapshot. Snapshot `/tmp/hci-sys/round-25-20261023T194304Z/rshell`; fingerprint `64ee34c084ee98ed39959f42573192bdeef828abb85ad2c3b185e0f842c6427e`. Concurrent live edits cannot change this source copy.

Command: `['cargo', 'test', '--locked', '--workspace', '--doc', '--message-format=json']`. Attempts: `[{'code': 101, 'log': 'round-25-20261023T194304Z/04-doctests-1.log'}, {'code': 101, 'log': 'round-25-20261023T194304Z/04-doctests-2.log'}]`. Locations: crates/hyprv-svc-power/src/sysstats.rs:1, crates/hyprv-svc-power/src/sysstats.rs:375, /home/wilf/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/alloc/src/macros.rs:44, crates/hyprv-svc-power/src/sysstats.rs:428, crates/hyprv-svc-power/src/sysstats.rs:1, crates/hyprv-svc-power/src/sysstats.rs:375, crates/hyprv-svc-power/src/sysstats/series.rs:4, crates/hyprv-svc-power/src/sysstats.rs:368, crates/hyprv-svc-power/src/sysstats.rs:377, crates/hyprv-svc-power/src/sysstats.rs:316, crates/hyprv-svc-power/src/sysstats.rs:428.

`crates/hyprv-svc-power/src/sysstats.rs`: `24a0994ff815db233245a43c249704f6401062112328bf5836f88de017e0faf2`
`crates/hyprv-svc-power/src/sysstats/series.rs`: `ef9fe0066cb8b734881dffbbd54ea53445d7d6ff6db1acd4669ea49de7b45ff7`

```text
error[E0583]: file not found for module `sample`
 --> crates/hyprv-svc-power/src/sysstats.rs:1:1
  |
1 | mod sample;
  | ^^^^^^^^^^^
  |
  = help: to create the module `sample`, create file "crates/hyprv-svc-power/src/sysstats/sample.rs" or "crates/hyprv-svc-power/src/sysstats/sample/mod.rs"
  = note: if there is a `mod sample` elsewhere in the crate already, import it with `use crate::...` instead


```

Logs relative to `docs/verify/raw/ci-snapshots/`. No causal commit inferred.


### Snapshot CI — 2026-10-23T19:44:44+00:00 — clippy

REPRODUCIBLE: failed twice on same snapshot. Snapshot `/tmp/hci-sys/round-25-20261023T194304Z/rshell`; fingerprint `64ee34c084ee98ed39959f42573192bdeef828abb85ad2c3b185e0f842c6427e`. Concurrent live edits cannot change this source copy.

Command: `['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '--message-format=json', '--', '-D', 'warnings']`. Attempts: `[{'code': 101, 'log': 'round-25-20261023T194304Z/05-clippy-1.log'}, {'code': 101, 'log': 'round-25-20261023T194304Z/05-clippy-2.log'}]`. Locations: crates/hyprv-svc-power/src/sysstats.rs:1, crates/hyprv-svc-power/src/sysstats/series.rs:10, crates/hyprv-svc-power/src/sysstats.rs:375, /home/wilf/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/alloc/src/macros.rs:44, crates/hyprv-svc-power/src/sysstats.rs:428, crates/hyprv-svc-power/src/sysstats.rs:1, crates/hyprv-svc-power/src/sysstats/series.rs:10, crates/hyprv-svc-power/src/sysstats.rs:375, crates/hyprv-svc-power/src/sysstats/series.rs:4, crates/hyprv-svc-power/src/sysstats.rs:368, crates/hyprv-svc-power/src/sysstats.rs:377, crates/hyprv-svc-power/src/sysstats.rs:316, crates/hyprv-svc-power/src/sysstats.rs:428.

`crates/hyprv-svc-power/src/sysstats.rs`: `24a0994ff815db233245a43c249704f6401062112328bf5836f88de017e0faf2`
`crates/hyprv-svc-power/src/sysstats/series.rs`: `ef9fe0066cb8b734881dffbbd54ea53445d7d6ff6db1acd4669ea49de7b45ff7`

```text
error[E0583]: file not found for module `sample`
 --> crates/hyprv-svc-power/src/sysstats.rs:1:1
  |
1 | mod sample;
  | ^^^^^^^^^^^
  |
  = help: to create the module `sample`, create file "crates/hyprv-svc-power/src/sysstats/sample.rs" or "crates/hyprv-svc-power/src/sysstats/sample/mod.rs"
  = note: if there is a `mod sample` elsewhere in the crate already, import it with `use crate::...` instead


```

Logs relative to `docs/verify/raw/ci-snapshots/`. No causal commit inferred.


### Snapshot CI — 2026-10-23T19:44:58+00:00 — engine idle-wakeup: quiet_nested_shell_has_no_repaints_or_extra_main_dispatches

REPRODUCIBLE: failed twice on same snapshot. Snapshot `/tmp/hci-sys/round-25-20261023T194304Z/rshell`; fingerprint `64ee34c084ee98ed39959f42573192bdeef828abb85ad2c3b185e0f842c6427e`. Concurrent live edits cannot change this source copy.

Command: `['cargo', 'test', '-p', 'hyprv-shell', '--locked', '--test', 'idle', '--', '--nocapture']`. Attempts: `[{'code': 101, 'log': 'round-25-20261023T194304Z/10-engine-idle-wakeup:-quiet_nested_shell_has_no_repaints_or_extra_main_dispatches-1.log'}, {'code': 101, 'log': 'round-25-20261023T194304Z/10-engine-idle-wakeup:-quiet_nested_shell_has_no_repaints_or_extra_main_dispatches-2.log'}]`. Locations: crates/hyprv-svc-power/src/sysstats.rs:1, crates/hyprv-svc-power/src/sysstats.rs:375, crates/hyprv-svc-power/src/sysstats/series.rs:4, crates/hyprv-svc-power/src/sysstats.rs:368, crates/hyprv-svc-power/src/sysstats.rs:377, crates/hyprv-svc-power/src/sysstats.rs:316, crates/hyprv-svc-power/src/sysstats.rs:428.

`crates/hyprv-svc-power/src/sysstats.rs`: `24a0994ff815db233245a43c249704f6401062112328bf5836f88de017e0faf2`
`crates/hyprv-svc-power/src/sysstats/series.rs`: `ef9fe0066cb8b734881dffbbd54ea53445d7d6ff6db1acd4669ea49de7b45ff7`

```text
    Updating crates.io index
warning: failed to write cache, path: /home/wilf/.cargo/registry/index/index.crates.io-1949cf8c6b5b557f/.cache/li/bz/libz-sys, error: Permission denied (os error 13)
    Blocking waiting for file lock on artifact directory
   Compiling hyprv-svc-power v0.1.0 (/tmp/hci-sys/round-25-20261023T194304Z/rshell/crates/hyprv-svc-power)
error[E0583]: file not found for module `sample`
 --> crates/hyprv-svc-power/src/sysstats.rs:1:1
  |
1 | mod sample;
  | ^^^^^^^^^^^
  |
  = help: to create the module `sample`, create file "crates/hyprv-svc-power/src/sysstats/sample.rs" or "crates/hyprv-svc-power/src/sysstats/sample/mod.rs"
  = note: if there is a `mod sample` elsewhere in the crate already, import it with `use crate::...` instead

error[E0277]: a value of type `Series<8>` cannot be built from an iterator over elements of type `std::option::Option<f64>`
    --> crates/hyprv-svc-power/src/sysstats.rs:375:18
     |
 375 |                 .collect()
     |                  ^^^^^^^ value of type `Series<8>` cannot be built from `std::iter::Iterator<Item=std::option::Option<f64>>`
     |
help: the trait `FromIterator<std::option::Option<f64>>` is not implemented for `Series<8>`
    --> crates/hyprv-svc-power/src/sysstats/series.rs:4:1
     |
   4 | pub struct Series<const N:usize> { values:[Option<f64>;N], start:usize, len:usize }
     | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
note: the method call chain might not have had the expected associated types
    --> crates/hyprv-svc-power/src/sysstats.rs:368:18
     |
 340 |           let cpu = if self.metrics.contains(&Metric::CpuTotal)
     |  ___________________-
 341 | |             || self.metrics.contains(&Metric::CpuCores)
 342 | |         {
 343 | |             parse_cpu(&std::fs::read_to_string(self.ctx.paths.proc.join("stat"))?)
 344 | |         } else {
 345 | |             BTreeMap::new()
 346 | |         };
     | |_________- this expression has type `BTreeMap<String, Cpu>`
...
 357 |                   .iter()
     |                    ------ `Iterator::Item` is `(&String, &Cpu)` here
 358 |                   .filter_map(|(k, v)| {
     |  __________________-
 359 | |                     k.strip_prefix("cpu")
 360 | |                         .filter(|s| !s.is_empty())
 361 | |                         .and_then(|s| s.parse::<u32>().ok())
 362 | |                         .map(|index| (index, *v, k))
 363 | |                 })
     | |__________________- `Iterator::Item` changed to `(u32, Cpu, &String)` here
...
 367 |                   .into_iter()
     |                    ----------- `Iterator::Item` is `(u32, Cpu, &String)` here
 368 |                   .map(|(_, v, k)| {
     |  __________________^
 369 | |                     self.baseline
 370 | |                         .as_ref()?
 371 | |                         .cpu
 372 | |                         .get(k)
 373 | |                         .and_then(|b| cpu_delta(*b, v))
 374 | |                 })
     | |__________________^ `Iterator::Item` changed to `Option<f64>` here
note: required by a bound in `std::iter::Iterator::collect`
    --> /home/wilf/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/core/src/iter/traits/iterator.rs:2051:19
     |
2051 |     fn collect<B: FromIterator<Self::Item>>(self) -> B
     |                   ^^^^^^^^^^^^^^^^^^^^^^^^ required by this bound in `Iterator::collect`

error[E0308]: mismatched types
   --> crates/hyprv-svc-power/src/sysstats.rs:377:13
    |
377 |             vec![]
    |             ^^^^^^ expected `Series<8>`, found `Vec<_>`
    |
    = note: expected struct `Series<8>`
               found struct `Vec<_>`
note: return type inferred to be `Series<8>` here
   --> crates/hyprv-svc-power/src/sysstats.rs:316:1
    |
316 | #[async_trait]
    | ^^^^^^^^^^^^^^
    = note: this error originates in the macro `vec` which comes from the expansion of the attribute macro `async_trait` (in Nightly builds, run with -Z macro-backtrace for more info)

error[E0308]: mismatched types
   --> crates/hyprv-svc-power/src/sysstats.rs:428:28
    |
428 |         self.state.iface = net.as_ref().map(|n| n.0.clone());
    |         ----------------   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ expected `Option<Arc<str>>`, found `Option<String>`
    |         |
    |         expected due to the type of this binding
    |
    = note: expected enum `std::option::Option<Arc<str>>`
               found enum `std::option::Option<std::string::String>`

Some errors have detailed explanations: E0277, E0308, E0583.
For more information about an error, try `rustc --explain E0277`.
error: could not compile `hyprv-svc-power` (lib) due to 4 previous errors
```

Logs relative to `docs/verify/raw/ci-snapshots/`. No causal commit inferred.

Sampling optimization in progress: cached proc/sensor FDs, bounded reusable input buffers; inline Series<8/120> + shared interface string preserve JSON and eliminate snapshot-clone heap allocations. Backend futures are unboxed Send futures instead of async_trait boxes; other backend logic unchanged. Elapsed-only metadata no longer forces publication. Existing power tests pass. Lease wiring and zero-allocation/performance regressions pending.


### Snapshot CI — 2026-10-23T19:58:03+00:00 — workspace tests

REPRODUCIBLE: failed twice on same snapshot. Snapshot `/tmp/hci-sys/round-26-20261023T195304Z/rshell`; fingerprint `f6433223079504ccc3a83f216bbe62e58d1dd94e476021239956c8872d344529`. Concurrent live edits cannot change this source copy.

Command: `['cargo', 'test', '--locked', '--workspace', '--all-targets', '--message-format=json']`. Attempts: `[{'code': 101, 'log': 'round-26-20261023T195304Z/03-workspace-tests-1.log'}, {'code': 101, 'log': 'round-26-20261023T195304Z/03-workspace-tests-2.log'}]`. Locations: crates/hyprv-svc-power/src/sysstats/sample.rs:307, crates/hyprv-svc-power/src/sysstats/sample.rs:307, crates/hyprv-svc-power/src/sysstats/sample.rs:309, crates/hyprv-svc-power/src/sysstats/sample.rs:309, crates/hyprv-svc-power/src/sysstats/sample.rs:307, crates/hyprv-svc-power/src/actor.rs:372, crates/hyprv-svc-power/src/sysstats/sample.rs:307, crates/hyprv-svc-power/src/sysstats/sample.rs:309, crates/hyprv-svc-power/src/actor.rs:372, crates/hyprv-svc-power/src/sysstats/sample.rs:309.

`crates/hyprv-svc-power/src/actor.rs`: `a363c05054f4e7e27f1923c01a0c26ead57cc6ef5d50e9cdce13e6d5db9887b6`
`crates/hyprv-svc-power/src/sysstats/sample.rs`: `04677a19169f314d339ebe126a537261f3569192bb3bd79d33d8e899c0d19202`

```text
error[E0283]: type annotations needed
   --> crates/hyprv-svc-power/src/sysstats/sample.rs:307:147
    |
307 | ...al","soc_thermal"]{for zone in crate::actor::entries("/sys/class/thermal".into()){if fs::read_to_string(zone.join("type")).is_ok...
    |                                   ---------------------                      ^^^^
    |                                   |
    |                                   required by a bound introduced by this call
    |
    = note: multiple `impl`s satisfying `_: AsRef<std::path::Path>` found in the following crates: `rustix`, `std`:
            - impl AsRef<std::path::Path> for OsStr;
            - impl AsRef<std::path::Path> for OsString;
            - impl AsRef<std::path::Path> for PathBuf;
            - impl AsRef<std::path::Path> for rustix::path::dec_int::DecInt;
            - impl AsRef<std::path::Path> for std::path::Path;
            - impl AsRef<std::path::Path> for std::string::String;
            - impl AsRef<std::path::Path> for str;
note: required by a bound in `entries`
   --> crates/hyprv-svc-power/src/actor.rs:372:27
    |
372 | pub fn entries(path: impl AsRef<std::path::Path>) -> Vec<PathBuf> {
    |                           ^^^^^^^^^^^^^^^^^^^^^^ required by this bound in `entries`
help: try using a fully qualified path to specify the expected types
    |
307 -         for preferred in ["x86_pkg_temp","TCPU","cpu_thermal","cpu-thermal","soc_thermal"]{for zone in crate::actor::entries("/sys/class/thermal".into()){if fs::read_to_string(zone.join("type")).is_ok_and(|s|s.trim()==preferred){reader.thermal.push(SampleFile::new(zone.join("temp"),64));}}}
307 +         for preferred in ["x86_pkg_temp","TCPU","cpu_thermal","cpu-thermal","soc_thermal"]{for zone in crate::actor::entries(<&str as Into<T>>::into("/sys/class/thermal")){if fs::read_to_string(zone.join("type")).is_ok_and(|s|s.trim()==preferred){reader.thermal.push(SampleFile::new(zone.join("temp"),64));}}}
    |
help: consider removing this method call, as the receiver has type `&'static str` and `&'static str: AsRef<std::path::Path>` trivially holds
    |
307 -         for preferred in ["x86_pkg_temp","TCPU","cpu_thermal","cpu-thermal","soc_thermal"]{for zone in crate::actor::entries("/sys/class/thermal".into()){if fs::read_to_string(zone.join("type")).is_ok_and(|s|s.trim()==preferred){reader.thermal.push(SampleFile::new(zone.join("temp"),64));}}}
307 +         for preferred in ["x86_pkg_temp","TCPU","cpu_thermal","cpu-thermal","soc_thermal"]{for zone in crate::actor::entries("/sys/class/thermal"){if fs::read_to_string(zone.join("type")).is_ok_and(|s|s.trim()==preferred){reader.thermal.push(SampleFile::new(zone.join("temp"),64));}}}
    |


```

Logs relative to `docs/verify/raw/ci-snapshots/`. No causal commit inferred.

- 2026-10-23 sampling-cost implementation: Sysstats optimization: retained bounded pread readers, inline bounded histories/first eight displayed cores, shared interface strings, unboxed Backend futures, and changed-value publication (elapsed alone is metadata). Authorized default-interface sysfs RX/TX fast path retains proc/dev fallback; route selection preserves the first UP IPv4 default from proc/net/route and is invalidated by existing rtnetlink route/link events, never polled per tick. Regression tests compare proc/sysfs counters, rates and resets; malformed source fallback; route-event cache invalidation; sensor leases/failures; zero warm allocations. Release sampler currently 15.4 us/sample (500 read-only samples); paced and controlled-quiet evidence follows in docs/verify/sampling-cost.md.


### Snapshot CI resolution — round 27 — 2026-10-23

The sysstats compile failures filed against immutable rounds 25/26 no longer reproduce in round 27, fingerprint `fd0d1ba1227137577270c8adbaf5b501ce9aff258fea57776194512db5bc77eb`. Debug/release workspace builds, full all-target workspace tests, and workspace Clippy with `-D warnings` all passed on the same snapshot. Logs: `docs/verify/raw/ci-snapshots/round-27-20261023T200304Z/`. No CI source fix was needed; concurrent owner changes supplied the repair. Performance acceptance remains separate.

- 2026-10-23 sampling validation: Remaining budget finding: 60 real 1 Hz samples average 98.866 us, zero allocations; tight-loop 15.447 us is not a real-cadence substitute. Independent cached-FD C probe measures preferred thermal read alone 74.923 us, proc/stat 38.409 us. The <=50 us target remains unmet without changing required sensor freshness/source/values. No such semantic change was made. See docs/verify/sampling-cost.md.

- 2026-10-23 sampling result: Final result: paced sysstats 98.866 us/sample vs earlier ~420 us, zero Rust allocations; 50 us target remains unmet due verified cold kernel/sensor reads. All 26 power tests/strict Clippy pass. Route selection equivalence and sysfs/proc fallback regressions pass; 100 read-only real counter comparisons pass. Evidence docs/verify/sampling-cost.md.

## P0 live CPU power (2026-10-24)
Root cause confirmed read-only: `/sys/class/powercap/intel-rapl:0/energy_uj` and the MMIO alias are root 0400. The new sysstats client acquires helper telemetry only while a CpuPackagePower metric lease is active and not sleeping; no user-side RAPL fallback remains. Signals carry watts/unavailability, provider owner changes reconnect, release/sleep unsubscribe, and in-flight acquisition survives actor cancellation so unsubscribe cannot race a late acquisition. Missing power/temperature/network/sample errors are logged on transitions. Source permissions/readability audit is recorded under `docs/verify/cpu-package-power/source-audit.json`.

### PQ-QA live audit L1 confirmation — 2026-10-24 00:35 EDT

Independent confirmation on the installed release eda5ca09:
- `strings ~/.local/bin/hyprv-shell` contains the `class/powercap … energy_uj` reader and no `SubscribeCpuPower`.
- The installed helper's introspection lists only `GetFans`, `SetFanLimit`, `RemoveExternalGpu` and `ProtocolVersion`.
- `/run/hyprv` is absent.
- The probe gives `package_power_w: null` with `sysstats.error: null`, and the shell journal has no unavailability line. The failure is silent at the live release, so make sure the new client's transition log actually reaches the journal.

Acceptance: the same probe run as wilf on the next release shows a finite `package_power_w`, and the live bar shows watts.

## P1 engine: LIVE hidden fan observer, 2026-10-24 (power owner keeps edits)

User requested findings/proposed diff only; engine does not edit this crate.
Live PID2709217 / release eda5ca090166790a41ddcd171f79008f01f589c766816b0d3040b41c07be0f03,
ELF2073ea266e34c5aee6a69038baeaaeb75a2a287da636ee55176469f7d54c89d0.
180.0017s uninstrumented: whole0.943490% CPU; service thread0.123308%,
**4149 voluntary switches =23.049778/s** (4176 total slices =23.199776/s).
These are shared-reactor totals, NOT all assigned to power.

A separate ~60s read-only `dbus-monitor --system --profile type=signal` observes
**45 FanStateChanged signals** (~0.75/s; inspect first/last timestamps for exact
coverage), plus2 PropertiesChanged. `bar dataStatus` confirms panelPolling=false;
no live panels were opened. Receipt docs/verify/raw/live-idle/dbus-system.txt.
System BecomeMonitor was denied; broadcast eavesdrop fallback worked. Unicast
GetFans requests were not captured; do not manufacture an exact request/wakeup
count from these 45 broadcasts. strace attachment also denied by Yama. Source
explains the avoidable path: fan.rs uses actor::spawn(..., false), connects a
broad helper-sender signal stream; actor.rs's unsampled branch snapshots after
all helper signals regardless of topic.subscriber_count. Each accepted signal
can therefore request ProtocolVersion + GetFans even with no consumer. This is
separate from your new, required leased CPU-power helper telemetry.

Proposed minimal diff shape:
- Narrow fan Native::connect to signals_filtered(...helper PATH, INTERFACE,
  "FanStateChanged") so CPU telemetry/helper signals cannot trigger GetFans.
- Add a backend opt-in for demand-driven event observation (default false),
  select it only for fan. Drop its signal inbox while no topic subscribers;
  on 0->1 resolve the current helper owner, reconnect and fetch one fresh
  snapshot before showing readings. Do not introduce a sampling timer.
- Continue accepting explicit SetLimit/Refresh actions with demand0; do not
  gate privileged control, helper watchdog or actual fan control. Retain
  provider replacement/resume recovery. UI close must release observation.
- Add a private-bus regression emitting e.g.60 FanStateChanged plus60 unrelated
  helper CPU signals: hidden fan =>0 refresh GetFans; visible =>fresh snapshot,
  only real fan notifications refresh; close =>0 further refreshes. Include
  helper owner replacement while hidden and first-open hydration.

Please attribute fan calls with private mock-server counters before/after and
return CPU/scheduling measurements. Engine will preserve your active edits,
re-read after your checkpoint and integrate only UI/shell changes. Raw baseline:
docs/verify/raw/live-idle/before.json; live UI0.197359%, discovery worker0.358488%,
shared blocking worker0.264310%. Full accounting is docs/verify/live-idle.md
(report being written); no live process restart or hardware writes occurred.

Proposed fan-only filter diff (NOT APPLIED/compiled by engine; generic demand
opt-in still required separately as described above):

```diff
 // actor.rs
-async fn signals_filtered(bus: ...)
+pub(crate) async fn signals_filtered(bus: ...)
 // fan.rs Native::connect
- self.stream = Some(actor::signals(&self.ctx.bus, hyprv_privhelper::BUS).await?);
+ self.stream = Some(actor::signals_filtered(&self.ctx.bus,
+     hyprv_privhelper::BUS,
+     Some((hyprv_privhelper::PATH, hyprv_privhelper::INTERFACE,
+           "FanStateChanged"))).await?);
```

Keep the new CPU telemetry stream distinct. The live45FanStateChanged broadcasts
were captured from1792815985.224685 through1792816043.226569 (raw last line;
monitor ran60s). A broadcast count is an input count, not exact reactor wakes or
GetFans calls. Do not apply blanket signal dropping without the visibility and
first-open/owner-change regression.

## Laptop Phase A engine — paired stats projections (2026-10-24, analysis only)

Earlier live hidden-fan findings/proposed diffs remain above at line490; no service
edits by engine. New private nested baseline uses installed shell ELF
`c364f556a1a5168a7bcac866616e1912a87b06951b7ac321b2a7a0f22d1d52df`, empty agent
fixture/private PID namespace and read-only native providers,6144x3456@2, panels closed.
Uninstrumented90.168 s: process CPU0.2107%, services15.959 voluntary switches/s
(shared reactor; not all attributable to stats). Separate45.166 s diagnostic:
**90 projected sysstats notifications,90 stats UI passes,92 paints**, two notifications
per second in pairs (~0.21/0.79 s apart initially), despite dataStatus systemInterval1000.
`sysstats.rs:329–336` already avoids resampling local counters on a helper-power event;
these observations are **not** proof of a2 Hz /proc sampler. Independently phased
package-power signals and1 Hz local values are a source-level explanation requiring
mock counters to confirm. Do not remove correct desktop updates under the freeze.

Laptop compact strip shows CPU/memory but no package-power on battery. Host plan:
a shared `{CpuTotal,Memory}`5 s lease, no hidden Network/Temperature/CpuPackagePower/
CpuCores demand; helper/cooling inert. Please ensure metric-negotiation tests prove
that this lease generates **zero** RAPL helper telemetry, no hidden sensors/history,
and no helper-signal-triggered local samples. Keep visible percent freshness and
first-open/reset behavior. Any further desktop coalescing needs exact behavior proof.

Evidence: `docs/verify/raw/laptop-minimal-resources/{summary.json,diagnostics.csv}`;
plan: `docs/spec/laptop-minimal-resources.md`. Notification counts are source events,
not exact kernel wakeups. No timer/service source patch is applied in Phase A.

## Laptop Phase B — svc-power ownership checkpoint (2026-10-24)

Implementing the laptop-services spec in this crate: explicit runtime profile constructors (desktop defaults retained), signal-cached laptop power/lid provider, leased battery details, laptop brightness planner/startup policy, metric-specific laptop sysstats and dormant fan observation. `docs/spec/laptop-deploy.md` was absent at the checkpoint. No live actions/install. Host owners should use the profile constructors rather than duplicate profile detection; API details will follow in the README. Edits are confined to power crate and its coordination/spec receipts.

Engine Phase B compile checkpoint: concurrent battery_history.rs:294 calls
`DirBuilder::mode` without importing `std::os::unix::fs::DirBuilderExt`; this blocks
`cargo check -p hyprv-shell`. Please fix in your current edit (engine leaves it alone).
UI-only checks continue meanwhile. Host is awaiting profile constructor/demand API.

### PQ-QA → service owners: test-only sysfs / system-bus overrides for laptop fakes (2026-10-24, laptop Phase A)

There is no XPS hardware here and no sudo (so no umockdev or dbusmock). Laptop e2e and budget runs (`docs/verify/laptop-qa-plan.md` §2.3–2.4) need every reader to accept:
- **`HYPRV_SYSFS_ROOT`** (default `/sys`), read once at startup, for:
  - `class/power_supply/*` (battery history / popup);
  - `class/backlight/*`;
  - `class/thermal/*` and `class/hwmon/*` (temperature);
  - `class/dmi/id/product_name` (profile auto-detect).
- **`DBUS_SYSTEM_BUS_ADDRESS`**, honoured by every system-bus client (UPower, PowerProfiles, login1, NM, BlueZ). zbus does this by default; please confirm no client hard-codes the system bus path.

Both must have no effect when unset, so the desktop stays byte-identical. Acceptance: the `lap.*` e2e IDs run against `parity/laptop/fakes/` (Phase B, QA-owned).

**Additional question (laptop CPU power):** the task says privhelper cooling is inert on laptop. Does laptop CPU package power still use `SubscribeCpuPower`, or is the W readout hidden whenever a battery is present (`batteryOrCpuPowerText`, laptop `shell.qml:276-279`)? QA covers both in `laptop-qa-plan.md` §3.5 and needs the answer before `lap.privhelper.inert` can be written exactly.

## Laptop: AnimatedGlassPanel shadow layer always on — spec-bar → svc-power (2026-10-24)

Spec: `docs/spec/laptop-bar.md` §10. `origin/laptop:quickshell/AnimatedGlassPanel.qml:178-183`
sets the shadow item to `layer.enabled: true` unconditionally and drops `visible: surfaceOpacity > 0`.
Main gates both on `shadowColor.a > 0 && surfaceOpacity > 0`.
- Bar-owned glass (tray menu, overflow, battery popup, CPU tooltip) always has a visible
  shadow at surfaceOpacity 0.82, so there is **no bar delta**.
- Your surfaces are affected:
  - `ControlPanelPopup.qml:769` (`shadowColor: "transparent"`, `surfaceOpacity` 0 on the
    main page);
  - `WifiFallback.qml:376` (`shadowColor: "transparent"`).

  On laptop their glass fill goes through a MultiEffect layer anyway (an 8-bit layer
  round-trip, plus a composited pass even at opacity 0). Please add
  `QUIRK_GLASS_SHADOW_LAYER_ALWAYS` (runtime, `Profile::Laptop`) to the panels glass path
  and cover it in the laptop panel fixtures. Desktop stays gated.
- Related, for your laptop brightness planner: QuickAdjust (spec-bar) emits only
  `BrightnessSet(n)` with an integer level 0..100, step 1, and never `BrightnessFinish` on
  laptop. See `docs/spec/laptop-quickadjust.md`.

### Laptop deployment boundary — svc-sys/cutover Phase A, 2026-10-24

`docs/spec/laptop-deploy.md` specifies no laptop `SubscribeCpuPower`, cooling
subscription or D-Bus activation of the desktop helper. With a real battery the
branch's battery-or-CPU-power slot displays battery data; an unavailable battery
must not arm the nct6799/RAPL helper as fallback. Any future laptop watts source
needs an unprivileged visible-consumer backend or a separately justified privilege
plan. Existing desktop helper boot safety is unchanged. This is a design contract;
inertness/non-activation tests are Phase B deployment prerequisites.

Engine Phase B update: native direct-window retirement screenshot control passes
8/8 after disabling only the private compositor's animated watchdog banner; all
output transforms at1/1.5/2 pass. No live config change. Adding shell-owned
`BarStatsDemand::{Hidden,Compact,Detailed}` with automatic lease drop/reacquire on
mode changes (Compact={CpuTotal,Memory}/5s). Backend constructors stay yours.
Scoped Clippy currently also reports backlight.rs:548 nonminimal_bool in your new
parser; please fix while editing that crate. Engine leaves service source untouched.

## Laptop Phase B completion receipt — 2026-10-24

Power Rust source bundle SHA-256: `7621c5185665f273b8e464715946947b2824fd7afff8427fc02f695afd75a59f` (sorted src paths + bytes).
The deployment spec landed during implementation and was read; constructors use
an explicit `hyprv_svc_power::Profile`, leaving selector/host construction to its
owner. `spawn` remains desktop by default.

Implemented: cached UPower battery/lid signals with owner recovery and selective
invalidation; receive-order floors keep queued pre-snapshot/property-confirmation
signals from rolling state backward; both power-profile cycles; leased physical
BAT details at1Hz; laptop backlight/HDR planner, auto restore and ALS adoption;
independent per-metric stats and5s painted compact strip; hidden kernel watches
released; laptop root telemetry/cooling inert. Desktop fan observer now drops
its stream/owner watch with zero subscribers, hydrates on first open, rejects
unrelated helper signals, and retains explicit hidden actions without activating
an absent helper.

Pinned full-script oracle corrected a Phase A error: the IFS byte is real TAB09,
not backslash+t. Native history preserves actual averages/counts/windows. Spec
corrected with evidence; no intentional legacy history bug was introduced.

Validation (shared target, CARGO_INCREMENTAL=0):
- `cargo test --locked -p hyprv-svc-power -p hyprv-privhelper --all-targets`: power50,
  helper39 unit +4 integration = **93 passed,0 failed**. Two manual benchmarks
  ignored by default (one is the new private fan receipt).
- `cargo clippy --locked -p hyprv-svc-power -p hyprv-privhelper --all-targets -- -D warnings`: PASS.
- `cargo test --locked -p hyprv-svc-power --lib fan::tests::private_bus_hidden_fan_idle_receipt -- --ignored --exact --nocapture`: PASS.
  10.000141s, service-thread CPU0ns (0.00000000%), voluntary switches0, GetFans0.
  Window is an isolated quiescent fake-provider case. Not a live whole-shell result.
- Private fan storm:60 FanStateChanged +60 CpuPowerChanged while hidden =>0 GetFans;
  visible first hydration =>1;60 CPU signals add0; one fan signal adds1; closed
  fan storm adds0; hidden owner replacement adds0; reopen hydrates current owner.
- Battery lifetime, full script/TSV oracle, all101 Lua brightness levels at five
  maxima, native HDR Lua fake, delayed hardware supersession, profile policy denial,
  lid/sleep, thermal priority, guest/memory/route quirks and independent cadence pass.

No root helper source/unit changes, install, release activation, live IPC, desktop
UI input or compositor/service restarts were performed. Existing desktop service
tests pass; laptop nested visual/resource/device gates are not claimed here.

Engine verification checkpoint: desktop functional204/204 PASS; GPU component
island380/380, bar300PASS+12existing exclusions, panels300/300 PASS. Final image
cache fix requires fresh island/bar render, and broad canonical input hashes also
track your service files. My packaging/build-release.sh correctly refused first
receipt due concurrent service changes. Retrying cached release now; please publish
a source-stable checkpoint (or coordinate a short~6min verification window after
API/test completion) so fresh GL+assembled receipts can be admitted. No request to
skip tests or change your runtime contracts; docs-only work is fine during the hold.
Host Compact demand API is implemented; Detailed currently means desktop metrics
including CpuPackagePower, so laptop expanded wiring must use your negotiated
metric/capability API rather than blindly acquiring desktop Detailed.

**Update 2026-10-24 (PQ-QA): the user approved the test hooks above.**
- `HYPRV_SYSFS_ROOT` and `DBUS_SYSTEM_BUS_ADDRESS` are now the requested contract for the laptop fakes. The QA fakes are being built in `parity/laptop/fakes/` against exactly these names and layouts.
- Laptop panel: 2880×1800 @2.
- No eGPU on the XPS: keep the legacy eGPU UI, but install no root service for it on the laptop.
- Laptop CPU power: no privileged RAPL client (`ARCHITECTURE.md`), so my earlier question is answered.

## User decision + corrected CPU-W visibility — laptop deploy, 2026-10-24

Supersedes my earlier blanket prohibition of laptop SubscribeCpuPower. Source audit
at origin/laptop 24972f7: shell.qml:4198–4204 has a separate expanded-bar CPU W
module, visible iff cpuPackagePowerAvailable, independently of battery. The next
module (:4218–4225) is batteryOrCpuPowerText. Thus expanded = [CPU W][battery]
with both sources; compact (:3631) shows battery only with a present battery.
User now explicitly permits an OPTIONAL READ-ONLY CPU-power helper, with cooling
and eGPU disabled. Default install stays user-only. Do not activate the current
combined desktop helper/unit; CPU-only entry point/interface/install set must
branch before cooling discovery and omit failsafes/fan/eGPU APIs/policies. Reuse
native RAPL arithmetic, demand scoped to visible W consumers; no compact-battery
W sampling. See docs/spec/laptop-deploy.md CPU audit/optional privilege sections
and docs/verify/raw/laptop-deploy/cpu-w-source.json. All sysfs readers must use
test-only HYPRV_SYSFS_ROOT, all system-bus paths DBUS_SYSTEM_BUS_ADDRESS, including
probe/reconnect/error paths; no host fallback. Implementation/tests remain Phase B.

## Phase B CPU-only endpoint contract — svc-sys (2026-10-24)

Implementing distinct helper now: hyprv_privhelper::cpu_power::{BUS,PATH,INTERFACE}
= org.hyprv.Shell.CpuPower / /org/hyprv/Shell/CpuPower /
org.hyprv.Shell.CpuPower1; version1, same SubscribeCpuPower/UnsubscribeCpuPower/
CpuPowerChanged JSON rapl::State. Please select these under shared core Profile::Laptop,
never the combined desktop BUS; Desktop constants/behavior remain unchanged. No
sample cadence argument; subscribe only under visible-metric demand and unsubscribe
on loss. CPU-only optional install default absent. User authorizes CPU-only privilege,
not cooling/eGPU. I own helper code/install set and won't edit your actor/sysstats.

### Shared runtime profile API LANDED (engine, laptop Phase B)

`hyprv_core::profile::{Profile, profile, sys_root, sys_path}` now exists.
`Profile::{Desktop,Laptop}`, `profile() -> Profile` is process-latched; exact
HYPRV_PROFILE override (invalid is fatal), otherwise exact portable DMI or present
System-scope Battery; ambiguous defaults Desktop. `sys_root() -> &Path` latches
HYPRV_SYSFS_ROOT or /; `sys_path(rel: impl AsRef<Path>) -> PathBuf` accepts root-relative
paths, strips leading /, rejects ..; no fallback. Code against this shared API now.
Shell startup/profile/service/UI wiring follows. Do not duplicate detection.

### Laptop Phase B svc-net ownership handoff (2026-10-24)
Network rates are owned by your sysstats (svc-net owns NM/BlueZ only). User requires rates ONLY while actually visible and no rate/route wakes in minimal state. Please retain Metric::Network typed lease, no fallback from Topic observers; baseline reset/reprime on reopen and no route monitor/read hidden. Core profile/sys_path landed. Please test read-counts and route events with fake roots; svc-net will not edit your crate.

Engine already filters CpuPackagePower before bar Detailed acquisition and excludes
it from laptop chart leases; laptop generic CP metrics are CPU+Memory only. No RAPL
or Network/Temperature hidden CP lease. Explicit typed constructors now wired.
Native fake-bus actor signal cases are being added to nested laptop tests.

### spec-panels → svc-power: laptop CP panel side landed (2026-10-24, laptop Phase B)

`crates/hyprv-ui-panels` `ControlPanel` now latches `hyprv_core::profile::profile()` at construction. Tests
and replay can use `ControlPanel::with_profile` and `Panels::with_profile`. With `Profile::Laptop`:
- No fan, VM or Viewflow tiles are built, and no `FanLimit`, `VmToggle` or `ViewflowSet` is ever
  emitted, whatever `controls.fan_available` says.
- The Power split tile (left column y=144) emits `ProfileSet(next)` with the usual cycle. The Display
  split tile emits `ToggleDemo`.
- The brightness slider emits only `BrightnessSet(n)`, with n an integer 0..100 at a step of 1. It never
  emits `BrightnessFinish`, on release or cancel.

Data the panel needs from you on laptop, all existing `Model.controls` fields:
- `profile` from PPD;
- `brightness_percent` from the laptop `brightness_get` path. The probe is accepted unguarded; that is
  your side, per §2.2;
- `screen_recording` from exact-name recorder matching;
- `demo_mode`, `dnd` and `prevent_sleep` as on desktop.

Nothing else is required, and desktop is unchanged.

## Shared API implementation complete — source-stable checkpoint (2026-10-24)

Core Profile re-export/default dispatch and sys_path("sys") root wiring complete;
fresh-process default-factory/private-bus tests pass, including a broken private
address with no fallback. Fixture symlinks are confined and atomic replacements
work without host kernel-event subscriptions. No laptop fan/helper calls, popup
sampling stops on close. Engine's detailed-metric filter and panel fan gate were
observed landed. Power/core/helper tests124pass; focused laptop CP tests3pass;
strict all-target Clippy passes. Power Rust source is now stable for integrated
verification; no further source edits planned. Report docs/verify/laptop-impl-svc-power.md.

Phase B update: cpu_power module/CPU-only binary now compiles; private-bus tests
pass for auth denial, missing fan/eGPU APIs and disconnect lease cleanup/idle exit.
Please wire the distinct constants above for laptop while preserving desktop BUS.
CPU sampler entry uses core::profile::sys_path("sys/class/powercap"), root fixture
contract is ROOT/sys/... (default root /). Optional helper fixed protocol1 uses
existing rapl::run arithmetic/1s active sampling; zero sampler without demand.

Engine re-read new optional CPU endpoint contract: blanket laptop watt filter now
removed. Detailed expanded/chart leases can request CpuPackagePower via your typed
profile-selected backend; compact stays CPU+Memory5s, and laptop CP creates NO
extra sysstats lease (it paints no stats; prior CPU+Memory CP fallback removed).
Landed mapping.controls.screen_recording = desktop.recording.state().active on
laptop. Desktop construction/CP stats policy unchanged. All final gates pending.

## CP follow-up implementation / request disposition (2026-10-24)

- PPD signal-cached profile projection and both cycle orders verified; no service
  gap found. Modern/old endpoint preference/fallback and stale-signal tests pass.
- Laptop brightness refresh now accepts every valid read-back even during drag or
  pending confirmation; desktop guards preserved. Added paired-profile regression.
- CPU-only endpoint integrated for explicit laptop watts leases. Compact CPU+memory
  5s remains telemetry-free. Missing optional endpoint logs unavailable and cannot
  fall back to desktop BUS. Added cancellation-safe lazy setup, last-demand release
  and ordered rapid reopen, with regression tests; 100 repeated transitions pass.
- svc-net rates request: added hidden file-removal/route-reset/reopen regression;
  baseline reprimes and no hidden reader is retained. Fan regressions remain green.
- Exact-name recorder matching is OPEN in svc-desktop request file (their source
  ownership), as is missing controls.screen_recording host projection in shell's.
  Host also needs visible-only laptop watts lease wiring now the optional API exists.
- Older glass shadow request is panel-owned; their implementation report records
  no visible delta. No service change required; panel handoff logged.

Fresh power/core/helper all-target tests134pass,2 manual benchmarks ignored;
strict all-target Clippy PASS. Panels library19/19 PASS. Updated detailed report:
docs/verify/laptop-impl-svc-power.md. No release/install/live actions/source edits
outside power. Open cross-owner requests are not reported as completed.

Final reread update: engine concurrently landed laptop-only CP recording projection
and enabled Detailed watts leases. Those two source handoffs are no longer open;
report corrected. Remaining CP gap is svc-desktop's exact-name recorder detection.

## svc-sys Phase B finished (2026-10-24)

CPU endpoint code/constants now ready: hyprv_privhelper::cpu_power::{BUS,PATH,
INTERFACE,VERSION}, protocol1; same State/Subscribe/Unsubscribe/Changed contract.
Unit name hyprv-cpu-power-only.service avoids legacy CPU publisher collision;
client uses D-Bus only. Please select this endpoint for Laptop, preserve old BUS
for Desktop, never fall back to combined helper. No changes to your files/rapl.rs.
Report docs/verify/laptop-impl-svc-sys.md; all focused checks pass. No install.
